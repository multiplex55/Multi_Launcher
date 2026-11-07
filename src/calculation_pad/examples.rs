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
    Physical {
        value: f64,
        unit: Unit,
    },
    ExactBase(&'static str),
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
                source: "-7 % 3",
                explanation: "% is the signed remainder operator.",
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
                source: "min(8, 3) + max(4, 9)",
                explanation: "min, max, and atan2 are available for two scalar arguments.",
                expected: ExampleExpectation::Scalar(12.0),
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
                source: "0x1235 + 0b0101010101",
                explanation: "0x, 0b, and 0o literals are scalar inputs, limited to exactly representable integers through 2^53.",
                expected: ExampleExpectation::Scalar(5002.0),
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
];

pub(crate) const EXAMPLE_GROUPS: &[CalculationExampleGroup] = EXAMPLES;

#[cfg(test)]
mod tests {
    use super::{EXAMPLE_GROUPS, ExampleExpectation};
    use crate::calculation_pad::{CalculationDocument, CalculationValue, LineOutcome};

    #[test]
    fn shared_examples_evaluate_to_their_documented_results() {
        let mut checked = 0;
        for group in EXAMPLE_GROUPS {
            for example in group.examples {
                let document = CalculationDocument::new(example.source);
                let evaluation = document.evaluate();
                let outcome = evaluation
                    .lines
                    .iter()
                    .rev()
                    .find(|line| !matches!(line.outcome, LineOutcome::Blank | LineOutcome::Comment))
                    .map(|line| &line.outcome)
                    .expect("each example has a result line");

                match (example.expected, outcome) {
                    (
                        ExampleExpectation::Scalar(expected),
                        LineOutcome::Value(CalculationValue::Scalar(actual)),
                    ) => {
                        let tolerance = f64::EPSILON * expected.abs().max(1.0) * 4.0;
                        assert!((actual - expected).abs() <= tolerance, "{}", example.title);
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

        assert_eq!(checked, 31);
    }
}
