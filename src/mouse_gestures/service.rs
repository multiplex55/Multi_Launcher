use crate::mouse_gestures::db::{
    format_gesture_label, load_gestures, GestureCandidate, GestureMatchType, SharedGestureDb,
    GESTURES_FILE,
};
use crate::mouse_gestures::engine::{token_from_delta, DirMode, GestureTracker};
use crate::mouse_gestures::overlay::{
    DefaultOverlayBackend, HintOverlay, OverlayBackend, TrailOverlay,
};
use crate::mouse_gestures::usage::{record_usage, GestureUsageEntry, GESTURES_USAGE_FILE};
use anyhow::anyhow;
use chrono::Local;
use once_cell::sync::OnceCell;
use std::collections::HashMap;
#[cfg(windows)]
use std::sync::atomic::AtomicBool;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq)]
pub struct MouseGestureConfig {
    pub enabled: bool,
    pub debug_logging: bool,
    pub trail_interval_ms: u64,
    pub recognition_interval_ms: u64,
    pub deadzone_px: f32,
    pub trail_start_move_px: f32,
    pub show_trail: bool,
    pub trail_color: [u8; 4],
    pub trail_width: f32,
    pub show_hint: bool,
    pub hint_offset: (f32, f32),
    pub dir_mode: DirMode,
    pub threshold_px: f32,
    pub long_threshold_x: f32,
    pub long_threshold_y: f32,
    pub max_tokens: usize,
    pub cancel_behavior: CancelBehavior,
    pub no_match_behavior: NoMatchBehavior,
    pub wheel_cycle_gate: WheelCycleGate,
    pub practice_mode: bool,
    pub ignore_window_titles: Vec<String>,
}

impl Default for MouseGestureConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            debug_logging: false,
            trail_interval_ms: 16,
            recognition_interval_ms: 40,
            deadzone_px: 12.0,
            trail_start_move_px: 8.0,
            show_trail: true,
            trail_color: [0xff, 0x00, 0x00, 0xff],
            trail_width: 2.0,
            show_hint: true,
            hint_offset: (16.0, 16.0),
            dir_mode: DirMode::Four,
            threshold_px: 8.0,
            long_threshold_x: 30.0,
            long_threshold_y: 30.0,
            max_tokens: 10,
            cancel_behavior: CancelBehavior::DoNothing,
            no_match_behavior: NoMatchBehavior::PassThroughClick,
            wheel_cycle_gate: WheelCycleGate::Deadzone,
            practice_mode: false,
            ignore_window_titles: Vec::new(),
        }
    }
}

use crate::gui::{send_event, WatchEvent};

pub fn should_ignore_window_title(ignore: &[String], title: &str) -> bool {
    if ignore.is_empty() {
        return false;
    }
    let normalized_title = title.trim().to_lowercase();
    if normalized_title.is_empty() {
        return false;
    }
    ignore.iter().any(|entry| {
        let normalized_entry = entry.trim().to_lowercase();
        !normalized_entry.is_empty() && normalized_title.contains(&normalized_entry)
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancelBehavior {
    DoNothing,
    PassThroughClick,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoMatchBehavior {
    DoNothing,
    PassThroughClick,
    ShowNoMatchHint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WheelCycleGate {
    Deadzone,
    Shift,
}

impl WheelCycleGate {
    fn as_usize(self) -> usize {
        match self {
            WheelCycleGate::Deadzone => 0,
            WheelCycleGate::Shift => 1,
        }
    }

    fn from_usize(value: usize) -> Self {
        match value {
            1 => WheelCycleGate::Shift,
            _ => WheelCycleGate::Deadzone,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum HookEvent {
    RButtonDown,
    RButtonUp,
    CycleNext,
    CyclePrev,
    SelectBinding(usize),
    Cancel,
}

pub trait HookBackend: Send {
    fn install(&mut self, sender: Sender<HookEvent>) -> anyhow::Result<()>;
    fn uninstall(&mut self) -> anyhow::Result<()>;
    fn is_installed(&self) -> bool;
}

pub trait OverlayFactory: Send + Sync {
    fn trail_backend(&self) -> Box<dyn OverlayBackend>;
    fn hint_backend(&self) -> Box<dyn OverlayBackend>;
}

#[derive(Debug)]
struct DefaultOverlayFactory;

impl OverlayFactory for DefaultOverlayFactory {
    fn trail_backend(&self) -> Box<dyn OverlayBackend> {
        Box::new(DefaultOverlayBackend::default())
    }

    fn hint_backend(&self) -> Box<dyn OverlayBackend> {
        Box::new(DefaultOverlayBackend::default())
    }
}

pub trait RightClickBackend: Send + Sync {
    fn send_right_click(&self);
}

#[derive(Debug)]
struct DefaultRightClickBackend;

impl RightClickBackend for DefaultRightClickBackend {
    fn send_right_click(&self) {
        send_right_click();
    }
}

pub trait CursorPositionProvider: Send + Sync {
    fn cursor_position(&self) -> Option<(f32, f32)>;
}

#[derive(Debug)]
struct DefaultCursorPositionProvider;

impl CursorPositionProvider for DefaultCursorPositionProvider {
    fn cursor_position(&self) -> Option<(f32, f32)> {
        get_cursor_position()
    }
}

#[derive(Debug)]
struct WorkerHandle {
    stop_tx: Sender<()>,
    join: JoinHandle<()>,
}

pub struct MouseGestureService {
    config: MouseGestureConfig,
    db: Option<SharedGestureDb>,
    backend: Box<dyn HookBackend>,
    overlay_factory: Arc<dyn OverlayFactory>,
    right_click_backend: Arc<dyn RightClickBackend>,
    cursor_provider: Arc<dyn CursorPositionProvider>,
    worker: Option<WorkerHandle>,
}

impl Default for MouseGestureService {
    fn default() -> Self {
        Self::new_with_backend(Box::new(DefaultHookBackend::default()))
    }
}

impl MouseGestureService {
    pub fn new_with_backend(backend: Box<dyn HookBackend>) -> Self {
        Self::new_with_backend_and_overlays(
            backend,
            Arc::new(DefaultOverlayFactory),
            Arc::new(DefaultRightClickBackend),
            Arc::new(DefaultCursorPositionProvider),
        )
    }

    pub fn new_with_backend_and_overlays(
        backend: Box<dyn HookBackend>,
        overlay_factory: Arc<dyn OverlayFactory>,
        right_click_backend: Arc<dyn RightClickBackend>,
        cursor_provider: Arc<dyn CursorPositionProvider>,
    ) -> Self {
        let db = load_gestures(GESTURES_FILE)
            .map(|db| Arc::new(Mutex::new(db)))
            .ok();
        Self {
            config: MouseGestureConfig::default(),
            db,
            backend,
            overlay_factory,
            right_click_backend,
            cursor_provider,
            worker: None,
        }
    }

    pub fn start(&mut self) {
        self.config.enabled = true;
        self.start_running();
    }

    pub fn stop(&mut self) {
        self.config.enabled = false;
        self.stop_running();
    }

    pub fn update_config(&mut self, config: MouseGestureConfig) {
        if self.config == config {
            return;
        }

        let enabled = config.enabled;
        let should_restart = self.worker.is_some();
        self.config = config;
        #[cfg(windows)]
        hook_dispatch().set_ignore_window_titles(self.config.ignore_window_titles.clone());

        if enabled {
            if should_restart {
                self.stop_running();
            }
            self.start_running();
        } else {
            self.stop_running();
        }
    }

    pub fn update_db(&mut self, db: Option<SharedGestureDb>) {
        self.db = db;
        // If the worker is already running, it captured the old Option<db> by value.
        // Restart so the worker sees the new DB.
        if self.worker.is_some() {
            self.stop_running();
            if self.config.enabled {
                self.start_running();
            }
        }
    }

    pub fn is_running(&self) -> bool {
        self.worker.is_some()
    }

    fn start_running(&mut self) {
        if self.worker.is_some() || !self.config.enabled {
            return;
        }

        #[cfg(windows)]
        {
            hook_dispatch().set_wheel_gate(self.config.wheel_cycle_gate);
            hook_dispatch().set_ignore_window_titles(self.config.ignore_window_titles.clone());
        }

        let (event_tx, event_rx) = mpsc::channel();
        let (stop_tx, stop_rx) = mpsc::channel();

        if let Err(err) = self.backend.install(event_tx) {
            tracing::error!(?err, "failed to install mouse hook");
            return;
        }

        let config = self.config.clone();
        let db = self.db.clone();
        let overlay_factory = Arc::clone(&self.overlay_factory);
        let right_click_backend = Arc::clone(&self.right_click_backend);
        let cursor_provider = Arc::clone(&self.cursor_provider);
        let join = thread::spawn(move || {
            worker_loop(
                config,
                db,
                event_rx,
                stop_rx,
                overlay_factory,
                right_click_backend,
                cursor_provider,
            )
        });
        self.worker = Some(WorkerHandle { stop_tx, join });
    }

    fn stop_running(&mut self) {
        if self.worker.is_none() && !self.backend.is_installed() {
            return;
        }

        if let Err(err) = self.backend.uninstall() {
            tracing::error!(?err, "failed to uninstall mouse hook");
        }

        if let Some(worker) = self.worker.take() {
            let _ = worker.stop_tx.send(());
            let _ = worker.join.join();
        }
    }
}

static SERVICE: OnceCell<Mutex<MouseGestureService>> = OnceCell::new();

pub fn with_service<F>(f: F)
where
    F: FnOnce(&mut MouseGestureService),
{
    let service = SERVICE.get_or_init(|| Mutex::new(MouseGestureService::default()));
    match service.lock() {
        Ok(mut guard) => f(&mut guard),
        Err(err) => tracing::error!(?err, "failed to lock mouse gesture service"),
    }
}

fn worker_loop(
    config: MouseGestureConfig,
    db: Option<SharedGestureDb>,
    event_rx: Receiver<HookEvent>,
    stop_rx: Receiver<()>,
    overlay_factory: Arc<dyn OverlayFactory>,
    right_click_backend: Arc<dyn RightClickBackend>,
    cursor_provider: Arc<dyn CursorPositionProvider>,
) {
    let mut tracker = GestureTracker::new(
        config.dir_mode,
        config.threshold_px,
        config.long_threshold_x,
        config.long_threshold_y,
        config.max_tokens,
    );
    let mut trail_overlay = TrailOverlay::new(
        overlay_factory.trail_backend(),
        config.show_trail,
        config.trail_color,
        config.trail_width,
        config.trail_start_move_px,
    );
    let mut hint_overlay = HintOverlay::new(
        overlay_factory.hint_backend(),
        config.show_hint,
        config.hint_offset,
    );
    let poll_interval = Duration::from_millis(config.trail_interval_ms.max(1));
    let recognition_interval = Duration::from_millis(config.recognition_interval_ms.max(1));
    let mut active = false;
    let mut exceeded_deadzone = false;
    let mut start_pos = (0.0_f32, 0.0_f32);
    let mut last_trail = Instant::now();
    let mut last_recognition = Instant::now();
    let mut start_time = Instant::now();
    let mut cheat_sheet_visible = false;
    let mut pending_selection_idx: Option<usize> = None;
    let mut selected_binding_idx: usize = 0;
    let mut cached_tokens = String::new();
    let mut cached_actions_tokens = String::new();
    let mut cached_actions: Vec<crate::actions::Action> = Vec::new();
    let mut cached_candidates: Vec<GestureCandidate> = Vec::new();
    let mut selection_state = load_selection_state(GESTURES_STATE_FILE);
    let mut exact_selection_key: Option<String> = None;
    let mut exact_binding_count: usize = 0;

    loop {
        #[cfg(windows)]
        {
            use windows::Win32::UI::WindowsAndMessaging::{
                DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
            };

            let mut msg = MSG::default();
            while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() } {
                unsafe {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }
        if stop_rx.try_recv().is_ok() {
            break;
        }
        match event_rx.recv_timeout(poll_interval) {
            Ok(event) => match event {
                HookEvent::RButtonDown => {
                    active = true;
                    exceeded_deadzone = false;
                    tracker.reset();
                    #[cfg(windows)]
                    hook_dispatch().set_tracking(false);
                    selected_binding_idx = 0;
                    pending_selection_idx = None;
                    cached_tokens.clear();
                    cached_actions_tokens.clear();
                    cached_actions.clear();
                    cached_candidates.clear();
                    exact_selection_key = None;
                    exact_binding_count = 0;
                    cheat_sheet_visible = false;
                    start_time = Instant::now();
                    let pos = cursor_provider.cursor_position().unwrap_or(start_pos);
                    start_pos = pos;
                    let ms = start_time.elapsed().as_millis() as u64;
                    tracker.feed_point(pos, ms);
                    trail_overlay.reset(pos);
                    hint_overlay.reset();
                    last_trail = Instant::now();
                    last_recognition = last_trail;
                    #[cfg(windows)]
                    hook_dispatch().set_active(true);
                }

                HookEvent::RButtonUp => {
                    if active {
                        // Sample cursor pos once on release so we tokenize the final motion.
                        let cursor_pos = cursor_provider.cursor_position().unwrap_or(start_pos);

                        // Always feed the final point so quick gestures still tokenize.
                        let ms = start_time.elapsed().as_millis() as u64;
                        let _ = tracker.feed_point(cursor_pos, ms);

                        let mut tokens = tracker.tokens_string();
                        if tokens.is_empty() {
                            let dx = cursor_pos.0 - start_pos.0;
                            let dy = cursor_pos.1 - start_pos.1;
                            if dx * dx + dy * dy >= config.threshold_px * config.threshold_px {
                                if let Some(token) = token_from_delta(dx, dy, config.dir_mode) {
                                    tokens = token.to_string();
                                }
                            }
                        }
                        if config.debug_logging {
                            tracing::debug!(tokens = %tokens, "mouse gesture tokens");
                        }

                        // If we produced any tokens, treat it as a gesture (swallow right click).
                        if !tokens.is_empty() {
                            // Execute the currently selected binding (wheel-cycled) if there are multiple.
                            if let Some((gesture_label, actions)) =
                                match_binding_actions(&db, &tokens, config.dir_mode)
                            {
                                if !actions.is_empty() {
                                    let idx = selected_binding_idx % actions.len();
                                    let key = selection_key(&gesture_label, &tokens);
                                    if selection_state
                                        .selections
                                        .get(&key)
                                        .copied()
                                        .unwrap_or(usize::MAX)
                                        != idx
                                    {
                                        selection_state.selections.insert(key, idx);
                                        save_selection_state(GESTURES_STATE_FILE, &selection_state);
                                    }
                                    if let Some(action) = actions.get(idx).cloned() {
                                        record_usage(
                                            GESTURES_USAGE_FILE,
                                            GestureUsageEntry {
                                                timestamp: Local::now().timestamp(),
                                                gesture_label: gesture_label.clone(),
                                                tokens: tokens.clone(),
                                                dir_mode: config.dir_mode,
                                                binding_idx: idx,
                                            },
                                        );
                                        if config.practice_mode {
                                            tracing::info!(
                                                tokens = %tokens,
                                                action = %action.action,
                                                "mouse gesture practice match"
                                            );
                                        } else {
                                            send_event(WatchEvent::ExecuteAction(action));
                                        }
                                    }
                                }
                            } else {
                                if config.practice_mode {
                                    let suggestion = cached_candidates
                                        .first()
                                        .map(|candidate| candidate.gesture_label.as_str())
                                        .unwrap_or("none");
                                    tracing::info!(
                                        tokens = %tokens,
                                        suggestion = suggestion,
                                        "mouse gesture practice miss"
                                    );
                                }
                                match config.no_match_behavior {
                                    NoMatchBehavior::DoNothing => {}
                                    NoMatchBehavior::PassThroughClick => {
                                        right_click_backend.send_right_click();
                                    }
                                    NoMatchBehavior::ShowNoMatchHint => {
                                        hint_overlay.update("No match", cursor_pos);
                                    }
                                }
                            }
                        } else {
                            // No tokens => normal right click
                            right_click_backend.send_right_click();
                        }

                        // Always clear visuals on release
                        trail_overlay.clear();
                        hint_overlay.reset();

                        // Reset state
                        active = false;
                        exceeded_deadzone = false;
                        tracker.reset();
                        selected_binding_idx = 0;
                        pending_selection_idx = None;
                        cached_tokens.clear();
                        cached_actions_tokens.clear();
                        cached_actions.clear();
                        cached_candidates.clear();
                        exact_selection_key = None;
                        exact_binding_count = 0;
                        cheat_sheet_visible = false;
                        #[cfg(windows)]
                        hook_dispatch().set_active(false);
                    }
                }

                HookEvent::SelectBinding(idx) => {
                    if active {
                        pending_selection_idx = Some(idx);
                        let binding_len = if exact_binding_count > 0 {
                            exact_binding_count
                        } else {
                            cached_actions.len()
                        };
                        if binding_len > 0 {
                            selected_binding_idx = idx.min(binding_len.saturating_sub(1));
                            pending_selection_idx = None;

                            if let Some(key) = exact_selection_key.as_ref() {
                                if exact_binding_count > 0 {
                                    let stored_idx = selected_binding_idx % exact_binding_count;
                                    if selection_state
                                        .selections
                                        .get(key)
                                        .copied()
                                        .unwrap_or(usize::MAX)
                                        != stored_idx
                                    {
                                        selection_state.selections.insert(key.clone(), stored_idx);
                                        save_selection_state(GESTURES_STATE_FILE, &selection_state);
                                    }
                                }
                            }

                            if let Some(pos) = cursor_provider.cursor_position() {
                                if let Some(text) = format_hint_text(
                                    &cached_tokens,
                                    &cached_candidates,
                                    selected_binding_idx,
                                    config.no_match_behavior,
                                    config.wheel_cycle_gate,
                                ) {
                                    hint_overlay.update(&text, pos);
                                }
                            }
                        }
                    }
                }
                HookEvent::CycleNext | HookEvent::CyclePrev => {
                    let allow_cycle = match config.wheel_cycle_gate {
                        WheelCycleGate::Deadzone => exceeded_deadzone,
                        WheelCycleGate::Shift => true,
                    };
                    if active && allow_cycle && !cached_actions.is_empty() {
                        let len = cached_actions.len();
                        match event {
                            HookEvent::CycleNext if len > 1 => {
                                selected_binding_idx = (selected_binding_idx + 1) % len;
                            }
                            HookEvent::CyclePrev if len > 1 => {
                                selected_binding_idx = (selected_binding_idx + len - 1) % len;
                            }
                            _ => {}
                        }

                        if let Some(key) = exact_selection_key.as_ref() {
                            if exact_binding_count > 0 {
                                let stored_idx = selected_binding_idx % exact_binding_count;
                                if selection_state
                                    .selections
                                    .get(key)
                                    .copied()
                                    .unwrap_or(usize::MAX)
                                    != stored_idx
                                {
                                    selection_state.selections.insert(key.clone(), stored_idx);
                                    save_selection_state(GESTURES_STATE_FILE, &selection_state);
                                }
                            }
                        }

                        if let Some(pos) = cursor_provider.cursor_position() {
                            if let Some(text) = format_hint_text(
                                &cached_tokens,
                                &cached_candidates,
                                selected_binding_idx,
                                config.no_match_behavior,
                                config.wheel_cycle_gate,
                            ) {
                                hint_overlay.update(&text, pos);
                            }
                        }
                    }
                }

                HookEvent::Cancel => {
                    if active {
                        if config.cancel_behavior == CancelBehavior::PassThroughClick {
                            right_click_backend.send_right_click();
                        }
                        trail_overlay.clear();
                        hint_overlay.reset();
                        active = false;
                        exceeded_deadzone = false;
                        tracker.reset();
                        selected_binding_idx = 0;
                        pending_selection_idx = None;
                        cached_tokens.clear();
                        cached_actions_tokens.clear();
                        cached_actions.clear();
                        cached_candidates.clear();
                        exact_selection_key = None;
                        exact_binding_count = 0;
                        cheat_sheet_visible = false;
                        #[cfg(windows)]
                        {
                            hook_dispatch().set_tracking(false);
                            hook_dispatch().set_active(false);
                        }
                    }
                }
            },
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }

        if active && last_trail.elapsed() >= poll_interval {
            if let Some(pos) = cursor_provider.cursor_position() {
                let dx = pos.0 - start_pos.0;
                let dy = pos.1 - start_pos.1;
                let dist_sq = dx * dx + dy * dy;
                if !exceeded_deadzone && dist_sq >= config.deadzone_px * config.deadzone_px {
                    exceeded_deadzone = true;
                    #[cfg(windows)]
                    hook_dispatch().set_tracking(true);
                    if cheat_sheet_visible {
                        hint_overlay.update("", pos);
                        cheat_sheet_visible = false;
                    }
                }

                trail_overlay.update_position(pos);

                if !exceeded_deadzone
                    && cached_tokens.is_empty()
                    && !cheat_sheet_visible
                    && start_time.elapsed() >= CHEATSHEET_DELAY
                {
                    if let Some(text) = format_cheatsheet_text(&db, CHEATSHEET_MAX_GESTURES) {
                        hint_overlay.update(&text, pos);
                        cheat_sheet_visible = true;
                    }
                }

                if last_recognition.elapsed() >= recognition_interval {
                    let ms = start_time.elapsed().as_millis() as u64;
                    let _ = tracker.feed_point(pos, ms);
                    let mut tokens = tracker.tokens_string();
                    if tokens.is_empty() {
                        let dx = pos.0 - start_pos.0;
                        let dy = pos.1 - start_pos.1;
                        if dx * dx + dy * dy >= config.threshold_px * config.threshold_px {
                            if let Some(token) = token_from_delta(dx, dy, config.dir_mode) {
                                tokens = token.to_string();
                            }
                        }
                    }
                    if tokens != cached_tokens {
                        cached_tokens = tokens.to_string();
                        selected_binding_idx = 0;
                        pending_selection_idx = None;
                        cached_candidates =
                            candidate_matches(&db, &tokens, config.dir_mode, MAX_HINT_CANDIDATES);
                        if let Some(candidate) = cached_candidates
                            .iter()
                            .find(|candidate| candidate.match_type == GestureMatchType::Exact)
                        {
                            cached_actions_tokens = candidate.tokens.clone();
                        } else {
                            cached_actions_tokens.clear();
                        }
                        if !cached_actions_tokens.is_empty() {
                            if let Some((_gesture_label, actions)) =
                                match_binding_actions(&db, &cached_actions_tokens, config.dir_mode)
                            {
                                cached_actions = actions;
                            } else {
                                cached_actions.clear();
                                cached_actions_tokens.clear();
                            }
                        } else {
                            cached_actions.clear();
                        }
                    }

                    if let Some(candidate) = cached_candidates
                        .iter()
                        .find(|candidate| candidate.match_type == GestureMatchType::Exact)
                    {
                        let new_key = selection_key(&candidate.gesture_label, &candidate.tokens);
                        if exact_selection_key.as_deref() != Some(new_key.as_str()) {
                            selected_binding_idx = selection_state
                                .selections
                                .get(&new_key)
                                .copied()
                                .unwrap_or(0);
                        }
                        exact_selection_key = Some(new_key);
                        exact_binding_count = candidate.bindings.len();
                    } else {
                        exact_selection_key = None;
                        exact_binding_count = 0;
                    }

                    if let Some(pending_idx) = pending_selection_idx.take() {
                        if !cached_actions.is_empty() {
                            let len = cached_actions.len();
                            selected_binding_idx = pending_idx.min(len.saturating_sub(1));
                            if let Some(key) = exact_selection_key.as_ref() {
                                if exact_binding_count > 0 {
                                    let stored_idx = selected_binding_idx % exact_binding_count;
                                    if selection_state
                                        .selections
                                        .get(key)
                                        .copied()
                                        .unwrap_or(usize::MAX)
                                        != stored_idx
                                    {
                                        selection_state.selections.insert(key.clone(), stored_idx);
                                        save_selection_state(GESTURES_STATE_FILE, &selection_state);
                                    }
                                }
                            }
                        }
                    }

                    if let Some(text) = format_hint_text(
                        &tokens,
                        &cached_candidates,
                        selected_binding_idx,
                        config.no_match_behavior,
                        config.wheel_cycle_gate,
                    ) {
                        hint_overlay.update(&text, pos);
                        cheat_sheet_visible = false;
                    } else if !cheat_sheet_visible {
                        hint_overlay.update("", pos);
                    }
                    last_recognition = Instant::now();
                }
            }
            last_trail = Instant::now();
        }
    }
}

#[allow(dead_code)]
fn match_binding_action(
    db: &Option<SharedGestureDb>,
    tokens: &str,
    dir_mode: DirMode,
) -> Option<crate::actions::Action> {
    let db = db.as_ref()?;
    let guard = db.lock().ok()?;
    guard
        .match_binding_owned(tokens, dir_mode)
        .map(|(label, binding)| binding.to_action(&label))
}

fn match_binding_actions(
    db: &Option<SharedGestureDb>,
    tokens: &str,
    dir_mode: DirMode,
) -> Option<(String, Vec<crate::actions::Action>)> {
    let db = db.as_ref()?;
    let guard = db.lock().ok()?;
    let (gesture_label, bindings) = guard.match_bindings_owned(tokens, dir_mode)?;
    let actions = bindings
        .iter()
        .map(|binding| binding.to_action(&gesture_label))
        .collect::<Vec<_>>();
    Some((gesture_label, actions))
}

const MAX_HINT_CANDIDATES: usize = 5;
const CHEATSHEET_MAX_GESTURES: usize = 5;
const CHEATSHEET_DELAY: Duration = Duration::from_millis(250);

#[allow(dead_code)]
fn best_match_name(
    db: &Option<SharedGestureDb>,
    tokens: &str,
    dir_mode: DirMode,
) -> Option<String> {
    let db = db.as_ref()?;
    let guard = db.lock().ok()?;
    guard
        .match_binding_owned(tokens, dir_mode)
        .map(|(label, binding)| format!("{}: {}", label, binding.label))
}

fn candidate_matches(
    db: &Option<SharedGestureDb>,
    tokens: &str,
    dir_mode: DirMode,
    limit: usize,
) -> Vec<GestureCandidate> {
    let db = match db.as_ref() {
        Some(db) => db,
        None => return Vec::new(),
    };
    let guard = match db.lock() {
        Ok(guard) => guard,
        Err(_) => return Vec::new(),
    };
    let mut candidates = guard.candidate_matches(tokens, dir_mode);
    candidates.truncate(limit);
    candidates
}

fn format_hint_text(
    tokens: &str,
    candidates: &[GestureCandidate],
    selected_binding_idx: usize,
    no_match_behavior: NoMatchBehavior,
    wheel_cycle_gate: WheelCycleGate,
) -> Option<String> {
    if tokens.is_empty() {
        return None;
    }

    let mut lines = Vec::new();
    let exact_candidate = candidates
        .iter()
        .find(|candidate| candidate.match_type == GestureMatchType::Exact);
    if let Some(candidate) = exact_candidate {
        let bindings = &candidate.bindings;
        let binding_count = bindings.len();
        let selected_idx = if binding_count == 0 {
            0
        } else {
            selected_binding_idx % binding_count
        };
        let binding_label = bindings
            .get(selected_idx)
            .map(|binding| binding.label.as_str())
            .unwrap_or("No binding");
        let mut line = format!("{tokens} — {binding_label}");
        if binding_count > 1 {
            line.push_str(&format!(" ({}/{})", selected_idx + 1, binding_count));
        }
        line.push_str(" [exact]");
        lines.push(line);

        if binding_count > 1 {
            for (idx, binding) in bindings.iter().enumerate() {
                lines.push(format!("{}) {}", idx + 1, binding.label));
            }
        }
    } else if no_match_behavior == NoMatchBehavior::ShowNoMatchHint {
        lines.push(format!("{tokens} — No match"));
    } else {
        lines.push(tokens.to_string());
    }

    if exact_candidate.is_none() {
        if let Some(candidate) = candidates.first() {
            lines.push(format!(
                "Closest: {} [{}]",
                candidate.gesture_label,
                match_type_label(candidate.match_type)
            ));
        }
    }

    let cycle_hint = match wheel_cycle_gate {
        WheelCycleGate::Deadzone => "Wheel: cycle",
        WheelCycleGate::Shift => "Shift+Wheel: cycle",
    };
    lines.insert(
        1,
        format!("{cycle_hint} • 1-9: select • Release: run • Esc: cancel"),
    );
    Some(lines.join("\n"))
}

fn match_type_label(match_type: GestureMatchType) -> &'static str {
    match match_type {
        GestureMatchType::Exact => "exact",
        GestureMatchType::Prefix => "prefix",
        GestureMatchType::Fuzzy => "fuzzy",
    }
}

fn format_cheatsheet_text(db: &Option<SharedGestureDb>, limit: usize) -> Option<String> {
    let db = db.as_ref()?;
    let guard = db.lock().ok()?;
    let mut lines = Vec::new();
    lines.push("Cheat sheet".to_string());
    let mut count = 0;
    for gesture in guard.gestures.iter().filter(|gesture| gesture.enabled) {
        lines.push(format!("• {}", format_gesture_label(gesture)));
        count += 1;
        if count >= limit {
            break;
        }
    }
    if count == 0 {
        lines.push("No gestures configured".to_string());
    }
    Some(lines.join("\n"))
}

const GESTURES_STATE_FILE: &str = "mouse_gestures_state.json";

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct GestureSelectionState {
    selections: HashMap<String, usize>,
}

fn selection_key(label: &str, tokens: &str) -> String {
    format!("{label}::{tokens}")
}

fn load_selection_state(path: &str) -> GestureSelectionState {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    if content.trim().is_empty() {
        return GestureSelectionState::default();
    }
    serde_json::from_str(&content).unwrap_or_default()
}

fn save_selection_state(path: &str, state: &GestureSelectionState) {
    match serde_json::to_string_pretty(state) {
        Ok(json) => {
            if let Err(err) = std::fs::write(path, json) {
                tracing::error!(?err, "failed to save mouse gesture selection state");
            }
        }
        Err(err) => tracing::error!(?err, "failed to serialize mouse gesture selection state"),
    }
}

#[cfg(windows)]
fn get_cursor_position() -> Option<(f32, f32)> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

    let mut point = POINT { x: 0, y: 0 };
    if unsafe { GetCursorPos(&mut point).is_ok() } {
        Some((point.x as f32, point.y as f32))
    } else {
        None
    }
}

#[cfg(not(windows))]
fn get_cursor_position() -> Option<(f32, f32)> {
    None
}

#[cfg(windows)]
fn get_foreground_window_title() -> Option<String> {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW,
    };

    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0 == 0 {
        return None;
    }
    let len = unsafe { GetWindowTextLengthW(hwnd) };
    if len == 0 {
        return None;
    }
    let mut buffer = vec![0u16; (len + 1) as usize];
    let copied = unsafe { GetWindowTextW(hwnd, &mut buffer) };
    if copied == 0 {
        return None;
    }
    buffer.truncate(copied as usize);
    String::from_utf16(&buffer).ok()
}

#[cfg(windows)]
const MG_INJECT_TAG: usize = 0x4D47_494E_4A; // "MG_INJ"

#[cfg(windows)]
fn send_right_click() {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
        MOUSEINPUT,
    };

    // Prevent the hook from consuming the injected click (and re-triggering itself)
    hook_dispatch().set_injecting(true);

    let down = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: 0,
                dwFlags: MOUSEEVENTF_RIGHTDOWN,
                time: 0,
                dwExtraInfo: MG_INJECT_TAG,
            },
        },
    };

    let up = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: 0,
                dwFlags: MOUSEEVENTF_RIGHTUP,
                time: 0,
                dwExtraInfo: MG_INJECT_TAG,
            },
        },
    };

    let _ = unsafe { SendInput(&[down, up], std::mem::size_of::<INPUT>() as i32) };

    hook_dispatch().set_injecting(false);
}

#[cfg(not(windows))]
fn send_right_click() {}

#[cfg(windows)]
struct HookThread {
    thread_id: u32,
    join: std::thread::JoinHandle<()>,
}

#[cfg(windows)]
#[derive(Default)]
pub struct DefaultHookBackend {
    hook_thread: Option<HookThread>,
}

#[cfg(windows)]
unsafe impl Send for DefaultHookBackend {}

#[cfg(windows)]
impl HookBackend for DefaultHookBackend {
    fn install(&mut self, sender: Sender<HookEvent>) -> anyhow::Result<()> {
        if self.hook_thread.is_some() {
            return Ok(());
        }

        // Put the sender where the hook proc can see it.
        hook_dispatch().set_sender(Some(sender));
        hook_dispatch().set_tracking(false);
        hook_dispatch().set_active(false);
        hook_dispatch().set_enabled(true);

        use std::time::Duration;
        use windows::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows::Win32::System::Threading::GetCurrentThreadId;
        use windows::Win32::UI::WindowsAndMessaging::{
            DispatchMessageW, GetMessageW, PeekMessageW, TranslateMessage, MSG, PM_NOREMOVE,
        };
        use windows::Win32::UI::WindowsAndMessaging::{
            SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL,
        };

        // Handshake so install() only returns once the hook thread is actually ready.
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel::<anyhow::Result<u32>>(1);

        let join = std::thread::spawn(move || {
            // Ensure the thread has a message queue.
            let mut msg = MSG::default();
            unsafe {
                let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
            }

            let thread_id = unsafe { GetCurrentThreadId() };

            let hmodule = match unsafe { GetModuleHandleW(None) } {
                Ok(h) => h,
                Err(e) => {
                    let _ = ready_tx.send(Err(anyhow!(e)));
                    return;
                }
            };

            let mouse_hook = match unsafe {
                SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook_proc), hmodule, 0)
            } {
                Ok(h) if !h.0.is_null() => h,
                Ok(_) => {
                    let _ = ready_tx.send(Err(anyhow!(windows::core::Error::from_win32())));
                    return;
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(anyhow!(e)));
                    return;
                }
            };

            let keyboard_hook = match unsafe {
                SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), hmodule, 0)
            } {
                Ok(h) if !h.0.is_null() => h,
                Ok(_) => {
                    let _ = ready_tx.send(Err(anyhow!(windows::core::Error::from_win32())));
                    unsafe {
                        let _ = UnhookWindowsHookEx(mouse_hook);
                    }
                    return;
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(anyhow!(e)));
                    unsafe {
                        let _ = UnhookWindowsHookEx(mouse_hook);
                    }
                    return;
                }
            };

            let _ = ready_tx.send(Ok(thread_id));

            // Message loop keeps WH_MOUSE_LL callbacks flowing.
            loop {
                let r = unsafe { GetMessageW(&mut msg, None, 0, 0) };
                if r.0 == 0 {
                    // WM_QUIT
                    break;
                }
                if r.0 == -1 {
                    break;
                }
                unsafe {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }

            unsafe {
                let _ = UnhookWindowsHookEx(mouse_hook);
                let _ = UnhookWindowsHookEx(keyboard_hook);
            }
        });

        let thread_id = ready_rx
            .recv_timeout(Duration::from_secs(2))
            .map_err(|_| anyhow!("hook thread did not signal readiness"))??;

        self.hook_thread = Some(HookThread { thread_id, join });
        Ok(())
    }

    fn uninstall(&mut self) -> anyhow::Result<()> {
        // Stop dispatch first to avoid any new work while shutting down.
        hook_dispatch().set_enabled(false);
        hook_dispatch().set_tracking(false);
        hook_dispatch().set_active(false);
        hook_dispatch().set_sender(None);

        if let Some(th) = self.hook_thread.take() {
            use windows::Win32::Foundation::{LPARAM, WPARAM};
            use windows::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_QUIT};
            unsafe {
                let _ = PostThreadMessageW(th.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
            }
            let _ = th.join.join();
        }

        Ok(())
    }

    fn is_installed(&self) -> bool {
        self.hook_thread.is_some()
    }
}

#[cfg(not(windows))]
#[derive(Default)]
pub struct DefaultHookBackend;

#[cfg(not(windows))]
impl HookBackend for DefaultHookBackend {
    fn install(&mut self, _sender: Sender<HookEvent>) -> anyhow::Result<()> {
        Err(anyhow!("mouse hooks are not supported on this platform"))
    }

    fn uninstall(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    fn is_installed(&self) -> bool {
        false
    }
}

#[derive(Clone)]
pub struct MockHookBackend {
    state: Arc<MockHookState>,
}

#[derive(Default)]
struct MockHookState {
    install_count: AtomicUsize,
    uninstall_count: AtomicUsize,
    sender: Mutex<Option<Sender<HookEvent>>>,
}

impl MockHookBackend {
    pub fn new() -> (Self, MockHookHandle) {
        let state = Arc::new(MockHookState::default());
        (
            Self {
                state: Arc::clone(&state),
            },
            MockHookHandle { state },
        )
    }
}

impl HookBackend for MockHookBackend {
    fn install(&mut self, sender: Sender<HookEvent>) -> anyhow::Result<()> {
        let mut guard = self.state.sender.lock().map_err(|_| anyhow!("lock"))?;
        if guard.is_none() {
            self.state.install_count.fetch_add(1, Ordering::SeqCst);
            *guard = Some(sender);
        }
        Ok(())
    }

    fn uninstall(&mut self) -> anyhow::Result<()> {
        let mut guard = self.state.sender.lock().map_err(|_| anyhow!("lock"))?;
        if guard.is_some() {
            self.state.uninstall_count.fetch_add(1, Ordering::SeqCst);
        }
        *guard = None;
        Ok(())
    }

    fn is_installed(&self) -> bool {
        match self.state.sender.lock() {
            Ok(guard) => guard.is_some(),
            Err(_) => false,
        }
    }
}

pub struct MockHookHandle {
    state: Arc<MockHookState>,
}

impl MockHookHandle {
    pub fn install_count(&self) -> usize {
        self.state.install_count.load(Ordering::SeqCst)
    }

    pub fn uninstall_count(&self) -> usize {
        self.state.uninstall_count.load(Ordering::SeqCst)
    }

    pub fn emit(&self, event: HookEvent) -> bool {
        match self.state.sender.lock() {
            Ok(guard) => guard
                .as_ref()
                .map(|sender| sender.send(event).is_ok())
                .unwrap_or(false),
            Err(_) => false,
        }
    }
}

#[cfg(windows)]
struct HookDispatch {
    enabled: AtomicBool,
    tracking: AtomicBool,
    injecting: AtomicBool,
    active: AtomicBool,
    wheel_gate: AtomicUsize,
    ignore_window_titles: Mutex<Vec<String>>,
    sender: Mutex<Option<Sender<HookEvent>>>,
}

#[cfg(windows)]
impl HookDispatch {
    fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Release);
    }

    fn set_tracking(&self, tracking: bool) {
        self.tracking.store(tracking, Ordering::Release);
    }

    fn is_tracking(&self) -> bool {
        self.tracking.load(Ordering::Acquire)
    }

    fn set_injecting(&self, injecting: bool) {
        self.injecting.store(injecting, Ordering::Release);
    }

    fn is_injecting(&self) -> bool {
        self.injecting.load(Ordering::Acquire)
    }

    fn set_active(&self, active: bool) {
        self.active.store(active, Ordering::Release);
    }

    fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }

    fn set_wheel_gate(&self, gate: WheelCycleGate) {
        self.wheel_gate.store(gate.as_usize(), Ordering::Release);
    }

    fn wheel_gate(&self) -> WheelCycleGate {
        WheelCycleGate::from_usize(self.wheel_gate.load(Ordering::Acquire))
    }

    fn set_ignore_window_titles(&self, titles: Vec<String>) {
        if let Ok(mut guard) = self.ignore_window_titles.lock() {
            *guard = titles;
        }
    }

    fn should_ignore_window_title(&self, title: &str) -> bool {
        if let Ok(guard) = self.ignore_window_titles.lock() {
            should_ignore_window_title(&guard, title)
        } else {
            false
        }
    }

    fn set_sender(&self, sender: Option<Sender<HookEvent>>) {
        if let Ok(mut guard) = self.sender.lock() {
            *guard = sender;
        }
    }
}

#[cfg(windows)]
static HOOK_DISPATCH: OnceCell<HookDispatch> = OnceCell::new();

#[cfg(windows)]
fn hook_dispatch() -> &'static HookDispatch {
    HOOK_DISPATCH.get_or_init(|| HookDispatch {
        enabled: AtomicBool::new(false),
        tracking: AtomicBool::new(false),
        injecting: AtomicBool::new(false),
        active: AtomicBool::new(false),
        wheel_gate: AtomicUsize::new(WheelCycleGate::Deadzone.as_usize()),
        ignore_window_titles: Mutex::new(Vec::new()),
        sender: Mutex::new(None),
    })
}

#[cfg(windows)]
unsafe extern "system" fn mouse_hook_proc(
    n_code: i32,
    w_param: windows::Win32::Foundation::WPARAM,
    l_param: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, HC_ACTION, WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_RBUTTONUP,
    };

    use windows::Win32::UI::WindowsAndMessaging::MSLLHOOKSTRUCT;

    if n_code == HC_ACTION as i32 {
        let msg = w_param.0 as u32;

        if msg == WM_RBUTTONDOWN || msg == WM_RBUTTONUP || msg == WM_MOUSEWHEEL {
            let dispatch = hook_dispatch();

            if dispatch.enabled.load(Ordering::Acquire) {
                // If we're injecting (or the event is injected), do NOT consume it and do NOT forward to worker.
                let info = &*(l_param.0 as *const MSLLHOOKSTRUCT);

                // Flags: 0x1 = LLMHF_INJECTED, 0x2 = LLMHF_LOWER_IL_INJECTED
                let injected_flagged = (info.flags & 0x1) != 0 || (info.flags & 0x2) != 0;
                let injected_tagged = info.dwExtraInfo == MG_INJECT_TAG;

                if dispatch.is_injecting() || injected_flagged || injected_tagged {
                    return CallNextHookEx(
                        windows::Win32::UI::WindowsAndMessaging::HHOOK(std::ptr::null_mut()),
                        n_code,
                        w_param,
                        l_param,
                    );
                }

                if let Some(title) = get_foreground_window_title() {
                    if dispatch.should_ignore_window_title(&title) {
                        return CallNextHookEx(
                            windows::Win32::UI::WindowsAndMessaging::HHOOK(std::ptr::null_mut()),
                            n_code,
                            w_param,
                            l_param,
                        );
                    }
                }

                // Only consume wheel events while a gesture is actively being tracked.
                if msg == WM_MOUSEWHEEL {
                    let allow = match dispatch.wheel_gate() {
                        WheelCycleGate::Deadzone => dispatch.is_tracking(),
                        WheelCycleGate::Shift => {
                            use windows::Win32::UI::Input::KeyboardAndMouse::{
                                GetAsyncKeyState, VK_SHIFT,
                            };
                            let shift_down = unsafe {
                                (GetAsyncKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0
                            };
                            dispatch.is_active() && shift_down
                        }
                    };
                    if !allow {
                        return CallNextHookEx(
                            windows::Win32::UI::WindowsAndMessaging::HHOOK(std::ptr::null_mut()),
                            n_code,
                            w_param,
                            l_param,
                        );
                    }
                }

                if let Ok(guard) = dispatch.sender.try_lock() {
                    if let Some(sender) = guard.as_ref() {
                        if msg == WM_RBUTTONDOWN {
                            // Wheel-cycling is only enabled after worker exceeds deadzone.
                            dispatch.set_tracking(false);
                            let _ = sender.send(HookEvent::RButtonDown);
                        } else if msg == WM_RBUTTONUP {
                            dispatch.set_tracking(false);
                            let _ = sender.send(HookEvent::RButtonUp);
                        } else if msg == WM_MOUSEWHEEL {
                            // mouseData high word contains signed wheel delta (WHEEL_DELTA multiples).
                            let delta = ((info.mouseData >> 16) & 0xFFFF) as i16;
                            if delta > 0 {
                                let _ = sender.send(HookEvent::CycleNext);
                            } else if delta < 0 {
                                let _ = sender.send(HookEvent::CyclePrev);
                            }
                        }
                    }
                }

                // Consume while MG is enabled (RMB always, wheel only when tracking).
                return windows::Win32::Foundation::LRESULT(1);
            }
        }
    }

    CallNextHookEx(
        windows::Win32::UI::WindowsAndMessaging::HHOOK(std::ptr::null_mut()),
        n_code,
        w_param,
        l_param,
    )
}

#[cfg(windows)]
unsafe extern "system" fn keyboard_hook_proc(
    n_code: i32,
    w_param: windows::Win32::Foundation::WPARAM,
    l_param: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        VK_1, VK_2, VK_3, VK_4, VK_5, VK_6, VK_7, VK_8, VK_9, VK_ESCAPE,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, HC_ACTION, KBDLLHOOKSTRUCT, KBDLLHOOKSTRUCT_FLAGS, WM_KEYDOWN,
        WM_SYSKEYDOWN,
    };

    if n_code == HC_ACTION as i32 {
        let msg = w_param.0 as u32;
        if msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN {
            let info = &*(l_param.0 as *const KBDLLHOOKSTRUCT);
            let injected = (info.flags & KBDLLHOOKSTRUCT_FLAGS(0x10)) != KBDLLHOOKSTRUCT_FLAGS(0);
            if !injected {
                let dispatch = hook_dispatch();
                if dispatch.enabled.load(Ordering::Acquire) && dispatch.is_active() {
                    if let Ok(guard) = dispatch.sender.try_lock() {
                        if let Some(sender) = guard.as_ref() {
                            if info.vkCode == VK_ESCAPE.0 as u32 {
                                let _ = sender.send(HookEvent::Cancel);
                                return windows::Win32::Foundation::LRESULT(1);
                            }
                            let selection = match info.vkCode {
                                code if code == VK_1.0 as u32 => Some(0),
                                code if code == VK_2.0 as u32 => Some(1),
                                code if code == VK_3.0 as u32 => Some(2),
                                code if code == VK_4.0 as u32 => Some(3),
                                code if code == VK_5.0 as u32 => Some(4),
                                code if code == VK_6.0 as u32 => Some(5),
                                code if code == VK_7.0 as u32 => Some(6),
                                code if code == VK_8.0 as u32 => Some(7),
                                code if code == VK_9.0 as u32 => Some(8),
                                _ => None,
                            };
                            if let Some(idx) = selection {
                                let _ = sender.send(HookEvent::SelectBinding(idx));
                                return windows::Win32::Foundation::LRESULT(1);
                            }
                        }
                    }
                }
            }
        }
    }

    CallNextHookEx(
        windows::Win32::UI::WindowsAndMessaging::HHOOK(std::ptr::null_mut()),
        n_code,
        w_param,
        l_param,
    )
}
