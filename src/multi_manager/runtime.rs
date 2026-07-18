use crate::multi_manager::activation::{
    self, ActivationDeps, ActivationOperation, ActivationResult,
};
use crate::multi_manager::model::{MmHotkey, MmRect, MmWorkspace};
use crate::multi_manager::{reconnect, win};
use crate::settings::MultiManagerSettings;
use anyhow::Result;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const DEFAULT_DEBOUNCE: Duration = Duration::from_millis(250);

pub trait WindowOps {
    fn is_window_at_rect(&self, hwnd: usize, rect: MmRect) -> bool;
    fn move_window_to_rect(&self, hwnd: usize, rect: MmRect) -> Result<()>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct WinWindowOps;

impl WindowOps for WinWindowOps {
    fn is_window_at_rect(&self, hwnd: usize, rect: MmRect) -> bool {
        win::is_window_at_rect(hwnd, rect)
    }

    fn move_window_to_rect(&self, hwnd: usize, rect: MmRect) -> Result<()> {
        win::move_window_to_rect(hwnd, rect)
    }
}

pub trait HotkeyOps {
    fn is_hotkey_pressed(&self, sequence: &str) -> bool;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct WinHotkeyOps;

impl HotkeyOps for WinHotkeyOps {
    fn is_hotkey_pressed(&self, sequence: &str) -> bool {
        win::is_hotkey_pressed(sequence)
    }
}

#[derive(Debug, Default)]
pub struct RuntimeControl {
    pub enabled: AtomicBool,
    pub capture_pending: AtomicBool,
    pub shutdown: AtomicBool,
    pub auto_reconnect_missing_windows: AtomicBool,
    pub auto_reconnect_interval_ms: AtomicU64,
}

impl RuntimeControl {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled: AtomicBool::new(enabled),
            capture_pending: AtomicBool::new(false),
            shutdown: AtomicBool::new(false),
            auto_reconnect_missing_windows: AtomicBool::new(true),
            auto_reconnect_interval_ms: AtomicU64::new(3000),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MultiManagerRuntimeEvent {
    WorkspaceActionCompleted {
        workspace_id: String,
        operation: ActivationOperation,
        result: ActivationResult,
    },
    BindingReconnected {
        workspace_id: String,
        result: ActivationResult,
    },
    MovementFailed {
        workspace_id: String,
        errors: Vec<String>,
    },
    RuntimeLockFailed {
        context: String,
    },
    EnumerationFailed {
        context: String,
        error: String,
    },
}

pub struct MultiManagerRuntime {
    pub workspaces: Arc<Mutex<Vec<MmWorkspace>>>,
    pub control: Arc<RuntimeControl>,
    pub last_hotkey_info: Arc<Mutex<Option<(String, Instant)>>>,
    pub event_queue: Arc<Mutex<VecDeque<MultiManagerRuntimeEvent>>>,
    pub join_handle: Option<JoinHandle<()>>,
}

impl MultiManagerRuntime {
    pub fn inactive(workspaces: Arc<Mutex<Vec<MmWorkspace>>>) -> Self {
        Self {
            workspaces,
            control: Arc::new(RuntimeControl::new(false)),
            last_hotkey_info: Arc::new(Mutex::new(None)),
            event_queue: Arc::new(Mutex::new(VecDeque::new())),
            join_handle: None,
        }
    }

    pub fn start(workspaces: Arc<Mutex<Vec<MmWorkspace>>>, settings: MultiManagerSettings) -> Self {
        let control = Arc::new(RuntimeControl::new(settings.enabled));
        control
            .auto_reconnect_missing_windows
            .store(settings.auto_reconnect_missing_windows, Ordering::Relaxed);
        control
            .auto_reconnect_interval_ms
            .store(settings.auto_reconnect_interval_ms, Ordering::Relaxed);
        let last_hotkey_info = Arc::new(Mutex::new(None));
        let event_queue = Arc::new(Mutex::new(VecDeque::new()));
        let thread_workspaces = Arc::clone(&workspaces);
        let thread_control = Arc::clone(&control);
        let thread_last_hotkey_info = Arc::clone(&last_hotkey_info);
        let thread_event_queue = Arc::clone(&event_queue);
        let poll = Duration::from_millis(settings.hotkey_poll_ms);
        let reconnect_interval = Duration::from_millis(settings.auto_reconnect_interval_ms);
        let join_handle = thread::spawn(move || {
            let mut debounce = HashMap::new();
            let mut last_reconnect = Instant::now() - reconnect_interval;
            let win_ops = WinWindowOps;
            let hotkey_ops = WinHotkeyOps;
            while !thread_control.shutdown.load(Ordering::Relaxed) {
                thread::sleep(poll);
                let now = Instant::now();
                maybe_runtime_reconnect(
                    &thread_workspaces,
                    &thread_control,
                    &mut last_reconnect,
                    now,
                    |hwnd| win::is_valid_window(hwnd),
                    || win::enumerate_top_level_windows().unwrap_or_default(),
                );
                if let Ok(mut workspaces) = thread_workspaces.lock() {
                    runtime_tick(
                        &mut workspaces,
                        &thread_control,
                        &thread_last_hotkey_info,
                        &thread_event_queue,
                        &mut debounce,
                        DEFAULT_DEBOUNCE,
                        &win_ops,
                        &hotkey_ops,
                        &|hwnd| win::is_valid_window(hwnd),
                        &|| win::enumerate_top_level_windows().unwrap_or_default(),
                        now,
                    );
                } else if let Ok(mut events) = thread_event_queue.lock() {
                    events.push_back(MultiManagerRuntimeEvent::RuntimeLockFailed {
                        context: "runtime tick workspace lock".to_string(),
                    });
                }
            }
        });

        Self {
            workspaces,
            control,
            last_hotkey_info,
            event_queue,
            join_handle: Some(join_handle),
        }
    }

    pub fn shutdown(&mut self) {
        self.control.shutdown.store(true, Ordering::Relaxed);
        if let Some(join_handle) = self.join_handle.take() {
            let _ = join_handle.join();
        }
    }
}

impl Drop for MultiManagerRuntime {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub fn send_workspace_home(workspace: &MmWorkspace) {
    send_workspace_home_with(workspace, &WinWindowOps);
}

pub fn send_workspace_target(workspace: &MmWorkspace) {
    send_workspace_target_with(workspace, &WinWindowOps);
}

pub fn send_all_home(workspaces: &[MmWorkspace]) {
    let ops = WinWindowOps;
    for workspace in workspaces {
        send_workspace_home_with(workspace, &ops);
    }
}

pub fn toggle_workspace(workspace: &mut MmWorkspace) {
    toggle_workspace_with(workspace, &WinWindowOps);
}

pub fn rotate_workspace(workspace: &mut MmWorkspace) {
    rotate_workspace_with(workspace, &WinWindowOps);
}

pub fn send_workspace_home_with(workspace: &MmWorkspace, ops: &impl WindowOps) {
    move_workspace_windows(workspace, RectKind::Home, ops);
}

pub fn send_workspace_target_with(workspace: &MmWorkspace, ops: &impl WindowOps) {
    move_workspace_windows(workspace, RectKind::Target, ops);
}

pub fn toggle_workspace_with(workspace: &mut MmWorkspace, ops: &impl WindowOps) {
    if workspace.disabled || !workspace.valid {
        return;
    }
    if workspace.rotate {
        rotate_workspace_with(workspace, ops);
        return;
    }

    let all_at_home = workspace
        .windows
        .iter()
        .filter(|w| w.can_activate())
        .all(|w| {
            w.home_rect
                .is_some_and(|rect| ops.is_window_at_rect(w.hwnd, rect))
        });

    if all_at_home {
        send_workspace_target_with(workspace, ops);
    } else {
        send_workspace_home_with(workspace, ops);
    }
}

pub fn rotate_workspace_with(workspace: &mut MmWorkspace, ops: &impl WindowOps) {
    if workspace.disabled || !workspace.valid {
        return;
    }

    let valid_indices: Vec<usize> = workspace
        .windows
        .iter()
        .enumerate()
        .filter_map(|(idx, window)| window.can_activate().then_some(idx))
        .collect();
    if valid_indices.is_empty() {
        return;
    }

    let primary = workspace.windows[valid_indices[0]].target_rect;
    let slots: Vec<MmRect> = valid_indices
        .iter()
        .filter_map(|&idx| workspace.windows[idx].home_rect)
        .collect();
    if slots.is_empty() {
        return;
    }

    let offset = workspace.rotation_offset % valid_indices.len();
    for (slot_idx, &window_idx) in valid_indices
        .iter()
        .cycle()
        .skip(offset)
        .take(valid_indices.len())
        .enumerate()
    {
        let target = if slot_idx == 0 {
            primary
        } else {
            slots.get(slot_idx - 1).copied()
        };
        if let Some(rect) = target {
            let _ = ops.move_window_to_rect(workspace.windows[window_idx].hwnd, rect);
        }
    }
    workspace.rotation_offset = workspace.rotation_offset.wrapping_add(1);
}

#[derive(Clone, Copy)]
enum RectKind {
    Home,
    Target,
}

fn move_workspace_windows(workspace: &MmWorkspace, kind: RectKind, ops: &impl WindowOps) {
    if workspace.disabled || !workspace.valid {
        return;
    }
    for window in &workspace.windows {
        if !window.can_activate() {
            continue;
        }
        let rect = match kind {
            RectKind::Home => window.home_rect.or(workspace.home_rect),
            RectKind::Target => window.target_rect.or(workspace.target_rect),
        };
        if let Some(rect) = rect {
            let _ = ops.move_window_to_rect(window.hwnd, rect);
        }
    }
}

fn hotkey_sequence(hotkey: &MmHotkey) -> Option<String> {
    let key = hotkey.key.trim();
    if key.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    if hotkey.ctrl {
        parts.push("Ctrl");
    }
    if hotkey.shift {
        parts.push("Shift");
    }
    if hotkey.alt {
        parts.push("Alt");
    }
    if hotkey.win {
        parts.push("Win");
    }
    parts.push(key);
    Some(parts.join("+"))
}

pub fn maybe_runtime_reconnect(
    workspaces: &Arc<Mutex<Vec<MmWorkspace>>>,
    control: &RuntimeControl,
    last_reconnect: &mut Instant,
    now: Instant,
    is_window: impl Fn(usize) -> bool,
    enumerate: impl FnOnce() -> Vec<win::EnumeratedWindow>,
) -> bool {
    let reconnect_interval =
        Duration::from_millis(control.auto_reconnect_interval_ms.load(Ordering::Relaxed));
    if !control.enabled.load(Ordering::Relaxed)
        || !control
            .auto_reconnect_missing_windows
            .load(Ordering::Relaxed)
        || control.capture_pending.load(Ordering::Relaxed)
        || now.duration_since(*last_reconnect) < reconnect_interval
    {
        return false;
    }

    let Ok(mut workspaces) = workspaces.lock() else {
        return false;
    };

    let mut hwnd_changed = false;
    for window in workspaces
        .iter_mut()
        .filter(|workspace| !workspace.disabled)
        .flat_map(|workspace| &mut workspace.windows)
        .filter(|window| !window.disabled && window.hwnd != 0)
    {
        if is_window(window.hwnd) {
            continue;
        }
        window.mark_closed();
        hwnd_changed = true;
    }

    let has_unresolved = workspaces
        .iter()
        .filter(|workspace| !workspace.disabled)
        .flat_map(|workspace| &workspace.windows)
        .any(|window| !window.disabled && (window.hwnd == 0 || !window.valid));

    if has_unresolved {
        let live = enumerate();
        let summary =
            reconnect::reconnect_unresolved_workspaces_with_windows(&mut workspaces, &live);
        hwnd_changed |= summary.binding_snapshot_changed;
    }

    *last_reconnect = now;
    hwnd_changed
}

pub fn runtime_tick(
    workspaces: &mut [MmWorkspace],
    control: &RuntimeControl,
    last_hotkey_info: &Arc<Mutex<Option<(String, Instant)>>>,
    event_queue: &Arc<Mutex<VecDeque<MultiManagerRuntimeEvent>>>,
    debounce: &mut HashMap<String, Instant>,
    debounce_duration: Duration,
    window_ops: &impl WindowOps,
    hotkey_ops: &impl HotkeyOps,
    is_window: &dyn Fn(usize) -> bool,
    enumerate_top_level_windows: &dyn Fn() -> Vec<win::EnumeratedWindow>,
    now: Instant,
) {
    if !control.enabled.load(Ordering::Relaxed) || control.capture_pending.load(Ordering::Relaxed) {
        return;
    }
    for workspace in workspaces.iter_mut().filter(|w| !w.disabled && w.valid) {
        let Some(sequence) = workspace.hotkey.as_ref().and_then(hotkey_sequence) else {
            continue;
        };
        if !hotkey_ops.is_hotkey_pressed(&sequence) {
            continue;
        }
        if debounce
            .get(&workspace.id)
            .is_some_and(|last| now.duration_since(*last) < debounce_duration)
        {
            continue;
        }
        debounce.insert(workspace.id.clone(), now);
        let workspace_id = workspace.id.clone();
        let deps = ActivationDeps {
            window_ops,
            is_window,
            enumerate_top_level_windows,
        };
        let result = activation::activate_workspace_with_deps(
            std::slice::from_mut(workspace),
            &workspace_id,
            ActivationOperation::Toggle,
            &deps,
        )
        .unwrap_or_default();
        push_runtime_activation_events(
            event_queue,
            workspace_id.clone(),
            ActivationOperation::Toggle,
            result,
        );
        if let Ok(mut info) = last_hotkey_info.lock() {
            *info = Some((workspace.id.clone(), now));
        }
    }
}

fn push_runtime_activation_events(
    event_queue: &Arc<Mutex<VecDeque<MultiManagerRuntimeEvent>>>,
    workspace_id: String,
    operation: ActivationOperation,
    result: ActivationResult,
) {
    let Ok(mut events) = event_queue.lock() else {
        return;
    };
    if result.reconnected > 0 {
        events.push_back(MultiManagerRuntimeEvent::BindingReconnected {
            workspace_id: workspace_id.clone(),
            result: result.clone(),
        });
    }
    events.push_back(MultiManagerRuntimeEvent::WorkspaceActionCompleted {
        workspace_id,
        operation,
        result,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::multi_manager::model::{MmHotkey, MmWindow};
    use crate::multi_manager::win::EnumeratedWindow;
    use std::cell::{Cell, RefCell};

    fn rect(x: i32) -> MmRect {
        MmRect {
            x,
            y: 0,
            w: 10,
            h: 10,
        }
    }

    #[derive(Default)]
    struct FakeWindowOps {
        at_home: HashMap<usize, MmRect>,
        moves: RefCell<Vec<(usize, MmRect)>>,
    }

    impl WindowOps for FakeWindowOps {
        fn is_window_at_rect(&self, hwnd: usize, rect: MmRect) -> bool {
            self.at_home.get(&hwnd).copied() == Some(rect)
        }
        fn move_window_to_rect(&self, hwnd: usize, rect: MmRect) -> Result<()> {
            self.moves.borrow_mut().push((hwnd, rect));
            Ok(())
        }
    }

    struct FakeHotkeyOps(bool);
    impl HotkeyOps for FakeHotkeyOps {
        fn is_hotkey_pressed(&self, _sequence: &str) -> bool {
            self.0
        }
    }

    fn window(hwnd: usize, valid: bool, home: MmRect, target: MmRect) -> MmWindow {
        MmWindow {
            hwnd,
            valid,
            home_rect: Some(home),
            target_rect: Some(target),
            ..MmWindow::default()
        }
    }

    fn event_queue() -> Arc<Mutex<VecDeque<MultiManagerRuntimeEvent>>> {
        Arc::new(Mutex::new(VecDeque::new()))
    }

    fn live(hwnd: usize, title: &str) -> EnumeratedWindow {
        EnumeratedWindow {
            hwnd,
            title: title.into(),
            executable: "app.exe".into(),
            class_name: "AppClass".into(),
            process_path: "C:/app.exe".into(),
            rect: rect(0),
        }
    }

    fn workspace() -> MmWorkspace {
        MmWorkspace {
            id: "ws".into(),
            windows: vec![
                window(1, true, rect(1), rect(11)),
                window(2, true, rect(2), rect(12)),
            ],
            hotkey: Some(MmHotkey {
                key: "F9".into(),
                ..MmHotkey::default()
            }),
            ..MmWorkspace::default()
        }
    }

    #[test]
    fn disabled_workspace_does_not_toggle() {
        let mut ws = workspace();
        ws.disabled = true;
        let ops = FakeWindowOps::default();
        toggle_workspace_with(&mut ws, &ops);
        assert!(ops.moves.borrow().is_empty());
    }

    #[test]
    fn invalid_windows_are_skipped() {
        let mut ws = workspace();
        ws.windows[1].valid = false;
        let mut ops = FakeWindowOps::default();
        ops.at_home.insert(1, rect(1));
        toggle_workspace_with(&mut ws, &ops);
        assert_eq!(*ops.moves.borrow(), vec![(1, rect(11))]);
    }

    #[test]
    fn normal_toggle_home_to_target() {
        let mut ws = workspace();
        let mut ops = FakeWindowOps::default();
        ops.at_home.insert(1, rect(1));
        ops.at_home.insert(2, rect(2));
        toggle_workspace_with(&mut ws, &ops);
        assert_eq!(*ops.moves.borrow(), vec![(1, rect(11)), (2, rect(12))]);
    }

    #[test]
    fn normal_toggle_target_or_mixed_to_home() {
        let mut ws = workspace();
        let mut ops = FakeWindowOps::default();
        ops.at_home.insert(1, rect(1));
        toggle_workspace_with(&mut ws, &ops);
        assert_eq!(*ops.moves.borrow(), vec![(1, rect(1)), (2, rect(2))]);
    }

    #[test]
    fn rotate_increments_rotation_offset() {
        let mut ws = workspace();
        ws.rotate = true;
        let ops = FakeWindowOps::default();
        rotate_workspace_with(&mut ws, &ops);
        assert_eq!(ws.rotation_offset, 1);
        assert_eq!(*ops.moves.borrow(), vec![(1, rect(11)), (2, rect(1))]);
    }

    #[test]
    fn rotate_skips_invalid_windows() {
        let mut ws = workspace();
        ws.rotate = true;
        ws.windows[0].valid = false;
        let ops = FakeWindowOps::default();
        rotate_workspace_with(&mut ws, &ops);
        assert_eq!(ws.rotation_offset, 1);
        assert_eq!(*ops.moves.borrow(), vec![(2, rect(12))]);
    }

    #[test]
    fn send_home_uses_window_rect_before_workspace_fallback() {
        let mut ws = workspace();
        ws.home_rect = Some(rect(99));
        ws.windows[1].home_rect = None;
        let ops = FakeWindowOps::default();
        send_workspace_home_with(&ws, &ops);
        assert_eq!(*ops.moves.borrow(), vec![(1, rect(1)), (2, rect(99))]);
    }

    #[test]
    fn send_target_uses_window_rect_before_workspace_fallback() {
        let mut ws = workspace();
        ws.target_rect = Some(rect(88));
        ws.windows[0].target_rect = None;
        let ops = FakeWindowOps::default();
        send_workspace_target_with(&ws, &ops);
        assert_eq!(*ops.moves.borrow(), vec![(1, rect(88)), (2, rect(12))]);
    }

    #[test]
    fn movement_skips_disabled_invalid_and_zero_handle_windows() {
        let mut ws = workspace();
        ws.windows[0].disabled = true;
        ws.windows[1].hwnd = 0;
        ws.windows.push(window(3, false, rect(3), rect(13)));
        let ops = FakeWindowOps::default();
        send_workspace_target_with(&ws, &ops);
        assert!(ops.moves.borrow().is_empty());
    }

    #[test]
    fn runtime_skips_when_capture_pending_is_true() {
        let mut workspaces = vec![workspace()];
        let control = RuntimeControl::new(true);
        control.capture_pending.store(true, Ordering::Relaxed);
        let info = Arc::new(Mutex::new(None));
        let mut debounce = HashMap::new();
        let ops = FakeWindowOps::default();
        runtime_tick(
            &mut workspaces,
            &control,
            &info,
            &event_queue(),
            &mut debounce,
            DEFAULT_DEBOUNCE,
            &ops,
            &FakeHotkeyOps(true),
            &|_| true,
            &|| Vec::new(),
            Instant::now(),
        );
        assert!(ops.moves.borrow().is_empty());
        assert!(info.lock().unwrap().is_none());
    }

    #[test]
    fn runtime_reconnect_skipped_during_capture() {
        let workspaces = Arc::new(Mutex::new(vec![workspace()]));
        workspaces.lock().unwrap()[0].windows[0].hwnd = 0;
        let control = RuntimeControl::new(true);
        control.capture_pending.store(true, Ordering::Relaxed);
        let now = Instant::now();
        let mut last = now - Duration::from_secs(10);
        let enumerated = RefCell::new(false);

        let reconnected = maybe_runtime_reconnect(
            &workspaces,
            &control,
            &mut last,
            now,
            |_| true,
            || {
                *enumerated.borrow_mut() = true;
                vec![live(10, "anything")]
            },
        );

        assert!(!reconnected);
        assert!(!*enumerated.borrow());
    }

    #[test]
    fn runtime_reconnect_does_not_enumerate_when_all_windows_are_valid() {
        let workspaces = Arc::new(Mutex::new(vec![workspace()]));
        let control = RuntimeControl::new(true);
        let now = Instant::now();
        let mut last = now - Duration::from_secs(10);
        let is_window_calls = Cell::new(0);
        let enumerations = Cell::new(0);

        let reconnected = maybe_runtime_reconnect(
            &workspaces,
            &control,
            &mut last,
            now,
            |_| {
                is_window_calls.set(is_window_calls.get() + 1);
                true
            },
            || {
                enumerations.set(enumerations.get() + 1);
                Vec::new()
            },
        );

        assert!(!reconnected);
        assert_eq!(is_window_calls.get(), 2);
        assert_eq!(enumerations.get(), 0);
    }

    #[test]
    fn runtime_reconnect_enumerates_once_when_one_hwnd_is_invalid() {
        let workspaces = Arc::new(Mutex::new(vec![workspace()]));
        let control = RuntimeControl::new(true);
        let now = Instant::now();
        let mut last = now - Duration::from_secs(10);
        let enumerations = Cell::new(0);

        let changed = maybe_runtime_reconnect(
            &workspaces,
            &control,
            &mut last,
            now,
            |hwnd| hwnd != 1,
            || {
                enumerations.set(enumerations.get() + 1);
                Vec::new()
            },
        );

        assert!(changed);
        assert_eq!(enumerations.get(), 1);
        assert_eq!(workspaces.lock().unwrap()[0].windows[0].hwnd, 0);
    }

    #[test]
    fn runtime_reconnect_clears_closed_hwnd_even_when_previously_valid() {
        let workspaces = Arc::new(Mutex::new(vec![workspace()]));
        let control = RuntimeControl::new(true);
        let now = Instant::now();
        let mut last = now - Duration::from_secs(10);

        let changed = maybe_runtime_reconnect(
            &workspaces,
            &control,
            &mut last,
            now,
            |hwnd| hwnd != 1,
            || Vec::new(),
        );

        let locked = workspaces.lock().unwrap();
        assert!(changed);
        assert_eq!(locked[0].windows[0].hwnd, 0);
        assert!(!locked[0].windows[0].valid);
        assert_eq!(locked[0].windows[1].hwnd, 2);
        assert!(locked[0].windows[1].valid);
    }

    #[test]
    fn runtime_reconnect_leaves_unresolved_window_disconnected_without_affecting_valid_bindings() {
        let workspaces = Arc::new(Mutex::new(vec![workspace()]));
        {
            let mut locked = workspaces.lock().unwrap();
            locked[0].windows[0].hwnd = 0;
            locked[0].windows[0].valid = false;
            locked[0].windows[0].captured_title = "Missing".into();
        }
        let control = RuntimeControl::new(true);
        let now = Instant::now();
        let mut last = now - Duration::from_secs(10);

        let changed = maybe_runtime_reconnect(
            &workspaces,
            &control,
            &mut last,
            now,
            |_| true,
            || vec![live(99, "Other")],
        );

        let locked = workspaces.lock().unwrap();
        assert!(!changed);
        assert_eq!(locked[0].windows[0].hwnd, 0);
        assert!(!locked[0].windows[0].valid);
        assert_eq!(locked[0].windows[1].hwnd, 2);
        assert!(locked[0].windows[1].valid);
    }

    #[test]
    fn runtime_reconnect_assigns_missing_window_from_unique_candidate() {
        let workspaces = Arc::new(Mutex::new(vec![workspace()]));
        {
            let mut locked = workspaces.lock().unwrap();
            locked[0].windows[0].hwnd = 0;
            locked[0].windows[0].captured_title = "Notes".into();
            locked[0].windows[0].executable = "app.exe".into();
            locked[0].windows[0].class_name = "AppClass".into();
            locked[0].windows[0].process_path = "C:/app.exe".into();
        }
        let control = RuntimeControl::new(true);
        let now = Instant::now();
        let mut last = now - Duration::from_secs(10);

        let reconnected = maybe_runtime_reconnect(
            &workspaces,
            &control,
            &mut last,
            now,
            |_| true,
            || vec![live(42, "Notes")],
        );

        assert!(reconnected);
        assert_eq!(workspaces.lock().unwrap()[0].windows[0].hwnd, 42);
    }

    #[test]
    fn runtime_reconnect_runs_when_existing_hwnd_is_stale() {
        let workspaces = Arc::new(Mutex::new(vec![workspace()]));
        {
            let mut locked = workspaces.lock().unwrap();
            locked[0].windows[0].hwnd = 7;
            locked[0].windows[0].valid = false;
            locked[0].windows[0].captured_title = "Notes".into();
            locked[0].windows[0].executable = "app.exe".into();
            locked[0].windows[0].class_name = "AppClass".into();
            locked[0].windows[0].process_path = "C:/app.exe".into();
        }
        let control = RuntimeControl::new(true);
        let now = Instant::now();
        let mut last = now - Duration::from_secs(10);
        let enumerated = RefCell::new(false);

        let reconnected = maybe_runtime_reconnect(
            &workspaces,
            &control,
            &mut last,
            now,
            |_| true,
            || {
                *enumerated.borrow_mut() = true;
                vec![live(42, "Notes")]
            },
        );

        assert!(reconnected);
        assert!(*enumerated.borrow());
        assert_eq!(workspaces.lock().unwrap()[0].windows[0].hwnd, 42);
    }

    #[test]
    fn runtime_hotkey_toggle_works_after_reconnect_assigns_hwnd() {
        let workspaces = Arc::new(Mutex::new(vec![workspace()]));
        {
            let mut locked = workspaces.lock().unwrap();
            locked[0].windows = vec![window(0, true, rect(1), rect(11))];
            locked[0].windows[0].captured_title = "Notes".into();
            locked[0].windows[0].executable = "app.exe".into();
            locked[0].windows[0].class_name = "AppClass".into();
            locked[0].windows[0].process_path = "C:/app.exe".into();
        }
        let control = RuntimeControl::new(true);
        let now = Instant::now();
        let mut last = now - Duration::from_secs(10);
        assert!(maybe_runtime_reconnect(
            &workspaces,
            &control,
            &mut last,
            now,
            |_| true,
            || vec![live(42, "Notes")],
        ));

        let info = Arc::new(Mutex::new(None));
        let mut debounce = HashMap::new();
        let mut ops = FakeWindowOps::default();
        ops.at_home.insert(42, rect(1));
        runtime_tick(
            &mut workspaces.lock().unwrap(),
            &control,
            &info,
            &event_queue(),
            &mut debounce,
            DEFAULT_DEBOUNCE,
            &ops,
            &FakeHotkeyOps(true),
            &|hwnd| hwnd != 0,
            &|| Vec::new(),
            now + Duration::from_secs(1),
        );

        assert_eq!(*ops.moves.borrow(), vec![(42, rect(11))]);
        assert_eq!(
            info.lock().unwrap().as_ref().map(|(id, _)| id.as_str()),
            Some("ws")
        );
    }

    #[test]
    fn debounce_prevents_repeat_trigger_spam() {
        let mut workspaces = vec![workspace()];
        let control = RuntimeControl::new(true);
        let info = Arc::new(Mutex::new(None));
        let events = event_queue();
        let mut debounce = HashMap::new();
        let mut ops = FakeWindowOps::default();
        ops.at_home.insert(1, rect(1));
        ops.at_home.insert(2, rect(2));
        let now = Instant::now();
        runtime_tick(
            &mut workspaces,
            &control,
            &info,
            &events,
            &mut debounce,
            DEFAULT_DEBOUNCE,
            &ops,
            &FakeHotkeyOps(true),
            &|_| true,
            &|| Vec::new(),
            now,
        );
        runtime_tick(
            &mut workspaces,
            &control,
            &info,
            &events,
            &mut debounce,
            DEFAULT_DEBOUNCE,
            &ops,
            &FakeHotkeyOps(true),
            &|_| true,
            &|| Vec::new(),
            now + Duration::from_millis(100),
        );
        assert_eq!(ops.moves.borrow().len(), 2);
        assert_eq!(events.lock().unwrap().len(), 1);
    }

    #[test]
    fn hotkey_activation_attempts_fallback_before_moving() {
        let mut workspaces = vec![workspace()];
        workspaces[0].windows = vec![window(7, false, rect(1), rect(11))];
        workspaces[0].windows[0].captured_title = "Notes".into();
        let control = RuntimeControl::new(true);
        let info = Arc::new(Mutex::new(None));
        let events = event_queue();
        let mut debounce = HashMap::new();
        let mut ops = FakeWindowOps::default();
        ops.at_home.insert(42, rect(1));
        let now = Instant::now();

        runtime_tick(
            &mut workspaces,
            &control,
            &info,
            &events,
            &mut debounce,
            DEFAULT_DEBOUNCE,
            &ops,
            &FakeHotkeyOps(true),
            &|hwnd| hwnd == 42,
            &|| vec![live(42, "Notes")],
            now,
        );

        assert_eq!(*ops.moves.borrow(), vec![(42, rect(11))]);
        assert_eq!(workspaces[0].windows[0].hwnd, 42);
    }

    #[test]
    fn hotkey_moves_valid_windows_when_one_binding_is_missing() {
        let mut workspaces = vec![workspace()];
        workspaces[0].windows[0].captured_title = "Missing".into();
        workspaces[0].windows[0].hwnd = 0;
        workspaces[0].windows[0].valid = false;
        let control = RuntimeControl::new(true);
        let info = Arc::new(Mutex::new(None));
        let events = event_queue();
        let mut debounce = HashMap::new();
        let mut ops = FakeWindowOps::default();
        ops.at_home.insert(2, rect(2));

        runtime_tick(
            &mut workspaces,
            &control,
            &info,
            &events,
            &mut debounce,
            DEFAULT_DEBOUNCE,
            &ops,
            &FakeHotkeyOps(true),
            &|hwnd| hwnd == 2,
            &|| Vec::new(),
            Instant::now(),
        );

        assert_eq!(*ops.moves.borrow(), vec![(2, rect(12))]);
    }

    #[test]
    fn runtime_events_report_unresolved_windows() {
        let mut workspaces = vec![workspace()];
        workspaces[0].windows[0].alias = "Missing App".into();
        workspaces[0].windows[0].captured_title = "Missing".into();
        workspaces[0].windows[0].hwnd = 0;
        workspaces[0].windows[0].valid = false;
        let control = RuntimeControl::new(true);
        let info = Arc::new(Mutex::new(None));
        let events = event_queue();
        let mut debounce = HashMap::new();
        let ops = FakeWindowOps::default();

        runtime_tick(
            &mut workspaces,
            &control,
            &info,
            &events,
            &mut debounce,
            DEFAULT_DEBOUNCE,
            &ops,
            &FakeHotkeyOps(true),
            &|hwnd| hwnd == 2,
            &|| Vec::new(),
            Instant::now(),
        );

        let events = events.lock().unwrap();
        let MultiManagerRuntimeEvent::WorkspaceActionCompleted { result, .. } =
            events.back().unwrap()
        else {
            panic!("expected workspace action event");
        };
        assert_eq!(result.missing, 1);
        assert_eq!(result.unresolved_labels, vec!["Missing App".to_string()]);
    }

    #[test]
    fn runtime_event_definitions_do_not_reference_egui_types() {
        let source = include_str!("runtime.rs");
        let start = source.find("pub enum MultiManagerRuntimeEvent").unwrap();
        let end = source[start..]
            .find("pub struct MultiManagerRuntime")
            .unwrap()
            + start;
        let event_definition = &source[start..end];
        assert!(!event_definition.contains("egui"));
        assert!(!event_definition.contains("Toast"));
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "Manual Windows desktop smoke test: use a visible non-elevated HWND and verify send/toggle restores activation, temporarily uses topmost as needed, and does not leave the window permanently topmost."]
    fn windows_activation_and_topmost_smoke_checklist() {
        // Manual checklist for real desktop behavior that fake WindowOps cannot prove:
        // 1. Capture a normal non-elevated window into a workspace with home/target rects.
        // 2. Minimize or cover the window, then trigger send target/home and hotkey toggle.
        // 3. Verify the window is restored/activated for movement.
        // 4. Verify any temporary topmost behavior is cleared after movement.
        // 5. Repeat with an elevated/inaccessible window and verify the error is surfaced.
    }
}
