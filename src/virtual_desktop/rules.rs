use super::{
    VirtualDesktopBinding, VirtualDesktopId, VirtualDesktopService, VirtualDesktopSnapshot,
};
use crate::window_activation::{WindowActivationRequest, activate_window};
use crate::window_catalog::WindowDescriptor;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, mpsc};

const SUPPRESSION_MILLIS: u64 = 500;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct VirtualDesktopRule {
    pub id: String,
    pub enabled: bool,
    #[serde(alias = "process", alias = "application")]
    pub executable: String,
    #[serde(alias = "path")]
    pub process_path: String,
    #[serde(alias = "window_title")]
    pub title: String,
    #[serde(alias = "window_class")]
    pub class_name: String,
    pub target: Option<VirtualDesktopBinding>,
}

impl VirtualDesktopRule {
    pub fn matches(&self, window: &WindowDescriptor) -> bool {
        if !self.enabled || self.executable.trim().is_empty() {
            return false;
        }
        let configured_executable = file_name(self.executable.trim());
        let actual_executable = window
            .executable
            .as_deref()
            .or_else(|| window.process_path.as_deref().map(file_name));
        if !actual_executable
            .is_some_and(|actual| actual.eq_ignore_ascii_case(configured_executable))
        {
            return false;
        }
        optional_exact(&self.process_path, window.process_path.as_deref())
            && optional_contains(&self.title, Some(&window.title))
            && optional_exact(&self.class_name, window.class_name.as_deref())
    }
}

fn file_name(value: &str) -> &str {
    value.rsplit(['\\', '/']).next().unwrap_or(value)
}

fn optional_exact(expected: &str, actual: Option<&str>) -> bool {
    expected.trim().is_empty()
        || actual.is_some_and(|actual| actual.trim().eq_ignore_ascii_case(expected.trim()))
}

fn optional_contains(expected: &str, actual: Option<&str>) -> bool {
    expected.trim().is_empty()
        || actual.is_some_and(|actual| {
            actual
                .to_ascii_lowercase()
                .contains(&expected.trim().to_ascii_lowercase())
        })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleSuppression {
    hwnd: usize,
    target: VirtualDesktopId,
    expires_at_ms: u64,
}

impl RuleSuppression {
    fn arm(hwnd: usize, target: VirtualDesktopId, now_ms: u64) -> Self {
        Self {
            hwnd,
            target,
            expires_at_ms: now_ms.saturating_add(SUPPRESSION_MILLIS),
        }
    }

    pub fn matches(&self, hwnd: usize, target: &VirtualDesktopId, now_ms: u64) -> bool {
        self.hwnd == hwnd && &self.target == target && now_ms <= self.expires_at_ms
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RulePlan {
    NoMatch,
    MissingTarget {
        rule_id: String,
    },
    Suppressed {
        rule_id: String,
    },
    AlreadyCorrect {
        rule_id: String,
    },
    Apply {
        rule_id: String,
        hwnd: usize,
        target: VirtualDesktopId,
    },
}

pub fn plan_foreground_rule(
    rules: &[VirtualDesktopRule],
    window: &WindowDescriptor,
    window_desktop: &VirtualDesktopId,
    snapshot: &VirtualDesktopSnapshot,
    suppression: Option<&RuleSuppression>,
    now_ms: u64,
) -> Result<RulePlan, String> {
    let Some(rule) = rules.iter().find(|rule| rule.matches(window)) else {
        return Ok(RulePlan::NoMatch);
    };
    let Some(binding) = rule.target.as_ref() else {
        return Ok(RulePlan::MissingTarget {
            rule_id: rule.id.clone(),
        });
    };
    let target = snapshot
        .resolve_binding(binding)
        .map_err(|error| format!("rule {:?}: {error}", rule.id))?
        .id
        .clone();
    if suppression.is_some_and(|suppression| suppression.matches(window.hwnd, &target, now_ms)) {
        return Ok(RulePlan::Suppressed {
            rule_id: rule.id.clone(),
        });
    }
    if window_desktop == &target {
        return Ok(RulePlan::AlreadyCorrect {
            rule_id: rule.id.clone(),
        });
    }
    Ok(RulePlan::Apply {
        rule_id: rule.id.clone(),
        hwnd: window.hwnd,
        target,
    })
}

trait RuleBackend {
    fn current_foreground(&mut self) -> Option<usize>;
    fn describe_window(&mut self, hwnd: usize) -> Option<WindowDescriptor>;
    fn snapshot(&mut self) -> Result<VirtualDesktopSnapshot, String>;
    fn desktop_for_window(&mut self, hwnd: usize) -> Result<VirtualDesktopId, String>;
    fn move_window(&mut self, hwnd: usize, target: &VirtualDesktopId) -> Result<(), String>;
    fn switch_desktop(&mut self, target: &VirtualDesktopId) -> Result<(), String>;
    fn activate(&mut self, hwnd: usize) -> Result<(), String>;
    fn now_ms(&mut self) -> u64;
    fn begin_programmatic_foreground(&mut self, _hwnd: usize) {}
    fn end_programmatic_foreground(&mut self, _hwnd: usize, _successful: bool) {}
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum RuleEventOutcome {
    NoOp(RulePlan),
    Applied { rule_id: String },
    Diagnostic(String),
}

fn process_foreground_event(
    backend: &mut impl RuleBackend,
    rules: &[VirtualDesktopRule],
    hwnd: usize,
    suppression: &mut Option<RuleSuppression>,
) -> RuleEventOutcome {
    if backend.current_foreground() != Some(hwnd) {
        return RuleEventOutcome::NoOp(RulePlan::NoMatch);
    }
    let Some(window) = backend.describe_window(hwnd) else {
        return RuleEventOutcome::NoOp(RulePlan::NoMatch);
    };
    let Some(rule) = rules.iter().find(|rule| rule.matches(&window)) else {
        return RuleEventOutcome::NoOp(RulePlan::NoMatch);
    };
    let snapshot = match backend.snapshot() {
        Ok(snapshot) => snapshot,
        Err(error) => {
            return RuleEventOutcome::Diagnostic(format!(
                "Virtual desktop rule {:?} could not enumerate desktops: {error}",
                rule.id
            ));
        }
    };
    let window_desktop = match backend.desktop_for_window(hwnd) {
        Ok(desktop) => desktop,
        Err(error) => {
            return RuleEventOutcome::Diagnostic(format!(
                "Virtual desktop rule {:?} could not determine the window desktop: {error}",
                rule.id
            ));
        }
    };
    let now_ms = backend.now_ms();
    let plan = match plan_foreground_rule(
        rules,
        &window,
        &window_desktop,
        &snapshot,
        suppression.as_ref(),
        now_ms,
    ) {
        Ok(plan) => plan,
        Err(error) => return RuleEventOutcome::Diagnostic(error),
    };
    let (rule_id, hwnd, target) = match plan {
        RulePlan::Apply {
            rule_id,
            hwnd,
            target,
        } => (rule_id, hwnd, target),
        RulePlan::MissingTarget { rule_id } => {
            return RuleEventOutcome::Diagnostic(format!(
                "Virtual desktop rule {rule_id:?} has no target desktop"
            ));
        }
        plan => return RuleEventOutcome::NoOp(plan),
    };
    // Metadata and desktop discovery can involve enough work for the foreground window to
    // change. Revalidate at the effect boundary so an old event can never move a window after
    // a newer foreground event has arrived; the queued latest event will be handled next.
    if backend.current_foreground() != Some(hwnd) {
        return RuleEventOutcome::NoOp(RulePlan::NoMatch);
    }
    backend.begin_programmatic_foreground(hwnd);
    if let Err(error) = backend.move_window(hwnd, &target) {
        backend.end_programmatic_foreground(hwnd, false);
        return RuleEventOutcome::Diagnostic(format!(
            "Virtual desktop rule {rule_id:?} could not move its foreground window: {error}"
        ));
    }
    if let Err(error) = backend.switch_desktop(&target) {
        backend.end_programmatic_foreground(hwnd, false);
        return RuleEventOutcome::Diagnostic(format!(
            "Virtual desktop rule {rule_id:?} moved the window but could not switch desktops: {error}"
        ));
    }
    if let Err(error) = backend.activate(hwnd) {
        backend.end_programmatic_foreground(hwnd, false);
        return RuleEventOutcome::Diagnostic(format!(
            "Virtual desktop rule {rule_id:?} moved and switched but could not restore foreground activation: {error}"
        ));
    }
    backend.end_programmatic_foreground(hwnd, true);
    *suppression = Some(RuleSuppression::arm(hwnd, target, backend.now_ms()));
    RuleEventOutcome::Applied { rule_id }
}

#[cfg(windows)]
struct ProductionRuleBackend {
    started: std::time::Instant,
    signal: LatestForegroundSignal,
}

#[cfg(windows)]
impl RuleBackend for ProductionRuleBackend {
    fn current_foreground(&mut self) -> Option<usize> {
        let hwnd = unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() };
        (!hwnd.0.is_null()).then_some(hwnd.0 as usize)
    }

    fn describe_window(&mut self, hwnd: usize) -> Option<WindowDescriptor> {
        crate::window_catalog::describe_window(hwnd)
    }

    fn snapshot(&mut self) -> Result<VirtualDesktopSnapshot, String> {
        VirtualDesktopService
            .snapshot()
            .map_err(|error| error.to_string())
    }

    fn desktop_for_window(&mut self, hwnd: usize) -> Result<VirtualDesktopId, String> {
        VirtualDesktopService
            .desktop_for_window(windows::Win32::Foundation::HWND(hwnd as *mut _))
            .map_err(|error| error.to_string())
    }

    fn move_window(&mut self, hwnd: usize, target: &VirtualDesktopId) -> Result<(), String> {
        VirtualDesktopService
            .move_window_to_desktop(windows::Win32::Foundation::HWND(hwnd as *mut _), target)
            .map_err(|error| error.to_string())
    }

    fn switch_desktop(&mut self, target: &VirtualDesktopId) -> Result<(), String> {
        VirtualDesktopService
            .switch_to_id(target)
            .map_err(|error| error.to_string())
    }

    fn activate(&mut self, hwnd: usize) -> Result<(), String> {
        activate_window(WindowActivationRequest::follow_window(hwnd))
            .map_err(|error| error.to_string())
    }

    fn now_ms(&mut self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    fn begin_programmatic_foreground(&mut self, hwnd: usize) {
        self.signal.ignore(hwnd);
    }

    fn end_programmatic_foreground(&mut self, hwnd: usize, successful: bool) {
        self.signal.finish_ignoring(hwnd, successful);
    }
}

const PROGRAMMATIC_EVENT_GRACE_MS: u64 = 1_500;
const FAILED_EVENT_GRACE_MS: u64 = 150;

struct IgnoredForeground {
    hwnd: std::sync::atomic::AtomicUsize,
    expires_at_ms: std::sync::atomic::AtomicU64,
}

impl IgnoredForeground {
    fn new() -> Self {
        Self {
            hwnd: std::sync::atomic::AtomicUsize::new(0),
            expires_at_ms: std::sync::atomic::AtomicU64::new(0),
        }
    }
}

struct LatestForegroundSignal {
    latest: Arc<std::sync::atomic::AtomicUsize>,
    ignored: Arc<[IgnoredForeground; 4]>,
    started: Arc<std::time::Instant>,
    wake: mpsc::SyncSender<()>,
}

impl Clone for LatestForegroundSignal {
    fn clone(&self) -> Self {
        Self {
            latest: Arc::clone(&self.latest),
            ignored: Arc::clone(&self.ignored),
            started: Arc::clone(&self.started),
            wake: self.wake.clone(),
        }
    }
}

impl LatestForegroundSignal {
    fn publish(&self, hwnd: usize) {
        self.latest
            .store(hwnd, std::sync::atomic::Ordering::Release);
        let _ = self.wake.try_send(());
    }

    fn publish_native(&self, hwnd: usize) {
        self.publish_native_at(hwnd, self.elapsed_ms());
    }

    fn publish_native_at(&self, hwnd: usize, now_ms: u64) {
        for marker in self.ignored.iter() {
            if marker.hwnd.load(std::sync::atomic::Ordering::Acquire) != hwnd {
                continue;
            }
            let expires_at = marker
                .expires_at_ms
                .load(std::sync::atomic::Ordering::Acquire);
            if now_ms <= expires_at
                && marker
                    .hwnd
                    .compare_exchange(
                        hwnd,
                        0,
                        std::sync::atomic::Ordering::AcqRel,
                        std::sync::atomic::Ordering::Acquire,
                    )
                    .is_ok()
            {
                return;
            }
            if now_ms > expires_at {
                let _ = marker.hwnd.compare_exchange(
                    hwnd,
                    0,
                    std::sync::atomic::Ordering::AcqRel,
                    std::sync::atomic::Ordering::Acquire,
                );
            }
        }
        self.publish(hwnd);
    }

    fn ignore(&self, hwnd: usize) {
        self.ignore_at(hwnd, self.elapsed_ms(), PROGRAMMATIC_EVENT_GRACE_MS);
    }

    fn ignore_at(&self, hwnd: usize, now_ms: u64, grace_ms: u64) {
        let expires_at = now_ms.saturating_add(grace_ms);
        for marker in self.ignored.iter() {
            let current = marker.hwnd.load(std::sync::atomic::Ordering::Acquire);
            if current == hwnd {
                marker
                    .expires_at_ms
                    .store(expires_at, std::sync::atomic::Ordering::Release);
                return;
            }
            if current == 0 {
                marker
                    .expires_at_ms
                    .store(expires_at, std::sync::atomic::Ordering::Relaxed);
            }
            if current == 0
                && marker
                    .hwnd
                    .compare_exchange(
                        0,
                        hwnd,
                        std::sync::atomic::Ordering::AcqRel,
                        std::sync::atomic::Ordering::Acquire,
                    )
                    .is_ok()
            {
                return;
            }
        }
        // Four unobserved programmatic transitions is already an abnormal shell backlog. Keep
        // the newest marker rather than allowing it to overwrite a genuine foreground signal.
        self.ignored[0]
            .expires_at_ms
            .store(expires_at, std::sync::atomic::Ordering::Relaxed);
        self.ignored[0]
            .hwnd
            .store(hwnd, std::sync::atomic::Ordering::Release);
    }

    fn finish_ignoring(&self, hwnd: usize, successful: bool) {
        self.finish_ignoring_at(hwnd, successful, self.elapsed_ms());
    }

    fn finish_ignoring_at(&self, hwnd: usize, successful: bool, now_ms: u64) {
        let expires_at = now_ms.saturating_add(if successful {
            PROGRAMMATIC_EVENT_GRACE_MS
        } else {
            FAILED_EVENT_GRACE_MS
        });
        // If the callback already consumed the marker during the operation, do not re-arm it:
        // there is no outstanding synthetic event left to suppress.
        if let Some(marker) = self
            .ignored
            .iter()
            .find(|marker| marker.hwnd.load(std::sync::atomic::Ordering::Acquire) == hwnd)
        {
            marker
                .expires_at_ms
                .store(expires_at, std::sync::atomic::Ordering::Release);
        }
    }

    fn elapsed_ms(&self) -> u64 {
        self.started.elapsed().as_millis().min(u64::MAX as u128) as u64
    }

    fn take_latest(&self) -> Option<usize> {
        let hwnd = self.latest.swap(0, std::sync::atomic::Ordering::AcqRel);
        (hwnd != 0).then_some(hwnd)
    }
}

pub(crate) trait RuleRuntimeHandle: Send + Sync {
    fn shutdown(self: Box<Self>);
}

pub(crate) trait RuleRuntimeFactory: Send + Sync {
    fn start(&self, rules: Arc<Vec<VirtualDesktopRule>>) -> Option<Box<dyn RuleRuntimeHandle>>;
}

#[derive(Default)]
pub(crate) struct NativeRuleRuntimeFactory;

impl RuleRuntimeFactory for NativeRuleRuntimeFactory {
    fn start(&self, rules: Arc<Vec<VirtualDesktopRule>>) -> Option<Box<dyn RuleRuntimeHandle>> {
        NativeRuleRuntime::start(rules)
            .map(|runtime| Box::new(runtime) as Box<dyn RuleRuntimeHandle>)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RuleRuntimeStatus {
    Disabled,
    Running,
    Unavailable,
}

pub(crate) struct RuleRuntimeController {
    factory: Arc<dyn RuleRuntimeFactory>,
    enabled_rules: Vec<VirtualDesktopRule>,
    runtime: Option<Box<dyn RuleRuntimeHandle>>,
    status: RuleRuntimeStatus,
}

impl Default for RuleRuntimeController {
    fn default() -> Self {
        Self::new(Arc::new(NativeRuleRuntimeFactory))
    }
}

impl RuleRuntimeController {
    pub(crate) fn new(factory: Arc<dyn RuleRuntimeFactory>) -> Self {
        Self {
            factory,
            enabled_rules: Vec::new(),
            runtime: None,
            status: RuleRuntimeStatus::Disabled,
        }
    }

    pub(crate) fn reconcile(&mut self, rules: &[VirtualDesktopRule]) {
        let enabled = rules
            .iter()
            .filter(|rule| rule.enabled)
            .cloned()
            .collect::<Vec<_>>();
        if enabled == self.enabled_rules {
            return;
        }
        self.stop();
        self.enabled_rules = enabled;
        if self.enabled_rules.is_empty() {
            self.status = RuleRuntimeStatus::Disabled;
            return;
        }
        self.runtime = self.factory.start(Arc::new(self.enabled_rules.clone()));
        self.status = if self.runtime.is_some() {
            RuleRuntimeStatus::Running
        } else {
            RuleRuntimeStatus::Unavailable
        };
    }

    pub(crate) fn status(&self) -> RuleRuntimeStatus {
        self.status
    }

    fn stop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown();
        }
    }
}

impl Drop for RuleRuntimeController {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(windows)]
thread_local! {
    static RULE_EVENT_SIGNAL: std::cell::RefCell<Option<LatestForegroundSignal>> =
        const { std::cell::RefCell::new(None) };
}

struct NativeRuleRuntime {
    #[cfg(windows)]
    cancel: Arc<std::sync::atomic::AtomicBool>,
    wake: Option<mpsc::SyncSender<()>>,
    #[cfg(windows)]
    hook_thread_id: u32,
    hook_join: Option<std::thread::JoinHandle<()>>,
    worker_join: Option<std::thread::JoinHandle<()>>,
}

impl NativeRuleRuntime {
    fn start(rules: Arc<Vec<VirtualDesktopRule>>) -> Option<Self> {
        #[cfg(windows)]
        {
            use std::sync::atomic::{AtomicBool, Ordering};
            use std::sync::mpsc::sync_channel;
            use std::time::Instant;
            use windows::Win32::Foundation::HWND;
            use windows::Win32::System::Threading::GetCurrentThreadId;
            use windows::Win32::UI::Accessibility::{
                HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent,
            };
            use windows::Win32::UI::WindowsAndMessaging::{
                DispatchMessageW, EVENT_SYSTEM_FOREGROUND, GetMessageW, MSG, PM_NOREMOVE,
                PeekMessageW, TranslateMessage, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
            };

            unsafe extern "system" fn callback(
                _: HWINEVENTHOOK,
                _: u32,
                hwnd: HWND,
                _: i32,
                _: i32,
                _: u32,
                _: u32,
            ) {
                if hwnd.0.is_null() {
                    return;
                }
                RULE_EVENT_SIGNAL.with(|slot| {
                    if let Ok(signal) = slot.try_borrow()
                        && let Some(signal) = signal.as_ref()
                    {
                        signal.publish_native(hwnd.0 as usize);
                    }
                });
            }

            let (wake_tx, wake_rx) = sync_channel(1);
            let signal = LatestForegroundSignal {
                latest: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
                ignored: Arc::new(std::array::from_fn(|_| IgnoredForeground::new())),
                started: Arc::new(Instant::now()),
                wake: wake_tx.clone(),
            };
            let (ready_tx, ready_rx) = sync_channel(0);
            let cancel = Arc::new(AtomicBool::new(false));
            let worker_cancel = Arc::clone(&cancel);
            let worker_signal = signal.clone();
            let worker_join = std::thread::Builder::new()
                .name("virtual-desktop-rule-actions".into())
                .spawn(move || {
                    let mut backend = ProductionRuleBackend {
                        started: Instant::now(),
                        signal: worker_signal.clone(),
                    };
                    let mut suppression = None;
                    while wake_rx.recv().is_ok() {
                        if worker_cancel.load(Ordering::Acquire) {
                            break;
                        }
                        let Some(hwnd) = worker_signal.take_latest() else {
                            continue;
                        };
                        match process_foreground_event(&mut backend, &rules, hwnd, &mut suppression)
                        {
                            RuleEventOutcome::Diagnostic(message) => tracing::warn!("{message}"),
                            RuleEventOutcome::Applied { rule_id } => {
                                tracing::debug!(
                                    rule_id,
                                    hwnd,
                                    "applied virtual desktop foreground rule"
                                )
                            }
                            RuleEventOutcome::NoOp(_) => {}
                        }
                    }
                })
                .ok()?;
            let hook_signal = signal;
            let hook_cancel = Arc::clone(&cancel);
            let hook_join = match std::thread::Builder::new()
                .name("virtual-desktop-rules".into())
                .spawn(move || {
                    RULE_EVENT_SIGNAL.with(|slot| *slot.borrow_mut() = Some(hook_signal));
                    let mut queued = MSG::default();
                    let _ = unsafe { PeekMessageW(&mut queued, None, 0, 0, PM_NOREMOVE) };
                    let thread_id = unsafe { GetCurrentThreadId() };
                    let hook = unsafe {
                        SetWinEventHook(
                            EVENT_SYSTEM_FOREGROUND,
                            EVENT_SYSTEM_FOREGROUND,
                            None,
                            Some(callback),
                            0,
                            0,
                            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
                        )
                    };
                    let usable = !hook.0.is_null();
                    let _ = ready_tx.send((usable, thread_id));
                    if usable {
                        let mut message = MSG::default();
                        while unsafe { GetMessageW(&mut message, None, 0, 0) }.0 > 0 {
                            let _ = unsafe { TranslateMessage(&message) };
                            unsafe { DispatchMessageW(&message) };
                            if hook_cancel.load(Ordering::Acquire) {
                                break;
                            }
                        }
                    }
                    if !hook.0.is_null() {
                        let _ = unsafe { UnhookWinEvent(hook) };
                    }
                    RULE_EVENT_SIGNAL.with(|slot| *slot.borrow_mut() = None);
                }) {
                Ok(join) => join,
                Err(_) => {
                    cancel.store(true, Ordering::Release);
                    let _ = wake_tx.try_send(());
                    let _ = worker_join.join();
                    return None;
                }
            };
            let (usable, hook_thread_id) = match ready_rx.recv() {
                Ok(ready) => ready,
                Err(_) => {
                    cancel.store(true, Ordering::Release);
                    let _ = wake_tx.try_send(());
                    let _ = hook_join.join();
                    let _ = worker_join.join();
                    return None;
                }
            };
            if !usable {
                cancel.store(true, Ordering::Release);
                let _ = wake_tx.try_send(());
                let _ = hook_join.join();
                let _ = worker_join.join();
                return None;
            }
            Some(Self {
                cancel,
                wake: Some(wake_tx),
                hook_thread_id,
                hook_join: Some(hook_join),
                worker_join: Some(worker_join),
            })
        }
        #[cfg(not(windows))]
        {
            let _ = rules;
            None
        }
    }
}

impl RuleRuntimeHandle for NativeRuleRuntime {
    fn shutdown(mut self: Box<Self>) {
        #[cfg(windows)]
        {
            use windows::Win32::Foundation::{LPARAM, WPARAM};
            use windows::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_QUIT};
            self.cancel
                .store(true, std::sync::atomic::Ordering::Release);
            if let Some(wake) = self.wake.take() {
                let _ = wake.try_send(());
            }
            let _ =
                unsafe { PostThreadMessageW(self.hook_thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
        }
        if let Some(join) = self.hook_join.take() {
            let _ = join.join();
        }
        if let Some(join) = self.worker_join.take() {
            let _ = join.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::virtual_desktop::{VirtualDesktopCapabilities, VirtualDesktopInfo};
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn id(number: u32) -> VirtualDesktopId {
        VirtualDesktopId::parse(&format!("{number:08x}-0000-0000-0000-000000000000")).unwrap()
    }

    fn snapshot() -> VirtualDesktopSnapshot {
        VirtualDesktopSnapshot {
            desktops: vec![
                VirtualDesktopInfo {
                    id: id(1),
                    index: 1,
                    name: Some("Work".into()),
                    is_current: true,
                },
                VirtualDesktopInfo {
                    id: id(2),
                    index: 2,
                    name: Some("Gaming".into()),
                    is_current: false,
                },
            ],
            capabilities: VirtualDesktopCapabilities::default(),
        }
    }

    fn rule() -> VirtualDesktopRule {
        VirtualDesktopRule {
            id: "runelite-gaming".into(),
            enabled: true,
            executable: "RuneLite.exe".into(),
            process_path: "C:\\Games\\RuneLite.exe".into(),
            title: "Old School".into(),
            class_name: "SunAwtFrame".into(),
            target: Some(VirtualDesktopBinding {
                id: id(2),
                cached_name: Some("Gaming".into()),
            }),
        }
    }

    fn window(hwnd: usize) -> WindowDescriptor {
        WindowDescriptor {
            hwnd,
            pid: 7,
            title: "Old School RuneScape".into(),
            executable: Some("runelite.EXE".into()),
            process_path: Some("c:\\games\\runelite.exe".into()),
            class_name: Some("sunawtframe".into()),
        }
    }

    #[test]
    fn matcher_supports_process_path_title_and_class_without_weakening_process_identity() {
        let base_rule = rule();
        assert!(base_rule.matches(&window(42)));
        let mut executable = base_rule.clone();
        executable.executable = "Other.exe".into();
        assert!(!executable.matches(&window(42)));
        let mut title = base_rule.clone();
        title.title = "different".into();
        assert!(!title.matches(&window(42)));
        let mut class = base_rule.clone();
        class.class_name = "OtherClass".into();
        assert!(!class.matches(&window(42)));
        let mut path = base_rule;
        path.process_path = "D:\\RuneLite.exe".into();
        assert!(!path.matches(&window(42)));

        let mut process_only = rule();
        process_only.process_path.clear();
        process_only.title.clear();
        process_only.class_name.clear();
        assert!(process_only.matches(&window(42)));
    }

    #[test]
    fn disabled_rule_and_already_correct_window_are_no_ops() {
        let mut disabled = rule();
        disabled.enabled = false;
        assert_eq!(
            plan_foreground_rule(&[disabled], &window(42), &id(1), &snapshot(), None, 0).unwrap(),
            RulePlan::NoMatch
        );
        assert!(matches!(
            plan_foreground_rule(&[rule()], &window(42), &id(2), &snapshot(), None, 0).unwrap(),
            RulePlan::AlreadyCorrect { .. }
        ));
    }

    #[test]
    fn stale_guid_is_diagnostic_and_never_retargets_cached_name() {
        let mut stale = rule();
        stale.target.as_mut().unwrap().id = id(99);
        let error =
            plan_foreground_rule(&[stale], &window(42), &id(1), &snapshot(), None, 0).unwrap_err();
        assert!(error.contains("no longer exists"));
    }

    #[test]
    fn suppression_is_narrow_to_window_target_and_time() {
        let suppression = RuleSuppression::arm(42, id(2), 100);
        assert!(suppression.matches(42, &id(2), 600));
        assert!(!suppression.matches(43, &id(2), 101));
        assert!(!suppression.matches(42, &id(1), 101));
        assert!(!suppression.matches(42, &id(2), 601));
    }

    struct FakeBackend {
        window: Option<WindowDescriptor>,
        foreground: Option<usize>,
        foreground_after_initial_check: Option<usize>,
        desktop: VirtualDesktopId,
        effects: Vec<String>,
        now_ms: u64,
        activation_duration_ms: u64,
    }

    impl RuleBackend for FakeBackend {
        fn current_foreground(&mut self) -> Option<usize> {
            let foreground = self.foreground;
            if let Some(next) = self.foreground_after_initial_check.take() {
                self.foreground = Some(next);
            }
            foreground
        }
        fn describe_window(&mut self, _: usize) -> Option<WindowDescriptor> {
            self.window.clone()
        }
        fn snapshot(&mut self) -> Result<VirtualDesktopSnapshot, String> {
            Ok(snapshot())
        }
        fn desktop_for_window(&mut self, _: usize) -> Result<VirtualDesktopId, String> {
            Ok(self.desktop.clone())
        }
        fn move_window(&mut self, hwnd: usize, target: &VirtualDesktopId) -> Result<(), String> {
            self.effects.push(format!("move:{hwnd}:{target}"));
            self.desktop = target.clone();
            Ok(())
        }
        fn switch_desktop(&mut self, target: &VirtualDesktopId) -> Result<(), String> {
            self.effects.push(format!("switch:{target}"));
            Ok(())
        }
        fn activate(&mut self, hwnd: usize) -> Result<(), String> {
            self.effects.push(format!("activate:{hwnd}"));
            self.now_ms += self.activation_duration_ms;
            Ok(())
        }
        fn now_ms(&mut self) -> u64 {
            self.now_ms
        }
    }

    #[test]
    fn worker_plan_moves_only_foreground_then_switches_and_activates() {
        let mut backend = FakeBackend {
            window: Some(window(42)),
            foreground: Some(42),
            foreground_after_initial_check: None,
            desktop: id(1),
            effects: Vec::new(),
            now_ms: 100,
            activation_duration_ms: 700,
        };
        let mut suppression = None;
        assert!(matches!(
            process_foreground_event(&mut backend, &[rule()], 42, &mut suppression),
            RuleEventOutcome::Applied { .. }
        ));
        assert_eq!(
            backend.effects,
            [
                format!("move:42:{}", id(2)),
                format!("switch:{}", id(2)),
                "activate:42".to_string()
            ]
        );
        assert!(suppression.is_some());
        let effect_count = backend.effects.len();
        assert!(matches!(
            process_foreground_event(&mut backend, &[rule()], 42, &mut suppression),
            RuleEventOutcome::NoOp(RulePlan::Suppressed { .. })
        ));
        assert_eq!(backend.effects.len(), effect_count);
        assert!(suppression.as_ref().unwrap().matches(42, &id(2), 1_300));
        assert!(!suppression.as_ref().unwrap().matches(42, &id(2), 1_301));
    }

    #[test]
    fn stale_window_and_subsequent_unrelated_event_do_not_apply_or_get_suppressed() {
        let mut backend = FakeBackend {
            window: None,
            foreground: Some(42),
            foreground_after_initial_check: None,
            desktop: id(1),
            effects: Vec::new(),
            now_ms: 101,
            activation_duration_ms: 0,
        };
        let mut suppression = Some(RuleSuppression::arm(42, id(2), 100));
        assert_eq!(
            process_foreground_event(&mut backend, &[rule()], 42, &mut suppression),
            RuleEventOutcome::NoOp(RulePlan::NoMatch)
        );
        backend.window = Some(window(43));
        backend.foreground = Some(43);
        backend.now_ms = 102;
        assert!(matches!(
            process_foreground_event(&mut backend, &[rule()], 43, &mut suppression),
            RuleEventOutcome::Applied { .. }
        ));
        assert!(
            backend
                .effects
                .iter()
                .any(|effect| effect.starts_with("move:43:"))
        );
    }

    #[test]
    fn stale_foreground_event_is_revalidated_before_metadata_or_actions() {
        let mut backend = FakeBackend {
            window: Some(window(42)),
            foreground: Some(43),
            foreground_after_initial_check: None,
            desktop: id(1),
            effects: Vec::new(),
            now_ms: 0,
            activation_duration_ms: 0,
        };
        let mut suppression = None;
        assert_eq!(
            process_foreground_event(&mut backend, &[rule()], 42, &mut suppression),
            RuleEventOutcome::NoOp(RulePlan::NoMatch)
        );
        assert!(backend.effects.is_empty());
    }

    #[test]
    fn foreground_change_during_planning_is_revalidated_before_any_effect() {
        let mut backend = FakeBackend {
            window: Some(window(42)),
            foreground: Some(42),
            foreground_after_initial_check: Some(43),
            desktop: id(1),
            effects: Vec::new(),
            now_ms: 0,
            activation_duration_ms: 0,
        };
        let mut suppression = None;
        assert_eq!(
            process_foreground_event(&mut backend, &[rule()], 42, &mut suppression),
            RuleEventOutcome::NoOp(RulePlan::NoMatch)
        );
        assert!(backend.effects.is_empty());
        assert!(suppression.is_none());
    }

    #[test]
    fn foreground_bursts_coalesce_to_latest_even_when_wake_queue_is_full() {
        let (wake, receiver) = mpsc::sync_channel(1);
        let signal = LatestForegroundSignal {
            latest: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            ignored: Arc::new(std::array::from_fn(|_| IgnoredForeground::new())),
            started: Arc::new(std::time::Instant::now()),
            wake,
        };
        signal.publish(41);
        signal.publish(42);
        signal.publish(43);
        receiver.recv().unwrap();
        assert_eq!(signal.take_latest(), Some(43));
        assert!(matches!(
            receiver.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
    }

    #[test]
    fn programmatic_foreground_event_cannot_overwrite_newer_genuine_event() {
        let (wake, _receiver) = mpsc::sync_channel(1);
        let signal = LatestForegroundSignal {
            latest: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            ignored: Arc::new(std::array::from_fn(|_| IgnoredForeground::new())),
            started: Arc::new(std::time::Instant::now()),
            wake,
        };
        signal.ignore_at(41, 100, PROGRAMMATIC_EVENT_GRACE_MS);
        signal.publish_native_at(42, 101);
        signal.finish_ignoring_at(41, true, 102);
        // The worker may begin B before the delayed callback for A is delivered. Arming B must
        // not discard A's outstanding source marker.
        signal.ignore_at(42, 103, PROGRAMMATIC_EVENT_GRACE_MS);
        signal.publish_native_at(41, 104);
        signal.finish_ignoring_at(42, false, 105);
        assert_eq!(signal.take_latest(), Some(42));
    }

    #[test]
    fn failed_programmatic_action_does_not_hide_later_genuine_focus() {
        let (wake, _receiver) = mpsc::sync_channel(1);
        let signal = LatestForegroundSignal {
            latest: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            ignored: Arc::new(std::array::from_fn(|_| IgnoredForeground::new())),
            started: Arc::new(std::time::Instant::now()),
            wake,
        };
        signal.ignore_at(41, 100, PROGRAMMATIC_EVENT_GRACE_MS);
        signal.finish_ignoring_at(41, false, 101);
        signal.publish_native_at(41, 252);
        assert_eq!(signal.take_latest(), Some(41));
    }

    #[test]
    fn successful_action_without_callback_accepts_focus_after_deadline() {
        let (wake, _receiver) = mpsc::sync_channel(1);
        let signal = LatestForegroundSignal {
            latest: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            ignored: Arc::new(std::array::from_fn(|_| IgnoredForeground::new())),
            started: Arc::new(std::time::Instant::now()),
            wake,
        };
        signal.ignore_at(41, 100, PROGRAMMATIC_EVENT_GRACE_MS);
        signal.finish_ignoring_at(41, true, 101);
        signal.publish_native_at(41, 1_602);
        assert_eq!(signal.take_latest(), Some(41));
    }

    #[test]
    fn completed_callback_is_not_rearmed_at_operation_end() {
        let (wake, _receiver) = mpsc::sync_channel(1);
        let signal = LatestForegroundSignal {
            latest: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            ignored: Arc::new(std::array::from_fn(|_| IgnoredForeground::new())),
            started: Arc::new(std::time::Instant::now()),
            wake,
        };
        signal.ignore_at(41, 100, PROGRAMMATIC_EVENT_GRACE_MS);
        signal.publish_native_at(41, 101);
        signal.finish_ignoring_at(41, true, 102);
        signal.publish_native_at(41, 103);
        assert_eq!(signal.take_latest(), Some(41));
    }

    #[derive(Default)]
    struct LifecycleCounts {
        starts: AtomicUsize,
        shutdowns: AtomicUsize,
    }

    struct FakeFactory(Arc<LifecycleCounts>);
    struct FakeRuntime(Arc<LifecycleCounts>);

    impl RuleRuntimeFactory for FakeFactory {
        fn start(&self, _: Arc<Vec<VirtualDesktopRule>>) -> Option<Box<dyn RuleRuntimeHandle>> {
            self.0.starts.fetch_add(1, Ordering::SeqCst);
            Some(Box::new(FakeRuntime(Arc::clone(&self.0))))
        }
    }

    impl RuleRuntimeHandle for FakeRuntime {
        fn shutdown(self: Box<Self>) {
            self.0.shutdowns.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn empty_config_has_no_runtime_and_enable_reload_disable_drop_join_exactly_once() {
        let counts = Arc::new(LifecycleCounts::default());
        {
            let mut controller =
                RuleRuntimeController::new(Arc::new(FakeFactory(Arc::clone(&counts))));
            controller.reconcile(&[]);
            assert_eq!(counts.starts.load(Ordering::SeqCst), 0);
            controller.reconcile(&[rule()]);
            assert_eq!(counts.starts.load(Ordering::SeqCst), 1);
            controller.reconcile(&[rule()]);
            assert_eq!(counts.starts.load(Ordering::SeqCst), 1);
            let mut changed = rule();
            changed.title.clear();
            controller.reconcile(&[changed]);
            assert_eq!(counts.starts.load(Ordering::SeqCst), 2);
            assert_eq!(counts.shutdowns.load(Ordering::SeqCst), 1);
            controller.reconcile(&[]);
            assert_eq!(counts.shutdowns.load(Ordering::SeqCst), 2);
            controller.reconcile(&[rule()]);
            assert_eq!(counts.starts.load(Ordering::SeqCst), 3);
        }
        assert_eq!(counts.shutdowns.load(Ordering::SeqCst), 3);
    }
}
