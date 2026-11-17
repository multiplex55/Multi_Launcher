//! Axis-domain validation and transforms for the shared chart scene.

use num_rational::BigRational;
use num_traits::{One, ToPrimitive, Zero};

use super::ChartView;
use super::options::{AxisBounds, AxisOptions, AxisScale, AxisTicks};

#[derive(Clone, Copy, Debug)]
pub(super) struct AxisTransform {
    scale: AxisScale,
    min: f64,
    max: f64,
}

impl AxisTransform {
    /// Validate all plotted data before selecting a display window: clipping
    /// must never hide invalid log coordinates.
    pub(super) fn new(
        options: &AxisOptions,
        values: impl IntoIterator<Item = f64>,
    ) -> Result<Self, &'static str> {
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        for value in values {
            if !value.is_finite() {
                return Err("Chart coordinates must be finite.");
            }
            if options.scale == AxisScale::Log10 && value <= 0.0 {
                return Err("Logarithmic chart coordinates must be positive.");
            }
            min = min.min(value);
            max = max.max(value);
        }
        if !min.is_finite() {
            return Err("There are no plottable points.");
        }
        if let AxisBounds::Fixed {
            min: lower,
            max: upper,
        } = options.bounds
        {
            min = lower.get();
            max = upper.get();
            if min >= max {
                return Err("Chart bounds must increase.");
            }
            if options.scale == AxisScale::Log10 && min <= 0.0 {
                return Err("Logarithmic chart bounds must be positive.");
            }
        }
        Ok(Self {
            scale: options.scale,
            min,
            max,
        })
    }

    fn log_distance(self, value: f64) -> f64 {
        // ln_1p retains the distinction between adjacent positive
        // floats even when their independently computed logs coincide.
        let ratio = (value - self.min) / self.min;
        if ratio.is_finite() && ratio > -0.5 {
            ratio.ln_1p()
        } else {
            value.ln() - self.min.ln()
        }
    }

    /// Position in the selected domain, before pan, zoom and clipping.
    pub(super) fn position(self, value: f64) -> f64 {
        if self.min == self.max {
            return 0.5;
        }
        match self.scale {
            AxisScale::Linear => {
                let width = self.max - self.min;
                if width.is_finite() {
                    (value - self.min) / width
                } else {
                    let scale = self.min.abs().max(self.max.abs());
                    (value / scale - self.min / scale) / (self.max / scale - self.min / scale)
                }
            }
            AxisScale::Log10 => self.log_distance(value) / self.log_distance(self.max),
        }
    }

    fn value_at(self, position: f64) -> f64 {
        if position == 0.0 || self.min == self.max {
            return self.min;
        }
        if position == 1.0 {
            return self.max;
        }
        match self.scale {
            AxisScale::Linear => {
                let span = self.max - self.min;
                if span.is_finite() {
                    span.mul_add(position, self.min)
                } else {
                    ((1.0 - position) * (self.min * 0.5) + position * (self.max * 0.5)) * 2.0
                }
            }
            AxisScale::Log10 => {
                let distance = position * self.log_distance(self.max);
                if distance.abs() < 0.5 {
                    self.min.mul_add(distance.exp_m1(), self.min)
                } else {
                    (self.min.ln() + distance).exp()
                }
            }
        }
    }

    fn position_exact(self, value: f64) -> Option<BigRational> {
        if self.min == self.max {
            return BigRational::from_float(0.5);
        }
        if self.scale == AxisScale::Log10 {
            return BigRational::from_float(self.position(value));
        }
        let lo = BigRational::from_float(self.min)?;
        let hi = BigRational::from_float(self.max)?;
        Some((BigRational::from_float(value)? - &lo) / (hi - lo))
    }
}

#[derive(Clone, Copy)]
pub(super) struct PlotAxes {
    transforms: [AxisTransform; 2],
    zoom: [f64; 2],
    pan: [f64; 2],
}

impl PlotAxes {
    pub(super) fn new(transforms: [AxisTransform; 2], view: ChartView, inset: f64) -> Self {
        Self {
            transforms,
            zoom: [view.zoom_x * (1.0 - 2.0 * inset), view.zoom_y],
            pan: [view.pan_x, view.pan_y],
        }
    }

    pub(super) fn position(self, axis: usize, value: f64) -> f64 {
        (self.transforms[axis].position(value) - 0.5) * self.zoom[axis] + 0.5 + self.pan[axis]
    }

    fn value_at(self, axis: usize, screen: f64) -> f64 {
        self.transforms[axis].value_at((screen - 0.5 - self.pan[axis]) / self.zoom[axis] + 0.5)
    }

    fn point_exact(self, axis: usize, value: f64) -> Option<BigRational> {
        let half = BigRational::from_float(0.5)?;
        Some(
            (self.transforms[axis].position_exact(value)? - &half)
                * BigRational::from_float(self.zoom[axis])?
                + half
                + BigRational::from_float(self.pan[axis])?,
        )
    }

    pub(super) fn clip_segment(
        self,
        start: [f64; 2],
        end: [f64; 2],
        exact_budget: &mut usize,
    ) -> Result<Option<([f64; 2], [f64; 2])>, &'static str> {
        let a = [self.position(0, start[0]), self.position(1, start[1])];
        let b = [self.position(0, end[0]), self.position(1, end[1])];
        if (0..2).any(|axis| (a[axis] < 0.0 && b[axis] < 0.0) || (a[axis] > 1.0 && b[axis] > 1.0)) {
            return Ok(None);
        }
        if a.into_iter()
            .chain(b)
            .all(|coordinate| coordinate.abs() <= 1_048_576.0)
        {
            let mut lo = 0.0_f64;
            let mut hi = 1.0_f64;
            for axis in 0..2 {
                let delta = b[axis] - a[axis];
                if delta != 0.0 {
                    let first = -a[axis] / delta;
                    let last = (1.0 - a[axis]) / delta;
                    lo = lo.max(first.min(last));
                    hi = hi.min(first.max(last));
                }
            }
            let at = |t| {
                std::array::from_fn(|axis| {
                    ((b[axis] - a[axis]).mul_add(t, a[axis])).clamp(0.0, 1.0)
                })
            };
            return Ok((lo <= hi).then(|| (at(lo), at(hi))));
        }

        // A very narrow display window can make the two intersections round
        // to the same parameter in f64. Use exact linear coordinates for the
        // bounded set of crossing segments that need them.
        *exact_budget = exact_budget
            .checked_sub(1)
            .ok_or("Chart has too many extreme crossings for this view.")?;
        let project = || -> Option<_> {
            let a = [
                self.point_exact(0, start[0])?,
                self.point_exact(1, start[1])?,
            ];
            let b = [self.point_exact(0, end[0])?, self.point_exact(1, end[1])?];
            let delta = [&b[0] - &a[0], &b[1] - &a[1]];
            let mut lo = BigRational::zero();
            let mut hi = BigRational::one();
            for axis in 0..2 {
                if delta[axis].is_zero() {
                    continue;
                }
                let first = -&a[axis] / &delta[axis];
                let last = (BigRational::one() - &a[axis]) / &delta[axis];
                let (first, last) = if first <= last {
                    (first, last)
                } else {
                    (last, first)
                };
                lo = lo.max(first);
                hi = hi.min(last);
            }
            if lo > hi {
                return Some(None);
            }
            let at = |t: &BigRational| -> Option<[f64; 2]> {
                Some([
                    (&a[0] + &delta[0] * t).to_f64()?.clamp(0.0, 1.0),
                    (&a[1] + &delta[1] * t).to_f64()?.clamp(0.0, 1.0),
                ])
            };
            Some(Some((at(&lo)?, at(&hi)?)))
        };
        project().ok_or("Chart transform is outside the finite range.")
    }

    pub(super) fn ticks(
        self,
        axis: usize,
        mode: AxisTicks,
    ) -> Result<Vec<(f64, f64)>, &'static str> {
        const MAX_TICKS: usize = 64;
        let at = |position| (position, self.value_at(axis, position));
        if self.transforms[axis].min == self.transforms[axis].max {
            return Ok(vec![at(self.position(axis, self.transforms[axis].min))]);
        }
        let (low, high) = (self.value_at(axis, 0.0), self.value_at(axis, 1.0));
        let mut ticks = Vec::new();
        let mut push = |value| {
            let position = self.position(axis, value);
            if (0.0..=1.0).contains(&position) {
                ticks.push((position, value));
            }
        };
        match mode {
            AxisTicks::Auto => return Ok(vec![at(0.0), at(1.0)]),
            AxisTicks::Density { count } => {
                let last = f64::from(count - 1);
                return Ok((0..count)
                    .map(|index| at(f64::from(index) / last))
                    .collect());
            }
            AxisTicks::LinearStep { step } => {
                let step = step.get();
                let first = (low / step).ceil();
                let last = (high / step).floor();
                let count = (last - first + 1.0).max(0.0);
                if !first.is_finite()
                    || !last.is_finite()
                    || count > MAX_TICKS as f64
                    || (count > 1.0 && first.abs().max(last.abs()) >= 4_503_599_627_370_496.0)
                {
                    return Err("Linear tick interval is too small for the displayed range.");
                }
                for index in 0..count as usize {
                    push((first + index as f64) * step);
                }
            }
            AxisTicks::LogDecades | AxisTicks::LogSubdivisions { .. } => {
                if low <= 0.0 || !low.is_finite() || !high.is_finite() {
                    return Err("Logarithmic ticks are outside the finite display range.");
                }
                let first = low.log10().floor() as i32;
                let last = high.log10().floor() as i32;
                let count = match mode {
                    AxisTicks::LogSubdivisions { count } => count,
                    _ => 1,
                };
                if (last - first + 1) as usize * usize::from(count) > MAX_TICKS {
                    return Err("Too many logarithmic ticks for the displayed range.");
                }
                for exponent in first..=last {
                    for index in 0..count {
                        let factor = 1.0 + 9.0 * f64::from(index) / f64::from(count);
                        // Scaling before the final multiply also covers the
                        // decade just below the least positive f64.
                        let value = if exponent < -308 {
                            (10.0_f64.powi(exponent + 16) * factor) * 1e-16
                        } else {
                            10.0_f64.powi(exponent) * factor
                        };
                        if value.is_finite() && value >= low && value <= high {
                            push(value);
                        }
                    }
                }
            }
        }
        Ok(ticks)
    }
}

#[cfg(test)]
mod tests {
    use super::super::options::FiniteValue;
    use super::*;

    fn fixed(scale: AxisScale, min: f64, max: f64) -> AxisOptions {
        AxisOptions {
            scale,
            bounds: AxisBounds::Fixed {
                min: FiniteValue::try_from(min).unwrap(),
                max: FiniteValue::try_from(max).unwrap(),
            },
            ..AxisOptions::default()
        }
    }

    #[test]
    fn axis_transform_handles_extremes_and_adjacent_values() {
        let axis = AxisTransform::new(&fixed(AxisScale::Linear, -1e308, 1e308), [0.0]).unwrap();
        assert_eq!(axis.position(0.0), 0.5);
        let min = 1.0_f64;
        let max = f64::from_bits(min.to_bits() + 1);
        for scale in [AxisScale::Linear, AxisScale::Log10] {
            let axis = AxisTransform::new(&fixed(scale, min, max), [min, max]).unwrap();
            assert_eq!(axis.position(min), 0.0);
            assert_eq!(axis.position(max), 1.0);
        }
        let axis = AxisTransform::new(&fixed(AxisScale::Log10, 1e-300, 1e300), [1.0]).unwrap();
        assert!((axis.position(1.0) - 0.5).abs() < 1e-12);
    }

    #[test]
    fn axis_transform_validates_data_before_fixed_bounds() {
        let options = fixed(AxisScale::Log10, 1.0, 10.0);
        assert!(AxisTransform::new(&options, [0.0, 2.0]).is_err());
        assert!(AxisTransform::new(&options, [-1.0, 2.0]).is_err());
        assert!(AxisTransform::new(&options, [f64::NAN]).is_err());
        let axis = AxisTransform::new(&AxisOptions::default(), [3.0]).unwrap();
        assert_eq!(axis.position(3.0), 0.5);
    }
}
