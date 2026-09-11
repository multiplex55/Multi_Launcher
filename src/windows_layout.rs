pub mod apply;
pub mod plan;

use crate::plugins::layouts_storage::{
    Layout, LayoutMatch, LayoutPlacement, LayoutWindow, LayoutWindowState,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct LayoutWindowOptions {
    pub only_active_monitor: bool,
    pub include_minimized: bool,
    pub exclude_minimized: bool,
}

#[derive(Debug, Clone)]
pub struct LayoutRestoreSummaryEntry {
    pub saved_matcher: LayoutMatch,
    pub matched_matcher: Option<LayoutMatch>,
    pub target_monitor: Option<String>,
    pub target_rect: Option<[i32; 4]>,
    pub target_desktop: Option<String>,
    pub desktop_error: Option<String>,
    pub state: LayoutWindowState,
    pub result: LayoutMatchResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutMatchResult {
    Found,
    Launched,
    Missing,
}

impl std::fmt::Display for LayoutMatchResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LayoutMatchResult::Found => write!(f, "found"),
            LayoutMatchResult::Launched => write!(f, "launched"),
            LayoutMatchResult::Missing => write!(f, "missing"),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct LayoutRestoreSummary {
    pub entries: Vec<LayoutRestoreSummaryEntry>,
    pub found_windows: usize,
    pub launched_windows: usize,
    pub missing_windows: usize,
}

#[derive(Debug, Clone, Default)]
pub struct LayoutRestorePlan {
    pub summary: LayoutRestoreSummary,
    pub missing_windows: usize,
    #[cfg(windows)]
    actions: Vec<LayoutRestoreAction>,
}

fn should_schedule_geometry(has_target_rect: bool, _desktop_error: Option<&str>) -> bool {
    has_target_rect
}

fn apply_all_with_diagnostics<T>(
    actions: &[T],
    mut diagnostics: Vec<String>,
    mut apply: impl FnMut(&T) -> Option<String>,
) -> anyhow::Result<()> {
    for action in actions {
        if let Some(error) = apply(action) {
            diagnostics.push(error);
        }
    }
    if diagnostics.is_empty() {
        Ok(())
    } else {
        anyhow::bail!(diagnostics.join("; "))
    }
}

#[cfg(windows)]
struct EnumeratedWindow {
    hwnd: windows::Win32::Foundation::HWND,
    matcher: LayoutMatch,
    placement: windows::Win32::UI::WindowsAndMessaging::WINDOWPLACEMENT,
    monitor: windows::Win32::Graphics::Gdi::MONITORINFOEXW,
    desktop: Option<String>,
}

#[cfg(windows)]
#[derive(Debug, Clone)]
struct LayoutRestoreAction {
    hwnd: windows::Win32::Foundation::HWND,
    rect: windows::Win32::Foundation::RECT,
    show_cmd: windows::Win32::UI::WindowsAndMessaging::SHOW_WINDOW_CMD,
    target_desktop: Option<crate::virtual_desktop::VirtualDesktopId>,
}

#[cfg(windows)]
fn wide_to_string(value: &[u16]) -> String {
    let nul = value.iter().position(|c| *c == 0).unwrap_or(value.len());
    String::from_utf16_lossy(&value[..nul])
}

#[cfg(windows)]
fn enumerate_windows(options: LayoutWindowOptions) -> anyhow::Result<Vec<EnumeratedWindow>> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use std::path::Path;

    use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, HMONITOR, MONITOR_DEFAULTTONEAREST, MONITORINFOEXW, MonitorFromWindow,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GW_OWNER, GWL_EXSTYLE, GetClassNameW, GetForegroundWindow, GetWindow,
        GetWindowLongPtrW, GetWindowPlacement, GetWindowTextLengthW, GetWindowTextW,
        GetWindowThreadProcessId, IsWindowVisible, SW_MINIMIZE, SW_SHOWMINIMIZED, WINDOWPLACEMENT,
        WS_EX_TOOLWINDOW,
    };
    use windows::core::PWSTR;

    struct Ctx {
        options: LayoutWindowOptions,
        active_monitor: Option<HMONITOR>,
        windows: Vec<EnumeratedWindow>,
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
            let _ = windows::Win32::Foundation::CloseHandle(handle);
            if !success || size == 0 {
                return None;
            }
            Some(
                OsString::from_wide(&buffer[..size as usize])
                    .to_string_lossy()
                    .to_string(),
            )
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
        let ctx = unsafe { &mut *(lparam.0 as *mut Ctx) };
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

        let title_len = unsafe { GetWindowTextLengthW(hwnd) };
        if title_len <= 0 {
            return BOOL(1);
        }
        let mut title_buf = vec![0u16; title_len as usize + 1];
        let title_read = unsafe { GetWindowTextW(hwnd, &mut title_buf) };
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
        if let Some(active) = ctx.active_monitor
            && active != monitor
        {
            return BOOL(1);
        }

        let mut placement = WINDOWPLACEMENT::default();
        placement.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
        if unsafe { GetWindowPlacement(hwnd, &mut placement) }.is_err() {
            return BOOL(1);
        }
        let minimized = placement.showCmd == SW_SHOWMINIMIZED.0 as u32
            || placement.showCmd == SW_MINIMIZE.0 as u32;
        let allow_minimized = ctx.options.include_minimized;
        if minimized && !allow_minimized {
            return BOOL(1);
        }

        let mut class_buf = vec![0u16; 256];
        let class_len = unsafe { GetClassNameW(hwnd, &mut class_buf) } as usize;
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
        ctx.windows.push(EnumeratedWindow {
            hwnd,
            matcher,
            placement,
            monitor: monitor_info,
            desktop: None,
        });
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

    let mut ctx = Ctx {
        options,
        active_monitor,
        windows: Vec::new(),
    };
    unsafe {
        let ctx_ptr = &mut ctx as *mut Ctx;
        let _ = EnumWindows(Some(enum_cb), LPARAM(ctx_ptr as isize));
    }
    let hwnds = ctx
        .windows
        .iter()
        .map(|window| window.hwnd.0 as usize)
        .collect::<Vec<_>>();
    if let Ok(memberships) =
        crate::virtual_desktop::VirtualDesktopService.desktops_for_windows(&hwnds)
    {
        let memberships = memberships
            .into_iter()
            .collect::<std::collections::HashMap<_, _>>();
        for window in &mut ctx.windows {
            window.desktop = memberships
                .get(&(window.hwnd.0 as usize))
                .and_then(|desktop| desktop.as_ref())
                .map(ToString::to_string);
        }
    }
    Ok(ctx.windows)
}

#[cfg(not(windows))]
pub fn collect_layout_windows(_options: LayoutWindowOptions) -> anyhow::Result<Vec<LayoutWindow>> {
    Ok(Vec::new())
}

#[cfg(windows)]
fn placement_state(show_cmd: u32) -> LayoutWindowState {
    use windows::Win32::UI::WindowsAndMessaging::{
        SW_MAXIMIZE, SW_MINIMIZE, SW_SHOWMAXIMIZED, SW_SHOWMINIMIZED,
    };

    if show_cmd == SW_SHOWMAXIMIZED.0 as u32 || show_cmd == SW_MAXIMIZE.0 as u32 {
        LayoutWindowState::Maximized
    } else if show_cmd == SW_SHOWMINIMIZED.0 as u32 || show_cmd == SW_MINIMIZE.0 as u32 {
        LayoutWindowState::Minimized
    } else {
        LayoutWindowState::Normal
    }
}

#[cfg(windows)]
fn collect_layout_windows_from_enumerated(enumerated: Vec<EnumeratedWindow>) -> Vec<LayoutWindow> {
    enumerated
        .into_iter()
        .filter_map(|window| {
            let work_area = window.monitor.monitorInfo.rcWork;
            let work_width = (work_area.right - work_area.left) as f32;
            let work_height = (work_area.bottom - work_area.top) as f32;
            if work_width <= 0.0 || work_height <= 0.0 {
                return None;
            }

            let rect = window.placement.rcNormalPosition;
            let rect_width = (rect.right - rect.left) as f32;
            let rect_height = (rect.bottom - rect.top) as f32;
            let x = (rect.left - work_area.left) as f32 / work_width;
            let y = (rect.top - work_area.top) as f32 / work_height;
            let w = rect_width / work_width;
            let h = rect_height / work_height;

            let monitor_name = wide_to_string(&window.monitor.szDevice);
            let placement = LayoutPlacement {
                rect: [x, y, w, h],
                monitor: if monitor_name.is_empty() {
                    None
                } else {
                    Some(monitor_name)
                },
                state: placement_state(window.placement.showCmd),
            };
            Some(LayoutWindow {
                matcher: window.matcher,
                placement,
                desktop: window.desktop,
                group: None,
                launch: None,
            })
        })
        .collect()
}

#[cfg(windows)]
pub fn collect_layout_windows(options: LayoutWindowOptions) -> anyhow::Result<Vec<LayoutWindow>> {
    let enumerated = enumerate_windows(options)?;
    Ok(collect_layout_windows_from_enumerated(enumerated))
}

#[cfg(windows)]
fn list_monitors() -> Vec<(String, windows::Win32::Graphics::Gdi::MONITORINFOEXW)> {
    use windows::Win32::Foundation::{BOOL, LPARAM, RECT};
    use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR, MONITORINFOEXW};

    unsafe extern "system" fn enum_monitor_cb(
        monitor: HMONITOR,
        _hdc: HDC,
        _rect: *mut RECT,
        lparam: LPARAM,
    ) -> BOOL {
        let monitors = unsafe { &mut *(lparam.0 as *mut Vec<(String, MONITORINFOEXW)>) };
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        if unsafe {
            windows::Win32::Graphics::Gdi::GetMonitorInfoW(monitor, &mut info as *mut _ as *mut _)
        }
        .as_bool()
        {
            let name = wide_to_string(&info.szDevice);
            monitors.push((name, info));
        }
        BOOL(1)
    }

    let mut monitors: Vec<(String, MONITORINFOEXW)> = Vec::new();
    unsafe {
        let monitors_ptr = &mut monitors as *mut _;
        let _ = EnumDisplayMonitors(
            HDC::default(),
            None,
            Some(enum_monitor_cb),
            LPARAM(monitors_ptr as isize),
        );
    }
    monitors
}

#[cfg(windows)]
fn matches_title_regex(pattern: &str, title: &str) -> bool {
    regex::RegexBuilder::new(pattern)
        .case_insensitive(true)
        .build()
        .map(|re| re.is_match(title))
        .unwrap_or(false)
}

#[cfg(windows)]
fn is_rule_match(rule: &LayoutMatch, candidate: &LayoutMatch) -> bool {
    if rule.app_id.is_none()
        && rule.process.is_none()
        && rule.class.is_none()
        && rule.title.is_none()
    {
        return false;
    }
    let app_ok = match (&rule.app_id, &candidate.app_id) {
        (Some(rule), Some(candidate)) => rule.eq_ignore_ascii_case(candidate),
        (Some(_), None) => false,
        (None, _) => true,
    };
    let process_ok = match (&rule.process, &candidate.process) {
        (Some(rule), Some(candidate)) => rule.eq_ignore_ascii_case(candidate),
        (Some(_), None) => false,
        (None, _) => true,
    };
    let class_ok = match (&rule.class, &candidate.class) {
        (Some(rule), Some(candidate)) => rule.eq_ignore_ascii_case(candidate),
        (Some(_), None) => false,
        (None, _) => true,
    };
    let title_ok = match (&rule.title, &candidate.title) {
        (Some(rule), Some(candidate)) => matches_title_regex(rule, candidate),
        (Some(_), None) => false,
        (None, _) => true,
    };
    app_ok && process_ok && class_ok && title_ok
}

#[cfg(windows)]
fn match_score(saved: &LayoutMatch, candidate: &LayoutMatch) -> Option<u8> {
    if saved.app_id.is_none()
        && saved.process.is_none()
        && saved.class.is_none()
        && saved.title.is_none()
    {
        return None;
    }
    if let (Some(saved), Some(candidate)) = (&saved.app_id, &candidate.app_id)
        && saved.eq_ignore_ascii_case(candidate)
    {
        return Some(4);
    }
    if let (Some(saved), Some(candidate)) = (&saved.process, &candidate.process)
        && saved.eq_ignore_ascii_case(candidate)
    {
        return Some(3);
    }
    if let (Some(saved), Some(candidate)) = (&saved.class, &candidate.class)
        && saved.eq_ignore_ascii_case(candidate)
    {
        return Some(2);
    }
    if let (Some(saved), Some(candidate)) = (&saved.title, &candidate.title)
        && matches_title_regex(saved, candidate)
    {
        return Some(1);
    }
    None
}

#[cfg(windows)]
fn target_rect_from_monitor(
    rect: [f32; 4],
    monitor_info: &windows::Win32::Graphics::Gdi::MONITORINFOEXW,
) -> Option<windows::Win32::Foundation::RECT> {
    let work_area = monitor_info.monitorInfo.rcWork;
    let work_width = (work_area.right - work_area.left) as f32;
    let work_height = (work_area.bottom - work_area.top) as f32;
    if work_width <= 0.0 || work_height <= 0.0 {
        return None;
    }
    let left = work_area.left as f32 + rect[0] * work_width;
    let top = work_area.top as f32 + rect[1] * work_height;
    let right = left + rect[2] * work_width;
    let bottom = top + rect[3] * work_height;
    Some(windows::Win32::Foundation::RECT {
        left: left.round() as i32,
        top: top.round() as i32,
        right: right.round() as i32,
        bottom: bottom.round() as i32,
    })
}

#[cfg(windows)]
fn show_cmd_for_state(
    state: &LayoutWindowState,
) -> windows::Win32::UI::WindowsAndMessaging::SHOW_WINDOW_CMD {
    use windows::Win32::UI::WindowsAndMessaging::{SW_MAXIMIZE, SW_MINIMIZE, SW_RESTORE};

    match state {
        LayoutWindowState::Normal => SW_RESTORE,
        LayoutWindowState::Maximized => SW_MAXIMIZE,
        LayoutWindowState::Minimized => SW_MINIMIZE,
    }
}

#[cfg(windows)]
pub fn plan_layout_restore(
    layout: &Layout,
    options: LayoutWindowOptions,
) -> anyhow::Result<LayoutRestorePlan> {
    let enumerated = enumerate_windows(options)?;
    let monitors = list_monitors();
    let monitor_map: std::collections::HashMap<
        String,
        windows::Win32::Graphics::Gdi::MONITORINFOEXW,
    > = monitors
        .into_iter()
        .filter(|(name, _)| !name.is_empty())
        .map(|(name, info)| (name.to_lowercase(), info))
        .collect();

    let candidates: Vec<EnumeratedWindow> = enumerated
        .into_iter()
        .filter(|window| {
            !layout
                .ignore
                .iter()
                .any(|rule| is_rule_match(rule, &window.matcher))
        })
        .collect();
    let mut used = vec![false; candidates.len()];

    let mut summary = LayoutRestoreSummary::default();
    let mut actions = Vec::new();
    let mut missing = 0;
    let mut found = 0;
    let desktop_snapshot = layout
        .windows
        .iter()
        .any(|window| window.desktop.is_some())
        .then(|| crate::virtual_desktop::VirtualDesktopService.snapshot());

    for saved in &layout.windows {
        let mut best_idx = None;
        let mut best_score = 0u8;
        for (idx, candidate) in candidates.iter().enumerate() {
            if used[idx] {
                continue;
            }
            if let Some(score) = match_score(&saved.matcher, &candidate.matcher)
                && score > best_score
            {
                best_score = score;
                best_idx = Some(idx);
            }
        }

        if let Some(idx) = best_idx {
            used[idx] = true;
            let candidate = &candidates[idx];
            let desired_monitor = saved
                .placement
                .monitor
                .as_ref()
                .and_then(|name| monitor_map.get(&name.to_lowercase()));
            let target_monitor = desired_monitor.unwrap_or(&candidate.monitor);
            let target_rect = target_rect_from_monitor(saved.placement.rect, target_monitor);
            let monitor_name = if !target_monitor.szDevice.is_empty() {
                Some(wide_to_string(&target_monitor.szDevice))
            } else {
                None
            };
            let state = saved.placement.state.clone();
            let (target_desktop, desktop_error) = match (&saved.desktop, desktop_snapshot.as_ref())
            {
                (None, _) => (None, None),
                (Some(_), Some(Err(error))) => (None, Some(error.to_string())),
                (Some(stored), Some(Ok(snapshot))) => {
                    match plan::resolve_layout_desktop(snapshot, Some(stored)) {
                        Ok(Some(target)) => {
                            let current = candidate.desktop.as_deref().and_then(|id| {
                                crate::virtual_desktop::VirtualDesktopId::parse(id).ok()
                            });
                            ((current.as_ref() != Some(&target)).then_some(target), None)
                        }
                        Ok(None) => (None, None),
                        Err(error) => (None, Some(error.to_string())),
                    }
                }
                (Some(_), None) => unreachable!("desktop snapshot requested for saved desktop"),
            };
            let schedule_geometry =
                should_schedule_geometry(target_rect.is_some(), desktop_error.as_deref());
            summary.entries.push(LayoutRestoreSummaryEntry {
                saved_matcher: saved.matcher.clone(),
                matched_matcher: Some(candidate.matcher.clone()),
                target_monitor: monitor_name.clone(),
                target_rect: target_rect.map(|rect| [rect.left, rect.top, rect.right, rect.bottom]),
                target_desktop: saved.desktop.clone(),
                desktop_error,
                state: state.clone(),
                result: LayoutMatchResult::Found,
            });
            found += 1;
            if schedule_geometry && let Some(rect) = target_rect {
                actions.push(LayoutRestoreAction {
                    hwnd: candidate.hwnd,
                    rect,
                    show_cmd: show_cmd_for_state(&state),
                    target_desktop,
                });
            }
        } else {
            missing += 1;
            summary.entries.push(LayoutRestoreSummaryEntry {
                saved_matcher: saved.matcher.clone(),
                matched_matcher: None,
                target_monitor: saved.placement.monitor.clone(),
                target_rect: None,
                target_desktop: saved.desktop.clone(),
                desktop_error: None,
                state: saved.placement.state.clone(),
                result: LayoutMatchResult::Missing,
            });
        }
    }

    summary.missing_windows = missing;
    summary.found_windows = found;
    Ok(LayoutRestorePlan {
        summary,
        missing_windows: missing,
        actions,
    })
}

#[cfg(not(windows))]
pub fn plan_layout_restore(
    _layout: &Layout,
    _options: LayoutWindowOptions,
) -> anyhow::Result<LayoutRestorePlan> {
    Ok(LayoutRestorePlan::default())
}

#[cfg(windows)]
pub fn apply_layout_restore_plan(plan: &LayoutRestorePlan) -> anyhow::Result<()> {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowPlacement, SW_SHOWNORMAL, SetWindowPlacement, ShowWindow, WINDOWPLACEMENT,
    };

    let desktop_errors = plan
        .summary
        .entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            entry
                .desktop_error
                .as_ref()
                .map(|error| format!("layout window {} desktop: {error}", index + 1))
        })
        .collect::<Vec<_>>();
    let move_requests = plan
        .actions
        .iter()
        .filter_map(|action| {
            action
                .target_desktop
                .as_ref()
                .map(|desktop| (action.hwnd.0 as usize, desktop.clone()))
        })
        .collect::<Vec<_>>();
    let move_results = crate::virtual_desktop::VirtualDesktopService
        .move_windows_to_desktops(&move_requests)
        .into_iter()
        .collect::<std::collections::HashMap<_, _>>();
    apply_all_with_diagnostics(&plan.actions, desktop_errors, |action| {
        let mut placement = WINDOWPLACEMENT::default();
        placement.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
        let _ = unsafe { GetWindowPlacement(action.hwnd, &mut placement) };
        let desktop_error = action.target_desktop.as_ref().and_then(|target_desktop| {
            move_results
                .get(&(action.hwnd.0 as usize))
                .and_then(|result| result.as_ref().err())
                .map(|error| {
                    format!(
                        "failed to move layout window {:?} to desktop {}: {}",
                        action.hwnd, target_desktop, error
                    )
                })
        });
        placement.rcNormalPosition = action.rect;
        placement.showCmd = SW_SHOWNORMAL.0 as u32;
        unsafe {
            let _ = SetWindowPlacement(action.hwnd, &placement);
            let _ = ShowWindow(action.hwnd, action.show_cmd);
        }
        desktop_error
    })
}

#[cfg(not(windows))]
pub fn apply_layout_restore_plan(_plan: &LayoutRestorePlan) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn non_dry_apply_surfaces_planning_desktop_diagnostics() {
        let plan = LayoutRestorePlan {
            summary: LayoutRestoreSummary {
                entries: vec![LayoutRestoreSummaryEntry {
                    saved_matcher: LayoutMatch::default(),
                    matched_matcher: Some(LayoutMatch::default()),
                    target_monitor: None,
                    target_rect: Some([0, 0, 100, 100]),
                    target_desktop: Some("Missing".into()),
                    desktop_error: Some("Virtual desktop was not found".into()),
                    state: LayoutWindowState::Normal,
                    result: LayoutMatchResult::Found,
                }],
                found_windows: 1,
                ..Default::default()
            },
            missing_windows: 0,
            actions: Vec::new(),
        };
        let error = apply_layout_restore_plan(&plan).unwrap_err().to_string();
        assert!(error.contains("layout window 1 desktop"));
        assert!(error.contains("was not found"));
    }

    #[test]
    fn desktop_resolution_error_does_not_suppress_geometry_planning() {
        assert!(should_schedule_geometry(
            true,
            Some("ambiguous virtual desktop")
        ));
        assert!(!should_schedule_geometry(false, None));
    }

    #[test]
    fn application_continues_actions_before_returning_planning_diagnostics() {
        let mut applied = Vec::new();
        let error = apply_all_with_diagnostics(
            &[1, 2],
            vec!["window 1 desktop missing".into()],
            |action| {
                applied.push(*action);
                None
            },
        )
        .unwrap_err()
        .to_string();
        assert_eq!(applied, [1, 2]);
        assert!(error.contains("desktop missing"));
    }

    #[test]
    fn capture_and_apply_use_batch_virtual_desktop_sessions() {
        let source = include_str!("windows_layout.rs");
        let enumerate = source
            .split("fn enumerate_windows")
            .nth(1)
            .unwrap()
            .split("fn placement_state")
            .next()
            .unwrap();
        assert!(enumerate.contains("desktops_for_windows(&hwnds)"));
        assert!(!enumerate.contains("desktop_for_window(hwnd)"));
        let apply = source
            .split("pub fn apply_layout_restore_plan")
            .nth(1)
            .unwrap()
            .split("#[cfg(not(windows))]")
            .next()
            .unwrap();
        assert!(apply.contains("move_windows_to_desktops(&move_requests)"));
        assert!(!apply.contains("move_window_to_desktop(action.hwnd"));
    }
}
