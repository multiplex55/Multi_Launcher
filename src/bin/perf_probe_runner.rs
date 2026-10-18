use multi_launcher::perf_harness::{
    LogStream, MeasurementKind, MetricValue, ProbeConfig, ProbeStatus, ProcessManifest,
    SampleRecord, Scenario, SourceProvenance, run_recorded,
};
use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

#[cfg(windows)]
#[path = "perf_probe_runner/windows.rs"]
mod windows_runner;

#[cfg(not(windows))]
#[path = "perf_probe_runner/non_windows.rs"]
mod windows_runner;

const USAGE: &str = "Usage:\n  perf_probe_runner --probe <radial|hotfolder|dashboard> --scenario <fixed-name> [--out target/perf-harness] [--warmups 3] [--samples 1..20] [--timeout-seconds 1..90]\n  perf_probe_runner --self-test batch\n\nThe runner accepts only registered sibling probes and fixed scenarios. Self-test runs bounded, internal exit, nonzero, timeout, and descendant fixtures. All child processes are launched directly and assigned to a kill-on-close Windows Job Object before resume.\n";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Probe {
    Radial,
    Hotfolder,
    Dashboard,
}

#[derive(Clone, Debug)]
enum Action {
    Help,
    Probe {
        probe: Probe,
        scenario: Scenario,
        warmups: usize,
        samples: usize,
        timeout_seconds: u64,
    },
    SelfTest,
}

fn parse_args(arguments: &[String]) -> Result<Action, String> {
    if arguments.len() == 1 && (arguments[0] == "--help" || arguments[0] == "-h") {
        return Ok(Action::Help);
    }
    if arguments.len() == 2 && arguments[0] == "--self-test" && arguments[1] == "batch" {
        return Ok(Action::SelfTest);
    }

    let mut probe = None;
    let mut scenario = None;
    let mut warmups = 3usize;
    let mut samples = 20usize;
    let mut timeout_seconds = 90u64;
    let mut saw_out = false;
    let mut saw_warmups = false;
    let mut saw_samples = false;
    let mut saw_timeout = false;
    let mut index = 0;
    while index < arguments.len() {
        let flag = arguments[index].as_str();
        index += 1;
        let value = arguments
            .get(index)
            .ok_or_else(|| format!("missing value for {flag}"))?;
        index += 1;
        match flag {
            "--probe" => {
                if probe.is_some() {
                    return Err("--probe may be specified only once".into());
                }
                probe = Some(match value.as_str() {
                    "radial" => Probe::Radial,
                    "hotfolder" => Probe::Hotfolder,
                    "dashboard" => Probe::Dashboard,
                    _ => return Err(format!("unknown probe: {value}")),
                });
            }
            "--scenario" => {
                if scenario.is_some() {
                    return Err("--scenario may be specified only once".into());
                }
                scenario = Some(
                    scenario_from_str(value).ok_or_else(|| format!("unknown scenario: {value}"))?,
                );
            }
            "--out" => {
                if saw_out {
                    return Err("--out may be specified only once".into());
                }
                saw_out = true;
                if !matches!(value.replace('\\', "/").as_str(), "target/perf-harness") {
                    return Err("--out must be exactly target/perf-harness".into());
                }
            }
            "--warmups" => {
                if saw_warmups {
                    return Err("--warmups may be specified only once".into());
                }
                saw_warmups = true;
                warmups = parse_bounded(value, "warmups", 3, 3)?;
            }
            "--samples" => {
                if saw_samples {
                    return Err("--samples may be specified only once".into());
                }
                saw_samples = true;
                samples = parse_bounded(value, "samples", 1, 20)?;
            }
            "--timeout-seconds" => {
                if saw_timeout {
                    return Err("--timeout-seconds may be specified only once".into());
                }
                saw_timeout = true;
                timeout_seconds = parse_bounded(value, "timeout-seconds", 1, 90)? as u64;
            }
            _ => return Err(format!("unknown argument: {flag}")),
        }
    }

    let probe = probe.ok_or_else(|| "--probe is required".to_string())?;
    let scenario = scenario.ok_or_else(|| "--scenario is required".to_string())?;
    if !probe_accepts(probe, scenario) {
        return Err(format!(
            "scenario {} does not belong to the selected probe",
            scenario.as_str()
        ));
    }
    Ok(Action::Probe {
        probe,
        scenario,
        warmups,
        samples,
        timeout_seconds,
    })
}

fn parse_bounded(
    value: &str,
    label: &str,
    minimum: usize,
    maximum: usize,
) -> Result<usize, String> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!("{label} must be an unsigned decimal integer"));
    }
    let value = value
        .parse::<usize>()
        .map_err(|_| format!("{label} is too large"))?;
    if !(minimum..=maximum).contains(&value) {
        return Err(format!("{label} must be between {minimum} and {maximum}"));
    }
    Ok(value)
}

fn scenario_from_str(value: &str) -> Option<Scenario> {
    Scenario::ALL
        .into_iter()
        .find(|scenario| scenario.as_str() == value)
}

fn probe_accepts(probe: Probe, scenario: Scenario) -> bool {
    match probe {
        Probe::Radial => matches!(
            scenario,
            Scenario::Radial8 | Scenario::Radial32 | Scenario::Radial128
        ),
        Probe::Hotfolder => matches!(
            scenario,
            Scenario::HotfolderDisabled | Scenario::HotfolderEnabledZeroRules
        ),
        Probe::Dashboard => matches!(
            scenario,
            Scenario::DashboardMinimal | Scenario::DashboardTypical | Scenario::DashboardDense
        ),
    }
}

fn probe_binary(probe: Probe) -> &'static str {
    match probe {
        Probe::Radial => "perf_radial_probe.exe",
        Probe::Hotfolder => "perf_hotfolder_probe.exe",
        Probe::Dashboard => "perf_dashboard_probe.exe",
    }
}

fn probe_name(probe: Probe) -> &'static str {
    match probe {
        Probe::Radial => "radial",
        Probe::Hotfolder => "hotfolder",
        Probe::Dashboard => "dashboard",
    }
}

fn run_recorded_child(
    workspace: &Path,
    scenario: Scenario,
    target: windows_runner::LaunchTarget,
    argv_redacted: Vec<String>,
    warmups: usize,
    child_samples: usize,
    timeout_seconds: u64,
) -> Result<multi_launcher::perf_harness::RunSummary, multi_launcher::perf_harness::RecorderError> {
    let outer_config = ProbeConfig {
        scenario,
        warmups,
        samples: 1,
        timeout_seconds,
    };
    let initial_manifest = ProcessManifest::not_attempted("waiting for checked Job Object launch");
    run_recorded(
        workspace,
        outer_config,
        MeasurementKind::RunnerContainment,
        SourceProvenance::from_environment(),
        None,
        initial_manifest,
        |recorder| {
            let child = windows_runner::run_owned(
                workspace,
                target,
                argv_redacted,
                scenario,
                warmups,
                child_samples,
                timeout_seconds,
            );
            recorder.set_process_manifest(child.manifest);
            recorder.append_log(LogStream::Stdout, &child.stdout)?;
            recorder.append_log(LogStream::Stderr, &child.stderr)?;
            if child.output_truncated {
                let _ = recorder.append_log(LogStream::Stderr, b"")?;
                // Appending one byte after the capture budget is exhausted marks
                // truncation without writing a marker into the raw child output.
                let _ = recorder.append_log(LogStream::Stderr, b"x")?;
            }

            if let Some(failure) = child.failure {
                return Err(failure);
            }
            let metric = match child.process_cpu_ms {
                Some(value) => MetricValue::measured(value, "ms")?,
                None => MetricValue::missing(
                    "ms",
                    child.process_cpu_reason.unwrap_or_else(|| {
                        "GetProcessTimes returned no root-process CPU value".into()
                    }),
                ),
            };
            recorder.record_sample(
                SampleRecord::new(child.elapsed).with_metric("process_cpu_ms", metric),
            )?;
            Ok(())
        },
    )
}

fn normal_probe(
    workspace: &Path,
    probe: Probe,
    scenario: Scenario,
    warmups: usize,
    samples: usize,
    timeout_seconds: u64,
) -> Result<multi_launcher::perf_harness::RunSummary, multi_launcher::perf_harness::RecorderError> {
    let args = vec![
        "--scenario".to_string(),
        scenario.as_str().to_string(),
        "--out".to_string(),
        "target/perf-harness".to_string(),
        "--warmups".to_string(),
        warmups.to_string(),
        "--samples".to_string(),
        samples.to_string(),
        "--timeout-seconds".to_string(),
        timeout_seconds.to_string(),
    ];
    let redacted = vec![
        "--probe".into(),
        probe_name(probe).into(),
        "--scenario".into(),
        scenario.as_str().into(),
        "--out".into(),
        "target/perf-harness".into(),
        "--warmups".into(),
        warmups.to_string(),
        "--samples".into(),
        samples.to_string(),
        "--timeout-seconds".into(),
        timeout_seconds.to_string(),
    ];
    run_recorded_child(
        workspace,
        scenario,
        windows_runner::LaunchTarget::Probe(probe_binary(probe), args),
        redacted,
        warmups,
        samples,
        timeout_seconds,
    )
}

fn self_test(workspace: &Path) -> ExitCode {
    // One short sequential batch creates one independent receipt per fixture.
    let fixtures = [
        (
            "exit-zero",
            vec!["--fixture-child", "exit-zero"],
            ProbeStatus::Success,
        ),
        (
            "nonzero-exit",
            vec!["--fixture-child", "exit-nonzero", "23"],
            ProbeStatus::Error,
        ),
        (
            "deadline-timeout",
            vec!["--fixture-child", "sleep", "5000"],
            ProbeStatus::Timeout,
        ),
        (
            "inherited-descendant",
            vec!["--fixture-child", "descendant", "30000"],
            ProbeStatus::Error,
        ),
    ];
    let mut passed = true;
    for (label, args, expected) in fixtures {
        let redacted = args.iter().map(|arg| (*arg).to_string()).collect();
        let target = windows_runner::LaunchTarget::RunnerFixture(
            args.iter().map(|arg| (*arg).to_string()).collect(),
        );
        let result = run_recorded_child(
            workspace,
            Scenario::RunnerSelfTest,
            target,
            redacted,
            3,
            1,
            if expected == ProbeStatus::Timeout {
                2
            } else {
                4
            },
        );
        match result {
            Ok(summary) => {
                let status_ok = summary.status == expected;
                let manifest_ok = manifest_self_test_ok(&summary.output_dir, label);
                println!(
                    "self-test {label}: status={} expected={} cleanup_verified={manifest_ok} receipt={}",
                    summary.status.as_str(),
                    expected.as_str(),
                    summary.output_dir.display()
                );
                let blocked_status = summary.status == ProbeStatus::BlockedEnvironment
                    || summary.status == ProbeStatus::BlockedDesktop;
                let ownership_verified = manifest_ownership_verified(&summary.output_dir);
                passed &= status_ok && manifest_ok;
                if blocked_status || !ownership_verified {
                    eprintln!(
                        "self-test batch stopped after {label}: environment access or owned cleanup was not verified"
                    );
                    return ExitCode::FAILURE;
                }
            }
            Err(error) => {
                eprintln!("self-test {label}: could not write receipt: {error}");
                return ExitCode::FAILURE;
            }
        }
    }
    if passed {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn manifest_self_test_ok(output_dir: &Path, fixture: &str) -> bool {
    let Some(manifest) = read_run_manifest(output_dir) else {
        return false;
    };
    manifest_ownership_verified_in(&manifest)
        && match fixture {
            "exit-zero" => manifest["exit_code"] == 0,
            "nonzero-exit" => manifest["exit_code"] == 23,
            "deadline-timeout" => manifest["deadline_reached"] == true,
            "inherited-descendant" => {
                manifest["exit_code"] == 0
                    && manifest["job_total_processes"]
                        .as_u64()
                        .is_some_and(|count| count >= 2)
                    && manifest["job_active_processes_at_root_exit"]
                        .as_u64()
                        .is_some_and(|count| count > 0)
                    && manifest["cleanup_reason"]
                        .as_str()
                        .is_some_and(|reason| reason.contains("descendants"))
            }
            _ => false,
        }
}

fn fixture_child(arguments: &[String]) -> Option<ExitCode> {
    let Some(mode) = arguments.get(1).map(String::as_str) else {
        return None;
    };
    match mode {
        "exit-zero" if arguments.len() == 2 => {
            println!("fixture stdout: exit zero");
            eprintln!("fixture stderr: exit zero");
            Some(ExitCode::SUCCESS)
        }
        "exit-nonzero" if arguments.len() == 3 => {
            let Some(code) = parse_internal_number(&arguments[2], 1, 255) else {
                return Some(ExitCode::from(2));
            };
            println!("fixture stdout: exit {code}");
            eprintln!("fixture stderr: exit {code}");
            Some(ExitCode::from(code as u8))
        }
        "sleep" if arguments.len() == 3 => {
            let Some(millis) = parse_internal_number(&arguments[2], 1, 30_000) else {
                return Some(ExitCode::from(2));
            };
            std::thread::sleep(Duration::from_millis(millis as u64));
            println!("fixture stdout: sleep completed");
            Some(ExitCode::SUCCESS)
        }
        "descendant" if arguments.len() == 3 => {
            let Some(millis) = parse_internal_number(&arguments[2], 1, 30_000) else {
                return Some(ExitCode::from(2));
            };
            let Ok(executable) = std::env::current_exe() else {
                return Some(ExitCode::from(3));
            };
            let child = std::process::Command::new(executable)
                .args(["--fixture-child", "sleep", &millis.to_string()])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
            if child.is_err() {
                return Some(ExitCode::from(4));
            }
            println!("fixture stdout: descendant started");
            Some(ExitCode::SUCCESS)
        }
        _ => None,
    }
}

fn manifest_ownership_verified(output_dir: &Path) -> bool {
    read_run_manifest(output_dir)
        .as_ref()
        .is_some_and(manifest_ownership_verified_in)
}

fn read_run_manifest(output_dir: &Path) -> Option<serde_json::Value> {
    let bytes = std::fs::read(output_dir.join("run.json")).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn manifest_ownership_verified_in(manifest: &serde_json::Value) -> bool {
    manifest["job_created"] == true
        && manifest["job_assigned"] == true
        && manifest["resumed"] == true
        && manifest["cleanup_verified"] == true
        && manifest["executable_sha256"].as_str().is_some()
        && manifest["child_pid"].as_u64().is_some()
        && manifest["child_creation_time_100ns"].as_u64().is_some()
        && manifest["process_containment"] == "job_object_verified"
}

fn parse_internal_number(value: &str, minimum: u32, maximum: u32) -> Option<u32> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let value = value.parse().ok()?;
    (minimum..=maximum).contains(&value).then_some(value)
}

fn main() -> ExitCode {
    let raw_arguments: Vec<String> = std::env::args().skip(1).collect();
    if raw_arguments
        .first()
        .is_some_and(|arg| arg == "--fixture-child")
    {
        return fixture_child(&raw_arguments).unwrap_or_else(|| ExitCode::from(2));
    }
    let workspace = match std::env::current_exe().ok().and_then(|path| {
        path.parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
            .map(Path::to_path_buf)
    }) {
        Some(path) => path,
        None => {
            eprintln!("could not derive repository root from the runner executable");
            return ExitCode::FAILURE;
        }
    };
    match parse_args(&raw_arguments) {
        Ok(Action::Help) => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}\n{USAGE}");
            ExitCode::from(2)
        }
        Ok(Action::SelfTest) => self_test(&workspace),
        Ok(Action::Probe {
            probe,
            scenario,
            warmups,
            samples,
            timeout_seconds,
        }) => match normal_probe(
            &workspace,
            probe,
            scenario,
            warmups,
            samples,
            timeout_seconds,
        ) {
            Ok(summary) => {
                println!(
                    "{}: {} ({})",
                    summary.status.as_str(),
                    scenario.as_str(),
                    summary.output_dir.display()
                );
                if summary.status == ProbeStatus::Success {
                    ExitCode::SUCCESS
                } else if summary.status == ProbeStatus::BlockedEnvironment {
                    ExitCode::from(3)
                } else {
                    ExitCode::FAILURE
                }
            }
            Err(error) => {
                eprintln!("could not record runner receipt: {error}");
                ExitCode::FAILURE
            }
        },
    }
}
