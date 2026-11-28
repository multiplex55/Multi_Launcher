use crate::actions::Action;
use crate::common::json_watch::watch_json;
use crate::common::strip_prefix_ci;
use crate::gui::utility_dialog_size::UtilityDialogSizeWindowExt as _;
use crate::mouse_gestures::db::{
    BindingMatchContext, GESTURES_FILE, GestureDb, SharedGestureDb, format_gesture_label,
    format_search_result_label, load_gestures,
};
use crate::mouse_gestures::service::{
    CancelBehavior, DEFAULT_RECOGNITION_INTERVAL_MS, DEFAULT_TRAIL_INTERVAL_MS, MouseGestureConfig,
    NoMatchBehavior, WheelCycleGate, with_service as with_gesture_service,
};
use crate::plugin::Plugin;
use eframe::egui;
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

pub(crate) const PLUGIN_NAME: &str = "mouse_gestures";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MouseGestureSettings {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_debug_logging")]
    pub debug_logging: bool,
    #[serde(default = "default_show_trail")]
    pub show_trail: bool,
    #[serde(default = "default_trail_color")]
    pub trail_color: [u8; 4],
    #[serde(default = "default_trail_width")]
    pub trail_width: f32,
    #[serde(default = "default_trail_start_move_px")]
    pub trail_start_move_px: f32,
    #[serde(default = "default_trail_interval_ms")]
    pub trail_interval_ms: u64,
    #[serde(default = "default_recognition_interval_ms")]
    pub recognition_interval_ms: u64,
    #[serde(default = "default_show_hint")]
    pub show_hint: bool,
    #[serde(default = "default_hint_offset")]
    pub hint_offset: (f32, f32),
    #[serde(default = "default_cancel_behavior")]
    pub cancel_behavior: CancelBehavior,
    #[serde(default = "default_no_match_behavior")]
    pub no_match_behavior: NoMatchBehavior,
    #[serde(default = "default_wheel_cycle_gate")]
    pub wheel_cycle_gate: WheelCycleGate,
    #[serde(default = "default_practice_mode")]
    pub practice_mode: bool,
    #[serde(default)]
    pub ignore_window_titles: Vec<String>,
}

impl Default for MouseGestureSettings {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            debug_logging: default_debug_logging(),
            show_trail: default_show_trail(),
            trail_color: default_trail_color(),
            trail_width: default_trail_width(),
            trail_start_move_px: default_trail_start_move_px(),
            trail_interval_ms: default_trail_interval_ms(),
            recognition_interval_ms: default_recognition_interval_ms(),
            show_hint: default_show_hint(),
            hint_offset: default_hint_offset(),
            cancel_behavior: default_cancel_behavior(),
            no_match_behavior: default_no_match_behavior(),
            wheel_cycle_gate: default_wheel_cycle_gate(),
            practice_mode: default_practice_mode(),
            ignore_window_titles: Vec::new(),
        }
    }
}

fn default_enabled() -> bool {
    true
}

fn default_debug_logging() -> bool {
    false
}

fn default_show_trail() -> bool {
    true
}

fn default_trail_color() -> [u8; 4] {
    [0xff, 0x00, 0x00, 0xff]
}

fn default_trail_width() -> f32 {
    2.0
}

fn default_trail_start_move_px() -> f32 {
    8.0
}

fn default_trail_interval_ms() -> u64 {
    DEFAULT_TRAIL_INTERVAL_MS
}

fn default_recognition_interval_ms() -> u64 {
    DEFAULT_RECOGNITION_INTERVAL_MS
}

fn default_show_hint() -> bool {
    true
}

fn default_hint_offset() -> (f32, f32) {
    (16.0, 16.0)
}

fn default_cancel_behavior() -> CancelBehavior {
    CancelBehavior::DoNothing
}

fn default_no_match_behavior() -> NoMatchBehavior {
    NoMatchBehavior::PassThroughClick
}

fn default_wheel_cycle_gate() -> WheelCycleGate {
    WheelCycleGate::Deadzone
}

fn default_practice_mode() -> bool {
    false
}

pub fn normalize_ignore_window_titles(values: &mut Vec<String>) -> bool {
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    for value in values.iter() {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        let key = trimmed.to_lowercase();
        if seen.insert(key) {
            normalized.push(trimmed.to_string());
        }
    }
    let changed = normalized != *values;
    if changed {
        *values = normalized;
    }
    changed
}

pub fn add_ignore_window_title(values: &mut Vec<String>, title: &str) -> bool {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return false;
    }
    if values
        .iter()
        .any(|entry| entry.trim().eq_ignore_ascii_case(trimmed))
    {
        return false;
    }
    values.push(trimmed.to_string());
    normalize_ignore_window_titles(values);
    true
}

pub fn collect_visible_window_titles() -> anyhow::Result<Vec<String>> {
    #[cfg(windows)]
    {
        use crate::windows_layout::{LayoutWindowOptions, collect_layout_windows};
        let windows = collect_layout_windows(LayoutWindowOptions::default())?;
        let mut seen = HashSet::new();
        let mut titles = Vec::new();
        for window in windows {
            let Some(title) = window.matcher.title else {
                continue;
            };
            let trimmed = title.trim();
            if trimmed.is_empty() {
                continue;
            }
            let key = trimmed.to_lowercase();
            if seen.insert(key) {
                titles.push(trimmed.to_string());
            }
        }
        titles.sort_by_key(|a| a.to_lowercase());
        Ok(titles)
    }
    #[cfg(not(windows))]
    {
        Ok(Vec::new())
    }
}

#[derive(Debug)]
struct MouseGestureRuntime {
    settings: MouseGestureSettings,
    plugin_enabled: bool,
    db: SharedGestureDb,
    directory_revision: u64,
    #[allow(dead_code)]
    watcher: Option<crate::common::json_watch::JsonWatcher>,
    gpui: Option<Arc<GpuiGestureOwner>>,
    watcher_active: Arc<AtomicBool>,
    watcher_gate: Arc<Mutex<()>>,
    path: std::path::PathBuf,
}

impl Default for MouseGestureRuntime {
    fn default() -> Self {
        let path = std::env::current_dir().unwrap_or_default().join(GESTURES_FILE);
        Self::new_at(path)
    }
}

impl MouseGestureRuntime {
    fn new_at(path: std::path::PathBuf) -> Self {
        let initial_db = match load_gestures(&path) {
            Ok(db) => db,
            Err(error) => {
                tracing::error!(
                    ?error,
                    "failed to load mouse gestures; using temporary defaults"
                );
                GestureDb::default()
            }
        };
        let db = Arc::new(Mutex::new(initial_db));
        let db_for_watcher = Arc::clone(&db);
        let watcher_active = Arc::new(AtomicBool::new(true));
        let watcher_gate = Arc::new(Mutex::new(()));
        let callback_active = watcher_active.clone();
        let callback_gate = watcher_gate.clone();
        let watcher_path = path.clone();

        let watcher = watch_json(&path, move || {
            let Ok(_guard) = callback_gate.lock() else { return; };
            if !callback_active.load(Ordering::Acquire) { return; }
            let mut result: anyhow::Result<bool> = Ok(false);
            with_service(|service| {
                result = reload_gesture_runtime_database(service, &watcher_path, &db_for_watcher);
            });
            match result {
                Ok(true) => {
                    crate::gui::send_event(crate::gui::WatchEvent::HotkeyDirectorySource(
                        hotkey_directory_source(),
                    ));
                }
                Ok(false) => {}
                Err(error) => tracing::error!(
                    ?error,
                    "failed to reload mouse gestures; retaining last-good data"
                ),
            }
        })
        .ok();

        Self {
            settings: MouseGestureSettings::default(),
            plugin_enabled: true,
            db,
            directory_revision: 0,
            watcher,
            gpui: None,
            watcher_active,
            watcher_gate,
            path,
        }
    }
}

fn reload_shared_gesture_db(
    path: impl AsRef<std::path::Path>,
    shared: &SharedGestureDb,
) -> anyhow::Result<bool> {
    let _transaction = crate::mouse_gestures::db::gestures_transaction_guard();
    let loaded = match crate::mouse_gestures::db::load_gestures_typed(path.as_ref())? {
        crate::common::persistence::LoadState::Missing => {
            anyhow::bail!("mouse gesture definitions were removed; retaining last-good state")
        }
        crate::common::persistence::LoadState::Empty => GestureDb::default(),
        crate::common::persistence::LoadState::Loaded(db) => db,
    };
    let mut current = shared
        .lock()
        .map_err(|_| anyhow::anyhow!("mouse gesture database lock poisoned"))?;
    if *current == loaded {
        return Ok(false);
    }
    *current = loaded;
    Ok(true)
}

fn reload_gesture_runtime_database(
    runtime: &mut MouseGestureRuntime,
    path: impl AsRef<std::path::Path>,
    expected_db: &SharedGestureDb,
) -> anyhow::Result<bool> {
    if !Arc::ptr_eq(&runtime.db, expected_db) {
        return Ok(false);
    }
    let result = if let Some(owner) = &runtime.gpui {
        reload_gpui_database(path, &runtime.db, owner)
    } else { reload_shared_gesture_db(path, &runtime.db) };
    let changed = match result {
        Ok(changed) => changed,
        Err(error) => {
            if runtime.gpui.is_some() { runtime.apply(); }
            return Err(error);
        }
    };
    if changed {
        runtime.bump_directory_revision();
    }
    if runtime.gpui.is_some() { runtime.apply(); }
    Ok(changed)
}

fn reload_gpui_database(path: impl AsRef<std::path::Path>, shared: &SharedGestureDb, owner: &GpuiGestureOwner) -> anyhow::Result<bool> {
    owner.close_admission();
    match reload_shared_gesture_db(path, shared) {
        Ok(changed) => { owner.open_admission(); Ok(changed) }
        Err(error) => { owner.set_status(Some(format!("Mouse Gestures database could not be loaded: {error}"))); Err(error) }
    }
}

impl MouseGestureRuntime {
    fn update_settings(&mut self, settings: MouseGestureSettings) {
        if self.settings != settings {
            self.bump_directory_revision();
        }
        self.settings = settings;
        self.apply();
    }

    fn set_plugin_enabled(&mut self, enabled: bool) {
        if self.plugin_enabled != enabled {
            self.bump_directory_revision();
        }
        self.plugin_enabled = enabled;
        self.apply();
    }

    fn bump_directory_revision(&mut self) {
        if let Some(owner) = &self.gpui { owner.invalidate(); owner.editor_revision.fetch_add(1, Ordering::AcqRel); }
        self.directory_revision = self.directory_revision.checked_add(1).unwrap_or(1);
    }

    fn hotkey_directory_binding_matches_committed(
        &self,
        publication: u64,
        entry_id: &crate::hotkey::directory::EntryId,
        gesture_index: usize,
        binding_index: usize,
        committed_database: &GestureDb,
    ) -> bool {
        if self.directory_revision != publication
            || entry_id != &hotkey_directory_binding_id(publication, gesture_index, binding_index)
        {
            return false;
        }

        let Ok(database) = self.db.lock() else {
            return false;
        };
        let (Some(current_gesture), Some(committed_gesture)) = (
            database.gestures.get(gesture_index),
            committed_database.gestures.get(gesture_index),
        ) else {
            return false;
        };
        current_gesture == committed_gesture
            && current_gesture.bindings.get(binding_index).is_some()
    }

    fn apply(&self) {
        let mut config = settings_to_config(&self.settings, self.plugin_enabled);
        if let Some(owner) = &self.gpui {
            config.enabled &= owner.active.load(Ordering::Acquire) && owner.ready.load(Ordering::Acquire) && self.db.lock().ok().is_some_and(|db| gpui_binding_available(&db, config.dir_mode));
        }
        let mut status = Err("Mouse Gestures service unavailable".to_owned());
        with_gesture_service(|svc| {
            svc.update_config(config);
            svc.update_db(Some(self.db.clone()));
            status = svc.gesture_capture_status();
        });
        if let Some(owner) = &self.gpui && owner.ready.load(Ordering::Acquire) { owner.set_status(status.err()); }
    }
}

fn hotkey_directory_binding_id(
    publication: u64,
    gesture_index: usize,
    binding_index: usize,
) -> crate::hotkey::directory::EntryId {
    crate::hotkey::directory::EntryId::new(format!(
        "gesture:{publication}:{gesture_index}:{binding_index}"
    ))
}

#[cfg(test)]
thread_local! {
    static TEST_DIRECTORY_RUNTIME: std::cell::RefCell<Option<MouseGestureRuntime>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
pub(crate) fn test_install_directory_runtime(db: GestureDb, revision: u64) -> crate::hotkey::directory::SourceSnapshot {
    let runtime = MouseGestureRuntime {
        settings: MouseGestureSettings::default(),
        plugin_enabled: true,
        db: Arc::new(Mutex::new(db)),
        directory_revision: revision,
        watcher: None,
        gpui: None,
        watcher_active: Arc::new(AtomicBool::new(true)),
        watcher_gate: Arc::new(Mutex::new(())),
        path: std::path::PathBuf::from(GESTURES_FILE),
    };
    let source = hotkey_directory_source_from_runtime(&runtime);
    TEST_DIRECTORY_RUNTIME.with(|slot| *slot.borrow_mut() = Some(runtime));
    source
}

#[cfg(test)]
pub(crate) struct TestDirectoryRuntimeGuard;

#[cfg(test)]
impl Drop for TestDirectoryRuntimeGuard {
    fn drop(&mut self) {
        TEST_DIRECTORY_RUNTIME.with(|slot| *slot.borrow_mut() = None);
    }
}

#[cfg(test)]
pub(crate) fn test_directory_runtime_guard() -> TestDirectoryRuntimeGuard {
    TestDirectoryRuntimeGuard
}

/// Confirms a directory request against the exact current gesture publication
/// and owner database before the editor reloads or reveals a positional row.
pub(crate) fn hotkey_directory_binding_matches_committed(
    publication: u64,
    entry_id: &crate::hotkey::directory::EntryId,
    gesture_index: usize,
    binding_index: usize,
    committed_database: &GestureDb,
) -> bool {
    #[cfg(test)]
    if let Some(result) = TEST_DIRECTORY_RUNTIME.with(|slot| slot.borrow().as_ref().map(|runtime| {
        runtime.hotkey_directory_binding_matches_committed(publication, entry_id, gesture_index, binding_index, committed_database)
    })) { return result; }
    let mut matches = false;
    with_service(|runtime| {
        matches = runtime.hotkey_directory_binding_matches_committed(
            publication,
            entry_id,
            gesture_index,
            binding_index,
            committed_database,
        );
    });
    matches
}

fn settings_to_config(settings: &MouseGestureSettings, plugin_enabled: bool) -> MouseGestureConfig {
    let mut config = MouseGestureConfig::default();
    config.enabled = settings.enabled && plugin_enabled;
    config.debug_logging = settings.debug_logging;
    config.trail_start_move_px = settings.trail_start_move_px;
    config.trail_interval_ms = settings.trail_interval_ms;
    config.recognition_interval_ms = settings.recognition_interval_ms;
    config.show_trail = settings.show_trail;
    config.trail_color = settings.trail_color;
    config.trail_width = settings.trail_width;
    config.show_hint = settings.show_hint;
    config.hint_offset = settings.hint_offset;
    config.cancel_behavior = settings.cancel_behavior;
    config.no_match_behavior = settings.no_match_behavior;
    config.wheel_cycle_gate = settings.wheel_cycle_gate;
    config.practice_mode = settings.practice_mode;
    config.ignore_window_titles = settings.ignore_window_titles.clone();
    config
}

static SERVICE: OnceCell<Mutex<MouseGestureRuntime>> = OnceCell::new();

mod editor;
pub use editor::{GestureEdit, GestureEditorSnapshot};

#[derive(Clone)]
pub struct GestureRelease {
    generation: u64,
    sequence: u64,
    tokens: String,
    mode: crate::mouse_gestures::engine::DirMode,
    index: usize,
    pub action: Action,
    pub foreground: crate::mouse_gestures::service::GestureForeground,
    pub visibility_revision: u64,
}

pub struct GpuiGestureOwner {
    db: SharedGestureDb,
    generation: Arc<AtomicU64>,
    editor_revision: AtomicU64,
    latest_sequence: Arc<AtomicU64>,
    active: AtomicBool,
    ready: AtomicBool,
    sender: std::sync::mpsc::SyncSender<GestureRelease>,
    receiver: Mutex<std::sync::mpsc::Receiver<GestureRelease>>,
    wake: Arc<dyn Fn() + Send + Sync>,
    shutdown_gate: Mutex<()>,
    shutdown_complete: AtomicBool,
    status: Mutex<(Option<String>, bool)>,
    shutdown_error: Mutex<Option<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GpuiGestureCapture {
    Active,
    Idle,
    Failed(String),
}

impl std::fmt::Debug for GpuiGestureOwner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.debug_struct("GpuiGestureOwner").finish_non_exhaustive() }
}

impl GpuiGestureOwner {
    /// Apply already-durable preference leaves off the UI thread. Calling this
    /// again retries capture even if the committed values did not change.
    pub fn apply_saved_preferences(&self, enabled: Option<bool>, practice_mode: Option<bool>, current: impl Fn() -> bool) -> Result<GpuiGestureCapture, String> {
        let mut runtime = SERVICE.get().ok_or_else(|| "Mouse Gestures runtime unavailable".to_owned())?.lock().map_err(|_| "Mouse Gestures runtime unavailable".to_owned())?;
        if !self.active.load(Ordering::Acquire) || !current() || !runtime.gpui.as_ref().is_some_and(|owner| std::ptr::eq(owner.as_ref(), self)) {
            return Err("Mouse Gestures owner retired".into());
        }
        let was_ready = self.ready.load(Ordering::Acquire);
        self.close_admission();
        if let Some(enabled) = enabled { runtime.settings.enabled = enabled; }
        if let Some(practice_mode) = practice_mode { runtime.settings.practice_mode = practice_mode; }
        runtime.bump_directory_revision();
        if !runtime.settings.enabled || !runtime.plugin_enabled {
            let mut stopped = Err("Mouse Gestures service unavailable".to_owned());
            with_gesture_service(|service| stopped = service.stop_gesture_checked());
            if let Err(error) = stopped {
                if was_ready && self.active.load(Ordering::Acquire) { self.open_admission(); }
                return Ok(GpuiGestureCapture::Failed(error));
            }
        }
        if was_ready && self.active.load(Ordering::Acquire) { self.open_admission(); }
        runtime.apply();
        let mut result = GpuiGestureCapture::Idle;
        with_gesture_service(|service| {
            service.reconcile_current();
            result = match service.gesture_capture_status() {
                Err(error) => GpuiGestureCapture::Failed(error),
                Ok(()) if service.is_running() => GpuiGestureCapture::Active,
                Ok(()) => GpuiGestureCapture::Idle,
            };
        });
        Ok(result)
    }
    fn invalidate(&self) { self.generation.fetch_add(1, Ordering::AcqRel); }
    fn close_admission(&self) { self.ready.store(false, Ordering::Release); self.invalidate(); }
    fn open_admission(&self) { self.invalidate(); self.ready.store(true, Ordering::Release); }
    fn set_status(&self, value: Option<String>) {
        let changed = self.status.lock().ok().is_some_and(|mut status| {
            if status.0 == value { false } else { status.0 = value; status.1 = true; true }
        });
        if changed { (self.wake)(); }
    }
    pub fn take_status(&self) -> Option<Option<String>> {
        let mut status = self.status.lock().ok()?;
        if !status.1 { return None; }
        status.1 = false;
        Some(status.0.clone())
    }
    pub fn is_current(&self, release: &GestureRelease) -> bool {
        if !self.active.load(Ordering::Acquire) || !self.ready.load(Ordering::Acquire) || self.generation.load(Ordering::Acquire) != release.generation || self.latest_sequence.load(Ordering::Acquire) != release.sequence { return false; }
        let Ok(db) = self.db.try_lock() else { return false; };
        if ambiguous_gesture(&db, &release.tokens, release.mode) { return false; }
        let Some((label, bindings)) = db.match_bindings_owned(&release.tokens, release.mode) else { return false; };
        let valid = bindings.get(release.index).is_some_and(|binding| binding.kind == crate::mouse_gestures::db::BindingKind::SetQuery && binding.to_action(&label) == release.action);
        valid && self.active.load(Ordering::Acquire) && self.ready.load(Ordering::Acquire) && self.generation.load(Ordering::Acquire) == release.generation && self.latest_sequence.load(Ordering::Acquire) == release.sequence
    }
    pub fn take(&self) -> Vec<GestureRelease> {
        let Ok(receiver) = self.receiver.lock() else { return Vec::new(); };
        receiver.try_iter().collect()
    }
    pub fn fence(&self) { self.active.store(false, Ordering::Release); self.invalidate(); }
    pub fn shutdown_checked(&self) -> Result<(), String> {
        let _shutdown = self.shutdown_gate.lock().map_err(|_| "Mouse Gestures shutdown unavailable".to_owned())?;
        if self.shutdown_complete.load(Ordering::Acquire) { return Ok(()); }
        if let Some(error) = self.shutdown_error.lock().map_err(|_| "Mouse Gestures shutdown status unavailable".to_owned())?.clone() { return Err(error); }
        self.fence();
        let Some(runtime) = SERVICE.get() else { return Ok(()); };
        let mut runtime = runtime.lock().map_err(|_| "Mouse Gestures runtime unavailable".to_owned())?;
        if runtime.gpui.as_ref().is_some_and(|owner| !std::ptr::eq(owner.as_ref(), self)) {
            return Err("Mouse Gestures owner changed before shutdown".into());
        }
        runtime.plugin_enabled = false;
        let mut stop_result = Err("Mouse Gestures service unavailable".to_owned());
        with_gesture_service(|service| { stop_result = service.stop_gesture_checked(); service.update_release_sink(None); });
        let watcher = if runtime.gpui.is_some() {
            runtime.gpui = None;
            runtime.watcher_active.store(false, Ordering::Release);
            runtime.watcher.take()
        } else { None };
        let watcher_gate = runtime.watcher_gate.clone();
        drop(runtime);
        drop(watcher);
        drop(watcher_gate.lock().map_err(|_| "Mouse Gestures watcher drain failed".to_owned())?);
        if let Err(error) = stop_result {
            if error.contains("panicked") && let Ok(mut status) = self.shutdown_error.lock() { *status = Some(error.clone()); }
            return Err(error);
        }
        let reservation = crate::mouse_gestures::service::reserve_frontend_restart()?;
        reservation.shutdown_checked()?;
        self.shutdown_complete.store(true, Ordering::Release);
        Ok(())
    }
}

impl crate::mouse_gestures::service::GestureReleaseSink for GpuiGestureOwner {
    fn stroke_generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }
    fn release(&self, generation: u64, sequence: Option<u64>, foreground: Option<crate::mouse_gestures::service::GestureForeground>, visibility_revision: Option<u64>, tokens: &str, mode: crate::mouse_gestures::engine::DirMode, index: usize, action: Action) -> crate::mouse_gestures::service::GestureReleaseDisposition {
        use crate::mouse_gestures::service::GestureReleaseDisposition::{Accepted, Ignore, PassThrough};
        let Some(foreground) = foreground else { return Ignore; };
        let Some(sequence) = sequence else { return Ignore; };
        let Some(visibility_revision) = visibility_revision else { return Ignore; };
        if !self.active.load(Ordering::Acquire) || !self.ready.load(Ordering::Acquire) || self.generation.load(Ordering::Acquire) != generation || self.latest_sequence.load(Ordering::Acquire) != sequence { return Ignore; }
        let Ok(db) = self.db.try_lock() else { return Ignore; };
        if ambiguous_gesture(&db, tokens, mode) {
            return if self.active.load(Ordering::Acquire) && self.ready.load(Ordering::Acquire) && self.generation.load(Ordering::Acquire) == generation && self.latest_sequence.load(Ordering::Acquire) == sequence { PassThrough } else { Ignore };
        }
        let Some((label, bindings)) = db.match_bindings_owned(tokens, mode) else { return Ignore; };
        let Some(binding) = bindings.get(index) else { return Ignore; };
        if binding.to_action(&label) != action || self.generation.load(Ordering::Acquire) != generation || self.latest_sequence.load(Ordering::Acquire) != sequence || !self.active.load(Ordering::Acquire) || !self.ready.load(Ordering::Acquire) { return Ignore; }
        if binding.kind != crate::mouse_gestures::db::BindingKind::SetQuery { return PassThrough; }
        drop(db);
        let release = GestureRelease { generation, sequence, tokens: tokens.to_owned(), mode, index, action, foreground, visibility_revision };
        if !self.is_current(&release) { return Ignore; }
        if self.sender.try_send(release).is_err() { return Ignore; }
        (self.wake)();
        Accepted
    }
}

fn ambiguous_gesture(db: &GestureDb, tokens: &str, mode: crate::mouse_gestures::engine::DirMode) -> bool {
    db.gestures.iter().filter(|gesture| gesture.enabled && gesture.dir_mode == mode && gesture.tokens == tokens).take(2).count() > 1
}

fn gpui_binding_available(db: &GestureDb, mode: crate::mouse_gestures::engine::DirMode) -> bool {
    db.gestures.iter().any(|gesture| gesture.enabled && gesture.dir_mode == mode && !gesture.tokens.is_empty() && gesture.tokens.chars().all(|token| match mode {
        crate::mouse_gestures::engine::DirMode::Four => matches!(token, 'U' | 'D' | 'L' | 'R'),
        crate::mouse_gestures::engine::DirMode::Eight => matches!(token, '1' | '2' | '3' | '4' | '6' | '7' | '8' | '9'),
    }) && !ambiguous_gesture(db, &gesture.tokens, mode) && gesture.bindings.iter().any(|binding| binding.enabled && binding.kind == crate::mouse_gestures::db::BindingKind::SetQuery && !binding.action.trim().is_empty() && !binding.action.contains(['\r', '\n'])))
}

/// Installs the existing watcher/service for the selected GPUI app session.
pub fn start_gpui_gestures(settings: MouseGestureSettings, enabled: bool, root: &crate::platform::app_data::AppDataRoot, visibility_revision: Arc<AtomicU64>, visibility_active: Arc<AtomicBool>, wake: Arc<dyn Fn() + Send + Sync>) -> Result<Arc<GpuiGestureOwner>, String> {
    let path = root.path().join(GESTURES_FILE);
    let read_database = || -> Result<GestureDb, String> {
        match crate::mouse_gestures::db::load_gestures_typed(&path).map_err(|error| error.to_string())? {
            crate::common::persistence::LoadState::Missing | crate::common::persistence::LoadState::Empty => Ok(GestureDb::default()),
            crate::common::persistence::LoadState::Loaded(database) => Ok(database),
        }
    };
    let initial = read_database();
    let runtime = SERVICE.get_or_init(|| Mutex::new(MouseGestureRuntime::new_at(path.clone())));
    let mut runtime = runtime.lock().map_err(|_| "Mouse Gestures runtime unavailable".to_owned())?;
    if runtime.gpui.is_some() { return Err("Mouse Gestures already has a GPUI owner".into()); }
    if runtime.path != path { return Err("Mouse Gestures is already bound to a different profile".into()); }
    if runtime.watcher.is_none() { return Err("Mouse Gestures database watcher is unavailable".into()); }
    let latest = read_database();
    if let (Ok(first), Ok(second)) = (&initial, &latest) && first != second {
        return Err("Mouse Gestures database changed while starting".into());
    }
    let startup_error = latest.as_ref().err().cloned();
    let database = latest.unwrap_or_default();
    crate::mouse_gestures::service::install_gesture_visibility_revision(visibility_revision, visibility_active)?;
    *runtime.db.lock().map_err(|_| "Mouse Gestures database unavailable".to_owned())? = database;
    let (sender, receiver) = std::sync::mpsc::sync_channel(64);
    let generation = crate::mouse_gestures::service::gesture_database_generation();
    generation.fetch_add(1, Ordering::AcqRel);
    let latest_sequence = crate::mouse_gestures::service::gesture_stroke_sequence();
    latest_sequence.fetch_add(1, Ordering::AcqRel);
    let owner = Arc::new(GpuiGestureOwner { db: runtime.db.clone(), generation, editor_revision: AtomicU64::new(0), latest_sequence, active: AtomicBool::new(true), ready: AtomicBool::new(startup_error.is_none()), sender, receiver: Mutex::new(receiver), wake, shutdown_gate: Mutex::new(()), shutdown_complete: AtomicBool::new(false), status: Mutex::new((None, false)), shutdown_error: Mutex::new(None) });
    runtime.settings = settings;
    runtime.plugin_enabled = enabled;
    runtime.gpui = Some(owner.clone());
    if let Some(error) = startup_error { owner.set_status(Some(format!("Mouse Gestures database could not be loaded: {error}"))); }
    with_gesture_service(|service| { service.stop(); service.update_release_sink(Some(owner.clone())); });
    runtime.apply();
    Ok(owner)
}

fn with_service<F>(f: F)
where
    F: FnOnce(&mut MouseGestureRuntime),
{
    let service = SERVICE.get_or_init(|| Mutex::new(MouseGestureRuntime::default()));
    match service.lock() {
        Ok(mut guard) => f(&mut guard),
        Err(e) => tracing::error!(?e, "failed to lock mouse gestures runtime"),
    }
}

fn gesture_db_snapshot() -> GestureDb {
    let mut snapshot = GestureDb::default();
    with_service(|service| {
        if let Ok(db) = service.db.lock() {
            snapshot = db.clone();
        }
    });
    snapshot
}

pub fn publish_committed_gesture_db(committed: GestureDb) {
    with_service(|service| {
        if let Some(owner) = &service.gpui { owner.close_admission(); }
        let mut published = false;
        let changed = if let Ok(mut db) = service.db.lock() {
            published = true;
            if *db != committed {
                *db = committed;
                true
            } else {
                false
            }
        } else {
            false
        };
        if changed {
            service.bump_directory_revision();
        }
        if let Some(owner) = &service.gpui {
            if published { owner.open_admission(); } else { owner.set_status(Some("Mouse Gestures database unavailable".into())); }
        }
        if service.gpui.is_some() { service.apply(); }
    });
}

/// Returns a sanitized view of configured gesture bindings for the hotkey
/// directory. Gesture labels, tokens, action text, arguments, and stroke data
/// remain inside the gesture owner; stable identities are scoped to this
/// immutable database publication.
pub fn hotkey_directory_source() -> crate::hotkey::directory::SourceSnapshot {
    #[cfg(test)]
    if let Some(source) = TEST_DIRECTORY_RUNTIME.with(|slot| slot.borrow().as_ref().map(hotkey_directory_source_from_runtime)) {
        return source;
    }
    let mut snapshot = None;
    with_service(|service| {
        snapshot = Some(hotkey_directory_source_from_runtime(service));
    });
    snapshot.unwrap_or_else(|| {
        crate::hotkey::directory::SourceSnapshot::new(
            crate::hotkey::directory::Source::MouseGestures,
            0,
            Vec::new(),
            Vec::new(),
        )
    })
}

fn hotkey_directory_source_from_runtime(
    service: &MouseGestureRuntime,
) -> crate::hotkey::directory::SourceSnapshot {
    use crate::hotkey::directory::{
        Candidate, CandidateState, EditDestination, EntryId, Owner, Reason, RouteKind, Scope,
        Source, SourceSnapshot,
    };
    let revision = service.directory_revision;
    let plugin_enabled = service.plugin_enabled;
    let gestures_enabled = service.settings.enabled;
    let database = service.db.lock().map(|db| db.clone()).unwrap_or_default();

    let mut candidates = Vec::new();
    for (gesture_index, gesture) in database.gestures.iter().enumerate() {
        for (binding_index, binding) in gesture.bindings.iter().enumerate() {
            let enabled = plugin_enabled && gestures_enabled && gesture.enabled && binding.enabled;
            candidates.push(Candidate {
                id: hotkey_directory_binding_id(revision, gesture_index, binding_index),
                owner: Owner::MouseGestures,
                owner_label: "Mouse Gestures".into(),
                action_label: format!("{} — binding {}", gesture.label, binding_index + 1),
                chord: None,
                route: RouteKind::Gesture,
                scope: Scope::Gesture,
                source_revision: revision,
                expected_runtime_generation: None,
                state: if enabled {
                    CandidateState::Unknown
                } else {
                    CandidateState::Disabled
                },
                reason: if enabled {
                    Some(Reason::LocalRouteHasNoGlobalReceipt)
                } else {
                    Some(Reason::OwnerDisabled)
                },
                edit: Some(EditDestination::GestureBinding {
                    publication: revision,
                    gesture_index: gesture_index as u32,
                    binding_index: binding_index as u32,
                }),
            });
        }
    }
    SourceSnapshot::new(Source::MouseGestures, revision, candidates, Vec::new())
}

pub fn apply_runtime_settings(settings: MouseGestureSettings) {
    with_service(|svc| svc.update_settings(settings));
}

pub fn sync_enabled_plugins(enabled_plugins: Option<&HashSet<String>>) {
    let enabled = enabled_plugins
        .map(|set| set.contains(PLUGIN_NAME))
        .unwrap_or(true);
    with_service(|svc| svc.set_plugin_enabled(enabled));
}

pub struct FrontendRestartReservation {
    service: crate::mouse_gestures::service::FrontendRestartReservation,
}

impl FrontendRestartReservation {
    pub fn is_current(&self) -> bool {
        self.service.is_current()
            && SERVICE
                .get()
                .is_none_or(|runtime| runtime.lock().is_ok())
    }

    /// Drop the JSON watcher only after the committed transition, outside the
    /// MouseGestureRuntime mutex, then require the native hook to retire.
    pub fn shutdown_checked(&self) -> Result<(), String> {
        if let Some(runtime) = SERVICE.get() {
            let mut runtime = runtime
                .lock()
                .map_err(|_| "Mouse Gestures runtime state is unavailable".to_owned())?;
            let watcher = runtime.watcher.take();
            drop(runtime);
            drop(watcher);
        }
        self.service.shutdown_checked()
    }
}

pub fn reserve_frontend_restart() -> Result<FrontendRestartReservation, String> {
    if let Some(runtime) = SERVICE.get() {
        let _runtime = runtime
            .lock()
            .map_err(|_| "Mouse Gestures runtime state is unavailable".to_owned())?;
    }
    Ok(FrontendRestartReservation {
        service: crate::mouse_gestures::service::reserve_frontend_restart()?,
    })
}

#[derive(Default)]
pub struct MouseGesturesPlugin {
    settings: MouseGestureSettings,
    ignore_input: String,
    window_picker_open: bool,
    window_picker_titles: Vec<String>,
    window_picker_error: Option<String>,
}

impl MouseGesturesPlugin {
    fn command_actions() -> Vec<Action> {
        vec![
            Action {
                label: "mg".into(),
                desc: "Mouse gestures".into(),
                action: "query:mg ".into(),
                args: None,
            },
            Action {
                label: "mg settings".into(),
                desc: "Mouse gestures".into(),
                action: "mg:dialog:settings".into(),
                args: None,
            },
            Action {
                label: "mg edit".into(),
                desc: "Mouse gestures".into(),
                action: "mg:dialog".into(),
                args: None,
            },
            Action {
                label: "mg add".into(),
                desc: "Mouse gestures".into(),
                action: "mg:dialog:binding".into(),
                args: None,
            },
            Action {
                label: "mg list".into(),
                desc: "Mouse gestures".into(),
                action: "query:mg list".into(),
                args: None,
            },
            Action {
                label: "mg find".into(),
                desc: "Mouse gestures".into(),
                action: "query:mg find ".into(),
                args: None,
            },
            Action {
                label: "mg where".into(),
                desc: "Mouse gestures".into(),
                action: "query:mg where ".into(),
                args: None,
            },
            Action {
                label: "mg conflicts".into(),
                desc: "Mouse gestures".into(),
                action: "query:mg conflicts".into(),
                args: None,
            },
        ]
    }

    fn list_gestures(filter: &str) -> Vec<Action> {
        let db = gesture_db_snapshot();
        let matcher = SkimMatcherV2::default();
        let filter = filter.trim().to_lowercase();
        db.gestures
            .iter()
            .filter(|gesture| {
                if filter.is_empty() {
                    return true;
                }
                let label = format_gesture_label(gesture).to_lowercase();
                matcher.fuzzy_match(&label, &filter).is_some()
            })
            .map(|gesture| Action {
                label: format_gesture_label(gesture),
                desc: "Mouse gestures".into(),
                action: "mg:dialog".into(),
                args: None,
            })
            .collect()
    }

    fn format_match_desc(context: &BindingMatchContext) -> String {
        if context.fields.is_empty() {
            return "Mouse gestures".into();
        }
        let labels: Vec<&str> = context
            .fields
            .iter()
            .map(|field| match field {
                crate::mouse_gestures::db::BindingMatchField::GestureLabel => "gesture label",
                crate::mouse_gestures::db::BindingMatchField::Tokens => "tokens",
                crate::mouse_gestures::db::BindingMatchField::BindingLabel => "binding label",
                crate::mouse_gestures::db::BindingMatchField::Action => "action",
                crate::mouse_gestures::db::BindingMatchField::Args => "args",
            })
            .collect();
        format!("Mouse gestures (matches: {})", labels.join(", "))
    }
}

impl Plugin for MouseGesturesPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        if trimmed.eq_ignore_ascii_case("mg") {
            return Self::command_actions();
        }
        if strip_prefix_ci(trimmed, "mg settings").is_some()
            || strip_prefix_ci(trimmed, "mg setting").is_some()
        {
            return vec![Action {
                label: "Open mouse gesture settings".into(),
                desc: "Mouse gestures".into(),
                action: "mg:dialog:settings".into(),
                args: None,
            }];
        }
        if strip_prefix_ci(trimmed, "mg edit").is_some() {
            return vec![Action {
                label: "Edit mouse gestures".into(),
                desc: "Mouse gestures".into(),
                action: "mg:dialog".into(),
                args: None,
            }];
        }
        if strip_prefix_ci(trimmed, "mg add").is_some() {
            return vec![Action {
                label: "Add mouse gesture binding".into(),
                desc: "Mouse gestures".into(),
                action: "mg:dialog:binding".into(),
                args: None,
            }];
        }
        if let Some(rest) = strip_prefix_ci(trimmed, "mg find") {
            let query = rest.trim();
            let db = gesture_db_snapshot();
            return db
                .search_bindings(query)
                .into_iter()
                .map(|(gesture, binding, context)| Action {
                    label: format_search_result_label(&gesture, &binding),
                    desc: Self::format_match_desc(&context),
                    action: "mg:dialog".into(),
                    args: None,
                })
                .collect();
        }
        if let Some(rest) = strip_prefix_ci(trimmed, "mg where") {
            let action_prefix = rest.trim();
            let db = gesture_db_snapshot();
            return db
                .find_by_action(action_prefix)
                .into_iter()
                .map(|(gesture, binding)| Action {
                    label: format_search_result_label(&gesture, &binding),
                    desc: "Mouse gestures".into(),
                    action: "mg:dialog".into(),
                    args: None,
                })
                .collect();
        }
        if let Some(rest) = strip_prefix_ci(trimmed, "mg conflicts") {
            if !rest.trim().is_empty() {
                return Vec::new();
            }
            let db = gesture_db_snapshot();
            let mut actions = Vec::new();
            for conflict in db.find_conflicts() {
                let conflict_desc = match conflict.kind {
                    crate::mouse_gestures::db::GestureConflictKind::DuplicateTokens => {
                        "Mouse gestures (conflict: duplicate tokens)"
                    }
                    crate::mouse_gestures::db::GestureConflictKind::PrefixOverlap => {
                        "Mouse gestures (conflict: prefix overlap)"
                    }
                };
                for gesture in conflict.gestures {
                    for binding in gesture.bindings.iter().filter(|binding| binding.enabled) {
                        actions.push(Action {
                            label: format_search_result_label(&gesture, binding),
                            desc: conflict_desc.into(),
                            action: "mg:dialog".into(),
                            args: None,
                        });
                    }
                }
            }
            return actions;
        }
        if let Some(rest) = strip_prefix_ci(trimmed, "mg list") {
            return Self::list_gestures(rest);
        }
        if let Some(rest) = strip_prefix_ci(trimmed, "mg ") {
            return Self::list_gestures(rest);
        }
        Vec::new()
    }

    fn name(&self) -> &str {
        PLUGIN_NAME
    }

    fn description(&self) -> &str {
        "Handle mouse gestures (prefix: `mg`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        Self::command_actions()
    }

    fn default_settings(&self) -> Option<serde_json::Value> {
        serde_json::to_value(MouseGestureSettings::default()).ok()
    }

    fn apply_settings(&mut self, value: &serde_json::Value) {
        if let Ok(settings) = serde_json::from_value::<MouseGestureSettings>(value.clone()) {
            self.settings = settings.clone();
            apply_runtime_settings(settings);
        }
    }

    fn settings_ui(&mut self, ui: &mut egui::Ui, value: &mut serde_json::Value) {
        use crate::gui::theme::widgets::{
            ButtonEmphasis, StatusTone, compact_section_heading, labeled_form_row, semantic_status,
            with_button_emphasis,
        };

        let mut cfg =
            serde_json::from_value::<MouseGestureSettings>(value.clone()).unwrap_or_default();

        let mut changed = false;

        compact_section_heading(ui, "Gesture behavior", None);
        changed |= ui
            .checkbox(&mut cfg.enabled, "Enable mouse gestures")
            .changed();
        changed |= ui
            .checkbox(&mut cfg.debug_logging, "Enable debug logging")
            .changed();

        changed |= ui
            .checkbox(&mut cfg.show_trail, "Show trail overlay")
            .changed();

        labeled_form_row(ui, "Trail color", |ui| {
            let mut color = egui::Color32::from_rgba_unmultiplied(
                cfg.trail_color[0],
                cfg.trail_color[1],
                cfg.trail_color[2],
                cfg.trail_color[3],
            );

            let resp = ui.color_edit_button_srgba(&mut color);
            if resp.changed() {
                cfg.trail_color = [color.r(), color.g(), color.b(), color.a()];
                changed = true;
            }
            resp
        });

        labeled_form_row(ui, "Trail width", |ui| {
            let response = ui.add(
                egui::DragValue::new(&mut cfg.trail_width)
                    .range(1.0..=20.0)
                    .speed(0.5),
            );
            changed |= response.changed();
            response
        });

        changed |= ui
            .checkbox(&mut cfg.show_hint, "Show hint overlay")
            .changed();

        labeled_form_row(ui, "Hint offset (x, y)", |ui| {
            let first = ui.add(
                egui::DragValue::new(&mut cfg.hint_offset.0)
                    .range(-200.0..=200.0)
                    .speed(1.0),
            );
            changed |= first.changed();
            let second = ui.add(
                egui::DragValue::new(&mut cfg.hint_offset.1)
                    .range(-200.0..=200.0)
                    .speed(1.0),
            );
            changed |= second.changed();
            second
        });

        labeled_form_row(ui, "Cancel behavior", |ui| {
            egui::ComboBox::from_id_salt("mg_cancel_behavior")
                .selected_text(cancel_behavior_label(cfg.cancel_behavior))
                .show_ui(ui, |ui| {
                    changed |= ui
                        .selectable_value(
                            &mut cfg.cancel_behavior,
                            CancelBehavior::DoNothing,
                            "Do nothing",
                        )
                        .changed();
                    changed |= ui
                        .selectable_value(
                            &mut cfg.cancel_behavior,
                            CancelBehavior::PassThroughClick,
                            "Pass through right-click",
                        )
                        .changed();
                })
                .response
        });

        labeled_form_row(ui, "No-match behavior", |ui| {
            egui::ComboBox::from_id_salt("mg_no_match_behavior")
                .selected_text(no_match_behavior_label(cfg.no_match_behavior))
                .show_ui(ui, |ui| {
                    changed |= ui
                        .selectable_value(
                            &mut cfg.no_match_behavior,
                            NoMatchBehavior::DoNothing,
                            "Do nothing",
                        )
                        .changed();
                    changed |= ui
                        .selectable_value(
                            &mut cfg.no_match_behavior,
                            NoMatchBehavior::PassThroughClick,
                            "Pass through right-click",
                        )
                        .changed();
                    changed |= ui
                        .selectable_value(
                            &mut cfg.no_match_behavior,
                            NoMatchBehavior::ShowNoMatchHint,
                            "Show no-match hint",
                        )
                        .changed();
                })
                .response
        });
        ui.small(
            "Fallback runs when a gesture does not match; default is pass-through right-click.",
        );

        ui.separator();
        compact_section_heading(
            ui,
            "Ignore windows (disable gestures)",
            Some(
                "Gestures will be ignored when the active window title contains one of these entries.",
            ),
        );

        let mut remove_index: Option<usize> = None;
        if cfg.ignore_window_titles.is_empty() {
            ui.label("No ignored windows.");
        } else {
            for (index, title) in cfg.ignore_window_titles.iter().enumerate() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(title);
                    let remove = ui.button("Remove");
                    if with_button_emphasis(ui, remove, ButtonEmphasis::Destructive).clicked() {
                        remove_index = Some(index);
                    }
                });
            }
        }
        if let Some(index) = remove_index {
            cfg.ignore_window_titles.remove(index);
            changed = true;
        }

        ui.horizontal_wrapped(|ui| {
            let response = ui.text_edit_singleline(&mut self.ignore_input);
            let add_button = ui.button("Add");
            let mut add_now =
                with_button_emphasis(ui, add_button, ButtonEmphasis::Primary).clicked();
            if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                add_now = true;
            }
            if add_now {
                if add_ignore_window_title(&mut cfg.ignore_window_titles, &self.ignore_input) {
                    changed = true;
                }
                self.ignore_input.clear();
            }
        });

        let window_button = ui.add_enabled(cfg!(windows), egui::Button::new("Select window..."));
        if !cfg!(windows) {
            window_button.on_hover_text("Window picker is only available on Windows.");
        } else if window_button.clicked() {
            self.window_picker_error = None;
            match collect_visible_window_titles() {
                Ok(titles) => {
                    self.window_picker_titles = titles;
                    self.window_picker_open = true;
                }
                Err(err) => {
                    self.window_picker_error = Some(format!("Failed to enumerate windows: {err}"));
                    self.window_picker_titles.clear();
                    self.window_picker_open = true;
                }
            }
        }

        if self.window_picker_open {
            let mut open = self.window_picker_open;
            egui::Window::new("Select window to ignore")
                .resizable(true)
                .show_utility(
                    ui.ctx(),
                    "gesture-ignore-window-picker",
                    egui::Id::new("Select window to ignore"),
                    Some(&mut open),
                    crate::gui::utility_dialog_size::UtilityWindowSizeRecipe::new(
                        crate::gui::utility_dialog_size::UtilityWindowSizeDefault::intrinsic(),
                        egui::Vec2::ZERO,
                        [None, None],
                    ),
                    |ui| {
                        if let Some(err) = &self.window_picker_error {
                            semantic_status(
                                ui,
                                StatusTone::Error,
                                "Window picker error",
                                Some(err),
                            );
                            ui.separator();
                        }

                        if self.window_picker_titles.is_empty() {
                            ui.label("No windows found.");
                        } else {
                            egui::ScrollArea::vertical()
                                .max_height(220.0)
                                .show(ui, |ui| {
                                    for title in self.window_picker_titles.clone() {
                                        ui.horizontal(|ui| {
                                            ui.label(&title);
                                            if ui.button("Add").clicked()
                                                && add_ignore_window_title(
                                                    &mut cfg.ignore_window_titles,
                                                    &title,
                                                )
                                            {
                                                changed = true;
                                            }
                                        });
                                    }
                                });
                        }
                    },
                );
            self.window_picker_open = open;
        }

        if normalize_ignore_window_titles(&mut cfg.ignore_window_titles) {
            changed = true;
        }

        // Only write+apply when something changed.
        if changed {
            match serde_json::to_value(&cfg) {
                Ok(v) => *value = v,
                Err(e) => tracing::error!(?e, "failed to serialize mouse gesture settings"),
            }
            self.settings = cfg.clone();
            apply_runtime_settings(cfg);
        }
    }
}

fn cancel_behavior_label(value: CancelBehavior) -> &'static str {
    match value {
        CancelBehavior::DoNothing => "Do nothing",
        CancelBehavior::PassThroughClick => "Pass through right-click",
    }
}

fn no_match_behavior_label(value: NoMatchBehavior) -> &'static str {
    match value {
        NoMatchBehavior::DoNothing => "Do nothing",
        NoMatchBehavior::PassThroughClick => "Pass through right-click",
        NoMatchBehavior::ShowNoMatchHint => "Show no-match hint",
    }
}

#[cfg(test)]
mod persistence_tests {
    use super::*;
    use crate::mouse_gestures::db::{
        BindingEntry, BindingKind, GestureDb, GestureEntry, SCHEMA_VERSION,
    };
    use crate::mouse_gestures::engine::DirMode;

    fn db(label: &str) -> GestureDb {
        GestureDb {
            schema_version: SCHEMA_VERSION,
            gestures: vec![GestureEntry {
                label: label.into(),
                tokens: "L".into(),
                dir_mode: DirMode::Four,
                stroke: Vec::new(),
                enabled: true,
                bindings: Vec::new(),
            }],
        }
    }

    #[test]
    fn gpui_release_requires_exact_selected_set_query_and_live_generation() {
        use crate::mouse_gestures::service::{GestureReleaseSink, GestureReleaseDisposition::{Accepted, Ignore, PassThrough}};
        let mut database = db("left");
        let query = BindingEntry { label: "query".into(), kind: BindingKind::SetQuery, action: "hello".into(), args: None, enabled: true };
        let unsupported = BindingEntry { label: "other".into(), kind: BindingKind::Execute, action: "query:hello".into(), args: None, enabled: true };
        database.gestures[0].bindings = vec![unsupported.clone(), query.clone()];
        let (sender, receiver) = std::sync::mpsc::sync_channel(2);
        let owner = GpuiGestureOwner { db: Arc::new(Mutex::new(database)), generation: Arc::new(AtomicU64::new(9)), editor_revision: AtomicU64::new(0), latest_sequence: Arc::new(AtomicU64::new(0)), active: AtomicBool::new(true), ready: AtomicBool::new(true), sender, receiver: Mutex::new(receiver), wake: Arc::new(|| {}), shutdown_gate: Mutex::new(()), shutdown_complete: AtomicBool::new(false), status: Mutex::new((None, false)), shutdown_error: Mutex::new(None) };
        let foreground = Some(crate::mouse_gestures::service::GestureForeground { hwnd: 1, process_id: 2, thread_id: 3 });
        assert_eq!(owner.release(9, Some(0), foreground, Some(0), "L", DirMode::Four, 0, unsupported.to_action("left")), PassThrough);
        assert_eq!(owner.release(9, Some(0), foreground, Some(0), "L", DirMode::Four, 1, query.to_action("left")), Accepted);
        let queued = owner.take().pop().unwrap();
        assert!(owner.is_current(&queued));
        owner.latest_sequence.fetch_add(1, Ordering::AcqRel);
        assert!(!owner.is_current(&queued));
        assert_eq!(owner.release(9, Some(1), foreground, Some(0), "L", DirMode::Four, 1, query.to_action("left")), Accepted);
        let queued = owner.take().pop().unwrap();
        assert!(owner.is_current(&queued));
        owner.invalidate();
        assert!(!owner.is_current(&queued));
        assert_eq!(owner.release(9, Some(0), foreground, Some(0), "L", DirMode::Four, 1, query.to_action("left")), Ignore);
        owner.fence();
        assert_eq!(owner.release(owner.stroke_generation(), Some(0), foreground, Some(0), "L", DirMode::Four, 1, query.to_action("left")), Ignore);
    }

    #[test]
    fn enablement_publication_requires_fresh_release_and_keeps_sibling() {
        use crate::mouse_gestures::service::{GestureReleaseSink, GestureReleaseDisposition::{Accepted, Ignore}};
        let mut database = db("left");
        let binding = BindingEntry { label: "query".into(), kind: BindingKind::SetQuery, action: "hello".into(), args: None, enabled: true };
        database.gestures[0].bindings.push(binding.clone());
        let mut sibling = database.gestures[0].clone();
        sibling.label = "right".into();
        sibling.tokens = "R".into();
        database.gestures.push(sibling);
        let (sender, receiver) = std::sync::mpsc::sync_channel(4);
        let owner = GpuiGestureOwner { db: Arc::new(Mutex::new(database)), generation: Arc::new(AtomicU64::new(9)), editor_revision: AtomicU64::new(0), latest_sequence: Arc::new(AtomicU64::new(0)), active: AtomicBool::new(true), ready: AtomicBool::new(true), sender, receiver: Mutex::new(receiver), wake: Arc::new(|| {}), shutdown_gate: Mutex::new(()), shutdown_complete: AtomicBool::new(false), status: Mutex::new((None, false)), shutdown_error: Mutex::new(None) };
        let foreground = Some(crate::mouse_gestures::service::GestureForeground { hwnd: 1, process_id: 2, thread_id: 3 });
        let initial = owner.stroke_generation();
        assert_eq!(owner.release(initial, Some(0), foreground, Some(0), "L", DirMode::Four, 0, binding.to_action("left")), Accepted);
        let queued = owner.take().pop().unwrap();
        owner.close_admission();
        owner.db.lock().unwrap().gestures[0].enabled = false;
        owner.open_admission();
        assert!(!owner.is_current(&queued));
        let disabled_generation = owner.stroke_generation();
        assert_eq!(owner.release(initial, Some(0), foreground, Some(0), "L", DirMode::Four, 0, binding.to_action("left")), Ignore);
        assert_eq!(owner.release(disabled_generation, Some(0), foreground, Some(0), "L", DirMode::Four, 0, binding.to_action("left")), Ignore);
        assert_eq!(owner.release(disabled_generation, Some(0), foreground, Some(0), "R", DirMode::Four, 0, binding.to_action("right")), Accepted);
        owner.take();
        owner.close_admission();
        owner.db.lock().unwrap().gestures[0].enabled = true;
        owner.open_admission();
        assert_eq!(owner.release(disabled_generation, Some(0), foreground, Some(0), "L", DirMode::Four, 0, binding.to_action("left")), Ignore);
        assert_eq!(owner.release(owner.stroke_generation(), Some(0), foreground, Some(0), "L", DirMode::Four, 0, binding.to_action("left")), Accepted);
    }

    #[test]
    fn gpui_reload_fence_and_duplicate_mode_admission() {
        use crate::mouse_gestures::service::{GestureReleaseSink, GestureReleaseDisposition::{Accepted, Ignore, PassThrough}};
        let mut database = db("Zebra");
        let binding = BindingEntry { label: "query".into(), kind: BindingKind::SetQuery, action: "hello".into(), args: None, enabled: true };
        database.gestures[0].dir_mode = DirMode::Eight;
        database.gestures[0].tokens = "4".into();
        database.gestures[0].bindings.push(binding.clone());
        assert!(!gpui_binding_available(&database, DirMode::Four));
        assert!(gpui_binding_available(&database, DirMode::Eight));
        let (sender, receiver) = std::sync::mpsc::sync_channel(2);
        let owner = GpuiGestureOwner { db: Arc::new(Mutex::new(database)), generation: Arc::new(AtomicU64::new(9)), editor_revision: AtomicU64::new(0), latest_sequence: Arc::new(AtomicU64::new(0)), active: AtomicBool::new(true), ready: AtomicBool::new(true), sender, receiver: Mutex::new(receiver), wake: Arc::new(|| {}), shutdown_gate: Mutex::new(()), shutdown_complete: AtomicBool::new(false), status: Mutex::new((None, false)), shutdown_error: Mutex::new(None) };
        let foreground = Some(crate::mouse_gestures::service::GestureForeground { hwnd: 1, process_id: 2, thread_id: 3 });
        owner.close_admission();
        let interim = owner.stroke_generation();
        assert_eq!(owner.release(interim, Some(0), foreground, Some(0), "4", DirMode::Eight, 0, binding.to_action("Zebra")), Ignore);
        owner.open_admission();
        assert_eq!(owner.release(interim, Some(0), foreground, Some(0), "4", DirMode::Eight, 0, binding.to_action("Zebra")), Ignore);
        let current = owner.stroke_generation();
        assert_eq!(owner.release(current, Some(0), foreground, Some(0), "4", DirMode::Eight, 0, binding.to_action("Zebra")), Accepted);
        let mut database = owner.db.lock().unwrap();
        let mut duplicate = database.gestures[0].clone(); duplicate.label = "Alpha".into();
        database.gestures.push(duplicate);
        assert!(!gpui_binding_available(&database, DirMode::Eight));
        drop(database);
        assert_eq!(owner.release(current, Some(0), foreground, Some(0), "4", DirMode::Eight, 0, binding.to_action("Zebra")), PassThrough);
        let mut database = owner.db.lock().unwrap();
        database.gestures[0].bindings.clear();
        assert!(!gpui_binding_available(&database, DirMode::Eight));
    }

    #[test]
    fn gpui_capture_requires_usable_tokens_and_query() {
        let mut database = db("blank");
        database.gestures[0].tokens.clear();
        database.gestures[0].bindings.push(BindingEntry { label: "query".into(), kind: BindingKind::SetQuery, action: "hello".into(), args: None, enabled: true });
        assert!(!gpui_binding_available(&database, DirMode::Four));
        database.gestures[0].tokens = "L".into();
        database.gestures[0].bindings[0].action = "\n".into();
        assert!(!gpui_binding_available(&database, DirMode::Four));
        database.gestures[0].bindings[0].action = "hello".into();
        assert!(gpui_binding_available(&database, DirMode::Four));
    }

    #[test]
    fn gpui_watched_reload_stays_fenced_until_valid_database() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("mouse_gestures.json");
        let initial = db("initial");
        let shared = Arc::new(Mutex::new(initial.clone()));
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        let owner = GpuiGestureOwner { db: shared.clone(), generation: Arc::new(AtomicU64::new(1)), editor_revision: AtomicU64::new(0), latest_sequence: Arc::new(AtomicU64::new(0)), active: AtomicBool::new(true), ready: AtomicBool::new(true), sender, receiver: Mutex::new(receiver), wake: Arc::new(|| {}), shutdown_gate: Mutex::new(()), shutdown_complete: AtomicBool::new(false), status: Mutex::new((None, false)), shutdown_error: Mutex::new(None) };
        std::fs::write(&path, b"{broken").unwrap();
        assert!(reload_gpui_database(&path, &shared, &owner).is_err());
        assert!(!owner.ready.load(Ordering::Acquire));
        assert_eq!(*shared.lock().unwrap(), initial);
        assert!(owner.take_status().is_some());
        let recovered = db("recovered");
        crate::common::persistence::save_json_atomic(&path, &recovered).unwrap();
        assert!(reload_gpui_database(&path, &shared, &owner).unwrap());
        assert!(owner.ready.load(Ordering::Acquire));
        assert_eq!(*shared.lock().unwrap(), recovered);
    }

    #[test]
    fn mouse_gesture_timing_defaults_and_round_trip() {
        let old_settings: MouseGestureSettings = serde_json::from_value(serde_json::json!({
            "enabled": true,
            "show_trail": true,
        }))
        .unwrap();

        assert_eq!(old_settings.trail_interval_ms, 16);
        assert_eq!(old_settings.recognition_interval_ms, 40);

        let default_config = MouseGestureConfig::default();
        assert_eq!(default_config.trail_interval_ms, 16);
        assert_eq!(default_config.recognition_interval_ms, 40);

        let mut custom_settings = old_settings;
        custom_settings.trail_interval_ms = 8;
        custom_settings.recognition_interval_ms = 20;
        let round_trip: MouseGestureSettings =
            serde_json::from_value(serde_json::to_value(&custom_settings).unwrap()).unwrap();

        assert_eq!(round_trip, custom_settings);
        assert_eq!(round_trip.trail_interval_ms, 8);
        assert_eq!(round_trip.recognition_interval_ms, 20);
    }

    #[test]
    fn mouse_gesture_timing_runtime_mapping() {
        let mut settings = MouseGestureSettings::default();
        settings.trail_interval_ms = 8;
        settings.recognition_interval_ms = 20;

        let mapped = settings_to_config(&settings, true);
        assert!(mapped.enabled);
        assert_eq!(mapped.trail_interval_ms, 8);
        assert_eq!(mapped.recognition_interval_ms, 20);

        settings.trail_interval_ms = 40;
        settings.recognition_interval_ms = 8;
        let reversed = settings_to_config(&settings, true);
        assert_eq!(reversed.trail_interval_ms, 40);
        assert_eq!(reversed.recognition_interval_ms, 8);

        assert!(!settings_to_config(&settings, false).enabled);
        settings.enabled = false;
        assert!(!settings_to_config(&settings, true).enabled);
    }

    #[test]
    fn runtime_reload_retains_last_good_recovers_and_deduplicates() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("mouse_gestures.json");
        let initial = db("initial");
        let shared = Arc::new(Mutex::new(initial.clone()));

        std::fs::write(&path, b"{broken").unwrap();
        assert!(reload_shared_gesture_db(&path, &shared).is_err());
        assert_eq!(*shared.lock().unwrap(), initial);

        std::fs::remove_file(&path).unwrap();
        assert!(reload_shared_gesture_db(&path, &shared).is_err());
        assert_eq!(*shared.lock().unwrap(), initial);

        crate::common::persistence::save_json_atomic(&path, &initial).unwrap();
        assert!(!reload_shared_gesture_db(&path, &shared).unwrap());

        let recovered = db("recovered");
        crate::common::persistence::save_json_atomic(&path, &recovered).unwrap();
        assert!(reload_shared_gesture_db(&path, &shared).unwrap());
        assert_eq!(*shared.lock().unwrap(), recovered);
    }

    #[test]
    fn hotkey_directory_publication_pairs_revision_with_database_reload() {
        fn db_with_binding(label: &str) -> GestureDb {
            let mut database = db(label);
            database.gestures[0].bindings.push(BindingEntry {
                label: "Harmless test binding".into(),
                kind: BindingKind::Execute,
                action: "test:inert-binding".into(),
                args: None,
                enabled: true,
            });
            database
        }

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("mouse_gestures.json");
        let shared = Arc::new(Mutex::new(db_with_binding("before")));
        let mut runtime = MouseGestureRuntime {
            settings: MouseGestureSettings::default(),
            plugin_enabled: true,
            db: Arc::clone(&shared),
            directory_revision: 7,
            watcher: None,
            gpui: None,
            watcher_active: Arc::new(AtomicBool::new(true)),
            watcher_gate: Arc::new(Mutex::new(())),
            path: std::path::PathBuf::from(GESTURES_FILE),
        };
        let before = hotkey_directory_source_from_runtime(&runtime);
        assert_eq!(before.revision, 7);
        assert!(before.candidates[0].id.as_str().contains(":7:"));
        assert_eq!(before.candidates[0].action_label, "before — binding 1");
        let before_database = shared.lock().unwrap().clone();
        assert!(runtime.hotkey_directory_binding_matches_committed(
            before.revision,
            &before.candidates[0].id,
            0,
            0,
            &before_database,
        ));

        crate::common::persistence::save_json_atomic(&path, &db_with_binding("after")).unwrap();
        assert!(reload_gesture_runtime_database(&mut runtime, &path, &shared).unwrap());
        let after = hotkey_directory_source_from_runtime(&runtime);
        assert_eq!(after.revision, 8);
        assert!(after.candidates[0].id.as_str().contains(":8:"));
        assert_eq!(after.candidates[0].action_label, "after — binding 1");
        let after_database = shared.lock().unwrap().clone();
        assert!(runtime.hotkey_directory_binding_matches_committed(
            after.revision,
            &after.candidates[0].id,
            0,
            0,
            &after_database,
        ));
        assert!(
            !runtime.hotkey_directory_binding_matches_committed(
                after.revision,
                &after.candidates[0].id,
                0,
                0,
                &before_database,
            ),
            "a matching live revision cannot validate a stale committed gesture row"
        );

        // A duplicate file notification is a no-op and cannot diverge the
        // immutable source from its owner revision.
        assert!(!reload_gesture_runtime_database(&mut runtime, &path, &shared).unwrap());
        let duplicate = hotkey_directory_source_from_runtime(&runtime);
        assert_eq!(duplicate.revision, after.revision);
        assert_eq!(duplicate.candidates, after.candidates);
    }

    #[test]
    fn utility_size_inventory_plugin_picker_shares_parent_context_without_locking_contents() {
        use crate::gui::utility_dialog_size::{UtilityDialogSizeManager, UtilityWindowSizeRecipe};
        use crate::settings::Settings;
        use crate::settings_save::SettingsSaveWorkerRegistry;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let settings = Settings::default();
        settings.save(path.to_str().unwrap()).unwrap();
        let workers = SettingsSaveWorkerRegistry::default();
        let ctx = egui::Context::default();
        let manager = UtilityDialogSizeManager::new(
            path.to_string_lossy().into_owned(),
            workers.clone(),
            settings.utility_dialog_sizes.clone(),
        );
        manager.install_in_context(&ctx);

        let mut plugin = MouseGesturesPlugin {
            window_picker_open: true,
            window_picker_titles: vec!["Inert fixture window".to_owned()],
            ..Default::default()
        };
        let mut plugin_value = serde_json::to_value(MouseGestureSettings::default()).unwrap();
        let mut parent_open = true;
        let parent_id = egui::Id::new("Settings");
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000.0, 800.0),
            )),
            ..Default::default()
        };
        ctx.run_ui(input, |root| {
            let ctx = root.ctx().clone();
            manager.begin_frame(&ctx).unwrap();
            egui::Window::new("Settings").id(parent_id).show_utility(
                &ctx,
                "settings",
                parent_id,
                Some(&mut parent_open),
                UtilityWindowSizeRecipe::intrinsic(),
                |ui| {
                    <MouseGesturesPlugin as Plugin>::settings_ui(
                        &mut plugin,
                        ui,
                        &mut plugin_value,
                    );
                },
            );
        })
        .drop_without_applying_deltas();

        let tracked = manager.tracked_windows();
        assert!(tracked.iter().any(|(_, key)| key.as_str() == "settings"));
        assert!(
            tracked
                .iter()
                .any(|(_, key)| key.as_str() == "gesture-ignore-window-picker"),
            "nested plugin picker did not use the installed context capability: {tracked:?}"
        );
        workers.join_all();
    }
}
