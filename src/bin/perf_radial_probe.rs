use multi_launcher::perf_harness::{
    CliAction, MeasurementKind, MetricValue, ProbeConfig, ProbeFailure, ProbeStatus,
    ProcessManifest, RunSummary, SampleRecord, Scenario, SourceProvenance, parse_cli, run_recorded,
};
use multi_launcher::radial::geometry::{
    HitShape, LayoutSnapshot, LogicalPoint, LogicalRect, PhysicalPoint, PhysicalRect, ScaleFactor,
    layout_menu,
};
use multi_launcher::radial::model::{CellId, MenuDefinition, RadialDocument};
use multi_launcher::radial::render::{VectorPrimitive, VectorScene, build_scene};
use std::hint::black_box;
use std::process::ExitCode;
use std::time::{Duration, Instant};

const GENERATION: u64 = 1;
const MINIMUM_SCALE: f32 = 0.25;
const ANCHOR: PhysicalPoint = PhysicalPoint { x: 0.0, y: 0.0 };
const WORK_AREA: PhysicalRect = PhysicalRect {
    min: PhysicalPoint {
        x: -10_000.0,
        y: -10_000.0,
    },
    max: PhysicalPoint {
        x: 10_000.0,
        y: 10_000.0,
    },
};
const SCALE_FACTOR_VALUE: f64 = 1.0;
const RADIAL_SCENARIOS: [(Scenario, usize); 3] = [
    (Scenario::Radial8, 8),
    (Scenario::Radial32, 32),
    (Scenario::Radial128, 128),
];

struct RadialFixture {
    cell_count: usize,
    menu: MenuDefinition,
    identity_bytes: Vec<u8>,
}

impl RadialFixture {
    fn new(scenario: Scenario) -> Result<Self, ProbeFailure> {
        let cell_count = RADIAL_SCENARIOS
            .iter()
            .find_map(|(candidate, count)| (*candidate == scenario).then_some(*count))
            .ok_or_else(|| {
                ProbeFailure::error(
                    "radial fixture accepts only radial_8, radial_32, or radial_128",
                )
            })?;

        let document = RadialDocument::starter();
        let mut menu = document
            .menus
            .first()
            .cloned()
            .ok_or_else(|| ProbeFailure::error("starter radial document has no menu"))?;
        let mut ring = menu
            .rings
            .first()
            .cloned()
            .ok_or_else(|| ProbeFailure::error("starter radial menu has no ring"))?;
        let cell_template = ring
            .cells
            .first()
            .cloned()
            .ok_or_else(|| ProbeFailure::error("starter radial ring has no cell template"))?;
        ring.cells = (0..cell_count)
            .map(|index| {
                let mut cell = cell_template.clone();
                cell.id = CellId::new(format!("perf-cell-{index}"));
                cell.label = format!("Cell{index}");
                cell
            })
            .collect();
        menu.rings = vec![ring];

        let mut identity_bytes = serde_json::to_vec(&menu).map_err(|error| {
            ProbeFailure::error(format!("serialize deterministic radial fixture: {error}"))
        })?;
        identity_bytes.extend_from_slice(&layout_input_identity());
        Ok(Self {
            cell_count,
            menu,
            identity_bytes,
        })
    }
}

fn layout_input_identity() -> Vec<u8> {
    format!(
        "\nradial-geometry-scene-v1\nanchor_physical={:.1},{:.1}\nwork_area_physical={:.1},{:.1},{:.1},{:.1}\nscale_factor={:.1}\nminimum_scale={:.2}\nscene_generation={GENERATION}\n",
        ANCHOR.x,
        ANCHOR.y,
        WORK_AREA.min.x,
        WORK_AREA.min.y,
        WORK_AREA.max.x,
        WORK_AREA.max.y,
        SCALE_FACTOR_VALUE,
        MINIMUM_SCALE
    )
    .into_bytes()
}

struct GeometryScene {
    layout: LayoutSnapshot,
    scene: VectorScene,
    elapsed: Duration,
}

fn build_geometry_scene(fixture: &RadialFixture) -> Result<GeometryScene, ProbeFailure> {
    let scale_factor = ScaleFactor::new(SCALE_FACTOR_VALUE)
        .ok_or_else(|| ProbeFailure::error("fixed radial scale factor is invalid"))?;

    let started = Instant::now();
    let layout = layout_menu(
        black_box(&fixture.menu),
        black_box(ANCHOR),
        black_box(WORK_AREA),
        black_box(scale_factor),
        black_box(MINIMUM_SCALE),
    )
    .map_err(|error| ProbeFailure::error(format!("production radial layout failed: {error:?}")))?;
    let layout = black_box(layout);
    let scene = black_box(build_scene(black_box(&layout), black_box(GENERATION)));
    let elapsed = started.elapsed();

    Ok(GeometryScene {
        layout,
        scene,
        elapsed,
    })
}

fn validate_geometry_scene(
    fixture: &RadialFixture,
    result: &GeometryScene,
) -> Result<(), ProbeFailure> {
    let layout = &result.layout;
    let scene = &result.scene;
    if layout.cells.len() != fixture.cell_count {
        return Err(ProbeFailure::error(format!(
            "layout contained {} cells; expected {}",
            layout.cells.len(),
            fixture.cell_count
        )));
    }
    for (index, cell) in layout.cells.iter().enumerate() {
        let expected_id = format!("perf-cell-{index}");
        let expected_label = format!("Cell{index}");
        if cell.cell_id.as_str() != expected_id || cell.label != expected_label {
            return Err(ProbeFailure::error(format!(
                "layout cell {index} was {} / {:?}; expected {expected_id} / {expected_label:?}",
                cell.cell_id, cell.label
            )));
        }
        if !hit_shape_is_finite(&cell.shape) {
            return Err(ProbeFailure::error(format!(
                "layout cell {index} contains non-finite geometry"
            )));
        }
    }
    if scene.generation != GENERATION || scene.primitives.is_empty() {
        return Err(ProbeFailure::error(
            "radial scene has the wrong generation or contains no primitives",
        ));
    }
    if !logical_rect_is_finite(scene.bounds) || !scene.primitives.iter().all(primitive_is_finite) {
        return Err(ProbeFailure::error(
            "radial scene contains non-finite bounds or primitive geometry",
        ));
    }
    Ok(())
}

fn logical_point_is_finite(point: LogicalPoint) -> bool {
    point.x.is_finite() && point.y.is_finite()
}

fn logical_rect_is_finite(rect: LogicalRect) -> bool {
    logical_point_is_finite(rect.min) && logical_point_is_finite(rect.max)
}

fn hit_shape_is_finite(shape: &HitShape) -> bool {
    match shape {
        HitShape::Circle { center, radius } => {
            logical_point_is_finite(*center) && radius.is_finite()
        }
        HitShape::Wedge {
            center,
            inner_radius,
            outer_radius,
            start_angle,
            end_angle,
        } => {
            logical_point_is_finite(*center)
                && inner_radius.is_finite()
                && outer_radius.is_finite()
                && start_angle.is_finite()
                && end_angle.is_finite()
        }
    }
}

fn primitive_is_finite(primitive: &VectorPrimitive) -> bool {
    match primitive {
        VectorPrimitive::FilledCircle { center, radius, .. } => {
            logical_point_is_finite(*center) && radius.is_finite()
        }
        VectorPrimitive::FilledWedge {
            center,
            inner_radius,
            outer_radius,
            start_angle,
            end_angle,
            ..
        } => {
            logical_point_is_finite(*center)
                && inner_radius.is_finite()
                && outer_radius.is_finite()
                && start_angle.is_finite()
                && end_angle.is_finite()
        }
        VectorPrimitive::Text { origin, size, .. } => {
            logical_point_is_finite(*origin) && size.is_finite()
        }
        VectorPrimitive::Image { bounds, .. } | VectorPrimitive::Tooltip { bounds, .. } => {
            logical_rect_is_finite(*bounds)
        }
        VectorPrimitive::StaticBoundary => true,
    }
}

fn main() -> ExitCode {
    match parse_cli(std::env::args_os().skip(1)) {
        Err(error) => {
            eprintln!("perf_radial_probe: {error}");
            eprintln!("Run with --help for the supported scenarios and flags.");
            ExitCode::FAILURE
        }
        Ok(CliAction::Help) => {
            print_help();
            ExitCode::SUCCESS
        }
        Ok(CliAction::Run(config)) => {
            if !RADIAL_SCENARIOS
                .iter()
                .any(|(scenario, _)| *scenario == config.scenario)
            {
                eprintln!(
                    "perf_radial_probe: unsupported scenario {}; allowed: radial_8, radial_32, radial_128",
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
                                eprintln!("perf_radial_probe: {reason}");
                            }
                            ExitCode::from(3)
                        }
                        _ => {
                            if let Some(reason) = summary.termination_reason {
                                eprintln!("perf_radial_probe: {reason}");
                            }
                            ExitCode::FAILURE
                        }
                    }
                }
                Err(error) => {
                    eprintln!("perf_radial_probe: could not record run: {error}");
                    ExitCode::FAILURE
                }
            }
        }
    }
}

fn run_probe(
    config: ProbeConfig,
) -> Result<RunSummary, multi_launcher::perf_harness::RecorderError> {
    let fixture = RadialFixture::new(config.scenario)
        .map_err(|failure| multi_launcher::perf_harness::RecorderError::Invalid(failure.reason))?;
    let warmups = config.warmups;
    let samples = config.samples;

    run_recorded(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")),
        config,
        MeasurementKind::CpuHeadless,
        SourceProvenance::from_environment(),
        Some(&fixture.identity_bytes),
        ProcessManifest::not_attempted(
            "in-process radial geometry and unprepared-scene fixture; no child process needed",
        ),
        |recorder| {
            for _ in 0..warmups {
                let result = build_geometry_scene(&fixture)?;
                validate_geometry_scene(&fixture, &result)?;
                black_box(result);
            }
            for _ in 0..samples {
                if recorder.deadline_reached() {
                    return Err(ProbeFailure::timeout(
                        "run deadline expired before radial geometry and scene construction",
                    ));
                }
                let result = build_geometry_scene(&fixture)?;
                validate_geometry_scene(&fixture, &result)?;
                let sample = SampleRecord::new(result.elapsed)
                    .with_metric(
                        "geometry_scene_ms",
                        MetricValue::measured(result.elapsed.as_secs_f64() * 1000.0, "ms")
                            .map_err(ProbeFailure::from)?,
                    )
                    .with_metric(
                        "cell_count",
                        MetricValue::measured(fixture.cell_count as f64, "cells")
                            .map_err(ProbeFailure::from)?,
                    )
                    .with_metric(
                        "primitive_count",
                        MetricValue::measured(result.scene.primitives.len() as f64, "primitives")
                            .map_err(ProbeFailure::from)?,
                    );
                recorder.record_sample(sample).map_err(ProbeFailure::from)?;
            }
            Ok(())
        },
    )
}

fn print_help() {
    println!(
        "Usage: perf_radial_probe --scenario <radial_8|radial_32|radial_128> [--out target/perf-harness] [--warmups 3] [--samples 1..20] [--timeout-seconds 1..90]\n\nMeasures production radial layout plus unprepared vector-scene construction. It does not measure full rendering, GPU work, text shaping, or presentation."
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radial_fixture_hash_and_scene_invariants_hold_for_supported_counts() {
        for (scenario, count) in RADIAL_SCENARIOS {
            let fixture = RadialFixture::new(scenario).expect("deterministic fixture");
            let repeated = RadialFixture::new(scenario).expect("repeat fixture");
            assert_eq!(fixture.cell_count, count);
            assert_eq!(fixture.identity_bytes, repeated.identity_bytes);
            assert_eq!(
                multi_launcher::perf_harness::fixture_sha256(&fixture.identity_bytes),
                multi_launcher::perf_harness::fixture_sha256(&repeated.identity_bytes)
            );

            let result = build_geometry_scene(&fixture).expect("production geometry and scene");
            validate_geometry_scene(&fixture, &result).expect("all output invariants hold");
            assert_eq!(result.layout.cells.len(), count);
            assert!(!result.scene.primitives.is_empty());
        }
    }
}
