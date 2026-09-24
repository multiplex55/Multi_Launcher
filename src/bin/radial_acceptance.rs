//! Opt-in native Windows acceptance runner for ROOT and the Radial Designer.

use multi_launcher::common::persistence::LoadState;
use multi_launcher::radial::{
    RadialDocument, settings as radial_settings, validation::validate as validate_radial_document,
};
use multi_launcher::settings::{
    LogFile, Settings, SubmenuMigrationState, SubmenuPresentationMigrationReceipt,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[path = "radial_acceptance/copied_profile.rs"]
mod copied_profile;
#[cfg(windows)]
#[path = "radial_acceptance/native.rs"]
mod native;
#[path = "radial_acceptance/private_artifacts.rs"]
mod private_artifacts;

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
const COPIED_CASE_IDS: [&str; 28] = [
    "CP_PREFLIGHT",
    "CP_H0",
    "CP_H1",
    "CP_H2",
    "CP_H3",
    "CP_D0",
    "CP_R1",
    "CP_D1",
    "CP_D2",
    "CP_D4",
    "CP_D5",
    "CP_A0",
    "CP_A1",
    "CP_G0",
    "CP_A2",
    "CP_A3",
    "CP_A4",
    "CP_A5",
    "CP_A6",
    "CP_D3",
    "CP_A7",
    "CP_A8",
    "CP_D6",
    "CP_D7",
    "CP_SOURCE_INTEGRITY",
    "R0",
    "R2",
    "CLEANUP",
];

#[derive(Clone, Debug)]
struct Arguments {
    launcher: Option<PathBuf>,
    output: Option<PathBuf>,
    report_file: Option<PathBuf>,
    profile_copy: Option<PathBuf>,
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

#[derive(Clone, Serialize)]
struct CandidateIdentity {
    executable: String,
    sha256: String,
}

#[derive(Clone, Serialize)]
struct MonitorIdentity {
    id: u32,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    scale_factor: f32,
}

#[derive(Clone, Serialize)]
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

#[derive(Clone, Serialize)]
struct ProfileIdentity {
    mode: &'static str,
    temporary_data_root: String,
    settings_sha256: String,
    radial_sha256: String,
    actions_sha256: String,
    configured_hotkey: &'static str,
    hold_threshold_ms: u64,
}

#[derive(Clone, Serialize)]
struct AcceptanceCaseResult {
    id: String,
    status: CaseStatus,
    elapsed_ms: u64,
    expected: String,
    observed: String,
    failure_stage: Option<FailureStage>,
    artifacts: Vec<String>,
}

#[derive(Clone, Default, Serialize)]
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

#[derive(Clone, Serialize)]
struct AcceptanceReport {
    schema_version: u16,
    run_id: String,
    mode: &'static str,
    started_unix_ms: u128,
    finished_unix_ms: u128,
    copied_profile_status: CopiedProfileStatus,
    copied_profile: Option<CopiedProfileSummary>,
    private_artifacts: Option<private_artifacts::PrivateArtifactSummary>,
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
    Running,
    Passed,
    Failed,
}

#[derive(Clone, Debug, Serialize)]
struct CopiedProfileSummary {
    source_tree_sha256_before: String,
    copied_initial_tree_sha256: String,
    source_tree_sha256_after: Option<String>,
    copied_file_count: usize,
    copied_total_bytes: u64,
    source_settings_sha256: String,
    source_radial_sha256: String,
    source_actions_sha256: Option<String>,
    copied_initial_settings_sha256: String,
    copied_initial_radial_sha256: String,
    copied_initial_actions_sha256: Option<String>,
    launch_settings_sha256: String,
    launch_radial_sha256: String,
    launch_actions_sha256: String,
    source_unchanged: bool,
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
        self.passed_native_cases()
            && !matches!(
                self.copied_profile_status,
                CopiedProfileStatus::Running | CopiedProfileStatus::Failed
            )
    }

    fn passed_native_cases(&self) -> bool {
        !self.capacity_saturated
            && self.cases.len()
                == if self.mode == "native_windows_copied_profile" {
                    COPIED_CASE_IDS.len()
                } else {
                    CASE_IDS.len()
                }
            && self.cases.iter().enumerate().all(|(index, case)| {
                self.cases[..index]
                    .iter()
                    .all(|previous| previous.id != case.id)
            })
            && self
                .cases
                .iter()
                .all(|case| matches!(case.status, CaseStatus::Passed))
            && (if self.mode == "native_windows_copied_profile" {
                COPIED_CASE_IDS
                    .iter()
                    .all(|id| self.cases.iter().any(|case| case.id == *id))
            } else {
                CASE_IDS
                    .iter()
                    .all(|id| self.cases.iter().any(|case| case.id == *id))
            })
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

#[derive(Clone, Debug)]
struct CopiedProfileMetadata {
    initial_copy_tree_sha256: String,
    copied_file_count: usize,
    copied_total_bytes: u64,
    source_settings_sha256: String,
    source_radial_sha256: String,
    source_actions_sha256: Option<String>,
    copied_initial_settings_sha256: String,
    copied_initial_radial_sha256: String,
    copied_initial_actions_sha256: Option<String>,
    launch_settings_sha256: String,
    launch_radial_sha256: String,
    launch_actions_sha256: String,
    target_action_index: usize,
    skin_index: usize,
    restore_menu_name: String,
    original_menu_sha256: Vec<String>,
    expected_submenu_migration_receipt: Option<SubmenuPresentationMigrationReceipt>,
    hold_threshold_ms: u64,
}

impl CopiedProfileMetadata {
    fn report_summary(
        &self,
        source_inventory: &copied_profile::ProfileInventory,
        source_tree_sha256_after: Option<String>,
        source_unchanged: bool,
    ) -> CopiedProfileSummary {
        CopiedProfileSummary {
            source_tree_sha256_before: source_inventory.tree_sha256.clone(),
            copied_initial_tree_sha256: self.initial_copy_tree_sha256.clone(),
            source_tree_sha256_after,
            copied_file_count: self.copied_file_count,
            copied_total_bytes: self.copied_total_bytes,
            source_settings_sha256: self.source_settings_sha256.clone(),
            source_radial_sha256: self.source_radial_sha256.clone(),
            source_actions_sha256: self.source_actions_sha256.clone(),
            copied_initial_settings_sha256: self.copied_initial_settings_sha256.clone(),
            copied_initial_radial_sha256: self.copied_initial_radial_sha256.clone(),
            copied_initial_actions_sha256: self.copied_initial_actions_sha256.clone(),
            launch_settings_sha256: self.launch_settings_sha256.clone(),
            launch_radial_sha256: self.launch_radial_sha256.clone(),
            launch_actions_sha256: self.launch_actions_sha256.clone(),
            source_unchanged,
        }
    }
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
    let mut profile_copy = None;
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
            Some("--profile-copy") => {
                profile_copy = Some(next_path(&mut args, "--profile-copy")?);
            }
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
        profile_copy,
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
    let proposed_output = proposed_output_directory(&arguments)?;
    let proposed_report = proposed_report_path(&arguments, &proposed_output)?;
    let proposed_copy_report = arguments
        .profile_copy
        .as_ref()
        .map(|_| copied_report_path(&proposed_report));
    let source_scan = arguments
        .profile_copy
        .as_deref()
        .map(copied_profile::ProfileInventory::scan);
    if let Some(source) = arguments.profile_copy.as_deref() {
        let canonical_source = copied_profile::canonical_profile_root(source)?;
        let mut targets = vec![proposed_output.as_path(), proposed_report.as_path()];
        if let Some(copy_report) = proposed_copy_report.as_deref() {
            targets.push(copy_report);
        }
        let private_copy_path = std::env::temp_dir().join(".radial-acceptance-copy-profile");
        targets.push(private_copy_path.as_path());
        copied_profile::ensure_no_overlap(&canonical_source, &targets)?;
    }
    let output = prepare_output_directory(&arguments)?;
    let report_path = if let Some(report) = arguments.report_file.as_deref() {
        output.join(
            report
                .file_name()
                .ok_or_else(|| "--report requires a file name".to_string())?,
        )
    } else {
        output.join("report.json")
    };
    let copied_path = arguments
        .profile_copy
        .as_ref()
        .map(|_| copied_report_path(&report_path));

    let mut deterministic_report = run_windows_deterministic(
        arguments.clone(),
        output.clone(),
        report_path.clone(),
        arguments.profile_copy.is_some(),
    )?;
    let deterministic_native_passed = deterministic_report.passed_native_cases();
    let copied_report = match (arguments.profile_copy.as_deref(), copied_path.as_deref()) {
        (Some(_), Some(copy_report_path)) => Some(match source_scan {
            Some(Ok(inventory)) => run_copied_profile_windows(
                &deterministic_report,
                inventory,
                copy_report_path,
                &arguments,
                &output,
            )?,
            Some(Err(error)) => copied_preflight_failure_report(
                &deterministic_report,
                copy_report_path,
                "profile inventory rejected the supplied directory",
                &error,
            )?,
            None => return Err("copied profile inventory was not initialized".into()),
        }),
        _ => None,
    };
    let copied_passed = copied_report
        .as_ref()
        .map_or(true, AcceptanceReport::passed_native_cases);
    let mut copied_reports_persisted = true;
    if let (Some(copy_path), Some(copied_report)) = (copied_path.as_deref(), copied_report.as_ref())
    {
        if let Err(error) = persist_copied_report_pair(copy_path, copied_report) {
            copied_reports_persisted = false;
            eprintln!("copied-profile report publication failed: {error}");
        } else {
            println!("Copied profile report: {}", copy_path.display());
        }
    }
    let deterministic_r0_was_valid = deterministic_report
        .cases
        .iter()
        .find(|case| case.id == "R0")
        .is_some_and(|case| matches!(case.status, CaseStatus::Passed));
    if arguments.profile_copy.is_some() {
        deterministic_report.copied_profile_status = if copied_passed && copied_reports_persisted {
            CopiedProfileStatus::Passed
        } else {
            CopiedProfileStatus::Failed
        };
        deterministic_report.copied_profile = copied_report
            .as_ref()
            .and_then(|report| report.copied_profile.clone());
        revalidate_final_r0(&mut deterministic_report, deterministic_r0_was_valid);
    }
    let deterministic_passed = aggregate_native_cases_passed(
        deterministic_native_passed,
        copied_report.as_ref(),
        copied_reports_persisted,
    );
    deterministic_report.outcome = if deterministic_passed {
        "passed"
    } else {
        "failed"
    };
    write_report(&report_path, &deterministic_report)?;
    write_text_report(&report_path.with_extension("txt"), &deterministic_report)?;

    Ok((
        report_path,
        deterministic_report.passed() && copied_passed && copied_reports_persisted,
    ))
}

#[cfg(windows)]
fn revalidate_final_r0(report: &mut AcceptanceReport, profile_hashes_previously_valid: bool) {
    let Some(index) = report.cases.iter().position(|case| case.id == "R0") else {
        report.push_case(AcceptanceCaseResult {
            id: "R0".into(),
            status: CaseStatus::Failed,
            elapsed_ms: 0,
            expected: expected_final_case("R0").into(),
            observed: "final aggregate report omitted its R0 record".into(),
            failure_stage: Some(FailureStage::Environment),
            artifacts: Vec::new(),
        });
        return;
    };
    let mut r0 = report.cases.remove(index);
    match validate_r0_report(report, Path::new(""), profile_hashes_previously_valid) {
        Ok(observed) => {
            r0.status = CaseStatus::Passed;
            r0.observed = bounded_text(&observed, MAX_RESULT_BYTES);
            r0.failure_stage = None;
        }
        Err(error) => {
            r0.status = CaseStatus::Failed;
            r0.observed = bounded_text(
                &format!("final aggregate report integrity validation failed: {error}"),
                MAX_RESULT_BYTES,
            );
            r0.failure_stage = Some(FailureStage::Environment);
        }
    }
    report.push_case(r0);
}

fn aggregate_native_cases_passed(
    deterministic_passed: bool,
    copied_report: Option<&AcceptanceReport>,
    copied_reports_persisted: bool,
) -> bool {
    deterministic_passed
        && copied_report.map_or(true, AcceptanceReport::passed_native_cases)
        && (copied_report.is_none() || copied_reports_persisted)
}

fn proposed_output_directory(arguments: &Arguments) -> Result<PathBuf, String> {
    if let Some(output) = arguments.output.as_deref() {
        return copied_profile::resolve_future_path(output);
    }
    let report = arguments
        .report_file
        .as_deref()
        .ok_or_else(|| "specify --output or --report".to_string())?;
    let parent = report
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    parent
        .canonicalize()
        .map_err(|error| format!("resolve report parent before output isolation: {error}"))
}

fn proposed_report_path(arguments: &Arguments, output: &Path) -> Result<PathBuf, String> {
    let report = if let Some(report) = arguments.report_file.as_deref() {
        output.join(
            report
                .file_name()
                .ok_or_else(|| "--report requires a file name".to_string())?,
        )
    } else {
        output.join("report.json")
    };
    copied_profile::resolve_future_path(&report)
}

fn copied_report_path(report_path: &Path) -> PathBuf {
    let stem = report_path
        .file_stem()
        .unwrap_or_else(|| std::ffi::OsStr::new("report"));
    let mut name = stem.to_os_string();
    name.push(".copied-profile.json");
    report_path.with_file_name(name)
}

#[cfg(windows)]
fn persist_copied_report_pair(report_path: &Path, report: &AcceptanceReport) -> Result<(), String> {
    let text_path = report_path.with_extension("txt");
    for final_path in [report_path, text_path.as_path()] {
        match fs::symlink_metadata(final_path) {
            Ok(_) => return Err("copied-profile report target already exists".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("inspect copied-profile report target".into()),
        }
    }
    let parent = report_path
        .parent()
        .ok_or_else(|| "copied-profile report has no parent directory".to_string())?;
    let staging = tempfile::Builder::new()
        .prefix(".radial-acceptance-report-stage-")
        .tempdir_in(parent)
        .map_err(|_| "create copied-profile report staging directory".to_string())?;
    let staged_json = staging.path().join("copied.json");
    let staged_text = staging.path().join("copied.txt");
    write_report(&staged_json, report)?;
    write_text_report(&staged_text, report)?;
    if let Err(_) = fs::rename(&staged_json, report_path) {
        return Err("publish verified copied-profile JSON report".into());
    }
    if fs::rename(&staged_text, &text_path).is_err() {
        let _ = fs::remove_file(report_path);
        return Err("publish verified copied-profile text report".into());
    }
    Ok(())
}

fn private_artifact_evidence(summary: &private_artifacts::PrivateArtifactSummary) -> String {
    let disposition = match summary.status {
        private_artifacts::PrivateArtifactStatus::EphemeralValidated => "ephemeral",
        private_artifacts::PrivateArtifactStatus::Retained => "retained",
        private_artifacts::PrivateArtifactStatus::NotRun
        | private_artifacts::PrivateArtifactStatus::Failed => "failed",
    };
    format!(
        "evidence:v1; artifact_retention={disposition}; artifact_count={}; artifact_bytes={}; artifact_id={}; artifact_sha256={}",
        summary.file_count,
        summary.total_bytes,
        summary.artifact_id.as_deref().unwrap_or("none"),
        summary.manifest_sha256.as_deref().unwrap_or("missing")
    )
}

#[cfg(windows)]
fn mark_private_artifact_case_failed(report: &mut AcceptanceReport) {
    if let Some(case) = report.cases.iter_mut().find(|case| case.id == "CP_R1") {
        case.status = CaseStatus::Failed;
        case.failure_stage = Some(FailureStage::Environment);
        case.observed =
            "private copied-profile diagnostics were not included in the public report".into();
    } else {
        append_copied_case(
            report,
            "CP_R1",
            CaseStatus::Failed,
            "private copied-profile diagnostics were not included in the public report",
            Some(FailureStage::Environment),
            Instant::now(),
        );
    }
}

#[cfg(windows)]
fn run_copied_profile_windows(
    deterministic_report: &AcceptanceReport,
    source_inventory: copied_profile::ProfileInventory,
    _report_path: &Path,
    arguments: &Arguments,
    _output: &Path,
) -> Result<AcceptanceReport, String> {
    let run_started = Instant::now();
    let started_unix_ms = unix_ms();
    let mut report = copied_report_seed(deterministic_report, started_unix_ms);
    let private_profile = match tempfile::Builder::new()
        .prefix("multi-launcher-copied-profile-")
        .tempdir()
    {
        Ok(profile) => profile,
        Err(_) => {
            append_copied_case(
                &mut report,
                "CP_PREFLIGHT",
                CaseStatus::Failed,
                "copied-profile private temporary directory could not be created",
                Some(FailureStage::Environment),
                run_started,
            );
            let source_after = copied_profile::ProfileInventory::scan(&source_inventory.root).ok();
            report.cleanup.profile_removed = true;
            finish_copied_profile_report(
                &mut report,
                &source_inventory,
                source_after,
                None,
                false,
                run_started,
            );
            return Ok(report);
        }
    };
    let copy_root = private_profile.path().to_path_buf();
    if copied_profile::ensure_no_overlap(&source_inventory.root, &[copy_root.as_path()]).is_err() {
        append_copied_case(
            &mut report,
            "CP_PREFLIGHT",
            CaseStatus::Failed,
            "isolated temporary copy overlaps the supplied source profile",
            Some(FailureStage::Environment),
            run_started,
        );
        let source_after = copied_profile::ProfileInventory::scan(&source_inventory.root).ok();
        let removed = private_profile.close().is_ok();
        report.cleanup.profile_removed = removed;
        finish_copied_profile_report(
            &mut report,
            &source_inventory,
            source_after,
            None,
            false,
            run_started,
        );
        return Ok(report);
    }

    let metadata = match prepare_copied_profile(&source_inventory, &copy_root) {
        Ok(metadata) => metadata,
        Err(_) => {
            append_copied_case(
                &mut report,
                "CP_PREFLIGHT",
                CaseStatus::Failed,
                "copied-profile typed validation or copy-only safety normalization failed",
                Some(FailureStage::Environment),
                run_started,
            );
            let source_after = copied_profile::ProfileInventory::scan(&source_inventory.root).ok();
            report.cleanup.profile_removed = private_profile.close().is_ok();
            finish_copied_profile_report(
                &mut report,
                &source_inventory,
                source_after,
                None,
                false,
                run_started,
            );
            return Ok(report);
        }
    };
    append_copied_case(
        &mut report,
        "CP_PREFLIGHT",
        CaseStatus::Passed,
        "copied source bytes verified; typed settings and radial data validated; copy-only safety controls applied",
        None,
        run_started,
    );
    report.profile = ProfileIdentity {
        mode: "copied_profile",
        temporary_data_root: bounded_text(&copy_root.to_string_lossy(), MAX_PATH_BYTES),
        settings_sha256: metadata.launch_settings_sha256.clone(),
        radial_sha256: metadata.launch_radial_sha256.clone(),
        actions_sha256: metadata.launch_actions_sha256.clone(),
        configured_hotkey: ACCEPTANCE_HOTKEY,
        hold_threshold_ms: metadata.hold_threshold_ms,
    };
    report.copied_profile = Some(metadata.report_summary(&source_inventory, None, false));
    report.environment.child_process_id = None;
    report.environment.child_started_unix_ms = None;
    report.run_id = format!("{}-copied", deterministic_report.run_id);

    let runner_log_path = copy_root.join("acceptance-runner.log");
    // The copied profile's normalized Settings.log_file is the candidate trace. Keep this
    // aligned with the deterministic runner so native predicates read the child's actual
    // acceptance events instead of an unrelated, never-created filename.
    let trace_path = copied_profile_trace_path(&copy_root);
    let mut runner_log = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&runner_log_path)
    {
        Ok(log) => log,
        Err(_) => {
            append_copied_case(
                &mut report,
                "CP_H0",
                CaseStatus::Failed,
                "copied-profile private runner log could not be created",
                Some(FailureStage::Environment),
                run_started,
            );
            let source_after = copied_profile::ProfileInventory::scan(&source_inventory.root).ok();
            report.cleanup.profile_removed = private_profile.close().is_ok();
            finish_copied_profile_report(
                &mut report,
                &source_inventory,
                source_after,
                Some(&metadata),
                false,
                run_started,
            );
            return Ok(report);
        }
    };
    let _ = writeln!(runner_log, "copied-profile native suite starting");
    let candidate_executable = report.candidate.executable.clone();
    let driver_profile = copy_root.clone();
    let driver_trace = trace_path.clone();
    let copied_options = native::CopiedAuthoringOptions {
        target_action_index: metadata.target_action_index,
        skin_index: metadata.skin_index,
        original_menu_sha256: metadata.original_menu_sha256.clone(),
        restore_menu_name: metadata.restore_menu_name.clone(),
    };
    let fallback_report = report.clone();
    let driver = std::thread::Builder::new()
        .name("radial-acceptance-copied-profile-driver".to_string())
        .spawn(move || match native::attach_to_input_desktop() {
            Ok((observed, desktop_attachment)) => {
                let _ = writeln!(runner_log, "input desktop attached: {observed}");
                let before_foreground = native::capture_foreground();
                let before_cursor = native::cursor_position();
                report.cleanup.foreground_restore_captured = !before_foreground.0.is_invalid();
                let anchor = native::run_copied_profile_suite(
                    &candidate_executable,
                    &driver_profile,
                    &driver_profile,
                    &driver_trace,
                    before_cursor.as_ref().ok().copied(),
                    &desktop_attachment,
                    &mut report,
                    &mut runner_log,
                    copied_options,
                );

                if let Ok(point) = &before_cursor {
                    let point = *point;
                    let already_restored = native::cursor_position().is_ok_and(|current| {
                        current.x.abs_diff(point.x) <= 1 && current.y.abs_diff(point.y) <= 1
                    });
                    if already_restored {
                        report.cleanup.cursor_restored = true;
                    } else if native::set_cursor_position(point).is_ok() {
                        report.cleanup.cursor_restored =
                            native::cursor_position().is_ok_and(|current| {
                                current.x.abs_diff(point.x) <= 1 && current.y.abs_diff(point.y) <= 1
                            });
                    }
                }
                report.cleanup.foreground_restored = if before_foreground.0.is_invalid() {
                    false
                } else {
                    report.cleanup.foreground_restore_attempted = true;
                    if let Some(anchor) = anchor.as_ref() {
                        let _ = anchor.focus();
                    }
                    native::restore_foreground(before_foreground.0, before_foreground.1).is_ok()
                };
                if let Ok(point) = &before_cursor {
                    let point = *point;
                    if !native::cursor_position().is_ok_and(|current| {
                        current.x.abs_diff(point.x) <= 1 && current.y.abs_diff(point.y) <= 1
                    }) {
                        report.cleanup.cursor_restored = native::set_cursor_position(point)
                            .and_then(|()| native::cursor_position())
                            .is_ok_and(|current| {
                                current.x.abs_diff(point.x) <= 1 && current.y.abs_diff(point.y) <= 1
                            });
                    }
                }
                drop(anchor);
                let handle = desktop_attachment.release_for_thread_exit();
                (report, handle)
            }
            Err(_) => {
                append_copied_case(
                    &mut report,
                    "CP_H0",
                    CaseStatus::Failed,
                    "native input desktop could not be attached for copied-profile acceptance",
                    Some(FailureStage::Environment),
                    run_started,
                );
                (report, None)
            }
        });
    let (mut report, input_desktop_handle) = match driver {
        Ok(driver) => match driver.join() {
            Ok(result) => result,
            Err(_) => (fallback_report, None),
        },
        Err(_) => (fallback_report, None),
    };
    let mut runner_log = OpenOptions::new().append(true).open(&runner_log_path).ok();
    if report.environment.child_process_id.is_none() {
        if let Some(runner_log) = runner_log.as_mut() {
            let _ = writeln!(runner_log, "copied-profile native driver did not launch");
        }
    }
    report.cleanup.input_desktop_released = match input_desktop_handle {
        Some(handle) => native::close_input_desktop_after_driver_exit(handle).is_ok(),
        None => true,
    };
    if let Some(runner_log) = runner_log.as_mut() {
        let _ = runner_log.flush();
    }

    let launch_profile_audit = validate_copied_profile_after_run(&copy_root, &metadata);
    let launch_profile_matches = launch_profile_audit.is_ok();
    if let Err(reason) = &launch_profile_audit {
        // Keep the public report path-free and generic, but retain the bounded typed
        // audit reason in the owner-restricted runner log so a copied-profile R0
        // failure can be diagnosed after the temporary profile is removed.
        if let Some(runner_log) = runner_log.as_mut() {
            let _ = writeln!(
                runner_log,
                "copied profile post-run safety audit failed: {reason}"
            );
            let _ = runner_log.flush();
        }
    }
    let source_after = copied_profile::ProfileInventory::scan(&source_inventory.root).ok();
    let source_unchanged = source_after.as_ref() == Some(&source_inventory);
    let failure_requires_retention =
        copied_failure_requires_retention(&report, launch_profile_matches, source_unchanged);
    let mut diagnostic_paths = report
        .cases
        .iter()
        .filter(|case| case.id == "CP_R1" || matches!(case.status, CaseStatus::Failed))
        .flat_map(|case| case.artifacts.iter().map(PathBuf::from))
        .collect::<Vec<_>>();
    let mut failure_logs_complete = true;
    match private_artifacts::write_bounded_run_log_snapshots(&copy_root, &report.run_id) {
        Ok(snapshots) => {
            if report.environment.child_process_id.is_some() {
                let expected = [
                    format!("case-{}-child-acceptance-trace.log", report.run_id),
                    format!("case-{}-child-stdout-private.log", report.run_id),
                    format!("case-{}-child-stderr-private.log", report.run_id),
                ];
                failure_logs_complete = expected.iter().all(|expected_name| {
                    snapshots.iter().any(|path| {
                        path.file_name().and_then(|name| name.to_str())
                            == Some(expected_name.as_str())
                    })
                });
            }
            diagnostic_paths.extend(snapshots);
        }
        Err(_) => failure_logs_complete = false,
    }

    let mut staged_success = None;
    let mut retained_private_directory = None;
    match private_artifacts::stage_diagnostics(&copy_root, diagnostic_paths) {
        Ok(staged) => {
            let control_artifacts_complete = staged.control_artifacts_complete();
            let cp_r1_passed = report
                .cases
                .iter()
                .find(|case| case.id == "CP_R1")
                .is_some_and(|case| matches!(case.status, CaseStatus::Passed));
            let artifact_evidence_complete =
                control_artifacts_complete && failure_logs_complete && cp_r1_passed;
            if !artifact_evidence_complete {
                mark_private_artifact_case_failed(&mut report);
            }

            let retain_evidence = failure_requires_retention
                || report
                    .cases
                    .iter()
                    .find(|case| case.id == "CP_R1")
                    .is_none_or(|case| !matches!(case.status, CaseStatus::Passed));
            if retain_evidence {
                let retained = staged.retain();
                retained_private_directory = Some(retained.directory.clone());
                report.private_artifacts = Some(retained.summary.clone());
                if artifact_evidence_complete {
                    if let Some(case) = report.cases.iter_mut().find(|case| case.id == "CP_R1") {
                        case.observed = private_artifact_evidence(&retained.summary);
                        case.failure_stage = None;
                    }
                }
            } else {
                let summary = staged.ephemeral_summary();
                report.private_artifacts = Some(summary.clone());
                if let Some(case) = report.cases.iter_mut().find(|case| case.id == "CP_R1") {
                    case.observed = private_artifact_evidence(&summary);
                    case.failure_stage = None;
                }
                staged_success = Some(staged);
            }
        }
        Err(_) => {
            report.private_artifacts = Some(private_artifacts::PrivateArtifactSummary::failed());
            mark_private_artifact_case_failed(&mut report);
        }
    }

    let copied_cases_failed = report
        .cases
        .iter()
        .any(|case| !matches!(case.status, CaseStatus::Passed));
    let preserve_private_copy = arguments.keep_profile_on_failure
        && (copied_cases_failed || !launch_profile_matches || !source_unchanged);
    if preserve_private_copy {
        let _retained_profile_path = private_profile.keep();
        report.cleanup.profile_removed = false;
        eprintln!("private copied-profile debugging copy retained after failure");
    } else {
        report.cleanup.profile_removed = private_profile.close().is_ok();
    }

    let artifact_bundle_verified = if let (Some(directory), Some(summary)) = (
        retained_private_directory.as_deref(),
        report.private_artifacts.as_ref(),
    ) {
        !directory.starts_with(&copy_root)
            && private_artifacts::verify_retained_artifacts(directory, summary).is_ok()
    } else if let Some(staged) = staged_success.as_ref() {
        report.cleanup.profile_removed && staged.verify_after_profile_cleanup().is_ok()
    } else {
        false
    };
    if !artifact_bundle_verified {
        if let Some(staged) = staged_success.take() {
            // Preserve evidence if post-cleanup validation fails, while ensuring the
            // copied run cannot report success for an unverified evidence bundle.
            let retained = staged.retain();
            let retained_verified = private_artifacts::verify_retained_artifacts(
                &retained.directory,
                &retained.summary,
            )
            .is_ok();
            report.private_artifacts = Some(retained.summary);
            if !retained_verified {
                mark_private_artifact_case_failed(&mut report);
            }
        }
        mark_private_artifact_case_failed(&mut report);
    } else if let Some(staged) = staged_success.take() {
        // Passing runs expose only a hash and counts; deleting this temporary bundle
        // avoids retaining user-profile diagnostics after validation.
        drop(staged);
    }
    finish_copied_profile_report(
        &mut report,
        &source_inventory,
        source_after,
        Some(&metadata),
        launch_profile_matches,
        run_started,
    );
    Ok(report)
}

#[cfg(windows)]
fn copied_report_seed(deterministic: &AcceptanceReport, started_unix_ms: u128) -> AcceptanceReport {
    let mut environment = deterministic.environment.clone();
    environment.child_process_id = None;
    environment.child_started_unix_ms = None;
    let mut profile = deterministic.profile.clone();
    profile.mode = "copied_profile";
    profile.temporary_data_root = "private copied-profile temporary directory".into();
    AcceptanceReport {
        schema_version: deterministic.schema_version,
        run_id: format!("{}-copied", deterministic.run_id),
        mode: "native_windows_copied_profile",
        started_unix_ms,
        finished_unix_ms: 0,
        copied_profile_status: CopiedProfileStatus::Running,
        copied_profile: None,
        private_artifacts: Some(private_artifacts::PrivateArtifactSummary::not_run()),
        h6_repeat_mode: deterministic.h6_repeat_mode,
        mouse_gesture_mode: deterministic.mouse_gesture_mode,
        outcome: "running",
        candidate: deterministic.candidate.clone(),
        environment,
        profile,
        cases: Vec::with_capacity(COPIED_CASE_IDS.len()),
        artifacts: Vec::new(),
        cleanup: CleanupResult::default(),
        capacity_saturated: false,
    }
}

#[cfg(windows)]
fn copied_preflight_failure_report(
    deterministic: &AcceptanceReport,
    _report_path: &Path,
    _reason: &str,
    _private_error: &str,
) -> Result<AcceptanceReport, String> {
    let run_started = Instant::now();
    let mut report = copied_report_seed(deterministic, unix_ms());
    report.cleanup.profile_removed = true;
    for id in COPIED_CASE_IDS {
        let (status, observed, stage) = match id {
            "CP_PREFLIGHT" => (
                CaseStatus::Failed,
                "supplied profile was rejected by bounded regular-file inventory preflight",
                Some(FailureStage::Environment),
            ),
            "CP_SOURCE_INTEGRITY" => (
                CaseStatus::Failed,
                "source profile integrity could not be established from the rejected inventory",
                Some(FailureStage::Environment),
            ),
            "R0" => (
                CaseStatus::Failed,
                "copied-profile report integrity validation failed",
                Some(FailureStage::Environment),
            ),
            "R2" | "CLEANUP" => (
                CaseStatus::Failed,
                "copied-profile native process and private copy were not started",
                Some(FailureStage::Cleanup),
            ),
            _ => (
                CaseStatus::Failed,
                "copied-profile native case was not run after source inventory rejection",
                Some(FailureStage::Environment),
            ),
        };
        append_copied_case(&mut report, id, status, observed, stage, run_started);
    }
    report.finished_unix_ms = unix_ms();
    report.copied_profile_status = CopiedProfileStatus::Failed;
    report.outcome = "failed";
    sanitize_copied_report(&mut report);
    Ok(report)
}

#[cfg(windows)]
fn append_copied_case(
    report: &mut AcceptanceReport,
    id: &str,
    status: CaseStatus,
    observed: &str,
    failure_stage: Option<FailureStage>,
    started: Instant,
) {
    report.push_case(AcceptanceCaseResult {
        id: id.to_string(),
        status,
        elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        expected: copied_case_expected(id).to_string(),
        observed: bounded_text(observed, MAX_RESULT_BYTES),
        failure_stage,
        artifacts: Vec::new(),
    });
}

#[cfg(windows)]
fn copied_case_expected(id: &str) -> &'static str {
    match id {
        "CP_PREFLIGHT" => {
            "source inventory, initial copy hashes, typed input, and safe copy normalization pass"
        }
        "CP_SOURCE_INTEGRITY" => {
            "source profile remains byte-for-byte unchanged after copied native acceptance"
        }
        "CP_R1" => {
            "bounded diagnostics are privately validated and retained only when copied acceptance fails"
        }
        "R0" => "copied-profile report identities, bounds, hashes, cases, and evidence validate",
        "R2" => "copied candidate and native runner restore system state and remove private copy",
        "CLEANUP" => "copied candidate and all child-owned windows close cleanly",
        _ => "copied profile passes the corresponding native ROOT or Designer acceptance behavior",
    }
}

#[cfg(windows)]
fn finish_copied_profile_report(
    report: &mut AcceptanceReport,
    source_inventory: &copied_profile::ProfileInventory,
    source_after: Option<copied_profile::ProfileInventory>,
    metadata: Option<&CopiedProfileMetadata>,
    launch_profile_matches: bool,
    run_started: Instant,
) {
    for id in COPIED_CASE_IDS {
        if matches!(
            id,
            "CP_PREFLIGHT" | "CP_SOURCE_INTEGRITY" | "R0" | "R2" | "CLEANUP"
        ) || report.cases.iter().any(|case| case.id == id)
        {
            continue;
        }
        append_copied_case(
            report,
            id,
            CaseStatus::Failed,
            "copied-profile native case did not produce a result",
            Some(FailureStage::Cleanup),
            run_started,
        );
    }
    if !report.cases.iter().any(|case| case.id == "CLEANUP") {
        append_copied_case(
            report,
            "CLEANUP",
            CaseStatus::Failed,
            "copied-profile candidate cleanup did not produce a result",
            Some(FailureStage::Cleanup),
            run_started,
        );
    }
    let source_unchanged = source_after
        .as_ref()
        .is_some_and(|after| after == source_inventory);
    append_copied_case(
        report,
        "CP_SOURCE_INTEGRITY",
        if source_unchanged {
            CaseStatus::Passed
        } else {
            CaseStatus::Failed
        },
        if source_unchanged {
            "pre-run and post-run source inventories have identical paths, sizes, and SHA-256 hashes"
        } else {
            "source profile integrity verification failed after copied-profile acceptance"
        },
        (!source_unchanged).then_some(FailureStage::Environment),
        run_started,
    );
    if let Some(metadata) = metadata {
        report.copied_profile = Some(metadata.report_summary(
            source_inventory,
            source_after.as_ref().map(|after| after.tree_sha256.clone()),
            source_unchanged,
        ));
    }
    let cleanup = &report.cleanup;
    let foreground_ok = !cleanup.foreground_restore_captured
        || (cleanup.foreground_restore_attempted && cleanup.foreground_restored);
    let cleanup_ok = report.environment.child_process_id.is_some()
        && cleanup.child_closed_normally
        && cleanup.child_owned_windows_closed
        && cleanup.profile_removed
        && cleanup.cursor_restored
        && cleanup.input_desktop_released
        && foreground_ok;
    append_copied_case(
        report,
        "R2",
        if cleanup_ok {
            CaseStatus::Passed
        } else {
            CaseStatus::Failed
        },
        if cleanup_ok {
            "copied candidate, owned windows, cursor, foreground, input desktop, and private profile cleaned up"
        } else {
            "copied candidate teardown or system-state restoration was not fully verified"
        },
        (!cleanup_ok).then_some(FailureStage::Cleanup),
        run_started,
    );
    report.finished_unix_ms = unix_ms();
    sanitize_copied_report(report);
    report.copied_profile_status = if copied_cases_passed_before_r0(report) {
        CopiedProfileStatus::Passed
    } else {
        CopiedProfileStatus::Failed
    };
    let validation = validate_r0_report(report, Path::new(""), launch_profile_matches);
    match validation {
        Ok(observed) => append_copied_case(
            report,
            "R0",
            CaseStatus::Passed,
            &observed,
            None,
            run_started,
        ),
        Err(_) => append_copied_case(
            report,
            "R0",
            CaseStatus::Failed,
            "copied-profile report integrity validation failed",
            Some(FailureStage::Environment),
            run_started,
        ),
    }
    report.outcome = if report.passed_native_cases() {
        "passed"
    } else {
        "failed"
    };
}

#[cfg(windows)]
fn copied_cases_passed_before_r0(report: &AcceptanceReport) -> bool {
    let required_cases = COPIED_CASE_IDS.len() - 1;
    !report.capacity_saturated
        && report.cases.len() == required_cases
        && report
            .cases
            .iter()
            .all(|case| case.id != "R0" && matches!(case.status, CaseStatus::Passed))
        && COPIED_CASE_IDS
            .iter()
            .filter(|id| **id != "R0")
            .all(|id| report.cases.iter().any(|case| case.id == *id))
}

fn copied_failure_requires_retention(
    report: &AcceptanceReport,
    launch_profile_matches: bool,
    source_unchanged: bool,
) -> bool {
    !launch_profile_matches
        || !source_unchanged
        || report
            .cases
            .iter()
            .any(|case| !matches!(case.status, CaseStatus::Passed))
        || !copied_native_cleanup_verified_before_profile_removal(report)
}

fn copied_native_cleanup_verified_before_profile_removal(report: &AcceptanceReport) -> bool {
    let cleanup = &report.cleanup;
    let foreground_ok = !cleanup.foreground_restore_captured
        || (cleanup.foreground_restore_attempted && cleanup.foreground_restored);
    report.environment.child_process_id.is_some()
        && cleanup.child_closed_normally
        && cleanup.child_owned_windows_closed
        && cleanup.cursor_restored
        && cleanup.input_desktop_released
        && foreground_ok
}

#[cfg(windows)]
fn validate_copied_profile_after_run(
    copy_root: &Path,
    metadata: &CopiedProfileMetadata,
) -> Result<(), String> {
    let settings = match Settings::load_typed(&copy_root.join("settings.json"))
        .map_err(|_| "copied settings could not be reloaded after native acceptance".to_string())?
    {
        LoadState::Loaded(settings) => settings,
        LoadState::Missing | LoadState::Empty => {
            return Err("copied settings were missing after native acceptance".into());
        }
    };
    let expected_log_path = copied_profile_trace_path(copy_root)
        .to_string_lossy()
        .into_owned();
    let expected_clipboard_settings =
        serde_json::to_value(multi_launcher::settings::ClipboardModifyPluginSettings::default())
            .map_err(|_| {
                "default clipboard settings could not be serialized for the safety audit"
                    .to_string()
            })?;
    let settings_checks = [
        (
            "hotkey",
            settings.hotkey.as_deref() == Some(ACCEPTANCE_HOTKEY),
        ),
        ("help_hotkey", settings.help_hotkey.is_none()),
        ("quit_hotkey", settings.quit_hotkey.is_none()),
        ("index_paths", settings.index_paths.is_none()),
        ("plugin_dirs", settings.plugin_dirs.is_none()),
        (
            "enabled_plugins",
            settings.enabled_plugins.as_ref().is_some_and(|plugins| {
                plugins.len() == 1
                    && plugins
                        .iter()
                        .any(|plugin| plugin.eq_ignore_ascii_case("radial"))
            }),
        ),
        (
            "log_file",
            matches!(settings.log_file.as_ref(), Some(LogFile::Path(path)) if path == &expected_log_path),
        ),
        (
            "plugin_settings",
            settings.plugin_settings.len() == 1
                && settings.plugin_settings.get("clipboard_modify")
                    == Some(&expected_clipboard_settings),
        ),
        ("pinned_panels", settings.pinned_panels.is_empty()),
        ("multi_manager.enabled", !settings.multi_manager.enabled),
        (
            "multi_manager.auto_reconnect_on_load",
            !settings.multi_manager.auto_reconnect_on_load,
        ),
        ("multi_manager.auto_save", !settings.multi_manager.auto_save),
        (
            "multi_manager.save_on_exit",
            !settings.multi_manager.save_on_exit,
        ),
        (
            "radial.global_item_inputs",
            !settings.radial.global_item_inputs,
        ),
        (
            "radial_submenu_migration",
            settings.radial_submenu_migration == metadata.expected_submenu_migration_receipt,
        ),
        (
            "multi_manager.workspaces_path",
            Path::new(&settings.multi_manager.workspaces_path).starts_with(copy_root),
        ),
        (
            "multi_manager.bindings_path",
            Path::new(&settings.multi_manager.bindings_path).starts_with(copy_root),
        ),
        (
            "screenshot_dir",
            settings
                .screenshot_dir
                .as_deref()
                .is_some_and(|path| Path::new(path).starts_with(copy_root)),
        ),
    ];
    if let Some((failed_check, _)) = settings_checks.iter().find(|(_, passed)| !passed) {
        return Err(format!(
            "copied settings failed safety check: {failed_check}"
        ));
    }
    if sha256_file(&copy_root.join("actions.json"))
        .map_err(|_| "copied actions could not be hashed after native acceptance".to_string())?
        != metadata.launch_actions_sha256
    {
        return Err("copied inert action catalog changed during native acceptance".into());
    }
    let radial_bytes = fs::read(copy_root.join("radial.json")).map_err(|_| {
        "copied radial document could not be read after native acceptance".to_string()
    })?;
    let radial = multi_launcher::radial::migration::decode_document(&radial_bytes)
        .map_err(|_| "copied radial document failed typed decoding after authoring".to_string())?;
    validate_radial_document(&radial.document)
        .map_err(|_| "copied radial document failed validation after authoring".to_string())?;
    Ok(())
}

fn sanitize_copied_report(report: &mut AcceptanceReport) {
    report.profile.temporary_data_root = "private copied-profile temporary directory".into();
    report.artifacts.clear();
    for case in &mut report.cases {
        case.artifacts.clear();
        case.observed = if matches!(case.status, CaseStatus::Passed) {
            copied_safe_evidence(case)
        } else {
            format!(
                "{}{}",
                case.failure_stage
                    .map(|stage| format!("failed at {stage:?}; "))
                    .unwrap_or_default(),
                "private copied-profile diagnostics were not included in the public report"
            )
        };
    }
}

fn copied_safe_evidence(case: &AcceptanceCaseResult) -> String {
    let id = case.id.as_str();
    let original = case.observed.as_str();
    if id == "CP_D1" {
        let round_trip =
            report_evidence_value(original, "tree_round_trip=").unwrap_or("unavailable");
        return retain_safe_evidence_owned(
            original,
            vec![
                format!("tree_round_trip={round_trip}"),
                "checked_pointer=true".into(),
                "tree_selected=true".into(),
            ],
        );
    }
    let fixed_tokens: Option<&[&str]> = match id {
        "CP_D2" => Some(&[
            "text_edit=restored",
            "tab_focus=menu_combo",
            "unsaved=false",
        ]),
        "CP_A2" => Some(&[
            "geometry=[8,10]",
            "candidate_ids_preserved=true",
            "committed=true",
        ]),
        "CP_A3" => Some(&[
            "blank_cell_selected=true",
            "catalog_rank_gt_50=true",
            "searched_action_assigned=true",
        ]),
        "CP_D3" => Some(&[
            "root_hidden=true",
            "designer_responsive=true",
            "preview_stop=accepted",
            "root_shown=true",
            "hook_pairs=true",
        ]),
        "CP_D6" => Some(&[
            "keep_editing=retained_dirty",
            "draft_glow=true",
            "discard=saved_json_unchanged",
        ]),
        "CP_D7" => Some(&[
            "pending_request=true",
            "cancelled_before_prompt=true",
            "late_reply=rejected",
            "stop=accepted",
            "no_reopen=1s",
            "marker_clean=true",
        ]),
        _ => None,
    };
    if let Some(tokens) = fixed_tokens {
        return retain_safe_evidence(original, tokens);
    }
    match id {
        "CP_PREFLIGHT" => {
            "typed copied profile validated; normalized writes are confined to the isolated copy"
                .into()
        }
        "CP_SOURCE_INTEGRITY" => {
            "source inventory hashes match before and after copied-profile acceptance".into()
        }
        "CP_R1" => {
            let values = [
                report_evidence_value(original, "artifact_retention="),
                report_evidence_value(original, "artifact_count="),
                report_evidence_value(original, "artifact_bytes="),
                report_evidence_value(original, "artifact_id="),
                report_evidence_value(original, "artifact_sha256="),
            ];
            let [
                Some(retention),
                Some(count),
                Some(bytes),
                Some(id),
                Some(hash),
            ] = values
            else {
                return "evidence:v1; required typed facts were not retained".into();
            };
            retain_safe_evidence_owned(
                original,
                vec![
                    format!("artifact_retention={retention}"),
                    format!("artifact_count={count}"),
                    format!("artifact_bytes={bytes}"),
                    format!("artifact_id={id}"),
                    format!("artifact_sha256={hash}"),
                ],
            )
        }
        "R0" => {
            "copied-profile report identities, case bounds, hashes, and typed evidence validated"
                .into()
        }
        "R2" | "CLEANUP" => "copied native child and private profile cleanup verified".into(),
        "CP_A5" => {
            let transition = report_evidence_value(original, "glow=").unwrap_or("unavailable");
            let mut tokens = vec![format!("glow={transition}")];
            tokens.extend([
                "preview_reply=accepted".to_string(),
                "preview_rendered=true".to_string(),
            ]);
            retain_safe_evidence_owned(original, tokens)
        }
        "CP_A6" => {
            let glow = report_evidence_value(original, "glow=").unwrap_or("unavailable");
            let mut tokens = vec![
                "typed_radial=decoded".to_string(),
                "authored_geometry=[8,10]".to_string(),
                "action_binding=true".to_string(),
                "after_action=close_tree".to_string(),
                "original_menus_preserved=true".to_string(),
                format!("glow={glow}"),
                "reopened=true".to_string(),
            ];
            tokens.shrink_to_fit();
            retain_safe_evidence_owned(original, tokens)
        }
        "CP_A7" => {
            let undo = report_evidence_value(original, "undo_restored=").unwrap_or("unavailable");
            let redo = report_evidence_value(original, "redo_restored=").unwrap_or("unavailable");
            retain_safe_evidence_owned(
                original,
                vec![
                    format!("undo_restored={undo}"),
                    format!("redo_restored={redo}"),
                ],
            )
        }
        _ => "copied-profile native behavior passed with bounded typed evidence".into(),
    }
}

fn retain_safe_evidence(original: &str, tokens: &[&str]) -> String {
    retain_safe_evidence_owned(
        original,
        tokens.iter().map(|token| (*token).to_string()).collect(),
    )
}

fn retain_safe_evidence_owned(original: &str, tokens: Vec<String>) -> String {
    if tokens
        .iter()
        .any(|token| !evidence_contains_token(original, token))
    {
        return "evidence:v1; required typed facts were not retained".into();
    }
    format!("evidence:v1; {}", tokens.join("; "))
}

fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(windows)]
fn run_windows_deterministic(
    arguments: Arguments,
    output: PathBuf,
    _report_path: PathBuf,
    copied_profile_requested: bool,
) -> Result<AcceptanceReport, String> {
    let run_started = Instant::now();
    let started_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
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
        schema_version: 6,
        run_id,
        mode: "native_windows",
        started_unix_ms,
        finished_unix_ms: 0,
        copied_profile_status: if copied_profile_requested {
            CopiedProfileStatus::Running
        } else {
            CopiedProfileStatus::NotRun
        },
        copied_profile: None,
        private_artifacts: None,
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
    report.outcome = if report.passed_native_cases() {
        "passed"
    } else {
        "failed"
    };
    Ok(report)
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
            "CP_R1",
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
    let is_copied_profile = report.mode == "native_windows_copied_profile";
    if !copied_status_contract_is_valid(report) {
        return Err("copied-profile status does not match the active report mode".into());
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
    let case_ids: &[&str] = if is_copied_profile {
        &COPIED_CASE_IDS
    } else {
        &CASE_IDS
    };
    if report.started_unix_ms == 0
        || report.finished_unix_ms < report.started_unix_ms
        || report.cases.len() + 1 != case_ids.len()
    {
        return Err("report elapsed time or pre-final case count is invalid".into());
    }
    for (index, case) in report.cases.iter().enumerate() {
        if !case_ids.contains(&case.id.as_str())
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
    if is_copied_profile {
        validate_private_artifact_report(report)?;
        validate_copied_style_evidence_relations(report)?;
    }
    if !case_ids
        .iter()
        .filter(|id| **id != "R0")
        .all(|id| report.cases.iter().any(|case| case.id == *id))
    {
        return Err("required native case identifiers are missing before R0".into());
    }
    Ok(format!(
        "source revision and candidate/runner hashes verified; {} profile hashes validated; copied_profile_status={}; {} bounded typed case records have unique IDs and evidence fields; elapsed_ms={}",
        if is_copied_profile {
            "copied launch profile"
        } else {
            "deterministic fixture"
        },
        match report.copied_profile_status {
            CopiedProfileStatus::NotRun => "not_run",
            CopiedProfileStatus::Running => "running",
            CopiedProfileStatus::Passed => "passed",
            CopiedProfileStatus::Failed => "failed",
        },
        report.cases.len(),
        report.finished_unix_ms - report.started_unix_ms
    ))
}

#[cfg(windows)]
fn copied_status_contract_is_valid(report: &AcceptanceReport) -> bool {
    let copied_summary_is_valid = report.copied_profile.as_ref().map_or(true, |summary| {
        validate_copied_profile_summary(report, summary).is_ok()
    });
    if !copied_summary_is_valid {
        return false;
    }
    if report.mode == "native_windows_copied_profile" {
        match report.copied_profile_status {
            CopiedProfileStatus::Passed => {
                report
                    .copied_profile
                    .as_ref()
                    .is_some_and(|summary| summary.source_unchanged)
                    && copied_cases_passed_before_r0(report)
            }
            CopiedProfileStatus::Failed => !copied_cases_passed_before_r0(report),
            CopiedProfileStatus::NotRun | CopiedProfileStatus::Running => false,
        }
    } else if report.mode == "native_windows" {
        match report.copied_profile_status {
            CopiedProfileStatus::NotRun | CopiedProfileStatus::Running => {
                report.copied_profile.is_none()
            }
            CopiedProfileStatus::Passed => report
                .copied_profile
                .as_ref()
                .is_some_and(|summary| summary.source_unchanged),
            CopiedProfileStatus::Failed => true,
        }
    } else {
        false
    }
}

#[cfg(windows)]
fn validate_copied_profile_summary(
    report: &AcceptanceReport,
    summary: &CopiedProfileSummary,
) -> Result<(), String> {
    let hashes = [
        &summary.source_tree_sha256_before,
        &summary.copied_initial_tree_sha256,
        &summary.source_settings_sha256,
        &summary.source_radial_sha256,
        &summary.copied_initial_settings_sha256,
        &summary.copied_initial_radial_sha256,
        &summary.launch_settings_sha256,
        &summary.launch_radial_sha256,
        &summary.launch_actions_sha256,
    ];
    if hashes.iter().any(|hash| !is_sha256(hash))
        || summary
            .source_actions_sha256
            .as_deref()
            .is_some_and(|hash| !is_sha256(hash))
        || summary
            .copied_initial_actions_sha256
            .as_deref()
            .is_some_and(|hash| !is_sha256(hash))
        || summary
            .source_tree_sha256_after
            .as_ref()
            .is_some_and(|hash| !is_sha256(hash))
    {
        return Err("copied-profile summary contains a malformed SHA-256 identity".into());
    }
    if summary.source_tree_sha256_before != summary.copied_initial_tree_sha256
        || summary.source_settings_sha256 != summary.copied_initial_settings_sha256
        || summary.source_radial_sha256 != summary.copied_initial_radial_sha256
        || summary.source_actions_sha256 != summary.copied_initial_actions_sha256
        || (report.mode == "native_windows_copied_profile"
            && (summary.launch_settings_sha256 != report.profile.settings_sha256
                || summary.launch_radial_sha256 != report.profile.radial_sha256
                || summary.launch_actions_sha256 != report.profile.actions_sha256))
        || !(2..=copied_profile::MAX_PROFILE_FILES).contains(&summary.copied_file_count)
        || summary.copied_total_bytes == 0
        || summary.copied_total_bytes > copied_profile::MAX_PROFILE_TOTAL_BYTES
    {
        return Err("copied-profile summary identities, counts, or launch hashes disagree".into());
    }
    let after_matches = summary
        .source_tree_sha256_after
        .as_ref()
        .is_some_and(|after| after == &summary.source_tree_sha256_before);
    if summary.source_unchanged != after_matches {
        return Err("copied-profile source-integrity flag disagrees with its post-run hash".into());
    }
    Ok(())
}

#[cfg(windows)]
fn validate_private_artifact_report(report: &AcceptanceReport) -> Result<(), String> {
    let summary = report.private_artifacts.as_ref().ok_or_else(|| {
        "copied-profile private diagnostic artifact summary is missing".to_string()
    })?;
    let expected_retention = match summary.status {
        private_artifacts::PrivateArtifactStatus::Retained
            if summary
                .artifact_id
                .as_deref()
                .is_some_and(private_artifacts::is_opaque_id) =>
        {
            "retained"
        }
        private_artifacts::PrivateArtifactStatus::EphemeralValidated
            if summary.artifact_id.is_none() =>
        {
            "ephemeral"
        }
        _ => return Err("copied-profile private diagnostic disposition is invalid".into()),
    };
    if !(4..=private_artifacts::MAX_PRIVATE_ARTIFACT_FILES).contains(&summary.file_count)
        || summary.total_bytes == 0
        || summary.total_bytes > private_artifacts::MAX_PRIVATE_ARTIFACT_BYTES
        || summary
            .manifest_sha256
            .as_deref()
            .is_none_or(|hash| !is_sha256(hash))
    {
        return Err(
            "copied-profile private diagnostic summary is incomplete or out of bounds".into(),
        );
    }
    let case = report
        .cases
        .iter()
        .find(|case| case.id == "CP_R1")
        .ok_or_else(|| "copied-profile CP_R1 artifact case is missing".to_string())?;
    let observed = case.observed.as_str();
    if !matches!(case.status, CaseStatus::Passed)
        || report_evidence_value(observed, "artifact_retention=") != Some(expected_retention)
        || report_evidence_value(observed, "artifact_count=")
            .and_then(|value| value.parse::<usize>().ok())
            != Some(summary.file_count)
        || report_evidence_value(observed, "artifact_bytes=")
            .and_then(|value| value.parse::<u64>().ok())
            != Some(summary.total_bytes)
        || report_evidence_value(observed, "artifact_id=")
            != Some(summary.artifact_id.as_deref().unwrap_or("none"))
        || report_evidence_value(observed, "artifact_sha256=") != summary.manifest_sha256.as_deref()
    {
        return Err("copied-profile CP_R1 evidence disagrees with private artifact summary".into());
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn validate_required_case_evidence(
    id: &str,
    status: CaseStatus,
    observed: &str,
) -> Result<(), String> {
    if id == "CP_R1" {
        let retention = report_evidence_value(observed, "artifact_retention=");
        let artifact_id = report_evidence_value(observed, "artifact_id=");
        if !matches!(status, CaseStatus::Passed)
            || !observed.starts_with("evidence:v1;")
            || !matches!(retention, Some("ephemeral" | "retained"))
            || report_evidence_value(observed, "artifact_count=")
                .and_then(|value| value.parse::<usize>().ok())
                .is_none_or(|count| {
                    !(4..=private_artifacts::MAX_PRIVATE_ARTIFACT_FILES).contains(&count)
                })
            || report_evidence_value(observed, "artifact_bytes=")
                .and_then(|value| value.parse::<u64>().ok())
                .is_none_or(|bytes| {
                    bytes == 0 || bytes > private_artifacts::MAX_PRIVATE_ARTIFACT_BYTES
                })
            || match retention {
                Some("retained") => {
                    artifact_id.is_none_or(|id| !private_artifacts::is_opaque_id(id))
                }
                Some("ephemeral") => artifact_id != Some("none"),
                _ => true,
            }
            || report_evidence_value(observed, "artifact_sha256=")
                .is_none_or(|hash| !is_sha256(hash))
        {
            return Err("copied case CP_R1 omitted bounded private artifact evidence".into());
        }
        return Ok(());
    }
    if id == "CP_D1" {
        if !matches!(status, CaseStatus::Passed)
            || !observed.starts_with("evidence:v1;")
            || report_evidence_value(observed, "tree_round_trip=")
                .is_none_or(|value| !["true", "false"].contains(&value))
            || !evidence_contains_token(observed, "checked_pointer=true")
            || !evidence_contains_token(observed, "tree_selected=true")
        {
            return Err("copied case CP_D1 omitted checked Tree pointer evidence".into());
        }
        return Ok(());
    }
    if id == "CP_A5" {
        validate_copied_style_evidence(
            id,
            status,
            observed,
            &["preview_reply=accepted", "preview_rendered=true"],
        )?;
        if !matches!(
            report_evidence_value(observed, "glow="),
            Some("true->false" | "false->true")
        ) {
            return Err("copied case CP_A5 omitted its reversible style transition".into());
        }
        return Ok(());
    }
    if id == "CP_A6" {
        validate_copied_style_evidence(
            id,
            status,
            observed,
            &[
                "typed_radial=decoded",
                "authored_geometry=[8,10]",
                "action_binding=true",
                "after_action=close_tree",
                "original_menus_preserved=true",
                "reopened=true",
            ],
        )?;
        if !matches!(
            report_evidence_value(observed, "glow="),
            Some("true" | "false")
        ) {
            return Err("copied case CP_A6 omitted its persisted style value".into());
        }
        return Ok(());
    }
    if id == "CP_A7" {
        if !matches!(status, CaseStatus::Passed)
            || !observed.starts_with("evidence:v1;")
            || !observed.contains("undo_restored=")
            || !observed.contains("redo_restored=")
        {
            return Err("copied case CP_A7 omitted its saved-style Undo/Redo evidence".into());
        }
        let saved = report_evidence_value(observed, "undo_restored=")
            .ok_or_else(|| "copied case CP_A7 omitted the saved style value".to_string())?;
        let redone = report_evidence_value(observed, "redo_restored=")
            .ok_or_else(|| "copied case CP_A7 omitted the edited style value".to_string())?;
        if !["true", "false"].contains(&saved)
            || !["true", "false"].contains(&redone)
            || saved == redone
        {
            return Err(
                "copied case CP_A7 did not distinguish saved and redone style values".into(),
            );
        }
        return Ok(());
    }
    let evidence_id = id.strip_prefix("CP_").unwrap_or(id);
    let Some(required) = required_case_evidence(evidence_id) else {
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

fn validate_copied_style_evidence(
    id: &str,
    status: CaseStatus,
    observed: &str,
    required: &[&str],
) -> Result<(), String> {
    if !matches!(status, CaseStatus::Passed) || !observed.starts_with("evidence:v1;") {
        return Err(format!(
            "copied case {id} omitted its versioned evidence summary"
        ));
    }
    if let Some(missing) = required.iter().find(|fact| !observed.contains(**fact)) {
        return Err(format!("copied case {id} omitted required fact {missing}"));
    }
    Ok(())
}

fn report_evidence_value<'a>(observed: &'a str, prefix: &str) -> Option<&'a str> {
    observed
        .split_once("evidence:v1;")?
        .1
        .split(';')
        .map(str::trim)
        .find_map(|field| field.strip_prefix(prefix))
        .map(|value| value.split([' ', ',']).next().unwrap_or_default())
}

fn evidence_contains_token(observed: &str, expected: &str) -> bool {
    observed
        .split_once("evidence:v1;")
        .map(|(_, fields)| fields)
        .is_some_and(|fields| fields.split(';').any(|field| field.trim() == expected))
}

fn validate_copied_style_evidence_relations(report: &AcceptanceReport) -> Result<(), String> {
    let observed = |id: &str| {
        report
            .cases
            .iter()
            .find(|case| case.id == id)
            .map(|case| case.observed.as_str())
    };
    let a5 =
        observed("CP_A5").ok_or_else(|| "copied CP_A5 style evidence is missing".to_string())?;
    let a6 = observed("CP_A6")
        .ok_or_else(|| "copied CP_A6 persistence evidence is missing".to_string())?;
    let a7 = observed("CP_A7")
        .ok_or_else(|| "copied CP_A7 lifecycle evidence is missing".to_string())?;
    let transition = report_evidence_value(a5, "glow=")
        .and_then(|value| value.split_once("->"))
        .ok_or_else(|| "copied CP_A5 style transition is malformed".to_string())?;
    let saved = report_evidence_value(a6, "glow=")
        .ok_or_else(|| "copied CP_A6 persisted style value is missing".to_string())?;
    let undone = report_evidence_value(a7, "undo_restored=")
        .ok_or_else(|| "copied CP_A7 undo value is missing".to_string())?;
    let redone = report_evidence_value(a7, "redo_restored=")
        .ok_or_else(|| "copied CP_A7 redo value is missing".to_string())?;
    if transition.1 != saved
        || transition.0 == transition.1
        || undone != saved
        || !["true", "false"].contains(&redone)
        || redone == saved
    {
        return Err("copied style edit, saved value, Undo, and Redo evidence disagree".into());
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
        "CP_R1" => {
            "copied-profile diagnostic evidence validates; failing bundles persist outside the copied profile"
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

#[cfg(windows)]
fn copied_profile_trace_path(copy_root: &Path) -> PathBuf {
    copy_root.join("acceptance.log")
}

#[cfg(windows)]
fn prepare_copied_profile(
    source_inventory: &copied_profile::ProfileInventory,
    copy_root: &Path,
) -> Result<CopiedProfileMetadata, String> {
    let initial_copy = source_inventory.copy_to(copy_root)?;
    let source_hash = |name: &str| {
        source_inventory
            .file_hash(name)
            .map(str::to_owned)
            .ok_or_else(|| format!("profile copy requires a regular root {name}"))
    };
    let source_settings_sha256 = source_hash("settings.json")?;
    let source_radial_sha256 = source_hash("radial.json")?;
    let source_actions_sha256 = match source_inventory.file_hash("actions.json") {
        Some(hash) => Some(hash.to_owned()),
        None => match fs::symlink_metadata(source_inventory.root.join("actions.json")) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Ok(_) => return Err("profile root actions.json must be a regular file".into()),
            Err(_) => return Err("profile root actions.json could not be inspected".into()),
        },
    };
    let copied_initial_settings_sha256 = initial_copy
        .file_hash("settings.json")
        .ok_or_else(|| "copied settings.json was not inventoried".to_string())?
        .to_owned();
    let copied_initial_radial_sha256 = initial_copy
        .file_hash("radial.json")
        .ok_or_else(|| "copied radial.json was not inventoried".to_string())?
        .to_owned();
    let copied_initial_actions_sha256 = initial_copy.file_hash("actions.json").map(str::to_owned);
    if copied_initial_settings_sha256 != source_settings_sha256
        || copied_initial_radial_sha256 != source_radial_sha256
        || copied_initial_actions_sha256 != source_actions_sha256
    {
        return Err(
            "copied critical profile bytes differ from the initial source inventory".into(),
        );
    }

    let settings_path = copy_root.join("settings.json");
    let mut settings = match Settings::load_typed(&settings_path)
        .map_err(|_| "copied settings.json could not be typed-loaded".to_string())?
    {
        LoadState::Loaded(settings) => settings,
        LoadState::Missing | LoadState::Empty => {
            return Err("copied settings.json must contain a typed settings object".into());
        }
    };
    if settings
        .radial_submenu_migration
        .as_ref()
        .is_some_and(|receipt| {
            matches!(
                receipt.state,
                SubmenuMigrationState::Prepared | SubmenuMigrationState::UndoPrepared
            )
        })
    {
        return Err(
            "copied profile has an unfinished submenu migration receipt with external recovery paths".into(),
        );
    }
    let expected_submenu_migration_receipt = settings.radial_submenu_migration.clone();
    let radial_bytes = fs::read(copy_root.join("radial.json"))
        .map_err(|_| "copied radial.json could not be read".to_string())?;
    let decoded = multi_launcher::radial::migration::decode_document(&radial_bytes)
        .map_err(|_| "copied radial.json failed typed decode or validation".to_string())?;
    validate_radial_document(&decoded.document)
        .map_err(|_| "copied radial.json failed current document validation".to_string())?;
    if decoded.document.skins.is_empty() {
        return Err("copied radial document has no skin to exercise in the Designer".into());
    }
    let reserved_hotkey = [(
        "acceptance launcher".to_string(),
        ACCEPTANCE_HOTKEY.to_string(),
    )];
    let issues = radial_settings::validate(&settings.radial, &decoded.document, &reserved_hotkey);
    if !issues.is_empty() {
        return Err(
            "copied radial settings conflict with the safe acceptance hotkey or document".into(),
        );
    }
    let original_menu_sha256 = decoded
        .document
        .menus
        .iter()
        .map(|menu| {
            serde_json::to_vec(menu)
                .map(|bytes| sha256_bytes(&bytes))
                .map_err(|_| "could not hash an initial radial menu definition".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let restore_menu_name = decoded
        .document
        .menus
        .iter()
        .find(|menu| menu.id == decoded.document.default_menu_id)
        .map(|menu| menu.name.clone())
        .ok_or_else(|| "copied radial document default menu could not be resolved".to_string())?;

    let startup_actions =
        multi_launcher::actions::load_startup_actions(copy_root.join("actions.json"));
    if startup_actions.diagnostic.is_some() {
        return Err("copied actions.json could not be typed-loaded".into());
    }
    let mut actions = startup_actions.actions;
    let target_action_index = actions
        .len()
        .checked_add(ACCEPTANCE_ACTION_COUNT - 1)
        .ok_or_else(|| "copied action index overflowed".to_string())?;
    for index in actions.len()..=target_action_index {
        let label = format!("Radial Acceptance Harmless Action {index:03}");
        if actions.iter().any(|action| action.label == label) {
            return Err("copied profile already contains an acceptance action label".into());
        }
        actions.push(multi_launcher::actions::Action {
            label,
            desc: "Inert native acceptance authoring target; never dispatched".into(),
            action: format!("radial_acceptance_inert_{index:03}"),
            args: None,
        });
    }

    settings.hotkey = Some(ACCEPTANCE_HOTKEY.to_string());
    settings.help_hotkey = None;
    settings.quit_hotkey = None;
    settings.index_paths = None;
    settings.plugin_dirs = None;
    settings.enabled_plugins = Some(std::collections::HashSet::from(["radial".to_string()]));
    settings.enabled_capabilities = Some(std::collections::HashMap::new());
    settings.plugin_settings.clear();
    settings.plugin_settings.insert(
        "clipboard_modify".into(),
        serde_json::to_value(multi_launcher::settings::ClipboardModifyPluginSettings::default())
            .map_err(|_| "default clipboard settings could not be serialized".to_string())?,
    );
    settings.pinned_panels.clear();
    settings.debug_logging = true;
    settings.log_file = Some(LogFile::Path(
        copied_profile_trace_path(copy_root)
            .to_string_lossy()
            .into_owned(),
    ));
    settings.screenshot_dir = Some(
        copy_root
            .join("acceptance_screenshots")
            .to_string_lossy()
            .into_owned(),
    );
    settings.screenshot_save_file = false;
    settings.screenshot_auto_save = false;
    settings.screenshot_use_editor = false;
    settings.radial.global_item_inputs = false;
    settings.dashboard.enabled = false;
    settings.dashboard.config_path = None;
    settings.dashboard.default_location = None;
    settings.multi_manager.enabled = false;
    settings.multi_manager.workspaces_path = copy_root
        .join("multi_manager/workspaces.json")
        .to_string_lossy()
        .into_owned();
    settings.multi_manager.bindings_path = copy_root
        .join("multi_manager/bindings.json")
        .to_string_lossy()
        .into_owned();
    settings.multi_manager.auto_reconnect_on_load = false;
    settings.multi_manager.auto_save = false;
    settings.multi_manager.save_on_exit = false;

    let post_issues =
        radial_settings::validate(&settings.radial, &decoded.document, &reserved_hotkey);
    if !post_issues.is_empty() || multi_launcher::hotkey::parse_hotkey(ACCEPTANCE_HOTKEY).is_none()
    {
        return Err("normalized copied profile failed acceptance settings validation".into());
    }
    let settings_bytes = serde_json::to_vec_pretty(&settings)
        .map_err(|_| "normalized copied settings could not be serialized".to_string())?;
    let actions_bytes = serde_json::to_vec_pretty(&actions)
        .map_err(|_| "copied acceptance action catalog could not be serialized".to_string())?;
    fs::write(&settings_path, settings_bytes).map_err(|_| {
        "normalized copied settings could not be written inside the copy".to_string()
    })?;
    fs::write(copy_root.join("actions.json"), actions_bytes)
        .map_err(|_| "copied action catalog could not be written inside the copy".to_string())?;

    let launch_settings_sha256 = sha256_file(&settings_path)
        .map_err(|_| "could not hash normalized copied settings".to_string())?;
    let launch_radial_sha256 = sha256_file(&copy_root.join("radial.json"))
        .map_err(|_| "could not hash launch radial document".to_string())?;
    let launch_actions_sha256 = sha256_file(&copy_root.join("actions.json"))
        .map_err(|_| "could not hash copied action catalog".to_string())?;
    let (copied_file_count, copied_total_bytes) = source_inventory.file_count_and_bytes();
    Ok(CopiedProfileMetadata {
        initial_copy_tree_sha256: initial_copy.tree_sha256,
        copied_file_count,
        copied_total_bytes,
        source_settings_sha256,
        source_radial_sha256,
        source_actions_sha256,
        copied_initial_settings_sha256,
        copied_initial_radial_sha256,
        copied_initial_actions_sha256,
        launch_settings_sha256,
        launch_radial_sha256,
        launch_actions_sha256,
        target_action_index,
        skin_index: 0,
        restore_menu_name,
        original_menu_sha256,
        expected_submenu_migration_receipt,
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
    let status_text = match report.copied_profile_status {
        CopiedProfileStatus::NotRun => "not_run",
        CopiedProfileStatus::Running => "running",
        CopiedProfileStatus::Passed => "passed",
        CopiedProfileStatus::Failed => "failed",
    };
    if stored != contents || !stored.contains(&format!("Copied profile: {status_text}")) {
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
        CopiedProfileStatus::Running => "running",
        CopiedProfileStatus::Passed => "passed",
        CopiedProfileStatus::Failed => "failed",
    };
    contents.push_str(&format!("Copied profile: {copied_profile_status}\n"));
    contents.push_str(&format!(
        "Profile SHA-256: settings={}, radial={}, actions={}\n",
        report.profile.settings_sha256, report.profile.radial_sha256, report.profile.actions_sha256
    ));
    if let Some(copied) = &report.copied_profile {
        contents.push_str(&format!(
            "Copied profile inventory: files={}, bytes={}, source_unchanged={}, source_tree_sha256={}, copied_initial_tree_sha256={}, source_tree_sha256_after={:?}\n",
            copied.copied_file_count,
            copied.copied_total_bytes,
            copied.source_unchanged,
            copied.source_tree_sha256_before,
            copied.copied_initial_tree_sha256,
            copied.source_tree_sha256_after
        ));
        contents.push_str(&format!(
            "Copied profile critical SHA-256: source_settings={}, source_radial={}, source_actions={:?}, initial_settings={}, initial_radial={}, initial_actions={:?}, launch_settings={}, launch_radial={}, launch_actions={}\n",
            copied.source_settings_sha256,
            copied.source_radial_sha256,
            copied.source_actions_sha256,
            copied.copied_initial_settings_sha256,
            copied.copied_initial_radial_sha256,
            copied.copied_initial_actions_sha256,
            copied.launch_settings_sha256,
            copied.launch_radial_sha256,
            copied.launch_actions_sha256
        ));
    }
    if let Some(private) = &report.private_artifacts {
        let status = match private.status {
            private_artifacts::PrivateArtifactStatus::NotRun => "not_run",
            private_artifacts::PrivateArtifactStatus::EphemeralValidated => "ephemeral_validated",
            private_artifacts::PrivateArtifactStatus::Retained => "retained",
            private_artifacts::PrivateArtifactStatus::Failed => "failed",
        };
        contents.push_str(&format!(
            "Private diagnostic evidence: status={status}, id={:?}, files={}, bytes={}, manifest_sha256={:?}\n",
            private.artifact_id,
            private.file_count,
            private.total_bytes,
            private.manifest_sha256
        ));
    }
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
        "Usage: radial_acceptance [--launcher <source-matched multi_launcher.exe>] --output <new-run-directory> [--profile-copy <profile-directory>] [--source-revision <id>] [--h6-repeat immediate|quiescent|production-only-diagnostic] [--mouse-gestures enabled|disabled-diagnostic] [--keep-profile-on-failure]\n       radial_acceptance [--candidate <multi_launcher.exe>] --report <new-report.json> [--profile-copy <profile-directory>]"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    fn migration_receipt(state: SubmenuMigrationState) -> SubmenuPresentationMigrationReceipt {
        SubmenuPresentationMigrationReceipt {
            migration_id: "radial-submenu-same-center-v1".into(),
            version: 1,
            state,
            source_settings_sha256: "1".repeat(64),
            source_radial_sha256: "2".repeat(64),
            settings_backup_path: r"G:\outside-source\settings.json.bak".into(),
            settings_backup_sha256: "3".repeat(64),
            settings_source_existed: true,
            radial_backup_path: r"G:\outside-source\radial.json.bak".into(),
            radial_backup_sha256: "4".repeat(64),
            settings_default_before: multi_launcher::radial::model::SubmenuPresentation::Cascade,
            settings_default_target: multi_launcher::radial::model::SubmenuPresentation::SameCenter,
            changed_menus: Vec::new(),
            target_radial_revision: 2,
            target_radial_sha256: "5".repeat(64),
            target_settings_content_sha256: "6".repeat(64),
            undo_restored_menu_ids: Vec::new(),
            undo_source_radial_sha256: None,
            undo_target_radial_revision: None,
            undo_target_radial_sha256: None,
            undo_source_settings_content_sha256: None,
            undo_target_settings_content_sha256: None,
            undo_settings_default_source: None,
            undo_restores_settings_default: false,
            failure: None,
        }
    }

    fn parse(args: &[&str]) -> Result<Arguments, String> {
        let parsed = parse_arguments(args.iter().map(OsString::from))?;
        match parsed {
            ParseResult::Run(arguments) => Ok(arguments),
            ParseResult::Help => Err("unexpected help result".into()),
        }
    }

    fn acceptance_report(mode: &'static str) -> AcceptanceReport {
        AcceptanceReport {
            schema_version: 6,
            run_id: "test-run".into(),
            mode,
            started_unix_ms: 1,
            finished_unix_ms: 2,
            copied_profile_status: CopiedProfileStatus::NotRun,
            copied_profile: None,
            private_artifacts: None,
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
                runner_sha256: Some("b".repeat(64)),
                child_process_id: None,
                child_started_unix_ms: None,
                source_revision: Some("deadbeef".into()),
                monitors: Vec::new(),
            },
            profile: ProfileIdentity {
                mode: "test",
                temporary_data_root: "private profile".into(),
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
        }
    }

    fn passing_copied_report() -> AcceptanceReport {
        let mut report = acceptance_report("native_windows_copied_profile");
        report.copied_profile_status = CopiedProfileStatus::Passed;
        let private_summary = private_artifacts::PrivateArtifactSummary {
            status: private_artifacts::PrivateArtifactStatus::Retained,
            artifact_id: Some("multi-launcher-private-evidence-test-1234".into()),
            file_count: 4,
            total_bytes: 64,
            manifest_sha256: Some("f".repeat(64)),
        };
        report.private_artifacts = Some(private_summary.clone());
        for id in COPIED_CASE_IDS {
            report.push_case(AcceptanceCaseResult {
                id: id.into(),
                status: CaseStatus::Passed,
                elapsed_ms: 1,
                expected: "bounded expectation".into(),
                observed: if id == "CP_R1" {
                    private_artifact_evidence(&private_summary)
                } else {
                    "bounded evidence".into()
                },
                failure_stage: None,
                artifacts: Vec::new(),
            });
        }
        report.cleanup.child_closed_normally = true;
        report.cleanup.child_owned_windows_closed = true;
        report.cleanup.profile_removed = true;
        report.cleanup.cursor_restored = true;
        report.cleanup.input_desktop_released = true;
        report
    }

    #[cfg(windows)]
    #[test]
    fn cleanup_only_failure_retains_private_diagnostic_bundle() {
        let mut report = acceptance_report("native_windows_copied_profile");
        report.environment.child_process_id = Some(4242);
        report.cleanup.child_closed_normally = true;
        report.cleanup.child_owned_windows_closed = true;
        report.cleanup.cursor_restored = true;
        report.cleanup.input_desktop_released = true;
        assert!(!copied_failure_requires_retention(&report, true, true));

        // Cursor restoration is the only failed native cleanup signal; all native
        // cases, the isolated profile audit, and the supplied source inventory pass.
        report.cleanup.cursor_restored = false;
        assert!(report.cases.is_empty());
        assert!(copied_failure_requires_retention(&report, true, true));

        let profile = tempfile::tempdir().unwrap();
        let names = [
            "case-R1-trace.log",
            "case-R1-windows.json",
            "case-R1-private.log",
            "case-R1.png",
        ];
        let paths = names
            .into_iter()
            .map(|name| {
                let path = profile.path().join(name);
                fs::write(&path, name.as_bytes()).unwrap();
                path
            })
            .collect::<Vec<_>>();
        let staged = private_artifacts::stage_diagnostics(profile.path(), paths).unwrap();
        let retained = if copied_failure_requires_retention(&report, true, true) {
            Some(staged.retain())
        } else {
            None
        };
        let profile_path = profile.path().to_path_buf();
        profile.close().unwrap();
        assert!(!profile_path.exists());

        let retained = retained.expect("cleanup-only failure must retain its private evidence");
        private_artifacts::verify_retained_artifacts(&retained.directory, &retained.summary)
            .unwrap();
        assert!(retained.directory.join("case-R1.png").is_file());
        fs::remove_dir_all(&retained.directory).unwrap();
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
    fn profile_copy_option_is_retained_without_changing_deterministic_defaults() {
        let arguments = parse(&["--output", "run", "--profile-copy", "supplied-profile"]).unwrap();
        assert_eq!(
            arguments.profile_copy,
            Some(PathBuf::from("supplied-profile"))
        );
        assert_eq!(arguments.h6_repeat_mode, H6RepeatMode::Quiescent);
        assert_eq!(arguments.mouse_gesture_mode, MouseGestureMode::Enabled);

        let without_copy = parse(&["--output", "run"]).unwrap();
        assert!(without_copy.profile_copy.is_none());
    }

    #[test]
    fn copied_case_capacity_and_dual_report_gate_are_explicit() {
        let copied = passing_copied_report();
        assert_eq!(copied.cases.len(), COPIED_CASE_IDS.len());
        assert!(copied.passed());
        assert!(!copied.capacity_saturated);

        assert!(aggregate_native_cases_passed(true, None, true));
        assert!(!aggregate_native_cases_passed(false, None, true));
        assert!(aggregate_native_cases_passed(true, Some(&copied), true));
        assert!(!aggregate_native_cases_passed(true, Some(&copied), false));

        let mut failed_copy = copied.clone();
        failed_copy.cases[0].status = CaseStatus::Failed;
        failed_copy.cases[0].failure_stage = Some(FailureStage::Environment);
        failed_copy.copied_profile_status = CopiedProfileStatus::Failed;
        assert!(!aggregate_native_cases_passed(
            true,
            Some(&failed_copy),
            true
        ));

        let mut saturated = copied;
        for index in 0..MAX_CASES {
            saturated.push_case(AcceptanceCaseResult {
                id: format!("extra-{index}"),
                status: CaseStatus::Passed,
                elapsed_ms: 0,
                expected: "bounded".into(),
                observed: "bounded".into(),
                failure_stage: None,
                artifacts: Vec::new(),
            });
        }
        assert!(saturated.capacity_saturated);
        assert!(!aggregate_native_cases_passed(true, Some(&saturated), true));
    }

    #[cfg(windows)]
    #[test]
    fn copied_report_publication_failure_blocks_aggregate_pass_without_partial_json() {
        let output = tempfile::tempdir().unwrap();
        let deterministic_path = output.path().join("report.json");
        let copied_path = copied_report_path(&deterministic_path);
        let copied_text_path = copied_path.with_extension("txt");
        fs::write(&copied_text_path, "owned conflict sentinel").unwrap();
        let copied = passing_copied_report();

        assert!(persist_copied_report_pair(&copied_path, &copied).is_err());
        assert!(!copied_path.exists());
        assert_eq!(
            fs::read_to_string(copied_text_path).unwrap(),
            "owned conflict sentinel"
        );
        assert!(!aggregate_native_cases_passed(true, Some(&copied), false));
        assert!(fs::read_dir(output.path()).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".radial-acceptance-report-stage-")
        }));
    }

    #[test]
    fn copied_report_sanitization_removes_private_paths_and_raw_observations() {
        let private_marker = "private copied UIA value and screenshot path";
        let mut report = acceptance_report("native_windows_copied_profile");
        report.copied_profile_status = CopiedProfileStatus::Failed;
        report.profile.temporary_data_root = private_marker.into();
        report.artifacts.push(private_marker.into());
        report.cases.push(AcceptanceCaseResult {
            id: "CP_D0".into(),
            status: CaseStatus::Failed,
            elapsed_ms: 1,
            expected: "bounded expectation".into(),
            observed: private_marker.into(),
            failure_stage: Some(FailureStage::DesignerReadiness),
            artifacts: vec![private_marker.into()],
        });
        sanitize_copied_report(&mut report);

        let json = serde_json::to_string(&report).unwrap();
        let text = render_text_report(&report);
        assert!(!json.contains(private_marker));
        assert!(!text.contains(private_marker));
        assert!(report.artifacts.is_empty());
        assert!(report.cases[0].artifacts.is_empty());
        assert_eq!(
            report.profile.temporary_data_root,
            "private copied-profile temporary directory"
        );
    }

    #[test]
    fn copied_style_sanitization_preserves_typed_values_and_relations() {
        let private_marker = "copied private menu title and path";
        let mut report = acceptance_report("native_windows_copied_profile");
        report.cases = vec![
            AcceptanceCaseResult {
                id: "CP_A5".into(),
                status: CaseStatus::Passed,
                elapsed_ms: 1,
                expected: "preview style transition".into(),
                observed: format!(
                    "{private_marker}; evidence:v1; glow=true->false; preview_reply=accepted; preview_rendered=true"
                ),
                failure_stage: None,
                artifacts: vec![private_marker.into()],
            },
            AcceptanceCaseResult {
                id: "CP_A6".into(),
                status: CaseStatus::Passed,
                elapsed_ms: 1,
                expected: "saved copied style".into(),
                observed: format!(
                    "{private_marker}; evidence:v1; typed_radial=decoded; authored_geometry=[8,10]; action_binding=true; after_action=close_tree; original_menus_preserved=true; glow=false; reopened=true"
                ),
                failure_stage: None,
                artifacts: Vec::new(),
            },
            AcceptanceCaseResult {
                id: "CP_A7".into(),
                status: CaseStatus::Passed,
                elapsed_ms: 1,
                expected: "saved-style Undo and Redo".into(),
                observed: format!(
                    "{private_marker}; evidence:v1; undo_restored=false; redo_restored=true"
                ),
                failure_stage: None,
                artifacts: Vec::new(),
            },
        ];

        sanitize_copied_report(&mut report);
        for case in &report.cases {
            validate_required_case_evidence(&case.id, case.status, &case.observed).unwrap();
        }
        validate_copied_style_evidence_relations(&report).unwrap();
        let public = serde_json::to_string(&report).unwrap();
        assert!(!public.contains(private_marker));
        assert_eq!(
            report.cases[0].observed,
            "evidence:v1; glow=true->false; preview_reply=accepted; preview_rendered=true"
        );
        assert_eq!(
            report.cases[2].observed,
            "evidence:v1; undo_restored=false; redo_restored=true"
        );

        report.cases[2].observed = "evidence:v1; undo_restored=true; redo_restored=false".into();
        assert!(
            validate_copied_style_evidence_relations(&report)
                .unwrap_err()
                .contains("style edit, saved value, Undo, and Redo evidence disagree")
        );
    }

    #[test]
    fn copied_pointer_and_disposable_evidence_survive_privacy_sanitization() {
        let private_marker = "private copied profile path and close detail";
        let mut report = acceptance_report("native_windows_copied_profile");
        report.cases = vec![
            AcceptanceCaseResult {
                id: "CP_D1".into(),
                status: CaseStatus::Passed,
                elapsed_ms: 1,
                expected: "checked copied Tree navigation".into(),
                observed: format!(
                    "{private_marker}; evidence:v1; tree_round_trip=true; checked_pointer=true; tree_selected=true"
                ),
                failure_stage: None,
                artifacts: Vec::new(),
            },
            AcceptanceCaseResult {
                id: "CP_D7".into(),
                status: CaseStatus::Passed,
                elapsed_ms: 1,
                expected: "cancelled late disposable preview".into(),
                observed: format!(
                    "{private_marker}; evidence:v1; pending_request=true; request_id=42; generation=9; cancelled_before_prompt=true; late_reply=rejected; stop=accepted; stop_request=43; no_reopen=1s; marker_clean=true; child_alive=true"
                ),
                failure_stage: None,
                artifacts: Vec::new(),
            },
        ];

        sanitize_copied_report(&mut report);
        for case in &report.cases {
            validate_required_case_evidence(&case.id, case.status, &case.observed).unwrap();
        }
        let public = serde_json::to_string(&report).unwrap();
        assert!(!public.contains(private_marker));
        assert_eq!(
            report.cases[0].observed,
            "evidence:v1; tree_round_trip=true; checked_pointer=true; tree_selected=true"
        );
        assert_eq!(
            report.cases[1].observed,
            "evidence:v1; pending_request=true; cancelled_before_prompt=true; late_reply=rejected; stop=accepted; no_reopen=1s; marker_clean=true"
        );
    }

    #[test]
    fn copied_private_artifact_evidence_is_typed_and_path_free() {
        let marker = "C:\\Users\\profile\\private trace contents";
        let summary = private_artifacts::PrivateArtifactSummary {
            status: private_artifacts::PrivateArtifactStatus::Retained,
            artifact_id: Some("multi-launcher-private-evidence-test-5678".into()),
            file_count: 4,
            total_bytes: 512,
            manifest_sha256: Some("a".repeat(64)),
        };
        let mut report = acceptance_report("native_windows_copied_profile");
        report.private_artifacts = Some(summary.clone());
        report.cases.push(AcceptanceCaseResult {
            id: "CP_R1".into(),
            status: CaseStatus::Passed,
            elapsed_ms: 1,
            expected: "private durable copied-profile evidence".into(),
            observed: format!("{marker}; {}", private_artifact_evidence(&summary)),
            failure_stage: None,
            artifacts: vec![marker.into()],
        });

        sanitize_copied_report(&mut report);
        validate_required_case_evidence("CP_R1", report.cases[0].status, &report.cases[0].observed)
            .unwrap();
        let public = serde_json::to_string(&report).unwrap();
        let text = render_text_report(&report);
        assert!(!public.contains(marker));
        assert!(!text.contains(marker));
        assert!(public.contains("multi-launcher-private-evidence-test-5678"));
        assert!(public.contains(&"a".repeat(64)));
        assert_eq!(
            report.cases[0].observed,
            format!(
                "evidence:v1; artifact_retention=retained; artifact_count=4; artifact_bytes=512; artifact_id=multi-launcher-private-evidence-test-5678; artifact_sha256={}",
                "a".repeat(64)
            )
        );
    }

    #[cfg(windows)]
    #[test]
    fn copied_ephemeral_artifact_evidence_is_typed_and_has_no_retained_id() {
        let summary = private_artifacts::PrivateArtifactSummary {
            status: private_artifacts::PrivateArtifactStatus::EphemeralValidated,
            artifact_id: None,
            file_count: 4,
            total_bytes: 512,
            manifest_sha256: Some("b".repeat(64)),
        };
        let mut report = acceptance_report("native_windows_copied_profile");
        report.private_artifacts = Some(summary.clone());
        report.cases.push(AcceptanceCaseResult {
            id: "CP_R1".into(),
            status: CaseStatus::Passed,
            elapsed_ms: 1,
            expected: "validated ephemeral copied-profile evidence".into(),
            observed: private_artifact_evidence(&summary),
            failure_stage: None,
            artifacts: Vec::new(),
        });

        sanitize_copied_report(&mut report);
        validate_required_case_evidence("CP_R1", report.cases[0].status, &report.cases[0].observed)
            .unwrap();
        validate_private_artifact_report(&report).unwrap();
        let public = serde_json::to_string(&report).unwrap();
        assert!(public.contains("artifact_retention=ephemeral"));
        assert!(public.contains("artifact_id=none"));
        assert!(!public.contains("multi-launcher-private-evidence-"));
    }

    #[cfg(windows)]
    #[test]
    fn copied_r0_status_contract_matches_final_report_state() {
        let source_tree = "1".repeat(64);
        let source_settings = "2".repeat(64);
        let source_radial = "3".repeat(64);
        let source_actions = "4".repeat(64);
        let launch_settings = "5".repeat(64);
        let launch_radial = "6".repeat(64);
        let launch_actions = "7".repeat(64);
        let summary = CopiedProfileSummary {
            source_tree_sha256_before: source_tree.clone(),
            copied_initial_tree_sha256: source_tree.clone(),
            source_tree_sha256_after: Some(source_tree),
            copied_file_count: 3,
            copied_total_bytes: 100,
            source_settings_sha256: source_settings.clone(),
            source_radial_sha256: source_radial.clone(),
            source_actions_sha256: Some(source_actions.clone()),
            copied_initial_settings_sha256: source_settings,
            copied_initial_radial_sha256: source_radial,
            copied_initial_actions_sha256: Some(source_actions),
            launch_settings_sha256: launch_settings.clone(),
            launch_radial_sha256: launch_radial.clone(),
            launch_actions_sha256: launch_actions.clone(),
            source_unchanged: true,
        };
        let mut report = acceptance_report("native_windows_copied_profile");
        report.profile.settings_sha256 = launch_settings;
        report.profile.radial_sha256 = launch_radial;
        report.profile.actions_sha256 = launch_actions;
        report.copied_profile = Some(summary.clone());
        report.copied_profile_status = CopiedProfileStatus::Passed;
        for id in COPIED_CASE_IDS.into_iter().filter(|id| *id != "R0") {
            report.cases.push(AcceptanceCaseResult {
                id: id.into(),
                status: CaseStatus::Passed,
                elapsed_ms: 1,
                expected: "expected".into(),
                observed: "evidence:v1; bounded=true".into(),
                failure_stage: None,
                artifacts: Vec::new(),
            });
        }
        assert!(copied_status_contract_is_valid(&report));

        let mut absent_actions_summary = summary.clone();
        absent_actions_summary.source_actions_sha256 = None;
        absent_actions_summary.copied_initial_actions_sha256 = None;
        absent_actions_summary.copied_file_count = 2;
        let serialized_summary = serde_json::to_value(&absent_actions_summary).unwrap();
        assert!(serialized_summary["source_actions_sha256"].is_null());
        assert!(serialized_summary["copied_initial_actions_sha256"].is_null());
        report.copied_profile = Some(absent_actions_summary.clone());
        assert!(validate_copied_profile_summary(&report, &absent_actions_summary).is_ok());
        assert!(copied_status_contract_is_valid(&report));

        let mut mismatched_actions_summary = absent_actions_summary.clone();
        mismatched_actions_summary.source_actions_sha256 = Some("4".repeat(64));
        report.copied_profile = Some(mismatched_actions_summary);
        assert!(!copied_status_contract_is_valid(&report));

        let mut malformed_actions_summary = absent_actions_summary.clone();
        malformed_actions_summary.source_actions_sha256 = Some("malformed".into());
        malformed_actions_summary.copied_initial_actions_sha256 = Some("malformed".into());
        report.copied_profile = Some(malformed_actions_summary);
        assert!(!copied_status_contract_is_valid(&report));

        report.copied_profile = Some(summary.clone());

        report.cases[1].status = CaseStatus::Failed;
        report.cases[1].failure_stage = Some(FailureStage::Environment);
        report.copied_profile_status = CopiedProfileStatus::Failed;
        assert!(copied_status_contract_is_valid(&report));

        report.cases[1].status = CaseStatus::Passed;
        report.cases[1].failure_stage = None;
        assert!(!copied_status_contract_is_valid(&report));
        report.copied_profile_status = CopiedProfileStatus::Running;
        assert!(!copied_status_contract_is_valid(&report));

        let mut aggregate = acceptance_report("native_windows");
        assert!(copied_status_contract_is_valid(&aggregate));
        aggregate.copied_profile_status = CopiedProfileStatus::Running;
        assert!(copied_status_contract_is_valid(&aggregate));
        aggregate.copied_profile = Some(summary);
        aggregate.copied_profile_status = CopiedProfileStatus::Passed;
        assert!(copied_status_contract_is_valid(&aggregate));
        aggregate.copied_profile_status = CopiedProfileStatus::Failed;
        assert!(copied_status_contract_is_valid(&aggregate));
    }

    #[cfg(windows)]
    #[test]
    fn copied_profile_normalization_changes_only_the_private_copy() {
        let source = tempfile::tempdir().unwrap();
        let fixture = deterministic_fixture(
            &source.path().join("acceptance.log"),
            MouseGestureMode::Enabled,
        )
        .unwrap();
        fs::write(source.path().join("settings.json"), &fixture.settings_json).unwrap();
        fs::write(source.path().join("radial.json"), &fixture.radial_json).unwrap();
        fs::write(source.path().join("actions.json"), &fixture.actions_json).unwrap();
        let source_inventory = copied_profile::ProfileInventory::scan(source.path()).unwrap();
        let original_settings = source_inventory
            .file_hash("settings.json")
            .unwrap()
            .to_owned();
        let original_radial = source_inventory
            .file_hash("radial.json")
            .unwrap()
            .to_owned();
        let original_actions = source_inventory
            .file_hash("actions.json")
            .unwrap()
            .to_owned();
        let copy = tempfile::tempdir().unwrap();

        let metadata = prepare_copied_profile(&source_inventory, copy.path()).unwrap();
        assert!(source_inventory.still_matches_source());
        assert_eq!(metadata.source_settings_sha256, original_settings);
        assert_eq!(metadata.source_radial_sha256, original_radial);
        assert_eq!(
            metadata.source_actions_sha256.as_deref(),
            Some(original_actions.as_str())
        );
        assert_eq!(metadata.copied_initial_settings_sha256, original_settings);
        assert_eq!(metadata.copied_initial_radial_sha256, original_radial);
        assert_eq!(
            metadata.copied_initial_actions_sha256.as_deref(),
            Some(original_actions.as_str())
        );
        assert_ne!(
            metadata.launch_settings_sha256,
            metadata.copied_initial_settings_sha256
        );
        assert_eq!(
            metadata.launch_radial_sha256,
            metadata.copied_initial_radial_sha256
        );
        assert_ne!(
            metadata.launch_actions_sha256,
            metadata.copied_initial_actions_sha256.as_deref().unwrap()
        );

        let mut settings = match Settings::load_typed(&copy.path().join("settings.json")).unwrap() {
            LoadState::Loaded(settings) => settings,
            LoadState::Missing | LoadState::Empty => panic!("normalized settings must load"),
        };
        let suite_trace_path = copied_profile_trace_path(copy.path());
        assert!(matches!(
            settings.log_file.as_ref(),
            Some(LogFile::Path(path)) if Path::new(path) == suite_trace_path
        ));
        assert_eq!(settings.hotkey.as_deref(), Some(ACCEPTANCE_HOTKEY));
        assert!(settings.help_hotkey.is_none());
        assert!(settings.quit_hotkey.is_none());
        assert!(settings.index_paths.is_none());
        assert!(settings.plugin_dirs.is_none());
        assert!(!settings.radial.global_item_inputs);
        assert_eq!(
            settings.plugin_settings.get("clipboard_modify"),
            Some(
                &serde_json::to_value(
                    multi_launcher::settings::ClipboardModifyPluginSettings::default()
                )
                .unwrap()
            )
        );
        assert_eq!(
            settings.enabled_plugins.as_ref().unwrap(),
            &std::collections::HashSet::from(["radial".to_string()])
        );
        assert!(!multi_launcher::plugins::clipboard_modify::migrate_enablement(&mut settings));
        assert_eq!(
            settings.enabled_plugins.as_ref().unwrap(),
            &std::collections::HashSet::from(["radial".to_string()])
        );
        validate_copied_profile_after_run(copy.path(), &metadata).unwrap();

        let mut migrated_settings = settings.clone();
        migrated_settings.plugin_settings.remove("clipboard_modify");
        assert!(
            multi_launcher::plugins::clipboard_modify::migrate_enablement(&mut migrated_settings)
        );
        fs::write(
            copy.path().join("settings.json"),
            serde_json::to_vec_pretty(&migrated_settings).unwrap(),
        )
        .unwrap();
        let migration_audit = validate_copied_profile_after_run(copy.path(), &metadata)
            .expect_err("startup migration must not enable Clipboard Modify in the copy");
        assert!(migration_audit.contains("enabled_plugins"));
        assert!(!settings.multi_manager.enabled);
        assert!(Path::new(&settings.multi_manager.workspaces_path).starts_with(copy.path()));
        assert!(Path::new(&settings.multi_manager.bindings_path).starts_with(copy.path()));

        let radial = sha256_file(&copy.path().join("radial.json")).unwrap();
        assert_eq!(radial, original_radial);
        let actions =
            multi_launcher::actions::load_actions_typed(copy.path().join("actions.json")).unwrap();
        let LoadState::Loaded(actions) = actions else {
            panic!("copied actions must load after adding inert entries");
        };
        let inert = &actions[metadata.target_action_index];
        assert!(inert.label.contains("Radial Acceptance Harmless Action"));
        assert!(inert.action.starts_with("radial_acceptance_inert_"));
        assert!(source_inventory.still_matches_source());
    }

    #[cfg(windows)]
    #[test]
    fn copied_profile_accepts_missing_and_empty_actions_without_touching_source() {
        let fixture_dir = tempfile::tempdir().unwrap();
        let fixture = deterministic_fixture(
            &fixture_dir.path().join("acceptance.log"),
            MouseGestureMode::Enabled,
        )
        .unwrap();

        for actions_present in [false, true] {
            let source = tempfile::tempdir().unwrap();
            fs::write(source.path().join("settings.json"), &fixture.settings_json).unwrap();
            fs::write(source.path().join("radial.json"), &fixture.radial_json).unwrap();
            if actions_present {
                fs::write(source.path().join("actions.json"), b"").unwrap();
            }
            let source_inventory = copied_profile::ProfileInventory::scan(source.path()).unwrap();
            let expected_actions_sha256 = actions_present.then(|| sha256_bytes(b""));
            assert_eq!(
                source_inventory
                    .file_hash("actions.json")
                    .map(str::to_owned),
                expected_actions_sha256
            );

            let untouched_copy = tempfile::tempdir().unwrap();
            let initial_copy = source_inventory.copy_to(untouched_copy.path()).unwrap();
            assert_eq!(initial_copy.tree_sha256, source_inventory.tree_sha256);
            assert_eq!(initial_copy.file_count, if actions_present { 3 } else { 2 });
            assert_eq!(
                initial_copy.file_hash("actions.json").map(str::to_owned),
                expected_actions_sha256
            );
            assert_eq!(
                untouched_copy.path().join("actions.json").exists(),
                actions_present
            );

            let copy = tempfile::tempdir().unwrap();
            let metadata = prepare_copied_profile(&source_inventory, copy.path()).unwrap();
            assert_eq!(metadata.source_actions_sha256, expected_actions_sha256);
            assert_eq!(
                metadata.copied_initial_actions_sha256,
                expected_actions_sha256
            );
            assert_eq!(
                metadata.initial_copy_tree_sha256,
                source_inventory.tree_sha256
            );
            assert!(source_inventory.still_matches_source());
            assert_eq!(source.path().join("actions.json").exists(), actions_present);

            let LoadState::Loaded(actions) =
                multi_launcher::actions::load_actions_typed(copy.path().join("actions.json"))
                    .unwrap()
            else {
                panic!("copied acceptance action catalog must load as typed actions");
            };
            assert_eq!(actions.len(), ACCEPTANCE_ACTION_COUNT);
            assert!(actions.iter().all(|action| {
                action
                    .label
                    .starts_with("Radial Acceptance Harmless Action ")
                    && action.action.starts_with("radial_acceptance_inert_")
                    && action.args.is_none()
            }));
            assert_eq!(
                metadata.target_action_index,
                actions.len().saturating_sub(1)
            );
            validate_copied_profile_after_run(copy.path(), &metadata).unwrap();

            let source_after = copied_profile::ProfileInventory::scan(source.path()).unwrap();
            assert_eq!(source_after, source_inventory);
            let summary = metadata.report_summary(
                &source_inventory,
                Some(source_after.tree_sha256.clone()),
                true,
            );
            assert_eq!(summary.source_actions_sha256, expected_actions_sha256);
            assert_eq!(
                summary.copied_initial_actions_sha256,
                expected_actions_sha256
            );
            let mut report = acceptance_report("native_windows_copied_profile");
            report.profile.settings_sha256 = metadata.launch_settings_sha256.clone();
            report.profile.radial_sha256 = metadata.launch_radial_sha256.clone();
            report.profile.actions_sha256 = metadata.launch_actions_sha256.clone();
            assert!(validate_copied_profile_summary(&report, &summary).is_ok());
        }
    }

    #[cfg(windows)]
    #[test]
    fn copied_profile_rejects_invalid_typed_settings_radial_and_actions() {
        let fixture_dir = tempfile::tempdir().unwrap();
        let fixture = deterministic_fixture(
            &fixture_dir.path().join("acceptance.log"),
            MouseGestureMode::Enabled,
        )
        .unwrap();
        for invalid_name in ["settings.json", "radial.json", "actions.json"] {
            let source = tempfile::tempdir().unwrap();
            fs::write(source.path().join("settings.json"), &fixture.settings_json).unwrap();
            fs::write(source.path().join("radial.json"), &fixture.radial_json).unwrap();
            fs::write(source.path().join("actions.json"), &fixture.actions_json).unwrap();
            fs::write(source.path().join(invalid_name), b"{ invalid json").unwrap();
            let inventory = copied_profile::ProfileInventory::scan(source.path()).unwrap();
            let copy = tempfile::tempdir().unwrap();
            assert!(prepare_copied_profile(&inventory, copy.path()).is_err());
            assert!(inventory.still_matches_source());
        }
    }

    #[cfg(windows)]
    #[test]
    fn copied_profile_disables_global_shortcuts_and_preserves_stable_migration_receipts() {
        for state in [
            SubmenuMigrationState::Applied,
            SubmenuMigrationState::Undone,
        ] {
            let source = tempfile::tempdir().unwrap();
            let fixture = deterministic_fixture(
                &source.path().join("acceptance.log"),
                MouseGestureMode::Enabled,
            )
            .unwrap();
            let mut settings: Settings = serde_json::from_slice(&fixture.settings_json).unwrap();
            settings.radial.global_item_inputs = true;
            let receipt = migration_receipt(state);
            settings.radial_submenu_migration = Some(receipt.clone());
            fs::write(
                source.path().join("settings.json"),
                serde_json::to_vec_pretty(&settings).unwrap(),
            )
            .unwrap();
            fs::write(source.path().join("radial.json"), &fixture.radial_json).unwrap();
            fs::write(source.path().join("actions.json"), &fixture.actions_json).unwrap();
            let source_inventory = copied_profile::ProfileInventory::scan(source.path()).unwrap();
            let copy = tempfile::tempdir().unwrap();

            let metadata = prepare_copied_profile(&source_inventory, copy.path()).unwrap();
            let normalized = match Settings::load_typed(&copy.path().join("settings.json")).unwrap()
            {
                LoadState::Loaded(settings) => settings,
                LoadState::Missing | LoadState::Empty => {
                    panic!("normalized settings must remain a typed object")
                }
            };
            assert!(!normalized.radial.global_item_inputs);
            assert_eq!(normalized.radial_submenu_migration, Some(receipt.clone()));
            assert_eq!(metadata.expected_submenu_migration_receipt, Some(receipt));
            assert_eq!(
                sha256_file(&copy.path().join("radial.json")).unwrap(),
                source_inventory.file_hash("radial.json").unwrap()
            );
            validate_copied_profile_after_run(copy.path(), &metadata).unwrap();

            let original = match Settings::load_typed(&source.path().join("settings.json")).unwrap()
            {
                LoadState::Loaded(settings) => settings,
                LoadState::Missing | LoadState::Empty => {
                    panic!("source settings must remain a typed object")
                }
            };
            assert!(original.radial.global_item_inputs);
            assert_eq!(original.radial_submenu_migration.unwrap().state, state);
            assert!(source_inventory.still_matches_source());
        }
    }

    #[cfg(windows)]
    #[test]
    fn copied_profile_rejects_unfinished_migration_receipts_before_normalization() {
        for state in [
            SubmenuMigrationState::Prepared,
            SubmenuMigrationState::UndoPrepared,
        ] {
            let source = tempfile::tempdir().unwrap();
            let fixture = deterministic_fixture(
                &source.path().join("acceptance.log"),
                MouseGestureMode::Enabled,
            )
            .unwrap();
            let mut settings: Settings = serde_json::from_slice(&fixture.settings_json).unwrap();
            let receipt = migration_receipt(state);
            settings.radial_submenu_migration = Some(receipt.clone());
            fs::write(
                source.path().join("settings.json"),
                serde_json::to_vec_pretty(&settings).unwrap(),
            )
            .unwrap();
            fs::write(source.path().join("radial.json"), &fixture.radial_json).unwrap();
            fs::write(source.path().join("actions.json"), &fixture.actions_json).unwrap();
            let source_inventory = copied_profile::ProfileInventory::scan(source.path()).unwrap();
            let copy = tempfile::tempdir().unwrap();

            let error = prepare_copied_profile(&source_inventory, copy.path()).unwrap_err();
            assert!(error.contains("unfinished submenu migration receipt"));
            let unchanged_copy =
                match Settings::load_typed(&copy.path().join("settings.json")).unwrap() {
                    LoadState::Loaded(settings) => settings,
                    LoadState::Missing | LoadState::Empty => {
                        panic!("rejected copy still retains source settings for diagnosis")
                    }
                };
            assert_eq!(unchanged_copy.radial_submenu_migration, Some(receipt));
            assert!(source_inventory.still_matches_source());
        }
    }

    #[test]
    fn report_capacity_saturation_is_explicit_and_fails_r0() {
        let mut report = AcceptanceReport {
            schema_version: 6,
            run_id: "test".into(),
            mode: "test",
            started_unix_ms: 1,
            finished_unix_ms: 2,
            copied_profile_status: CopiedProfileStatus::NotRun,
            copied_profile: None,
            private_artifacts: None,
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
            schema_version: 6,
            run_id: "test".into(),
            mode: "test",
            started_unix_ms: 100,
            finished_unix_ms: 250,
            copied_profile_status: CopiedProfileStatus::NotRun,
            copied_profile: None,
            private_artifacts: None,
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
