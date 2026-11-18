//! Validated, additive chart presentation settings.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use super::{ChartSource, ChartVectorReference};

/// A finite number, including signed zero. Construction and deserialization
/// enforce finiteness, so numerical equality is reflexive and supports Eq.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "f64", into = "f64")]
pub(crate) struct FiniteValue(f64);

impl Eq for FiniteValue {}

impl TryFrom<f64> for FiniteValue {
    type Error = &'static str;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        if value.is_finite() { Ok(Self(value)) } else { Err("chart number must be finite") }
    }
}

impl From<FiniteValue> for f64 {
    fn from(value: FiniteValue) -> Self { value.0 }
}

impl FiniteValue {
    pub(crate) fn get(self) -> f64 { self.0 }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AxisScale {
    #[default]
    Linear,
    Log10,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum AxisBounds {
    #[default]
    Auto,
    Fixed { min: FiniteValue, max: FiniteValue },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum AxisTicks {
    #[default]
    Auto,
    LinearStep { step: FiniteValue },
    Density { count: u8 },
    LogDecades,
    LogSubdivisions { count: u8 },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct AxisOptions {
    pub scale: AxisScale,
    pub bounds: AxisBounds,
    pub ticks: AxisTicks,
}

impl AxisOptions {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if let AxisBounds::Fixed { min, max } = self.bounds {
            if min.get() >= max.get() { return Err("chart bounds must increase"); }
            if self.scale == AxisScale::Log10 && min.get() <= 0.0 {
                return Err("logarithmic chart bounds must be positive");
            }
        }
        match self.ticks {
            AxisTicks::Auto => Ok(()),
            AxisTicks::LinearStep { step } if self.scale == AxisScale::Linear && step.get() > 0.0 => Ok(()),
            AxisTicks::Density { count } if (2..=20).contains(&count) => Ok(()),
            AxisTicks::LogDecades if self.scale == AxisScale::Log10 => Ok(()),
            AxisTicks::LogSubdivisions { count } if self.scale == AxisScale::Log10 && (1..=9).contains(&count) => Ok(()),
            _ => Err("invalid chart tick settings"),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LegendPlacement {
    #[default]
    Top,
    Bottom,
    Hidden,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ReferenceAxis { X, Y }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ReferenceValue {
    Literal(FiniteValue),
    Named(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReferenceLine {
    pub id: u64,
    pub axis: ReferenceAxis,
    pub value: ReferenceValue,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum SeriesKey {
    Vector(ChartVectorReference),
    Sensitivity(String),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LineStyle {
    #[default]
    Solid,
    Dashed,
    Dotted,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MarkerStyle {
    #[default]
    Auto,
    None,
    Circle,
    Square,
    Cross,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SeriesStyle {
    pub key: SeriesKey,
    pub color: [u8; 4],
    pub line: LineStyle,
    pub marker: MarkerStyle,
    pub thickness: FiniteValue,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct ChartOptions {
    pub x: AxisOptions,
    pub y: AxisOptions,
    pub grid: bool,
    pub title: String,
    pub legend: LegendPlacement,
    pub references: Vec<ReferenceLine>,
    pub next_reference_id: u64,
    pub styles: Vec<SeriesStyle>,
}

impl Default for ChartOptions {
    fn default() -> Self {
        Self {
            x: AxisOptions::default(), y: AxisOptions::default(), grid: false,
            title: String::new(), legend: LegendPlacement::Top,
            references: Vec::new(), next_reference_id: 1, styles: Vec::new(),
        }
    }
}

impl ChartOptions {
    pub(crate) fn is_default(&self) -> bool { self == &Self::default() }

    pub(crate) fn retain_selected_styles(&mut self, source: &ChartSource) {
        self.styles.retain(|style| match (&style.key, source) {
            (SeriesKey::Vector(reference), ChartSource::Vectors { y, .. }) => y.contains(reference),
            (SeriesKey::Sensitivity(name), ChartSource::Sensitivity { outputs }) => outputs.contains(name),
            _ => false,
        });
    }

    pub(crate) fn allocate_reference_id(&mut self) -> Result<u64, &'static str> {
        let id = self.next_reference_id;
        if id == 0 { return Err("invalid chart reference identity"); }
        self.next_reference_id = id.checked_add(1).ok_or("chart reference identity space exhausted")?;
        Ok(id)
    }

    pub(crate) fn validate(&self, source: &ChartSource) -> Result<(), &'static str> {
        self.x.validate()?;
        self.y.validate()?;
        if self.title.len() > 128 || self.title.chars().any(char::is_control) {
            return Err("invalid chart title");
        }
        if self.references.len() > 16 || self.next_reference_id == 0 || self.styles.len() > 8 {
            return Err("chart options exceed their limits");
        }
        let valid_name = |name: &str| super::super::document::watch_name(&format!("{{{name}}}")).as_deref() == Some(name);
        let mut ids = HashSet::new();
        for reference in &self.references {
            if reference.id == 0 || reference.id >= self.next_reference_id || !ids.insert(reference.id)
                || reference.label.len() > 128 || reference.label.chars().any(char::is_control)
                || matches!(&reference.value, ReferenceValue::Named(name) if !valid_name(name))
            {
                return Err("invalid chart reference line");
            }
        }
        for (index, style) in self.styles.iter().enumerate() {
            let selected = match (&style.key, source) {
                (SeriesKey::Vector(reference), ChartSource::Vectors { y, .. }) => y.contains(reference),
                (SeriesKey::Sensitivity(name), ChartSource::Sensitivity { outputs }) => outputs.contains(name),
                _ => false,
            };
            if !selected || !(0.5..=8.0).contains(&style.thickness.get())
                || self.styles[..index].iter().any(|old| old.key == style.key)
            {
                return Err("invalid or duplicate chart series style");
            }
        }
        Ok(())
    }
}
