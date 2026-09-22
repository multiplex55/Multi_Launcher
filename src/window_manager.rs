pub use crate::platform::windows_api::{
    MOCK_MOUSE_LOCK, clear_mock_mouse_position, current_mouse_position, mock_mouse_position_is_set,
    set_mock_mouse_position,
};

use crate::radial::acceptance_trace::{
    self, Correlation, Event, NativeActivationEdge, NativeWindowIdentity,
};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_RESTORE_TRACE_ID: AtomicU64 = AtomicU64::new(1);

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
pub fn restore_launcher_to_current_desktop(hwnd: windows::Win32::Foundation::HWND) {
    let request =
        crate::window_activation::WindowActivationRequest::move_to_current_desktop(hwnd.0 as usize);
    let trace_enabled = acceptance_trace::enabled();
    let trace_hwnd_value = hwnd.0 as usize;
    let trace_hwnd = trace_hwnd_value as u64;
    let correlation = if trace_enabled {
        let trace_id = NEXT_RESTORE_TRACE_ID.fetch_add(1, Ordering::Relaxed);
        Correlation {
            request_id: trace_id,
            request_kind: Default::default(),
            session_id: 0,
            generation: trace_id,
            terminal: false,
        }
    } else {
        Correlation::default()
    };
    if trace_enabled {
        acceptance_trace::emit(Event::NativeActivation {
            edge: NativeActivationEdge::RestoreRequested,
            hwnd: trace_hwnd,
            correlation,
        });
    }
    // Desktop transitions and foreground verification use bounded backoff. Keep that work off
    // egui's render path so a slow or policy-blocked target cannot stall a frame.
    std::thread::spawn(move || {
        if let Err(error) = crate::window_activation::activate_window(request) {
            if trace_enabled {
                let terminal_correlation = Correlation {
                    terminal: true,
                    ..correlation
                };
                acceptance_trace::emit(Event::NativeActivation {
                    edge: NativeActivationEdge::RestoreFailed,
                    hwnd: trace_hwnd,
                    correlation: terminal_correlation,
                });
                emit_window_snapshot(
                    windows::Win32::Foundation::HWND(trace_hwnd_value as *mut _),
                    terminal_correlation,
                );
            }
            tracing::warn!(error = %error, "failed to restore launcher window");
        } else if trace_enabled {
            let terminal_correlation = Correlation {
                terminal: true,
                ..correlation
            };
            acceptance_trace::emit(Event::NativeActivation {
                edge: NativeActivationEdge::RestoreCompleted,
                hwnd: trace_hwnd,
                correlation: terminal_correlation,
            });
            emit_window_snapshot(
                windows::Win32::Foundation::HWND(trace_hwnd_value as *mut _),
                terminal_correlation,
            );
        }
    });
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
    use windows::Win32::UI::WindowsAndMessaging::{GetWindowRect, IsIconic, IsWindowVisible};

    acceptance_trace::register_root_hwnd(hwnd.0 as usize as u64);

    let mut rect = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut rect) }.is_err() {
        return;
    }
    acceptance_trace::emit(Event::NativeWindowSnapshot {
        hwnd: hwnd.0 as usize as u64,
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

pub fn send_end_key() {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput,
        VIRTUAL_KEY, VK_END,
    };
    unsafe {
        let mut input = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(VK_END.0),
                    wScan: 0,
                    dwFlags: KEYBD_EVENT_FLAGS(0),
                    time: 0,
                    dwExtraInfo: crate::hotkey::launcher_invocation::MULTI_LAUNCHER_INJECT_TAG,
                },
            },
        };
        let _ = SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
        input.Anonymous.ki.dwFlags = KEYEVENTF_KEYUP;
        let _ = SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
    }
}
