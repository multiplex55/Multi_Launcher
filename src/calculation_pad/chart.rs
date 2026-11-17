//! Typed chart data and one bounded scene for screen and raster rendering.
use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::ToPrimitive;
use std::collections::HashMap;

pub(crate) mod distribution;

use super::document::CalculationValue;
use super::input::{InputKind, SampleNumber};
use super::sensitivity::SensitivityTable;
use crate::annotation::raster::{self, Color, Point, Rect};

const MAX_SERIES: usize = 8;
const MAX_POINTS: usize = super::vector::MAX_VECTOR_COMPONENTS;
const MAX_DIMENSION: u32 = 2048;
const MAX_PIXELS: u64 = 4_194_304;
const MAX_PRIMITIVES: usize = 20_000;
const MAX_RASTER_WORK: u64 = 32_000_000;
const COLORS: [[u8; 4]; 8] = [
    [35, 95, 190, 255],
    [210, 95, 30, 255],
    [35, 140, 95, 255],
    [160, 70, 165, 255],
    [195, 155, 20, 255],
    [30, 150, 170, 255],
    [190, 65, 100, 255],
    [90, 95, 110, 255],
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ChartKind {
    Line,
    Bar,
    Scatter,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(crate) enum ChartVectorReference {
    Physical(super::LineId),
    Named { name: String },
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "source", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ChartSource {
    Vectors { x: Option<ChartVectorReference>, y: Vec<ChartVectorReference> },
    Sensitivity { outputs: Vec<String> },
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ChartDefinition {
    pub source: ChartSource,
    pub kind: ChartKind,
    pub x_label: String,
    pub y_label: String,
    pub hidden: Vec<usize>,
    pub bar_labels: Vec<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct ChartPoint {
    pub x: f64,
    pub y: f64,
    pub raw_x: String,
    pub raw_y: String,
    pub approximate: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct ChartSeries {
    pub label: String,
    pub visible: bool,
    pub points: Vec<Option<ChartPoint>>,
    pub diagnostics: Vec<Option<String>>,
}

#[derive(Clone, Debug)]
pub(crate) struct ChartDataset {
    pub x_label: String,
    pub y_label: String,
    pub x_unit: String,
    pub y_unit: String,
    pub bar_labels: Vec<String>,
    pub series: Vec<ChartSeries>,
    pub approximate: bool,
    pub coordinate_collisions: bool,
}

fn bounded_label(value: &str) -> String {
    value
        .chars()
        .filter(|ch| !ch.is_control())
        .take(128)
        .collect()
}

fn fit_label(value: &str, max_chars: usize) -> String {
    let clean = bounded_label(value);
    if clean.chars().count() <= max_chars {
        return clean;
    }
    let mut result = clean
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    result.push('…');
    result
}

fn finite(value: f64) -> Result<f64, String> {
    value
        .is_finite()
        .then_some(value)
        .ok_or_else(|| "A chart value is outside the finite plotting range.".into())
}

fn rational_coordinate(value: &BigRational) -> Result<(f64, bool), String> {
    let coordinate = value
        .to_f64()
        .filter(|value| value.is_finite())
        .ok_or("An exact chart value exceeds the finite plotting range.")?;
    if coordinate == 0.0 && value != &BigRational::from_integer(0.into()) {
        return Err("An exact chart value is too small to plot.".into());
    }
    let approximate = BigRational::from_float(coordinate).as_ref() != Some(value);
    Ok((coordinate, approximate))
}

struct Numeric {
    coordinate: f64,
    raw: String,
    exact: Option<BigRational>,
    approximate: bool,
}
fn numeric(
    value: &CalculationValue,
    quantity_anchor: Option<&CalculationValue>,
) -> Result<Numeric, String> {
    let raw = super::sensitivity::output_text(value);
    let (coordinate, exact, approximate) = match value {
        CalculationValue::Scalar(value) => (finite(*value)?, None, false),
        CalculationValue::ApproximateScalar(value) => (finite(*value)?, None, true),
        CalculationValue::Integer(value) => {
            let integer = BigInt::parse_bytes(value.formatted_decimal().as_bytes(), 10)
                .ok_or("Invalid integer value.")?;
            let exact = BigRational::from_integer(integer);
            let (coordinate, approximate) = rational_coordinate(&exact)?;
            (coordinate, Some(exact), approximate)
        }
        CalculationValue::ExactBase { value, .. } => {
            let exact = BigRational::from_integer(BigInt::from(*value));
            let (coordinate, approximate) = rational_coordinate(&exact)?;
            (coordinate, Some(exact), approximate)
        }
        CalculationValue::Exact(value) => {
            let exact = value.ratio().clone();
            let (coordinate, approximate) = rational_coordinate(&exact)?;
            (coordinate, Some(exact), approximate || value.is_rounded())
        }
        CalculationValue::PhysicalQuantity { .. } | CalculationValue::DimensionalQuantity(_) => {
            let anchor = quantity_anchor.ok_or("A quantity axis requires a unit reference.")?;
            let (coordinate, _, approximate) = super::quantity::chart_coordinate(anchor, value)
                .map_err(|error| error.to_string())?;
            (finite(coordinate)?, None, approximate)
        }
        _ => return Err("This value type cannot be plotted.".into()),
    };
    Ok(Numeric {
        coordinate,
        raw,
        exact,
        approximate,
    })
}

fn is_quantity(value: &CalculationValue) -> bool {
    matches!(
        value,
        CalculationValue::PhysicalQuantity { .. } | CalculationValue::DimensionalQuantity(_)
    )
}

fn record_collision(seen: &mut HashMap<u64, BigRational>, value: &Numeric) -> bool {
    let Some(exact) = &value.exact else {
        return false;
    };
    let key = if value.coordinate == 0.0 {
        0
    } else {
        value.coordinate.to_bits()
    };
    match seen.get(&key) {
        Some(previous) => previous != exact,
        None => {
            seen.insert(key, exact.clone());
            false
        }
    }
}

impl ChartDataset {
    /// A missing X vector uses indexes 0 through N−1. Every series shares X.
    pub(crate) fn from_vectors(
        x: Option<&[f64]>,
        vectors: &[(&str, &[f64])],
    ) -> Result<Self, String> {
        if vectors.is_empty() || vectors.len() > MAX_SERIES {
            return Err("Choose between one and eight vector series.".into());
        }
        let count = vectors[0].1.len();
        if count > MAX_POINTS {
            return Err("Chart vectors may contain at most 1024 points.".into());
        }
        if x.is_some_and(|values| values.len() != count)
            || vectors.iter().any(|(_, values)| values.len() != count)
        {
            return Err("Chart vectors must have matching lengths.".into());
        }
        let mut series = Vec::with_capacity(vectors.len());
        for (label, values) in vectors {
            let mut points = Vec::with_capacity(count);
            for (index, value) in values.iter().enumerate() {
                let x = finite(x.map_or(index as f64, |values| values[index]))?;
                let y = finite(*value)?;
                points.push(Some(ChartPoint {
                    x,
                    y,
                    raw_x: x.to_string(),
                    raw_y: y.to_string(),
                    approximate: false,
                }));
            }
            series.push(ChartSeries {
                label: bounded_label(label),
                visible: true,
                points,
                diagnostics: vec![None; count],
            });
        }
        Ok(Self {
            x_label: if x.is_some() {
                "X".into()
            } else {
                "Index".into()
            },
            y_label: "Value".into(),
            x_unit: String::new(),
            y_unit: String::new(),
            bar_labels: (0..count).map(|index| index.to_string()).collect(),
            series,
            approximate: false,
            coordinate_collisions: false,
        })
    }

    /// Build from an already-current completed table; failed cells stay gaps.
    pub(crate) fn from_sensitivity(
        table: &SensitivityTable,
        columns: &[usize],
    ) -> Result<Self, String> {
        if table.rows.is_empty()
            || table.rows.len() > super::sensitivity::MAX_CASES
            || columns.is_empty()
            || columns.len() > super::sensitivity::MAX_OUTPUTS
        {
            return Err("Choose one to eight columns from a nonempty bounded table.".into());
        }
        let mut unique = std::collections::HashSet::new();
        if columns
            .iter()
            .any(|index| *index >= table.basis.definition.outputs.len() || !unique.insert(*index))
        {
            return Err("Invalid or duplicate chart column.".into());
        }
        if table
            .rows
            .iter()
            .any(|row| row.cells.len() != table.basis.definition.outputs.len())
        {
            return Err("Sensitivity rows have inconsistent columns.".into());
        }
        let mut anchor: Option<CalculationValue> = None;
        let mut category: Option<bool> = None;
        for &column in columns {
            for row in &table.rows {
                if let Ok(value) = &row.cells[column] {
                    let quantity = is_quantity(value);
                    if category.is_some_and(|previous| previous != quantity) {
                        return Err("Chart series have incompatible numeric units.".into());
                    }
                    category = Some(quantity);
                    if quantity && anchor.is_none() {
                        anchor = Some(value.clone());
                    }
                    if quantity {
                        super::quantity::chart_coordinate(
                            anchor.as_ref().ok_or("Missing chart unit reference.")?,
                            value,
                        )
                        .map_err(|_| "Chart series have incompatible numeric units.")?;
                    }
                }
            }
        }
        let y_unit = match &anchor {
            Some(value) => {
                super::quantity::chart_coordinate(value, value)
                    .map_err(|error| error.to_string())?
                    .1
            }
            None => String::new(),
        };
        let x_unit = match &table.input_kind {
            InputKind::Quantity { unit } => unit.clone(),
            _ => String::new(),
        };
        let mut x_seen = HashMap::new();
        let mut y_seen = HashMap::new();
        let mut approximate = false;
        let mut coordinate_collisions = false;
        let mut series = Vec::with_capacity(columns.len());
        for &column in columns {
            let mut points = Vec::with_capacity(table.rows.len());
            let mut diagnostics = Vec::with_capacity(table.rows.len());
            for row in &table.rows {
                let SampleNumber::Exact(exact_x) = &row.intended;
                let x = match rational_coordinate(exact_x) {
                    Ok((coordinate, approximate)) => Numeric {
                        coordinate,
                        raw: super::sensitivity::intended_text(&row.intended),
                        exact: Some(exact_x.clone()),
                        approximate,
                    },
                    Err(error) => {
                        points.push(None);
                        diagnostics.push(Some(error));
                        continue;
                    }
                };
                let y = match &row.cells[column] {
                    Ok(value) => match numeric(value, anchor.as_ref()) {
                        Ok(value) => value,
                        Err(error) => {
                            points.push(None);
                            diagnostics.push(Some(error));
                            continue;
                        }
                    },
                    Err(error) => {
                        points.push(None);
                        diagnostics.push(Some(error.clone()));
                        continue;
                    }
                };
                let x_collision = record_collision(&mut x_seen, &x);
                let y_collision = record_collision(&mut y_seen, &y);
                coordinate_collisions |= x_collision || y_collision;
                approximate |= x.approximate || y.approximate;
                points.push(Some(ChartPoint {
                    x: x.coordinate,
                    y: y.coordinate,
                    raw_x: x.raw,
                    raw_y: y.raw,
                    approximate: x.approximate || y.approximate,
                }));
                diagnostics.push(None);
            }
            series.push(ChartSeries {
                label: bounded_label(&table.basis.definition.outputs[column]),
                visible: true,
                points,
                diagnostics,
            });
        }
        Ok(Self {
            x_label: bounded_label(&table.basis.input_name),
            y_label: "Value".into(),
            x_unit,
            y_unit,
            bar_labels: table
                .rows
                .iter()
                .map(|row| bounded_label(&row.input))
                .collect(),
            series,
            approximate: approximate || coordinate_collisions,
            coordinate_collisions,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calculation_pad::{
        CalculationDocument, DocumentId, DocumentIncarnation, DocumentOptions, LineId, LineOutcome,
        Sampling, SensitivityBasis, SensitivityDefinition, SensitivityRow,
    };

    fn table(rows: Vec<SensitivityRow>) -> SensitivityTable {
        let definition = SensitivityDefinition {
            input: LineId(1),
            start: "0".into(),
            end: "2".into(),
            sampling: Sampling::Count(3),
            outputs: vec!["output".into()],
        };
        SensitivityTable {
            basis: SensitivityBasis {
                document: DocumentId::fresh(),
                incarnation: DocumentIncarnation(DocumentId::fresh()),
                revision: 0,
                options: DocumentOptions::default(),
                definition,
                input_name: "input".into(),
                generation: 1,
            },
            input_kind: InputKind::Scalar,
            rows,
            work_units: 0,
        }
    }
    fn row(index: i64, cell: Result<CalculationValue, String>) -> SensitivityRow {
        SensitivityRow {
            input: index.to_string(),
            intended: SampleNumber::Exact(BigRational::from_integer(index.into())),
            cells: vec![cell],
        }
    }

    #[test]
    fn vector_validation_and_extreme_transforms_stay_bounded() {
        assert!(ChartDataset::from_vectors(None, &[]).is_err());
        assert!(ChartDataset::from_vectors(Some(&[1.0]), &[("y", &[1.0, 2.0])]).is_err());
        assert!(ChartDataset::from_vectors(None, &[("y", &[f64::NAN])]).is_err());
        let dataset =
            ChartDataset::from_vectors(Some(&[-1e308, 1e308]), &[("value", &[-1e308, 1e308])])
                .unwrap();
        let mut view = ChartView::default();
        assert!(view.set(f64::INFINITY, 1.0, 0.0, 0.0).is_err());
        view.set(10.0, 10.0, 0.1, -0.1).unwrap();
        for kind in [ChartKind::Line, ChartKind::Bar, ChartKind::Scatter] {
            let scene = dataset.scene(kind, view, 640, 400).unwrap().unwrap();
            assert!(scene.rasterize().is_ok());
        }
        view.reset();
        assert_eq!(view, ChartView::default());
        assert!(dataset.scene(ChartKind::Line, view, 4096, 100).is_err());
        let singleton = ChartDataset::from_vectors(None, &[("single", &[4.0])]).unwrap();
        assert!(
            singleton
                .scene(ChartKind::Line, view, 320, 200)
                .unwrap()
                .unwrap()
                .rasterize()
                .is_ok()
        );
        let names = (0..8)
            .map(|index| format!("Series {index}"))
            .collect::<Vec<_>>();
        let values = [1.0, 2.0];
        let series = names
            .iter()
            .map(|name| (name.as_str(), values.as_slice()))
            .collect::<Vec<_>>();
        let legend = ChartDataset::from_vectors(None, &series)
            .unwrap()
            .scene(ChartKind::Line, view, 320, 240)
            .unwrap().unwrap();
        let positions = legend
            .primitives
            .iter()
            .filter_map(|primitive| match primitive {
                ScenePrimitive::Text { position, text, .. } if text.starts_with("Series ") => {
                    Some((position[0] as u32, position[1] as u32))
                }
                _ => None,
            })
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(positions.len(), 8);
        assert!(legend.rasterize().is_ok());
    }

    #[test]
    fn chart_vector_capacity_empty_and_singleton_states_are_distinct() {
        let empty = ChartDataset::from_vectors(Some(&[]), &[("empty", &[])]).unwrap();
        for kind in [ChartKind::Line, ChartKind::Bar, ChartKind::Scatter] {
            assert!(empty.scene(kind, ChartView::default(), 640, 400).unwrap().is_none());
        }
        let mut hidden = empty.clone();
        hidden.series[0].visible = false;
        assert!(hidden.scene(ChartKind::Line, ChartView::default(), 640, 400).unwrap_err().contains("at least one"));
        assert!(ChartDataset::from_vectors(Some(&[1.0]), &[("empty", &[])]).unwrap_err().contains("matching"));
        assert!(ChartDataset::from_vectors(Some(&[]), &[("nonempty", &[1.0])]).unwrap_err().contains("matching"));
        let singleton = ChartDataset::from_vectors(None, &[("one", &[5.0])]).unwrap();
        assert_eq!(singleton.series[0].points[0].as_ref().unwrap().raw_x, "0");
        assert_eq!(singleton.bar_labels, ["0"]);
        let scene = singleton.scene(ChartKind::Line, ChartView::default(), 640, 400).unwrap().unwrap();
        assert!(scene.primitives.iter().any(|primitive| matches!(primitive, ScenePrimitive::Circle { color, .. } if *color == COLORS[0])));
        assert_eq!(scene.hits.len(), 1);
        assert!(ChartDataset::from_vectors(None, &[("too large", &vec![0.0; MAX_POINTS + 1])]).is_err());
    }

    #[test]
    fn chart_all_kinds_render_eight_complete_1024_point_series() {
        let values = (0..MAX_POINTS).map(|value| value as f64 + 1.0).collect::<Vec<_>>();
        let series = (0..8).map(|_| ("series", values.as_slice())).collect::<Vec<_>>();
        let dataset = ChartDataset::from_vectors(None, &series).unwrap();
        assert_eq!(dataset.series[0].points.last().unwrap().as_ref().unwrap().raw_x, "1023");
        assert_eq!(dataset.bar_labels.last().unwrap(), "1023");
        for kind in [ChartKind::Line, ChartKind::Bar, ChartKind::Scatter] {
            let started = std::time::Instant::now();
            let scene = dataset.scene(kind, ChartView::default(), 640, 400).unwrap().unwrap();
            assert_eq!(scene.hits.len(), 8 * MAX_POINTS, "{kind:?}");
            if kind == ChartKind::Bar {
                let bars = scene.primitives.iter().filter(|primitive| matches!(primitive, ScenePrimitive::Rect { color, .. } if COLORS.contains(color))).count();
                assert_eq!(bars, 8 * MAX_POINTS);
            }
            let image = scene.rasterize().unwrap();
            assert_eq!((image.width(), image.height()), (640, 400));
            eprintln!("{kind:?}: {} primitives, {} hits, {:?}", scene.primitives.len(), scene.hits.len(), started.elapsed());
        }
    }

    #[test]
    fn chart_raster_work_counts_inclusive_thin_rectangles() {
        let scene = ChartScene {
            width: 2048, height: 2048,
            primitives: vec![ScenePrimitive::Rect { min: [1.1, 0.0], max: [1.2, 2047.0], color: COLORS[0] }; 7000],
            hits: Vec::new(), approximate: false, coordinate_collisions: false,
        };
        assert!(scene.rasterize().unwrap_err().contains("work limit"));
    }

    #[test]
    fn chart_bar_ticks_include_the_group_inset() {
        let dataset = ChartDataset::from_vectors(Some(&[0.0, 100.0]), &[("series", &[2.0, 4.0])]).unwrap();
        let scene = dataset.scene(ChartKind::Bar, ChartView::default(), 640, 400).unwrap().unwrap();
        let plot_width = 640.0_f32 - 20.0 - 66.0;
        let inset = f64::from(32.0_f32 / (2.0 * plot_width));
        let expected = visible_tick(0.0, 100.0, 1.0 - 2.0 * inset, 0.0, 1.0);
        assert!(scene.primitives.iter().any(|primitive| matches!(primitive, ScenePrimitive::Text { text, .. } if text == &expected)));
        assert_ne!(expected, visible_tick(0.0, 100.0, 1.0, 0.0, 1.0));
    }

    #[test]
    fn chart_sensitivity_capacity_stays_at_1000_samples() {
        let full = table((0..1000).map(|index| row(index, Ok(CalculationValue::Scalar(1.0)))).collect());
        assert_eq!(ChartDataset::from_sensitivity(&full, &[0]).unwrap().series[0].points.len(), 1000);
        let oversized = table((0..1001).map(|index| row(index, Ok(CalculationValue::Scalar(1.0)))).collect());
        assert!(ChartDataset::from_sensitivity(&oversized, &[0]).is_err());
    }

    #[test]
    fn quantity_normalization_gaps_and_exact_tooltips_are_preserved() {
        let document = CalculationDocument::new(
            "1 m\n100 cm\n1 s\nu128(9007199254740992)\nu128(9007199254740993)",
        );
        let evaluation = document.evaluate();
        let value = |index: usize| match &evaluation.lines[index].outcome {
            LineOutcome::Value(value) => value.clone(),
            other => panic!("{other:?}"),
        };
        let data = table(vec![
            row(0, Ok(value(0))),
            row(1, Err("division by zero".into())),
            row(2, Ok(value(1))),
        ]);
        let chart = ChartDataset::from_sensitivity(&data, &[0]).unwrap();
        assert_eq!(chart.y_unit, "m");
        assert_eq!(chart.series[0].points[0].as_ref().unwrap().y, 1.0);
        assert_eq!(chart.series[0].points[2].as_ref().unwrap().y, 1.0);
        assert!(chart.series[0].points[1].is_none());
        assert!(
            chart.series[0].diagnostics[1]
                .as_ref()
                .unwrap()
                .contains("division")
        );
        let scene = chart
            .scene(ChartKind::Line, ChartView::default(), 640, 400)
            .unwrap().unwrap();
        assert!(
            !scene
                .primitives
                .iter()
                .any(|p| matches!(p, ScenePrimitive::Line { color, .. } if *color == COLORS[0]))
        );
        let bad = table(vec![row(0, Ok(value(0))), row(1, Ok(value(2)))]);
        assert!(ChartDataset::from_sensitivity(&bad, &[0]).is_err());
        let exact = table(vec![row(0, Ok(value(3))), row(1, Ok(value(4)))]);
        let chart = ChartDataset::from_sensitivity(&exact, &[0]).unwrap();
        assert!(chart.coordinate_collisions && chart.approximate);
        assert_eq!(
            chart.series[0].points[1].as_ref().unwrap().raw_y,
            "9007199254740993"
        );
        let scalar = table(vec![row(0, Ok(CalculationValue::Scalar(1.23456789012345)))]);
        let chart = ChartDataset::from_sensitivity(&scalar, &[0]).unwrap();
        assert_eq!(
            chart.series[0].points[0].as_ref().unwrap().raw_y,
            "1.23456789012345"
        );
        let mut huge = row(0, Ok(CalculationValue::Scalar(1.0)));
        huge.intended = SampleNumber::Exact(BigRational::from_integer(BigInt::from(10u8).pow(400)));
        let chart = ChartDataset::from_sensitivity(&table(vec![huge]), &[0]).unwrap();
        assert!(chart.series[0].points[0].is_none());
        assert!(chart.series[0].diagnostics[0].is_some());
        assert!(
            chart
                .scene(ChartKind::Scatter, ChartView::default(), 640, 400)
                .is_err()
        );
    }

    #[test]
    fn chart_raster_has_labels_gaps_and_optional_evidence() {
        let data = table(vec![
            row(0, Ok(CalculationValue::Scalar(1.0))),
            row(1, Ok(CalculationValue::Scalar(2.0))),
            row(2, Err("gap".into())),
            row(3, Ok(CalculationValue::Scalar(4.0))),
            row(4, Ok(CalculationValue::Scalar(5.0))),
        ]);
        let chart = ChartDataset::from_sensitivity(&data, &[0]).unwrap();
        let scene = chart
            .scene(ChartKind::Line, ChartView::default(), 640, 400)
            .unwrap().unwrap();
        assert_eq!(scene.hits.len(), 4);
        let gap_x = (scene.hits[1].position[0] + scene.hits[2].position[0]) * 0.5;
        let segments = scene
            .primitives
            .iter()
            .filter_map(|primitive| match primitive {
                ScenePrimitive::Line {
                    start, end, color, ..
                } if *color == COLORS[0] => Some((start, end)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(segments.len(), 2);
        assert!(
            segments
                .iter()
                .all(|(start, end)| (start[0] < gap_x && end[0] < gap_x)
                    || (start[0] > gap_x && end[0] > gap_x))
        );
        assert!(scene.primitives.iter().any(
            |primitive| matches!(primitive, ScenePrimitive::Text { text, .. } if text == "output")
        ));
        let image = scene.rasterize().unwrap();
        assert!(image.pixels().any(|pixel| pixel.0 != [255, 255, 255, 255]));
        let axes = scene
            .primitives
            .iter()
            .filter_map(|primitive| match primitive {
                ScenePrimitive::Line {
                    start, end, color, ..
                } if *color == [45, 50, 58, 255] => Some((start, end)),
                _ => None,
            })
            .take(2)
            .collect::<Vec<_>>();
        assert_eq!(axes.len(), 2);
        for (start, end) in axes {
            let x = ((start[0] + end[0]) * 0.5).floor() as u32;
            let y = ((start[1] + end[1]) * 0.5).floor() as u32;
            assert_eq!(image.get_pixel(x, y).0, [45, 50, 58, 255]);
        }
        assert!(right_aligned_x(620.0, 66.0, "0", 11.0) > 600.0);
        if let Some(path) = std::env::var_os("CALC_CHART_EVIDENCE") {
            let path = std::path::PathBuf::from(path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            image.save(path).unwrap();
        }
        let mut malformed = scene;
        malformed.primitives.push(ScenePrimitive::Line {
            start: [f32::NAN, 0.0],
            end: [0.0, 0.0],
            color: [0, 0, 0, 255],
            width: 1.0,
        });
        assert!(malformed.rasterize().is_err());
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ChartView {
    pub zoom_x: f64,
    pub zoom_y: f64,
    pub pan_x: f64,
    pub pan_y: f64,
}
impl Default for ChartView {
    fn default() -> Self {
        Self {
            zoom_x: 1.0,
            zoom_y: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
        }
    }
}
impl ChartView {
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }
    pub(crate) fn set(
        &mut self,
        zoom_x: f64,
        zoom_y: f64,
        pan_x: f64,
        pan_y: f64,
    ) -> Result<(), String> {
        if ![zoom_x, zoom_y, pan_x, pan_y]
            .iter()
            .all(|value| value.is_finite())
            || !(0.05..=1000.0).contains(&zoom_x)
            || !(0.05..=1000.0).contains(&zoom_y)
            || pan_x.abs() > 10.0
            || pan_y.abs() > 10.0
        {
            return Err("Chart zoom or pan is outside the supported range.".into());
        }
        *self = Self {
            zoom_x,
            zoom_y,
            pan_x,
            pan_y,
        };
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub(crate) enum ScenePrimitive {
    Line {
        start: [f32; 2],
        end: [f32; 2],
        color: [u8; 4],
        width: f32,
    },
    Rect {
        min: [f32; 2],
        max: [f32; 2],
        color: [u8; 4],
    },
    Circle {
        center: [f32; 2],
        radius: f32,
        color: [u8; 4],
    },
    Text {
        position: [f32; 2],
        text: String,
        color: [u8; 4],
        size: f32,
    },
}
#[derive(Clone, Debug)]
pub(crate) struct ChartHit {
    pub position: [f32; 2],
    pub series: usize,
    pub row: usize,
    pub raw_x: String,
    pub raw_y: String,
    pub approximate: bool,
}
#[derive(Clone, Debug)]
pub(crate) struct ChartScene {
    pub width: u32,
    pub height: u32,
    pub primitives: Vec<ScenePrimitive>,
    pub hits: Vec<ChartHit>,
    pub approximate: bool,
    pub coordinate_collisions: bool,
}
impl ChartScene {
    pub(crate) fn hit(&self, position: [f32; 2], radius: f32) -> Option<&ChartHit> {
        if !position.iter().all(|value| value.is_finite()) || !radius.is_finite() || radius < 0.0 {
            return None;
        }
        self.hits
            .iter()
            .filter_map(|hit| {
                let dx = hit.position[0] - position[0];
                let dy = hit.position[1] - position[1];
                let distance = dx * dx + dy * dy;
                (distance <= radius * radius).then_some((distance, hit))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, hit)| hit)
    }

    pub(crate) fn rasterize(&self) -> Result<image::RgbaImage, String> {
        validate_dimensions(self.width, self.height)?;
        if self.primitives.len() > MAX_PRIMITIVES {
            return Err("Chart has too many drawing commands.".into());
        }
        let mut work = u64::from(self.width) * u64::from(self.height);
        let in_image = |point: &[f32; 2]| {
            point[0].is_finite()
                && point[1].is_finite()
                && point[0] >= 0.0
                && point[1] >= 0.0
                && point[0] <= self.width as f32
                && point[1] <= self.height as f32
        };
        for primitive in &self.primitives {
            let cost = match primitive {
                ScenePrimitive::Line {
                    start, end, width, ..
                } if in_image(start)
                    && in_image(end)
                    && width.is_finite()
                    && *width > 0.0
                    && *width <= 8.0 =>
                {
                    let steps = (end[0] - start[0])
                        .abs()
                        .max((end[1] - start[1]).abs())
                        .ceil() as u64
                        + 2;
                    steps.saturating_mul((width.ceil().max(1.0) as u64 + 2).saturating_pow(2))
                }
                ScenePrimitive::Rect { min, max, .. } if in_image(min) && in_image(max) => {
                    // Match draw_rect_fill's inclusive, clipped pixel bounds.
                    let span = |axis: usize, limit: u32| {
                        let first = min[axis].min(max[axis]).floor().max(0.0) as i64;
                        let last = min[axis].max(max[axis]).ceil().min((limit - 1) as f32) as i64;
                        (last - first + 1).max(0) as u64
                    };
                    span(0, self.width).saturating_mul(span(1, self.height))
                }
                ScenePrimitive::Circle { center, radius, .. }
                    if in_image(center)
                        && radius.is_finite()
                        && *radius > 0.0
                        && *radius <= 32.0 =>
                {
                    ((radius.ceil() as u64).saturating_mul(2).saturating_add(1)).saturating_pow(2)
                }
                ScenePrimitive::Text {
                    position,
                    text,
                    size,
                    ..
                } if in_image(position)
                    && text.len() <= 512
                    && size.is_finite()
                    && (1.0..=32.0).contains(size) =>
                {
                    (text.chars().count() as u64).saturating_mul(1024)
                }
                _ => return Err("Chart scene contains invalid drawing coordinates.".into()),
            };
            work = work
                .checked_add(cost)
                .ok_or("Chart rendering work overflowed.")?;
            if work > MAX_RASTER_WORK {
                return Err("Chart exceeds the raster rendering work limit.".into());
            }
        }
        let (font, tweak) = raster::default_font_arc().ok_or("Chart font is unavailable.")?;
        let mut image = image::RgbaImage::from_pixel(
            self.width,
            self.height,
            image::Rgba([255, 255, 255, 255]),
        );
        for primitive in &self.primitives {
            match primitive {
                ScenePrimitive::Line {
                    start,
                    end,
                    color,
                    width,
                } => raster::draw_line(
                    &mut image,
                    Point::new(start[0], start[1]),
                    Point::new(end[0], end[1]),
                    Color(*color),
                    *width,
                ),
                ScenePrimitive::Rect { min, max, color } => raster::draw_rect_fill(
                    &mut image,
                    Rect::from_corners(Point::new(min[0], min[1]), Point::new(max[0], max[1])),
                    Color(*color),
                ),
                ScenePrimitive::Circle {
                    center,
                    radius,
                    color,
                } => raster::draw_circle(
                    &mut image,
                    Point::new(center[0], center[1]),
                    *radius,
                    Color(*color),
                ),
                ScenePrimitive::Text {
                    position,
                    text,
                    color,
                    size,
                } => raster::draw_text(
                    &mut image,
                    &font,
                    tweak.clone(),
                    Point::new(position[0], position[1]),
                    text,
                    Color(*color),
                    *size,
                ),
            }
        }
        Ok(image)
    }
}

fn validate_dimensions(width: u32, height: u32) -> Result<(), String> {
    if width < 160
        || height < 120
        || width > MAX_DIMENSION
        || height > MAX_DIMENSION
        || u64::from(width) * u64::from(height) > MAX_PIXELS
    {
        return Err("Chart dimensions must be between 160×120 and 2048×2048 pixels.".into());
    }
    Ok(())
}

fn unit_position(value: f64, minimum: f64, maximum: f64) -> f64 {
    if minimum == maximum {
        return 0.5;
    }
    let scale = minimum.abs().max(maximum.abs()).max(1.0);
    let low = minimum / scale;
    let high = maximum / scale;
    let width = high - low;
    if width == 0.0 {
        0.5
    } else {
        ((value / scale) - low) / width
    }
}

fn clip_line(a: [f32; 2], b: [f32; 2], bounds: [f32; 4]) -> Option<([f32; 2], [f32; 2])> {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let mut lo: f32 = 0.0;
    let mut hi: f32 = 1.0;
    for (p, q) in [
        (-dx, a[0] - bounds[0]),
        (dx, bounds[2] - a[0]),
        (-dy, a[1] - bounds[1]),
        (dy, bounds[3] - a[1]),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let ratio = q / p;
            if p < 0.0 {
                lo = lo.max(ratio);
            } else {
                hi = hi.min(ratio);
            }
            if lo > hi {
                return None;
            }
        }
    }
    Some((
        [a[0] + lo * dx, a[1] + lo * dy],
        [a[0] + hi * dx, a[1] + hi * dy],
    ))
}

fn tick(value: f64) -> String {
    if value == 0.0 {
        "0".into()
    } else if value.abs() >= 1e6 || value.abs() < 1e-4 {
        format!("{value:.3e}")
    } else {
        format!("{value:.4}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_owned()
    }
}

fn visible_tick(minimum: f64, maximum: f64, zoom: f64, pan: f64, edge: f64) -> String {
    if minimum == maximum {
        return tick(minimum);
    }
    let t = (edge - 0.5 - pan) / zoom + 0.5;
    let scale = minimum.abs().max(maximum.abs()).max(1.0);
    let value = ((minimum / scale) * (1.0 - t) + (maximum / scale) * t) * scale;
    if value.is_finite() {
        tick(value)
    } else {
        "out of range".into()
    }
}

fn right_aligned_x(right: f32, left: f32, text: &str, size: f32) -> f32 {
    static FONT: std::sync::OnceLock<Option<(ab_glyph::FontArc, eframe::egui::FontTweak)>> =
        std::sync::OnceLock::new();
    let ink_right = FONT
        .get_or_init(raster::default_font_arc)
        .as_ref()
        .and_then(|(font, tweak)| raster::measure_text_ink_bounds(font, tweak, text, size))
        .map_or(text.chars().count() as f32 * size * 0.65, |bounds| {
            bounds.max.x
        });
    (right - ink_right).max(left)
}

impl ChartDataset {
    pub(crate) fn scene(
        &self,
        kind: ChartKind,
        view: ChartView,
        width: u32,
        height: u32,
    ) -> Result<Option<ChartScene>, String> {
        validate_dimensions(width, height)?;
        let mut checked = ChartView::default();
        checked.set(view.zoom_x, view.zoom_y, view.pan_x, view.pan_y)?;
        let visible = self
            .series
            .iter()
            .filter(|series| series.visible)
            .collect::<Vec<_>>();
        if visible.is_empty() || visible.len() > MAX_SERIES {
            return Err("Show at least one chart series.".into());
        }
        let expected = visible[0].points.len();
        if visible.iter().any(|series| {
            series.points.len() > MAX_POINTS
                || series.points.len() != series.diagnostics.len()
                || series.points.len() != expected
        }) {
            return Err("Chart data exceeds its bounded shape.".into());
        }
        if expected == 0 {
            return Ok(None);
        }
        let points = visible
            .iter()
            .flat_map(|series| series.points.iter().flatten())
            .collect::<Vec<_>>();
        if points.is_empty() {
            return Err("There are no plottable points; inspect the table diagnostics.".into());
        }
        if points
            .iter()
            .any(|point| !point.x.is_finite() || !point.y.is_finite())
        {
            return Err("Chart coordinates must be finite.".into());
        }
        let xmin = points.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
        let xmax = points.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
        let mut ymin = points.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
        let mut ymax = points.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
        if kind == ChartKind::Bar {
            ymin = ymin.min(0.0);
            ymax = ymax.max(0.0);
        }
        let legend_columns = ((width.saturating_sub(16) / 125) as usize)
            .max(1)
            .min(visible.len());
        let legend_rows = visible.len().div_ceil(legend_columns);
        let warning_rows = usize::from(self.approximate) + usize::from(self.coordinate_collisions);
        let top = 22.0 + (legend_rows + warning_rows) as f32 * 16.0;
        let bounds = [66.0, top, width as f32 - 20.0, height as f32 - 52.0];
        if bounds[3] - bounds[1] < 24.0 {
            return Err("Increase chart height to show the legend and plot.".into());
        }
        let plot_width = bounds[2] - bounds[0];
        let bar_width = (plot_width / expected.max(1) as f32 * 0.75 / visible.len() as f32).min(32.0);
        let inset = if kind == ChartKind::Bar { f64::from(bar_width * visible.len() as f32 / (2.0 * plot_width)) } else { 0.0 };
        let map = |point: &ChartPoint| -> [f32; 2] {
            let position = unit_position(point.x, xmin, xmax);
            let position = inset + position * (1.0 - 2.0 * inset);
            let x = (position - 0.5) * checked.zoom_x + 0.5 + checked.pan_x;
            let y =
                (unit_position(point.y, ymin, ymax) - 0.5) * checked.zoom_y + 0.5 + checked.pan_y;
            [
                bounds[0] + (bounds[2] - bounds[0]) * x as f32,
                bounds[3] - (bounds[3] - bounds[1]) * y as f32,
            ]
        };
        let inside = |point: [f32; 2]| {
            point[0] >= bounds[0]
                && point[0] <= bounds[2]
                && point[1] >= bounds[1]
                && point[1] <= bounds[3]
        };
        let black = [45, 50, 58, 255];
        let x_zoom = checked.zoom_x * (1.0 - 2.0 * inset);
        let right_tick = visible_tick(xmin, xmax, x_zoom, checked.pan_x, 1.0);
        let mut primitives = vec![
            ScenePrimitive::Line {
                start: [bounds[0], bounds[3]],
                end: [bounds[2], bounds[3]],
                color: black,
                width: 2.0,
            },
            ScenePrimitive::Line {
                start: [bounds[0], bounds[1]],
                end: [bounds[0], bounds[3]],
                color: black,
                width: 2.0,
            },
            ScenePrimitive::Line {
                start: [bounds[2], bounds[3]],
                end: [bounds[2], bounds[3] + 3.0],
                color: black,
                width: 2.0,
            },
            ScenePrimitive::Line {
                start: [bounds[0] - 3.0, bounds[1]],
                end: [bounds[0], bounds[1]],
                color: black,
                width: 2.0,
            },
            ScenePrimitive::Text {
                position: [bounds[0], height as f32 - 22.0],
                text: bounded_label(&format!("{} {}", self.x_label, self.x_unit)),
                color: black,
                size: 13.0,
            },
            ScenePrimitive::Text {
                position: [4.0, 5.0],
                text: bounded_label(&format!("{} {}", self.y_label, self.y_unit)),
                color: black,
                size: 13.0,
            },
            ScenePrimitive::Text {
                position: [4.0, bounds[1]],
                text: visible_tick(ymin, ymax, checked.zoom_y, checked.pan_y, 1.0),
                color: black,
                size: 11.0,
            },
            ScenePrimitive::Text {
                position: [4.0, bounds[3] - 12.0],
                text: visible_tick(ymin, ymax, checked.zoom_y, checked.pan_y, 0.0),
                color: black,
                size: 11.0,
            },
            ScenePrimitive::Text {
                position: [bounds[0], bounds[3] + 4.0],
                text: visible_tick(xmin, xmax, x_zoom, checked.pan_x, 0.0),
                color: black,
                size: 11.0,
            },
            ScenePrimitive::Text {
                position: [
                    right_aligned_x(bounds[2], bounds[0], &right_tick, 11.0),
                    bounds[3] + 4.0,
                ],
                text: right_tick,
                color: black,
                size: 11.0,
            },
        ];
        if self.approximate {
            primitives.push(ScenePrimitive::Text {
                position: [8.0, 22.0 + legend_rows as f32 * 16.0],
                text: "Approximate coordinates".into(),
                color: [150, 70, 20, 255],
                size: 11.0,
            });
            if self.coordinate_collisions {
                primitives.push(ScenePrimitive::Text {
                    position: [8.0, 22.0 + (legend_rows + 1) as f32 * 16.0],
                    text: "Coordinate collisions".into(),
                    color: [150, 70, 20, 255],
                    size: 11.0,
                });
            }
        }
        let mut hits = Vec::new();
        for (visible_index, (series_index, series)) in self
            .series
            .iter()
            .enumerate()
            .filter(|(_, series)| series.visible)
            .enumerate()
        {
            let color = COLORS[series_index % COLORS.len()];
            let mut previous = None;
            for (row, point) in series.points.iter().enumerate() {
                let Some(point) = point else {
                    previous = None;
                    continue;
                };
                let position = map(point);
                if !position.iter().all(|value| value.is_finite()) {
                    return Err("Chart transform is outside the finite range.".into());
                }
                let mut hit_position = position;
                match kind {
                    ChartKind::Line => {
                        if let Some(before) = previous
                            && let Some((start, end)) = clip_line(before, position, bounds)
                        {
                            primitives.push(ScenePrimitive::Line {
                                start,
                                end,
                                color,
                                width: 2.0,
                            });
                        }
                        if inside(position) {
                            primitives.push(ScenePrimitive::Circle {
                                center: position,
                                radius: 3.0,
                                color,
                            });
                        }
                    }
                    ChartKind::Scatter => {
                        if inside(position) {
                            primitives.push(ScenePrimitive::Circle {
                                center: position,
                                radius: 4.0,
                                color,
                            });
                        }
                    }
                    ChartKind::Bar => {
                        let baseline = ((unit_position(0.0, ymin, ymax) - 0.5) * checked.zoom_y
                            + 0.5
                            + checked.pan_y)
                            .clamp(0.0, 1.0);
                        let zero = bounds[3] - (bounds[3] - bounds[1]) * baseline as f32;
                        let width = bar_width;
                        let center = position[0]
                            + (visible_index as f32 - (visible.len() - 1) as f32 * 0.5) * width;
                        hit_position[0] = center;
                        let left = (center - width * 0.5).clamp(bounds[0], bounds[2]);
                        let right = (center + width * 0.5).clamp(bounds[0], bounds[2]);
                        if left < right {
                            primitives.push(ScenePrimitive::Rect {
                                min: [left, position[1].min(zero).clamp(bounds[1], bounds[3])],
                                max: [right, position[1].max(zero).clamp(bounds[1], bounds[3])],
                                color,
                            });
                        }
                    }
                }
                if inside(hit_position) {
                    hits.push(ChartHit {
                        position: hit_position,
                        series: series_index,
                        row,
                        raw_x: point.raw_x.clone(),
                        raw_y: point.raw_y.clone(),
                        approximate: point.approximate,
                    });
                }
                if kind == ChartKind::Bar
                    && visible_index == 0
                    && series.points.len() <= 12
                    && inside(position)
                    && let Some(label) = self.bar_labels.get(row)
                {
                    primitives.push(ScenePrimitive::Text {
                        position: [position[0].max(bounds[0]), bounds[3] + 15.0],
                        text: bounded_label(label),
                        color: black,
                        size: 10.0,
                    });
                }
                previous = Some(position);
            }
            let slot_width = (width as f32 - 16.0) / legend_columns as f32;
            primitives.push(ScenePrimitive::Text {
                position: [
                    8.0 + (visible_index % legend_columns) as f32 * slot_width,
                    22.0 + (visible_index / legend_columns) as f32 * 16.0,
                ],
                text: fit_label(
                    &series.label,
                    ((slot_width - 8.0) / 7.0).floor().max(2.0) as usize,
                ),
                color,
                size: 11.0,
            });
        }
        if primitives.len() > MAX_PRIMITIVES {
            return Err("Chart has too many drawing commands.".into());
        }
        Ok(Some(ChartScene {
            width,
            height,
            primitives,
            hits,
            approximate: self.approximate,
            coordinate_collisions: self.coordinate_collisions,
        }))
    }
}
