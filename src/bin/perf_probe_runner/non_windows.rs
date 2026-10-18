use multi_launcher::perf_harness::{
    ProbeFailure, ProbeStatus, ProcessContainment, ProcessManifest, Scenario,
};
use std::path::Path;
use std::time::{Duration, Instant};

pub enum LaunchTarget {
    Probe(&'static str, Vec<String>),
    RunnerFixture(Vec<String>),
}

pub struct ChildOutcome {
    pub manifest: ProcessManifest,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub output_truncated: bool,
    pub elapsed: Duration,
    pub process_cpu_ms: Option<f64>,
    pub process_cpu_reason: Option<String>,
    pub failure: Option<ProbeFailure>,
}

pub fn run_owned(
    _workspace: &Path,
    _target: LaunchTarget,
    _argv_redacted: Vec<String>,
    _scenario: Scenario,
    _warmups: usize,
    _samples: usize,
    _timeout_seconds: u64,
) -> ChildOutcome {
    let started = Instant::now();
    let reason = "contained probe execution requires Windows Job Objects";
    let mut manifest = ProcessManifest::not_attempted(reason);
    manifest.process_containment = ProcessContainment::Unsupported;
    ChildOutcome {
        manifest,
        stdout: Vec::new(),
        stderr: Vec::new(),
        output_truncated: false,
        elapsed: started.elapsed(),
        process_cpu_ms: None,
        process_cpu_reason: Some(reason.into()),
        failure: Some(ProbeFailure::new(ProbeStatus::Unsupported, reason)),
    }
}
