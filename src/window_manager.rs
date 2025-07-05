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
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Foundation::POINT;
        use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
        let mut pt = POINT::default();
        if unsafe { GetCursorPos(&mut pt).is_ok() } {
            Some((pt.x as f32, pt.y as f32))
        } else {
            Some((0.0, 0.0))
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        use std::ptr;
        use x11::xlib;
        unsafe {
            let display = xlib::XOpenDisplay(ptr::null());
            if display.is_null() {
                return Some((0.0, 0.0));
            }
            let root = xlib::XDefaultRootWindow(display);
            let mut root_ret = 0;
            let mut child_ret = 0;
            let mut root_x = 0;
            let mut root_y = 0;
            let mut win_x = 0;
            let mut win_y = 0;
            let mut mask = 0;
            let status = xlib::XQueryPointer(
                display,
                root,
                &mut root_ret,
                &mut child_ret,
                &mut root_x,
                &mut root_y,
                &mut win_x,
                &mut win_y,
                &mut mask,
            );
            xlib::XCloseDisplay(display);
            if status == 0 {
                Some((0.0, 0.0))
            } else {
                Some((root_x as f32, root_y as f32))
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        use core_graphics::event::{CGEvent, CGEventSource};
        use core_graphics::event_source::CGEventSourceStateID;
        let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState).ok();
        if let Some(source) = source {
            if let Ok(event) = CGEvent::new(source) {
                let loc = event.location();
                return Some((loc.x as f32, loc.y as f32));
            }
        }
        Some((0.0, 0.0))
    }

    #[cfg(not(any(target_os = "windows", unix)))]
    {
        Some((0.0, 0.0))
    }
}

#[cfg(target_os = "windows")]
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
#[cfg(target_os = "windows")]
use raw_window_handle::HasWindowHandle;

/// On Windows, restore the window and bring it to the foreground.
#[cfg(target_os = "windows")]
pub fn force_restore_and_foreground(hwnd: windows::Win32::Foundation::HWND) {
    use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SetForegroundWindow, SW_RESTORE};
    unsafe {
        ShowWindow(hwnd, SW_RESTORE);
        SetForegroundWindow(hwnd);
    }
}

/// Extract the HWND from an eframe [`Frame`].
#[cfg(target_os = "windows")]
pub fn get_hwnd(frame: &eframe::Frame) -> Option<windows::Win32::Foundation::HWND> {
    if let Ok(handle) = frame.window_handle() {
        match handle.raw_window_handle() {
            RawWindowHandle::Win32(h) =>
                Some(windows::Win32::Foundation::HWND(h.hwnd.cast())),
            _ => None,
        }
    } else {
        None
    }
}
