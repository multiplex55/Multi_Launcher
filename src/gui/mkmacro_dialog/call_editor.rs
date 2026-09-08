use super::{
    action_editor::variable_picker_ui,
    typed_value,
    variable_catalog::{VariableCatalog, VariableValueType},
};
use crate::mkmacro::*;
use eframe::egui;
use std::collections::HashSet;

#[derive(Default)]
pub(super) struct CallUiOutcome {
    pub open_target: Option<u64>,
}

fn source_ui(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + Copy,
    source: &mut MkValueSource,
    expected: Option<MkValueType>,
    catalog: &VariableCatalog,
) {
    let variable = matches!(source, MkValueSource::Variable { .. });
    let mut next_variable = variable;
    ui.horizontal(|ui| {
        ui.selectable_value(&mut next_variable, false, "Literal");
        ui.selectable_value(&mut next_variable, true, "Variable");
    });
    if next_variable != variable {
        *source = if next_variable {
            MkValueSource::Variable {
                name: String::new(),
            }
        } else {
            MkValueSource::Literal(
                expected
                    .map(typed_value::initial_value)
                    .unwrap_or(MkValue::Null),
            )
        };
    }
    match source {
        MkValueSource::Literal(value) => {
            if let Some(kind) = expected {
                let response = typed_value::fixed_type_ui(ui, kind, value);
                if !response.compatible
                    && ui
                        .button(format!("Replace with {} value", kind.label()))
                        .clicked()
                {
                    *value = typed_value::initial_value(kind);
                }
            } else {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    "The referenced definition no longer exists; this literal is preserved.",
                );
                typed_value::value_controls(ui, value);
            }
        }
        MkValueSource::Variable { name } => variable_picker_ui(ui, id, name, catalog, |actual| {
            expected.is_none_or(|expected| actual == VariableValueType::Known(expected))
        }),
    }
}

fn target_caption(document: &MkMacroDocument, id: u64) -> String {
    document.macros.iter().find(|m| m.id == id).map_or_else(
        || {
            if id == 0 {
                "Choose a macro…".into()
            } else {
                format!("Missing macro #{id}")
            }
        },
        |m| {
            format!(
                "{} · #{}{}",
                m.name,
                m.id,
                if m.enabled { "" } else { " · disabled" }
            )
        },
    )
}

fn replace_target(call: &mut MkCallMacroPayload, target: u64) {
    call.macro_id = target;
    call.arguments.clear();
    call.outputs.clear();
}

pub(super) fn call_ui(
    ui: &mut egui::Ui,
    call: &mut MkCallMacroPayload,
    document: &MkMacroDocument,
    caller_id: u64,
    catalog: &VariableCatalog,
    search: &mut String,
    pending_target: &mut Option<u64>,
) -> CallUiOutcome {
    let mut outcome = CallUiOutcome::default();
    ui.heading("Call Macro");
    ui.label(target_caption(document, call.macro_id));
    ui.horizontal(|ui| {
        ui.label("Find target");
        ui.text_edit_singleline(search);
    });
    let query = search.trim().to_lowercase();
    egui::ScrollArea::vertical()
        .max_height(150.0)
        .show(ui, |ui| {
            for target in &document.macros {
                let folder = target
                    .folder_id
                    .and_then(|id| document.folders.iter().find(|f| f.id == id))
                    .map(|f| f.name.as_str())
                    .unwrap_or("Unfiled");
                let searchable =
                    format!("{} {} {}", target.name, folder, target.description).to_lowercase();
                if !query.is_empty() && !searchable.contains(&query) {
                    continue;
                }
                let label = format!(
                    "{} · {}{}",
                    target.name,
                    folder,
                    if target.enabled { "" } else { " · disabled" }
                );
                if ui
                    .add_enabled(
                        target.id != caller_id,
                        egui::SelectableLabel::new(*pending_target == Some(target.id), label),
                    )
                    .on_hover_text(&target.description)
                    .clicked()
                {
                    *pending_target = Some(target.id);
                }
            }
        });
    if let Some(target) = *pending_target {
        ui.colored_label(egui::Color32::YELLOW, "Changing target clears all argument and output bindings so equal numeric IDs cannot be reinterpreted.");
        ui.horizontal(|ui| {
            if ui.button("Replace Target & Clear Bindings").clicked() {
                replace_target(call, target);
                *pending_target = None;
            }
            if ui.button("Cancel Target Change").clicked() {
                *pending_target = None;
            }
        });
    }
    if document.macros.iter().any(|m| m.id == call.macro_id)
        && ui.button("Apply and Open Target").clicked()
    {
        outcome.open_target = Some(call.macro_id);
    }

    let target = document.macros.iter().find(|m| m.id == call.macro_id);
    let parameters = target
        .map(|m| m.signature.parameters.as_slice())
        .unwrap_or_default();
    let outputs = target
        .map(|m| m.signature.outputs.as_slice())
        .unwrap_or_default();
    ui.separator();
    ui.heading("Arguments");
    let mut seen = HashSet::new();
    let mut remove = None;
    for (index, binding) in call.arguments.iter_mut().enumerate() {
        let definition = parameters.iter().find(|p| p.id == binding.parameter_id);
        let duplicate = !seen.insert(binding.parameter_id);
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.strong(definition.map_or_else(
                    || format!("Obsolete parameter #{}", binding.parameter_id.0),
                    |p| format!("{} · #{} · {}", p.name, p.id.0, p.value_type.label()),
                ));
                if duplicate {
                    ui.colored_label(ui.visuals().error_fg_color, "duplicate");
                }
                if ui.small_button("Remove").clicked() {
                    remove = Some(index);
                }
            });
            source_ui(
                ui,
                ("call_argument", index),
                &mut binding.source,
                definition.map(|p| p.value_type),
                catalog,
            );
        });
    }
    if let Some(index) = remove {
        call.arguments.remove(index);
    }
    for parameter in parameters {
        if !call
            .arguments
            .iter()
            .any(|b| b.parameter_id == parameter.id)
        {
            ui.horizontal(|ui| {
                ui.label(format!(
                    "{} · {}{}",
                    parameter.name,
                    parameter.value_type.label(),
                    if parameter.default_value.is_some() {
                        " · uses default when omitted"
                    } else {
                        " · required"
                    }
                ));
                if ui.small_button("Bind").clicked() {
                    call.arguments.push(MkCallArgumentBinding {
                        parameter_id: parameter.id,
                        source: MkValueSource::Literal(typed_value::initial_value(
                            parameter.value_type,
                        )),
                    });
                }
            });
        }
    }

    ui.separator();
    ui.heading("Output mappings");
    let mut seen = HashSet::new();
    let mut remove = None;
    for (index, binding) in call.outputs.iter_mut().enumerate() {
        let definition = outputs.iter().find(|o| o.id == binding.output_id);
        let duplicate = !seen.insert(binding.output_id);
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.strong(definition.map_or_else(
                    || format!("Obsolete output #{}", binding.output_id.0),
                    |o| format!("{} · #{} · {}", o.name, o.id.0, o.value_type.label()),
                ));
                if duplicate {
                    ui.colored_label(ui.visuals().error_fg_color, "duplicate");
                }
                if ui.small_button("Remove").clicked() {
                    remove = Some(index);
                }
            });
            ui.horizontal(|ui| {
                ui.label("Caller variable");
                ui.text_edit_singleline(&mut binding.caller_variable);
            });
        });
    }
    if let Some(index) = remove {
        call.outputs.remove(index);
    }
    for output in outputs {
        if !call.outputs.iter().any(|b| b.output_id == output.id)
            && ui
                .button(format!(
                    "Map {} ({})",
                    output.name,
                    output.value_type.label()
                ))
                .clicked()
        {
            call.outputs.push(MkCallOutputBinding {
                output_id: output.id,
                caller_variable: output.name.clone(),
            });
        }
    }
    outcome
}

pub(super) fn return_ui(
    ui: &mut egui::Ui,
    payload: &mut MkReturnPayload,
    signature: &MkMacroSignature,
    catalog: &VariableCatalog,
) {
    ui.heading("Return");
    if signature.outputs.is_empty() {
        ui.label("This macro declares no outputs. Return ends the current macro immediately.");
    }
    let mut seen = HashSet::new();
    let mut remove = None;
    for (index, binding) in payload.outputs.iter_mut().enumerate() {
        let definition = signature.outputs.iter().find(|o| o.id == binding.output_id);
        let duplicate = !seen.insert(binding.output_id);
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.strong(definition.map_or_else(
                    || format!("Obsolete output #{}", binding.output_id.0),
                    |o| format!("{} · #{} · {}", o.name, o.id.0, o.value_type.label()),
                ));
                if duplicate {
                    ui.colored_label(ui.visuals().error_fg_color, "duplicate");
                }
                if ui.small_button("Remove").clicked() {
                    remove = Some(index);
                }
            });
            source_ui(
                ui,
                ("return_output", index),
                &mut binding.source,
                definition.map(|o| o.value_type),
                catalog,
            );
        });
    }
    if let Some(index) = remove {
        payload.outputs.remove(index);
    }
    for output in &signature.outputs {
        if !payload.outputs.iter().any(|b| b.output_id == output.id)
            && ui
                .button(format!(
                    "Provide {} ({})",
                    output.name,
                    output.value_type.label()
                ))
                .clicked()
        {
            payload.outputs.push(MkReturnValueBinding {
                output_id: output.id,
                source: MkValueSource::Literal(typed_value::initial_value(output.value_type)),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn target_replacement_is_explicit_and_clears_bindings() {
        let mut call = MkCallMacroPayload {
            macro_id: 2,
            arguments: vec![MkCallArgumentBinding {
                parameter_id: MkSignatureId(1),
                source: MkValueSource::Literal(MkValue::String("old".into())),
            }],
            outputs: vec![MkCallOutputBinding {
                output_id: MkSignatureId(1),
                caller_variable: "old".into(),
            }],
        };
        replace_target(&mut call, 3);
        assert_eq!(call.macro_id, 3);
        assert!(call.arguments.is_empty() && call.outputs.is_empty());
    }

    #[test]
    fn target_caption_resolves_live_name_and_missing_or_disabled_state_by_id() {
        let mut document: MkMacroDocument = serde_json::from_value(serde_json::json!({
            "macros": [{"id": 2, "name": "Before", "enabled": false}]
        }))
        .unwrap();
        assert!(target_caption(&document, 2).contains("Before"));
        assert!(target_caption(&document, 2).contains("disabled"));
        document.macros[0].name = "After".into();
        assert!(target_caption(&document, 2).contains("After"));
        assert_eq!(target_caption(&document, 99), "Missing macro #99");
    }
}
