//! Bounded histogram bins and empirical cumulative distribution data.

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{ToPrimitive, Zero};

use super::{ChartVectorReference, MAX_POINTS};
use super::options::FiniteValue;
use serde::{Deserialize, Serialize};

pub(crate) const MAX_BINS: usize = 1024;
pub(crate) const BOUNDARY_NOTE: &str =
    "≈ marks a rounded boundary. Exact boundaries determine bin membership.";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum DistributionBins {
    #[default]
    Auto,
    Count { count: u16 },
    Width { width: FiniteValue },
}

impl DistributionBins {
    pub(crate) fn validate(self) -> Result<(), &'static str> {
        match self {
            Self::Auto => Ok(()),
            Self::Count { count } if (1..=MAX_BINS as u16).contains(&count) => Ok(()),
            Self::Width { width } if width.get() > 0.0 => Ok(()),
            _ => Err("Invalid distribution bin settings."),
        }
    }

    pub(crate) fn histogram_bins(self) -> HistogramBins {
        match self {
            Self::Auto => HistogramBins::Auto,
            Self::Count { count } => HistogramBins::Count(count as usize),
            Self::Width { width } => HistogramBins::Width(width.get()),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DistributionScale {
    #[default]
    Count,
    Percentage,
}

impl From<DistributionScale> for HistogramScale {
    fn from(value: DistributionScale) -> Self {
        match value { DistributionScale::Count => Self::Count, DistributionScale::Percentage => Self::Percentage }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DistributionDefinition {
    pub vector: ChartVectorReference,
    #[serde(default)]
    pub bins: DistributionBins,
    #[serde(default)]
    pub scale: DistributionScale,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum HistogramBins {
    Auto,
    Count(usize),
    Width(f64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HistogramScale {
    Count,
    Percentage,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum BinBounds {
    Value(f64),
    Interval {
        lower: BigRational,
        upper: BigRational,
        upper_inclusive: bool,
    },
}

impl BinBounds {
    /// A compact label; approximate bounds are explicitly marked. Membership
    /// always uses the exact rational edges stored on the interval.
    pub(crate) fn label(&self) -> String {
        match self {
            Self::Value(value) => format!("Value {value:e}"),
            Self::Interval { lower, upper, upper_inclusive } => format!(
                "[{}, {}{}",
                edge_label(lower),
                edge_label(upper),
                if *upper_inclusive { "]" } else { ")" },
            ),
        }
    }
}

fn edge_label(edge: &BigRational) -> String {
    match edge.to_f64() {
        Some(value) if value.is_finite() => {
            let exact = BigRational::from_float(value).as_ref() == Some(edge);
            format!("{}{value:e}", if exact { "" } else { "≈" })
        }
        _ if edge < &BigRational::zero() => "below finite scalar range".into(),
        _ => "above finite scalar range".into(),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HistogramBin {
    pub bounds: BinBounds,
    pub count: usize,
    pub cumulative_count: usize,
}

impl HistogramBin {
    pub(crate) fn percentage(&self, total: usize) -> f64 {
        if total == 0 { 0.0 } else { self.count as f64 * 100.0 / total as f64 }
    }

    pub(crate) fn height(&self, total: usize, scale: HistogramScale) -> f64 {
        match scale {
            HistogramScale::Count => self.count as f64,
            HistogramScale::Percentage => self.percentage(total),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EcdfStep {
    pub threshold: f64,
    pub count: usize,
    pub cumulative_count: usize,
}

impl EcdfStep {
    pub(crate) fn rate(&self, total: usize) -> f64 {
        if total == 0 { 0.0 } else { self.count as f64 / total as f64 }
    }

    pub(crate) fn cumulative_rate(&self, total: usize) -> f64 {
        if total == 0 { 0.0 } else { self.cumulative_count as f64 / total as f64 }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DistributionData {
    pub total: usize,
    pub scale: HistogramScale,
    pub bins: Vec<HistogramBin>,
    pub ecdf: Vec<EcdfStep>,
}

impl DistributionData {
    /// Build the empirical CDF without consulting histogram bin settings.
    pub(crate) fn cumulative(values: &[f64]) -> Result<Self, String> {
        if values.len() > MAX_POINTS {
            return Err("Distribution exceeds the vector point limit.".into());
        }
        if values.iter().any(|value| !value.is_finite()) {
            return Err("Distribution requires finite values.".into());
        }
        let mut sorted = values.to_vec();
        sorted.sort_by(|left, right| {
            if left == right { std::cmp::Ordering::Equal } else { left.total_cmp(right) }
        });
        let mut ecdf: Vec<EcdfStep> = Vec::new();
        for value in sorted {
            if let Some(last) = ecdf.last_mut().filter(|last| last.threshold == value) {
                last.count += 1;
                last.cumulative_count += 1;
            } else {
                let cumulative_count = ecdf.last().map_or(1, |last| last.cumulative_count + 1);
                ecdf.push(EcdfStep { threshold: value, count: 1, cumulative_count });
            }
        }
        Ok(Self { total: values.len(), scale: HistogramScale::Count, bins: Vec::new(), ecdf })
    }

    pub(crate) fn build(
        values: &[f64],
        bins: HistogramBins,
        scale: HistogramScale,
    ) -> Result<Self, String> {
        match bins {
            HistogramBins::Count(count) if !(1..=MAX_BINS).contains(&count) =>
                return Err("Bin count must be between 1 and 1024.".into()),
            HistogramBins::Width(width) if !width.is_finite() || width <= 0.0 =>
                return Err("Bin width must be finite and positive.".into()),
            _ => {}
        }
        let cumulative = Self::cumulative(values)?;
        let total = values.len();
        if total == 0 {
            return Ok(Self { scale, ..cumulative });
        }
        let mut sorted = values.to_vec();
        sorted.sort_by(|left, right| {
            if left == right { std::cmp::Ordering::Equal } else { left.total_cmp(right) }
        });
        let ecdf = cumulative.ecdf;
        let minimum = sorted[0];
        let maximum = sorted[total - 1];
        if minimum == maximum {
            return Ok(Self {
                total,
                scale,
                bins: vec![HistogramBin { bounds: BinBounds::Value(values[0]), count: total, cumulative_count: total }],
                ecdf,
            });
        }
        let minimum = exact(minimum)?;
        let span = exact(maximum)? - &minimum;
        let (count, width) = match bins {
            HistogramBins::Auto => {
                let count = (total as f64).sqrt().ceil() as usize;
                (count, &span / BigInt::from(count))
            }
            HistogramBins::Count(count) => (count, &span / BigInt::from(count)),
            HistogramBins::Width(width) => {
                let width = exact(width)?;
                let quotient = &span / &width;
                let mut count = quotient.numer() / quotient.denom();
                if quotient.numer() % quotient.denom() != BigInt::zero() { count += 1; }
                if count > BigInt::from(MAX_BINS) {
                    return Err("The requested width requires more than 1024 bins.".into());
                }
                let count = count.to_usize().ok_or("Invalid derived bin count.")?;
                (count, width)
            }
        };
        let mut counts = vec![0usize; count];
        for value in &sorted {
            let position = (exact(*value)? - &minimum) / &width;
            let index = (position.numer() / position.denom()).to_usize().ok_or("Invalid bin position.")?;
            let index = if index == count && *value == maximum { count - 1 } else { index };
            let bin = counts.get_mut(index).ok_or("Value falls outside the derived bins.")?;
            *bin += 1;
        }
        let mut cumulative = 0;
        let mut output = Vec::with_capacity(count);
        for (index, bin_count) in counts.into_iter().enumerate() {
            cumulative += bin_count;
            output.push(HistogramBin {
                bounds: BinBounds::Interval {
                    lower: &minimum + &width * BigInt::from(index),
                    upper: &minimum + &width * BigInt::from(index + 1),
                    upper_inclusive: index + 1 == count,
                },
                count: bin_count,
                cumulative_count: cumulative,
            });
        }
        Ok(Self { total, scale, bins: output, ecdf })
    }
}

fn exact(value: f64) -> Result<BigRational, String> {
    BigRational::from_float(value).ok_or_else(|| "Distribution requires finite values.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counts(data: &DistributionData) -> Vec<usize> {
        data.bins.iter().map(|bin| bin.count).collect()
    }

    #[test]
    fn distribution_bins_and_ecdf_count_every_value_once() {
        let values = [0.0, 1.0, 2.0, 2.0, 3.0, 4.0];
        let data = DistributionData::build(&values, HistogramBins::Count(2), HistogramScale::Percentage).unwrap();
        assert_eq!(counts(&data), [2, 4]);
        assert_eq!(data.bins[0].cumulative_count, 2);
        assert_eq!(data.bins[1].cumulative_count, 6);
        assert!((data.bins.iter().map(|bin| bin.height(data.total, data.scale)).sum::<f64>() - 100.0).abs() < 1e-12);
        assert!(matches!(&data.bins[0].bounds, BinBounds::Interval { upper_inclusive: false, .. }));
        assert!(matches!(&data.bins[1].bounds, BinBounds::Interval { upper_inclusive: true, .. }));
        assert_eq!(data.ecdf.iter().map(|step| (step.threshold, step.count, step.cumulative_count)).collect::<Vec<_>>(),
            [(0.0, 1, 1), (1.0, 1, 2), (2.0, 2, 4), (3.0, 1, 5), (4.0, 1, 6)]);
        assert_eq!(data.ecdf.last().unwrap().cumulative_rate(data.total), 1.0);
        assert_eq!(DistributionData::build(&values, HistogramBins::Auto, HistogramScale::Count).unwrap().bins.len(), 3);
    }

    #[test]
    fn distribution_width_keeps_exact_edges_and_rejects_excess_bins() {
        let data = DistributionData::build(&[0.0, 1.0, 2.0, 2.5], HistogramBins::Width(1.0), HistogramScale::Count).unwrap();
        assert_eq!(counts(&data), [1, 1, 2]);
        for (index, bin) in data.bins.iter().enumerate() {
            let BinBounds::Interval { lower, upper, upper_inclusive } = &bin.bounds else { panic!("interval expected"); };
            assert_eq!(*lower, BigRational::from_integer(index.into()));
            assert_eq!(*upper, BigRational::from_integer((index + 1).into()));
            assert_eq!(*upper_inclusive, index == 2);
        }
        assert!(DistributionData::build(&[0.0, 1025.0], HistogramBins::Width(1.0), HistogramScale::Count).is_err());
        assert_eq!(DistributionData::build(&[0.0, 1024.0], HistogramBins::Width(1.0), HistogramScale::Count).unwrap().bins.len(), 1024);
    }

    #[test]
    fn distribution_validates_empty_and_constant_inputs() {
        for bins in [HistogramBins::Count(0), HistogramBins::Count(1025), HistogramBins::Width(0.0), HistogramBins::Width(f64::NAN), HistogramBins::Width(f64::INFINITY)] {
            assert!(DistributionData::build(&[], bins, HistogramScale::Count).is_err());
            assert!(DistributionData::build(&[1.0], bins, HistogramScale::Count).is_err());
        }
        let empty = DistributionData::build(&[], HistogramBins::Auto, HistogramScale::Count).unwrap();
        assert!(empty.bins.is_empty() && empty.ecdf.is_empty());
        let constant = DistributionData::build(&[-0.0, 0.0, -0.0], HistogramBins::Count(7), HistogramScale::Count).unwrap();
        assert_eq!(constant.bins.len(), 1);
        assert!(matches!(constant.bins[0].bounds, BinBounds::Value(value) if value.to_bits() == (-0.0_f64).to_bits()));
        assert_eq!(constant.ecdf.len(), 1);
        assert_eq!(constant.ecdf[0].cumulative_count, 3);
        assert!(DistributionData::build(&[f64::NAN], HistogramBins::Auto, HistogramScale::Count).is_err());
        assert!(DistributionData::build(&vec![1.0; MAX_POINTS + 1], HistogramBins::Auto, HistogramScale::Count).is_err());
    }

    #[test]
    fn distribution_exact_edges_handle_extremes_and_adjacent_values() {
        let data = DistributionData::build(&[-f64::MAX, 0.0, f64::MAX], HistogramBins::Count(2), HistogramScale::Count).unwrap();
        assert_eq!(counts(&data), [1, 2]);
        let BinBounds::Interval { upper, .. } = &data.bins[0].bounds else { panic!("interval expected"); };
        assert_eq!(*upper, BigRational::zero());
        let tiny = f64::from_bits(1);
        let data = DistributionData::build(&[0.0, tiny], HistogramBins::Count(4), HistogramScale::Count).unwrap();
        assert_eq!(counts(&data), [1, 0, 0, 1]);
        let BinBounds::Interval { upper, .. } = &data.bins[0].bounds else { panic!("interval expected"); };
        assert_eq!(*upper, exact(tiny).unwrap() / BigInt::from(4));
        assert!(data.bins[0].bounds.label().contains('≈'));
        let data = DistributionData::build(&[f64::MAX / 2.0, f64::MAX], HistogramBins::Width(f64::MAX), HistogramScale::Count).unwrap();
        let BinBounds::Interval { upper, .. } = &data.bins[0].bounds else { panic!("interval expected"); };
        assert!(upper > &exact(f64::MAX).unwrap());
        assert!(data.bins[0].bounds.label().contains("finite scalar range") || data.bins[0].bounds.label().contains('≈'));
    }

    #[test]
    fn cumulative_distribution_is_independent_of_histogram_width() {
        let values = [0.0, 1.0, 1.0, 2048.0];
        assert!(DistributionData::build(&values, HistogramBins::Width(1.0), HistogramScale::Count).is_err());
        let cdf = DistributionData::cumulative(&values).unwrap();
        assert!(cdf.bins.is_empty());
        assert_eq!(cdf.ecdf.iter().map(|step| (step.threshold, step.count, step.cumulative_count)).collect::<Vec<_>>(), [(0.0, 1, 1), (1.0, 2, 3), (2048.0, 1, 4)]);
        assert!(DistributionData::cumulative(&[f64::NAN]).is_err());
    }
}
