//! Shared Windows foreground activation with explicit virtual-desktop policy.

use crate::virtual_desktop::{VirtualDesktopError, VirtualDesktopId, VirtualDesktopService};
use std::{collections::BTreeMap, fmt, time::Duration};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowDesktopPolicy {
    /// Switch to the desktop which already owns the window. Never relocate it.
    FollowWindow,
    /// Refuse to activate a window that is not already on the current desktop.
    CurrentDesktopOnly,
    /// Relocate the target onto the current desktop before activation.
    MoveToCurrentDesktop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowActivationRequest {
    pub hwnd: usize,
    pub desktop_policy: WindowDesktopPolicy,
}

impl WindowActivationRequest {
    pub const fn follow_window(hwnd: usize) -> Self {
        Self {
            hwnd,
            desktop_policy: WindowDesktopPolicy::FollowWindow,
        }
    }

    pub const fn move_to_current_desktop(hwnd: usize) -> Self {
        Self {
            hwnd,
            desktop_policy: WindowDesktopPolicy::MoveToCurrentDesktop,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowActivationErrorKind {
    InvalidWindow,
    Desktop,
    ForegroundDenied,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowActivationError {
    pub kind: WindowActivationErrorKind,
    pub message: String,
    pub context: BTreeMap<String, String>,
}

impl WindowActivationError {
    fn new(kind: WindowActivationErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            context: BTreeMap::new(),
        }
    }

    pub fn context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context.insert(key.into(), value.into());
        self
    }
}

impl fmt::Display for WindowActivationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(f)
    }
}

impl std::error::Error for WindowActivationError {}

pub fn activate_window(request: WindowActivationRequest) -> Result<(), WindowActivationError> {
    #[cfg(windows)]
    {
        activate_with(&mut WindowsActivationBackend, request)
    }
    #[cfg(not(windows))]
    {
        let _ = request;
        Err(WindowActivationError::new(
            WindowActivationErrorKind::ForegroundDenied,
            "Window activation is available only on Windows",
        ))
    }
}

trait ActivationBackend {
    fn is_window(&mut self, hwnd: usize) -> bool;
    fn window_desktop(&mut self, hwnd: usize) -> Result<VirtualDesktopId, VirtualDesktopError>;
    fn current_desktop(&mut self) -> Result<VirtualDesktopId, VirtualDesktopError>;
    fn is_window_on_current(&mut self, hwnd: usize) -> Result<bool, VirtualDesktopError>;
    fn switch_desktop(&mut self, desktop: &VirtualDesktopId) -> Result<(), VirtualDesktopError>;
    fn move_window(
        &mut self,
        hwnd: usize,
        desktop: &VirtualDesktopId,
    ) -> Result<(), VirtualDesktopError>;
    fn restore(&mut self, hwnd: usize) -> bool;
    fn is_minimized(&mut self, hwnd: usize) -> bool;
    fn foreground_window(&mut self) -> Option<usize>;
    fn window_thread(&mut self, hwnd: usize) -> u32;
    fn current_thread(&mut self) -> u32;
    fn attach_input(&mut self, from: u32, to: u32, attach: bool) -> Result<(), String>;
    fn bring_to_top(&mut self, hwnd: usize) -> bool;
    fn set_foreground(&mut self, hwnd: usize) -> bool;
    fn target_process(&mut self, hwnd: usize) -> u32;
    fn pause(&mut self, duration: Duration);
}

const FOREGROUND_VERIFY_DELAYS: [Duration; 4] = [
    Duration::from_millis(0),
    Duration::from_millis(10),
    Duration::from_millis(20),
    Duration::from_millis(40),
];
const RESTORE_VERIFY_DELAYS: [Duration; 5] = [
    Duration::from_millis(0),
    Duration::from_millis(10),
    Duration::from_millis(20),
    Duration::from_millis(40),
    Duration::from_millis(80),
];
const DESKTOP_TRANSITION_DELAYS: [Duration; 8] = [
    Duration::from_millis(0),
    Duration::from_millis(25),
    Duration::from_millis(50),
    Duration::from_millis(100),
    Duration::from_millis(150),
    Duration::from_millis(200),
    Duration::from_millis(250),
    Duration::from_millis(300),
];

fn activate_with(
    backend: &mut impl ActivationBackend,
    request: WindowActivationRequest,
) -> Result<(), WindowActivationError> {
    if request.hwnd == 0 || !backend.is_window(request.hwnd) {
        return Err(WindowActivationError::new(
            WindowActivationErrorKind::InvalidWindow,
            "Target window no longer exists",
        )
        .context("hwnd", request.hwnd.to_string()));
    }

    prepare_desktop(backend, request)?;
    let restore_result = backend.restore(request.hwnd);
    if backend.is_minimized(request.hwnd)
        && !wait_until(backend, &RESTORE_VERIFY_DELAYS, |backend| {
            !backend.is_minimized(request.hwnd)
        })
    {
        return Err(WindowActivationError::new(
            WindowActivationErrorKind::ForegroundDenied,
            "Target window did not restore before activation",
        )
        .context("hwnd", request.hwnd.to_string())
        .context("restore_request", restore_result.to_string()));
    }

    let direct_result = backend.set_foreground(request.hwnd);
    if wait_for_foreground(backend, request.hwnd) {
        return Ok(());
    }

    let bring_result = backend.bring_to_top(request.hwnd);
    let top_set_result = backend.set_foreground(request.hwnd);
    if wait_for_foreground(backend, request.hwnd) {
        return Ok(());
    }

    let current_thread = backend.current_thread();
    let foreground_thread = backend
        .foreground_window()
        .map(|hwnd| backend.window_thread(hwnd))
        .unwrap_or(0);
    let target_thread = backend.window_thread(request.hwnd);
    let target_pid = backend.target_process(request.hwnd);
    let mut guard = InputAttachmentGuard::new(backend);
    let mut attach_failures = Vec::new();
    for thread in [foreground_thread, target_thread] {
        if thread != 0
            && current_thread != 0
            && thread != current_thread
            && !guard.attached.contains(&(thread, current_thread))
        {
            if let Err(error) = guard.attach(thread, current_thread) {
                attach_failures.push(error);
            }
        }
    }

    let attached_bring_result = guard.backend().bring_to_top(request.hwnd);
    let fallback_result = guard.backend().set_foreground(request.hwnd);
    let activated = wait_for_foreground(guard.backend(), request.hwnd);
    let detach_failures = guard.finish();
    if !detach_failures.is_empty() {
        return Err(WindowActivationError::new(
            WindowActivationErrorKind::ForegroundDenied,
            "Window activation completed, but input queues could not be detached cleanly",
        )
        .context("hwnd", request.hwnd.to_string())
        .context("detach_failures", detach_failures.join("; "))
        .context("activation_observed", activated.to_string()));
    }
    if activated {
        return Ok(());
    }

    Err(WindowActivationError::new(
        WindowActivationErrorKind::ForegroundDenied,
        "Windows denied foreground activation after the direct and attached-input attempts",
    )
    .context("hwnd", request.hwnd.to_string())
    .context("target_pid", target_pid.to_string())
    .context("desktop_policy", format!("{:?}", request.desktop_policy))
    .context("direct_set_foreground", direct_result.to_string())
    .context("bring_to_top", bring_result.to_string())
    .context("top_set_foreground", top_set_result.to_string())
    .context("attached_bring_to_top", attached_bring_result.to_string())
    .context("fallback_set_foreground", fallback_result.to_string())
    .context("attach_failures", attach_failures.join("; "))
    .context(
        "possible_cause",
        "foreground-lock policy, elevation/UIPI mismatch, or an unresponsive target",
    ))
}

fn wait_until<B: ActivationBackend>(
    backend: &mut B,
    delays: &[Duration],
    mut condition: impl FnMut(&mut B) -> bool,
) -> bool {
    for &delay in delays {
        if !delay.is_zero() {
            backend.pause(delay);
        }
        if condition(backend) {
            return true;
        }
    }
    false
}

fn wait_for_foreground<B: ActivationBackend>(backend: &mut B, hwnd: usize) -> bool {
    wait_until(backend, &FOREGROUND_VERIFY_DELAYS, |backend| {
        backend.foreground_window() == Some(hwnd)
    })
}

struct InputAttachmentGuard<'a, B: ActivationBackend> {
    backend: &'a mut B,
    attached: Vec<(u32, u32)>,
}
impl<'a, B: ActivationBackend> InputAttachmentGuard<'a, B> {
    fn new(backend: &'a mut B) -> Self {
        Self {
            backend,
            attached: Vec::with_capacity(2),
        }
    }
    fn backend(&mut self) -> &mut B {
        self.backend
    }
    fn attach(&mut self, from: u32, to: u32) -> Result<(), String> {
        self.backend.attach_input(from, to, true)?;
        self.attached.push((from, to));
        Ok(())
    }
    fn finish(mut self) -> Vec<String> {
        let mut failures = Vec::new();
        while let Some((from, to)) = self.attached.pop() {
            if let Err(error) = self.backend.attach_input(from, to, false) {
                failures.push(error);
            }
        }
        failures
    }
}
impl<B: ActivationBackend> Drop for InputAttachmentGuard<'_, B> {
    fn drop(&mut self) {
        while let Some((from, to)) = self.attached.pop() {
            let _ = self.backend.attach_input(from, to, false);
        }
    }
}

fn prepare_desktop(
    backend: &mut impl ActivationBackend,
    request: WindowActivationRequest,
) -> Result<(), WindowActivationError> {
    let result = (|| -> Result<(), VirtualDesktopError> {
        match request.desktop_policy {
            WindowDesktopPolicy::FollowWindow => {
                if !backend.is_window_on_current(request.hwnd)? {
                    let target = backend.window_desktop(request.hwnd)?;
                    backend.switch_desktop(&target)?;
                    if !wait_until(backend, &DESKTOP_TRANSITION_DELAYS, |backend| {
                        backend.is_window_on_current(request.hwnd).unwrap_or(false)
                    }) {
                        return Err(VirtualDesktopError::new(
                            crate::virtual_desktop::VirtualDesktopErrorKind::Native,
                            "verify virtual desktop switch",
                            "Target desktop did not become current before activation",
                        ));
                    }
                }
                Ok(())
            }
            WindowDesktopPolicy::CurrentDesktopOnly => {
                if backend.is_window_on_current(request.hwnd)? {
                    Ok(())
                } else {
                    Err(VirtualDesktopError::new(
                        crate::virtual_desktop::VirtualDesktopErrorKind::InvalidWindow,
                        "activate window",
                        "Target window is on another virtual desktop",
                    ))
                }
            }
            WindowDesktopPolicy::MoveToCurrentDesktop => {
                if !backend.is_window_on_current(request.hwnd)? {
                    let current = backend.current_desktop()?;
                    backend.move_window(request.hwnd, &current)?;
                    if !wait_until(backend, &DESKTOP_TRANSITION_DELAYS, |backend| {
                        backend.is_window_on_current(request.hwnd).unwrap_or(false)
                    }) {
                        return Err(VirtualDesktopError::new(
                            crate::virtual_desktop::VirtualDesktopErrorKind::Native,
                            "verify window desktop move",
                            "Target window did not reach the current desktop before activation",
                        ));
                    }
                }
                Ok(())
            }
        }
    })();
    result.map_err(|error| {
        let mut activation_error =
            WindowActivationError::new(WindowActivationErrorKind::Desktop, error.message)
                .context("desktop_operation", error.operation)
                .context("desktop_policy", format!("{:?}", request.desktop_policy));
        for (key, value) in error.context {
            activation_error = activation_error.context(key, value);
        }
        activation_error
    })
}

#[cfg(windows)]
struct WindowsActivationBackend;

#[cfg(windows)]
impl ActivationBackend for WindowsActivationBackend {
    fn is_window(&mut self, hwnd: usize) -> bool {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::IsWindow;
        unsafe { IsWindow(HWND(hwnd as *mut _)) }.as_bool()
    }

    fn window_desktop(&mut self, hwnd: usize) -> Result<VirtualDesktopId, VirtualDesktopError> {
        use windows::Win32::Foundation::HWND;
        VirtualDesktopService.desktop_for_window(HWND(hwnd as *mut _))
    }

    fn current_desktop(&mut self) -> Result<VirtualDesktopId, VirtualDesktopError> {
        use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
        let foreground = unsafe { GetForegroundWindow() };
        if !foreground.0.is_null() {
            return VirtualDesktopService.desktop_for_window(foreground);
        }
        VirtualDesktopService.current().map(|desktop| desktop.id)
    }

    fn is_window_on_current(&mut self, hwnd: usize) -> Result<bool, VirtualDesktopError> {
        use windows::Win32::Foundation::HWND;
        VirtualDesktopService.is_window_on_current_desktop(HWND(hwnd as *mut _))
    }

    fn switch_desktop(&mut self, desktop: &VirtualDesktopId) -> Result<(), VirtualDesktopError> {
        VirtualDesktopService.switch_to_id(desktop)
    }

    fn move_window(
        &mut self,
        hwnd: usize,
        desktop: &VirtualDesktopId,
    ) -> Result<(), VirtualDesktopError> {
        use windows::Win32::Foundation::HWND;
        VirtualDesktopService.move_window_to_desktop(HWND(hwnd as *mut _), desktop)
    }

    fn restore(&mut self, hwnd: usize) -> bool {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{SW_RESTORE, ShowWindowAsync};
        unsafe { ShowWindowAsync(HWND(hwnd as *mut _), SW_RESTORE) }.as_bool()
    }

    fn is_minimized(&mut self, hwnd: usize) -> bool {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::IsIconic;
        unsafe { IsIconic(HWND(hwnd as *mut _)) }.as_bool()
    }

    fn foreground_window(&mut self) -> Option<usize> {
        use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
        let hwnd = unsafe { GetForegroundWindow() };
        (!hwnd.0.is_null()).then_some(hwnd.0 as usize)
    }

    fn window_thread(&mut self, hwnd: usize) -> u32 {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
        unsafe { GetWindowThreadProcessId(HWND(hwnd as *mut _), None) }
    }

    fn current_thread(&mut self) -> u32 {
        use windows::Win32::System::Threading::GetCurrentThreadId;
        unsafe { GetCurrentThreadId() }
    }

    fn attach_input(&mut self, from: u32, to: u32, attach: bool) -> Result<(), String> {
        use windows::Win32::System::Threading::AttachThreadInput;
        if unsafe { AttachThreadInput(from, to, attach) }.as_bool() {
            Ok(())
        } else {
            Err(windows::core::Error::from_win32().to_string())
        }
    }

    fn bring_to_top(&mut self, hwnd: usize) -> bool {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::BringWindowToTop;
        unsafe { BringWindowToTop(HWND(hwnd as *mut _)) }.is_ok()
    }

    fn set_foreground(&mut self, hwnd: usize) -> bool {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;
        unsafe { SetForegroundWindow(HWND(hwnd as *mut _)) }.as_bool()
    }

    fn target_process(&mut self, hwnd: usize) -> u32 {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(HWND(hwnd as *mut _), Some(&mut pid)) };
        pid
    }

    fn pause(&mut self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeBackend {
        events: Vec<String>,
        valid: bool,
        target: VirtualDesktopId,
        current: VirtualDesktopId,
        on_current: bool,
        foreground: Option<usize>,
        activate_on_attempt: usize,
        attempts: usize,
        attach_fail_at: Option<(u32, u32)>,
        detach_fail_at: Option<(u32, u32)>,
        transition_delay_after_action: usize,
        transition_checks_remaining: usize,
    }

    impl FakeBackend {
        fn new() -> Self {
            Self {
                events: Vec::new(),
                valid: true,
                target: id(2),
                current: id(1),
                on_current: false,
                foreground: Some(7),
                activate_on_attempt: 1,
                attempts: 0,
                attach_fail_at: None,
                detach_fail_at: None,
                transition_delay_after_action: 0,
                transition_checks_remaining: 0,
            }
        }
    }

    fn id(number: u32) -> VirtualDesktopId {
        VirtualDesktopId::parse(&format!("{number:08x}-0000-0000-0000-000000000000")).unwrap()
    }

    impl ActivationBackend for FakeBackend {
        fn is_window(&mut self, _: usize) -> bool {
            self.events.push("validate".into());
            self.valid
        }
        fn window_desktop(&mut self, _: usize) -> Result<VirtualDesktopId, VirtualDesktopError> {
            self.events.push("target_desktop".into());
            Ok(self.target.clone())
        }
        fn current_desktop(&mut self) -> Result<VirtualDesktopId, VirtualDesktopError> {
            self.events.push("current_desktop".into());
            Ok(self.current.clone())
        }
        fn is_window_on_current(&mut self, _: usize) -> Result<bool, VirtualDesktopError> {
            self.events.push("is_current".into());
            if self.transition_checks_remaining > 0 {
                self.transition_checks_remaining -= 1;
                return Ok(false);
            }
            Ok(self.on_current)
        }
        fn switch_desktop(
            &mut self,
            desktop: &VirtualDesktopId,
        ) -> Result<(), VirtualDesktopError> {
            self.events.push(format!("switch:{desktop}"));
            self.current = desktop.clone();
            self.on_current = true;
            self.transition_checks_remaining = self.transition_delay_after_action;
            Ok(())
        }
        fn move_window(
            &mut self,
            _: usize,
            desktop: &VirtualDesktopId,
        ) -> Result<(), VirtualDesktopError> {
            self.events.push(format!("move:{desktop}"));
            self.target = desktop.clone();
            self.on_current = true;
            self.transition_checks_remaining = self.transition_delay_after_action;
            Ok(())
        }
        fn restore(&mut self, _: usize) -> bool {
            self.events.push("restore".into());
            true
        }
        fn is_minimized(&mut self, _: usize) -> bool {
            false
        }
        fn foreground_window(&mut self) -> Option<usize> {
            self.events.push("foreground".into());
            self.foreground
        }
        fn window_thread(&mut self, hwnd: usize) -> u32 {
            self.events.push(format!("thread:{hwnd}"));
            if hwnd == 42 { 3 } else { 2 }
        }
        fn current_thread(&mut self) -> u32 {
            self.events.push("thread:current".into());
            1
        }
        fn attach_input(&mut self, from: u32, to: u32, attach: bool) -> Result<(), String> {
            self.events.push(format!("attach:{from}:{to}:{attach}"));
            let failure = if attach {
                self.attach_fail_at
            } else {
                self.detach_fail_at
            };
            if failure == Some((from, to)) {
                Err(format!("attach failure {from}->{to}"))
            } else {
                Ok(())
            }
        }
        fn bring_to_top(&mut self, hwnd: usize) -> bool {
            self.events.push(format!("top:{hwnd}"));
            false
        }
        fn set_foreground(&mut self, hwnd: usize) -> bool {
            self.attempts += 1;
            self.events.push(format!("set:{hwnd}"));
            if self.attempts >= self.activate_on_attempt {
                self.foreground = Some(hwnd);
                true
            } else {
                false
            }
        }
        fn target_process(&mut self, _: usize) -> u32 {
            99
        }
        fn pause(&mut self, duration: Duration) {
            self.events.push(format!("pause:{}", duration.as_millis()));
        }
    }

    #[test]
    fn follow_window_switches_then_activates_without_moving() {
        let mut backend = FakeBackend::new();
        activate_with(&mut backend, WindowActivationRequest::follow_window(42)).unwrap();
        assert_eq!(
            &backend.events[..6],
            [
                "validate",
                "is_current",
                "target_desktop",
                &format!("switch:{}", id(2)),
                "is_current",
                "restore"
            ]
        );
        assert!(
            !backend
                .events
                .iter()
                .any(|event| event.starts_with("move:"))
        );
    }

    #[test]
    fn move_to_current_is_explicit_and_does_not_switch() {
        let mut backend = FakeBackend::new();
        activate_with(
            &mut backend,
            WindowActivationRequest::move_to_current_desktop(42),
        )
        .unwrap();
        assert!(
            backend
                .events
                .iter()
                .any(|event| event.starts_with("move:"))
        );
        assert!(
            !backend
                .events
                .iter()
                .any(|event| event.starts_with("switch:"))
        );
    }

    #[test]
    fn follow_window_allows_an_animated_desktop_transition_to_settle() {
        let mut backend = FakeBackend::new();
        backend.transition_delay_after_action = 5;
        activate_with(&mut backend, WindowActivationRequest::follow_window(42)).unwrap();
        let pauses: Vec<_> = backend
            .events
            .iter()
            .filter(|event| event.starts_with("pause:"))
            .cloned()
            .collect();
        assert_eq!(
            pauses,
            [
                "pause:25",
                "pause:50",
                "pause:100",
                "pause:150",
                "pause:200"
            ]
        );
    }

    #[test]
    fn desktop_transition_wait_is_bounded_when_the_shell_never_settles() {
        let mut backend = FakeBackend::new();
        backend.transition_delay_after_action = usize::MAX;
        let error =
            activate_with(&mut backend, WindowActivationRequest::follow_window(42)).unwrap_err();
        assert_eq!(error.kind, WindowActivationErrorKind::Desktop);
        let pauses: Vec<_> = backend
            .events
            .iter()
            .filter(|event| event.starts_with("pause:"))
            .cloned()
            .collect();
        assert_eq!(
            pauses,
            [
                "pause:25",
                "pause:50",
                "pause:100",
                "pause:150",
                "pause:200",
                "pause:250",
                "pause:300"
            ]
        );
    }

    #[test]
    fn invalid_window_stops_before_desktop_or_foreground_calls() {
        let mut backend = FakeBackend::new();
        backend.valid = false;
        let error =
            activate_with(&mut backend, WindowActivationRequest::follow_window(42)).unwrap_err();
        assert_eq!(error.kind, WindowActivationErrorKind::InvalidWindow);
        assert_eq!(backend.events, ["validate"]);
    }

    #[test]
    fn fallback_attaches_and_always_detaches_in_reverse_order() {
        let mut backend = FakeBackend::new();
        backend.target = backend.current.clone();
        backend.activate_on_attempt = usize::MAX;
        let error =
            activate_with(&mut backend, WindowActivationRequest::follow_window(42)).unwrap_err();
        assert_eq!(error.kind, WindowActivationErrorKind::ForegroundDenied);
        let attachments: Vec<_> = backend
            .events
            .iter()
            .filter(|event| event.starts_with("attach:"))
            .cloned()
            .collect();
        assert_eq!(
            attachments,
            [
                "attach:2:1:true",
                "attach:3:1:true",
                "attach:3:1:false",
                "attach:2:1:false"
            ]
        );
    }

    #[test]
    fn current_desktop_only_rejects_cross_desktop_target_before_restore() {
        let mut backend = FakeBackend::new();
        let error = activate_with(
            &mut backend,
            WindowActivationRequest {
                hwnd: 42,
                desktop_policy: WindowDesktopPolicy::CurrentDesktopOnly,
            },
        )
        .unwrap_err();
        assert_eq!(error.kind, WindowActivationErrorKind::Desktop);
        assert_eq!(backend.events, ["validate", "is_current"]);
    }

    #[test]
    fn successful_fallback_still_detaches_every_attached_queue() {
        let mut backend = FakeBackend::new();
        backend.target = backend.current.clone();
        backend.on_current = true;
        backend.activate_on_attempt = 3;
        activate_with(&mut backend, WindowActivationRequest::follow_window(42)).unwrap();
        let attachments: Vec<_> = backend
            .events
            .iter()
            .filter(|event| event.starts_with("attach:"))
            .cloned()
            .collect();
        assert_eq!(
            attachments,
            [
                "attach:2:1:true",
                "attach:3:1:true",
                "attach:3:1:false",
                "attach:2:1:false"
            ]
        );
    }

    #[test]
    fn partial_attach_failure_detaches_only_successful_attachment() {
        let mut backend = FakeBackend::new();
        backend.target = backend.current.clone();
        backend.on_current = true;
        backend.activate_on_attempt = usize::MAX;
        backend.attach_fail_at = Some((3, 1));
        let error =
            activate_with(&mut backend, WindowActivationRequest::follow_window(42)).unwrap_err();
        assert_eq!(error.kind, WindowActivationErrorKind::ForegroundDenied);
        let attachments: Vec<_> = backend
            .events
            .iter()
            .filter(|e| e.starts_with("attach:"))
            .cloned()
            .collect();
        assert_eq!(
            attachments,
            ["attach:2:1:true", "attach:3:1:true", "attach:2:1:false"]
        );
    }

    #[test]
    fn detach_failure_is_reported_even_after_activation() {
        let mut backend = FakeBackend::new();
        backend.target = backend.current.clone();
        backend.on_current = true;
        backend.activate_on_attempt = 3;
        backend.detach_fail_at = Some((3, 1));
        let error =
            activate_with(&mut backend, WindowActivationRequest::follow_window(42)).unwrap_err();
        assert_eq!(error.kind, WindowActivationErrorKind::ForegroundDenied);
        assert_eq!(
            error.context.get("activation_observed").map(String::as_str),
            Some("true")
        );
        assert!(error.context["detach_failures"].contains("3->1"));
    }
}
