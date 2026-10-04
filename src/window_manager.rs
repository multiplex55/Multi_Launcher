pub use crate::platform::windows_api::{
    MOCK_MOUSE_LOCK, clear_mock_mouse_position, current_mouse_position, mock_mouse_position_is_set,
    set_mock_mouse_position,
};

use crate::radial::acceptance_trace::{
    self, Correlation, Event, NativeActivationEdge, NativeWindowIdentity,
};
use std::sync::{
    Arc, Condvar, Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
};

#[cfg_attr(not(test), allow(dead_code))]
pub fn virtual_key_from_string(key: &str) -> Option<u32> {
    match key.to_uppercase().as_str() {
        "F1" => Some(0x70),
        "F2" => Some(0x71),
        "F3" => Some(0x72),
        "F4" => Some(0x73),
        "F5" => Some(0x74),
        "F6" => Some(0x75),
        "F7" => Some(0x76),
        "F8" => Some(0x77),
        "F9" => Some(0x78),
        "F10" => Some(0x79),
        "F11" => Some(0x7A),
        "F12" => Some(0x7B),
        "F13" => Some(0x7C),
        "F14" => Some(0x7D),
        "F15" => Some(0x7E),
        "F16" => Some(0x7F),
        "F17" => Some(0x80),
        "F18" => Some(0x81),
        "F19" => Some(0x82),
        "F20" => Some(0x83),
        "F21" => Some(0x84),
        "F22" => Some(0x85),
        "F23" => Some(0x86),
        "F24" => Some(0x87),

        "A" => Some(0x41),
        "B" => Some(0x42),
        "C" => Some(0x43),
        "D" => Some(0x44),
        "E" => Some(0x45),
        "F" => Some(0x46),
        "G" => Some(0x47),
        "H" => Some(0x48),
        "I" => Some(0x49),
        "J" => Some(0x4A),
        "K" => Some(0x4B),
        "L" => Some(0x4C),
        "M" => Some(0x4D),
        "N" => Some(0x4E),
        "O" => Some(0x4F),
        "P" => Some(0x50),
        "Q" => Some(0x51),
        "R" => Some(0x52),
        "S" => Some(0x53),
        "T" => Some(0x54),
        "U" => Some(0x55),
        "V" => Some(0x56),
        "W" => Some(0x57),
        "X" => Some(0x58),
        "Y" => Some(0x59),
        "Z" => Some(0x5A),

        "0" => Some(0x30),
        "1" => Some(0x31),
        "2" => Some(0x32),
        "3" => Some(0x33),
        "4" => Some(0x34),
        "5" => Some(0x35),
        "6" => Some(0x36),
        "7" => Some(0x37),
        "8" => Some(0x38),
        "9" => Some(0x39),

        "NUMPAD0" => Some(0x60),
        "NUMPAD1" => Some(0x61),
        "NUMPAD2" => Some(0x62),
        "NUMPAD3" => Some(0x63),
        "NUMPAD4" => Some(0x64),
        "NUMPAD5" => Some(0x65),
        "NUMPAD6" => Some(0x66),
        "NUMPAD7" => Some(0x67),
        "NUMPAD8" => Some(0x68),
        "NUMPAD9" => Some(0x69),
        "NUMPADMULTIPLY" => Some(0x6A),
        "NUMPADADD" => Some(0x6B),
        "NUMPADSEPARATOR" => Some(0x6C),
        "NUMPADSUBTRACT" => Some(0x6D),
        "NUMPADDOT" => Some(0x6E),
        "NUMPADDIVIDE" => Some(0x6F),

        "UP" => Some(0x26),
        "DOWN" => Some(0x28),
        "LEFT" => Some(0x25),
        "RIGHT" => Some(0x27),

        "BACKSPACE" => Some(0x08),
        "TAB" => Some(0x09),
        "ENTER" => Some(0x0D),
        "SHIFT" => Some(0x10),
        "CTRL" => Some(0x11),
        "ALT" => Some(0x12),
        "PAUSE" => Some(0x13),
        "CAPSLOCK" => Some(0x14),
        "ESCAPE" => Some(0x1B),
        "SPACE" => Some(0x20),
        "PAGEUP" => Some(0x21),
        "PAGEDOWN" => Some(0x22),
        "END" => Some(0x23),
        "HOME" => Some(0x24),
        "INSERT" => Some(0x2D),
        "DELETE" => Some(0x2E),

        "OEM_PLUS" => Some(0xBB),
        "OEM_COMMA" => Some(0xBC),
        "OEM_MINUS" => Some(0xBD),
        "OEM_PERIOD" => Some(0xBE),
        "OEM_1" => Some(0xBA),
        "OEM_2" => Some(0xBF),
        "OEM_3" => Some(0xC0),
        "OEM_4" => Some(0xDB),
        "OEM_5" => Some(0xDC),
        "OEM_6" => Some(0xDD),
        "OEM_7" => Some(0xDE),

        "PRINTSCREEN" => Some(0x2C),
        "SCROLLLOCK" => Some(0x91),
        "NUMLOCK" => Some(0x90),
        "LEFTSHIFT" => Some(0xA0),
        "RIGHTSHIFT" => Some(0xA1),
        "LEFTCTRL" => Some(0xA2),
        "RIGHTCTRL" => Some(0xA3),
        "LEFTALT" => Some(0xA4),
        "RIGHTALT" => Some(0xA5),

        _ => None,
    }
}

use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// Restore and activate an arbitrary window by following it to its existing desktop.
pub fn force_restore_and_foreground(hwnd: windows::Win32::Foundation::HWND) {
    let request = crate::window_activation::WindowActivationRequest::follow_window(hwnd.0 as usize);
    std::thread::spawn(move || {
        if let Err(error) = crate::window_activation::activate_window(request) {
            tracing::warn!(error = %error, "failed to activate window");
        }
    });
}

/// Restore the launcher while explicitly relocating it onto the current desktop.
pub fn restore_launcher_to_current_desktop_ordered(
    target: crate::visibility::RootActivationTarget,
    revision: crate::visibility::VisibilityRevision,
    request_revision: u64,
    visible: Arc<AtomicBool>,
    root_window: crate::visibility::RootWindowBridge,
) -> bool {
    submit_launcher_restore(
        target,
        revision,
        request_revision,
        visible,
        root_window,
        enqueue_launcher_restore,
    )
}

fn submit_launcher_restore(
    target: crate::visibility::RootActivationTarget,
    revision: crate::visibility::VisibilityRevision,
    request_revision: u64,
    visible: Arc<AtomicBool>,
    root_window: crate::visibility::RootWindowBridge,
    enqueue: impl FnOnce(LauncherRestoreRequest) -> bool,
) -> bool {
    let trace_hwnd_value = target.hwnd;
    let Some(admission) = admit_launcher_restore(
        trace_hwnd_value,
        &revision,
        request_revision,
        &visible,
        &root_window,
    ) else {
        tracing::debug!(
            requested_hwnd = trace_hwnd_value,
            request_revision,
            "skipping a stale or non-activating ROOT restore before trace admission"
        );
        return false;
    };
    if admission.root_window_generation != target.generation {
        return false;
    }
    let owner = revision.clone();
    owner
        .submit_root_activation(
            request_revision,
            target,
            || {
                visible.load(Ordering::Acquire)
                    && revision.focus_intent() == crate::visibility::RootFocusIntent::ActivateRoot
                    && root_window.is_current(target.hwnd, target.generation)
            },
            || {
                enqueue(build_launcher_restore(
                    target,
                    revision.clone(),
                    request_revision,
                    visible.clone(),
                    root_window.clone(),
                    admission,
                ))
            },
        )
        .unwrap_or(false)
}

fn build_launcher_restore(
    target: crate::visibility::RootActivationTarget,
    revision: crate::visibility::VisibilityRevision,
    request_revision: u64,
    visible: Arc<AtomicBool>,
    root_window: crate::visibility::RootWindowBridge,
    admission: LauncherRestoreAdmission,
) -> LauncherRestoreRequest {
    let request =
        crate::window_activation::WindowActivationRequest::move_to_current_desktop(target.hwnd);
    let trace_enabled = acceptance_trace::enabled() || cfg!(test);
    let trace_hwnd_value = target.hwnd;
    let trace_hwnd = trace_hwnd_value as u64;
    // Publish the actual admitted Focus request before the worker can emit its
    // native group. Each sample/request keeps the existing unique namespace;
    // neither a thread-local visibility link nor queue admission proves focus.
    let correlation = {
        if !trace_enabled {
            Correlation::default()
        } else {
            let focus = launcher_restore_correlation(
                acceptance_trace::next_request_id(),
                request_revision,
                admission.invocation_id,
            );
            acceptance_trace::emit(Event::RootCommand {
                command: crate::radial::acceptance_trace::RootCommandKind::Focus,
                correlation: focus,
            });
            acceptance_trace::request_window_sample(focus);
            let correlation = launcher_restore_correlation(
                acceptance_trace::next_request_id(),
                request_revision,
                admission.invocation_id,
            );
            acceptance_trace::emit(Event::NativeActivation {
                edge: NativeActivationEdge::RestoreRequested,
                hwnd: trace_hwnd,
                correlation,
            });
            correlation
        }
    };
    let expected_process = unsafe { windows::Win32::System::Threading::GetCurrentProcessId() };
    let activation = crate::window_activation::WindowActivationFence {
        revision,
        request_revision,
        visible,
        expected_process,
        root_window,
        root_window_generation: admission.root_window_generation,
    };
    LauncherRestoreRequest {
        request,
        fence: activation,
        hwnd: trace_hwnd,
        hwnd_value: trace_hwnd_value,
        correlation,
        trace_enabled,
    }
}

#[cfg(test)]
fn prepare_launcher_restore(
    target: crate::visibility::RootActivationTarget,
    revision: crate::visibility::VisibilityRevision,
    request_revision: u64,
    visible: Arc<AtomicBool>,
    root_window: crate::visibility::RootWindowBridge,
) -> Option<LauncherRestoreRequest> {
    let mut pending = None;
    submit_launcher_restore(
        target,
        revision,
        request_revision,
        visible,
        root_window,
        |request| {
            pending = Some(request);
            true
        },
    );
    pending
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LauncherRestoreAdmission {
    root_window_generation: u64,
    invocation_id: Option<u64>,
}

fn admit_launcher_restore(
    requested_hwnd: usize,
    revision: &crate::visibility::VisibilityRevision,
    request_revision: u64,
    visible: &AtomicBool,
    root_window: &crate::visibility::RootWindowBridge,
) -> Option<LauncherRestoreAdmission> {
    let (observed_revision, (is_visible, focus_intent, invocation_id)) = revision.inspect(|| {
        (
            visible.load(Ordering::Acquire),
            revision.focus_intent(),
            revision.invocation_id(),
        )
    });
    if observed_revision != request_revision
        || !is_visible
        || focus_intent != crate::visibility::RootFocusIntent::ActivateRoot
    {
        return None;
    }

    let (current_hwnd, root_window_generation) = revision.with_current(
        request_revision,
        || visible.load(Ordering::Acquire),
        || root_window.identity(),
    )?;
    (requested_hwnd != 0 && root_window_generation != 0 && current_hwnd == requested_hwnd)
        .then_some(LauncherRestoreAdmission {
            root_window_generation,
            invocation_id,
        })
}

fn launcher_restore_correlation(
    request_id: u64,
    visibility_revision: u64,
    invocation_id: Option<u64>,
) -> Correlation {
    Correlation {
        request_id,
        generation: request_id,
        visibility_revision,
        invocation_id: invocation_id.unwrap_or(0),
        ..Correlation::default()
    }
}

struct LauncherRestoreRequest {
    request: crate::window_activation::WindowActivationRequest,
    fence: crate::window_activation::WindowActivationFence,
    hwnd: u64,
    hwnd_value: usize,
    correlation: Correlation,
    trace_enabled: bool,
}

#[derive(Default)]
struct LauncherRestoreQueue {
    pending: Mutex<Option<LauncherRestoreRequest>>,
    wake: Condvar,
}

static LAUNCHER_RESTORE_QUEUE: OnceLock<Option<Arc<LauncherRestoreQueue>>> = OnceLock::new();

fn launcher_restore_queue() -> Option<&'static Arc<LauncherRestoreQueue>> {
    LAUNCHER_RESTORE_QUEUE
        .get_or_init(|| {
            let queue = Arc::new(LauncherRestoreQueue::default());
            let worker_queue = Arc::clone(&queue);
            match std::thread::Builder::new()
                .name("launcher-root-restore".into())
                .spawn(move || {
                    loop {
                        let request = {
                            let mut pending = worker_queue
                                .pending
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                            while pending.is_none() {
                                pending = worker_queue
                                    .wake
                                    .wait(pending)
                                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                            }
                            let Some(request) = pending.take() else {
                                continue;
                            };
                            request
                        };
                        run_launcher_restore(request);
                    }
                }) {
                Ok(_) => Some(queue),
                Err(error) => {
                    tracing::error!(%error, "could not start bounded launcher restore worker");
                    None
                }
            }
        })
        .as_ref()
}

#[cfg(test)]
thread_local! {
    static TEST_RESTORE_QUEUE: std::cell::RefCell<Option<Vec<(u64, usize)>>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
pub(crate) fn with_launcher_restore_queue_for_test<T>(
    run: impl FnOnce() -> T,
) -> (T, Vec<(u64, usize)>) {
    // Observations below belong to this actual dispatch interval, matching the
    // scoped queue capture rather than events left by an earlier fixture.
    acceptance_trace::take_root_activation_test_events();
    acceptance_trace::take_root_sample_requests_for_test();
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            TEST_RESTORE_QUEUE.with(|slot| *slot.borrow_mut() = None);
        }
    }
    TEST_RESTORE_QUEUE.with(|slot| {
        assert!(slot.borrow().is_none());
        *slot.borrow_mut() = Some(Vec::new());
    });
    let _reset = Reset;
    let result = run();
    let requests = TEST_RESTORE_QUEUE.with(|slot| slot.borrow_mut().take().unwrap());
    (result, requests)
}

fn enqueue_launcher_restore(request: LauncherRestoreRequest) -> bool {
    #[cfg(test)]
    if TEST_RESTORE_QUEUE.with(|slot| {
        if let Some(requests) = slot.borrow_mut().as_mut() {
            requests.push((request.fence.request_revision, request.hwnd_value));
            true
        } else {
            false
        }
    }) {
        return true;
    }
    enqueue_launcher_restore_with_queue(request, launcher_restore_queue())
}

fn enqueue_launcher_restore_with_queue(
    request: LauncherRestoreRequest,
    queue: Option<&Arc<LauncherRestoreQueue>>,
) -> bool {
    let Some(queue) = queue else {
        if request.trace_enabled {
            emit_launcher_restore_result(&request, false, false);
        }
        return false;
    };
    let replaced = {
        let mut pending = queue
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        pending.replace(request)
    };
    if let Some(replaced) = replaced
        && replaced.trace_enabled
    {
        emit_launcher_restore_result(&replaced, false, false);
    }
    queue.wake.notify_one();
    true
}

fn run_launcher_restore(request: LauncherRestoreRequest) {
    let result = crate::window_activation::activate_window_if_current(
        request.request,
        request.fence.clone(),
    );
    complete_launcher_restore(&request, result);
}

fn complete_launcher_restore(
    request: &LauncherRestoreRequest,
    result: Result<(), crate::window_activation::WindowActivationError>,
) {
    match result {
        Err(error) => {
            if request.trace_enabled {
                emit_launcher_restore_result(request, false, true);
            }
            if !matches!(
                error.kind,
                crate::window_activation::WindowActivationErrorKind::Superseded
            ) {
                tracing::warn!(error = %error, "failed to restore launcher window");
            }
        }
        Ok(()) => {
            if request.trace_enabled {
                emit_launcher_restore_result(request, true, true);
            }
        }
    }
}

fn emit_launcher_restore_result(
    request: &LauncherRestoreRequest,
    completed: bool,
    include_snapshot: bool,
) {
    let terminal_correlation = Correlation {
        terminal: true,
        ..request.correlation
    };
    acceptance_trace::emit(Event::NativeActivation {
        edge: if completed {
            NativeActivationEdge::RestoreCompleted
        } else {
            NativeActivationEdge::RestoreFailed
        },
        hwnd: request.hwnd,
        correlation: terminal_correlation,
    });
    if include_snapshot {
        emit_window_snapshot(
            windows::Win32::Foundation::HWND(request.hwnd_value as *mut _),
            terminal_correlation,
        );
    }
}

/// Return the native window currently under the pointer for the opt-in trace.
/// This stays behind the trace switch so disabled runs do not add a native query.
#[cfg(windows)]
pub(crate) fn window_under_cursor() -> Option<NativeWindowIdentity> {
    if !acceptance_trace::enabled() {
        return None;
    }
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, WindowFromPoint};

    let mut point = POINT::default();
    if unsafe { GetCursorPos(&mut point) }.is_err() {
        return None;
    }
    let hwnd = unsafe { WindowFromPoint(point) };
    (!hwnd.0.is_null()).then(|| {
        let hwnd = hwnd.0 as usize as u64;
        NativeWindowIdentity {
            hwnd,
            owner: acceptance_trace::classify_window(hwnd),
            screen_x: point.x,
            screen_y: point.y,
        }
    })
}

#[cfg(not(windows))]
pub(crate) fn window_under_cursor() -> Option<NativeWindowIdentity> {
    None
}

/// Register the actual launcher root HWND for trace-only owner classification.
/// The disabled path does not inspect or retain the handle.
#[cfg(windows)]
pub(crate) fn register_root_hwnd(hwnd: windows::Win32::Foundation::HWND) {
    if acceptance_trace::enabled() && !hwnd.0.is_null() {
        acceptance_trace::register_root_hwnd(hwnd.0 as usize as u64);
    }
}

#[cfg(not(windows))]
pub(crate) fn register_root_hwnd(_hwnd: windows::Win32::Foundation::HWND) {}

/// Emit an actual native root window snapshot at a visibility boundary.
/// Only scalar handle, geometry, and state are exposed.
#[cfg(windows)]
pub(crate) fn emit_window_snapshot(
    hwnd: windows::Win32::Foundation::HWND,
    correlation: Correlation,
) {
    if !acceptance_trace::enabled() || hwnd.0.is_null() {
        return;
    }
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowRect, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
    };

    acceptance_trace::register_root_hwnd(hwnd.0 as usize as u64);

    let mut rect = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut rect) }.is_err() {
        return;
    }
    let mut process_id = 0;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
    acceptance_trace::emit(Event::NativeWindowSnapshot {
        hwnd: hwnd.0 as usize as u64,
        process_id,
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
        visible: unsafe { IsWindowVisible(hwnd) }.as_bool(),
        minimized: unsafe { IsIconic(hwnd) }.as_bool(),
        correlation,
    });
}

#[cfg(not(windows))]
pub(crate) fn emit_window_snapshot(
    _hwnd: windows::Win32::Foundation::HWND,
    _correlation: Correlation,
) {
}

/// Extract the HWND from an eframe [`Frame`].
pub fn get_hwnd(frame: &eframe::Frame) -> Option<windows::Win32::Foundation::HWND> {
    if let Ok(handle) = frame.window_handle() {
        match handle.as_raw() {
            RawWindowHandle::Win32(h) => Some(windows::Win32::Foundation::HWND(
                h.hwnd.get() as *mut core::ffi::c_void
            )),
            _ => None,
        }
    } else {
        None
    }
}

pub fn activate_process(pid: u32) {
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GW_OWNER, GetWindow, GetWindowThreadProcessId, IsWindowVisible,
    };
    unsafe extern "system" fn enum_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let target = lparam.0 as u32;
        let mut pid: u32 = 0;
        unsafe {
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
        }
        if pid == target
            && unsafe { IsWindowVisible(hwnd) }.as_bool()
            && unsafe { GetWindow(hwnd, GW_OWNER) }
                .unwrap_or_default()
                .0
                .is_null()
        {
            crate::window_manager::force_restore_and_foreground(hwnd);
            return BOOL(0);
        }
        BOOL(1)
    }

    unsafe {
        let _ = EnumWindows(Some(enum_cb), LPARAM(pid as isize));
    }
}

pub fn close_window(hwnd: usize) {
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};
    unsafe {
        let _ = PostMessageW(HWND(hwnd as *mut _), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
}

#[cfg(test)]
mod launcher_restore_trace_tests {
    use super::*;

    #[test]
    fn restore_and_boundary_events_use_interleaved_unique_request_ids() {
        let restore_request_id = acceptance_trace::next_request_id();
        let restore = launcher_restore_correlation(restore_request_id, 31, Some(44));
        let boundary = acceptance_trace::root_command_correlation();
        let next_restore_request_id = acceptance_trace::next_request_id();

        assert_ne!(restore.request_id, boundary.request_id);
        assert_ne!(restore.request_id, next_restore_request_id);
        assert_ne!(boundary.request_id, next_restore_request_id);
        assert_eq!(restore.visibility_revision, 31);
        assert_eq!(restore.invocation_id, 44);
    }

    #[test]
    fn launcher_restore_correlation_carries_visibility_revision_and_invocation() {
        let revision = crate::visibility::VisibilityRevision::default();
        let visible = AtomicBool::new(false);
        let (request_revision, ()) = revision.request_with_focus_intent_and_invocation(
            crate::visibility::RootFocusIntent::ActivateRoot,
            Some(17),
            || visible.store(true, Ordering::Release),
        );
        let root_window = crate::visibility::RootWindowBridge::default();
        root_window.set_identity_for_test(101);

        let admission =
            admit_launcher_restore(101, &revision, request_revision, &visible, &root_window)
                .unwrap();
        let correlation =
            launcher_restore_correlation(22, request_revision, admission.invocation_id);

        assert_eq!(admission.root_window_generation, root_window.identity().1);
        assert_eq!(correlation.request_id, 22);
        assert_eq!(correlation.visibility_revision, request_revision);
        assert_eq!(correlation.invocation_id, 17);
    }

    #[test]
    fn stale_root_hwnd_is_rejected_before_restore_request_trace_admission() {
        let revision = crate::visibility::VisibilityRevision::default();
        let visible = AtomicBool::new(false);
        let (request_revision, ()) = revision.request_with_focus_intent_and_invocation(
            crate::visibility::RootFocusIntent::ActivateRoot,
            Some(19),
            || visible.store(true, Ordering::Release),
        );
        let root_window = crate::visibility::RootWindowBridge::default();
        root_window.set_identity_for_test(202);

        assert!(
            admit_launcher_restore(101, &revision, request_revision, &visible, &root_window,)
                .is_none()
        );
    }

    #[test]
    fn stale_visibility_request_is_rejected_before_restore_trace_admission() {
        let revision = crate::visibility::VisibilityRevision::default();
        let visible = AtomicBool::new(false);
        let (stale_revision, ()) = revision.request_with_focus_intent_and_invocation(
            crate::visibility::RootFocusIntent::ActivateRoot,
            Some(21),
            || visible.store(true, Ordering::Release),
        );
        let (current_revision, ()) = revision.request_with_focus_intent_and_invocation(
            crate::visibility::RootFocusIntent::PreserveForeground,
            Some(22),
            || visible.store(true, Ordering::Release),
        );
        let root_window = crate::visibility::RootWindowBridge::default();
        root_window.set_identity_for_test(101);

        assert!(current_revision > stale_revision);
        assert!(
            admit_launcher_restore(101, &revision, stale_revision, &visible, &root_window,)
                .is_none()
        );
        assert!(
            admit_launcher_restore(101, &revision, current_revision, &visible, &root_window,)
                .is_none()
        );
    }

    fn activation_fixture() -> (
        crate::visibility::RootActivationTarget,
        crate::visibility::VisibilityRevision,
        u64,
        Arc<AtomicBool>,
        crate::visibility::RootWindowBridge,
    ) {
        acceptance_trace::take_root_activation_test_events();
        acceptance_trace::take_root_sample_requests_for_test();
        let revision = crate::visibility::VisibilityRevision::default();
        let visible = Arc::new(AtomicBool::new(true));
        let request_revision = revision
            .request_with_focus_intent_and_invocation(
                crate::visibility::RootFocusIntent::ActivateRoot,
                Some(71),
                || (),
            )
            .0;
        let root = crate::visibility::RootWindowBridge::default();
        root.set_identity_for_test(42);
        (
            root.activation_target().unwrap(),
            revision,
            request_revision,
            visible,
            root,
        )
    }

    #[test]
    fn owned_focus_request_precedes_native_admission_with_explicit_unique_correlation() {
        let (target, revision, request_revision, visible, root) = activation_fixture();
        let (_, queue) = with_launcher_restore_queue_for_test(|| {
            acceptance_trace::with_visibility_trace_link(999, Some(888), || {
                assert!(restore_launcher_to_current_desktop_ordered(
                    target,
                    revision.clone(),
                    request_revision,
                    visible.clone(),
                    root,
                ));
            });
        });
        assert_eq!(queue, [(request_revision, 42)]);
        let events = acceptance_trace::take_root_activation_test_events();
        let [
            Event::RootCommand {
                command: crate::radial::acceptance_trace::RootCommandKind::Focus,
                correlation: focus,
            },
            Event::NativeActivation {
                edge: NativeActivationEdge::RestoreRequested,
                hwnd: 42,
                correlation: native,
            },
        ] = events.as_slice()
        else {
            panic!("unexpected admitted request sequence: {events:?}");
        };
        assert!(focus.request_id > 0 && focus.request_id < native.request_id);
        assert_ne!(focus.request_id, native.request_id);
        for correlation in [focus, native] {
            assert_eq!(correlation.visibility_revision, request_revision);
            assert_eq!(correlation.invocation_id, 71);
            assert_eq!(correlation.generation, correlation.request_id);
            assert!(!correlation.terminal);
        }
        assert_eq!(
            acceptance_trace::take_root_sample_requests_for_test(),
            [*focus]
        );
        // Actual queue admission has not run a backend or emitted completion.
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn captured_root_generation_hide_and_nonactivating_intent_reject_before_focus() {
        for invalid in 0..6 {
            let (mut target, revision, mut request_revision, visible, root) = activation_fixture();
            match invalid {
                0 => target.hwnd = 43,
                1 => {
                    root.set_identity_for_test(43);
                    root.set_identity_for_test(42);
                }
                2 => {
                    revision.request(|| visible.store(false, Ordering::Release));
                }
                3 => {
                    request_revision = revision
                        .request_with_focus_intent(
                            crate::visibility::RootFocusIntent::PreserveForeground,
                            || (),
                        )
                        .0;
                }
                4 => target.hwnd = 0,
                5 => root.clear(),
                _ => unreachable!(),
            }
            let (admitted, queued) = with_launcher_restore_queue_for_test(|| {
                restore_launcher_to_current_desktop_ordered(
                    target,
                    revision,
                    request_revision,
                    visible,
                    root,
                )
            });
            assert!(!admitted, "invalid ownership {invalid}");
            assert!(queued.is_empty());
            assert!(acceptance_trace::take_root_activation_test_events().is_empty());
            assert!(acceptance_trace::take_root_sample_requests_for_test().is_empty());
        }
    }

    #[test]
    fn native_queue_refusal_and_pending_replacement_emit_failed_not_completed() {
        let (target, revision, request_revision, visible, root) = activation_fixture();
        assert!(!submit_launcher_restore(
            target,
            revision.clone(),
            request_revision,
            visible.clone(),
            root.clone(),
            |request| enqueue_launcher_restore_with_queue(request, None)
        ));
        let refused_events = acceptance_trace::take_root_activation_test_events();
        assert!(
            matches!(refused_events.as_slice(), [Event::RootCommand { .. },
            Event::NativeActivation { edge: NativeActivationEdge::RestoreRequested, .. },
            Event::NativeActivation { edge: NativeActivationEdge::RestoreFailed, correlation, .. }] if correlation.terminal)
        );
        let queue = Arc::new(LauncherRestoreQueue::default());
        let first = prepare_launcher_restore(
            target,
            revision.clone(),
            request_revision,
            visible.clone(),
            root.clone(),
        )
        .unwrap();
        let first_id = first.correlation.request_id;
        assert!(enqueue_launcher_restore_with_queue(first, Some(&queue)));
        let next_revision = revision
            .request_with_focus_intent_and_invocation(
                crate::visibility::RootFocusIntent::ActivateRoot,
                Some(72),
                || (),
            )
            .0;
        let next =
            prepare_launcher_restore(target, revision, next_revision, visible, root).unwrap();
        let next_id = next.correlation.request_id;
        assert!(enqueue_launcher_restore_with_queue(next, Some(&queue)));
        assert!(acceptance_trace::take_root_activation_test_events().iter().any(|event| matches!(
            event, Event::NativeActivation { edge: NativeActivationEdge::RestoreFailed, correlation, .. }
                if correlation.request_id == first_id && correlation.terminal
        )));
        let admitted = queue.pending.lock().unwrap().take().unwrap();
        assert_eq!(admitted.correlation.request_id, next_id);
        assert_eq!(admitted.fence.request_revision, next_revision);
        let (result, effects) = crate::window_activation::activate_root_fixture_for_test(
            admitted.request,
            admitted.fence.clone(),
            false,
            false,
        );
        assert!(result.is_ok());
        assert_eq!(
            effects
                .iter()
                .filter(|effect| effect.as_str() == "set:42")
                .count(),
            1
        );
        complete_launcher_restore(&admitted, result);
        assert!(
            matches!(acceptance_trace::take_root_activation_test_events().as_slice(), [
            Event::NativeActivation { edge: NativeActivationEdge::RestoreCompleted, hwnd: 42, correlation }
        ] if correlation.request_id == next_id && correlation.terminal)
        );
    }

    #[test]
    fn fenced_backend_denial_and_supersession_cannot_emit_focus_completion() {
        for supersede in [false, true] {
            let (target, revision, request_revision, visible, root) = activation_fixture();
            let request =
                prepare_launcher_restore(target, revision, request_revision, visible, root)
                    .unwrap();
            let (result, effects) = crate::window_activation::activate_root_fixture_for_test(
                request.request,
                request.fence.clone(),
                !supersede,
                supersede,
            );
            let error = result.as_ref().unwrap_err();
            assert_eq!(
                error.kind,
                if supersede {
                    crate::window_activation::WindowActivationErrorKind::Superseded
                } else {
                    crate::window_activation::WindowActivationErrorKind::ForegroundDenied
                }
            );
            if supersede {
                assert!(!effects.iter().any(|effect| effect == "set:42"));
                assert!(!request.fence.visible.load(Ordering::Acquire));
            } else {
                assert!(effects.iter().any(|effect| effect == "set:42"));
                assert_eq!(
                    error.context.get("target_pid"),
                    Some(&request.fence.expected_process.to_string())
                );
            }
            complete_launcher_restore(&request, result);
            let events = acceptance_trace::take_root_activation_test_events();
            assert!(matches!(events.last(), Some(Event::NativeActivation {
                edge: NativeActivationEdge::RestoreFailed, correlation, ..
            }) if correlation.terminal && correlation.request_id == request.correlation.request_id));
            assert!(!events.iter().any(|event| matches!(
                event,
                Event::NativeActivation {
                    edge: NativeActivationEdge::RestoreCompleted,
                    ..
                }
            )));
        }
    }

    #[test]
    fn duplicate_current_root_submission_has_one_group_and_fresh_identity_remains_usable() {
        let (target, revision, request_revision, visible, root) = activation_fixture();
        let (_, queued) = with_launcher_restore_queue_for_test(|| {
            for _ in 0..3 {
                assert!(restore_launcher_to_current_desktop_ordered(
                    target,
                    revision.clone(),
                    request_revision,
                    visible.clone(),
                    root.clone()
                ));
            }
            root.set_identity_for_test(43);
            root.set_identity_for_test(42);
            assert!(!restore_launcher_to_current_desktop_ordered(
                target,
                revision.clone(),
                request_revision,
                visible.clone(),
                root.clone()
            ));
            let fresh = root.activation_target().unwrap();
            assert_ne!(fresh.generation, target.generation);
            assert!(restore_launcher_to_current_desktop_ordered(
                fresh,
                revision.clone(),
                request_revision,
                visible.clone(),
                root.clone()
            ));
        });
        assert_eq!(queued, [(request_revision, 42), (request_revision, 42)]);
        let events = acceptance_trace::take_root_activation_test_events();
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(
                    event,
                    Event::RootCommand {
                        command: crate::radial::acceptance_trace::RootCommandKind::Focus,
                        ..
                    }
                ))
                .count(),
            2
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(
                    event,
                    Event::NativeActivation {
                        edge: NativeActivationEdge::RestoreRequested,
                        ..
                    }
                ))
                .count(),
            2
        );
        assert!(!events.iter().any(|event| matches!(
            event,
            Event::NativeActivation {
                edge: NativeActivationEdge::RestoreCompleted,
                ..
            }
        )));
    }
}
