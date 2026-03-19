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
