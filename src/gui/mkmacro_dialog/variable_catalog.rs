//! Authoring-time inventory of variables produced by earlier macro steps.
//!
//! This deliberately contains descriptors, not runtime values.  It is safe to
//! rebuild whenever a picker opens and is never serialized with a macro.

use super::action_catalog::action_name;
use crate::mkmacro::{MkAction, MkImageNotFoundPolicy, MkImageOutputs, MkStep, MkValue};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariableDescriptor {
    pub name: String,
    pub value_type: VariableValueType,
    pub source_step_id: u64,
    pub source_step_index: usize,
    pub source_action_label: &'static str,
    pub availability: VariableAvailability,
    /// Extra picker guidance, including when a value can be Null at runtime.
    pub help_text: Option<&'static str>,
}

/// Source-ordered history of all variable definitions visible at a location.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VariableCatalog {
    descriptors: Vec<VariableDescriptor>,
}

impl VariableCatalog {
    /// Builds the catalog immediately before `consumer_index`.
    ///
    /// Out-of-range indices are consistently clamped to `steps.len()`, making
    /// `usize::MAX` useful for asking for the catalog at the end of a macro.
    pub fn before_step(steps: &[MkStep], consumer_index: usize) -> Self {
        let end = consumer_index.min(steps.len());
        let descriptors = steps[..end]
            .iter()
            .enumerate()
            .flat_map(|(index, step)| descriptors_for_action(step, index))
            .collect();
        Self { descriptors }
    }

    pub fn descriptors(&self) -> &[VariableDescriptor] {
        &self.descriptors
    }

    /// Returns one descriptor per name, choosing its latest definition while
    /// retaining the deterministic source order of those winning definitions.
    pub fn latest_definitions(&self) -> Vec<&VariableDescriptor> {
        let mut latest = Vec::new();
        for descriptor in self.descriptors.iter().rev() {
            if !latest
                .iter()
                .any(|existing: &&VariableDescriptor| existing.name == descriptor.name)
            {
                latest.push(descriptor);
            }
        }
        latest.reverse();
        latest
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
                source_action_label: label,
                availability,
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
        AlphaPolicy, MkFileCollisionPolicy, MkImagePayload, MkPixelSearchPayload, MkPoint,
        MkPromptInputPayload, MkScreenshotDestination, MkScreenshotFormat, MkScreenshotPayload,
        MkWaitOptions, ReturnPoint, SearchRegion,
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
}
