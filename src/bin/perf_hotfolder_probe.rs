use multi_launcher::gui::HotFolderIdleFixture;
use multi_launcher::perf_harness::{
    CliAction, MeasurementKind, MetricValue, ProbeConfig, ProbeFailure, ProbeStatus,
    ProcessManifest, RunSummary, SampleRecord, Scenario, SourceProvenance,
    owned_process_cpu_ticks_100ns, parse_cli, run_recorded,
};
use std::process::ExitCode;
use std::thread;
use std::time::{Duration, Instant};

const WINDOW: Duration = Duration::from_secs(1);
const MISSING_DRIVER_METRIC: &str = "production driver counters are not instrumented";

fn main() -> ExitCode {
    match parse_cli(std::env::args_os().skip(1)) {
        Err(error) => {
            eprintln!("perf_hotfolder_probe: {error}");
            eprintln!("Run with --help for the supported scenarios and flags.");
            ExitCode::FAILURE
        }
        Ok(CliAction::Help) => {
            print_help();
            ExitCode::SUCCESS
        }
        Ok(CliAction::Run(config)) => {
            if !matches!(
                config.scenario,
                Scenario::HotfolderDisabled | Scenario::HotfolderEnabledZeroRules
            ) {
                eprintln!(
                    "perf_hotfolder_probe: unsupported scenario {}; allowed: hotfolder_disabled, hotfolder_enabled_zero_rules",
                    config.scenario
                );
                return ExitCode::FAILURE;
            }
            match run_probe(config) {
                Ok(summary) => {
                    println!(
                        "status={} scenario={} samples={}/{} evidence={}",
                        summary.status,
                        summary.scenario,
                        summary.valid_sample_count,
                        summary.requested_workload.samples,
                        summary.output_dir.display()
                    );
                    match summary.status {
                        ProbeStatus::Success => ExitCode::SUCCESS,
                        ProbeStatus::BlockedEnvironment | ProbeStatus::BlockedDesktop => {
                            if let Some(reason) = summary.termination_reason {
                                eprintln!("perf_hotfolder_probe: {reason}");
                            }
                            ExitCode::from(3)
                        }
                        _ => {
                            if let Some(reason) = summary.termination_reason {
                                eprintln!("perf_hotfolder_probe: {reason}");
                            }
                            ExitCode::FAILURE
                        }
                    }
                }
                Err(error) => {
                    eprintln!("perf_hotfolder_probe: could not record run: {error}");
                    ExitCode::FAILURE
                }
            }
        }
    }
}

fn run_probe(
    config: ProbeConfig,
) -> Result<RunSummary, multi_launcher::perf_harness::RecorderError> {
    let scenario = config.scenario;
    let warmups = config.warmups;
    let samples = config.samples;
    let fixture_bytes = format!(
        "hotfolder-idle-fixture-v1;scenario={};rules=0;roots=0",
        config.scenario.as_str()
    )
    .into_bytes();
    run_recorded(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")),
        config,
        MeasurementKind::CpuHeadless,
        SourceProvenance::from_environment(),
        Some(&fixture_bytes),
        ProcessManifest::not_attempted(
            "in-process Hot Folder CPU fixture; no child process needed",
        ),
        |recorder| {
            let mut fixture = HotFolderIdleFixture::new(recorder.output_dir(), scenario)?;
            fixture.validate()?;

            for window_index in 0..(warmups + samples) {
                if recorder.deadline_reached() {
                    return Err(ProbeFailure::timeout(
                        "run deadline expired before a one-second Hot Folder window",
                    ));
                }
                let is_sample = window_index >= warmups;
                let cpu_start = if is_sample {
                    owned_process_cpu_ticks_100ns()?
                } else {
                    None
                };
                let window_started = Instant::now();
                thread::sleep(WINDOW.min(recorder.remaining_time()));
                let elapsed = window_started.elapsed();
                if recorder.deadline_reached() || elapsed < WINDOW {
                    return Err(ProbeFailure::timeout(
                        "run deadline expired during a one-second Hot Folder window",
                    ));
                }
                let cpu_end = if is_sample {
                    owned_process_cpu_ticks_100ns()?
                } else {
                    None
                };
                fixture.validate()?;

                if is_sample {
                    let process_cpu = match (cpu_start, cpu_end) {
                        (Some(start), Some(end)) => {
                            let delta = end.checked_sub(start).ok_or_else(|| {
                                ProbeFailure::error(
                                    "current-process CPU counter decreased during a sample",
                                )
                            })?;
                            MetricValue::measured(delta as f64 / 10_000.0, "ms")?
                        }
                        _ => MetricValue::missing(
                            "ms",
                            "current-process CPU timing is unavailable on this platform",
                        ),
                    };
                    let sample = SampleRecord::new(elapsed)
                        .with_metric("process_cpu_ms", process_cpu)
                        .with_metric(
                            "driver_timeout_wakeups",
                            MetricValue::missing("count", MISSING_DRIVER_METRIC),
                        )
                        .with_metric(
                            "snapshot_publications",
                            MetricValue::missing("count", MISSING_DRIVER_METRIC),
                        )
                        .with_metric(
                            "work_iterations",
                            MetricValue::missing("count", MISSING_DRIVER_METRIC),
                        );
                    recorder.record_sample(sample).map_err(ProbeFailure::from)?;
                }
            }

            fixture.shutdown();
            Ok(())
        },
    )
}

fn print_help() {
    println!(
        "Usage: perf_hotfolder_probe --scenario <hotfolder_disabled|hotfolder_enabled_zero_rules> [--out target/perf-harness] [--warmups 3] [--samples 1..20] [--timeout-seconds 1..90]\n\nRuns a fresh isolated Hot Folder fixture with three one-second warmups and one to twenty one-second recorded windows (default 20). The CPU metric covers all threads in this probe process, including harness overhead."
    );
}
