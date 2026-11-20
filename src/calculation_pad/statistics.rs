//! Bounded statistics over the Pad's finite numeric vectors.

use super::document::{CalculationValue, DiagnosticKind, script::WorkBudget};
use num_rational::BigRational;
use num_traits::{ToPrimitive, Zero};

#[derive(Clone, Copy, Debug)]
pub(super) enum VectorStatistic {
    Quantile,
    Percentile,
    VariancePopulation,
    VarianceSample,
    StdevSample,
    CovariancePopulation,
    CovarianceSample,
    Correlation,
    RegressionSlope,
    RegressionIntercept,
    RegressionR2,
    PredictLinear,
}

impl VectorStatistic {
    pub(super) fn from_name(name: &str) -> Option<Self> {
        Some(match name.to_ascii_lowercase().as_str() {
            "quantile" => Self::Quantile,
            "percentile" => Self::Percentile,
            "variance_population" => Self::VariancePopulation,
            "variance_sample" => Self::VarianceSample,
            "stdev_sample" => Self::StdevSample,
            "covariance_population" => Self::CovariancePopulation,
            "covariance_sample" => Self::CovarianceSample,
            "correlation" => Self::Correlation,
            "regression_slope" => Self::RegressionSlope,
            "regression_intercept" => Self::RegressionIntercept,
            "regression_r2" => Self::RegressionR2,
            "predict_linear" => Self::PredictLinear,
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
            (Self::PredictLinear, [_, CalculationValue::Vector(y), CalculationValue::Scalar(x_new)])
                if x_new.is_finite() => paired(self, values, y.components(), Some(*x_new), work)?,
            (Self::CovariancePopulation | Self::CovarianceSample | Self::Correlation
                | Self::RegressionSlope | Self::RegressionIntercept | Self::RegressionR2,
                [_, CalculationValue::Vector(y)]) =>
                paired(self, values, y.components(), None, work)?,
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
    let axis = CenteredAxis::new(values, work)?;
    if axis.scale == 0.0 { return Ok(0.0); }
    work.charge_vector(values.len())?;
    let squared = values.iter().map(|value| {
        let deviation = axis.deviation(*value);
        deviation * deviation
    }).sum::<f64>();
    let divisor = (values.len() - usize::from(sample)) as f64;
    let deviation = finite((squared / divisor).sqrt() * axis.scale)?;
    if standard_deviation { Ok(deviation) } else { finite(deviation * deviation) }
}

struct CenteredAxis {
    center: f64,
    scale: f64,
    mean: f64,
}

impl CenteredAxis {
    fn new(values: &[f64], work: &mut WorkBudget) -> Result<Self, DiagnosticKind> {
        work.charge_vector(values.len())?;
        let (minimum, maximum) = values.iter().copied().fold(
            (f64::INFINITY, f64::NEG_INFINITY),
            |(minimum, maximum), value| (minimum.min(value), maximum.max(value)),
        );
        let center = minimum.midpoint(maximum);
        let scale = (minimum - center).abs().max((maximum - center).abs());
        if scale == 0.0 {
            return Ok(Self { center, scale, mean: 0.0 });
        }
        work.charge_vector(values.len())?;
        let mut sum = 0.0;
        let mut correction = 0.0;
        for &value in values {
            compensated_add(&mut sum, &mut correction, (value - center) / scale);
        }
        let mean = (sum + correction) / values.len() as f64;
        Ok(Self { center, scale, mean })
    }

    fn deviation(&self, value: f64) -> f64 {
        if self.scale == 0.0 { 0.0 } else { (value - self.center) / self.scale - self.mean }
    }

}

fn rational(value: f64) -> Result<BigRational, DiagnosticKind> {
    BigRational::from_float(value).ok_or(DiagnosticKind::InvalidArithmetic)
}

fn rational_float(value: BigRational) -> Result<f64, DiagnosticKind> {
    finite(value.to_f64().ok_or(DiagnosticKind::InvalidArithmetic)?)
}

// A compensated sum limits cancellation in the centered cross products.
fn compensated_add(sum: &mut f64, correction: &mut f64, value: f64) {
    let next = *sum + value;
    if sum.abs() >= value.abs() {
        *correction += (*sum - next) + value;
    } else {
        *correction += (value - next) + *sum;
    }
    *sum = next;
}

fn exact_cross(x: &[f64], y: &[f64], work: &mut WorkBudget) -> Result<BigRational, DiagnosticKind> {
    let mut x_sum = BigRational::zero();
    let mut y_sum = BigRational::zero();
    let mut products = BigRational::zero();
    for (left, right) in x.iter().zip(y) {
        work.charge_vector(8)?;
        let left = rational(*left)?;
        let right = rational(*right)?;
        products += &left * &right;
        x_sum += left;
        y_sum += right;
    }
    Ok(products - x_sum * y_sum / BigRational::from_integer(x.len().into()))
}

fn exact_mean(values: &[f64], work: &mut WorkBudget) -> Result<BigRational, DiagnosticKind> {
    let mut sum = BigRational::zero();
    for &value in values {
        work.charge_vector(4)?;
        sum += rational(value)?;
    }
    Ok(sum / BigRational::from_integer(values.len().into()))
}

fn paired(
    operation: VectorStatistic,
    x: &[f64],
    y: &[f64],
    x_new: Option<f64>,
    work: &mut WorkBudget,
) -> Result<f64, DiagnosticKind> {
    if x.len() != y.len() {
        return Err(error("paired vectors must have matching lengths"));
    }
    let population = matches!(operation, VectorStatistic::CovariancePopulation);
    if x.len() < if population { 1 } else { 2 } {
        return Err(error(if population {
            "population covariance requires at least one pair"
        } else {
            "paired statistic requires at least two pairs"
        }));
    }
    let x_axis = CenteredAxis::new(x, work)?;
    let y_axis = CenteredAxis::new(y, work)?;
    let covariance = matches!(operation, VectorStatistic::CovariancePopulation | VectorStatistic::CovarianceSample);
    if covariance && (x_axis.scale == 0.0 || y_axis.scale == 0.0) {
        return Ok(0.0);
    }
    if x_axis.scale == 0.0 {
        return Err(error("statistic requires nonconstant X values"));
    }
    if y_axis.scale == 0.0 {
        return match operation {
            VectorStatistic::RegressionSlope => Ok(0.0),
            VectorStatistic::RegressionIntercept | VectorStatistic::PredictLinear => Ok(y_axis.center),
            _ => Err(error("statistic requires nonconstant Y values")),
        };
    }

    work.charge_vector(x.len().saturating_mul(3))?;
    let mut xx = 0.0;
    let mut yy = 0.0;
    let mut xy = 0.0;
    let mut correction = 0.0;
    let mut lost_precision = false;
    for (&left, &right) in x.iter().zip(y) {
        let dx = x_axis.deviation(left);
        let dy = y_axis.deviation(right);
        xx += dx * dx;
        yy += dy * dy;
        let product = dx * dy;
        lost_precision |= (dx != 0.0 && dy != 0.0 && (product == 0.0 || product.is_subnormal()))
            || (left != x_axis.center && (left - x_axis.center) / x_axis.scale == 0.0)
            || (right != y_axis.center && (right - y_axis.center) / y_axis.scale == 0.0);
        compensated_add(&mut xy, &mut correction, product);
    }
    xy += correction;
    let cross = if xy == 0.0 || lost_precision {
        let exact = exact_cross(x, y, work)?;
        xy = rational_float(exact.clone() / rational(x_axis.scale)? / rational(y_axis.scale)?)?;
        exact
    } else {
        rational(xy)? * rational(x_axis.scale)? * rational(y_axis.scale)?
    };
    if covariance {
        let divisor = x.len() - usize::from(matches!(operation, VectorStatistic::CovarianceSample));
        return rational_float(cross / BigRational::from_integer(divisor.into()));
    }
    if matches!(operation, VectorStatistic::Correlation) {
        return finite((xy / xx.sqrt() / yy.sqrt()).clamp(-1.0, 1.0));
    }
    if matches!(operation, VectorStatistic::RegressionR2) {
        work.charge_vector(x.len().saturating_mul(2))?;
        let coefficient = xy / xx;
        let residual = x.iter().zip(y).map(|(&left, &right)| {
            let value = y_axis.deviation(right) - coefficient * x_axis.deviation(left);
            value * value
        }).sum::<f64>();
        return finite(1.0 - residual / yy);
    }
    let slope = cross / (rational(xx)? * rational(x_axis.scale)? * rational(x_axis.scale)?);
    if matches!(operation, VectorStatistic::RegressionSlope) {
        return rational_float(slope);
    }
    let target = match operation {
        VectorStatistic::RegressionIntercept => BigRational::zero(),
        VectorStatistic::PredictLinear => rational(x_new.ok_or(DiagnosticKind::InvalidExpression)?)?,
        _ => return Err(DiagnosticKind::InvalidExpression),
    };
    rational_float(exact_mean(y, work)? + slope * (target - exact_mean(x, work)?))
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

    #[test]
    fn paired_statistics_handle_scaling_and_degenerate_axes() {
        let x = vector(&[1.0, 2.0, 3.0]);
        let y = vector(&[3.0, 5.0, 7.0]);
        let args = [x.clone(), y.clone()];
        for (operation, expected) in [
            (VectorStatistic::CovariancePopulation, 4.0 / 3.0),
            (VectorStatistic::CovarianceSample, 2.0),
            (VectorStatistic::Correlation, 1.0),
            (VectorStatistic::RegressionSlope, 2.0),
            (VectorStatistic::RegressionIntercept, 1.0),
            (VectorStatistic::RegressionR2, 1.0),
        ] {
            assert!((value(operation, &args) - expected).abs() < 1e-12);
        }
        assert!((value(VectorStatistic::PredictLinear, &[x.clone(), y, CalculationValue::Scalar(4.0)]) - 9.0).abs() < 1e-12);
        let constant = vector(&[7.0, 7.0, 7.0]);
        let args = [x, constant];
        assert_eq!(value(VectorStatistic::CovarianceSample, &args), 0.0);
        assert_eq!(value(VectorStatistic::RegressionSlope, &args), 0.0);
        assert_eq!(value(VectorStatistic::RegressionIntercept, &args), 7.0);
        assert!(VectorStatistic::RegressionR2.call(&args, &mut WorkBudget::default()).is_err());
        assert!(VectorStatistic::Correlation.call(&args, &mut WorkBudget::default()).is_err());

        let steep_x = vector(&[-1e-200, 1e-200]);
        let steep_y = vector(&[-1e200, 1e200]);
        assert!(VectorStatistic::RegressionSlope.call(&[steep_x.clone(), steep_y.clone()], &mut WorkBudget::default()).is_err());
        assert_eq!(value(VectorStatistic::PredictLinear, &[steep_x, steep_y, CalculationValue::Scalar(0.0)]), 0.0);
        let mixed_x = vector(&[1e200, -1e200, 1.0, -1.0]);
        let mixed_y = vector(&[1.0, -1.0, 1e200, -1e200]);
        assert!((value(VectorStatistic::CovariancePopulation, &[mixed_x, mixed_y]) / 1e200 - 1.0).abs() < 1e-12);
        let extreme_x = vector(&[1e308, -1e308, 1e-200, -1e-200]);
        let extreme_y = vector(&[1e-200, -1e-200, 1e308, -1e308]);
        assert!((value(VectorStatistic::CovariancePopulation, &[extreme_x, extreme_y]) / 1e108 - 1.0).abs() < 1e-12);
        let subnormal_x = vector(&[1e308, -1e308, 3e-16, -3e-16]);
        let subnormal_y = vector(&[3e-16, -3e-16, 1e308, -1e308]);
        assert!((value(VectorStatistic::CovariancePopulation, &[subnormal_x, subnormal_y]) / 3e292 - 1.0).abs() < 1e-12);
        let centered_x = vector(&[-1.0, 0.0, 1.0]);
        let cancellation_y = vector(&[1e16, 1.0, -1e16]);
        let intercept = value(VectorStatistic::RegressionIntercept, &[centered_x.clone(), cancellation_y.clone()]);
        let prediction = value(VectorStatistic::PredictLinear, &[centered_x, cancellation_y, CalculationValue::Scalar(0.0)]);
        assert!((intercept - 1.0 / 3.0).abs() < 1e-15);
        assert!((prediction - 1.0 / 3.0).abs() < 1e-15);
    }

    #[test]
    fn paired_statistics_report_invalid_inputs_and_cancellation() {
        let two = vector(&[1.0, 2.0]);
        let one = vector(&[1.0]);
        assert!(VectorStatistic::Correlation.call(&[two.clone(), one.clone()], &mut WorkBudget::default()).is_err());
        assert!(VectorStatistic::CovarianceSample.call(&[one.clone(), one.clone()], &mut WorkBudget::default()).is_err());
        assert_eq!(value(VectorStatistic::CovariancePopulation, &[one.clone(), one]), 0.0);
        let constant = vector(&[1.0, 1.0]);
        assert!(VectorStatistic::RegressionSlope.call(&[constant.clone(), two.clone()], &mut WorkBudget::default()).is_err());
        assert_eq!(value(VectorStatistic::CovariancePopulation, &[constant, two.clone()]), 0.0);
        let mut cancelled = WorkBudget::with_cancellation(Arc::new(AtomicBool::new(true)));
        assert!(matches!(VectorStatistic::Correlation.call(&[two.clone(), two], &mut cancelled), Err(DiagnosticKind::Canceled)));
    }
}
