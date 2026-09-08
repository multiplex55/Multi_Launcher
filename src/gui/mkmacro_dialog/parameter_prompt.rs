use crate::mkmacro::{
    MkInvocationValues,
    invocation_prompt::{
        InvocationGuiRegistration, PendingInvocation, production_invocation_prompt_broker,
    },
    prepare_parameters,
};
use eframe::egui;
use std::sync::Arc;

/// An app lifetime owns availability and the held request. Dropping either the
/// app or a replaced registration cancels only that registration's token.
pub(crate) struct ParameterPromptUi {
    registration: Option<InvocationGuiRegistration>,
    pending: Option<PendingInvocation>,
    values: MkInvocationValues,
    error: Option<String>,
    notice: Option<String>,
    first_frame: bool,
    valid: bool,
}
impl ParameterPromptUi {
    pub fn new(ctx: &egui::Context) -> Self {
        let ctx = ctx.clone();
        Self {
            registration: Some(
                production_invocation_prompt_broker()
                    .register_gui(Arc::new(move || ctx.request_repaint())),
            ),
            pending: None,
            values: Default::default(),
            error: None,
            notice: None,
            first_frame: false,
            valid: false,
        }
    }
    pub fn shutdown(&mut self) {
        if let Some(registration) = self.registration.take() {
            registration.unregister();
        }
        self.pending = None;
        self.values.clear();
        self.error = None;
        self.notice = None;
    }
    pub fn show(&mut self, ctx: &egui::Context) {
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
        }
        if let Some(notice) = registration.take_notice() {
            self.notice = Some(notice);
            self.first_frame = true;
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
            self.valid = prepare_parameters(&pending.request().parameters, &[], &self.values)
                .is_ok_and(|prepared| prepared.missing.is_empty());
            self.pending = Some(pending);
            self.error = None;
            self.first_frame = true;
        }
        if self.pending.is_none() && self.notice.is_none() {
            return;
        }
        let mut cancel = false;
        let mut submit = false;
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
                egui::TopBottomPanel::bottom("invocation_actions").show(child, |ui| {
                    ui.horizontal(|ui| {
                        if self.pending.is_some() {
                            submit = ui
                                .add_enabled(self.valid, egui::Button::new("Run"))
                                .clicked();
                            cancel = ui.button("Cancel").clicked();
                        } else {
                            cancel = ui.button("Close").clicked();
                        }
                    });
                });
                egui::CentralPanel::default().show(child, |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .max_height(ui.available_height())
                        .show(ui, |ui| {
                            if let Some(notice) = &self.notice {
                                ui.colored_label(egui::Color32::RED, notice);
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
                                    ui.push_id(parameter.id, |ui| {
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
                                                    super::runtime_inspector::format_value(value)
                                                        .hover_text
                                                ),
                                                None => "Required • no default".into(),
                                            });
                                            if let Some(value) = self.values.get_mut(&parameter.id)
                                            {
                                                let edit = super::typed_value::fixed_type_ui(
                                                    ui,
                                                    parameter.value_type,
                                                    value,
                                                );
                                                changed |= edit.changed;
                                                if !edit.compatible {
                                                    self.valid = false;
                                                }
                                            }
                                        });
                                    });
                                }
                                if changed {
                                    self.valid =
                                        prepare_parameters(&request.parameters, &[], &self.values)
                                            .is_ok_and(|prepared| prepared.missing.is_empty());
                                }
                                if let Some(error) = &self.error {
                                    ui.colored_label(egui::Color32::RED, error);
                                }
                            }
                        });
                });
                if child.input(|input| {
                    input.key_pressed(egui::Key::Escape) || input.viewport().close_requested()
                }) {
                    cancel = true;
                }
            },
        );
        if cancel {
            self.pending = None;
            self.values.clear();
            self.error = None;
            self.notice = None;
        } else if submit && let Some(pending) = &mut self.pending {
            match pending.submit(self.values.clone()) {
                Ok(()) => {
                    self.pending = None;
                    self.values.clear();
                    self.error = None;
                    self.notice = None;
                }
                Err(error) => self.error = Some(error.to_string()),
            }
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
