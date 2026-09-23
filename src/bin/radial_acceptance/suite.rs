use super::super::{
    AcceptanceCaseResult, AcceptanceReport, CaseStatus, FailureStage, H6RepeatMode, MAX_PATH_BYTES,
    MAX_RESULT_BYTES,
};
use super::*;
use serde::Serialize;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, UNIX_EPOCH};

const TAP_TIME: Duration = Duration::from_millis(135);
const ROOT_TIMEOUT: Duration = Duration::from_secs(3);
const UIA_TIMEOUT: Duration = Duration::from_secs(5);
const TRACE_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_TRACE_EXCERPT: usize = 512;
const DESIGNER_TEXT_PROBE: &str = "Native Edit Probe";
const DESIGNER_STARTER_NAME: &str = "Starter";
static NEXT_HOOK_PUMP_PROBE_ID: AtomicU64 = AtomicU64::new(1);
const CASE_IDS: [&str; 13] = [
    "H0", "H1", "H2", "H3", "H4", "H5", "H6", "D0", "D1", "D2", "D4", "D5", "CLEANUP",
];

struct HoldReleaseHandoff {
    release_at_unix_ms: Option<u128>,
    sentinel_at_unix_ms: Option<u128>,
    quiescent_acknowledged: bool,
    observer: Option<RunnerHookObserver>,
}

#[derive(Clone, Copy, Debug)]
enum DesignerSemanticTarget {
    Menus,
    Skins,
    Tree,
    Inspector,
    DefaultMenu,
    MenuName,
    MenuDefaultSkin,
}

struct F11HoldGuard<'a> {
    child: &'a NativeChild,
    anchor: &'a FocusAnchor,
    armed: bool,
}

impl<'a> F11HoldGuard<'a> {
    fn new(child: &'a NativeChild, anchor: &'a FocusAnchor) -> Self {
        Self {
            child,
            anchor,
            armed: true,
        }
    }

    fn release(&mut self) -> Result<NativeInputEdgeEvidence, String> {
        if !self.armed {
            return Err("F11 hold was already released".into());
        }

        let (foreground, process_id) = capture_foreground();
        let target =
            if process_id == self.child.process_id() || process_id == self.anchor.process_id() {
                foreground
            } else {
                self.anchor.focus()?;
                self.anchor.hwnd()
            };
        let evidence = self.child.release_f11(target)?;
        self.armed = false;
        Ok(evidence)
    }
}

impl Drop for F11HoldGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            let _ = self.release();
        }
    }
}

pub fn run_suite(
    executable: &str,
    profile: &Path,
    output: &Path,
    trace_path: &Path,
    hold_threshold_ms: u64,
    h6_repeat_mode: H6RepeatMode,
    _desktop: &InputDesktopAttachment,
    report: &mut AcceptanceReport,
    runner_log: &mut File,
) {
    if let Err(error) = preflight_acceptance_hotkey() {
        record_environment_failure(
            format!("acceptance hotkey preflight failed: {error}"),
            report,
            output,
            trace_path,
            runner_log,
        );
        return;
    }
    let _ = writeln!(
        runner_log,
        "acceptance hotkey F11 registered and unregistered successfully before child launch"
    );

    let stdout_path = output.join("child.stdout.log");
    let stderr_path = output.join("child.stderr.log");
    let mut child = match NativeChild::launch(
        Path::new(executable),
        profile,
        trace_path,
        &stdout_path,
        &stderr_path,
    ) {
        Ok(child) => child,
        Err(error) => {
            report.environment.child_process_id = error.process_id;
            report.environment.child_started_unix_ms = error.started.and_then(|started| {
                started
                    .duration_since(UNIX_EPOCH)
                    .ok()
                    .map(|duration| duration.as_millis())
            });
            if let Some(inventory) = save_launch_failure_inventory(&error, output) {
                report.push_artifact(inventory.to_string_lossy());
            }
            let failure = CaseFailure::new(FailureStage::CandidateStartup, error.to_string());
            for id in CASE_IDS {
                append_case(
                    report,
                    id,
                    expected(id),
                    started_now(),
                    Err(failure.clone()),
                    None,
                    output,
                    trace_path,
                );
            }
            let _ = writeln!(
                runner_log,
                "candidate startup failed pid={:?} windows={} : {failure}",
                error.process_id,
                error.windows.len()
            );
            return;
        }
    };

    report.environment.child_process_id = Some(child.process_id());
    report.environment.child_started_unix_ms = child
        .started()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis());
    let _ = writeln!(
        runner_log,
        "launched source-matched candidate pid={} root_hwnd={} desktop={} started={:?}",
        child.process_id(),
        hwnd_id(child.root().hwnd),
        child.desktop_name(),
        child.started()
    );

    let anchor = match FocusAnchor::create() {
        Ok(anchor) => anchor,
        Err(error) => {
            let failure = CaseFailure::new(FailureStage::Environment, error);
            for id in CASE_IDS {
                append_case(
                    report,
                    id,
                    expected(id),
                    started_now(),
                    Err(failure.clone()),
                    Some(&child),
                    output,
                    trace_path,
                );
            }
            stop_child(&mut child, report, runner_log, output, trace_path);
            return;
        }
    };
    let mut uia: Option<UiAutomation> = None;
    let mut designer_window: Option<WindowSnapshot> = None;
    let mut hold_windows: Option<Vec<WindowSnapshot>> = None;

    run_tap_case(
        report,
        "H0",
        &mut child,
        &anchor,
        trace_path,
        output,
        "focused ROOT tap parks ROOT offscreen with one short-tap path",
        true,
        false,
        1,
    );
    run_tap_case(
        report,
        "H1",
        &mut child,
        &anchor,
        trace_path,
        output,
        "runner-owned focus tap shows and focuses ROOT",
        false,
        true,
        1,
    );
    run_other_focus_case(report, &mut child, &anchor, trace_path, output);
    let (hold_open, hold_guard, hold_observer, hold_observer_error) = run_hold_open_case(
        &child,
        &anchor,
        trace_path,
        hold_threshold_ms,
        &mut hold_windows,
    );
    append_case(
        report,
        "H4",
        expected("H4"),
        started_now(),
        hold_open,
        Some(&child),
        output,
        trace_path,
    );
    let h5_handoff = run_hold_release_case(
        report,
        &child,
        &anchor,
        trace_path,
        output,
        hold_windows.as_deref(),
        hold_guard,
        hold_observer,
        hold_observer_error,
        h6_repeat_mode,
    );
    run_second_hold_case(
        report,
        &child,
        &anchor,
        trace_path,
        output,
        hold_threshold_ms,
        hold_windows.as_deref(),
        h6_repeat_mode,
        h5_handoff,
    );

    let ui_result = UiAutomation::new();
    match ui_result {
        Ok(automation) => uia = Some(automation),
        Err(error) => {
            let started = Instant::now();
            append_case(
                report,
                "D0",
                expected("D0"),
                started,
                Err(CaseFailure::new(FailureStage::Environment, error.clone())),
                Some(&child),
                output,
                trace_path,
            );
        }
    }

    if let Some(automation) = uia.as_ref() {
        match run_designer_entry(&mut child, automation, trace_path) {
            Ok(window) => {
                designer_window = Some(window);
                append_case(
                    report,
                    "D0",
                    expected("D0"),
                    started_now(),
                    Ok("one child-owned Designer HWND appeared; UIA root belongs to the child and its InitialSnapshot reply was accepted".into()),
                    None,
                    output,
                    trace_path,
                );
            }
            Err(failure) => {
                append_case(
                    report,
                    "D0",
                    expected("D0"),
                    started_now(),
                    Err(failure.clone()),
                    Some(&child),
                    output,
                    trace_path,
                );
                append_blocked_designer_cases(report, &failure, Some(&child), output, trace_path);
            }
        }
    } else {
        let failure = CaseFailure::new(
            FailureStage::Environment,
            "UI Automation initialization failed".into(),
        );
        append_blocked_designer_cases(report, &failure, Some(&child), output, trace_path);
    }

    if let (Some(automation), Some(designer)) = (uia.as_ref(), designer_window.as_ref()) {
        run_designer_focus_case(report, &mut child, automation, designer, trace_path, output);
        run_designer_pointer_case(report, &mut child, automation, designer, trace_path, output);
        run_tab_case(report, &mut child, automation, designer, output, trace_path);
        run_skins_command_case(report, &mut child, automation, designer, trace_path, output);
        run_designer_close_case(report, &mut child, designer, output, trace_path);
    }

    stop_child(&mut child, report, runner_log, output, trace_path);
    drop(anchor);
    if report.cases.len() < CASE_IDS.len() {
        for id in CASE_IDS.iter().skip(report.cases.len()) {
            append_case(
                report,
                id,
                expected(id),
                started_now(),
                Err(CaseFailure::new(
                    FailureStage::Cleanup,
                    "runner omitted a required case result".into(),
                )),
                None,
                output,
                trace_path,
            );
        }
    }
    let _ = writeln!(
        runner_log,
        "native suite completed with {} case records",
        report.cases.len()
    );
}

pub fn record_environment_failure(
    message: String,
    report: &mut AcceptanceReport,
    output: &Path,
    trace_path: &Path,
    runner_log: &mut File,
) {
    let failure = CaseFailure::new(FailureStage::Environment, message);
    for id in CASE_IDS {
        append_case(
            report,
            id,
            expected(id),
            started_now(),
            Err(failure.clone()),
            None,
            output,
            trace_path,
        );
    }
    let _ = writeln!(runner_log, "native environment setup failed: {failure}");
}

fn run_tap_case(
    report: &mut AcceptanceReport,
    id: &str,
    child: &mut NativeChild,
    anchor: &FocusAnchor,
    trace_path: &Path,
    output: &Path,
    expected: &str,
    start_visible: bool,
    end_visible: bool,
    taps: usize,
) {
    let started = Instant::now();
    let result = (|| {
        // NativeChild becomes discoverable as soon as USER creates the HWND. Wait for the
        // actual initial visible state before testing a focused ROOT tap; input must never
        // be used to repair an unobserved startup state.
        if id == "H0" && start_visible && !wait_root_visibility(child, true, ROOT_TIMEOUT) {
            let root = child.refresh_root().ok();
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                format!(
                    "ROOT did not become initially visible and drawable before H0; no input was sent: {:?}",
                    root.map(|window| (window.visible, window.minimized, window.bounds))
                ),
            ));
        }
        let mut root = child
            .refresh_root()
            .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
        if start_visible {
            require_visible(&root)
                .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
            child
                .focus_window(&root)
                .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        } else {
            require_hidden(&root)
                .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
            anchor
                .focus()
                .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        }
        let mut runner_observer = None;
        let mut hook_diagnostic = String::new();
        if id == "H0" {
            let startup_events = trace_lines(trace_path);
            let hook_ready = startup_events
                .iter()
                .find(|line| line.contains("trace_event=\"hook_service_ready\""))
                .cloned()
                .unwrap_or_else(|| "hook_service_ready was not recorded before H0".into());
            let sentinel_cursor = startup_events.len();
            let (observer, observer_error) = match RunnerHookObserver::start() {
                Ok(observer) => (Some(observer), None),
                Err(error) => (None, Some(error)),
            };
            runner_observer = observer;
            let sentinel_input = child
                .press_hook_sentinel(root.hwnd, child.process_id())
                .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
            let sentinel_events =
                wait_trace(trace_path, sentinel_cursor, TRACE_TIMEOUT, |events| {
                    has_trace(events, "hook_observed", &["vk=135", "down=true"])
                        && has_trace(events, "hook_observed", &["vk=135", "down=false"])
                });
            let sentinel_events = if has_trace(&sentinel_events, "frontend_key", &["key=F24"]) {
                sentinel_events
            } else {
                wait_trace(
                    trace_path,
                    sentinel_cursor,
                    Duration::from_millis(250),
                    |events| has_trace(events, "frontend_key", &["key=F24"]),
                )
            };
            let down_seen = has_trace(
                &sentinel_events,
                "hook_observed",
                &["vk=135", "down=true", "injected=true"],
            );
            let up_seen = has_trace(
                &sentinel_events,
                "hook_observed",
                &["vk=135", "down=false", "injected=true"],
            );
            let runner_observation = match runner_observer.as_mut() {
                Some(observer) => {
                    let observation = observer.wait_for_vk(0x87, Duration::from_secs(1));
                    observation.describe()
                }
                None => format!(
                    "runner observer unavailable: {}",
                    observer_error.unwrap_or_else(|| "not started".into())
                ),
            };
            hook_diagnostic = format!(
                "pre-H0 hook={hook_ready}; F24 sentinel production callback down/up={down_seen}/{up_seen} injected, frontend WM_KEYDOWN={}; {runner_observation}, checked input=[{}]",
                has_trace(&sentinel_events, "frontend_key", &["key=F24"]),
                sentinel_input.describe()
            );
        }
        let mut cursor = trace_lines(trace_path).len();
        let mut input_evidence = Vec::with_capacity(taps);
        for index in 0..taps {
            if index > 0 {
                anchor
                    .focus()
                    .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
            }
            let (target_hwnd, target_pid) = if index == 0 && start_visible {
                (root.hwnd, child.process_id())
            } else {
                (anchor.hwnd(), anchor.process_id())
            };
            let input = child
                .send_f11(target_hwnd, target_pid, TAP_TIME)
                .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
            input_evidence.push(input.describe());
            let wanted_visible = if taps > 1 {
                index % 2 == 1
            } else {
                end_visible
            };
            let events = wait_trace(trace_path, cursor, TRACE_TIMEOUT, |events| {
                tap_trace_complete(events, 1, wanted_visible)
            });
            if index == 0 && id == "H0" {
                let runner_observation = runner_observer.as_mut().map(|observer| {
                    let observation = observer.wait_for_vk(0x7A, Duration::from_secs(1));
                    observation.describe()
                });
                let production_down = has_trace(
                    &events,
                    "hook_observed",
                    &["vk=122", "down=true", "injected=true"],
                );
                let production_up = has_trace(
                    &events,
                    "hook_observed",
                    &["vk=122", "down=false", "injected=true"],
                );
                hook_diagnostic.push_str(&format!(
                    "; production F11 callback down/up={production_down}/{production_up} injected; {}",
                    runner_observation.unwrap_or_else(|| "runner F11 observer unavailable".into())
                ));
            }
            if !tap_trace_complete(&events, 1, wanted_visible) {
                let observed_root = child.refresh_root().ok();
                let stage = tap_trace_failure_stage(&events, wanted_visible);
                return Err(CaseFailure::new(
                    stage,
                    format!(
                        "F11 tap {} input edges [{}] lacked complete hook/gesture/visibility evidence for visible={wanted_visible}; root={:?}; observed production edges: {}; {}",
                        index + 1,
                        input.describe(),
                        observed_root.map(|window| (
                            window.visible,
                            window.minimized,
                            window.bounds
                        )),
                        input_trace_summary(&events),
                        hook_diagnostic
                    ),
                ));
            }
            if !wait_root_visibility(child, wanted_visible, ROOT_TIMEOUT) {
                let observed_root = child.refresh_root().ok();
                return Err(CaseFailure::new(
                    FailureStage::NativeRootState,
                    format!(
                        "ROOT did not reach visible={wanted_visible} after tap {}; checked F11 edges [{}]; root={:?}; production edges: {}",
                        index + 1,
                        input.describe(),
                        observed_root.map(|window| (
                            window.visible,
                            window.minimized,
                            window.bounds
                        )),
                        input_trace_summary(&events)
                    ),
                ));
            }
            cursor = trace_lines(trace_path).len();
            root = child
                .refresh_root()
                .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
        }
        if end_visible {
            require_visible(&root)
                .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
        } else {
            require_hidden(&root)
                .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
        }
        Ok(format!(
            "{} checked F11 input pairs reached expected ROOT state; HWND={} bounds={:?}; edges={:?}; {}",
            taps,
            hwnd_id(root.hwnd),
            root.bounds,
            input_evidence,
            hook_diagnostic
        ))
    })();
    append_case(
        report,
        id,
        expected,
        started,
        result,
        Some(child),
        output,
        trace_path,
    );
}

fn run_other_focus_case(
    report: &mut AcceptanceReport,
    child: &mut NativeChild,
    anchor: &FocusAnchor,
    trace_path: &Path,
    output: &Path,
) {
    let started = Instant::now();
    let result = (|| {
        let root = child
            .refresh_root()
            .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
        require_visible(&root)
            .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
        let restore_request = wait_for_latest_root_restore(trace_path, ROOT_TIMEOUT).ok_or_else(|| {
            CaseFailure::new(
                FailureStage::RootCommand,
                "ROOT did not produce a matching native restore-completion edge after H1; H2 anchor input was not sent".into(),
            )
        })?;
        anchor
            .focus()
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        let mut cursor = trace_lines(trace_path).len();
        let mut inputs = Vec::with_capacity(2);
        for visible in [false, true] {
            anchor
                .focus()
                .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
            focus_is_validated(anchor.hwnd(), anchor.process_id()).map_err(|error| {
                CaseFailure::new(
                    FailureStage::InputInjection,
                    format!("runner-owned H2 anchor was not foreground immediately before input: {error}"),
                )
            })?;
            let input = child
                .send_f11(anchor.hwnd(), anchor.process_id(), TAP_TIME)
                .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
            inputs.push(input.describe());
            if !wait_root_visibility(child, visible, ROOT_TIMEOUT) {
                return Err(CaseFailure::new(
                    FailureStage::NativeRootState,
                    format!("runner-owned focus F11 failed to toggle ROOT to visible={visible}"),
                ));
            }
            let trace = wait_trace(trace_path, cursor, TRACE_TIMEOUT, |events| {
                tap_trace_complete(events, 1, visible)
            });
            if !tap_trace_complete(&trace, 1, visible) {
                return Err(CaseFailure::new(FailureStage::GestureDecision, "missing hook admission or one short-tap visibility decision from runner-owned focus".into()));
            }
            cursor = trace_lines(trace_path).len();
        }
        Ok(format!(
            "H1 ROOT restore request {restore_request} completed before runner-owned anchor HWND={} focus; both checked F11 taps targeted the exact runner HWND/PID and toggled ROOT offscreen/back on-screen: {inputs:?}",
            hwnd_id(anchor.hwnd()),
        ))
    })();
    append_case(
        report,
        "H2",
        expected("H2"),
        started,
        result,
        Some(child),
        output,
        trace_path,
    );
}

fn run_hold_open_case<'a>(
    child: &'a NativeChild,
    anchor: &'a FocusAnchor,
    trace_path: &Path,
    hold_threshold_ms: u64,
    held_windows: &mut Option<Vec<WindowSnapshot>>,
) -> (
    Result<String, CaseFailure>,
    Option<F11HoldGuard<'a>>,
    Option<RunnerHookObserver>,
    Option<String>,
) {
    let mut hold_guard = None;
    let mut hook_observer = None;
    let mut hook_observer_error = None;
    let result = (|| {
        let root = child
            .refresh_root()
            .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
        require_visible(&root)
            .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
        child
            .focus_window(&root)
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        let before = runtime_windows(child);
        let cursor = trace_lines(trace_path).len();
        match RunnerHookObserver::start() {
            Ok(observer) => hook_observer = Some(observer),
            Err(error) => hook_observer_error = Some(error),
        }
        let input = child
            .press_f11(root.hwnd, child.process_id())
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        hold_guard = Some(F11HoldGuard::new(child, anchor));
        std::thread::sleep(Duration::from_millis(
            hold_threshold_ms.saturating_add(250).min(5_000),
        ));
        let after = wait_runtime_windows(child, &before, Duration::from_secs(2))
            .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
        let traces = trace_lines(trace_path)
            .into_iter()
            .skip(cursor)
            .collect::<Vec<_>>();
        let (foreground_hwnd, foreground_pid) = capture_foreground();
        if !has_trace(
            &traces,
            "hook_primary",
            &["transition=Press", "provenance=ExternalInjected"],
        ) || !has_trace(
            &traces,
            "configured_primary",
            &[
                "transition=Press",
                "provenance=ExternalInjected",
                "modifiers_match=true",
            ],
        ) {
            let runner_observation = hook_observer
                .as_mut()
                .map(|observer| observer.wait_for_vk(0x7A, Duration::from_millis(50)))
                .map(|observation| observation.describe())
                .or_else(|| {
                    hook_observer_error
                        .as_ref()
                        .map(|error| format!("runner hook observer unavailable: {error}"))
                })
                .unwrap_or_else(|| "runner hook observer unavailable".into());
            return Err(CaseFailure::new(
                FailureStage::HookAdmission,
                format!(
                    "held F11 down edge [{}] targeted ROOT HWND={} PID={}; foreground after hold HWND={} PID={}; observed {} production trace event(s), but no matching hook/configured press edge; {runner_observation}",
                    input.describe(),
                    hwnd_id(root.hwnd),
                    child.process_id(),
                    hwnd_id(foreground_hwnd),
                    foreground_pid,
                    traces.len()
                ),
            ));
        }
        if has_trace(&traces, "short_tap", &[]) || has_trace(&traces, "desired_visibility", &[]) {
            return Err(CaseFailure::new(
                FailureStage::GestureDecision,
                "hold incorrectly emitted short-tap or ROOT visibility work".into(),
            ));
        }
        if after.len() < 2 {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                format!(
                    "hold produced {} stable new visible child-owned radial HWND(s); expected the input and visual surfaces",
                    after.len()
                ),
            ));
        }
        validate_radial_surfaces(child, &after)
            .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
        let current_root = child
            .refresh_root()
            .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
        require_visible(&current_root).map_err(|error| {
            CaseFailure::new(
                FailureStage::NativeRootState,
                format!("ROOT changed while opening radial: {error}"),
            )
        })?;
        if !same_window_state(&root, &current_root) {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                format!(
                    "opening the radial changed ROOT HWND/state/bounds: before={}; after={}",
                    describe_root_snapshot(&root),
                    describe_root_snapshot(&current_root)
                ),
            ));
        }
        *held_windows = Some(after.clone());
        Ok(format!(
            "held F11 opened stable visible child radial surfaces [{}]; ROOT stayed on-screen; down edge=[{}]",
            describe_radial_surfaces(&after),
            input.describe()
        ))
    })();
    if result.is_err() {
        drop(hold_guard.take());
        (result, None, hook_observer, hook_observer_error)
    } else {
        (result, hold_guard, hook_observer, hook_observer_error)
    }
}

fn run_hold_release_case(
    report: &mut AcceptanceReport,
    child: &NativeChild,
    _anchor: &FocusAnchor,
    trace_path: &Path,
    output: &Path,
    held_windows: Option<&[WindowSnapshot]>,
    hold_guard: Option<F11HoldGuard<'_>>,
    mut hook_observer: Option<RunnerHookObserver>,
    hook_observer_error: Option<String>,
    h6_repeat_mode: H6RepeatMode,
) -> HoldReleaseHandoff {
    let started = Instant::now();
    let mut release_at_unix_ms = None;
    let mut sentinel_at_unix_ms = None;
    let mut quiescent_acknowledged = false;
    let result = (|| {
        let mut hold_guard = hold_guard.ok_or_else(|| {
            CaseFailure::new(
                FailureStage::InputInjection,
                "H4 did not leave F11 held for a checked release".into(),
            )
        })?;
        let runner_edges_drained = hook_observer
            .as_mut()
            .map_or(0, RunnerHookObserver::drain_pending);
        let cursor = trace_lines(trace_path).len();
        let release = hold_guard
            .release()
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        release_at_unix_ms = Some(release.at_unix_ms);
        let runner_observation = hook_observer
            .as_mut()
            .map(|observer| observer.wait_for_vk_edge(0x7A, false, Duration::from_secs(1)));
        let runner_observation_text = runner_observation
            .as_ref()
            .map(|observation| observation.describe())
            .or_else(|| {
                hook_observer_error
                    .as_ref()
                    .map(|error| format!("observer unavailable: {error}"))
            })
            .unwrap_or_else(|| "observer unavailable".into());
        let lines = wait_trace(trace_path, cursor, TRACE_TIMEOUT, |events| {
            has_trace(
                events,
                "hook_primary",
                &["transition=Release", "provenance=ExternalInjected"],
            ) && has_trace(
                events,
                "configured_primary",
                &[
                    "transition=Release",
                    "provenance=ExternalInjected",
                    "modifiers_match=true",
                ],
            )
        });
        if !has_trace(
            &lines,
            "hook_primary",
            &["transition=Release", "provenance=ExternalInjected"],
        ) || !has_trace(
            &lines,
            "configured_primary",
            &[
                "transition=Release",
                "provenance=ExternalInjected",
                "modifiers_match=true",
            ],
        ) {
            return Err(CaseFailure::new(
                FailureStage::HookAdmission,
                format!(
                    "held F11 release edge [{}] did not reach the production hook and configured chord; {runner_observation_text}",
                    release.describe()
                ),
            ));
        }
        let runner_release_observed = runner_observation
            .as_ref()
            .is_some_and(|observation| observation.up_seen && observation.up_injected);
        if !runner_release_observed {
            return Err(CaseFailure::new(
                FailureStage::HookAdmission,
                format!(
                    "held F11 release was not acknowledged by the independent observer; release edge [{}]; {runner_observation_text}",
                    release.describe()
                ),
            ));
        }
        if has_trace(&lines, "short_tap", &[]) || has_trace(&lines, "desired_visibility", &[]) {
            return Err(CaseFailure::new(
                FailureStage::GestureDecision,
                format!(
                    "release after threshold co-fired the short-tap path; release edge [{}]",
                    release.describe()
                ),
            ));
        }
        let held_windows = held_windows.ok_or_else(|| {
            CaseFailure::new(
                FailureStage::NativeRootState,
                "H4 did not identify the radial surface set to check after release".into(),
            )
        })?;
        if !radial_surfaces_are_active(child, held_windows) {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                format!(
                    "one or more radial surfaces did not remain active after releasing held F11; surfaces=[{}]; release edge [{}]",
                    describe_radial_surfaces(held_windows),
                    release.describe()
                ),
            ));
        }
        let mut handoff_text = String::new();
        if h6_repeat_mode != H6RepeatMode::Immediate {
            let root = child
                .refresh_root()
                .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
            require_visible(&root).map_err(|error| {
                CaseFailure::new(
                    FailureStage::NativeRootState,
                    format!("ROOT changed visibility before the repeat handoff: {error}"),
                )
            })?;
            focus_is_validated(root.hwnd, child.process_id()).map_err(|error| {
                CaseFailure::new(
                    FailureStage::InputInjection,
                    format!(
                        "H5 release did not leave ROOT foreground for the repeat handoff: {error}"
                    ),
                )
            })?;

            let sentinel_cursor = trace_lines(trace_path).len();
            let sentinel = child
                .press_hook_sentinel(root.hwnd, child.process_id())
                .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
            sentinel_at_unix_ms = Some(sentinel.at_unix_ms);
            let runner_sentinel = hook_observer
                .as_mut()
                .map(|observer| observer.wait_for_vk(0x87, Duration::from_secs(1)));
            let sentinel_trace = wait_trace(
                trace_path,
                sentinel_cursor,
                Duration::from_secs(1),
                |events| {
                    has_trace(
                        events,
                        "hook_observed",
                        &["vk=135", "down=true", "injected=true"],
                    ) && has_trace(
                        events,
                        "hook_observed",
                        &["vk=135", "down=false", "injected=true"],
                    )
                },
            );
            let production_sentinel = has_trace(
                &sentinel_trace,
                "hook_observed",
                &["vk=135", "down=true", "injected=true"],
            ) && has_trace(
                &sentinel_trace,
                "hook_observed",
                &["vk=135", "down=false", "injected=true"],
            );
            let observer_sentinel = runner_sentinel.as_ref().is_some_and(|observation| {
                observation.down_seen
                    && observation.up_seen
                    && observation.down_injected
                    && observation.up_injected
            });
            let sentinel_text = runner_sentinel
                .as_ref()
                .map(RunnerHookObservation::describe)
                .or_else(|| {
                    hook_observer_error
                        .as_ref()
                        .map(|error| format!("observer unavailable: {error}"))
                })
                .unwrap_or_else(|| "observer unavailable".into());
            if !production_sentinel || !observer_sentinel {
                return Err(CaseFailure::new(
                    FailureStage::HookAdmission,
                    format!(
                        "quiescent H5→H6 handoff sentinel did not reach both hooks; production_down_up={production_sentinel}; {sentinel_text}; checked F24=[{}]",
                        sentinel.describe()
                    ),
                ));
            }
            quiescent_acknowledged = true;
            handoff_text = format!(
                "; quiescent F24 handoff reached both hooks before H6: production down/up=true/true, {sentinel_text}, checked input=[{}]",
                sentinel.describe()
            );
        }
        Ok(format!(
            "checked F11 release generated no tap or ROOT visibility edge; radial surfaces [{}] remain visible; release edge=[{}]; discarded {runner_edges_drained} pre-release observer edge(s) before correlating this F11 up; {runner_observation_text}{handoff_text}",
            describe_radial_surfaces(held_windows),
            release.describe()
        ))
    })();
    append_case(
        report,
        "H5",
        expected("H5"),
        started,
        result,
        Some(child),
        output,
        trace_path,
    );
    HoldReleaseHandoff {
        release_at_unix_ms,
        sentinel_at_unix_ms,
        quiescent_acknowledged,
        observer: if h6_repeat_mode != H6RepeatMode::Immediate {
            hook_observer
        } else {
            None
        },
    }
}

fn run_second_hold_case(
    report: &mut AcceptanceReport,
    child: &NativeChild,
    anchor: &FocusAnchor,
    trace_path: &Path,
    output: &Path,
    hold_threshold_ms: u64,
    held_windows: Option<&[WindowSnapshot]>,
    h6_repeat_mode: H6RepeatMode,
    mut h5_handoff: HoldReleaseHandoff,
) {
    let started = Instant::now();
    let result = (|| {
        let held_windows = held_windows.ok_or_else(|| {
            CaseFailure::new(
                FailureStage::NativeRootState,
                "H4 did not identify the radial surface set to toggle closed".into(),
            )
        })?;
        validate_radial_surfaces(child, held_windows)
            .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
        let root_before = child
            .refresh_root()
            .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
        require_visible(&root_before)
            .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
        let root = &root_before;
        if h6_repeat_mode != H6RepeatMode::Immediate {
            if !h5_handoff.quiescent_acknowledged {
                return Err(CaseFailure::new(
                    FailureStage::HookAdmission,
                    "H5 did not complete the production/independent F24 quiescence handoff; H6 input was not sent".into(),
                ));
            }
            focus_is_validated(root.hwnd, child.process_id()).map_err(|error| {
                CaseFailure::new(
                    FailureStage::InputInjection,
                    format!("foreground changed after the acknowledged H5 handoff: {error}"),
                )
            })?;
        } else {
            child
                .focus_window(&root)
                .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        }
        let cursor = trace_lines(trace_path).len();
        let mut observer_drop_probe = None;
        let (mut hook_observer, hook_observer_error) = match h6_repeat_mode {
            H6RepeatMode::Immediate => match RunnerHookObserver::start() {
                Ok(observer) => (Some(observer), None),
                Err(error) => (None, Some(error)),
            },
            H6RepeatMode::Quiescent => {
                let observer = h5_handoff.observer.take();
                let error = observer
                    .is_none()
                    .then(|| "H5 observer was not retained through the quiescent handoff".into());
                (observer, error)
            }
            H6RepeatMode::ProductionOnlyDiagnostic => {
                let mut previous_observer = h5_handoff.observer.take();
                let previous_thread = previous_observer
                    .as_ref()
                    .map(RunnerHookObserver::thread_id);
                let previous_hook = previous_observer.as_ref().map(RunnerHookObserver::hook_id);
                let unhook_result = match previous_observer.as_mut() {
                    Some(observer) => observer
                        .stop_and_report()
                        .map(|()| {
                            format!(
                                "UnhookWindowsHookEx succeeded for runner PID {} owned HHOOK=0x{:x}",
                                std::process::id(),
                                observer.hook_id()
                            )
                        })
                        .unwrap_or_else(|error| format!("observer stop failed: {error}")),
                    None => "H5 observer was unavailable to stop".into(),
                };
                drop(previous_observer);
                let post_unhook_probe =
                    checked_production_f24_without_runner_observer(child, root.hwnd, trace_path);
                observer_drop_probe = Some(format!(
                    "stopped runner observer thread {previous_thread:?} HHOOK={previous_hook:?}: {unhook_result}; immediate checked F24 probe: {post_unhook_probe}"
                ));
                (
                    None,
                    Some(
                        "runner observer intentionally absent for production-only H6 diagnostic"
                            .into(),
                    ),
                )
            }
        };
        let app_hook_thread = hook_service_thread_id(trace_path);
        let runner_thread_before = hook_observer
            .as_ref()
            .map(|observer| format!("runner observer {}", thread_liveness(observer.thread_id())));
        let app_thread_before = app_hook_thread
            .map(|thread_id| format!("app hook {}", thread_liveness(thread_id)))
            .unwrap_or_else(|| "app hook thread id unavailable".into());
        let down = child
            .press_f11(root.hwnd, child.process_id())
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        let handoff_description = match h6_repeat_mode {
            H6RepeatMode::Immediate => format!(
                "mode=immediate; H5-release-to-H6-down={}ms",
                h5_handoff
                    .release_at_unix_ms
                    .map_or(0, |released| down.at_unix_ms.saturating_sub(released))
            ),
            H6RepeatMode::Quiescent => format!(
                "mode=quiescent; H5-release-to-F24={}ms; F24-to-H6-down={}ms; both hooks acknowledged H5/F24 before H6",
                h5_handoff
                    .release_at_unix_ms
                    .zip(h5_handoff.sentinel_at_unix_ms)
                    .map_or(0, |(released, sentinel)| sentinel.saturating_sub(released)),
                h5_handoff
                    .sentinel_at_unix_ms
                    .map_or(0, |sentinel| down.at_unix_ms.saturating_sub(sentinel))
            ),
            H6RepeatMode::ProductionOnlyDiagnostic => {
                format!(
                    "mode=production_only_diagnostic; H5 quiescence acknowledged; {}; no runner LL hook installed during H6 down, hold, release, or initial post-release F24",
                    observer_drop_probe
                        .as_deref()
                        .unwrap_or("observer-drop probe unavailable")
                )
            }
        };
        let mut hold_guard = F11HoldGuard::new(child, anchor);
        std::thread::sleep(Duration::from_millis(
            hold_threshold_ms.saturating_add(250).min(5_000),
        ));
        let deadline_events =
            wait_trace(trace_path, cursor, Duration::from_millis(250), |events| {
                has_trace(
                    events,
                    "hook_deadline",
                    &["edge=Fired", "radial_intent=true"],
                )
            });
        let deadline_fired = has_trace(
            &deadline_events,
            "hook_deadline",
            &["edge=Fired", "radial_intent=true"],
        );
        let production_pre_release_pump = app_hook_thread.map_or_else(
            || "production pump probe unavailable: service thread id missing".into(),
            |thread_id| {
                probe_production_hook_pump(
                    thread_id,
                    child.process_id(),
                    trace_path,
                    Duration::from_millis(500),
                )
            },
        );
        let runner_pre_release_pump = hook_observer.as_ref().map_or_else(
            || "runner pump probe unavailable: observer intentionally absent".into(),
            |observer| {
                let probe_id = NEXT_HOOK_PUMP_PROBE_ID.fetch_add(1, Ordering::Relaxed);
                match observer.pump_roundtrip(probe_id, Duration::from_millis(500)) {
                    Ok(()) => format!(
                        "runner thread {} acknowledged pre-release probe {probe_id}",
                        observer.thread_id()
                    ),
                    Err(error) => error,
                }
            },
        );
        let handoff_description = format!(
            "{handoff_description}; after hold deadline fired={deadline_fired}, before F11 up: {production_pre_release_pump}; {runner_pre_release_pump}"
        );
        let down_observation = hook_observer
            .as_mut()
            .map(|observer| observer.wait_for_vk(0x7A, Duration::from_millis(50)));
        let up = hold_guard
            .release()
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        let up_observation = hook_observer
            .as_mut()
            .map(|observer| observer.wait_for_vk(0x7A, Duration::from_millis(250)));
        let runner_thread_after_release = hook_observer
            .as_ref()
            .map(|observer| format!("runner observer {}", thread_liveness(observer.thread_id())));
        let app_thread_after_release = app_hook_thread
            .map(|thread_id| format!("app hook {}", thread_liveness(thread_id)))
            .unwrap_or_else(|| "app hook thread id unavailable".into());
        // Never inject a second key while the toggle chord is still held.  The
        // native hook must observe the actual key-up edge before an independent
        // sentinel is sent to prove the hook chain remains active afterward.
        let sentinel_cursor = trace_lines(trace_path).len();
        let sentinel_input = child
            .press_hook_sentinel(root.hwnd, child.process_id())
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        let sentinel_observation = hook_observer
            .as_mut()
            .map(|observer| observer.wait_for_vk(0x87, Duration::from_millis(100)));
        let runner_thread_after_sentinel = hook_observer
            .as_ref()
            .map(|observer| format!("runner observer {}", thread_liveness(observer.thread_id())));
        let app_thread_after_sentinel = app_hook_thread
            .map(|thread_id| format!("app hook {}", thread_liveness(thread_id)))
            .unwrap_or_else(|| "app hook thread id unavailable".into());
        let sentinel_trace = wait_trace(
            trace_path,
            sentinel_cursor,
            Duration::from_millis(250),
            |events| {
                (has_trace(
                    events,
                    "hook_observed",
                    &["vk=135", "down=true", "injected=true"],
                ) && has_trace(
                    events,
                    "hook_observed",
                    &["vk=135", "down=false", "injected=true"],
                )) || has_trace(events, "frontend_key", &["key=F24"])
            },
        );
        let runner_observation = down_observation
            .as_ref()
            .zip(up_observation.as_ref())
            .map(|(down, up)| down.merge(up))
            .or_else(|| down_observation.or(up_observation));
        let runner_observation_text = runner_observation
            .as_ref()
            .map(|observation| {
                let sentinel = sentinel_observation.as_ref().map_or_else(
                    || "runner F24 sentinel observer unavailable".to_string(),
                    |sentinel| format!("{}", sentinel.describe()),
                );
                format!(
                    "{}; after-hold sentinel input=[{}] observer=[{}] production_pair={}",
                    observation.describe(),
                    sentinel_input.describe(),
                    sentinel,
                    has_trace(
                        &sentinel_trace,
                        "hook_observed",
                        &["vk=135", "down=true", "injected=true"]
                    ) && has_trace(
                        &sentinel_trace,
                        "hook_observed",
                        &["vk=135", "down=false", "injected=true"]
                    )
                )
            })
            .or_else(|| hook_observer_error.map(|error| format!("observer unavailable: {error}")))
            .unwrap_or_else(|| "observer unavailable".into());
        let thread_liveness_text = format!(
            "thread liveness before=[{}; {}], after sentinel=[{}; {}], after release=[{}; {}]",
            runner_thread_before
                .as_deref()
                .unwrap_or("runner observer unavailable"),
            app_thread_before,
            runner_thread_after_sentinel
                .as_deref()
                .unwrap_or("runner observer unavailable"),
            app_thread_after_sentinel,
            runner_thread_after_release
                .as_deref()
                .unwrap_or("runner observer unavailable"),
            app_thread_after_release
        );
        let frontend_key_observed = has_trace(&sentinel_trace, "frontend_key", &["key=F24"]);
        let runner_observation_text = format!(
            "{runner_observation_text}; foreground framework received F24 WM_KEYDOWN={frontend_key_observed}; {thread_liveness_text}"
        );
        let lines = wait_trace(trace_path, cursor, TRACE_TIMEOUT, |events| {
            has_trace(
                events,
                "hook_primary",
                &["transition=Release", "provenance=ExternalInjected"],
            ) && has_trace(
                events,
                "configured_primary",
                &[
                    "transition=Release",
                    "provenance=ExternalInjected",
                    "modifiers_match=true",
                ],
            )
        });
        let release_traced = has_trace(
            &lines,
            "hook_primary",
            &["transition=Release", "provenance=ExternalInjected"],
        ) && has_trace(
            &lines,
            "configured_primary",
            &[
                "transition=Release",
                "provenance=ExternalInjected",
                "modifiers_match=true",
            ],
        );
        let release_observed = runner_observation.as_ref().is_some_and(|observation| {
            observation.down_seen
                && observation.up_seen
                && observation.down_injected
                && observation.up_injected
        });
        let sentinel_observed = sentinel_observation.as_ref().is_some_and(|observation| {
            observation.down_seen
                && observation.up_seen
                && observation.down_injected
                && observation.up_injected
        });
        let sentinel_traced = has_trace(
            &sentinel_trace,
            "hook_observed",
            &["vk=135", "down=true", "injected=true"],
        ) && has_trace(
            &sentinel_trace,
            "hook_observed",
            &["vk=135", "down=false", "injected=true"],
        );
        let recovery_diagnostic =
            if !release_traced || !release_observed || !sentinel_observed || !sentinel_traced {
                Some(diagnose_hook_delivery_after_hold(
                    child,
                    root.hwnd,
                    app_hook_thread,
                    &mut hook_observer,
                    trace_path,
                ))
            } else {
                None
            };
        let closed = wait_until(Duration::from_secs(2), || {
            radial_surfaces_are_inactive(child, held_windows)
        });
        if !closed {
            let current = child.windows();
            let active_radial = active_radial_surfaces(&current, child.process_id());
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                format!(
                    "second full hold did not remove, hide, or park every radial surface [{}]; final active class-matched surfaces=[{}]; production_release={release_traced}; after_hold_production_sentinel={sentinel_traced}; down=[{}] up=[{}]; {runner_observation_text}; {handoff_description}; recovery diagnostic=[{}]",
                    describe_radial_surfaces(held_windows),
                    describe_radial_surfaces(&active_radial),
                    down.describe(),
                    up.describe(),
                    recovery_diagnostic.as_deref().unwrap_or("not run")
                ),
            ));
        }
        if !release_traced || !release_observed || !sentinel_observed || !sentinel_traced {
            return Err(CaseFailure::new(
                FailureStage::HookAdmission,
                format!(
                    "second hold release was not proven by both production hook and independent observer; production_release={release_traced}; after_hold_observer_sentinel={sentinel_observed}; after_hold_production_sentinel={sentinel_traced}; {runner_observation_text}; {handoff_description}; recovery diagnostic=[{}]; down=[{}] up=[{}]",
                    recovery_diagnostic.as_deref().unwrap_or("not run"),
                    down.describe(),
                    up.describe()
                ),
            ));
        }
        if has_trace(&lines, "short_tap", &[]) || has_trace(&lines, "desired_visibility", &[]) {
            return Err(CaseFailure::new(
                FailureStage::GestureDecision,
                format!(
                    "second hold co-fired a short tap or ROOT visibility edge; down=[{}] up=[{}]",
                    down.describe(),
                    up.describe()
                ),
            ));
        }
        let root = child
            .refresh_root()
            .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
        require_visible(&root).map_err(|error| {
            CaseFailure::new(
                FailureStage::NativeRootState,
                format!("ROOT visibility changed during second hold: {error}"),
            )
        })?;
        if !same_window_state(&root_before, &root) {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                format!(
                    "second hold changed ROOT HWND/state/bounds: before={}; after={}",
                    describe_root_snapshot(&root_before),
                    describe_root_snapshot(&root)
                ),
            ));
        }
        let final_windows = child.windows();
        let active_radial = active_radial_surfaces(&final_windows, child.process_id());
        if !radial_surface_set_and_owner_are_inactive(
            held_windows,
            &final_windows,
            child.process_id(),
        ) {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                format!(
                    "H6 left candidate-owned radial surfaces active after closing the captured pair [{}]; final active class-matched surfaces=[{}]",
                    describe_radial_surfaces(held_windows),
                    describe_radial_surfaces(&active_radial)
                ),
            ));
        }
        Ok(format!(
            "second threshold hold closed radial surfaces [{}] while ROOT stayed unchanged; down=[{}] up=[{}]; {runner_observation_text}; {handoff_description}",
            describe_radial_surfaces(held_windows),
            down.describe(),
            up.describe()
        ))
    })();
    append_case(
        report,
        "H6",
        expected("H6"),
        started,
        result,
        Some(child),
        output,
        trace_path,
    );
}

fn diagnose_hook_delivery_after_hold(
    child: &NativeChild,
    expected_root_hwnd: windows::Win32::Foundation::HWND,
    app_hook_thread: Option<u32>,
    old_observer: &mut Option<RunnerHookObserver>,
    trace_path: &Path,
) -> String {
    let mut evidence = Vec::new();
    let app_probe_id = NEXT_HOOK_PUMP_PROBE_ID.fetch_add(1, Ordering::Relaxed);
    let app_probe_cursor = trace_lines(trace_path).len();
    let app_pump_acknowledged = if let Some(thread_id) = app_hook_thread {
        match post_validated_hook_pump_probe(thread_id, child.process_id(), app_probe_id) {
            Ok(()) => {
                let events = wait_trace(
                    trace_path,
                    app_probe_cursor,
                    Duration::from_secs(1),
                    |events| {
                        has_trace(
                            events,
                            "hook_pump_probe",
                            &[&format!("probe_id={app_probe_id}")],
                        )
                    },
                );
                let acknowledged = has_trace(
                    &events,
                    "hook_pump_probe",
                    &[&format!("probe_id={app_probe_id}")],
                );
                evidence.push(format!(
                    "production thread {thread_id} belongs to child PID {}; posted probe {app_probe_id}; pump_ack={acknowledged}",
                    child.process_id()
                ));
                acknowledged
            }
            Err(error) => {
                evidence.push(format!("production pump probe failed: {error}"));
                false
            }
        }
    } else {
        evidence.push("production hook thread id unavailable".into());
        false
    };

    let old_thread_id = old_observer.as_ref().map(RunnerHookObserver::thread_id);
    let runner_probe_id = NEXT_HOOK_PUMP_PROBE_ID.fetch_add(1, Ordering::Relaxed);
    let old_runner_pump_acknowledged = if let Some(observer) = old_observer.as_ref() {
        match observer.pump_roundtrip(runner_probe_id, Duration::from_secs(1)) {
            Ok(()) => {
                evidence.push(format!(
                    "existing runner observer thread {} acknowledged pump probe {runner_probe_id}",
                    observer.thread_id()
                ));
                true
            }
            Err(error) => {
                evidence.push(error);
                false
            }
        }
    } else {
        evidence.push("existing runner observer unavailable for pump probe".into());
        false
    };

    // Remove the original independent hook before installing a new observer, so
    // this diagnostic can distinguish a stale hook registration from input loss.
    drop(old_observer.take());
    let mut fresh_observer = match RunnerHookObserver::start() {
        Ok(observer) => observer,
        Err(error) => {
            evidence.push(format!("fresh runner observer install failed: {error}"));
            return format!(
                "{}; no fresh observer could be installed",
                evidence.join("; ")
            );
        }
    };
    let fresh_thread_id = fresh_observer.thread_id();
    let fresh_desktop = fresh_observer.desktop.clone();
    let fresh_desktop_is_default = fresh_desktop == "Default";
    let fresh_probe_id = NEXT_HOOK_PUMP_PROBE_ID.fetch_add(1, Ordering::Relaxed);
    let fresh_pump_acknowledged =
        match fresh_observer.pump_roundtrip(fresh_probe_id, Duration::from_secs(1)) {
            Ok(()) => true,
            Err(error) => {
                evidence.push(error);
                false
            }
        };
    evidence.push(format!(
        "fresh runner observer thread {fresh_thread_id} replaced {:?}; desktop={fresh_desktop:?} default={fresh_desktop_is_default}; pump_ack={fresh_pump_acknowledged}",
        old_thread_id
    ));

    let fresh_sentinel = (|| {
        if !fresh_desktop_is_default {
            return Err(format!(
                "refused fresh-observer F24 because its desktop was {fresh_desktop:?}, expected Default"
            ));
        }
        let root = child.refresh_root()?;
        if root.hwnd != expected_root_hwnd {
            return Err(format!(
                "refused fresh-observer F24 because ROOT HWND changed from {} to {}",
                hwnd_id(expected_root_hwnd),
                hwnd_id(root.hwnd)
            ));
        }
        focus_is_validated(expected_root_hwnd, child.process_id())?;
        let cursor = trace_lines(trace_path).len();
        let input = child.press_hook_sentinel(expected_root_hwnd, child.process_id())?;
        let runner = fresh_observer.wait_for_vk(0x87, Duration::from_secs(1));
        let events = wait_trace(trace_path, cursor, Duration::from_secs(1), |events| {
            has_trace(
                events,
                "hook_observed",
                &["vk=135", "down=true", "injected=true"],
            ) && has_trace(
                events,
                "hook_observed",
                &["vk=135", "down=false", "injected=true"],
            )
        });
        let production_pair = has_trace(
            &events,
            "hook_observed",
            &["vk=135", "down=true", "injected=true"],
        ) && has_trace(
            &events,
            "hook_observed",
            &["vk=135", "down=false", "injected=true"],
        );
        let runner_pair =
            runner.down_seen && runner.up_seen && runner.down_injected && runner.up_injected;
        Ok(format!(
            "checked F24 targeted foreground ROOT HWND={} PID={}; fresh observer desktop={} runner_pair={runner_pair}; production_pair={production_pair}; input=[{}]",
            hwnd_id(expected_root_hwnd),
            child.process_id(),
            fresh_desktop,
            input.describe()
        ))
    })();
    match fresh_sentinel {
        Ok(result) => evidence.push(result),
        Err(error) => evidence.push(format!("fresh checked F24 diagnostic failed: {error}")),
    }

    evidence.push(format!(
        "classification: production_pump_ack={app_pump_acknowledged}, old_runner_pump_ack={old_runner_pump_acknowledged}, fresh_runner_pump_ack={fresh_pump_acknowledged}"
    ));
    evidence.join("; ")
}

fn checked_production_f24_without_runner_observer(
    child: &NativeChild,
    expected_root_hwnd: windows::Win32::Foundation::HWND,
    trace_path: &Path,
) -> String {
    if let Err(error) = focus_is_validated(expected_root_hwnd, child.process_id()) {
        return format!(
            "refused F24 because ROOT HWND={} PID={} was not foreground: {error}",
            hwnd_id(expected_root_hwnd),
            child.process_id()
        );
    }
    let cursor = trace_lines(trace_path).len();
    let input = match child.press_hook_sentinel(expected_root_hwnd, child.process_id()) {
        Ok(input) => input,
        Err(error) => return format!("checked production-only F24 insertion failed: {error}"),
    };
    let events = wait_trace(trace_path, cursor, Duration::from_secs(1), |events| {
        (has_trace(
            events,
            "hook_observed",
            &["vk=135", "down=true", "injected=true"],
        ) && has_trace(
            events,
            "hook_observed",
            &["vk=135", "down=false", "injected=true"],
        )) || has_trace(events, "frontend_key", &["key=F24"])
    });
    let production_pair = has_trace(
        &events,
        "hook_observed",
        &["vk=135", "down=true", "injected=true"],
    ) && has_trace(
        &events,
        "hook_observed",
        &["vk=135", "down=false", "injected=true"],
    );
    let frontend_received = has_trace(&events, "frontend_key", &["key=F24"]);
    format!(
        "checked F24 targeted foreground ROOT HWND={} PID={} with no runner observer; production_pair={production_pair}; frontend_received={frontend_received}; input=[{}]",
        hwnd_id(expected_root_hwnd),
        child.process_id(),
        input.describe()
    )
}

fn probe_production_hook_pump(
    thread_id: u32,
    child_process_id: u32,
    trace_path: &Path,
    timeout: Duration,
) -> String {
    let probe_id = NEXT_HOOK_PUMP_PROBE_ID.fetch_add(1, Ordering::Relaxed);
    let cursor = trace_lines(trace_path).len();
    if let Err(error) = post_validated_hook_pump_probe(thread_id, child_process_id, probe_id) {
        return format!("production pre-release pump probe {probe_id} failed: {error}");
    }
    let events = wait_trace(trace_path, cursor, timeout, |events| {
        has_trace(
            events,
            "hook_pump_probe",
            &[&format!("probe_id={probe_id}")],
        )
    });
    let acknowledged = has_trace(
        &events,
        "hook_pump_probe",
        &[&format!("probe_id={probe_id}")],
    );
    format!(
        "production thread {thread_id} acknowledged pre-release pump probe {probe_id}={acknowledged}"
    )
}

fn run_designer_entry(
    child: &mut NativeChild,
    uia: &UiAutomation,
    trace_path: &Path,
) -> Result<WindowSnapshot, CaseFailure> {
    let root = child
        .refresh_root()
        .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
    require_visible(&root)
        .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
    if !uia.root_is_queryable(root.hwnd, child.process_id()) {
        return Err(CaseFailure::new(
            FailureStage::DesignerEntry,
            "ROOT UIA root is not owned by the launched candidate".into(),
        ));
    }
    let trace_cursor = trace_lines(trace_path).len();
    let file_click = activate_named(
        uia,
        child,
        &root,
        "File",
        FailureStage::DesignerEntry,
        trace_path,
    )?;
    let apps_click = activate_named(
        uia,
        child,
        &root,
        "Apps",
        FailureStage::DesignerEntry,
        trace_path,
    )
    .map_err(|error| {
        CaseFailure::new(
            error.stage,
            format!(
                "{}; preceding checked File menu click=[{}]",
                error.message,
                file_click.describe()
            ),
        )
    })?;
    let edit_control = uia
        .wait_named(
            root.hwnd,
            child.process_id(),
            "Edit Radial Menus",
            UIA_TIMEOUT,
        )
        .map_err(|error| CaseFailure::new(FailureStage::DesignerEntry, error))?;
    let edit_click = click_semantic_control(child, &root, &edit_control, trace_path)
        .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
    let mut enter_events = None;
    if !wait_until(Duration::from_millis(750), || {
        find_child_window(child, WindowRole::Designer).is_some()
    }) {
        child
            .focus_window(&root)
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        uia.focus(&edit_control)
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        enter_events = Some(
            send_enter_current(child, &root)
                .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?,
        );
    }
    let designer = wait_for_designer(child)
        .map_err(|error| {
            CaseFailure::new(
                FailureStage::DesignerEntry,
                format!(
                    "{error}; checked semantic pointer events inserted: File=[{}], Apps=[{}], Edit Radial Menus=[{}]; validated UIA focus+Enter events inserted={enter_events:?}",
                    file_click.describe(),
                    apps_click.describe(),
                    edit_click.describe()
                ),
            )
        })?;
    if child
        .windows()
        .iter()
        .filter(|window| window.role == WindowRole::Designer)
        .count()
        != 1
    {
        return Err(CaseFailure::new(
            FailureStage::DesignerNativeTarget,
            "expected exactly one child-owned Radial Designer HWND after entry".into(),
        ));
    }
    if !uia.root_is_queryable(designer.hwnd, child.process_id()) {
        return Err(CaseFailure::new(
            FailureStage::DesignerNativeTarget,
            "Designer HWND was found but its UIA root was not queryable as the child PID".into(),
        ));
    }
    let snapshot_accepted = wait_trace(
        trace_path,
        trace_cursor,
        Duration::from_secs(10),
        |events| {
            has_trace(
                events,
                "authoring",
                &["edge=ReplyAccepted", "request_kind=Snapshot"],
            )
        },
    );
    if !has_trace(
        &snapshot_accepted,
        "authoring",
        &["edge=ReplyAccepted", "request_kind=Snapshot"],
    ) {
        return Err(CaseFailure::new(
            FailureStage::DesignerReadiness,
            "Designer did not emit an accepted InitialSnapshot reply; UIA enabled state alone is not sufficient readiness evidence".into(),
        ));
    }
    // The accepted InitialSnapshot is the entry-readiness signal. D1 separately proves
    // readiness through a real semantic Tree click, production Enabled body trace, and
    // changed widget state; AccessKit's enabled bit alone is not a reliable loading signal.
    Ok(designer)
}

fn run_designer_focus_case(
    report: &mut AcceptanceReport,
    child: &mut NativeChild,
    uia: &UiAutomation,
    designer: &WindowSnapshot,
    trace_path: &Path,
    output: &Path,
) {
    let started = Instant::now();
    let result = (|| {
        if !uia.root_is_queryable(designer.hwnd, child.process_id()) {
            return Err(CaseFailure::new(
                FailureStage::DesignerNativeTarget,
                "Designer HWND stopped responding before H3".into(),
            ));
        }
        let mut cursor = trace_lines(trace_path).len();
        for visible in [false, true] {
            child
                .focus_window(designer)
                .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
            let (mut observer, observer_error) = match RunnerHookObserver::start() {
                Ok(observer) => (Some(observer), None),
                Err(error) => (None, Some(error)),
            };
            let tap = child
                .send_f11(designer.hwnd, child.process_id(), TAP_TIME)
                .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
            let observation = observer
                .as_mut()
                .map(|observer| observer.wait_for_vk(0x7A, Duration::from_secs(1)));
            let observer_text = observation
                .as_ref()
                .map(RunnerHookObservation::describe)
                .or_else(|| observer_error.map(|error| format!("observer unavailable: {error}")))
                .unwrap_or_else(|| "observer unavailable".into());
            if !wait_root_visibility(child, visible, ROOT_TIMEOUT) {
                let events = trace_lines(trace_path)
                    .into_iter()
                    .skip(cursor)
                    .filter(|line| {
                        line.contains("trace_event=\"hook_observed\"")
                            || line.contains("trace_event=\"hook_primary\"")
                            || line.contains("trace_event=\"configured_primary\"")
                            || line.contains("trace_event=\"hook_callback\"")
                    })
                    .collect::<Vec<_>>();
                return Err(CaseFailure::new(
                    FailureStage::NativeRootState,
                    format!(
                        "Designer-focused F11 failed to toggle ROOT to visible={visible}; checked tap=[{}]; {observer_text}; production hook edges={:?}",
                        tap.describe(),
                        events
                    ),
                ));
            }
            if !uia.root_is_queryable(designer.hwnd, child.process_id()) {
                return Err(CaseFailure::new(
                    FailureStage::DesignerReadiness,
                    "Designer stopped servicing UIA after the ROOT toggle".into(),
                ));
            }
            let events = wait_trace(trace_path, cursor, TRACE_TIMEOUT, |events| {
                tap_trace_complete(events, 1, visible)
            });
            if !tap_trace_complete(&events, 1, visible) {
                return Err(CaseFailure::new(
                    FailureStage::GestureDecision,
                    "Designer-focused tap did not produce the configured short-tap path".into(),
                ));
            }
            cursor = trace_lines(trace_path).len();
        }
        Ok(format!(
            "Designer HWND={} remained queryable while two checked focused taps toggled ROOT only",
            hwnd_id(designer.hwnd)
        ))
    })();
    append_case(
        report,
        "H3",
        expected("H3"),
        started,
        result,
        Some(child),
        output,
        trace_path,
    );
}

fn run_designer_pointer_case(
    report: &mut AcceptanceReport,
    child: &mut NativeChild,
    uia: &UiAutomation,
    designer: &WindowSnapshot,
    trace_path: &Path,
    output: &Path,
) {
    let started = Instant::now();
    let result = (|| {
        save_uia_snapshot("D1", "designer", uia, designer.hwnd, output);
        let before = wait_for_designer_semantic_target(
            trace_path,
            DesignerSemanticTarget::Tree,
            UIA_TIMEOUT,
            |_| true,
        )
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerNativeTarget,
                "production Designer did not publish its Tree semantic target".into(),
            )
        })?;
        if before.selected {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "Tree semantic target was already selected before the native click".into(),
            ));
        }
        let cursor = trace_lines(trace_path).len();
        let click_evidence =
            click_designer_client_bounds(child, designer, before.bounds, trace_path)
                .map_err(|error| CaseFailure::new(FailureStage::DesignerNativeTarget, error))?;
        let events = wait_trace(trace_path, cursor, TRACE_TIMEOUT, |events| {
            has_trace(events, "designer_pointer", &["pointer_down=true"])
                && has_trace(events, "designer_pointer", &["pointer_up=true"])
                && has_trace(events, "designer_body", &["state=Enabled"])
                && has_trace(
                    events,
                    "designer_widget",
                    &["category=Tree", "response=Accepted"],
                )
                && events.iter().any(|line| {
                    parse_designer_semantic_target(line, DesignerSemanticTarget::Tree)
                        .is_some_and(|state| state.selected)
                })
        });
        if !has_trace(&events, "designer_pointer", &["pointer_down=true"])
            || !has_trace(&events, "designer_pointer", &["pointer_up=true"])
            || !has_trace(&events, "designer_body", &["state=Enabled"])
            || !has_trace(
                &events,
                "designer_widget",
                &["category=Tree", "response=Accepted"],
            )
            || !events.iter().any(|line| {
                parse_designer_semantic_target(line, DesignerSemanticTarget::Tree)
                    .is_some_and(|state| state.selected != before.selected)
            })
        {
            return Err(CaseFailure::new(
                FailureStage::DesignerFrameworkInput,
                format!(
                    "native click did not reach the production Designer pointer/body/Tree accepted boundaries; click proof=[{}]",
                    click_evidence.describe()
                ),
            ));
        }
        Ok(format!(
            "native click on production egui Tree SelectableLabel changed selected {} -> true and reached Enabled body/accepted widget; client bounds={:?}; click edges=[{}]",
            before.selected,
            before.bounds,
            click_evidence.describe()
        ))
    })();
    append_case(
        report,
        "D1",
        expected("D1"),
        started,
        result,
        Some(child),
        output,
        trace_path,
    );
}

fn run_tab_case(
    report: &mut AcceptanceReport,
    child: &mut NativeChild,
    uia: &UiAutomation,
    designer: &WindowSnapshot,
    output: &Path,
    trace_path: &Path,
) {
    let started = Instant::now();
    let result = (|| {
        save_uia_snapshot("D2", "designer", uia, designer.hwnd, output);
        let tree = wait_for_designer_semantic_target(
            trace_path,
            DesignerSemanticTarget::Tree,
            UIA_TIMEOUT,
            |_| true,
        )
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerNativeTarget,
                "production Designer did not publish its Tree semantic target".into(),
            )
        })?;
        let mut click_proofs = Vec::new();
        if !tree.selected {
            let click = click_designer_client_bounds(child, designer, tree.bounds, trace_path)
                .map_err(|error| CaseFailure::new(FailureStage::DesignerNativeTarget, error))?;
            click_proofs.push(format!("Tree=[{}]", click.describe()));
            wait_for_designer_semantic_target(
                trace_path,
                DesignerSemanticTarget::Tree,
                TRACE_TIMEOUT,
                |state| state.selected,
            )
            .ok_or_else(|| {
                CaseFailure::new(
                    FailureStage::DesignerFrameworkInput,
                    "native Tree click did not show the Tree pane".into(),
                )
            })?;
        }
        let inspector = wait_for_designer_semantic_target(
            trace_path,
            DesignerSemanticTarget::Inspector,
            UIA_TIMEOUT,
            |_| true,
        )
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerNativeTarget,
                "production Designer did not publish its Inspector toolbar target".into(),
            )
        })?;
        if !inspector.selected {
            let inspector_cursor = trace_lines(trace_path).len();
            let click = click_designer_client_bounds(child, designer, inspector.bounds, trace_path)
                .map_err(|error| CaseFailure::new(FailureStage::DesignerNativeTarget, error))?;
            click_proofs.push(format!("Inspector=[{}]", click.describe()));
            let inspector_events =
                wait_trace(trace_path, inspector_cursor, TRACE_TIMEOUT, |events| {
                    events.iter().any(|line| {
                        parse_designer_semantic_target(line, DesignerSemanticTarget::Inspector)
                            .is_some_and(|state| state.selected)
                    })
                });
            let inspector_selected = inspector_events.iter().any(|line| {
                parse_designer_semantic_target(line, DesignerSemanticTarget::Inspector)
                    .is_some_and(|state| state.selected)
            });
            if !inspector_selected {
                return Err(CaseFailure::new(
                    FailureStage::DesignerFrameworkInput,
                    format!(
                        "native Inspector click did not show the Inspector pane; native setup clicks={click_proofs:?}; Inspector pointer/semantic edges={:?}",
                        inspector_events
                            .iter()
                            .filter(|line| {
                                line.contains("designer_widget_pointer")
                                    || line.contains("designer_widget")
                                    || line.contains("target=Inspector")
                            })
                            .collect::<Vec<_>>()
                    ),
                ));
            }
        }
        let default_menu = wait_for_designer_semantic_target(
            trace_path,
            DesignerSemanticTarget::DefaultMenu,
            UIA_TIMEOUT,
            |_| true,
        )
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerNativeTarget,
                "production Designer did not publish the default menu tree target".into(),
            )
        })?;
        if !default_menu.selected {
            let click =
                click_designer_client_bounds(child, designer, default_menu.bounds, trace_path)
                    .map_err(|error| CaseFailure::new(FailureStage::DesignerNativeTarget, error))?;
            click_proofs.push(format!("default menu=[{}]", click.describe()));
            wait_for_designer_semantic_target(
                trace_path,
                DesignerSemanticTarget::DefaultMenu,
                TRACE_TIMEOUT,
                |state| state.selected,
            )
            .ok_or_else(|| {
                CaseFailure::new(
                    FailureStage::DesignerFrameworkInput,
                    "native default-menu click did not select the production menu".into(),
                )
            })?;
        }
        let menu_name = wait_for_designer_semantic_target(
            trace_path,
            DesignerSemanticTarget::MenuName,
            UIA_TIMEOUT,
            |_| true,
        )
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerNativeTarget,
                "production menu Inspector did not publish its real TextEdit".into(),
            )
        })?;
        let focus_cursor = trace_lines(trace_path).len();
        let name_click =
            click_designer_client_bounds(child, designer, menu_name.bounds, trace_path)
                .map_err(|error| CaseFailure::new(FailureStage::DesignerNativeTarget, error))?;
        click_proofs.push(format!("menu TextEdit=[{}]", name_click.describe()));
        let focused_name = wait_trace(trace_path, focus_cursor, TRACE_TIMEOUT, |events| {
            events.iter().any(|line| {
                parse_designer_semantic_target(line, DesignerSemanticTarget::MenuName)
                    .is_some_and(|state| state.focused)
            })
        });
        if !focused_name.iter().any(|line| {
            parse_designer_semantic_target(line, DesignerSemanticTarget::MenuName)
                .is_some_and(|state| state.focused)
        }) {
            return Err(CaseFailure::new(
                FailureStage::DesignerFrameworkInput,
                format!(
                    "native click on the production menu TextEdit did not acquire egui focus; click=[{}]",
                    name_click.describe()
                ),
            ));
        }
        let uia_menu_edit_focus = uia
            .edit_focus_at_screen_point(designer.hwnd, child.process_id(), name_click.screen_point)
            .map_err(|error| CaseFailure::new(FailureStage::DesignerNativeTarget, error))?;
        let uia_limitation = match uia_menu_edit_focus {
            Some(true) => {
                "UIA Edit at the production TextEdit point reports focus; production egui trace independently confirms the target"
            }
            Some(false) => {
                "UIA exposes an Edit at the production TextEdit point but does not report its keyboard focus; production egui trace is the focus fallback"
            }
            None => {
                "UIA exposes no Edit node at the production TextEdit point; production egui semantic trace supplies target bounds and focus"
            }
        };

        let edit_cursor = trace_lines(trace_path).len();
        let probe_input = replace_focused_designer_text(child, designer, DESIGNER_TEXT_PROBE);
        let (probe_input_count, probe_error) = match probe_input {
            Ok(count) => (Some(count), None),
            Err(error) => (None, Some(error)),
        };
        let edited = wait_trace(trace_path, edit_cursor, TRACE_TIMEOUT, |events| {
            has_trace(
                events,
                "designer_edit_state",
                &[
                    "widget_changed=true",
                    "model_changed=true",
                    "input_matches_model=true",
                    "draft_dirty=true",
                ],
            )
        });
        let model_edit_proved = has_trace(
            &edited,
            "designer_edit_state",
            &[
                "widget_changed=true",
                "model_changed=true",
                "input_matches_model=true",
                "draft_dirty=true",
            ],
        );
        if !model_edit_proved {
            let restored = replace_focused_designer_text(child, designer, DESIGNER_STARTER_NAME);
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "native Unicode input did not prove a production TextEdit-to-model mutation; input={probe_input_count:?}, error={probe_error:?}, baseline restore={restored:?}"
                ),
            ));
        }

        let restore_cursor = trace_lines(trace_path).len();
        let restore_input =
            replace_focused_designer_text(child, designer, DESIGNER_STARTER_NAME)
                .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        let restored = wait_trace(trace_path, restore_cursor, TRACE_TIMEOUT, |events| {
            has_trace(
                events,
                "designer_edit_state",
                &[
                    "widget_changed=true",
                    "input_matches_model=true",
                    "draft_dirty=false",
                ],
            )
        });
        if !has_trace(
            &restored,
            "designer_edit_state",
            &[
                "widget_changed=true",
                "input_matches_model=true",
                "draft_dirty=false",
            ],
        ) {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "native TextEdit input changed the production draft, but the deterministic starter value was not restored to a clean checkpoint; restore input events={restore_input}"
                ),
            ));
        }

        let cursor = trace_lines(trace_path).len();
        let count = send_tab(child, designer)
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        let events = wait_trace(trace_path, cursor, TRACE_TIMEOUT, |events| {
            events.iter().any(|line| {
                parse_designer_semantic_target(line, DesignerSemanticTarget::MenuDefaultSkin)
                    .is_some_and(|state| state.focused)
            })
        });
        let next = events.iter().find_map(|line| {
            parse_designer_semantic_target(line, DesignerSemanticTarget::MenuDefaultSkin)
                .filter(|state| state.focused)
        });
        let Some(next_state) = next else {
            return Err(CaseFailure::new(
                FailureStage::DesignerFrameworkInput,
                "checked Tab did not move focus from the production menu TextEdit to its menu-skin ComboBox"
                    .into(),
            ));
        };
        Ok(format!(
            "{} checked native Tab events moved production Designer focus from the real menu TextEdit {:?} to its ComboBox {:?}; native Unicode edit ({:?} SendInput events) changed the authoring model and was restored without saving; UIA focus limitation/fallback: {uia_limitation}; native setup clicks={:?}",
            count, menu_name.bounds, next_state.bounds, probe_input_count, click_proofs
        ))
    })();
    append_case(
        report,
        "D2",
        expected("D2"),
        started,
        result,
        Some(child),
        output,
        trace_path,
    );
}

fn replace_focused_designer_text(
    child: &NativeChild,
    designer: &WindowSnapshot,
    value: &str,
) -> Result<usize, String> {
    let selection_events = send_select_all_to_focused_window(child, designer)?;
    let text_events = send_text_to_focused_window(child, designer, value)?;
    Ok(selection_events.saturating_add(text_events))
}

fn run_skins_command_case(
    report: &mut AcceptanceReport,
    child: &mut NativeChild,
    uia: &UiAutomation,
    designer: &WindowSnapshot,
    trace_path: &Path,
    output: &Path,
) {
    let started = Instant::now();
    let result = (|| {
        let root = child
            .refresh_root()
            .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
        require_visible(&root)
            .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
        child
            .focus_window(&root)
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        save_uia_snapshot("D4", "root", uia, root.hwnd, output);
        save_uia_snapshot("D4", "designer", uia, designer.hwnd, output);
        let edit = uia
            .find_first_edit(root.hwnd, child.process_id())
            .map_err(|error| CaseFailure::new(FailureStage::DesignerEntry, error))?
            .ok_or_else(|| {
                CaseFailure::new(
                    FailureStage::DesignerEntry,
                    "ROOT did not publish its semantic Edit query control in UIA".into(),
                )
            })?;
        let typed = send_text(child, &root, &edit, &uia, "radial skins")
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        save_uia_snapshot("D4", "root-after-type", uia, root.hwnd, output);
        let result_control = uia
            .wait_named(
                root.hwnd,
                child.process_id(),
                "Edit radial skins : Radial menu",
                UIA_TIMEOUT,
            )
            .map_err(|error| CaseFailure::new(FailureStage::DesignerEntry, error))?;
        let semantic_cursor = trace_lines(trace_path).len();
        let click = click_semantic_control(child, &root, &result_control, trace_path)
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        let same = wait_until(Duration::from_secs(4), || {
            find_child_window(child, WindowRole::Designer)
                .is_some_and(|window| window.hwnd == designer.hwnd)
        });
        if !same {
            return Err(CaseFailure::new(
                FailureStage::DesignerNativeTarget,
                "radial skins command replaced or closed the existing Designer HWND".into(),
            ));
        }
        let skins_selected = wait_trace(trace_path, semantic_cursor, TRACE_TIMEOUT, |events| {
            events.iter().any(|line| {
                parse_designer_semantic_target(line, DesignerSemanticTarget::Skins)
                    .is_some_and(|state| state.selected)
            })
        });
        if !skins_selected.iter().any(|line| {
            parse_designer_semantic_target(line, DesignerSemanticTarget::Skins)
                .is_some_and(|state| state.selected)
        }) {
            let (foreground_hwnd, foreground_pid) = capture_foreground();
            return Err(CaseFailure::new(
                FailureStage::DesignerPresentation,
                format!(
                    "production radial skins command did not select the Skins semantic target; native click proof=[{}]; exact command bounds={:?}; foreground=HWND:{} PID:{}; post-click Designer/command traces={:?}",
                    click.describe(),
                    result_control.bounds,
                    hwnd_id(foreground_hwnd),
                    foreground_pid,
                    skins_selected
                        .iter()
                        .filter(|line| {
                            line.contains("designer_semantic_target")
                                || line.contains("root_command")
                                || line.contains("designer_focus")
                        })
                        .collect::<Vec<_>>()
                ),
            ));
        }
        if !uia.root_is_queryable(designer.hwnd, child.process_id()) {
            return Err(CaseFailure::new(
                FailureStage::DesignerReadiness,
                "same Designer HWND stopped responding after Skins entry".into(),
            ));
        }
        Ok(format!(
            "typed radial skins using {typed} checked Unicode events, found the exact command at {:?}, and activated it with a checked native pointer click=[{}]; Skins semantic target became selected on the same queryable Designer HWND={}",
            result_control.bounds,
            click.describe(),
            hwnd_id(designer.hwnd)
        ))
    })();
    append_case(
        report,
        "D4",
        expected("D4"),
        started,
        result,
        Some(child),
        output,
        trace_path,
    );
}

fn run_designer_close_case(
    report: &mut AcceptanceReport,
    child: &mut NativeChild,
    designer: &WindowSnapshot,
    output: &Path,
    trace_path: &Path,
) {
    let started = Instant::now();
    let result = (|| {
        child
            .validate_window(designer.hwnd)
            .map_err(|error| CaseFailure::new(FailureStage::DesignerNativeTarget, error))?;
        let close_cursor = trace_lines(trace_path).len();
        let events = send_alt_f4(child, designer)
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        let close_state = wait_trace(trace_path, close_cursor, TRACE_TIMEOUT, |events| {
            events
                .iter()
                .any(|line| line.contains("trace_event=\"designer_close\""))
        })
        .into_iter()
        .rev()
        .find(|line| line.contains("trace_event=\"designer_close\""));
        if !close_state
            .as_ref()
            .is_some_and(|line| line.contains("open=false"))
        {
            return Err(CaseFailure::new(
                FailureStage::Cleanup,
                format!(
                    "checked Alt+F4 did not produce a terminal clean production close state: {}; {events} checked input events",
                    close_state.as_deref().unwrap_or("no close decision trace")
                ),
            ));
        }
        let closed = wait_until(Duration::from_secs(4), || {
            find_child_window(child, WindowRole::Designer).is_none()
        });
        if !closed {
            return Err(CaseFailure::new(
                FailureStage::Cleanup,
                format!(
                    "Designer HWND remained after checked child-focused Alt+F4; production state={}",
                    close_state.as_deref().unwrap_or("missing")
                ),
            ));
        }
        if child
            .try_wait()
            .map_err(|error| CaseFailure::new(FailureStage::Cleanup, error))?
            .is_some()
        {
            return Err(CaseFailure::new(
                FailureStage::Cleanup,
                "closing Designer also terminated the launcher process".into(),
            ));
        }
        if find_child_window(child, WindowRole::Root).is_none() {
            return Err(CaseFailure::new(
                FailureStage::Cleanup,
                "ROOT HWND disappeared while closing Designer".into(),
            ));
        }
        Ok(format!(
            "{} checked Alt+F4 events closed Designer HWND={} while child process and ROOT remained alive; production close state={}",
            events,
            hwnd_id(designer.hwnd),
            close_state.as_deref().unwrap_or("missing")
        ))
    })();
    append_case(
        report,
        "D5",
        expected("D5"),
        started,
        result,
        Some(child),
        output,
        trace_path,
    );
}

fn activate_named(
    uia: &UiAutomation,
    child: &NativeChild,
    target: &WindowSnapshot,
    name: &str,
    stage: FailureStage,
    trace_path: &Path,
) -> Result<PointerClickEvidence, CaseFailure> {
    let control = uia
        .wait_named(target.hwnd, child.process_id(), name, UIA_TIMEOUT)
        .map_err(|error| CaseFailure::new(stage, error))?;
    if !control.enabled {
        return Err(CaseFailure::new(
            stage,
            format!("semantic control '{name}' is disabled"),
        ));
    }
    // Menu controls in the egui UIA provider may advertise Invoke without actually
    // expanding the corresponding menu. Use their process-validated client bounds so the
    // same native interaction the user performs advances each menu level.
    click_semantic_control(child, target, &control, &trace_path)
        .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))
}

fn append_blocked_designer_cases(
    report: &mut AcceptanceReport,
    cause: &CaseFailure,
    child: Option<&NativeChild>,
    output: &Path,
    trace_path: &Path,
) {
    for id in ["H3", "D1", "D2", "D4", "D5"] {
        if report.cases.iter().any(|case| case.id == id) {
            continue;
        }
        append_case(
            report,
            id,
            expected(id),
            started_now(),
            Err(CaseFailure::new(
                cause.stage,
                format!("not run because D0 failed first: {}", cause.message),
            )),
            child,
            output,
            trace_path,
        );
    }
}

fn stop_child(
    child: &mut NativeChild,
    report: &mut AcceptanceReport,
    runner_log: &mut File,
    output: &Path,
    trace_path: &Path,
) {
    let started = Instant::now();
    let result = (|| {
        if child.try_wait().ok().flatten().is_some() {
            report.cleanup.child_closed_normally = false;
            return Err(CaseFailure::new(
                FailureStage::Cleanup,
                "candidate exited before the runner requested normal shutdown".into(),
            ));
        }
        let mut close_errors = Vec::new();
        if let Ok(root) = child.refresh_root() {
            if let Err(error) = request_window_close(child, &root) {
                close_errors.push(error);
            }
        }
        if let Some(designer) = child.designer() {
            if let Err(error) = request_window_close(child, &designer) {
                close_errors.push(error);
            }
        }
        if let Some(status) = wait_child(child, Duration::from_secs(5)) {
            report.cleanup.child_closed_normally = status.success();
            if status.success() {
                return Ok(format!(
                    "candidate exited normally with {status} after bounded WM_CLOSE"
                ));
            }
            return Err(CaseFailure::new(
                FailureStage::Cleanup,
                format!("candidate exited with failure status {status}"),
            ));
        }
        child
            .kill()
            .map_err(|error| CaseFailure::new(FailureStage::Cleanup, error))?;
        let status = child
            .wait()
            .map_err(|error| CaseFailure::new(FailureStage::Cleanup, error))?;
        report.cleanup.child_terminated_after_timeout = true;
        report.cleanup.child_closed_normally = false;
        let close_details = if close_errors.is_empty() {
            String::new()
        } else {
            format!("; WM_CLOSE errors: {}", close_errors.join("; "))
        };
        Err(CaseFailure::new(
            FailureStage::Cleanup,
            format!(
                "normal close timed out; terminated only isolated child PID {} ({status}){close_details}",
                child.process_id(),
            ),
        ))
    })();
    let result =
        if let Some(cleanup_case) = report.cases.iter_mut().find(|case| case.id == "CLEANUP") {
            cleanup_case.elapsed_ms = elapsed_ms(started);
            match result {
                Ok(observed) => {
                    cleanup_case.status = CaseStatus::Passed;
                    cleanup_case.observed = observed;
                    cleanup_case.failure_stage = None;
                }
                Err(error) => {
                    cleanup_case.status = CaseStatus::Failed;
                    cleanup_case.observed = error.message;
                    cleanup_case.failure_stage = Some(error.stage);
                }
            }
            return;
        } else {
            result
        };
    append_case(
        report,
        "CLEANUP",
        expected("CLEANUP"),
        started,
        result,
        Some(child),
        output,
        trace_path,
    );
    let _ = writeln!(
        runner_log,
        "cleanup closed_normally={} terminated_after_timeout={}",
        report.cleanup.child_closed_normally, report.cleanup.child_terminated_after_timeout
    );
}

fn append_case(
    report: &mut AcceptanceReport,
    id: &str,
    expected: &str,
    started: Instant,
    result: Result<String, CaseFailure>,
    child: Option<&NativeChild>,
    output: &Path,
    trace_path: &Path,
) {
    let mut artifacts = Vec::new();
    let (status, observed, failure_stage) = match result {
        Ok(observed) => (CaseStatus::Passed, observed, None),
        Err(error) => {
            let saved = save_failure_artifacts(id, child, output, trace_path);
            for path in saved {
                report.push_artifact(path.to_string_lossy());
                artifacts.push(bounded_text(&path.to_string_lossy(), MAX_PATH_BYTES));
            }
            (CaseStatus::Failed, error.message, Some(error.stage))
        }
    };
    report.push_case(AcceptanceCaseResult {
        id: id.to_string(),
        status,
        elapsed_ms: elapsed_ms(started),
        expected: bounded_text(expected, MAX_RESULT_BYTES),
        observed: bounded_text(&observed, MAX_RESULT_BYTES),
        failure_stage,
        artifacts,
    });
}

fn save_failure_artifacts(
    id: &str,
    child: Option<&NativeChild>,
    output: &Path,
    trace_path: &Path,
) -> Vec<PathBuf> {
    let mut written = Vec::new();
    let trace_destination = output.join(format!("case-{id}-trace.log"));
    let trace_excerpt = fs::read_to_string(trace_path)
        .unwrap_or_default()
        .lines()
        .filter(|line| line.contains("trace_event=\"") || line.contains("radial acceptance"))
        .take(MAX_TRACE_EXCERPT)
        .collect::<Vec<_>>()
        .join("\n");
    if fs::write(&trace_destination, trace_excerpt).is_ok() {
        written.push(trace_destination);
    }
    let diagnostic_prefix = format!("case-{id}-uia-");
    if let Ok(entries) = fs::read_dir(output) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| {
                        name.starts_with(&diagnostic_prefix) && name.ends_with(".txt")
                    })
            {
                written.push(path);
            }
        }
    }
    let inventory_destination = output.join(format!("case-{id}-windows.json"));
    let windows = child.map(NativeChild::windows).unwrap_or_default();
    let record = WindowInventory {
        runner_process_id: std::process::id(),
        child_process_id: child.map(NativeChild::process_id),
        windows: windows.iter().map(WindowRecord::from).collect(),
    };
    if serde_json::to_vec_pretty(&record)
        .ok()
        .is_some_and(|bytes| fs::write(&inventory_destination, bytes).is_ok())
    {
        written.push(inventory_destination);
    }
    let screenshot_destination = output.join(format!("case-{id}.png"));
    if capture_diagnostic_screenshot(child, &screenshot_destination).is_ok() {
        written.push(screenshot_destination);
    }
    written
}

fn save_uia_snapshot(case_id: &str, role: &str, uia: &UiAutomation, hwnd: HWND, output: &Path) {
    let path = output.join(format!("case-{case_id}-uia-{role}.txt"));
    let content = uia
        .describe_tree(hwnd)
        .unwrap_or_else(|error| format!("UIA tree diagnostic failed: {error}"));
    let _ = fs::write(path, content);
}

fn save_launch_failure_inventory(failure: &NativeLaunchFailure, output: &Path) -> Option<PathBuf> {
    let destination = output.join("candidate-startup-windows.json");
    let record = WindowInventory {
        runner_process_id: std::process::id(),
        child_process_id: failure.process_id,
        windows: failure.windows.iter().map(WindowRecord::from).collect(),
    };
    let bytes = serde_json::to_vec_pretty(&record).ok()?;
    fs::write(&destination, bytes).ok()?;
    Some(destination)
}

fn capture_diagnostic_screenshot(child: Option<&NativeChild>, path: &Path) -> Result<(), String> {
    let candidate = child
        .and_then(|child| {
            child
                .windows()
                .into_iter()
                .find(|window| window.visible && window.intersects_virtual_screen())
        })
        .or_else(|| child.map(|child| child.root().clone()));
    let image = if let Some(window) = candidate {
        let center_x = window.bounds[0] + (window.bounds[2] - window.bounds[0]) / 2;
        let center_y = window.bounds[1] + (window.bounds[3] - window.bounds[1]) / 2;
        let screen = screenshots::Screen::from_point(center_x, center_y)
            .map_err(|error| format!("select screenshot monitor: {error}"))?;
        let info = screen.display_info;
        let left = (window.bounds[0] - info.x).max(0);
        let top = (window.bounds[1] - info.y).max(0);
        let right = (window.bounds[2] - info.x).min(info.width as i32);
        let bottom = (window.bounds[3] - info.y).min(info.height as i32);
        if right <= left || bottom <= top {
            screen
                .capture()
                .map_err(|error| format!("capture diagnostic monitor: {error}"))?
        } else {
            screen
                .capture_area(left, top, (right - left) as u32, (bottom - top) as u32)
                .map_err(|error| format!("capture diagnostic window crop: {error}"))?
        }
    } else {
        screenshots::Screen::all()
            .map_err(|error| format!("enumerate screenshot monitors: {error}"))?
            .into_iter()
            .next()
            .ok_or_else(|| "no screenshot monitor is available".to_string())?
            .capture()
            .map_err(|error| format!("capture diagnostic monitor: {error}"))?
    };
    image
        .save(path)
        .map_err(|error| format!("save diagnostic screenshot: {error}"))
}

#[derive(Serialize)]
struct WindowInventory {
    runner_process_id: u32,
    child_process_id: Option<u32>,
    windows: Vec<WindowRecord>,
}

#[derive(Serialize)]
struct WindowRecord {
    hwnd: u64,
    process_id: u32,
    role: &'static str,
    visible: bool,
    minimized: bool,
    bounds: [i32; 4],
}

impl From<&WindowSnapshot> for WindowRecord {
    fn from(window: &WindowSnapshot) -> Self {
        Self {
            hwnd: hwnd_id(window.hwnd),
            process_id: window.process_id,
            role: match window.role {
                WindowRole::Root => "root",
                WindowRole::Designer => "designer",
                WindowRole::OtherChild => "other_child",
            },
            visible: window.visible,
            minimized: window.minimized,
            bounds: window.bounds,
        }
    }
}

#[derive(Clone)]
struct CaseFailure {
    stage: FailureStage,
    message: String,
}

impl CaseFailure {
    fn new(stage: FailureStage, message: String) -> Self {
        Self { stage, message }
    }
}

impl std::fmt::Display for CaseFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.stage, self.message)
    }
}

fn expected(id: &str) -> &'static str {
    match id {
        "H0" => "ROOT focused F11 tap parks ROOT offscreen; exactly one short tap; no radial open",
        "H1" => "hidden ROOT is shown from known runner-owned focus",
        "H2" => "runner-owned other focus toggles visible ROOT in both directions",
        "H3" => "Designer-focused F11 toggles only ROOT; Designer remains queryable",
        "H4" => "threshold hold opens runtime radial while ROOT stays visible",
        "H5" => "release after hold does not co-fire grid/ROOT visibility",
        "H6" => "second threshold hold closes runtime radial while ROOT stays visible",
        "D0" => "production Edit Radial Menus entry opens one ready child-owned Designer",
        "D1" => {
            "validated native client click on production Tree semantic target reaches accepted widget and changes state"
        }
        "D2" => {
            "native TextEdit input changes and restores the production draft, then checked Tab moves focus to the next control"
        }
        "D4" => "production radial skins command selects the same Designer Skins semantic target",
        "D5" => "checked close closes Designer while ROOT and candidate remain alive",
        _ => "candidate exits normally through production close path",
    }
}

fn tap_trace_complete(events: &[String], taps: usize, visible: bool) -> bool {
    let hooks = events
        .iter()
        .filter(|line| {
            line.contains("trace_event=\"hook_primary\"")
                && line.contains("provenance=ExternalInjected")
                && (line.contains("transition=Press") || line.contains("transition=Release"))
        })
        .count();
    let configured = events
        .iter()
        .filter(|line| {
            line.contains("trace_event=\"configured_primary\"")
                && line.contains("provenance=ExternalInjected")
                && line.contains("modifiers_match=true")
                && (line.contains("transition=Press") || line.contains("transition=Release"))
        })
        .count();
    let short = events
        .iter()
        .filter(|line| line.contains("trace_event=\"short_tap\""))
        .count();
    let visibility = events
        .iter()
        .filter(|line| {
            line.contains("trace_event=\"desired_visibility\"")
                && line.contains(if visible {
                    "visible=true"
                } else {
                    "visible=false"
                })
        })
        .count();
    hooks >= taps * 2 && configured >= taps * 2 && short >= taps && visibility >= taps
}

fn tap_trace_failure_stage(events: &[String], visible: bool) -> FailureStage {
    let hook_press = has_trace(
        events,
        "hook_primary",
        &["transition=Press", "provenance=ExternalInjected"],
    );
    let hook_release = has_trace(
        events,
        "hook_primary",
        &["transition=Release", "provenance=ExternalInjected"],
    );
    let configured_press = has_trace(
        events,
        "configured_primary",
        &[
            "transition=Press",
            "provenance=ExternalInjected",
            "modifiers_match=true",
        ],
    );
    let configured_release = has_trace(
        events,
        "configured_primary",
        &[
            "transition=Release",
            "provenance=ExternalInjected",
            "modifiers_match=true",
        ],
    );
    if !(hook_press && hook_release && configured_press && configured_release) {
        FailureStage::HookAdmission
    } else if !has_trace(events, "short_tap", &[]) {
        FailureStage::GestureDecision
    } else if !events.iter().any(|line| {
        line.contains("trace_event=\"desired_visibility\"")
            && line.contains(if visible {
                "visible=true"
            } else {
                "visible=false"
            })
    }) {
        FailureStage::RootCommand
    } else {
        FailureStage::GestureDecision
    }
}

fn input_trace_summary(events: &[String]) -> String {
    let lines = events
        .iter()
        .filter(|line| {
            [
                "trace_event=\"hook_primary\"",
                "trace_event=\"configured_primary\"",
                "trace_event=\"short_tap\"",
                "trace_event=\"desired_visibility\"",
            ]
            .iter()
            .any(|marker| line.contains(marker))
        })
        .take(4)
        .cloned()
        .collect::<Vec<_>>();
    if lines.is_empty() {
        "none".to_string()
    } else {
        lines.join(" | ")
    }
}

fn has_trace(events: &[String], event: &str, fields: &[&str]) -> bool {
    events.iter().any(|line| {
        line.contains(&format!("trace_event=\"{event}\""))
            && fields.iter().all(|field| line.contains(field))
    })
}

#[derive(Clone, Copy, Debug)]
struct DesignerSemanticTargetState {
    bounds: [i32; 4],
    selected: bool,
    focused: bool,
}

fn parse_designer_semantic_target(
    line: &str,
    target: DesignerSemanticTarget,
) -> Option<DesignerSemanticTargetState> {
    let name = format!("target={target:?}");
    let role = match target {
        DesignerSemanticTarget::Menus
        | DesignerSemanticTarget::Skins
        | DesignerSemanticTarget::Tree
        | DesignerSemanticTarget::Inspector => "SelectableLabel",
        DesignerSemanticTarget::DefaultMenu => "Button",
        DesignerSemanticTarget::MenuName => "TextEdit",
        DesignerSemanticTarget::MenuDefaultSkin => "ComboBox",
    };
    if !line.contains("trace_event=\"designer_semantic_target\"")
        || !line.contains(&name)
        || !line.contains(&format!("role=\"{role}\""))
        || !line.contains("viewport=Deferred")
    {
        return None;
    }
    let field = |name: &str| {
        line.split_whitespace()
            .find_map(|part| part.strip_prefix(&format!("{name}=")))
    };
    Some(DesignerSemanticTargetState {
        bounds: [
            field("left_px")?.parse().ok()?,
            field("top_px")?.parse().ok()?,
            field("right_px")?.parse().ok()?,
            field("bottom_px")?.parse().ok()?,
        ],
        selected: field("selected")?.parse().ok()?,
        focused: field("focused")?.parse().ok()?,
    })
}

fn wait_for_designer_semantic_target(
    trace_path: &Path,
    target: DesignerSemanticTarget,
    timeout: Duration,
    predicate: impl Fn(&DesignerSemanticTargetState) -> bool,
) -> Option<DesignerSemanticTargetState> {
    let deadline = Instant::now() + timeout;
    loop {
        let state = trace_lines(trace_path)
            .into_iter()
            .rev()
            .find_map(|line| parse_designer_semantic_target(&line, target));
        if state.as_ref().is_some_and(&predicate) {
            return state;
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

fn trace_lines(path: &Path) -> Vec<String> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|line| line.contains("trace_event=\""))
        .map(str::to_owned)
        .collect()
}

fn hook_service_thread_id(path: &Path) -> Option<u32> {
    trace_lines(path).into_iter().find_map(|line| {
        let event = line.split("trace_event=\"").nth(1)?.split('\"').next()?;
        if event != "hook_service_ready" {
            return None;
        }
        line.split("thread_id=")
            .nth(1)?
            .split_ascii_whitespace()
            .next()?
            .parse()
            .ok()
    })
}

fn wait_trace<F>(path: &Path, cursor: usize, timeout: Duration, mut predicate: F) -> Vec<String>
where
    F: FnMut(&[String]) -> bool,
{
    let deadline = Instant::now() + timeout;
    loop {
        let events = trace_lines(path)
            .into_iter()
            .skip(cursor)
            .collect::<Vec<_>>();
        if predicate(&events) || Instant::now() >= deadline {
            return events;
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

fn wait_for_latest_root_restore(path: &Path, timeout: Duration) -> Option<String> {
    let mut request_id = None;
    let completed = wait_until(timeout, || {
        let events = trace_lines(path);
        let Some(request) = events.iter().rev().find(|line| {
            line.contains("trace_event=\"native_activation\"")
                && line.contains("edge=RestoreRequested")
        }) else {
            return false;
        };
        let Some(id) = request
            .split("request_id=")
            .nth(1)
            .and_then(|value| value.split_ascii_whitespace().next())
        else {
            return false;
        };
        request_id = Some(id.to_owned());
        events.iter().any(|line| {
            line.contains("trace_event=\"native_activation\"")
                && line.contains("edge=RestoreCompleted")
                && line.contains("terminal=true")
                && line.contains(&format!("request_id={id}"))
        })
    });
    if completed { request_id } else { None }
}

fn wait_root_visibility(child: &mut NativeChild, visible: bool, timeout: Duration) -> bool {
    wait_until(timeout, || {
        child.refresh_root().is_ok_and(|root| {
            if visible {
                root.visible && !root.minimized && root.intersects_virtual_screen()
            } else {
                !root.intersects_virtual_screen()
            }
        })
    })
}

fn require_visible(root: &WindowSnapshot) -> Result<(), String> {
    if root.visible && !root.minimized && root.intersects_virtual_screen() {
        Ok(())
    } else {
        Err(format!(
            "ROOT is not on-screen and drawable: visible={} minimized={} bounds={:?}",
            root.visible, root.minimized, root.bounds
        ))
    }
}

fn require_hidden(root: &WindowSnapshot) -> Result<(), String> {
    if !root.intersects_virtual_screen() {
        Ok(())
    } else {
        Err(format!(
            "ROOT remains in the virtual screen bounds after hide: visible={} minimized={} bounds={:?}",
            root.visible, root.minimized, root.bounds
        ))
    }
}

fn runtime_windows(child: &NativeChild) -> Vec<WindowSnapshot> {
    child
        .windows()
        .into_iter()
        .filter(|window| {
            is_radial_surface(window, child.process_id())
                && window.visible
                && !window.minimized
                && window.intersects_virtual_screen()
        })
        .collect()
}

fn wait_runtime_windows(
    child: &NativeChild,
    before: &[WindowSnapshot],
    timeout: Duration,
) -> Result<Vec<WindowSnapshot>, String> {
    let deadline = Instant::now() + timeout;
    let mut previous_ids: Option<Vec<u64>> = None;
    loop {
        let found: Vec<WindowSnapshot> = runtime_windows(child)
            .into_iter()
            .filter(|window| {
                !before.iter().any(|previous| {
                    previous.process_id == window.process_id
                        && hwnd_id(previous.hwnd) == hwnd_id(window.hwnd)
                })
            })
            .collect();
        let mut ids = found
            .iter()
            .map(|window| hwnd_id(window.hwnd))
            .collect::<Vec<_>>();
        ids.sort_unstable();
        if ids.len() >= 2 && previous_ids.as_ref() == Some(&ids) {
            return Ok(found);
        }
        previous_ids = (ids.len() >= 2).then_some(ids);
        if Instant::now() >= deadline {
            return Err(format!(
                "hold did not produce a stable set of at least two new visible child-owned radial HWNDs; observed [{}]",
                describe_radial_surfaces(&found)
            ));
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

fn validate_radial_surfaces(
    child: &NativeChild,
    surfaces: &[WindowSnapshot],
) -> Result<(), String> {
    if surfaces.len() < 2 {
        return Err(format!(
            "radial surface set must contain input and visual HWNDs, got [{}]",
            describe_radial_surfaces(surfaces)
        ));
    }
    for (index, surface) in surfaces.iter().enumerate() {
        if !is_radial_surface(surface, child.process_id()) {
            return Err(format!(
                "radial HWND={} is not a {} surface owned by exact candidate PID {}; observed PID={} role={:?} class={:?}",
                hwnd_id(surface.hwnd),
                RADIAL_HOST_WINDOW_CLASS,
                child.process_id(),
                surface.process_id,
                surface.role,
                surface.class_name
            ));
        }
        if hwnd_id(surface.hwnd) == 0
            || surfaces[..index]
                .iter()
                .any(|previous| hwnd_id(previous.hwnd) == hwnd_id(surface.hwnd))
        {
            return Err("radial surface set contains a null or duplicate HWND".into());
        }
    }
    Ok(())
}

fn radial_surfaces_are_active(child: &NativeChild, surfaces: &[WindowSnapshot]) -> bool {
    let current = child.windows();
    radial_surface_set_matches_active_state(surfaces, &current, child.process_id(), true)
}

fn radial_surfaces_are_inactive(child: &NativeChild, surfaces: &[WindowSnapshot]) -> bool {
    let current = child.windows();
    radial_surface_set_and_owner_are_inactive(surfaces, &current, child.process_id())
}

fn radial_surface_set_and_owner_are_inactive(
    surfaces: &[WindowSnapshot],
    current: &[WindowSnapshot],
    process_id: u32,
) -> bool {
    radial_surface_set_matches_active_state(surfaces, current, process_id, false)
        && active_radial_surfaces(current, process_id).is_empty()
}

fn active_radial_surfaces(windows: &[WindowSnapshot], process_id: u32) -> Vec<WindowSnapshot> {
    windows
        .iter()
        .filter(|window| {
            is_radial_surface(window, process_id)
                && window.visible
                && !window.minimized
                && window.intersects_virtual_screen()
        })
        .cloned()
        .collect()
}

fn radial_surface_set_matches_active_state(
    surfaces: &[WindowSnapshot],
    current: &[WindowSnapshot],
    process_id: u32,
    expected_active: bool,
) -> bool {
    if surfaces.len() < 2
        || surfaces
            .iter()
            .any(|surface| !is_radial_surface(surface, process_id))
    {
        return false;
    }
    surfaces.iter().all(|surface| {
        let active = current.iter().any(|window| {
            is_radial_surface(window, process_id)
                && hwnd_id(window.hwnd) == hwnd_id(surface.hwnd)
                && window.visible
                && !window.minimized
                && window.intersects_virtual_screen()
        });
        active == expected_active
    })
}

fn is_radial_surface(window: &WindowSnapshot, process_id: u32) -> bool {
    window.process_id == process_id
        && window.role == WindowRole::OtherChild
        && window.class_name == RADIAL_HOST_WINDOW_CLASS
}

fn describe_radial_surfaces(surfaces: &[WindowSnapshot]) -> String {
    if surfaces.is_empty() {
        return "none".into();
    }
    surfaces
        .iter()
        .map(|surface| {
            format!(
                "HWND:{} PID:{} role={:?} class={:?} visible={} minimized={} bounds={:?}",
                hwnd_id(surface.hwnd),
                surface.process_id,
                surface.role,
                surface.class_name,
                surface.visible,
                surface.minimized,
                surface.bounds
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn same_window_state(before: &WindowSnapshot, after: &WindowSnapshot) -> bool {
    hwnd_id(before.hwnd) == hwnd_id(after.hwnd)
        && before.process_id == after.process_id
        && before.role == after.role
        && before.class_name == after.class_name
        && before.visible == after.visible
        && before.minimized == after.minimized
        && before.bounds == after.bounds
}

fn describe_root_snapshot(root: &WindowSnapshot) -> String {
    format!(
        "HWND:{} PID:{} role={:?} class={:?} visible={} minimized={} bounds={:?}",
        hwnd_id(root.hwnd),
        root.process_id,
        root.role,
        root.class_name,
        root.visible,
        root.minimized,
        root.bounds
    )
}

fn wait_child(child: &mut NativeChild, timeout: Duration) -> Option<NativeExitStatus> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            return Some(status);
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis().min(u64::MAX as u128) as u64
}

fn started_now() -> Instant {
    Instant::now()
}

fn bounded_text(text: &str, maximum: usize) -> String {
    if text.len() <= maximum {
        return text.to_owned();
    }
    let mut end = maximum;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(hwnd: usize, process_id: u32, role: WindowRole, active: bool) -> WindowSnapshot {
        let class_name = match role {
            WindowRole::OtherChild => RADIAL_HOST_WINDOW_CLASS,
            WindowRole::Root => "MultiLauncherRoot",
            WindowRole::Designer => "RadialDesigner",
        };
        window_with_class(hwnd, process_id, role, class_name, active)
    }

    fn window_with_class(
        hwnd: usize,
        process_id: u32,
        role: WindowRole,
        class_name: &str,
        active: bool,
    ) -> WindowSnapshot {
        let (left, top, width, height) = super::super::virtual_screen_bounds();
        WindowSnapshot {
            hwnd: windows::Win32::Foundation::HWND(hwnd as *mut std::ffi::c_void),
            process_id,
            role,
            class_name: class_name.into(),
            visible: active,
            minimized: false,
            bounds: [
                left,
                top,
                left.saturating_add(width),
                top.saturating_add(height),
            ],
        }
    }

    #[test]
    fn radial_surface_set_requires_all_exact_child_surfaces_to_transition() {
        let radial = [
            window(101, 44, WindowRole::OtherChild, true),
            window(102, 44, WindowRole::OtherChild, true),
        ];
        let both_active = radial.to_vec();
        assert!(radial_surface_set_matches_active_state(
            &radial,
            &both_active,
            44,
            true
        ));
        let with_auxiliary = vec![
            radial[0].clone(),
            radial[1].clone(),
            window_with_class(103, 44, WindowRole::OtherChild, "ApplicationDialog", true),
        ];
        assert!(!is_radial_surface(&with_auxiliary[2], 44));
        assert!(radial_surface_set_matches_active_state(
            &radial,
            &with_auxiliary,
            44,
            true
        ));
        assert!(!radial_surface_set_matches_active_state(
            &radial,
            &both_active,
            44,
            false
        ));

        let one_active = vec![
            radial[0].clone(),
            window(102, 44, WindowRole::OtherChild, false),
        ];
        assert!(!radial_surface_set_matches_active_state(
            &radial,
            &one_active,
            44,
            true
        ));
        assert!(!radial_surface_set_matches_active_state(
            &radial,
            &one_active,
            44,
            false
        ));

        let both_closed = vec![window(102, 44, WindowRole::OtherChild, false)];
        assert!(radial_surface_set_matches_active_state(
            &radial,
            &both_closed,
            44,
            false
        ));
        assert!(radial_surface_set_and_owner_are_inactive(
            &radial,
            &both_closed,
            44
        ));
        let untracked_radial_open = vec![
            window(102, 44, WindowRole::OtherChild, false),
            window(103, 44, WindowRole::OtherChild, true),
        ];
        assert!(!radial_surface_set_and_owner_are_inactive(
            &radial,
            &untracked_radial_open,
            44
        ));

        let reused_by_other_process = vec![window(101, 55, WindowRole::OtherChild, true)];
        assert!(radial_surface_set_matches_active_state(
            &radial,
            &reused_by_other_process,
            44,
            false
        ));
    }

    #[test]
    fn radial_surface_state_keeps_root_identity_and_bounds_separate() {
        let radial = [
            window(101, 44, WindowRole::OtherChild, true),
            window(102, 44, WindowRole::OtherChild, true),
        ];
        let with_root = vec![
            radial[0].clone(),
            radial[1].clone(),
            window(100, 44, WindowRole::Root, true),
        ];
        assert!(radial_surface_set_matches_active_state(
            &radial, &with_root, 44, true
        ));
        assert!(!radial_surface_set_matches_active_state(
            &[radial[0].clone(), window(100, 44, WindowRole::Root, true)],
            &with_root,
            44,
            true
        ));

        let root_before = window(100, 44, WindowRole::Root, true);
        let mut root_after = root_before.clone();
        assert!(same_window_state(&root_before, &root_after));
        root_after.bounds[0] = root_after.bounds[0].saturating_add(1);
        assert!(!same_window_state(&root_before, &root_after));
    }
}
