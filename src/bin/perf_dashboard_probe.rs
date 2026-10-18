use multi_launcher::dashboard::{DashboardCpuFixture, DashboardFrame};
use multi_launcher::perf_harness::{
    CliAction, MeasurementKind, MetricValue, ProbeConfig, ProbeFailure, ProbeStatus,
    ProcessManifest, RunSummary, SampleRecord, Scenario, SourceProvenance, parse_cli, run_recorded,
};
use std::hint::black_box;
use std::process::ExitCode;

const DASHBOARD_SCENARIOS: [Scenario; 3] = [
    Scenario::DashboardMinimal,
    Scenario::DashboardTypical,
    Scenario::DashboardDense,
];

fn main() -> ExitCode {
    match parse_cli(std::env::args_os().skip(1)) {
        Err(error) => {
            eprintln!("perf_dashboard_probe: {error}");
            eprintln!("Run with --help for the supported scenarios and flags.");
            ExitCode::FAILURE
        }
        Ok(CliAction::Help) => {
            print_help();
            ExitCode::SUCCESS
        }
        Ok(CliAction::Run(config)) => {
            if !DASHBOARD_SCENARIOS.contains(&config.scenario) {
                eprintln!(
                    "perf_dashboard_probe: unsupported scenario {}; allowed: dashboard_minimal, dashboard_typical, dashboard_dense",
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
                                eprintln!("perf_dashboard_probe: {reason}");
                            }
                            ExitCode::from(3)
                        }
                        _ => {
                            if let Some(reason) = summary.termination_reason {
                                eprintln!("perf_dashboard_probe: {reason}");
                            }
                            ExitCode::FAILURE
                        }
                    }
                }
                Err(error) => {
                    eprintln!("perf_dashboard_probe: could not record run: {error}");
                    ExitCode::FAILURE
                }
            }
        }
    }
}

fn run_probe(
    config: ProbeConfig,
) -> Result<RunSummary, multi_launcher::perf_harness::RecorderError> {
    let identity_bytes = DashboardCpuFixture::identity_bytes(config.scenario)
        .map_err(multi_launcher::perf_harness::RecorderError::Invalid)?;
    let scenario = config.scenario;
    let warmups = config.warmups;
    let samples = config.samples;

    run_recorded(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")),
        config,
        MeasurementKind::CpuHeadless,
        SourceProvenance::from_environment(),
        Some(&identity_bytes),
        ProcessManifest::not_attempted(
            "headless in-process egui dashboard fixture; no child process or native presentation; pinned_commands intentionally renders its empty placeholder with no user favorites or pinned IDs",
        ),
        |recorder| {
            let fixture_directory = recorder.output_dir().join("fixture");
            let mut fixture = DashboardCpuFixture::new(fixture_directory, scenario)?;
            let context = eframe::egui::Context::default();

            for frame_index in 0..warmups {
                if recorder.deadline_reached() {
                    return Err(ProbeFailure::timeout(
                        "run deadline expired before dashboard warm-up frame",
                    ));
                }
                let frame = fixture
                    .render_frame(&context, frame_index as u64)
                    .map_err(ProbeFailure::error)?;
                black_box(frame);
            }

            for sample_index in 0..samples {
                if recorder.deadline_reached() {
                    return Err(ProbeFailure::timeout(
                        "run deadline expired before dashboard sample frame",
                    ));
                }
                let frame_index = (warmups + sample_index) as u64;
                let frame = fixture
                    .render_frame(&context, frame_index)
                    .map_err(ProbeFailure::error)?;
                let sample = sample_record(frame, fixture.widget_count())?;
                recorder.record_sample(sample).map_err(ProbeFailure::from)?;
            }
            Ok(())
        },
    )
}

fn sample_record(frame: DashboardFrame, widget_count: usize) -> Result<SampleRecord, ProbeFailure> {
    Ok(SampleRecord::new(frame.elapsed)
        .with_metric(
            "frame_cpu_wall_ms",
            MetricValue::measured(frame.elapsed.as_secs_f64() * 1000.0, "ms")
                .map_err(ProbeFailure::from)?,
        )
        .with_metric(
            "shape_count",
            MetricValue::measured(frame.shape_count as f64, "shapes")
                .map_err(ProbeFailure::from)?,
        )
        .with_metric(
            "widget_count",
            MetricValue::measured(widget_count as f64, "widgets").map_err(ProbeFailure::from)?,
        ))
}

fn print_help() {
    println!(
        "Usage: perf_dashboard_probe --scenario <dashboard_minimal|dashboard_typical|dashboard_dense> [--out target/perf-harness] [--warmups 3] [--samples 1..20] [--timeout-seconds 1..90]\n\nMeasures headless egui Dashboard frame layout and shape generation with synthetic safe widgets. The pinned_commands slot intentionally shows its empty placeholder. It does not measure native windows, GPU work, presentation, or application startup."
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn dashboard_fixtures_render_expected_safe_topologies_and_reject_existing_paths() {
        let temporary = tempfile::tempdir().expect("temporary fixture root");
        let occupied = temporary.path().join("occupied");
        fs::create_dir(&occupied).expect("preexisting fixture path");
        fs::write(occupied.join("sentinel.txt"), b"preserve").expect("sentinel");
        assert!(DashboardCpuFixture::new(&occupied, Scenario::DashboardMinimal).is_err());
        assert_eq!(
            fs::read(occupied.join("sentinel.txt")).expect("sentinel remains"),
            b"preserve"
        );

        let expected_widgets = [1, 4, 9];
        for (scenario, expected_widget_count) in
            DASHBOARD_SCENARIOS.into_iter().zip(expected_widgets)
        {
            let identity =
                DashboardCpuFixture::identity_bytes(scenario).expect("supported topology identity");
            let repeated_identity = DashboardCpuFixture::identity_bytes(scenario)
                .expect("repeat supported topology identity");
            assert_eq!(identity, repeated_identity);
            assert_eq!(
                multi_launcher::perf_harness::fixture_sha256(&identity),
                multi_launcher::perf_harness::fixture_sha256(&repeated_identity)
            );

            let fixture_path = temporary.path().join(scenario.as_str());
            let mut fixture =
                DashboardCpuFixture::new(fixture_path, scenario).expect("inert dashboard fixture");
            assert_eq!(fixture.widget_count(), expected_widget_count);
            let context = eframe::egui::Context::default();
            for frame_index in 0..3 {
                let frame = fixture
                    .render_frame(&context, frame_index)
                    .expect("warm frame satisfies topology and diagnostics invariants");
                assert!(frame.shape_count > 0);
            }
        }
    }
}
