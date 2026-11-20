use super::super::examples::{CalculationExample, ExampleExpectation};
use super::{ReferenceCategory, ReferenceEntry, ReferenceId};
use crate::unit_conversion::Unit;

const NUMBER: &str = "Finite ordinary dimensionless scalar.";
const NONFINITE: &str = "Nonfinite results report an error.";
const NUMBERS: &str = "An ordinary scalar, an approximate scalar if a floating operand is approximate, or an exact number when an exact, radix or typed integer operand selects exact arithmetic.";
const EXACT_PATH: &str = "The exact path accepts only integral ordinary scalars alongside exact, radix or typed integer values. It rejects approximate inputs. All-decimal results must terminate. Nonfinite results and size or work limit violations report an error.";
const INTEGER_PAIR: &str = "Nonnegative integers n and k, with k ≤ n. Integral ordinary scalars, radix-conversion values, typed integers and exact integers are accepted.";
const COUNT_RESULT: &str = "An ordinary scalar for ordinary arguments and results at most 2^53. Other valid results are exact integer-valued numbers.";
const COUNT_LIMITS: &str =
    "At most 10,000 steps and a 4,096-bit result, within the normal evaluation budget.";
const COLLECTION: &str = "A vector or range sequence of ordinary scalars.";
const NONEMPTY_COLLECTION: &str =
    "An empty collection or nonfinite intermediate result reports an error.";
const STATS_VECTOR: &str = "An ordinary numeric vector with 1–1,024 elements.";
const SAMPLE_VECTOR: &str = "An ordinary numeric vector with 2–1,024 elements.";
const PAIRS: &str = "Two equally sized ordinary numeric vectors.";
const FIT: &str =
    "At least two pairs and variation in X are required. Nonfinite results report an error.";
const EXACT_LIMITS: &str =
    "4,096-bit coefficient, numerator and denominator limits and a 1,000-digit scale limit apply.";
const WORD: &str =
    "Typed integer, or a source integer literal when the integer mode can be inferred.";
const CONSTRUCTOR: &str = "Integer literal or expression, typed integer, exact radix-conversion result, or finite integral ordinary scalar reference within ±2^53.";
const INTEGER_RANGE: &str = "The operand must fit the target width and signedness. Decimal-point and scientific-notation literal tokens are rejected.";
const SHIFT: &str = "Integer value and a nonnegative integral count, reduced modulo its width.";
const FIELD: &str = "Integer value and nonnegative integral start/count, with start + count at most the width. Bit zero is the least significant.";
const DATE_SHIFT: &str = "Date, offset timestamp or named-zone timestamp, with an exact integer shift fitting signed 64 bits. A quoted earlier or later fold choice is accepted only for named zones.";
const ZONE_LIMITS: &str = "A DST gap reports an error. A DST fold requires earlier or later when the target local time is ambiguous. Out-of-range values report an error.";

macro_rules! call {
    ($name:literal, $group:ident, $summary:expr, $parameters:expr, $returns:expr, $limits:expr,
        $source:literal => $expected:expr, [$($keyword:literal),*], [$($related:literal),*] $(, $variants:expr)?) => {
        ReferenceEntry {
            id: ReferenceId::Call($name),
            category: ReferenceCategory::$group,
            spelling: None,
            aliases: &[],
            concepts: &[$($keyword),*],
            summary: $summary,
            parameters: $parameters,
            returns: $returns,
            limits: $limits,
            example: CalculationExample::new($name, $source, $summary, $expected),
            variants: call!(@variants $($variants)?),
            unit_category: None,
            related: &[$(ReferenceId::Call($related)),*],
        }
    };
    (@variants $variants:expr) => { $variants };
    (@variants) => { &[] };
}

macro_rules! math {
    ($name:literal, $summary:literal, $domain:expr, $limits:expr,
        $source:literal => $expected:expr, [$($keyword:literal),*], [$($related:literal),*]) => {
        call!($name, Mathematics, $summary, $domain, NUMBER, $limits, $source => Scalar($expected), [$($keyword),*], [$($related),*])
    };
}

macro_rules! integer {
    ($name:literal, $description:literal, $source:literal => $mode:literal, $value:literal) => {
        call!($name, ProgrammerExact, $description, CONSTRUCTOR, $description, INTEGER_RANGE,
            $source => ExactInteger { mode: $mode, value: $value }, ["fixed-width integer", "cast"], ["fraction"])
    };
}

use ExampleExpectation::{ApproximateScalar, Calendar, Exact, ExactInteger, Physical, Scalar};

pub(super) const FUNCTIONS: &[ReferenceEntry] = &[
    math!("abs", "Return the magnitude.", NUMBER, NONFINITE, "abs(-5)" => 5.0, ["absolute value", "magnitude"], ["signum"]),
    math!("signum", "Return 1 for a positive sign bit and -1 for a negative sign bit, including signed zero.", NUMBER, NONFINITE, "signum(-5)" => -1.0, ["sign"], ["abs"]),
    math!("sin", "Return the sine of a radian angle.", "Radian angle as a finite ordinary scalar.", NONFINITE, "sin(PI / 2)" => 1.0, ["sine", "radians"], ["asin", "cos"]),
    math!("cos", "Return the cosine of a radian angle.", "Radian angle as a finite ordinary scalar.", NONFINITE, "cos(0)" => 1.0, ["cosine", "radians"], ["acos", "sin"]),
    math!("tan", "Return the tangent of a radian angle.", "Radian angle as a finite ordinary scalar.", "Near odd multiples of π/2, floating-point results can be very large.", "tan(PI / 4)" => 1.0, ["tangent", "radians"], ["atan", "atan2"]),
    math!("asin", "Return the principal inverse sine in radians.", "Ordinary scalar in [-1, 1].", "Outside [-1, 1] reports an error.", "asin(1)" => std::f64::consts::FRAC_PI_2, ["arcsine"], ["sin"]),
    math!("acos", "Return the principal inverse cosine in radians.", "Ordinary scalar in [-1, 1].", "Outside [-1, 1] reports an error.", "acos(-1)" => std::f64::consts::PI, ["arccosine"], ["cos"]),
    math!("atan", "Return the principal inverse tangent in radians.", NUMBER, NONFINITE, "atan(1)" => std::f64::consts::FRAC_PI_4, ["arctangent"], ["tan", "atan2"]),
    math!("atan2", "Return the signed angle of (x, y) in radians, using both signs to determine the quadrant.", "Two finite ordinary scalars, ordered y then x. Also available infix.", "Results are in [-π, π]. Signed zeros affect the boundary.", "atan2(1, 0)" => std::f64::consts::FRAC_PI_2, ["bearing", "quadrant", "angle"], ["atan"]),
    math!("sinh", "Return the hyperbolic sine.", NUMBER, NONFINITE, "sinh(0)" => 0.0, ["hyperbolic sine"], ["asinh", "cosh"]),
    math!("cosh", "Return the hyperbolic cosine.", NUMBER, NONFINITE, "cosh(0)" => 1.0, ["hyperbolic cosine"], ["acosh", "sinh"]),
    math!("tanh", "Return the hyperbolic tangent.", NUMBER, NONFINITE, "tanh(0)" => 0.0, ["hyperbolic tangent"], ["atanh", "sinh"]),
    math!("asinh", "Return the inverse hyperbolic sine.", NUMBER, NONFINITE, "asinh(0)" => 0.0, ["inverse hyperbolic sine"], ["sinh"]),
    math!("acosh", "Return the nonnegative inverse hyperbolic cosine.", "Ordinary scalar at least 1.", "Inputs below 1 report an error.", "acosh(1)" => 0.0, ["inverse hyperbolic cosine"], ["cosh"]),
    math!("atanh", "Return the inverse hyperbolic tangent.", "Ordinary scalar strictly between -1 and 1.", "Inputs at or outside the endpoints report an error.", "atanh(0)" => 0.0, ["inverse hyperbolic tangent"], ["tanh"]),
    math!("floor", "Round down to the nearest integer.", NUMBER, NONFINITE, "floor(-1.2)" => -2.0, ["round down"], ["ceil", "floor_to"]),
    math!("round", "Round to an integer, with ties away from zero.", NUMBER, NONFINITE, "round(-2.5)" => -3.0, ["nearest integer", "rounding"], ["round_to", "trunc"]),
    math!("ceil", "Round up to the nearest integer.", NUMBER, NONFINITE, "ceil(-1.2)" => -1.0, ["ceiling", "round up"], ["floor", "ceil_to"]),
    math!("trunc", "Discard the fractional part, toward zero.", NUMBER, NONFINITE, "trunc(-1.75)" => -1.0, ["truncate", "integer part"], ["fract", "floor"]),
    math!("fract", "Return the fractional part with the input's sign.", NUMBER, NONFINITE, "fract(-1.75)" => -0.75, ["fractional part"], ["trunc", "fraction"]),
    math!("exp", "Raise Euler's number e to the given power.", NUMBER, NONFINITE, "exp(0)" => 1.0, ["exponential"], ["ln"]),
    math!("sqrt", "Return the nonnegative square root.", "Nonnegative ordinary scalar.", "Negative inputs report an error.", "sqrt(81)" => 9.0, ["square root"], ["cbrt"]),
    math!("cbrt", "Return the signed cube root.", NUMBER, NONFINITE, "cbrt(-8)" => -2.0, ["cube root"], ["sqrt"]),
    math!("ln", "Return the natural logarithm (base e).", "Positive ordinary scalar.", "Zero and negative inputs report an error.", "ln(E)" => 1.0, ["natural logarithm"], ["log", "exp"]),
    math!("log", "Return the natural logarithm. This spelling is an alias of ln.", "Positive ordinary scalar.", "Zero and negative inputs report an error.", "log(1)" => 0.0, ["natural logarithm"], ["ln", "log10"]),
    math!("log2", "Return the base-two logarithm.", "Positive ordinary scalar.", "Zero and negative inputs report an error.", "log2(32)" => 5.0, ["binary logarithm"], ["log10", "ln"]),
    math!("log10", "Return the base-ten logarithm.", "Positive ordinary scalar.", "Zero and negative inputs report an error.", "log10(1000)" => 3.0, ["common logarithm"], ["log2", "ln"]),
    math!("mod", "Return the Euclidean remainder.", "Finite ordinary dividend and nonzero ordinary divisor.", "Floating-point rounding may reach the divisor's magnitude. Infix mod uses signed remainder.", "mod(-7, 3)" => 2.0, ["euclidean remainder", "modulo"], ["abs"]),
    call!("round_to", Mathematics, "Round to the nearest multiple of a step, with ties away from zero.", "Numeric value and positive numeric step.", NUMBERS, EXACT_PATH,
        "round_to(7.5, 5)" => Scalar(10.0), ["nearest multiple", "increment"], ["round", "floor_to", "ceil_to"]),
    call!("floor_to", Mathematics, "Round down to a multiple of a step.", "Numeric value and positive numeric step.", NUMBERS, EXACT_PATH,
        "floor_to(-7.5, 5)" => Scalar(-10.0), ["round down by step"], ["floor", "ceil_to"]),
    call!("ceil_to", Mathematics, "Round up to a multiple of a step.", "Numeric value and positive numeric step.", NUMBERS, EXACT_PATH,
        "ceil_to(-7.5, 5)" => Scalar(-5.0), ["round up by step"], ["ceil", "floor_to"]),
    call!("clamp", Mathematics, "Limit a value to inclusive bounds.", "Numeric value plus numeric minimum and maximum with min ≤ max.", NUMBERS, "Reversed bounds report an error. Mixed exact arithmetic requires integral ordinary scalars and rejects approximate inputs.",
        "clamp(15, 0, 10)" => Scalar(10.0), ["cap", "bound"], ["min", "max"]),
    call!("percent_change", Mathematics, "Return (new - old) / old. A ratio of 0.25 means 25%.", "Numeric new and old values. Old must differ from zero.", NUMBERS, EXACT_PATH,
        "percent_change(100, 80)" => Scalar(0.25), ["growth rate", "percentage change"], ["percent_of", "percent_changes"]),
    call!("percent_of", Mathematics, "Return part / total. A ratio of 0.25 means 25%.", "Numeric part and nonzero numeric total.", NUMBERS, EXACT_PATH,
        "percent_of(25, 100)" => Scalar(0.25), ["fraction of total", "share"], ["percent_change"], RATIO_VARIANTS),
    call!("choose", Mathematics, "Count ways to select k items from n without regard to order.", INTEGER_PAIR, COUNT_RESULT, COUNT_LIMITS,
        "choose(5, 2)" => Scalar(10.0), ["combinations", "binomial coefficient"], ["permutations"]),
    call!("permutations", Mathematics, "Count ordered selections of k items from n.", INTEGER_PAIR, COUNT_RESULT, COUNT_LIMITS,
        "permutations(5, 2)" => Scalar(20.0), ["ordered selections"], ["choose"]),
    call!("lerp", Mathematics, "Return start + (end - start) × t.", "Numeric start, end and t. Values of t outside [0, 1] extrapolate.", NUMBERS, EXACT_PATH,
        "lerp(10, 20, 0.25)" => Scalar(12.5), ["linear interpolation", "extrapolate"], ["map_range"]),
    call!("map_range", Mathematics, "Linearly map a value from a–b to c–d.", "Numeric value and four numeric endpoints with a ≠ b. Reversed intervals and extrapolation are allowed.", NUMBERS, EXACT_PATH,
        "map_range(50, 0, 100, 0, 10)" => Scalar(5.0), ["rescale", "linear mapping"], ["lerp"]),
    ReferenceEntry {
        related: &[
            ReferenceId::Call("sum"),
            ReferenceId::Topic("Iteration"),
            ReferenceId::Topic("Chart series"),
        ],
        ..call!("range", ControlFlow, "Create an ordered sequence that stops before the exclusive endpoint.", "Finite integral ordinary scalars. Default start is 0 and default step is 1.", "Range sequence usable for loops and collection statistics.", "A zero step, more than 1,024 elements, or an unrepresentable intermediate integer reports an error. A step away from the endpoint gives an empty sequence.",
        "sum(range(5, 0, -2))" => Scalar(9.0), ["sequence", "iterate", "integer range"], ["sum"], RANGE_VARIANTS)
    },
    call!("if", ControlFlow, "Evaluate the condition and then only the selected result branch.", "Boolean condition and two result expressions.", "The selected branch's value type.", "A non-Boolean condition reports an error. Source syntax must still parse.",
        "if(2 > 1, 5, sqrt(-1))" => Scalar(5.0), ["lazy condition", "branch", "ternary"], ["filter"]),
    call!("sum", Statistics, "Add the components of the collection.", COLLECTION, NUMBER, "Empty input gives zero. Nonfinite intermediate sums report an error.",
        "sum(range(4))" => Scalar(6.0), ["total"], ["average", "cumsum"]),
    call!("average", Statistics, "Return the arithmetic mean.", COLLECTION, NUMBER, NONEMPTY_COLLECTION,
        "average([2, 4, 6])" => Scalar(4.0), ["mean"], ["median", "rolling_average"]),
    call!("median", Statistics, "Return the middle value, averaging the two center values for an even-length input.", COLLECTION, NUMBER, NONEMPTY_COLLECTION,
        "median([8, 2, 4, 6])" => Scalar(5.0), ["middle value", "50th percentile"], ["quantile", "average"]),
    call!("min", Statistics, "Return the smallest collection element or the smaller of two scalar arguments.", "One vector/range sequence or two finite ordinary scalars. Also available as a scalar infix operator.", NUMBER, NONEMPTY_COLLECTION,
        "min([8, 2, 4])" => Scalar(2.0), ["minimum", "lowest"], ["max", "rolling_min"], MIN_VARIANTS),
    call!("max", Statistics, "Return the largest collection element or the larger of two scalar arguments.", "One vector/range sequence or two finite ordinary scalars. Also available as a scalar infix operator.", NUMBER, NONEMPTY_COLLECTION,
        "max([8, 2, 4])" => Scalar(8.0), ["maximum", "highest"], ["min", "rolling_max"], MAX_VARIANTS),
    call!("stdev", Statistics, "Return population standard deviation, using n as the variance divisor.", COLLECTION, NUMBER, NONEMPTY_COLLECTION,
        "stdev([-1, 1])" => Scalar(1.0), ["population standard deviation", "spread"], ["stdev_sample", "variance_population"]),
    call!("quantile", Statistics, "Sort a copy and interpolate at (n - 1) × q.", "Nonempty ordinary numeric vector and ordinary scalar q in [0, 1].", NUMBER, "Empty input, q outside the interval or a nonfinite result reports an error.",
        "quantile([30, 10, 20], 0.25)" => Scalar(15.0), ["quartile", "interpolated quantile"], ["percentile", "median"]),
    call!("percentile", Statistics, "Compute the quantile at rank p / 100.", "Nonempty ordinary numeric vector and ordinary scalar p in [0, 100].", NUMBER, "Empty input, p outside the interval or a nonfinite result reports an error.",
        "percentile([30, 10, 20], 25)" => Scalar(15.0), ["percentile rank"], ["quantile", "median"]),
    call!("variance_population", Statistics, "Return the mean of squared deviations from the mean.", STATS_VECTOR, NUMBER, "One element gives zero. Empty input or nonfinite results report an error.",
        "variance_population([-2, 2])" => Scalar(4.0), ["population variance"], ["variance_sample", "stdev"]),
    call!("variance_sample", Statistics, "Return summed squared deviations divided by n - 1.", SAMPLE_VECTOR, NUMBER, "Fewer than two values or nonfinite results report an error.",
        "variance_sample([1, 2, 3])" => Scalar(1.0), ["sample variance", "Bessel correction"], ["variance_population", "stdev_sample"]),
    call!("stdev_sample", Statistics, "Return sample standard deviation, using n - 1 as the variance divisor.", SAMPLE_VECTOR, NUMBER, "Fewer than two values or a nonfinite standard deviation reports an error.",
        "stdev_sample([1, 2, 3])" => Scalar(1.0), ["sample standard deviation"], ["stdev", "variance_sample"]),
    call!("covariance_population", Statistics, "Return paired covariance with divisor n.", PAIRS, NUMBER, "At least one pair is required. Unequal lengths or nonfinite results report an error.",
        "covariance_population([-1, 1], [-2, 2])" => Scalar(2.0), ["population covariance", "paired data"], ["covariance_sample", "correlation"]),
    call!("covariance_sample", Statistics, "Return paired covariance with divisor n - 1.", PAIRS, NUMBER, "At least two pairs are required. Unequal lengths or nonfinite results report an error.",
        "covariance_sample([-1, 1], [-2, 2])" => Scalar(4.0), ["sample covariance", "paired data"], ["covariance_population", "correlation"]),
    call!("correlation", Statistics, "Return Pearson's linear correlation coefficient.", PAIRS, "Ordinary scalar in [-1, 1].", "At least two pairs and variation in both vectors are required.",
        "correlation([1, 2, 3], [7, 5, 3])" => Scalar(-1.0), ["Pearson", "linear relationship"], ["covariance_sample", "regression_r2"]),
    call!("regression_slope", Statistics, "Return the slope of the least-squares line predicting Y from X.", PAIRS, NUMBER, FIT,
        "regression_slope([1, 2, 3], [3, 5, 7])" => Scalar(2.0), ["linear fit", "slope"], ["regression_intercept", "predict_linear"]),
    call!("regression_intercept", Statistics, "Return the fitted Y value at X = 0.", PAIRS, NUMBER, FIT,
        "regression_intercept([1, 2, 3], [3, 5, 7])" => Scalar(1.0), ["linear fit", "intercept"], ["regression_slope", "predict_linear"]),
    call!("regression_r2", Statistics, "Return the coefficient of determination for a least-squares line with an intercept.", PAIRS, "Ordinary scalar in [0, 1].", "At least two pairs and variation in X and Y are required.",
        "regression_r2([1, 2, 3], [3, 5, 7])" => Scalar(1.0), ["R-squared", "coefficient of determination"], ["correlation", "regression_slope"]),
    call!("predict_linear", Statistics, "Evaluate the least-squares fitted line at a new X coordinate.", "Two equally sized ordinary numeric vectors and a finite ordinary scalar x_new.", NUMBER, "The fit requires variation in X and at least two pairs. Values outside observed X are extrapolations. Nonfinite results report an error.",
        "predict_linear([1, 2, 3], [3, 5, 7], 4)" => Scalar(9.0), ["prediction", "forecast", "linear fit"], ["regression_slope", "regression_intercept"]),
    call!("decimal", ProgrammerExact, "Construct a base-ten exact number from quoted decimal text.", "A quoted decimal value, optionally signed and with a decimal exponent.", "Exact decimal.", EXACT_LIMITS,
        "decimal(\"0.1\") + decimal(\"0.2\")" => Exact("0.3"), ["exact base ten", "decimal literal"], ["fraction", "decimal_div", "to_float"]),
    call!("fraction", ProgrammerExact, "Construct a reduced exact fraction.", "Integer numerator and nonzero integer denominator, including exact or representable integral values.", "Exact rational.", EXACT_LIMITS,
        "fraction(1, 3) + fraction(1, 6)" => Exact("1/2"), ["rational", "reduced fraction"], ["decimal", "to_float"]),
    call!("decimal_div", ProgrammerExact, "Divide and round to decimal places with ties to even.", "Numeric dividend and nonzero divisor convertible to exact values, plus an integer places value in [0, 1000].", "Exact decimal with approximation provenance when rounding is needed.", EXACT_LIMITS,
        "decimal_div(decimal(\"1\"), decimal(\"8\"), 2)" => Exact("≈ 0.12"), ["bankers rounding", "rounded division"], ["decimal", "fraction"]),
    call!("to_float", ProgrammerExact, "Convert an exact decimal or fraction to floating point explicitly.", "An exact-number value from exact arithmetic or an exact constructor.", "Approximate ordinary scalar.", "Reports an error when the exact value cannot be represented as a finite scalar.",
        "to_float(fraction(1, 2))" => ApproximateScalar(0.5), ["approximate scalar", "convert to floating point"], ["fraction", "decimal"]),
    call!("not", ProgrammerExact, "Complement every bit of an integer in its mode.", WORD, "Same-mode typed integer.", "Lowercase spelling. Width limits the result to the operand's bits.",
        "not(u8(0))" => ExactInteger { mode: "u8", value: "255" }, ["bitwise complement", "bitwise negation", "~"], ["popcount", "mask"]),
    call!("popcount", ProgrammerExact, "Count the set bits in the operand's width.", WORD, "Nonnegative typed integer in the operand's mode.", "Lowercase spelling. Includes sign bits for signed modes.",
        "popcount(u8(0xb5))" => ExactInteger { mode: "u8", value: "5" }, ["count bits", "population count"], ["mask", "not"]),
    call!("rotl", ProgrammerExact, "Rotate bits left within the operand's width.", SHIFT, "Same-mode typed integer.", "Lowercase spelling. Negative counts report an error.",
        "rotl(u8(129), 1)" => ExactInteger { mode: "u8", value: "3" }, ["left rotation", "rotate"], ["rotr", "mask"]),
    call!("rotr", ProgrammerExact, "Rotate bits right within the operand's width.", SHIFT, "Same-mode typed integer.", "Lowercase spelling. Negative counts report an error.",
        "rotr(u8(129), 1)" => ExactInteger { mode: "u8", value: "192" }, ["right rotation", "rotate"], ["rotl", "mask"]),
    call!("mask", ProgrammerExact, "Build a field mask in the supplied integer mode.", FIELD, "Same-mode typed integer with the requested bits set.", "Invalid fields report an error. A zero-width field produces zero.",
        "mask(u8(0), 2, 3)" => ExactInteger { mode: "u8", value: "28" }, ["bit mask", "bit field"], ["extract", "popcount"]),
    call!("extract", ProgrammerExact, "Return the selected bits shifted down to bit zero.", FIELD, "Unsigned integer of the operand's width.", "Invalid fields report an error. A zero-width field produces zero.",
        "extract(u8(0xb5), 2, 4)" => ExactInteger { mode: "u8", value: "13" }, ["read bit field"], ["mask", "popcount"]),
    integer!("int", "Construct a checked signed 64-bit integer (i64).", "int(7)" => "i64", "7"),
    integer!("i8", "Construct a checked signed 8-bit integer.", "i8(-128)" => "i8", "-128"),
    integer!("i16", "Construct a checked signed 16-bit integer.", "i16(-32768)" => "i16", "-32768"),
    integer!("i32", "Construct a checked signed 32-bit integer.", "i32(-2147483648)" => "i32", "-2147483648"),
    integer!("i64", "Construct a checked signed 64-bit integer.", "i64(-9223372036854775808)" => "i64", "-9223372036854775808"),
    integer!("i128", "Construct a checked signed 128-bit integer.", "i128(-17)" => "i128", "-17"),
    integer!("u8", "Construct a checked unsigned 8-bit integer.", "u8(255)" => "u8", "255"),
    integer!("u16", "Construct a checked unsigned 16-bit integer.", "u16(65535)" => "u16", "65535"),
    integer!("u32", "Construct a checked unsigned 32-bit integer.", "u32(4294967295)" => "u32", "4294967295"),
    integer!("u64", "Construct a checked unsigned 64-bit integer.", "u64(18446744073709551615)" => "u64", "18446744073709551615"),
    integer!("u128", "Construct a checked unsigned 128-bit integer.", "u128(340282366920938463463374607431768211455)" => "u128", "340282366920938463463374607431768211455"),
    call!("date", CalendarTime, "Construct a calendar date.", "Quoted YYYY-MM-DD date.", "Calendar date.", "Invalid dates report an error.",
        "date(\"2024-02-29\")" => Calendar("2024-02-29"), ["calendar day", "ISO date"], ["time", "datetime", "add_days"]),
    call!("time", CalendarTime, "Construct a wall-clock time without a date or zone.", "Quoted HH:MM or HH:MM:SS, with optional fractional seconds after seconds.", "Time of day.", "Invalid clocks and leap seconds report an error.",
        "time(\"09:30:00\")" => Calendar("09:30:00"), ["clock time"], ["date", "elapsed_between"]),
    call!("datetime", CalendarTime, "Construct an absolute timestamp with an explicit offset.", "Quoted RFC3339 timestamp, including UTC Z or a signed offset.", "Offset timestamp.", "Invalid dates, times or offsets report an error.",
        "datetime(\"2026-11-15T12:34:56.125-05:00\")" => Calendar("2026-11-15T12:34:56.125-05:00"), ["timestamp", "RFC3339"], ["zoned_datetime", "date"]),
    call!("zoned_datetime", CalendarTime, "Construct a timestamp from a wall time and IANA time zone.", "Quoted ISO local date and time, quoted IANA zone, and optional quoted fold choice (earlier or later).", "Named-zone timestamp with offset.", ZONE_LIMITS,
        "zoned_datetime(\"2026-11-01T01:30:00\", \"America/New_York\", \"later\")" => Calendar("2026-11-01T01:30:00 UTC-05:00 America/New_York"), ["DST fold", "named time zone"], ["datetime", "add_days"], ZONED_VARIANTS),
    call!("add_days", CalendarTime, "Shift calendar days while preserving the local wall time of a timestamp.", DATE_SHIFT, "Calendar value of the input's kind.", ZONE_LIMITS,
        "add_days(date(\"2026-11-15\"), 45)" => Calendar("2026-12-30"), ["calendar days", "shift date"], ["add_months", "add_duration"], DAY_VARIANTS),
    call!("add_months", CalendarTime, "Shift calendar months and clamp to the last valid day of the target month.", DATE_SHIFT, "Calendar value of the input's kind.", ZONE_LIMITS,
        "add_months(date(\"2024-01-31\"), 1)" => Calendar("2024-02-29"), ["month end", "shift months"], ["add_days", "add_years"], MONTH_VARIANTS),
    call!("add_years", CalendarTime, "Shift calendar years and clamp to the last valid day of the target month.", DATE_SHIFT, "Calendar value of the input's kind.", ZONE_LIMITS,
        "add_years(date(\"2024-02-29\"), 1)" => Calendar("2025-02-28"), ["leap year", "shift years"], ["add_months", "add_days"], YEAR_VARIANTS),
    call!("days_between", CalendarTime, "Count signed calendar days from start to end.", "Two calendar dates.", "Integral ordinary scalar.", "Other calendar types report an error.",
        "days_between(date(\"2026-12-30\"), date(\"2026-11-15\"))" => Scalar(-45.0), ["date difference", "days apart"], ["elapsed_between", "add_days"]),
    call!("weekday", CalendarTime, "Return the weekday name.", "A calendar date.", "Named weekday value.", "Timestamp and time-only arguments report an error.",
        "weekday(date(\"2026-11-15\"))" => Calendar("Sunday"), ["day of week"], ["date"]),
    call!("elapsed_between", CalendarTime, "Subtract start from end in physical time.", "Two time-only values or two timestamps. Offset and named-zone timestamps represent instants.", "Physical quantity in seconds.", "Dates, incompatible pairs or out-of-range differences report an error.",
        "elapsed_between(time(\"08:00\"), time(\"09:30\"))" => Physical { value: 5400.0, unit: Unit::Second }, ["duration", "elapsed time"], ["days_between", "add_duration"]),
    call!("add_duration", CalendarTime, "Advance a time or timestamp by a physical duration.", "Time or timestamp and an unapproximated time quantity such as 1 h.", "Calendar value of the input's kind.", "A time-only value cannot cross midnight. A named-zone timestamp advances its instant and recomputes its local offset.",
        "add_duration(time(\"08:00:00\"), 90 min)" => Calendar("09:30:00"), ["add hours", "physical duration"], ["add_days", "elapsed_between"]),
];

const fn variant(
    title: &'static str,
    source: &'static str,
    expected: ExampleExpectation,
) -> CalculationExample {
    CalculationExample::new(title, source, title, expected)
}

const RANGE_VARIANTS: &[CalculationExample] = &[
    variant("Default start and step", "sum(range(4))", Scalar(6.0)),
    variant(
        "Explicit start, default step",
        "sum(range(3, 6))",
        Scalar(12.0),
    ),
];
const MIN_VARIANTS: &[CalculationExample] = &[
    variant("Compare two scalars", "min(7, 3)", Scalar(3.0)),
    variant("Use the infix form", "7 min 3", Scalar(3.0)),
];
const MAX_VARIANTS: &[CalculationExample] = &[
    variant("Compare two scalars", "max(7, 3)", Scalar(7.0)),
    variant("Use the infix form", "7 max 3", Scalar(7.0)),
];
const ZONED_VARIANTS: &[CalculationExample] = &[
    variant(
        "Construct an unambiguous wall time",
        "zoned_datetime(\"2026-11-02T01:30\", \"America/New_York\")",
        Calendar("2026-11-02T01:30:00 UTC-05:00 America/New_York"),
    ),
    variant(
        "Require a fold choice when ambiguous",
        "zoned_datetime(\"2026-11-01T01:30\", \"America/New_York\")",
        ExampleExpectation::ErrorContains("is ambiguous in America/New_York"),
    ),
];
const DAY_VARIANTS: &[CalculationExample] = &[
    variant(
        "Select the earlier fold after shifting a day",
        "add_days(zoned_datetime(\"2026-10-31T01:30\", \"America/New_York\"), 1, \"earlier\")",
        Calendar("2026-11-01T01:30:00 UTC-04:00 America/New_York"),
    ),
    variant(
        "Report a daylight-saving gap",
        "add_days(zoned_datetime(\"2026-03-07T02:30\", \"America/New_York\"), 1, \"later\")",
        ExampleExpectation::ErrorContains("does not exist in America/New_York"),
    ),
];
const MONTH_VARIANTS: &[CalculationExample] = &[variant(
    "Select the later fold after shifting a month",
    "add_months(zoned_datetime(\"2026-10-01T01:30\", \"America/New_York\"), 1, \"later\")",
    Calendar("2026-11-01T01:30:00 UTC-05:00 America/New_York"),
)];
const YEAR_VARIANTS: &[CalculationExample] = &[variant(
    "Select the earlier fold after shifting a year",
    "add_years(zoned_datetime(\"2025-11-01T01:30\", \"America/New_York\"), 1, \"earlier\")",
    Calendar("2026-11-01T01:30:00 UTC-04:00 America/New_York"),
)];

const RATIO_VARIANTS: &[CalculationExample] = &[variant(
    "Keep a terminating ratio exact",
    "percent_of(decimal(\"1\"), decimal(\"8\"))",
    Exact("0.125"),
)];
