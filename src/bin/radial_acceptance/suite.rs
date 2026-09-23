use super::super::{
    AcceptanceCaseResult, AcceptanceReport, CaseStatus, FailureStage, MAX_PATH_BYTES,
    MAX_RESULT_BYTES,
};
use super::*;
use serde::Serialize;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, UNIX_EPOCH};

const TAP_TIME: Duration = Duration::from_millis(135);
const ROOT_TIMEOUT: Duration = Duration::from_secs(3);
const UIA_TIMEOUT: Duration = Duration::from_secs(5);
const TRACE_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_TRACE_EXCERPT: usize = 512;
const DESIGNER_TEXT_PROBE: &str = "Native Edit Probe";
const DESIGNER_STARTER_NAME: &str = "Starter";
const CASE_IDS: [&str; 13] = [
    "H0", "H1", "H2", "H3", "H4", "H5", "H6", "D0", "D1", "D2", "D4", "D5", "CLEANUP",
];

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
    let mut hold_window: Option<WindowSnapshot> = None;

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
        &mut hold_window,
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
    run_hold_release_case(
        report,
        &child,
        &anchor,
        trace_path,
        output,
        hold_window.as_ref(),
        hold_guard,
        hold_observer,
        hold_observer_error,
    );
    run_second_hold_case(
        report,
        &child,
        &anchor,
        trace_path,
        output,
        hold_threshold_ms,
        hold_window.as_ref(),
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
        anchor
            .focus()
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        let mut cursor = trace_lines(trace_path).len();
        for visible in [false, true] {
            anchor
                .focus()
                .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
            child
                .send_f11(anchor.hwnd(), anchor.process_id(), TAP_TIME)
                .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
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
            "runner-owned anchor HWND={} toggled ROOT offscreen and back on-screen",
            hwnd_id(anchor.hwnd())
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
    held_window: &mut Option<WindowSnapshot>,
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
        let after = wait_runtime_window(child, &before, Duration::from_secs(2));
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
        let window = after.into_iter().next().ok_or_else(|| {
            CaseFailure::new(
                FailureStage::NativeRootState,
                "hold produced no new visible child-owned radial HWND".into(),
            )
        })?;
        let current_root = child
            .refresh_root()
            .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
        require_visible(&current_root).map_err(|error| {
            CaseFailure::new(
                FailureStage::NativeRootState,
                format!("ROOT changed while opening radial: {error}"),
            )
        })?;
        *held_window = Some(window.clone());
        Ok(format!(
            "held F11 opened visible child HWND={} bounds={:?}; ROOT stayed on-screen; down edge=[{}]",
            hwnd_id(window.hwnd),
            window.bounds,
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
    held_window: Option<&WindowSnapshot>,
    hold_guard: Option<F11HoldGuard<'_>>,
    mut hook_observer: Option<RunnerHookObserver>,
    hook_observer_error: Option<String>,
) {
    let started = Instant::now();
    let result = (|| {
        let mut hold_guard = hold_guard.ok_or_else(|| {
            CaseFailure::new(
                FailureStage::InputInjection,
                "H4 did not leave F11 held for a checked release".into(),
            )
        })?;
        let cursor = trace_lines(trace_path).len();
        let release = hold_guard
            .release()
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        let runner_observation = hook_observer
            .as_mut()
            .map(|observer| observer.wait_for_vk(0x7A, Duration::from_secs(1)));
        let runner_observation_text = runner_observation
            .as_ref()
            .map(|observation| observation.describe())
            .or_else(|| hook_observer_error.map(|error| format!("observer unavailable: {error}")))
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
        if has_trace(&lines, "short_tap", &[]) || has_trace(&lines, "desired_visibility", &[]) {
            return Err(CaseFailure::new(
                FailureStage::GestureDecision,
                format!(
                    "release after threshold co-fired the short-tap path; release edge [{}]",
                    release.describe()
                ),
            ));
        }
        let held_window = held_window.ok_or_else(|| {
            CaseFailure::new(
                FailureStage::NativeRootState,
                "H4 did not identify a radial HWND to check after release".into(),
            )
        })?;
        if !window_still_active(child, held_window) {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                format!(
                    "radial HWND did not remain active after releasing held F11; release edge [{}]",
                    release.describe()
                ),
            ));
        }
        Ok(format!(
            "checked F11 release generated no tap or ROOT visibility edge; radial HWND={} remains visible; release edge=[{}]; {runner_observation_text}",
            hwnd_id(held_window.hwnd),
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
}

fn run_second_hold_case(
    report: &mut AcceptanceReport,
    child: &NativeChild,
    anchor: &FocusAnchor,
    trace_path: &Path,
    output: &Path,
    hold_threshold_ms: u64,
    held_window: Option<&WindowSnapshot>,
) {
    let started = Instant::now();
    let result = (|| {
        let held_window = held_window.ok_or_else(|| {
            CaseFailure::new(
                FailureStage::NativeRootState,
                "H4 did not identify a radial HWND to toggle closed".into(),
            )
        })?;
        let root = child
            .refresh_root()
            .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
        child
            .focus_window(&root)
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        let cursor = trace_lines(trace_path).len();
        let (mut hook_observer, hook_observer_error) = match RunnerHookObserver::start() {
            Ok(observer) => (Some(observer), None),
            Err(error) => (None, Some(error)),
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
        let mut hold_guard = F11HoldGuard::new(child, anchor);
        std::thread::sleep(Duration::from_millis(
            hold_threshold_ms.saturating_add(250).min(5_000),
        ));
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
        let closed = wait_until(Duration::from_secs(2), || {
            !window_still_active(child, held_window)
        });
        if !closed {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                format!(
                    "second full hold did not remove, hide, or park the radial HWND; down=[{}] up=[{}]; {runner_observation_text}",
                    down.describe(),
                    up.describe()
                ),
            ));
        }
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
        if !release_traced || !release_observed || !sentinel_observed || !sentinel_traced {
            return Err(CaseFailure::new(
                FailureStage::HookAdmission,
                format!(
                    "second hold release was not proven by both production hook and independent observer; production_release={release_traced}; after_hold_observer_sentinel={sentinel_observed}; after_hold_production_sentinel={sentinel_traced}; {runner_observation_text}; down=[{}] up=[{}]",
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
        Ok(format!(
            "second threshold hold closed radial HWND={} while ROOT stayed visible; down=[{}] up=[{}]; {runner_observation_text}",
            hwnd_id(held_window.hwnd),
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
    let file_click = activate_named(uia, child, &root, "File", FailureStage::DesignerEntry)?;
    let apps_click = activate_named(uia, child, &root, "Apps", FailureStage::DesignerEntry)?;
    let edit_control = uia
        .wait_named(
            root.hwnd,
            child.process_id(),
            "Edit Radial Menus",
            UIA_TIMEOUT,
        )
        .map_err(|error| CaseFailure::new(FailureStage::DesignerEntry, error))?;
    let edit_click = click_semantic_control(child, &root, &edit_control)
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
        let click = click_semantic_control(child, &root, &result_control)
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
    click_semantic_control(child, target, &control)
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
            window.role == WindowRole::OtherChild
                && window.visible
                && !window.minimized
                && window.intersects_virtual_screen()
        })
        .collect()
}

fn wait_runtime_window(
    child: &NativeChild,
    before: &[WindowSnapshot],
    timeout: Duration,
) -> Vec<WindowSnapshot> {
    let mut found = Vec::new();
    let _ = wait_until(timeout, || {
        found = runtime_windows(child)
            .into_iter()
            .filter(|window| !before.iter().any(|previous| previous.hwnd == window.hwnd))
            .collect();
        !found.is_empty()
    });
    found
}

fn window_still_active(child: &NativeChild, previous: &WindowSnapshot) -> bool {
    child.windows().into_iter().any(|window| {
        window.hwnd == previous.hwnd
            && window.visible
            && !window.minimized
            && window.intersects_virtual_screen()
    })
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
