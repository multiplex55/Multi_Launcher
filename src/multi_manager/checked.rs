//! Explicit, non-persistent reconnect and identity-checked placement.
use crate::multi_manager::{
    model::{MmRect, MmWorkspace},
    reconnect::{self, ReconnectDeps, ReconnectSummary},
    win::{self, EnumeratedWindow},
};
use crate::window_catalog::{WindowNativeIdentity, WindowSwitchIdentity};
use anyhow::{Context, Result};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    sync::{Arc, atomic::AtomicBool},
};

#[derive(Clone)]
pub struct ReconnectedWorkspace {
    pub workspace: MmWorkspace,
    pub identities: HashMap<usize, WindowSwitchIdentity>,
    pub summary: ReconnectSummary,
}

/// Enumeration is injected so a real failure cannot be confused with an empty desktop.
pub trait ReconnectBackend {
    fn enumerate(&self) -> Result<Vec<(EnumeratedWindow, WindowSwitchIdentity)>>;
}

pub struct NativeReconnect;

#[cfg(any(test, windows))]
fn observation_is_current(
    before: &crate::window_catalog::WindowDescriptor,
    after: &crate::window_catalog::WindowDescriptor,
    first: &WindowSwitchIdentity,
    second: &WindowSwitchIdentity,
    tool: bool,
    launcher_pid: u32,
) -> bool {
    !tool
        && first.native.pid != launcher_pid
        && !before.title.trim().is_empty()
        && before == after
        && first == second
}
impl ReconnectBackend for NativeReconnect {
    fn enumerate(&self) -> Result<Vec<(EnumeratedWindow, WindowSwitchIdentity)>> {
        let mut checked = Vec::new();
        for candidate in win::enumerate_top_level_windows()? {
            if let Some(observation) = fresh_candidate(candidate.hwnd) {
                checked.push(observation);
            }
        }
        Ok(checked)
    }
}

#[cfg(windows)]
fn native_extended_style(hwnd: windows::Win32::Foundation::HWND) -> Result<u32> {
    use windows::Win32::{
        Foundation::{GetLastError, SetLastError, WIN32_ERROR},
        UI::WindowsAndMessaging::{GWL_EXSTYLE, GetWindowLongPtrW},
    };
    unsafe {
        SetLastError(WIN32_ERROR(0));
    }
    let style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32;
    let error = unsafe { GetLastError() };
    anyhow::ensure!(
        style != 0 || error == WIN32_ERROR(0),
        "Could not read window style: {error:?}"
    );
    Ok(style)
}

#[cfg(windows)]
fn fresh_candidate(hwnd: usize) -> Option<(EnumeratedWindow, WindowSwitchIdentity)> {
    use windows::Win32::{Foundation::HWND, UI::WindowsAndMessaging::WS_EX_TOOLWINDOW};
    let (before, identity) = crate::window_catalog::current_switch_window(hwnd)?;
    let rect = win::window_rect(hwnd)?;
    let style = native_extended_style(HWND(hwnd as *mut _)).ok()?;
    let (after, final_identity) = crate::window_catalog::current_switch_window(hwnd)?;
    if !observation_is_current(
        &before,
        &after,
        &identity,
        &final_identity,
        style & WS_EX_TOOLWINDOW.0 != 0,
        std::process::id(),
    ) {
        return None;
    }
    Some((
        EnumeratedWindow {
            hwnd,
            title: after.title,
            executable: after.executable.unwrap_or_default(),
            class_name: after.class_name.unwrap_or_default(),
            process_path: after.process_path.unwrap_or_default(),
            rect,
        },
        identity,
    ))
}

#[cfg(not(windows))]
fn fresh_candidate(_: usize) -> Option<(EnumeratedWindow, WindowSwitchIdentity)> {
    None
}

pub fn reconnect_checked(
    workspaces: &[MmWorkspace],
    workspace_index: usize,
    current: impl Fn() -> bool,
    backend: &impl ReconnectBackend,
) -> Result<ReconnectedWorkspace> {
    anyhow::ensure!(current(), "Reconnect superseded");
    let selected = workspaces
        .get(workspace_index)
        .context("Workspace no longer exists")?;
    anyhow::ensure!(
        !selected.id.is_empty()
            && workspaces
                .iter()
                .filter(|row| row.id == selected.id)
                .count()
                == 1,
        "Workspace ID is ambiguous; fix duplicate IDs before reconnecting"
    );
    anyhow::ensure!(
        !selected.disabled && selected.valid,
        "Workspace is disabled or invalid"
    );
    let mut shadow = vec![selected.clone()];
    // Saved HWNDs and historical binding flags never become placement authority.
    for window in &mut shadow[0].windows {
        window.hwnd = 0;
        window.binding_verified = false;
    }
    let snapshot = reconnect::collect_reconnect_snapshot(&shadow);
    let captured = RefCell::new(HashMap::<usize, WindowSwitchIdentity>::new());
    let error = RefCell::new(None::<String>);
    let duplicate = RefCell::new(false);
    let enumerate = || match backend.enumerate() {
        Ok(rows) => rows
            .into_iter()
            .filter_map(|(window, identity)| {
                if identity.native.hwnd != window.hwnd {
                    return None;
                }
                if captured.borrow().contains_key(&window.hwnd) {
                    *duplicate.borrow_mut() = true;
                    return None;
                }
                captured.borrow_mut().insert(window.hwnd, identity);
                Some(window)
            })
            .collect(),
        Err(failure) => {
            *error.borrow_mut() = Some(format!("{failure:#}"));
            Vec::new()
        }
    };
    let (_worker, patches) = reconnect::build_reconnect_patches(
        &snapshot,
        ReconnectDeps {
            is_window: &|_| false,
            query_identity: &|_| None,
            enumerate_top_level_windows: &enumerate,
        },
    );
    if let Some(error) = error.into_inner() {
        anyhow::bail!("Could not enumerate windows: {error}");
    }
    anyhow::ensure!(
        !duplicate.into_inner(),
        "Window enumeration contained duplicate handles"
    );
    anyhow::ensure!(current(), "Reconnect superseded");
    let summary = reconnect::apply_reconnect_patches(&mut shadow, &patches);
    let captured = captured.into_inner();
    let mut identities = HashMap::new();
    let mut assigned = HashSet::new();
    for (index, window) in shadow[0].windows.iter().enumerate() {
        if window.disabled || !window.binding_verified || window.hwnd == 0 {
            continue;
        }
        let Some(identity) = captured.get(&window.hwnd) else {
            continue;
        };
        anyhow::ensure!(
            assigned.insert(identity.native.hwnd),
            "Reconnect produced duplicate window handles"
        );
        identities.insert(index, identity.clone());
    }
    Ok(ReconnectedWorkspace {
        workspace: shadow.remove(0),
        identities,
        summary,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Destination {
    Home,
    Target,
}

#[derive(Debug, Default)]
pub struct PlacementReport {
    pub moved: usize,
    pub skipped: usize,
    pub error: Option<String>,
}

#[derive(Debug, Default)]
pub struct PlacementOutcome {
    pub cleanup_warning: Option<String>,
}

pub trait PlacementBackend {
    fn workspace_on_current_desktop(
        &mut self,
        required: Option<&crate::virtual_desktop::VirtualDesktopId>,
        current: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<bool>;
    fn on_current_desktop(
        &mut self,
        target: &WindowSwitchIdentity,
        current: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<bool>;
    fn activate(
        &mut self,
        issuer: WindowNativeIdentity,
        target: &WindowSwitchIdentity,
        previous: Option<&WindowSwitchIdentity>,
        required: Option<&crate::virtual_desktop::VirtualDesktopId>,
        current: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<()>;
    fn place(
        &mut self,
        issuer: WindowNativeIdentity,
        target: &WindowSwitchIdentity,
        rect: MmRect,
        required: Option<&crate::virtual_desktop::VirtualDesktopId>,
        current: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<PlacementOutcome>;
}

/// Applies captured targets in recorded order. The only accepted foreground
/// predecessor after a successful placement is the exact prior target.
pub fn place_checked(
    overlay: &ReconnectedWorkspace,
    issuer: WindowNativeIdentity,
    destination: Destination,
    current: Arc<dyn Fn() -> bool + Send + Sync>,
    backend: &mut impl PlacementBackend,
) -> PlacementReport {
    let mut report = PlacementReport::default();
    let mut previous = None::<WindowSwitchIdentity>;
    let required = if destination == Destination::Target {
        overlay
            .workspace
            .virtual_desktop
            .as_ref()
            .map(|binding| &binding.id)
    } else {
        None
    };
    match backend.workspace_on_current_desktop(required, current.clone()) {
        Ok(true) => {}
        Ok(false) => {
            report.skipped = overlay.identities.len();
            return report;
        }
        Err(error) => {
            report.error = Some(format!("{error:#}"));
            return report;
        }
    }
    let mut seen = HashSet::new();
    if overlay
        .identities
        .values()
        .any(|target| !seen.insert(target.native.hwnd))
    {
        report.error = Some("Duplicate window handle in reconnect result".into());
        return report;
    }
    seen.clear();
    for (index, window) in overlay.workspace.windows.iter().enumerate() {
        if window.disabled {
            continue;
        }
        let Some(target) = overlay.identities.get(&index) else {
            report.skipped += 1;
            continue;
        };
        if !seen.insert(target.native.hwnd) {
            report.error = Some("Duplicate window handle in reconnect result".into());
            break;
        }
        let rect = match destination {
            Destination::Home => window.home_rect.or(overlay.workspace.home_rect),
            Destination::Target => window.target_rect.or(overlay.workspace.target_rect),
        };
        let Some(rect) = rect.filter(|rect| rect.w > 0 && rect.h > 0) else {
            report.skipped += 1;
            continue;
        };
        if !(current)() {
            report.error = Some("Placement superseded".into());
            break;
        }
        match backend.on_current_desktop(target, current.clone()) {
            Ok(true) => {}
            Ok(false) => {
                report.skipped += 1;
                continue;
            }
            Err(error) => {
                report.error = Some(format!("{error:#}"));
                break;
            }
        }
        if let Err(error) =
            backend.activate(issuer, target, previous.as_ref(), required, current.clone())
        {
            report.error = Some(format!("{error:#}"));
            break;
        }
        match backend.place(issuer, target, rect, required, current.clone()) {
            Ok(outcome) => {
                report.moved += 1;
                if let Some(warning) = outcome.cleanup_warning {
                    report.error = Some(warning);
                    break;
                }
            }
            Err(error) => {
                report.error = Some(format!("{error:#}"));
                break;
            }
        }
        previous = Some(target.clone());
    }
    report
}

pub struct NativePlacement {
    pub effect_started: Arc<AtomicBool>,
}
impl PlacementBackend for NativePlacement {
    fn workspace_on_current_desktop(
        &mut self,
        required: Option<&crate::virtual_desktop::VirtualDesktopId>,
        current: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<bool> {
        native_workspace_on_current_desktop(required, current)
    }
    fn on_current_desktop(
        &mut self,
        target: &WindowSwitchIdentity,
        current: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<bool> {
        native_target_on_current_desktop(target, current)
    }
    fn activate(
        &mut self,
        issuer: WindowNativeIdentity,
        target: &WindowSwitchIdentity,
        previous: Option<&WindowSwitchIdentity>,
        required: Option<&crate::virtual_desktop::VirtualDesktopId>,
        current: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<()> {
        crate::window_activation::activate_checked_window_switch_after(
            crate::window_activation::CheckedWindowSwitch {
                initiating: issuer,
                selected: target.clone(),
                is_current: current,
            },
            previous.cloned(),
            required.cloned(),
            self.effect_started.clone(),
        )
        .map_err(anyhow::Error::new)
    }
    fn place(
        &mut self,
        issuer: WindowNativeIdentity,
        target: &WindowSwitchIdentity,
        rect: MmRect,
        required: Option<&crate::virtual_desktop::VirtualDesktopId>,
        current: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<PlacementOutcome> {
        native_checked_geometry(issuer, target, rect, required, current)
    }
}

#[cfg(windows)]
fn native_workspace_on_current_desktop(
    required: Option<&crate::virtual_desktop::VirtualDesktopId>,
    current: Arc<dyn Fn() -> bool + Send + Sync>,
) -> Result<bool> {
    if let Some(required) = required {
        anyhow::ensure!(current(), "Placement superseded");
        let desktop = crate::virtual_desktop::VirtualDesktopService
            .current_for_launcher_restore()
            .map_err(anyhow::Error::new)?;
        anyhow::ensure!(current(), "Placement superseded");
        Ok(required == &desktop)
    } else {
        Ok(true)
    }
}

#[cfg(not(windows))]
fn native_workspace_on_current_desktop(
    _: Option<&crate::virtual_desktop::VirtualDesktopId>,
    _: Arc<dyn Fn() -> bool + Send + Sync>,
) -> Result<bool> {
    anyhow::bail!("MultiManager window movement is available only on Windows")
}

#[cfg(any(test, windows))]
fn identity_only_cleanup(
    identity_current: impl FnOnce() -> bool,
    demote: impl FnOnce() -> Result<()>,
) -> Result<bool> {
    if identity_current() {
        demote()?;
        return Ok(true);
    }
    Ok(false)
}

#[cfg(any(test, windows))]
fn cleanup_after_move(
    was_topmost: bool,
    identity_current: impl FnOnce() -> bool,
    demote: impl FnOnce() -> Result<()>,
) -> Option<String> {
    if was_topmost {
        return None;
    }
    match identity_only_cleanup(identity_current, demote) {
        Ok(true) => None,
        Ok(false) => {
            Some("Window moved, but its identity changed before temporary topmost cleanup.".into())
        }
        Err(error) => Some(format!(
            "Window moved, but temporary topmost cleanup failed: {error:#}"
        )),
    }
}

#[cfg(windows)]
fn native_target_on_current_desktop(
    target: &WindowSwitchIdentity,
    current: Arc<dyn Fn() -> bool + Send + Sync>,
) -> Result<bool> {
    use windows::Win32::Foundation::HWND;
    anyhow::ensure!(
        current()
            && crate::window_catalog::current_switch_identity(target.native.hwnd).as_ref()
                == Some(target),
        "Window identity changed or placement superseded"
    );
    let result = crate::virtual_desktop::VirtualDesktopService
        .is_window_on_current_desktop(HWND(target.native.hwnd as *mut _))
        .map_err(anyhow::Error::new)?;
    anyhow::ensure!(current(), "Placement superseded");
    Ok(result)
}

#[cfg(not(windows))]
fn native_target_on_current_desktop(
    _: &WindowSwitchIdentity,
    _: Arc<dyn Fn() -> bool + Send + Sync>,
) -> Result<bool> {
    anyhow::bail!("MultiManager window movement is available only on Windows")
}

#[cfg(windows)]
fn native_checked_geometry(
    issuer: WindowNativeIdentity,
    target: &WindowSwitchIdentity,
    rect: MmRect,
    required: Option<&crate::virtual_desktop::VirtualDesktopId>,
    current: Arc<dyn Fn() -> bool + Send + Sync>,
) -> Result<PlacementOutcome> {
    use windows::Win32::{
        Foundation::HWND,
        UI::WindowsAndMessaging::{
            GetForegroundWindow, HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE,
            SWP_NOSIZE, SWP_SHOWWINDOW, SetWindowPos, WS_EX_TOPMOST,
        },
    };
    let hwnd = HWND(target.native.hwnd as *mut _);
    anyhow::ensure!(
        rect.w > 0 && rect.h > 0 && current(),
        "Placement superseded"
    );
    anyhow::ensure!(
        crate::window_catalog::current_switch_identity(target.native.hwnd).as_ref() == Some(target),
        "Window identity changed"
    );
    anyhow::ensure!(
        crate::virtual_desktop::VirtualDesktopService
            .is_window_on_current_desktop(hwnd)
            .map_err(anyhow::Error::new)?,
        "Window is on another virtual desktop"
    );
    anyhow::ensure!(current(), "Placement superseded");
    let style = native_extended_style(hwnd)?;
    let was_topmost = style & WS_EX_TOPMOST.0 != 0;
    // The authority callback can read disk; sample all native proofs after it.
    if let Some(required) = required {
        let actual = crate::virtual_desktop::VirtualDesktopService
            .current_for_launcher_restore()
            .map_err(anyhow::Error::new)?;
        anyhow::ensure!(&actual == required, "Workspace target desktop changed");
    }
    anyhow::ensure!(
        crate::window_catalog::native_window_identity(issuer.hwnd) == Some(issuer)
            && crate::virtual_desktop::VirtualDesktopService
                .is_window_on_current_desktop(HWND(issuer.hwnd as *mut _))
                .map_err(anyhow::Error::new)?
            && crate::window_catalog::current_switch_identity(target.native.hwnd).as_ref()
                == Some(target)
            && crate::virtual_desktop::VirtualDesktopService
                .is_window_on_current_desktop(hwnd)
                .map_err(anyhow::Error::new)?
            && crate::window_catalog::native_window_identity(
                unsafe { GetForegroundWindow() }.0 as usize
            ) == Some(target.native),
        "Window identity, desktop or foreground changed"
    );
    unsafe {
        SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            rect.x,
            rect.y,
            rect.w,
            rect.h,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        )
    }
    .context("place window")?;
    // Cleanup is identity-only: revocation must never strand a valid window topmost.
    let cleanup_warning = cleanup_after_move(
        was_topmost,
        || {
            crate::window_catalog::native_window_identity(target.native.hwnd) == Some(target.native)
                && crate::window_catalog::describe_window(target.native.hwnd)
                    .is_some_and(|descriptor| target.descriptor.matches(&descriptor))
                && crate::window_catalog::native_window_identity(target.native.hwnd)
                    == Some(target.native)
        },
        || {
            unsafe {
                SetWindowPos(
                    hwnd,
                    HWND_NOTOPMOST,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                )
            }
            .context("remove temporary topmost")
        },
    );
    Ok(PlacementOutcome { cleanup_warning })
}

#[cfg(not(windows))]
fn native_checked_geometry(
    _: WindowNativeIdentity,
    _: &WindowSwitchIdentity,
    _: MmRect,
    _: Option<&crate::virtual_desktop::VirtualDesktopId>,
    _: Arc<dyn Fn() -> bool + Send + Sync>,
) -> Result<PlacementOutcome> {
    anyhow::bail!("MultiManager window movement is available only on Windows")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::multi_manager::model::MmWindow;
    use crate::window_catalog::WindowTargetIdentity;

    fn identity(hwnd: usize, pid: u32) -> WindowSwitchIdentity {
        WindowSwitchIdentity {
            native: WindowNativeIdentity {
                hwnd,
                pid,
                tid: hwnd as u32,
            },
            descriptor: WindowTargetIdentity {
                hwnd,
                pid,
                executable: Some("editor.exe".into()),
                process_path: Some("C:\\editor.exe".into()),
                class_name: Some("Editor".into()),
            },
        }
    }
    fn candidate(hwnd: usize) -> EnumeratedWindow {
        EnumeratedWindow {
            hwnd,
            title: "Editor".into(),
            executable: "editor.exe".into(),
            process_path: "C:\\editor.exe".into(),
            class_name: "Editor".into(),
            rect: MmRect {
                x: 0,
                y: 0,
                w: 100,
                h: 100,
            },
        }
    }
    #[test]
    fn candidate_observation_rejects_reused_hidden_class_and_self() {
        let id = identity(10, 22);
        let before = crate::window_catalog::WindowDescriptor {
            title: "Editor".into(),
            hwnd: 10,
            pid: 22,
            executable: Some("editor.exe".into()),
            process_path: Some("C:\\editor.exe".into()),
            class_name: Some("Editor".into()),
        };
        assert!(observation_is_current(&before, &before, &id, &id, false, 1));
        assert!(!observation_is_current(&before, &before, &id, &id, true, 1));
        assert!(!observation_is_current(
            &before, &before, &id, &id, false, 22
        ));
        let mut changed = before.clone();
        changed.title = "Other".into();
        assert!(!observation_is_current(
            &before, &changed, &id, &id, false, 1
        ));
        let mut reused = id.clone();
        reused.native.tid += 1;
        assert!(!observation_is_current(
            &before, &before, &id, &reused, false, 1
        ));
    }
    struct FakeReconnect {
        fail: bool,
        rows: Vec<(EnumeratedWindow, WindowSwitchIdentity)>,
    }
    impl ReconnectBackend for FakeReconnect {
        fn enumerate(&self) -> Result<Vec<(EnumeratedWindow, WindowSwitchIdentity)>> {
            if self.fail {
                anyhow::bail!("enumeration failed");
            }
            Ok(self.rows.clone())
        }
    }
    fn workspace() -> MmWorkspace {
        MmWorkspace {
            id: "one".into(),
            name: "Workspace".into(),
            windows: vec![MmWindow {
                captured_title: "Editor".into(),
                executable: "editor.exe".into(),
                process_path: "C:\\editor.exe".into(),
                class_name: "Editor".into(),
                hwnd: 999,
                binding_verified: true,
                ..Default::default()
            }],
            ..Default::default()
        }
    }
    #[test]
    fn reconnect_preserves_fallible_enumeration_and_never_trusts_saved_hwnd() {
        let row = workspace();
        assert!(
            reconnect_checked(
                &[row.clone()],
                0,
                || true,
                &FakeReconnect {
                    fail: true,
                    rows: vec![]
                }
            )
            .is_err()
        );
        let empty = reconnect_checked(
            &[row.clone()],
            0,
            || true,
            &FakeReconnect {
                fail: false,
                rows: vec![],
            },
        )
        .unwrap();
        assert!(empty.identities.is_empty());
        let found = reconnect_checked(
            &[row.clone()],
            0,
            || true,
            &FakeReconnect {
                fail: false,
                rows: vec![(candidate(10), identity(10, 22))],
            },
        )
        .unwrap();
        assert_eq!(found.identities.get(&0), Some(&identity(10, 22)));
        assert_ne!(found.workspace.windows[0].hwnd, 999);
        assert!(
            reconnect_checked(
                &[row.clone(), row],
                0,
                || true,
                &FakeReconnect {
                    fail: false,
                    rows: vec![]
                }
            )
            .is_err()
        );
    }

    struct FakePlacement {
        foreground: WindowNativeIdentity,
        moved: Vec<(usize, MmRect)>,
        unrelated_after_first: bool,
        cleanup_warning: bool,
        foreign_target: bool,
        change_target_at_activation: bool,
    }
    impl PlacementBackend for FakePlacement {
        fn workspace_on_current_desktop(
            &mut self,
            required: Option<&crate::virtual_desktop::VirtualDesktopId>,
            current: Arc<dyn Fn() -> bool + Send + Sync>,
        ) -> Result<bool> {
            anyhow::ensure!(current(), "revoked");
            Ok(required.is_none() || !self.foreign_target)
        }
        fn on_current_desktop(
            &mut self,
            _: &WindowSwitchIdentity,
            current: Arc<dyn Fn() -> bool + Send + Sync>,
        ) -> Result<bool> {
            anyhow::ensure!(current(), "revoked");
            Ok(true)
        }
        fn activate(
            &mut self,
            issuer: WindowNativeIdentity,
            target: &WindowSwitchIdentity,
            previous: Option<&WindowSwitchIdentity>,
            required: Option<&crate::virtual_desktop::VirtualDesktopId>,
            current: Arc<dyn Fn() -> bool + Send + Sync>,
        ) -> Result<()> {
            anyhow::ensure!(
                current()
                    && !(required.is_some() && self.change_target_at_activation)
                    && self.foreground == previous.map_or(issuer, |previous| previous.native),
                "unrelated foreground"
            );
            self.foreground = target.native;
            Ok(())
        }
        fn place(
            &mut self,
            _: WindowNativeIdentity,
            target: &WindowSwitchIdentity,
            rect: MmRect,
            _: Option<&crate::virtual_desktop::VirtualDesktopId>,
            current: Arc<dyn Fn() -> bool + Send + Sync>,
        ) -> Result<PlacementOutcome> {
            anyhow::ensure!(
                current() && self.foreground == target.native,
                "stale placement"
            );
            self.moved.push((target.native.hwnd, rect));
            if self.unrelated_after_first {
                self.foreground = WindowNativeIdentity {
                    hwnd: 90,
                    pid: 90,
                    tid: 90,
                };
            }
            Ok(PlacementOutcome {
                cleanup_warning: self.cleanup_warning.then(|| "cleanup failed".into()),
            })
        }
    }
    #[test]
    fn checked_placement_preserves_order_rect_fallback_and_foreground_chain() {
        let mut row = workspace();
        row.windows.push(row.windows[0].clone());
        let fallback = MmRect {
            x: -30,
            y: 5,
            w: 400,
            h: 300,
        };
        let specific = MmRect {
            x: 20,
            y: -9,
            w: 100,
            h: 80,
        };
        row.home_rect = Some(fallback);
        row.windows[1].home_rect = Some(specific);
        let overlay = ReconnectedWorkspace {
            workspace: row,
            identities: HashMap::from([(0, identity(10, 22)), (1, identity(11, 22))]),
            summary: ReconnectSummary::default(),
        };
        let issuer = WindowNativeIdentity {
            hwnd: 1,
            pid: 1,
            tid: 1,
        };
        let mut backend = FakePlacement {
            foreground: issuer,
            moved: vec![],
            unrelated_after_first: false,
            cleanup_warning: false,
            foreign_target: false,
            change_target_at_activation: false,
        };
        let report = place_checked(
            &overlay,
            issuer,
            Destination::Home,
            Arc::new(|| true),
            &mut backend,
        );
        assert_eq!(report.moved, 2);
        assert!(report.error.is_none());
        assert_eq!(backend.moved, vec![(10, fallback), (11, specific)]);
        let mut backend = FakePlacement {
            foreground: issuer,
            moved: vec![],
            unrelated_after_first: true,
            cleanup_warning: false,
            foreign_target: false,
            change_target_at_activation: false,
        };
        let report = place_checked(
            &overlay,
            issuer,
            Destination::Home,
            Arc::new(|| true),
            &mut backend,
        );
        assert_eq!(report.moved, 1);
        assert!(report.error.is_some());
        let mut backend = FakePlacement {
            foreground: issuer,
            moved: vec![],
            unrelated_after_first: false,
            cleanup_warning: true,
            foreign_target: false,
            change_target_at_activation: false,
        };
        let report = place_checked(
            &overlay,
            issuer,
            Destination::Home,
            Arc::new(|| true),
            &mut backend,
        );
        assert_eq!(report.moved, 1);
        assert_eq!(report.error.as_deref(), Some("cleanup failed"));
    }
    #[test]
    fn topmost_cleanup_depends_only_on_exact_identity() {
        let calls = std::cell::Cell::new(0);
        assert!(
            cleanup_after_move(
                true,
                || true,
                || {
                    calls.set(calls.get() + 1);
                    Ok(())
                }
            )
            .is_none()
        );
        identity_only_cleanup(
            || true,
            || {
                calls.set(calls.get() + 1);
                Ok(())
            },
        )
        .unwrap();
        identity_only_cleanup(
            || false,
            || {
                calls.set(calls.get() + 1);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn home_ignores_foreign_target_binding_but_target_rechecks_it() {
        let mut row = workspace();
        let rect = MmRect {
            x: 0,
            y: 0,
            w: 100,
            h: 100,
        };
        row.home_rect = Some(rect);
        row.target_rect = Some(rect);
        row.virtual_desktop = Some(crate::virtual_desktop::VirtualDesktopBinding {
            id: crate::virtual_desktop::VirtualDesktopId::parse(
                "550e8400-e29b-41d4-a716-446655440000",
            )
            .unwrap(),
            cached_name: None,
        });
        let overlay = ReconnectedWorkspace {
            workspace: row,
            identities: HashMap::from([(0, identity(10, 22))]),
            summary: ReconnectSummary::default(),
        };
        let issuer = WindowNativeIdentity {
            hwnd: 1,
            pid: 1,
            tid: 1,
        };
        let mut foreign = FakePlacement {
            foreground: issuer,
            moved: vec![],
            unrelated_after_first: false,
            cleanup_warning: false,
            foreign_target: true,
            change_target_at_activation: false,
        };
        assert_eq!(
            place_checked(
                &overlay,
                issuer,
                Destination::Home,
                Arc::new(|| true),
                &mut foreign
            )
            .moved,
            1
        );
        foreign.foreground = issuer;
        foreign.moved.clear();
        let target = place_checked(
            &overlay,
            issuer,
            Destination::Target,
            Arc::new(|| true),
            &mut foreign,
        );
        assert_eq!(target.moved, 0);
        assert_eq!(target.skipped, 1);
        let mut changed = FakePlacement {
            foreground: issuer,
            moved: vec![],
            unrelated_after_first: false,
            cleanup_warning: false,
            foreign_target: false,
            change_target_at_activation: true,
        };
        let target = place_checked(
            &overlay,
            issuer,
            Destination::Target,
            Arc::new(|| true),
            &mut changed,
        );
        assert_eq!(target.moved, 0);
        assert!(target.error.is_some());
    }
}
