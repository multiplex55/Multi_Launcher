use crate::draw::service::MonitorRect;

pub fn monitor_contains_point(rect: MonitorRect, point: (i32, i32)) -> bool {
    point.0 >= rect.x
        && point.0 < rect.x + rect.width
        && point.1 >= rect.y
        && point.1 < rect.y + rect.height
}

pub fn select_monitor_for_point(
    monitors: &[MonitorRect],
    point: (i32, i32),
) -> Option<MonitorRect> {
    monitors
        .iter()
        .copied()
        .find(|rect| monitor_contains_point(*rect, point))
}

pub fn resolve_monitor_from_cursor() -> Option<MonitorRect> {
    #[cfg(windows)]
    {
        let monitors = enumerate_monitors();
        let cursor = resolve_cursor_position()?;
        return select_monitor_for_point(&monitors, cursor).or_else(|| monitors.first().copied());
    }

    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(windows)]
fn resolve_cursor_position() -> Option<(i32, i32)> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

    let mut point = POINT::default();
    if unsafe { GetCursorPos(&mut point) }.is_ok() {
        Some((point.x, point.y))
    } else {
        None
    }
}

#[cfg(windows)]
fn enumerate_monitors() -> Vec<MonitorRect> {
    use std::mem;
    use windows::Win32::Foundation::{BOOL, LPARAM, RECT};
    use windows::Win32::Graphics::Gdi::{
        EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW,
    };

    extern "system" fn monitor_enum_proc(
        monitor: HMONITOR,
        _hdc: HDC,
        _rc_clip: *mut RECT,
        data: LPARAM,
    ) -> BOOL {
        let monitors = unsafe { &mut *(data.0 as *mut Vec<MonitorRect>) };
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = mem::size_of::<MONITORINFOEXW>() as u32;
        if unsafe { GetMonitorInfoW(monitor, &mut info.monitorInfo as *mut _ as *mut _) }.as_bool()
        {
            let rc = info.monitorInfo.rcMonitor;
            monitors.push(MonitorRect {
                x: rc.left,
                y: rc.top,
                width: rc.right - rc.left,
                height: rc.bottom - rc.top,
            });
        }
        BOOL(1)
    }

    let mut monitors = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(
            HDC::default(),
            None,
            Some(monitor_enum_proc),
            LPARAM(&mut monitors as *mut Vec<MonitorRect> as isize),
        );
    }
    monitors
}

#[cfg(test)]
mod tests {
    use super::{monitor_contains_point, select_monitor_for_point};
    use crate::draw::service::MonitorRect;

    #[test]
    fn cursor_positions_map_to_expected_monitor_rect() {
        let monitors = [
            MonitorRect {
                x: -1920,
                y: 0,
                width: 1920,
                height: 1080,
            },
            MonitorRect {
                x: 0,
                y: 0,
                width: 2560,
                height: 1440,
            },
        ];

        assert_eq!(
            select_monitor_for_point(&monitors, (-10, 100)),
            Some(monitors[0])
        );
        assert_eq!(
            select_monitor_for_point(&monitors, (200, 100)),
            Some(monitors[1])
        );
        assert!(monitor_contains_point(monitors[1], (2559, 1439)));
        assert!(!monitor_contains_point(monitors[1], (2560, 10)));
    }
}
