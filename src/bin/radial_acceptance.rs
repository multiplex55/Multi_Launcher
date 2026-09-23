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
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(windows)]
#[path = "radial_acceptance/native.rs"]
mod native;

const MAX_CASES: usize = 32;
const MAX_ARTIFACTS: usize = 48;
const MAX_PATH_BYTES: usize = 2_048;
const MAX_RESULT_BYTES: usize = 1_024;
const ACCEPTANCE_HOTKEY: &str = "F11";

#[derive(Debug)]
struct Arguments {
    launcher: Option<PathBuf>,
    output: Option<PathBuf>,
    report_file: Option<PathBuf>,
    source_revision: Option<String>,
    keep_profile_on_failure: bool,
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
    profile_removed: bool,
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
    outcome: &'static str,
    candidate: CandidateIdentity,
    environment: EnvironmentIdentity,
    profile: ProfileIdentity,
    cases: Vec<AcceptanceCaseResult>,
    artifacts: Vec<String>,
    cleanup: CleanupResult,
}

impl AcceptanceReport {
    fn push_case(&mut self, case: AcceptanceCaseResult) {
        if self.cases.len() < MAX_CASES {
            self.cases.push(case);
        }
    }

    fn push_artifact(&mut self, path: impl AsRef<str>) {
        if self.artifacts.len() < MAX_ARTIFACTS {
            self.artifacts
                .push(bounded_text(path.as_ref(), MAX_PATH_BYTES));
        }
    }

    fn passed(&self) -> bool {
        !self.cases.is_empty()
            && self
                .cases
                .iter()
                .all(|case| matches!(case.status, CaseStatus::Passed))
            && self.cleanup.child_closed_normally
            && self.cleanup.profile_removed
            && self.cleanup.cursor_restored
            && self.cleanup.input_desktop_released
    }
}

struct DeterministicFixture {
    settings_json: Vec<u8>,
    radial_json: Vec<u8>,
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
    Ok(ParseResult::Run(Arguments {
        launcher,
        output,
        report_file,
        source_revision: source_revision.map(|value| bounded_text(&value, 160)),
        keep_profile_on_failure,
    }))
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
    let log_path = output.join("acceptance.log");
    let fixture = deterministic_fixture(&log_path)?;
    let profile = tempfile::Builder::new()
        .prefix("multi-launcher-radial-acceptance-")
        .tempdir()
        .map_err(|error| format!("create isolated profile directory: {error}"))?;
    let profile_path = profile.path().to_path_buf();
    write_new(&profile_path.join("settings.json"), &fixture.settings_json)?;
    write_new(&profile_path.join("radial.json"), &fixture.radial_json)?;
    let settings_sha256 = sha256_bytes(&fixture.settings_json);
    let radial_sha256 = sha256_bytes(&fixture.radial_json);

    let mut report = AcceptanceReport {
        schema_version: 2,
        run_id,
        mode: "native_windows",
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
            configured_hotkey: ACCEPTANCE_HOTKEY,
            hold_threshold_ms: fixture.hold_threshold_ms,
        },
        cases: Vec::with_capacity(10),
        artifacts: Vec::new(),
        cleanup: CleanupResult::default(),
    };

    let runner_log = output.join("runner.log");
    let mut log = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&runner_log)
        .map_err(|error| format!("create runner log: {error}"))?;
    report.push_artifact(runner_log.to_string_lossy());
    report.push_artifact(log_path.to_string_lossy());
    for filename in ["child.stdout.log", "child.stderr.log"] {
        report.push_artifact(output.join(filename).to_string_lossy());
    }

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
                    native::run_suite(
                        &candidate_executable,
                        &driver_profile,
                        &driver_output,
                        &driver_trace,
                        hold_threshold_ms,
                        &desktop_attachment,
                        &mut report,
                        &mut log,
                    );

                    match before_cursor {
                        Ok(point) => {
                            let already_restored = native::cursor_position().is_ok_and(|current| {
                                current.x == point.x && current.y == point.y
                            });
                            if already_restored {
                                report.cleanup.cursor_restored = true;
                            } else {
                                match native::set_cursor_position(point) {
                                    Ok(()) => report.cleanup.cursor_restored = true,
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

    report.outcome = if report.passed() { "passed" } else { "failed" };
    let passed = report.passed();
    write_report(&report_path, &report)?;
    let text_path = report_path.with_extension("txt");
    write_text_report(&text_path, &report)?;
    println!("Acceptance report: {}", report_path.display());
    Ok((report_path, passed))
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

fn deterministic_fixture(log_path: &Path) -> Result<DeterministicFixture, String> {
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

    let document = RadialDocument::starter();
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
    let bytes = serde_json::to_vec_pretty(report)
        .map_err(|error| format!("serialize acceptance report: {error}"))?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("create acceptance report: {error}"))?;
    file.write_all(&bytes)
        .map_err(|error| format!("write acceptance report: {error}"))
}

fn write_text_report(path: &Path, report: &AcceptanceReport) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("create text report: {error}"))?;
    writeln!(file, "Radial native acceptance: {}", report.outcome)
        .and_then(|()| writeln!(file, "Candidate SHA-256: {}", report.candidate.sha256))
        .and_then(|()| writeln!(file, "Child PID: {:?}", report.environment.child_process_id))
        .and_then(|()| writeln!(file, "Hotkey: {}", report.profile.configured_hotkey))
        .map_err(|error| format!("write text report: {error}"))?;
    for case in &report.cases {
        writeln!(
            file,
            "{}: {:?}{} — {}",
            case.id,
            case.status,
            case.failure_stage
                .map(|stage| format!(" ({stage:?})"))
                .unwrap_or_default(),
            bounded_text(&case.observed, MAX_RESULT_BYTES)
        )
        .map_err(|error| format!("write case summary: {error}"))?;
    }
    writeln!(
        file,
        "Cleanup: child_closed_normally={}, profile_removed={}, foreground_restore_attempted={}, foreground_restored={}, cursor_restored={}, input_desktop_released={}",
        report.cleanup.child_closed_normally,
        report.cleanup.profile_removed,
        report.cleanup.foreground_restore_attempted,
        report.cleanup.foreground_restored,
        report.cleanup.cursor_restored,
        report.cleanup.input_desktop_released
    )
    .map_err(|error| format!("write cleanup summary: {error}"))
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
        "Usage: radial_acceptance [--launcher <source-matched multi_launcher.exe>] --output <new-run-directory> [--source-revision <id>] [--keep-profile-on-failure]\n       radial_acceptance [--candidate <multi_launcher.exe>] --report <new-report.json>"
    );
}
