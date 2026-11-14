//! Read-only comparison of two slots against one frozen evaluated Pad context.
use super::{CalculationValue, DocumentId, DocumentIncarnation, LineId, LineOutcome};
use std::{cmp::Ordering, collections::HashMap, sync::Arc};

#[derive(Clone, Debug, PartialEq)]
pub struct ComparisonRow {
    pub id: LineId,
    pub assignment_name: Option<String>,
    pub outcome: LineOutcome,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ComparisonContext {
    pub incarnation: DocumentIncarnation,
    pub document_id: DocumentId,
    pub revision: u64,
    pub rows: Arc<[ComparisonRow]>,
    pub(crate) final_bindings: Arc<HashMap<String, usize>>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComparisonSlot {
    Expression(String),
    Result(ComparisonResultRef),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComparisonResultRef {
    pub incarnation: DocumentIncarnation,
    pub document_id: DocumentId,
    pub line_id: LineId,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComparisonRequest {
    pub left: ComparisonSlot,
    pub right: ComparisonSlot,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ComparisonSide {
    pub input: ComparisonSlot,
    pub value: Result<CalculationValue, String>,
}
impl ComparisonSide {
    pub fn formatted(&self) -> String {
        match &self.value {
            Ok(value) => value
                .formatted_result()
                .unwrap_or_else(|| "unavailable".into()),
            Err(error) => error.clone(),
        }
    }
    pub fn type_label(&self) -> &'static str {
        match &self.value {
            Ok(CalculationValue::Integer(value)) => value.mode().type_name(),
            Ok(CalculationValue::ExactBase { .. }) => "exact base",
            Ok(CalculationValue::Scalar(_)) => "scalar",
            Ok(CalculationValue::Vector(_)) => "vector",
            Ok(CalculationValue::DimensionalQuantity(_)) => "quantity",
            Ok(CalculationValue::PhysicalQuantity { .. }) => "quantity",
            Err(_) => "error",
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum ComparisonRelation {
    SideError,
    UnsupportedVector,
    TypeMismatch {
        left: &'static str,
        right: &'static str,
    },
    IntegerModeMismatch {
        left: super::IntegerMode,
        right: super::IntegerMode,
    },
    IncompatibleUnits,
    DifferenceOverflow,
    ExactInteger {
        ordering: Ordering,
        negative_difference: bool,
        difference: u128,
        xor: u128,
        differing_bits: u32,
    },
    ExactBase {
        ordering: Ordering,
        negative_difference: bool,
        difference: u128,
        xor: u128,
        differing_bits: u32,
    },
    Scalar {
        ordering: Ordering,
        difference: f64,
    },
    Quantity {
        ordering: Ordering,
        difference: String,
        approximate: bool,
    },
}
impl ComparisonRelation {
    pub fn description(&self) -> String {
        fn ordering(value: Ordering) -> &'static str {
            match value {
                Ordering::Less => "Left is less",
                Ordering::Equal => "Equal",
                Ordering::Greater => "Left is greater",
            }
        }
        match self {
            Self::SideError => "A side could not be evaluated.".into(),
            Self::UnsupportedVector => "Vector ordering is not supported.".into(),
            Self::TypeMismatch { left, right } => format!("Type mismatch: {left} and {right}."),
            Self::IntegerModeMismatch { left, right } => format!(
                "Integer mode mismatch: {} and {}.",
                left.type_name(),
                right.type_name()
            ),
            Self::IncompatibleUnits => "Units or dimensions are incompatible.".into(),
            Self::DifferenceOverflow => "Difference is outside the finite range.".into(),
            Self::ExactInteger {
                ordering: order,
                negative_difference,
                difference,
                xor,
                differing_bits,
            }
            | Self::ExactBase {
                ordering: order,
                negative_difference,
                difference,
                xor,
                differing_bits,
            } => format!(
                "{}; left − right = {}{difference}; XOR 0x{xor:x} ({differing_bits} differing bits).",
                ordering(*order),
                if *negative_difference { "-" } else { "" }
            ),
            Self::Scalar {
                ordering: order,
                difference,
            } => format!("{}; left − right = {difference}.", ordering(*order)),
            Self::Quantity {
                ordering: order,
                difference,
                approximate,
            } => format!(
                "{}; left − right = {difference}{}.",
                ordering(*order),
                if *approximate { " (approximate)" } else { "" }
            ),
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct ComparisonResult {
    pub context: ComparisonContext,
    pub request: ComparisonRequest,
    pub left: ComparisonSide,
    pub right: ComparisonSide,
    pub relation: ComparisonRelation,
}
impl ComparisonContext {
    pub fn compare(&self, request: ComparisonRequest) -> ComparisonResult {
        fn side(context: &ComparisonContext, input: &ComparisonSlot) -> ComparisonSide {
            let outcome = match input {
                ComparisonSlot::Expression(expression) => {
                    super::document::evaluate_comparison_expression(
                        expression,
                        &context.rows,
                        &context.final_bindings,
                    )
                }
                ComparisonSlot::Result(reference)
                    if reference.incarnation != context.incarnation
                        || reference.document_id != context.document_id =>
                {
                    return ComparisonSide {
                        input: input.clone(),
                        value: Err(
                            "Selected result belongs to a previous document; select it again"
                                .into(),
                        ),
                    };
                }
                ComparisonSlot::Result(reference) => {
                    match context.rows.iter().find(|row| row.id == reference.line_id) {
                        Some(row) => row.outcome.clone(),
                        None => {
                            return ComparisonSide {
                                input: input.clone(),
                                value: Err("Selected row is unavailable".into()),
                            };
                        }
                    }
                }
            };
            let value = match outcome {
                LineOutcome::Value(value) => Ok(value),
                LineOutcome::Error(error) => Err(error.to_string()),
                LineOutcome::Blank | LineOutcome::Comment => Err("No result on this row".into()),
            };
            ComparisonSide {
                input: input.clone(),
                value,
            }
        }
        let left = side(self, &request.left);
        let right = side(self, &request.right);
        let relation = match (&left.value, &right.value) {
            (Ok(CalculationValue::Vector(_)), Ok(CalculationValue::Vector(_))) => {
                ComparisonRelation::UnsupportedVector
            }
            (Ok(CalculationValue::Integer(a)), Ok(CalculationValue::Integer(b))) => {
                if a.mode() != b.mode() {
                    ComparisonRelation::IntegerModeMismatch {
                        left: a.mode(),
                        right: b.mode(),
                    }
                } else {
                    let (a_value, b_value, ordering) = if a.mode().is_signed() {
                        let a = a.as_i128().expect("signed integer");
                        let b = b.as_i128().expect("signed integer");
                        (
                            (a as u128) ^ (1_u128 << 127),
                            (b as u128) ^ (1_u128 << 127),
                            a.cmp(&b),
                        )
                    } else {
                        let a = a.bit_pattern();
                        let b = b.bit_pattern();
                        (a, b, a.cmp(&b))
                    };
                    let xor = a.bit_pattern() ^ b.bit_pattern();
                    ComparisonRelation::ExactInteger {
                        ordering,
                        negative_difference: ordering == Ordering::Less,
                        difference: a_value.abs_diff(b_value),
                        xor,
                        differing_bits: xor.count_ones(),
                    }
                }
            }
            (
                Ok(CalculationValue::ExactBase { value: a, .. }),
                Ok(CalculationValue::ExactBase { value: b, .. }),
            ) => {
                let xor = a ^ b;
                ComparisonRelation::ExactBase {
                    ordering: a.cmp(b),
                    negative_difference: a < b,
                    difference: a.abs_diff(*b),
                    xor,
                    differing_bits: xor.count_ones(),
                }
            }
            (Ok(CalculationValue::Scalar(a)), Ok(CalculationValue::Scalar(b))) => {
                match a.partial_cmp(b) {
                    Some(ordering) if (a - b).is_finite() => ComparisonRelation::Scalar {
                        ordering,
                        difference: a - b,
                    },
                    _ => ComparisonRelation::DifferenceOverflow,
                }
            }
            (
                Ok(
                    a @ (CalculationValue::PhysicalQuantity { .. }
                    | CalculationValue::DimensionalQuantity(_)),
                ),
                Ok(
                    b @ (CalculationValue::PhysicalQuantity { .. }
                    | CalculationValue::DimensionalQuantity(_)),
                ),
            ) => match super::quantity::compare_values(a, b) {
                Ok((ordering, difference, approximate)) => ComparisonRelation::Quantity {
                    ordering,
                    difference,
                    approximate,
                },
                Err(
                    super::QuantityError::IncompatibleDimensions
                    | super::QuantityError::NonLinearUnit(_),
                ) => ComparisonRelation::IncompatibleUnits,
                Err(_) => ComparisonRelation::DifferenceOverflow,
            },
            (Ok(_), Ok(_)) => ComparisonRelation::TypeMismatch {
                left: left.type_label(),
                right: right.type_label(),
            },
            _ => ComparisonRelation::SideError,
        };
        ComparisonResult {
            context: self.clone(),
            request,
            left,
            right,
            relation,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calculation_pad::{CalculationPadSession, IntegerMode, IntegerWidth};
    use crate::platform::app_data::AppDataRoot;
    fn frozen(source: &str) -> (tempfile::TempDir, CalculationPadSession, ComparisonContext) {
        let temp = tempfile::tempdir().unwrap();
        let mut session =
            CalculationPadSession::new(&AppDataRoot::from_path(temp.path().join("data")));
        session.replace_source(source).unwrap();
        session.evaluation();
        let context = session.comparison_context().unwrap();
        (temp, session, context)
    }
    fn compare(context: &ComparisonContext, left: &str, right: &str) -> ComparisonResult {
        context.compare(ComparisonRequest {
            left: ComparisonSlot::Expression(left.into()),
            right: ComparisonSlot::Expression(right.into()),
        })
    }
    #[test]
    fn comparison_reuses_frozen_named_and_physical_row_results_independently() {
        let (_temp, mut session, context) =
            frozen("α = u64(9007199254740993)\n\n{雪} = α + 1\nα + 4");
        let result = compare(&context, "{雪} + r1", "r4");
        assert!(
            matches!(result.left.value, Ok(CalculationValue::Integer(value)) if value.bit_pattern() == 18014398509481987)
        );
        assert!(
            matches!(result.right.value, Ok(CalculationValue::Integer(value)) if value.bit_pattern() == 9007199254740997)
        );
        let bad = compare(&context, "r99", "{雪}");
        assert!(bad.left.value.is_err());
        assert!(bad.right.value.is_ok());
        assert!(compare(&context, "x = 2", "1\n2").left.value.is_err());
        session.replace_source("α = u64(7)").unwrap();
        assert_eq!(
            compare(&context, "r1", "r1").left.formatted(),
            "9007199254740993"
        );
        assert!(session.comparison_context().is_none());
    }

    #[test]
    fn comparison_uses_final_binding_and_keeps_selected_physical_value() {
        let (_temp, mut session, context) = frozen("x = 2\nold = x\nx = x + 3\nr1\nx");
        let first = ComparisonResultRef {
            incarnation: context.incarnation,
            document_id: context.document_id,
            line_id: context.rows[0].id,
        };
        let result = context.compare(ComparisonRequest {
            left: ComparisonSlot::Expression("x".into()),
            right: ComparisonSlot::Result(first),
        });
        assert_eq!(result.left.formatted(), "5");
        assert_eq!(result.right.formatted(), "2");
        assert_eq!(compare(&context, "old", "r1").left.formatted(), "2");
        let snapshot = session.dependency_snapshot().unwrap();
        assert_eq!(snapshot.rows[1].dependencies, [snapshot.rows[0].id]);
        assert_eq!(snapshot.rows[2].dependencies, [snapshot.rows[0].id]);
        assert_eq!(snapshot.rows[4].dependencies, [snapshot.rows[2].id]);

        session.replace_source("x = 8\nx = 1 / 0\nx").unwrap();
        session.evaluation();
        let failed = session.comparison_context().unwrap();
        assert!(compare(&failed, "x", "r1").left.value.is_err());
        assert_eq!(compare(&failed, "x", "r1").right.formatted(), "8");
        assert_eq!(compare(&context, "x", "r1").left.formatted(), "5");
    }

    #[test]
    fn vector_comparison_reports_unsupported_ordering() {
        let (_temp, _session, context) = frozen("v = [1, 2]\n[3, 4]");
        let result = compare(&context, "v", "r2");
        assert_eq!(result.left.type_label(), "vector");
        assert_eq!(result.left.formatted(), "[1, 2]");
        assert_eq!(result.relation, ComparisonRelation::UnsupportedVector);
        assert!(result.relation.description().contains("not supported"));
    }

    #[test]
    fn comparison_selected_result_rejects_same_document_reopen_collision() {
        let (_temp, mut session, _) = frozen("u8(7)");
        session.save_as("comparison identity").unwrap();
        let id = session.current_id().unwrap();
        session.request_open(id).unwrap();
        session.evaluation();
        let old = session.comparison_context().unwrap();
        let selected = ComparisonResultRef {
            incarnation: old.incarnation,
            document_id: old.document_id,
            line_id: old.rows[0].id,
        };
        let old_result = old.compare(ComparisonRequest {
            left: ComparisonSlot::Result(selected),
            right: ComparisonSlot::Expression("u8(2)".into()),
        });
        session.request_open(id).unwrap();
        session.evaluation();
        let fresh = session.comparison_context().unwrap();
        assert_eq!(old.document_id, fresh.document_id);
        assert_eq!(old.revision, fresh.revision);
        assert_eq!(old.rows[0].id, fresh.rows[0].id);
        assert_ne!(old.incarnation, fresh.incarnation);
        let next = fresh.compare(old_result.request.clone());
        assert!(next.left.value.is_err());
        assert_eq!(next.right.formatted(), "2");
        assert_eq!(old_result.left.formatted(), "7");
    }
    #[test]
    fn comparison_integer_modes_and_full_width_differences_are_exact() {
        let (_temp, _session, context) = frozen("");
        let result = compare(
            &context,
            "i128(-170141183460469231731687303715884105728)",
            "i128(170141183460469231731687303715884105727)",
        );
        assert!(matches!(
            result.relation,
            ComparisonRelation::ExactInteger {
                difference: u128::MAX,
                negative_difference: true,
                ..
            }
        ));
        assert!(
            matches!(compare(&context, "i8(-1)", "u8(255)").relation, ComparisonRelation::IntegerModeMismatch { left, right } if left == IntegerMode::signed(IntegerWidth::Bits8) && right == IntegerMode::unsigned(IntegerWidth::Bits8))
        );
        assert!(matches!(
            compare(&context, "u8(3)", "u8(1)").relation,
            ComparisonRelation::ExactInteger {
                difference: 2,
                xor: 2,
                differing_bits: 1,
                ..
            }
        ));
        let legacy = compare(&context, "255 dec to hex", "255 dec to bin");
        assert_eq!(legacy.left.type_label(), "exact base");
        assert_eq!(legacy.left.formatted(), "ff");
        assert!(matches!(
            legacy.relation,
            ComparisonRelation::ExactBase {
                ordering: Ordering::Equal,
                difference: 0,
                ..
            }
        ));
    }
    #[test]
    fn comparison_preserves_reassigned_failed_names_and_unit_semantics() {
        let (_temp, _session, context) = frozen("a = 1\na = 2\nb = 1 / 0\n10 m\n20 cm");
        let reassigned = compare(&context, "a", "r4");
        assert_eq!(reassigned.left.formatted(), "2");
        assert!(reassigned.right.value.is_ok());
        assert!(compare(&context, "b", "r4").left.value.is_err());
        let units = compare(&context, "r4", "r5");
        assert!(matches!(
            units.relation,
            ComparisonRelation::Quantity {
                ordering: Ordering::Greater,
                ..
            }
        ));
        assert!(matches!(
            compare(&context, "10 m", "10 s").relation,
            ComparisonRelation::IncompatibleUnits
        ));
        assert!(matches!(
            compare(&context, "min(2, 3)", "2").relation,
            ComparisonRelation::Scalar {
                ordering: Ordering::Equal,
                ..
            }
        ));
        let fuel = compare(&context, "10 L/100 km to L/100 km", "20 mpg to mpg");
        let reversed = compare(&context, "20 mpg to mpg", "10 L/100 km to L/100 km");
        assert!(matches!(
            fuel.relation,
            ComparisonRelation::Quantity {
                ordering: Ordering::Greater,
                ..
            }
        ));
        assert!(matches!(
            reversed.relation,
            ComparisonRelation::Quantity {
                ordering: Ordering::Less,
                ..
            }
        ));
    }
}
