use crate::gui::{send_event, MouseGestureEvent, WatchEvent};
use crate::mouse_gestures::mouse_gesture_overlay;
use crate::plugins::mouse_gestures::db::{
    select_binding, select_profile, ForegroundWindowInfo, MouseGestureDb,
};
use crate::plugins::mouse_gestures::engine::{
    direction_from_vector, direction_sequence, direction_similarity, parse_gesture,
    preprocess_points_for_directions, straightness_ratio, track_length, GestureDirection, Point,
    Vector,
};
use crate::plugins::mouse_gestures::settings::MouseGesturePluginSettings;
use once_cell::sync::OnceCell;
use std::cmp::Ordering as CmpOrdering;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

#[cfg(windows)]
pub const MG_PASSTHROUGH_MARK: usize = 0x4D475054;

const PREVIEW_SIMILARITY_TOP_N: usize = 3;
const PREVIEW_SIMILARITY_LABEL_MAX_LEN: usize = 40;
const PREVIEW_SIMILARITY_MAX_LEN: usize = 140;
const PREVIEW_SIMILARITY_PREFIX: &str = "Similarity: ";
const PREVIEW_SIMILARITY_ELLIPSIS: &str = "…";

#[cfg(windows)]
pub fn should_ignore_event(flags: u32, extra_info: usize) -> bool {
    let _ = flags;
    extra_info == MG_PASSTHROUGH_MARK
}

#[cfg(windows)]
fn send_passthrough_click(button: TriggerButton) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
        MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
        MOUSEINPUT,
    };

    let (down_flag, up_flag) = match button {
        TriggerButton::Left => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
        TriggerButton::Right => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
        TriggerButton::Middle => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP),
    };

    let inputs = [
        INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: 0,
                    dwFlags: down_flag,
                    time: 0,
                    dwExtraInfo: MG_PASSTHROUGH_MARK,
                },
            },
        },
        INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: 0,
                    dwFlags: up_flag,
                    time: 0,
                    dwExtraInfo: MG_PASSTHROUGH_MARK,
                },
            },
        },
    ];
    let _ = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
}

#[derive(Clone, Debug)]
pub struct TrackOutcome {
    pub matched: bool,
    pub passthrough_click: bool,
}

impl TrackOutcome {
    fn passthrough() -> Self {
        Self {
            matched: false,
            passthrough_click: true,
        }
    }

    fn no_match() -> Self {
        Self {
            matched: false,
            passthrough_click: false,
        }
    }

    fn matched() -> Self {
        Self {
            matched: true,
            passthrough_click: false,
        }
    }
}

#[derive(Clone)]
struct MouseGestureSnapshots {
    settings: MouseGesturePluginSettings,
    db: MouseGestureDb,
    gesture_templates: HashMap<String, GestureTemplate>,
}

impl Default for MouseGestureSnapshots {
    fn default() -> Self {
        let settings = MouseGesturePluginSettings::default();
        let db = MouseGestureDb::default();
        let gesture_templates = build_gesture_templates(&db, &settings);
        Self {
            settings,
            db,
            gesture_templates,
        }
    }
}

#[derive(Clone, Default)]
struct ProfileCache {
    window: Option<ForegroundWindowInfo>,
    profile_id: Option<String>,
    #[cfg(test)]
    cache_hits: usize,
}

impl ProfileCache {
    fn clear(&mut self) {
        self.window = None;
        self.profile_id = None;
    }
}

#[derive(Clone)]
pub struct MouseGestureRuntime {
    snapshots: Arc<RwLock<MouseGestureSnapshots>>,
    event_sink: Arc<dyn MouseGestureEventSink>,
    profile_cache: Arc<Mutex<ProfileCache>>,
}

impl MouseGestureRuntime {
    fn select_profile_cached<'a>(
        &self,
        db: &'a MouseGestureDb,
        window: &ForegroundWindowInfo,
    ) -> Option<&'a crate::plugins::mouse_gestures::db::MouseGestureProfile> {
        if let Ok(cache) = self.profile_cache.lock() {
            if let Some(cached_window) = cache.window.as_ref() {
                if cached_window == window {
                    let cached_profile_id = cache.profile_id.clone();
                    #[cfg(test)]
                    {
                        drop(cache);
                        if let Ok(mut cache) = self.profile_cache.lock() {
                            cache.cache_hits = cache.cache_hits.saturating_add(1);
                        }
                    }
                    return cached_profile_id.as_ref().and_then(|profile_id| {
                        db.profiles.iter().find(|profile| profile.id == *profile_id)
                    });
                }
            }
        }
        let profile = select_profile(db, window);
        if let Ok(mut cache) = self.profile_cache.lock() {
            cache.window = Some(window.clone());
            cache.profile_id = profile.map(|profile| profile.id.clone());
        }
        profile
    }

    fn best_match(&self, points: &[Point]) -> Option<(String, f32)> {
        let snapshots = self
            .snapshots
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_default();
        self.best_match_with_snapshots(points, &snapshots)
    }

    fn best_match_with_snapshots(
        &self,
        points: &[Point],
        snapshots: &MouseGestureSnapshots,
    ) -> Option<(String, f32)> {
        if points.len() < 2 {
            return None;
        }
        let length = track_length(points);
        if length < snapshots.settings.min_track_len {
            return None;
        }
        if snapshots.settings.max_track_len > 0.0 && length > snapshots.settings.max_track_len {
            return None;
        }
        let track_dirs = track_directions_with_override(points, &snapshots.settings, None, None);
        if track_dirs.is_empty() {
            return None;
        }
        let gesture_templates = &snapshots.gesture_templates;
        let mut distances = HashMap::new();
        for (gesture_id, template) in gesture_templates {
            let similarity = direction_similarity(&track_dirs, &template.directions);
            let threshold = snapshots
                .settings
                .match_threshold_for_template_len(template.directions.len());
            if similarity < threshold {
                continue;
            }
            distances.insert(gesture_id.clone(), 1.0 - similarity);
        }
        if distances.is_empty() {
            return None;
        }
        let window_info = current_foreground_window();
        let profile = self.select_profile_cached(&snapshots.db, &window_info)?;
        let binding = select_binding(profile, &distances, snapshots.settings.max_distance)?;
        let similarity = 1.0 - binding.distance;
        let label = if binding.binding.label.trim().is_empty() {
            binding.binding.action.clone()
        } else {
            binding.binding.label.clone()
        };
        Some((label, similarity))
    }

    fn preview_text(&self, points: &[Point]) -> Option<String> {
        let snapshots = self
            .snapshots
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_default();
        let encoder = PreviewTextEncoder::default();
        self.preview_text_with_snapshots(points, &snapshots, &encoder)
    }

    fn preview_text_with_snapshots(
        &self,
        points: &[Point],
        snapshots: &MouseGestureSnapshots,
        encoder: &PreviewTextEncoder,
    ) -> Option<String> {
        if points.len() < 2 {
            return None;
        }
        let length = track_length(points);
        if length < snapshots.settings.min_track_len {
            return Some("Keep drawing".to_string());
        }
        if snapshots.settings.max_track_len > 0.0 && length > snapshots.settings.max_track_len {
            return Some("Too long".to_string());
        }
        if snapshots.settings.debug_show_similarity {
            return self.preview_similarity_text_with_encoder(points, snapshots, encoder);
        }
        let Some((label, similarity)) = self.best_match_with_snapshots(points, snapshots) else {
            return Some("No match".to_string());
        };
        Some(format!(
            "Will trigger: {label} ({:.0}%)",
            similarity * 100.0
        ))
    }

    fn preview_similarity_text(
        &self,
        points: &[Point],
        snapshots: &MouseGestureSnapshots,
    ) -> Option<String> {
        let encoder = PreviewTextEncoder::default();
        self.preview_similarity_text_with_encoder(points, snapshots, &encoder)
    }

    fn preview_similarity_text_with_encoder(
        &self,
        points: &[Point],
        snapshots: &MouseGestureSnapshots,
        encoder: &PreviewTextEncoder,
    ) -> Option<String> {
        let track_dirs = track_directions_with_override(points, &snapshots.settings, None, None);
        if track_dirs.is_empty() {
            return Some("No match".to_string());
        }
        let gesture_templates = &snapshots.gesture_templates;
        if gesture_templates.is_empty() {
            return Some("No match".to_string());
        }
        let window_info = current_foreground_window();
        let Some(profile) = self.select_profile_cached(&snapshots.db, &window_info) else {
            return Some("No match".to_string());
        };

        let mut similarities: Vec<(f32, String)> = Vec::new();
        for binding in profile.bindings.iter().filter(|binding| binding.enabled) {
            let Some(template) = gesture_templates.get(&binding.gesture_id) else {
                continue;
            };
            let similarity = direction_similarity(&track_dirs, &template.directions);
            let threshold = snapshots
                .settings
                .match_threshold_for_template_len(template.directions.len());
            if similarity < threshold {
                continue;
            }
            if !similarity.is_finite() {
                continue;
            }

            let mut min_index = None;
            let mut min_similarity = None;
            if similarities.len() >= PREVIEW_SIMILARITY_TOP_N {
                if let Some((index, (value, _))) = similarities
                    .iter()
                    .enumerate()
                    .min_by(|a, b| a.1 .0.partial_cmp(&b.1 .0).unwrap_or(CmpOrdering::Equal))
                {
                    min_index = Some(index);
                    min_similarity = Some(*value);
                }
            }

            if let Some(min_similarity) = min_similarity {
                if similarity <= min_similarity {
                    continue;
                }
            }

            let label = if binding.label.trim().is_empty() {
                binding.action.as_str()
            } else {
                binding.label.as_str()
            };
            let truncated = encoder.truncate_label(label.trim());
            if let Some(index) = min_index {
                similarities[index] = (similarity, truncated);
            } else {
                similarities.push((similarity, truncated));
            }
        }

        if similarities.is_empty() {
            return Some("No match".to_string());
        }

        Some(encoder.encode_similarity(similarities))
    }

    fn evaluate_track(&self, points: &[Point]) -> TrackOutcome {
        let snapshots = self
            .snapshots
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_default();
        self.evaluate_track_with_snapshots(points, snapshots)
    }

    fn evaluate_track_with_limit(&self, points: &[Point], too_long: bool) -> TrackOutcome {
        let snapshots = self
            .snapshots
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_default();
        let passthrough_on_no_match = snapshots.settings.passthrough_on_no_match;
        if too_long {
            return if passthrough_on_no_match {
                TrackOutcome::passthrough()
            } else {
                TrackOutcome::no_match()
            };
        }
        self.evaluate_track_with_snapshots(points, snapshots)
    }

    fn evaluate_track_with_snapshots(
        &self,
        points: &[Point],
        snapshots: MouseGestureSnapshots,
    ) -> TrackOutcome {
        let passthrough_on_no_match = snapshots.settings.passthrough_on_no_match;
        if points.len() < 2 {
            return TrackOutcome::passthrough();
        }
        let length = track_length(points);
        let displacement = track_displacement(points);
        let straightness = straightness_ratio(points);
        if length < snapshots.settings.min_track_len {
            return TrackOutcome::passthrough();
        }
        if snapshots.settings.max_track_len > 0.0 && length > snapshots.settings.max_track_len {
            return if passthrough_on_no_match {
                TrackOutcome::passthrough()
            } else {
                TrackOutcome::no_match()
            };
        }

        let track_dirs = track_directions_with_override(
            points,
            &snapshots.settings,
            Some(displacement),
            Some(straightness),
        );
        if track_dirs.is_empty() {
            return if passthrough_on_no_match {
                TrackOutcome::passthrough()
            } else {
                TrackOutcome::no_match()
            };
        }

        let gesture_templates = &snapshots.gesture_templates;
        if gesture_templates.is_empty() {
            return if passthrough_on_no_match {
                TrackOutcome::passthrough()
            } else {
                TrackOutcome::no_match()
            };
        }

        let mut distances = HashMap::new();
        for (gesture_id, template) in gesture_templates {
            let similarity = direction_similarity(&track_dirs, &template.directions);
            let threshold = snapshots
                .settings
                .match_threshold_for_template_len(template.directions.len());
            if similarity < threshold {
                continue;
            }
            let distance = 1.0 - similarity;
            distances.insert(gesture_id.clone(), distance);
        }
        if distances.is_empty() {
            return if passthrough_on_no_match {
                TrackOutcome::passthrough()
            } else {
                TrackOutcome::no_match()
            };
        }

        let window_info = current_foreground_window();
        let Some(profile) = self.select_profile_cached(&snapshots.db, &window_info) else {
            return if passthrough_on_no_match {
                TrackOutcome::passthrough()
            } else {
                TrackOutcome::no_match()
            };
        };
        let Some(binding_match) =
            select_binding(profile, &distances, snapshots.settings.max_distance)
        else {
            return if passthrough_on_no_match {
                TrackOutcome::passthrough()
            } else {
                TrackOutcome::no_match()
            };
        };

        let template = gesture_templates
            .get(&binding_match.binding.gesture_id)
            .cloned();
        let event = MouseGestureEvent {
            gesture_id: binding_match.binding.gesture_id.clone(),
            gesture_name: template.and_then(|t| t.name),
            profile_id: profile.id.clone(),
            profile_label: profile.label.clone(),
            action_payload: binding_match.binding.action.clone(),
            action_args: binding_match.binding.args.clone(),
            distance: binding_match.distance,
        };
        self.event_sink.dispatch(event);
        TrackOutcome::matched()
    }
}

fn should_compute_preview(last: Instant, now: Instant, throttle_ms: u64) -> bool {
    now.duration_since(last) >= Duration::from_millis(throttle_ms)
}

fn should_cancel(started_at: Option<Instant>, now: Instant, max_duration_ms: u64) -> bool {
    if max_duration_ms == 0 {
        return false;
    }
    let Some(started_at) = started_at else {
        return false;
    };
    now.checked_duration_since(started_at)
        .is_some_and(|duration| duration >= Duration::from_millis(max_duration_ms))
}

fn sequence_changed(last_hash: Option<u64>, new_hash: u64) -> bool {
    last_hash.map_or(true, |last| last != new_hash)
}

fn hash_direction_sequence(directions: &[GestureDirection]) -> u64 {
    let mut hash = 0u64;
    for direction in directions {
        let value = match direction {
            GestureDirection::Up => 1u64,
            GestureDirection::Down => 2u64,
            GestureDirection::Left => 3u64,
            GestureDirection::Right => 4u64,
            GestureDirection::UpRight => 5u64,
            GestureDirection::UpLeft => 6u64,
            GestureDirection::DownRight => 7u64,
            GestureDirection::DownLeft => 8u64,
        };
        hash = hash.wrapping_mul(31).wrapping_add(value);
    }
    hash
}

fn track_displacement(points: &[Point]) -> f32 {
    if points.len() < 2 {
        return 0.0;
    }
    let first = points[0];
    let last = points[points.len() - 1];
    ((last.x - first.x).powi(2) + (last.y - first.y).powi(2)).sqrt()
}

fn is_tap_track(points: &[Point], tap_threshold_px: f32) -> bool {
    if tap_threshold_px <= 0.0 {
        return false;
    }
    let displacement = track_displacement(points);
    let length = track_length(points);
    displacement <= tap_threshold_px || length <= tap_threshold_px
}

fn finalize_hook_outcome(
    runtime: &MouseGestureRuntime,
    points: &[Point],
    too_long: bool,
    tap_threshold_px: f32,
) -> (TrackOutcome, bool) {
    if is_tap_track(points, tap_threshold_px) {
        return (TrackOutcome::passthrough(), true);
    }
    let outcome = runtime.evaluate_track_with_limit(points, too_long);
    if outcome.matched {
        (outcome, false)
    } else {
        (TrackOutcome::passthrough(), true)
    }
}

fn track_directions_with_override(
    points: &[Point],
    settings: &MouseGesturePluginSettings,
    displacement: Option<f32>,
    straightness: Option<f32>,
) -> Vec<GestureDirection> {
    if points.len() < 2 {
        return Vec::new();
    }
    let displacement = displacement.unwrap_or_else(|| track_displacement(points));
    let straightness = straightness.unwrap_or_else(|| straightness_ratio(points));
    if displacement >= settings.straightness_min_displacement_px
        && straightness >= settings.straightness_threshold
    {
        let first = points[0];
        let last = points[points.len() - 1];
        return vec![direction_from_vector(Vector {
            x: last.x - first.x,
            y: last.y - first.y,
        })];
    }
    let processed_points = preprocess_points_for_directions(points, settings);
    direction_sequence(&processed_points, settings)
}

fn truncate_with_ellipsis(input: &str, max_chars: usize) -> String {
    let mut chars = input.chars();
    let count = chars.clone().count();
    if count <= max_chars {
        return input.to_string();
    }
    if max_chars == 0 {
        return String::new();
    }
    if max_chars == 1 {
        return PREVIEW_SIMILARITY_ELLIPSIS.to_string();
    }
    let mut truncated: String = chars.by_ref().take(max_chars - 1).collect();
    truncated.push_str(PREVIEW_SIMILARITY_ELLIPSIS);
    truncated
}

#[derive(Clone)]
struct GestureTemplate {
    name: Option<String>,
    directions: Vec<GestureDirection>,
}

#[derive(Clone, Copy, Debug, Default)]
struct PreviewTextEncoder;

impl PreviewTextEncoder {
    fn truncate_label(&self, label: &str) -> String {
        truncate_with_ellipsis(label, PREVIEW_SIMILARITY_LABEL_MAX_LEN)
    }

    fn encode_similarity(&self, mut similarities: Vec<(f32, String)>) -> String {
        similarities.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(CmpOrdering::Equal));
        similarities.truncate(PREVIEW_SIMILARITY_TOP_N);
        let summary = similarities
            .into_iter()
            .map(|(similarity, label)| format!("{label}: {:.0}%", similarity * 100.0))
            .collect::<Vec<_>>()
            .join(" | ");
        let full_text = format!("{PREVIEW_SIMILARITY_PREFIX}{summary}");
        truncate_with_ellipsis(&full_text, PREVIEW_SIMILARITY_MAX_LEN)
    }
}

#[cfg(test)]
static TEMPLATE_BUILD_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn build_gesture_templates(
    db: &MouseGestureDb,
    settings: &MouseGesturePluginSettings,
) -> HashMap<String, GestureTemplate> {
    #[cfg(test)]
    {
        TEMPLATE_BUILD_COUNTER.fetch_add(1, Ordering::SeqCst);
    }

    let mut templates = HashMap::new();
    for (gesture_id, serialized) in &db.bindings {
        let parsed = match parse_gesture(serialized) {
            Ok(def) => def,
            Err(_) => continue,
        };
        let displacement = track_displacement(&parsed.points);
        let straightness = straightness_ratio(&parsed.points);
        let directions = track_directions_with_override(
            &parsed.points,
            settings,
            Some(displacement),
            Some(straightness),
        );
        if directions.is_empty() {
            continue;
        }
        templates.insert(
            gesture_id.clone(),
            GestureTemplate {
                name: parsed.name,
                directions,
            },
        );
    }
    templates
}

fn rebuild_gesture_templates(snapshots: &mut MouseGestureSnapshots) {
    snapshots.gesture_templates = build_gesture_templates(&snapshots.db, &snapshots.settings);
}

#[cfg(test)]
fn reset_template_build_counter() {
    TEMPLATE_BUILD_COUNTER.store(0, Ordering::SeqCst);
}

#[cfg(test)]
fn template_build_count() -> usize {
    TEMPLATE_BUILD_COUNTER.load(Ordering::SeqCst)
}

fn should_refresh_gesture_templates(
    current: &MouseGesturePluginSettings,
    updated: &MouseGesturePluginSettings,
) -> bool {
    current.min_point_distance != updated.min_point_distance
        || current.segment_threshold_px != updated.segment_threshold_px
        || current.direction_tolerance_deg != updated.direction_tolerance_deg
        || current.straightness_threshold != updated.straightness_threshold
        || current.straightness_min_displacement_px != updated.straightness_min_displacement_px
        || current.sampling_enabled != updated.sampling_enabled
        || current.smoothing_enabled != updated.smoothing_enabled
}

fn current_foreground_window() -> ForegroundWindowInfo {
    #[cfg(windows)]
    {
        use std::ffi::OsString;
        use std::os::windows::ffi::OsStringExt;
        use std::path::Path;
        use windows::core::PWSTR;
        use windows::Win32::Foundation::HWND;
        use windows::Win32::System::Threading::{
            OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
            PROCESS_QUERY_LIMITED_INFORMATION,
        };
        use windows::Win32::UI::WindowsAndMessaging::{
            GetClassNameW, GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW,
            GetWindowThreadProcessId,
        };

        fn window_title(hwnd: HWND) -> Option<String> {
            unsafe {
                let len = GetWindowTextLengthW(hwnd);
                if len <= 0 {
                    return None;
                }
                let mut buf = vec![0u16; len as usize + 1];
                let read = GetWindowTextW(hwnd, &mut buf);
                if read == 0 {
                    return None;
                }
                let title = String::from_utf16_lossy(&buf[..read as usize]);
                if title.trim().is_empty() {
                    None
                } else {
                    Some(title)
                }
            }
        }

        fn window_class(hwnd: HWND) -> Option<String> {
            unsafe {
                let mut buf = vec![0u16; 256];
                let len = GetClassNameW(hwnd, &mut buf) as usize;
                if len == 0 {
                    return None;
                }
                let class = String::from_utf16_lossy(&buf[..len]);
                if class.trim().is_empty() {
                    None
                } else {
                    Some(class)
                }
            }
        }

        fn window_exe(hwnd: HWND) -> Option<String> {
            unsafe {
                let mut pid = 0u32;
                let _ = GetWindowThreadProcessId(hwnd, Some(&mut pid));
                if pid == 0 {
                    return None;
                }
                let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
                let mut buffer = vec![0u16; 1024];
                let mut size = buffer.len() as u32;
                let success = QueryFullProcessImageNameW(
                    handle,
                    PROCESS_NAME_FORMAT(0),
                    PWSTR(buffer.as_mut_ptr()),
                    &mut size,
                )
                .is_ok();
                let _ = windows::Win32::Foundation::CloseHandle(handle);
                if !success || size == 0 {
                    return None;
                }
                let path = OsString::from_wide(&buffer[..size as usize])
                    .to_string_lossy()
                    .to_string();
                Path::new(path.as_str())
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
            }
        }

        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.0.is_null() {
                return ForegroundWindowInfo {
                    exe: None,
                    class: None,
                    title: None,
                };
            }
            ForegroundWindowInfo {
                exe: window_exe(hwnd),
                class: window_class(hwnd),
                title: window_title(hwnd),
            }
        }
    }
    #[cfg(not(windows))]
    {
        ForegroundWindowInfo {
            exe: None,
            class: None,
            title: None,
        }
    }
}

pub trait MouseGestureEventSink: Send + Sync {
    fn dispatch(&self, event: MouseGestureEvent);
}

struct GuiMouseGestureEventSink;

impl MouseGestureEventSink for GuiMouseGestureEventSink {
    fn dispatch(&self, event: MouseGestureEvent) {
        send_event(WatchEvent::MouseGesture(event));
    }
}

pub trait MouseHookBackend: Send + Sync {
    fn start(&self, runtime: MouseGestureRuntime) -> anyhow::Result<()>;
    fn stop(&self);
    fn is_running(&self) -> bool;
}

pub struct MouseGestureService {
    snapshots: Arc<RwLock<MouseGestureSnapshots>>,
    backend: Arc<dyn MouseHookBackend>,
    event_sink: Arc<dyn MouseGestureEventSink>,
    profile_cache: Arc<Mutex<ProfileCache>>,
    running: AtomicBool,
}

impl MouseGestureService {
    pub fn new_with_backend_and_sink(
        backend: Arc<dyn MouseHookBackend>,
        event_sink: Arc<dyn MouseGestureEventSink>,
    ) -> Self {
        Self {
            snapshots: Arc::new(RwLock::new(MouseGestureSnapshots::default())),
            backend,
            event_sink,
            profile_cache: Arc::new(Mutex::new(ProfileCache::default())),
            running: AtomicBool::new(false),
        }
    }

    pub fn new_with_backend(backend: Arc<dyn MouseHookBackend>) -> Self {
        Self::new_with_backend_and_sink(backend, Arc::new(GuiMouseGestureEventSink))
    }

    pub fn update_settings(&self, settings: MouseGesturePluginSettings) {
        if let Ok(mut guard) = self.snapshots.write() {
            if should_refresh_gesture_templates(&guard.settings, &settings) {
                guard.settings = settings.clone();
                rebuild_gesture_templates(&mut guard);
            } else {
                guard.settings = settings.clone();
            }
        }
        if let Ok(mut overlay) = mouse_gesture_overlay().lock() {
            overlay.update_settings(&settings);
            if !settings.enabled {
                overlay.end_stroke();
                overlay.update_preview(None, None);
            }
        }
        if settings.enabled {
            self.start();
        } else {
            self.stop();
        }
    }

    pub fn update_db(&self, db: MouseGestureDb) {
        if let Ok(mut guard) = self.snapshots.write() {
            guard.db = db;
            rebuild_gesture_templates(&mut guard);
        }
        if let Ok(mut cache) = self.profile_cache.lock() {
            cache.clear();
        }
    }

    pub fn settings_snapshot(&self) -> MouseGesturePluginSettings {
        self.snapshots
            .read()
            .map(|guard| guard.settings.clone())
            .unwrap_or_default()
    }

    pub fn hook_status(&self) -> MouseGestureHookStatus {
        let hook_active = self.running.load(Ordering::SeqCst);
        #[cfg(windows)]
        if let Some(state) = HOOK_STATE.get() {
            return state.diagnostics.status(hook_active);
        }
        MouseGestureHookStatus {
            hook_active,
            overlay_ready: false,
            last_event_at: None,
        }
    }

    pub fn start(&self) {
        if self.running.swap(true, Ordering::SeqCst) {
            return;
        }
        let runtime = MouseGestureRuntime {
            snapshots: Arc::clone(&self.snapshots),
            event_sink: Arc::clone(&self.event_sink),
            profile_cache: Arc::clone(&self.profile_cache),
        };
        if let Err(err) = self.backend.start(runtime) {
            self.running.store(false, Ordering::SeqCst);
            tracing::error!(?err, "failed to start mouse gesture backend");
        }
    }

    pub fn stop(&self) {
        if !self.running.swap(false, Ordering::SeqCst) {
            return;
        }
        self.backend.stop();
    }
}

static MOUSE_GESTURE_SERVICE: OnceCell<Arc<MouseGestureService>> = OnceCell::new();

pub fn mouse_gesture_service() -> Arc<MouseGestureService> {
    MOUSE_GESTURE_SERVICE
        .get_or_init(|| {
            Arc::new(MouseGestureService::new_with_backend(Arc::new(
                WindowsMouseHookBackend::default(),
            )))
        })
        .clone()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TriggerButton {
    Left,
    Right,
    Middle,
}

impl TriggerButton {
    fn from_setting(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            "middle" => Some(Self::Middle),
            _ => None,
        }
    }
}

#[cfg(windows)]
fn trigger_button_down(button: TriggerButton) -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_LBUTTON, VK_MBUTTON, VK_RBUTTON,
    };

    let virtual_key = match button {
        TriggerButton::Left => VK_LBUTTON,
        TriggerButton::Right => VK_RBUTTON,
        TriggerButton::Middle => VK_MBUTTON,
    };
    const KEY_PRESSED_MASK: i16 = 0x8000u16 as i16;
    unsafe { (GetAsyncKeyState(virtual_key.0 as i32) & KEY_PRESSED_MASK) != 0 }
}

#[cfg(windows)]
fn cursor_position() -> Point {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

    let mut pt = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut pt);
    }
    Point {
        x: pt.x as f32,
        y: pt.y as f32,
    }
}

#[cfg(windows)]
#[derive(Default)]
pub struct WindowsMouseHookBackend {
    running: Arc<AtomicBool>,
    stop_flag: Arc<AtomicBool>,
    thread_id: Arc<AtomicUsize>,
    handle: Arc<Mutex<Option<std::thread::JoinHandle<()>>>>,
}

const MIN_DECIMATION_DISTANCE_SQ: f32 = 4.0;
const TRACKING_START_DISTANCE_SQ: f32 = 9.0;
const TRACKING_WATCHDOG_FALLBACK_MS: u64 = 10_000;
const VISUAL_POINT_DISTANCE_SQ: f32 = 4.0;
pub const MAX_TRACK_POINTS: usize = 4096;

struct HookDiagnosticsSnapshot {
    total_hook_callbacks: usize,
    rbutton_down: usize,
    rbutton_up: usize,
    mousemove_while_tracking: usize,
    start_requested: usize,
    start_failed: usize,
    start_received: usize,
    tracking_started: usize,
    tracking_canceled_timeout: usize,
    tracking_ended: usize,
    stored_point_accepted: usize,
    overlay_ready: bool,
    last_event_ms: Option<u64>,
}

struct HookDiagnostics {
    total_hook_callbacks: AtomicUsize,
    rbutton_down: AtomicUsize,
    rbutton_up: AtomicUsize,
    mousemove_while_tracking: AtomicUsize,
    start_requested: AtomicUsize,
    start_failed: AtomicUsize,
    start_received: AtomicUsize,
    tracking_started: AtomicUsize,
    tracking_canceled_timeout: AtomicUsize,
    tracking_ended: AtomicUsize,
    stored_point_accepted: AtomicUsize,
    last_log_ms: AtomicUsize,
    last_event_ms: AtomicU64,
    overlay_ready: AtomicBool,
    started_at: Instant,
}

impl Default for HookDiagnostics {
    fn default() -> Self {
        Self {
            total_hook_callbacks: AtomicUsize::new(0),
            rbutton_down: AtomicUsize::new(0),
            rbutton_up: AtomicUsize::new(0),
            mousemove_while_tracking: AtomicUsize::new(0),
            start_requested: AtomicUsize::new(0),
            start_failed: AtomicUsize::new(0),
            start_received: AtomicUsize::new(0),
            tracking_started: AtomicUsize::new(0),
            tracking_canceled_timeout: AtomicUsize::new(0),
            tracking_ended: AtomicUsize::new(0),
            stored_point_accepted: AtomicUsize::new(0),
            last_log_ms: AtomicUsize::new(0),
            last_event_ms: AtomicU64::new(0),
            overlay_ready: AtomicBool::new(false),
            started_at: Instant::now(),
        }
    }
}

impl HookDiagnostics {
    fn snapshot(&self) -> HookDiagnosticsSnapshot {
        HookDiagnosticsSnapshot {
            total_hook_callbacks: self.total_hook_callbacks.load(Ordering::SeqCst),
            rbutton_down: self.rbutton_down.load(Ordering::SeqCst),
            rbutton_up: self.rbutton_up.load(Ordering::SeqCst),
            mousemove_while_tracking: self.mousemove_while_tracking.load(Ordering::SeqCst),
            start_requested: self.start_requested.load(Ordering::SeqCst),
            start_failed: self.start_failed.load(Ordering::SeqCst),
            start_received: self.start_received.load(Ordering::SeqCst),
            tracking_started: self.tracking_started.load(Ordering::SeqCst),
            tracking_canceled_timeout: self.tracking_canceled_timeout.load(Ordering::SeqCst),
            tracking_ended: self.tracking_ended.load(Ordering::SeqCst),
            stored_point_accepted: self.stored_point_accepted.load(Ordering::SeqCst),
            overlay_ready: self.overlay_ready.load(Ordering::SeqCst),
            last_event_ms: self.last_event_at().map(|instant| {
                instant
                    .saturating_duration_since(self.started_at)
                    .as_millis() as u64
            }),
        }
    }

    fn record_hook_event(&self, overlay_ready: bool) {
        let elapsed_ms = self.started_at.elapsed().as_millis() as u64;
        self.last_event_ms
            .store(elapsed_ms.saturating_add(1), Ordering::SeqCst);
        self.overlay_ready.store(overlay_ready, Ordering::SeqCst);
    }

    fn last_event_at(&self) -> Option<Instant> {
        let stored = self.last_event_ms.load(Ordering::SeqCst);
        if stored == 0 {
            None
        } else {
            Some(self.started_at + Duration::from_millis(stored.saturating_sub(1)))
        }
    }

    fn status(&self, hook_active: bool) -> MouseGestureHookStatus {
        MouseGestureHookStatus {
            hook_active,
            overlay_ready: self.overlay_ready.load(Ordering::SeqCst),
            last_event_at: self.last_event_at(),
        }
    }

    fn maybe_log(&self, enabled: bool) {
        if !enabled {
            return;
        }
        let elapsed_ms = self.started_at.elapsed().as_millis() as usize;
        let last_log = self.last_log_ms.load(Ordering::Relaxed);
        if elapsed_ms.saturating_sub(last_log) < 1_000 {
            return;
        }
        if self
            .last_log_ms
            .compare_exchange(last_log, elapsed_ms, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }
        let snapshot = self.snapshot();
        tracing::debug!(
            total_hook_callbacks = snapshot.total_hook_callbacks,
            rbutton_down = snapshot.rbutton_down,
            rbutton_up = snapshot.rbutton_up,
            mousemove_while_tracking = snapshot.mousemove_while_tracking,
            start_requested = snapshot.start_requested,
            start_failed = snapshot.start_failed,
            start_received = snapshot.start_received,
            tracking_started = snapshot.tracking_started,
            tracking_canceled_timeout = snapshot.tracking_canceled_timeout,
            tracking_ended = snapshot.tracking_ended,
            stored_point_accepted = snapshot.stored_point_accepted,
            overlay_ready = snapshot.overlay_ready,
            last_event_ms = snapshot.last_event_ms,
            "mouse gesture hook diagnostics"
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MouseGestureHookStatus {
    pub hook_active: bool,
    pub overlay_ready: bool,
    pub last_event_at: Option<Instant>,
}

pub fn format_mouse_gesture_hook_status(status: &MouseGestureHookStatus, now: Instant) -> String {
    let hook_active = if status.hook_active { "yes" } else { "no" };
    let overlay_ready = if status.overlay_ready { "yes" } else { "no" };
    let last_event = match status.last_event_at {
        Some(at) => format!("{}s ago", now.saturating_duration_since(at).as_secs()),
        None => "never".to_string(),
    };
    format!(
        "Hook active: {hook_active} • Last event: {last_event} • Overlay ready: {overlay_ready}"
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TrackingState {
    Idle,
    Armed,
    Tracking,
    Finalizing,
    Canceled,
}

pub struct HookTrackingState {
    tracking_state: TrackingState,
    active_button: Option<TriggerButton>,
    points: Vec<Point>,
    last_point: Option<Point>,
    last_stored_point: Option<Point>,
    last_visual_point: Option<Point>,
    visual_points: usize,
    trigger_started_at: Option<Instant>,
    trigger_start_point: Option<Point>,
    tracking_started_at: Option<Instant>,
    acc_len: f32,
    too_long: bool,
    stored_points: usize,
    decimation_stride: usize,
    diagnostics: Option<Arc<HookDiagnostics>>,
}

#[cfg(any(test, windows))]
#[derive(Clone)]
struct PreviewRequest {
    points: Vec<Point>,
    force: bool,
}

#[cfg(any(test, windows))]
fn spawn_preview_worker(
    runtime: MouseGestureRuntime,
) -> (SyncSender<PreviewRequest>, Arc<Mutex<Option<String>>>) {
    let (sender, receiver) = mpsc::sync_channel(1);
    let preview_text = Arc::new(Mutex::new(None));
    let preview_text_handle = Arc::clone(&preview_text);
    std::thread::spawn(move || preview_worker_loop(runtime, receiver, preview_text_handle));
    (sender, preview_text)
}

#[cfg(any(test, windows))]
fn preview_worker_loop(
    runtime: MouseGestureRuntime,
    receiver: Receiver<PreviewRequest>,
    preview_text: Arc<Mutex<Option<String>>>,
) {
    let encoder = PreviewTextEncoder::default();
    let mut last_preview_at: Option<Instant> = None;
    let mut last_sequence_hash = None;
    for request in receiver {
        let now = Instant::now();
        let snapshots = runtime
            .snapshots
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_default();
        let throttle_ms = snapshots.settings.preview_throttle_ms();
        if !snapshots.settings.preview_enabled {
            continue;
        }
        let directions =
            track_directions_with_override(&request.points, &snapshots.settings, None, None);
        let sequence_hash = hash_direction_sequence(&directions);

        if snapshots.settings.preview_on_end_only && !request.force {
            continue;
        }

        if !request.force {
            if !sequence_changed(last_sequence_hash, sequence_hash) {
                continue;
            }
            if last_preview_at.is_some_and(|last| !should_compute_preview(last, now, throttle_ms)) {
                continue;
            }
        }

        let text = runtime.preview_text_with_snapshots(&request.points, &snapshots, &encoder);
        if let Ok(mut guard) = preview_text.lock() {
            *guard = text;
        }
        last_preview_at = Some(now);
        last_sequence_hash = Some(sequence_hash);
    }
}

#[cfg(windows)]
fn cached_preview_text(preview_text: &Arc<Mutex<Option<String>>>) -> Option<String> {
    preview_text.lock().ok().and_then(|guard| guard.clone())
}

#[cfg(any(test, windows))]
#[derive(Clone, Copy)]
struct SamplerConfig {
    min_point_distance_sq: f32,
    visual_point_distance_sq: f32,
    max_track_len: f32,
    max_gesture_duration_ms: u64,
    max_sample_count: usize,
    preview_enabled: bool,
    preview_on_end_only: bool,
    preview_throttle_ms: u64,
    sample_interval: Duration,
}

#[cfg(any(test, windows))]
enum SamplerCommand {
    Start {
        start_point: Point,
        trigger_button: Option<TriggerButton>,
        config: SamplerConfig,
    },
    Shutdown,
}

#[cfg(any(test, windows))]
fn cancel_tracking_state(
    tracking: &Arc<Mutex<HookTrackingState>>,
    preview_text: &Arc<Mutex<Option<String>>>,
    tracking_active: &Arc<AtomicBool>,
    diagnostics: &Arc<HookDiagnostics>,
    timed_out: bool,
) -> TrackOutcome {
    let outcome = tracking
        .lock()
        .map(|mut guard| guard.cancel_tracking())
        .unwrap_or_else(|_| TrackOutcome::no_match());
    if timed_out {
        diagnostics
            .tracking_canceled_timeout
            .fetch_add(1, Ordering::SeqCst);
    }
    clear_overlay_and_preview(preview_text);
    tracking_active.store(false, Ordering::SeqCst);
    outcome
}

#[cfg(any(test, windows))]
fn request_sampler_start(
    sampler_sender: &SyncSender<SamplerCommand>,
    tracking: &Arc<Mutex<HookTrackingState>>,
    preview_text: &Arc<Mutex<Option<String>>>,
    tracking_active: &Arc<AtomicBool>,
    stop_flag: &Arc<AtomicBool>,
    diagnostics: &Arc<HookDiagnostics>,
    start_point: Point,
    trigger_button: Option<TriggerButton>,
    config: SamplerConfig,
) -> bool {
    diagnostics.start_requested.fetch_add(1, Ordering::SeqCst);
    stop_flag.store(false, Ordering::SeqCst);
    match sampler_sender.try_send(SamplerCommand::Start {
        start_point,
        trigger_button,
        config,
    }) {
        Ok(()) => {
            tracking_active.store(true, Ordering::SeqCst);
            true
        }
        Err(_) => {
            diagnostics.start_failed.fetch_add(1, Ordering::SeqCst);
            let _ = cancel_tracking_state(tracking, preview_text, tracking_active, diagnostics, false);
            false
        }
    }
}

#[cfg(any(test, windows))]
fn spawn_sampler_worker(
    runtime: MouseGestureRuntime,
    preview_sender: SyncSender<PreviewRequest>,
    preview_text: Arc<Mutex<Option<String>>>,
    tracking: Arc<Mutex<HookTrackingState>>,
    tracking_active: Arc<AtomicBool>,
    stop_flag: Arc<AtomicBool>,
    diagnostics: Arc<HookDiagnostics>,
    cursor_provider: impl Fn() -> Point + Send + Sync + 'static,
    trigger_down: impl Fn(TriggerButton) -> bool + Send + Sync + 'static,
) -> SyncSender<SamplerCommand> {
    let (sender, receiver) = mpsc::sync_channel(2);
    std::thread::spawn(move || {
        sampler_worker_loop(
            runtime,
            receiver,
            preview_sender,
            preview_text,
            tracking,
            tracking_active,
            stop_flag,
            diagnostics,
            cursor_provider,
            trigger_down,
        );
    });
    sender
}

#[cfg(any(test, windows))]
fn sampler_worker_loop(
    _runtime: MouseGestureRuntime,
    receiver: Receiver<SamplerCommand>,
    preview_sender: SyncSender<PreviewRequest>,
    preview_text: Arc<Mutex<Option<String>>>,
    tracking: Arc<Mutex<HookTrackingState>>,
    tracking_active: Arc<AtomicBool>,
    stop_flag: Arc<AtomicBool>,
    diagnostics: Arc<HookDiagnostics>,
    cursor_provider: impl Fn() -> Point + Send + Sync + 'static,
    trigger_down: impl Fn(TriggerButton) -> bool + Send + Sync + 'static,
) {
    for command in receiver {
        match command {
            SamplerCommand::Start {
                start_point,
                trigger_button,
                config,
            } => {
                diagnostics.start_received.fetch_add(1, Ordering::SeqCst);
                {
                    let Ok(mut guard) = tracking.lock() else {
                        continue;
                    };
                    if !matches!(
                        guard.tracking_state,
                        TrackingState::Idle | TrackingState::Armed
                    ) {
                        continue;
                    }
                    guard.begin_stroke(trigger_button, start_point);
                }
                diagnostics.tracking_started.fetch_add(1, Ordering::SeqCst);
                stop_flag.store(false, Ordering::SeqCst);
                tracking_active.store(true, Ordering::SeqCst);
                if let Ok(mut preview_guard) = preview_text.lock() {
                    *preview_guard = None;
                }
                if let Ok(mut overlay) = mouse_gesture_overlay().try_lock() {
                    overlay.begin_stroke(start_point);
                    overlay.update_preview(None, None);
                }
                let mut last_preview_at =
                    Instant::now() - Duration::from_millis(config.preview_throttle_ms);
                loop {
                    if stop_flag.load(Ordering::SeqCst) {
                        break;
                    }
                    let now = Instant::now();
                    let point = cursor_provider();
                    let mut stored = None;
                    let mut overlay_point = None;
                    let mut should_cancel_tracking = false;
                    let mut timed_out = false;
                    let mut active_button = None;
                    if let Ok(mut guard) = tracking.lock() {
                        if guard.active_button.is_none() {
                            break;
                        }
                        active_button = guard.active_button;
                        guard.update_tracking_started(point, TRACKING_START_DISTANCE_SQ, now);
                        let watchdog_limit_ms = if config.max_gesture_duration_ms > 0 {
                            config.max_gesture_duration_ms
                        } else {
                            TRACKING_WATCHDOG_FALLBACK_MS
                        };
                        if guard.should_watchdog_cancel(now, watchdog_limit_ms) {
                            timed_out = true;
                            should_cancel_tracking = true;
                        }
                        if should_cancel_tracking {
                            // Fall through to cancellation outside the tracking lock.
                        } else {
                            let visual_stored =
                                guard.record_visual_point(point, config.visual_point_distance_sq);
                            let recognition_stored = guard.sample_point(
                                point,
                                config.min_point_distance_sq,
                                config.max_track_len,
                                config.max_sample_count,
                            );
                            if recognition_stored && !visual_stored {
                                guard.force_visual_point(point);
                            }
                            if recognition_stored {
                                stored = Some((point, guard.points.clone()));
                            }
                            if visual_stored || recognition_stored {
                                overlay_point = Some(point);
                            }
                        }
                    }
                    if !should_cancel_tracking {
                        if let Some(button) = active_button {
                            if !trigger_down(button) {
                                should_cancel_tracking = true;
                            }
                        }
                    }
                    if should_cancel_tracking {
                        let _ = cancel_tracking_state(
                            &tracking,
                            &preview_text,
                            &tracking_active,
                            &diagnostics,
                            timed_out,
                        );
                        break;
                    }
                    if let Some(point) = overlay_point {
                        if let Ok(mut overlay) = mouse_gesture_overlay().try_lock() {
                            overlay.push_point(point);
                        }
                    }
                    if let Some((point, points)) = stored {
                        if config.preview_enabled && !config.preview_on_end_only {
                            let _ = preview_sender.try_send(PreviewRequest {
                                points: points.clone(),
                                force: false,
                            });
                        }
                        if let Ok(mut overlay) = mouse_gesture_overlay().try_lock() {
                            if config.preview_enabled
                                && !config.preview_on_end_only
                                && should_compute_preview(
                                    last_preview_at,
                                    now,
                                    config.preview_throttle_ms,
                                )
                            {
                                let text = cached_preview_text(&preview_text);
                                overlay.update_preview(text, Some(point));
                                last_preview_at = now;
                            }
                        }
                    }
                    std::thread::sleep(config.sample_interval);
                }
                tracking_active.store(false, Ordering::SeqCst);
                if stop_flag.load(Ordering::SeqCst) {
                    if tracking
                        .try_lock()
                        .ok()
                        .is_some_and(|guard| guard.active_button.is_some())
                    {
                        let _ = cancel_tracking_state(
                            &tracking,
                            &preview_text,
                            &tracking_active,
                            &diagnostics,
                            false,
                        );
                    }
                }
            }
            SamplerCommand::Shutdown => break,
        }
    }
}

#[cfg(any(test, windows))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HookEventKind {
    TriggerDown,
    TriggerUp,
    MouseMove,
    Other,
}

#[cfg(any(test, windows))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HookAction {
    Start,
    Stop,
    Ignore,
}

#[cfg(any(test, windows))]
fn hook_action_for_event(
    event_kind: HookEventKind,
    event_button: Option<TriggerButton>,
    trigger_button: Option<TriggerButton>,
) -> HookAction {
    match event_kind {
        HookEventKind::TriggerDown if event_button == trigger_button => HookAction::Start,
        HookEventKind::TriggerUp if event_button == trigger_button => HookAction::Stop,
        // Mouse moves are ignored here; the sampler thread updates tracking + overlay visuals.
        HookEventKind::MouseMove => HookAction::Ignore,
        HookEventKind::Other => HookAction::Ignore,
        _ => HookAction::Ignore,
    }
}

impl Default for HookTrackingState {
    fn default() -> Self {
        Self {
            tracking_state: TrackingState::Idle,
            active_button: None,
            points: Vec::new(),
            last_point: None,
            last_stored_point: None,
            last_visual_point: None,
            visual_points: 0,
            trigger_started_at: None,
            trigger_start_point: None,
            tracking_started_at: None,
            acc_len: 0.0,
            too_long: false,
            stored_points: 0,
            decimation_stride: 1,
            diagnostics: None,
        }
    }
}

impl HookTrackingState {
    fn attach_diagnostics(&mut self, diagnostics: Arc<HookDiagnostics>) {
        self.diagnostics = Some(diagnostics);
    }

    fn reset_tracking(&mut self) {
        self.tracking_state = TrackingState::Idle;
        self.active_button = None;
        self.points.clear();
        self.last_point = None;
        self.last_stored_point = None;
        self.last_visual_point = None;
        self.visual_points = 0;
        self.trigger_started_at = None;
        self.trigger_start_point = None;
        self.tracking_started_at = None;
        self.acc_len = 0.0;
        self.too_long = false;
        self.stored_points = 0;
        self.decimation_stride = 1;
    }

    fn clear_tracking_buffers(&mut self) {
        self.active_button = None;
        self.points.clear();
        self.last_point = None;
        self.last_stored_point = None;
        self.last_visual_point = None;
        self.visual_points = 0;
        self.tracking_started_at = None;
        self.acc_len = 0.0;
        self.too_long = false;
        self.stored_points = 0;
        self.decimation_stride = 1;
    }

    fn update_tracking_started(&mut self, point: Point, threshold_sq: f32, now: Instant) {
        if self.tracking_started_at.is_some() {
            return;
        }
        let start_point = self.trigger_start_point.or(self.last_point);
        let Some(start_point) = start_point else {
            return;
        };
        let dx = point.x - start_point.x;
        let dy = point.y - start_point.y;
        if (dx * dx + dy * dy) >= threshold_sq {
            self.tracking_started_at = Some(now);
            self.tracking_state = TrackingState::Tracking;
        }
    }

    fn should_watchdog_cancel(&self, now: Instant, max_duration_ms: u64) -> bool {
        if max_duration_ms == 0 {
            return false;
        }
        let started_at = match self.tracking_state {
            TrackingState::Armed => self.trigger_started_at,
            TrackingState::Tracking => self.tracking_started_at.or(self.trigger_started_at),
            _ => None,
        };
        should_cancel(started_at, now, max_duration_ms)
    }

    fn cancel_tracking(&mut self) -> TrackOutcome {
        self.tracking_state = TrackingState::Canceled;
        self.reset_tracking();
        TrackOutcome::no_match()
    }

    fn update_length(&mut self, point: Point) {
        if let Some(last_point) = self.last_point {
            let dx = point.x - last_point.x;
            let dy = point.y - last_point.y;
            self.acc_len += (dx * dx + dy * dy).sqrt();
        }
        self.last_point = Some(point);
    }

    fn should_store_point(&self, point: Point, min_point_distance_sq: f32) -> bool {
        let distance_sq = min_point_distance_sq.max(MIN_DECIMATION_DISTANCE_SQ);
        match self.last_stored_point {
            None => true,
            Some(last_stored_point) => {
                let dx = point.x - last_stored_point.x;
                let dy = point.y - last_stored_point.y;
                (dx * dx + dy * dy) >= distance_sq
            }
        }
    }

    fn should_store_visual_point(&self, point: Point, min_visual_distance_sq: f32) -> bool {
        match self.last_visual_point {
            None => true,
            Some(last_visual_point) => {
                let dx = point.x - last_visual_point.x;
                let dy = point.y - last_visual_point.y;
                (dx * dx + dy * dy) >= min_visual_distance_sq
            }
        }
    }

    fn record_visual_point(&mut self, point: Point, min_visual_distance_sq: f32) -> bool {
        if !self.should_store_visual_point(point, min_visual_distance_sq) {
            return false;
        }
        self.last_visual_point = Some(point);
        self.visual_points = self.visual_points.saturating_add(1);
        true
    }

    fn force_visual_point(&mut self, point: Point) {
        self.last_visual_point = Some(point);
        self.visual_points = self.visual_points.saturating_add(1);
    }

    fn max_allowed_points(max_sample_count: usize) -> usize {
        if max_sample_count == 0 {
            MAX_TRACK_POINTS
        } else {
            max_sample_count.min(MAX_TRACK_POINTS)
        }
    }

    fn store_point(&mut self, point: Point, max_sample_count: usize) -> bool {
        let max_points = Self::max_allowed_points(max_sample_count);
        self.stored_points = self.stored_points.saturating_add(1);
        if self.points.len() < max_points {
            self.points.push(point);
            self.last_stored_point = Some(point);
            return true;
        }
        if self.decimation_stride == 1 {
            self.decimation_stride = 2;
        }
        // Once we hit the cap, keep the buffer size fixed and only replace the last point every
        // Nth accepted sample. This bounds memory while still refreshing the tail of long tracks.
        if self.stored_points % self.decimation_stride == 0 {
            if let Some(last) = self.points.last_mut() {
                *last = point;
            }
            self.last_stored_point = Some(point);
            return true;
        }
        false
    }

    fn begin_stroke(&mut self, button: Option<TriggerButton>, point: Point) {
        self.clear_tracking_buffers();
        self.tracking_state = TrackingState::Armed;
        self.active_button = button;
        self.last_point = Some(point);
        self.last_stored_point = Some(point);
        self.last_visual_point = Some(point);
        if self.trigger_started_at.is_none() {
            self.trigger_started_at = Some(Instant::now());
        }
        self.trigger_start_point = Some(point);
        self.points.push(point);
        self.stored_points = 1;
        self.visual_points = 1;
    }

    pub fn begin_track(&mut self, point: Point) {
        self.begin_stroke(None, point);
    }

    pub fn handle_move(
        &mut self,
        point: Point,
        min_point_distance_sq: f32,
        max_track_len: f32,
        max_sample_count: usize,
    ) -> bool {
        self.update_length(point);
        if max_track_len > 0.0 && self.acc_len > max_track_len {
            self.too_long = true;
            return false;
        }
        if self.too_long || !self.should_store_point(point, min_point_distance_sq) {
            return false;
        }
        let stored = self.store_point(point, max_sample_count);
        if stored {
            if let Some(diagnostics) = self.diagnostics.as_ref() {
                diagnostics
                    .stored_point_accepted
                    .fetch_add(1, Ordering::SeqCst);
            }
        }
        stored
    }

    pub fn sample_point(
        &mut self,
        point: Point,
        min_point_distance_sq: f32,
        max_track_len: f32,
        max_sample_count: usize,
    ) -> bool {
        self.handle_move(
            point,
            min_point_distance_sq,
            max_track_len,
            max_sample_count,
        )
    }

    pub fn finish_stroke(
        &mut self,
        point: Point,
        min_point_distance_sq: f32,
        max_track_len: f32,
        max_sample_count: usize,
    ) -> (Vec<Point>, bool) {
        self.tracking_state = TrackingState::Finalizing;
        self.update_length(point);
        if max_track_len > 0.0 && self.acc_len > max_track_len {
            self.too_long = true;
        }
        if !self.too_long && self.should_store_point(point, min_point_distance_sq) {
            let _ = self.store_point(point, max_sample_count);
        }
        let points = std::mem::take(&mut self.points);
        let too_long = self.too_long;
        self.reset_tracking();
        (points, too_long)
    }

    pub fn points_len(&self) -> usize {
        self.points.len()
    }

    #[cfg(test)]
    fn visual_points_len(&self) -> usize {
        self.visual_points
    }

    pub fn acc_len(&self) -> f32 {
        self.acc_len
    }

    pub fn too_long(&self) -> bool {
        self.too_long
    }
}

#[cfg(any(test, windows))]
fn clear_overlay_and_preview(preview_text: &Arc<Mutex<Option<String>>>) {
    if let Ok(mut preview_guard) = preview_text.lock() {
        *preview_guard = None;
    }
    if let Ok(mut overlay) = mouse_gesture_overlay().try_lock() {
        overlay.end_stroke();
        overlay.update_preview(None, None);
    }
}

#[cfg(windows)]
struct HookState {
    runtime: MouseGestureRuntime,
    preview_sender: SyncSender<PreviewRequest>,
    preview_text: Arc<Mutex<Option<String>>>,
    tracking: Arc<Mutex<HookTrackingState>>,
    tracking_active: Arc<AtomicBool>,
    stop_flag: Arc<AtomicBool>,
    sampler_sender: SyncSender<SamplerCommand>,
    diagnostics: Arc<HookDiagnostics>,
}

#[cfg(windows)]
static HOOK_STATE: OnceCell<Arc<HookState>> = OnceCell::new();

#[cfg(windows)]
impl MouseHookBackend for WindowsMouseHookBackend {
    fn start(&self, runtime: MouseGestureRuntime) -> anyhow::Result<()> {
        if self.running.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        self.stop_flag.store(false, Ordering::SeqCst);
        let runtime_state = HOOK_STATE.get_or_init(|| {
            let (preview_sender, preview_text) = spawn_preview_worker(runtime.clone());
            let tracking = Arc::new(Mutex::new(HookTrackingState::default()));
            let diagnostics = Arc::new(HookDiagnostics::default());
            if let Ok(mut guard) = tracking.lock() {
                guard.attach_diagnostics(Arc::clone(&diagnostics));
            }
            let tracking_active = Arc::new(AtomicBool::new(false));
            let stop_flag = Arc::new(AtomicBool::new(false));
            let sampler_sender = spawn_sampler_worker(
                runtime.clone(),
                preview_sender.clone(),
                Arc::clone(&preview_text),
                Arc::clone(&tracking),
                Arc::clone(&tracking_active),
                Arc::clone(&stop_flag),
                Arc::clone(&diagnostics),
                cursor_position,
                trigger_button_down,
            );
            Arc::new(HookState {
                runtime,
                preview_sender,
                preview_text,
                tracking,
                tracking_active,
                stop_flag,
                sampler_sender,
                diagnostics,
            })
        });
        if let Ok(mut tracking) = runtime_state.tracking.lock() {
            *tracking = HookTrackingState::default();
            tracking.attach_diagnostics(Arc::clone(&runtime_state.diagnostics));
        }
        runtime_state.tracking_active.store(false, Ordering::SeqCst);
        runtime_state.stop_flag.store(false, Ordering::SeqCst);

        let stop_flag = Arc::clone(&self.stop_flag);
        let thread_id = Arc::clone(&self.thread_id);
        let handle = std::thread::spawn(move || {
            use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
            use windows::Win32::UI::WindowsAndMessaging::{
                CallNextHookEx, DispatchMessageW, GetMessageW, PostThreadMessageW,
                SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, HC_ACTION, MSG,
                MSLLHOOKSTRUCT, WH_MOUSE_LL, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN,
                WM_MBUTTONUP, WM_MOUSEMOVE, WM_QUIT, WM_RBUTTONDOWN, WM_RBUTTONUP,
            };

            unsafe extern "system" fn hook_proc(
                code: i32,
                wparam: WPARAM,
                lparam: LPARAM,
            ) -> LRESULT {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    if code != HC_ACTION as i32 {
                        return CallNextHookEx(None, code, wparam, lparam);
                    }
                    let Some(state) = HOOK_STATE.get() else {
                        return CallNextHookEx(None, code, wparam, lparam);
                    };
                    let event = wparam.0 as u32;
                    let data = &*(lparam.0 as *const MSLLHOOKSTRUCT);
                    let overlay_ready = mouse_gesture_overlay()
                        .try_lock()
                        .map(|_| true)
                        .unwrap_or(false);
                    state.diagnostics.record_hook_event(overlay_ready);
                    if should_ignore_event(data.flags, data.dwExtraInfo) {
                        return CallNextHookEx(None, code, wparam, lparam);
                    }
                    let point = Point {
                        x: data.pt.x as f32,
                        y: data.pt.y as f32,
                    };
                    let (
                        trigger_button,
                        min_point_distance,
                        max_track_len,
                        max_gesture_duration_ms,
                        max_sample_count,
                        gestures_enabled,
                        preview_enabled,
                        preview_on_end_only,
                        preview_throttle_ms,
                        sample_interval,
                        tap_threshold_px,
                    ) = state
                        .runtime
                        .snapshots
                        .try_read()
                        .ok()
                        .map(|snap| {
                            let settings = &snap.settings;
                            (
                                TriggerButton::from_setting(&settings.trigger_button),
                                settings.min_point_distance.max(0.0),
                                settings.max_track_len.max(0.0),
                                settings.max_gesture_duration_ms,
                                settings.max_sample_count,
                                settings.enabled,
                                settings.preview_enabled,
                                settings.preview_on_end_only,
                                settings.preview_throttle_ms(),
                                Duration::from_millis(settings.sample_interval_ms()),
                                settings.tap_threshold_px.max(0.0),
                            )
                        })
                        .unwrap_or((
                            None,
                            0.0,
                            0.0,
                            0,
                            0,
                            false,
                            false,
                            false,
                            crate::plugins::mouse_gestures::settings::PREVIEW_THROTTLE_MS,
                            Duration::from_millis(16),
                            0.0,
                        ));
                    let min_point_distance_sq = min_point_distance * min_point_distance;

                    state
                        .diagnostics
                        .total_hook_callbacks
                        .fetch_add(1, Ordering::SeqCst);
                    if event == WM_RBUTTONDOWN {
                        state
                            .diagnostics
                            .rbutton_down
                            .fetch_add(1, Ordering::SeqCst);
                    }
                    if event == WM_RBUTTONUP {
                        state.diagnostics.rbutton_up.fetch_add(1, Ordering::SeqCst);
                    }
                    if event == WM_MOUSEMOVE && state.tracking_active.load(Ordering::SeqCst) {
                        state
                            .diagnostics
                            .mousemove_while_tracking
                            .fetch_add(1, Ordering::SeqCst);
                    }
                    state.diagnostics.maybe_log(gestures_enabled);

                    let event_button = match event {
                        WM_LBUTTONDOWN | WM_LBUTTONUP => Some(TriggerButton::Left),
                        WM_RBUTTONDOWN | WM_RBUTTONUP => Some(TriggerButton::Right),
                        WM_MBUTTONDOWN | WM_MBUTTONUP => Some(TriggerButton::Middle),
                        _ => None,
                    };

                    let event_kind = match event {
                        WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN => {
                            HookEventKind::TriggerDown
                        }
                        WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP => HookEventKind::TriggerUp,
                        WM_MOUSEMOVE => HookEventKind::MouseMove,
                        _ => HookEventKind::Other,
                    };

                    match hook_action_for_event(event_kind, event_button, trigger_button) {
                        HookAction::Start => {
                            let config = SamplerConfig {
                                min_point_distance_sq,
                                visual_point_distance_sq: VISUAL_POINT_DISTANCE_SQ,
                                max_track_len,
                                max_gesture_duration_ms,
                                max_sample_count,
                                preview_enabled,
                                preview_on_end_only,
                                preview_throttle_ms,
                                sample_interval,
                            };
                            request_sampler_start(
                                &state.sampler_sender,
                                &state.tracking,
                                &state.preview_text,
                                &state.tracking_active,
                                &state.stop_flag,
                                &state.diagnostics,
                                point,
                                event_button,
                                config,
                            );
                            return LRESULT(1);
                        }
                        HookAction::Stop => {
                            state.stop_flag.store(true, Ordering::SeqCst);
                            let Ok(mut tracking) = state.tracking.try_lock() else {
                                return CallNextHookEx(None, code, wparam, lparam);
                            };
                            if tracking.tracking_state == TrackingState::Idle
                                || tracking.active_button != event_button
                            {
                                return CallNextHookEx(None, code, wparam, lparam);
                            }
                            let (points, too_long) = tracking.finish_stroke(
                                point,
                                min_point_distance_sq,
                                max_track_len,
                                max_sample_count,
                            );
                            state
                                .diagnostics
                                .tracking_ended
                                .fetch_add(1, Ordering::SeqCst);

                            if preview_enabled {
                                let _ = state.preview_sender.try_send(PreviewRequest {
                                    points: points.clone(),
                                    force: true,
                                });
                            }

                            clear_overlay_and_preview(&state.preview_text);

                            let (outcome, should_inject) = finalize_hook_outcome(
                                &state.runtime,
                                &points,
                                too_long,
                                tap_threshold_px,
                            );
                            if outcome.matched {
                                return LRESULT(1);
                            }
                            if should_inject {
                                if let Some(button) = event_button {
                                    send_passthrough_click(button);
                                }
                                return LRESULT(1);
                            }
                            return CallNextHookEx(None, code, wparam, lparam);
                        }
                        HookAction::Ignore => {
                            return CallNextHookEx(None, code, wparam, lparam);
                        }
                    }
                }));

                match result {
                    Ok(result) => result,
                    Err(error) => {
                        let message = if let Some(message) = error.downcast_ref::<&str>() {
                            (*message).to_string()
                        } else if let Some(message) = error.downcast_ref::<String>() {
                            message.clone()
                        } else {
                            "unknown panic".to_string()
                        };
                        tracing::error!(error = %message, "mouse gesture hook panicked");
                        CallNextHookEx(None, code, wparam, lparam)
                    }
                }
            }

            let hook = unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(hook_proc), None, 0).ok() };
            let thread = unsafe { windows::Win32::System::Threading::GetCurrentThreadId() };
            thread_id.store(thread as usize, Ordering::SeqCst);
            let mut msg = MSG::default();
            loop {
                let result = unsafe { GetMessageW(&mut msg, HWND(std::ptr::null_mut()), 0, 0) };
                if result.0 == -1 {
                    break;
                }
                if result.0 == 0 || msg.message == WM_QUIT {
                    break;
                }
                unsafe {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                if stop_flag.load(Ordering::SeqCst) {
                    unsafe {
                        let _ = PostThreadMessageW(thread, WM_QUIT, WPARAM(0), LPARAM(0));
                    }
                }
            }
            if let Some(hook) = hook {
                unsafe {
                    let _ = UnhookWindowsHookEx(hook);
                }
            }
        });

        if let Ok(mut guard) = self.handle.lock() {
            *guard = Some(handle);
        }
        Ok(())
    }

    fn stop(&self) {
        if !self.running.swap(false, Ordering::SeqCst) {
            return;
        }
        self.stop_flag.store(true, Ordering::SeqCst);
        let thread_id = self.thread_id.load(Ordering::SeqCst) as u32;
        if thread_id != 0 {
            #[allow(clippy::cast_possible_wrap)]
            unsafe {
                use windows::Win32::Foundation::{LPARAM, WPARAM};
                use windows::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_QUIT};
                let _ = PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
            }
        }
        if let Ok(mut guard) = self.handle.lock() {
            if let Some(handle) = guard.take() {
                let _ = handle.join();
            }
        }
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

#[cfg(not(windows))]
#[derive(Default)]
pub struct WindowsMouseHookBackend;

#[cfg(not(windows))]
impl MouseHookBackend for WindowsMouseHookBackend {
    fn start(&self, _runtime: MouseGestureRuntime) -> anyhow::Result<()> {
        Ok(())
    }

    fn stop(&self) {}

    fn is_running(&self) -> bool {
        false
    }
}

#[derive(Default)]
pub struct MockMouseHookBackend {
    runtime: Mutex<Option<MouseGestureRuntime>>,
    start_count: AtomicUsize,
    stop_count: AtomicUsize,
    passthrough_clicks: AtomicUsize,
}

impl MockMouseHookBackend {
    pub fn start_count(&self) -> usize {
        self.start_count.load(Ordering::SeqCst)
    }

    pub fn stop_count(&self) -> usize {
        self.stop_count.load(Ordering::SeqCst)
    }

    pub fn passthrough_clicks(&self) -> usize {
        self.passthrough_clicks.load(Ordering::SeqCst)
    }

    pub fn simulate_track(&self, points: Vec<Point>) -> TrackOutcome {
        let runtime = self.runtime.lock().ok().and_then(|guard| guard.clone());
        let Some(runtime) = runtime else {
            return TrackOutcome::no_match();
        };
        let outcome = runtime.evaluate_track(&points);
        if outcome.passthrough_click {
            self.passthrough_clicks.fetch_add(1, Ordering::SeqCst);
        }
        outcome
    }

    pub fn simulate_track_with_limit(&self, points: Vec<Point>, too_long: bool) -> TrackOutcome {
        let runtime = self.runtime.lock().ok().and_then(|guard| guard.clone());
        let Some(runtime) = runtime else {
            return TrackOutcome::no_match();
        };
        let outcome = runtime.evaluate_track_with_limit(&points, too_long);
        if outcome.passthrough_click {
            self.passthrough_clicks.fetch_add(1, Ordering::SeqCst);
        }
        outcome
    }
}

impl MouseHookBackend for MockMouseHookBackend {
    fn start(&self, runtime: MouseGestureRuntime) -> anyhow::Result<()> {
        self.start_count.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut guard) = self.runtime.lock() {
            *guard = Some(runtime);
        }
        Ok(())
    }

    fn stop(&self) {
        self.stop_count.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut guard) = self.runtime.lock() {
            *guard = None;
        }
    }

    fn is_running(&self) -> bool {
        self.runtime
            .lock()
            .map(|guard| guard.is_some())
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_gesture_templates, clear_overlay_and_preview, finalize_hook_outcome,
        format_mouse_gesture_hook_status, hash_direction_sequence, hook_action_for_event,
        is_tap_track, mouse_gesture_overlay, preview_worker_loop, reset_template_build_counter,
        sampler_worker_loop, sequence_changed, should_cancel, should_compute_preview,
        template_build_count, track_directions_with_override, truncate_with_ellipsis, HookAction,
        HookDiagnostics, HookEventKind, HookTrackingState, MouseGestureEventSink,
        MouseGestureRuntime, MouseGestureService, MouseGestureSnapshots, PreviewRequest,
        ProfileCache, SamplerCommand, SamplerConfig, TrackOutcome, TrackingState, TriggerButton,
        PREVIEW_SIMILARITY_LABEL_MAX_LEN, PREVIEW_SIMILARITY_MAX_LEN, PREVIEW_SIMILARITY_TOP_N,
        TRACKING_START_DISTANCE_SQ, VISUAL_POINT_DISTANCE_SQ,
    };
    use crate::gui::MouseGestureEvent;
    use crate::mouse_gestures::overlay::overlay_test_counters;
    use crate::mouse_gestures::MouseHookBackend;
    use crate::plugins::mouse_gestures::db::{
        ForegroundWindowInfo, MouseGestureBinding, MouseGestureDb, MouseGestureProfile,
    };
    use crate::plugins::mouse_gestures::engine::{GestureDirection, Point};
    use crate::plugins::mouse_gestures::settings::MouseGesturePluginSettings;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{mpsc, Arc, Mutex, RwLock};
    use std::time::{Duration, Instant};

    #[cfg(windows)]
    use super::{should_ignore_event, MG_PASSTHROUGH_MARK};

    struct TestBackend;

    impl MouseHookBackend for TestBackend {
        fn start(&self, _runtime: MouseGestureRuntime) -> anyhow::Result<()> {
            Ok(())
        }

        fn stop(&self) {}

        fn is_running(&self) -> bool {
            false
        }
    }

    struct TestEventSink;

    impl MouseGestureEventSink for TestEventSink {
        fn dispatch(&self, _event: MouseGestureEvent) {}
    }

    fn make_gesture(points: &[(f32, f32)]) -> String {
        points
            .iter()
            .map(|(x, y)| format!("{x},{y}"))
            .collect::<Vec<_>>()
            .join("|")
    }

    fn make_snapshots(
        db: MouseGestureDb,
        settings: MouseGesturePluginSettings,
    ) -> MouseGestureSnapshots {
        let gesture_templates = build_gesture_templates(&db, &settings);
        MouseGestureSnapshots {
            settings,
            db,
            gesture_templates,
        }
    }

    fn make_runtime(
        db: MouseGestureDb,
        settings: MouseGesturePluginSettings,
    ) -> MouseGestureRuntime {
        MouseGestureRuntime {
            snapshots: Arc::new(RwLock::new(make_snapshots(db, settings))),
            event_sink: Arc::new(TestEventSink),
            profile_cache: Arc::new(Mutex::new(ProfileCache::default())),
        }
    }

    fn test_profile(bindings: Vec<MouseGestureBinding>) -> MouseGestureProfile {
        MouseGestureProfile {
            id: "profile".into(),
            label: "Test".into(),
            enabled: true,
            priority: 0,
            rules: Vec::new(),
            bindings,
        }
    }

    #[test]
    fn match_thresholds_prefer_multi_template_when_single_is_stricter() {
        let mut db = MouseGestureDb::default();
        db.bindings
            .insert("single".into(), make_gesture(&[(0.0, 0.0), (0.0, -20.0)]));
        db.bindings.insert(
            "multi".into(),
            make_gesture(&[(0.0, 0.0), (0.0, -20.0), (20.0, -20.0)]),
        );
        db.profiles = vec![test_profile(vec![
            MouseGestureBinding {
                gesture_id: "single".into(),
                label: "Single".into(),
                action: "single_action".into(),
                args: None,
                priority: 0,
                enabled: true,
            },
            MouseGestureBinding {
                gesture_id: "multi".into(),
                label: "Multi".into(),
                action: "multi_action".into(),
                args: None,
                priority: 0,
                enabled: true,
            },
        ])];
        let settings = MouseGesturePluginSettings {
            min_track_len: 0.0,
            segment_threshold_px: 5.0,
            direction_tolerance_deg: 0.0,
            single_dir_match_threshold: 0.9,
            multi_dir_match_threshold: 0.8,
            sampling_enabled: false,
            smoothing_enabled: false,
            ..MouseGesturePluginSettings::default()
        };
        let runtime = make_runtime(db, settings);
        let points = vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 0.0, y: -20.0 },
            Point { x: 20.0, y: -20.0 },
        ];

        let (label, _similarity) = runtime.best_match(&points).expect("expected multi match");

        assert_eq!(label, "Multi");
    }

    #[test]
    fn match_thresholds_prefer_single_template_when_multi_is_stricter() {
        let mut db = MouseGestureDb::default();
        db.bindings
            .insert("single".into(), make_gesture(&[(0.0, 0.0), (0.0, -20.0)]));
        db.bindings.insert(
            "multi".into(),
            make_gesture(&[(0.0, 0.0), (0.0, -20.0), (20.0, -20.0)]),
        );
        db.profiles = vec![test_profile(vec![
            MouseGestureBinding {
                gesture_id: "single".into(),
                label: "Single".into(),
                action: "single_action".into(),
                args: None,
                priority: 0,
                enabled: true,
            },
            MouseGestureBinding {
                gesture_id: "multi".into(),
                label: "Multi".into(),
                action: "multi_action".into(),
                args: None,
                priority: 0,
                enabled: true,
            },
        ])];
        let settings = MouseGesturePluginSettings {
            min_track_len: 0.0,
            segment_threshold_px: 5.0,
            direction_tolerance_deg: 0.0,
            single_dir_match_threshold: 0.8,
            multi_dir_match_threshold: 0.9,
            sampling_enabled: false,
            smoothing_enabled: false,
            ..MouseGesturePluginSettings::default()
        };
        let runtime = make_runtime(db, settings);
        let points = vec![Point { x: 0.0, y: 0.0 }, Point { x: 0.0, y: -20.0 }];

        let (label, _similarity) = runtime.best_match(&points).expect("expected single match");

        assert_eq!(label, "Single");
    }

    #[test]
    fn hook_status_line_reflects_diagnostics() {
        let diagnostics = HookDiagnostics::default();
        diagnostics.record_hook_event(true);
        let status = diagnostics.status(true);
        let last_event_at = status.last_event_at.expect("expected last event");
        let now = last_event_at + Duration::from_secs(5);

        let line = format_mouse_gesture_hook_status(&status, now);

        assert!(line.contains("Hook active: yes"));
        assert!(line.contains("Last event: 5s ago"));
        assert!(line.contains("Overlay ready: yes"));
    }

    #[test]
    fn preview_throttle_boundary() {
        let now = Instant::now();
        let throttle_ms = 100;
        let too_soon = now - Duration::from_millis(throttle_ms - 1);
        let on_time = now - Duration::from_millis(throttle_ms);

        assert!(!should_compute_preview(too_soon, now, throttle_ms));
        assert!(should_compute_preview(on_time, now, throttle_ms));
    }

    #[test]
    fn sampler_stops_when_stop_flag_set() {
        let runtime = make_runtime(
            MouseGestureDb::default(),
            MouseGesturePluginSettings::default(),
        );
        let (preview_sender, _preview_receiver) = mpsc::sync_channel(1);
        let preview_text = Arc::new(Mutex::new(None));
        let tracking = Arc::new(Mutex::new(HookTrackingState::default()));
        let tracking_active = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::new(AtomicBool::new(false));
        let diagnostics = Arc::new(HookDiagnostics::default());
        if let Ok(mut guard) = tracking.lock() {
            guard.attach_diagnostics(Arc::clone(&diagnostics));
        }
        let (sender, receiver) = mpsc::sync_channel(1);

        let tracking_handle = Arc::clone(&tracking);
        let tracking_active_handle = Arc::clone(&tracking_active);
        let stop_flag_handle = Arc::clone(&stop_flag);
        let preview_text_handle = Arc::clone(&preview_text);
        let runtime_handle = runtime.clone();

        let worker = std::thread::spawn(move || {
            sampler_worker_loop(
                runtime_handle,
                receiver,
                preview_sender,
                preview_text_handle,
                tracking_handle,
                tracking_active_handle,
                stop_flag_handle,
                Arc::clone(&diagnostics),
                || Point { x: 1.0, y: 1.0 },
                |_| true,
            );
        });

        let config = SamplerConfig {
            min_point_distance_sq: 0.0,
            visual_point_distance_sq: VISUAL_POINT_DISTANCE_SQ,
            max_track_len: 0.0,
            max_gesture_duration_ms: 0,
            max_sample_count: 0,
            preview_enabled: false,
            preview_on_end_only: false,
            preview_throttle_ms: crate::plugins::mouse_gestures::settings::PREVIEW_THROTTLE_MS,
            sample_interval: Duration::from_millis(5),
        };
        sender
            .send(SamplerCommand::Start {
                start_point: Point { x: 0.0, y: 0.0 },
                trigger_button: Some(TriggerButton::Right),
                config,
            })
            .expect("send start command");

        let start_deadline = Instant::now() + Duration::from_millis(200);
        while !tracking_active.load(Ordering::SeqCst) {
            if Instant::now() >= start_deadline {
                panic!("sampler never reported active tracking");
            }
            std::thread::sleep(Duration::from_millis(5));
        }

        stop_flag.store(true, Ordering::SeqCst);
        let stop_deadline = Instant::now() + Duration::from_millis(200);
        while tracking_active.load(Ordering::SeqCst) {
            if Instant::now() >= stop_deadline {
                panic!("sampler did not stop after stop flag");
            }
            std::thread::sleep(Duration::from_millis(5));
        }

        drop(sender);
        worker.join().expect("sampler worker exits");
    }

    #[test]
    fn sampler_cancels_when_trigger_released() {
        let runtime = make_runtime(
            MouseGestureDb::default(),
            MouseGesturePluginSettings::default(),
        );
        let (preview_sender, _preview_receiver) = mpsc::sync_channel(1);
        let preview_text = Arc::new(Mutex::new(None));
        let tracking = Arc::new(Mutex::new(HookTrackingState::default()));
        let tracking_active = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::new(AtomicBool::new(false));
        let diagnostics = Arc::new(HookDiagnostics::default());
        if let Ok(mut guard) = tracking.lock() {
            guard.attach_diagnostics(Arc::clone(&diagnostics));
        }
        let (sender, receiver) = mpsc::sync_channel(1);
        let trigger_checks = Arc::new(AtomicUsize::new(0));

        let tracking_handle = Arc::clone(&tracking);
        let tracking_active_handle = Arc::clone(&tracking_active);
        let stop_flag_handle = Arc::clone(&stop_flag);
        let preview_text_handle = Arc::clone(&preview_text);
        let runtime_handle = runtime.clone();
        let trigger_checks_handle = Arc::clone(&trigger_checks);

        let worker = std::thread::spawn(move || {
            sampler_worker_loop(
                runtime_handle,
                receiver,
                preview_sender,
                preview_text_handle,
                tracking_handle,
                tracking_active_handle,
                stop_flag_handle,
                Arc::clone(&diagnostics),
                || Point { x: 1.0, y: 1.0 },
                move |_| trigger_checks_handle.fetch_add(1, Ordering::SeqCst) < 3,
            );
        });

        let config = SamplerConfig {
            min_point_distance_sq: 0.0,
            visual_point_distance_sq: VISUAL_POINT_DISTANCE_SQ,
            max_track_len: 0.0,
            max_gesture_duration_ms: 0,
            max_sample_count: 0,
            preview_enabled: false,
            preview_on_end_only: false,
            preview_throttle_ms: crate::plugins::mouse_gestures::settings::PREVIEW_THROTTLE_MS,
            sample_interval: Duration::from_millis(5),
        };
        sender
            .send(SamplerCommand::Start {
                start_point: Point { x: 0.0, y: 0.0 },
                trigger_button: Some(TriggerButton::Right),
                config,
            })
            .expect("send start command");

        let start_deadline = Instant::now() + Duration::from_millis(200);
        while !tracking_active.load(Ordering::SeqCst) {
            if Instant::now() >= start_deadline {
                panic!("sampler never reported active tracking");
            }
            std::thread::sleep(Duration::from_millis(5));
        }

        if let Ok(mut preview_guard) = preview_text.lock() {
            *preview_guard = Some("Preview".into());
        }

        let cancel_deadline = Instant::now() + Duration::from_millis(200);
        while tracking_active.load(Ordering::SeqCst) {
            if Instant::now() >= cancel_deadline {
                panic!("sampler did not cancel after trigger release");
            }
            std::thread::sleep(Duration::from_millis(5));
        }

        let guard = tracking.lock().expect("lock tracking");
        assert_eq!(guard.points_len(), 0);
        assert_eq!(guard.active_button, None);
        drop(guard);

        assert!(
            preview_text.lock().expect("lock preview text").is_none(),
            "preview text should be cleared after cancellation"
        );

        drop(sender);
        worker.join().expect("sampler worker exits");
    }

    #[test]
    fn watchdog_cancels_when_trigger_up_missing() {
        let runtime = make_runtime(
            MouseGestureDb::default(),
            MouseGesturePluginSettings::default(),
        );
        let (preview_sender, _preview_receiver) = mpsc::sync_channel(1);
        let preview_text = Arc::new(Mutex::new(None));
        let tracking = Arc::new(Mutex::new(HookTrackingState::default()));
        let tracking_active = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::new(AtomicBool::new(false));
        let diagnostics = Arc::new(HookDiagnostics::default());
        if let Ok(mut guard) = tracking.lock() {
            guard.attach_diagnostics(Arc::clone(&diagnostics));
        }
        let (sender, receiver) = mpsc::sync_channel(1);

        let tracking_handle = Arc::clone(&tracking);
        let tracking_active_handle = Arc::clone(&tracking_active);
        let stop_flag_handle = Arc::clone(&stop_flag);
        let preview_text_handle = Arc::clone(&preview_text);
        let runtime_handle = runtime.clone();

        let worker = std::thread::spawn(move || {
            sampler_worker_loop(
                runtime_handle,
                receiver,
                preview_sender,
                preview_text_handle,
                tracking_handle,
                tracking_active_handle,
                stop_flag_handle,
                Arc::clone(&diagnostics),
                || Point { x: 1.0, y: 1.0 },
                |_| true,
            );
        });

        let config = SamplerConfig {
            min_point_distance_sq: 0.0,
            visual_point_distance_sq: VISUAL_POINT_DISTANCE_SQ,
            max_track_len: 0.0,
            max_gesture_duration_ms: 20,
            max_sample_count: 0,
            preview_enabled: false,
            preview_on_end_only: false,
            preview_throttle_ms: crate::plugins::mouse_gestures::settings::PREVIEW_THROTTLE_MS,
            sample_interval: Duration::from_millis(5),
        };
        sender
            .send(SamplerCommand::Start {
                start_point: Point { x: 0.0, y: 0.0 },
                trigger_button: Some(TriggerButton::Right),
                config,
            })
            .expect("send start command");

        let start_deadline = Instant::now() + Duration::from_millis(200);
        while !tracking_active.load(Ordering::SeqCst) {
            if Instant::now() >= start_deadline {
                panic!("sampler never reported active tracking");
            }
            std::thread::sleep(Duration::from_millis(5));
        }

        let cancel_deadline = Instant::now() + Duration::from_millis(200);
        while tracking_active.load(Ordering::SeqCst) {
            if Instant::now() >= cancel_deadline {
                panic!("sampler did not cancel on watchdog timeout");
            }
            std::thread::sleep(Duration::from_millis(5));
        }

        let guard = tracking.lock().expect("lock tracking");
        assert_eq!(guard.tracking_state, TrackingState::Idle);
        drop(guard);

        drop(sender);
        worker.join().expect("sampler worker exits");
    }

    #[test]
    fn diagnostics_counters_increment_on_sample_and_timeout() {
        let runtime = make_runtime(
            MouseGestureDb::default(),
            MouseGesturePluginSettings::default(),
        );
        let (preview_sender, _preview_receiver) = mpsc::sync_channel(1);
        let preview_text = Arc::new(Mutex::new(None));
        let tracking = Arc::new(Mutex::new(HookTrackingState::default()));
        let diagnostics = Arc::new(HookDiagnostics::default());
        if let Ok(mut guard) = tracking.lock() {
            guard.attach_diagnostics(Arc::clone(&diagnostics));
        }
        let tracking_active = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::new(AtomicBool::new(false));
        let (sender, receiver) = mpsc::sync_channel(1);
        let point_counter = Arc::new(AtomicUsize::new(0));

        let tracking_handle = Arc::clone(&tracking);
        let tracking_active_handle = Arc::clone(&tracking_active);
        let stop_flag_handle = Arc::clone(&stop_flag);
        let preview_text_handle = Arc::clone(&preview_text);
        let runtime_handle = runtime.clone();
        let point_counter_handle = Arc::clone(&point_counter);
        let diagnostics_handle = Arc::clone(&diagnostics);

        let worker = std::thread::spawn(move || {
            sampler_worker_loop(
                runtime_handle,
                receiver,
                preview_sender,
                preview_text_handle,
                tracking_handle,
                tracking_active_handle,
                stop_flag_handle,
                diagnostics_handle,
                move || {
                    let step = point_counter_handle.fetch_add(1, Ordering::SeqCst) as f32;
                    Point { x: step, y: 0.0 }
                },
                |_| true,
            );
        });

        let config = SamplerConfig {
            min_point_distance_sq: 0.0,
            visual_point_distance_sq: VISUAL_POINT_DISTANCE_SQ,
            max_track_len: 0.0,
            max_gesture_duration_ms: 20,
            max_sample_count: 0,
            preview_enabled: false,
            preview_on_end_only: false,
            preview_throttle_ms: crate::plugins::mouse_gestures::settings::PREVIEW_THROTTLE_MS,
            sample_interval: Duration::from_millis(5),
        };
        sender
            .send(SamplerCommand::Start {
                start_point: Point { x: 0.0, y: 0.0 },
                trigger_button: Some(TriggerButton::Right),
                config,
            })
            .expect("send start command");

        let start_deadline = Instant::now() + Duration::from_millis(200);
        while !tracking_active.load(Ordering::SeqCst) {
            if Instant::now() >= start_deadline {
                panic!("sampler never reported active tracking");
            }
            std::thread::sleep(Duration::from_millis(5));
        }

        let stored_deadline = Instant::now() + Duration::from_millis(200);
        while diagnostics.snapshot().stored_point_accepted == 0 {
            if Instant::now() >= stored_deadline {
                panic!("sampler never stored a point");
            }
            std::thread::sleep(Duration::from_millis(5));
        }

        let cancel_deadline = Instant::now() + Duration::from_millis(300);
        while tracking_active.load(Ordering::SeqCst) {
            if Instant::now() >= cancel_deadline {
                panic!("sampler did not cancel on timeout");
            }
            std::thread::sleep(Duration::from_millis(5));
        }

        drop(sender);
        worker.join().expect("sampler worker exits");

        let snapshot = diagnostics.snapshot();
        assert!(snapshot.tracking_started >= 1);
        assert!(snapshot.stored_point_accepted >= 1);
        assert_eq!(snapshot.tracking_canceled_timeout, 1);
    }

    #[test]
    fn diagnostics_start_fails_when_sender_disconnected() {
        let (sender, receiver) = mpsc::sync_channel(1);
        drop(receiver);

        let preview_text = Arc::new(Mutex::new(None));
        let tracking = Arc::new(Mutex::new(HookTrackingState::default()));
        let tracking_active = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::new(AtomicBool::new(false));
        let diagnostics = Arc::new(HookDiagnostics::default());
        if let Ok(mut guard) = tracking.lock() {
            guard.attach_diagnostics(Arc::clone(&diagnostics));
        }

        let config = SamplerConfig {
            min_point_distance_sq: 0.0,
            visual_point_distance_sq: VISUAL_POINT_DISTANCE_SQ,
            max_track_len: 0.0,
            max_gesture_duration_ms: 0,
            max_sample_count: 0,
            preview_enabled: false,
            preview_on_end_only: false,
            preview_throttle_ms: crate::plugins::mouse_gestures::settings::PREVIEW_THROTTLE_MS,
            sample_interval: Duration::from_millis(5),
        };

        let started = request_sampler_start(
            &sender,
            &tracking,
            &preview_text,
            &tracking_active,
            &stop_flag,
            &diagnostics,
            Point { x: 0.0, y: 0.0 },
            Some(TriggerButton::Right),
            config,
        );

        assert!(!started);
        assert!(!tracking_active.load(Ordering::SeqCst));
        let snapshot = diagnostics.snapshot();
        assert_eq!(snapshot.start_requested, 1);
        assert_eq!(snapshot.start_failed, 1);
    }

    #[test]
    fn diagnostics_start_counters_increment_on_start() {
        let runtime = make_runtime(
            MouseGestureDb::default(),
            MouseGesturePluginSettings::default(),
        );
        let (preview_sender, _preview_receiver) = mpsc::sync_channel(1);
        let preview_text = Arc::new(Mutex::new(None));
        let tracking = Arc::new(Mutex::new(HookTrackingState::default()));
        let tracking_active = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::new(AtomicBool::new(false));
        let diagnostics = Arc::new(HookDiagnostics::default());
        if let Ok(mut guard) = tracking.lock() {
            guard.attach_diagnostics(Arc::clone(&diagnostics));
        }
        let (sender, receiver) = mpsc::sync_channel(1);

        let tracking_handle = Arc::clone(&tracking);
        let tracking_active_handle = Arc::clone(&tracking_active);
        let stop_flag_handle = Arc::clone(&stop_flag);
        let preview_text_handle = Arc::clone(&preview_text);
        let runtime_handle = runtime.clone();
        let diagnostics_handle = Arc::clone(&diagnostics);

        let worker = std::thread::spawn(move || {
            sampler_worker_loop(
                runtime_handle,
                receiver,
                preview_sender,
                preview_text_handle,
                tracking_handle,
                tracking_active_handle,
                stop_flag_handle,
                diagnostics_handle,
                || Point { x: 1.0, y: 1.0 },
                |_| true,
            );
        });

        let config = SamplerConfig {
            min_point_distance_sq: 0.0,
            visual_point_distance_sq: VISUAL_POINT_DISTANCE_SQ,
            max_track_len: 0.0,
            max_gesture_duration_ms: 0,
            max_sample_count: 0,
            preview_enabled: false,
            preview_on_end_only: false,
            preview_throttle_ms: crate::plugins::mouse_gestures::settings::PREVIEW_THROTTLE_MS,
            sample_interval: Duration::from_millis(5),
        };

        let started = request_sampler_start(
            &sender,
            &tracking,
            &preview_text,
            &tracking_active,
            &stop_flag,
            &diagnostics,
            Point { x: 0.0, y: 0.0 },
            Some(TriggerButton::Right),
            config,
        );

        assert!(started);

        let receive_deadline = Instant::now() + Duration::from_millis(200);
        while diagnostics.snapshot().start_received == 0 {
            if Instant::now() >= receive_deadline {
                panic!("sampler never received start command");
            }
            std::thread::sleep(Duration::from_millis(5));
        }

        stop_flag.store(true, Ordering::SeqCst);
        drop(sender);
        worker.join().expect("sampler worker exits");

        let snapshot = diagnostics.snapshot();
        assert_eq!(snapshot.start_requested, 1);
        assert_eq!(snapshot.start_received, 1);
    }

    #[test]
    fn sampler_updates_visual_points_over_time() {
        let runtime = make_runtime(
            MouseGestureDb::default(),
            MouseGesturePluginSettings::default(),
        );
        let (preview_sender, _preview_receiver) = mpsc::sync_channel(1);
        let preview_text = Arc::new(Mutex::new(None));
        let tracking = Arc::new(Mutex::new(HookTrackingState::default()));
        let diagnostics = Arc::new(HookDiagnostics::default());
        if let Ok(mut guard) = tracking.lock() {
            guard.attach_diagnostics(Arc::clone(&diagnostics));
        }
        let tracking_active = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::new(AtomicBool::new(false));
        let (sender, receiver) = mpsc::sync_channel(1);
        let point_counter = Arc::new(AtomicUsize::new(0));

        let tracking_handle = Arc::clone(&tracking);
        let tracking_active_handle = Arc::clone(&tracking_active);
        let stop_flag_handle = Arc::clone(&stop_flag);
        let preview_text_handle = Arc::clone(&preview_text);
        let runtime_handle = runtime.clone();
        let point_counter_handle = Arc::clone(&point_counter);

        let worker = std::thread::spawn(move || {
            sampler_worker_loop(
                runtime_handle,
                receiver,
                preview_sender,
                preview_text_handle,
                tracking_handle,
                tracking_active_handle,
                stop_flag_handle,
                Arc::clone(&diagnostics),
                move || {
                    let step = point_counter_handle.fetch_add(1, Ordering::SeqCst) as f32;
                    Point {
                        x: step * 3.0,
                        y: 0.0,
                    }
                },
                |_| true,
            );
        });

        let config = SamplerConfig {
            min_point_distance_sq: 25.0,
            visual_point_distance_sq: VISUAL_POINT_DISTANCE_SQ,
            max_track_len: 0.0,
            max_gesture_duration_ms: 0,
            max_sample_count: 0,
            preview_enabled: false,
            preview_on_end_only: false,
            preview_throttle_ms: crate::plugins::mouse_gestures::settings::PREVIEW_THROTTLE_MS,
            sample_interval: Duration::from_millis(5),
        };
        sender
            .send(SamplerCommand::Start {
                start_point: Point { x: 0.0, y: 0.0 },
                trigger_button: Some(TriggerButton::Right),
                config,
            })
            .expect("send start command");

        let start_deadline = Instant::now() + Duration::from_millis(200);
        while !tracking_active.load(Ordering::SeqCst) {
            if Instant::now() >= start_deadline {
                panic!("sampler never reported active tracking");
            }
            std::thread::sleep(Duration::from_millis(5));
        }

        let visual_deadline = Instant::now() + Duration::from_millis(200);
        loop {
            if tracking
                .lock()
                .expect("lock tracking")
                .visual_points_len()
                >= 3
            {
                break;
            }
            if Instant::now() >= visual_deadline {
                panic!("visual points did not increase during sampling");
            }
            std::thread::sleep(Duration::from_millis(5));
        }

        stop_flag.store(true, Ordering::SeqCst);
        drop(sender);
        worker.join().expect("sampler worker exits");
    }

    #[test]
    fn hook_ignores_mouse_move_when_sampling_enabled() {
        let mut tracking = HookTrackingState::default();
        tracking.begin_track(Point { x: 1.0, y: 1.0 });
        let points_before = tracking.points.clone();
        let visual_points_before = tracking.visual_points_len();

        let action = hook_action_for_event(
            HookEventKind::MouseMove,
            Some(TriggerButton::Right),
            Some(TriggerButton::Right),
        );

        assert_eq!(action, HookAction::Ignore);
        assert_eq!(tracking.points, points_before);
        assert_eq!(tracking.visual_points_len(), visual_points_before);
    }

    #[test]
    fn trigger_down_up_finishes_overlay_and_resets_tracking() {
        let counters = overlay_test_counters();
        counters.reset();
        let tracking = Arc::new(Mutex::new(HookTrackingState::default()));
        let preview_text = Arc::new(Mutex::new(None));
        let point = Point { x: 10.0, y: 20.0 };
        let trigger_button = Some(TriggerButton::Right);

        {
            let mut guard = tracking.lock().expect("lock tracking");
            guard.begin_stroke(trigger_button, point);
            assert_eq!(guard.tracking_state, TrackingState::Armed);
        }
        if let Ok(mut overlay) = mouse_gesture_overlay().try_lock() {
            overlay.begin_stroke(point);
            overlay.update_preview(None, None);
        }

        {
            let mut guard = tracking.lock().expect("lock tracking");
            let _ = guard.finish_stroke(point, 0.0, 0.0, 0);
        }
        clear_overlay_and_preview(&preview_text);

        let guard = tracking.lock().expect("lock tracking");
        assert_eq!(guard.tracking_state, TrackingState::Idle);
        assert_eq!(guard.active_button, None);

        assert!(counters.begin_calls() >= 1);
        assert_eq!(counters.end_calls(), 1);
    }

    #[test]
    fn sample_interval_is_clamped() {
        let mut settings = MouseGesturePluginSettings::default();
        settings.sample_interval_ms = 1;
        assert_eq!(settings.sample_interval_ms(), 5);

        settings.sample_interval_ms = 80;
        assert_eq!(settings.sample_interval_ms(), 50);

        settings.sample_interval_ms = 20;
        assert_eq!(settings.sample_interval_ms(), 20);
    }

    #[test]
    fn tracking_max_duration_cancels() {
        let settings = MouseGesturePluginSettings {
            max_gesture_duration_ms: 10,
            ..MouseGesturePluginSettings::default()
        };
        let started_at = Instant::now();
        let before_deadline =
            started_at + Duration::from_millis(settings.max_gesture_duration_ms - 1);
        let after_deadline =
            started_at + Duration::from_millis(settings.max_gesture_duration_ms + 1);

        assert!(!should_cancel(
            Some(started_at),
            before_deadline,
            settings.max_gesture_duration_ms
        ));
        assert!(should_cancel(
            Some(started_at),
            after_deadline,
            settings.max_gesture_duration_ms
        ));
        assert!(!should_cancel(Some(started_at), after_deadline, 0));
        assert!(!should_cancel(
            None,
            after_deadline,
            settings.max_gesture_duration_ms
        ));
    }

    #[test]
    fn tracking_duration_starts_on_first_move() {
        let mut tracking = HookTrackingState::default();
        tracking.begin_track(Point { x: 0.0, y: 0.0 });
        let now = Instant::now();

        tracking.update_tracking_started(Point { x: 1.0, y: 1.0 }, TRACKING_START_DISTANCE_SQ, now);
        assert!(tracking.tracking_started_at.is_none());

        tracking.update_tracking_started(Point { x: 4.0, y: 0.0 }, TRACKING_START_DISTANCE_SQ, now);
        assert!(tracking.tracking_started_at.is_some());
    }

    #[test]
    fn tracking_no_timeout_without_movement() {
        let settings = MouseGesturePluginSettings {
            max_gesture_duration_ms: 5,
            ..MouseGesturePluginSettings::default()
        };
        let mut tracking = HookTrackingState::default();
        tracking.begin_track(Point { x: 0.0, y: 0.0 });
        let after_deadline = Instant::now() + Duration::from_millis(10);

        assert!(!should_cancel(
            tracking.tracking_started_at,
            after_deadline,
            settings.max_gesture_duration_ms
        ));
    }

    #[test]
    fn max_duration_cancel_returns_no_match() {
        let settings = MouseGesturePluginSettings {
            max_gesture_duration_ms: 5,
            ..MouseGesturePluginSettings::default()
        };
        let mut tracking = HookTrackingState::default();
        tracking.begin_track(Point { x: 0.0, y: 0.0 });
        tracking.tracking_started_at =
            Some(Instant::now() - Duration::from_millis(settings.max_gesture_duration_ms + 1));

        assert!(should_cancel(
            tracking.tracking_started_at,
            Instant::now(),
            settings.max_gesture_duration_ms
        ));

        let outcome = tracking.cancel_tracking();
        assert!(!outcome.matched);
        assert!(!outcome.passthrough_click);
    }

    #[test]
    fn max_sample_count_clamps_points_and_finishes() {
        let mut tracking = HookTrackingState::default();
        tracking.begin_track(Point { x: 0.0, y: 0.0 });
        let max_sample_count = 3;

        for index in 1..10 {
            tracking.handle_move(
                Point {
                    x: index as f32,
                    y: 0.0,
                },
                0.0,
                0.0,
                max_sample_count,
            );
        }

        assert_eq!(tracking.points_len(), max_sample_count);

        let (points, too_long) =
            tracking.finish_stroke(Point { x: 10.0, y: 0.0 }, 0.0, 0.0, max_sample_count);

        assert_eq!(points.len(), max_sample_count);
        assert!(!too_long);
        assert_eq!(tracking.points_len(), 0);
    }

    #[test]
    fn visual_sampling_tracks_small_movements() {
        let mut tracking = HookTrackingState::default();
        tracking.begin_track(Point { x: 0.0, y: 0.0 });
        let min_point_distance_sq = 25.0;
        let visual_point_distance_sq = 1.0;

        for step in 1..=3 {
            let point = Point {
                x: step as f32,
                y: 0.0,
            };
            let visual_stored = tracking.record_visual_point(point, visual_point_distance_sq);
            let recognition_stored = tracking.sample_point(point, min_point_distance_sq, 0.0, 0);
            if recognition_stored && !visual_stored {
                tracking.force_visual_point(point);
            }
        }

        assert_eq!(tracking.points_len(), 1);
        assert_eq!(tracking.visual_points_len(), 4);
    }

    #[test]
    fn canceling_tracking_resets_state() {
        let mut tracking = HookTrackingState::default();
        tracking.begin_track(Point { x: 1.0, y: 1.0 });
        tracking.handle_move(Point { x: 2.0, y: 2.0 }, 0.0, 0.0, 0);

        let outcome = tracking.cancel_tracking();

        assert_eq!(tracking.points_len(), 0);
        assert!(!tracking.too_long());
        assert_eq!(tracking.active_button, None);
        assert_eq!(tracking.acc_len(), 0.0);
        assert_eq!(tracking.tracking_state, TrackingState::Idle);
        assert_eq!(outcome.matched, TrackOutcome::no_match().matched);
        assert_eq!(
            outcome.passthrough_click,
            TrackOutcome::no_match().passthrough_click
        );
    }

    #[test]
    fn preview_recompute_requires_sequence_change() {
        let directions = [GestureDirection::Up, GestureDirection::Right];
        let hash = hash_direction_sequence(&directions);

        assert!(sequence_changed(None, hash));
        assert!(!sequence_changed(Some(hash), hash));

        let new_hash = hash_direction_sequence(&[GestureDirection::Down]);
        assert!(sequence_changed(Some(hash), new_hash));
    }

    #[test]
    fn preview_on_end_only_updates_on_force() {
        let db = make_db_with_right_gesture();
        let settings = MouseGesturePluginSettings {
            min_point_distance: 0.0,
            preview_enabled: true,
            preview_on_end_only: true,
            ..MouseGesturePluginSettings::default()
        };
        let runtime = make_runtime(db, settings);
        let preview_text = Arc::new(Mutex::new(None));
        let preview_text_handle = Arc::clone(&preview_text);
        let (sender, receiver) = mpsc::sync_channel(2);
        let points = make_right_points();

        let worker = std::thread::spawn(move || {
            preview_worker_loop(runtime, receiver, preview_text_handle);
        });

        sender
            .send(PreviewRequest {
                points: points.clone(),
                force: false,
            })
            .expect("send non-force request");

        std::thread::sleep(Duration::from_millis(30));
        assert!(
            preview_text.lock().expect("lock preview text").is_none(),
            "preview text should stay empty until forced"
        );

        sender
            .send(PreviewRequest {
                points,
                force: true,
            })
            .expect("send forced request");
        drop(sender);

        let deadline = Instant::now() + Duration::from_millis(500);
        loop {
            if preview_text.lock().expect("lock preview text").is_some() {
                break;
            }
            if Instant::now() >= deadline {
                panic!("preview text not updated on forced request");
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        worker.join().expect("worker finished");
    }

    #[test]
    fn preview_disabled_skips_computation() {
        let db = make_db_with_right_gesture();
        let settings = MouseGesturePluginSettings {
            min_point_distance: 0.0,
            preview_enabled: false,
            ..MouseGesturePluginSettings::default()
        };
        let runtime = make_runtime(db, settings);
        let preview_text = Arc::new(Mutex::new(None));
        let preview_text_handle = Arc::clone(&preview_text);
        let (sender, receiver) = mpsc::sync_channel(2);
        let points = make_right_points();

        let worker = std::thread::spawn(move || {
            preview_worker_loop(runtime, receiver, preview_text_handle);
        });

        sender
            .send(PreviewRequest {
                points,
                force: true,
            })
            .expect("send preview request");
        drop(sender);

        worker.join().expect("preview worker finished");

        assert!(
            preview_text.lock().expect("lock preview text").is_none(),
            "preview text should remain empty when preview is disabled"
        );
    }

    #[test]
    fn stop_path_clears_preview_state() {
        let preview_text = Arc::new(Mutex::new(Some("Preview".into())));
        clear_overlay_and_preview(&preview_text);
        assert!(
            preview_text.lock().expect("lock preview text").is_none(),
            "preview text should clear on stop cleanup"
        );
    }

    #[test]
    fn preview_throttle_prevents_frequent_updates() {
        let db = make_db_with_right_and_up_gestures();
        let settings = MouseGesturePluginSettings {
            min_point_distance: 0.0,
            segment_threshold_px: 0.0,
            direction_tolerance_deg: 0.0,
            preview_enabled: true,
            preview_throttle_ms: 200,
            ..MouseGesturePluginSettings::default()
        };
        let runtime = make_runtime(db, settings);
        let preview_text = Arc::new(Mutex::new(None));
        let preview_text_handle = Arc::clone(&preview_text);
        let (sender, receiver) = mpsc::sync_channel(2);

        let worker = std::thread::spawn(move || {
            preview_worker_loop(runtime, receiver, preview_text_handle);
        });

        sender
            .send(PreviewRequest {
                points: make_right_points(),
                force: false,
            })
            .expect("send preview request");

        let deadline = Instant::now() + Duration::from_millis(500);
        let first_text = loop {
            if let Some(text) = preview_text.lock().expect("lock preview text").clone() {
                break text;
            }
            if Instant::now() >= deadline {
                panic!("preview text not computed after first request");
            }
            std::thread::sleep(Duration::from_millis(10));
        };

        sender
            .send(PreviewRequest {
                points: make_up_points(),
                force: false,
            })
            .expect("send second preview request");

        std::thread::sleep(Duration::from_millis(60));

        let second_text = preview_text.lock().expect("lock preview text").clone();
        assert_eq!(
            second_text,
            Some(first_text),
            "preview text should not update within throttle window"
        );

        drop(sender);
        worker.join().expect("preview worker finished");
    }

    #[test]
    fn preview_similarity_limits_top_matches() {
        let points = vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 10.0, y: 0.0 },
            Point { x: 10.0, y: -10.0 },
        ];

        let mut bindings_map = HashMap::new();
        bindings_map.insert(
            "g1".to_string(),
            make_gesture(&[(0.0, 0.0), (10.0, 0.0), (10.0, -10.0)]),
        );
        bindings_map.insert("g2".to_string(), make_gesture(&[(0.0, 0.0), (10.0, 0.0)]));
        bindings_map.insert("g3".to_string(), make_gesture(&[(0.0, 0.0), (0.0, -10.0)]));
        bindings_map.insert("g4".to_string(), make_gesture(&[(0.0, 0.0), (-10.0, 0.0)]));

        let bindings = vec![
            MouseGestureBinding {
                gesture_id: "g1".into(),
                label: "Exact".into(),
                action: "a1".into(),
                args: None,
                priority: 0,
                enabled: true,
            },
            MouseGestureBinding {
                gesture_id: "g2".into(),
                label: "Short".into(),
                action: "a2".into(),
                args: None,
                priority: 0,
                enabled: true,
            },
            MouseGestureBinding {
                gesture_id: "g3".into(),
                label: "Up".into(),
                action: "a3".into(),
                args: None,
                priority: 0,
                enabled: true,
            },
            MouseGestureBinding {
                gesture_id: "g4".into(),
                label: "Left".into(),
                action: "a4".into(),
                args: None,
                priority: 0,
                enabled: true,
            },
        ];

        let db = MouseGestureDb {
            profiles: vec![test_profile(bindings)],
            bindings: bindings_map,
            ..MouseGestureDb::default()
        };
        let settings = MouseGesturePluginSettings {
            min_point_distance: 0.0,
            segment_threshold_px: 0.0,
            direction_tolerance_deg: 0.0,
            preview_enabled: true,
            ..MouseGesturePluginSettings::default()
        };
        let runtime = make_runtime(db.clone(), settings.clone());
        let snapshots = make_snapshots(db, settings);

        let summary = runtime
            .preview_similarity_text(&points, &snapshots)
            .expect("summary");

        assert!(summary.contains("Exact"));
        assert!(summary.contains("Short"));
        assert!(summary.contains("Up"));
        assert!(!summary.contains("Left"));
        assert_eq!(summary.matches('%').count(), PREVIEW_SIMILARITY_TOP_N);
    }

    #[test]
    fn tap_detection_below_threshold_counts_as_tap() {
        let short_points = vec![Point { x: 0.0, y: 0.0 }, Point { x: 2.0, y: 1.0 }];
        let long_points = vec![Point { x: 0.0, y: 0.0 }, Point { x: 12.0, y: 0.0 }];

        assert!(is_tap_track(&short_points, 8.0));
        assert!(!is_tap_track(&long_points, 8.0));
    }

    #[test]
    fn tap_tracks_generate_passthrough_outcome() {
        let runtime = make_runtime(
            MouseGestureDb::default(),
            MouseGesturePluginSettings::default(),
        );
        let points = vec![Point { x: 1.0, y: 1.0 }, Point { x: 2.0, y: 2.0 }];

        let (outcome, should_inject) = finalize_hook_outcome(&runtime, &points, false, 10.0);

        assert!(should_inject);
        assert!(outcome.passthrough_click);
        assert!(!outcome.matched);
    }

    #[test]
    fn matched_gestures_do_not_passthrough_clicks() {
        let db = make_db_with_right_gesture();
        let settings = MouseGesturePluginSettings {
            min_point_distance: 0.0,
            segment_threshold_px: 0.0,
            direction_tolerance_deg: 0.0,
            ..MouseGesturePluginSettings::default()
        };
        let runtime = make_runtime(db, settings);
        let points = make_right_points();

        let (outcome, should_inject) = finalize_hook_outcome(&runtime, &points, false, 5.0);

        assert!(!should_inject);
        assert!(outcome.matched);
        assert!(!outcome.passthrough_click);
    }

    #[cfg(windows)]
    #[test]
    fn passthrough_mark_ignored_by_hook() {
        assert!(should_ignore_event(0, MG_PASSTHROUGH_MARK));
        assert!(!should_ignore_event(0, MG_PASSTHROUGH_MARK + 1));
    }

    #[test]
    fn preview_similarity_truncates_labels() {
        let points = vec![Point { x: 0.0, y: 0.0 }, Point { x: 10.0, y: 0.0 }];
        let long_label = "L".repeat(PREVIEW_SIMILARITY_LABEL_MAX_LEN + 10);
        let mut bindings_map = HashMap::new();
        bindings_map.insert("g1".to_string(), make_gesture(&[(0.0, 0.0), (10.0, 0.0)]));

        let bindings = vec![MouseGestureBinding {
            gesture_id: "g1".into(),
            label: long_label.clone(),
            action: "a1".into(),
            args: None,
            priority: 0,
            enabled: true,
        }];

        let db = MouseGestureDb {
            profiles: vec![test_profile(bindings)],
            bindings: bindings_map,
            ..MouseGestureDb::default()
        };
        let settings = MouseGesturePluginSettings {
            min_point_distance: 0.0,
            segment_threshold_px: 0.0,
            direction_tolerance_deg: 0.0,
            ..MouseGesturePluginSettings::default()
        };
        let runtime = make_runtime(db.clone(), settings.clone());
        let snapshots = make_snapshots(db, settings);

        let summary = runtime
            .preview_similarity_text(&points, &snapshots)
            .expect("summary");

        let truncated = truncate_with_ellipsis(&long_label, PREVIEW_SIMILARITY_LABEL_MAX_LEN);
        assert!(summary.contains(&truncated));
        assert!(!summary.contains(&long_label));
    }

    #[test]
    fn preview_similarity_caps_total_length() {
        let points = vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 10.0, y: 0.0 },
            Point { x: 10.0, y: -10.0 },
        ];

        let mut bindings_map = HashMap::new();
        bindings_map.insert(
            "g1".to_string(),
            make_gesture(&[(0.0, 0.0), (10.0, 0.0), (10.0, -10.0)]),
        );
        bindings_map.insert("g2".to_string(), make_gesture(&[(0.0, 0.0), (10.0, 0.0)]));
        bindings_map.insert("g3".to_string(), make_gesture(&[(0.0, 0.0), (0.0, -10.0)]));

        let bindings = vec![
            MouseGestureBinding {
                gesture_id: "g1".into(),
                label: "A".repeat(80),
                action: "a1".into(),
                args: None,
                priority: 0,
                enabled: true,
            },
            MouseGestureBinding {
                gesture_id: "g2".into(),
                label: "B".repeat(80),
                action: "a2".into(),
                args: None,
                priority: 0,
                enabled: true,
            },
            MouseGestureBinding {
                gesture_id: "g3".into(),
                label: "C".repeat(80),
                action: "a3".into(),
                args: None,
                priority: 0,
                enabled: true,
            },
        ];

        let db = MouseGestureDb {
            profiles: vec![test_profile(bindings)],
            bindings: bindings_map,
            ..MouseGestureDb::default()
        };
        let settings = MouseGesturePluginSettings {
            min_point_distance: 0.0,
            segment_threshold_px: 0.0,
            direction_tolerance_deg: 0.0,
            ..MouseGesturePluginSettings::default()
        };
        let runtime = make_runtime(db.clone(), settings.clone());
        let snapshots = make_snapshots(db, settings);

        let summary = runtime
            .preview_similarity_text(&points, &snapshots)
            .expect("summary");

        assert!(summary.chars().count() <= PREVIEW_SIMILARITY_MAX_LEN);
    }

    #[test]
    fn straight_line_collapses_to_single_direction() {
        let points = vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 50.0, y: 0.0 },
            Point { x: 120.0, y: 0.0 },
        ];
        let settings = MouseGesturePluginSettings {
            segment_threshold_px: 0.0,
            direction_tolerance_deg: 0.0,
            smoothing_enabled: false,
            sampling_enabled: false,
            straightness_threshold: 0.9,
            straightness_min_displacement_px: 20.0,
            ..MouseGesturePluginSettings::default()
        };

        let directions = track_directions_with_override(&points, &settings, None, None);

        assert_eq!(directions, vec![GestureDirection::Right]);
    }

    #[test]
    fn l_shape_does_not_trigger_straightness_override() {
        let points = vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 60.0, y: 0.0 },
            Point { x: 60.0, y: 60.0 },
        ];
        let settings = MouseGesturePluginSettings {
            segment_threshold_px: 0.0,
            direction_tolerance_deg: 0.0,
            smoothing_enabled: false,
            sampling_enabled: false,
            straightness_threshold: 0.9,
            straightness_min_displacement_px: 20.0,
            ..MouseGesturePluginSettings::default()
        };

        let directions = track_directions_with_override(&points, &settings, None, None);

        assert_eq!(
            directions,
            vec![GestureDirection::Right, GestureDirection::Down]
        );
    }

    fn make_right_points() -> Vec<Point> {
        vec![Point { x: 0.0, y: 0.0 }, Point { x: 80.0, y: 0.0 }]
    }

    fn make_up_points() -> Vec<Point> {
        vec![Point { x: 0.0, y: 0.0 }, Point { x: 0.0, y: -80.0 }]
    }

    fn make_db_with_right_gesture() -> MouseGestureDb {
        let mut db = MouseGestureDb::default();
        db.bindings.insert(
            "gesture-right".to_string(),
            "Swipe Right: 0,0|80,0".to_string(),
        );
        db.profiles.push(MouseGestureProfile {
            id: "profile-default".to_string(),
            label: "Default".to_string(),
            enabled: true,
            priority: 0,
            rules: Vec::new(),
            bindings: vec![MouseGestureBinding {
                gesture_id: "gesture-right".to_string(),
                label: "Swipe Right".to_string(),
                action: "noop".to_string(),
                args: None,
                priority: 0,
                enabled: true,
            }],
        });
        db
    }

    fn make_db_with_right_and_up_gestures() -> MouseGestureDb {
        let mut db = make_db_with_right_gesture();
        db.bindings
            .insert("gesture-up".to_string(), "Swipe Up: 0,0|0,-80".to_string());
        if let Some(profile) = db.profiles.first_mut() {
            profile.bindings.push(MouseGestureBinding {
                gesture_id: "gesture-up".to_string(),
                label: "Swipe Up".to_string(),
                action: "noop".to_string(),
                args: None,
                priority: 0,
                enabled: true,
            });
        }
        db
    }

    #[test]
    fn update_db_refreshes_cached_gesture_templates() {
        let service = MouseGestureService::new_with_backend_and_sink(
            Arc::new(TestBackend),
            Arc::new(TestEventSink),
        );
        let settings = MouseGesturePluginSettings {
            min_point_distance: 0.0,
            segment_threshold_px: 0.0,
            direction_tolerance_deg: 0.0,
            ..MouseGesturePluginSettings::default()
        };
        service.update_settings(settings);
        let runtime = MouseGestureRuntime {
            snapshots: Arc::clone(&service.snapshots),
            event_sink: Arc::new(TestEventSink),
            profile_cache: Arc::clone(&service.profile_cache),
        };
        let points = make_right_points();

        let snapshots = runtime.snapshots.read().expect("read snapshots").clone();
        let text = runtime
            .preview_similarity_text(&points, &snapshots)
            .expect("preview text");
        assert_eq!(text, "No match");

        service.update_db(make_db_with_right_gesture());

        let snapshots = runtime.snapshots.read().expect("read snapshots").clone();
        let text = runtime
            .preview_similarity_text(&points, &snapshots)
            .expect("preview text after update");
        assert!(
            text.contains("Swipe Right"),
            "expected updated templates in text: {text}"
        );
    }

    #[test]
    fn update_settings_rebuilds_templates_on_segment_threshold_change() {
        let service = MouseGestureService::new_with_backend_and_sink(
            Arc::new(TestBackend),
            Arc::new(TestEventSink),
        );
        reset_template_build_counter();

        let mut settings = MouseGesturePluginSettings::default();
        settings.segment_threshold_px = 2.0;
        service.update_settings(settings.clone());
        let builds_after_first = template_build_count();

        settings.segment_threshold_px = 12.0;
        service.update_settings(settings);
        let builds_after_second = template_build_count();

        assert!(
            builds_after_second > builds_after_first,
            "expected template rebuild when segment threshold changes"
        );
    }

    #[test]
    fn update_settings_rebuilds_templates_on_direction_tolerance_change() {
        let service = MouseGestureService::new_with_backend_and_sink(
            Arc::new(TestBackend),
            Arc::new(TestEventSink),
        );
        reset_template_build_counter();

        let mut settings = MouseGesturePluginSettings::default();
        settings.direction_tolerance_deg = 10.0;
        service.update_settings(settings.clone());
        let builds_after_first = template_build_count();

        settings.direction_tolerance_deg = 45.0;
        service.update_settings(settings);
        let builds_after_second = template_build_count();

        assert!(
            builds_after_second > builds_after_first,
            "expected template rebuild when direction tolerance changes"
        );
    }

    #[test]
    fn preview_worker_uses_cached_templates() {
        let db = make_db_with_right_gesture();
        let settings = MouseGesturePluginSettings {
            min_point_distance: 0.0,
            segment_threshold_px: 0.0,
            direction_tolerance_deg: 0.0,
            preview_enabled: true,
            ..MouseGesturePluginSettings::default()
        };
        let runtime = make_runtime(db, settings);
        reset_template_build_counter();

        let preview_text = Arc::new(Mutex::new(None));
        let preview_text_handle = Arc::clone(&preview_text);
        let (sender, receiver) = mpsc::sync_channel(2);
        let points = make_right_points();

        let worker = std::thread::spawn(move || {
            preview_worker_loop(runtime, receiver, preview_text_handle);
        });

        sender
            .send(PreviewRequest {
                points,
                force: true,
            })
            .expect("send preview request");
        drop(sender);

        worker.join().expect("preview worker finished");

        assert_eq!(
            template_build_count(),
            0,
            "preview worker should use cached templates"
        );
        assert!(
            preview_text.lock().expect("lock preview text").is_some(),
            "expected preview text to be updated"
        );
    }

    #[test]
    fn profile_cache_reuses_selection_for_same_window() {
        let service = MouseGestureService::new_with_backend_and_sink(
            Arc::new(TestBackend),
            Arc::new(TestEventSink),
        );
        let runtime = MouseGestureRuntime {
            snapshots: Arc::clone(&service.snapshots),
            event_sink: Arc::new(TestEventSink),
            profile_cache: Arc::clone(&service.profile_cache),
        };
        let db = make_db_with_right_gesture();
        let window = ForegroundWindowInfo {
            exe: Some("app.exe".to_string()),
            class: None,
            title: None,
        };

        assert!(runtime.select_profile_cached(&db, &window).is_some());
        assert!(runtime.select_profile_cached(&db, &window).is_some());

        let cache_hits = runtime
            .profile_cache
            .lock()
            .expect("lock profile cache")
            .cache_hits;
        assert_eq!(cache_hits, 1);
    }
}
