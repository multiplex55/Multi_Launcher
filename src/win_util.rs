#[cfg(target_os = "windows")]
use windows::Win32::Foundation::HWND;
#[cfg(target_os = "windows")]
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

#[cfg(target_os = "windows")]
pub fn force_restore_and_foreground(hwnd: HWND) {
    use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SetForegroundWindow, SW_RESTORE};
    unsafe {
        let _ = ShowWindow(hwnd, SW_RESTORE);
        let _ = SetForegroundWindow(hwnd);
    }
}

#[cfg(not(target_os = "windows"))]
pub fn force_restore_and_foreground<T>(_hwnd: T) {}

#[cfg(target_os = "windows")]
pub fn get_hwnd(frame: &eframe::Frame) -> Option<HWND> {
    frame
        .window_handle()
        .ok()
        .and_then(|wh| match wh.as_raw() {
            RawWindowHandle::Win32(handle) => {
                Some(HWND(handle.hwnd.get() as *mut core::ffi::c_void))
            }
            _ => None,
        })
}

#[cfg(not(target_os = "windows"))]
pub fn get_hwnd(_frame: &eframe::Frame) -> Option<()> {
    None
}

