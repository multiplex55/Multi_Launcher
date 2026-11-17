//! Bounded statistics over the Pad's finite numeric vectors.

use super::document::{CalculationValue, DiagnosticKind, script::WorkBudget};

#[derive(Clone, Copy, Debug)]
pub(super) enum VectorStatistic {
    Quantile,
    Percentile,
    VariancePopulation,
    VarianceSample,
    StdevSample,
}

impl VectorStatistic {
    pub(super) fn from_name(name: &str) -> Option<Self> {
        Some(match name.to_ascii_lowercase().as_str() {
            "quantile" => Self::Quantile,
            "percentile" => Self::Percentile,
            "variance_population" => Self::VariancePopulation,
            "variance_sample" => Self::VarianceSample,
            "stdev_sample" => Self::StdevSample,
            _ => return None,
        })
    }

    pub(super) fn call(
        self,
        arguments: &[CalculationValue],
        work: &mut WorkBudget,
    ) -> Result<CalculationValue, DiagnosticKind> {
        let Some(CalculationValue::Vector(vector)) = arguments.first() else {
            return Err(error("statistic requires a numeric vector"));
        };
        let values = vector.components();
        let result = match (self, arguments) {
            (Self::Quantile | Self::Percentile, [_, CalculationValue::Scalar(probability)])
                if probability.is_finite() =>
            {
                let upper = if matches!(self, Self::Percentile) { 100.0 } else { 1.0 };
                if !(0.0..=upper).contains(probability) {
                    return Err(error("probability is outside the supported range"));
                }
                let quantile = if matches!(self, Self::Percentile) {
                    probability / 100.0
                } else {
                    *probability
                };
                quantile_value(values, quantile, work)?
            }
            (Self::VariancePopulation, [_]) => dispersion(values, false, false, work)?,
            (Self::VarianceSample, [_]) => dispersion(values, true, false, work)?,
            (Self::StdevSample, [_]) => dispersion(values, true, true, work)?,
            _ => return Err(error("invalid statistic arguments")),
        };
        Ok(CalculationValue::Scalar(result))
    }
}

fn error(message: &str) -> DiagnosticKind {
    DiagnosticKind::Script(message.to_owned())
}

fn finite(value: f64) -> Result<f64, DiagnosticKind> {
    if value.is_finite() { Ok(value) } else { Err(DiagnosticKind::InvalidArithmetic) }
}

fn quantile_value(values: &[f64], q: f64, work: &mut WorkBudget) -> Result<f64, DiagnosticKind> {
    if values.is_empty() {
        return Err(error("quantile requires a nonempty vector"));
    }
    let levels = (usize::BITS - values.len().leading_zeros()) as usize;
    work.charge_vector(values.len().saturating_mul(levels + 2))?;
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    if q == 0.0 { return Ok(sorted[0]); }
    if q == 1.0 { return Ok(sorted[sorted.len() - 1]); }
    let rank = (sorted.len() - 1) as f64 * q;
    let lower = rank.floor() as usize;
    let fraction = rank - lower as f64;
    if fraction == 0.0 { return Ok(sorted[lower]); }
    let a = sorted[lower];
    let b = sorted[lower + 1];
    if a.is_sign_negative() != b.is_sign_negative() {
        finite(a * (1.0 - fraction) + b * fraction)
    } else {
        finite(a + (b - a) * fraction)
    }
}

fn dispersion(
    values: &[f64],
    sample: bool,
    standard_deviation: bool,
    work: &mut WorkBudget,
) -> Result<f64, DiagnosticKind> {
    if values.len() < if sample { 2 } else { 1 } {
        return Err(error(if sample {
            "sample statistic requires at least two values"
        } else {
            "population statistic requires a nonempty vector"
        }));
    }
    work.charge_vector(values.len())?;
    let (minimum, maximum) = values.iter().copied().fold(
        (f64::INFINITY, f64::NEG_INFINITY),
        |(minimum, maximum), value| (minimum.min(value), maximum.max(value)),
    );
    let center = minimum.midpoint(maximum);
    let scale = (minimum - center).abs().max((maximum - center).abs());
    if scale == 0.0 { return Ok(0.0); }

    work.charge_vector(values.len())?;
    let mean = values.iter().map(|value| (value - center) / scale).sum::<f64>() / values.len() as f64;
    work.charge_vector(values.len())?;
    let squared = values.iter().map(|value| {
        let deviation = (value - center) / scale - mean;
        deviation * deviation
    }).sum::<f64>();
    let divisor = (values.len() - usize::from(sample)) as f64;
    let deviation = finite((squared / divisor).sqrt() * scale)?;
    if standard_deviation { Ok(deviation) } else { finite(deviation * deviation) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calculation_pad::vector::CalculationVector;
    use std::sync::{Arc, atomic::AtomicBool};

    fn vector(values: &[f64]) -> CalculationValue {
        CalculationValue::Vector(CalculationVector::new(values.to_vec()).unwrap())
    }

    fn value(operation: VectorStatistic, arguments: &[CalculationValue]) -> f64 {
        let CalculationValue::Scalar(result) = operation.call(arguments, &mut WorkBudget::default()).unwrap() else {
            panic!("expected scalar");
        };
        result
    }

    #[test]
    fn univariate_statistics_are_stable_for_extreme_and_offset_values() {
        let extremes = vector(&[-f64::MAX, f64::MAX]);
        assert_eq!(value(VectorStatistic::Quantile, &[extremes.clone(), CalculationValue::Scalar(0.5)]), 0.0);
        assert_eq!(value(VectorStatistic::Quantile, &[extremes.clone(), CalculationValue::Scalar(0.0)]), -f64::MAX);
        assert_eq!(value(VectorStatistic::Quantile, &[extremes.clone(), CalculationValue::Scalar(1.0)]), f64::MAX);
        assert!(matches!(VectorStatistic::VariancePopulation.call(&[extremes], &mut WorkBudget::default()), Err(DiagnosticKind::InvalidArithmetic)));
        let wide = vector(&[-1e200, 1e200]);
        assert!((value(VectorStatistic::StdevSample, &[wide.clone()]) / 1e200 - 2.0_f64.sqrt()).abs() < 1e-14);
        assert!(matches!(VectorStatistic::VarianceSample.call(&[wide], &mut WorkBudget::default()), Err(DiagnosticKind::InvalidArithmetic)));
        let offset = vector(&[1e12 + 1.0, 1e12 + 2.0, 1e12 + 3.0, 1e12 + 4.0]);
        assert!((value(VectorStatistic::VariancePopulation, &[offset.clone()]) - 1.25).abs() < 1e-12);
        assert!((value(VectorStatistic::VarianceSample, &[offset]) - 5.0 / 3.0).abs() < 1e-12);
    }

    #[test]
    fn univariate_statistics_charge_work_and_cancel() {
        let vector = vector(&vec![1.0; 1024]);
        let arguments = [vector, CalculationValue::Scalar(0.5)];
        let mut work = WorkBudget::default();
        work.charge_vector(super::super::document::script::MAX_VECTOR_COMPONENT_WORK - 1).unwrap();
        assert!(matches!(VectorStatistic::Quantile.call(&arguments, &mut work), Err(DiagnosticKind::WorkLimit)));
        let mut work = WorkBudget::with_cancellation(Arc::new(AtomicBool::new(true)));
        assert!(matches!(VectorStatistic::Quantile.call(&arguments, &mut work), Err(DiagnosticKind::Canceled)));
    }
}
