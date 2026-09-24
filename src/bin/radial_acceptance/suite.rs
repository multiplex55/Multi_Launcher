use super::super::{
    ACCEPTANCE_TARGET_ACTION_INDEX, AcceptanceCaseResult, AcceptanceReport, CASE_IDS, CaseStatus,
    FailureStage, H6RepeatMode, MAX_PATH_BYTES, MAX_RESULT_BYTES,
};
use super::*;
use serde::Serialize;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const TAP_TIME: Duration = Duration::from_millis(135);
const ROOT_TIMEOUT: Duration = Duration::from_secs(3);
const UIA_TIMEOUT: Duration = Duration::from_secs(5);
const TRACE_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_TRACE_EXCERPT: usize = 512;
const MAX_TRACE_BYTES: usize = 128 * 1024;
const STARTUP_TRACE_EVENTS: usize = 32;
const STARTUP_TRACE_BYTES: usize = 16 * 1024;
const MAX_PRIVATE_LOG_BYTES: usize = 64 * 1024;
const MAX_KEYBOARD_FOCUS_STEPS: usize = 32;
const POST_G2_ROOT_STABILITY_WINDOW: Duration = Duration::from_millis(750);
const POST_G2_ROOT_STABLE_SAMPLES: u8 = 3;
const DESIGNER_TEXT_PROBE: &str = "Native Edit Probe";
const DESIGNER_STARTER_NAME: &str = "Starter";
static NEXT_HOOK_PUMP_PROBE_ID: AtomicU64 = AtomicU64::new(1);
const DEFERRED_REPORT_CASE_IDS: [&str; 3] = ["R0", "R1", "R2"];
const BLOCKED_DESIGNER_CASE_IDS: [&str; 20] = [
    "H3", "D1", "D2", "D4", "D5", "A0", "A1", "G0", "A2", "G1", "G2", "A3", "A4", "A5", "A6", "A7",
    "A8", "D3", "D6", "D7",
];

fn missing_case_ids(existing_ids: &[&str]) -> Vec<&'static str> {
    CASE_IDS
        .iter()
        .copied()
        .filter(|id| !existing_ids.contains(id))
        .collect()
}

struct HoldReleaseHandoff {
    release_at_unix_ms: Option<u128>,
    sentinel_at_unix_ms: Option<u128>,
    quiescent_acknowledged: bool,
    observer: Option<RunnerHookObserver>,
}

struct DesignerEntry {
    window: WindowSnapshot,
    session_id: u64,
    root_recovery: Option<String>,
    root_menu_resolution: Option<String>,
}

#[derive(Clone, Debug)]
struct PersistedMenuGraphExpectation {
    menu_index: usize,
    ring_slots: Vec<usize>,
    populated_cells: usize,
    cell_ids_digest: u64,
}

#[derive(Clone, Debug)]
struct ActionSideEffectBaseline {
    history: Option<Vec<u8>>,
    trace_event_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AuthoringRequestIdentity {
    request_id: u64,
    generation: u64,
    session_id: u64,
}

#[derive(Clone, Debug)]
struct AuthoringReplyEvidence {
    identity: AuthoringRequestIdentity,
}

struct AcceptancePrepareHold {
    path: PathBuf,
    released: bool,
}

impl AcceptancePrepareHold {
    fn create(profile: &Path) -> Result<Self, String> {
        let path = profile.join(super::PREPARE_HOLD_FILE_NAME);
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| format!("create D7 preview preparation hold marker: {error}"))?;
        Ok(Self {
            path,
            released: false,
        })
    }

    fn release(&mut self) -> Result<String, String> {
        fs::remove_file(&self.path)
            .map_err(|error| format!("release D7 preview preparation hold marker: {error}"))?;
        self.released = true;
        if self.path.exists() {
            return Err("D7 preview preparation hold marker remained after release".into());
        }
        Ok("removed the bounded temp-profile hold marker".into())
    }
}

impl Drop for AcceptancePrepareHold {
    fn drop(&mut self) {
        if !self.released {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[derive(Serialize)]
struct PostG2RootSnapshot {
    captured_unix_ms: u128,
    child_process_id: u32,
    root_hwnd: u64,
    visible: bool,
    minimized: bool,
    bounds: [i32; 4],
    physical_displays: Vec<[i32; 4]>,
    intersects_physical_display: bool,
    sample_count: u8,
    drawable_on_display_samples: u8,
    consecutive_stable_samples: u8,
    stable_on_display: bool,
    root_trace_tail: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct RootMenuState {
    file_open: bool,
    apps_open: bool,
}

impl RootMenuState {
    fn any_open(self) -> bool {
        self.file_open || self.apps_open
    }
}

fn root_file_menu_ready_for_apps(state: RootMenuState) -> bool {
    state.file_open && !state.apps_open
}

fn should_retry_root_file_menu_open(state: RootMenuState) -> bool {
    !root_file_menu_ready_for_apps(state) && !state.any_open()
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
    cursor_restore: Option<POINT>,
    _desktop: &InputDesktopAttachment,
    report: &mut AcceptanceReport,
    runner_log: &mut File,
) -> Option<FocusAnchor> {
    if let Err(error) = preflight_acceptance_hotkey() {
        record_environment_failure(
            format!("acceptance hotkey preflight failed: {error}"),
            report,
            output,
            trace_path,
            runner_log,
        );
        return None;
    }
    let _ = writeln!(
        runner_log,
        "acceptance hotkey F11 registered and unregistered successfully before child launch"
    );

    let stdout_path = profile.join("child.stdout.log");
    let stderr_path = profile.join("child.stderr.log");
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
            for (index, id) in CASE_IDS
                .into_iter()
                .filter(|id| !DEFERRED_REPORT_CASE_IDS.contains(id))
                .enumerate()
            {
                if index == 0 {
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
                } else {
                    append_case_without_artifacts(
                        report,
                        id,
                        Err(CaseFailure::new(
                            failure.stage,
                            "not run because candidate startup failed; see the first case result"
                                .into(),
                        )),
                    );
                }
            }
            let _ = writeln!(
                runner_log,
                "candidate startup failed pid={:?} windows={} : {failure}",
                error.process_id,
                error.windows.len()
            );
            return None;
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
            for (index, id) in CASE_IDS
                .into_iter()
                .filter(|id| !DEFERRED_REPORT_CASE_IDS.contains(id))
                .enumerate()
            {
                if index == 0 {
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
                } else {
                    append_case_without_artifacts(
                        report,
                        id,
                        Err(CaseFailure::new(
                            failure.stage,
                            "not run because runner setup failed; see the first case result".into(),
                        )),
                    );
                }
            }
            restore_cursor_before_shutdown(cursor_restore, runner_log);
            stop_child(&mut child, report, runner_log, output, trace_path);
            return None;
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
        match run_designer_entry(&mut child, automation, &anchor, trace_path) {
            Ok(entry) => {
                designer_window = Some(entry.window);
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

    run_failure_artifact_case(report, &child, output, trace_path);

    if let (Some(automation), Some(designer)) = (uia.as_ref(), designer_window.as_ref()) {
        run_designer_focus_case(report, &mut child, automation, designer, trace_path, output);
        run_designer_pointer_case(report, &mut child, automation, designer, trace_path, output);
        run_tab_case(report, &mut child, automation, designer, output, trace_path);
        run_skins_command_case(report, &mut child, automation, designer, trace_path, output);
        run_designer_close_case(report, &mut child, designer, output, trace_path);
        match run_designer_entry(&mut child, automation, &anchor, trace_path) {
            Ok(entry) => run_authoring_geometry_cases(
                report,
                &mut child,
                automation,
                &anchor,
                profile,
                &entry.window,
                entry.session_id,
                output,
                trace_path,
            ),
            Err(failure) => {
                append_blocked_authoring_cases(report, &failure, Some(&child), output, trace_path)
            }
        }
    }

    restore_cursor_before_shutdown(cursor_restore, runner_log);
    stop_child(&mut child, report, runner_log, output, trace_path);
    let existing_case_ids = report
        .cases
        .iter()
        .map(|case| case.id.as_str())
        .collect::<Vec<_>>();
    for id in missing_case_ids(&existing_case_ids)
        .into_iter()
        .filter(|id| !DEFERRED_REPORT_CASE_IDS.contains(id))
    {
        append_case_without_artifacts(
            report,
            id,
            Err(CaseFailure::new(
                FailureStage::Cleanup,
                "runner omitted a required case result".into(),
            )),
        );
    }
    let _ = writeln!(
        runner_log,
        "native suite completed with {} case records",
        report.cases.len()
    );
    Some(anchor)
}

pub fn record_environment_failure(
    message: String,
    report: &mut AcceptanceReport,
    output: &Path,
    trace_path: &Path,
    runner_log: &mut File,
) {
    let failure = CaseFailure::new(FailureStage::Environment, message);
    for (index, id) in CASE_IDS
        .into_iter()
        .filter(|id| !DEFERRED_REPORT_CASE_IDS.contains(id))
        .enumerate()
    {
        if index == 0 {
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
        } else {
            append_case_without_artifacts(
                report,
                id,
                Err(CaseFailure::new(
                    failure.stage,
                    "not run because native environment setup failed; see the first case result"
                        .into(),
                )),
            );
        }
    }
    let _ = writeln!(runner_log, "native environment setup failed: {failure}");
}

fn restore_cursor_before_shutdown(cursor_restore: Option<POINT>, runner_log: &mut File) {
    let Some(point) = cursor_restore else {
        return;
    };

    match set_cursor_position(point) {
        Ok(()) => {
            let _ = writeln!(
                runner_log,
                "cursor restored to captured position ({},{}) before candidate shutdown",
                point.x, point.y
            );
        }
        Err(error) => {
            let _ = writeln!(
                runner_log,
                "cursor restoration before candidate shutdown failed: {error}"
            );
        }
    }
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

fn record_post_g2_root_snapshot(
    report: &mut AcceptanceReport,
    child: &NativeChild,
    output: &Path,
    trace_path: &Path,
) -> String {
    let result = collect_post_g2_root_snapshot(child, trace_path);
    match result {
        Ok(snapshot) => {
            let summary = format!(
                "ROOT HWND={} PID={} visible={} minimized={} bounds={:?} intersects_physical_display={} drawable_on_display_samples={}/{} stable_samples={} stable_on_display={}",
                snapshot.root_hwnd,
                snapshot.child_process_id,
                snapshot.visible,
                snapshot.minimized,
                snapshot.bounds,
                snapshot.intersects_physical_display,
                snapshot.drawable_on_display_samples,
                snapshot.sample_count,
                snapshot.consecutive_stable_samples,
                snapshot.stable_on_display
            );
            let path = output.join("case-G2-root-snapshot.json");
            let serialized = serde_json::to_vec_pretty(&snapshot)
                .map_err(|error| format!("serialize post-G2 ROOT snapshot: {error}"))
                .and_then(|bytes| {
                    fs::write(&path, bytes)
                        .map_err(|error| format!("write post-G2 ROOT snapshot: {error}"))
                });
            match serialized {
                Ok(()) => {
                    if let Some(case) = report.cases.iter_mut().find(|case| case.id == "G2") {
                        case.observed = bounded_text(
                            &format!("{}; post-G2 ROOT snapshot: {summary}", case.observed),
                            MAX_RESULT_BYTES,
                        );
                        case.artifacts
                            .push(bounded_text(&path.to_string_lossy(), MAX_PATH_BYTES));
                        if !snapshot.stable_on_display {
                            case.status = CaseStatus::Failed;
                            case.failure_stage = Some(FailureStage::NativeRootState);
                            case.observed = bounded_text(
                                &format!(
                                    "{}; ROOT did not remain visible, drawable, and stable on a physical display for the required post-G2 samples",
                                    case.observed
                                ),
                                MAX_RESULT_BYTES,
                            );
                        }
                    }
                    report.push_artifact(path.to_string_lossy());
                }
                Err(error) => {
                    mark_post_g2_snapshot_failure(report, &error);
                    return format!("snapshot artifact failed: {error}; state={summary}");
                }
            }
            summary
        }
        Err(error) => {
            mark_post_g2_snapshot_failure(report, &error);
            format!("snapshot collection failed: {error}")
        }
    }
}

fn mark_post_g2_snapshot_failure(report: &mut AcceptanceReport, error: &str) {
    if let Some(case) = report.cases.iter_mut().find(|case| case.id == "G2") {
        case.status = CaseStatus::Failed;
        case.failure_stage = Some(FailureStage::NativeRootState);
        case.observed = bounded_text(
            &format!("{}; post-G2 ROOT snapshot failed: {error}", case.observed),
            MAX_RESULT_BYTES,
        );
    }
}

fn collect_post_g2_root_snapshot(
    child: &NativeChild,
    trace_path: &Path,
) -> Result<PostG2RootSnapshot, String> {
    let displays = native_display_bounds()?;
    let deadline = Instant::now() + POST_G2_ROOT_STABILITY_WINDOW;
    let mut previous = None;
    let mut stable_samples = 0_u8;
    let mut sample_count = 0_u8;
    let mut drawable_on_display_samples = 0_u8;
    let root = loop {
        let root = child.refresh_root()?;
        if root.role != WindowRole::Root || root.process_id != child.process_id() {
            return Err(format!(
                "post-G2 snapshot found a non-child ROOT identity: role={:?} pid={} expected_pid={}",
                root.role,
                root.process_id,
                child.process_id()
            ));
        }
        if root.visible
            && !root.minimized
            && root.is_nonzero()
            && intersects_display_bounds(root.bounds, &displays)
        {
            drawable_on_display_samples = drawable_on_display_samples.saturating_add(1);
        }
        let fingerprint = (
            hwnd_id(root.hwnd),
            root.process_id,
            root.visible,
            root.minimized,
            root.bounds,
        );
        if previous == Some(fingerprint) {
            stable_samples = stable_samples.saturating_add(1);
        } else {
            stable_samples = 1;
            previous = Some(fingerprint);
        }
        sample_count = sample_count.saturating_add(1);
        if Instant::now() >= deadline {
            break root;
        }
        std::thread::sleep(WINDOW_POLL);
    };
    let intersects_physical_display = intersects_display_bounds(root.bounds, &displays);
    let stable_on_display = stable_samples >= POST_G2_ROOT_STABLE_SAMPLES
        && drawable_on_display_samples == sample_count
        && root.visible
        && !root.minimized
        && root.is_nonzero()
        && intersects_physical_display;
    Ok(PostG2RootSnapshot {
        captured_unix_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
        child_process_id: child.process_id(),
        root_hwnd: hwnd_id(root.hwnd),
        visible: root.visible,
        minimized: root.minimized,
        bounds: root.bounds,
        physical_displays: displays,
        intersects_physical_display,
        sample_count,
        drawable_on_display_samples,
        consecutive_stable_samples: stable_samples,
        stable_on_display,
        root_trace_tail: sanitized_root_trace_tail(trace_path),
    })
}

fn sanitized_root_trace_tail(trace_path: &Path) -> Vec<String> {
    const ROOT_EVENTS: &[&str] = &[
        "root_command",
        "desired_visibility",
        "native_window_snapshot",
        "native_activation",
        "window_sample_truncated",
        "restore",
        "budget_exhausted",
    ];
    trace_lines(trace_path)
        .into_iter()
        .filter(|line| ROOT_EVENTS.iter().any(|event| line.contains(event)))
        .filter_map(|line| sanitize_trace_line(&line))
        .map(|line| bounded_text(&line, 768))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .take(16)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

fn run_designer_entry(
    child: &mut NativeChild,
    uia: &UiAutomation,
    anchor: &FocusAnchor,
    trace_path: &Path,
) -> Result<DesignerEntry, CaseFailure> {
    let root = child
        .refresh_root()
        .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
    if !root.visible || root.minimized {
        return Err(CaseFailure::new(
            FailureStage::NativeRootState,
            format!(
                "ROOT is not drawable before Designer entry: visible={} minimized={} bounds={:?}",
                root.visible, root.minimized, root.bounds
            ),
        ));
    }
    let display_bounds = native_display_bounds().map_err(|error| {
        CaseFailure::new(
            FailureStage::Environment,
            format!("could not inspect display bounds before Designer entry: {error}"),
        )
    })?;
    let (mut root, mut restored) =
        ensure_root_on_physical_display(child, anchor, &root, &display_bounds, ROOT_TIMEOUT)
            .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
    require_visible(&root)
        .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
    if !uia.root_is_queryable(root.hwnd, child.process_id()) {
        return Err(CaseFailure::new(
            FailureStage::DesignerEntry,
            "ROOT UIA root is not owned by the launched candidate".into(),
        ));
    }
    // The Apps menu is nested inside File. UIA can retain the submenu command
    // while its popup is closed, so use the production ROOT menu trace as the
    // open-state oracle and click the File parent only when a popup is active.
    let apps_popup_item = uia
        .find_named(root.hwnd, child.process_id(), "Edit Radial Menus")
        .map_err(|error| CaseFailure::new(FailureStage::DesignerEntry, error))?;
    let menu_state = root_menu_state_from_trace(&trace_lines(trace_path)).ok_or_else(|| {
        CaseFailure::new(
            FailureStage::DesignerEntry,
            "ROOT menu state is unavailable or its bounded trace overflowed; refusing speculative menu input".into(),
        )
    })?;
    let closed_apps_menu = if menu_state.any_open() {
        let click = activate_named(
            uia,
            child,
            anchor,
            &root,
            "File",
            FailureStage::DesignerEntry,
            trace_path,
        )
        .map_err(|error| {
            CaseFailure::new(
                error.stage,
                format!(
                    "could not close the published ROOT File/Apps menu by clicking the parent toggle: {}",
                    error.message
                ),
            )
        })?;
        let menus_closed = wait_until(UIA_TIMEOUT, || {
            root_menu_state_from_trace(&trace_lines(trace_path))
                .is_some_and(|state| !state.any_open())
        });
        let closed_state = root_menu_state_from_trace(&trace_lines(trace_path));
        if !menus_closed || !closed_state.is_some_and(|state| !state.any_open()) {
            return Err(CaseFailure::new(
                FailureStage::DesignerEntry,
                format!(
                    "ROOT File/Apps popup remained open in production trace after checked File-parent toggle click=[{}]; refusing to enter Designer with an unknown menu state",
                    click.describe()
                ),
            ));
        }
        Some(format!(
            "checked File-parent click=[{}] closed production menu state from {:?}; UIA submenu node present before click={}",
            click.describe(),
            menu_state,
            apps_popup_item.is_some()
        ))
    } else if apps_popup_item.is_some() {
        Some(format!(
            "no popup input sent: production trace reports File/Apps closed despite the UIA submenu node being published; state={menu_state:?}"
        ))
    } else {
        None
    };
    let (fresh_root, restored_after_menu) =
        ensure_root_on_physical_display(child, anchor, &root, &display_bounds, ROOT_TIMEOUT)
            .map_err(|error| {
                CaseFailure::new(
                    FailureStage::NativeRootState,
                    format!(
                        "ROOT normalization after Apps popup close click={closed_apps_menu:?} failed: {error}"
                    ),
                )
            })?;
    root = fresh_root;
    if restored.is_none() {
        restored = restored_after_menu;
    }
    let trace_cursor = trace_lines(trace_path).len();
    let file_menu = open_root_file_menu(uia, child, anchor, &root, trace_path).map_err(
        |mut error| {
            error.message = format!(
                "{}; entry ROOT bounds={:?}, screenshot display bounds={display_bounds:?}, display_intersection={}, USER32 virtual_intersection={}, checked_F11_restore={}, closed_apps_menu_click={closed_apps_menu:?}",
                error.message,
                root.bounds,
                intersects_display_bounds(root.bounds, &display_bounds),
                root.intersects_virtual_screen(),
                restored.as_deref().unwrap_or("not needed")
            );
            error
        },
    )?;
    let apps_click = activate_named(
        uia,
        child,
        anchor,
        &root,
        "Apps",
        FailureStage::DesignerEntry,
        trace_path,
    )
    .map_err(|error| {
        CaseFailure::new(
            error.stage,
            format!(
                "{}; checked File menu transition={file_menu}",
                error.message
            ),
        )
    })?;
    let edit_click = activate_named(
        uia,
        child,
        anchor,
        &root,
        "Edit Radial Menus",
        FailureStage::DesignerEntry,
        trace_path,
    )?;
    let mut enter_events = None;
    if !wait_until(Duration::from_millis(750), || {
        find_child_window(child, WindowRole::Designer).is_some()
    }) {
        let (fresh_root, _) =
            ensure_root_on_physical_display(child, anchor, &root, &display_bounds, ROOT_TIMEOUT)
                .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
        root = fresh_root;
        let edit_control =
            wait_named_control_in_client(uia, child, &root, "Edit Radial Menus", UIA_TIMEOUT)
                .map_err(|error| CaseFailure::new(FailureStage::DesignerEntry, error))?;
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
                    "{error}; checked File menu transition={file_menu}; semantic pointer events inserted: Apps=[{}], Edit Radial Menus=[{}]; validated UIA focus+Enter events inserted={enter_events:?}",
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
    let Some(snapshot_event) = snapshot_accepted.iter().rev().find(|line| {
        line.contains("trace_event=\"authoring\"")
            && line.contains("edge=ReplyAccepted")
            && line.contains("request_kind=Snapshot")
    }) else {
        return Err(CaseFailure::new(
            FailureStage::DesignerReadiness,
            "Designer did not emit an accepted InitialSnapshot reply; UIA enabled state alone is not sufficient readiness evidence".into(),
        ));
    };
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
    let session_id = trace_field(snapshot_event, "session_id")
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerReadiness,
                "accepted InitialSnapshot reply omitted its Designer session identity".into(),
            )
        })?;
    Ok(DesignerEntry {
        window: designer,
        session_id,
        root_recovery: restored,
        root_menu_resolution: closed_apps_menu,
    })
}

fn open_root_file_menu(
    uia: &UiAutomation,
    child: &NativeChild,
    anchor: &FocusAnchor,
    root: &WindowSnapshot,
    trace_path: &Path,
) -> Result<String, CaseFailure> {
    const FILE_MENU_ATTEMPTS: usize = 2;
    const FILE_MENU_OPEN_TIMEOUT: Duration = Duration::from_millis(900);

    let mut clicks = Vec::with_capacity(FILE_MENU_ATTEMPTS);
    for attempt in 0..FILE_MENU_ATTEMPTS {
        let state = root_menu_state_from_trace(&trace_lines(trace_path)).ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerEntry,
                "ROOT menu state became unavailable or its bounded trace overflowed while opening File".into(),
            )
        })?;
        if root_file_menu_ready_for_apps(state) {
            return Ok(format!(
                "production File menu was already open with Apps closed; checked clicks={}",
                clicks
                    .iter()
                    .map(PointerClickEvidence::describe)
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
        if state.any_open() {
            return Err(CaseFailure::new(
                FailureStage::DesignerEntry,
                format!(
                    "ROOT menu state changed to {state:?} after popup normalization; refusing to toggle an unknown submenu"
                ),
            ));
        }

        let click = activate_named(
            uia,
            child,
            anchor,
            root,
            "File",
            FailureStage::DesignerEntry,
            trace_path,
        )
        .map_err(|error| {
            CaseFailure::new(
                error.stage,
                format!(
                    "checked File-parent click attempt {} failed: {}",
                    attempt + 1,
                    error.message
                ),
            )
        })?;
        clicks.push(click);

        let opened = wait_until(FILE_MENU_OPEN_TIMEOUT, || {
            root_menu_state_from_trace(&trace_lines(trace_path))
                .is_some_and(root_file_menu_ready_for_apps)
        });
        let state = root_menu_state_from_trace(&trace_lines(trace_path)).ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerEntry,
                "ROOT menu state became unavailable or its bounded trace overflowed after the checked File click".into(),
            )
        })?;
        if opened && root_file_menu_ready_for_apps(state) {
            return Ok(format!(
                "production File menu opened with Apps closed after {} checked click(s): [{}]",
                clicks.len(),
                clicks
                    .iter()
                    .map(PointerClickEvidence::describe)
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
        if !should_retry_root_file_menu_open(state) {
            return Err(CaseFailure::new(
                FailureStage::DesignerEntry,
                format!(
                    "checked File-parent click(s) did not reach the expected File-open/Apps-closed state; production state={state:?}, clicks=[{}]",
                    clicks
                        .iter()
                        .map(PointerClickEvidence::describe)
                        .collect::<Vec<_>>()
                        .join("; ")
                ),
            ));
        }
    }

    let displays = native_display_bounds().map_err(|error| {
        CaseFailure::new(
            FailureStage::Environment,
            format!("could not inspect displays before checked File keyboard activation: {error}"),
        )
    })?;
    let fresh_root = child
        .refresh_root()
        .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
    if fresh_root.hwnd != root.hwnd || fresh_root.process_id != root.process_id {
        return Err(CaseFailure::new(
            FailureStage::WindowDiscovery,
            "ROOT identity changed before the checked File keyboard activation".into(),
        ));
    }
    let (stable_root, _) =
        ensure_root_on_physical_display(child, anchor, &fresh_root, &displays, ROOT_TIMEOUT)
            .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
    let current_state = root_menu_state_from_trace(&trace_lines(trace_path)).ok_or_else(|| {
        CaseFailure::new(
            FailureStage::DesignerEntry,
            "ROOT menu state became unavailable or its bounded trace overflowed before the checked File keyboard activation".into(),
        )
    })?;
    if !should_retry_root_file_menu_open(current_state) {
        return Err(CaseFailure::new(
            FailureStage::DesignerEntry,
            format!(
                "ROOT entered an ambiguous menu state before keyboard activation: {current_state:?}"
            ),
        ));
    }
    let file_control = wait_named_control_in_client(uia, child, &stable_root, "File", UIA_TIMEOUT)
        .map_err(|error| CaseFailure::new(FailureStage::DesignerEntry, error))?;
    child
        .focus_window(&stable_root)
        .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
    uia.focus(&file_control)
        .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
    let enter_events = send_enter_current(child, &stable_root)
        .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
    let opened = wait_until(FILE_MENU_OPEN_TIMEOUT, || {
        root_menu_state_from_trace(&trace_lines(trace_path))
            .is_some_and(root_file_menu_ready_for_apps)
    });
    let final_state = root_menu_state_from_trace(&trace_lines(trace_path)).ok_or_else(|| {
        CaseFailure::new(
            FailureStage::DesignerEntry,
            "ROOT menu state became unavailable or its bounded trace overflowed after the checked File keyboard activation".into(),
        )
    })?;
    if opened && root_file_menu_ready_for_apps(final_state) {
        return Ok(format!(
            "production File menu opened with Apps closed after {} checked pointer click(s) and UIA-focused Enter inserted={enter_events}; clicks=[{}]",
            clicks.len(),
            clicks
                .iter()
                .map(PointerClickEvidence::describe)
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }

    Err(CaseFailure::new(
        FailureStage::DesignerEntry,
        format!(
            "production File menu remained closed after {} checked pointer click(s) and UIA-focused Enter inserted={enter_events}; state={final_state:?}, clicks=[{}]",
            clicks.len(),
            clicks
                .iter()
                .map(PointerClickEvidence::describe)
                .collect::<Vec<_>>()
                .join("; ")
        ),
    ))
}

pub(super) fn native_display_bounds() -> Result<Vec<[i32; 4]>, String> {
    let screens = screenshots::Screen::all()
        .map_err(|error| format!("enumerate physical display bounds: {error:?}"))?;
    let bounds = screens
        .into_iter()
        .take(16)
        .map(|screen| {
            let display = screen.display_info;
            [
                display.x,
                display.y,
                display
                    .x
                    .saturating_add(i32::try_from(display.width).unwrap_or(i32::MAX)),
                display
                    .y
                    .saturating_add(i32::try_from(display.height).unwrap_or(i32::MAX)),
            ]
        })
        .collect::<Vec<_>>();
    if bounds.is_empty() {
        return Err("display enumeration returned no monitors".into());
    }
    Ok(bounds)
}

pub(super) fn intersects_display_bounds(window: [i32; 4], displays: &[[i32; 4]]) -> bool {
    displays.iter().any(|display| {
        window[0] < display[2]
            && window[2] > display[0]
            && window[1] < display[3]
            && window[3] > display[1]
    })
}

fn ensure_root_on_physical_display(
    child: &NativeChild,
    anchor: &FocusAnchor,
    expected: &WindowSnapshot,
    displays: &[[i32; 4]],
    timeout: Duration,
) -> Result<(WindowSnapshot, Option<String>), String> {
    if expected.process_id != child.process_id() || expected.role != WindowRole::Root {
        return Err(
            "refused to normalize a ROOT snapshot not owned by the launched candidate".into(),
        );
    }

    let initial = child.refresh_root()?;
    if initial.hwnd != expected.hwnd || initial.process_id != expected.process_id {
        return Err(format!(
            "ROOT identity changed before UI interaction: expected HWND={} PID={}, found HWND={} PID={}",
            hwnd_id(expected.hwnd),
            expected.process_id,
            hwnd_id(initial.hwnd),
            initial.process_id
        ));
    }
    if !initial.visible || initial.minimized || !initial.is_nonzero() {
        return Err(format!(
            "ROOT is not drawable before UI interaction: visible={} minimized={} bounds={:?}",
            initial.visible, initial.minimized, initial.bounds
        ));
    }

    let mut restored = None;
    if !intersects_display_bounds(initial.bounds, displays) {
        anchor.focus().map_err(|error| {
            format!(
                "ROOT requires checked recovery from offscreen bounds {:?} (visible={}, minimized={}, displays={displays:?}); runner anchor focus failed: {error}",
                initial.bounds, initial.visible, initial.minimized
            )
        })?;
        let tap = child.send_f11(anchor.hwnd(), anchor.process_id(), TAP_TIME)?;
        restored = Some(format!(
            "checked runner-anchored F11 restored parked ROOT HWND={} with down/up events=[{},{}]",
            hwnd_id(initial.hwnd),
            tap.down.inserted,
            tap.up.inserted
        ));
    }

    let deadline = Instant::now() + timeout;
    let mut previous_bounds = None;
    let mut stable_samples = 0_u8;
    loop {
        let fresh = child.refresh_root()?;
        if fresh.hwnd != expected.hwnd || fresh.process_id != expected.process_id {
            return Err(format!(
                "ROOT identity changed while stabilizing: expected HWND={} PID={}, found HWND={} PID={}",
                hwnd_id(expected.hwnd),
                expected.process_id,
                hwnd_id(fresh.hwnd),
                fresh.process_id
            ));
        }
        if !fresh.visible || fresh.minimized || !fresh.is_nonzero() {
            return Err(format!(
                "ROOT stopped being drawable while stabilizing: visible={} minimized={} bounds={:?}",
                fresh.visible, fresh.minimized, fresh.bounds
            ));
        }
        if intersects_display_bounds(fresh.bounds, displays) {
            if previous_bounds == Some(fresh.bounds) {
                stable_samples = stable_samples.saturating_add(1);
            } else {
                stable_samples = 1;
                previous_bounds = Some(fresh.bounds);
            }
            if stable_samples >= 2 {
                return Ok((fresh, restored));
            }
        } else {
            previous_bounds = None;
            stable_samples = 0;
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "ROOT did not stabilize on a physical display within {} ms: HWND={} PID={} bounds={:?} displays={displays:?}; restoration={}",
                timeout.as_millis(),
                hwnd_id(fresh.hwnd),
                fresh.process_id,
                fresh.bounds,
                restored.as_deref().unwrap_or("not needed")
            ));
        }
        std::thread::sleep(WINDOW_POLL);
    }
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
        let _ = (menu_name, next_state, uia_limitation, click_proofs);
        Ok(format!(
            "evidence:v1; text_edit=restored; tab_focus=menu_combo; unsaved=false; tab_events={count}; unicode_events={probe_input_count:?}"
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

fn wait_for_geometry_state(
    trace_path: &Path,
    session_id: u64,
    timeout: Duration,
    mut predicate: impl FnMut(&GeometryStateSnapshot) -> bool,
) -> Result<GeometryStateSnapshot, String> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(state) = latest_geometry_state(trace_path)?
            && state.session_id == session_id
            && predicate(&state)
        {
            return Ok(state);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "Designer geometry state for session {session_id} did not reach the required state"
            ));
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

fn wait_for_geometry_state_matching(
    trace_path: &Path,
    session_id: u64,
    timeout: Duration,
    mut predicate: impl FnMut(&GeometryStateSnapshot) -> bool,
) -> Result<GeometryStateSnapshot, String> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(state) = geometry_states(trace_path)?
            .into_iter()
            .rev()
            .find(|state| state.session_id == session_id && predicate(state))
        {
            return Ok(state);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "Designer geometry trace did not publish a matching state for session {session_id}"
            ));
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

fn wait_for_unique_authoring_control_any_index(
    trace_path: &Path,
    session_id: u64,
    target: AuthoringControlTarget,
    role: AuthoringControlRole,
    timeout: Duration,
) -> Result<AuthoringControlSnapshot, String> {
    let deadline = Instant::now() + timeout;
    loop {
        let matches = list_authoring_controls(trace_path, session_id)?
            .into_iter()
            .filter(|control| control.target == target && control.role == role)
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [control] => return Ok(*control),
            [] => {}
            _ => {
                return Err(format!(
                    "ambiguous Designer semantic target {target:?} at multiple indices for session {session_id}"
                ));
            }
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "Designer did not publish target {target:?} with role {role:?} for session {session_id}"
            ));
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

fn wait_for_authoring_control(
    trace_path: &Path,
    session_id: u64,
    target: AuthoringControlTarget,
    index: Option<usize>,
    role: AuthoringControlRole,
    timeout: Duration,
) -> Result<AuthoringControlSnapshot, String> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(control) = find_authoring_control(trace_path, session_id, target, index, role)?
        {
            return Ok(control);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "Designer did not publish target {target:?} index={index:?} role={role:?} for session {session_id}"
            ));
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

fn wait_for_authoring_control_after(
    trace_path: &Path,
    first_line: usize,
    session_id: u64,
    expected_client_size: [i32; 2],
    target: AuthoringControlTarget,
    index: Option<usize>,
    role: AuthoringControlRole,
    timeout: Duration,
) -> Result<AuthoringControlSnapshot, String> {
    let deadline = Instant::now() + timeout;
    let mut latest_client_size = None;
    loop {
        if let Some(control) =
            find_authoring_control_after(trace_path, first_line, session_id, target, index, role)?
        {
            latest_client_size = Some(control.client_size);
            if client_size_matches(control.client_size, expected_client_size) {
                return Ok(control);
            }
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "Designer did not render target {target:?} index={index:?} role={role:?} for session {session_id} at compact client size {expected_client_size:?} after trace line {first_line}; last frame size={latest_client_size:?}"
            ));
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

fn client_size_matches(rendered: [i32; 2], native: [i32; 2]) -> bool {
    rendered[0].abs_diff(native[0]) <= 1 && rendered[1].abs_diff(native[1]) <= 1
}

fn committed_cell_ids_match(
    candidate_digest_available: bool,
    candidate_digest: u64,
    committed_digest: u64,
) -> bool {
    candidate_digest_available && candidate_digest == committed_digest
}

fn click_authoring_target(
    child: &NativeChild,
    designer: &WindowSnapshot,
    trace_path: &Path,
    session_id: u64,
    target: AuthoringControlTarget,
    index: Option<usize>,
    role: AuthoringControlRole,
) -> Result<(AuthoringControlSnapshot, PointerClickEvidence), String> {
    let control =
        wait_for_authoring_control(trace_path, session_id, target, index, role, UIA_TIMEOUT)?;
    if !control.enabled {
        return Err(format!(
            "refused native click on disabled Designer target {target:?} index={index:?}"
        ));
    }
    let evidence = click_designer_client_bounds(child, designer, control.bounds, trace_path)?;
    Ok((control, evidence))
}

fn set_requested_slots(
    child: &NativeChild,
    designer: &WindowSnapshot,
    trace_path: &Path,
    session_id: u64,
    value: usize,
) -> Result<String, String> {
    let click_cursor = trace_lines(trace_path).len();
    let (control, click) = click_authoring_target(
        child,
        designer,
        trace_path,
        session_id,
        AuthoringControlTarget::Slots,
        None,
        AuthoringControlRole::DragValue,
    )?;
    let click_cycle = wait_trace(trace_path, click_cursor, TRACE_TIMEOUT, |events| {
        authoring_control_click_finished(
            events,
            session_id,
            AuthoringControlTarget::Slots,
            None,
            AuthoringControlRole::DragValue,
        )
    });
    if !authoring_control_click_finished(
        &click_cycle,
        session_id,
        AuthoringControlTarget::Slots,
        None,
        AuthoringControlRole::DragValue,
    ) {
        return Err(
            "native Slots click was not followed by a fresh Designer frame before text entry"
                .into(),
        );
    }
    let select_all = send_select_all_to_focused_window(child, designer)?;
    let text_edges = send_text_to_focused_window(child, designer, &value.to_string())?;
    let enter_commit = send_enter_current(child, designer)?;
    let state = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
        state.requested_slots == value
    })?;
    Ok(format!(
        "semantic Slots {:?}, native click=[{}], waited for its next rendered frame, Ctrl+A events={select_all}, checked text events={text_edges}, Enter commit={enter_commit}; requested slots={} observed while committed ring slots remained {}",
        control.bounds,
        click.describe(),
        state.requested_slots,
        state.selected_ring_slots
    ))
}

fn append_authoring_case(
    report: &mut AcceptanceReport,
    child: &NativeChild,
    output: &Path,
    trace_path: &Path,
    id: &str,
    operation: impl FnOnce() -> Result<String, CaseFailure>,
) {
    let started = Instant::now();
    append_case(
        report,
        id,
        expected(id),
        started,
        operation(),
        Some(child),
        output,
        trace_path,
    );
}

fn authoring_case_failure(stage: FailureStage, error: impl std::fmt::Display) -> CaseFailure {
    CaseFailure::new(stage, error.to_string())
}

fn run_authoring_geometry_cases(
    report: &mut AcceptanceReport,
    child: &mut NativeChild,
    uia: &UiAutomation,
    anchor: &FocusAnchor,
    profile: &Path,
    designer: &WindowSnapshot,
    session_id: u64,
    output: &Path,
    trace_path: &Path,
) {
    let mut authored_menu_graph = None;
    let mut overflow_root_graph = None;
    append_authoring_case(report, child, output, trace_path, "A0", || {
        let menus = wait_for_designer_semantic_target_in_session(
            trace_path,
            DesignerSemanticTarget::Menus,
            session_id,
            UIA_TIMEOUT,
            |_| true,
        )
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerReadiness,
                format!(
                    "the reopened Designer session {session_id} did not publish its Menus mode target"
                ),
            )
        })?;
        let mode_evidence = if menus.selected {
            "reopened Designer already selected Menus".to_string()
        } else {
            let mode_click =
                click_designer_client_bounds(child, designer, menus.bounds, trace_path).map_err(
                    |error| authoring_case_failure(FailureStage::DesignerNativeTarget, error),
                )?;
            let selected = wait_for_designer_semantic_target_in_session(
                trace_path,
                DesignerSemanticTarget::Menus,
                session_id,
                TRACE_TIMEOUT,
                |state| state.selected,
            )
            .ok_or_else(|| {
                CaseFailure::new(
                    FailureStage::DesignerMutation,
                    format!(
                        "checked native Menus mode click did not select Menus in session {session_id}"
                    ),
                )
            })?;
            format!(
                "reopened Designer switched from Skins to Menus by checked click [{}], selected target bounds={:?}",
                mode_click.describe(),
                selected.bounds
            )
        };
        let before = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |_| true)
            .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        let (control, click) = click_authoring_target(
            child,
            designer,
            trace_path,
            session_id,
            AuthoringControlTarget::NewMenu,
            None,
            AuthoringControlRole::Button,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        let after = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
            state.menu_count == before.menu_count + 1
                && state.selected_menu_index == Some(before.menu_count)
                && !state.proposal_active
        })
        .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        Ok(format!(
            "{mode_evidence}; New Menu target role={:?} client_bounds={:?}; checked click=[{}]; menu count {} -> {}, selected menu index {:?}, generation {}",
            control.role,
            control.bounds,
            click.describe(),
            before.menu_count,
            after.menu_count,
            after.selected_menu_index,
            after.generation
        ))
    });

    append_authoring_case(report, child, output, trace_path, "A1", || {
        let before = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
            !state.proposal_active && state.selected_menu_index.is_some()
        })
        .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        if before.selected_menu_index != Some(before.menu_count.saturating_sub(1))
            || before.ring_count == 0
        {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "New Menu selection was not retained before Add Ring: menus={}, selected={:?}, rings={}",
                    before.menu_count, before.selected_menu_index, before.ring_count
                ),
            ));
        }
        let (control, click) = click_authoring_target(
            child,
            designer,
            trace_path,
            session_id,
            AuthoringControlTarget::AddRing,
            None,
            AuthoringControlRole::Button,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        let preview = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
            state.proposal_active && state.proposal_kind == AuthoringProposalKind::NewRing
        })
        .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        if preview.ring_count != before.ring_count
            || preview.selected_ring_slots != before.selected_ring_slots
            || preview.generation != before.generation
            || preview.proposal_candidate_rings != before.ring_count + 1
            || preview.proposal_slots != 8
            || !preview.proposal_cell_ids_preserved
        {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "Add Ring preview changed the draft or proposed an unexpected candidate: rings={} -> {}, slots={} -> {}, candidate_rings={}, proposal_slots={}, existing_cell_ids_preserved={}",
                    before.ring_count,
                    preview.ring_count,
                    before.selected_ring_slots,
                    preview.selected_ring_slots,
                    preview.proposal_candidate_rings,
                    preview.proposal_slots,
                    preview.proposal_cell_ids_preserved
                ),
            ));
        }
        let ready = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
            state.proposal_active && state.proposal_ready
        })
        .map_err(|error| authoring_case_failure(FailureStage::DesignerPresentation, error))?;
        let apply = wait_for_authoring_control(
            trace_path,
            session_id,
            AuthoringControlTarget::ApplyProposal,
            None,
            AuthoringControlRole::Button,
            UIA_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        if !apply.enabled || !ready.proposal_ready {
            return Err(CaseFailure::new(
                FailureStage::DesignerPresentation,
                "Apply proposal did not become enabled for the exact prepared candidate".into(),
            ));
        }
        Ok(format!(
            "Add Ring role={:?} checked click=[{}]; proposal remains uncommitted at generation {} with committed rings/slots {}/{}, candidate rings={} and slots={} are previewed; candidate preserves existing cell IDs={}; Apply control enabled={} and is reserved for G0",
            control.role,
            click.describe(),
            preview.generation,
            preview.ring_count,
            preview.selected_ring_slots,
            ready.proposal_candidate_rings,
            ready.proposal_slots,
            ready.proposal_cell_ids_preserved,
            apply.enabled
        ))
    });

    append_authoring_case(report, child, output, trace_path, "G0", || {
        let prepared = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
            state.proposal_active
                && state.proposal_kind == AuthoringProposalKind::NewRing
                && state.proposal_ready
        })
        .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        if prepared.proposal_candidate_rings != prepared.ring_count + 1
            || prepared.proposal_slots != 8
            || !prepared.proposal_cell_ids_preserved
        {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "outer-ring proposal was not the exact safe candidate to apply: committed rings={}, candidate rings={}, slots={}, existing cell IDs preserved={}",
                    prepared.ring_count,
                    prepared.proposal_candidate_rings,
                    prepared.proposal_slots,
                    prepared.proposal_cell_ids_preserved
                ),
            ));
        }
        let apply = wait_for_authoring_control(
            trace_path,
            session_id,
            AuthoringControlTarget::ApplyProposal,
            None,
            AuthoringControlRole::Button,
            UIA_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        if !apply.enabled {
            return Err(CaseFailure::new(
                FailureStage::DesignerPresentation,
                "Apply proposal was disabled for the ready outer-ring candidate".into(),
            ));
        }
        let apply_click = click_designer_client_bounds(child, designer, apply.bounds, trace_path)
            .map_err(|error| {
            authoring_case_failure(FailureStage::DesignerNativeTarget, error)
        })?;
        let applied = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
            !state.proposal_active
                && state.ring_count == prepared.proposal_candidate_rings
                && state.selected_menu_index == prepared.selected_menu_index
                && state.selected_ring_index == Some(prepared.ring_count)
                && state.selected_ring_slots == prepared.proposal_slots
                && state.generation > prepared.generation
        })
        .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        let post_apply_ids_preserved = committed_cell_ids_match(
            prepared.proposal_cell_ids_digest_available,
            prepared.proposal_cell_ids_digest,
            applied.draft_cell_ids_digest,
        );
        if !post_apply_ids_preserved {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "committed outer-ring draft did not match the prepared candidate cell IDs after Apply".into(),
            ));
        }
        if applied.menu_populated != prepared.menu_populated {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "applying an empty outer ring changed existing menu contents: populated cells {} -> {}",
                    prepared.menu_populated, applied.menu_populated
                ),
            ));
        }
        let menu_index = applied.selected_menu_index.ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerMutation,
                "applied outer-ring draft lost its selected menu identity".into(),
            )
        })?;
        if menu_index != prepared.selected_menu_index.unwrap_or(usize::MAX)
            || applied.ring_count != 2
            || applied.selected_ring_index != Some(1)
            || applied.selected_ring_slots != 8
            || applied.menu_populated != 0
        {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "A0/G0 geometry was not a selected two-ring blank menu after Apply: index={menu_index}, rings={}, selected ring={:?}, selected slots={}, populated={}",
                    applied.ring_count,
                    applied.selected_ring_index,
                    applied.selected_ring_slots,
                    applied.menu_populated
                ),
            ));
        }
        Ok(format!(
            "ready outer-ring candidate with {} committed and {} proposed rings, {} proposed slots, and existing cell IDs preserved=true; Apply enabled={}; checked click=[{}]; committed state has {} rings, selects new ring {:?}, {} slots, unchanged populated cells={}, post-Apply stable cell IDs match candidate={}, generation {} -> {}",
            prepared.ring_count,
            prepared.proposal_candidate_rings,
            prepared.proposal_slots,
            apply.enabled,
            apply_click.describe(),
            applied.ring_count,
            applied.selected_ring_index,
            applied.selected_ring_slots,
            applied.menu_populated,
            post_apply_ids_preserved,
            prepared.generation,
            applied.generation
        ))
    });

    append_authoring_case(report, child, output, trace_path, "A2", || {
        let start = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
            !state.proposal_active && state.ring_count >= 2 && state.selected_ring_index == Some(1)
        })
        .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        if start.selected_ring_slots != 8 {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "new ring has unexpected slot count {}",
                    start.selected_ring_slots
                ),
            ));
        }
        let (_, selector_click) = click_authoring_target(
            child,
            designer,
            trace_path,
            session_id,
            AuthoringControlTarget::RingSelector,
            None,
            AuthoringControlRole::ComboBox,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        let ring_zero = wait_for_authoring_control(
            trace_path,
            session_id,
            AuthoringControlTarget::RingOption,
            Some(0),
            AuthoringControlRole::Selectable,
            UIA_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        if ring_zero.selected {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "Ring selector unexpectedly reported its first option as selected".into(),
            ));
        }
        let ring_zero_click =
            click_designer_client_bounds(child, designer, ring_zero.bounds, trace_path).map_err(
                |error| authoring_case_failure(FailureStage::DesignerNativeTarget, error),
            )?;
        let selected_zero =
            wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
                !state.proposal_active && state.selected_ring_index == Some(0)
            })
            .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        if selected_zero.selected_ring_slots == 0 {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "selecting Ring 1 unexpectedly produced an empty ring".into(),
            ));
        }
        let (_, selector_reopen) = click_authoring_target(
            child,
            designer,
            trace_path,
            session_id,
            AuthoringControlTarget::RingSelector,
            None,
            AuthoringControlRole::ComboBox,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        let ring_one = wait_for_authoring_control(
            trace_path,
            session_id,
            AuthoringControlTarget::RingOption,
            Some(1),
            AuthoringControlRole::Selectable,
            UIA_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        if ring_one.selected {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "Ring selector unexpectedly reported its second option selected after choosing Ring 1".into(),
            ));
        }
        let ring_one_click =
            click_designer_client_bounds(child, designer, ring_one.bounds, trace_path).map_err(
                |error| authoring_case_failure(FailureStage::DesignerNativeTarget, error),
            )?;
        let selected_one =
            wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
                !state.proposal_active && state.selected_ring_index == Some(1)
            })
            .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        if selected_one.selected_ring_slots != 8 {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "selecting Ring 2 changed its committed slot count to {}",
                    selected_one.selected_ring_slots
                ),
            ));
        }
        let slots_evidence = set_requested_slots(child, designer, trace_path, session_id, 10)
            .map_err(|error| authoring_case_failure(FailureStage::DesignerFrameworkInput, error))?;
        let (_, preview_click) = click_authoring_target(
            child,
            designer,
            trace_path,
            session_id,
            AuthoringControlTarget::PreviewProposal,
            None,
            AuthoringControlRole::Button,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        let prepared = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
            state.proposal_active
                && state.proposal_kind == AuthoringProposalKind::Resize
                && state.proposal_ready
        })
        .map_err(|error| authoring_case_failure(FailureStage::DesignerPresentation, error))?;
        if prepared.selected_ring_slots != 8
            || prepared.requested_slots != 10
            || prepared.proposal_slots != 10
            || prepared.proposal_candidate_rings != 2
            || !prepared.proposal_cell_ids_preserved
        {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "slot growth preview did not preserve draft geometry and original cell IDs: slots={} requested={} candidate={} rings={} IDs_preserved={}",
                    prepared.selected_ring_slots,
                    prepared.requested_slots,
                    prepared.proposal_slots,
                    prepared.proposal_candidate_rings,
                    prepared.proposal_cell_ids_preserved
                ),
            ));
        }
        let apply = wait_for_authoring_control(
            trace_path,
            session_id,
            AuthoringControlTarget::ApplyProposal,
            None,
            AuthoringControlRole::Button,
            UIA_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        if !apply.enabled {
            return Err(CaseFailure::new(
                FailureStage::DesignerPresentation,
                "Apply proposal remained disabled after the exact slot-growth candidate was prepared".into(),
            ));
        }
        let apply_click = click_designer_client_bounds(child, designer, apply.bounds, trace_path)
            .map_err(|error| {
            authoring_case_failure(FailureStage::DesignerNativeTarget, error)
        })?;
        let applied = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
            !state.proposal_active
                && state.selected_ring_index == Some(1)
                && state.selected_ring_slots == 10
                && state.generation > prepared.generation
        })
        .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        let post_apply_ids_preserved = committed_cell_ids_match(
            prepared.proposal_cell_ids_digest_available,
            prepared.proposal_cell_ids_digest,
            applied.draft_cell_ids_digest,
        );
        if !post_apply_ids_preserved {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "committed grown-ring draft did not match the prepared candidate cell IDs after Apply".into(),
            ));
        }
        let menu_index = applied.selected_menu_index.ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerMutation,
                "grown-ring A0 menu no longer had a selected menu index".into(),
            )
        })?;
        if menu_index != start.selected_menu_index.unwrap_or(usize::MAX)
            || applied.ring_count != 2
            || applied.selected_ring_index != Some(1)
            || applied.selected_ring_slots != 10
            || applied.menu_populated != 0
        {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "A2 did not retain the A0/G0 two-ring blank menu with [8,10] slots: index={menu_index}, rings={}, selected ring={:?}, selected slots={}, populated={}",
                    applied.ring_count,
                    applied.selected_ring_index,
                    applied.selected_ring_slots,
                    applied.menu_populated
                ),
            ));
        }
        authored_menu_graph = Some(PersistedMenuGraphExpectation {
            menu_index,
            ring_slots: vec![8, 10],
            populated_cells: 0,
            cell_ids_digest: applied.draft_cell_ids_digest,
        });
        let _ = (
            selector_click,
            ring_zero_click,
            selector_reopen,
            ring_one_click,
            slots_evidence,
            preview_click,
            apply_click,
        );
        Ok(format!(
            "evidence:v1; geometry=[8,10]; candidate_ids_preserved={}; committed={}; stable_ids={}; generation={}",
            prepared.proposal_cell_ids_preserved,
            !applied.proposal_active,
            post_apply_ids_preserved,
            applied.generation
        ))
    });

    append_authoring_case(report, child, output, trace_path, "G1", || {
        let evidence = run_populated_shrink_resolution(child, designer, trace_path, session_id)
            .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        let root = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
            !state.proposal_active
                && state.selected_menu_index == Some(0)
                && state.selected_ring_index == Some(0)
                && state.ring_count == 2
                && state.selected_ring_slots == 8
                && state.menu_populated == 9
        })
        .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        let (_, selector_click) = click_authoring_target(
            child,
            designer,
            trace_path,
            session_id,
            AuthoringControlTarget::RingSelector,
            None,
            AuthoringControlRole::ComboBox,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        let ring_one = wait_for_authoring_control(
            trace_path,
            session_id,
            AuthoringControlTarget::RingOption,
            Some(1),
            AuthoringControlRole::Selectable,
            UIA_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        if ring_one.selected {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "G1 selected the new overflow ring before its exact slot count was checked".into(),
            ));
        }
        let ring_one_click =
            click_designer_client_bounds(child, designer, ring_one.bounds, trace_path).map_err(
                |error| authoring_case_failure(FailureStage::DesignerNativeTarget, error),
            )?;
        let overflow = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
            !state.proposal_active
                && state.selected_menu_index == Some(0)
                && state.selected_ring_index == Some(1)
                && state.ring_count == 2
                && state.selected_ring_slots == 1
                && state.menu_populated == 9
        })
        .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        if overflow.draft_cell_ids_digest != root.draft_cell_ids_digest {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "selecting the G1 overflow ring changed the complete root menu/ring/cell ID graph"
                    .into(),
            ));
        }
        let (_, selector_reopen) = click_authoring_target(
            child,
            designer,
            trace_path,
            session_id,
            AuthoringControlTarget::RingSelector,
            None,
            AuthoringControlRole::ComboBox,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        let ring_zero = wait_for_authoring_control(
            trace_path,
            session_id,
            AuthoringControlTarget::RingOption,
            Some(0),
            AuthoringControlRole::Selectable,
            UIA_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        let ring_zero_click =
            click_designer_client_bounds(child, designer, ring_zero.bounds, trace_path).map_err(
                |error| authoring_case_failure(FailureStage::DesignerNativeTarget, error),
            )?;
        let restored = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
            !state.proposal_active
                && state.selected_menu_index == Some(0)
                && state.selected_ring_index == Some(0)
                && state.ring_count == 2
                && state.selected_ring_slots == 8
                && state.menu_populated == 9
        })
        .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        if restored.draft_cell_ids_digest != overflow.draft_cell_ids_digest {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "returning to the G1 root ring changed the complete root menu/ring/cell ID graph"
                    .into(),
            ));
        }
        overflow_root_graph = Some(PersistedMenuGraphExpectation {
            menu_index: 0,
            ring_slots: vec![8, 1],
            populated_cells: 9,
            cell_ids_digest: restored.draft_cell_ids_digest,
        });
        let _ = (
            evidence,
            selector_click,
            ring_one_click,
            ring_zero_click,
            selector_reopen,
            overflow,
        );
        Ok(format!(
            "evidence:v1; overflow_root=[8,1]/9; stable_ids={}; root_ring_slots={}; populated_cells={}",
            restored.draft_cell_ids_digest == root.draft_cell_ids_digest,
            root.selected_ring_slots,
            root.menu_populated
        ))
    });

    append_authoring_case(report, child, output, trace_path, "G2", || {
        let compact = run_compact_geometry_case(child, designer, trace_path, session_id)
            .map_err(|error| authoring_case_failure(FailureStage::DesignerPresentation, error))?;
        let working_viewport = restore_authoring_viewport(child, designer, trace_path, session_id)
            .map_err(|error| authoring_case_failure(FailureStage::DesignerPresentation, error))?;
        Ok(format!(
            "{compact}; {working_viewport}; retaining the authored A0/G0/A2 menu and G1 overflow-root draft for the A3-A6 Save/reopen workflow"
        ))
    });

    let post_g2_observation = record_post_g2_root_snapshot(report, child, output, trace_path);
    let side_effect_baseline = match capture_action_side_effect_baseline(profile, trace_path) {
        Ok(baseline) => baseline,
        Err(error) => {
            let failure = CaseFailure::new(FailureStage::DesignerReadiness, error);
            append_authoring_case(report, child, output, trace_path, "A3", || {
                Err(failure.clone())
            });
            append_blocked_ids(
                report,
                &["A4", "A5", "A6", "A7", "A8", "D3", "D6", "D7"],
                &failure,
                output,
                trace_path,
                "strict pre-A3 leaf-side-effect baseline could not be captured",
            );
            return;
        }
    };
    let entry_evidence = format!(
        "post-G2 ROOT snapshot={post_g2_observation}; same Designer session {session_id} and authored geometry remain open for A3-A6"
    );
    let Some(entry) = run_radial_action_authoring_cases(
        report,
        child,
        uia,
        anchor,
        designer,
        output,
        trace_path,
        profile,
        session_id,
        ACCEPTANCE_TARGET_ACTION_INDEX,
        &entry_evidence,
        authored_menu_graph,
        overflow_root_graph,
        &side_effect_baseline,
    ) else {
        append_blocked_lifecycle_cases(report, output, trace_path);
        return;
    };
    run_designer_lifecycle_cases(
        report,
        child,
        uia,
        anchor,
        &entry,
        output,
        trace_path,
        profile,
        &side_effect_baseline,
    );
}

fn run_radial_action_authoring_cases(
    report: &mut AcceptanceReport,
    child: &mut NativeChild,
    uia: &UiAutomation,
    anchor: &FocusAnchor,
    designer: &WindowSnapshot,
    output: &Path,
    trace_path: &Path,
    profile: &Path,
    session_id: u64,
    target_action_index: usize,
    entry_evidence: &str,
    authored_menu_graph: Option<PersistedMenuGraphExpectation>,
    overflow_root_graph: Option<PersistedMenuGraphExpectation>,
    side_effect_baseline: &ActionSideEffectBaseline,
) -> Option<DesignerEntry> {
    let mut selected_canvas_cell_index = None;
    let mut selected_cell_slot_index = None;
    let mut authored_cell_identity = None;
    let mut action_mutation_generation = None;
    let mut style_mutation_generation = None;
    append_authoring_case(report, child, output, trace_path, "A3", || {
        let expected_graph = authored_menu_graph.as_ref().ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerReadiness,
                "A0/G0/A2 did not retain a typed two-ring menu identity graph for action authoring"
                    .into(),
            )
        })?;
        let before_menu = latest_geometry_state(trace_path)
            .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?
            .filter(|state| state.session_id == session_id && !state.proposal_active)
            .ok_or_else(|| {
                CaseFailure::new(
                    FailureStage::DesignerReadiness,
                    "Designer did not publish pre-authoring geometry before selecting the blank A0 menu"
                        .into(),
                )
            })?;
        if before_menu.menu_count <= expected_graph.menu_index {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "captured A0 menu index {} is outside the live menu list of {}",
                    expected_graph.menu_index, before_menu.menu_count
                ),
            ));
        }
        let menu_row = wait_for_authoring_control(
            trace_path,
            session_id,
            AuthoringControlTarget::MenuRow,
            Some(expected_graph.menu_index),
            AuthoringControlRole::Selectable,
            UIA_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        let menu_click = if menu_row.selected {
            None
        } else {
            Some(
                click_designer_client_bounds(child, designer, menu_row.bounds, trace_path)
                    .map_err(|error| {
                        authoring_case_failure(FailureStage::DesignerNativeTarget, error)
                    })?,
            )
        };
        let selected_menu_state =
            wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
                state.menu_count > expected_graph.menu_index
                    && state.selected_menu_index == Some(expected_graph.menu_index)
                    && state.ring_count == expected_graph.ring_slots.len()
                    && state.menu_populated == expected_graph.populated_cells
                    && state.draft_cell_ids_digest == expected_graph.cell_ids_digest
            })
            .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        if expected_graph.ring_slots != [8, 10] || expected_graph.populated_cells != 0 {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "captured A0/G0/A2 graph did not describe the approved blank [8,10] menu: rings={:?}, populated={}",
                    expected_graph.ring_slots, expected_graph.populated_cells
                ),
            ));
        }
        let (_, selector_click) = click_authoring_target(
            child,
            designer,
            trace_path,
            session_id,
            AuthoringControlTarget::RingSelector,
            None,
            AuthoringControlRole::ComboBox,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        let ring_one = wait_for_authoring_control(
            trace_path,
            session_id,
            AuthoringControlTarget::RingOption,
            Some(1),
            AuthoringControlRole::Selectable,
            UIA_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        if ring_one.selected {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "A3 expected the retained A2 outer ring to be available for selection".into(),
            ));
        }
        let ring_click = click_designer_client_bounds(child, designer, ring_one.bounds, trace_path)
            .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        let blank_menu = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
            state.selected_menu_index == Some(expected_graph.menu_index)
                && state.generation >= selected_menu_state.generation
                && state.selected_ring_index == Some(1)
                && state.selected_ring_slots == 10
                && state.ring_count == 2
                && state.menu_populated == 0
                && state.draft_cell_ids_digest == expected_graph.cell_ids_digest
        })
        .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        let cell_slot_index = 0;
        let cell_index = flat_canvas_cell_index(&expected_graph.ring_slots, 1, cell_slot_index)
            .ok_or_else(|| {
                CaseFailure::new(
                    FailureStage::DesignerMutation,
                    "authored outer ring did not contain blank slot 0".into(),
                )
            })?;
        let cell = wait_for_canvas_cell_in_generation(
            trace_path,
            session_id,
            blank_menu.generation,
            cell_index,
            expected_graph.cell_ids_digest,
            1,
            cell_slot_index,
            UIA_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        let index = cell.index.ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerReadiness,
                "authored canvas cell omitted its numeric slot identity".into(),
            )
        })?;
        if index != cell_index {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "authored outer-ring slot 0 mapped to CanvasCell index {index}, expected flat index {cell_index}"
                ),
            ));
        }
        selected_canvas_cell_index = Some(index);
        selected_cell_slot_index = Some(cell_slot_index);
        let click_cursor = trace_lines(trace_path).len();
        let click = click_designer_client_bounds(child, designer, cell.bounds, trace_path)
            .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        wait_for_authoring_control_click_finished(
            trace_path,
            click_cursor,
            session_id,
            AuthoringControlTarget::CanvasCell,
            Some(index),
            AuthoringControlRole::Region,
            TRACE_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        wait_for_authoring_control_selected(
            trace_path,
            session_id,
            AuthoringControlTarget::CanvasCell,
            Some(index),
            AuthoringControlRole::Region,
            true,
            TRACE_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;

        click_authoring_target(
            child,
            designer,
            trace_path,
            session_id,
            AuthoringControlTarget::CellType,
            None,
            AuthoringControlRole::ComboBox,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        click_authoring_target(
            child,
            designer,
            trace_path,
            session_id,
            AuthoringControlTarget::ActionTypeOption,
            None,
            AuthoringControlRole::Selectable,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;

        let target_rank = wait_for_action_catalog_rank(
            trace_path,
            session_id,
            target_action_index,
            TRACE_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        if target_rank.rank < 50 {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "fixture action source index {} was only rank {} of {} before search",
                    target_rank.custom_action_index, target_rank.rank, target_rank.catalog_len
                ),
            ));
        }
        let search = wait_for_authoring_control(
            trace_path,
            session_id,
            AuthoringControlTarget::ActionSearch,
            None,
            AuthoringControlRole::TextEdit,
            UIA_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        click_designer_client_bounds(child, designer, search.bounds, trace_path)
            .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        let query = format!("Radial Acceptance Harmless Action {target_action_index:03}");
        send_text_to_focused_window(child, designer, &query)
            .map_err(|error| authoring_case_failure(FailureStage::InputInjection, error))?;
        let action_row = wait_for_authoring_control(
            trace_path,
            session_id,
            AuthoringControlTarget::ActionRow,
            Some(target_action_index),
            AuthoringControlRole::Selectable,
            UIA_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        if !action_row.enabled {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "searched custom action row was not assignable".into(),
            ));
        }
        let action_click =
            click_designer_client_bounds(child, designer, action_row.bounds, trace_path).map_err(
                |error| authoring_case_failure(FailureStage::DesignerNativeTarget, error),
            )?;
        let assigned_row = wait_for_authoring_control_selected(
            trace_path,
            session_id,
            AuthoringControlTarget::ActionRow,
            Some(target_action_index),
            AuthoringControlRole::Selectable,
            true,
            TRACE_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        if assigned_row.index != Some(target_action_index) || !assigned_row.selected {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "native custom action click did not retain the exact selected source row".into(),
            ));
        }
        let _ = (
            entry_evidence,
            menu_click,
            menu_row,
            selector_click,
            ring_click,
            click,
            action_click,
        );
        Ok(format!(
            "evidence:v1; blank_cell_selected=true; geometry=[8,10]; cell_slot={cell_slot_index}; catalog_rank_gt_50={}; catalog_rank={}/{}; searched_action_assigned=true; source_index={}; menu_graph={}",
            target_rank.rank >= 50,
            target_rank.rank,
            target_rank.catalog_len,
            target_rank.custom_action_index,
            expected_graph.cell_ids_digest
        ))
    });

    append_authoring_case(report, child, output, trace_path, "A4", || {
        let cell_index = selected_canvas_cell_index.ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerReadiness,
                "A3 did not identify an authored cell for the Inspector handoff".into(),
            )
        })?;
        let cell_slot_index = selected_cell_slot_index.ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerReadiness,
                "A3 did not identify the authored slot within its ring for the Inspector handoff"
                    .into(),
            )
        })?;
        let row = wait_for_authoring_control_selected(
            trace_path,
            session_id,
            AuthoringControlTarget::ActionRow,
            Some(target_action_index),
            AuthoringControlRole::Selectable,
            true,
            TRACE_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        let before = latest_geometry_state(trace_path)
            .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?
            .filter(|state| state.session_id == session_id)
            .ok_or_else(|| {
                CaseFailure::new(
                    FailureStage::DesignerReadiness,
                    "Designer did not publish pre-handoff geometry state".into(),
                )
            })?;
        let cell_identity = before.selected_cell_id_digest.ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerReadiness,
                "selected authored cell did not expose its hashed stable identity before handoff"
                    .into(),
            )
        })?;
        click_authoring_target(
            child,
            designer,
            trace_path,
            session_id,
            AuthoringControlTarget::PopupOpenInspector,
            None,
            AuthoringControlRole::Button,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        click_authoring_target(
            child,
            designer,
            trace_path,
            session_id,
            AuthoringControlTarget::PopupApplyAndOpen,
            None,
            AuthoringControlRole::Button,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        let applied =
            wait_for_geometry_state_matching(trace_path, session_id, TRACE_TIMEOUT, |state| {
                state.generation > before.generation
                    && !state.proposal_active
                    && state.selected_cell_index == Some(cell_slot_index)
                    && state.selected_cell_id_digest == Some(cell_identity)
                    && state.selected_cell_custom_action_index_known
                    && state.selected_cell_custom_action_index == Some(target_action_index)
            })
            .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        let inspector = wait_for_authoring_control(
            trace_path,
            session_id,
            AuthoringControlTarget::InspectorCell,
            Some(cell_index),
            AuthoringControlRole::Region,
            UIA_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerPresentation, error))?;
        if !row.selected || !inspector.selected {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "Apply and open did not retain the authored binding/cell handoff: pre-apply action row selected={}, post-apply model cell={:?} identity={:?} action source={:?} (known={}), Inspector selected={}, expected cell/action={cell_index}/{target_action_index}",
                    row.selected,
                    applied.selected_cell_index,
                    applied.selected_cell_id_digest,
                    applied.selected_cell_custom_action_index,
                    applied.selected_cell_custom_action_index_known,
                    inspector.selected
                ),
            ));
        }
        authored_cell_identity = Some(cell_identity);
        action_mutation_generation = Some(applied.generation);
        Ok(format!(
            "dirty popup Apply and open advanced generation {} -> {}; production model retained outer-ring slot {} (CanvasCell global index {}) identity digest {} and resolves it to custom action source index {}; Inspector reported that same slot selected",
            before.generation,
            applied.generation,
            cell_slot_index,
            cell_index,
            cell_identity,
            target_action_index
        ))
    });

    append_authoring_case(report, child, output, trace_path, "A5", || {
        let skins = wait_for_designer_semantic_target_in_session(
            trace_path,
            DesignerSemanticTarget::Skins,
            session_id,
            UIA_TIMEOUT,
            |_| true,
        )
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerReadiness,
                "Skins mode did not publish a current semantic target".into(),
            )
        })?;
        let mode_click = if skins.selected {
            None
        } else {
            Some(
                click_designer_client_bounds(child, designer, skins.bounds, trace_path).map_err(
                    |error| authoring_case_failure(FailureStage::DesignerNativeTarget, error),
                )?,
            )
        };
        let selected_skins = wait_for_designer_semantic_target_in_session(
            trace_path,
            DesignerSemanticTarget::Skins,
            session_id,
            TRACE_TIMEOUT,
            |state| state.selected,
        )
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerMutation,
                "checked mode click did not select Skins".into(),
            )
        })?;
        let skin = wait_for_authoring_control(
            trace_path,
            session_id,
            AuthoringControlTarget::SkinRow,
            Some(0),
            AuthoringControlRole::Button,
            UIA_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        let skin_click = if skin.selected {
            None
        } else {
            Some(
                click_designer_client_bounds(child, designer, skin.bounds, trace_path).map_err(
                    |error| authoring_case_failure(FailureStage::DesignerNativeTarget, error),
                )?,
            )
        };
        wait_for_authoring_control_selected(
            trace_path,
            session_id,
            AuthoringControlTarget::SkinRow,
            Some(0),
            AuthoringControlRole::Button,
            true,
            TRACE_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        let glow = wait_for_unique_authoring_control_any_index(
            trace_path,
            session_id,
            AuthoringControlTarget::SkinGlowEnabled,
            AuthoringControlRole::Checkbox,
            UIA_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        if !glow.selected {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "deterministic Carbon skin glow fixture was not enabled before the edit".into(),
            ));
        }
        let before = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |_| true)
            .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        let preview_cursor = trace_lines(trace_path).len();
        let glow_click = click_designer_client_bounds(child, designer, glow.bounds, trace_path)
            .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        let after = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
            state.generation > before.generation
        })
        .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        let _disabled = wait_for_authoring_control_selected(
            trace_path,
            session_id,
            AuthoringControlTarget::SkinGlowEnabled,
            glow.index,
            AuthoringControlRole::Checkbox,
            false,
            TRACE_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        let preview_request = wait_for_authoring_request_generation(
            trace_path,
            preview_cursor,
            session_id,
            "PrepareEmbeddedPreview",
            after.generation,
            TRACE_TIMEOUT,
        )
        .ok_or_else(|| {
            authoring_case_failure(
                FailureStage::DesignerPresentation,
                "glow draft generation did not request a fresh embedded preview",
            )
        })?;
        let preview_reply = wait_trace(trace_path, preview_cursor, TRACE_TIMEOUT, |events| {
            events.iter().any(|line| {
                authoring_reply_matches(line, preview_request, "PrepareEmbeddedPreview")
            })
        })
        .into_iter()
        .find(|line| authoring_reply_matches(line, preview_request, "PrepareEmbeddedPreview"))
        .ok_or_else(|| {
            authoring_case_failure(
                FailureStage::DesignerPresentation,
                "glow-generation preview request did not receive its correlated accepted reply",
            )
        })?;
        // Skins mode replaces the canvas with the resource editor. Return to
        // Menus through the production mode selector before requiring visual
        // evidence that the edited preview generation rendered.
        let menus = wait_for_designer_semantic_target_in_session(
            trace_path,
            DesignerSemanticTarget::Menus,
            session_id,
            UIA_TIMEOUT,
            |_| true,
        )
        .ok_or_else(|| {
            authoring_case_failure(
                FailureStage::DesignerReadiness,
                "Menus mode target was not available after the glow edit",
            )
        })?;
        let menu_mode_click = if menus.selected {
            None
        } else {
            Some(
                click_designer_client_bounds(child, designer, menus.bounds, trace_path).map_err(
                    |error| authoring_case_failure(FailureStage::DesignerNativeTarget, error),
                )?,
            )
        };
        let _returned_to_menus = wait_for_designer_semantic_target_in_session(
            trace_path,
            DesignerSemanticTarget::Menus,
            session_id,
            TRACE_TIMEOUT,
            |state| state.selected,
        )
        .ok_or_else(|| {
            authoring_case_failure(
                FailureStage::DesignerMutation,
                "checked Menus mode click did not restore the preview canvas after the glow edit",
            )
        })?;
        let rendered_preview = wait_trace(trace_path, preview_cursor, TRACE_TIMEOUT, |events| {
            events.iter().any(|line| {
                line.contains("trace_event=\"designer_preview_rendered\"")
                    && trace_field_value(line, "session_id")
                        .and_then(|value| value.parse::<u64>().ok())
                        == Some(session_id)
                    && trace_field_value(line, "generation")
                        .and_then(|value| value.parse::<u64>().ok())
                        == Some(after.generation)
            })
        })
        .into_iter()
        .find(|line| {
            line.contains("trace_event=\"designer_preview_rendered\"")
                && trace_field_value(line, "session_id").and_then(|value| value.parse::<u64>().ok())
                    == Some(session_id)
                && trace_field_value(line, "generation").and_then(|value| value.parse::<u64>().ok())
                    == Some(after.generation)
        })
        .ok_or_else(|| {
            authoring_case_failure(
                FailureStage::DesignerPresentation,
                "Designer did not render a preview frame from the glow draft generation",
            )
        })?;
        style_mutation_generation = Some(after.generation);
        let _ = (
            selected_skins,
            mode_click,
            skin_click,
            glow_click,
            preview_reply,
            menus,
            menu_mode_click,
            rendered_preview,
        );
        Ok(format!(
            "evidence:v1; glow=true->false; generation={}->{}; preview_reply=accepted; preview_request={}; preview_generation={}; preview_rendered=true; render_session={session_id}",
            before.generation,
            after.generation,
            preview_request.request_id,
            preview_request.generation
        ))
    });

    let mut reopened_entry = None;
    let a6_started = Instant::now();
    let a6_result = (|| {
        let cell_identity = authored_cell_identity.ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerMutation,
                "Save is blocked until A4 proves the same authored cell retained its action binding".into(),
            )
        })?;
        let action_generation = action_mutation_generation.ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerMutation,
                "Save is blocked until A4 proves the action-binding draft mutation".into(),
            )
        })?;
        let style_generation = style_mutation_generation.ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerMutation,
                "Save is blocked until A5 proves the glow style draft mutation".into(),
            )
        })?;
        if style_generation <= action_generation {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "style mutation generation {style_generation} did not follow action-binding generation {action_generation}"
                ),
            ));
        }
        let menus = wait_for_designer_semantic_target_in_session(
            trace_path,
            DesignerSemanticTarget::Menus,
            session_id,
            UIA_TIMEOUT,
            |_| true,
        )
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerReadiness,
                "Menus mode did not publish before Save".into(),
            )
        })?;
        if !menus.selected {
            click_designer_client_bounds(child, designer, menus.bounds, trace_path).map_err(
                |error| authoring_case_failure(FailureStage::DesignerNativeTarget, error),
            )?;
        }
        wait_for_designer_semantic_target_in_session(
            trace_path,
            DesignerSemanticTarget::Menus,
            session_id,
            TRACE_TIMEOUT,
            |state| state.selected,
        )
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerMutation,
                "checked mode click did not select Menus before Save".into(),
            )
        })?;
        let expected_graph = authored_menu_graph.as_ref().ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerMutation,
                "A6 has no captured A0/G0/A2 menu graph to save".into(),
            )
        })?;
        let root_graph = overflow_root_graph.as_ref().ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerMutation,
                "A6 has no captured G1 overflow-root graph to save".into(),
            )
        })?;
        let before = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
            !state.proposal_active && state.menu_count > expected_graph.menu_index
        })
        .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        if before.menu_count <= expected_graph.menu_index {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "captured A0 menu index was not present in the live save draft".into(),
            ));
        }
        let menu_index = expected_graph.menu_index;
        let menu_row = wait_for_authoring_control(
            trace_path,
            session_id,
            AuthoringControlTarget::MenuRow,
            Some(menu_index),
            AuthoringControlRole::Selectable,
            UIA_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        let menu_click = if menu_row.selected {
            None
        } else {
            Some(
                click_designer_client_bounds(child, designer, menu_row.bounds, trace_path)
                    .map_err(|error| {
                        authoring_case_failure(FailureStage::DesignerNativeTarget, error)
                    })?,
            )
        };
        let menu_geometry =
            wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
                !state.proposal_active
                    && state.selected_menu_index == Some(menu_index)
                    && state.ring_count == expected_graph.ring_slots.len()
                    && state.draft_cell_ids_digest == expected_graph.cell_ids_digest
            })
            .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        if expected_graph.ring_slots != [8, 10] || expected_graph.populated_cells != 0 {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "A6 captured authored graph is not the A0/G0/A2 [8,10] menu: rings={:?}, initial populated={}",
                    expected_graph.ring_slots, expected_graph.populated_cells
                ),
            ));
        }
        if menu_geometry.selected_menu_after_action
            != Some(multi_launcher::radial::model::AfterActionPolicy::Inherit)
        {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "new authored menu had unexpected initial after-action policy {:?}",
                    menu_geometry.selected_menu_after_action
                ),
            ));
        }
        let (_, policy_combo_click) = click_authoring_target(
            child,
            designer,
            trace_path,
            session_id,
            AuthoringControlTarget::MenuAfterAction,
            None,
            AuthoringControlRole::ComboBox,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        let close_tree_option = wait_for_authoring_control(
            trace_path,
            session_id,
            AuthoringControlTarget::AfterActionOption,
            Some(3),
            AuthoringControlRole::Selectable,
            UIA_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        if close_tree_option.selected {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "new authored menu unexpectedly already selected Close tree".into(),
            ));
        }
        let policy_click =
            click_designer_client_bounds(child, designer, close_tree_option.bounds, trace_path)
                .map_err(|error| {
                    authoring_case_failure(FailureStage::DesignerNativeTarget, error)
                })?;
        let policy_applied =
            wait_for_geometry_state_matching(trace_path, session_id, TRACE_TIMEOUT, |state| {
                state.generation > menu_geometry.generation
                    && state.selected_menu_index == Some(menu_index)
                    && state.selected_menu_after_action
                        == Some(multi_launcher::radial::model::AfterActionPolicy::CloseTree)
            })
            .map_err(|error| {
                authoring_case_failure(
                    FailureStage::DesignerMutation,
                    format!("new menu did not retain the checked Close tree policy edit: {error}"),
                )
            })?;
        let ring_one_click = if policy_applied.selected_ring_index == Some(1) {
            None
        } else {
            click_authoring_target(
                child,
                designer,
                trace_path,
                session_id,
                AuthoringControlTarget::RingSelector,
                None,
                AuthoringControlRole::ComboBox,
            )
            .and_then(|_| {
                wait_for_authoring_control(
                    trace_path,
                    session_id,
                    AuthoringControlTarget::RingOption,
                    Some(1),
                    AuthoringControlRole::Selectable,
                    UIA_TIMEOUT,
                )
            })
            .and_then(|ring| click_designer_client_bounds(child, designer, ring.bounds, trace_path))
            .map(Some)
            .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?
        };
        let selected_authored_ring =
            wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
                !state.proposal_active
                    && state.selected_menu_index == Some(menu_index)
                    && state.selected_ring_index == Some(1)
                    && state.selected_ring_slots == 10
                    && state.menu_populated == 1
                    && state.draft_cell_ids_digest == expected_graph.cell_ids_digest
                    && state.selected_menu_after_action
                        == Some(multi_launcher::radial::model::AfterActionPolicy::CloseTree)
            })
            .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        let cell_index = selected_canvas_cell_index.ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerReadiness,
                "A3 did not identify the action cell for durable Save verification".into(),
            )
        })?;
        let cell_slot_index = selected_cell_slot_index.ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerReadiness,
                "A3 did not identify the persisted slot within its authored ring".into(),
            )
        })?;
        if selected_authored_ring.selected_ring_index != Some(1)
            || selected_authored_ring.selected_ring_slots <= cell_slot_index
        {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "authored menu geometry no longer contains the selected cell".into(),
            ));
        }
        let authored_cell = wait_for_canvas_cell_in_generation(
            trace_path,
            session_id,
            policy_applied.generation,
            cell_index,
            expected_graph.cell_ids_digest,
            1,
            cell_slot_index,
            UIA_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        if authored_cell.index != Some(cell_index) {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "selected authored menu did not publish expected canvas cell {cell_index}; observed {:?}",
                    authored_cell.index
                ),
            ));
        }
        let cell_click_cursor = trace_lines(trace_path).len();
        let cell_click =
            click_designer_client_bounds(child, designer, authored_cell.bounds, trace_path)
                .map_err(|error| {
                    authoring_case_failure(FailureStage::DesignerNativeTarget, error)
                })?;
        wait_for_authoring_control_click_finished(
            trace_path,
            cell_click_cursor,
            session_id,
            AuthoringControlTarget::CanvasCell,
            Some(cell_index),
            AuthoringControlRole::Region,
            TRACE_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        let saved_draft = wait_for_geometry_state_matching(
            trace_path,
            session_id,
            TRACE_TIMEOUT,
            |state| {
                state.generation >= policy_applied.generation
                    && state.generation >= style_generation
                    && state.generation > action_generation
                    && state.selected_cell_index == Some(cell_slot_index)
                    && state.selected_cell_id_digest == Some(cell_identity)
                    && state.selected_ring_index == Some(1)
                    && state.selected_ring_slots == 10
                    && state.menu_populated == 1
                    && state.draft_cell_ids_digest == expected_graph.cell_ids_digest
                    && state.selected_menu_after_action
                        == Some(multi_launcher::radial::model::AfterActionPolicy::CloseTree)
                    && state.selected_cell_custom_action_index_known
                    && state.selected_cell_custom_action_index == Some(target_action_index)
            },
        )
        .map_err(|error| {
            authoring_case_failure(
                FailureStage::DesignerMutation,
                format!(
                    "Save is blocked until the same selected cell proves the action and style draft mutations: {error}"
                ),
            )
        })?;
        let save = wait_for_authoring_control(
            trace_path,
            session_id,
            AuthoringControlTarget::Save,
            None,
            AuthoringControlRole::Button,
            UIA_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        if !save.enabled {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "Save was disabled for the valid authoring draft".into(),
            ));
        }
        let save_cursor = trace_lines(trace_path).len();
        let save_click = click_designer_client_bounds(child, designer, save.bounds, trace_path)
            .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        let saved = wait_trace(trace_path, save_cursor, UIA_TIMEOUT, |events| {
            events.iter().any(|line| {
                line.contains("trace_event=\"authoring\"")
                    && line.contains("edge=ReplyAccepted")
                    && line.contains("request_kind=CommitSave")
                    && line.contains("terminal=true")
            })
        });
        let Some(save_reply) = saved.iter().rev().find(|line| {
            line.contains("trace_event=\"authoring\"")
                && line.contains("edge=ReplyAccepted")
                && line.contains("request_kind=CommitSave")
                && line.contains("terminal=true")
        }) else {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "Save did not receive its terminal accepted CommitSave reply".into(),
            ));
        };
        if !wait_until(Duration::from_secs(4), || child.designer().is_none()) {
            return Err(CaseFailure::new(
                FailureStage::DesignerPresentation,
                format!("accepted Save did not close Designer: {save_reply}"),
            ));
        }
        if child.refresh_root().is_err()
            || child
                .try_wait()
                .map_err(|error| CaseFailure::new(FailureStage::Cleanup, error))?
                .is_some()
        {
            return Err(CaseFailure::new(
                FailureStage::DesignerPresentation,
                "Save also removed ROOT or terminated the candidate process".into(),
            ));
        }
        let persisted = verify_saved_authoring_fixture(
            profile,
            expected_graph,
            root_graph,
            cell_slot_index,
            target_action_index,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        let reopened = run_designer_entry(child, uia, anchor, trace_path)?;
        let reopened_menus = wait_for_designer_semantic_target_in_session(
            trace_path,
            DesignerSemanticTarget::Menus,
            reopened.session_id,
            UIA_TIMEOUT,
            |_| true,
        )
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerReadiness,
                "reopened Designer did not publish Menus mode".into(),
            )
        })?;
        if !reopened_menus.selected {
            click_designer_client_bounds(
                child,
                &reopened.window,
                reopened_menus.bounds,
                trace_path,
            )
            .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        }
        let row = wait_for_authoring_control_selected(
            trace_path,
            reopened.session_id,
            AuthoringControlTarget::MenuRow,
            Some(menu_index),
            AuthoringControlRole::Selectable,
            true,
            UIA_TIMEOUT,
        );
        let row = match row {
            Ok(row) => row,
            Err(_) => {
                let row = wait_for_authoring_control(
                    trace_path,
                    reopened.session_id,
                    AuthoringControlTarget::MenuRow,
                    Some(menu_index),
                    AuthoringControlRole::Selectable,
                    UIA_TIMEOUT,
                )
                .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
                click_designer_client_bounds(child, &reopened.window, row.bounds, trace_path)
                    .map_err(|error| {
                        authoring_case_failure(FailureStage::DesignerNativeTarget, error)
                    })?;
                wait_for_authoring_control_selected(
                    trace_path,
                    reopened.session_id,
                    AuthoringControlTarget::MenuRow,
                    Some(menu_index),
                    AuthoringControlRole::Selectable,
                    true,
                    TRACE_TIMEOUT,
                )
                .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?
            }
        };
        let reloaded =
            wait_for_geometry_state(trace_path, reopened.session_id, TRACE_TIMEOUT, |state| {
                state.selected_menu_index == Some(menu_index)
                    && state.ring_count == expected_graph.ring_slots.len()
                    && state.draft_cell_ids_digest == expected_graph.cell_ids_digest
                    && state.selected_menu_after_action
                        == Some(multi_launcher::radial::model::AfterActionPolicy::CloseTree)
            })
            .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        if reloaded.draft_cell_ids_digest != expected_graph.cell_ids_digest
            || reloaded.menu_populated != 1
        {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "Save/reopen changed the authored [8,10] menu identity graph or one action population".into(),
            ));
        }
        if reloaded.selected_ring_index != Some(1) {
            let _ = click_authoring_target(
                child,
                &reopened.window,
                trace_path,
                reopened.session_id,
                AuthoringControlTarget::RingSelector,
                None,
                AuthoringControlRole::ComboBox,
            )
            .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
            let ring = wait_for_authoring_control(
                trace_path,
                reopened.session_id,
                AuthoringControlTarget::RingOption,
                Some(1),
                AuthoringControlRole::Selectable,
                UIA_TIMEOUT,
            )
            .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
            click_designer_client_bounds(child, &reopened.window, ring.bounds, trace_path)
                .map_err(|error| {
                    authoring_case_failure(FailureStage::DesignerNativeTarget, error)
                })?;
        }
        let reloaded_ring =
            wait_for_geometry_state(trace_path, reopened.session_id, TRACE_TIMEOUT, |state| {
                state.selected_menu_index == Some(menu_index)
                    && state.selected_ring_index == Some(1)
                    && state.selected_ring_slots == 10
                    && state.menu_populated == 1
                    && state.draft_cell_ids_digest == expected_graph.cell_ids_digest
                    && state.selected_menu_after_action
                        == Some(multi_launcher::radial::model::AfterActionPolicy::CloseTree)
            })
            .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        let reloaded_canvas_cell = wait_for_canvas_cell_in_generation(
            trace_path,
            reopened.session_id,
            reloaded_ring.generation,
            cell_index,
            expected_graph.cell_ids_digest,
            1,
            cell_slot_index,
            UIA_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        if reloaded_canvas_cell.index != Some(cell_index) {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "reopened authored menu did not publish the same ring-scoped CanvasCell index {cell_index}; observed {:?}",
                    reloaded_canvas_cell.index
                ),
            ));
        }
        let reloaded_cell_click_cursor = trace_lines(trace_path).len();
        let reloaded_cell_click = click_designer_client_bounds(
            child,
            &reopened.window,
            reloaded_canvas_cell.bounds,
            trace_path,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        wait_for_authoring_control_click_finished(
            trace_path,
            reloaded_cell_click_cursor,
            reopened.session_id,
            AuthoringControlTarget::CanvasCell,
            Some(cell_index),
            AuthoringControlRole::Region,
            TRACE_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        wait_for_authoring_control_selected(
            trace_path,
            reopened.session_id,
            AuthoringControlTarget::CanvasCell,
            Some(cell_index),
            AuthoringControlRole::Region,
            true,
            TRACE_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        let reloaded_cell =
            wait_for_geometry_state(trace_path, reopened.session_id, TRACE_TIMEOUT, |state| {
                state.selected_menu_index == Some(menu_index)
                    && state.selected_ring_index == Some(1)
                    && state.selected_ring_slots == 10
                    && state.menu_populated == 1
                    && state.draft_cell_ids_digest == expected_graph.cell_ids_digest
                    && state.selected_cell_index == Some(cell_slot_index)
                    && state.selected_cell_id_digest == Some(cell_identity)
                    && state.selected_cell_custom_action_index_known
                    && state.selected_cell_custom_action_index == Some(target_action_index)
            })
            .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        let root_row = wait_for_authoring_control(
            trace_path,
            reopened.session_id,
            AuthoringControlTarget::MenuRow,
            Some(root_graph.menu_index),
            AuthoringControlRole::Selectable,
            UIA_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        if !root_row.selected {
            click_designer_client_bounds(child, &reopened.window, root_row.bounds, trace_path)
                .map_err(|error| {
                    authoring_case_failure(FailureStage::DesignerNativeTarget, error)
                })?;
        }
        let reloaded_root_menu =
            wait_for_geometry_state(trace_path, reopened.session_id, TRACE_TIMEOUT, |state| {
                state.selected_menu_index == Some(root_graph.menu_index)
                    && state.ring_count == root_graph.ring_slots.len()
                    && state.draft_cell_ids_digest == root_graph.cell_ids_digest
            })
            .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        let root_ring_click = if reloaded_root_menu.selected_ring_index == Some(0) {
            None
        } else {
            click_authoring_target(
                child,
                &reopened.window,
                trace_path,
                reopened.session_id,
                AuthoringControlTarget::RingSelector,
                None,
                AuthoringControlRole::ComboBox,
            )
            .and_then(|_| {
                wait_for_authoring_control(
                    trace_path,
                    reopened.session_id,
                    AuthoringControlTarget::RingOption,
                    Some(0),
                    AuthoringControlRole::Selectable,
                    UIA_TIMEOUT,
                )
            })
            .and_then(|ring| {
                click_designer_client_bounds(child, &reopened.window, ring.bounds, trace_path)
            })
            .map(Some)
            .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?
        };
        let reloaded_root =
            wait_for_geometry_state(trace_path, reopened.session_id, TRACE_TIMEOUT, |state| {
                state.selected_menu_index == Some(root_graph.menu_index)
                    && state.ring_count == root_graph.ring_slots.len()
                    && state.selected_ring_index == Some(0)
                    && state.selected_ring_slots == 8
                    && state.menu_populated == root_graph.populated_cells
                    && state.draft_cell_ids_digest == root_graph.cell_ids_digest
            })
            .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        if root_graph.ring_slots != [8, 1] || root_graph.populated_cells != 9 {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "G1 expected persisted root graph is not [8,1] / 9 populated: rings={:?}, populated={}",
                    root_graph.ring_slots, root_graph.populated_cells
                ),
            ));
        }
        verify_no_leaf_side_effects(profile, trace_path, &side_effect_baseline)
            .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        reopened_entry = Some(reopened);
        let _ = (
            persisted,
            menu_click,
            ring_one_click,
            policy_combo_click,
            policy_click,
            cell_click,
            save_click,
            row,
            reloaded_cell_click,
            root_row,
            root_ring_click,
            reloaded_root,
        );
        Ok(format!(
            "evidence:v1; typed_radial=decoded; authored_geometry=[8,10]; action_binding=true; cell_index={cell_index}; cell_identity={cell_identity}; action_index={target_action_index}; after_action=close_tree; overflow_root=[8,1]/9; root_ids={}; glow=false; reopened=true; save_generation={}; reopened_generation={}",
            root_graph.cell_ids_digest, saved_draft.generation, reloaded_cell.generation
        ))
    })();
    append_case(
        report,
        "A6",
        expected("A6"),
        a6_started,
        a6_result,
        Some(child),
        output,
        trace_path,
    );
    reopened_entry
}

fn verify_saved_authoring_fixture(
    profile: &Path,
    authored_graph: &PersistedMenuGraphExpectation,
    root_graph: &PersistedMenuGraphExpectation,
    cell_index: usize,
    target_action_index: usize,
) -> Result<String, String> {
    let radial_bytes = fs::read(profile.join("radial.json"))
        .map_err(|error| format!("read saved radial.json: {error}"))?;
    let decoded = multi_launcher::radial::migration::decode_document(&radial_bytes)
        .map_err(|error| format!("typed decode saved radial.json: {error:?}"))?;
    let actions_bytes = fs::read(profile.join("actions.json"))
        .map_err(|error| format!("read deterministic action fixture: {error}"))?;
    let actions = serde_json::from_slice::<Vec<multi_launcher::actions::Action>>(&actions_bytes)
        .map_err(|error| format!("decode deterministic action fixture: {error}"))?;
    let expected_action = actions
        .get(target_action_index)
        .ok_or_else(|| "target action index was absent from deterministic fixture".to_string())?;
    let menu = decoded
        .document
        .menus
        .get(authored_graph.menu_index)
        .ok_or_else(|| "saved document omitted the authored menu".to_string())?;
    if menu.after_action != multi_launcher::radial::model::AfterActionPolicy::CloseTree {
        return Err(format!(
            "saved authored menu did not retain its checked Close tree after-action policy: {:?}",
            menu.after_action
        ));
    }
    let ring = menu
        .rings
        .get(1)
        .ok_or_else(|| "saved authored menu omitted its selected ring".to_string())?;
    let cell = ring
        .cells
        .get(cell_index)
        .ok_or_else(|| "saved authored ring omitted the selected cell".to_string())?;
    let authored_slots = menu
        .rings
        .iter()
        .map(|ring| ring.cells.len())
        .collect::<Vec<_>>();
    if authored_slots != authored_graph.ring_slots
        || count_menu_populated_cells(menu) != authored_graph.populated_cells + 1
    {
        return Err(format!(
            "saved A0/G0/A2 menu geometry/population changed: expected rings {:?} with {} blank cells plus the assigned action, observed rings {authored_slots:?} and {} populated cells",
            authored_graph.ring_slots,
            authored_graph.populated_cells,
            count_menu_populated_cells(menu)
        ));
    }
    let binding = match &cell.content {
        multi_launcher::radial::model::CellContent::Action {
            binding: multi_launcher::radial::model::ActionBinding::Persisted { action },
        } => action,
        _ => return Err("saved selected cell was not a persisted action binding".into()),
    };
    match binding.target.as_ref() {
        Some(multi_launcher::universal_actions::PersistableActionTargetRef::CustomAction {
            action,
        }) if action == expected_action => {}
        _ => {
            return Err(
                "saved action binding did not match the exact target fixture source row".into(),
            );
        }
    }
    if binding.action_id.as_str().is_empty() {
        return Err("saved action binding omitted its semantic action ID".into());
    }
    validate_menu_identity_graph(menu, "authored")?;

    let root_menu = decoded
        .document
        .menus
        .get(root_graph.menu_index)
        .ok_or_else(|| "saved document omitted the G1 starter root menu".to_string())?;
    let root_slots = root_menu
        .rings
        .iter()
        .map(|ring| ring.cells.len())
        .collect::<Vec<_>>();
    if root_slots != root_graph.ring_slots
        || count_menu_populated_cells(root_menu) != root_graph.populated_cells
    {
        return Err(format!(
            "saved G1 root graph did not preserve overflow resolution: expected rings {:?} / {} populated cells, observed {root_slots:?} / {}",
            root_graph.ring_slots,
            root_graph.populated_cells,
            count_menu_populated_cells(root_menu)
        ));
    }
    validate_menu_identity_graph(root_menu, "G1 root")?;
    if !matches!(
        decoded
            .document
            .skins
            .first()
            .map(|skin| &skin.style.values.effects.glow_enabled),
        Some(multi_launcher::radial::model::Override::Value(false))
    ) {
        return Err("saved Carbon skin did not retain the harmless glow=false style edit".into());
    }
    Ok(format!(
        "typed radial.json decoded the same A0/G0/A2 authored menu index {} with geometry {:?}, {} populated action cells including exact fixture action index {target_action_index}, stable authored ID graph, and persisted Close tree policy; G1 starter root index {} retained resolved geometry {:?}, {} populated cells and stable menu/ring/cell IDs; Carbon glow=false persisted",
        authored_graph.menu_index,
        authored_slots,
        count_menu_populated_cells(menu),
        root_graph.menu_index,
        root_slots,
        count_menu_populated_cells(root_menu)
    ))
}

fn count_menu_populated_cells(menu: &multi_launcher::radial::model::MenuDefinition) -> usize {
    menu.rings
        .iter()
        .flat_map(|ring| &ring.cells)
        .filter(|cell| {
            !matches!(
                &cell.content,
                multi_launcher::radial::model::CellContent::Spacer
            )
        })
        .count()
}

fn validate_menu_identity_graph(
    menu: &multi_launcher::radial::model::MenuDefinition,
    label: &str,
) -> Result<(), String> {
    if menu.id.as_str().is_empty() {
        return Err(format!("saved {label} menu had an empty stable ID"));
    }
    let mut ring_ids = std::collections::BTreeSet::new();
    let mut cell_ids = std::collections::BTreeSet::new();
    for ring in &menu.rings {
        if ring.id.as_str().is_empty() || !ring_ids.insert(ring.id.as_str()) {
            return Err(format!("saved {label} ring IDs were empty or repeated"));
        }
        for cell in &ring.cells {
            if cell.id.as_str().is_empty() || !cell_ids.insert(cell.id.as_str()) {
                return Err(format!("saved {label} cell IDs were empty or repeated"));
            }
        }
    }
    Ok(())
}

fn capture_action_side_effect_baseline(
    profile: &Path,
    trace_path: &Path,
) -> Result<ActionSideEffectBaseline, String> {
    let history_path = profile.join("history.json");
    let history = match fs::read(&history_path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(format!("read pre-A3 history baseline: {error}")),
    };
    let trace = fs::read_to_string(trace_path)
        .map_err(|error| format!("read pre-A3 leaf-dispatch trace baseline: {error}"))?;
    let trace_event_count = trace
        .lines()
        .filter(|line| line.contains("trace_event=\""))
        .count();
    Ok(ActionSideEffectBaseline {
        history,
        trace_event_count,
    })
}

fn verify_no_leaf_side_effects(
    profile: &Path,
    trace_path: &Path,
    baseline: &ActionSideEffectBaseline,
) -> Result<String, String> {
    let history_path = profile.join("history.json");
    let history_after = match fs::read(&history_path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(format!("read history after Designer workflow: {error}")),
    };
    if history_after != baseline.history {
        return Err("Design authoring changed the candidate history file".into());
    }
    let trace = fs::read_to_string(trace_path)
        .map_err(|error| format!("read full leaf-dispatch trace: {error}"))?;
    let event_lines = trace
        .lines()
        .filter(|line| line.contains("trace_event=\""))
        .collect::<Vec<_>>();
    if event_lines.len() < baseline.trace_event_count {
        return Err(format!(
            "trace lost events after baseline: {} before A3, {} now",
            baseline.trace_event_count,
            event_lines.len()
        ));
    }
    let after = &event_lines[baseline.trace_event_count..];
    if after
        .iter()
        .any(|line| line.contains("trace_event=\"radial_action\""))
    {
        return Err("Designer workflow emitted a real radial leaf action dispatch".into());
    }
    let mut dispatch_samples = 0usize;
    for line in after
        .iter()
        .filter(|line| line.contains("trace_event=\"native_preview_dispatch_count\""))
    {
        let count = trace_field(line, "count")
            .ok_or_else(|| "preview dispatch trace omitted its count".to_string())?
            .parse::<usize>()
            .map_err(|error| format!("parse preview dispatch count: {error}"))?;
        dispatch_samples = dispatch_samples.saturating_add(1);
        if count != 0 {
            return Err(format!(
                "Designer workflow observed a nonzero native preview leaf-dispatch count {count}"
            ));
        }
    }
    Ok(format!(
        "full trace from the pre-A3 baseline through this point contains no radial_action event; all {dispatch_samples} native preview dispatch samples are zero and history is byte-identical"
    ))
}

fn append_blocked_continued_designer_cases(
    report: &mut AcceptanceReport,
    cause: &CaseFailure,
    child: Option<&NativeChild>,
    output: &Path,
    trace_path: &Path,
) {
    if !report.cases.iter().any(|case| case.id == "A3") {
        append_case(
            report,
            "A3",
            expected("A3"),
            started_now(),
            Err(cause.clone()),
            child,
            output,
            trace_path,
        );
    }
    append_blocked_ids(
        report,
        &["A4", "A5", "A6", "A7", "A8", "D3", "D6", "D7"],
        cause,
        output,
        trace_path,
        "A3 post-geometry Designer entry failed",
    );
}

fn append_blocked_lifecycle_cases(report: &mut AcceptanceReport, output: &Path, trace_path: &Path) {
    let cause = CaseFailure::new(
        FailureStage::DesignerReadiness,
        "lifecycle sequence was not run because Save/reopen did not produce a ready Designer"
            .into(),
    );
    append_blocked_ids(
        report,
        &["A7", "A8", "D3", "D6", "D7"],
        &cause,
        output,
        trace_path,
        "Save/reopen did not produce a ready Designer",
    );
}

fn append_blocked_ids(
    report: &mut AcceptanceReport,
    ids: &[&str],
    cause: &CaseFailure,
    output: &Path,
    trace_path: &Path,
    reason: &str,
) {
    for id in ids {
        if report.cases.iter().any(|case| case.id == *id) {
            continue;
        }
        append_case(
            report,
            id,
            expected(id),
            started_now(),
            Err(CaseFailure::new(
                cause.stage,
                format!("not run because {reason}: {}", cause.message),
            )),
            None,
            output,
            trace_path,
        );
    }
}

fn run_designer_lifecycle_cases(
    report: &mut AcceptanceReport,
    child: &mut NativeChild,
    uia: &UiAutomation,
    anchor: &FocusAnchor,
    entry: &DesignerEntry,
    output: &Path,
    trace_path: &Path,
    profile: &Path,
    side_effect_baseline: &ActionSideEffectBaseline,
) {
    append_case(
        report,
        "D3",
        expected("D3"),
        started_now(),
        run_root_hidden_designer_case(child, uia, &entry.window, entry.session_id, trace_path),
        Some(child),
        output,
        trace_path,
    );

    append_authoring_case(report, child, output, trace_path, "A7", || {
        let skins = wait_for_designer_semantic_target_in_session(
            trace_path,
            DesignerSemanticTarget::Skins,
            entry.session_id,
            UIA_TIMEOUT,
            |_| true,
        )
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerReadiness,
                "Skins mode did not publish for Undo/Redo".into(),
            )
        })?;
        if !skins.selected {
            click_designer_client_bounds(child, &entry.window, skins.bounds, trace_path).map_err(
                |error| authoring_case_failure(FailureStage::DesignerNativeTarget, error),
            )?;
        }
        let glow = wait_for_unique_authoring_control_any_index(
            trace_path,
            entry.session_id,
            AuthoringControlTarget::SkinGlowEnabled,
            AuthoringControlRole::Checkbox,
            UIA_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        if glow.selected {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "saved glow=false value was not present before Undo".into(),
            ));
        }
        let before_edit =
            wait_for_geometry_state(trace_path, entry.session_id, TRACE_TIMEOUT, |_| true)
                .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        let edit_click =
            click_designer_client_bounds(child, &entry.window, glow.bounds, trace_path).map_err(
                |error| authoring_case_failure(FailureStage::DesignerNativeTarget, error),
            )?;
        let edited =
            wait_for_geometry_state(trace_path, entry.session_id, TRACE_TIMEOUT, |state| {
                state.generation > before_edit.generation
            })
            .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        wait_for_authoring_control_selected(
            trace_path,
            entry.session_id,
            AuthoringControlTarget::SkinGlowEnabled,
            glow.index,
            AuthoringControlRole::Checkbox,
            true,
            TRACE_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        let undo = click_authoring_target(
            child,
            &entry.window,
            trace_path,
            entry.session_id,
            AuthoringControlTarget::Undo,
            None,
            AuthoringControlRole::Button,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        let undone =
            wait_for_geometry_state(trace_path, entry.session_id, TRACE_TIMEOUT, |state| {
                state.generation > edited.generation
            })
            .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        let restored = wait_for_authoring_control_selected(
            trace_path,
            entry.session_id,
            AuthoringControlTarget::SkinGlowEnabled,
            glow.index,
            AuthoringControlRole::Checkbox,
            false,
            TRACE_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        let redo = click_authoring_target(
            child,
            &entry.window,
            trace_path,
            entry.session_id,
            AuthoringControlTarget::Redo,
            None,
            AuthoringControlRole::Button,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        let redone =
            wait_for_geometry_state(trace_path, entry.session_id, TRACE_TIMEOUT, |state| {
                state.generation > undone.generation
            })
            .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        let reapplied = wait_for_authoring_control_selected(
            trace_path,
            entry.session_id,
            AuthoringControlTarget::SkinGlowEnabled,
            glow.index,
            AuthoringControlRole::Checkbox,
            true,
            TRACE_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        let _ = (edit_click, undo.1, redo.1, restored, reapplied);
        Ok(format!(
            "evidence:v1; undo_restored=false; redo_restored=true; edit_generation={}; undo_generation={}; redo_generation={}",
            edited.generation, undone.generation, redone.generation
        ))
    });

    append_authoring_case(report, child, output, trace_path, "A8", || {
        let cursor = trace_lines(trace_path).len();
        let visible_hosts_before = visible_radial_host_windows(child);
        let (start_control, start_click) = click_authoring_target(
            child,
            &entry.window,
            trace_path,
            entry.session_id,
            AuthoringControlTarget::OpenDesktopPreview,
            None,
            AuthoringControlRole::Button,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        let start_reply = wait_for_terminal_authoring_request(
            trace_path,
            cursor,
            entry.session_id,
            "StartNativePreview",
            UIA_TIMEOUT,
        )
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerReadiness,
                "safe desktop preview did not receive its correlated same-session terminal start reply".into(),
            )
        })?;
        if !wait_until(TRACE_TIMEOUT, || {
            visible_radial_host_windows(child) != visible_hosts_before
        }) {
            return Err(CaseFailure::new(
                FailureStage::DesignerPresentation,
                "accepted safe preview start did not create a visible child-owned radial preview surface".into(),
            ));
        }
        let stop_cursor = trace_lines(trace_path).len();
        let (stop_control, tab_events, enter_events) = tab_and_activate_authoring_control(
            child,
            &entry.window,
            trace_path,
            entry.session_id,
            start_reply.identity.generation,
            AuthoringControlTarget::StopDesktopPreview,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerFrameworkInput, error))?;
        let stop_click = wait_for_authoring_control_clicked(
            trace_path,
            stop_cursor,
            entry.session_id,
            AuthoringControlTarget::StopDesktopPreview,
            TRACE_TIMEOUT,
        )
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerFrameworkInput,
                "checked focused Enter did not reach the production Stop desktop preview widget"
                    .into(),
            )
        })?;
        let stop_reply = wait_for_terminal_authoring_request(
            trace_path,
            stop_cursor,
            entry.session_id,
            "StopNativePreview",
            UIA_TIMEOUT,
        )
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerMutation,
                "safe desktop preview did not receive its correlated same-session terminal stop reply".into(),
            )
        })?;
        if !wait_until(Duration::from_secs(2), || {
            visible_radial_host_windows(child) == visible_hosts_before
        }) {
            return Err(CaseFailure::new(
                FailureStage::DesignerPresentation,
                "accepted safe preview stop left a visible child-owned radial preview surface"
                    .into(),
            ));
        }
        let dispatch_counts = trace_lines(trace_path)
            .into_iter()
            .skip(cursor)
            .filter(|line| {
                line.contains("trace_event=\"native_preview_dispatch_count\"")
                    && trace_field(line, "editor_session")
                        .is_some_and(|value| value == entry.session_id.to_string())
            })
            .filter_map(|line| trace_field(&line, "count")?.parse::<usize>().ok())
            .collect::<Vec<_>>();
        if dispatch_counts.len() < 2 || dispatch_counts.iter().any(|count| *count != 0) {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                format!(
                    "safe preview did not prove a zero-dispatch baseline and terminal count: observations={dispatch_counts:?}"
                ),
            ));
        }
        let no_side_effects =
            verify_no_leaf_side_effects(profile, trace_path, side_effect_baseline)
                .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        Ok(format!(
            "checked {:?} click=[{}] started safe preview with correlated request={} generation={}; {} checked Tab events focused the current enabled Stop desktop preview button {:?} and {} checked Enter events reached widget evidence [{}] and terminal request={} generation={}; native preview surface returned to its pre-start window set, dispatch count stayed zero across {} samples, and complete A3-through-A8 side-effect oracle passed: {no_side_effects}",
            start_control.target,
            start_click.describe(),
            start_reply.identity.request_id,
            start_reply.identity.generation,
            tab_events,
            stop_control.target,
            enter_events,
            stop_click,
            stop_reply.identity.request_id,
            stop_reply.identity.generation,
            dispatch_counts.len()
        ))
    });

    append_authoring_case(report, child, output, trace_path, "D6", || {
        let saved_radial = fs::read(profile.join("radial.json"))
            .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        let before = wait_for_geometry_state(trace_path, entry.session_id, TRACE_TIMEOUT, |_| true)
            .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        let prompt_cursor = trace_lines(trace_path).len();
        let alt_f4 = send_alt_f4(child, &entry.window)
            .map_err(|error| authoring_case_failure(FailureStage::InputInjection, error))?;
        let prompt = wait_trace(trace_path, prompt_cursor, TRACE_TIMEOUT, |events| {
            events
                .iter()
                .any(|line| designer_close_matches(line, entry.session_id, true, true))
        })
        .into_iter()
        .rev()
        .find(|line| designer_close_matches(line, entry.session_id, true, true))
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerMutation,
                format!("dirty Alt+F4 input ({alt_f4} events) did not open the close prompt"),
            )
        })?;
        let keep_cursor = trace_lines(trace_path).len();
        let (keep, keep_click) = click_authoring_target(
            child,
            &entry.window,
            trace_path,
            entry.session_id,
            AuthoringControlTarget::KeepEditing,
            None,
            AuthoringControlRole::Button,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerNativeTarget, error))?;
        let keep_clicked = wait_for_authoring_control_clicked(
            trace_path,
            keep_cursor,
            entry.session_id,
            AuthoringControlTarget::KeepEditing,
            TRACE_TIMEOUT,
        )
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerMutation,
                "native Keep Editing input did not reach the same session's current widget".into(),
            )
        })?;
        let kept = wait_trace(trace_path, keep_cursor, TRACE_TIMEOUT, |events| {
            events
                .iter()
                .any(|line| designer_close_matches(line, entry.session_id, false, true))
        });
        if !kept
            .iter()
            .any(|line| designer_close_matches(line, entry.session_id, false, true))
        {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "Keep Editing did not clear the prompt while retaining a dirty draft".into(),
            ));
        }
        if child.designer().is_none()
            || !uia.root_is_queryable(entry.window.hwnd, child.process_id())
            || child
                .try_wait()
                .map_err(|error| CaseFailure::new(FailureStage::Cleanup, error))?
                .is_some()
        {
            return Err(CaseFailure::new(
                FailureStage::DesignerPresentation,
                "Keep Editing did not retain the same live, queryable Designer and candidate"
                    .into(),
            ));
        }
        let glow = wait_for_unique_authoring_control_any_index(
            trace_path,
            entry.session_id,
            AuthoringControlTarget::SkinGlowEnabled,
            AuthoringControlRole::Checkbox,
            TRACE_TIMEOUT,
        )
        .map_err(|error| authoring_case_failure(FailureStage::DesignerMutation, error))?;
        if !glow.selected {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "Keep Editing did not retain the unsaved glow=true draft from Redo".into(),
            ));
        }
        let after_keep =
            wait_for_geometry_state(trace_path, entry.session_id, TRACE_TIMEOUT, |_| true)
                .map_err(|error| authoring_case_failure(FailureStage::DesignerReadiness, error))?;
        if after_keep.generation != before.generation {
            return Err(CaseFailure::new(
                FailureStage::DesignerMutation,
                "Keep Editing changed the retained authoring generation".into(),
            ));
        }
        let discard = discard_dirty_designer(child, &entry.window, trace_path, entry.session_id)
            .map_err(|error| authoring_case_failure(FailureStage::Cleanup, error))?;
        let discarded_radial = fs::read(profile.join("radial.json"))
            .map_err(|error| authoring_case_failure(FailureStage::Cleanup, error))?;
        if discarded_radial != saved_radial {
            return Err(CaseFailure::new(
                FailureStage::Cleanup,
                "explicit discard changed the saved radial.json instead of dropping the dirty style draft".into(),
            ));
        }
        let _ = (prompt, keep, keep_click, keep_clicked, glow, discard);
        Ok(format!(
            "evidence:v1; keep_editing=retained_dirty; draft_glow=true; generation={}; discard=saved_json_unchanged; same_designer=true",
            after_keep.generation
        ))
    });

    append_case(
        report,
        "D7",
        expected("D7"),
        started_now(),
        run_disposable_close_case(child, uia, anchor, trace_path, profile),
        Some(child),
        output,
        trace_path,
    );
}

fn run_root_hidden_designer_case(
    child: &mut NativeChild,
    uia: &UiAutomation,
    designer: &WindowSnapshot,
    session_id: u64,
    trace_path: &Path,
) -> Result<String, CaseFailure> {
    let initial = child
        .refresh_root()
        .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
    require_visible(&initial)
        .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
    let app_hook_thread = hook_service_thread_id(trace_path);
    let mut runner_observer = RunnerHookObserver::start().map_err(|error| {
        CaseFailure::new(
            FailureStage::InputInjection,
            format!("could not install independent D3 keyboard-hook observer: {error}"),
        )
    })?;
    let runner_thread = runner_observer.thread_id();
    let runner_liveness_before = thread_liveness(runner_thread);
    let runner_probe_before = runner_observer
        .pump_roundtrip(
            NEXT_HOOK_PUMP_PROBE_ID.fetch_add(1, Ordering::Relaxed),
            Duration::from_millis(500),
        )
        .map(|()| "acknowledged".to_string())
        .unwrap_or_else(|error| format!("failed: {error}"));
    let app_liveness_before = app_hook_thread
        .map(thread_liveness)
        .unwrap_or_else(|| "production hook thread id unavailable".into());
    let app_probe_before = app_hook_thread.map_or_else(
        || "production pre-hide pump probe unavailable: service thread id missing".into(),
        |thread_id| {
            probe_production_hook_pump(
                thread_id,
                child.process_id(),
                trace_path,
                Duration::from_millis(500),
            )
        },
    );
    child
        .focus_window(designer)
        .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
    focus_is_validated(designer.hwnd, child.process_id()).map_err(|error| {
        CaseFailure::new(
            FailureStage::InputInjection,
            format!("D3 hide requires the exact Designer foreground HWND/PID: {error}"),
        )
    })?;
    let mut cursor = trace_lines(trace_path).len();
    let drained_before_hide = runner_observer.drain_pending();
    let hide_input = child
        .send_f11(designer.hwnd, child.process_id(), TAP_TIME)
        .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
    let hide_runner = runner_observer.wait_for_vk(0x7A, TRACE_TIMEOUT);
    let hidden_events = wait_trace(trace_path, cursor, TRACE_TIMEOUT, |events| {
        tap_trace_complete(events, 1, false)
            || (has_trace(
                events,
                "hook_observed",
                &["vk=122", "down=true", "injected=true"],
            ) && has_trace(
                events,
                "hook_observed",
                &["vk=122", "down=false", "injected=true"],
            ))
    });
    if !tap_trace_complete(&hidden_events, 1, false) {
        let product_pair = has_trace(
            &hidden_events,
            "hook_observed",
            &["vk=122", "down=true", "injected=true"],
        ) && has_trace(
            &hidden_events,
            "hook_observed",
            &["vk=122", "down=false", "injected=true"],
        );
        let runner_pair = hide_runner.down_seen
            && hide_runner.up_seen
            && hide_runner.down_injected
            && hide_runner.up_injected;
        let app_liveness_after = app_hook_thread
            .map(thread_liveness)
            .unwrap_or_else(|| "production hook thread id unavailable".into());
        let app_probe_after = app_hook_thread.map_or_else(
            || "production post-hide pump probe unavailable: service thread id missing".into(),
            |thread_id| {
                probe_production_hook_pump(
                    thread_id,
                    child.process_id(),
                    trace_path,
                    Duration::from_millis(500),
                )
            },
        );
        let runner_probe_after = runner_observer
            .pump_roundtrip(
                NEXT_HOOK_PUMP_PROBE_ID.fetch_add(1, Ordering::Relaxed),
                Duration::from_millis(500),
            )
            .map(|()| "acknowledged".to_string())
            .unwrap_or_else(|error| format!("failed: {error}"));
        let actual_foreground = capture_foreground();
        let root_after_input = child.refresh_root().ok();
        let classification = if runner_pair && !product_pair {
            "independent runner hook observed the exact injected pair while the production callback did not; this localizes to production hook-chain delivery/lifetime"
        } else if !runner_pair && !product_pair {
            "neither independent nor production hook observed the pair; native input delivery/environment remains upstream"
        } else if product_pair {
            "production hook observed the pair but did not complete the short-tap/admission/visibility chain"
        } else {
            "production hook evidence was incomplete"
        };
        let _ = runner_observer.stop_and_report();
        return Err(CaseFailure::new(
            tap_trace_failure_stage(&hidden_events, false),
            format!(
                "Designer-focused hide input [{}] did not complete one short ROOT toggle: {}; classification={classification}; runner_observer=[{}], drained_before_hide={drained_before_hide}, runner_thread_before=[{}], runner_pump_before={runner_probe_before}, runner_thread_after=[{}], runner_pump_after={runner_probe_after}, production_thread_before=[{}], production_pump_before=[{}], production_thread_after=[{}], production_pump_after=[{}], foreground_after=HWND:{} PID:{}, ROOT_before={:?}, ROOT_after_input={:?}, hook_and_admission_trace={:?}",
                hide_input.describe(),
                input_trace_summary(&hidden_events),
                hide_runner.describe(),
                runner_liveness_before,
                thread_liveness(runner_thread),
                app_liveness_before,
                app_probe_before,
                app_liveness_after,
                app_probe_after,
                hwnd_id(actual_foreground.0),
                actual_foreground.1,
                initial.bounds,
                root_after_input
                    .as_ref()
                    .map(|root| (hwnd_id(root.hwnd), root.bounds)),
                hidden_events
                    .iter()
                    .filter(|line| {
                        line.contains("hook_observed")
                            || line.contains("hook_primary")
                            || line.contains("hook_admission")
                            || line.contains("configured_primary")
                            || line.contains("short_tap")
                            || line.contains("desired_visibility")
                            || line.contains("root_command")
                    })
                    .cloned()
                    .collect::<Vec<_>>()
            ),
        ));
    }
    if !(hide_runner.down_seen
        && hide_runner.up_seen
        && hide_runner.down_injected
        && hide_runner.up_injected)
    {
        let _ = runner_observer.stop_and_report();
        return Err(CaseFailure::new(
            FailureStage::InputInjection,
            format!(
                "production short-tap visibility trace completed, but independent runner observer did not prove injected F11 down/up: {}",
                hide_runner.describe()
            ),
        ));
    }
    let hide_hook_observed = has_trace(
        &hidden_events,
        "hook_observed",
        &["vk=122", "down=true", "injected=true"],
    ) && has_trace(
        &hidden_events,
        "hook_observed",
        &["vk=122", "down=false", "injected=true"],
    );
    if !hide_hook_observed {
        let _ = runner_observer.stop_and_report();
        return Err(CaseFailure::new(
            FailureStage::InputInjection,
            format!(
                "ROOT hide trace completed without production HookObserved down/up despite independent runner proof; events={:?}",
                hidden_events
            ),
        ));
    }
    let hidden = child
        .refresh_root()
        .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
    require_hidden(&hidden)
        .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
    let physical_displays = native_display_bounds()
        .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
    let app_liveness_after_hide = app_hook_thread
        .map(thread_liveness)
        .unwrap_or_else(|| "production hook thread id unavailable".into());
    let app_probe_after_hide = app_hook_thread.map_or_else(
        || "production post-hide pump probe unavailable: service thread id missing".into(),
        |thread_id| {
            probe_production_hook_pump(
                thread_id,
                child.process_id(),
                trace_path,
                Duration::from_millis(500),
            )
        },
    );
    let runner_liveness_after_hide = thread_liveness(runner_thread);
    let runner_probe_after_hide = runner_observer
        .pump_roundtrip(
            NEXT_HOOK_PUMP_PROBE_ID.fetch_add(1, Ordering::Relaxed),
            Duration::from_millis(500),
        )
        .map(|()| "acknowledged".to_string())
        .unwrap_or_else(|error| format!("failed: {error}"));
    if !uia.root_is_queryable(designer.hwnd, child.process_id()) {
        return Err(CaseFailure::new(
            FailureStage::DesignerNativeTarget,
            "Designer stopped answering UIA while ROOT was parked".into(),
        ));
    }
    let mode = wait_for_designer_semantic_target_in_session(
        trace_path,
        DesignerSemanticTarget::Skins,
        session_id,
        UIA_TIMEOUT,
        |_| true,
    )
    .ok_or_else(|| {
        CaseFailure::new(
            FailureStage::DesignerReadiness,
            "Designer did not publish Skins target while ROOT was hidden".into(),
        )
    })?;
    let mode_click = if mode.selected {
        None
    } else {
        Some(
            click_designer_client_bounds(child, designer, mode.bounds, trace_path)
                .map_err(|error| CaseFailure::new(FailureStage::DesignerNativeTarget, error))?,
        )
    };
    let _selected_mode = wait_for_designer_semantic_target_in_session(
        trace_path,
        DesignerSemanticTarget::Skins,
        session_id,
        TRACE_TIMEOUT,
        |state| state.selected,
    )
    .ok_or_else(|| {
        CaseFailure::new(
            FailureStage::DesignerFrameworkInput,
            "checked Designer pointer click did not select Skins while ROOT was hidden".into(),
        )
    })?;
    let visible_hosts_before = visible_radial_host_windows(child);
    let preview_cursor = trace_lines(trace_path).len();
    let (preview_control, preview_click) = click_authoring_target(
        child,
        designer,
        trace_path,
        session_id,
        AuthoringControlTarget::OpenDesktopPreview,
        None,
        AuthoringControlRole::Button,
    )
    .map_err(|error| CaseFailure::new(FailureStage::DesignerNativeTarget, error))?;
    let preview_start = wait_for_terminal_authoring_request(
        trace_path,
        preview_cursor,
        session_id,
        "StartNativePreview",
        UIA_TIMEOUT,
    )
    .ok_or_else(|| {
        CaseFailure::new(
            FailureStage::DesignerReadiness,
            "Designer accepted Skins input while ROOT was hidden but its safe preview request did not receive a correlated same-session service reply".into(),
        )
    })?;
    if !wait_until(TRACE_TIMEOUT, || {
        visible_radial_host_windows(child) != visible_hosts_before
    }) {
        return Err(CaseFailure::new(
            FailureStage::DesignerPresentation,
            "accepted D3 preview start did not create a visible child-owned radial preview surface"
                .into(),
        ));
    }
    let root_after_preview_start = require_parked_root_state(child, &hidden, &physical_displays)
        .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
    let stop_cursor = trace_lines(trace_path).len();
    let (stop_control, tab_events, enter_events) = tab_and_activate_authoring_control(
        child,
        designer,
        trace_path,
        session_id,
        preview_start.identity.generation,
        AuthoringControlTarget::StopDesktopPreview,
    )
    .map_err(|error| CaseFailure::new(FailureStage::DesignerFrameworkInput, error))?;
    let stop_click = wait_for_authoring_control_clicked(
        trace_path,
        stop_cursor,
        session_id,
        AuthoringControlTarget::StopDesktopPreview,
        TRACE_TIMEOUT,
    )
    .ok_or_else(|| {
        CaseFailure::new(
            FailureStage::DesignerFrameworkInput,
            "checked focused Enter did not reach the production Stop desktop preview widget while ROOT was hidden".into(),
        )
    })?;
    let preview_stop = wait_for_terminal_authoring_request(
        trace_path,
        stop_cursor,
        session_id,
        "StopNativePreview",
        UIA_TIMEOUT,
    )
    .ok_or_else(|| {
        CaseFailure::new(
            FailureStage::DesignerReadiness,
            "Designer accepted Stop desktop preview while ROOT was hidden but its same-session stop request did not receive a terminal service reply".into(),
        )
    })?;
    if !wait_until(Duration::from_secs(2), || {
        visible_radial_host_windows(child) == visible_hosts_before
    }) {
        return Err(CaseFailure::new(
            FailureStage::DesignerPresentation,
            "terminal Stop desktop preview reply left a visible child-owned radial preview window"
                .into(),
        ));
    }
    let root_after_preview_stop = require_parked_root_state(child, &hidden, &physical_displays)
        .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
    if !uia.root_is_queryable(designer.hwnd, child.process_id()) {
        return Err(CaseFailure::new(
            FailureStage::DesignerNativeTarget,
            "Designer lost its child-owned UIA root while ROOT was hidden".into(),
        ));
    }
    child
        .focus_window(designer)
        .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
    focus_is_validated(designer.hwnd, child.process_id()).map_err(|error| {
        CaseFailure::new(
            FailureStage::InputInjection,
            format!("D3 show requires the exact Designer foreground HWND/PID: {error}"),
        )
    })?;
    cursor = trace_lines(trace_path).len();
    let drained_before_show = runner_observer.drain_pending();
    let show_input = child
        .send_f11(designer.hwnd, child.process_id(), TAP_TIME)
        .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
    let show_runner = runner_observer.wait_for_vk(0x7A, TRACE_TIMEOUT);
    let show_events = wait_trace(trace_path, cursor, TRACE_TIMEOUT, |events| {
        tap_trace_complete(events, 1, true)
            || (has_trace(
                events,
                "hook_observed",
                &["vk=122", "down=true", "injected=true"],
            ) && has_trace(
                events,
                "hook_observed",
                &["vk=122", "down=false", "injected=true"],
            ))
    });
    if !tap_trace_complete(&show_events, 1, true) {
        let _ = runner_observer.stop_and_report();
        return Err(CaseFailure::new(
            tap_trace_failure_stage(&show_events, true),
            format!(
                "Designer-focused show input [{}] did not complete one short ROOT toggle: {}; independent observer=[{}], drained before show={drained_before_show}, production HookObserved pair={}, trace={:?}",
                show_input.describe(),
                input_trace_summary(&show_events),
                show_runner.describe(),
                has_trace(
                    &show_events,
                    "hook_observed",
                    &["vk=122", "down=true", "injected=true"],
                ) && has_trace(
                    &show_events,
                    "hook_observed",
                    &["vk=122", "down=false", "injected=true"],
                ),
                show_events
            ),
        ));
    }
    let show_hook_observed = has_trace(
        &show_events,
        "hook_observed",
        &["vk=122", "down=true", "injected=true"],
    ) && has_trace(
        &show_events,
        "hook_observed",
        &["vk=122", "down=false", "injected=true"],
    );
    if !show_hook_observed
        || !(show_runner.down_seen
            && show_runner.up_seen
            && show_runner.down_injected
            && show_runner.up_injected)
    {
        let _ = runner_observer.stop_and_report();
        return Err(CaseFailure::new(
            FailureStage::InputInjection,
            format!(
                "production show tap completed without both independent and product F11 pairs: runner=[{}], production_pair={show_hook_observed}, trace={:?}",
                show_runner.describe(),
                show_events
            ),
        ));
    }
    let app_liveness_after_show = app_hook_thread
        .map(thread_liveness)
        .unwrap_or_else(|| "production hook thread id unavailable".into());
    let app_probe_after_show = app_hook_thread.map_or_else(
        || "production post-show pump probe unavailable: service thread id missing".into(),
        |thread_id| {
            probe_production_hook_pump(
                thread_id,
                child.process_id(),
                trace_path,
                Duration::from_millis(500),
            )
        },
    );
    let runner_liveness_after_show = thread_liveness(runner_thread);
    let runner_probe_after_show = runner_observer
        .pump_roundtrip(
            NEXT_HOOK_PUMP_PROBE_ID.fetch_add(1, Ordering::Relaxed),
            Duration::from_millis(500),
        )
        .map(|()| "acknowledged".to_string())
        .unwrap_or_else(|error| format!("failed: {error}"));
    let shown = child
        .refresh_root()
        .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
    require_visible(&shown)
        .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
    if !uia.root_is_queryable(designer.hwnd, child.process_id())
        || child.try_wait().ok().flatten().is_some()
    {
        return Err(CaseFailure::new(
            FailureStage::DesignerNativeTarget,
            "Designer UIA/service or child process failed after ROOT show".into(),
        ));
    }
    let settled_cursor = trace_lines(trace_path).len();
    let extra_visibility = wait_trace(
        trace_path,
        settled_cursor,
        Duration::from_millis(250),
        |events| {
            events
                .iter()
                .any(|line| line.contains("trace_event=\"desired_visibility\""))
        },
    );
    if extra_visibility
        .iter()
        .any(|line| line.contains("trace_event=\"desired_visibility\""))
    {
        return Err(CaseFailure::new(
            FailureStage::NativeRootState,
            "unexpected additional ROOT visibility toggle followed the checked hide/show pair"
                .into(),
        ));
    }
    let observer_cleanup = runner_observer
        .stop_and_report()
        .map(|()| "independent D3 hook observer uninstalled cleanly".to_string())
        .unwrap_or_else(|error| format!("independent D3 observer cleanup failed: {error}"));
    if !observer_cleanup.ends_with("cleanly") {
        return Err(CaseFailure::new(FailureStage::Cleanup, observer_cleanup));
    }
    let _ = (
        hide_input,
        hidden,
        mode_click,
        preview_control,
        preview_click,
        root_after_preview_start,
        stop_control,
        tab_events,
        enter_events,
        stop_click,
        root_after_preview_stop,
        show_input,
        shown,
        runner_liveness_before,
        runner_probe_before,
        runner_liveness_after_hide,
        runner_probe_after_hide,
        runner_liveness_after_show,
        runner_probe_after_show,
        app_liveness_before,
        app_probe_before,
        app_liveness_after_hide,
        app_probe_after_hide,
        app_liveness_after_show,
        app_probe_after_show,
        observer_cleanup,
        extra_visibility,
    );
    Ok(format!(
        "evidence:v1; root_hidden=true; designer_responsive=true; preview_start=accepted; preview_stop=accepted; root_shown=true; hook_pairs=true; start_request={}; stop_request={}; no_extra_toggle=true",
        preview_start.identity.request_id, preview_stop.identity.request_id
    ))
}

fn run_disposable_close_case(
    child: &mut NativeChild,
    uia: &UiAutomation,
    anchor: &FocusAnchor,
    trace_path: &Path,
    profile: &Path,
) -> Result<String, CaseFailure> {
    let entry = run_designer_entry(child, uia, anchor, trace_path)?;
    let menus = wait_for_designer_semantic_target_in_session(
        trace_path,
        DesignerSemanticTarget::Menus,
        entry.session_id,
        UIA_TIMEOUT,
        |_| true,
    )
    .ok_or_else(|| {
        CaseFailure::new(
            FailureStage::DesignerReadiness,
            "fresh D7 Designer session did not publish its Menus mode target".into(),
        )
    })?;
    let menu_mode_click = if menus.selected {
        None
    } else {
        Some(
            click_designer_client_bounds(child, &entry.window, menus.bounds, trace_path)
                .map_err(|error| CaseFailure::new(FailureStage::DesignerNativeTarget, error))?,
        )
    };
    wait_for_designer_semantic_target_in_session(
        trace_path,
        DesignerSemanticTarget::Menus,
        entry.session_id,
        TRACE_TIMEOUT,
        |state| state.selected,
    )
    .ok_or_else(|| {
        CaseFailure::new(
            FailureStage::DesignerMutation,
            "checked D7 mode transition did not select Menus before preparing New Menu".into(),
        )
    })?;
    let baseline = wait_for_geometry_state(trace_path, entry.session_id, TRACE_TIMEOUT, |_| true)
        .map_err(|error| CaseFailure::new(FailureStage::DesignerReadiness, error))?;
    let mut hold = AcceptancePrepareHold::create(profile)
        .map_err(|error| CaseFailure::new(FailureStage::Environment, error))?;
    let preparation_cursor = trace_lines(trace_path).len();
    let (new_menu, new_menu_click) = click_authoring_target(
        child,
        &entry.window,
        trace_path,
        entry.session_id,
        AuthoringControlTarget::NewMenu,
        None,
        AuthoringControlRole::Button,
    )
    .map_err(|error| CaseFailure::new(FailureStage::DesignerNativeTarget, error))?;
    let changed = wait_for_geometry_state(trace_path, entry.session_id, TRACE_TIMEOUT, |state| {
        state.generation > baseline.generation && state.menu_count == baseline.menu_count + 1
    })
    .map_err(|error| CaseFailure::new(FailureStage::DesignerMutation, error))?;
    if !new_menu.enabled || changed.selected_menu_index != Some(changed.menu_count - 1) {
        return Err(CaseFailure::new(
            FailureStage::DesignerMutation,
            "checked D7 New Menu did not leave a selected dirty menu for preview preparation"
                .into(),
        ));
    }
    let request = wait_for_authoring_request_generation(
        trace_path,
        preparation_cursor,
        entry.session_id,
        "PrepareEmbeddedPreview",
        changed.generation,
        TRACE_TIMEOUT,
    )
    .ok_or_else(|| {
        CaseFailure::new(
            FailureStage::DesignerReadiness,
            "dirty New Menu selection did not enqueue its real PrepareEmbeddedPreview request"
                .into(),
        )
    })?;
    let held = wait_trace(trace_path, preparation_cursor, TRACE_TIMEOUT, |events| {
        events
            .iter()
            .any(|line| acceptance_prepare_gate_matches(line, request, "Held"))
    });
    let gate_held = held
        .iter()
        .find(|line| acceptance_prepare_gate_matches(line, request, "Held"))
        .cloned()
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerReadiness,
                "D7 service did not enter the bounded gate for the real preview request".into(),
            )
        })?;
    if held.iter().any(|line| {
        authoring_edge_matches(line, request, "PrepareEmbeddedPreview", "ReplyEnqueued")
    }) {
        return Err(CaseFailure::new(
            FailureStage::DesignerReadiness,
            "D7 preview service replied before the controlled pending interval began".into(),
        ));
    }
    let close_cursor = trace_lines(trace_path).len();
    let key_events = send_alt_f4(child, &entry.window)
        .map_err(|error| CaseFailure::new(FailureStage::InputInjection, error))?;
    let close_events = wait_trace(trace_path, close_cursor, TRACE_TIMEOUT, |events| {
        let cancelled = events
            .iter()
            .any(|line| disposable_cancel_matches(line, request, "PrepareEmbeddedPreview"));
        let close_prompt = events
            .iter()
            .any(|line| designer_close_matches(line, entry.session_id, true, true));
        cancelled && close_prompt
    });
    let cancelled = close_events
        .iter()
        .find(|line| disposable_cancel_matches(line, request, "PrepareEmbeddedPreview"))
        .cloned()
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerMutation,
                "close did not cancel the exact pending PrepareEmbeddedPreview request".into(),
            )
        })?;
    let close_state = close_events
        .iter()
        .find(|line| designer_close_matches(line, entry.session_id, true, true))
        .cloned()
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::DesignerReadiness,
                format!(
                    "Alt+F4 events={key_events} did not produce a same-session dirty close prompt"
                ),
            )
        })?;
    if trace_field_value(&close_state, "pending_disposable") != Some("false") {
        return Err(CaseFailure::new(
            FailureStage::DesignerMutation,
            "close state did not reflect local cancellation before prompt publication".into(),
        ));
    }
    let close_index = close_events
        .iter()
        .position(|line| designer_close_matches(line, entry.session_id, true, true))
        .unwrap_or(usize::MAX);
    let cancel_index = close_events
        .iter()
        .position(|line| disposable_cancel_matches(line, request, "PrepareEmbeddedPreview"))
        .unwrap_or(usize::MAX);
    if cancel_index >= close_index {
        return Err(CaseFailure::new(
            FailureStage::DesignerMutation,
            "request cancellation was not observed before the close prompt state".into(),
        ));
    }
    let release_cursor = trace_lines(trace_path).len();
    let release_evidence = hold
        .release()
        .map_err(|error| CaseFailure::new(FailureStage::Cleanup, error))?;
    let released_events = wait_trace(trace_path, release_cursor, TRACE_TIMEOUT, |events| {
        events
            .iter()
            .any(|line| acceptance_prepare_gate_matches(line, request, "Released"))
            && events.iter().any(|line| {
                authoring_edge_matches(line, request, "PrepareEmbeddedPreview", "ReplyEnqueued")
            })
    });
    let gate_released = released_events
        .iter()
        .find(|line| acceptance_prepare_gate_matches(line, request, "Released"))
        .cloned()
        .ok_or_else(|| {
            CaseFailure::new(
                FailureStage::Cleanup,
                "D7 service did not observe removal of the hold marker before its bounded timeout"
                    .into(),
            )
        })?;
    let late_reply = wait_for_authoring_edge(
        trace_path,
        release_cursor,
        request,
        "PrepareEmbeddedPreview",
        "ReplyRejected",
        TRACE_TIMEOUT,
    )
    .ok_or_else(|| {
        CaseFailure::new(
            FailureStage::DesignerMutation,
            "the late prepared-frame reply was not rejected after the close path cancelled it"
                .into(),
        )
    })?;
    let cursor = trace_lines(trace_path).len();
    let (discard, click) = click_authoring_target(
        child,
        &entry.window,
        trace_path,
        entry.session_id,
        AuthoringControlTarget::DiscardDraft,
        None,
        AuthoringControlRole::Button,
    )
    .map_err(|error| CaseFailure::new(FailureStage::DesignerNativeTarget, error))?;
    let stop = wait_for_terminal_authoring_request(
        trace_path,
        cursor,
        entry.session_id,
        "StopNativePreview",
        TRACE_TIMEOUT,
    )
    .ok_or_else(|| {
        CaseFailure::new(
            FailureStage::Cleanup,
            "D7 discard did not receive its same-session terminal preview-stop reply".into(),
        )
    })?;
    if !wait_until(Duration::from_secs(4), || child.designer().is_none()) {
        return Err(CaseFailure::new(
            FailureStage::DesignerPresentation,
            "Designer did not close after D7 terminal disposal".into(),
        ));
    }
    if child.refresh_root().is_err()
        || child
            .try_wait()
            .map_err(|error| CaseFailure::new(FailureStage::Cleanup, error))?
            .is_some()
    {
        return Err(CaseFailure::new(
            FailureStage::Cleanup,
            "D7 disposal closed ROOT or terminated the child process".into(),
        ));
    }
    let closed_observation = verify_designer_stays_closed_for(child, Duration::from_secs(1))
        .map_err(|error| CaseFailure::new(FailureStage::DesignerPresentation, error))?;
    if hold.path.exists() {
        return Err(CaseFailure::new(
            FailureStage::Cleanup,
            "D7 preview hold marker remained after the terminal path".into(),
        ));
    }
    let _ = (
        key_events,
        menu_mode_click,
        new_menu,
        new_menu_click,
        gate_held,
        cancelled,
        close_state,
        release_evidence,
        gate_released,
        late_reply,
        discard,
        click,
    );
    Ok(format!(
        "evidence:v1; pending_request=true; request_id={}; generation={}; cancelled_before_prompt=true; late_reply=rejected; stop=accepted; stop_request={}; no_reopen=1s ({closed_observation}); marker_clean=true; child_alive=true",
        request.request_id, changed.generation, stop.identity.request_id
    ))
}

fn wait_for_authoring_control_selected(
    trace_path: &Path,
    session_id: u64,
    target: AuthoringControlTarget,
    index: Option<usize>,
    role: AuthoringControlRole,
    selected: bool,
    timeout: Duration,
) -> Result<AuthoringControlSnapshot, String> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(control) = find_authoring_control(trace_path, session_id, target, index, role)?
            && control.selected == selected
        {
            return Ok(control);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "Designer target {target:?} index={index:?} did not become selected={selected} for session {session_id}"
            ));
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

fn wait_for_canvas_cell_in_generation(
    trace_path: &Path,
    session_id: u64,
    generation: u64,
    flat_index: usize,
    menu_cell_ids_digest: u64,
    ring_index: usize,
    slot_index: usize,
    timeout: Duration,
) -> Result<AuthoringControlSnapshot, String> {
    let deadline = Instant::now() + timeout;
    loop {
        let controls = list_authoring_controls(trace_path, session_id)?;
        if let Some(cell) = fresh_canvas_cell_for_generation(
            &controls,
            session_id,
            generation,
            flat_index,
            menu_cell_ids_digest,
            ring_index,
            slot_index,
        ) {
            return Ok(cell);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "selected blank menu did not publish a fresh authored spacer in session {session_id} generation {generation}"
            ));
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

fn flat_canvas_cell_index(
    ring_slots: &[usize],
    ring_index: usize,
    slot_index: usize,
) -> Option<usize> {
    let current_ring_slots = *ring_slots.get(ring_index)?;
    if slot_index >= current_ring_slots {
        return None;
    }
    ring_slots
        .iter()
        .take(ring_index)
        .try_fold(slot_index, |flat_index, slots| {
            flat_index.checked_add(*slots)
        })
}

fn wait_for_authoring_control_click_finished(
    trace_path: &Path,
    first_line: usize,
    session_id: u64,
    target: AuthoringControlTarget,
    index: Option<usize>,
    role: AuthoringControlRole,
    timeout: Duration,
) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    loop {
        let lines = trace_lines(trace_path);
        if authoring_control_click_finished(
            lines.get(first_line..).unwrap_or_default(),
            session_id,
            target,
            index,
            role,
        ) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "Designer target {target:?} index={index:?} did not publish a completed native click in session {session_id}"
            ));
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

fn wait_for_action_catalog_rank(
    trace_path: &Path,
    session_id: u64,
    custom_action_index: usize,
    timeout: Duration,
) -> Result<ActionCatalogRankSnapshot, String> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(rank) = action_catalog_ranks(trace_path)?.into_iter().find(|rank| {
            rank.session_id == session_id && rank.custom_action_index == custom_action_index
        }) {
            return Ok(rank);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "unfiltered catalog rank for custom action index {custom_action_index} was not published in session {session_id}"
            ));
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

fn run_populated_shrink_resolution(
    child: &NativeChild,
    designer: &WindowSnapshot,
    trace_path: &Path,
    session_id: u64,
) -> Result<String, String> {
    let root_row = wait_for_authoring_control(
        trace_path,
        session_id,
        AuthoringControlTarget::MenuRow,
        Some(0),
        AuthoringControlRole::Selectable,
        UIA_TIMEOUT,
    )?;
    let root_select = click_designer_client_bounds(child, designer, root_row.bounds, trace_path)?;
    let root = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
        !state.proposal_active
            && !state.resize_prompt_open
            && state.selected_menu_index == Some(0)
            && state.selected_ring_index == Some(0)
            && state.ring_count > 0
    })?;
    if root.selected_ring_slots <= 1 || root.selected_ring_populated == 0 {
        return Err(format!(
            "starter root ring is not suitable for a populated shrink: rings={} slots={} populated={}",
            root.ring_count, root.selected_ring_slots, root.selected_ring_populated
        ));
    }

    let requested = root.selected_ring_slots - 1;
    let slots_input = set_requested_slots(child, designer, trace_path, session_id, requested)?;
    let (_, preview_click) = click_authoring_target(
        child,
        designer,
        trace_path,
        session_id,
        AuthoringControlTarget::PreviewProposal,
        None,
        AuthoringControlRole::Button,
    )?;
    let prompted = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
        state.resize_prompt_open && state.resize_prompt_populated > 0
    })?;
    if prompted.selected_menu_index != Some(0)
        || prompted.selected_ring_index != Some(0)
        || prompted.selected_ring_slots != root.selected_ring_slots
        || prompted.selected_ring_populated != root.selected_ring_populated
        || prompted.menu_populated != root.menu_populated
        || prompted.requested_slots != requested
        || prompted.proposal_active
        || prompted.resize_prompt_populated > root.selected_ring_populated
    {
        return Err(format!(
            "populated shrink prompt changed committed state or described an impossible number of overflow cells: root slots/population/menu={}/{}/{}, prompt slots/population/menu={}/{}/{}, requested={}, prompt overflow cells={}",
            root.selected_ring_slots,
            root.selected_ring_populated,
            root.menu_populated,
            prompted.selected_ring_slots,
            prompted.selected_ring_populated,
            prompted.menu_populated,
            prompted.requested_slots,
            prompted.resize_prompt_populated
        ));
    }
    let (_, cancel_click) = click_authoring_target(
        child,
        designer,
        trace_path,
        session_id,
        AuthoringControlTarget::CancelResolution,
        None,
        AuthoringControlRole::Button,
    )?;
    let cancelled = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
        !state.resize_prompt_open && !state.proposal_active && state.requested_slots == requested
    })?;
    if cancelled.selected_ring_slots != root.selected_ring_slots
        || cancelled.selected_ring_populated != root.selected_ring_populated
        || cancelled.menu_populated != root.menu_populated
        || cancelled.ring_count != root.ring_count
    {
        return Err(format!(
            "Cancel changed committed geometry or removed content: slots {}/{} population {}/{} menu population {}/{} rings {}/{}",
            root.selected_ring_slots,
            cancelled.selected_ring_slots,
            root.selected_ring_populated,
            cancelled.selected_ring_populated,
            root.menu_populated,
            cancelled.menu_populated,
            root.ring_count,
            cancelled.ring_count
        ));
    }

    let slots_again = set_requested_slots(child, designer, trace_path, session_id, requested)?;
    let (_, preview_again) = click_authoring_target(
        child,
        designer,
        trace_path,
        session_id,
        AuthoringControlTarget::PreviewProposal,
        None,
        AuthoringControlRole::Button,
    )?;
    let prompted_again = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
        state.resize_prompt_open && state.resize_prompt_populated > 0
    })?;
    if prompted_again.menu_populated != root.menu_populated
        || prompted_again.selected_ring_slots != root.selected_ring_slots
        || prompted_again.resize_prompt_populated != prompted.resize_prompt_populated
    {
        return Err("reopening populated shrink resolution changed committed content".into());
    }
    let (_, overflow_click) = click_authoring_target(
        child,
        designer,
        trace_path,
        session_id,
        AuthoringControlTarget::MoveToOverflow,
        None,
        AuthoringControlRole::Button,
    )?;
    let resolved = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
        state.proposal_active
            && state.proposal_kind == AuthoringProposalKind::ResolvedResize
            && state.proposal_ready
    })?;
    if resolved.resize_prompt_open
        || resolved.ring_count != root.ring_count
        || resolved.selected_ring_slots != root.selected_ring_slots
        || resolved.menu_populated != root.menu_populated
        || resolved.proposal_candidate_rings != root.ring_count + 1
        || resolved.proposal_slots != requested
        || resolved.proposal_resolution_populated != prompted.resize_prompt_populated
        || !resolved.proposal_cell_ids_preserved
    {
        return Err(format!(
            "overflow resolution did not prepare a preserving candidate: rings={} committed_slots={} menu_populated={} candidate_rings={} proposal_slots={} resolved_populated={} original_populated={} stable_ids_preserved={} ready={}",
            resolved.ring_count,
            resolved.selected_ring_slots,
            resolved.menu_populated,
            resolved.proposal_candidate_rings,
            resolved.proposal_slots,
            resolved.proposal_resolution_populated,
            prompted.resize_prompt_populated,
            resolved.proposal_cell_ids_preserved,
            resolved.proposal_ready
        ));
    }
    let apply = wait_for_authoring_control(
        trace_path,
        session_id,
        AuthoringControlTarget::ApplyProposal,
        None,
        AuthoringControlRole::Button,
        UIA_TIMEOUT,
    )?;
    if !apply.enabled {
        return Err(
            "Apply proposal remained disabled after overflow resolution was prepared".into(),
        );
    }
    let apply_click = click_designer_client_bounds(child, designer, apply.bounds, trace_path)?;
    let applied = wait_for_geometry_state(trace_path, session_id, TRACE_TIMEOUT, |state| {
        !state.proposal_active
            && state.ring_count == root.ring_count + 1
            && state.selected_menu_index == Some(0)
            && state.selected_ring_index == Some(0)
            && state.selected_ring_slots == requested
            && state.generation > resolved.generation
    })?;
    let post_apply_ids_preserved = committed_cell_ids_match(
        resolved.proposal_cell_ids_digest_available,
        resolved.proposal_cell_ids_digest,
        applied.draft_cell_ids_digest,
    );
    if applied.menu_populated != root.menu_populated
        || applied.selected_ring_populated >= root.selected_ring_populated
        || !post_apply_ids_preserved
    {
        return Err(format!(
            "overflow Apply did not preserve menu content and prepared stable cell IDs while shrinking the selected ring: menu populated {} -> {}, selected ring populated {} -> {}, stable cell IDs match candidate={}",
            root.menu_populated,
            applied.menu_populated,
            root.selected_ring_populated,
            applied.selected_ring_populated,
            post_apply_ids_preserved
        ));
    }
    Ok(format!(
        "selected starter root through MenuRow 0 {:?} with checked click=[{}]; {slots_input}; Preview=[{}] retained {} slots, {} populated cells, menu population {} while showing a {}-cell resolution prompt; Cancel=[{}] preserved all counts; {slots_again}; reopened Preview=[{}], Move to overflow=[{}] prepared {} slots and {} rings with {} populated cells resolved and all prior cell IDs preserved; Apply enabled={}, checked click=[{}] yielded {} rings, {} slots, {} selected-ring populated cells and unchanged menu population {}; committed stable cell IDs match the prepared candidate={}",
        root_row.bounds,
        root_select.describe(),
        preview_click.describe(),
        root.selected_ring_slots,
        root.selected_ring_populated,
        root.menu_populated,
        prompted.resize_prompt_populated,
        cancel_click.describe(),
        preview_again.describe(),
        overflow_click.describe(),
        resolved.proposal_slots,
        resolved.proposal_candidate_rings,
        resolved.proposal_resolution_populated,
        apply.enabled,
        apply_click.describe(),
        applied.ring_count,
        applied.selected_ring_slots,
        applied.selected_ring_populated,
        applied.menu_populated,
        post_apply_ids_preserved
    ))
}

fn run_compact_geometry_case(
    child: &NativeChild,
    designer: &WindowSnapshot,
    trace_path: &Path,
    session_id: u64,
) -> Result<String, String> {
    let post_resize_trace_cursor = trace_lines(trace_path).len();
    child.resize_window(designer, 640, 480)?;
    let deadline = Instant::now() + TRACE_TIMEOUT;
    let (compact, client) = loop {
        let compact = child
            .designer()
            .ok_or_else(|| "Designer HWND disappeared during compact resize".to_string())?;
        if compact.hwnd != designer.hwnd {
            return Err("Designer HWND changed during compact resize".into());
        }
        let client = child.client_bounds(&compact)?;
        let outer_width = compact.bounds[2] - compact.bounds[0];
        let outer_height = compact.bounds[3] - compact.bounds[1];
        if compact.visible
            && !compact.minimized
            && compact.is_nonzero()
            && outer_width <= 700
            && outer_height <= 540
            && client[2] - client[0] >= 520
            && client[3] - client[1] >= 380
        {
            break (compact, client);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "compact Designer did not reach visible bounded outer/client geometry: outer={:?} client={client:?}",
                compact.bounds
            ));
        }
        std::thread::sleep(WINDOW_POLL);
    };
    // Authoring-control events carry the client size seen by that egui frame. The
    // pre-resize cursor retains the resize-triggered frame even if it is rendered
    // before USER's refreshed HWND snapshot becomes observable; only a frame matching
    // the compact native client size can satisfy the check.
    let targets = [
        (
            AuthoringControlTarget::NewMenu,
            None,
            AuthoringControlRole::Button,
        ),
        (
            AuthoringControlTarget::AddRing,
            None,
            AuthoringControlRole::Button,
        ),
        (
            AuthoringControlTarget::RingSelector,
            None,
            AuthoringControlRole::ComboBox,
        ),
        (
            AuthoringControlTarget::Slots,
            None,
            AuthoringControlRole::DragValue,
        ),
        (
            AuthoringControlTarget::PreviewProposal,
            None,
            AuthoringControlRole::Button,
        ),
        (
            AuthoringControlTarget::Canvas,
            None,
            AuthoringControlRole::Region,
        ),
    ];
    let mut controls = Vec::with_capacity(targets.len());
    for (target, index, role) in targets {
        let control = wait_for_authoring_control_after(
            trace_path,
            post_resize_trace_cursor,
            session_id,
            [client[2] - client[0], client[3] - client[1]],
            target,
            index,
            role,
            TRACE_TIMEOUT,
        )?;
        let [left, top, right, bottom] = control.bounds;
        if left < client[0] || top < client[1] || right > client[2] || bottom > client[3] {
            let allocation = designer_canvas_allocations_after(
                trace_path,
                post_resize_trace_cursor,
                session_id,
            )?
            .into_iter()
            .rev()
            .find(|allocation| allocation.requested_size[1] > 0);
            let allocation_detail = allocation.map_or_else(
                || "Canvas allocation/clip snapshot missing".to_string(),
                |allocation| {
                    format!(
                        "allocated={:?} clip={:?} requested_px={:?}",
                        allocation.allocated_rect, allocation.clip_rect, allocation.requested_size
                    )
                },
            );
            return Err(format!(
                "compact Designer target {target:?} bounds {:?} exceed client bounds {client:?}; egui {allocation_detail}",
                control.bounds,
            ));
        }
        if right <= left || bottom <= top {
            return Err(format!(
                "compact Designer target {target:?} has empty bounds {:?}",
                control.bounds
            ));
        }
        controls.push((target, control.bounds));
    }
    let canvas = controls
        .iter()
        .find_map(|(target, bounds)| (*target == AuthoringControlTarget::Canvas).then_some(*bounds))
        .ok_or_else(|| "compact Designer did not publish its canvas target".to_string())?;
    if canvas[2] - canvas[0] < 100 || canvas[3] - canvas[1] < 100 {
        return Err(format!(
            "compact Designer left too little usable canvas area: {canvas:?}"
        ));
    }
    let current = child
        .designer()
        .ok_or_else(|| "Designer HWND disappeared while validating compact geometry".to_string())?;
    let current_client = child.client_bounds(&current)?;
    if current.hwnd != compact.hwnd
        || !current.visible
        || current.minimized
        || !current.is_nonzero()
        || !client_size_matches(
            [
                current_client[2] - current_client[0],
                current_client[3] - current_client[1],
            ],
            [client[2] - client[0], client[3] - client[1]],
        )
        || current.bounds[2] - current.bounds[0] > 700
        || current.bounds[3] - current.bounds[1] > 540
    {
        return Err(format!(
            "compact Designer window changed after fresh controls were rendered: {:?}",
            current.bounds,
        ));
    }
    Ok(format!(
        "resized the checked child Designer HWND to compact outer bounds {:?}; client={client:?}; all authoring controls and canvas stayed inside the client; canvas={canvas:?} ({}x{}); inspected {} semantic targets",
        compact.bounds,
        canvas[2] - canvas[0],
        canvas[3] - canvas[1],
        controls.len()
    ))
}

fn restore_authoring_viewport(
    child: &NativeChild,
    original: &WindowSnapshot,
    trace_path: &Path,
    session_id: u64,
) -> Result<String, String> {
    let width = original.bounds[2] - original.bounds[0];
    let height = original.bounds[3] - original.bounds[1];
    if width < 700 || height < 540 {
        return Err(format!(
            "original Designer viewport is too small for the retained menu list: {:?}",
            original.bounds
        ));
    }
    let first_line = trace_lines(trace_path).len();
    child.resize_window(original, width, height)?;
    let deadline = Instant::now() + TRACE_TIMEOUT;
    let (current, client) = loop {
        let current = child.designer().ok_or_else(|| {
            "Designer HWND disappeared while restoring its authoring viewport".to_string()
        })?;
        if current.hwnd != original.hwnd || current.process_id != original.process_id {
            return Err("Designer identity changed while restoring its authoring viewport".into());
        }
        let client = child.client_bounds(&current)?;
        let current_width = current.bounds[2] - current.bounds[0];
        let current_height = current.bounds[3] - current.bounds[1];
        if current.visible
            && !current.minimized
            && current_width == width
            && current_height == height
        {
            break (current, client);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "same Designer HWND did not restore to its original authoring viewport: expected={:?}, observed={:?}",
                original.bounds, current.bounds
            ));
        }
        std::thread::sleep(WINDOW_POLL);
    };
    let geometry = latest_geometry_state(trace_path)?
        .filter(|state| state.session_id == session_id)
        .ok_or_else(|| {
            "Designer did not publish geometry before restoring its menu list".to_string()
        })?;
    let last_menu_row = geometry.menu_count.checked_sub(1).ok_or_else(|| {
        "Designer had no menu rows to verify after viewport restoration".to_string()
    })?;
    let client_size = [client[2] - client[0], client[3] - client[1]];
    let row = wait_for_authoring_control_after(
        trace_path,
        first_line,
        session_id,
        client_size,
        AuthoringControlTarget::MenuRow,
        Some(last_menu_row),
        AuthoringControlRole::Selectable,
        TRACE_TIMEOUT,
    )?;
    if row.bounds[0] < client[0]
        || row.bounds[1] < client[1]
        || row.bounds[2] > client[2]
        || row.bounds[3] > client[3]
    {
        return Err(format!(
            "restored last menu row {:?} remains outside the same Designer HWND client {:?}",
            row.bounds, client
        ));
    }
    Ok(format!(
        "restored the same checked Designer HWND={} to its original outer bounds {:?}; fresh client {:?} rendered last MenuRow {} at {:?} inside the client",
        hwnd_id(current.hwnd),
        current.bounds,
        client,
        last_menu_row,
        row.bounds
    ))
}

fn discard_dirty_designer(
    child: &NativeChild,
    designer: &WindowSnapshot,
    trace_path: &Path,
    session_id: u64,
) -> Result<String, String> {
    child.validate_window(designer.hwnd)?;
    let root_recovery = make_root_visible_before_designer_close(child, designer)?;
    let cursor = trace_lines(trace_path).len();
    let key_events = send_alt_f4(child, designer)?;
    let prompt_trace = wait_trace(trace_path, cursor, TRACE_TIMEOUT, |events| {
        events.iter().any(|line| {
            line.contains("trace_event=\"designer_close\"")
                && line.contains("close_prompt=true")
                && line.contains("dirty=true")
        })
    })
    .into_iter()
    .rev()
    .find(|line| {
        line.contains("trace_event=\"designer_close\"")
            && line.contains("close_prompt=true")
            && line.contains("dirty=true")
    })
    .ok_or_else(|| {
        format!("Alt+F4 did not present a dirty close prompt after {key_events} input events")
    })?;
    let (discard, click) = click_authoring_target(
        child,
        designer,
        trace_path,
        session_id,
        AuthoringControlTarget::DiscardDraft,
        None,
        AuthoringControlRole::Button,
    )?;
    let stop_reply = wait_trace(trace_path, cursor, TRACE_TIMEOUT, |events| {
        events.iter().any(|line| {
            line.contains("trace_event=\"authoring\"")
                && line.contains("edge=ReplyAccepted")
                && line.contains("request_kind=StopNativePreview")
                && line.contains("terminal=true")
        })
    })
    .into_iter()
    .rev()
    .find(|line| {
        line.contains("trace_event=\"authoring\"")
            && line.contains("edge=ReplyAccepted")
            && line.contains("request_kind=StopNativePreview")
            && line.contains("terminal=true")
    })
    .ok_or_else(|| {
        format!("Discard did not receive its terminal preview-stop reply: {prompt_trace}")
    })?;
    if !wait_until(Duration::from_secs(4), || child.designer().is_none()) {
        return Err(format!(
            "child-owned Designer HWND remained after production Discard; terminal reply={stop_reply}"
        ));
    }
    let root = child
        .refresh_root()
        .map_err(|error| format!("production Discard removed ROOT: {error}"))?;
    let displays = native_display_bounds()?;
    if !root.visible || root.minimized || !intersects_display_bounds(root.bounds, &displays) {
        return Err(format!(
            "production Discard left ROOT offscreen or nondrawable: visible={} minimized={} bounds={:?}",
            root.visible, root.minimized, root.bounds
        ));
    }
    if child.try_wait()?.is_some() {
        return Err("production Discard terminated the candidate process".into());
    }
    let mut stable_bounds = None;
    let mut stable_samples = 0_u8;
    let stable_on_display = wait_until(Duration::from_millis(750), || {
        let Ok(current) = child.refresh_root() else {
            stable_bounds = None;
            stable_samples = 0;
            return false;
        };
        if current.visible
            && !current.minimized
            && intersects_display_bounds(current.bounds, &displays)
        {
            if stable_bounds == Some(current.bounds) {
                stable_samples = stable_samples.saturating_add(1);
            } else {
                stable_bounds = Some(current.bounds);
                stable_samples = 1;
            }
            stable_samples >= 3
        } else {
            stable_bounds = None;
            stable_samples = 0;
            false
        }
    });
    if !stable_on_display {
        return Err(format!(
            "production Discard did not leave ROOT stably drawable on a physical display; last ROOT={:?}",
            child.refresh_root().ok()
        ));
    }
    Ok(format!(
        "{}; checked Alt+F4={key_events} events reached the dirty close prompt ({prompt_trace}); production Discard target {:?} click=[{}] received terminal stop reply ({stop_reply}) and closed its child-owned Designer HWND while on-screen ROOT and candidate remained alive",
        root_recovery
            .as_deref()
            .unwrap_or("ROOT was already visible on a physical display before Designer close"),
        discard.bounds,
        click.describe()
    ))
}

fn make_root_visible_before_designer_close(
    child: &NativeChild,
    designer: &WindowSnapshot,
) -> Result<Option<String>, String> {
    child.validate_window(designer.hwnd)?;
    let displays = native_display_bounds()?;
    let root = child.refresh_root()?;
    if root.visible && !root.minimized && intersects_display_bounds(root.bounds, &displays) {
        return Ok(None);
    }

    child.focus_window(designer)?;
    let tap = child.send_f11(designer.hwnd, child.process_id(), TAP_TIME)?;
    if !wait_root_visibility(child, true, ROOT_TIMEOUT) {
        return Err(format!(
            "checked Designer-focused F11 did not restore ROOT before close; input=[{}]",
            tap.describe()
        ));
    }
    let restored = child.refresh_root()?;
    if !intersects_display_bounds(restored.bounds, &displays) {
        return Err(format!(
            "Designer-focused F11 restored ROOT only to offscreen bounds {:?}; physical displays={displays:?}",
            restored.bounds
        ));
    }
    if child
        .designer()
        .is_none_or(|current| current.hwnd != designer.hwnd)
        || child.try_wait()?.is_some()
    {
        return Err("Designer or candidate stopped while restoring ROOT before close".into());
    }
    Ok(Some(format!(
        "checked Designer-focused F11 restored ROOT onscreen at {:?} before close with input=[{}]",
        restored.bounds,
        tap.describe()
    )))
}

fn activate_named(
    uia: &UiAutomation,
    child: &NativeChild,
    anchor: &FocusAnchor,
    target: &WindowSnapshot,
    name: &str,
    stage: FailureStage,
    trace_path: &Path,
) -> Result<PointerClickEvidence, CaseFailure> {
    let displays = native_display_bounds()
        .map_err(|error| CaseFailure::new(FailureStage::Environment, error))?;
    let deadline = Instant::now() + UIA_TIMEOUT;
    let mut last_retryable_error = None;
    loop {
        if Instant::now() >= deadline {
            return Err(CaseFailure::new(
                FailureStage::NativeRootState,
                format!(
                    "could not obtain stable on-screen ROOT geometry for semantic control '{}' before timeout; last recoverable edge={:?}",
                    bounded_label(name),
                    last_retryable_error
                ),
            ));
        }
        let fresh = child
            .refresh_root()
            .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
        if fresh.hwnd != target.hwnd || fresh.process_id != target.process_id {
            return Err(CaseFailure::new(
                FailureStage::WindowDiscovery,
                "ROOT identity changed while entering the Designer; refusing stale UIA input"
                    .into(),
            ));
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        let (stable, _) =
            ensure_root_on_physical_display(child, anchor, &fresh, &displays, remaining)
                .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
        let control = wait_named_control_in_client(
            uia,
            child,
            &stable,
            name,
            deadline.saturating_duration_since(Instant::now()),
        )
        .map_err(|error| CaseFailure::new(stage, error))?;
        if !control.enabled {
            return Err(CaseFailure::new(
                stage,
                format!("semantic control '{}' is disabled", bounded_label(name)),
            ));
        }

        // UIA bounds must be reacquired after ROOT has been refreshed and stabilized.
        // If ROOT moves between lookup and click, the native helper rejects input before
        // sending a mouse event and this bounded loop resolves a fresh target.
        let (click_root, _) = ensure_root_on_physical_display(
            child,
            anchor,
            &stable,
            &displays,
            deadline.saturating_duration_since(Instant::now()),
        )
        .map_err(|error| CaseFailure::new(FailureStage::NativeRootState, error))?;
        if click_root.bounds != stable.bounds {
            last_retryable_error = Some("ROOT bounds changed after UIA lookup".to_string());
            continue;
        }
        let latest_control = uia
            .find_named(click_root.hwnd, child.process_id(), name)
            .map_err(|error| CaseFailure::new(stage, error))?
            .ok_or_else(|| {
                CaseFailure::new(
                    stage,
                    format!(
                        "semantic control '{}' disappeared after ROOT stabilization",
                        bounded_label(name)
                    ),
                )
            })?;
        let client = child
            .client_screen_bounds(&click_root)
            .map_err(|error| CaseFailure::new(FailureStage::WindowDiscovery, error))?;
        if !semantic_bounds_center_inside(latest_control.bounds, client) {
            last_retryable_error = Some("UIA bounds left the fresh ROOT client".to_string());
            continue;
        }
        if !latest_control.enabled {
            return Err(CaseFailure::new(
                stage,
                format!("semantic control '{}' became disabled", bounded_label(name)),
            ));
        }
        // Menu controls may advertise UIA Invoke without opening a menu. Use the
        // process-validated native pointer path that mirrors the user's interaction.
        match click_semantic_control(child, &click_root, &latest_control, trace_path) {
            Ok(evidence) => return Ok(evidence),
            Err(error)
                if error.contains("blocked precondition: ROOT")
                    || error.contains("geometry changed after semantic bounds") =>
            {
                last_retryable_error = Some(error);
            }
            Err(error) => {
                return Err(CaseFailure::new(FailureStage::InputInjection, error));
            }
        }
    }
}

fn wait_named_control_in_client(
    uia: &UiAutomation,
    child: &NativeChild,
    target: &WindowSnapshot,
    name: &str,
    timeout: Duration,
) -> Result<SemanticControl, String> {
    child.validate_window(target.hwnd)?;
    let deadline = Instant::now() + timeout;
    let mut latest_bounds = None;
    loop {
        let client = child.client_screen_bounds(target)?;
        if let Some(control) = uia.find_named(target.hwnd, child.process_id(), name)? {
            latest_bounds = Some(control.bounds);
            if semantic_bounds_center_inside(control.bounds, client) {
                return Ok(control);
            }
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "UIA control '{}' did not publish fresh bounds inside child client {:?} before timeout; last bounds={latest_bounds:?}",
                bounded_label(name),
                client
            ));
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

fn semantic_bounds_center_inside(bounds: [i32; 4], client: [i32; 4]) -> bool {
    if bounds[2] <= bounds[0]
        || bounds[3] <= bounds[1]
        || client[2] <= client[0]
        || client[3] <= client[1]
    {
        return false;
    }
    let center = [
        bounds[0] + (bounds[2] - bounds[0]) / 2,
        bounds[1] + (bounds[3] - bounds[1]) / 2,
    ];
    center[0] >= client[0]
        && center[0] < client[2]
        && center[1] >= client[1]
        && center[1] < client[3]
}

fn append_blocked_designer_cases(
    report: &mut AcceptanceReport,
    cause: &CaseFailure,
    child: Option<&NativeChild>,
    output: &Path,
    trace_path: &Path,
) {
    for id in BLOCKED_DESIGNER_CASE_IDS {
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

fn append_blocked_authoring_cases(
    report: &mut AcceptanceReport,
    cause: &CaseFailure,
    child: Option<&NativeChild>,
    output: &Path,
    trace_path: &Path,
) {
    for id in ["A0", "A1", "G0", "A2", "G1", "G2"] {
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
                format!(
                    "not run because authoring Designer entry failed: {}",
                    cause.message
                ),
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
            let hwnd_deadline = Instant::now() + Duration::from_secs(2);
            let mut remaining_windows = child.windows();
            while !remaining_windows.is_empty() && Instant::now() < hwnd_deadline {
                std::thread::sleep(WINDOW_POLL);
                remaining_windows = child.windows();
            }
            report.cleanup.child_owned_windows_closed = remaining_windows.is_empty();
            if status.success() {
                if !report.cleanup.child_owned_windows_closed {
                    let remaining = remaining_windows
                        .iter()
                        .map(|window| format!("{}:{:?}", hwnd_id(window.hwnd), window.role))
                        .collect::<Vec<_>>()
                        .join(",");
                    return Err(CaseFailure::new(
                        FailureStage::Cleanup,
                        format!(
                            "candidate exited normally with {status} but child-owned HWNDs remained: {remaining}"
                        ),
                    ));
                }
                return Ok(format!(
                    "candidate exited normally with {status} after bounded WM_CLOSE; all child-owned HWNDs closed"
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
            let saved = if error.message.starts_with("not run because")
                || error
                    .message
                    .starts_with("runner omitted a required case result")
            {
                Vec::new()
            } else {
                save_failure_artifacts(id, child, output, trace_path)
            };
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

fn append_case_without_artifacts(
    report: &mut AcceptanceReport,
    id: &str,
    result: Result<String, CaseFailure>,
) {
    let (status, observed, failure_stage) = match result {
        Ok(observed) => (CaseStatus::Passed, observed, None),
        Err(error) => (CaseStatus::Failed, error.message, Some(error.stage)),
    };
    report.push_case(AcceptanceCaseResult {
        id: id.to_string(),
        status,
        elapsed_ms: 0,
        expected: bounded_text(expected(id), MAX_RESULT_BYTES),
        observed: bounded_text(&observed, MAX_RESULT_BYTES),
        failure_stage,
        artifacts: Vec::new(),
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
        .map(|trace| safe_trace_excerpt(&trace))
        .unwrap_or_default();
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
    if let Some(child) = child {
        let private_log_destination = output.join(format!("case-{id}-private.log"));
        if copy_bounded_log_tail(child.log_path(), &private_log_destination).is_ok() {
            written.push(private_log_destination);
        }
    }
    let screenshot_destination = output.join(format!("case-{id}.png"));
    if capture_diagnostic_screenshot(child, &screenshot_destination).is_ok() {
        written.push(screenshot_destination);
    }
    written
}

fn run_failure_artifact_case(
    report: &mut AcceptanceReport,
    child: &NativeChild,
    output: &Path,
    trace_path: &Path,
) {
    let started = Instant::now();
    let artifacts = save_failure_artifacts("R1", Some(child), output, trace_path);
    let result = validate_r1_artifacts(&artifacts, child.process_id(), output);
    for path in &artifacts {
        report.push_artifact(path.to_string_lossy());
    }
    let (status, observed, failure_stage) = match result {
        Ok(()) => (
            CaseStatus::Passed,
            format!(
                "controlled harness failure produced a bounded sanitized trace, private log tail, validated child HWND inventory, and screenshot cropped from a visible child-owned window; artifact_count={}",
                artifacts.len()
            ),
            None,
        ),
        Err(error) => (
            CaseStatus::Failed,
            bounded_text(
                &format!("controlled diagnostic artifact validation failed: {error}"),
                MAX_RESULT_BYTES,
            ),
            Some(FailureStage::Environment),
        ),
    };
    report.push_case(AcceptanceCaseResult {
        id: "R1".into(),
        status,
        elapsed_ms: elapsed_ms(started),
        expected: bounded_text(expected("R1"), MAX_RESULT_BYTES),
        observed,
        failure_stage,
        artifacts: artifacts
            .iter()
            .map(|path| bounded_text(&path.to_string_lossy(), MAX_PATH_BYTES))
            .collect(),
    });
}

fn validate_r1_artifacts(
    artifacts: &[PathBuf],
    child_process_id: u32,
    output: &Path,
) -> Result<(), String> {
    let required = [
        output.join("case-R1-trace.log"),
        output.join("case-R1-windows.json"),
        output.join("case-R1-private.log"),
        output.join("case-R1.png"),
    ];
    for path in &required {
        if !artifacts.contains(path) {
            return Err(format!(
                "required bounded artifact is missing: {}",
                path.file_name().unwrap_or_default().to_string_lossy()
            ));
        }
        let length = fs::metadata(path)
            .map_err(|error| {
                format!(
                    "inspect artifact {}: {error}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                )
            })?
            .len();
        let maximum = match path.file_name().and_then(|name| name.to_str()) {
            Some("case-R1-private.log") => MAX_PRIVATE_LOG_BYTES as u64,
            Some("case-R1-trace.log") => MAX_TRACE_BYTES as u64,
            Some("case-R1.png") => 16 * 1024 * 1024,
            _ => 64 * 1024,
        };
        if length == 0 || length > maximum {
            return Err(format!(
                "artifact {} is empty or exceeds the bound",
                path.file_name().unwrap_or_default().to_string_lossy()
            ));
        }
    }
    let trace = fs::read_to_string(&required[0])
        .map_err(|error| format!("read sanitized trace: {error}"))?;
    if trace.trim().is_empty() || trace.len() > MAX_TRACE_BYTES {
        return Err("sanitized trace is empty or exceeds its byte bound".into());
    }
    let windows: serde_json::Value = serde_json::from_slice(
        &fs::read(&required[1]).map_err(|error| format!("read child HWND inventory: {error}"))?,
    )
    .map_err(|error| format!("parse child HWND inventory: {error}"))?;
    if windows["child_process_id"].as_u64() != Some(u64::from(child_process_id)) {
        return Err(
            "HWND inventory child process identity does not match the launched candidate".into(),
        );
    }
    let entries = windows["windows"]
        .as_array()
        .ok_or_else(|| "HWND inventory omitted its window list".to_string())?;
    if entries.is_empty()
        || entries
            .iter()
            .any(|entry| entry["process_id"].as_u64() != Some(u64::from(child_process_id)))
    {
        return Err("HWND inventory does not contain only child-owned native windows".into());
    }
    let (width, height) = image::image_dimensions(&required[3])
        .map_err(|error| format!("decode child-owned screenshot dimensions: {error}"))?;
    if width == 0 || height == 0 || width > 16_384 || height > 16_384 {
        return Err("child-owned screenshot dimensions are invalid or exceed the bound".into());
    }
    Ok(())
}

fn safe_trace_excerpt(trace: &str) -> String {
    let lines = trace
        .lines()
        .filter_map(sanitize_trace_line)
        .collect::<Vec<_>>();

    if lines.len() <= MAX_TRACE_EXCERPT
        && lines.iter().map(|line| line.len() + 1).sum::<usize>() <= MAX_TRACE_BYTES
    {
        return lines.join("\n");
    }

    // Keep an early startup header and the newest events around the failure. A
    // tail-only excerpt can hide the trace-ready/root-creation context, while a
    // head-only excerpt can end long before a late failure (for example ROOT
    // parking after a Designer close). Bound the two regions independently so
    // an unusually large early event cannot evict the diagnostic tail.
    let startup_end = lines.len().min(STARTUP_TRACE_EVENTS);
    let startup = retain_trace_segment(lines[..startup_end].iter(), STARTUP_TRACE_BYTES, false);
    let tail_count = MAX_TRACE_EXCERPT.saturating_sub(startup.len());
    let tail_byte_budget = MAX_TRACE_BYTES.saturating_sub(STARTUP_TRACE_BYTES + 1);
    let tail = retain_trace_segment(lines.iter().rev().take(tail_count), tail_byte_budget, true);

    startup
        .into_iter()
        .chain(tail)
        .collect::<Vec<_>>()
        .join("\n")
}

fn retain_trace_segment<'a>(
    lines: impl Iterator<Item = &'a String>,
    byte_budget: usize,
    reverse_output: bool,
) -> Vec<String> {
    let mut retained = Vec::new();
    let mut bytes = 0usize;
    for line in lines {
        let next_bytes = bytes.saturating_add(line.len()).saturating_add(1);
        if next_bytes > byte_budget {
            break;
        }
        bytes = next_bytes;
        retained.push(line.clone());
    }
    if reverse_output {
        retained.reverse();
    }
    retained
}

fn sanitize_trace_line(line: &str) -> Option<String> {
    const EVENT_NAMES: &[&str] = &[
        "designer_callback",
        "designer_focus",
        "designer_pointer",
        "designer_pointer_moved",
        "root_pointer_moved",
        "root_pointer_button",
        "root_menu_interaction",
        "root_menu_body",
        "designer_submitted",
        "designer_body",
        "designer_widget",
        "designer_widget_pointer",
        "root_result_pointer",
        "radial_action",
        "designer_mutation",
        "authoring",
        "hook_primary",
        "hook_admission",
        "hook_deadline",
        "hook_service_ready",
        "hook_pump_probe",
        "hook_service_exit",
        "hook_observed",
        "hook_callback",
        "frontend_key",
        "designer_semantic_target",
        "designer_authoring_control",
        "designer_canvas_allocation",
        "designer_action_catalog_rank",
        "native_preview_dispatch_count",
        "designer_geometry_state",
        "designer_edit_state",
        "designer_close",
        "disposable_request_cancelled",
        "acceptance_prepare_gate",
        "designer_preview_rendered",
        "configured_primary",
        "short_tap",
        "desired_visibility",
        "root_command",
        "window_sample_truncated",
        "restore",
        "native_window_snapshot",
        "native_activation",
        "native_pointer",
        "budget_exhausted",
        "trace_ready",
    ];
    const SAFE_FIELDS: &[&str] = &[
        "elapsed_ms",
        "event_budget",
        "phase",
        "viewport",
        "edge",
        "request_id",
        "request_kind",
        "session_id",
        "generation",
        "menu_cell_ids_digest",
        "cell_ring_index",
        "cell_slot_index",
        "terminal",
        "pointer_down",
        "pointer_up",
        "window_under_cursor_hwnd",
        "window_under_cursor_owner",
        "cursor_screen_x",
        "cursor_screen_y",
        "client_x",
        "client_y",
        "screen_x",
        "screen_y",
        "menu",
        "hovered",
        "clicked",
        "open",
        "entered",
        "close_prompt",
        "dirty",
        "pending_disposable",
        "pending_durable",
        "pending_native_preview",
        "state",
        "category",
        "response",
        "pointer_pressed",
        "pointer_released",
        "button_down_on",
        "pointer_inside",
        "layer_is_topmost",
        "has_position",
        "pointer_x",
        "pointer_y",
        "kind",
        "result_index",
        "pressed",
        "released",
        "stage",
        "skins",
        "editor_open",
        "skins_selected",
        "panel_registered",
        "result",
        "transition",
        "provenance",
        "foreground_owner",
        "owner",
        "global_exclusive_owners",
        "adapter_exclusive",
        "recovery",
        "deadline_scheduled",
        "radial_intent",
        "invocation_id",
        "timer_id",
        "delay_ms",
        "thread_id",
        "desktop",
        "command",
        "source",
        "requested_x",
        "requested_y",
        "requested_width",
        "requested_height",
        "primary_vk",
        "probe_id",
        "message_result",
        "callback_elapsed_us",
        "shutdown_requested",
        "primary_down",
        "owned_input",
        "pending_deadlines",
        "vk",
        "down",
        "injected",
        "primary",
        "elapsed_us",
        "key",
        "focused",
        "left_px",
        "top_px",
        "right_px",
        "bottom_px",
        "enabled",
        "selected",
        "index",
        "client_width_px",
        "client_height_px",
        "allocated_rect_px",
        "clip_rect_px",
        "requested_size_px",
        "custom_action_index",
        "rank",
        "catalog_len",
        "count",
        "widget_changed",
        "model_changed",
        "input_matches_model",
        "draft_dirty",
        "request_kind",
        "request_id",
        "hwnd",
        "visible",
        "minimized",
        "terminal",
        "correlation",
        "stage",
    ];

    let tokens = split_trace_tokens(line);
    let mut event = None;
    let mut fields = Vec::new();
    for token in tokens {
        let Some((key, value)) = token.split_once('=') else {
            continue;
        };
        if key == "trace_event" {
            let value = trace_value(value)?;
            if !EVENT_NAMES.contains(&value.as_str()) {
                return None;
            }
            event = Some(value);
            continue;
        }
        if SAFE_FIELDS.contains(&key) {
            let value = trace_value(value)?;
            if !safe_trace_field_value(key, &value) {
                continue;
            }
            fields.push(format!("{key}={value}"));
        }
    }
    let event = event?;
    let mut sanitized = format!("trace_event={event}");
    for field in fields {
        sanitized.push(' ');
        sanitized.push_str(&field);
    }
    Some(sanitized)
}

fn safe_trace_field_value(key: &str, value: &str) -> bool {
    match key {
        "focused" => matches!(value, "true" | "false"),
        "command" => matches!(
            value,
            "position" | "size" | "Show" | "Minimize" | "Focus" | "ParkingBoundary"
        ),
        "source" => matches!(value, "ToggleBatch" | "LegacyTrigger" | "Queued"),
        "requested_x" | "requested_y" | "requested_width" | "requested_height" => {
            value.parse::<i32>().is_ok()
        }
        "menu_cell_ids_digest" => value.parse::<u64>().is_ok(),
        "cell_ring_index" | "cell_slot_index" => value.parse::<i32>().is_ok(),
        _ => true,
    }
}

fn split_trace_tokens(line: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let mut token_start = None;
    let mut in_quotes = false;
    for (index, character) in line.char_indices() {
        if character == '"' {
            in_quotes = !in_quotes;
        } else if character.is_whitespace() && !in_quotes {
            if let Some(start) = token_start.take() {
                tokens.push(&line[start..index]);
            }
            continue;
        }
        token_start.get_or_insert(index);
    }
    if let Some(start) = token_start {
        tokens.push(&line[start..]);
    }
    tokens
}

fn trace_value(raw: &str) -> Option<String> {
    let value = raw.trim_matches('"');
    if value.is_empty()
        || value.len() > 96
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_-.(),:?[]".contains(&byte))
    {
        return None;
    }
    Some(value.to_string())
}

fn copy_bounded_log_tail(source: &Path, destination: &Path) -> Result<(), String> {
    let mut input = File::open(source).map_err(|error| format!("open candidate log: {error}"))?;
    let length = input
        .metadata()
        .map_err(|error| format!("inspect candidate log: {error}"))?
        .len();
    let start = length.saturating_sub(MAX_PRIVATE_LOG_BYTES as u64);
    input
        .seek(SeekFrom::Start(start))
        .map_err(|error| format!("seek bounded candidate log tail: {error}"))?;
    let mut bytes = Vec::with_capacity(length.saturating_sub(start) as usize);
    input
        .take(MAX_PRIVATE_LOG_BYTES as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("read bounded candidate log tail: {error}"))?;
    if bytes.is_empty() {
        return Err("candidate log is empty".into());
    }
    fs::write(destination, bytes)
        .map_err(|error| format!("write private bounded log tail: {error}"))
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
    let child =
        child.ok_or_else(|| "diagnostic screenshot requires a child-owned HWND".to_string())?;
    let displays = native_display_bounds()?;
    let candidate = child
        .windows()
        .into_iter()
        .find(|window| {
            window.visible
                && !window.minimized
                && window.process_id == child.process_id()
                && intersects_display_bounds(window.bounds, &displays)
        })
        .ok_or_else(|| "no visible child-owned HWND intersects a physical display".to_string())?;
    let image = {
        let window = candidate;
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
            return Err(
                "child-owned diagnostic HWND crop does not intersect its physical display".into(),
            );
        }
        screen
            .capture_area(left, top, (right - left) as u32, (bottom - top) as u32)
            .map_err(|error| format!("capture diagnostic window crop: {error}"))?
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
        "A0" => "New Menu creates one stable menu and selects it through the production toolbar",
        "A1" => {
            "Menus mode creates a menu and Add Ring publishes a ready proposal without changing committed geometry"
        }
        "G0" => {
            "explicitly applying the prepared outer-ring proposal commits the candidate and preserves existing cell IDs"
        }
        "A2" => {
            "Ring selector and Slots grow apply a prepared candidate while preserving prior cell IDs"
        }
        "G1" => {
            "populated shrink cancels without loss, then moves cells to overflow and applies safely"
        }
        "G2" => {
            "compact Designer fits its controls and canvas; ROOT remains stable on a physical display after close"
        }
        "A3" => {
            "a searched harmless custom action beyond the unfiltered first 50 rows is assigned through the real popup"
        }
        "A4" => "dirty popup Apply and open preserves the selected authored cell in Inspector",
        "A5" => "a harmless skin style edit is made through the production Skins editor",
        "A6" => {
            "Save, close, and reopen persist stable radial identities, action binding, and style"
        }
        "A7" => "Undo and Redo restore the same authored edit through the production toolbar",
        "A8" => "Design and safe desktop preview do not dispatch a leaf action or append history",
        "D3" => "ROOT can hide and show while the open Designer remains interactive and serviced",
        "D6" => "dirty close Keep Editing retains the draft before explicit terminal discard",
        "D7" => "close cancels a pending disposable preview preparation with no late reopen",
        "R0" => {
            "valid bounded JSON and text reports identify source, profile, hashes, elapsed time, and evidence"
        }
        "R1" => {
            "controlled harness failure writes bounded privacy-safe trace and owned screenshot evidence"
        }
        "R2" => "child process, HWNDs, temp profile, and native input state are cleaned up",
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

fn parse_authoring_request_identity(
    line: &str,
    session_id: u64,
    request_kind: &str,
) -> Option<AuthoringRequestIdentity> {
    if !line.contains("trace_event=\"authoring\"")
        || trace_field_value(line, "edge")? != "RequestSent"
        || trace_field_value(line, "request_kind")? != request_kind
        || trace_field_value(line, "session_id")?.parse::<u64>().ok()? != session_id
        || trace_field_value(line, "terminal")? != "false"
    {
        return None;
    }
    Some(AuthoringRequestIdentity {
        request_id: trace_field_value(line, "request_id")?.parse().ok()?,
        generation: trace_field_value(line, "generation")?.parse().ok()?,
        session_id,
    })
}

fn wait_for_authoring_request_generation(
    trace_path: &Path,
    cursor: usize,
    session_id: u64,
    request_kind: &str,
    generation: u64,
    timeout: Duration,
) -> Option<AuthoringRequestIdentity> {
    let events = wait_trace(trace_path, cursor, timeout, |events| {
        events.iter().any(|line| {
            parse_authoring_request_identity(line, session_id, request_kind)
                .is_some_and(|identity| identity.generation == generation)
        })
    });
    events.iter().find_map(|line| {
        parse_authoring_request_identity(line, session_id, request_kind)
            .filter(|identity| identity.generation == generation)
    })
}

fn authoring_edge_matches(
    line: &str,
    identity: AuthoringRequestIdentity,
    request_kind: &str,
    edge: &str,
) -> bool {
    line.contains("trace_event=\"authoring\"")
        && trace_field_value(line, "edge") == Some(edge)
        && trace_field_value(line, "request_kind") == Some(request_kind)
        && trace_field_value(line, "request_id").and_then(|value| value.parse::<u64>().ok())
            == Some(identity.request_id)
        && trace_field_value(line, "generation").and_then(|value| value.parse::<u64>().ok())
            == Some(identity.generation)
        && trace_field_value(line, "session_id").and_then(|value| value.parse::<u64>().ok())
            == Some(identity.session_id)
}

fn wait_for_authoring_edge(
    trace_path: &Path,
    cursor: usize,
    identity: AuthoringRequestIdentity,
    request_kind: &str,
    edge: &str,
    timeout: Duration,
) -> Option<String> {
    wait_trace(trace_path, cursor, timeout, |events| {
        events
            .iter()
            .any(|line| authoring_edge_matches(line, identity, request_kind, edge))
    })
    .into_iter()
    .find(|line| authoring_edge_matches(line, identity, request_kind, edge))
}

fn acceptance_prepare_gate_matches(
    line: &str,
    identity: AuthoringRequestIdentity,
    edge: &str,
) -> bool {
    line.contains("trace_event=\"acceptance_prepare_gate\"")
        && trace_field_value(line, "edge") == Some(edge)
        && trace_field_value(line, "request_kind") == Some("PrepareEmbeddedPreview")
        && trace_field_value(line, "request_id").and_then(|value| value.parse::<u64>().ok())
            == Some(identity.request_id)
        && trace_field_value(line, "generation").and_then(|value| value.parse::<u64>().ok())
            == Some(identity.generation)
        && trace_field_value(line, "session_id").and_then(|value| value.parse::<u64>().ok())
            == Some(identity.session_id)
}

fn disposable_cancel_matches(
    line: &str,
    identity: AuthoringRequestIdentity,
    request_kind: &str,
) -> bool {
    line.contains("trace_event=\"disposable_request_cancelled\"")
        && trace_field_value(line, "request_kind") == Some(request_kind)
        && trace_field_value(line, "request_id").and_then(|value| value.parse::<u64>().ok())
            == Some(identity.request_id)
        && trace_field_value(line, "generation").and_then(|value| value.parse::<u64>().ok())
            == Some(identity.generation)
        && trace_field_value(line, "session_id").and_then(|value| value.parse::<u64>().ok())
            == Some(identity.session_id)
}

fn designer_close_matches(line: &str, session_id: u64, close_prompt: bool, dirty: bool) -> bool {
    line.contains("trace_event=\"designer_close\"")
        && trace_field_value(line, "session_id").and_then(|value| value.parse::<u64>().ok())
            == Some(session_id)
        && trace_field_value(line, "close_prompt")
            == Some(if close_prompt { "true" } else { "false" })
        && trace_field_value(line, "dirty") == Some(if dirty { "true" } else { "false" })
}

fn verify_designer_stays_closed_for(
    child: &mut NativeChild,
    duration: Duration,
) -> Result<String, String> {
    let started = Instant::now();
    let mut samples = 0usize;
    while started.elapsed() < duration {
        if child.designer().is_some() {
            return Err(format!(
                "Designer reappeared during the bounded closed interval after {} ms",
                started.elapsed().as_millis()
            ));
        }
        if child
            .try_wait()
            .map_err(|error| format!("poll candidate during close stability interval: {error}"))?
            .is_some()
        {
            return Err(
                "candidate process exited during the Designer close stability interval".into(),
            );
        }
        samples += 1;
        std::thread::sleep(Duration::from_millis(25));
    }
    Ok(format!(
        "{} ms with {samples} Designer-absent and candidate-alive samples",
        started.elapsed().as_millis()
    ))
}

fn authoring_reply_matches(
    line: &str,
    identity: AuthoringRequestIdentity,
    request_kind: &str,
) -> bool {
    line.contains("trace_event=\"authoring\"")
        && trace_field_value(line, "edge") == Some("ReplyAccepted")
        && trace_field_value(line, "request_kind") == Some(request_kind)
        && trace_field_value(line, "request_id").and_then(|value| value.parse::<u64>().ok())
            == Some(identity.request_id)
        && trace_field_value(line, "generation").and_then(|value| value.parse::<u64>().ok())
            == Some(identity.generation)
        && trace_field_value(line, "session_id").and_then(|value| value.parse::<u64>().ok())
            == Some(identity.session_id)
        && trace_field_value(line, "terminal") == Some("true")
}

fn wait_for_terminal_authoring_request(
    trace_path: &Path,
    cursor: usize,
    session_id: u64,
    request_kind: &str,
    timeout: Duration,
) -> Option<AuthoringReplyEvidence> {
    let events = wait_trace(trace_path, cursor, timeout, |events| {
        let Some(identity) = events
            .iter()
            .find_map(|line| parse_authoring_request_identity(line, session_id, request_kind))
        else {
            return false;
        };
        events
            .iter()
            .any(|line| authoring_reply_matches(line, identity, request_kind))
    });
    let identity = events
        .iter()
        .find_map(|line| parse_authoring_request_identity(line, session_id, request_kind))?;
    if !events
        .iter()
        .any(|line| authoring_reply_matches(line, identity, request_kind))
    {
        return None;
    }
    Some(AuthoringReplyEvidence { identity })
}

fn wait_for_authoring_control_clicked(
    trace_path: &Path,
    cursor: usize,
    session_id: u64,
    target: AuthoringControlTarget,
    timeout: Duration,
) -> Option<String> {
    let target = format!("{target:?}");
    wait_trace(trace_path, cursor, timeout, |events| {
        events.iter().any(|line| {
            line.contains("trace_event=\"designer_authoring_control\"")
                && trace_field_value(line, "target") == Some(target.as_str())
                && trace_field_value(line, "clicked") == Some("true")
                && trace_field_value(line, "session_id").and_then(|value| value.parse::<u64>().ok())
                    == Some(session_id)
        })
    })
    .into_iter()
    .find(|line| {
        line.contains("trace_event=\"designer_authoring_control\"")
            && trace_field_value(line, "target") == Some(target.as_str())
            && trace_field_value(line, "clicked") == Some("true")
            && trace_field_value(line, "session_id").and_then(|value| value.parse::<u64>().ok())
                == Some(session_id)
    })
}

fn wait_for_authoring_control_focused_after(
    trace_path: &Path,
    first_line: usize,
    session_id: u64,
    generation: u64,
    target: AuthoringControlTarget,
    role: AuthoringControlRole,
    timeout: Duration,
) -> Result<AuthoringControlSnapshot, String> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(control) =
            find_authoring_control_after(trace_path, first_line, session_id, target, None, role)?
            && control.generation == generation
            && control.enabled
            && control.focused
        {
            return Ok(control);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "Designer did not focus enabled target {target:?} with role {role:?} in session {session_id} generation {generation} after trace line {first_line}"
            ));
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

fn tab_and_activate_authoring_control(
    child: &NativeChild,
    window: &WindowSnapshot,
    trace_path: &Path,
    session_id: u64,
    generation: u64,
    target: AuthoringControlTarget,
) -> Result<(AuthoringControlSnapshot, usize, usize), String> {
    let mut tab_events = 0;
    let mut focused = None;
    for _ in 0..MAX_KEYBOARD_FOCUS_STEPS {
        let cursor = trace_lines(trace_path).len();
        tab_events += send_tab(child, window)?;
        match wait_for_authoring_control_focused_after(
            trace_path,
            cursor,
            session_id,
            generation,
            target,
            AuthoringControlRole::Button,
            Duration::from_millis(150),
        ) {
            Ok(control) => {
                focused = Some(control);
                break;
            }
            Err(error) if error.contains("did not focus enabled target") => {}
            Err(error) => return Err(error),
        }
    }
    let control = focused.ok_or_else(|| {
        format!(
            "checked Tab did not focus {target:?} after at most {MAX_KEYBOARD_FOCUS_STEPS} steps"
        )
    })?;
    let click_cursor = trace_lines(trace_path).len();
    let enter_events = send_enter_current(child, window)?;
    wait_for_authoring_control_clicked(trace_path, click_cursor, session_id, target, TRACE_TIMEOUT)
        .ok_or_else(|| format!("checked Enter did not activate focused {target:?}"))?;
    Ok((control, tab_events, enter_events))
}

fn visible_radial_host_windows(child: &NativeChild) -> std::collections::BTreeSet<u64> {
    child
        .windows()
        .into_iter()
        .filter(|window| window.visible && window.class_name == "MultiLauncherRadialHost")
        .map(|window| hwnd_id(window.hwnd))
        .collect()
}

#[derive(Clone, Copy, Debug)]
struct DesignerSemanticTargetState {
    bounds: [i32; 4],
    selected: bool,
    focused: bool,
    session_id: u64,
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
        session_id: field("session_id")?.parse().ok()?,
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
        let state = latest_designer_semantic_target(&trace_lines(trace_path), target, None);
        if state.as_ref().is_some_and(&predicate) {
            return state;
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

fn wait_for_designer_semantic_target_in_session(
    trace_path: &Path,
    target: DesignerSemanticTarget,
    session_id: u64,
    timeout: Duration,
    predicate: impl Fn(&DesignerSemanticTargetState) -> bool,
) -> Option<DesignerSemanticTargetState> {
    let deadline = Instant::now() + timeout;
    loop {
        let state =
            latest_designer_semantic_target(&trace_lines(trace_path), target, Some(session_id));
        if state.as_ref().is_some_and(&predicate) {
            return state;
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(WINDOW_POLL);
    }
}

fn latest_designer_semantic_target(
    lines: &[String],
    target: DesignerSemanticTarget,
    session_id: Option<u64>,
) -> Option<DesignerSemanticTargetState> {
    lines
        .iter()
        .rev()
        .filter_map(|line| parse_designer_semantic_target(line, target))
        .find(|state| session_id.is_none_or(|session_id| state.session_id == session_id))
}

fn trace_lines(path: &Path) -> Vec<String> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|line| line.contains("trace_event=\""))
        .map(str::to_owned)
        .collect()
}

fn root_menu_state_from_trace(lines: &[String]) -> Option<RootMenuState> {
    if lines
        .iter()
        .any(|line| line.contains("trace_event=\"budget_exhausted\""))
        || !lines
            .iter()
            .any(|line| line.contains("trace_event=\"trace_ready\""))
    {
        return None;
    }

    let mut state = RootMenuState::default();
    for line in lines
        .iter()
        .filter(|line| line.contains("trace_event=\"root_menu_interaction\""))
    {
        let open = match trace_field_value(line, "open")? {
            "true" => true,
            "false" => false,
            _ => return None,
        };
        match trace_field_value(line, "menu")? {
            "File" => state.file_open = open,
            "Apps" => state.apps_open = open,
            _ => return None,
        }
    }
    let mut file_body_entered = None;
    let mut apps_body_entered = None;
    for line in lines
        .iter()
        .filter(|line| line.contains("trace_event=\"root_menu_body\""))
    {
        let entered = match trace_field_value(line, "entered")? {
            "true" => true,
            "false" => false,
            _ => return None,
        };
        match trace_field_value(line, "menu")? {
            "File" => file_body_entered = Some(entered),
            "Apps" => apps_body_entered = Some(entered),
            _ => return None,
        }
    }
    if let Some(entered) = file_body_entered {
        state.file_open = entered;
    }
    if let Some(entered) = apps_body_entered {
        state.apps_open = entered;
    }
    Some(state)
}

fn trace_field_value<'a>(line: &'a str, field: &str) -> Option<&'a str> {
    line.split_ascii_whitespace()
        .find_map(|part| part.strip_prefix(&format!("{field}=")))
        .map(|value| value.trim_matches('"'))
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

fn wait_root_visibility(child: &NativeChild, visible: bool, timeout: Duration) -> bool {
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

fn require_parked_root_state(
    child: &NativeChild,
    expected: &WindowSnapshot,
    physical_displays: &[[i32; 4]],
) -> Result<WindowSnapshot, String> {
    let current = child.refresh_root()?;
    if current.hwnd != expected.hwnd
        || current.process_id != expected.process_id
        || current.role != WindowRole::Root
        || current.visible != expected.visible
        || current.minimized != expected.minimized
        || intersects_display_bounds(current.bounds, physical_displays)
    {
        return Err(format!(
            "ROOT left its parked physical-display state during Designer preview: expected HWND={} PID={} visible={} minimized={} off_display_bounds={:?}, observed HWND={} PID={} visible={} minimized={} bounds={:?} physical_displays={physical_displays:?}",
            hwnd_id(expected.hwnd),
            expected.process_id,
            expected.visible,
            expected.minimized,
            expected.bounds,
            hwnd_id(current.hwnd),
            current.process_id,
            current.visible,
            current.minimized,
            current.bounds,
        ));
    }
    Ok(current)
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

    fn spacer_from(
        template: &multi_launcher::radial::model::CellDefinition,
        id: String,
    ) -> multi_launcher::radial::model::CellDefinition {
        let mut cell = template.clone();
        cell.id = multi_launcher::radial::model::CellId::new(id);
        cell.label = "Spacer".into();
        cell.content = multi_launcher::radial::model::CellContent::Spacer;
        cell.alternate_clicks.clear();
        cell.alternate_controls.clear();
        cell.shortcuts.clear();
        cell.hotstrings.clear();
        cell
    }

    fn write_saved_graph_fixture(
        profile: &Path,
    ) -> (PersistedMenuGraphExpectation, PersistedMenuGraphExpectation) {
        use multi_launcher::radial::model::{
            ActionBinding, AfterActionPolicy, CellContent, MenuId, Override, RadialDocument, RingId,
        };
        use multi_launcher::universal_actions::{
            PersistableActionTargetRef, PersistedUniversalActionRef, action_ids,
        };

        let mut document = RadialDocument::starter();
        let target_action_index = ACCEPTANCE_TARGET_ACTION_INDEX;
        let target_action = multi_launcher::actions::Action {
            label: format!("Radial Acceptance Harmless Action {target_action_index:03}"),
            desc: "Deterministic native authoring fixture".into(),
            action: format!("radial_acceptance_harmless_{target_action_index:03}"),
            args: None,
        };

        let root = &mut document.menus[0];
        let mut root_first = root.rings[0].clone();
        root_first.id = RingId::new("starter-main-after-overflow");
        let moved = root_first
            .cells
            .pop()
            .expect("starter root should have an overflow candidate");
        let mut root_overflow = root_first.clone();
        root_overflow.id = RingId::new("starter-overflow");
        root_overflow.radius = root_first.radius + 80.0;
        root_overflow.cells = vec![moved];
        root.rings = vec![root_first.clone(), root_overflow];

        let mut authored = root.clone();
        authored.id = MenuId::new("radial-acceptance-authored-menu");
        authored.name = "Radial acceptance authored graph".into();
        authored.after_action = AfterActionPolicy::CloseTree;
        let mut inner = root_first.clone();
        inner.id = RingId::new("radial-acceptance-inner");
        inner.cells = (0..8)
            .map(|index| {
                spacer_from(
                    &root_first.cells[0],
                    format!("radial-acceptance-inner-cell-{index}"),
                )
            })
            .collect();
        let mut outer = root_first.clone();
        outer.id = RingId::new("radial-acceptance-outer");
        outer.radius = inner.radius + 80.0;
        outer.cells = (0..10)
            .map(|index| {
                spacer_from(
                    &root_first.cells[0],
                    format!("radial-acceptance-outer-cell-{index}"),
                )
            })
            .collect();
        outer.cells[0].content = CellContent::Action {
            binding: ActionBinding::Persisted {
                action: PersistedUniversalActionRef {
                    target: Some(PersistableActionTargetRef::CustomAction {
                        action: target_action.clone(),
                    }),
                    action_id: action_ids::RESULT_EXECUTE,
                },
            },
        };
        authored.rings = vec![inner, outer];
        let authored_menu_index = document.menus.len();
        document.menus.push(authored);
        document.skins[0].style.values.effects.glow_enabled = Override::Value(false);

        fs::write(
            profile.join("radial.json"),
            serde_json::to_vec_pretty(&document).expect("serialize saved graph fixture"),
        )
        .expect("write saved graph fixture");
        fs::write(
            profile.join("actions.json"),
            serde_json::to_vec_pretty(&vec![target_action; target_action_index + 1])
                .expect("serialize action fixture"),
        )
        .expect("write action fixture");

        (
            PersistedMenuGraphExpectation {
                menu_index: authored_menu_index,
                ring_slots: vec![8, 10],
                populated_cells: 0,
                cell_ids_digest: 0,
            },
            PersistedMenuGraphExpectation {
                menu_index: 0,
                ring_slots: vec![8, 1],
                populated_cells: 9,
                cell_ids_digest: 0,
            },
        )
    }

    #[test]
    fn post_apply_cell_id_check_requires_available_equal_candidate_digest() {
        assert!(committed_cell_ids_match(true, 41, 41));
        assert!(!committed_cell_ids_match(true, 41, 42));
        assert!(!committed_cell_ids_match(false, 41, 41));
    }

    #[test]
    fn canvas_cell_index_maps_outer_ring_slot_to_flat_menu_index() {
        assert_eq!(flat_canvas_cell_index(&[8, 10], 0, 0), Some(0));
        assert_eq!(flat_canvas_cell_index(&[8, 10], 1, 0), Some(8));
        assert_eq!(flat_canvas_cell_index(&[8, 10], 1, 9), Some(17));
        assert_eq!(flat_canvas_cell_index(&[8, 10], 1, 10), None);
        assert_eq!(flat_canvas_cell_index(&[8, 10], 2, 0), None);
    }

    #[test]
    fn typed_save_oracle_requires_authored_geometry_and_g1_overflow_graph() {
        let profile = tempfile::tempdir().expect("temporary profile");
        let (authored, root) = write_saved_graph_fixture(profile.path());
        let saved = verify_saved_authoring_fixture(
            profile.path(),
            &authored,
            &root,
            0,
            ACCEPTANCE_TARGET_ACTION_INDEX,
        )
        .expect("typed saved graph should retain both authored menus");
        assert!(saved.contains("[8, 10]"));
        assert!(saved.contains("[8, 1]"));
        assert!(saved.contains("9 populated cells"));

        let mut wrong_root = root.clone();
        wrong_root.ring_slots = vec![8, 7];
        let error = verify_saved_authoring_fixture(
            profile.path(),
            &authored,
            &wrong_root,
            0,
            ACCEPTANCE_TARGET_ACTION_INDEX,
        )
        .expect_err("the oracle must reject a lost/changed overflow ring");
        assert!(error.contains("G1 root graph"));
    }

    #[test]
    fn leaf_side_effect_oracle_uses_a_strict_pre_a3_history_and_trace_baseline() {
        let profile = tempfile::tempdir().expect("temporary profile");
        let history_path = profile.path().join("history.json");
        fs::write(&history_path, b"[]").expect("write initial history");
        let trace_path = profile.path().join("acceptance.log");
        fs::write(
            &trace_path,
            "WARN target trace_event=\"trace_ready\" elapsed_ms=0\n",
        )
        .expect("write pre-A3 trace");
        let baseline = capture_action_side_effect_baseline(profile.path(), &trace_path)
            .expect("capture history and trace baseline");

        fs::OpenOptions::new()
            .append(true)
            .open(&trace_path)
            .expect("open trace append")
            .write_all(
                b"WARN target trace_event=\"designer_widget\" elapsed_ms=2\nWARN target trace_event=\"native_preview_dispatch_count\" elapsed_ms=3 count=0\n",
            )
            .expect("append harmless design events");
        verify_no_leaf_side_effects(profile.path(), &trace_path, &baseline)
            .expect("zero-dispatch design interaction should pass");

        fs::OpenOptions::new()
            .append(true)
            .open(&trace_path)
            .expect("open trace append")
            .write_all(b"WARN target trace_event=\"radial_action\" elapsed_ms=4\n")
            .expect("append real action dispatch");
        assert!(
            verify_no_leaf_side_effects(profile.path(), &trace_path, &baseline)
                .expect_err("real action dispatch must fail")
                .contains("real radial leaf action")
        );

        fs::write(&trace_path, "WARN target trace_event=\"trace_ready\" elapsed_ms=0\nWARN target trace_event=\"native_preview_dispatch_count\" elapsed_ms=3 count=1\n")
            .expect("rewrite trace with nonzero native dispatch");
        assert!(
            verify_no_leaf_side_effects(profile.path(), &trace_path, &baseline)
                .expect_err("nonzero native dispatch counter must fail")
                .contains("nonzero native preview")
        );

        fs::write(
            &trace_path,
            "WARN target trace_event=\"trace_ready\" elapsed_ms=0\n",
        )
        .expect("restore trace event count");
        fs::write(&history_path, b"[\"changed\"]").expect("mutate history");
        assert!(
            verify_no_leaf_side_effects(profile.path(), &trace_path, &baseline)
                .expect_err("history mutation must fail")
                .contains("history file")
        );
    }

    #[test]
    fn blocked_designer_and_tail_fill_preserve_g0_by_case_id() {
        assert!(BLOCKED_DESIGNER_CASE_IDS.contains(&"G0"));
        let prior_results = CASE_IDS
            .into_iter()
            .filter(|id| *id != "G0")
            .collect::<Vec<_>>();
        assert_eq!(prior_results.len(), CASE_IDS.len() - 1);
        assert_eq!(missing_case_ids(&prior_results), vec!["G0"]);
    }

    #[test]
    fn designer_semantic_targets_are_scoped_to_the_reopened_session() {
        let lines = vec![
            "trace_event=\"designer_semantic_target\" target=Menus role=\"SelectableLabel\" viewport=Deferred left_px=10 top_px=20 right_px=40 bottom_px=44 selected=true focused=false session_id=1 generation=7".to_string(),
            "trace_event=\"designer_semantic_target\" target=Menus role=\"SelectableLabel\" viewport=Deferred left_px=10 top_px=20 right_px=40 bottom_px=44 selected=false focused=false session_id=2 generation=2".to_string(),
        ];

        let reopened =
            latest_designer_semantic_target(&lines, DesignerSemanticTarget::Menus, Some(2))
                .expect("reopened Designer Menus target should be present");
        assert_eq!(reopened.session_id, 2);
        assert!(!reopened.selected);

        let earlier =
            latest_designer_semantic_target(&lines, DesignerSemanticTarget::Menus, Some(1))
                .expect("earlier Designer Menus target should remain addressable");
        assert_eq!(earlier.session_id, 1);
        assert!(earlier.selected);
    }

    #[test]
    fn disposable_close_oracle_correlates_cancel_and_prompt_to_one_request_session() {
        let identity = AuthoringRequestIdentity {
            request_id: 41,
            generation: 9,
            session_id: 7,
        };
        let sent = "trace_event=\"authoring\" edge=RequestSent request_kind=PrepareEmbeddedPreview request_id=41 generation=9 session_id=7 terminal=false";
        let cancelled = "trace_event=\"disposable_request_cancelled\" request_kind=PrepareEmbeddedPreview request_id=41 generation=9 session_id=7";
        let prompt = "trace_event=\"designer_close\" session_id=7 open=true close_prompt=true dirty=true pending_disposable=false";
        let other_session_prompt = "trace_event=\"designer_close\" session_id=8 open=true close_prompt=true dirty=true pending_disposable=false";

        assert_eq!(
            parse_authoring_request_identity(sent, 7, "PrepareEmbeddedPreview"),
            Some(identity)
        );
        assert!(disposable_cancel_matches(
            cancelled,
            identity,
            "PrepareEmbeddedPreview"
        ));
        assert!(designer_close_matches(prompt, 7, true, true));
        assert!(!designer_close_matches(other_session_prompt, 7, true, true));
        assert!(acceptance_prepare_gate_matches(
            "trace_event=\"acceptance_prepare_gate\" edge=Held request_kind=PrepareEmbeddedPreview request_id=41 generation=9 session_id=7",
            identity,
            "Held"
        ));
        assert!(authoring_edge_matches(
            "trace_event=\"authoring\" edge=ReplyRejected request_kind=PrepareEmbeddedPreview request_id=41 generation=9 session_id=7 terminal=false",
            identity,
            "PrepareEmbeddedPreview",
            "ReplyRejected"
        ));
    }

    #[test]
    fn designer_preview_render_trace_is_sanitized_with_only_typed_identity() {
        let excerpt = safe_trace_excerpt(
            "WARN target trace_event=\"designer_preview_rendered\" elapsed_ms=12 session_id=7 generation=9 menu_cell_ids_digest=123 private_title=secret",
        );
        assert_eq!(
            excerpt,
            "trace_event=designer_preview_rendered elapsed_ms=12 session_id=7 generation=9 menu_cell_ids_digest=123"
        );
        let gate = safe_trace_excerpt(
            "WARN target trace_event=\"acceptance_prepare_gate\" elapsed_ms=13 edge=Held request_kind=PrepareEmbeddedPreview request_id=41 generation=9 session_id=7 private_path=C:\\private",
        );
        assert_eq!(
            gate,
            "trace_event=acceptance_prepare_gate elapsed_ms=13 edge=Held request_kind=PrepareEmbeddedPreview request_id=41 generation=9 session_id=7"
        );
        assert!(!gate.contains("private"));
    }

    #[test]
    fn semantic_control_bounds_must_be_current_inside_the_native_client() {
        let client = [240, 180, 1_156, 869];
        assert!(semantic_bounds_center_inside([251, 189, 283, 207], client));
        assert!(!semantic_bounds_center_inside(
            [2_011, 2_033, 2_043, 2_051],
            client
        ));
        assert!(!semantic_bounds_center_inside([0, 0, 0, 10], client));
    }

    #[test]
    fn root_pointer_geometry_requires_intersection_with_a_physical_display() {
        let displays = [[0, 0, 1_920, 1_080], [-1_280, 0, 0, 1_024]];
        assert!(intersects_display_bounds(
            [1_800, 900, 2_000, 1_100],
            &displays
        ));
        assert!(intersects_display_bounds(
            [-1_300, 100, -1_200, 200],
            &displays
        ));
        assert!(!intersects_display_bounds(
            [2_000, 2_000, 2_900, 2_700],
            &displays
        ));
        assert!(!intersects_display_bounds(
            [1_920, 0, 2_000, 100],
            &displays
        ));
    }

    #[test]
    fn failure_trace_excerpt_keeps_typed_fields_and_drops_private_unknown_fields() {
        let excerpt = safe_trace_excerpt(
            "WARN target trace_event=\"authoring\" elapsed_ms=12 edge=ReplyAccepted request_kind=Snapshot secret_marker=NeverPublish label=\"private action title\"\nWARN target trace_event=\"not_a_schema_event\" token=private",
        );
        assert_eq!(
            excerpt,
            "trace_event=authoring elapsed_ms=12 edge=ReplyAccepted request_kind=Snapshot"
        );
        assert!(!excerpt.contains("secret_marker"));
        assert!(!excerpt.contains("private action title"));
        assert!(!excerpt.contains("not_a_schema_event"));
    }

    #[test]
    fn failure_trace_excerpt_keeps_numeric_canvas_scope_without_text_identity() {
        let excerpt = safe_trace_excerpt(
            "WARN target trace_event=\"designer_authoring_control\" elapsed_ms=9 menu_cell_ids_digest=123456 cell_ring_index=1 cell_slot_index=0 private_action=secret",
        );
        assert_eq!(
            excerpt,
            "trace_event=designer_authoring_control elapsed_ms=9 menu_cell_ids_digest=123456 cell_ring_index=1 cell_slot_index=0"
        );
        assert!(!excerpt.contains("private_action"));
    }

    #[test]
    fn failure_trace_excerpt_keeps_typed_authoring_focus_state() {
        let excerpt = safe_trace_excerpt(
            "WARN target trace_event=\"designer_authoring_control\" elapsed_ms=9 focused=true session_id=4 generation=5 private_label=secret",
        );
        assert_eq!(
            excerpt,
            "trace_event=designer_authoring_control elapsed_ms=9 focused=true session_id=4 generation=5"
        );
        assert!(!excerpt.contains("private_label"));

        let malformed = safe_trace_excerpt(
            "WARN target trace_event=\"designer_authoring_control\" elapsed_ms=9 focused=private session_id=4 generation=5",
        );
        assert_eq!(
            malformed,
            "trace_event=designer_authoring_control elapsed_ms=9 session_id=4 generation=5"
        );
    }

    #[test]
    fn failure_trace_excerpt_retains_designer_close_state_transition() {
        let excerpt = safe_trace_excerpt(
            "WARN target trace_event=\"designer_close\" elapsed_ms=7 open=true close_prompt=false dirty=true pending_disposable=false pending_durable=false pending_native_preview=false private_note=unpublished",
        );
        assert_eq!(
            excerpt,
            "trace_event=designer_close elapsed_ms=7 open=true close_prompt=false dirty=true pending_disposable=false pending_durable=false pending_native_preview=false"
        );
        assert!(!excerpt.contains("private_note"));
    }

    #[test]
    fn failure_trace_excerpt_keeps_startup_and_late_root_parking_context() {
        let mut trace =
            String::from("WARN target trace_event=\"trace_ready\" elapsed_ms=0 event_budget=256\n");
        for elapsed_ms in 1..600 {
            trace.push_str(&format!(
                "WARN target trace_event=\"authoring\" elapsed_ms={elapsed_ms} edge=ReplyAccepted request_kind=Snapshot\n"
            ));
        }
        trace.push_str(
            "WARN target trace_event=\"root_command\" elapsed_ms=600 command=ParkingBoundary request_id=1 request_kind=Snapshot session_id=0 generation=0 terminal=true\n",
        );

        let excerpt = safe_trace_excerpt(&trace);
        let lines = excerpt.lines().collect::<Vec<_>>();
        assert!(lines.len() <= MAX_TRACE_EXCERPT);
        assert!(excerpt.len() <= MAX_TRACE_BYTES);
        assert!(lines[0].contains("trace_event=trace_ready"));
        assert!(
            lines
                .last()
                .is_some_and(|line| line.contains("command=ParkingBoundary"))
        );
        assert!(excerpt.contains("elapsed_ms=600"));
        assert!(!excerpt.contains("elapsed_ms=100 "));

        let private_command = sanitize_trace_line(
            "WARN target trace_event=\"root_command\" elapsed_ms=601 command=private_action_name",
        )
        .expect("known event should be retained even if its command value is unknown");
        assert!(!private_command.contains("command="));
    }

    #[test]
    fn file_menu_body_trace_is_sanitized_with_only_typed_state() {
        let line = sanitize_trace_line(
            "WARN target trace_event=\"root_menu_body\" elapsed_ms=17 menu=File entered=true private_label=secret",
        )
        .expect("typed File body trace should be retained");
        assert_eq!(
            line,
            "trace_event=root_menu_body elapsed_ms=17 menu=File entered=true"
        );
        assert!(!line.contains("secret"));
    }

    #[test]
    fn root_menu_state_uses_production_open_transitions_not_uia_subtree_presence() {
        let open_trace = vec![
            "trace_event=\"trace_ready\" elapsed_ms=0".to_string(),
            "trace_event=\"root_menu_interaction\" menu=File hovered=true clicked=true open=true"
                .to_string(),
            "trace_event=\"root_menu_interaction\" menu=Apps hovered=true clicked=true open=true"
                .to_string(),
        ];
        assert_eq!(
            root_menu_state_from_trace(&open_trace),
            Some(RootMenuState {
                file_open: true,
                apps_open: true
            })
        );

        let closed_trace = open_trace
            .into_iter()
            .chain([
                "trace_event=\"root_menu_interaction\" menu=Apps hovered=false clicked=false open=false"
                    .to_string(),
                "trace_event=\"root_menu_interaction\" menu=File hovered=false clicked=false open=false"
                    .to_string(),
            ])
            .collect::<Vec<_>>();
        assert_eq!(
            root_menu_state_from_trace(&closed_trace),
            Some(RootMenuState::default())
        );

        let exhausted = vec![
            "trace_event=\"trace_ready\" elapsed_ms=0".to_string(),
            "trace_event=\"budget_exhausted\" elapsed_ms=100 event_budget=4096".to_string(),
        ];
        assert_eq!(root_menu_state_from_trace(&exhausted), None);
    }

    #[test]
    fn acknowledged_file_click_retries_only_while_production_trace_keeps_menu_closed() {
        let click_closed = vec![
            "trace_event=\"trace_ready\" elapsed_ms=0".to_string(),
            "trace_event=\"root_menu_interaction\" menu=File hovered=true clicked=true open=false"
                .to_string(),
            "trace_event=\"root_menu_interaction\" menu=File hovered=true clicked=false open=false"
                .to_string(),
        ];
        let closed = root_menu_state_from_trace(&click_closed).expect("closed menu state");
        assert_eq!(closed, RootMenuState::default());
        assert!(should_retry_root_file_menu_open(closed));

        let open_file = click_closed
            .into_iter()
            .chain([
                "trace_event=\"root_menu_interaction\" menu=File hovered=true clicked=true open=true"
                    .to_string(),
            ])
            .collect::<Vec<_>>();
        let open = root_menu_state_from_trace(&open_file).expect("open File menu state");
        assert!(root_file_menu_ready_for_apps(open));
        assert!(!should_retry_root_file_menu_open(open));

        let open_apps = open_file
            .into_iter()
            .chain([
                "trace_event=\"root_menu_interaction\" menu=Apps hovered=true clicked=true open=true"
                    .to_string(),
            ])
            .collect::<Vec<_>>();
        let nested = root_menu_state_from_trace(&open_apps).expect("open Apps menu state");
        assert!(!root_file_menu_ready_for_apps(nested));
        assert!(!should_retry_root_file_menu_open(nested));
    }

    #[test]
    fn root_menu_body_trace_proves_rendered_popup_when_button_trace_disagrees() {
        let file_body = vec![
            "trace_event=\"trace_ready\" elapsed_ms=0".to_string(),
            "trace_event=\"root_menu_interaction\" menu=File hovered=true clicked=true open=false"
                .to_string(),
            "trace_event=\"root_menu_body\" menu=File entered=true".to_string(),
        ];
        let state = root_menu_state_from_trace(&file_body).expect("File closure state");
        assert_eq!(
            state,
            RootMenuState {
                file_open: true,
                apps_open: false
            }
        );

        let closed = file_body
            .into_iter()
            .chain(["trace_event=\"root_menu_body\" menu=File entered=false".to_string()])
            .collect::<Vec<_>>();
        assert_eq!(
            root_menu_state_from_trace(&closed),
            Some(RootMenuState::default())
        );
    }

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
