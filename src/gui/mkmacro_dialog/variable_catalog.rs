//! Authoring-time inventory of variables produced by earlier macro steps.
//!
//! This deliberately contains descriptors, not runtime values.  It is safe to
//! rebuild whenever a picker opens and is never serialized with a macro.

use super::action_catalog::action_name;
use crate::mkmacro::{
    MkAction, MkBlockKind, MkImageNotFoundPolicy, MkImageOutputs, MkStep, MkValue,
    structure::analyze_structure,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariableValueType {
    String,
    Number,
    Boolean,
    Point,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariableAvailability {
    DefinitelyAvailable,
    PossiblyUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariableUncertaintyReason {
    ProducedInside(MkBlockKind),
    MayBeNullIfNotFound,
}

impl VariableUncertaintyReason {
    pub fn help_text(self) -> &'static str {
        match self {
            Self::ProducedInside(MkBlockKind::If) => "Produced inside If",
            Self::ProducedInside(MkBlockKind::While) => "Produced inside While",
            Self::ProducedInside(MkBlockKind::Repeat) => "Produced inside Repeat",
            Self::MayBeNullIfNotFound => "May be Null if not found",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariableDescriptor {
    pub name: String,
    pub value_type: VariableValueType,
    pub source_step_id: u64,
    pub source_step_index: usize,
    /// One-based step number suitable for presentation in the picker.
    pub source_step_number: usize,
    pub source_action_label: &'static str,
    pub availability: VariableAvailability,
    /// Structured causes used by picker warning icons and tooltips.
    pub uncertainty_reasons: Vec<VariableUncertaintyReason>,
    /// Extra picker guidance, including when a value can be Null at runtime.
    pub help_text: Option<&'static str>,
}

impl VariableDescriptor {
    /// Marker rendered beside entries whose producer may not execute or may
    /// yield Null.
    pub fn warning_marker(&self) -> Option<&'static str> {
        (self.availability == VariableAvailability::PossiblyUnavailable).then_some("⚠")
    }
}

/// Both views of the variable definitions visible at a consumer location.
///
/// Names have the same semantics as [`crate::mkmacro::RuntimeVariables`] keys:
/// surrounding editor whitespace is removed when a descriptor is made, but
/// comparison is otherwise exact and case-sensitive.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VariableCatalog {
    history: Vec<VariableDescriptor>,
    effective: Vec<VariableDescriptor>,
}

impl VariableCatalog {
    /// Builds the catalog immediately before `consumer_index`.
    ///
    /// Out-of-range indices are consistently clamped to `steps.len()`, making
    /// `usize::MAX` useful for asking for the catalog at the end of a macro.
    pub fn before_step(steps: &[MkStep], consumer_index: usize) -> Self {
        let end = consumer_index.min(steps.len());
        // Analyze once for the whole construction. In particular, do not
        // reproduce block parsing here: structure analysis also owns the
        // conservative semantics for incomplete editor drafts.
        let structure = analyze_structure(steps);
        let history: Vec<_> = steps[..end]
            .iter()
            .enumerate()
            .flat_map(|(index, step)| {
                let complete_kind = structure
                    .step(step.id)
                    .and_then(|_| structure.containing_block(step.id))
                    .map(|block| block.kind);
                let enclosing_kind =
                    complete_kind.or_else(|| structure.editor_enclosing_kind(step.id));
                descriptors_for_action(step, index)
                    .into_iter()
                    .map(move |mut descriptor| {
                        if let Some(kind) = enclosing_kind {
                            descriptor.availability = VariableAvailability::PossiblyUnavailable;
                            descriptor
                                .uncertainty_reasons
                                .insert(0, VariableUncertaintyReason::ProducedInside(kind));
                            // Keep a producer-specific nullable explanation (for example an
                            // image miss) when one exists. Structural uncertainty is still
                            // retained separately in `uncertainty_reasons`.
                            descriptor.help_text.get_or_insert(
                                VariableUncertaintyReason::ProducedInside(kind).help_text(),
                            );
                        }
                        descriptor
                    })
            })
            .collect();

        // Walk the source-ordered history and replace an old winner whenever
        // its exact name is produced again. Removing before appending means
        // picker order is the source position of each latest definition, then
        // the output order declared by that action.
        let mut effective = Vec::new();
        for descriptor in &history {
            if let Some(old) = effective
                .iter()
                .position(|old: &VariableDescriptor| old.name == descriptor.name)
            {
                effective.remove(old);
            }
            effective.push(descriptor.clone());
        }
        Self { history, effective }
    }

    /// Ordered producer history, intended for diagnostics and hover details.
    pub fn history(&self) -> &[VariableDescriptor] {
        &self.history
    }

    /// Effective variables at the consumer, with one entry per exact name.
    ///
    /// Entries are ordered by the source position of their latest definition,
    /// then by that action's output order.
    pub fn effective_variables(&self) -> &[VariableDescriptor] {
        &self.effective
    }

    /// Effective picker entries of `value_type`. Shadowing is deliberately
    /// resolved before this filter is applied.
    pub fn effective_variables_of_type(
        &self,
        value_type: VariableValueType,
    ) -> impl Iterator<Item = &VariableDescriptor> {
        self.effective
            .iter()
            .filter(move |descriptor| descriptor.value_type == value_type)
    }

    /// Compatibility alias for the ordered producer history.
    pub fn descriptors(&self) -> &[VariableDescriptor] {
        self.history()
    }

    /// Returns one descriptor per name, choosing its latest definition while
    /// retaining the deterministic source order of those winning definitions.
    pub fn latest_definitions(&self) -> Vec<&VariableDescriptor> {
        self.effective.iter().collect()
    }
}

/// The sole action-to-produced-output mapping for the authoring UI.
pub fn descriptors_for_action(step: &MkStep, step_index: usize) -> Vec<VariableDescriptor> {
    let mut result = Vec::new();
    let label = action_name(&step.action);
    let mut add = |name: &str, value_type, availability, help_text| {
        let name = name.trim();
        if !name.is_empty() {
            result.push(VariableDescriptor {
                name: name.to_owned(),
                value_type,
                source_step_id: step.id,
                source_step_index: step_index,
                source_step_number: step_index + 1,
                source_action_label: label,
                availability,
                uncertainty_reasons: if availability == VariableAvailability::PossiblyUnavailable {
                    vec![VariableUncertaintyReason::MayBeNullIfNotFound]
                } else {
                    Vec::new()
                },
                help_text,
            });
        }
    };

    match &step.action {
        MkAction::SetVariable { name, value } => add(
            name,
            match value {
                MkValue::String(_) => VariableValueType::String,
                MkValue::Number(_) => VariableValueType::Number,
                MkValue::Boolean(_) => VariableValueType::Boolean,
                MkValue::Point(_) => VariableValueType::Point,
                MkValue::Null => VariableValueType::Unknown,
            },
            VariableAvailability::DefinitelyAvailable,
            None,
        ),
        MkAction::PromptInput(payload) => add(
            &payload.variable,
            VariableValueType::String,
            VariableAvailability::DefinitelyAvailable,
            None,
        ),
        MkAction::ImageFind(payload) => add_visual_outputs(
            &payload.outputs,
            &mut add,
            payload.not_found_policy == MkImageNotFoundPolicy::Continue,
            "May be Null if the image is not found",
        ),
        MkAction::FindPixel(payload) => add_visual_outputs(
            &payload.outputs,
            &mut add,
            payload.not_found_policy == MkImageNotFoundPolicy::Continue,
            "May be Null if the pixel is not found",
        ),
        MkAction::CaptureScreenshot(payload) if payload.destination.produces_file() => {
            if let Some(name) = &payload.path_output {
                add(
                    name,
                    VariableValueType::String,
                    VariableAvailability::DefinitelyAvailable,
                    None,
                );
            }
        }
        _ => {}
    }
    result
}

fn add_visual_outputs(
    outputs: &MkImageOutputs,
    add: &mut impl FnMut(&str, VariableValueType, VariableAvailability, Option<&'static str>),
    can_continue_missing: bool,
    nullable_help: &'static str,
) {
    let optional = if can_continue_missing {
        VariableAvailability::PossiblyUnavailable
    } else {
        VariableAvailability::DefinitelyAvailable
    };
    if let Some(name) = &outputs.found {
        add(
            name,
            VariableValueType::Boolean,
            VariableAvailability::DefinitelyAvailable,
            None,
        );
    }
    for (name, value_type) in [
        (&outputs.point, VariableValueType::Point),
        (&outputs.x, VariableValueType::Number),
        (&outputs.y, VariableValueType::Number),
    ] {
        if let Some(name) = name {
            add(
                name,
                value_type,
                optional,
                can_continue_missing.then_some(nullable_help),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::{
        AlphaPolicy, MkBlockKind, MkCondition, MkFileCollisionPolicy, MkImagePayload,
        MkPixelSearchPayload, MkPoint, MkPromptInputPayload, MkScreenshotDestination,
        MkScreenshotFormat, MkScreenshotPayload, MkWaitOptions, ReturnPoint, SearchRegion,
    };

    fn step(id: u64, action: MkAction) -> MkStep {
        MkStep {
            id,
            enabled: true,
            repeat: 1,
            delay_after_ms: 0,
            on_error: Default::default(),
            action,
        }
    }
    fn outputs() -> MkImageOutputs {
        MkImageOutputs {
            found: Some("found".into()),
            point: Some("point".into()),
            x: Some("x".into()),
            y: Some("y".into()),
        }
    }
    fn image(outputs: MkImageOutputs) -> MkAction {
        MkAction::ImageFind(MkImagePayload {
            asset_id: 1,
            wait: MkWaitOptions::default(),
            region: SearchRegion::Desktop,
            tolerance: 0,
            alpha: AlphaPolicy::Compare,
            return_point: ReturnPoint::Center,
            not_found_policy: MkImageNotFoundPolicy::Continue,
            outputs,
        })
    }
    fn pixel(outputs: MkImageOutputs) -> MkAction {
        MkAction::FindPixel(MkPixelSearchPayload {
            search_id: 1,
            color: "#000000".into(),
            tolerance: 0,
            region: SearchRegion::Desktop,
            wait: MkWaitOptions::default(),
            not_found_policy: MkImageNotFoundPolicy::Continue,
            outputs,
        })
    }
    fn set(id: u64, name: &str) -> MkStep {
        step(
            id,
            MkAction::SetVariable {
                name: name.into(),
                value: MkValue::Number(id as f64),
            },
        )
    }
    fn set_value(id: u64, name: &str, value: MkValue) -> MkStep {
        step(
            id,
            MkAction::SetVariable {
                name: name.into(),
                value,
            },
        )
    }
    fn empty_condition() -> MkCondition {
        MkCondition::All { conditions: vec![] }
    }

    #[test]
    fn set_variable_maps_every_value_type() {
        let values = [
            (MkValue::String("s".into()), VariableValueType::String),
            (MkValue::Number(1.0), VariableValueType::Number),
            (MkValue::Boolean(true), VariableValueType::Boolean),
            (
                MkValue::Point(MkPoint { x: 1, y: 2 }),
                VariableValueType::Point,
            ),
            (MkValue::Null, VariableValueType::Unknown),
        ];
        for (index, (value, expected)) in values.into_iter().enumerate() {
            let descriptors = descriptors_for_action(
                &step(
                    index as u64,
                    MkAction::SetVariable {
                        name: format!("v{index}"),
                        value,
                    },
                ),
                index,
            );
            assert_eq!(descriptors[0].value_type, expected);
        }
    }

    #[test]
    fn prompt_and_visual_actions_produce_expected_types() {
        let prompt = descriptors_for_action(
            &step(
                1,
                MkAction::PromptInput(MkPromptInputPayload {
                    variable: " answer ".into(),
                    ..Default::default()
                }),
            ),
            0,
        );
        assert_eq!(
            (&prompt[0].name, prompt[0].value_type),
            (&"answer".to_owned(), VariableValueType::String)
        );
        for action in [image(outputs()), pixel(outputs())] {
            let descriptors = descriptors_for_action(&step(2, action), 1);
            assert_eq!(
                descriptors
                    .iter()
                    .map(|d| (&*d.name, d.value_type))
                    .collect::<Vec<_>>(),
                vec![
                    ("found", VariableValueType::Boolean),
                    ("point", VariableValueType::Point),
                    ("x", VariableValueType::Number),
                    ("y", VariableValueType::Number)
                ]
            );
            assert_eq!(
                descriptors[0].availability,
                VariableAvailability::DefinitelyAvailable
            );
            for descriptor in &descriptors[1..] {
                assert_eq!(
                    descriptor.availability,
                    VariableAvailability::PossiblyUnavailable
                );
                assert!(descriptor.help_text.unwrap().contains("Null"));
            }
        }
    }

    #[test]
    fn screenshot_only_exposes_paths_for_file_destinations() {
        let screenshot = |destination| {
            MkAction::CaptureScreenshot(MkScreenshotPayload {
                region: SearchRegion::Desktop,
                destination,
                path: None,
                format: MkScreenshotFormat::Png,
                collision: MkFileCollisionPolicy::Unique,
                path_output: Some(" saved ".into()),
            })
        };
        assert_eq!(
            descriptors_for_action(&step(1, screenshot(MkScreenshotDestination::File)), 0)[0].name,
            "saved"
        );
        assert!(
            descriptors_for_action(&step(2, screenshot(MkScreenshotDestination::Clipboard)), 0)
                .is_empty()
        );
        assert_eq!(
            descriptors_for_action(&step(3, screenshot(MkScreenshotDestination::Both)), 0).len(),
            1
        );
    }

    #[test]
    fn blank_names_are_ignored() {
        let blank = MkImageOutputs {
            found: None,
            point: Some(String::new()),
            x: Some("  ".into()),
            y: None,
        };
        assert!(descriptors_for_action(&step(1, image(blank)), 0).is_empty());
        assert!(
            descriptors_for_action(
                &step(
                    2,
                    MkAction::SetVariable {
                        name: " \t".into(),
                        value: MkValue::Null
                    }
                ),
                0
            )
            .is_empty()
        );
    }

    #[test]
    fn before_step_is_half_open_clamped_and_deterministic() {
        let set = |id, name: &str| {
            step(
                id,
                MkAction::SetVariable {
                    name: name.into(),
                    value: MkValue::Number(id as f64),
                },
            )
        };
        let steps = vec![set(10, "a"), set(11, "b"), set(12, "a"), set(13, "later")];
        let catalog = VariableCatalog::before_step(&steps, 2);
        assert_eq!(
            catalog
                .descriptors()
                .iter()
                .map(|d| (&*d.name, d.source_step_index))
                .collect::<Vec<_>>(),
            vec![("a", 0), ("b", 1)]
        );
        assert!(
            !catalog
                .descriptors()
                .iter()
                .any(|d| d.source_step_id == 12 || d.source_step_id == 13)
        );
        let all = VariableCatalog::before_step(&steps, usize::MAX);
        assert_eq!(
            all.descriptors()
                .iter()
                .map(|d| &*d.name)
                .collect::<Vec<_>>(),
            vec!["a", "b", "a", "later"]
        );
        assert_eq!(
            all.latest_definitions()
                .iter()
                .map(|d| &*d.name)
                .collect::<Vec<_>>(),
            vec!["b", "a", "later"]
        );
    }

    #[test]
    fn effective_view_replaces_same_name_and_keeps_latest_metadata() {
        let steps = vec![
            set_value(10, " value ", MkValue::Number(1.0)),
            step(
                20,
                MkAction::PromptInput(MkPromptInputPayload {
                    variable: "value".into(),
                    ..Default::default()
                }),
            ),
        ];
        let catalog = VariableCatalog::before_step(&steps, usize::MAX);

        assert_eq!(catalog.history().len(), 2, "history retains both producers");
        assert_eq!(catalog.effective_variables().len(), 1);
        let effective = &catalog.effective_variables()[0];
        assert_eq!(effective.value_type, VariableValueType::String);
        assert_eq!(effective.source_step_id, 20);
        assert_eq!(effective.source_step_index, 1);
        assert_eq!(effective.source_step_number, 2);
        assert_eq!(effective.source_action_label, "Prompt for Input");
    }

    #[test]
    fn effective_type_filter_runs_after_point_string_shadow_resolution() {
        let point = |id, name: &str| set_value(id, name, MkValue::Point(MkPoint { x: 1, y: 2 }));
        let string = |id, name: &str| set_value(id, name, MkValue::String("text".into()));

        let point_then_string =
            VariableCatalog::before_step(&[point(1, "target"), string(4, "target")], 2);
        assert_eq!(
            point_then_string
                .effective_variables_of_type(VariableValueType::Point)
                .count(),
            0
        );

        let string_then_point =
            VariableCatalog::before_step(&[string(1, "target"), point(4, "target")], 2);
        let points: Vec<_> = string_then_point
            .effective_variables_of_type(VariableValueType::Point)
            .collect();
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].source_step_id, 4);
    }

    #[test]
    fn conditional_latest_definition_supplies_type_and_availability() {
        let conditional_string = vec![
            set_value(1, "target", MkValue::Point(MkPoint { x: 1, y: 2 })),
            step(2, MkAction::If(empty_condition())),
            set_value(3, "target", MkValue::String("conditional".into())),
            step(4, MkAction::EndIf),
        ];
        let catalog = VariableCatalog::before_step(&conditional_string, usize::MAX);
        let effective = &catalog.effective_variables()[0];

        assert_eq!(effective.value_type, VariableValueType::String);
        assert_eq!(
            effective.availability,
            VariableAvailability::PossiblyUnavailable
        );
        assert_eq!(effective.source_step_id, 3);
        assert_eq!(
            catalog
                .effective_variables_of_type(VariableValueType::Point)
                .count(),
            0
        );
    }

    #[test]
    fn effective_names_are_case_sensitive_ignore_future_steps_and_have_stable_order() {
        let steps = vec![
            set(1, "target"),
            set(2, "middle"),
            set(3, "Target"),
            set(4, "target"),
            set(5, "future"),
        ];
        let catalog = VariableCatalog::before_step(&steps, 4);
        let effective: Vec<_> = catalog
            .effective_variables()
            .iter()
            .map(|descriptor| (&*descriptor.name, descriptor.source_step_index))
            .collect();

        // Winners are ordered by their latest source position. Exact casing
        // keeps the two runtime keys distinct.
        assert_eq!(effective, vec![("middle", 1), ("Target", 2), ("target", 3)]);
        assert!(!catalog.history().iter().any(|item| item.name == "future"));
    }

    #[test]
    fn structural_enclosures_make_producers_possibly_unavailable() {
        let cases = [
            (
                vec![
                    step(1, MkAction::If(empty_condition())),
                    set(2, "if_body"),
                    step(3, MkAction::EndIf),
                ],
                MkBlockKind::If,
            ),
            (
                vec![
                    step(1, MkAction::If(empty_condition())),
                    step(2, MkAction::Else),
                    set(3, "else_body"),
                    step(4, MkAction::EndIf),
                ],
                MkBlockKind::If,
            ),
            (
                vec![
                    step(
                        1,
                        MkAction::WhileStart {
                            condition: empty_condition(),
                        },
                    ),
                    set(2, "while_body"),
                    step(3, MkAction::WhileEnd),
                ],
                MkBlockKind::While,
            ),
            (
                vec![
                    step(1, MkAction::RepeatStart { count: 2 }),
                    set(2, "repeat_body"),
                    step(3, MkAction::RepeatEnd),
                ],
                MkBlockKind::Repeat,
            ),
        ];
        for (steps, kind) in cases {
            let catalog = VariableCatalog::before_step(&steps, usize::MAX);
            let descriptor = &catalog.descriptors()[0];
            assert_eq!(
                descriptor.availability,
                VariableAvailability::PossiblyUnavailable
            );
            assert_eq!(
                descriptor.uncertainty_reasons,
                vec![VariableUncertaintyReason::ProducedInside(kind)]
            );
            assert_eq!(descriptor.warning_marker(), Some("⚠"));
        }
    }

    #[test]
    fn nested_production_stays_possible_but_completed_block_restores_top_level() {
        let steps = vec![
            set(10, "top"),
            step(11, MkAction::If(empty_condition())),
            step(12, MkAction::RepeatStart { count: 2 }),
            set(13, "nested"),
            step(14, MkAction::RepeatEnd),
            step(15, MkAction::EndIf),
            set(16, "after"),
        ];
        let catalog = VariableCatalog::before_step(&steps, usize::MAX);
        let descriptors = catalog.descriptors();
        assert_eq!(
            descriptors[0].availability,
            VariableAvailability::DefinitelyAvailable
        );
        assert_eq!(
            descriptors[1].availability,
            VariableAvailability::PossiblyUnavailable
        );
        assert_eq!(
            descriptors[2].availability,
            VariableAvailability::DefinitelyAvailable
        );
    }

    #[test]
    fn unclosed_draft_is_conservative_and_preserves_source_metadata() {
        let steps = vec![step(40, MkAction::If(empty_condition())), set(987, "draft")];
        let catalog = VariableCatalog::before_step(&steps, usize::MAX);
        let descriptor = &catalog.descriptors()[0];
        assert_eq!(
            descriptor.availability,
            VariableAvailability::PossiblyUnavailable
        );
        assert_eq!(descriptor.source_step_id, 987);
        assert_eq!(descriptor.source_step_index, 1);
        assert_eq!(descriptor.source_step_number, 2);
        assert_eq!(descriptor.source_action_label, "Set Variable");
    }

    #[test]
    fn top_level_nullable_point_remains_possibly_unavailable() {
        let steps = vec![step(
            55,
            image(MkImageOutputs {
                found: None,
                point: Some("location".into()),
                x: None,
                y: None,
            }),
        )];
        let catalog = VariableCatalog::before_step(&steps, usize::MAX);
        let descriptor = &catalog.descriptors()[0];
        assert_eq!(
            descriptor.availability,
            VariableAvailability::PossiblyUnavailable
        );
        assert_eq!(
            descriptor.uncertainty_reasons,
            vec![VariableUncertaintyReason::MayBeNullIfNotFound]
        );
    }
}
