//! Opt-in preflight for the native radial acceptance runner.
//!
//! This first runner milestone validates a source-matched candidate, the
//! deterministic typed profile fixture, and an optional copied-profile source.
//! Native process/window/input driving is implemented by the later runner
//! milestone; a preflight report is explicitly labeled so it cannot be mistaken
//! for a native acceptance pass.

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

const MAX_REPORT_CASES: usize = 32;
const MAX_REPORT_ARTIFACTS: usize = 32;
const MAX_REPORT_TEXT_BYTES: usize = 2_048;
const MAX_CASE_TEXT_BYTES: usize = 512;

#[derive(Debug)]
struct Arguments {
    candidate: PathBuf,
    report: PathBuf,
    profile_copy: Option<PathBuf>,
    source_revision: Option<String>,
    preflight_only: bool,
    keep_profile_on_failure: bool,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum ReportMode {
    PreflightOnly,
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
    Cli,
    Candidate,
    Profile,
    Fixture,
}

#[derive(Serialize)]
struct CandidateIdentity {
    executable: String,
    sha256: String,
}

#[derive(Serialize)]
struct EnvironmentIdentity {
    os: String,
    architecture: String,
    process_id: u32,
    runner_sha256: Option<String>,
    source_revision: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum ProfileMode {
    DeterministicFixture,
    CopiedProfileSourcePreflight,
}

#[derive(Serialize)]
struct ProfileIdentity {
    mode: ProfileMode,
    source_directory: Option<String>,
    source_settings_sha256: Option<String>,
    source_radial_sha256: Option<String>,
    fixture_settings_sha256: String,
    fixture_radial_sha256: String,
    temporary_data_root: Option<String>,
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

#[derive(Serialize)]
struct AcceptanceReport {
    schema_version: u16,
    run_id: String,
    mode: ReportMode,
    candidate: CandidateIdentity,
    environment: EnvironmentIdentity,
    profile: ProfileIdentity,
    cases: Vec<AcceptanceCaseResult>,
    artifacts: Vec<String>,
    cleanup_complete: bool,
}

impl AcceptanceReport {
    fn push_case(&mut self, case: AcceptanceCaseResult) {
        if self.cases.len() < MAX_REPORT_CASES {
            self.cases.push(case);
        }
    }

    #[allow(dead_code)]
    fn push_artifact(&mut self, path: impl AsRef<str>) {
        if self.artifacts.len() < MAX_REPORT_ARTIFACTS {
            self.artifacts
                .push(bounded_text(path.as_ref(), MAX_REPORT_TEXT_BYTES));
        }
    }
}

struct DeterministicFixture {
    settings_json: Vec<u8>,
    radial_json: Vec<u8>,
}

struct ProfileSource {
    directory: PathBuf,
    settings_sha256: String,
    radial_sha256: String,
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
        Ok(ParseResult::Run(arguments)) => match preflight(arguments) {
            Ok(report_path) => {
                println!(
                    "Preflight report written to {}. This report does not claim native acceptance.",
                    report_path.display()
                );
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("radial_acceptance preflight failed: {error}");
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
    let mut candidate = None;
    let mut report = None;
    let mut profile_copy = None;
    let mut source_revision = None;
    let mut preflight_only = false;
    let mut keep_profile_on_failure = false;
    let mut args = args.into_iter();

    while let Some(argument) = args.next() {
        match argument.to_str() {
            Some("--help" | "-h") => return Ok(ParseResult::Help),
            Some("--preflight-only") => preflight_only = true,
            Some("--keep-profile-on-failure") => keep_profile_on_failure = true,
            Some("--candidate") => {
                candidate = Some(next_path(&mut args, "--candidate")?);
            }
            Some("--report") => {
                report = Some(next_path(&mut args, "--report")?);
            }
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
            Some(option) => return Err(format!("unknown option: {option}")),
            None => return Err("command-line options must be valid UTF-8".to_string()),
        }
    }

    if !preflight_only {
        return Err("this milestone requires the explicit --preflight-only flag".to_string());
    }

    Ok(ParseResult::Run(Arguments {
        candidate: candidate.ok_or_else(|| "--candidate is required".to_string())?,
        report: report.ok_or_else(|| "--report is required".to_string())?,
        profile_copy,
        source_revision: source_revision.map(|value| bounded_text(&value, 160)),
        preflight_only,
        keep_profile_on_failure,
    }))
}

fn next_path(args: &mut impl Iterator<Item = OsString>, option: &str) -> Result<PathBuf, String> {
    args.next()
        .map(PathBuf::from)
        .ok_or_else(|| format!("{option} requires a path"))
}

fn preflight(arguments: Arguments) -> Result<PathBuf, String> {
    debug_assert!(arguments.preflight_only);
    let started = SystemTime::now();
    let candidate = inspect_candidate(&arguments.candidate)?;
    let fixture = deterministic_fixture().map_err(|error| format!("fixture: {error}"))?;
    let profile_source = arguments
        .profile_copy
        .as_deref()
        .map(inspect_profile_source)
        .transpose()?;

    let current_exe = std::env::current_exe().ok();
    let runner_sha256 = current_exe
        .as_deref()
        .and_then(|path| sha256_file(path).ok());
    let run_id = format!(
        "{:x}-{:x}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    );
    let profile = match profile_source {
        Some(source) => ProfileIdentity {
            mode: ProfileMode::CopiedProfileSourcePreflight,
            source_directory: Some(bounded_text(
                &source.directory.to_string_lossy(),
                MAX_REPORT_TEXT_BYTES,
            )),
            source_settings_sha256: Some(source.settings_sha256),
            source_radial_sha256: Some(source.radial_sha256),
            fixture_settings_sha256: sha256_bytes(&fixture.settings_json),
            fixture_radial_sha256: sha256_bytes(&fixture.radial_json),
            temporary_data_root: None,
        },
        None => ProfileIdentity {
            mode: ProfileMode::DeterministicFixture,
            source_directory: None,
            source_settings_sha256: None,
            source_radial_sha256: None,
            fixture_settings_sha256: sha256_bytes(&fixture.settings_json),
            fixture_radial_sha256: sha256_bytes(&fixture.radial_json),
            temporary_data_root: None,
        },
    };
    let mut report = AcceptanceReport {
        schema_version: 1,
        run_id,
        mode: ReportMode::PreflightOnly,
        candidate,
        environment: EnvironmentIdentity {
            os: bounded_text(std::env::consts::OS, 64),
            architecture: bounded_text(std::env::consts::ARCH, 64),
            process_id: std::process::id(),
            runner_sha256,
            source_revision: arguments.source_revision,
        },
        profile,
        cases: Vec::with_capacity(1),
        artifacts: Vec::new(),
        cleanup_complete: true,
    };
    report.push_case(AcceptanceCaseResult {
        id: "candidate_profile_fixture_preflight".to_string(),
        status: CaseStatus::Passed,
        elapsed_ms: started
            .elapsed()
            .unwrap_or_default()
            .as_millis()
            .min(u64::MAX as u128) as u64,
        expected: bounded_text(
            "candidate, profile source, and typed deterministic fixture pass preflight",
            MAX_CASE_TEXT_BYTES,
        ),
        observed: bounded_text(
            "preflight passed; native process and UI automation are not part of this report",
            MAX_CASE_TEXT_BYTES,
        ),
        failure_stage: None,
        artifacts: Vec::new(),
    });

    write_report(&arguments.report, &report)?;
    if arguments.keep_profile_on_failure {
        // The profile-copy flag is parsed here for forward-compatible CLI
        // validation. No profile copy or native launch occurs during preflight.
    }
    Ok(arguments.report)
}

fn deterministic_fixture() -> Result<DeterministicFixture, String> {
    let mut settings = Settings::default();
    settings.hotkey = Some("F12".to_string());
    settings.help_hotkey = None;
    settings.quit_hotkey = None;
    settings.debug_logging = true;
    settings.log_file = Some(LogFile::Path("acceptance.log".to_string()));
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
    let reserved = [
        ("launcher", settings.hotkey.as_deref()),
        ("help", settings.help_hotkey.as_deref()),
        ("quit", settings.quit_hotkey.as_deref()),
    ]
    .into_iter()
    .filter_map(|(owner, chord)| chord.map(|chord| (owner.to_string(), chord.to_string())))
    .collect::<Vec<_>>();
    let issues = radial_settings::validate(&settings.radial, &document, &reserved);
    if !issues.is_empty() {
        return Err(format!("radial settings are invalid: {issues:?}"));
    }
    multi_launcher::hotkey::parse_hotkey("F12")
        .ok_or_else(|| "the deterministic F12 acceptance chord is not supported".to_string())?;

    Ok(DeterministicFixture {
        settings_json: serde_json::to_vec_pretty(&settings)
            .map_err(|error| format!("serialize settings: {error}"))?,
        radial_json: serde_json::to_vec_pretty(&document)
            .map_err(|error| format!("serialize radial document: {error}"))?,
    })
}

fn inspect_candidate(path: &Path) -> Result<CandidateIdentity, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("candidate is not accessible: {error}"))?;
    if !metadata.is_file() || is_reparse_point(&metadata) {
        return Err("candidate must be a regular, non-reparse executable file".to_string());
    }
    if !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
    {
        return Err("candidate must have an .exe extension".to_string());
    }
    let executable = path
        .canonicalize()
        .map_err(|error| format!("resolve candidate path: {error}"))?;
    let sha256 = sha256_file(&executable).map_err(|error| format!("hash candidate: {error}"))?;
    Ok(CandidateIdentity {
        executable: bounded_text(&executable.to_string_lossy(), MAX_REPORT_TEXT_BYTES),
        sha256,
    })
}

fn inspect_profile_source(path: &Path) -> Result<ProfileSource, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("profile source is not accessible: {error}"))?;
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        return Err(
            "profile source must be a regular directory, not a link or reparse point".to_string(),
        );
    }
    let directory = path
        .canonicalize()
        .map_err(|error| format!("resolve profile source: {error}"))?;
    let settings = inspect_profile_file(&directory.join("settings.json"), "settings.json")?;
    let radial = inspect_profile_file(&directory.join("radial.json"), "radial.json")?;
    Ok(ProfileSource {
        directory,
        settings_sha256: settings,
        radial_sha256: radial,
    })
}

fn inspect_profile_file(path: &Path, label: &str) -> Result<String, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("profile {label} is not accessible: {error}"))?;
    if !metadata.is_file() || is_reparse_point(&metadata) {
        return Err(format!("profile {label} must be a regular file"));
    }
    sha256_file(path).map_err(|error| format!("hash profile {label}: {error}"))
}

fn write_report(path: &Path, report: &AcceptanceReport) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if !parent.is_dir() {
        return Err("report parent directory must already exist".to_string());
    }
    let bytes =
        serde_json::to_vec_pretty(report).map_err(|error| format!("serialize report: {error}"))?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("create report without overwriting an existing file: {error}"))?;
    output
        .write_all(&bytes)
        .map_err(|error| format!("write report: {error}"))
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
        "Usage: radial_acceptance --preflight-only --candidate <multi_launcher.exe> --report <new-report.json> [--profile-copy <profile-directory>] [--source-revision <id>] [--keep-profile-on-failure]"
    );
}
