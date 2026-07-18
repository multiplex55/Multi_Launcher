use crate::multi_manager::model::MmRect;
use anyhow::{anyhow, Error};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedWindow {
    pub hwnd: usize,
    pub title: String,
    pub rect: MmRect,
    pub executable: String,
    pub class_name: String,
    pub process_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumeratedWindow {
    pub hwnd: usize,
    pub title: String,
    pub executable: String,
    pub class_name: String,
    pub process_path: String,
    pub rect: MmRect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowIdentitySnapshot {
    pub hwnd: usize,
    pub is_window: bool,
    pub live_title: String,
    pub process_path: String,
    pub executable: String,
    pub class_name: String,
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

fn executable_from_process_path(process_path: &str) -> Option<String> {
    process_path
        .rsplit(['\\', '/'])
        .next()
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(windows)]
pub(crate) fn capture_key_is_down(vk: u32) -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    unsafe { (GetAsyncKeyState(vk as i32) as u16 & 0x8000) != 0 }
}

#[cfg(not(windows))]
pub(crate) fn capture_key_is_down(_vk: u32) -> bool {
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
        executable: window_executable(hwnd_value).unwrap_or_default(),
        class_name: window_class_name(hwnd_value).unwrap_or_default(),
        process_path: window_process_path(hwnd_value).unwrap_or_default(),
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
pub fn window_class_name(hwnd: usize) -> Option<String> {
    use windows::Win32::UI::WindowsAndMessaging::GetClassNameW;

    let mut buffer = vec![0u16; 256];
    let copied = unsafe { GetClassNameW(hwnd_from_usize(hwnd), &mut buffer) };
    if copied <= 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buffer[..copied as usize]))
}

#[cfg(not(windows))]
pub fn window_class_name(_hwnd: usize) -> Option<String> {
    None
}

#[cfg(windows)]
pub fn window_process_path(hwnd: usize) -> Option<String> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;

    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

    let mut pid = 0u32;
    unsafe {
        let _ = GetWindowThreadProcessId(hwnd_from_usize(hwnd), Some(&mut pid));
    }
    if pid == 0 {
        return None;
    }

    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let mut buffer = vec![0u16; 1024];
    let mut size = buffer.len() as u32;
    let result = unsafe {
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_FORMAT(0),
            PWSTR(buffer.as_mut_ptr()),
            &mut size,
        )
    };
    let _ = unsafe { CloseHandle(handle) };

    if result.is_err() || size == 0 {
        return None;
    }

    Some(
        OsString::from_wide(&buffer[..size as usize])
            .to_string_lossy()
            .to_string(),
    )
}

#[cfg(not(windows))]
pub fn window_process_path(_hwnd: usize) -> Option<String> {
    None
}

#[cfg(windows)]
pub fn window_executable(hwnd: usize) -> Option<String> {
    window_process_path(hwnd).and_then(|process_path| executable_from_process_path(&process_path))
}

#[cfg(not(windows))]
pub fn window_executable(_hwnd: usize) -> Option<String> {
    None
}

pub fn query_hwnd_identity(hwnd: usize) -> WindowIdentitySnapshot {
    let is_window = is_valid_window(hwnd);
    let live_title = if is_window {
        window_title(hwnd).unwrap_or_default()
    } else {
        String::new()
    };
    let process_path = if is_window {
        window_process_path(hwnd).unwrap_or_default()
    } else {
        String::new()
    };
    let executable = if is_window {
        executable_from_process_path(&process_path)
            .or_else(|| window_executable(hwnd))
            .unwrap_or_default()
    } else {
        String::new()
    };
    let class_name = if is_window {
        window_class_name(hwnd).unwrap_or_default()
    } else {
        String::new()
    };

    WindowIdentitySnapshot {
        hwnd,
        is_window,
        live_title,
        process_path,
        executable,
        class_name,
    }
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
pub fn enumerate_top_level_windows() -> anyhow::Result<Vec<EnumeratedWindow>> {
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindow, GetWindowLongPtrW, IsWindow, IsWindowVisible, GWL_EXSTYLE,
        GW_OWNER, WS_EX_TOOLWINDOW,
    };

    struct Ctx {
        windows: Vec<EnumeratedWindow>,
    }

    fn hwnd_to_usize(hwnd: HWND) -> usize {
        hwnd.0 as usize
    }

    unsafe extern "system" fn enum_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let ctx = unsafe { &mut *(lparam.0 as *mut Ctx) };
        if hwnd.0.is_null() || !unsafe { IsWindow(hwnd) }.as_bool() {
            return BOOL(1);
        }
        if !unsafe { IsWindowVisible(hwnd) }.as_bool() {
            return BOOL(1);
        }
        if !unsafe { GetWindow(hwnd, GW_OWNER) }
            .unwrap_or_default()
            .0
            .is_null()
        {
            return BOOL(1);
        }
        let ex_style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32;
        if ex_style & WS_EX_TOOLWINDOW.0 != 0 {
            return BOOL(1);
        }

        let hwnd_value = hwnd_to_usize(hwnd);
        let Some(title) = window_title(hwnd_value).filter(|title| !title.trim().is_empty()) else {
            return BOOL(1);
        };
        let Some(rect) = window_rect(hwnd_value) else {
            return BOOL(1);
        };

        ctx.windows.push(EnumeratedWindow {
            hwnd: hwnd_value,
            title,
            executable: window_executable(hwnd_value).unwrap_or_default(),
            class_name: window_class_name(hwnd_value).unwrap_or_default(),
            process_path: window_process_path(hwnd_value).unwrap_or_default(),
            rect,
        });
        BOOL(1)
    }

    let mut ctx = Ctx {
        windows: Vec::new(),
    };
    unsafe {
        let ctx_ptr = &mut ctx as *mut Ctx;
        EnumWindows(Some(enum_cb), LPARAM(ctx_ptr as isize))
            .map_err(|err| anyhow!("failed to enumerate top-level windows: {err}"))?;
    }
    Ok(ctx.windows)
}

#[cfg(not(windows))]
pub fn enumerate_top_level_windows() -> anyhow::Result<Vec<EnumeratedWindow>> {
    Ok(Vec::new())
}

#[cfg(windows)]
pub fn move_window_to_rect(hwnd: usize, rect: MmRect) -> Result<(), MmWindowError> {
    use windows::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, IsIconic, SetForegroundWindow, SetWindowPos, ShowWindowAsync,
        HWND_NOTOPMOST, HWND_TOPMOST, SWP_SHOWWINDOW, SW_RESTORE,
    };

    if !is_valid_window(hwnd) {
        return Err(anyhow!("invalid window handle: {hwnd}"));
    }

    let hwnd = hwnd_from_usize(hwnd);
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindowAsync(hwnd, SW_RESTORE);
        }
        let _ = BringWindowToTop(hwnd);
        let _ = SetForegroundWindow(hwnd);

        SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            rect.x,
            rect.y,
            rect.w,
            rect.h,
            SWP_SHOWWINDOW,
        )
        .map_err(|err| anyhow!("failed to move window to rect as temporary topmost: {err}"))?;
        SetWindowPos(
            hwnd,
            HWND_NOTOPMOST,
            rect.x,
            rect.y,
            rect.w,
            rect.h,
            SWP_SHOWWINDOW,
        )
        .map_err(|err| anyhow!("failed to remove temporary topmost after move: {err}"))
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
    hotkey_pressed_with(sequence, capture_key_is_down)
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

    #[test]
    fn executable_from_process_path_handles_common_path_styles() {
        assert_eq!(
            executable_from_process_path(r"C:\Program Files\App\app.exe"),
            Some("app.exe".to_string())
        );
        assert_eq!(
            executable_from_process_path("/usr/bin/app"),
            Some("app".to_string())
        );
        assert_eq!(executable_from_process_path("app"), Some("app".to_string()));
        assert_eq!(executable_from_process_path(""), None);
        assert_eq!(executable_from_process_path(r"C:\Program Files\App\"), None);
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "Windows smoke test checklist: use a real, non-elevated application window HWND and verify minimized windows restore, the window is brought foreground, it is not left topmost, and elevated/inaccessible windows surface an error."]
    fn windows_move_window_to_rect_smoke_checklist() {
        // Manual checklist for move_window_to_rect:
        // 1. Capture a normal application HWND and minimize it.
        // 2. Call move_window_to_rect(hwnd, target_rect) and verify the window is restored.
        // 3. Verify the window is brought to the foreground/top for the move.
        // 4. Open another always-on-top candidate afterward and verify the moved window was
        //    returned to NOTOPMOST rather than remaining permanently topmost.
        // 5. Try an elevated/inaccessible target from a non-elevated launcher and verify the
        //    returned error is surfaced by the UI as multi_manager.move_window.
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

    #[cfg(not(windows))]
    #[test]
    fn non_windows_capture_and_query_stubs_are_safe() {
        assert!(active_window().is_none());
        assert!(enumerate_top_level_windows()
            .expect("non-Windows enumeration stub must succeed")
            .is_empty());
        assert!(window_rect(1).is_none());
        assert!(window_title(1).is_none());
        assert!(!is_valid_window(1));
        assert!(!is_window_at_rect(
            1,
            MmRect {
                x: 0,
                y: 0,
                w: 100,
                h: 100
            }
        ));
        assert!(!is_hotkey_pressed("Ctrl+Shift+A"));
        assert!(poll_capture_keys().is_none());
    }

    // Manual Windows smoke tests for embedded MultiManager:
    // 1. Capture a Notepad window into a workspace.
    // 2. Set and verify distinct home and target rectangles.
    // 3. Toggle the workspace with its configured hotkey.
    // 4. Rotate two or three captured windows through their slots.
    // 5. Close a captured window and verify recapture rejects/handles the invalid HWND safely.
    // 6. Save, restart the launcher, and verify the workspace and bindings load correctly.
    // 7. Verify launcher self-capture is rejected when that safety setting is enabled.

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
