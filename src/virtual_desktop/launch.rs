use crate::actions::exec::{LaunchIdentity, launch_with_identity};
use crate::commands::VirtualDesktopLaunchPayload;
use crate::virtual_desktop::{
    VirtualDesktopId, VirtualDesktopSelector, VirtualDesktopService, VirtualDesktopSnapshot,
};
use crate::window_activation::{WindowActivationRequest, activate_window};
use crate::window_catalog::{WindowCatalog, WindowDescriptor};
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

const DISCOVERY_CADENCE: Duration = Duration::from_millis(250);
const CATALOG_WAIT: Duration = Duration::from_secs(2);

trait LaunchWorkflowBackend {
    fn monotonic_now(&mut self) -> Duration;
    fn desktop_snapshot(&mut self) -> anyhow::Result<VirtualDesktopSnapshot>;
    fn fresh_windows(&mut self, timeout: Duration) -> anyhow::Result<Arc<Vec<WindowDescriptor>>>;
    fn launch(&mut self, application: &str, args: Option<&str>) -> anyhow::Result<LaunchIdentity>;
    fn foreground_window(&mut self) -> Option<usize>;
    fn wait_for_input_idle(&mut self, pid: Option<u32>, timeout: Duration);
    fn pause(&mut self, duration: Duration);
    fn move_window(&mut self, hwnd: usize, desktop: &VirtualDesktopId) -> anyhow::Result<()>;
    fn switch_desktop(&mut self, desktop: &VirtualDesktopId) -> anyhow::Result<()>;
    fn activate(&mut self, hwnd: usize) -> anyhow::Result<()>;
}

struct ProductionLaunchBackend {
    catalog: Arc<WindowCatalog>,
    clock_origin: Instant,
}

impl LaunchWorkflowBackend for ProductionLaunchBackend {
    fn monotonic_now(&mut self) -> Duration {
        self.clock_origin.elapsed()
    }
    fn desktop_snapshot(&mut self) -> anyhow::Result<VirtualDesktopSnapshot> {
        VirtualDesktopService.snapshot().map_err(Into::into)
    }
    fn fresh_windows(&mut self, timeout: Duration) -> anyhow::Result<Arc<Vec<WindowDescriptor>>> {
        self.catalog
            .refresh_and_wait(timeout)
            .map(|snapshot| snapshot.windows)
            .map_err(|error| anyhow::anyhow!(error))
    }
    fn launch(&mut self, application: &str, args: Option<&str>) -> anyhow::Result<LaunchIdentity> {
        launch_with_identity(application, args)
    }
    fn foreground_window(&mut self) -> Option<usize> {
        #[cfg(windows)]
        {
            let hwnd = unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() };
            return (!hwnd.0.is_null()).then_some(hwnd.0 as usize);
        }
        #[cfg(not(windows))]
        None
    }
    fn wait_for_input_idle(&mut self, pid: Option<u32>, timeout: Duration) {
        #[cfg(windows)]
        if let Some(pid) = pid {
            use windows::Win32::Foundation::CloseHandle;
            use windows::Win32::System::Threading::{
                OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, WaitForInputIdle,
            };
            if let Ok(process) =
                unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }
            {
                let _ = unsafe {
                    WaitForInputIdle(process, timeout.as_millis().min(u32::MAX as u128) as u32)
                };
                let _ = unsafe { CloseHandle(process) };
            }
        }
        #[cfg(not(windows))]
        let _ = (pid, timeout);
    }
    fn pause(&mut self, duration: Duration) {
        std::thread::sleep(duration);
    }
    fn move_window(&mut self, hwnd: usize, desktop: &VirtualDesktopId) -> anyhow::Result<()> {
        #[cfg(windows)]
        return VirtualDesktopService
            .move_window_to_desktop(windows::Win32::Foundation::HWND(hwnd as *mut _), desktop)
            .map_err(Into::into);
        #[cfg(not(windows))]
        {
            let _ = (hwnd, desktop);
            anyhow::bail!("Window movement is available only on Windows")
        }
    }
    fn switch_desktop(&mut self, desktop: &VirtualDesktopId) -> anyhow::Result<()> {
        VirtualDesktopService
            .switch_to_id(desktop)
            .map_err(Into::into)
    }
    fn activate(&mut self, hwnd: usize) -> anyhow::Result<()> {
        activate_window(WindowActivationRequest::follow_window(hwnd)).map_err(Into::into)
    }
}

pub fn launch_on_desktop(
    payload: &VirtualDesktopLaunchPayload,
    catalog: Arc<WindowCatalog>,
) -> anyhow::Result<()> {
    launch_with_backend(
        payload,
        &mut ProductionLaunchBackend {
            catalog,
            clock_origin: Instant::now(),
        },
    )
}

fn launch_with_backend(
    payload: &VirtualDesktopLaunchPayload,
    backend: &mut impl LaunchWorkflowBackend,
) -> anyhow::Result<()> {
    let snapshot = backend.desktop_snapshot()?;
    let target = snapshot
        .resolve(&VirtualDesktopSelector::parse(&payload.target)?)?
        .clone();

    // The baseline must be a completed post-request generation before process creation. A stale
    // cached omission must never make an unrelated pre-existing window look newly launched.
    let before = backend.fresh_windows(CATALOG_WAIT)?;
    let before_hwnds = before
        .iter()
        .map(|window| window.hwnd)
        .collect::<HashSet<_>>();
    let foreground_before = backend.foreground_window();
    let launch = backend.launch(&payload.application, payload.args.as_deref())?;
    let timeout = Duration::from_millis(payload.timeout_ms.clamp(100, 120_000));
    let deadline = backend.monotonic_now().saturating_add(timeout);
    let idle_wait = remaining_until(deadline, backend.monotonic_now()).min(Duration::from_secs(1));
    if !idle_wait.is_zero() {
        backend.wait_for_input_idle(launch.pid, idle_wait);
    }
    let mut found = None;
    loop {
        let remaining = remaining_until(deadline, backend.monotonic_now());
        if remaining.is_zero() {
            break;
        }
        backend.pause(DISCOVERY_CADENCE.min(remaining));
        let remaining = remaining_until(deadline, backend.monotonic_now());
        if remaining.is_zero() {
            break;
        }
        let windows = backend.fresh_windows(CATALOG_WAIT.min(remaining))?;
        if remaining_until(deadline, backend.monotonic_now()).is_zero() {
            break;
        }
        let foreground_after = backend.foreground_window();
        if let Some(window) = select_launch_window(
            &before_hwnds,
            &windows,
            &launch,
            foreground_before,
            foreground_after,
        ) {
            found = Some(window.clone());
            break;
        }
    }
    let found = found.ok_or_else(|| anyhow::anyhow!(
        "Application was launched, but no movable top-level window could be identified within {} seconds",
        timeout.as_secs_f32()
    ))?;
    backend.move_window(found.hwnd, &target.id)?;
    if payload.follow {
        backend.switch_desktop(&target.id)?;
        backend.activate(found.hwnd)?;
    }
    Ok(())
}

fn remaining_until(deadline: Duration, now: Duration) -> Duration {
    deadline.saturating_sub(now)
}

pub(crate) fn select_launch_window<'a>(
    before: &HashSet<usize>,
    windows: &'a [WindowDescriptor],
    launch: &LaunchIdentity,
    foreground_before: Option<usize>,
    foreground_after: Option<usize>,
) -> Option<&'a WindowDescriptor> {
    if let Some(pid) = launch.pid
        && let Some(window) = windows.iter().find(|window| window.pid == pid)
    {
        return Some(window);
    }
    let target_name = Path::new(&launch.target)
        .file_name()
        .and_then(|name| name.to_str())?;
    let matches = |window: &&WindowDescriptor| {
        window
            .executable
            .as_deref()
            .is_some_and(|exe| exe.eq_ignore_ascii_case(target_name))
    };
    windows
        .iter()
        .filter(matches)
        .find(|window| !before.contains(&window.hwnd))
        .or_else(|| {
            let foreground = foreground_after.filter(|after| Some(*after) != foreground_before)?;
            windows
                .iter()
                .filter(matches)
                .find(|window| window.hwnd == foreground)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::virtual_desktop::{VirtualDesktopCapabilities, VirtualDesktopInfo};
    use std::collections::VecDeque;

    fn id(n: u32) -> VirtualDesktopId {
        VirtualDesktopId::parse(&format!("{n:08x}-0000-0000-0000-000000000000")).unwrap()
    }
    fn window(hwnd: usize, pid: u32, exe: &str) -> WindowDescriptor {
        WindowDescriptor {
            title: exe.into(),
            hwnd,
            pid,
            executable: Some(exe.into()),
            process_path: None,
            class_name: None,
        }
    }
    fn payload(follow: bool) -> VirtualDesktopLaunchPayload {
        VirtualDesktopLaunchPayload {
            target: "2".into(),
            application: "app.exe".into(),
            args: None,
            follow,
            timeout_ms: 500,
        }
    }

    struct FakeBackend {
        windows: VecDeque<anyhow::Result<Arc<Vec<WindowDescriptor>>>>,
        launch: Option<anyhow::Result<LaunchIdentity>>,
        foreground: VecDeque<Option<usize>>,
        now: Duration,
        idle_delay: Duration,
        refresh_delays: VecDeque<Duration>,
        refresh_timeouts: Vec<Duration>,
        log: Vec<String>,
    }
    impl FakeBackend {
        fn new(windows: Vec<anyhow::Result<Vec<WindowDescriptor>>>) -> Self {
            Self {
                windows: windows
                    .into_iter()
                    .map(|result| result.map(Arc::new))
                    .collect(),
                launch: Some(Ok(LaunchIdentity {
                    pid: Some(22),
                    target: "app.exe".into(),
                })),
                foreground: VecDeque::from([Some(1), Some(8), Some(8)]),
                now: Duration::ZERO,
                idle_delay: Duration::ZERO,
                refresh_delays: VecDeque::new(),
                refresh_timeouts: Vec::new(),
                log: Vec::new(),
            }
        }
    }
    impl LaunchWorkflowBackend for FakeBackend {
        fn monotonic_now(&mut self) -> Duration {
            self.now
        }
        fn desktop_snapshot(&mut self) -> anyhow::Result<VirtualDesktopSnapshot> {
            Ok(VirtualDesktopSnapshot {
                desktops: vec![
                    VirtualDesktopInfo {
                        id: id(1),
                        index: 1,
                        name: None,
                        is_current: true,
                    },
                    VirtualDesktopInfo {
                        id: id(2),
                        index: 2,
                        name: Some("Work".into()),
                        is_current: false,
                    },
                ],
                capabilities: VirtualDesktopCapabilities::default(),
            })
        }
        fn fresh_windows(
            &mut self,
            timeout: Duration,
        ) -> anyhow::Result<Arc<Vec<WindowDescriptor>>> {
            self.log.push("refresh".into());
            self.refresh_timeouts.push(timeout);
            let delay = self.refresh_delays.pop_front().unwrap_or_default();
            self.now = self.now.saturating_add(delay.min(timeout));
            self.windows
                .pop_front()
                .unwrap_or_else(|| Ok(Arc::new(Vec::new())))
        }
        fn launch(&mut self, _: &str, _: Option<&str>) -> anyhow::Result<LaunchIdentity> {
            self.log.push("launch".into());
            self.launch.take().unwrap()
        }
        fn foreground_window(&mut self) -> Option<usize> {
            self.foreground.pop_front().flatten()
        }
        fn wait_for_input_idle(&mut self, _: Option<u32>, timeout: Duration) {
            self.log.push("idle".into());
            self.now = self.now.saturating_add(self.idle_delay.min(timeout));
        }
        fn pause(&mut self, duration: Duration) {
            self.log.push("pause".into());
            self.now = self.now.saturating_add(duration);
        }
        fn move_window(&mut self, hwnd: usize, _: &VirtualDesktopId) -> anyhow::Result<()> {
            self.log.push(format!("move:{hwnd}"));
            Ok(())
        }
        fn switch_desktop(&mut self, _: &VirtualDesktopId) -> anyhow::Result<()> {
            self.log.push("switch".into());
            Ok(())
        }
        fn activate(&mut self, hwnd: usize) -> anyhow::Result<()> {
            self.log.push(format!("activate:{hwnd}"));
            Ok(())
        }
    }

    #[test]
    fn default_launch_moves_without_switching_or_activation() {
        let mut backend = FakeBackend::new(vec![
            Ok(vec![window(1, 1, "other.exe")]),
            Ok(vec![window(1, 1, "other.exe"), window(8, 22, "app.exe")]),
        ]);
        launch_with_backend(&payload(false), &mut backend).unwrap();
        assert_eq!(
            backend.log,
            ["refresh", "launch", "idle", "pause", "refresh", "move:8"]
        );
    }
    #[test]
    fn follow_orders_move_then_switch_then_activate() {
        let mut backend =
            FakeBackend::new(vec![Ok(Vec::new()), Ok(vec![window(8, 22, "app.exe")])]);
        launch_with_backend(&payload(true), &mut backend).unwrap();
        assert!(
            backend
                .log
                .ends_with(&["move:8".into(), "switch".into(), "activate:8".into()])
        );
    }
    #[test]
    fn single_instance_existing_window_is_reused_only_after_foreground_transition() {
        let existing = window(7, 70, "app.exe");
        let mut backend = FakeBackend::new(vec![Ok(vec![existing.clone()]), Ok(vec![existing])]);
        backend.launch = Some(Ok(LaunchIdentity {
            pid: None,
            target: "app.exe".into(),
        }));
        backend.foreground = VecDeque::from([Some(1), Some(7)]);
        launch_with_backend(&payload(false), &mut backend).unwrap();
        assert!(backend.log.ends_with(&["refresh".into(), "move:7".into()]));
    }
    #[test]
    fn stale_baseline_failure_prevents_launch() {
        let mut backend = FakeBackend::new(vec![Err(anyhow::anyhow!("baseline timeout"))]);
        assert!(
            launch_with_backend(&payload(false), &mut backend)
                .unwrap_err()
                .to_string()
                .contains("baseline timeout")
        );
        assert!(!backend.log.contains(&"launch".into()));
    }
    #[test]
    fn timeout_rejects_unrelated_windows_and_does_not_move_or_kill() {
        let mut backend = FakeBackend::new(vec![
            Ok(vec![window(1, 1, "other.exe")]),
            Ok(vec![window(1, 1, "other.exe")]),
            Ok(vec![window(2, 2, "unrelated.exe")]),
        ]);
        assert!(launch_with_backend(&payload(false), &mut backend).is_err());
        assert!(
            !backend
                .log
                .iter()
                .any(|entry| entry.starts_with("move") || entry == "kill")
        );
    }
    #[test]
    fn matching_preexisting_window_without_a_foreground_transition_is_not_reused() {
        let existing = window(7, 70, "app.exe");
        let mut backend = FakeBackend::new(vec![
            Ok(vec![existing.clone()]),
            Ok(vec![existing.clone()]),
            Ok(vec![existing]),
        ]);
        backend.launch = Some(Ok(LaunchIdentity {
            pid: None,
            target: "app.exe".into(),
        }));
        backend.foreground = VecDeque::from([Some(1), Some(1), Some(1)]);
        assert!(launch_with_backend(&payload(false), &mut backend).is_err());
        assert!(!backend.log.iter().any(|entry| entry.starts_with("move")));
    }
    #[test]
    fn slow_refresh_is_capped_by_the_real_monotonic_deadline() {
        let mut backend =
            FakeBackend::new(vec![Ok(Vec::new()), Ok(vec![window(8, 22, "app.exe")])]);
        backend.refresh_delays = VecDeque::from([Duration::ZERO, Duration::from_secs(5)]);
        assert!(launch_with_backend(&payload(false), &mut backend).is_err());
        assert_eq!(backend.now, Duration::from_millis(500));
        assert_eq!(
            backend.refresh_timeouts,
            [CATALOG_WAIT, Duration::from_millis(250)]
        );
        assert!(!backend.log.iter().any(|entry| entry.starts_with("move")));
    }
    #[test]
    fn launch_and_refresh_errors_propagate_without_movement() {
        let mut launch_error = FakeBackend::new(vec![Ok(Vec::new())]);
        launch_error.launch = Some(Err(anyhow::anyhow!("spawn failed")));
        assert!(
            launch_with_backend(&payload(false), &mut launch_error)
                .unwrap_err()
                .to_string()
                .contains("spawn failed")
        );
        let mut refresh_error =
            FakeBackend::new(vec![Ok(Vec::new()), Err(anyhow::anyhow!("refresh failed"))]);
        assert!(
            launch_with_backend(&payload(false), &mut refresh_error)
                .unwrap_err()
                .to_string()
                .contains("refresh failed")
        );
    }
}
