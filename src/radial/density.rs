//! Advisories derived from the exact prepared hit shapes and measured text.

use super::diagnostics::{
    DensityPressure, RadialDiagnostic, RadialDiagnosticKind, RadialDiagnosticSeverity,
    RadialDiagnosticSource,
};
use super::geometry::{HitShape, LayoutSnapshot, PhysicalRect};
use super::model::MenuDefinition;
use super::render::{PreparedSceneResources, shape_bounds};
use std::collections::{BTreeMap, BTreeSet};

pub fn analyze(
    menu: &MenuDefinition,
    layout: &LayoutSnapshot,
    resources: &PreparedSceneResources,
    work: PhysicalRect,
) -> Vec<RadialDiagnostic> {
    let mut narrow = 0usize;
    let mut labels = Vec::new();
    let mut truncated = 0usize;
    let mut small_text = 0usize;
    for cell in &layout.cells {
        // Disabled spacers and navigation controls are not executable targets.
        if !cell.actionable || cell.control.is_some() {
            continue;
        }
        let width = match cell.shape {
            HitShape::Circle { radius, .. } => radius * 2.0,
            HitShape::Wedge {
                inner_radius,
                outer_radius,
                start_angle,
                end_angle,
                ..
            } => (outer_radius - inner_radius)
                .min((outer_radius + inner_radius) * 0.5 * (end_angle - start_angle).abs()),
        };
        narrow += usize::from(width < 24.0);
        if !cell.visual.text_visible || cell.label.is_empty() {
            continue;
        }
        let Some(text) = resources.text.get(&cell.cell_id) else {
            continue;
        };
        truncated += usize::from(
            text.diagnostics
                .contains(&super::font_cache::FontDiagnostic::LabelTruncated),
        );
        small_text += usize::from(cell.visual.font_size < 10.0);
        // Match the Text primitive's anchor exactly, including its vertical
        // offset. Use measured rendered bounds, never a font-size estimate.
        let bounds = shape_bounds(&cell.shape);
        let x = (bounds.min.x + bounds.max.x) * 0.5;
        let y = (bounds.min.y + bounds.max.y) * 0.5
            + (cell.visual.text_y_ratio - 0.5) * cell.visual.font_size * cell.visual.text_box_scale;
        let w = text.measured_width_milli as f32 / 1000.0;
        let h = text.measured_height_milli as f32 / 1000.0;
        labels.push([x - w * 0.5, y - h * 0.5, x + w * 0.5, y + h * 0.5]);
    }
    let crowded = overlapping_labels(&labels);
    let mut diagnostics = Vec::new();
    let context = (
        layout.scale.to_bits(),
        layout.scale_factor.get().to_bits(),
        work.min.x.to_bits(),
        work.min.y.to_bits(),
        work.max.x.to_bits(),
        work.max.y.to_bits(),
        labels
            .iter()
            .map(|bounds| bounds.map(f32::to_bits))
            .collect::<Vec<_>>(),
    );
    let mut warning = |kind, count, message: String| {
        diagnostics.push(RadialDiagnostic::new(
            RadialDiagnosticSeverity::Warning,
            RadialDiagnosticKind::DensityPressure(kind),
            RadialDiagnosticSource::Menu {
                menu_id: menu.id.clone(),
            },
            (&context, count),
            message,
        ));
    };
    if narrow > 0 {
        warning(
            DensityPressure::NarrowTargets,
            narrow,
            format!(
                "{narrow} executable targets are narrower than 24 logical pixels in this prepared view. Increase menu scale/spacing when room permits, or use fewer visible slots/pagination."
            ),
        );
    }
    if !crowded.is_empty() {
        warning(
            DensityPressure::LabelOverlap,
            crowded.len(),
            format!(
                "{} measured labels overlap in this prepared view. Use shorter labels, wider radial spacing/rings, or fewer visible slots/pagination.",
                crowded.len()
            ),
        );
    }
    if small_text > 0 || (!labels.is_empty() && truncated * 5 > labels.len() * 2) {
        warning(
            DensityPressure::Readability,
            truncated + small_text,
            format!(
                "{truncated} of {} labels are shortened; {small_text} labels are smaller than 10 logical pixels. Increase label size/scale when room permits or shorten labels.",
                labels.len()
            ),
        );
    }
    if layout.scale < 0.95 {
        warning(
            DensityPressure::WorkAreaFit,
            1,
            format!(
                "This work area fitted the wheel to {:.0}% at {:.2} DPI scale. More work area, fewer visible slots, or smaller ring extent can relieve fitting pressure.",
                layout.scale * 100.0,
                layout.scale_factor.get()
            ),
        );
    }
    diagnostics
}

fn overlapping_labels(labels: &[[f32; 4]]) -> BTreeSet<usize> {
    // Adapt the buckets to the largest measured label. Every full rectangle
    // occupies at most nine buckets per axis, including oversized labels.
    let bucket_width = labels
        .iter()
        .map(|bounds| (bounds[2] - bounds[0]) / 7.0)
        .fold(64.0, f32::max);
    let bucket_height = labels
        .iter()
        .map(|bounds| (bounds[3] - bounds[1]) / 7.0)
        .fold(64.0, f32::max);
    let mut buckets = BTreeMap::<(i32, i32), Vec<usize>>::new();
    let mut crowded = BTreeSet::new();
    for (index, bounds) in labels.iter().enumerate() {
        let left = (bounds[0] / bucket_width).floor() as i32;
        let top = (bounds[1] / bucket_height).floor() as i32;
        let right = (bounds[2] / bucket_width).floor() as i32;
        let bottom = (bounds[3] / bucket_height).floor() as i32;
        for x in left..=right {
            for y in top..=bottom {
                let entries = buckets.entry((x, y)).or_default();
                for other in entries.iter().take(super::model::limits::MAX_TOTAL_CELLS) {
                    if crowded.contains(&index) && crowded.contains(other) {
                        continue;
                    }
                    let b = labels[*other];
                    if bounds[0] < b[2] && bounds[2] > b[0] && bounds[1] < b[3] && bounds[3] > b[1]
                    {
                        crowded.insert(index);
                        crowded.insert(*other);
                    }
                }
                entries.push(index);
            }
        }
    }
    crowded
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::radial::{model::*, preparation::*};
    fn prepare(document: &RadialDocument) -> PreparedFrameInput {
        PreviewFramePreparer::new(Default::default())
            .prepare(
                document,
                &document.default_menu_id,
                super::super::geometry::PhysicalPoint {
                    x: 10000.0,
                    y: 10000.0,
                },
                PhysicalRect {
                    min: super::super::geometry::PhysicalPoint { x: 0.0, y: 0.0 },
                    max: super::super::geometry::PhysicalPoint {
                        x: 20000.0,
                        y: 20000.0,
                    },
                },
                super::super::geometry::ScaleFactor::new(1.0).unwrap(),
                1,
                None,
                &PreviewProjection::default(),
            )
            .unwrap()
    }
    #[test]
    fn valid_authored_double_underscore_action_is_included_in_prepared_readability_analysis() {
        let mut document = RadialDocument::starter();
        let cell = &mut document.menus[0].rings[0].cells[0];
        cell.id = CellId::new("__mail");
        cell.style.text.font_size = Override::Value(8.0);
        crate::radial::validation::validate(&document).unwrap();
        let frame = prepare(&document);
        assert!(
            frame
                .layout
                .cells
                .iter()
                .any(|cell| cell.cell_id.as_str() == "__mail"
                    && cell.actionable
                    && cell.control.is_none())
        );
        assert!(frame.diagnostics.iter().any(|diagnostic| matches!(
            diagnostic.kind,
            RadialDiagnosticKind::DensityPressure(DensityPressure::Readability)
        )));
    }
    #[test]
    fn large_legal_prepared_font_bounds_are_compared_without_corner_clipping() {
        let mut document = RadialDocument::starter();
        document.menus[0].rings.truncate(1);
        document.menus[0].rings[0].radius = 4000.0;
        let template = document.menus[0].rings[0].cells[0].clone();
        document.menus[0].rings[0].cells = (0..6)
            .map(|index| {
                let mut cell = template.clone();
                cell.id = CellId::new(format!("wide-{index}"));
                cell.label = "W".repeat(32);
                cell
            })
            .collect();
        let style = &mut document.menus[0].style.values;
        style.geometry.item_size = Override::Value(2048.0);
        style.text.font_size = Override::Value(512.0);
        style.text.text_box_scale = Override::Value(16.0);
        crate::radial::validation::validate(&document).unwrap();
        let frame = prepare(&document);
        assert!(
            frame
                .resources
                .text
                .values()
                .any(|text| text.measured_width_milli > 512_000),
            "actual text={:?}, visuals={:?}",
            frame
                .resources
                .text
                .values()
                .map(|text| (&text.text, text.measured_width_milli, &text.diagnostics))
                .collect::<Vec<_>>(),
            frame
                .layout
                .cells
                .iter()
                .map(|cell| (
                    &cell.label,
                    cell.visual.font_size,
                    cell.visual.text_box_scale
                ))
                .collect::<Vec<_>>()
        );
        let measured = frame
            .layout
            .cells
            .iter()
            .filter(|cell| cell.cell_id.as_str().starts_with("wide-"))
            .map(|cell| {
                let HitShape::Circle { center, .. } = cell.shape else {
                    panic!("authored circular fixture");
                };
                let text = &frame.resources.text[&cell.cell_id];
                (
                    center,
                    text.measured_width_milli as f32 / 1000.0,
                    text.measured_height_milli as f32 / 1000.0,
                )
            })
            .collect::<Vec<_>>();
        // Independently prove this actual prepared fixture has a horizontal
        // overlap beyond the old 8*64 corner coverage before testing the
        // diagnostic. Merely measuring a wide label does not imply overlap.
        assert!(
            measured.iter().enumerate().any(|(index, (a, aw, ah))| {
                measured.iter().skip(index + 1).any(|(b, bw, bh)| {
                    (a.x - b.x).abs() > 512.0
                        && (a.x - b.x).abs() < (aw + bw) * 0.5
                        && (a.y - b.y).abs() < (ah + bh) * 0.5
                })
            }),
            "fixture prepared centers/widths/heights={measured:?}"
        );
        assert!(frame.diagnostics.iter().any(|diagnostic| matches!(
            diagnostic.kind,
            RadialDiagnosticKind::DensityPressure(DensityPressure::LabelOverlap)
        )));
    }
    #[test]
    fn oversized_measured_rectangles_are_compared_over_their_complete_bounds() {
        assert_eq!(
            overlapping_labels(&[[-600.0, 0.0, 600.0, 100.0], [400.0, 0.0, 1600.0, 100.0]]),
            BTreeSet::from([0, 1])
        );
        assert!(
            overlapping_labels(&[[-600.0, 0.0, 600.0, 100.0], [601.0, 0.0, 1600.0, 100.0]])
                .is_empty()
        );
    }
    #[test]
    fn measured_dense_fifty_cell_fixture_warns_without_mutating_or_capping() {
        let mut document = RadialDocument::starter();
        document.menus[0].layout = LayoutKind::Wedges;
        let template = document.menus[0].rings[0].cells[0].clone();
        let ring = &mut document.menus[0].rings[0];
        ring.cells = (0..50)
            .map(|index| {
                let mut cell = template.clone();
                cell.id = CellId::new(format!("dense-{index}"));
                cell.label = "Long measurable label".into();
                cell
            })
            .collect();
        document.menus[0].style.values.text.font_size = Override::Value(22.0);
        crate::radial::validation::validate(&document).unwrap();
        let before = document.clone();
        let frame = PreviewFramePreparer::new(std::path::PathBuf::new())
            .prepare(
                &document,
                &document.default_menu_id,
                super::super::geometry::PhysicalPoint {
                    x: 1200.0,
                    y: 700.0,
                },
                PhysicalRect {
                    min: super::super::geometry::PhysicalPoint { x: 0.0, y: 0.0 },
                    max: super::super::geometry::PhysicalPoint {
                        x: 2560.0,
                        y: 1440.0,
                    },
                },
                super::super::geometry::ScaleFactor::new(1.0).unwrap(),
                1,
                None,
                &PreviewProjection::default(),
            )
            .unwrap();
        assert_eq!(document, before);
        assert_eq!(document.menus[0].rings[0].cells.len(), 50);
        assert!(frame.diagnostics.iter().any(|d| matches!(
            d.kind,
            RadialDiagnosticKind::DensityPressure(
                DensityPressure::LabelOverlap | DensityPressure::Readability
            )
        )));
        assert!(
            frame
                .diagnostics
                .iter()
                .filter(|d| matches!(d.kind, RadialDiagnosticKind::DensityPressure(_)))
                .count()
                <= 4
        );
    }
}
