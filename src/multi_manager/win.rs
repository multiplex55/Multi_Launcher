use crate::multi_manager::model::MmRect;
use anyhow::{anyhow, Error};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedWindow {
    pub hwnd: usize,
    pub title: String,
    pub rect: MmRect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureKeyAction {
    Confirm,
    Skip,
    Cancel,
}

pub type MmWindowError = Error;

type KeyDownFn = fn(u32) -> bool;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedHotkey {
    key: u32,
    ctrl: bool,
    shift: bool,
    alt: bool,
    win: bool,
}

fn parse_hotkey(sequence: &str) -> Option<ParsedHotkey> {
    let mut key = None;
    let mut ctrl = false;
    let mut shift = false;
    let mut alt = false;
    let mut win = false;

    for part in sequence.split('+').map(str::trim).filter(|p| !p.is_empty()) {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => ctrl = true,
            "shift" => shift = true,
            "alt" => alt = true,
            "win" | "windows" | "meta" | "super" => win = true,
            _ => {
                if key.is_some() {
                    return None;
                }
                key = crate::window_manager::virtual_key_from_string(part);
                key?;
            }
        }
    }

    Some(ParsedHotkey {
        key: key?,
        ctrl,
        shift,
        alt,
        win,
    })
}

fn hotkey_pressed_with(sequence: &str, is_down: KeyDownFn) -> bool {
    let Some(parsed) = parse_hotkey(sequence) else {
        return false;
    };

    (!parsed.ctrl || is_down(0x11))
        && (!parsed.shift || is_down(0x10))
        && (!parsed.alt || is_down(0x12))
        && (!parsed.win || is_down(0x5B) || is_down(0x5C))
        && is_down(parsed.key)
}

#[cfg(windows)]
fn hwnd_from_usize(hwnd: usize) -> windows::Win32::Foundation::HWND {
    windows::Win32::Foundation::HWND(hwnd as *mut core::ffi::c_void)
}

#[cfg(windows)]
fn rect_from_win32(rect: windows::Win32::Foundation::RECT) -> MmRect {
    MmRect {
        x: rect.left,
        y: rect.top,
        w: rect.right - rect.left,
        h: rect.bottom - rect.top,
    }
}

#[cfg(windows)]
fn key_is_down(vk: u32) -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    unsafe { (GetAsyncKeyState(vk as i32) as u16 & 0x8000) != 0 }
}

#[cfg(not(windows))]
fn key_is_down(_vk: u32) -> bool {
    false
}

#[cfg(windows)]
pub fn active_window() -> Option<CapturedWindow> {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0.is_null() {
        return None;
    }
    let hwnd_value = hwnd.0 as usize;
    Some(CapturedWindow {
        hwnd: hwnd_value,
        title: window_title(hwnd_value)?,
        rect: window_rect(hwnd_value)?,
    })
}

#[cfg(not(windows))]
pub fn active_window() -> Option<CapturedWindow> {
    None
}

#[cfg(windows)]
pub fn window_rect(hwnd: usize) -> Option<MmRect> {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;

    let mut rect = RECT::default();
    unsafe { GetWindowRect(hwnd_from_usize(hwnd), &mut rect) }
        .ok()
        .map(|()| rect_from_win32(rect))
}

#[cfg(not(windows))]
pub fn window_rect(_hwnd: usize) -> Option<MmRect> {
    None
}

#[cfg(windows)]
pub fn window_title(hwnd: usize) -> Option<String> {
    use windows::Win32::UI::WindowsAndMessaging::{GetWindowTextLengthW, GetWindowTextW};

    let hwnd = hwnd_from_usize(hwnd);
    let len = unsafe { GetWindowTextLengthW(hwnd) };
    if len <= 0 {
        return None;
    }
    let mut buffer = vec![0u16; len as usize + 1];
    let copied = unsafe { GetWindowTextW(hwnd, &mut buffer) };
    if copied <= 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buffer[..copied as usize]))
}

#[cfg(not(windows))]
pub fn window_title(_hwnd: usize) -> Option<String> {
    None
}

#[cfg(windows)]
pub fn is_valid_window(hwnd: usize) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::IsWindow;
    hwnd != 0 && unsafe { IsWindow(hwnd_from_usize(hwnd)).as_bool() }
}

#[cfg(not(windows))]
pub fn is_valid_window(_hwnd: usize) -> bool {
    false
}

#[cfg(windows)]
pub fn move_window_to_rect(hwnd: usize, rect: MmRect) -> Result<(), MmWindowError> {
    use windows::Win32::UI::WindowsAndMessaging::{
        IsIconic, MoveWindow, ShowWindowAsync, SW_RESTORE,
    };

    if !is_valid_window(hwnd) {
        return Err(anyhow!("invalid window handle: {hwnd}"));
    }

    let hwnd = hwnd_from_usize(hwnd);
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindowAsync(hwnd, SW_RESTORE);
        }
        MoveWindow(hwnd, rect.x, rect.y, rect.w, rect.h, true)
            .map_err(|err| anyhow!("failed to move window to rect: {err}"))
    }
}

#[cfg(not(windows))]
pub fn move_window_to_rect(_hwnd: usize, _rect: MmRect) -> Result<(), MmWindowError> {
    Err(anyhow!(
        "MultiManager window movement is unsupported on this platform"
    ))
}

pub fn is_window_at_rect(hwnd: usize, rect: MmRect) -> bool {
    window_rect(hwnd) == Some(rect)
}

pub fn is_hotkey_pressed(sequence: &str) -> bool {
    hotkey_pressed_with(sequence, key_is_down)
}

pub fn poll_capture_keys() -> Option<CaptureKeyAction> {
    if is_hotkey_pressed("Enter") {
        Some(CaptureKeyAction::Confirm)
    } else if is_hotkey_pressed("S") {
        Some(CaptureKeyAction::Skip)
    } else if is_hotkey_pressed("Escape") {
        Some(CaptureKeyAction::Cancel)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn down(vk: u32) -> bool {
        matches!(vk, 0x11 | 0x10 | 0x41 | 0x70)
    }

    #[test]
    fn supported_hotkey_strings_map_to_expected_key_checks() {
        assert!(hotkey_pressed_with("Ctrl+Shift+A", down));
        assert!(hotkey_pressed_with("F1", down));
        assert_eq!(parse_hotkey("Alt+Enter").map(|h| h.key), Some(0x0D));
        assert_eq!(parse_hotkey("Win+S").map(|h| h.key), Some(0x53));
    }

    #[test]
    fn unsupported_hotkey_strings_return_false_instead_of_panicking() {
        assert!(!hotkey_pressed_with("Ctrl+NoSuchKey", down));
        assert!(!hotkey_pressed_with("Ctrl+A+B", down));
        assert!(!hotkey_pressed_with("Ctrl+", down));
    }

    #[cfg(not(windows))]
    #[test]
    fn non_windows_movement_stub_returns_error() {
        let err = move_window_to_rect(
            1,
            MmRect {
                x: 0,
                y: 0,
                w: 100,
                h: 100,
            },
        )
        .expect_err("non-Windows movement must fail");
        assert!(err.to_string().contains("unsupported"));
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "manual Win32 smoke test: focus a normal window before running"]
    fn manual_active_window_capture() {
        assert!(active_window().is_some());
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "manual Win32 smoke test: focus a normal window before running"]
    fn manual_rect_capture() {
        let captured = active_window().expect("active window");
        assert!(window_rect(captured.hwnd).is_some());
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "manual Win32 smoke test: minimize a safe test window and pass its HWND by adapting this test"]
    fn manual_move_minimized_window() {
        let captured = active_window().expect("active window");
        move_window_to_rect(captured.hwnd, captured.rect)
            .expect("move minimized or restored window");
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "manual Win32 smoke test: hold Enter, S, or Escape while running"]
    fn manual_enter_s_escape_capture_polling() {
        let _ = poll_capture_keys();
    }
}
