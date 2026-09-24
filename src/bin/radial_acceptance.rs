//! Opt-in native Windows acceptance runner for ROOT and the Radial Designer.

use multi_launcher::radial::{
    RadialDocument, settings as radial_settings, validation::validate as validate_radial_document,
};
use multi_launcher::settings::{LogFile, Settings};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[cfg(windows)]
#[path = "radial_acceptance/native.rs"]
mod native;

const MAX_CASES: usize = 32;
const MAX_ARTIFACTS: usize = 48;
const MAX_PATH_BYTES: usize = 2_048;
const MAX_RESULT_BYTES: usize = 2_048;
const MAX_JSON_REPORT_BYTES: usize = 512 * 1024;
const MAX_TEXT_REPORT_BYTES: usize = 256 * 1024;
const ACCEPTANCE_HOTKEY: &str = "F11";
const ACCEPTANCE_ACTION_COUNT: usize = 64;
const ACCEPTANCE_TARGET_ACTION_INDEX: usize = ACCEPTANCE_ACTION_COUNT - 1;
pub(crate) const CASE_IDS: [&str; 31] = [
    "H0", "H1", "H2", "H3", "H4", "H5", "H6", "D0", "D1", "D2", "D4", "D5", "A0", "A1", "G0", "A2",
    "G1", "G2", "A3", "A4", "A5", "A6", "A7", "A8", "D3", "D6", "D7", "R0", "R1", "R2", "CLEANUP",
];

#[derive(Debug)]
struct Arguments {
    launcher: Option<PathBuf>,
    output: Option<PathBuf>,
    report_file: Option<PathBuf>,
    source_revision: Option<String>,
    keep_profile_on_failure: bool,
    h6_repeat_mode: H6RepeatMode,
    mouse_gesture_mode: MouseGestureMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum H6RepeatMode {
    Immediate,
    Quiescent,
    ProductionOnlyDiagnostic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum MouseGestureMode {
    Enabled,
    DisabledDiagnostic,
}

impl MouseGestureMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Enabled => "enabled",
            Self::DisabledDiagnostic => "disabled_diagnostic",
        }
    }
}

impl H6RepeatMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Immediate => "immediate",
            Self::Quiescent => "quiescent",
            Self::ProductionOnlyDiagnostic => "production_only_diagnostic",
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum CaseStatus {
    Passed,
    Failed,
    Skipped,
    Unsupported,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum FailureStage {
    Environment,
    CandidateStartup,
    WindowDiscovery,
    InputInjection,
    HookAdmission,
    GestureDecision,
    RootCommand,
    NativeRootState,
    DesignerEntry,
    DesignerNativeTarget,
    DesignerFrameworkInput,
    DesignerReadiness,
    DesignerWidget,
    DesignerMutation,
    DesignerPresentation,
    Cleanup,
}

#[derive(Serialize)]
struct CandidateIdentity {
    executable: String,
    sha256: String,
}

#[derive(Serialize)]
struct MonitorIdentity {
    id: u32,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    scale_factor: f32,
}

#[derive(Serialize)]
struct EnvironmentIdentity {
    os_version: String,
    architecture: String,
    runner_process_id: u32,
    runner_sha256: Option<String>,
    child_process_id: Option<u32>,
    child_started_unix_ms: Option<u128>,
    source_revision: Option<String>,
    monitors: Vec<MonitorIdentity>,
}

#[derive(Serialize)]
struct ProfileIdentity {
    mode: &'static str,
    temporary_data_root: String,
    settings_sha256: String,
    radial_sha256: String,
    actions_sha256: String,
    configured_hotkey: &'static str,
    hold_threshold_ms: u64,
}

#[derive(Serialize)]
struct AcceptanceCaseResult {
    id: String,
    status: CaseStatus,
    elapsed_ms: u64,
    expected: String,
    observed: String,
    failure_stage: Option<FailureStage>,
    artifacts: Vec<String>,
}

#[derive(Default, Serialize)]
struct CleanupResult {
    child_closed_normally: bool,
    child_terminated_after_timeout: bool,
    child_owned_windows_closed: bool,
    profile_removed: bool,
    foreground_restore_captured: bool,
    foreground_restore_attempted: bool,
    foreground_restored: bool,
    cursor_restored: bool,
    input_desktop_released: bool,
}

#[derive(Serialize)]
struct AcceptanceReport {
    schema_version: u16,
    run_id: String,
    mode: &'static str,
    started_unix_ms: u128,
    finished_unix_ms: u128,
    copied_profile_status: CopiedProfileStatus,
    h6_repeat_mode: H6RepeatMode,
    mouse_gesture_mode: MouseGestureMode,
    outcome: &'static str,
    candidate: CandidateIdentity,
    environment: EnvironmentIdentity,
    profile: ProfileIdentity,
    cases: Vec<AcceptanceCaseResult>,
    artifacts: Vec<String>,
    cleanup: CleanupResult,
    capacity_saturated: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum CopiedProfileStatus {
    NotRun,
}

impl AcceptanceReport {
    fn push_case(&mut self, mut case: AcceptanceCaseResult) {
        let is_final_integrity_case = case.id == "R0";
        let reserved_for_r0 = !is_final_integrity_case && self.cases.len() >= MAX_CASES - 1;
        if self.cases.len() < MAX_CASES && !reserved_for_r0 {
            if is_final_integrity_case && self.capacity_saturated {
                case.status = CaseStatus::Failed;
                case.failure_stage = Some(FailureStage::Environment);
                case.observed =
                    "bounded report capacity was exceeded; evidence omissions are explicit".into();
            }
            self.cases.push(case);
        } else {
            self.mark_capacity_saturated();
        }
    }

    fn push_artifact(&mut self, path: impl AsRef<str>) {
        if self.artifacts.len() < MAX_ARTIFACTS {
            self.artifacts
                .push(bounded_text(path.as_ref(), MAX_PATH_BYTES));
        } else {
            self.mark_capacity_saturated();
        }
    }

    fn mark_capacity_saturated(&mut self) {
        self.capacity_saturated = true;
        if let Some(case) = self.cases.iter_mut().find(|case| case.id == "R0") {
            case.status = CaseStatus::Failed;
            case.failure_stage = Some(FailureStage::Environment);
            case.observed =
                "bounded report capacity was exceeded; one or more evidence records were omitted"
                    .into();
        }
    }

    fn passed(&self) -> bool {
        !self.capacity_saturated
            && self.cases.len() == CASE_IDS.len()
            && self.cases.iter().enumerate().all(|(index, case)| {
                self.cases[..index]
                    .iter()
                    .all(|previous| previous.id != case.id)
            })
            && self
                .cases
                .iter()
                .all(|case| matches!(case.status, CaseStatus::Passed))
            && self.cleanup.child_closed_normally
            && self.cleanup.child_owned_windows_closed
            && self.cleanup.profile_removed
            && (!self.cleanup.foreground_restore_captured || self.cleanup.foreground_restored)
            && self.cleanup.cursor_restored
            && self.cleanup.input_desktop_released
    }
}

struct DeterministicFixture {
    settings_json: Vec<u8>,
    radial_json: Vec<u8>,
    actions_json: Vec<u8>,
    hold_threshold_ms: u64,
}

enum ParseResult {
    Help,
    Run(Arguments),
}

fn main() -> ExitCode {
    match parse_arguments(std::env::args_os().skip(1)) {
        Ok(ParseResult::Help) => {
            print_usage();
            ExitCode::SUCCESS
        }
        Ok(ParseResult::Run(arguments)) => match run(arguments) {
            Ok((path, passed)) => {
                println!(
                    "{} radial acceptance report: {}",
                    if passed { "PASS" } else { "FAIL" },
                    path.display()
                );
                if passed {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::FAILURE
                }
            }
            Err(error) => {
                eprintln!("radial_acceptance could not start: {error}");
                ExitCode::FAILURE
            }
        },
        Err(error) => {
            eprintln!("{error}");
            print_usage();
            ExitCode::from(2)
        }
    }
}

fn parse_arguments(args: impl IntoIterator<Item = OsString>) -> Result<ParseResult, String> {
    let mut launcher = None;
    let mut output = None;
    let mut report_file = None;
    let mut source_revision = None;
    let mut keep_profile_on_failure = false;
    let mut h6_repeat_mode = H6RepeatMode::Quiescent;
    let mut mouse_gesture_mode = MouseGestureMode::Enabled;
    let mut args = args.into_iter();

    while let Some(argument) = args.next() {
        match argument.to_str() {
            Some("--help" | "-h") => return Ok(ParseResult::Help),
            Some("--keep-profile-on-failure") => keep_profile_on_failure = true,
            Some("--launcher" | "--candidate") => {
                launcher = Some(next_path(&mut args, "--launcher")?);
            }
            Some("--output") => output = Some(next_path(&mut args, "--output")?),
            Some("--report") => report_file = Some(next_path(&mut args, "--report")?),
            Some("--source-revision") => {
                let value = args
                    .next()
                    .ok_or_else(|| "--source-revision requires a value".to_string())?;
                source_revision = Some(
                    value
                        .into_string()
                        .map_err(|_| "--source-revision must be valid UTF-8".to_string())?,
                );
            }
            Some("--h6-repeat") => {
                let value = args.next().ok_or_else(|| {
                    "--h6-repeat requires immediate, quiescent, or production-only-diagnostic"
                        .to_string()
                })?;
                h6_repeat_mode = match value.to_str() {
                    Some("immediate") => H6RepeatMode::Immediate,
                    Some("quiescent") => H6RepeatMode::Quiescent,
                    Some("production-only-diagnostic") => H6RepeatMode::ProductionOnlyDiagnostic,
                    _ => {
                        return Err("--h6-repeat must be immediate, quiescent, or production-only-diagnostic".into());
                    }
                };
            }
            Some("--mouse-gestures") => {
                let value = args.next().ok_or_else(|| {
                    "--mouse-gestures requires enabled or disabled-diagnostic".to_string()
                })?;
                mouse_gesture_mode = match value.to_str() {
                    Some("enabled") => MouseGestureMode::Enabled,
                    Some("disabled-diagnostic") => MouseGestureMode::DisabledDiagnostic,
                    _ => {
                        return Err(
                            "--mouse-gestures must be enabled or disabled-diagnostic".into()
                        );
                    }
                };
            }
            Some("--profile-copy") => {
                return Err(
                    "copied-profile execution is reserved for the authoring acceptance milestone"
                        .into(),
                );
            }
            Some(option) => return Err(format!("unknown option: {option}")),
            None => return Err("command-line options must be valid UTF-8".to_string()),
        }
    }

    if output.is_some() == report_file.is_some() {
        return Err("specify exactly one of --output <directory> or --report <file>".into());
    }
    let source_revision = match source_revision {
        Some(revision) => Some(validate_source_revision(revision)?),
        None => Some(derive_source_revision()?),
    };
    Ok(ParseResult::Run(Arguments {
        launcher,
        output,
        report_file,
        source_revision,
        keep_profile_on_failure,
        h6_repeat_mode,
        mouse_gesture_mode,
    }))
}

fn validate_source_revision(revision: String) -> Result<String, String> {
    if revision.is_empty()
        || revision.len() > 160
        || !revision.bytes().all(|byte| byte.is_ascii_graphic())
    {
        return Err(
            "source revision must be a non-empty printable token of at most 160 bytes".into(),
        );
    }
    Ok(revision)
}

fn derive_source_revision() -> Result<String, String> {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
    let revision = Command::new("git")
        .args(["rev-parse", "--verify", "HEAD"])
        .current_dir(repository)
        .output()
        .map_err(|error| format!("derive source revision with git: {error}"))?;
    if !revision.status.success() {
        return Err("derive source revision: git rev-parse failed".into());
    }
    let commit = String::from_utf8(revision.stdout)
        .map_err(|_| "derive source revision: git returned non-UTF-8 commit id".to_string())?;
    let commit = commit.trim();
    if commit.len() != 40 || !commit.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("derive source revision: git returned a malformed commit id".into());
    }
    let status = Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=all"])
        .current_dir(repository)
        .output()
        .map_err(|error| format!("inspect source worktree state: {error}"))?;
    if !status.status.success() {
        return Err("inspect source worktree state: git status failed".into());
    }
    let suffix = if status.stdout.is_empty() {
        "+clean"
    } else {
        "+dirty"
    };
    Ok(format!("{commit}{suffix}"))
}

fn next_path(args: &mut impl Iterator<Item = OsString>, option: &str) -> Result<PathBuf, String> {
    args.next()
        .map(PathBuf::from)
        .ok_or_else(|| format!("{option} requires a path"))
}

fn run(arguments: Arguments) -> Result<(PathBuf, bool), String> {
    #[cfg(not(windows))]
    {
        let _ = arguments;
        return Err("native radial acceptance is available only on Windows".into());
    }

    #[cfg(windows)]
    run_windows(arguments)
}

#[cfg(windows)]
fn run_windows(arguments: Arguments) -> Result<(PathBuf, bool), String> {
    let run_started = Instant::now();
    let started_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let output = prepare_output_directory(&arguments)?;
    let report_path = arguments
        .report_file
        .as_ref()
        .cloned()
        .unwrap_or_else(|| output.join("report.json"));
    let candidate = inspect_candidate(arguments.launcher.as_deref())?;
    let runner_path = std::env::current_exe().ok();
    let runner_sha256 = runner_path
        .as_deref()
        .and_then(|path| sha256_file(path).ok());
    let run_id = format!(
        "{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    );
    let profile = tempfile::Builder::new()
        .prefix("multi-launcher-radial-acceptance-")
        .tempdir()
        .map_err(|error| format!("create isolated profile directory: {error}"))?;
    let profile_path = profile.path().to_path_buf();
    let log_path = profile_path.join("acceptance.log");
    let fixture = deterministic_fixture(&log_path, arguments.mouse_gesture_mode)?;
    write_new(&profile_path.join("settings.json"), &fixture.settings_json)?;
    write_new(&profile_path.join("radial.json"), &fixture.radial_json)?;
    write_new(&profile_path.join("actions.json"), &fixture.actions_json)?;
    let settings_sha256 = sha256_bytes(&fixture.settings_json);
    let radial_sha256 = sha256_bytes(&fixture.radial_json);
    let actions_sha256 = sha256_bytes(&fixture.actions_json);

    let mut report = AcceptanceReport {
        schema_version: 5,
        run_id,
        mode: "native_windows",
        started_unix_ms,
        finished_unix_ms: 0,
        copied_profile_status: CopiedProfileStatus::NotRun,
        h6_repeat_mode: arguments.h6_repeat_mode,
        mouse_gesture_mode: arguments.mouse_gesture_mode,
        outcome: "running",
        candidate,
        environment: EnvironmentIdentity {
            os_version: sysinfo::System::long_os_version()
                .unwrap_or_else(|| "unknown Windows version".to_string()),
            architecture: bounded_text(std::env::consts::ARCH, 64),
            runner_process_id: std::process::id(),
            runner_sha256,
            child_process_id: None,
            child_started_unix_ms: None,
            source_revision: arguments.source_revision,
            monitors: monitor_inventory(),
        },
        profile: ProfileIdentity {
            mode: "deterministic_fixture",
            temporary_data_root: bounded_text(&profile_path.to_string_lossy(), MAX_PATH_BYTES),
            settings_sha256,
            radial_sha256,
            actions_sha256,
            configured_hotkey: ACCEPTANCE_HOTKEY,
            hold_threshold_ms: fixture.hold_threshold_ms,
        },
        cases: Vec::with_capacity(10),
        artifacts: Vec::new(),
        cleanup: CleanupResult::default(),
        capacity_saturated: false,
    };
    let profile_hashes_match_at_launch =
        validate_profile_hashes(&profile_path, &report.profile).is_ok();

    let runner_log = output.join("runner.log");
    let mut log = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&runner_log)
        .map_err(|error| format!("create runner log: {error}"))?;
    let _ = writeln!(log, "H6 repeat mode: {}", arguments.h6_repeat_mode.as_str());
    let _ = writeln!(
        log,
        "mouse gesture mode: {}",
        arguments.mouse_gesture_mode.as_str()
    );
    report.push_artifact(runner_log.to_string_lossy());
    let candidate_executable = report.candidate.executable.clone();
    let driver_profile = profile_path.clone();
    let driver_output = output.clone();
    let driver_trace = log_path.clone();
    let hold_threshold_ms = fixture.hold_threshold_ms;
    let (mut report, mut log, input_desktop_handle) = std::thread::Builder::new()
        .name("radial-acceptance-native-driver".to_string())
        .spawn(move || {
            // The terminal's main thread can already be bound to a private desktop. Windows
            // only permits SetThreadDesktop before a thread creates windows or installs hooks,
            // so all native work starts on this fresh thread and attaches before UIA or HWND use.
            match native::attach_to_input_desktop() {
                Ok((observed, desktop_attachment)) => {
                    let _ = writeln!(log, "input desktop attached: {observed}");
            let before_foreground = native::capture_foreground();
            let before_cursor = native::cursor_position();
            report.cleanup.foreground_restore_captured = !before_foreground.0.is_invalid();
                    let focus_anchor = native::run_suite(
                        &candidate_executable,
                        &driver_profile,
                        &driver_output,
                        &driver_trace,
                        hold_threshold_ms,
                        arguments.h6_repeat_mode,
                        before_cursor.as_ref().ok().copied(),
                        &desktop_attachment,
                        &mut report,
                        &mut log,
                    );

                    match &before_cursor {
                        Ok(point) => {
                            let point = *point;
                            let already_restored = native::cursor_position().is_ok_and(|current| {
                                current.x == point.x && current.y == point.y
                            });
                            if already_restored {
                                report.cleanup.cursor_restored = true;
                            } else {
                                match native::set_cursor_position(point) {
                                    Ok(()) => {
                                        report.cleanup.cursor_restored = true;
                                        let _ = writeln!(
                                            log,
                                            "cursor restoration before foreground reached ({},{})",
                                            point.x,
                                            point.y
                                        );
                                    }
                                    Err(error) => {
                                        let _ = writeln!(log, "cursor restoration failed: {error}");
                                    }
                                }
                            }
                        }
                        Err(error) => {
                            let _ = writeln!(
                                log,
                                "cursor position capture failed; restoration is unverified: {error}"
                            );
                        }
                    }
                    report.cleanup.foreground_restored = if before_foreground.0.is_invalid() {
                        let _ = writeln!(log, "foreground restoration skipped: no foreground HWND was captured");
                        false
                    } else {
                        report.cleanup.foreground_restore_attempted = true;
                        if let Some(anchor) = focus_anchor.as_ref() {
                            if let Err(error) = anchor.focus() {
                                let _ = writeln!(log, "could not focus retained runner anchor before foreground restore: {error}");
                            }
                        }
                        match native::restore_foreground(before_foreground.0, before_foreground.1) {
                            Ok(()) => {
                                let _ = writeln!(
                                    log,
                                    "foreground restored to captured HWND={} PID={}",
                                    before_foreground.0.0 as usize,
                                    before_foreground.1
                                );
                                true
                            }
                            Err(error) => {
                                let _ = writeln!(
                                    log,
                                    "foreground restoration failed for captured HWND={} PID={}: {error}",
                                    before_foreground.0.0 as usize,
                                    before_foreground.1
                                );
                                false
                            }
                        }
                    };
                    if let Ok(point) = &before_cursor {
                        let point = *point;
                        let restored_after_foreground = native::cursor_position().is_ok_and(|current| {
                            current.x.abs_diff(point.x) <= 1 && current.y.abs_diff(point.y) <= 1
                        });
                        report.cleanup.cursor_restored = restored_after_foreground;
                        if !restored_after_foreground {
                            let observed = native::cursor_position()
                                .map(|current| format!("({}, {})", current.x, current.y))
                                .unwrap_or_else(|error| format!("unavailable: {error}"));
                            let _ = writeln!(
                                log,
                                "cursor changed during foreground restoration: expected=({},{}), observed={observed}",
                                point.x,
                                point.y
                            );
                            report.cleanup.cursor_restored = match native::set_cursor_position(point) {
                                Ok(()) => native::cursor_position().is_ok_and(|current| {
                                    current.x.abs_diff(point.x) <= 1
                                        && current.y.abs_diff(point.y) <= 1
                                }),
                                Err(error) => {
                                    let _ = writeln!(log, "post-foreground cursor restoration failed: {error}");
                                    false
                                }
                            };
                        }
                    }
                    drop(focus_anchor);
                    let handle = desktop_attachment.release_for_thread_exit();
                    let _ = writeln!(
                        log,
                        "native driver thread leaving input desktop; desktop handle deferred until thread exit"
                    );
                    (report, log, handle)
                }
                Err(error) => {
                    let _ = writeln!(log, "input desktop attachment failed: {error}");
                    native::record_environment_failure(
                        error,
                        &mut report,
                        &driver_output,
                        &driver_trace,
                        &mut log,
                    );
                    (report, log, None)
                }
            }
        })
        .map_err(|error| format!("start native driver thread: {error}"))?
        .join()
        .map_err(|_| "native driver thread panicked".to_string())?;

    report.cleanup.input_desktop_released = match input_desktop_handle {
        Some(handle) => match native::close_input_desktop_after_driver_exit(handle) {
            Ok(()) => true,
            Err(error) => {
                let _ = writeln!(log, "input desktop release failed: {error}");
                false
            }
        },
        None => true,
    };

    let failed = report
        .cases
        .iter()
        .any(|case| !matches!(case.status, CaseStatus::Passed));
    if failed && arguments.keep_profile_on_failure {
        let retained = profile.keep();
        report.profile.temporary_data_root =
            bounded_text(&retained.to_string_lossy(), MAX_PATH_BYTES);
        report.cleanup.profile_removed = false;
        let _ = writeln!(log, "failure profile retained at {}", retained.display());
    } else {
        let cleanup = profile.close();
        report.cleanup.profile_removed = cleanup.is_ok();
        if let Err(error) = cleanup {
            report.push_artifact(format!("profile cleanup failed: {error}"));
        }
    }

    report.finished_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    ensure_final_acceptance_cases(
        &mut report,
        &profile_path,
        profile_hashes_match_at_launch,
        run_started,
    );
    report.outcome = if report.passed() { "passed" } else { "failed" };
    let passed = report.passed();
    write_report(&report_path, &report)?;
    let text_path = report_path.with_extension("txt");
    write_text_report(&text_path, &report)?;
    println!("Acceptance report: {}", report_path.display());
    Ok((report_path, passed))
}

#[cfg(windows)]
fn validate_profile_hashes(profile_path: &Path, identity: &ProfileIdentity) -> Result<(), String> {
    for (name, file, expected) in [
        ("settings", "settings.json", &identity.settings_sha256),
        ("radial document", "radial.json", &identity.radial_sha256),
        ("actions", "actions.json", &identity.actions_sha256),
    ] {
        let actual = sha256_file(&profile_path.join(file))
            .map_err(|error| format!("hash deterministic {name} fixture: {error}"))?;
        if &actual != expected {
            return Err(format!(
                "deterministic {name} profile hash changed during acceptance"
            ));
        }
    }
    Ok(())
}

#[cfg(windows)]
fn ensure_final_acceptance_cases(
    report: &mut AcceptanceReport,
    profile_path: &Path,
    profile_hashes_match: bool,
    run_started: Instant,
) {
    if !report.cases.iter().any(|case| case.id == "R1") {
        push_final_case(
            report,
            "R1",
            CaseStatus::Failed,
            "controlled artifact capture was not run because the native child was unavailable",
            Some(FailureStage::CandidateStartup),
            Vec::new(),
            run_started,
        );
    }

    let cleanup = &report.cleanup;
    let foreground_ok = !cleanup.foreground_restore_captured
        || (cleanup.foreground_restore_attempted && cleanup.foreground_restored);
    let r2_passed = report.environment.child_process_id.is_some()
        && cleanup.child_closed_normally
        && cleanup.child_owned_windows_closed
        && cleanup.profile_removed
        && cleanup.cursor_restored
        && cleanup.input_desktop_released
        && foreground_ok;
    let foreground_observed = if !cleanup.foreground_restore_captured {
        "no foreground HWND was present to restore"
    } else if cleanup.foreground_restored {
        "captured foreground HWND/PID restored"
    } else {
        "captured foreground HWND/PID restoration was not verified"
    };
    push_final_case(
        report,
        "R2",
        if r2_passed {
            CaseStatus::Passed
        } else {
            CaseStatus::Failed
        },
        &format!(
            "child_closed_normally={}, child_owned_windows_closed={}, temp_profile_removed={}, cursor_restored={}, input_desktop_released={}, foreground_restore_captured={}, foreground_restore_attempted={}, foreground_restored={} ({foreground_observed})",
            cleanup.child_closed_normally,
            cleanup.child_owned_windows_closed,
            cleanup.profile_removed,
            cleanup.cursor_restored,
            cleanup.input_desktop_released,
            cleanup.foreground_restore_captured,
            cleanup.foreground_restore_attempted,
            cleanup.foreground_restored,
        ),
        (!r2_passed).then_some(FailureStage::Cleanup),
        Vec::new(),
        run_started,
    );

    let validation = validate_r0_report(report, profile_path, profile_hashes_match);
    let (status, observed, failure_stage) = match validation {
        Ok(observed) => (CaseStatus::Passed, observed, None),
        Err(error) => (
            CaseStatus::Failed,
            format!("report integrity validation failed: {error}"),
            Some(FailureStage::Environment),
        ),
    };
    push_final_case(
        report,
        "R0",
        status,
        &observed,
        failure_stage,
        Vec::new(),
        run_started,
    );
}

#[cfg(windows)]
fn validate_r0_report(
    report: &AcceptanceReport,
    _profile_path: &Path,
    profile_hashes_match: bool,
) -> Result<String, String> {
    if report.capacity_saturated {
        return Err("bounded report capacity was exceeded".into());
    }
    if !profile_hashes_match {
        return Err(
            "on-disk deterministic profile hashes did not match the recorded fixture".into(),
        );
    }
    if report.copied_profile_status != CopiedProfileStatus::NotRun {
        return Err("copied-profile status is not truthfully not_run".into());
    }
    let revision = report
        .environment
        .source_revision
        .as_deref()
        .filter(|revision| !revision.trim().is_empty())
        .ok_or_else(|| "source revision identity was not supplied".to_string())?;
    if revision.len() > 160 || !revision.bytes().all(|byte| byte.is_ascii_graphic()) {
        return Err("source revision identity is not a bounded printable token".into());
    }
    for (label, hash) in [
        ("candidate", report.candidate.sha256.as_str()),
        (
            "runner",
            report
                .environment
                .runner_sha256
                .as_deref()
                .ok_or_else(|| "runner executable hash is unavailable".to_string())?,
        ),
        ("settings profile", report.profile.settings_sha256.as_str()),
        ("radial profile", report.profile.radial_sha256.as_str()),
        ("actions profile", report.profile.actions_sha256.as_str()),
    ] {
        if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(format!("{label} SHA-256 identity is malformed"));
        }
    }
    let candidate_path = Path::new(&report.candidate.executable);
    let candidate_actual = sha256_file(candidate_path)
        .map_err(|error| format!("rehash source-matched candidate executable: {error}"))?;
    if candidate_actual != report.candidate.sha256 {
        return Err("candidate executable hash changed during acceptance".into());
    }
    let runner_path = std::env::current_exe()
        .map_err(|error| format!("resolve source-matched runner executable: {error}"))?;
    let runner_actual = sha256_file(&runner_path)
        .map_err(|error| format!("rehash source-matched runner executable: {error}"))?;
    if report.environment.runner_sha256.as_deref() != Some(runner_actual.as_str()) {
        return Err("runner executable hash changed during acceptance".into());
    }
    if report.started_unix_ms == 0
        || report.finished_unix_ms < report.started_unix_ms
        || report.cases.len() + 1 != CASE_IDS.len()
    {
        return Err("report elapsed time or pre-final case count is invalid".into());
    }
    for (index, case) in report.cases.iter().enumerate() {
        if !CASE_IDS.contains(&case.id.as_str())
            || report.cases[..index]
                .iter()
                .any(|previous| previous.id == case.id)
            || case.expected.is_empty()
            || case.observed.is_empty()
            || case.expected.len() > MAX_RESULT_BYTES
            || case.observed.len() > MAX_RESULT_BYTES
            || (matches!(case.status, CaseStatus::Failed) != case.failure_stage.is_some())
            || case
                .artifacts
                .iter()
                .any(|path| path.is_empty() || path.len() > MAX_PATH_BYTES)
        {
            return Err(format!(
                "case {} is missing typed stage/evidence or has invalid bounds",
                case.id
            ));
        }
        validate_required_case_evidence(&case.id, case.status, &case.observed)?;
    }
    if !CASE_IDS
        .iter()
        .filter(|id| **id != "R0")
        .all(|id| report.cases.iter().any(|case| case.id == *id))
    {
        return Err("required native case identifiers are missing before R0".into());
    }
    Ok(format!(
        "source revision and candidate/runner hashes verified; deterministic settings/radial/actions hashes match; copied_profile_status=not_run; {} bounded typed case records have unique IDs and evidence fields; elapsed_ms={}",
        report.cases.len(),
        report.finished_unix_ms - report.started_unix_ms
    ))
}

fn validate_required_case_evidence(
    id: &str,
    status: CaseStatus,
    observed: &str,
) -> Result<(), String> {
    let Some(required) = required_case_evidence(id) else {
        return Ok(());
    };
    if !matches!(status, CaseStatus::Passed) {
        return Err(format!("case {id} is not passed"));
    }
    if observed.len() >= MAX_RESULT_BYTES || !observed.starts_with("evidence:v1;") {
        return Err(format!(
            "case {id} report summary is missing its versioned, untruncated evidence prefix"
        ));
    }
    if let Some(missing) = required.iter().find(|fact| !observed.contains(**fact)) {
        return Err(format!(
            "case {id} report summary omitted required fact {missing}"
        ));
    }
    Ok(())
}

fn required_case_evidence(id: &str) -> Option<&'static [&'static str]> {
    match id {
        "D2" => Some(&[
            "text_edit=restored",
            "tab_focus=menu_combo",
            "unsaved=false",
        ]),
        "A2" => Some(&[
            "geometry=[8,10]",
            "candidate_ids_preserved=true",
            "committed=true",
        ]),
        "G1" => Some(&["overflow_root=[8,1]/9", "stable_ids=true"]),
        "A3" => Some(&[
            "blank_cell_selected=true",
            "catalog_rank_gt_50=true",
            "searched_action_assigned=true",
        ]),
        "A5" => Some(&[
            "glow=true->false",
            "preview_reply=accepted",
            "preview_rendered=true",
        ]),
        "A6" => Some(&[
            "typed_radial=decoded",
            "authored_geometry=[8,10]",
            "action_binding=true",
            "after_action=close_tree",
            "overflow_root=[8,1]/9",
            "glow=false",
        ]),
        "A7" => Some(&["undo_restored=false", "redo_restored=true"]),
        "D3" => Some(&[
            "root_hidden=true",
            "designer_responsive=true",
            "preview_stop=accepted",
            "root_shown=true",
            "hook_pairs=true",
        ]),
        "D6" => Some(&[
            "keep_editing=retained_dirty",
            "draft_glow=true",
            "discard=saved_json_unchanged",
        ]),
        "D7" => Some(&[
            "pending_request=true",
            "cancelled_before_prompt=true",
            "late_reply=rejected",
            "stop=accepted",
            "no_reopen=1s",
            "marker_clean=true",
        ]),
        _ => None,
    }
}

#[cfg(windows)]
fn push_final_case(
    report: &mut AcceptanceReport,
    id: &str,
    status: CaseStatus,
    observed: &str,
    failure_stage: Option<FailureStage>,
    artifacts: Vec<String>,
    run_started: Instant,
) {
    report.push_case(AcceptanceCaseResult {
        id: id.into(),
        status,
        elapsed_ms: u64::try_from(run_started.elapsed().as_millis()).unwrap_or(u64::MAX),
        expected: expected_final_case(id).into(),
        observed: bounded_text(observed, MAX_RESULT_BYTES),
        failure_stage,
        artifacts,
    });
}

fn expected_final_case(id: &str) -> &'static str {
    match id {
        "R0" => {
            "valid bounded JSON and text reports identify source, profile, hashes, elapsed time, and evidence"
        }
        "R1" => {
            "controlled harness failure writes bounded privacy-safe trace and owned screenshot evidence"
        }
        "R2" => "child process, HWNDs, temp profile, and native input state are cleaned up",
        _ => "required native acceptance case is recorded",
    }
}

#[cfg(windows)]
fn prepare_output_directory(arguments: &Arguments) -> Result<PathBuf, String> {
    if let Some(report) = &arguments.report_file {
        let parent = report
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        if !parent.is_dir() {
            return Err("--report parent directory must already exist".into());
        }
        return parent
            .canonicalize()
            .map_err(|error| format!("resolve report parent: {error}"));
    }
    let output = arguments
        .output
        .as_deref()
        .ok_or_else(|| "--output is required".to_string())?;
    fs::create_dir_all(output).map_err(|error| format!("create output directory: {error}"))?;
    let output = output
        .canonicalize()
        .map_err(|error| format!("resolve output directory: {error}"))?;
    let mut entries =
        fs::read_dir(&output).map_err(|error| format!("inspect output directory: {error}"))?;
    if entries.next().is_some() {
        return Err("output directory must be empty so run artifacts are never overwritten".into());
    }
    Ok(output)
}

fn deterministic_fixture(
    log_path: &Path,
    mouse_gesture_mode: MouseGestureMode,
) -> Result<DeterministicFixture, String> {
    let mut settings = Settings::default();
    settings.hotkey = Some(ACCEPTANCE_HOTKEY.to_string());
    settings.help_hotkey = None;
    settings.quit_hotkey = None;
    settings.debug_logging = true;
    settings.log_file = Some(LogFile::Path(log_path.to_string_lossy().to_string()));
    settings.follow_mouse = false;
    settings.static_location_enabled = true;
    settings.static_pos = Some((240, 180));
    settings.static_size = Some((900, 650));
    settings.window_size = Some((900, 650));
    settings.radial.enabled = true;
    settings.radial.shared_tap_hold = true;
    if mouse_gesture_mode == MouseGestureMode::DisabledDiagnostic {
        settings.plugin_settings.insert(
            "mouse_gestures".into(),
            serde_json::json!({ "enabled": false }),
        );
    }
    let document = RadialDocument::starter();
    let actions = (0..ACCEPTANCE_ACTION_COUNT)
        .map(|index| multi_launcher::actions::Action {
            label: format!("Radial Acceptance Harmless Action {index:03}"),
            desc: "Deterministic native authoring fixture".into(),
            action: format!("radial_acceptance_harmless_{index:03}"),
            args: None,
        })
        .collect::<Vec<_>>();
    validate_radial_document(&document)
        .map_err(|error| format!("starter radial document is invalid: {error:?}"))?;
    let reserved = [("launcher", settings.hotkey.as_deref())]
        .into_iter()
        .filter_map(|(owner, chord)| chord.map(|chord| (owner.to_string(), chord.to_string())))
        .collect::<Vec<_>>();
    let issues = radial_settings::validate(&settings.radial, &document, &reserved);
    if !issues.is_empty() {
        return Err(format!("radial settings are invalid: {issues:?}"));
    }
    multi_launcher::hotkey::parse_hotkey(ACCEPTANCE_HOTKEY)
        .ok_or_else(|| "the deterministic F11 acceptance chord is unsupported".to_string())?;

    Ok(DeterministicFixture {
        settings_json: serde_json::to_vec_pretty(&settings)
            .map_err(|error| format!("serialize settings: {error}"))?,
        radial_json: serde_json::to_vec_pretty(&document)
            .map_err(|error| format!("serialize radial document: {error}"))?,
        actions_json: serde_json::to_vec_pretty(&actions)
            .map_err(|error| format!("serialize custom action fixture: {error}"))?,
        hold_threshold_ms: settings.radial.hold_threshold_ms,
    })
}

fn inspect_candidate(explicit_path: Option<&Path>) -> Result<CandidateIdentity, String> {
    let path = if let Some(path) = explicit_path {
        path.to_path_buf()
    } else {
        let runner = std::env::current_exe()
            .map_err(|error| format!("find radial_acceptance executable: {error}"))?;
        runner
            .parent()
            .ok_or_else(|| "runner executable has no parent directory".to_string())?
            .join("multi_launcher.exe")
    };
    let metadata = fs::symlink_metadata(&path)
        .map_err(|error| format!("launcher candidate is not accessible: {error}"))?;
    if !metadata.is_file() || is_reparse_point(&metadata) {
        return Err("launcher candidate must be a regular non-reparse executable".into());
    }
    if !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
    {
        return Err("launcher candidate must have an .exe extension".into());
    }
    let executable = path
        .canonicalize()
        .map_err(|error| format!("resolve launcher candidate: {error}"))?;
    let sha256 =
        sha256_file(&executable).map_err(|error| format!("hash launcher candidate: {error}"))?;
    Ok(CandidateIdentity {
        executable: bounded_text(&executable.to_string_lossy(), MAX_PATH_BYTES),
        sha256,
    })
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| {
            format!(
                "create {}: {error}",
                path.file_name().unwrap_or_default().to_string_lossy()
            )
        })?;
    output.write_all(bytes).map_err(|error| {
        format!(
            "write {}: {error}",
            path.file_name().unwrap_or_default().to_string_lossy()
        )
    })
}

fn write_report(path: &Path, report: &AcceptanceReport) -> Result<(), String> {
    let expected_value = serde_json::to_value(report)
        .map_err(|error| format!("build acceptance report JSON value: {error}"))?;
    let bytes = serde_json::to_vec_pretty(report)
        .map_err(|error| format!("serialize acceptance report: {error}"))?;
    if bytes.is_empty() || bytes.len() > MAX_JSON_REPORT_BYTES {
        return Err(format!(
            "serialized acceptance JSON is empty or exceeds {} bytes",
            MAX_JSON_REPORT_BYTES
        ));
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("create acceptance report: {error}"))?;
    file.write_all(&bytes)
        .map_err(|error| format!("write acceptance report: {error}"))?;
    drop(file);
    let stored = fs::read(path).map_err(|error| format!("read acceptance report back: {error}"))?;
    let decoded: serde_json::Value = serde_json::from_slice(&stored)
        .map_err(|error| format!("validate persisted acceptance JSON: {error}"))?;
    if decoded != expected_value {
        return Err("persisted JSON report differs from the serialized report model".into());
    }
    Ok(())
}

fn write_text_report(path: &Path, report: &AcceptanceReport) -> Result<(), String> {
    let contents = render_text_report(report);
    if contents.is_empty() || contents.len() > MAX_TEXT_REPORT_BYTES {
        return Err(format!(
            "serialized text report is empty or exceeds {} bytes",
            MAX_TEXT_REPORT_BYTES
        ));
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("create text report: {error}"))?;
    file.write_all(contents.as_bytes())
        .map_err(|error| format!("write text report: {error}"))?;
    drop(file);
    let stored =
        fs::read_to_string(path).map_err(|error| format!("read text report back: {error}"))?;
    if stored != contents || !stored.contains("Copied profile: not_run") {
        return Err(
            "persisted text report does not match the complete bounded report model".into(),
        );
    }
    Ok(())
}

fn render_text_report(report: &AcceptanceReport) -> String {
    let mut contents = String::new();
    contents.push_str(&format!("Radial native acceptance: {}\n", report.outcome));
    contents.push_str(&format!("Candidate SHA-256: {}\n", report.candidate.sha256));
    contents.push_str(&format!(
        "Runner SHA-256: {:?}\n",
        report.environment.runner_sha256
    ));
    contents.push_str(&format!(
        "Source revision: {:?}\n",
        report.environment.source_revision
    ));
    contents.push_str(&format!(
        "Child PID: {:?}\n",
        report.environment.child_process_id
    ));
    contents.push_str(&format!("Hotkey: {}\n", report.profile.configured_hotkey));
    contents.push_str(&format!("Started Unix ms: {}\n", report.started_unix_ms));
    contents.push_str(&format!("Finished Unix ms: {}\n", report.finished_unix_ms));
    let copied_profile_status = match report.copied_profile_status {
        CopiedProfileStatus::NotRun => "not_run",
    };
    contents.push_str(&format!("Copied profile: {copied_profile_status}\n"));
    contents.push_str(&format!(
        "Profile SHA-256: settings={}, radial={}, actions={}\n",
        report.profile.settings_sha256, report.profile.radial_sha256, report.profile.actions_sha256
    ));
    for case in &report.cases {
        contents.push_str(&format!(
            "{}: {:?}{} elapsed_ms={} — {}\n",
            case.id,
            case.status,
            case.failure_stage
                .map(|stage| format!(" ({stage:?})"))
                .unwrap_or_default(),
            case.elapsed_ms,
            bounded_text(&case.observed, MAX_RESULT_BYTES)
        ));
    }
    contents.push_str(&format!(
        "Cleanup: child_closed_normally={}, child_owned_windows_closed={}, profile_removed={}, foreground_restore_captured={}, foreground_restore_attempted={}, foreground_restored={}, cursor_restored={}, input_desktop_released={}\n",
        report.cleanup.child_closed_normally,
        report.cleanup.child_owned_windows_closed,
        report.cleanup.profile_removed,
        report.cleanup.foreground_restore_captured,
        report.cleanup.foreground_restore_attempted,
        report.cleanup.foreground_restored,
        report.cleanup.cursor_restored,
        report.cleanup.input_desktop_released
    ));
    contents
}

fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 32 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(hex::encode(digest.finalize()))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn bounded_text(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let mut end = max_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

#[cfg(windows)]
fn monitor_inventory() -> Vec<MonitorIdentity> {
    screenshots::Screen::all()
        .unwrap_or_default()
        .into_iter()
        .take(16)
        .map(|screen| MonitorIdentity {
            id: screen.display_info.id,
            x: screen.display_info.x,
            y: screen.display_info.y,
            width: screen.display_info.width,
            height: screen.display_info.height,
            scale_factor: screen.display_info.scale_factor,
        })
        .collect()
}

#[cfg(windows)]
fn is_reparse_point(metadata: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_point(metadata: &Metadata) -> bool {
    metadata.file_type().is_symlink()
}

fn print_usage() {
    println!(
        "Usage: radial_acceptance [--launcher <source-matched multi_launcher.exe>] --output <new-run-directory> [--source-revision <id>] [--h6-repeat immediate|quiescent|production-only-diagnostic] [--mouse-gestures enabled|disabled-diagnostic] [--keep-profile-on-failure]\n       radial_acceptance [--candidate <multi_launcher.exe>] --report <new-report.json>"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Arguments, String> {
        let parsed = parse_arguments(args.iter().map(OsString::from))?;
        match parsed {
            ParseResult::Run(arguments) => Ok(arguments),
            ParseResult::Help => Err("unexpected help result".into()),
        }
    }

    #[test]
    fn h6_repeat_mode_defaults_to_quiescent_and_accepts_immediate_probe() {
        let default = parse(&["--output", "run-default"]).unwrap();
        assert_eq!(default.h6_repeat_mode, H6RepeatMode::Quiescent);
        assert_eq!(default.mouse_gesture_mode, MouseGestureMode::Enabled);

        let immediate = parse(&["--output", "run-immediate", "--h6-repeat", "immediate"]).unwrap();
        assert_eq!(immediate.h6_repeat_mode, H6RepeatMode::Immediate);
        let gestures_disabled = parse(&[
            "--output",
            "run-no-gestures",
            "--mouse-gestures",
            "disabled-diagnostic",
        ])
        .unwrap();
        assert_eq!(
            gestures_disabled.mouse_gesture_mode,
            MouseGestureMode::DisabledDiagnostic
        );

        let production_only = parse(&[
            "--output",
            "run-production-only",
            "--h6-repeat",
            "production-only-diagnostic",
        ])
        .unwrap();
        assert_eq!(
            production_only.h6_repeat_mode,
            H6RepeatMode::ProductionOnlyDiagnostic
        );
    }

    #[test]
    fn optional_source_revision_derives_git_commit_and_worktree_state_for_both_modes() {
        let parsed = parse(&["--output", "run-derived-source"])
            .expect("documented invocation should derive its source identity");
        let revision = parsed
            .source_revision
            .as_deref()
            .expect("output runs need a source identity");
        assert_eq!(revision.len(), 46);
        assert!(revision[..40].bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert!(matches!(&revision[40..], "+clean" | "+dirty"));

        let report_mode = parse(&["--report", "run-report.json"])
            .expect("documented report invocation should also derive its source identity");
        assert!(report_mode.report_file.is_some());
        let report_revision = report_mode
            .source_revision
            .as_deref()
            .expect("report mode needs a source identity for R0");
        assert_eq!(report_revision.len(), 46);
        assert!(
            report_revision[..40]
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        );
        assert!(matches!(&report_revision[40..], "+clean" | "+dirty"));

        let supplied = parse(&[
            "--output",
            "run-explicit-source",
            "--source-revision",
            "reviewed-build-id",
        ])
        .expect("explicit source identity should remain supported");
        assert_eq!(
            supplied.source_revision.as_deref(),
            Some("reviewed-build-id")
        );
    }

    #[test]
    fn source_revision_rejects_empty_or_nonprintable_values() {
        assert!(validate_source_revision(String::new()).is_err());
        assert!(validate_source_revision("bad\nidentity".into()).is_err());
        assert!(validate_source_revision("good-id".into()).is_ok());
    }

    #[test]
    fn r0_requires_decisive_native_case_facts_in_untruncated_summaries() {
        for id in ["D2", "A2", "G1", "A3", "A5", "A6", "A7", "D3", "D6", "D7"] {
            let required = required_case_evidence(id).expect("required case evidence exists");
            let observed = format!("evidence:v1; {}", required.join("; "));
            assert!(
                validate_required_case_evidence(id, CaseStatus::Passed, &observed).is_ok(),
                "valid evidence was rejected for {id}"
            );
            assert!(
                validate_required_case_evidence(id, CaseStatus::Passed, &observed[..12]).is_err(),
                "truncated evidence was accepted for {id}"
            );
            let incomplete = format!("evidence:v1; {}", required[1..].join("; "));
            assert!(
                validate_required_case_evidence(id, CaseStatus::Passed, &incomplete).is_err(),
                "summary with a missing decisive fact was accepted for {id}"
            );
            assert!(
                validate_required_case_evidence(id, CaseStatus::Failed, &observed).is_err(),
                "failed case was accepted for {id}"
            );
        }
        assert!(
            validate_required_case_evidence(
                "D7",
                CaseStatus::Passed,
                &"x".repeat(MAX_RESULT_BYTES)
            )
            .is_err()
        );
    }

    #[test]
    fn h6_repeat_mode_rejects_unknown_values() {
        let error = parse(&["--output", "run", "--h6-repeat", "retry"]).unwrap_err();
        assert!(error.contains("production-only-diagnostic"));
    }

    #[test]
    fn disabled_mouse_gesture_diagnostic_is_written_to_isolated_profile() {
        let fixture = deterministic_fixture(
            Path::new("acceptance.log"),
            MouseGestureMode::DisabledDiagnostic,
        )
        .unwrap();
        let settings: Settings = serde_json::from_slice(&fixture.settings_json).unwrap();
        assert_eq!(
            settings.plugin_settings["mouse_gestures"]["enabled"],
            serde_json::Value::Bool(false)
        );
    }

    #[test]
    fn mouse_gesture_mode_rejects_unknown_values() {
        let error = parse(&["--output", "run", "--mouse-gestures", "off"]).unwrap_err();
        assert!(error.contains("--mouse-gestures must be"));
    }

    #[test]
    fn report_capacity_saturation_is_explicit_and_fails_r0() {
        let mut report = AcceptanceReport {
            schema_version: 5,
            run_id: "test".into(),
            mode: "test",
            started_unix_ms: 1,
            finished_unix_ms: 2,
            copied_profile_status: CopiedProfileStatus::NotRun,
            h6_repeat_mode: H6RepeatMode::Quiescent,
            mouse_gesture_mode: MouseGestureMode::Enabled,
            outcome: "running",
            candidate: CandidateIdentity {
                executable: "candidate.exe".into(),
                sha256: "a".repeat(64),
            },
            environment: EnvironmentIdentity {
                os_version: "test".into(),
                architecture: "x64".into(),
                runner_process_id: 1,
                runner_sha256: None,
                child_process_id: None,
                child_started_unix_ms: None,
                source_revision: None,
                monitors: Vec::new(),
            },
            profile: ProfileIdentity {
                mode: "test",
                temporary_data_root: "profile".into(),
                settings_sha256: "b".repeat(64),
                radial_sha256: "c".repeat(64),
                actions_sha256: "d".repeat(64),
                configured_hotkey: ACCEPTANCE_HOTKEY,
                hold_threshold_ms: 1,
            },
            cases: vec![AcceptanceCaseResult {
                id: "R0".into(),
                status: CaseStatus::Passed,
                elapsed_ms: 0,
                expected: "valid report".into(),
                observed: "valid report".into(),
                failure_stage: None,
                artifacts: Vec::new(),
            }],
            artifacts: Vec::new(),
            cleanup: CleanupResult::default(),
            capacity_saturated: false,
        };
        for index in 0..=MAX_ARTIFACTS {
            report.push_artifact(format!("artifact-{index}"));
        }
        assert_eq!(report.artifacts.len(), MAX_ARTIFACTS);
        assert!(report.capacity_saturated);
        assert!(!report.passed());
        assert!(report.cases[0].observed.contains("capacity was exceeded"));
        assert!(matches!(report.cases[0].status, CaseStatus::Failed));

        report.cases.clear();
        report.capacity_saturated = false;
        for _ in 0..=MAX_CASES - 1 {
            report.push_case(AcceptanceCaseResult {
                id: "overflow".into(),
                status: CaseStatus::Passed,
                elapsed_ms: 0,
                expected: "bounded".into(),
                observed: "bounded".into(),
                failure_stage: None,
                artifacts: Vec::new(),
            });
        }
        report.push_case(AcceptanceCaseResult {
            id: "R0".into(),
            status: CaseStatus::Passed,
            elapsed_ms: 0,
            expected: "valid report".into(),
            observed: "valid report".into(),
            failure_stage: None,
            artifacts: Vec::new(),
        });
        assert_eq!(report.cases.len(), MAX_CASES);
        assert!(report.capacity_saturated);
        assert_eq!(report.cases.last().map(|case| case.id.as_str()), Some("R0"));
        assert!(matches!(
            report.cases.last().unwrap().status,
            CaseStatus::Failed
        ));
    }

    #[test]
    fn text_report_marks_copied_profile_as_not_run_and_has_timing_and_hashes() {
        let report = AcceptanceReport {
            schema_version: 5,
            run_id: "test".into(),
            mode: "test",
            started_unix_ms: 100,
            finished_unix_ms: 250,
            copied_profile_status: CopiedProfileStatus::NotRun,
            h6_repeat_mode: H6RepeatMode::Quiescent,
            mouse_gesture_mode: MouseGestureMode::Enabled,
            outcome: "failed",
            candidate: CandidateIdentity {
                executable: "candidate.exe".into(),
                sha256: "a".repeat(64),
            },
            environment: EnvironmentIdentity {
                os_version: "test".into(),
                architecture: "x64".into(),
                runner_process_id: 1,
                runner_sha256: Some("b".repeat(64)),
                child_process_id: None,
                child_started_unix_ms: None,
                source_revision: Some("deadbeef".into()),
                monitors: Vec::new(),
            },
            profile: ProfileIdentity {
                mode: "test",
                temporary_data_root: "profile".into(),
                settings_sha256: "c".repeat(64),
                radial_sha256: "d".repeat(64),
                actions_sha256: "e".repeat(64),
                configured_hotkey: ACCEPTANCE_HOTKEY,
                hold_threshold_ms: 1,
            },
            cases: Vec::new(),
            artifacts: Vec::new(),
            cleanup: CleanupResult::default(),
            capacity_saturated: false,
        };
        let text = render_text_report(&report);
        assert!(text.contains("Copied profile: not_run"));
        assert!(text.contains("Started Unix ms: 100"));
        assert!(text.contains("Finished Unix ms: 250"));
        assert!(text.contains(&"a".repeat(64)));
        assert!(text.contains(&"c".repeat(64)));
    }
}
