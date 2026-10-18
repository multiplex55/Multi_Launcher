use multi_launcher::perf_harness::{
    CliAction, MeasurementKind, ProbeFailure, ProbeStatus, ProcessContainment, ProcessManifest,
    SourceProvenance, parse_cli, run_recorded,
};
use std::path::Path;
use std::process::ExitCode;

const USAGE: &str = "Usage: perf_probe_runner --scenario <fixed-name> [--out target/perf-harness] [--warmups 3] [--samples 1..20] [--timeout-seconds 1..90]\n\nH1-A records an unsupported receipt. Safe native process containment is added in H1-B.\n";

fn main() -> ExitCode {
    match parse_cli(std::env::args_os().skip(1)) {
        Ok(CliAction::Help) => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}\n{USAGE}");
            ExitCode::from(2)
        }
        Ok(CliAction::Run(config)) => {
            let mut manifest = ProcessManifest::not_attempted(
                "native process ownership runner is introduced by H1-B",
            );
            manifest.process_containment = ProcessContainment::Unsupported;
            manifest.process_containment_reason =
                "H1-A implements the receipt protocol only; no process API was attempted".into();
            let result = run_recorded(
                Path::new(env!("CARGO_MANIFEST_DIR")),
                config,
                MeasurementKind::RunnerContainment,
                SourceProvenance::from_environment(),
                None,
                manifest,
                |_| {
                    Err(ProbeFailure::new(
                        ProbeStatus::Unsupported,
                        "native process containment is not implemented in H1-A",
                    ))
                },
            );

            match result {
                Ok(summary) => {
                    eprintln!(
                        "{}: {}",
                        summary.status.as_str(),
                        summary.output_dir.display()
                    );
                    if summary.status == ProbeStatus::Unsupported {
                        ExitCode::from(2)
                    } else {
                        ExitCode::FAILURE
                    }
                }
                Err(error) => {
                    eprintln!("could not record run: {error}");
                    ExitCode::FAILURE
                }
            }
        }
    }
}
