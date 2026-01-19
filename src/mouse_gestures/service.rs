use crate::gui::{send_event, MouseGestureEvent, WatchEvent};
use crate::mouse_gestures::mouse_gesture_overlay;
use crate::plugins::mouse_gestures::db::{
    select_binding, select_profile, ForegroundWindowInfo, MouseGestureDb,
};
use crate::plugins::mouse_gestures::engine::{
    direction_sequence, direction_similarity, parse_gesture, preprocess_points_for_directions,
    track_length, GestureDirection, Point,
};
use crate::plugins::mouse_gestures::settings::MouseGesturePluginSettings;
use once_cell::sync::OnceCell;
use std::cmp::Ordering as CmpOrdering;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
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
        let processed_points = preprocess_points_for_directions(points, &snapshots.settings);
        let track_dirs =
            direction_sequence(&processed_points, snapshots.settings.min_point_distance);
        if track_dirs.is_empty() {
            return None;
        }
        let gesture_templates = &snapshots.gesture_templates;
        let mut distances = HashMap::new();
        for (gesture_id, template) in gesture_templates {
            let similarity = direction_similarity(&track_dirs, &template.directions);
            if similarity < snapshots.settings.match_threshold {
                continue;
            }
            distances.insert(gesture_id.clone(), 1.0 - similarity);
        }
        if distances.is_empty() {
            return None;
        }
        let window_info = current_foreground_window();
        let profile = self.select_profile_cached(&snapshots.db, &window_info)?;
        let binding =
            select_binding(profile, &distances, snapshots.settings.max_distance)?;
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
            return self.preview_similarity_text(points, &snapshots);
        }
        let Some((label, similarity)) = self.best_match(points) else {
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
        let processed_points = preprocess_points_for_directions(points, &snapshots.settings);
        let track_dirs =
            direction_sequence(&processed_points, snapshots.settings.min_point_distance);
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
            if !similarity.is_finite() {
                continue;
            }

            let mut min_index = None;
            let mut min_similarity = None;
            if similarities.len() >= PREVIEW_SIMILARITY_TOP_N {
                if let Some((index, (value, _))) = similarities
                    .iter()
                    .enumerate()
                    .min_by(|a, b| a.1.0.partial_cmp(&b.1.0).unwrap_or(CmpOrdering::Equal))
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
            let truncated =
                truncate_with_ellipsis(label.trim(), PREVIEW_SIMILARITY_LABEL_MAX_LEN);
            if let Some(index) = min_index {
                similarities[index] = (similarity, truncated);
            } else {
                similarities.push((similarity, truncated));
            }
        }

        if similarities.is_empty() {
            return Some("No match".to_string());
        }

        similarities.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(CmpOrdering::Equal));
        let summary = similarities
            .into_iter()
            .map(|(similarity, label)| format!("{label}: {:.0}%", similarity * 100.0))
            .collect::<Vec<_>>()
            .join(" | ");
        let full_text = format!("{PREVIEW_SIMILARITY_PREFIX}{summary}");
        Some(truncate_with_ellipsis(
            &full_text,
            PREVIEW_SIMILARITY_MAX_LEN,
        ))
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

        let processed_points = preprocess_points_for_directions(points, &snapshots.settings);
        let track_dirs =
            direction_sequence(&processed_points, snapshots.settings.min_point_distance);
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
            if similarity < snapshots.settings.match_threshold {
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

const PREVIEW_THROTTLE_MS: u64 = 120;

fn should_compute_preview(last: Instant, now: Instant, throttle_ms: u64) -> bool {
    now.duration_since(last) >= Duration::from_millis(throttle_ms)
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

fn build_gesture_templates(
    db: &MouseGestureDb,
    settings: &MouseGesturePluginSettings,
) -> HashMap<String, GestureTemplate> {
    let mut templates = HashMap::new();
    for (gesture_id, serialized) in &db.bindings {
        let parsed = match parse_gesture(serialized) {
            Ok(def) => def,
            Err(_) => continue,
        };
        let processed_points = preprocess_points_for_directions(&parsed.points, settings);
        let directions = direction_sequence(&processed_points, settings.min_point_distance);
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

fn should_refresh_gesture_templates(
    current: &MouseGesturePluginSettings,
    updated: &MouseGesturePluginSettings,
) -> bool {
    current.min_point_distance != updated.min_point_distance
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
                guard.gesture_templates = build_gesture_templates(&guard.db, &settings);
            }
            guard.settings = settings.clone();
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
            guard.gesture_templates = build_gesture_templates(&guard.db, &guard.settings);
        }
        if let Ok(mut cache) = self.profile_cache.lock() {
            cache.clear();
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
#[derive(Default)]
pub struct WindowsMouseHookBackend {
    running: Arc<AtomicBool>,
    stop_flag: Arc<AtomicBool>,
    thread_id: Arc<AtomicUsize>,
    handle: Arc<Mutex<Option<std::thread::JoinHandle<()>>>>,
}

const MIN_DECIMATION_DISTANCE_SQ: f32 = 4.0;
pub const MAX_TRACK_POINTS: usize = 4096;

pub struct HookTrackingState {
    active_button: Option<TriggerButton>,
    points: Vec<Point>,
    last_point: Option<Point>,
    last_stored_point: Option<Point>,
    acc_len: f32,
    too_long: bool,
    stored_points: usize,
    decimation_stride: usize,
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
    let mut last_preview_at = Instant::now() - Duration::from_millis(PREVIEW_THROTTLE_MS);
    let mut last_sequence_hash = None;
    for request in receiver {
        let now = Instant::now();
        let snapshots = runtime
            .snapshots
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_default();
        let processed_points =
            preprocess_points_for_directions(&request.points, &snapshots.settings);
        let min_point_distance = snapshots.settings.min_point_distance.max(0.0);
        let directions = direction_sequence(&processed_points, min_point_distance);
        let sequence_hash = hash_direction_sequence(&directions);

        if snapshots.settings.preview_on_end_only && !request.force {
            continue;
        }

        if !request.force {
            if !sequence_changed(last_sequence_hash, sequence_hash) {
                continue;
            }
            if !should_compute_preview(last_preview_at, now, PREVIEW_THROTTLE_MS) {
                continue;
            }
        }

        let text = runtime.preview_text(&request.points);
        if let Ok(mut guard) = preview_text.lock() {
            *guard = text;
        }
        last_preview_at = now;
        last_sequence_hash = Some(sequence_hash);
    }
}

#[cfg(windows)]
fn cached_preview_text(preview_text: &Arc<Mutex<Option<String>>>) -> Option<String> {
    preview_text
        .lock()
        .ok()
        .and_then(|guard| guard.clone())
}

impl Default for HookTrackingState {
    fn default() -> Self {
        Self {
            active_button: None,
            points: Vec::new(),
            last_point: None,
            last_stored_point: None,
            acc_len: 0.0,
            too_long: false,
            stored_points: 0,
            decimation_stride: 1,
        }
    }
}

impl HookTrackingState {
    fn reset_tracking(&mut self) {
        self.active_button = None;
        self.points.clear();
        self.last_point = None;
        self.last_stored_point = None;
        self.acc_len = 0.0;
        self.too_long = false;
        self.stored_points = 0;
        self.decimation_stride = 1;
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

    fn store_point(&mut self, point: Point) -> bool {
        self.stored_points = self.stored_points.saturating_add(1);
        if self.points.len() < MAX_TRACK_POINTS {
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
        self.reset_tracking();
        self.active_button = button;
        self.last_point = Some(point);
        self.last_stored_point = Some(point);
        self.points.push(point);
        self.stored_points = 1;
    }

    pub fn begin_track(&mut self, point: Point) {
        self.begin_stroke(None, point);
    }

    pub fn handle_move(
        &mut self,
        point: Point,
        min_point_distance_sq: f32,
        max_track_len: f32,
    ) -> bool {
        self.update_length(point);
        if max_track_len > 0.0 && self.acc_len > max_track_len {
            self.too_long = true;
            return false;
        }
        if self.too_long || !self.should_store_point(point, min_point_distance_sq) {
            return false;
        }
        self.store_point(point)
    }

    pub fn finish_stroke(
        &mut self,
        point: Point,
        min_point_distance_sq: f32,
        max_track_len: f32,
    ) -> (Vec<Point>, bool) {
        self.update_length(point);
        if max_track_len > 0.0 && self.acc_len > max_track_len {
            self.too_long = true;
        }
        if !self.too_long && self.should_store_point(point, min_point_distance_sq) {
            let _ = self.store_point(point);
        }
        let points = std::mem::take(&mut self.points);
        let too_long = self.too_long;
        self.reset_tracking();
        (points, too_long)
    }

    pub fn points_len(&self) -> usize {
        self.points.len()
    }

    pub fn acc_len(&self) -> f32 {
        self.acc_len
    }

    pub fn too_long(&self) -> bool {
        self.too_long
    }
}

#[cfg(windows)]
struct HookState {
    runtime: MouseGestureRuntime,
    tracking: Mutex<HookTrackingState>,
    preview_sender: SyncSender<PreviewRequest>,
    preview_text: Arc<Mutex<Option<String>>>,
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
            Arc::new(HookState {
                runtime,
                tracking: Mutex::new(HookTrackingState::default()),
                preview_sender,
                preview_text,
            })
        });
        if let Ok(mut tracking) = runtime_state.tracking.lock() {
            *tracking = HookTrackingState::default();
        }

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
                        preview_enabled,
                        preview_on_end_only,
                    ) =
                        state
                            .runtime
                            .snapshots
                            .read()
                            .ok()
                            .map(|snap| {
                                (
                                    TriggerButton::from_setting(&snap.settings.trigger_button),
                                    snap.settings.min_point_distance.max(0.0),
                                    snap.settings.max_track_len.max(0.0),
                                    snap.settings.preview_enabled,
                                    snap.settings.preview_on_end_only,
                                )
                            })
                            .unwrap_or((None, 0.0, 0.0, false, false));
                    let min_point_distance_sq = min_point_distance * min_point_distance;

                    let mut tracking = match state.tracking.lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => poisoned.into_inner(),
                    };

                    let event_button = match event {
                        WM_LBUTTONDOWN | WM_LBUTTONUP => Some(TriggerButton::Left),
                        WM_RBUTTONDOWN | WM_RBUTTONUP => Some(TriggerButton::Right),
                        WM_MBUTTONDOWN | WM_MBUTTONUP => Some(TriggerButton::Middle),
                        _ => None,
                    };

                    if matches!(event, WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN)
                        && event_button == trigger_button
                    {
                        tracking.begin_stroke(event_button, point);
                        if let Ok(mut preview_guard) = state.preview_text.lock() {
                            *preview_guard = None;
                        }
                        if let Ok(mut overlay) = mouse_gesture_overlay().try_lock() {
                            overlay.begin_stroke(point);
                            overlay.update_preview(None, None);
                        }
                        return CallNextHookEx(None, code, wparam, lparam);
                    }

                    if event == WM_MOUSEMOVE && tracking.active_button.is_some() {
                        let stored =
                            tracking.handle_move(point, min_point_distance_sq, max_track_len);
                        if stored {
                            if preview_enabled && !preview_on_end_only {
                                let _ = state.preview_sender.try_send(PreviewRequest {
                                    points: tracking.points.clone(),
                                    force: false,
                                });
                            }
                            if let Ok(mut overlay) = mouse_gesture_overlay().try_lock() {
                                overlay.push_point(point);
                                if preview_enabled && !preview_on_end_only {
                                    let text = cached_preview_text(&state.preview_text);
                                    overlay.update_preview(text, Some(point));
                                } else {
                                    overlay.update_preview(None, None);
                                }
                            }
                        }
                        return CallNextHookEx(None, code, wparam, lparam);
                    }

                    if matches!(event, WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP)
                        && tracking.active_button == event_button
                    {
                        let (points, too_long) =
                            tracking.finish_stroke(point, min_point_distance_sq, max_track_len);
                        drop(tracking);

                        if preview_enabled {
                            let _ = state.preview_sender.try_send(PreviewRequest {
                                points: points.clone(),
                                force: true,
                            });
                        }

                        if let Ok(mut overlay) = mouse_gesture_overlay().try_lock() {
                            overlay.end_stroke();
                            overlay.update_preview(None, None);
                        }

                        let outcome = state.runtime.evaluate_track_with_limit(&points, too_long);
                        if outcome.matched {
                            return LRESULT(1);
                        }
                        return CallNextHookEx(None, code, wparam, lparam);
                    }

                    CallNextHookEx(None, code, wparam, lparam)
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
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                if stop_flag.load(Ordering::SeqCst) {
                    unsafe {
                        PostThreadMessageW(thread, WM_QUIT, WPARAM(0), LPARAM(0));
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
        build_gesture_templates, hash_direction_sequence, preview_worker_loop,
        sequence_changed, should_compute_preview, truncate_with_ellipsis, MouseGestureEventSink,
        MouseGestureRuntime, MouseGestureService, MouseGestureSnapshots, PreviewRequest,
        ProfileCache, PREVIEW_SIMILARITY_LABEL_MAX_LEN, PREVIEW_SIMILARITY_MAX_LEN,
        PREVIEW_SIMILARITY_TOP_N,
    };
    use crate::mouse_gestures::MouseHookBackend;
    use crate::gui::MouseGestureEvent;
    use crate::plugins::mouse_gestures::db::{
        ForegroundWindowInfo, MouseGestureBinding, MouseGestureDb, MouseGestureProfile,
    };
    use crate::plugins::mouse_gestures::engine::{GestureDirection, Point};
    use crate::plugins::mouse_gestures::settings::MouseGesturePluginSettings;
    use std::collections::HashMap;
    use std::sync::{mpsc, Arc, Mutex, RwLock};
    use std::time::{Duration, Instant};

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

    fn make_runtime(db: MouseGestureDb, settings: MouseGesturePluginSettings) -> MouseGestureRuntime {
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
    fn preview_throttle_boundary() {
        let now = Instant::now();
        let throttle_ms = 100;
        let too_soon = now - Duration::from_millis(throttle_ms - 1);
        let on_time = now - Duration::from_millis(throttle_ms);

        assert!(!should_compute_preview(too_soon, now, throttle_ms));
        assert!(should_compute_preview(on_time, now, throttle_ms));
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

        let deadline = Instant::now() + Duration::from_millis(200);
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
    fn preview_similarity_limits_top_matches() {
        let points = vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 10.0, y: 0.0 },
            Point { x: 10.0, y: -10.0 },
        ];

        let mut bindings_map = HashMap::new();
        bindings_map.insert("g1".to_string(), make_gesture(&[(0.0, 0.0), (10.0, 0.0), (10.0, -10.0)]));
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
        bindings_map.insert("g1".to_string(), make_gesture(&[(0.0, 0.0), (10.0, 0.0), (10.0, -10.0)]));
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
            ..MouseGesturePluginSettings::default()
        };
        let runtime = make_runtime(db.clone(), settings.clone());
        let snapshots = make_snapshots(db, settings);

        let summary = runtime
            .preview_similarity_text(&points, &snapshots)
            .expect("summary");

        assert!(summary.chars().count() <= PREVIEW_SIMILARITY_MAX_LEN);
    }

    fn make_right_points() -> Vec<Point> {
        vec![Point { x: 0.0, y: 0.0 }, Point { x: 80.0, y: 0.0 }]
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

    #[test]
    fn update_db_refreshes_cached_gesture_templates() {
        let service = MouseGestureService::new_with_backend_and_sink(
            Arc::new(TestBackend),
            Arc::new(TestEventSink),
        );
        let settings = MouseGesturePluginSettings {
            min_point_distance: 0.0,
            ..MouseGesturePluginSettings::default()
        };
        service.update_settings(settings);
        let runtime = MouseGestureRuntime {
            snapshots: Arc::clone(&service.snapshots),
            event_sink: Arc::new(TestEventSink),
            profile_cache: Arc::clone(&service.profile_cache),
        };
        let points = make_right_points();

        let snapshots = runtime
            .snapshots
            .read()
            .expect("read snapshots")
            .clone();
        let text = runtime
            .preview_similarity_text(&points, &snapshots)
            .expect("preview text");
        assert_eq!(text, "No match");

        service.update_db(make_db_with_right_gesture());

        let snapshots = runtime
            .snapshots
            .read()
            .expect("read snapshots")
            .clone();
        let text = runtime
            .preview_similarity_text(&points, &snapshots)
            .expect("preview text after update");
        assert!(
            text.contains("Swipe Right"),
            "expected updated templates in text: {text}"
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
