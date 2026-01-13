use crate::plugins::layouts_storage::{LayoutMatch, LayoutPlacement, LayoutWindow};

#[derive(Debug, Clone, Copy, Default)]
pub struct LayoutWindowOptions {
    pub only_active_monitor: bool,
    pub include_minimized: bool,
    pub exclude_minimized: bool,
}

#[cfg(windows)]
pub fn collect_layout_windows(options: LayoutWindowOptions) -> anyhow::Result<Vec<LayoutWindow>> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use std::path::Path;

    use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, HMONITOR, MONITORINFOEXW, MONITOR_DEFAULTTONEAREST,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetForegroundWindow, GetWindow, GetWindowLongPtrW,
        GetWindowPlacement, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
        IsWindowVisible, GWL_EXSTYLE, GW_OWNER, SW_MINIMIZE, SW_SHOWMINIMIZED, WINDOWPLACEMENT,
        WS_EX_TOOLWINDOW,
    };
    use windows_core::PWSTR;

    struct Ctx {
        options: LayoutWindowOptions,
        active_monitor: Option<HMONITOR>,
        windows: Vec<LayoutWindow>,
    }

    fn wide_to_string(value: &[u16]) -> String {
        let nul = value.iter().position(|c| *c == 0).unwrap_or(value.len());
        String::from_utf16_lossy(&value[..nul])
    }

    fn window_exe_path(hwnd: HWND) -> Option<String> {
        unsafe {
            let mut pid = 0u32;
            let _ = GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == 0 {
                return None;
            }
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buffer = vec![0u16; 1024];
            let mut size = buffer.len() as u32;
            let success = QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_FORMAT(0),
                PWSTR(buffer.as_mut_ptr()),
                &mut size,
            )
            .is_ok();
            drop(handle);
            if !success || size == 0 {
                return None;
            }
            Some(OsString::from_wide(&buffer[..size as usize]).to_string_lossy().to_string())
        }
    }

    fn window_process_name(path: &str) -> Option<String> {
        Path::new(path)
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
    }

    fn monitor_info(hwnd: HWND) -> Option<(HMONITOR, MONITORINFOEXW)> {
        unsafe {
            let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
            if monitor.0.is_null() {
                return None;
            }
            let mut info = MONITORINFOEXW::default();
            info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
            if GetMonitorInfoW(monitor, &mut info as *mut _ as *mut _).as_bool() {
                Some((monitor, info))
            } else {
                None
            }
        }
    }

    unsafe extern "system" fn enum_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let ctx = &mut *(lparam.0 as *mut Ctx);
        if !IsWindowVisible(hwnd).as_bool() {
            return BOOL(1);
        }
        if !GetWindow(hwnd, GW_OWNER).unwrap_or_default().0.is_null() {
            return BOOL(1);
        }
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        if ex_style & WS_EX_TOOLWINDOW.0 != 0 {
            return BOOL(1);
        }

        let title_len = GetWindowTextLengthW(hwnd);
        if title_len <= 0 {
            return BOOL(1);
        }
        let mut title_buf = vec![0u16; title_len as usize + 1];
        let title_read = GetWindowTextW(hwnd, &mut title_buf);
        if title_read == 0 {
            return BOOL(1);
        }
        let title = String::from_utf16_lossy(&title_buf[..title_read as usize]);
        if title.trim().is_empty() {
            return BOOL(1);
        }

        let (monitor, monitor_info) = match monitor_info(hwnd) {
            Some(info) => info,
            None => return BOOL(1),
        };
        if let Some(active) = ctx.active_monitor {
            if active != monitor {
                return BOOL(1);
            }
        }

        let mut placement = WINDOWPLACEMENT::default();
        placement.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
        if GetWindowPlacement(hwnd, &mut placement).is_err() {
            return BOOL(1);
        }
        let minimized = placement.showCmd == SW_SHOWMINIMIZED.0 as u32
            || placement.showCmd == SW_MINIMIZE.0 as u32;
        let allow_minimized = if ctx.options.include_minimized {
            true
        } else if ctx.options.exclude_minimized {
            false
        } else {
            false
        };
        if minimized && !allow_minimized {
            return BOOL(1);
        }

        let work_area = monitor_info.monitorInfo.rcWork;
        let work_width = (work_area.right - work_area.left) as f32;
        let work_height = (work_area.bottom - work_area.top) as f32;
        if work_width <= 0.0 || work_height <= 0.0 {
            return BOOL(1);
        }

        let rect = placement.rcNormalPosition;
        let rect_width = (rect.right - rect.left) as f32;
        let rect_height = (rect.bottom - rect.top) as f32;
        let x = (rect.left - work_area.left) as f32 / work_width;
        let y = (rect.top - work_area.top) as f32 / work_height;
        let w = rect_width / work_width;
        let h = rect_height / work_height;

        let mut class_buf = vec![0u16; 256];
        let class_len = GetClassNameW(hwnd, &mut class_buf) as usize;
        let class = if class_len > 0 {
            Some(wide_to_string(&class_buf[..class_len]))
        } else {
            None
        };

        let exe_path = window_exe_path(hwnd);
        let process = exe_path
            .as_deref()
            .and_then(window_process_name)
            .filter(|name| !name.is_empty());

        let matcher = LayoutMatch {
            app_id: exe_path.clone(),
            title: Some(title),
            class,
            process,
        };

        let monitor_name = wide_to_string(&monitor_info.szDevice);
        let placement = LayoutPlacement {
            rect: [x, y, w, h],
            monitor: if monitor_name.is_empty() {
                None
            } else {
                Some(monitor_name)
            },
        };
        ctx.windows.push(LayoutWindow { matcher, placement });
        BOOL(1)
    }

    let active_monitor = if options.only_active_monitor {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.0.is_null() {
                None
            } else {
                Some(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST))
            }
        }
    } else {
        None
    };

    let mut ctx = Ctx { options, active_monitor, windows: Vec::new() };
    unsafe {
        let ctx_ptr = &mut ctx as *mut Ctx;
        let _ = EnumWindows(Some(enum_cb), LPARAM(ctx_ptr as isize));
    }
    Ok(ctx.windows)
}

#[cfg(not(windows))]
pub fn collect_layout_windows(_options: LayoutWindowOptions) -> anyhow::Result<Vec<LayoutWindow>> {
    Ok(Vec::new())
}
