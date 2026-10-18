//! Opt-in, bounded evidence protocol for local performance probes.
//!
//! This module is only compiled with the `perf-harness` feature. It never
//! starts processes or contacts the network. The caller supplies any known
//! source provenance instead of asking the recorder to run Git commands.

use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_SAMPLES: usize = 20;
pub const REQUIRED_WARMUPS: usize = 3;
pub const MAX_TIMEOUT_SECONDS: u64 = 90;
pub const MAX_RUN_LOG_BYTES: u64 = 30 * 1024 * 1024;
pub const MAX_AGGREGATE_BYTES: u64 = 100 * 1024 * 1024;
pub const EVIDENCE_ROOT: &str = "target/perf-harness";
const RUN_RESERVATION_BYTES: u64 = 32 * 1024 * 1024;
const MAX_EVENT_BYTES: usize = 64 * 1024;
const MAX_SUMMARY_BYTES: usize = 1024 * 1024;
const MAX_METRICS: usize = 64;
const MAX_METRIC_NAME_BYTES: usize = 64;
const MAX_REASON_BYTES: usize = 512;
const MAX_UNIT_BYTES: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Scenario {
    HotfolderDisabled,
    HotfolderEnabledZeroRules,
    Radial8,
    Radial32,
    Radial128,
    DashboardMinimal,
    DashboardTypical,
    DashboardDense,
    RunnerSelfTest,
}

impl Scenario {
    pub const ALL: [Self; 9] = [
        Self::HotfolderDisabled,
        Self::HotfolderEnabledZeroRules,
        Self::Radial8,
        Self::Radial32,
        Self::Radial128,
        Self::DashboardMinimal,
        Self::DashboardTypical,
        Self::DashboardDense,
        Self::RunnerSelfTest,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HotfolderDisabled => "hotfolder_disabled",
            Self::HotfolderEnabledZeroRules => "hotfolder_enabled_zero_rules",
            Self::Radial8 => "radial_8",
            Self::Radial32 => "radial_32",
            Self::Radial128 => "radial_128",
            Self::DashboardMinimal => "dashboard_minimal",
            Self::DashboardTypical => "dashboard_typical",
            Self::DashboardDense => "dashboard_dense",
            Self::RunnerSelfTest => "runner_self_test",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|scenario| scenario.as_str() == value)
    }
}

impl fmt::Display for Scenario {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MeasurementKind {
    CpuHeadless,
    NativeFixture,
    ProductionApp,
    CompileCommand,
    RunnerContainment,
}

impl MeasurementKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CpuHeadless => "cpu_headless",
            Self::NativeFixture => "native_fixture",
            Self::ProductionApp => "production_app",
            Self::CompileCommand => "compile_command",
            Self::RunnerContainment => "runner_containment",
        }
    }

    const fn screen_visibility_kind(self) -> &'static str {
        match self {
            Self::CpuHeadless => "cpu_headless",
            Self::NativeFixture => "native_fixture",
            Self::ProductionApp => "production_app",
            Self::CompileCommand | Self::RunnerContainment => "not_applicable",
        }
    }
}

impl fmt::Display for MeasurementKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeStatus {
    Success,
    Error,
    Timeout,
    Unsupported,
    BlockedEnvironment,
    BlockedDesktop,
    Truncated,
}

impl ProbeStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Error => "error",
            Self::Timeout => "timeout",
            Self::Unsupported => "unsupported",
            Self::BlockedEnvironment => "blocked_environment",
            Self::BlockedDesktop => "blocked_desktop",
            Self::Truncated => "truncated",
        }
    }

    fn is_success(self) -> bool {
        self == Self::Success
    }
}

impl fmt::Display for ProbeStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProbeConfig {
    pub scenario: Scenario,
    pub warmups: usize,
    pub samples: usize,
    pub timeout_seconds: u64,
}

impl ProbeConfig {
    pub fn new(scenario: Scenario) -> Self {
        Self {
            scenario,
            warmups: REQUIRED_WARMUPS,
            samples: MAX_SAMPLES,
            timeout_seconds: MAX_TIMEOUT_SECONDS,
        }
    }

    fn validate(&self) -> Result<(), CliError> {
        if self.warmups != REQUIRED_WARMUPS {
            return Err(CliError::new("warmups must be exactly 3"));
        }
        if !(1..=MAX_SAMPLES).contains(&self.samples) {
            return Err(CliError::new("samples must be between 1 and 20"));
        }
        if !(1..=MAX_TIMEOUT_SECONDS).contains(&self.timeout_seconds) {
            return Err(CliError::new("timeout-seconds must be between 1 and 90"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CliAction {
    Help,
    Run(ProbeConfig),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CliError {
    message: String,
}

impl CliError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CliError {}

/// Parse the shared probe CLI without touching the filesystem or invoking Git.
/// `--out` accepts only the repository-relative `target/perf-harness` root;
/// each run receives a recorder-created unique child directory.
pub fn parse_cli<I, S>(arguments: I) -> Result<CliAction, CliError>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let arguments = arguments
        .into_iter()
        .map(|argument| {
            argument
                .into()
                .into_string()
                .map_err(|_| CliError::new("arguments must be valid UTF-8"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    if arguments
        .iter()
        .any(|argument| argument == "--help" || argument == "-h")
    {
        return if arguments.len() == 1 {
            Ok(CliAction::Help)
        } else {
            Err(CliError::new(
                "--help cannot be combined with other arguments",
            ))
        };
    }

    let mut scenario = None;
    let mut warmups = REQUIRED_WARMUPS;
    let mut samples = MAX_SAMPLES;
    let mut timeout_seconds = MAX_TIMEOUT_SECONDS;
    let mut saw_warmups = false;
    let mut saw_samples = false;
    let mut saw_timeout = false;
    let mut saw_output = false;
    let mut index = 0;

    while index < arguments.len() {
        let flag = arguments[index].as_str();
        index += 1;
        let value = arguments
            .get(index)
            .ok_or_else(|| CliError::new(format!("missing value for {flag}")))?;
        index += 1;
        match flag {
            "--scenario" => {
                if scenario.is_some() {
                    return Err(CliError::new("--scenario may be specified only once"));
                }
                scenario = Some(
                    Scenario::parse(value)
                        .ok_or_else(|| CliError::new(format!("unknown scenario: {value}")))?,
                );
            }
            "--out" => {
                if saw_output {
                    return Err(CliError::new("--out may be specified only once"));
                }
                saw_output = true;
                if !is_allowed_output_root(value) {
                    return Err(CliError::new(
                        "--out must be exactly target/perf-harness (or target\\perf-harness)",
                    ));
                }
            }
            "--warmups" => {
                if saw_warmups {
                    return Err(CliError::new("--warmups may be specified only once"));
                }
                saw_warmups = true;
                warmups = parse_unsigned(value, "warmups")?;
            }
            "--samples" => {
                if saw_samples {
                    return Err(CliError::new("--samples may be specified only once"));
                }
                saw_samples = true;
                samples = parse_unsigned(value, "samples")?;
            }
            "--timeout-seconds" => {
                if saw_timeout {
                    return Err(CliError::new(
                        "--timeout-seconds may be specified only once",
                    ));
                }
                saw_timeout = true;
                timeout_seconds = parse_unsigned(value, "timeout-seconds")? as u64;
            }
            _ => return Err(CliError::new(format!("unknown argument: {flag}"))),
        }
    }

    let scenario = scenario.ok_or_else(|| CliError::new("--scenario is required"))?;
    let config = ProbeConfig {
        scenario,
        warmups,
        samples,
        timeout_seconds,
    };
    config.validate()?;
    Ok(CliAction::Run(config))
}

fn parse_unsigned(value: &str, label: &str) -> Result<usize, CliError> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(CliError::new(format!(
            "{label} must be an unsigned decimal integer"
        )));
    }
    value
        .parse()
        .map_err(|_| CliError::new(format!("{label} is too large")))
}

fn is_allowed_output_root(value: &str) -> bool {
    matches!(value.replace('\\', "/").as_str(), "target/perf-harness")
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MetricValue {
    pub value: Option<f64>,
    pub unit: String,
    pub reason: Option<String>,
}

impl MetricValue {
    pub fn measured(value: f64, unit: impl Into<String>) -> Result<Self, RecorderError> {
        if !value.is_finite() {
            return Err(RecorderError::invalid("metric value must be finite"));
        }
        let unit = bounded_text(unit.into(), MAX_UNIT_BYTES);
        if unit.is_empty() {
            return Err(RecorderError::invalid("metric unit cannot be empty"));
        }
        Ok(Self {
            value: Some(value),
            unit,
            reason: None,
        })
    }

    pub fn missing(unit: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            value: None,
            unit: bounded_text(unit.into(), MAX_UNIT_BYTES),
            reason: Some(safe_reason(reason.into())),
        }
    }

    fn validate(&self) -> Result<(), RecorderError> {
        if self.unit.is_empty() || self.unit.len() > MAX_UNIT_BYTES {
            return Err(RecorderError::invalid("metric unit is empty or too long"));
        }
        match (self.value, self.reason.as_ref()) {
            (Some(value), None) if value.is_finite() => Ok(()),
            (None, Some(reason)) if !reason.is_empty() && reason.len() <= MAX_REASON_BYTES => {
                Ok(())
            }
            _ => Err(RecorderError::invalid(
                "metric must have either a finite value or a missing-value reason",
            )),
        }
    }
}

#[derive(Clone, Debug)]
pub struct SampleRecord {
    pub duration: Duration,
    pub metrics: BTreeMap<String, MetricValue>,
}

impl SampleRecord {
    pub fn new(duration: Duration) -> Self {
        Self {
            duration,
            metrics: BTreeMap::new(),
        }
    }

    pub fn with_metric(mut self, name: impl Into<String>, value: MetricValue) -> Self {
        self.metrics.insert(name.into(), value);
        self
    }
}

#[derive(Clone, Debug)]
pub struct SourceProvenance {
    baseline_sha: Option<String>,
    baseline_sha_reason: Option<String>,
    source_dirty: Option<bool>,
    source_dirty_reason: Option<String>,
    build_profile: Option<String>,
    build_profile_reason: Option<String>,
}

impl SourceProvenance {
    pub fn new(
        baseline_sha: Option<String>,
        source_dirty: Option<bool>,
        build_profile: Option<String>,
    ) -> Result<Self, RecorderError> {
        if baseline_sha
            .as_deref()
            .is_some_and(|sha| !valid_sha256_or_git_sha(sha))
        {
            return Err(RecorderError::invalid(
                "baseline SHA must contain 40 or 64 hexadecimal characters",
            ));
        }
        if build_profile
            .as_deref()
            .is_some_and(|profile| !valid_profile_name(profile))
        {
            return Err(RecorderError::invalid("build profile name is invalid"));
        }
        Ok(Self {
            baseline_sha_reason: baseline_sha
                .is_none()
                .then(|| "source revision was not supplied; recorder does not run Git".into()),
            source_dirty_reason: source_dirty
                .is_none()
                .then(|| "dirty state was not supplied; recorder does not run Git".into()),
            build_profile_reason: build_profile
                .is_none()
                .then(|| "Cargo profile name was not supplied by the caller or environment".into()),
            baseline_sha,
            source_dirty,
            build_profile,
        })
    }

    /// Read explicitly supplied values. Invalid or absent values remain null
    /// with a reason; this method never guesses or launches a subprocess.
    pub fn from_environment() -> Self {
        let sha = std::env::var("PERF_HARNESS_BASELINE_SHA").ok();
        let dirty = std::env::var("PERF_HARNESS_SOURCE_DIRTY").ok();
        let profile = std::env::var("PERF_HARNESS_PROFILE").ok();

        let mut result = Self::new(None, None, None).expect("empty provenance is valid");
        match sha {
            Some(value) if valid_sha256_or_git_sha(&value) => {
                result.baseline_sha = Some(value.to_ascii_lowercase());
                result.baseline_sha_reason = None;
            }
            Some(_) => {
                result.baseline_sha_reason = Some(
                    "PERF_HARNESS_BASELINE_SHA was present but was not a 40/64-digit hexadecimal SHA".into(),
                );
            }
            None => {}
        }
        match dirty.as_deref() {
            Some("true") => {
                result.source_dirty = Some(true);
                result.source_dirty_reason = None;
            }
            Some("false") => {
                result.source_dirty = Some(false);
                result.source_dirty_reason = None;
            }
            Some(_) => {
                result.source_dirty_reason =
                    Some("PERF_HARNESS_SOURCE_DIRTY must be exactly true or false".into());
            }
            None => {}
        }
        match profile {
            Some(value) if valid_profile_name(&value) => {
                result.build_profile = Some(value);
                result.build_profile_reason = None;
            }
            Some(_) => {
                result.build_profile_reason =
                    Some("PERF_HARNESS_PROFILE contained an invalid profile name".into());
            }
            None => {}
        }
        result
    }
}

fn valid_sha256_or_git_sha(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_profile_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

pub fn fixture_sha256(fixture: &[u8]) -> String {
    hex::encode(Sha256::digest(fixture))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessContainment {
    NotRequired,
    NotAttempted,
    JobObjectVerified,
    BlockedEnvironment,
    BlockedDesktop,
    Unsupported,
    Failed,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProcessManifest {
    pub executable_basename: Option<String>,
    pub executable_sha256: Option<String>,
    /// Only normalized, redacted arguments belong here; never store free-form
    /// user input, a command line, or a personal path.
    pub argv_redacted: Vec<String>,
    pub child_pid: Option<u32>,
    /// Creation FILETIME read from the owned process handle. This disambiguates
    /// a PID from a later process that may reuse the same numeric identifier.
    pub child_creation_time_100ns: Option<u64>,
    pub child_creation_time_reason: Option<String>,
    /// CPU time for the root child only, read from its owned process handle.
    pub root_process_cpu_ms: Option<f64>,
    pub root_process_cpu_reason: Option<String>,
    pub job_total_processes: Option<u32>,
    pub job_active_processes_at_root_exit: Option<u32>,
    pub job_created: Option<bool>,
    pub job_assigned: Option<bool>,
    pub resumed: Option<bool>,
    pub exit_code: Option<i32>,
    pub deadline_reached: Option<bool>,
    pub cleanup_verified: Option<bool>,
    pub cleanup_reason: Option<String>,
    pub process_containment: ProcessContainment,
    pub process_containment_reason: String,
}

impl ProcessManifest {
    pub fn not_attempted(reason: impl Into<String>) -> Self {
        let reason = safe_reason(reason.into());
        Self {
            executable_basename: None,
            executable_sha256: None,
            argv_redacted: Vec::new(),
            child_pid: None,
            child_creation_time_100ns: None,
            child_creation_time_reason: Some("no child process was created".into()),
            root_process_cpu_ms: None,
            root_process_cpu_reason: Some("no owned process handle was available".into()),
            job_total_processes: None,
            job_active_processes_at_root_exit: None,
            job_created: None,
            job_assigned: None,
            resumed: None,
            exit_code: None,
            deadline_reached: None,
            cleanup_verified: None,
            cleanup_reason: Some("no child process was created".into()),
            process_containment: ProcessContainment::NotAttempted,
            process_containment_reason: reason,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ProbeFailure {
    pub status: ProbeStatus,
    pub reason: String,
}

impl ProbeFailure {
    pub fn new(status: ProbeStatus, reason: impl Into<String>) -> Self {
        let status = if status.is_success() {
            ProbeStatus::Error
        } else {
            status
        };
        Self {
            status,
            reason: safe_reason(reason.into()),
        }
    }

    pub fn timeout(reason: impl Into<String>) -> Self {
        Self::new(ProbeStatus::Timeout, reason)
    }

    pub fn error(reason: impl Into<String>) -> Self {
        Self::new(ProbeStatus::Error, reason)
    }
}

impl From<RecorderError> for ProbeFailure {
    fn from(error: RecorderError) -> Self {
        Self::error(error.to_string())
    }
}

#[derive(Debug)]
pub enum RecorderError {
    Io(io::Error),
    Serialization(serde_json::Error),
    Invalid(String),
    UnsafeOutput(String),
    AggregateLimit(String),
    AlreadyFinished,
    TooManySamples,
}

impl RecorderError {
    fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }
}

impl fmt::Display for RecorderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "I/O error: {error}"),
            Self::Serialization(error) => write!(f, "JSON serialization error: {error}"),
            Self::Invalid(message) => write!(f, "invalid run data: {message}"),
            Self::UnsafeOutput(message) => write!(f, "unsafe output path: {message}"),
            Self::AggregateLimit(message) => write!(f, "evidence limit reached: {message}"),
            Self::AlreadyFinished => f.write_str("run was already finalized"),
            Self::TooManySamples => f.write_str("recorded more samples than requested"),
        }
    }
}

impl std::error::Error for RecorderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Serialization(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for RecorderError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for RecorderError {
    fn from(error: serde_json::Error) -> Self {
        Self::Serialization(error)
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct MetricSummary {
    pub unit: String,
    pub p50: Option<f64>,
    pub p95: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub valid_sample_count: usize,
    pub invalid_sample_count: usize,
    pub null_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct RunSummary {
    pub schema_version: u32,
    pub run_id: String,
    pub scenario: Scenario,
    pub measurement_kind: MeasurementKind,
    pub status: ProbeStatus,
    pub workload_status: ProbeStatus,
    pub termination_reason: Option<String>,
    pub requested_workload: RequestedWorkload,
    pub valid_sample_count: usize,
    pub invalid_sample_count: usize,
    pub metrics: BTreeMap<String, MetricSummary>,
    pub baseline_sha: Option<String>,
    pub baseline_sha_reason: Option<String>,
    pub source_dirty: Option<bool>,
    pub source_dirty_reason: Option<String>,
    pub build_profile: Option<String>,
    pub build_profile_reason: Option<String>,
    pub debug_assertions: bool,
    pub fixture_sha256: Option<String>,
    pub fixture_hash_reason: Option<String>,
    pub os: &'static str,
    pub architecture: &'static str,
    pub started_at_utc: String,
    pub stopped_at_utc: Option<String>,
    pub elapsed_ns: Option<u64>,
    pub logs_truncated: bool,
    pub process_containment: ProcessContainment,
    pub process_containment_reason: String,
    pub screen_visibility_kind: &'static str,
    #[serde(skip)]
    pub output_dir: PathBuf,
}

#[derive(Clone, Debug, Serialize)]
pub struct RequestedWorkload {
    pub warmups: usize,
    pub samples: usize,
    pub timeout_seconds: u64,
}

#[derive(Serialize)]
struct EventRecord<'a> {
    schema_version: u32,
    event: &'static str,
    scenario: Scenario,
    measurement_kind: MeasurementKind,
    timestamp_utc: String,
    elapsed_ns: u64,
    sample_index: Option<usize>,
    duration_ns: Option<u64>,
    status: Option<ProbeStatus>,
    requested_workload: &'a RequestedWorkload,
    metrics: Option<&'a BTreeMap<String, MetricValue>>,
    reason: Option<String>,
}

#[derive(Default)]
struct MetricAccumulator {
    unit: String,
    values: Vec<f64>,
    invalid_samples: usize,
    first_missing_reason: Option<String>,
}

impl MetricAccumulator {
    fn push(&mut self, metric: &MetricValue) -> Result<(), RecorderError> {
        metric.validate()?;
        if self.unit.is_empty() {
            self.unit.clone_from(&metric.unit);
        } else if self.unit != metric.unit {
            return Err(RecorderError::invalid(
                "metric unit changed between samples",
            ));
        }
        match metric.value {
            Some(value) => self.values.push(value),
            None => {
                self.invalid_samples += 1;
                if self.first_missing_reason.is_none() {
                    self.first_missing_reason = metric.reason.clone();
                }
            }
        }
        Ok(())
    }

    fn summary(&self) -> MetricSummary {
        let mut values = self.values.clone();
        values.sort_by(f64::total_cmp);
        let (min, max, p50, p95) = if values.is_empty() {
            (None, None, None, None)
        } else {
            (
                values.first().copied(),
                values.last().copied(),
                Some(quantile(&values, 0.50)),
                Some(quantile(&values, 0.95)),
            )
        };
        let null_reason = if self.invalid_samples == 0 {
            None
        } else {
            self.first_missing_reason
                .clone()
                .or_else(|| Some("metric was missing for one or more samples".into()))
        };
        MetricSummary {
            unit: self.unit.clone(),
            p50,
            p95,
            min,
            max,
            valid_sample_count: self.values.len(),
            invalid_sample_count: self.invalid_samples,
            null_reason,
        }
    }
}

fn quantile(sorted: &[f64], percentile: f64) -> f64 {
    if sorted.len() == 1 {
        return sorted[0];
    }
    let position = percentile * (sorted.len() - 1) as f64;
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;
    if lower == upper {
        sorted[lower]
    } else {
        let fraction = position - lower as f64;
        sorted[lower] + (sorted[upper] - sorted[lower]) * fraction
    }
}

#[derive(Clone, Copy)]
struct RecorderLimits {
    aggregate_bytes: u64,
    reserve_bytes: u64,
    run_bytes: u64,
    log_bytes: u64,
}

impl Default for RecorderLimits {
    fn default() -> Self {
        Self {
            aggregate_bytes: MAX_AGGREGATE_BYTES,
            reserve_bytes: RUN_RESERVATION_BYTES,
            run_bytes: RUN_RESERVATION_BYTES,
            log_bytes: MAX_RUN_LOG_BYTES,
        }
    }
}

struct AggregateLock {
    path: PathBuf,
    file: Option<File>,
}

impl AggregateLock {
    fn acquire(root: &Path) -> Result<Self, RecorderError> {
        let path = root.join(".aggregate.lock");
        let mut file = None;
        for _ in 0..100 {
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(created) => {
                    file = Some(created);
                    break;
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    std::thread::sleep(Duration::from_millis(2));
                }
                Err(error) => return Err(RecorderError::Io(error)),
            }
        }
        let mut file = file.ok_or_else(|| {
            RecorderError::UnsafeOutput(
                "evidence allocation lock is busy or was left by an interrupted allocator".into(),
            )
        })?;
        writeln!(file, "{}", std::process::id())?;
        file.sync_all()?;
        Ok(Self {
            path,
            file: Some(file),
        })
    }
}

struct RunReservation {
    path: PathBuf,
    evidence_root: PathBuf,
    active: bool,
}

impl RunReservation {
    fn create(run_dir: &Path, evidence_root: &Path, bytes: u64) -> Result<Self, RecorderError> {
        let path = run_dir.join(".reservation");
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        writeln!(file, "{bytes}")?;
        file.sync_all()?;
        Ok(Self {
            path,
            evidence_root: evidence_root.to_path_buf(),
            active: true,
        })
    }

    fn release(&mut self) -> Result<(), RecorderError> {
        if !self.active {
            return Ok(());
        }
        let _lock = AggregateLock::acquire(&self.evidence_root)?;
        let aggregate = scan_regular_tree(&self.evidence_root)?;
        if aggregate > MAX_AGGREGATE_BYTES {
            return Err(RecorderError::AggregateLimit(
                "final evidence exceeds the aggregate limit; reservation was retained".into(),
            ));
        }
        fs::remove_file(&self.path)?;
        self.active = false;
        Ok(())
    }
}

impl Drop for RunReservation {
    fn drop(&mut self) {
        if self.active
            && let Ok(_lock) = AggregateLock::acquire(&self.evidence_root)
        {
            if fs::remove_file(&self.path).is_ok() {
                self.active = false;
            }
        }
    }
}

impl Drop for AggregateLock {
    fn drop(&mut self) {
        self.file.take();
        let _ = fs::remove_file(&self.path);
    }
}

/// Shared recorder. Its lifecycle wrapper catches probe errors and panics,
/// emits a terminal event, flushes the event stream and atomically publishes
/// `summary.json` plus `run.json` on a best-effort basis.
pub struct RunRecorder {
    config: ProbeConfig,
    measurement_kind: MeasurementKind,
    requested: RequestedWorkload,
    output_dir: PathBuf,
    run_id: String,
    started: Instant,
    started_at_utc: String,
    events: BufWriter<File>,
    stdout: File,
    stderr: File,
    base_bytes: u64,
    run_bytes: u64,
    log_bytes: u64,
    limits: RecorderLimits,
    metrics: BTreeMap<String, MetricAccumulator>,
    valid_samples: usize,
    invalid_samples: usize,
    logs_truncated: bool,
    manifest: ProcessManifest,
    provenance: SourceProvenance,
    fixture_sha256: Option<String>,
    fixture_hash_reason: Option<String>,
    process_containment: ProcessContainment,
    process_containment_reason: String,
    terminal_written: bool,
    finished: bool,
    reservation: RunReservation,
}

impl RunRecorder {
    fn start(
        workspace_root: &Path,
        config: ProbeConfig,
        measurement_kind: MeasurementKind,
        provenance: SourceProvenance,
        fixture: Option<&[u8]>,
        manifest: ProcessManifest,
        limits: RecorderLimits,
    ) -> Result<Self, RecorderError> {
        config
            .validate()
            .map_err(|error| RecorderError::invalid(error.to_string()))?;
        let workspace_root = workspace_root.canonicalize().map_err(RecorderError::Io)?;
        if !cfg!(test) {
            let repository_root = Path::new(env!("CARGO_MANIFEST_DIR"))
                .canonicalize()
                .map_err(RecorderError::Io)?;
            if workspace_root != repository_root {
                return Err(RecorderError::UnsafeOutput(
                    "workspace root must be this repository".into(),
                ));
            }
        }
        let evidence_root = ensure_evidence_root(&workspace_root)?;
        let (base_bytes, output_dir, run_id, reservation) = {
            let _lock = AggregateLock::acquire(&evidence_root)?;
            let base_bytes = scan_regular_tree(&evidence_root)?;
            let reserved_total = base_bytes
                .checked_add(limits.reserve_bytes)
                .ok_or_else(|| RecorderError::AggregateLimit("byte count overflow".into()))?;
            if reserved_total > limits.aggregate_bytes {
                return Err(RecorderError::AggregateLimit(format!(
                    "existing evidence ({base_bytes} bytes) plus a reserved run ({}) would exceed {} bytes",
                    limits.reserve_bytes, limits.aggregate_bytes
                )));
            }
            let scenario_dir = evidence_root.join(config.scenario.as_str());
            ensure_directory(&scenario_dir)?;
            let (output_dir, run_id) = create_unique_run_dir(&scenario_dir)?;
            let reservation =
                RunReservation::create(&output_dir, &evidence_root, limits.reserve_bytes)?;
            (base_bytes, output_dir, run_id, reservation)
        };
        let events_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output_dir.join("events.jsonl"))?;
        let stdout = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output_dir.join("stdout.log"))?;
        let stderr = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output_dir.join("stderr.log"))?;
        let started_at_utc = utc_now();
        let requested = RequestedWorkload {
            warmups: config.warmups,
            samples: config.samples,
            timeout_seconds: config.timeout_seconds,
        };
        let mut recorder = Self {
            config,
            measurement_kind,
            requested,
            output_dir,
            run_id,
            started: Instant::now(),
            started_at_utc,
            events: BufWriter::new(events_file),
            stdout,
            stderr,
            base_bytes,
            run_bytes: 0,
            log_bytes: 0,
            limits,
            metrics: BTreeMap::new(),
            valid_samples: 0,
            invalid_samples: 0,
            logs_truncated: false,
            manifest,
            provenance,
            fixture_sha256: fixture.map(fixture_sha256),
            fixture_hash_reason: fixture
                .is_none()
                .then(|| "no deterministic fixture bytes were supplied".into()),
            process_containment: ProcessContainment::NotAttempted,
            process_containment_reason: "no process ownership operation was recorded".into(),
            terminal_written: false,
            finished: false,
            reservation,
        };
        recorder.process_containment = recorder.manifest.process_containment;
        recorder.process_containment_reason = recorder.manifest.process_containment_reason.clone();
        recorder.write_event("started", None, None, None, None, None)?;
        Ok(recorder)
    }

    pub fn output_dir(&self) -> &Path {
        &self.output_dir
    }

    pub fn deadline_reached(&self) -> bool {
        self.started.elapsed() >= Duration::from_secs(self.config.timeout_seconds)
    }

    pub fn remaining_time(&self) -> Duration {
        Duration::from_secs(self.config.timeout_seconds).saturating_sub(self.started.elapsed())
    }

    pub fn set_process_manifest(&mut self, manifest: ProcessManifest) {
        self.process_containment = manifest.process_containment;
        self.process_containment_reason = safe_reason(manifest.process_containment_reason.clone());
        self.manifest = manifest;
    }

    pub fn record_sample(&mut self, sample: SampleRecord) -> Result<(), RecorderError> {
        if self.valid_samples + self.invalid_samples >= self.config.samples {
            return Err(RecorderError::TooManySamples);
        }
        if self.deadline_reached() {
            return Err(RecorderError::invalid(
                "run deadline expired before sample recording",
            ));
        }
        if sample.metrics.len() > MAX_METRICS {
            return Err(RecorderError::invalid("sample contains too many metrics"));
        }
        let mut metrics = sample.metrics;
        let wall = MetricValue::measured(sample.duration.as_secs_f64() * 1_000.0, "ms")?;
        metrics.entry("wall_elapsed_ms".into()).or_insert(wall);
        metrics.entry("process_cpu_ms".into()).or_insert_with(|| {
            MetricValue::missing(
                "ms",
                "per-process CPU requires an owned process handle; none was provided",
            )
        });
        metrics
            .entry("native_present_ms".into())
            .or_insert_with(|| {
                MetricValue::missing(
                    "ms",
                    "no owned native presentation acknowledgement was provided",
                )
            });
        validate_metrics(&metrics)?;
        let index = self.valid_samples + self.invalid_samples + 1;
        self.push_metrics(&metrics)?;
        self.valid_samples += 1;
        self.write_event(
            "sample",
            Some(index),
            Some(sample.duration),
            Some(ProbeStatus::Success),
            Some(&metrics),
            None,
        )
    }

    pub fn append_log(
        &mut self,
        stream: LogStream,
        bytes: &[u8],
    ) -> Result<LogWrite, RecorderError> {
        let run_remaining = self.limits.log_bytes.saturating_sub(self.log_bytes);
        let aggregate_used = self.base_bytes.saturating_add(self.run_bytes);
        let aggregate_remaining = self.limits.aggregate_bytes.saturating_sub(aggregate_used);
        let artifact_remaining = self.limits.run_bytes.saturating_sub(self.run_bytes);
        let allowed = usize::try_from(
            run_remaining
                .min(aggregate_remaining)
                .min(artifact_remaining),
        )
        .unwrap_or(usize::MAX);
        let write_len = bytes.len().min(allowed);
        if write_len > 0 {
            let file = match stream {
                LogStream::Stdout => &mut self.stdout,
                LogStream::Stderr => &mut self.stderr,
            };
            file.write_all(&bytes[..write_len])?;
            self.log_bytes += write_len as u64;
            self.run_bytes += write_len as u64;
        }
        let truncated = write_len < bytes.len();
        if truncated {
            self.logs_truncated = true;
        }
        Ok(LogWrite {
            written_bytes: write_len,
            truncated,
        })
    }

    fn push_metrics(
        &mut self,
        metrics: &BTreeMap<String, MetricValue>,
    ) -> Result<(), RecorderError> {
        if !self.metrics.is_empty()
            && (self.metrics.len() != metrics.len() || self.metrics.keys().ne(metrics.keys()))
        {
            return Err(RecorderError::invalid("metric set changed between samples"));
        }
        for (name, metric) in metrics {
            metric.validate()?;
            if self.metrics.get(name).is_some_and(|accumulator| {
                !accumulator.unit.is_empty() && accumulator.unit != metric.unit
            }) {
                return Err(RecorderError::invalid(
                    "metric unit changed between samples",
                ));
            }
        }
        for (name, metric) in metrics {
            let accumulator = self.metrics.entry(name.clone()).or_default();
            accumulator.push(metric)?;
        }
        Ok(())
    }

    fn record_failure_sample(
        &mut self,
        status: ProbeStatus,
        reason: &str,
    ) -> Result<(), RecorderError> {
        if self.valid_samples + self.invalid_samples >= self.config.samples {
            return Ok(());
        }
        let mut metrics = BTreeMap::new();
        metrics.insert(
            "wall_elapsed_ms".into(),
            MetricValue::missing("ms", "sample did not complete"),
        );
        metrics.insert(
            "process_cpu_ms".into(),
            MetricValue::missing(
                "ms",
                "sample did not complete; process CPU was not available",
            ),
        );
        metrics.insert(
            "native_present_ms".into(),
            MetricValue::missing(
                "ms",
                "sample did not complete; native presentation was not available",
            ),
        );
        for (name, accumulator) in &self.metrics {
            metrics.entry(name.clone()).or_insert_with(|| {
                MetricValue::missing(
                    accumulator.unit.clone(),
                    "sample failed before this metric was measured",
                )
            });
        }
        let index = self.valid_samples + self.invalid_samples + 1;
        self.push_metrics(&metrics)?;
        self.invalid_samples += 1;
        self.write_event(
            "sample",
            Some(index),
            None,
            Some(status),
            Some(&metrics),
            Some(safe_reason(reason.into())),
        )
    }

    fn write_event(
        &mut self,
        event: &'static str,
        sample_index: Option<usize>,
        duration: Option<Duration>,
        status: Option<ProbeStatus>,
        metrics: Option<&BTreeMap<String, MetricValue>>,
        reason: Option<String>,
    ) -> Result<(), RecorderError> {
        let record = EventRecord {
            schema_version: SCHEMA_VERSION,
            event,
            scenario: self.config.scenario,
            measurement_kind: self.measurement_kind,
            timestamp_utc: utc_now(),
            elapsed_ns: duration_ns(self.started.elapsed()),
            sample_index,
            duration_ns: duration.map(duration_ns),
            status,
            requested_workload: &self.requested,
            metrics,
            reason: reason.map(safe_reason),
        };
        let mut encoded = serde_json::to_vec(&record)?;
        if encoded.len() + 1 > MAX_EVENT_BYTES {
            return Err(RecorderError::AggregateLimit(
                "event exceeded its size limit".into(),
            ));
        }
        encoded.push(b'\n');
        self.ensure_write_capacity(encoded.len() as u64)?;
        self.events.write_all(&encoded)?;
        self.run_bytes += encoded.len() as u64;
        self.events.flush()?;
        Ok(())
    }

    fn ensure_write_capacity(&self, additional: u64) -> Result<(), RecorderError> {
        let next_run = self
            .run_bytes
            .checked_add(additional)
            .ok_or_else(|| RecorderError::AggregateLimit("run byte count overflow".into()))?;
        let next_aggregate = self
            .base_bytes
            .checked_add(next_run)
            .ok_or_else(|| RecorderError::AggregateLimit("aggregate byte count overflow".into()))?;
        if next_run > self.limits.run_bytes || next_aggregate > self.limits.aggregate_bytes {
            return Err(RecorderError::AggregateLimit(
                "writing the next event would exceed the run or aggregate byte cap".into(),
            ));
        }
        Ok(())
    }

    fn finish(
        &mut self,
        workload_status: ProbeStatus,
        reason: Option<String>,
    ) -> Result<RunSummary, RecorderError> {
        if self.finished {
            return Err(RecorderError::AlreadyFinished);
        }
        let status = if self.logs_truncated {
            ProbeStatus::Truncated
        } else {
            workload_status
        };
        let termination_reason = match (self.logs_truncated, reason) {
            (true, Some(reason)) => Some(format!(
                "logs truncated; workload status {workload_status}: {}",
                safe_reason(reason)
            )),
            (true, None) => Some(format!(
                "logs truncated at the configured byte cap; workload status {workload_status}"
            )),
            (false, reason) => reason.map(safe_reason),
        };
        if !self.terminal_written {
            self.write_event(
                "terminal",
                None,
                None,
                Some(status),
                None,
                termination_reason.clone(),
            )?;
            self.terminal_written = true;
        }
        self.events.flush()?;
        self.events.get_ref().sync_all()?;
        self.stdout.flush()?;
        self.stdout.sync_all()?;
        self.stderr.flush()?;
        self.stderr.sync_all()?;
        self.manifest.process_containment = self.process_containment;
        self.manifest.process_containment_reason =
            safe_reason(self.process_containment_reason.clone());
        let summary = self.build_summary(status, workload_status, termination_reason);
        let manifest = self.manifest.clone();
        self.write_atomic_json("run.json", &manifest, MAX_EVENT_BYTES)?;
        self.write_atomic_json("summary.json", &summary, MAX_SUMMARY_BYTES)?;
        self.reservation.release()?;
        self.finished = true;
        Ok(summary)
    }

    fn build_summary(
        &self,
        status: ProbeStatus,
        workload_status: ProbeStatus,
        termination_reason: Option<String>,
    ) -> RunSummary {
        let metrics = self
            .metrics
            .iter()
            .map(|(name, values)| (name.clone(), values.summary()))
            .collect();
        let now = self.started.elapsed();
        RunSummary {
            schema_version: SCHEMA_VERSION,
            run_id: self.run_id.clone(),
            scenario: self.config.scenario,
            measurement_kind: self.measurement_kind,
            status,
            workload_status,
            termination_reason,
            requested_workload: self.requested.clone(),
            valid_sample_count: self.valid_samples,
            invalid_sample_count: self.invalid_samples,
            metrics,
            baseline_sha: self.provenance.baseline_sha.clone(),
            baseline_sha_reason: self.provenance.baseline_sha_reason.clone(),
            source_dirty: self.provenance.source_dirty,
            source_dirty_reason: self.provenance.source_dirty_reason.clone(),
            build_profile: self.provenance.build_profile.clone(),
            build_profile_reason: self.provenance.build_profile_reason.clone(),
            debug_assertions: cfg!(debug_assertions),
            fixture_sha256: self.fixture_sha256.clone(),
            fixture_hash_reason: self.fixture_hash_reason.clone(),
            os: std::env::consts::OS,
            architecture: std::env::consts::ARCH,
            started_at_utc: self.started_at_utc.clone(),
            stopped_at_utc: Some(utc_now()),
            elapsed_ns: Some(duration_ns(now)),
            logs_truncated: self.logs_truncated,
            process_containment: self.process_containment,
            process_containment_reason: safe_reason(self.process_containment_reason.clone()),
            screen_visibility_kind: self.measurement_kind.screen_visibility_kind(),
            output_dir: self.output_dir.clone(),
        }
    }

    fn write_atomic_json<T: Serialize>(
        &mut self,
        filename: &str,
        value: &T,
        max_bytes: usize,
    ) -> Result<(), RecorderError> {
        let mut encoded = serde_json::to_vec_pretty(value)?;
        encoded.push(b'\n');
        if encoded.len() > max_bytes {
            return Err(RecorderError::AggregateLimit(format!(
                "{filename} exceeded its size limit"
            )));
        }
        self.ensure_write_capacity(encoded.len() as u64)?;
        let temp_path = self.output_dir.join(format!("{filename}.tmp"));
        let final_path = self.output_dir.join(filename);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)?;
        file.write_all(&encoded)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp_path, &final_path)?;
        self.run_bytes += encoded.len() as u64;
        Ok(())
    }

    fn best_effort_finish(&mut self, status: ProbeStatus, reason: &str) {
        if self.finished {
            return;
        }
        let _ = self.finish(status, Some(safe_reason(reason.into())));
    }
}

impl Drop for RunRecorder {
    fn drop(&mut self) {
        if !self.finished {
            let status = if self.logs_truncated {
                ProbeStatus::Truncated
            } else {
                ProbeStatus::Error
            };
            self.best_effort_finish(status, "run exited without a terminal result");
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogStream {
    Stdout,
    Stderr,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LogWrite {
    pub written_bytes: usize,
    pub truncated: bool,
}

/// Run a probe operation inside the recorder lifecycle. Errors and panics are
/// converted into terminal receipts; a wall-time overrun is classified as a
/// timeout after the callback returns. Callbacks can cooperatively inspect
/// `deadline_reached` between bounded units of work.
pub fn run_recorded<F>(
    workspace_root: &Path,
    config: ProbeConfig,
    measurement_kind: MeasurementKind,
    provenance: SourceProvenance,
    fixture: Option<&[u8]>,
    manifest: ProcessManifest,
    operation: F,
) -> Result<RunSummary, RecorderError>
where
    F: FnOnce(&mut RunRecorder) -> Result<(), ProbeFailure>,
{
    run_recorded_with_limits(
        workspace_root,
        config,
        measurement_kind,
        provenance,
        fixture,
        manifest,
        operation,
        RecorderLimits::default(),
    )
}

fn run_recorded_with_limits<F>(
    workspace_root: &Path,
    config: ProbeConfig,
    measurement_kind: MeasurementKind,
    provenance: SourceProvenance,
    fixture: Option<&[u8]>,
    manifest: ProcessManifest,
    operation: F,
    limits: RecorderLimits,
) -> Result<RunSummary, RecorderError>
where
    F: FnOnce(&mut RunRecorder) -> Result<(), ProbeFailure>,
{
    let mut recorder = RunRecorder::start(
        workspace_root,
        config,
        measurement_kind,
        provenance,
        fixture,
        manifest,
        limits,
    )?;
    let result =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| operation(&mut recorder)));
    let callback_succeeded = matches!(&result, Ok(Ok(())));
    let (mut workload_status, mut reason) = match result {
        Ok(Ok(())) => {
            if recorder.deadline_reached() {
                (
                    ProbeStatus::Timeout,
                    Some("run exceeded its requested wall-time deadline".into()),
                )
            } else if recorder.valid_samples != recorder.config.samples {
                (
                    ProbeStatus::Error,
                    Some(format!(
                        "completed {} of {} requested samples",
                        recorder.valid_samples, recorder.config.samples
                    )),
                )
            } else {
                (ProbeStatus::Success, None)
            }
        }
        Ok(Err(failure)) => (failure.status, Some(failure.reason)),
        Err(_) => (ProbeStatus::Error, Some("probe panicked".into())),
    };

    if callback_succeeded && recorder.deadline_reached() {
        workload_status = ProbeStatus::Timeout;
        reason = Some("run exceeded its requested wall-time deadline".into());
    }
    if !workload_status.is_success() {
        let sample_status = workload_status;
        let sample_reason = reason
            .clone()
            .unwrap_or_else(|| "probe ended without a success result".into());
        if let Err(error) = recorder.record_failure_sample(sample_status, &sample_reason) {
            reason = Some(format!(
                "{}; failure sample could not be recorded: {error}",
                sample_reason
            ));
        }
    }
    let summary_status = if recorder.logs_truncated {
        ProbeStatus::Truncated
    } else {
        workload_status
    };
    let summary = recorder.finish(workload_status, reason)?;
    debug_assert_eq!(summary.status, summary_status);
    Ok(summary)
}

fn ensure_evidence_root(workspace: &Path) -> Result<PathBuf, RecorderError> {
    let target = workspace.join("target");
    ensure_directory(&target)?;
    let evidence = target.join("perf-harness");
    ensure_directory(&evidence)?;
    Ok(evidence)
}

fn ensure_directory(path: &Path) -> Result<(), RecorderError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if is_symlink_or_reparse(&metadata) || !metadata.is_dir() {
                return Err(RecorderError::UnsafeOutput(format!(
                    "{} is not a regular directory",
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("output")
                )));
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir(path)?;
            let metadata = fs::symlink_metadata(path)?;
            if is_symlink_or_reparse(&metadata) || !metadata.is_dir() {
                return Err(RecorderError::UnsafeOutput(
                    "created output component was replaced or is not a directory".into(),
                ));
            }
        }
        Err(error) => return Err(RecorderError::Io(error)),
    }
    Ok(())
}

#[cfg(windows)]
fn is_symlink_or_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.file_type().is_symlink()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_symlink_or_reparse(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

fn create_unique_run_dir(scenario_dir: &Path) -> Result<(PathBuf, String), RecorderError> {
    for attempt in 0..16 {
        let epoch_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let run_id = format!("run-{epoch_ns}-{attempt}");
        let path = scenario_dir.join(&run_id);
        match fs::create_dir(&path) {
            Ok(()) => {
                ensure_directory(&path)?;
                return Ok((path, run_id));
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(RecorderError::Io(error)),
        }
    }
    Err(RecorderError::UnsafeOutput(
        "could not allocate a unique run directory".into(),
    ))
}

fn scan_regular_tree(root: &Path) -> Result<u64, RecorderError> {
    fn scan_directory(directory: &Path) -> Result<u64, RecorderError> {
        let mut total = 0u64;
        let mut reservation = None;
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            if name == ".aggregate.lock" {
                continue;
            }
            let metadata = fs::symlink_metadata(&path)?;
            if is_symlink_or_reparse(&metadata) {
                return Err(RecorderError::UnsafeOutput(
                    "evidence root contains a symlink or reparse point".into(),
                ));
            }
            if name == ".reservation" {
                if !metadata.is_file() || metadata.len() > 32 {
                    return Err(RecorderError::UnsafeOutput(
                        "run reservation marker is malformed".into(),
                    ));
                }
                let bytes = fs::read_to_string(&path)?
                    .trim()
                    .parse::<u64>()
                    .map_err(|_| {
                        RecorderError::UnsafeOutput("run reservation marker is invalid".into())
                    })?;
                reservation = Some(bytes);
            } else if metadata.is_dir() {
                total = total.checked_add(scan_directory(&path)?).ok_or_else(|| {
                    RecorderError::AggregateLimit("existing evidence size overflow".into())
                })?;
            } else if metadata.is_file() {
                total = total.checked_add(metadata.len()).ok_or_else(|| {
                    RecorderError::AggregateLimit("existing evidence size overflow".into())
                })?;
            } else {
                return Err(RecorderError::UnsafeOutput(
                    "evidence root contains a non-regular filesystem object".into(),
                ));
            }
        }
        Ok(reservation.map_or(total, |reserved| reserved.max(total)))
    }

    scan_directory(root)
}

fn validate_metrics(metrics: &BTreeMap<String, MetricValue>) -> Result<(), RecorderError> {
    if metrics.len() > MAX_METRICS {
        return Err(RecorderError::invalid("sample contains too many metrics"));
    }
    for (name, metric) in metrics {
        if name.is_empty()
            || name.len() > MAX_METRIC_NAME_BYTES
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        {
            return Err(RecorderError::invalid("metric name is empty or unsafe"));
        }
        metric.validate()?;
    }
    Ok(())
}

fn duration_ns(duration: Duration) -> u64 {
    duration.as_nanos().min(u64::MAX as u128) as u64
}

fn utc_now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Nanos, true)
}

fn bounded_text(value: String, limit: usize) -> String {
    let mut output = String::new();
    for character in value.chars().filter(|character| !character.is_control()) {
        if output.len() + character.len_utf8() > limit {
            break;
        }
        output.push(character);
    }
    output
}

fn safe_reason(value: String) -> String {
    let value = bounded_text(value, MAX_REASON_BYTES);
    if value.trim().is_empty() {
        "unspecified failure".into()
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn config(samples: usize) -> ProbeConfig {
        ProbeConfig {
            samples,
            ..ProbeConfig::new(Scenario::Radial8)
        }
    }

    fn provenance() -> SourceProvenance {
        SourceProvenance::new(Some("a".repeat(40)), Some(false), Some("fast-dev".into()))
            .expect("test provenance is valid")
    }

    fn add_sample(recorder: &mut RunRecorder, millis: u64) -> Result<(), ProbeFailure> {
        let metric = MetricValue::measured(millis as f64, "work_units")?;
        recorder
            .record_sample(
                SampleRecord::new(Duration::from_millis(millis)).with_metric("work", metric),
            )
            .map_err(ProbeFailure::from)
    }

    fn jsonl(path: &Path) -> Vec<serde_json::Value> {
        fs::read_to_string(path.join("events.jsonl"))
            .expect("events exist")
            .lines()
            .map(|line| serde_json::from_str(line).expect("event is JSON"))
            .collect()
    }

    #[test]
    fn successful_run_writes_ordered_events_summary_provenance_and_fixture_hash() {
        let workspace = tempfile::tempdir().expect("temporary workspace");
        let fixture = br#"{"synthetic":true,"cells":8}"#;
        let mut nested_dir = None;
        let summary = run_recorded(
            workspace.path(),
            config(2),
            MeasurementKind::CpuHeadless,
            provenance(),
            Some(fixture),
            ProcessManifest::not_attempted("CPU-only run; no child process needed"),
            |recorder| {
                let nested = run_recorded(
                    workspace.path(),
                    ProbeConfig {
                        scenario: Scenario::Radial32,
                        ..config(1)
                    },
                    MeasurementKind::CpuHeadless,
                    provenance(),
                    Some(b"nested fixture"),
                    ProcessManifest::not_attempted("nested CPU-only run"),
                    |child| add_sample(child, 3),
                )
                .map_err(ProbeFailure::from)?;
                assert_eq!(nested.status, ProbeStatus::Success);
                nested_dir = Some(nested.output_dir);
                add_sample(recorder, 10)?;
                let omitted = SampleRecord::new(Duration::from_millis(15));
                assert!(recorder.record_sample(omitted).is_err());
                let changed_unit = SampleRecord::new(Duration::from_millis(15))
                    .with_metric("work", MetricValue::measured(15.0, "other_units")?);
                assert!(recorder.record_sample(changed_unit).is_err());
                add_sample(recorder, 20)
            },
        )
        .expect("run is recorded");

        assert_eq!(summary.status, ProbeStatus::Success);
        assert_eq!(summary.valid_sample_count, 2);
        assert_eq!(summary.invalid_sample_count, 0);
        assert_eq!(
            summary.baseline_sha.as_deref(),
            Some("a".repeat(40).as_str())
        );
        assert_eq!(summary.source_dirty, Some(false));
        assert_eq!(summary.build_profile.as_deref(), Some("fast-dev"));
        assert_eq!(
            summary.fixture_sha256.as_deref(),
            Some(fixture_sha256(fixture).as_str())
        );
        let wall = &summary.metrics["wall_elapsed_ms"];
        assert_eq!(wall.p50, Some(15.0));
        assert_eq!(wall.p95, Some(19.5));
        assert_eq!(wall.valid_sample_count, 2);
        assert_eq!(wall.invalid_sample_count, 0);
        assert_eq!(summary.metrics["work"].valid_sample_count, 2);
        let cpu = &summary.metrics["process_cpu_ms"];
        assert_eq!(cpu.p50, None);
        assert!(
            cpu.null_reason
                .as_deref()
                .is_some_and(|reason| reason.contains("owned process handle"))
        );
        assert!(
            summary
                .output_dir
                .starts_with(workspace.path().canonicalize().unwrap().join(EVIDENCE_ROOT))
        );
        assert!(summary.output_dir.join("stdout.log").exists());
        assert!(summary.output_dir.join("stderr.log").exists());
        assert!(summary.output_dir.join("run.json").exists());
        assert!(nested_dir.is_some_and(|path| {
            path.starts_with(workspace.path().canonicalize().unwrap().join(EVIDENCE_ROOT))
        }));
        let events = jsonl(&summary.output_dir);
        assert_eq!(events.len(), 4);
        assert_eq!(events[0]["event"], "started");
        assert_eq!(events[1]["event"], "sample");
        assert_eq!(events[2]["event"], "sample");
        assert_eq!(events[3]["event"], "terminal");
        assert_eq!(events[3]["status"], "success");
        assert!(
            events
                .windows(2)
                .all(|pair| pair[1]["elapsed_ns"].as_u64() >= pair[0]["elapsed_ns"].as_u64())
        );
        let persisted: serde_json::Value = serde_json::from_slice(
            &fs::read(summary.output_dir.join("summary.json")).expect("summary bytes"),
        )
        .expect("summary is valid JSON");
        assert_eq!(persisted["scenario"], "radial_8");
    }

    #[test]
    fn error_timeout_and_panic_each_get_a_terminal_receipt() {
        let workspace = tempfile::tempdir().expect("temporary workspace");
        let timeout = run_recorded(
            workspace.path(),
            config(1),
            MeasurementKind::CpuHeadless,
            provenance(),
            None,
            ProcessManifest::not_attempted("no child"),
            |_| {
                Err(ProbeFailure::timeout(
                    "deadline reached by controlled fixture",
                ))
            },
        )
        .expect("timeout is recorded");
        assert_eq!(timeout.status, ProbeStatus::Timeout);
        assert_eq!(timeout.invalid_sample_count, 1);
        assert_eq!(
            jsonl(&timeout.output_dir).last().unwrap()["status"],
            "timeout"
        );
        let persisted_timeout: serde_json::Value = serde_json::from_slice(
            &fs::read(timeout.output_dir.join("summary.json")).expect("persisted timeout summary"),
        )
        .expect("timeout summary is valid JSON");
        assert_eq!(persisted_timeout["status"], timeout.status.as_str());
        assert_eq!(
            persisted_timeout["termination_reason"].as_str(),
            timeout.termination_reason.as_deref()
        );

        let error = run_recorded(
            workspace.path(),
            config(1),
            MeasurementKind::CpuHeadless,
            provenance(),
            None,
            ProcessManifest::not_attempted("no child"),
            |_| Err(ProbeFailure::error("controlled failure")),
        )
        .expect("error is recorded");
        assert_eq!(error.status, ProbeStatus::Error);
        assert_eq!(
            jsonl(&error.output_dir).last().unwrap()["event"],
            "terminal"
        );

        let mut late_config = config(1);
        late_config.timeout_seconds = 1;
        let late_failure = run_recorded(
            workspace.path(),
            late_config,
            MeasurementKind::CpuHeadless,
            provenance(),
            None,
            ProcessManifest::not_attempted("no child"),
            |recorder| {
                recorder.started = Instant::now() - Duration::from_secs(2);
                Err(ProbeFailure::new(
                    ProbeStatus::BlockedEnvironment,
                    "controlled access denied fixture",
                ))
            },
        )
        .expect("explicit native failure remains recorded");
        assert_eq!(late_failure.status, ProbeStatus::BlockedEnvironment);
        assert_eq!(
            late_failure.termination_reason.as_deref(),
            Some("controlled access denied fixture")
        );

        let panic = run_recorded(
            workspace.path(),
            config(1),
            MeasurementKind::CpuHeadless,
            provenance(),
            None,
            ProcessManifest::not_attempted("no child"),
            |_| -> Result<(), ProbeFailure> { panic!("controlled panic") },
        )
        .expect("panic is converted to an error receipt");
        assert_eq!(panic.status, ProbeStatus::Error);
        assert_eq!(panic.termination_reason.as_deref(), Some("probe panicked"));
        assert_eq!(
            jsonl(&panic.output_dir).last().unwrap()["event"],
            "terminal"
        );
    }

    #[test]
    fn cli_is_pure_and_rejects_ambiguous_or_unbounded_inputs() {
        let workspace = tempfile::tempdir().expect("temporary workspace");
        assert_eq!(parse_cli(["--help"]).unwrap(), CliAction::Help);
        assert!(parse_cli(["--help", "--scenario", "radial_8"]).is_err());
        assert!(parse_cli(["--scenario", "other"]).is_err());
        assert!(parse_cli(["--scenario", "radial_8", "--samples", "-1"]).is_err());
        assert!(parse_cli(["--scenario", "radial_8", "--samples", "21"]).is_err());
        assert!(parse_cli(["--scenario", "radial_8", "--timeout-seconds", "91"]).is_err());
        assert!(parse_cli(["--scenario", "radial_8", "--out", "C:\\Users\\someone"]).is_err());
        assert!(!workspace.path().join("target").exists());
    }

    #[test]
    fn log_and_aggregate_caps_are_enforced_before_artifacts_escape_limits() {
        let workspace = tempfile::tempdir().expect("temporary workspace");
        let limits = RecorderLimits {
            aggregate_bytes: 16 * 1024,
            reserve_bytes: 16 * 1024,
            run_bytes: 16 * 1024,
            log_bytes: 4,
        };
        let summary = run_recorded_with_limits(
            workspace.path(),
            config(1),
            MeasurementKind::CpuHeadless,
            provenance(),
            None,
            ProcessManifest::not_attempted("no child"),
            |recorder| {
                let result = recorder.append_log(LogStream::Stdout, b"abcdef")?;
                assert_eq!(result.written_bytes, 4);
                assert!(result.truncated);
                add_sample(recorder, 1)
            },
            limits,
        )
        .expect("truncated log is still reported");
        assert_eq!(summary.status, ProbeStatus::Truncated);
        assert_eq!(
            fs::metadata(summary.output_dir.join("stdout.log"))
                .unwrap()
                .len(),
            4
        );
        assert_eq!(summary.logs_truncated, true);

        let overfull_workspace = tempfile::tempdir().expect("separate aggregate fixture");
        let root = overfull_workspace.path().join(EVIDENCE_ROOT);
        let stale_dir = root.join("existing");
        fs::create_dir_all(&stale_dir).expect("existing evidence directory");
        fs::write(stale_dir.join("large.log"), b"123456789").expect("existing evidence");
        let too_tight = RecorderLimits {
            aggregate_bytes: 16,
            reserve_bytes: 8,
            run_bytes: 8,
            log_bytes: 4,
        };
        let error = run_recorded_with_limits(
            overfull_workspace.path(),
            config(1),
            MeasurementKind::CpuHeadless,
            provenance(),
            None,
            ProcessManifest::not_attempted("no child"),
            |_| Ok(()),
            too_tight,
        )
        .expect_err("aggregate reservation prevents an over-cap run");
        assert!(matches!(error, RecorderError::AggregateLimit(_)));
        assert!(!root.join("radial_8").exists());
    }
}
