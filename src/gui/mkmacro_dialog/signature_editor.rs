use super::{MkMacroDialog, typed_value};
use crate::mkmacro::{
    MK_PARAMETER_CHOICE_MAX_OPTIONS, MkMacroOutput, MkMacroParameter, MkParameterInputKind,
    MkSignatureId, MkValue, MkValueType,
};
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

#[derive(Clone, Copy)]
enum ChoiceEdit {
    Add,
    Remove(usize),
    Move(usize, bool),
}

fn input_kind_label(kind: &MkParameterInputKind) -> String {
    match kind {
        MkParameterInputKind::Text => "Text".into(),
        MkParameterInputKind::File => "File picker".into(),
        MkParameterInputKind::Folder => "Folder picker".into(),
        MkParameterInputKind::Choice(options) => format!("Choice list ({} options)", options.len()),
    }
}

fn string_input_kind_label(kind: &MkParameterInputKind) -> &'static str {
    match kind {
        MkParameterInputKind::Text => "Text",
        MkParameterInputKind::File => "File picker",
        MkParameterInputKind::Folder => "Folder picker",
        MkParameterInputKind::Choice(_) => "Choice list",
    }
}

fn choice_options_ui(ui: &mut egui::Ui, macro_id: u64, parameter: &mut MkMacroParameter) {
    let MkParameterInputKind::Choice(options) = &mut parameter.input_kind else {
        return;
    };

    ui.small("Options are literal strings and are passed unchanged at runtime.");
    egui::ScrollArea::vertical()
        .id_salt(("signature_choice_options", macro_id, parameter.id))
        .max_height(132.0)
        .show(ui, |ui| {
            let mut edit = None;
            let options_len = options.len();
            for (index, option) in options.iter_mut().enumerate() {
                ui.push_id(
                    ("signature_choice_option", macro_id, parameter.id, index),
                    |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(format!("Option {}", index + 1));
                            ui.add(
                                egui::TextEdit::singleline(option)
                                    .hint_text(format!("Choice option {}", index + 1))
                                    .desired_width(ui.available_width().max(1.0)),
                            );
                            if ui
                                .add_enabled(index > 0, egui::Button::new("Move up"))
                                .on_hover_text("Move option up")
                                .clicked()
                            {
                                edit = Some(ChoiceEdit::Move(index, false));
                            }
                            if ui
                                .add_enabled(
                                    index + 1 < options_len,
                                    egui::Button::new("Move down"),
                                )
                                .on_hover_text("Move option down")
                                .clicked()
                            {
                                edit = Some(ChoiceEdit::Move(index, true));
                            }
                            if ui.small_button("Remove option").clicked() {
                                edit = Some(ChoiceEdit::Remove(index));
                            }
                        });
                    },
                );
            }
            if ui
                .add_enabled(
                    options.len() < MK_PARAMETER_CHOICE_MAX_OPTIONS,
                    egui::Button::new("Add option"),
                )
                .clicked()
            {
                edit = Some(ChoiceEdit::Add);
            }

            if let Some(edit) = edit {
                match edit {
                    ChoiceEdit::Add => options.push(String::new()),
                    ChoiceEdit::Remove(index) if index < options.len() => {
                        options.remove(index);
                    }
                    ChoiceEdit::Move(index, down) => {
                        move_item(options, index, down);
                    }
                    ChoiceEdit::Remove(_) => {}
                }
                // Row IDs use stable owner + position rather than editable text.
                // Retire any focused row before a structural edit can make that
                // positional ID refer to a different option on the next pass.
                if let Some(focused) = ui.memory(|memory| memory.focused()) {
                    ui.memory_mut(|memory| memory.surrender_focus(focused));
                }
            }
        });
}

fn choice_default_ui(ui: &mut egui::Ui, macro_id: u64, parameter: &mut MkMacroParameter) {
    let MkParameterInputKind::Choice(options) = &parameter.input_kind else {
        return;
    };
    let Some(MkValue::String(default)) = &mut parameter.default_value else {
        return;
    };
    ui.horizontal_wrapped(|ui| {
        ui.label("Default choice");
        egui::ComboBox::from_id_salt(("signature_choice_default", macro_id, parameter.id))
            .selected_text(default.as_str())
            .show_ui(ui, |ui| {
                for (index, option) in options.iter().enumerate() {
                    ui.push_id(("choice_default_option", index), |ui| {
                        ui.selectable_value(default, option.clone(), option);
                    });
                }
            });
    });
}

fn type_selector(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    kind: &mut MkValueType,
) -> bool {
    let before = *kind;
    egui::ComboBox::from_id_salt(id)
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
    macro_id: u64,
    parameter: &mut MkMacroParameter,
    index: usize,
) -> Option<StructuralEdit> {
    let mut edit = None;
    ui.push_id(("signature_parameter", macro_id, parameter.id), |ui| {
        ui.group(|ui| {
            ui.horizontal_wrapped(|ui| {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(format!(
                            "Parameter {} · #{}",
                            index + 1,
                            parameter.id.0
                        ))
                        .strong(),
                    )
                    .wrap(),
                );
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
            ui.horizontal_wrapped(|ui| {
                ui.label("Name");
                ui.add(
                    egui::TextEdit::singleline(&mut parameter.name)
                        .desired_width(ui.available_width().max(1.0)),
                );
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("Type");
                type_selector(
                    ui,
                    ("parameter_type", parameter.id),
                    &mut parameter.value_type,
                );
            });
            if parameter.value_type == MkValueType::String {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Input control");
                    egui::ComboBox::from_id_salt(("parameter_input_kind", macro_id, parameter.id))
                        .selected_text(string_input_kind_label(&parameter.input_kind))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut parameter.input_kind,
                                MkParameterInputKind::Text,
                                "Text",
                            );
                            ui.selectable_value(
                                &mut parameter.input_kind,
                                MkParameterInputKind::File,
                                "File picker",
                            );
                            ui.selectable_value(
                                &mut parameter.input_kind,
                                MkParameterInputKind::Folder,
                                "Folder picker",
                            );
                            let choice_selected =
                                matches!(&parameter.input_kind, MkParameterInputKind::Choice(_));
                            if ui
                                .selectable_label(choice_selected, "Choice list")
                                .clicked()
                                && !choice_selected
                            {
                                parameter.input_kind = MkParameterInputKind::Choice(Vec::new());
                            }
                        });
                });
                ui.small("File, Folder, and Choice controls still provide a String at runtime.");
                choice_options_ui(ui, macro_id, parameter);
            } else if !matches!(&parameter.input_kind, MkParameterInputKind::Text) {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Input control");
                    ui.label(input_kind_label(&parameter.input_kind));
                    if ui.button("Reset input control to Text").clicked() {
                        parameter.input_kind = MkParameterInputKind::Text;
                    }
                });
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    "This saved input control requires a String parameter.",
                );
                choice_options_ui(ui, macro_id, parameter);
            }
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
            let choice_string_default = parameter.value_type == MkValueType::String
                && matches!(&parameter.input_kind, MkParameterInputKind::Choice(_))
                && matches!(&parameter.default_value, Some(MkValue::String(_)));
            if choice_string_default {
                choice_default_ui(ui, macro_id, parameter);
            } else if let Some(value) = &mut parameter.default_value {
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
        })
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
        ui.horizontal_wrapped(|ui| {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(format!("Output {} · #{}", index + 1, output.id.0))
                        .strong(),
                )
                .wrap(),
            );
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
        ui.horizontal_wrapped(|ui| {
            ui.label("Name");
            ui.add(
                egui::TextEdit::singleline(&mut output.name)
                    .desired_width(ui.available_width().max(1.0)),
            );
        });
        ui.horizontal_wrapped(|ui| {
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
                input_kind: MkParameterInputKind::Text,
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
    ui.small("Parameters are local inputs. Outputs are named values returned to callers. IDs remain stable when names or order change.");
    ui.heading("Parameters");
    for (index, parameter) in signature.parameters.iter_mut().enumerate() {
        let edit = parameter_ui(ui, macro_id, parameter, index);
        if structural.is_none() {
            structural = edit;
        }
    }
    if ui
        .add(egui::Button::new("+ Add Parameter").wrap())
        .clicked()
    {
        structural = Some(StructuralEdit::Add(DefinitionKind::Parameter));
    }
    ui.separator();
    ui.heading("Outputs");
    for (index, output) in signature.outputs.iter_mut().enumerate() {
        let edit = output_ui(ui, output, index);
        if structural.is_none() {
            structural = edit;
        }
    }
    if ui.add(egui::Button::new("+ Add Output").wrap()).clicked() {
        structural = Some(StructuralEdit::Add(DefinitionKind::Output));
    }
    for diagnostic in diagnostics.iter().filter(|d| {
        d.macro_id == macro_id
            && d.step_id.is_none()
            && matches!(
                d.code,
                "invalid_signature_id"
                    | "invalid_signature_name"
                    | "duplicate_signature_name"
                    | "invalid_parameter_default"
                    | "invalid_parameter_input_kind"
                    | "invalid_parameter_choice_options"
                    | "invalid_parameter_choice_default"
            )
    }) {
        ui.colored_label(ui.visuals().error_fg_color, &diagnostic.message);
    }
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
    use std::{fs, sync::Arc};

    fn parameter(
        id: u64,
        value_type: MkValueType,
        input_kind: MkParameterInputKind,
        default_value: Option<MkValue>,
    ) -> MkMacroParameter {
        MkMacroParameter {
            id: MkSignatureId(id),
            name: "mode".into(),
            value_type,
            input_kind,
            description: "Mode used by the target".into(),
            default_value,
        }
    }

    fn macro_definition(id: u64, name: &str) -> MkMacro {
        MkMacro {
            id,
            name: name.into(),
            description: String::new(),
            enabled: true,
            hotkey: None,
            hotkey_scope: Default::default(),
            folder_id: None,
            playback: Default::default(),
            signature: Default::default(),
            steps: Vec::new(),
        }
    }

    fn seeded_dialog(parameters: Vec<MkMacroParameter>) -> (tempfile::TempDir, MkMacroDialog, u64) {
        let directory = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(directory.path()).unwrap();
        let parameter_id = parameters[0].id;
        let mut target = macro_definition(2, "Target");
        target.signature.parameters = parameters;
        let mut caller = macro_definition(1, "Caller");
        caller.steps.push(MkStep {
            id: 1,
            enabled: true,
            breakpoint: false,
            repeat: 1,
            delay_after_ms: 0,
            on_error: Default::default(),
            metadata: Default::default(),
            action: MkAction::CallMacro(MkCallMacroPayload {
                macro_id: target.id,
                arguments: vec![MkCallArgumentBinding {
                    parameter_id,
                    source: MkValueSource::Literal(MkValue::String("alpha".into())),
                }],
                outputs: Vec::new(),
            }),
        });
        store
            .save(MkMacroDocument {
                macros: vec![caller, target],
                ..Default::default()
            })
            .unwrap();
        let dialog = MkMacroDialog::new(Arc::new(store));
        (directory, dialog, 2)
    }

    fn screen_rect() -> egui::Rect {
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(820.0, 900.0))
    }

    fn render_signature(
        context: &egui::Context,
        dialog: &mut MkMacroDialog,
        macro_id: u64,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let screen_rect = screen_rect();
        context.run_ui(
            egui::RawInput {
                screen_rect: Some(screen_rect),
                focused: true,
                events,
                ..Default::default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    show(ui, dialog, macro_id);
                });
            },
        )
    }

    fn accessible_bounds(output: &egui::FullOutput, label: &str) -> Vec<egui::Rect> {
        let nodes = output
            .platform_output
            .accesskit_update
            .as_ref()
            .into_iter()
            .flat_map(|update| update.nodes.iter())
            .collect::<Vec<_>>();
        let bounds_for = |matches: &dyn Fn(&egui::accesskit::Node) -> bool| {
            nodes
                .iter()
                .filter_map(|(_, node)| {
                    if !matches(node) || node.is_disabled() {
                        return None;
                    }
                    let bounds = node.bounds()?;
                    let rect = egui::Rect::from_min_max(
                        egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                        egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                    );
                    (rect.is_positive() && screen_rect().contains(rect.center())).then_some(rect)
                })
                .collect::<Vec<_>>()
        };

        // ComboBox exposes its selected option as the node value, while an
        // empty TextEdit exposes its hint as a placeholder. Prefer labels so
        // a text field's current value cannot steal a click from a menu item
        // with the same text.
        let labeled = bounds_for(&|node| node.label() == Some(label));
        if !labeled.is_empty() {
            return labeled;
        }
        bounds_for(&|node| node.value() == Some(label) || node.placeholder() == Some(label))
    }

    fn click_point(
        context: &egui::Context,
        dialog: &mut MkMacroDialog,
        macro_id: u64,
        point: egui::Pos2,
    ) {
        let pressed = render_signature(
            context,
            dialog,
            macro_id,
            vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::default(),
                },
            ],
        );
        pressed.drop_without_applying_deltas();
        let released = render_signature(
            context,
            dialog,
            macro_id,
            vec![egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::default(),
            }],
        );
        released.drop_without_applying_deltas();
    }

    fn click_label(
        context: &egui::Context,
        dialog: &mut MkMacroDialog,
        macro_id: u64,
        label: &str,
        occurrence: usize,
    ) {
        let output = render_signature(context, dialog, macro_id, Vec::new());
        let mut bounds = accessible_bounds(&output, label);
        bounds.sort_by(|left, right| left.center().y.total_cmp(&right.center().y));
        output.drop_without_applying_deltas();
        let target = bounds
            .get(occurrence)
            .unwrap_or_else(|| panic!("missing accessible {label:?} occurrence {occurrence}"));
        click_point(context, dialog, macro_id, target.center());
    }

    fn enter_text(
        context: &egui::Context,
        dialog: &mut MkMacroDialog,
        macro_id: u64,
        label: &str,
        text: &str,
    ) {
        let output = render_signature(context, dialog, macro_id, Vec::new());
        let target = accessible_bounds(&output, label).into_iter().next();
        output.drop_without_applying_deltas();
        let target = target.unwrap_or_else(|| panic!("missing accessible text input {label:?}"));
        click_point(context, dialog, macro_id, target.center());
        let typed = render_signature(
            context,
            dialog,
            macro_id,
            vec![egui::Event::Text(text.into())],
        );
        typed.drop_without_applying_deltas();
    }

    fn macro_with_parameter(dialog: &MkMacroDialog, macro_id: u64) -> &MkMacro {
        dialog
            .draft
            .macros
            .iter()
            .find(|item| item.id == macro_id)
            .unwrap()
    }

    #[test]
    fn signature_input_ui_choice_options_are_editable_stable_and_persisted() {
        let (directory, mut dialog, macro_id) = seeded_dialog(vec![parameter(
            7,
            MkValueType::String,
            MkParameterInputKind::Text,
            Some(MkValue::String("alpha".into())),
        )]);
        let context = egui::Context::default();
        context.enable_accesskit();
        let before = dialog.draft.clone();
        let initial = render_signature(&context, &mut dialog, macro_id, Vec::new());
        let initially_clean = !dialog.dirty;
        let unchanged = dialog.draft == before;
        initial.drop_without_applying_deltas();
        assert!(
            initially_clean,
            "passive rendering must not dirty the signature"
        );
        assert_eq!(dialog.draft, before);
        assert!(unchanged);

        click_label(&context, &mut dialog, macro_id, "Text", 0);
        let menu = render_signature(&context, &mut dialog, macro_id, Vec::new());
        let has_file_picker = !accessible_bounds(&menu, "File picker").is_empty();
        let has_folder_picker = !accessible_bounds(&menu, "Folder picker").is_empty();
        let has_choice_list = !accessible_bounds(&menu, "Choice list").is_empty();
        menu.drop_without_applying_deltas();
        assert!(has_file_picker);
        assert!(has_folder_picker);
        assert!(has_choice_list);
        click_label(&context, &mut dialog, macro_id, "Choice list", 0);
        assert_eq!(
            macro_with_parameter(&dialog, macro_id).signature.parameters[0].input_kind,
            MkParameterInputKind::Choice(Vec::new())
        );
        assert!(dialog.dirty);

        click_label(&context, &mut dialog, macro_id, "Add option", 0);
        enter_text(&context, &mut dialog, macro_id, "Choice option 1", "alpha");
        click_label(&context, &mut dialog, macro_id, "Add option", 0);
        enter_text(&context, &mut dialog, macro_id, "Choice option 2", "beta");
        assert_eq!(
            macro_with_parameter(&dialog, macro_id).signature.parameters[0].input_kind,
            MkParameterInputKind::Choice(vec!["alpha".into(), "beta".into()])
        );
        click_label(&context, &mut dialog, macro_id, "Move up", 0);
        assert_eq!(
            macro_with_parameter(&dialog, macro_id).signature.parameters[0].input_kind,
            MkParameterInputKind::Choice(vec!["beta".into(), "alpha".into()])
        );
        click_label(&context, &mut dialog, macro_id, "Remove option", 0);
        assert_eq!(
            macro_with_parameter(&dialog, macro_id).signature.parameters[0].input_kind,
            MkParameterInputKind::Choice(vec!["alpha".into()])
        );
        assert_eq!(
            macro_with_parameter(&dialog, macro_id).signature.parameters[0].default_value,
            Some(MkValue::String("alpha".into())),
            "removing another option must not replace the authored default"
        );

        let MkAction::CallMacro(call) = &dialog.draft.macros[0].steps[0].action else {
            panic!("the original caller binding remains a Call Macro action")
        };
        assert_eq!(call.arguments[0].parameter_id, MkSignatureId(7));
        dialog.save().unwrap();
        let saved_bytes = fs::read(directory.path().join(crate::mkmacro::MKMACROS_FILE)).unwrap();
        let (reopened_store, _) = MkMacroStore::open(directory.path()).unwrap();
        let reopened = reopened_store.snapshot();
        assert_eq!(
            reopened.macros[1].signature.parameters[0].input_kind,
            MkParameterInputKind::Choice(vec!["alpha".into()])
        );
        let MkAction::CallMacro(reopened_call) = &reopened.macros[0].steps[0].action else {
            panic!("the saved caller binding remains a Call Macro action")
        };
        assert_eq!(reopened_call.arguments[0].parameter_id, MkSignatureId(7));

        click_label(&context, &mut dialog, macro_id, "Choice list", 0);
        click_label(&context, &mut dialog, macro_id, "File picker", 0);
        assert!(dialog.dirty);
        assert!(dialog.close_with_decision(super::super::DirtyDecision::Discard));
        assert_eq!(dialog.draft, *reopened);
        assert_eq!(
            fs::read(directory.path().join(crate::mkmacro::MKMACROS_FILE)).unwrap(),
            saved_bytes,
            "discarding a presentation edit must preserve the saved document bytes"
        );
    }

    #[test]
    fn signature_input_ui_removed_default_stays_visible_until_explicit_correction() {
        let (_directory, mut dialog, macro_id) = seeded_dialog(vec![parameter(
            7,
            MkValueType::String,
            MkParameterInputKind::Choice(vec!["alpha".into(), "beta".into()]),
            Some(MkValue::String("alpha".into())),
        )]);
        let context = egui::Context::default();
        context.enable_accesskit();
        let initial = render_signature(&context, &mut dialog, macro_id, Vec::new());
        let initially_clean = !dialog.dirty;
        initial.drop_without_applying_deltas();
        assert!(initially_clean);

        click_label(&context, &mut dialog, macro_id, "Remove option", 0);
        let parameter = &macro_with_parameter(&dialog, macro_id).signature.parameters[0];
        assert_eq!(
            parameter.input_kind,
            MkParameterInputKind::Choice(vec!["beta".into()])
        );
        assert_eq!(
            parameter.default_value,
            Some(MkValue::String("alpha".into()))
        );
        assert!(dialog.dirty);

        let invalid = render_signature(&context, &mut dialog, macro_id, Vec::new());
        let default_is_visible = !accessible_bounds(&invalid, "Default choice").is_empty();
        let invalid_default_is_visible = !accessible_bounds(&invalid, "alpha").is_empty();
        let labels = invalid
            .platform_output
            .accesskit_update
            .as_ref()
            .into_iter()
            .flat_map(|update| update.nodes.iter())
            .filter_map(|(_, node)| node.label().or(node.value()))
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let diagnostic_is_visible = labels
            .iter()
            .any(|label| label.contains("Default for choice parameter"));
        invalid.drop_without_applying_deltas();
        assert!(default_is_visible, "the default selector is accessible");
        assert!(
            invalid_default_is_visible,
            "the removed default remains visible"
        );
        assert!(
            diagnostic_is_visible,
            "the central invalid-default diagnostic remains visible: {labels:?}"
        );

        click_label(&context, &mut dialog, macro_id, "alpha", 0);
        click_label(&context, &mut dialog, macro_id, "beta", 0);
        assert_eq!(
            macro_with_parameter(&dialog, macro_id).signature.parameters[0].default_value,
            Some(MkValue::String("beta".into())),
            "only an explicit listed choice corrects the removed default"
        );
    }

    #[test]
    fn signature_input_ui_incompatible_nonstring_metadata_requires_explicit_reset() {
        let (_directory, mut dialog, macro_id) = seeded_dialog(vec![parameter(
            7,
            MkValueType::Number,
            MkParameterInputKind::Choice(vec!["one".into()]),
            Some(MkValue::Number(17.0)),
        )]);
        let context = egui::Context::default();
        context.enable_accesskit();
        let before = dialog.draft.clone();
        let initial = render_signature(&context, &mut dialog, macro_id, Vec::new());
        let initially_clean = !dialog.dirty;
        let unchanged = dialog.draft == before;
        let reset_is_visible =
            !accessible_bounds(&initial, "Reset input control to Text").is_empty();
        let option_editor_is_visible = !accessible_bounds(&initial, "Choice option 1").is_empty();
        initial.drop_without_applying_deltas();
        assert!(initially_clean);
        assert!(unchanged);
        assert!(reset_is_visible, "the explicit reset action is accessible");
        assert!(
            option_editor_is_visible,
            "the incompatible Choice metadata remains inspectable"
        );

        click_label(
            &context,
            &mut dialog,
            macro_id,
            "Reset input control to Text",
            0,
        );
        let parameter = &macro_with_parameter(&dialog, macro_id).signature.parameters[0];
        assert_eq!(parameter.input_kind, MkParameterInputKind::Text);
        assert_eq!(parameter.value_type, MkValueType::Number);
        assert_eq!(parameter.default_value, Some(MkValue::Number(17.0)));
        assert!(dialog.dirty);
    }

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
                input_kind: MkParameterInputKind::Text,
                description: String::new(),
                default_value: Some(MkValue::String("preserve".into())),
            },
            MkMacroParameter {
                id: MkSignatureId(8),
                name: "b".into(),
                value_type: MkValueType::Number,
                input_kind: MkParameterInputKind::Text,
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
