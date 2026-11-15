use crate::unit_conversion::Unit;

#[derive(Clone, Copy, Debug)]
pub(crate) struct CalculationExample {
    pub(crate) title: &'static str,
    pub(crate) source: &'static str,
    pub(crate) explanation: &'static str,
    expected: ExampleExpectation,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct CalculationExampleGroup {
    pub(crate) title: &'static str,
    pub(crate) examples: &'static [CalculationExample],
}

#[derive(Clone, Copy, Debug)]
enum ExampleExpectation {
    Scalar(f64),
    ApproximateScalar(f64),
    ScalarWithSkipped(f64),
    Vector(&'static [f64]),
    Physical {
        value: f64,
        unit: Unit,
    },
    ExactBase(&'static str),
    Exact(&'static str),
    Calendar(&'static str),
    ExactInteger {
        mode: &'static str,
        value: &'static str,
    },
    Dimensional {
        formatted_result: &'static str,
        approximate: bool,
    },
    ErrorContains(&'static str),
}

impl CalculationExample {
    pub(crate) fn matches(&self, group: &str, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        query.is_empty()
            || [group, self.title, self.source, self.explanation]
                .iter()
                .any(|text| text.to_lowercase().contains(&query))
    }

    pub(crate) fn result_text(&self) -> String {
        use crate::common::number_format::format_number;
        match self.expected {
            ExampleExpectation::Scalar(value) | ExampleExpectation::ScalarWithSkipped(value) => {
                format_number(value).unwrap_or_default()
            }
            ExampleExpectation::ApproximateScalar(value) => format!("≈ {}", format_number(value).unwrap_or_default()),
            ExampleExpectation::Vector(values) => format!(
                "[{}]",
                values
                    .iter()
                    .filter_map(|value| format_number(*value))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            ExampleExpectation::Physical { value, unit } => format!(
                "{} {}",
                format_number(value).unwrap_or_default(),
                unit.symbol()
            ),
            ExampleExpectation::ExactBase(digits) => digits.into(),
            ExampleExpectation::Exact(value) => value.into(),
            ExampleExpectation::Calendar(value) => value.into(),
            ExampleExpectation::ExactInteger { value, .. } => value.into(),
            ExampleExpectation::Dimensional {
                formatted_result, ..
            } => formatted_result.into(),
            ExampleExpectation::ErrorContains(message) => format!("Error: {message}"),
        }
    }
}

const EXAMPLES: &[CalculationExampleGroup] = &[
    CalculationExampleGroup {
        title: "Variables and physical lines",
        examples: &[
            CalculationExample {
                title: "Assign and reuse a value",
                source: "radius = 3\nPI * radius ^ 2",
                explanation: "Assignments are available to later lines.",
                expected: ExampleExpectation::Scalar(std::f64::consts::PI * 9.0),
            },
            CalculationExample {
                title: "Reference a physical source line",
                source: "# Input\n2 + 3\nr2 * 4",
                explanation: "rN uses one-based physical lines; comments and blanks count.",
                expected: ExampleExpectation::Scalar(20.0),
            },
            CalculationExample {
                title: "Use a Unicode name",
                source: "{顧客 名} = 4\n{顧客 名} * 2",
                explanation: "Braced names can contain spaces and Unicode.",
                expected: ExampleExpectation::Scalar(8.0),
            },
        ],
    },
    CalculationExampleGroup {
        title: "Dates and times",
        examples: &[
            CalculationExample {
                title: "Shift a calendar date",
                source: "add_days(date(\"2026-11-15\"), 45)",
                explanation: "Calendar day shifts preserve the date type.",
                expected: ExampleExpectation::Calendar("2026-12-30"),
            },
            CalculationExample {
                title: "Count signed calendar days",
                source: "days_between(date(\"2026-12-30\"), date(\"2026-11-15\"))",
                explanation: "Days between follows start-to-end order and may be negative.",
                expected: ExampleExpectation::Scalar(-45.0),
            },
            CalculationExample {
                title: "Clamp a month-end shift",
                source: "add_months(date(\"2024-01-31\"), 1)",
                explanation: "A month shift clamps to the last valid day in the target month.",
                expected: ExampleExpectation::Calendar("2024-02-29"),
            },
            CalculationExample {
                title: "Name a weekday",
                source: "weekday(date(\"2026-11-15\"))",
                explanation: "Weekday results display their names.",
                expected: ExampleExpectation::Calendar("Sunday"),
            },
            CalculationExample {
                title: "Keep an explicit offset and fractional seconds",
                source: "datetime(\"2026-11-15T12:34:56.125-05:00\")",
                explanation: "Offset timestamps retain their instant and subsecond precision.",
                expected: ExampleExpectation::Calendar("2026-11-15T12:34:56.125-05:00"),
            },
            CalculationExample {
                title: "Choose a daylight-saving fold",
                source: "zoned_datetime(\"2026-11-01T01:30:00\", \"America/New_York\", \"later\")",
                explanation: "Ambiguous named-zone wall times need an explicit earlier or later choice.",
                expected: ExampleExpectation::Calendar("2026-11-01T01:30:00 UTC-05:00 America/New_York"),
            },
            CalculationExample {
                title: "Measure elapsed time",
                source: "elapsed_between(time(\"08:00\"), time(\"09:30\"))",
                explanation: "Elapsed differences produce a physical time quantity in seconds.",
                expected: ExampleExpectation::Physical { value: 5400.0, unit: Unit::Second },
            },
            CalculationExample {
                title: "Reject time-only date rollover",
                source: "add_duration(time(\"23:30\"), 1 h)",
                explanation: "Use a timestamp when adding a duration crosses a date boundary.",
                expected: ExampleExpectation::ErrorContains("use a timestamp"),
            },
        ],
    },
    CalculationExampleGroup {
        title: "Exact decimals and fractions",
        examples: &[
            CalculationExample {
                title: "Add exact decimals",
                source: "decimal(\"0.1\") + decimal(\"0.2\")",
                explanation: "Quoted decimal literals preserve their base-10 values.",
                expected: ExampleExpectation::Exact("0.3"),
            },
            CalculationExample {
                title: "Add fractions",
                source: "fraction(1,3) + fraction(1,6)",
                explanation: "Fractions reduce to lowest terms.",
                expected: ExampleExpectation::Exact("1/2"),
            },
            CalculationExample {
                title: "Combine a large integer and fraction",
                source: "u64(9007199254740993) + fraction(1,2)",
                explanation: "Fixed-width integers promote losslessly when combined with an exact number.",
                expected: ExampleExpectation::Exact("18014398509481987/2"),
            },
            CalculationExample {
                title: "Round a decimal division",
                source: "decimal_div(decimal(\"1\"), decimal(\"8\"), 2)",
                explanation: "Decimal division rounds ties to even and identifies a rounded result.",
                expected: ExampleExpectation::Exact("≈ 0.12"),
            },
            CalculationExample {
                title: "Convert explicitly to floating point",
                source: "to_float(fraction(1,2))",
                explanation: "Explicit conversion permits floating-point operations and marks the result approximate.",
                expected: ExampleExpectation::ApproximateScalar(0.5),
            },
            CalculationExample {
                title: "Reject a zero denominator",
                source: "fraction(1,0)",
                explanation: "A fraction must have a nonzero denominator.",
                expected: ExampleExpectation::ErrorContains("denominator"),
            },
            CalculationExample {
                title: "Reject unrounded repeating division",
                source: "decimal(\"1\") / decimal(\"3\")",
                explanation: "Use decimal_div with an explicit precision for repeating decimal results.",
                expected: ExampleExpectation::ErrorContains("nonterminating"),
            },
        ],
    },
    CalculationExampleGroup {
        title: "Operators and functions",
        examples: &[
            CalculationExample {
                title: "Scientific notation",
                source: "1.25e-2 + 2.5E-1",
                explanation: "A decimal mantissa may use e or E with a signed exponent.",
                expected: ExampleExpectation::Scalar(0.2625),
            },
            CalculationExample {
                title: "Remainder keeps the dividend sign",
                source: "-7 mod 3",
                explanation: "Infix mod and % share signed remainder semantics.",
                expected: ExampleExpectation::Scalar(-1.0),
            },
            CalculationExample {
                title: "Euclidean modulo is non-negative",
                source: "mod(-7, 3)",
                explanation: "mod(a, b) returns the Euclidean remainder: 2 here.",
                expected: ExampleExpectation::Scalar(2.0),
            },
            CalculationExample {
                title: "Square root, cube root, and absolute value",
                source: "sqrt(81) + cbrt(27) + abs(-2)",
                explanation: "The existing root and absolute-value functions use finite f64 values.",
                expected: ExampleExpectation::Scalar(14.0),
            },
            CalculationExample {
                title: "Rounding functions",
                source: "round(2.6) + floor(2.9) + ceil(2.1)",
                explanation: "round, floor, ceil, trunc, and fract are available.",
                expected: ExampleExpectation::Scalar(8.0),
            },
            CalculationExample {
                title: "Logarithms",
                source: "ln(E) + log10(100)",
                explanation: "ln and log are natural logarithms; log2 and log10 are also available.",
                expected: ExampleExpectation::Scalar(3.0),
            },
            CalculationExample {
                title: "Trigonometry",
                source: "sin(PI / 2) + cos(0)",
                explanation: "Trigonometric functions use radians.",
                expected: ExampleExpectation::Scalar(2.0),
            },
            CalculationExample {
                title: "Minimum and maximum",
                source: "min(100, min(50, 75))",
                explanation: "min, max, and atan2 accept two scalar arguments, including nested calls.",
                expected: ExampleExpectation::Scalar(50.0),
            },
            CalculationExample {
                title: "Trigonometric function families",
                source: "sin(0) + cos(0) + tan(0)\nasin(0) + acos(1) + atan(0)\nsinh(0) + cosh(0) + tanh(0)\nasinh(0) + acosh(1) + atanh(0)",
                explanation: "sin cos tan asin acos atan sinh cosh tanh asinh acosh atanh use radians.",
                expected: ExampleExpectation::Scalar(0.0),
            },
            CalculationExample {
                title: "More scalar functions",
                source: "signum(-2) + trunc(2.9) + fract(1.5)\nexp(0) + log2(8) + log(E) + atan2(0, 1)",
                explanation: "signum trunc fract exp log log2 and atan2 complement the other scalar functions.",
                expected: ExampleExpectation::Scalar(5.0),
            },
            CalculationExample {
                title: "Case-insensitive function and constant aliases",
                source: "Floor(2.9)\npi + pie - PI",
                explanation: "Floor, FLOOR and floor share a function; unbound pi and pie mean PI.",
                expected: ExampleExpectation::Scalar(std::f64::consts::PI),
            },
        ],
    },
    CalculationExampleGroup {
        title: "Constants and number bases",
        examples: &[
            CalculationExample {
                title: "Built-in constants",
                source: "PI + TAU + E",
                explanation: "PI, π, E, e, TAU, and τ are scalar constants.",
                expected: ExampleExpectation::Scalar(
                    std::f64::consts::PI + std::f64::consts::TAU + std::f64::consts::E,
                ),
            },
            CalculationExample {
                title: "Mix scalar integer bases",
                source: "0b10101\n0x10 + 0o10 + 0b10",
                explanation: "Binary 0b, octal 0o and hexadecimal 0x can be mixed through 2^53.",
                expected: ExampleExpectation::Scalar(26.0),
            },
            CalculationExample {
                title: "Convert an exact unsigned base value",
                source: "9007199254740993 dec to hex",
                explanation: "The separate base-conversion path stays exact through u128::MAX.",
                expected: ExampleExpectation::ExactBase("20000000000001"),
            },
            CalculationExample {
                title: "Convert a physical quantity",
                source: "6 ft 2 in to cm",
                explanation: "Physical quantities use the shared unit catalog and remain separate from scalars.",
                expected: ExampleExpectation::Physical {
                    value: 187.96,
                    unit: Unit::Centimeter,
                },
            },
        ],
    },
    CalculationExampleGroup {
        title: "Vectors",
        examples: &[
            CalculationExample {
                title: "Scale a vector",
                source: "v = [1, 2, 3]\nv * 3",
                explanation: "Multiply a vector by a scalar in either order.",
                expected: ExampleExpectation::Vector(&[3.0, 6.0, 9.0]),
            },
            CalculationExample {
                title: "Element-wise operations",
                source: "[1, 2, 3] * [4, 5, 6]",
                explanation: "Equal-length vector +, -, *, and / act element by element.",
                expected: ExampleExpectation::Vector(&[4.0, 10.0, 18.0]),
            },
            CalculationExample {
                title: "Dot product",
                source: "dot([1, 2, 3], [4, 5, 6])",
                explanation: "dot(a, b) or a · b returns a scalar.",
                expected: ExampleExpectation::Scalar(32.0),
            },
            CalculationExample {
                title: "Cross product",
                source: "cross([1, 0, 0], [0, 1, 0])",
                explanation: "cross(a, b) or a × b requires three components.",
                expected: ExampleExpectation::Vector(&[0.0, 0.0, 1.0]),
            },
        ],
    },
    CalculationExampleGroup {
        title: "Errors and limits",
        examples: &[
            CalculationExample {
                title: "Invalid base digit",
                source: "0b102",
                explanation: "A prefixed literal must contain digits valid for its base.",
                expected: ExampleExpectation::ErrorContains("invalid base-2 integer literal"),
            },
            CalculationExample {
                title: "Scalar precision boundary",
                source: "0x20000000000001",
                explanation: "Scalar literals above 2^53 are rejected instead of rounded.",
                expected: ExampleExpectation::ErrorContains("exactly representable"),
            },
            CalculationExample {
                title: "Zero remainder divisor",
                source: "1 % 0",
                explanation: "Division and remainder by zero, and non-finite intermediate results, are errors.",
                expected: ExampleExpectation::ErrorContains("remainder by zero"),
            },
        ],
    },
    CalculationExampleGroup {
        title: "Exact programmer arithmetic",
        examples: &[
            CalculationExample {
                title: "Shift a default u64 value",
                source: "123 >> 12",
                explanation: "Untyped programmer expressions use u64, and this shift produces zero.",
                expected: ExampleExpectation::ExactInteger {
                    mode: "u64",
                    value: "0",
                },
            },
            CalculationExample {
                title: "Default unsigned shift",
                source: "0xFF << 8",
                explanation: "Programmer operators select exact u64 arithmetic when no typed value selects another width.",
                expected: ExampleExpectation::ExactInteger {
                    mode: "u64",
                    value: "65280",
                },
            },
            CalculationExample {
                title: "Width-limited left shift",
                source: "u8(255) << 1",
                explanation: "The left shift keeps the low eight bits, so 510 becomes 254.",
                expected: ExampleExpectation::ExactInteger {
                    mode: "u8",
                    value: "254",
                },
            },
            CalculationExample {
                title: "Signed right shift",
                source: "i8(-1) >> 1",
                explanation: "A signed right shift extends the sign bit.",
                expected: ExampleExpectation::ExactInteger {
                    mode: "i8",
                    value: "-1",
                },
            },
            CalculationExample {
                title: "Named and row references stay exact",
                source: "wide = u64(9007199254740993)\nr1 + wide",
                explanation: "The name and physical-row reference reuse the exact value above the f64 integer-precision limit.",
                expected: ExampleExpectation::ExactInteger {
                    mode: "u64",
                    value: "18014398509481986",
                },
            },
            CalculationExample {
                title: "Checked integer quotient",
                source: "u32(100) idiv 6",
                explanation: "idiv divides exact integer operands and keeps the selected u32 mode.",
                expected: ExampleExpectation::ExactInteger {
                    mode: "u32",
                    value: "16",
                },
            },
            CalculationExample {
                title: "Extract a bit field",
                source: "extract(u8(0xb5), 2, 4)",
                explanation: "The field begins at bit 2 and contains four bits; extraction returns u8.",
                expected: ExampleExpectation::ExactInteger {
                    mode: "u8",
                    value: "13",
                },
            },
            CalculationExample {
                title: "Rotate, count, and mask bits",
                source: "rotl(u8(129), 1)\nrotr(u8(129), 1)\npopcount(u8(0xb5))\nmask(u8(0), 2, 3)",
                explanation: "rotl, rotr, popcount and mask operate on exact-width integers.",
                expected: ExampleExpectation::ExactInteger {
                    mode: "u8",
                    value: "28",
                },
            },
            CalculationExample {
                title: "Integer width constructors",
                source: "int(7)\nu16(255)",
                explanation: "int, i8, i16, i32, i64, i128, u8, u16, u32, u64, and u128 select checked modes.",
                expected: ExampleExpectation::ExactInteger {
                    mode: "u16",
                    value: "255",
                },
            },
            CalculationExample {
                title: "Overflow is diagnosed",
                source: "u8(255) + 1",
                explanation: "Checked arithmetic reports overflow instead of wrapping.",
                expected: ExampleExpectation::ErrorContains("integer arithmetic overflowed"),
            },
        ],
    },
    CalculationExampleGroup {
        title: "Dimensional unit arithmetic",
        examples: &[
            CalculationExample {
                title: "Add compatible lengths",
                source: "10 m + 20 cm",
                explanation: "Compatible length units combine and the result keeps its length unit.",
                expected: ExampleExpectation::Dimensional {
                    formatted_result: "10.2 m",
                    approximate: false,
                },
            },
            CalculationExample {
                title: "Multiply lengths into area",
                source: "(2 m) * (3 m)",
                explanation: "Length multiplied by length produces an area result.",
                expected: ExampleExpectation::Dimensional {
                    formatted_result: "6 m²",
                    approximate: false,
                },
            },
            CalculationExample {
                title: "Divide distance by duration",
                source: "(5 m) / (2 s)",
                explanation: "Length divided by duration produces a speed result.",
                expected: ExampleExpectation::Dimensional {
                    formatted_result: "2.5 m/s",
                    approximate: false,
                },
            },
            CalculationExample {
                title: "Convert a named area",
                source: "area = (2 m) * (3 m)\narea to cm^2",
                explanation: "A compatible target converts the referenced area while retaining its dimension.",
                expected: ExampleExpectation::Dimensional {
                    formatted_result: "60000 cm²",
                    approximate: false,
                },
            },
            CalculationExample {
                title: "Keep month approximation through cancellation",
                source: "ratio = 1 month/month\nratio",
                explanation: "The dimension cancels to a scalar display while the month approximation remains attached.",
                expected: ExampleExpectation::Dimensional {
                    formatted_result: "1",
                    approximate: true,
                },
            },
        ],
    },
    CalculationExampleGroup {
        title: "Loops and Iteration",
        examples: &[
            CalculationExample {
                title: "Count from zero with range",
                source: "total = 0\nfor i in range(5):\n    total += i\ntotal",
                explanation: "range(5) visits 0 through 4. Calculation Pad supports for loops. while loops are unsupported.",
                expected: ExampleExpectation::Scalar(10.0),
            },
            CalculationExample {
                title: "Sum a start and stop range",
                source: "total = 0\nfor i in range(1, 6):\n    total += i\ntotal",
                explanation: "The running total adds 1 through 5. The stop value 6 is excluded.",
                expected: ExampleExpectation::Scalar(15.0),
            },
            CalculationExample {
                title: "Iterate with a positive step",
                source: "total = 0\nfor i in range(0, 10, 2):\n    total += i\ntotal",
                explanation: "A step of 2 visits 0, 2, 4, 6, and 8.",
                expected: ExampleExpectation::Scalar(20.0),
            },
            CalculationExample {
                title: "Reverse iteration with a negative step",
                source: "digits = 0\nfor i in range(5, 0, -1):\n    digits = digits * 10 + i\ndigits",
                explanation: "The negative step counts down from 5 to 1, excluding 0.",
                expected: ExampleExpectation::Scalar(54321.0),
            },
            CalculationExample {
                title: "Iterate over a vector literal",
                source: "total = 0\nfor price in [4, 7, 9]:\n    total += price\ntotal",
                explanation: "Each vector component is visited in order.",
                expected: ExampleExpectation::Scalar(20.0),
            },
            CalculationExample {
                title: "Iterate over a named vector",
                source: "measurements = [2, 4, 6]\ntotal = 0\nfor value in measurements:\n    total += value\ntotal",
                explanation: "A vector stored in a name can be used as the iteration source.",
                expected: ExampleExpectation::Scalar(12.0),
            },
            CalculationExample {
                title: "Nested loops",
                source: "total = 0\nfor x in [1, 2]:\n    for y in [3, 4]:\n        total += x * y\ntotal",
                explanation: "The inner loop runs once for each outer value, adding four products.",
                expected: ExampleExpectation::Scalar(21.0),
            },
            CalculationExample {
                title: "Multiple statements in a loop",
                source: "total = 0\nfor price in [3, 5]:\n    doubled = price * 2\n    total += doubled\ntotal",
                explanation: "Both equally indented statements run for each price.",
                expected: ExampleExpectation::Scalar(16.0),
            },
            CalculationExample {
                title: "Calculate a factorial",
                source: "number = 5\nfactorial = 1\nfor i in range(1, number + 1):\n    factorial *= i\nfactorial",
                explanation: "Multiply by each integer from 1 through number to calculate 5 factorial.",
                expected: ExampleExpectation::Scalar(120.0),
            },
            CalculationExample {
                title: "Loop with zero iterations",
                source: "total = 3\nfor i in range(0):\n    total += 1\ntotal",
                explanation: "An empty range skips the body, leaving the previous total unchanged. The skipped row shows not executed.",
                expected: ExampleExpectation::ScalarWithSkipped(3.0),
            },
        ],
    },
    CalculationExampleGroup {
        title: "Variables and compound assignments",
        examples: &[
            CalculationExample {
                title: "Reassignment and earlier values",
                source: "x = 10\nearlier = x * 2\nx = 7\nearlier + x",
                explanation: "A new assignment becomes the latest binding. earlier keeps its calculated value of 20.",
                expected: ExampleExpectation::Scalar(27.0),
            },
            CalculationExample {
                title: "Compound assignment updates",
                source: "x = 10\nx += 5\nx *= 2\nx",
                explanation: "A compound assignment combines the latest x with the right side and stores the result.",
                expected: ExampleExpectation::Scalar(30.0),
            },
            CalculationExample {
                title: "Subtract, divide, remainder, and power assignments",
                source: "x = 20\nx -= 2\nx /= 3\nx %= 4\nx ^= 3\nx",
                explanation: "These compound operators subtract, divide, take signed remainder, and raise to a power in order.",
                expected: ExampleExpectation::Scalar(8.0),
            },
            CalculationExample {
                title: "Compound vector addition",
                source: "v = [1, 2]\nv += [3, 4]\nv",
                explanation: "Vector compound addition adds corresponding elements.",
                expected: ExampleExpectation::Vector(&[4.0, 6.0]),
            },
            CalculationExample {
                title: "Compound exact integer addition",
                source: "n = u128(9007199254740993)\nn += 9007199254740993\nn",
                explanation: "Exact integer compound arithmetic retains the integer mode and precision.",
                expected: ExampleExpectation::ExactInteger {
                    mode: "u128",
                    value: "18014398509481986",
                },
            },
            CalculationExample {
                title: "Compound dimensional addition",
                source: "distance = 1 m\ndistance += 2 m\ndistance",
                explanation: "Compatible physical dimensions can be added with a compound assignment.",
                expected: ExampleExpectation::Dimensional {
                    formatted_result: "3 m",
                    approximate: false,
                },
            },
        ],
    },
    CalculationExampleGroup {
        title: "Conditions and collections",
        examples: &[
            CalculationExample {
                title: "Comparison operators",
                source: "if(3 > 2 && 2 < 3 && 3 >= 3 && 2 <= 2 && 3 == 3 && 2 != 3, 1, 0)",
                explanation: "All six comparisons are true, so the conditional expression returns 1.",
                expected: ExampleExpectation::Scalar(1.0),
            },
            CalculationExample {
                title: "Logical OR and NOT",
                source: "if(!(2 > 3) || 1 == 2, 10, 0)",
                explanation: "! reverses a Boolean result. || is true when either side is true.",
                expected: ExampleExpectation::Scalar(10.0),
            },
            CalculationExample {
                title: "Conditional expression in a calculation",
                source: "price = 100\ndiscount = if(price >= 100, 10, 0)\nprice - discount",
                explanation: "if(condition, true value, false value) selects a value inside an expression. Multiline if statements are unsupported.",
                expected: ExampleExpectation::Scalar(90.0),
            },
            CalculationExample {
                title: "Short-circuit logical OR",
                source: "if(1 == 1 || 1 / 0 > 0, 42, 0)",
                explanation: "The first condition is true, so OR skips the division on its right.",
                expected: ExampleExpectation::Scalar(42.0),
            },
            CalculationExample {
                title: "Average and median of measurements",
                source: "values = [2, 4, 9]\naverage(values) + median(values)",
                explanation: "average is 5 and median is 4. The final sum is 9.",
                expected: ExampleExpectation::Scalar(9.0),
            },
            CalculationExample {
                title: "Minimum and maximum from a range",
                source: "min(range(1, 6)) + max(range(1, 6))",
                explanation: "min and max find the smallest and largest sequence values, 1 and 5.",
                expected: ExampleExpectation::Scalar(6.0),
            },
            CalculationExample {
                title: "Population standard deviation of a range",
                source: "stdev(range(1, 4)) * stdev(range(1, 4))",
                explanation: "stdev uses population standard deviation. Its square is the population variance, 2/3.",
                expected: ExampleExpectation::Scalar(2.0 / 3.0),
            },
            CalculationExample {
                title: "Choose a value",
                source: "if(2 > 1, 7, 1 / 0)",
                explanation: "Only the selected branch is evaluated.",
                expected: ExampleExpectation::Scalar(7.0),
            },
            CalculationExample {
                title: "Sum a range",
                source: "sum(range(1, 6))",
                explanation: "Ranges exclude their stop and contain at most 1,024 values.",
                expected: ExampleExpectation::Scalar(15.0),
            },
            CalculationExample {
                title: "Population standard deviation",
                source: "stdev([2, 2])",
                explanation: "Standard deviation uses the population convention.",
                expected: ExampleExpectation::Scalar(0.0),
            },
            CalculationExample {
                title: "Running total",
                source: "total = 0\nfor amount in [10, 20, 30]:\n    total = total + amount\ntotal",
                explanation: "A loop visits each value and keeps the final assignment.",
                expected: ExampleExpectation::Scalar(60.0),
            },
        ],
    },
    CalculationExampleGroup {
        title: "Script errors and boundaries",
        examples: &[
            CalculationExample {
                title: "Missing loop body",
                source: "for i in range(3):",
                explanation: "A for header must be followed by a more-indented body.",
                expected: ExampleExpectation::ErrorContains("for block requires an indented body"),
            },
            CalculationExample {
                title: "Inconsistent loop indentation",
                source: "for i in range(2):\n    x = i\n      x = i + 1",
                explanation: "Statements at the same loop level need the same indentation using spaces.",
                expected: ExampleExpectation::ErrorContains("unexpected indentation"),
            },
            CalculationExample {
                title: "Range step cannot be zero",
                source: "range(1, 5, 0)",
                explanation: "A zero step cannot advance toward the stop.",
                expected: ExampleExpectation::ErrorContains("range step cannot be zero"),
            },
            CalculationExample {
                title: "Range size limit",
                source: "range(1025)",
                explanation: "A range can contain at most 1,024 values.",
                expected: ExampleExpectation::ErrorContains("range exceeds the 1024-element limit"),
            },
            CalculationExample {
                title: "Division by zero in a loop",
                source: "for divisor in [2, 0]:\n    value = 10 / divisor",
                explanation: "The second iteration divides by zero and reports its failing iteration.",
                expected: ExampleExpectation::ErrorContains(
                    "iteration 2: division or remainder by zero",
                ),
            },
            CalculationExample {
                title: "Compound assignment requires a binding",
                source: "missing += 1",
                explanation: "Compound assignment needs an earlier binding for its target.",
                expected: ExampleExpectation::ErrorContains("unknown name: missing"),
            },
            CalculationExample {
                title: "Compound exact integer overflow",
                source: "n = u8(255)\nn += 1",
                explanation: "Checked u8 addition reports overflow when the result exceeds 255.",
                expected: ExampleExpectation::ErrorContains("integer arithmetic overflowed"),
            },
        ],
    },
    CalculationExampleGroup {
        title: "Formula functions",
        examples: &[
            CalculationExample {
                title: "Calculate a discounted price",
                source: "fn discounted_price(price, discount) = price * (1 - discount)\ndiscounted_price(100, 0.10)",
                explanation: "Parameters are local to each call and the definition has no result row.",
                expected: ExampleExpectation::Scalar(90.0),
            },
            CalculationExample {
                title: "Use an earlier formula function",
                source: "fn double(value) = value * 2\nfn add_one(value) = double(value) + 1\nadd_one(4)",
                explanation: "A function may call another function declared earlier in the document.",
                expected: ExampleExpectation::Scalar(9.0),
            },
            CalculationExample {
                title: "Use a lazy condition in a function",
                source: "fn safe_inverse(value) = if(value == 0, 0, 1 / value)\nsafe_inverse(0)",
                explanation: "The unused branch is not evaluated.",
                expected: ExampleExpectation::Scalar(0.0),
            },
            CalculationExample {
                title: "Pass document values explicitly",
                source: "tax = 0.2\nfn total(price) = price * (1 + tax)",
                explanation: "Function bodies cannot capture document variables. Pass them as parameters.",
                expected: ExampleExpectation::ErrorContains("parameters and constants"),
            },
            CalculationExample {
                title: "Multiline formula in a conditional loop",
                source: "fn double(value) = (\n    value * 2\n)\ntotal = 0\nfor i in range(6):\n    if i mod 2 == 0:\n        total += double(i)\ntotal",
                explanation: "A multiline pure formula is called only for even loop values; the final total is 12.",
                expected: ExampleExpectation::Scalar(12.0),
            },
        ],
    },
    CalculationExampleGroup {
        title: "Everyday numeric helpers",
        examples: &[
            CalculationExample { title: "Round to an increment", source: "round_to(137, 25)", explanation: "Round to the nearest positive increment, with ties away from zero.", expected: ExampleExpectation::Scalar(125.0) },
            CalculationExample { title: "Floor to an increment", source: "floor_to(137, 25)", explanation: "Round down to the next increment.", expected: ExampleExpectation::Scalar(125.0) },
            CalculationExample { title: "Ceil to an increment", source: "ceil_to(137, 25)", explanation: "Round up to the next increment.", expected: ExampleExpectation::Scalar(150.0) },
            CalculationExample { title: "Clamp to bounds", source: "clamp(125, 0, 100)", explanation: "Limit a value to inclusive bounds.", expected: ExampleExpectation::Scalar(100.0) },
            CalculationExample { title: "Percentage change", source: "percent_change(120, 100)", explanation: "The result is a ratio, so 0.2 represents a 20% increase.", expected: ExampleExpectation::Scalar(0.2) },
            CalculationExample { title: "Part of a total", source: "percent_of(25, 100)", explanation: "Divide the part by the total to obtain a ratio.", expected: ExampleExpectation::Scalar(0.25) },
            CalculationExample { title: "Combinations", source: "choose(10, 2)", explanation: "Count unordered selections without computing factorials.", expected: ExampleExpectation::Scalar(45.0) },
            CalculationExample { title: "Permutations", source: "permutations(5, 2)", explanation: "Count ordered selections.", expected: ExampleExpectation::Scalar(20.0) },
            CalculationExample { title: "Interpolate", source: "lerp(10, 20, 0.5)", explanation: "Interpolate or extrapolate between two values.", expected: ExampleExpectation::Scalar(15.0) },
            CalculationExample { title: "Map between ranges", source: "map_range(50, 0, 100, 0, 10)", explanation: "Map a value between ranges, including reversed ranges.", expected: ExampleExpectation::Scalar(5.0) },
        ],
    },
];

pub(crate) const EXAMPLE_GROUPS: &[CalculationExampleGroup] = EXAMPLES;

#[cfg(test)]
mod tests {
    use super::{EXAMPLE_GROUPS, ExampleExpectation};
    use crate::calculation_pad::{
        CalculationDocument, CalculationValue, DiagnosticKind, LineOutcome,
    };

    #[test]
    fn shared_examples_evaluate_to_their_documented_results() {
        let mut checked = 0;
        for group in EXAMPLE_GROUPS {
            for example in group.examples {
                let document = CalculationDocument::new(example.source);
                let evaluation = document.evaluate();
                if !matches!(example.expected, ExampleExpectation::ErrorContains(_)) {
                    if matches!(example.expected, ExampleExpectation::ScalarWithSkipped(_)) {
                        assert!(
                            evaluation.lines.iter().any(|line| matches!(
                                &line.outcome,
                                LineOutcome::Error(diagnostic)
                                    if matches!(diagnostic.kind, DiagnosticKind::NotExecuted)
                            )),
                            "{} must contain a skipped row",
                            example.title
                        );
                    }
                    assert!(
                        evaluation.lines.iter().all(|line| match &line.outcome {
                            LineOutcome::Error(diagnostic) => matches!(
                                (example.expected, &diagnostic.kind),
                                (
                                    ExampleExpectation::ScalarWithSkipped(_),
                                    DiagnosticKind::NotExecuted
                                )
                            ),
                            _ => true,
                        }),
                        "{} has an invalid source line",
                        example.title
                    );
                }
                let outcome = evaluation
                    .lines
                    .iter()
                    .rev()
                    .find(|line| !matches!(line.outcome, LineOutcome::Blank | LineOutcome::Comment))
                    .map(|line| &line.outcome)
                    .expect("each example has a result line");

                match (example.expected, outcome) {
                    (
                        ExampleExpectation::Scalar(expected)
                        | ExampleExpectation::ScalarWithSkipped(expected),
                        LineOutcome::Value(CalculationValue::Scalar(actual)),
                    ) => {
                        let tolerance = f64::EPSILON * expected.abs().max(1.0) * 4.0;
                        assert!((actual - expected).abs() <= tolerance, "{}", example.title);
                    }
                    (ExampleExpectation::ApproximateScalar(expected), LineOutcome::Value(CalculationValue::ApproximateScalar(actual))) => {
                        assert_eq!(*actual, expected, "{}", example.title);
                    }
                    (ExampleExpectation::Exact(expected), LineOutcome::Value(actual @ CalculationValue::Exact(_))) => {
                        assert_eq!(actual.formatted_result().as_deref(), Some(expected), "{}", example.title);
                    }
                    (ExampleExpectation::Calendar(expected), LineOutcome::Value(actual @ CalculationValue::Calendar(_))) => {
                        assert_eq!(actual.formatted_result().as_deref(), Some(expected), "{}", example.title);
                    }
                    (
                        ExampleExpectation::Vector(expected),
                        LineOutcome::Value(CalculationValue::Vector(actual)),
                    ) => {
                        assert_eq!(actual.components(), expected, "{}", example.title);
                    }
                    (
                        ExampleExpectation::Physical {
                            value: expected,
                            unit: expected_unit,
                        },
                        LineOutcome::Value(CalculationValue::PhysicalQuantity {
                            value, unit, ..
                        }),
                    ) => {
                        assert_eq!(*unit, expected_unit, "{}", example.title);
                        assert!((*value - expected).abs() <= 1e-10, "{}", example.title);
                    }
                    (
                        ExampleExpectation::ExactBase(expected),
                        LineOutcome::Value(CalculationValue::ExactBase { digits, .. }),
                    ) => {
                        assert_eq!(digits, expected, "{}", example.title);
                    }
                    (
                        ExampleExpectation::ExactInteger {
                            mode: expected_mode,
                            value: expected_value,
                        },
                        LineOutcome::Value(CalculationValue::Integer(actual)),
                    ) => {
                        assert_eq!(
                            actual.mode().type_name(),
                            expected_mode,
                            "{}",
                            example.title
                        );
                        assert_eq!(
                            actual.formatted_decimal(),
                            expected_value,
                            "{}",
                            example.title
                        );
                    }
                    (
                        ExampleExpectation::Dimensional {
                            formatted_result: expected_result,
                            approximate: expected_approximation,
                        },
                        LineOutcome::Value(actual @ CalculationValue::DimensionalQuantity(_)),
                    ) => {
                        assert_eq!(
                            actual.formatted_result().as_deref(),
                            Some(expected_result),
                            "{}",
                            example.title
                        );
                        assert_eq!(
                            actual.is_approximate(),
                            expected_approximation,
                            "{}",
                            example.title
                        );
                    }
                    (
                        ExampleExpectation::ErrorContains(expected),
                        LineOutcome::Error(diagnostic),
                    ) => {
                        assert!(
                            diagnostic.to_string().contains(expected),
                            "{}: {diagnostic}",
                            example.title
                        );
                    }
                    (expected, actual) => {
                        panic!("{}: expected {expected:?}, got {actual:?}", example.title)
                    }
                }
                checked += 1;
            }
        }

        assert_eq!(checked, 104);
    }

    #[test]
    fn catalogue_search_finds_function_keywords_and_vector_examples() {
        for title in [
            "Loops and Iteration",
            "Variables and compound assignments",
            "Conditions and collections",
            "Script errors and boundaries",
        ] {
            let group = EXAMPLE_GROUPS
                .iter()
                .find(|group| group.title == title)
                .expect("expected catalogue group");
            let query = title.to_uppercase();
            assert!(
                group
                    .examples
                    .iter()
                    .all(|example| example.matches(group.title, &query))
            );
        }
        for keyword in [
            "ceil",
            "floor",
            "sqrt",
            "binary",
            "dot",
            "cross",
            "atanh",
            "rotl",
            "mask",
            "u128",
            "for",
            "range",
            "nested",
            "factorial",
            "compound",
            "+=",
            "median",
            "iteration",
            "round_to",
            "percent_change",
            "permutations",
            "map_range",
            "discounted_price",
        ] {
            assert!(
                EXAMPLE_GROUPS.iter().any(|group| group
                    .examples
                    .iter()
                    .any(|example| example.matches(group.title, keyword))),
                "missing {keyword}"
            );
        }
        assert!(EXAMPLE_GROUPS.iter().any(|group| {
            group
                .examples
                .iter()
                .any(|example| example.matches(group.title, "VeCtOrS"))
        }));
        assert!(!EXAMPLE_GROUPS.iter().any(|group| {
            group
                .examples
                .iter()
                .any(|example| example.matches(group.title, "no such example"))
        }));
        assert!(EXAMPLE_GROUPS.iter().all(|group| {
            group
                .examples
                .iter()
                .all(|example| example.matches(group.title, ""))
        }));
    }
}
