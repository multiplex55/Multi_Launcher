use super::{MkMacroDialog, typed_value};
use crate::mkmacro::{MkMacroOutput, MkMacroParameter, MkSignatureId, MkValueType};
use eframe::egui;

#[derive(Clone, Copy)]
enum DefinitionKind {
    Parameter,
    Output,
}

#[derive(Clone, Copy)]
enum StructuralEdit {
    Add(DefinitionKind),
    Delete(DefinitionKind, usize),
    Move(DefinitionKind, usize, bool),
}

fn type_selector(ui: &mut egui::Ui, id: impl std::hash::Hash, kind: &mut MkValueType) -> bool {
    let before = *kind;
    egui::ComboBox::from_id_source(id)
        .selected_text(kind.label())
        .show_ui(ui, |ui| {
            for candidate in [
                MkValueType::String,
                MkValueType::Number,
                MkValueType::Boolean,
                MkValueType::Point,
            ] {
                ui.selectable_value(kind, candidate, candidate.label());
            }
        });
    before != *kind
}

fn parameter_ui(
    ui: &mut egui::Ui,
    parameter: &mut MkMacroParameter,
    index: usize,
) -> Option<StructuralEdit> {
    let mut edit = None;
    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.strong(format!("Parameter {} · #{}", index + 1, parameter.id.0));
            if ui.small_button("↑").on_hover_text("Move up").clicked() {
                edit = Some(StructuralEdit::Move(
                    DefinitionKind::Parameter,
                    index,
                    false,
                ));
            }
            if ui.small_button("↓").on_hover_text("Move down").clicked() {
                edit = Some(StructuralEdit::Move(DefinitionKind::Parameter, index, true));
            }
            if ui.small_button("Delete").clicked() {
                edit = Some(StructuralEdit::Delete(DefinitionKind::Parameter, index));
            }
        });
        ui.horizontal(|ui| {
            ui.label("Name");
            ui.text_edit_singleline(&mut parameter.name);
        });
        ui.horizontal(|ui| {
            ui.label("Type");
            type_selector(
                ui,
                ("parameter_type", parameter.id),
                &mut parameter.value_type,
            );
        });
        ui.add(
            egui::TextEdit::multiline(&mut parameter.description)
                .desired_rows(2)
                .hint_text("Description"),
        );
        let mut has_default = parameter.default_value.is_some();
        if ui.checkbox(&mut has_default, "Default value").changed() {
            parameter.default_value =
                has_default.then(|| typed_value::initial_value(parameter.value_type));
        }
        if let Some(value) = &mut parameter.default_value {
            let response = typed_value::fixed_type_ui(ui, parameter.value_type, value);
            if !response.compatible
                && ui
                    .button(format!(
                        "Replace with {} default",
                        parameter.value_type.label()
                    ))
                    .clicked()
            {
                *value = typed_value::initial_value(parameter.value_type);
            }
        }
    });
    edit
}

fn output_ui(
    ui: &mut egui::Ui,
    output: &mut MkMacroOutput,
    index: usize,
) -> Option<StructuralEdit> {
    let mut edit = None;
    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.strong(format!("Output {} · #{}", index + 1, output.id.0));
            if ui.small_button("↑").on_hover_text("Move up").clicked() {
                edit = Some(StructuralEdit::Move(DefinitionKind::Output, index, false));
            }
            if ui.small_button("↓").on_hover_text("Move down").clicked() {
                edit = Some(StructuralEdit::Move(DefinitionKind::Output, index, true));
            }
            if ui.small_button("Delete").clicked() {
                edit = Some(StructuralEdit::Delete(DefinitionKind::Output, index));
            }
        });
        ui.horizontal(|ui| {
            ui.label("Name");
            ui.text_edit_singleline(&mut output.name);
        });
        ui.horizontal(|ui| {
            ui.label("Type");
            type_selector(ui, ("output_type", output.id), &mut output.value_type);
        });
        ui.add(
            egui::TextEdit::multiline(&mut output.description)
                .desired_rows(2)
                .hint_text("Description"),
        );
    });
    edit
}

fn apply_structural(dialog: &mut MkMacroDialog, macro_id: u64, edit: StructuralEdit) -> bool {
    if let StructuralEdit::Add(kind) = edit {
        let Some(id) = dialog.draft.next_signature_id(macro_id) else {
            return false;
        };
        let Some(owner) = dialog.draft.macros.iter_mut().find(|m| m.id == macro_id) else {
            return false;
        };
        match kind {
            DefinitionKind::Parameter => owner.signature.parameters.push(MkMacroParameter {
                id,
                name: format!("parameter_{}", id.0),
                value_type: MkValueType::String,
                description: String::new(),
                default_value: None,
            }),
            DefinitionKind::Output => owner.signature.outputs.push(MkMacroOutput {
                id,
                name: format!("output_{}", id.0),
                value_type: MkValueType::String,
                description: String::new(),
            }),
        }
        return true;
    }
    let Some(owner) = dialog.draft.macros.iter_mut().find(|m| m.id == macro_id) else {
        return false;
    };
    match edit {
        StructuralEdit::Delete(DefinitionKind::Parameter, index)
            if index < owner.signature.parameters.len() =>
        {
            owner.signature.parameters.remove(index);
            true
        }
        StructuralEdit::Delete(DefinitionKind::Output, index)
            if index < owner.signature.outputs.len() =>
        {
            owner.signature.outputs.remove(index);
            true
        }
        StructuralEdit::Move(DefinitionKind::Parameter, index, down) => {
            move_item(&mut owner.signature.parameters, index, down)
        }
        StructuralEdit::Move(DefinitionKind::Output, index, down) => {
            move_item(&mut owner.signature.outputs, index, down)
        }
        _ => false,
    }
}

fn move_item<T>(items: &mut [T], index: usize, down: bool) -> bool {
    let other = if down {
        index.checked_add(1)
    } else {
        index.checked_sub(1)
    };
    let Some(other) = other.filter(|other| *other < items.len()) else {
        return false;
    };
    items.swap(index, other);
    true
}

pub(super) fn show(ui: &mut egui::Ui, dialog: &mut MkMacroDialog, macro_id: u64) {
    let diagnostics = dialog.cached_diagnostics();
    let Some(index) = dialog.draft.macros.iter().position(|m| m.id == macro_id) else {
        return;
    };
    let before = dialog.draft.macros[index].signature.clone();
    let mut signature = before.clone();
    let mut structural = None;
    egui::CollapsingHeader::new("Reusable Signature").default_open(false).show(ui, |ui| {
        ui.small("Parameters are local inputs. Outputs are named values returned to callers. IDs remain stable when names or order change.");
        ui.heading("Parameters");
        for (index, parameter) in signature.parameters.iter_mut().enumerate() {
            let edit = parameter_ui(ui, parameter, index);
            if structural.is_none() {
                structural = edit;
            }
        }
        if ui.button("+ Add Parameter").clicked() { structural = Some(StructuralEdit::Add(DefinitionKind::Parameter)); }
        ui.separator();
        ui.heading("Outputs");
        for (index, output) in signature.outputs.iter_mut().enumerate() {
            let edit = output_ui(ui, output, index);
            if structural.is_none() {
                structural = edit;
            }
        }
        if ui.button("+ Add Output").clicked() { structural = Some(StructuralEdit::Add(DefinitionKind::Output)); }
        for diagnostic in diagnostics.iter().filter(|d| d.macro_id == macro_id && d.step_id.is_none() && matches!(d.code, "invalid_signature_id" | "invalid_signature_name" | "duplicate_signature_name" | "invalid_parameter_default")) {
            ui.colored_label(ui.visuals().error_fg_color, &diagnostic.message);
        }
    });
    let mut changed = false;
    if signature != before {
        dialog.draft.macros[index].signature = signature;
        changed = true;
    }
    if let Some(edit) = structural {
        changed |= apply_structural(dialog, macro_id, edit);
    }
    if changed {
        dialog.mark_dirty();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::*;

    #[test]
    fn rename_reorder_and_delete_preserve_ids_and_dangling_bindings() {
        let mut doc = MkMacroDocument::default();
        let mut target = MkMacro {
            id: 2,
            name: "target".into(),
            description: String::new(),
            enabled: true,
            hotkey: None,
            hotkey_scope: Default::default(),
            folder_id: None,
            playback: Default::default(),
            signature: Default::default(),
            steps: vec![],
        };
        target.signature.parameters = vec![
            MkMacroParameter {
                id: MkSignatureId(4),
                name: "a".into(),
                value_type: MkValueType::String,
                description: String::new(),
                default_value: Some(MkValue::String("preserve".into())),
            },
            MkMacroParameter {
                id: MkSignatureId(8),
                name: "b".into(),
                value_type: MkValueType::Number,
                description: String::new(),
                default_value: None,
            },
        ];
        let mut caller = MkMacro {
            id: 1,
            name: "caller".into(),
            description: String::new(),
            enabled: true,
            hotkey: None,
            hotkey_scope: Default::default(),
            folder_id: None,
            playback: Default::default(),
            signature: Default::default(),
            steps: vec![],
        };
        caller.steps.push(MkStep {
            id: 1,
            enabled: true,
            breakpoint: false,
            repeat: 1,
            delay_after_ms: 0,
            on_error: Default::default(),
            metadata: Default::default(),
            action: MkAction::CallMacro(MkCallMacroPayload {
                macro_id: 2,
                arguments: vec![MkCallArgumentBinding {
                    parameter_id: MkSignatureId(4),
                    source: MkValueSource::Literal(MkValue::String("x".into())),
                }],
                outputs: vec![],
            }),
        });
        doc.macros = vec![caller, target];
        doc.macros[1].signature.parameters[0].name = "renamed".into();
        doc.macros[1].signature.parameters[0].value_type = MkValueType::Number;
        assert_eq!(
            doc.macros[1].signature.parameters[0].default_value,
            Some(MkValue::String("preserve".into()))
        );
        assert!(move_item(&mut doc.macros[1].signature.parameters, 0, true));
        assert_eq!(
            doc.macros[1]
                .signature
                .parameters
                .iter()
                .map(|p| p.id)
                .collect::<Vec<_>>(),
            [MkSignatureId(8), MkSignatureId(4)]
        );
        doc.macros[1].signature.parameters.pop();
        let MkAction::CallMacro(call) = &doc.macros[0].steps[0].action else {
            unreachable!()
        };
        assert_eq!(call.arguments[0].parameter_id, MkSignatureId(4));
        assert_eq!(doc.next_signature_id(2), Some(MkSignatureId(9)));
    }
}
