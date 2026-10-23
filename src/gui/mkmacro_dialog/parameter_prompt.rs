use crate::gui::theme::widgets::{StatusTone, semantic_status};
use crate::mkmacro::{
    MkInvocationValues,
    invocation_prompt::{
        InvocationGuiRegistration, InvocationPromptBroker, PendingInvocation,
        production_invocation_prompt_broker,
    },
    prepare_parameters,
};
use eframe::egui;
use std::{path::PathBuf, sync::Arc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParameterPathKind {
    File,
    Folder,
}

type ParameterPathChooser =
    Arc<dyn Fn(ParameterPathKind) -> Result<Option<PathBuf>, String> + Send + Sync>;

fn choose_parameter_path(kind: ParameterPathKind) -> Result<Option<PathBuf>, String> {
    let path = match kind {
        ParameterPathKind::File => rfd::FileDialog::new().pick_file(),
        ParameterPathKind::Folder => rfd::FileDialog::new().pick_folder(),
    };
    Ok(path)
}

fn prompt_values_valid(
    parameters: &[crate::mkmacro::MkMacroParameter],
    values: &MkInvocationValues,
) -> (bool, Option<String>) {
    match prepare_parameters(parameters, &[], values) {
        Ok(prepared) if prepared.missing.is_empty() => (true, None),
        Ok(prepared) => {
            let names = prepared
                .missing
                .iter()
                .map(|parameter| parameter.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            (
                false,
                Some(format!("Enter values for the required parameters: {names}")),
            )
        }
        Err(error) => (false, Some(error.message)),
    }
}

fn input_path_kind(kind: &crate::mkmacro::MkParameterInputKind) -> Option<ParameterPathKind> {
    match kind {
        crate::mkmacro::MkParameterInputKind::File => Some(ParameterPathKind::File),
        crate::mkmacro::MkParameterInputKind::Folder => Some(ParameterPathKind::Folder),
        crate::mkmacro::MkParameterInputKind::Text
        | crate::mkmacro::MkParameterInputKind::Choice(_) => None,
    }
}

fn parameter_value_ui(
    ui: &mut egui::Ui,
    request_id: u64,
    parameter: &crate::mkmacro::MkMacroParameter,
    value: &mut crate::mkmacro::MkValue,
    handle_ui_actions: bool,
    browse_request: &mut Option<(u64, crate::mkmacro::MkSignatureId, ParameterPathKind)>,
) -> (bool, bool) {
    if parameter.value_type != crate::mkmacro::MkValueType::String {
        let edit = super::typed_value::fixed_type_ui(ui, parameter.value_type, value);
        return (edit.changed, edit.compatible);
    }

    match &parameter.input_kind {
        crate::mkmacro::MkParameterInputKind::Text => {
            let edit = super::typed_value::fixed_type_ui(ui, parameter.value_type, value);
            (edit.changed, edit.compatible)
        }
        crate::mkmacro::MkParameterInputKind::File
        | crate::mkmacro::MkParameterInputKind::Folder => {
            let Some(kind) = input_path_kind(&parameter.input_kind) else {
                return (false, false);
            };
            let mut changed = false;
            let mut compatible = true;
            ui.horizontal_wrapped(|ui| {
                let edit = super::typed_value::fixed_type_ui(ui, parameter.value_type, value);
                changed |= edit.changed;
                compatible &= edit.compatible;
                let label = match kind {
                    ParameterPathKind::File => "Browse for file…",
                    ParameterPathKind::Folder => "Browse for folder…",
                };
                if ui
                    .add_enabled(handle_ui_actions, egui::Button::new(label))
                    .clicked()
                {
                    *browse_request = Some((request_id, parameter.id, kind));
                }
            });
            ui.small("Enter a path or choose one. Choosing a path does not run the macro.");
            (changed, compatible)
        }
        crate::mkmacro::MkParameterInputKind::Choice(options) => {
            let crate::mkmacro::MkValue::String(selected) = value else {
                let edit = super::typed_value::fixed_type_ui(ui, parameter.value_type, value);
                return (edit.changed, edit.compatible);
            };
            let mut changed = false;
            ui.horizontal(|ui| {
                ui.label("Value");
                let selected_text = if selected.is_empty() {
                    "Choose an option…".to_owned()
                } else {
                    selected.clone()
                };
                ui.add_enabled_ui(handle_ui_actions, |ui| {
                    egui::ComboBox::from_id_salt(("invocation_choice", request_id, parameter.id))
                        .selected_text(selected_text)
                        .show_ui(ui, |ui| {
                            for option in options {
                                changed |= ui
                                    .selectable_value(selected, option.clone(), option)
                                    .changed();
                            }
                        });
                });
            });
            ui.small("Choose one of the configured options.");
            (changed, true)
        }
    }
}

/// An app lifetime owns availability and the held request. Dropping either the
/// app or a replaced registration cancels only that registration's token.
pub(crate) struct ParameterPromptUi {
    registration: Option<InvocationGuiRegistration>,
    pending: Option<PendingInvocation>,
    values: MkInvocationValues,
    error: Option<String>,
    validation_error: Option<String>,
    browse_feedback: Option<String>,
    notice: Option<String>,
    first_frame: bool,
    valid: bool,
    bootstrap_pending: bool,
    chooser: ParameterPathChooser,
}
impl ParameterPromptUi {
    pub fn new(ctx: &egui::Context) -> Self {
        Self::with_broker(ctx, production_invocation_prompt_broker())
    }

    pub(crate) fn with_broker(ctx: &egui::Context, broker: Arc<InvocationPromptBroker>) -> Self {
        Self::with_chooser(ctx, broker, Arc::new(choose_parameter_path))
    }

    fn with_chooser(
        ctx: &egui::Context,
        broker: Arc<InvocationPromptBroker>,
        chooser: ParameterPathChooser,
    ) -> Self {
        let ctx = ctx.clone();
        Self {
            registration: Some(broker.register_gui(Arc::new(move || ctx.request_repaint()))),
            pending: None,
            values: Default::default(),
            error: None,
            validation_error: None,
            browse_feedback: None,
            notice: None,
            first_frame: false,
            valid: false,
            bootstrap_pending: false,
            chooser,
        }
    }

    #[cfg(test)]
    fn with_broker_and_chooser(
        ctx: &egui::Context,
        broker: Arc<InvocationPromptBroker>,
        chooser: impl Fn(ParameterPathKind) -> Result<Option<PathBuf>, String> + Send + Sync + 'static,
    ) -> Self {
        Self::with_chooser(ctx, broker, Arc::new(chooser))
    }
    pub fn shutdown(&mut self) {
        if let Some(registration) = self.registration.take() {
            registration.unregister();
        }
        self.pending = None;
        self.values.clear();
        self.error = None;
        self.validation_error = None;
        self.browse_feedback = None;
        self.notice = None;
        self.bootstrap_pending = false;
    }
    pub fn acquire_pending(&mut self) {
        let Some(registration) = &self.registration else {
            return;
        };
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| !pending.is_active())
        {
            self.pending = None;
            self.values.clear();
            self.error = None;
            self.validation_error = None;
            self.browse_feedback = None;
            self.bootstrap_pending = false;
        }
        if let Some(notice) = registration.take_notice() {
            self.notice = Some(notice);
            self.first_frame = true;
            self.bootstrap_pending = true;
        }
        if self.pending.is_none()
            && let Some(pending) = registration.take_pending()
        {
            self.values = pending.request().prepared_values.clone();
            for parameter in &pending.request().parameters {
                self.values
                    .entry(parameter.id)
                    .or_insert_with(|| super::typed_value::initial_value(parameter.value_type));
            }
            (self.valid, self.validation_error) =
                prompt_values_valid(&pending.request().parameters, &self.values);
            self.pending = Some(pending);
            self.error = None;
            self.browse_feedback = None;
            self.first_frame = true;
            self.bootstrap_pending = true;
        }
    }

    pub fn needs_viewport_bootstrap(&self) -> bool {
        self.bootstrap_pending
    }

    pub(crate) fn owns_input(&self) -> bool {
        self.pending.is_some() || self.notice.is_some()
    }

    pub fn show(&mut self, ctx: &egui::Context, handle_ui_actions: bool) {
        if self.pending.is_none() && self.notice.is_none() {
            return;
        }
        let mut cancel = false;
        let mut submit = false;
        let mut browse_request = None;
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("mkmacro_invocation_parameters"),
            egui::ViewportBuilder::default()
                .with_title("Macro parameters")
                .with_inner_size([460.0, 440.0])
                .with_min_inner_size([360.0, 240.0])
                .with_max_inner_size([720.0, 720.0])
                .with_always_on_top(),
            |child, _| {
                if self.first_frame {
                    child.send_viewport_cmd(egui::ViewportCommand::Focus);
                    self.first_frame = false;
                }
                egui::Panel::bottom("invocation_actions").show(child, |ui| {
                    ui.horizontal(|ui| {
                        if self.pending.is_some() {
                            submit = ui
                                .add_enabled(self.valid, egui::Button::new("Run"))
                                .clicked()
                                && handle_ui_actions;
                            cancel = ui.button("Cancel").clicked() && handle_ui_actions;
                        } else {
                            cancel = ui.button("Close").clicked() && handle_ui_actions;
                        }
                    });
                });
                egui::CentralPanel::default().show(child, |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .max_height(ui.available_height())
                        .show(ui, |ui| {
                            if let Some(notice) = &self.notice {
                                semantic_status(ui, StatusTone::Error, notice, None);
                                ui.separator();
                            }
                            if let Some(pending) = &self.pending {
                                let request = pending.request();
                                ui.heading(&request.macro_name);
                                if !request.macro_description.is_empty() {
                                    ui.label(&request.macro_description);
                                }
                                let mut changed = false;
                                for parameter in &request.parameters {
                                    ui.push_id(
                                        ("invocation_parameter", request.id, parameter.id),
                                        |ui| {
                                            ui.group(|ui| {
                                                ui.strong(format!(
                                                    "{} ({})",
                                                    parameter.name,
                                                    parameter.value_type.label()
                                                ));
                                                if !parameter.description.is_empty() {
                                                    ui.label(&parameter.description);
                                                }
                                                ui.small(match &parameter.default_value {
                                                    Some(value) => format!(
                                                        "Default: {}",
                                                        super::runtime_inspector::format_value(
                                                            value
                                                        )
                                                        .hover_text
                                                    ),
                                                    None => "Required • no default".into(),
                                                });
                                                if let Some(value) =
                                                    self.values.get_mut(&parameter.id)
                                                {
                                                    let (value_changed, compatible) =
                                                        parameter_value_ui(
                                                            ui,
                                                            request.id,
                                                            parameter,
                                                            value,
                                                            handle_ui_actions,
                                                            &mut browse_request,
                                                        );
                                                    changed |= value_changed;
                                                    if !compatible {
                                                        self.valid = false;
                                                    }
                                                }
                                            });
                                        },
                                    );
                                }
                                if changed {
                                    (self.valid, self.validation_error) =
                                        prompt_values_valid(&request.parameters, &self.values);
                                    self.error = None;
                                    self.browse_feedback = None;
                                }
                                if let Some(error) = &self.error {
                                    semantic_status(ui, StatusTone::Error, error, None);
                                }
                                if let Some(error) = &self.validation_error {
                                    semantic_status(ui, StatusTone::Error, error, None);
                                }
                                if let Some(feedback) = &self.browse_feedback {
                                    semantic_status(ui, StatusTone::Info, feedback, None);
                                }
                            }
                        });
                });
                if handle_ui_actions
                    && child.input(|input| {
                        input.key_pressed(egui::Key::Escape) || input.viewport().close_requested()
                    })
                {
                    cancel = true;
                }
            },
        );
        self.bootstrap_pending = false;
        if cancel {
            self.pending = None;
            self.values.clear();
            self.error = None;
            self.validation_error = None;
            self.browse_feedback = None;
            self.notice = None;
        } else if let Some((request_id, parameter_id, kind)) = browse_request {
            self.choose_path(ctx, request_id, parameter_id, kind);
        } else if submit && let Some(pending) = &mut self.pending {
            match pending.submit(self.values.clone()) {
                Ok(()) => {
                    self.pending = None;
                    self.values.clear();
                    self.error = None;
                    self.validation_error = None;
                    self.browse_feedback = None;
                    self.notice = None;
                }
                Err(error) => self.error = Some(error.to_string()),
            }
        }
    }

    fn choose_path(
        &mut self,
        ctx: &egui::Context,
        request_id: u64,
        parameter_id: crate::mkmacro::MkSignatureId,
        kind: ParameterPathKind,
    ) {
        let Some(pending) = self.pending.as_ref() else {
            return;
        };
        let valid_request = pending.request().id == request_id
            && pending.is_active()
            && pending.request().parameters.iter().any(|parameter| {
                parameter.id == parameter_id
                    && parameter.value_type == crate::mkmacro::MkValueType::String
                    && input_path_kind(&parameter.input_kind) == Some(kind)
            });
        if !valid_request {
            self.clear_inactive_request(request_id);
            return;
        }

        self.error = None;
        self.browse_feedback = None;
        let chooser = Arc::clone(&self.chooser);
        let result = chooser(kind);

        // The native picker is modal and may outlive this broker lease. Never
        // apply its return value to a cancelled or replaced invocation.
        let still_current = self.pending.as_ref().is_some_and(|pending| {
            pending.request().id == request_id
                && pending.is_active()
                && pending.request().parameters.iter().any(|parameter| {
                    parameter.id == parameter_id
                        && parameter.value_type == crate::mkmacro::MkValueType::String
                        && input_path_kind(&parameter.input_kind) == Some(kind)
                })
        });
        if !still_current {
            self.clear_inactive_request(request_id);
            ctx.request_repaint();
            return;
        }

        match result {
            Ok(Some(path)) => match path.into_os_string().into_string() {
                Ok(path) => {
                    if let Some(crate::mkmacro::MkValue::String(value)) =
                        self.values.get_mut(&parameter_id)
                    {
                        *value = path;
                        if let Some(pending) = &self.pending {
                            (self.valid, self.validation_error) =
                                prompt_values_valid(&pending.request().parameters, &self.values);
                        }
                    } else {
                        self.error = Some(
                            "The selected parameter is no longer editable. You can enter a path directly."
                                .into(),
                        );
                    }
                }
                Err(_) => {
                    self.error = Some(
                        "The selected path cannot be represented as text. You can enter a path directly."
                            .into(),
                    );
                }
            },
            Ok(None) => {
                self.browse_feedback = Some(
                    "No path was selected. You can enter a path or choose Browse again.".into(),
                );
            }
            Err(error) => {
                self.error = Some(format!(
                    "The path picker failed: {error}. You can enter a path directly."
                ));
            }
        }
        ctx.request_repaint();
    }

    fn clear_inactive_request(&mut self, request_id: u64) {
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.request().id == request_id)
        {
            self.pending = None;
            self.values.clear();
            self.valid = false;
            self.error = None;
            self.validation_error = None;
            self.browse_feedback = None;
            self.bootstrap_pending = false;
        }
    }
}

impl Drop for ParameterPromptUi {
    fn drop(&mut self) {
        // `eframe::App::on_exit` is the normal path. This also covers test
        // teardown and hosts that drop the app without invoking that hook.
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::{
        MkAction, MkMacro, MkMacroDocument, MkMacroParameter, MkMacroSignature, MkMacroStore,
        MkParameterInputKind, MkSignatureId, MkStep, MkTextMode, MkTextPayload, MkValue,
        MkValueType,
        executor::fake::FakeBackend,
        invocation_prompt::InvocationPromptBroker,
        runtime::{InvocationDisposition, RuntimeCommand, RuntimeState},
    };
    use std::{
        collections::{BTreeMap, VecDeque},
        sync::{Arc, Mutex},
        time::{Duration, Instant},
    };

    struct PromptHarness {
        _directory: tempfile::TempDir,
        runtime: Arc<crate::mkmacro::MacroRuntime>,
        backend: Arc<FakeBackend>,
        broker: Arc<InvocationPromptBroker>,
        ctx: egui::Context,
        prompt: ParameterPromptUi,
    }

    fn parameter(
        id: u64,
        name: &str,
        input_kind: MkParameterInputKind,
        default_value: Option<MkValue>,
    ) -> MkMacroParameter {
        MkMacroParameter {
            id: MkSignatureId(id),
            name: name.into(),
            description: format!("Value for {name}"),
            value_type: MkValueType::String,
            input_kind,
            default_value,
        }
    }

    fn parameters() -> Vec<MkMacroParameter> {
        vec![
            parameter(1, "file", MkParameterInputKind::File, None),
            parameter(2, "folder", MkParameterInputKind::Folder, None),
            parameter(
                3,
                "choice",
                MkParameterInputKind::Choice(vec!["Second".into(), "First".into()]),
                None,
            ),
        ]
    }

    fn harness(
        parameters: Vec<MkMacroParameter>,
        chooser: impl Fn(ParameterPathKind) -> Result<Option<PathBuf>, String> + Send + Sync + 'static,
    ) -> PromptHarness {
        harness_with_broker(
            parameters,
            Arc::new(InvocationPromptBroker::default()),
            chooser,
        )
    }

    fn harness_with_broker(
        parameters: Vec<MkMacroParameter>,
        broker: Arc<InvocationPromptBroker>,
        chooser: impl Fn(ParameterPathKind) -> Result<Option<PathBuf>, String> + Send + Sync + 'static,
    ) -> PromptHarness {
        let directory = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(directory.path()).unwrap();
        let text = parameters
            .iter()
            .map(|parameter| format!("{}=${{{}}}", parameter.name, parameter.name))
            .collect::<Vec<_>>()
            .join("; ");
        store
            .save(MkMacroDocument {
                macros: vec![MkMacro {
                    id: 1,
                    name: "Prompt fixture".into(),
                    description: "Invocation prompt test".into(),
                    enabled: true,
                    hotkey: None,
                    hotkey_scope: Default::default(),
                    folder_id: None,
                    playback: Default::default(),
                    signature: MkMacroSignature {
                        parameters,
                        outputs: Vec::new(),
                    },
                    steps: vec![MkStep {
                        id: 1,
                        enabled: true,
                        breakpoint: false,
                        repeat: 1,
                        delay_after_ms: 0,
                        on_error: Default::default(),
                        metadata: Default::default(),
                        action: MkAction::Text(MkTextPayload {
                            text,
                            mode: MkTextMode::Type,
                        }),
                    }],
                }],
                ..Default::default()
            })
            .unwrap();
        let store = Arc::new(store);
        let backend = Arc::new(FakeBackend::default());
        let runtime = Arc::new(crate::mkmacro::MacroRuntime::new(
            store,
            backend.clone().backends(),
        ));
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut prompt = ParameterPromptUi::with_broker_and_chooser(&ctx, broker.clone(), chooser);
        assert!(matches!(
            runtime.prepare_invocation(crate::mkmacro::MkInvocation::new(1), &broker),
            Ok(InvocationDisposition::AwaitingInput { .. })
        ));
        prompt.acquire_pending();
        let mut harness = PromptHarness {
            _directory: directory,
            runtime,
            backend,
            broker,
            ctx,
            prompt,
        };
        // The child viewport's first immediate pass establishes its layout;
        // tests interact only with the following frame's real responses.
        frame(&mut harness, Vec::new()).drop_without_applying_deltas();
        harness
    }

    fn screen_rect() -> egui::Rect {
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 820.0))
    }

    fn frame(harness: &mut PromptHarness, events: Vec<egui::Event>) -> egui::FullOutput {
        let ctx = harness.ctx.clone();
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(screen_rect()),
                focused: true,
                events,
                ..Default::default()
            },
            |ctx| {
                harness.prompt.acquire_pending();
                harness.prompt.show(ctx, true);
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
        let bounds_for = |predicate: &dyn Fn(&egui::accesskit::Node) -> bool| {
            nodes
                .iter()
                .filter_map(|(_, node)| {
                    if !predicate(node) || node.is_disabled() {
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
        let labeled = bounds_for(&|node| node.label() == Some(label));
        if !labeled.is_empty() {
            return labeled;
        }
        bounds_for(&|node| node.value() == Some(label) || node.placeholder() == Some(label))
    }

    fn text_input_bounds(output: &egui::FullOutput) -> Vec<egui::Rect> {
        output
            .platform_output
            .accesskit_update
            .as_ref()
            .into_iter()
            .flat_map(|update| update.nodes.iter())
            .filter_map(|(_, node)| {
                if node.role() != egui::accesskit::Role::TextInput || node.is_disabled() {
                    return None;
                }
                let bounds = node.bounds()?;
                let rect = egui::Rect::from_min_max(
                    egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                    egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                );
                (rect.is_positive() && screen_rect().contains(rect.center())).then_some(rect)
            })
            .collect()
    }

    fn pointer_click(harness: &mut PromptHarness, point: egui::Pos2) {
        let pressed = frame(
            harness,
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
        let released = frame(
            harness,
            vec![egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::default(),
            }],
        );
        released.drop_without_applying_deltas();
    }

    fn click_label(harness: &mut PromptHarness, label: &str, occurrence: usize) {
        let output = frame(harness, Vec::new());
        let mut bounds = accessible_bounds(&output, label);
        bounds.sort_by(|left, right| left.center().y.total_cmp(&right.center().y));
        output.drop_without_applying_deltas();
        let target = *bounds
            .get(occurrence)
            .unwrap_or_else(|| panic!("missing enabled accessible {label:?} #{occurrence}"));
        pointer_click(harness, target.center());
    }

    fn type_into(harness: &mut PromptHarness, field_index: usize, text: &str) {
        let output = frame(harness, Vec::new());
        let mut inputs = text_input_bounds(&output);
        inputs.sort_by(|left, right| left.center().y.total_cmp(&right.center().y));
        output.drop_without_applying_deltas();
        let target = *inputs
            .get(field_index)
            .unwrap_or_else(|| panic!("missing enabled text input #{field_index}"));
        pointer_click(harness, target.center());
        let typed = frame(
            harness,
            vec![
                egui::Event::Key {
                    key: egui::Key::A,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers {
                        ctrl: true,
                        command: true,
                        ..Default::default()
                    },
                },
                egui::Event::Key {
                    key: egui::Key::A,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers: egui::Modifiers {
                        ctrl: true,
                        command: true,
                        ..Default::default()
                    },
                },
                egui::Event::Text(text.into()),
            ],
        );
        typed.drop_without_applying_deltas();
    }

    fn wait_for_completion(runtime: &crate::mkmacro::MacroRuntime) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while runtime.snapshot().state != RuntimeState::Completed {
            assert!(
                Instant::now() < deadline,
                "macro did not complete: {:?}",
                runtime.snapshot()
            );
            std::thread::yield_now();
        }
    }

    #[test]
    fn parameter_prompt_ui_browse_and_choice_wait_for_explicit_run() {
        let mut harness = harness(parameters(), |kind| {
            Ok(Some(match kind {
                ParameterPathKind::File => PathBuf::from("/picked/input.txt"),
                ParameterPathKind::Folder => PathBuf::from("/picked/source"),
            }))
        });
        let initial = frame(&mut harness, Vec::new());
        let initial_fields = text_input_bounds(&initial).len();
        initial.drop_without_applying_deltas();
        assert_eq!(initial_fields, 2, "file and folder paths remain editable");
        assert_eq!(
            harness.prompt.values.get(&MkSignatureId(3)),
            Some(&MkValue::String(String::new())),
            "Choice must not select its first option automatically"
        );
        assert!(!harness.prompt.valid);

        click_label(&mut harness, "Browse for file…", 0);
        click_label(&mut harness, "Browse for folder…", 0);
        click_label(&mut harness, "Choose an option…", 0);
        let options = frame(&mut harness, Vec::new());
        let second = accessible_bounds(&options, "Second");
        let first = accessible_bounds(&options, "First");
        let authored_order = second
            .first()
            .zip(first.first())
            .is_some_and(|(a, b)| a.center().y < b.center().y);
        options.drop_without_applying_deltas();
        assert!(authored_order, "Choice options stay in authored order");
        click_label(&mut harness, "First", 0);

        assert_eq!(
            harness.prompt.values.get(&MkSignatureId(1)),
            Some(&MkValue::String("/picked/input.txt".into()))
        );
        assert_eq!(
            harness.prompt.values.get(&MkSignatureId(2)),
            Some(&MkValue::String("/picked/source".into()))
        );
        assert_eq!(
            harness.prompt.values.get(&MkSignatureId(3)),
            Some(&MkValue::String("First".into()))
        );
        assert!(harness.prompt.valid);
        assert!(harness.runtime.take_test_commands().is_empty());
        assert!(harness.backend.events().is_empty());
        let run = frame(&mut harness, Vec::new());
        let run_enabled = !accessible_bounds(&run, "Run").is_empty();
        run.drop_without_applying_deltas();
        assert!(run_enabled);

        click_label(&mut harness, "Run", 0);
        wait_for_completion(&harness.runtime);
        let commands = harness.runtime.take_test_commands();
        assert_eq!(commands.len(), 1);
        let RuntimeCommand::Invoke(invocation) = &commands[0] else {
            panic!("Run admits the current invocation once")
        };
        assert_eq!(
            invocation.arguments,
            BTreeMap::from([
                (
                    MkSignatureId(1),
                    MkValue::String("/picked/input.txt".into())
                ),
                (MkSignatureId(2), MkValue::String("/picked/source".into())),
                (MkSignatureId(3), MkValue::String("First".into())),
            ])
        );
        assert_eq!(
            harness.backend.events(),
            ["text:file=/picked/input.txt; folder=/picked/source; choice=First"]
        );
        harness.prompt.shutdown();
        harness.runtime.shutdown();
    }

    #[test]
    fn parameter_prompt_ui_cancelled_or_failed_browse_keeps_editable_request() {
        let outcomes = Arc::new(Mutex::new(VecDeque::from([
            Ok(Some(PathBuf::from("/kept/path"))),
            Ok(None),
            Err("fake picker failure".to_owned()),
        ])));
        let chooser_outcomes = outcomes.clone();
        let mut harness = harness(
            vec![
                parameter(1, "file", MkParameterInputKind::File, None),
                parameter(2, "legacy", MkParameterInputKind::Text, None),
            ],
            move |_| chooser_outcomes.lock().unwrap().pop_front().unwrap(),
        );
        frame(&mut harness, Vec::new()).drop_without_applying_deltas();
        click_label(&mut harness, "Browse for file…", 0);
        assert_eq!(
            harness.prompt.values.get(&MkSignatureId(1)),
            Some(&MkValue::String("/kept/path".into()))
        );

        click_label(&mut harness, "Browse for file…", 0);
        assert_eq!(
            harness.prompt.values.get(&MkSignatureId(1)),
            Some(&MkValue::String("/kept/path".into())),
            "native picker cancellation leaves the value and prompt intact"
        );
        assert!(
            harness
                .prompt
                .pending
                .as_ref()
                .is_some_and(|pending| pending.is_active())
        );
        assert!(
            harness
                .prompt
                .browse_feedback
                .as_deref()
                .is_some_and(|message| message.contains("No path was selected"))
        );

        click_label(&mut harness, "Browse for file…", 0);
        assert_eq!(
            harness.prompt.values.get(&MkSignatureId(1)),
            Some(&MkValue::String("/kept/path".into()))
        );
        assert!(
            harness
                .prompt
                .error
                .as_deref()
                .is_some_and(|message| message.contains("fake picker failure"))
        );
        type_into(&mut harness, 0, "/manual/fallback");
        type_into(&mut harness, 1, "legacy text");
        assert_eq!(
            harness.prompt.values.get(&MkSignatureId(1)),
            Some(&MkValue::String("/manual/fallback".into())),
            "path remains directly editable after a detectable picker failure"
        );
        assert_eq!(
            harness.prompt.values.get(&MkSignatureId(2)),
            Some(&MkValue::String("legacy text".into())),
            "legacy Text parameters retain their existing editable control"
        );
        assert!(harness.runtime.take_test_commands().is_empty());
        assert!(harness.backend.events().is_empty());

        click_label(&mut harness, "Cancel", 0);
        assert!(!harness.prompt.owns_input());
        assert!(harness.broker.ensure_idle().is_ok());
        assert!(harness.runtime.take_test_commands().is_empty());
        assert!(harness.backend.events().is_empty());
        harness.prompt.shutdown();
        harness.runtime.shutdown();
    }

    #[test]
    fn parameter_prompt_ui_late_path_result_cannot_mutate_replaced_request() {
        let broker = Arc::new(InvocationPromptBroker::default());
        let successor = Arc::new(Mutex::new(None));
        let chooser_broker = broker.clone();
        let successor_slot = successor.clone();
        let mut harness = harness_with_broker(
            vec![parameter(1, "file", MkParameterInputKind::File, None)],
            broker.clone(),
            move |_| {
                *successor_slot.lock().unwrap() =
                    Some(chooser_broker.register_gui(Arc::new(|| {})));
                Ok(Some(PathBuf::from("/stale/result.txt")))
            },
        );
        frame(&mut harness, Vec::new()).drop_without_applying_deltas();
        click_label(&mut harness, "Browse for file…", 0);

        assert!(harness.prompt.pending.is_none());
        assert!(harness.prompt.values.is_empty());
        assert!(broker.ensure_idle().is_ok());
        let successor_pending = successor
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|registration| registration.take_pending().is_none());
        assert!(
            successor_pending,
            "the stale picker cannot bind a successor token"
        );
        assert!(harness.runtime.take_test_commands().is_empty());
        assert!(harness.backend.events().is_empty());

        harness.prompt.shutdown();
        harness.runtime.shutdown();
    }
}
