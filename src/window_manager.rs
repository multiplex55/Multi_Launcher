use once_cell::sync::Lazy;
use std::sync::Mutex;

static MOCK_MOUSE_POSITION: Lazy<Mutex<Option<Option<(f32, f32)>>>> =
    Lazy::new(|| Mutex::new(None));

#[cfg_attr(not(test), allow(dead_code))]
pub static MOCK_MOUSE_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[cfg_attr(not(test), allow(dead_code))]
pub fn set_mock_mouse_position(pos: Option<(f32, f32)>) {
    if let Ok(mut guard) = MOCK_MOUSE_POSITION.lock() {
        *guard = Some(pos);
    } else {
        tracing::error!("failed to lock MOCK_MOUSE_POSITION");
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn clear_mock_mouse_position() {
    if let Ok(mut guard) = MOCK_MOUSE_POSITION.lock() {
        *guard = None;
    } else {
        tracing::error!("failed to lock MOCK_MOUSE_POSITION");
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn mock_mouse_position_is_set() -> bool {
    if let Ok(guard) = MOCK_MOUSE_POSITION.lock() {
        guard.is_some()
    } else {
        tracing::error!("failed to lock MOCK_MOUSE_POSITION");
        false
    }
}

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

/// Return the current mouse position in screen coordinates.
pub fn current_mouse_position() -> Option<(f32, f32)> {
    if let Ok(guard) = MOCK_MOUSE_POSITION.lock() {
        if let Some(pos) = *guard {
            return pos;
        }
    } else {
        tracing::error!("failed to lock MOCK_MOUSE_POSITION");
    }

    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let mut pt = POINT::default();
    if unsafe { GetCursorPos(&mut pt).is_ok() } {
        Some((pt.x as f32, pt.y as f32))
    } else {
        None
    }
}
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

#[cfg(windows)]
mod virtual_desktop {
    use windows::core::{IUnknown, IUnknown_Vtbl, Interface, GUID, HRESULT, HSTRING};
    use windows::Win32::UI::Shell::Common::IObjectArray;

    #[repr(transparent)]
    #[derive(Clone, PartialEq, Eq)]
    pub struct IVirtualDesktop(pub IUnknown);

    unsafe impl Interface for IVirtualDesktop {
        type Vtable = IVirtualDesktop_Vtbl;
        const IID: GUID = GUID::from_u128(0xff72ffdd_be7e_43fc_9c03_ad81681e88e4);
    }

    #[repr(C)]
    #[allow(non_snake_case)]
    pub struct IVirtualDesktop_Vtbl {
        pub base__: IUnknown_Vtbl,
        pub IsViewVisible: unsafe extern "system" fn(
            *mut core::ffi::c_void,
            *mut core::ffi::c_void,
            *mut i32,
        ) -> HRESULT,
        pub GetID: unsafe extern "system" fn(*mut core::ffi::c_void, *mut GUID) -> HRESULT,
        pub Proc5: unsafe extern "system" fn(*mut core::ffi::c_void) -> HRESULT,
        pub GetName: unsafe extern "system" fn(*mut core::ffi::c_void, *mut HSTRING) -> HRESULT,
        pub GetWallpaperPath:
            unsafe extern "system" fn(*mut core::ffi::c_void, *mut HSTRING) -> HRESULT,
    }

    impl IVirtualDesktop {
        pub unsafe fn get_id(&self) -> windows::core::Result<GUID> {
            let mut result = GUID::zeroed();
            (Interface::vtable(self).GetID)(Interface::as_raw(self), &mut result).map(|| result)
        }

        pub unsafe fn get_name(&self) -> windows::core::Result<HSTRING> {
            let mut result = HSTRING::new();
            (Interface::vtable(self).GetName)(Interface::as_raw(self), &mut result).map(|| result)
        }
    }

    #[repr(transparent)]
    #[derive(Clone, PartialEq, Eq)]
    pub struct IVirtualDesktopManagerInternal(pub IUnknown);

    unsafe impl Interface for IVirtualDesktopManagerInternal {
        type Vtable = IVirtualDesktopManagerInternal_Vtbl;
        const IID: GUID = GUID::from_u128(0xf31574d6_b682_4cdc_bd56_1827860abec6);
    }

    #[repr(C)]
    #[allow(non_snake_case)]
    pub struct IVirtualDesktopManagerInternal_Vtbl {
        pub base__: IUnknown_Vtbl,
        pub GetCount: unsafe extern "system" fn(*mut core::ffi::c_void, isize, *mut i32) -> HRESULT,
        pub MoveViewToDesktop: unsafe extern "system" fn(
            *mut core::ffi::c_void,
            *mut core::ffi::c_void,
            *mut core::ffi::c_void,
        ) -> HRESULT,
        pub CanViewMoveDesktops: unsafe extern "system" fn(
            *mut core::ffi::c_void,
            *mut core::ffi::c_void,
            *mut i32,
        ) -> HRESULT,
        pub GetCurrentDesktop: unsafe extern "system" fn(
            *mut core::ffi::c_void,
            isize,
            *mut *mut core::ffi::c_void,
        ) -> HRESULT,
        pub GetDesktops: unsafe extern "system" fn(
            *mut core::ffi::c_void,
            isize,
            *mut *mut core::ffi::c_void,
        ) -> HRESULT,
        pub GetAdjacentDesktop: unsafe extern "system" fn(
            *mut core::ffi::c_void,
            *mut core::ffi::c_void,
            i32,
            *mut *mut core::ffi::c_void,
        ) -> HRESULT,
        pub SwitchDesktop: unsafe extern "system" fn(
            *mut core::ffi::c_void,
            isize,
            *mut core::ffi::c_void,
        ) -> HRESULT,
        pub CreateDesktop: unsafe extern "system" fn(
            *mut core::ffi::c_void,
            isize,
            *mut *mut core::ffi::c_void,
        ) -> HRESULT,
        pub MoveDesktop: unsafe extern "system" fn(
            *mut core::ffi::c_void,
            *mut core::ffi::c_void,
            isize,
            i32,
        ) -> HRESULT,
        pub RemoveDesktop: unsafe extern "system" fn(
            *mut core::ffi::c_void,
            *mut core::ffi::c_void,
            *mut core::ffi::c_void,
        ) -> HRESULT,
        pub FindDesktop: unsafe extern "system" fn(
            *mut core::ffi::c_void,
            *const GUID,
            *mut *mut core::ffi::c_void,
        ) -> HRESULT,
        pub GetDesktopSwitchIncludeExcludeViews: unsafe extern "system" fn(
            *mut core::ffi::c_void,
            *mut core::ffi::c_void,
            *mut *mut core::ffi::c_void,
            *mut *mut core::ffi::c_void,
        ) -> HRESULT,
        pub SetDesktopName: unsafe extern "system" fn(
            *mut core::ffi::c_void,
            *mut core::ffi::c_void,
            HSTRING,
        ) -> HRESULT,
        pub SetDesktopWallpaper: unsafe extern "system" fn(
            *mut core::ffi::c_void,
            *mut core::ffi::c_void,
            HSTRING,
        ) -> HRESULT,
        pub UpdateWallpaperPathForAllDesktops:
            unsafe extern "system" fn(*mut core::ffi::c_void, HSTRING) -> HRESULT,
        pub CopyDesktopState: unsafe extern "system" fn(
            *mut core::ffi::c_void,
            *mut core::ffi::c_void,
            *mut core::ffi::c_void,
        ) -> HRESULT,
        pub GetDesktopIsPerMonitor:
            unsafe extern "system" fn(*mut core::ffi::c_void, *mut i32) -> HRESULT,
        pub SetDesktopIsPerMonitor:
            unsafe extern "system" fn(*mut core::ffi::c_void, i32) -> HRESULT,
    }

    impl IVirtualDesktopManagerInternal {
        pub unsafe fn get_desktops(
            &self,
            hwnd_or_mon: isize,
        ) -> windows::core::Result<IObjectArray> {
            let mut result = core::ptr::null_mut();
            (Interface::vtable(self).GetDesktops)(Interface::as_raw(self), hwnd_or_mon, &mut result)
                .and_then(|| windows::core::Type::from_abi(result))
        }
    }
}

/// Ensure the given window resides on the active virtual desktop.
///
/// This uses the `IVirtualDesktopManager` COM interface to check if `hwnd`
/// already belongs to the current desktop. If not, it is moved to the desktop
/// of the foreground window.
pub fn move_to_current_desktop(hwnd: windows::Win32::Foundation::HWND) {
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::{IVirtualDesktopManager, VirtualDesktopManager};
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        if let Ok(vdm) =
            CoCreateInstance::<_, IVirtualDesktopManager>(&VirtualDesktopManager, None, CLSCTX_ALL)
        {
            if let Ok(on_current) = vdm.IsWindowOnCurrentVirtualDesktop(hwnd) {
                if !on_current.as_bool() {
                    if let Ok(desktop) = vdm.GetWindowDesktopId(GetForegroundWindow()) {
                        let _ = vdm.MoveWindowToDesktop(hwnd, &desktop);
                    }
                }
            }
        }
        CoUninitialize();
    }
}

#[cfg(windows)]
fn format_desktop_id(desktop_id: &windows::core::GUID) -> String {
    format!(
        "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        desktop_id.data1,
        desktop_id.data2,
        desktop_id.data3,
        desktop_id.data4[0],
        desktop_id.data4[1],
        desktop_id.data4[2],
        desktop_id.data4[3],
        desktop_id.data4[4],
        desktop_id.data4[5],
        desktop_id.data4[6],
        desktop_id.data4[7]
    )
}

#[cfg(windows)]
fn parse_desktop_id(value: &str) -> Option<windows::core::GUID> {
    let trimmed = value.trim();
    if trimmed.len() != 36 {
        return None;
    }
    let is_valid = trimmed.chars().enumerate().all(|(idx, ch)| match idx {
        8 | 13 | 18 | 23 => ch == '-',
        _ => ch.is_ascii_hexdigit(),
    });
    if !is_valid {
        return None;
    }
    Some(windows::core::GUID::from(trimmed))
}

#[cfg(windows)]
pub fn resolve_virtual_desktop_name(desktop_id: &windows::core::GUID) -> Option<String> {
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
    };

    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let manager = CoCreateInstance::<_, virtual_desktop::IVirtualDesktopManagerInternal>(
            &windows::core::GUID::from_u128(0xc5e0cdca_7b6e_41b2_9fc4_d93975cc467b),
            None,
            CLSCTX_ALL,
        )
        .ok();
        let name = manager.and_then(|manager| {
            let desktops = manager.get_desktops(0).ok()?;
            let count = desktops.GetCount().ok()?;
            for idx in 0..count {
                let desktop: virtual_desktop::IVirtualDesktop = desktops.GetAt(idx).ok()?;
                let id = desktop.get_id().ok()?;
                if &id == desktop_id {
                    if let Ok(name) = desktop.get_name() {
                        let name = name.to_string_lossy();
                        let trimmed = name.trim();
                        if !trimmed.is_empty() {
                            return Some(trimmed.to_string());
                        }
                    }
                    break;
                }
            }
            None
        });
        CoUninitialize();
        name
    }
}

#[cfg(windows)]
fn resolve_virtual_desktop_id_by_name(name: &str) -> Option<windows::core::GUID> {
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
    };

    let trimmed = name.trim();
    if trimmed.is_empty() {
        return None;
    }

    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let manager = CoCreateInstance::<_, virtual_desktop::IVirtualDesktopManagerInternal>(
            &windows::core::GUID::from_u128(0xc5e0cdca_7b6e_41b2_9fc4_d93975cc467b),
            None,
            CLSCTX_ALL,
        )
        .ok();
        let resolved = manager.and_then(|manager| {
            let desktops = manager.get_desktops(0).ok()?;
            let count = desktops.GetCount().ok()?;
            for idx in 0..count {
                let desktop: virtual_desktop::IVirtualDesktop = desktops.GetAt(idx).ok()?;
                let id = desktop.get_id().ok()?;
                if let Ok(name) = desktop.get_name() {
                    let current = name.to_string_lossy();
                    if current.trim().eq_ignore_ascii_case(trimmed) {
                        return Some(id);
                    }
                }
            }
            None
        });
        CoUninitialize();
        resolved
    }
}

#[cfg(windows)]
pub fn window_desktop_label(hwnd: windows::Win32::Foundation::HWND) -> Option<String> {
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::{IVirtualDesktopManager, VirtualDesktopManager};

    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let manager =
            CoCreateInstance::<_, IVirtualDesktopManager>(&VirtualDesktopManager, None, CLSCTX_ALL)
                .ok();
        let desktop_id = manager.and_then(|manager| manager.GetWindowDesktopId(hwnd).ok());
        let label = desktop_id.map(|desktop_id| {
            resolve_virtual_desktop_name(&desktop_id)
                .unwrap_or_else(|| format_desktop_id(&desktop_id))
        });
        CoUninitialize();
        label
    }
}

#[cfg(windows)]
pub fn move_window_to_desktop(hwnd: windows::Win32::Foundation::HWND, target: &str) -> bool {
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::{IVirtualDesktopManager, VirtualDesktopManager};

    let target_id = parse_desktop_id(target).or_else(|| resolve_virtual_desktop_id_by_name(target));
    let Some(target_id) = target_id else {
        return false;
    };

    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let manager =
            CoCreateInstance::<_, IVirtualDesktopManager>(&VirtualDesktopManager, None, CLSCTX_ALL)
                .ok();
        let moved = manager
            .and_then(|manager| {
                let current = manager.GetWindowDesktopId(hwnd).ok()?;
                if current == target_id {
                    return Some(true);
                }
                manager.MoveWindowToDesktop(hwnd, &target_id).ok()?;
                Some(true)
            })
            .unwrap_or(false);
        CoUninitialize();
        moved
    }
}

/// On Windows, restore the window and bring it to the foreground.
pub fn force_restore_and_foreground(hwnd: windows::Win32::Foundation::HWND) {
    use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId, SetForegroundWindow, ShowWindowAsync,
        SW_RESTORE,
    };
    unsafe {
        move_to_current_desktop(hwnd);
        let fg_hwnd = GetForegroundWindow();
        let fg_thread = GetWindowThreadProcessId(fg_hwnd, None);
        let current_thread = GetCurrentThreadId();

        tracing::debug!("Forcing window restore and foreground");
        let _ = ShowWindowAsync(hwnd, SW_RESTORE);

        let _ = AttachThreadInput(fg_thread, current_thread, true);
        let fg_success = SetForegroundWindow(hwnd).as_bool();
        let _ = AttachThreadInput(fg_thread, current_thread, false);

        tracing::debug!("SetForegroundWindow success: {fg_success}");
    }
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
        EnumWindows, GetWindow, GetWindowThreadProcessId, IsWindowVisible, GW_OWNER,
    };
    unsafe extern "system" fn enum_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let target = lparam.0 as u32;
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == target
            && IsWindowVisible(hwnd).as_bool()
            && GetWindow(hwnd, GW_OWNER).unwrap_or_default().0.is_null()
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

pub fn activate_window(hwnd: usize) {
    use windows::Win32::Foundation::HWND;
    crate::window_manager::force_restore_and_foreground(HWND(hwnd as *mut _));
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
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
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
                    dwExtraInfo: 0,
                },
            },
        };
        let _ = SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
        input.Anonymous.ki.dwFlags = KEYEVENTF_KEYUP;
        let _ = SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
    }
}
