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
const CASE_IDS: [&str; 13] = [
    "H0", "H1", "H2", "H3", "H4", "H5", "H6", "D0", "D1", "D2", "D4", "D5", "CLEANUP",
];

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
    let (hold_open, hold_guard) = run_hold_open_case(
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
            if !tap_trace_complete(&events, 1, wanted_visible) {
                let observed_root = child.refresh_root().ok();
                let stage = tap_trace_failure_stage(&events, wanted_visible);
                return Err(CaseFailure::new(
                    stage,
                    format!(
                        "F11 tap {} input edges [{}] lacked complete hook/gesture/visibility evidence for visible={wanted_visible}; root={:?}; observed production edges: {}",
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
            "{} checked F11 input pairs reached expected ROOT state; HWND={} bounds={:?}; edges={:?}",
            taps,
            hwnd_id(root.hwnd),
            root.bounds,
            input_evidence
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
) -> (Result<String, CaseFailure>, Option<F11HoldGuard<'a>>) {
    let mut hold_guard = None;
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
            return Err(CaseFailure::new(
                FailureStage::HookAdmission,
                format!(
                    "held F11 down edge [{}] targeted ROOT HWND={} PID={}; foreground after hold HWND={} PID={}; observed {} production trace event(s), but no matching hook/configured press edge",
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
        (result, None)
    } else {
        (result, hold_guard)
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
                    "held F11 release edge [{}] did not reach the production hook and configured chord",
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
            "checked F11 release generated no tap or ROOT visibility edge; radial HWND={} remains visible; release edge=[{}]",
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
        let down = child
            .press_f11(root.hwnd, child.process_id())
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        let mut hold_guard = F11HoldGuard::new(child, anchor);
        std::thread::sleep(Duration::from_millis(
            hold_threshold_ms.saturating_add(250).min(5_000),
        ));
        let up = hold_guard
            .release()
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        let closed = wait_until(Duration::from_secs(2), || {
            !window_still_active(child, held_window)
        });
        if !closed {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                format!(
                    "second full hold did not remove, hide, or park the radial HWND; down=[{}] up=[{}]",
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
            "second threshold hold closed radial HWND={} while ROOT stayed visible; down=[{}] up=[{}]",
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
            child
                .send_f11(designer.hwnd, child.process_id(), TAP_TIME)
                .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
            if !wait_root_visibility(child, visible, ROOT_TIMEOUT) {
                return Err(CaseFailure::new(
                    FailureStage::NativeRootState,
                    format!("Designer-focused F11 failed to toggle ROOT to visible={visible}"),
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
        let control = uia
            .wait_named(designer.hwnd, child.process_id(), "Tree", UIA_TIMEOUT)
            .map_err(|error| CaseFailure::new(FailureStage::DesignerNativeTarget, error))?;
        let before = uia
            .selection_state(&control)
            .map(Some)
            .or_else(|| uia.toggle_state(&control).map(|state| Some(state.0 == 1)))
            .flatten();
        let cursor = trace_lines(trace_path).len();
        let click_evidence = click_semantic_control(child, designer, &control)
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        let events = wait_trace(trace_path, cursor, TRACE_TIMEOUT, |events| {
            has_trace(events, "designer_pointer", &["pointer_down=true"])
                && has_trace(events, "designer_pointer", &["pointer_up=true"])
                && has_trace(events, "designer_body", &["state=Enabled"])
                && has_trace(
                    events,
                    "designer_widget",
                    &["category=Tree", "response=Accepted"],
                )
        });
        if !has_trace(&events, "designer_pointer", &["pointer_down=true"])
            || !has_trace(&events, "designer_pointer", &["pointer_up=true"])
            || !has_trace(&events, "designer_body", &["state=Enabled"])
            || !has_trace(
                &events,
                "designer_widget",
                &["category=Tree", "response=Accepted"],
            )
        {
            return Err(CaseFailure::new(FailureStage::DesignerFrameworkInput, "native click did not reach the production Designer pointer/body/Tree accepted boundaries".into()));
        }
        let before = before.ok_or_else(|| CaseFailure::new(
            FailureStage::DesignerMutation,
            "native Tree click reached the production widget, but UIA did not expose a pre-click selection/toggle state".into(),
        ))?;
        let changed = wait_until(UIA_TIMEOUT, || {
            uia.find_named(designer.hwnd, child.process_id(), "Tree")
                .ok()
                .flatten()
                .and_then(|current| {
                    uia.selection_state(&current)
                        .or_else(|| uia.toggle_state(&current).map(|state| state.0 == 1))
                })
                .is_some_and(|current| current != before)
        });
        if !changed {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "Tree click was accepted but UIA did not expose a changed selection/toggle state"
                    .into(),
            ));
        }
        Ok(format!(
            "Tree native pointer click reached Enabled body and accepted widget; accessible state changed {before} -> {}; click edges=[{}]",
            !before,
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
        let start = uia
            .wait_named(designer.hwnd, child.process_id(), "Tree", UIA_TIMEOUT)
            .map_err(|error| CaseFailure::new(FailureStage::DesignerNativeTarget, error))?;
        uia.focus(&start)
            .map_err(|error| CaseFailure::new(FailureStage::DesignerFrameworkInput, error))?;
        child
            .focus_window(designer)
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        let before = uia
            .focused_element()
            .map_err(|error| CaseFailure::new(FailureStage::DesignerFrameworkInput, error))?;
        if uia.element_process_id(&before) != Some(child.process_id()) {
            return Err(CaseFailure::new(
                FailureStage::DesignerFrameworkInput,
                "UIA keyboard focus before Tab does not belong to candidate".into(),
            ));
        }
        let count = send_tab(child, designer)
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        let after_changed = wait_until(UIA_TIMEOUT, || {
            uia.focused_element().ok().is_some_and(|after| {
                uia.element_process_id(&after) == Some(child.process_id())
                    && !uia.same_element(&before, &after).unwrap_or(true)
            })
        });
        if !after_changed {
            return Err(CaseFailure::new(
                FailureStage::DesignerFrameworkInput,
                "checked Tab input did not move UIA focus to another child-owned Designer element"
                    .into(),
            ));
        }
        let after = uia
            .focused_element()
            .map_err(|error| CaseFailure::new(FailureStage::DesignerFrameworkInput, error))?;
        Ok(format!(
            "{} checked Tab events moved UIA focus from '{}' to '{}'",
            count,
            uia.element_name(&before).unwrap_or_default(),
            uia.element_name(&after).unwrap_or_default()
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
        let result_control = uia
            .wait_named(
                root.hwnd,
                child.process_id(),
                "Edit radial skins",
                UIA_TIMEOUT,
            )
            .map_err(|error| CaseFailure::new(FailureStage::DesignerEntry, error))?;
        let click_evidence = click_semantic_control(child, &root, &result_control)
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
        let heading = uia
            .find_named(
                designer.hwnd,
                child.process_id(),
                "Skins, assets, import and export",
            )
            .map_err(|error| CaseFailure::new(FailureStage::DesignerPresentation, error))?;
        if heading.is_none() {
            let skins = uia.find_named(designer.hwnd, child.process_id(), "Skins")
                .map_err(|error| CaseFailure::new(FailureStage::DesignerPresentation, error))?
                .ok_or_else(|| CaseFailure::new(FailureStage::DesignerPresentation, "Designer did not expose Skins/resources mode after production radial skins command".into()))?;
            if uia.selection_state(&skins) != Some(true)
                && uia.toggle_state(&skins).is_none_or(|state| state.0 != 1)
            {
                return Err(CaseFailure::new(
                    FailureStage::DesignerPresentation,
                    "Skins mode is not selected and the resources heading is not exposed".into(),
                ));
            }
        }
        if !uia.root_is_queryable(designer.hwnd, child.process_id()) {
            return Err(CaseFailure::new(
                FailureStage::DesignerReadiness,
                "same Designer HWND stopped responding after Skins entry".into(),
            ));
        }
        let interactive = uia
            .wait_named(designer.hwnd, child.process_id(), "New skin", UIA_TIMEOUT)
            .map_err(|error| CaseFailure::new(FailureStage::DesignerReadiness, error))?;
        if !interactive.enabled {
            return Err(CaseFailure::new(
                FailureStage::DesignerReadiness,
                "Skins mode was visible but New skin was disabled".into(),
            ));
        }
        Ok(format!(
            "typed radial skins using {typed} checked Unicode events and activated the command with [{}]; same Designer HWND={} remained queryable in resources mode",
            click_evidence.describe(),
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
        let events = send_alt_f4(child, designer)
            .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
        let closed = wait_until(Duration::from_secs(4), || {
            find_child_window(child, WindowRole::Designer).is_none()
        });
        if !closed {
            return Err(CaseFailure::new(
                FailureStage::Cleanup,
                "Designer HWND remained after checked child-focused Alt+F4".into(),
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
            "{} checked Alt+F4 events closed Designer HWND={} while child process and ROOT remained alive",
            events,
            hwnd_id(designer.hwnd)
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
            "validated native client click on Tree reaches accepted widget and changes UIA state"
        }
        "D2" => "checked Tab moves UIA keyboard focus to another child-owned Designer element",
        "D4" => "production radial skins command switches the same Designer to resources mode",
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

fn trace_lines(path: &Path) -> Vec<String> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|line| line.contains("trace_event=\""))
        .map(str::to_owned)
        .collect()
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
