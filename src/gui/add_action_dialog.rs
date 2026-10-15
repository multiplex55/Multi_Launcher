use crate::actions::Action;
#[cfg(test)]
use crate::actions_editor::ScrollLayoutObservation;
use crate::gui::LauncherApp;
use crate::gui::theme::widgets::{
    ButtonEmphasis, StatusTone, compact_section_heading, labeled_form_row, semantic_status,
    with_button_emphasis,
};
use eframe::egui;
use rfd::FileDialog;

/// Dialog state used when adding a new user defined command.
///
/// The struct holds the text the user has entered as well as an `open`
/// flag indicating if the dialog should currently be visible.
pub struct AddActionDialog {
    /// `true` while the dialog window is displayed.
    pub open: bool,
    /// Command label being edited.
    label: String,
    /// Command description being edited.
    desc: String,
    /// Path to the executable or file to launch.
    path: String,
    /// Whether the arguments field is visible.
    show_args: bool,
    /// Additional arguments to pass when launching.
    args: String,
    /// Current dialog mode (add or edit existing command).
    mode: DialogMode,
    /// Inline feedback from validation or failed publication.
    feedback: Option<String>,
    #[cfg(test)]
    pub(crate) test_form_layout: Option<AddActionLayoutObservation>,
}

/// Mode of the action dialog.
#[derive(Default)]
enum DialogMode {
    /// Add a new command when confirmed.
    #[default]
    Add,
    /// Edit the command at the given index when confirmed.
    Edit(usize),
}

impl Default for AddActionDialog {
    fn default() -> Self {
        Self {
            open: false,
            label: String::new(),
            desc: String::new(),
            path: String::new(),
            show_args: false,
            args: String::new(),
            mode: DialogMode::Add,
            feedback: None,
            #[cfg(test)]
            test_form_layout: None,
        }
    }
}

impl AddActionDialog {
    /// Start the dialog for creating a new command.
    pub fn open_add(&mut self) {
        self.open_add_with_path("");
    }

    /// Start the dialog for creating a new command using `path` as the
    /// pre-filled executable.
    pub fn open_add_with_path(&mut self, path: &str) {
        self.open = true;
        self.label.clear();
        self.desc.clear();
        self.path = path.into();
        self.args.clear();
        self.show_args = false;
        self.feedback = None;
        self.mode = DialogMode::Add;
    }

    /// Start editing an existing command at `idx` with the given values.
    pub fn open_edit(&mut self, idx: usize, act: &Action) {
        self.open = true;
        self.label = act.label.clone();
        self.desc = act.desc.clone();
        self.path = act.action.clone();
        self.args = act.args.clone().unwrap_or_default();
        self.show_args = act.args.is_some();
        self.feedback = None;
        self.mode = DialogMode::Edit(idx);
    }

    #[cfg(test)]
    pub(crate) fn test_draft_fields(&self) -> (&str, &str, &str, bool, &str) {
        (
            &self.label,
            &self.desc,
            &self.path,
            self.show_args,
            &self.args,
        )
    }

    /// Draw the "Add App" dialog and update `app` with any new action.
    ///
    /// * `ctx` - Egui context used to render the window.
    /// * `app` - Application state that will receive the new action if the
    ///   user confirms the dialog. The actions list is persisted on success.
    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open {
            return;
        }
        let mut should_close = false;
        let title = match self.mode {
            DialogMode::Add => "Add App",
            DialogMode::Edit(_) => "Edit App",
        };
        let button_label = match self.mode {
            DialogMode::Add => "Add",
            DialogMode::Edit(_) => "Save",
        };
        #[cfg(test)]
        let mut test_layout = None;
        #[cfg(test)]
        let mut test_visible = false;
        #[cfg(test)]
        let mut test_sizing_pass = false;
        let window_output = egui::Window::new(title)
            .open(&mut self.open)
            .resizable(true)
            .max_width(ctx.content_rect().width().max(1.0))
            .max_height(ctx.content_rect().height().max(1.0))
            .show(ctx, |ui| {
                #[cfg(test)]
                {
                    test_visible = ui.is_visible();
                    test_sizing_pass = ui.is_sizing_pass();
                }
                compact_section_heading(ui, "App details", None);
                if let Some(feedback) = &self.feedback {
                    semantic_status(ui, StatusTone::Error, "Unable to save app", Some(feedback));
                }

                let footer_height = action_footer_reserved_height(ui, &[button_label, "Cancel"]);
                let form_max_rect = ui.max_rect();
                let form_clip_rect = ui.clip_rect();
                let body_height = (ui.available_height() - footer_height).max(0.0);
                let form_output = egui::ScrollArea::vertical()
                    .id_salt("add_action_form")
                    .auto_shrink([false, false])
                    .min_scrolled_height(0.0)
                    .max_height(body_height)
                    .show(ui, |ui| {
                        let label = labeled_form_row(ui, "Label", |ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.label)
                                    .desired_width(ui.available_width().max(72.0)),
                            )
                        });
                        let description = labeled_form_row(ui, "Description", |ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.desc)
                                    .desired_width(ui.available_width().max(72.0)),
                            )
                        });
                        let mut path_browsed = false;
                        let path = labeled_form_row(ui, "Path", |ui| {
                            let path = ui.add(
                                egui::TextEdit::singleline(&mut self.path)
                                    .desired_width(ui.available_width().max(72.0)),
                            );
                            if ui.button("Browse").clicked()
                                && let Some(file) = FileDialog::new().pick_file()
                            {
                                if let Some(path) = file.to_str() {
                                    self.path = path.to_owned();
                                } else {
                                    self.path = file.display().to_string();
                                }
                                path_browsed = true;
                            }
                            path
                        });
                        let mut changed =
                            label.changed() | description.changed() | path.changed() | path_browsed;

                        ui.horizontal_wrapped(|ui| {
                            changed |= ui.checkbox(&mut self.show_args, "Add arguments").changed();
                            if self.show_args {
                                changed |= ui
                                    .add(
                                        egui::TextEdit::singleline(&mut self.args)
                                            .desired_width(ui.available_width().max(72.0)),
                                    )
                                    .changed();
                            }
                        });
                        if changed {
                            self.feedback = None;
                        }
                    });

                ui.separator();
                let mut primary_rect = egui::Rect::NOTHING;
                let mut cancel_rect = egui::Rect::NOTHING;
                ui.horizontal_wrapped(|ui| {
                    let primary = ui.button(button_label);
                    let primary = with_button_emphasis(ui, primary, ButtonEmphasis::Primary);
                    primary_rect = primary.rect;
                    if primary.clicked() {
                        use std::path::Path;
                        if self.path.is_empty() || !Path::new(&self.path).exists() {
                            let message = "Path does not exist".to_owned();
                            self.feedback = Some(message.clone());
                            app.report_error_message("ui operation", message);
                        } else {
                            let candidate = Action {
                                label: self.label.clone(),
                                desc: self.desc.clone(),
                                action: self.path.clone(),
                                args: if self.show_args && !self.args.trim().is_empty() {
                                    Some(self.args.clone())
                                } else {
                                    None
                                },
                            };
                            let result = match self.mode {
                                DialogMode::Add => app.update_custom_actions(move |actions| {
                                    actions.push(candidate);
                                    Ok(())
                                }),
                                DialogMode::Edit(idx) => {
                                    app.update_custom_actions(move |actions| {
                                        let action = actions.get_mut(idx).ok_or_else(|| {
                                            anyhow::anyhow!("custom action no longer exists")
                                        })?;
                                        *action = candidate;
                                        Ok(())
                                    })
                                }
                            };
                            match result {
                                Ok(()) => {
                                    self.label.clear();
                                    self.desc.clear();
                                    self.path.clear();
                                    self.args.clear();
                                    self.show_args = false;
                                    self.feedback = None;
                                    should_close = true;
                                }
                                Err(error) => {
                                    let message = format!("Failed to save: {error}");
                                    self.feedback = Some(message.clone());
                                    app.report_error_message("ui operation", message);
                                }
                            }
                        }
                    }

                    let cancel = ui.button("Cancel");
                    cancel_rect = cancel.rect;
                    if cancel.clicked() {
                        should_close = true;
                    }
                });

                #[cfg(test)]
                {
                    test_layout = Some(AddActionLayoutObservation {
                        scroll: ScrollLayoutObservation {
                            id: form_output.id,
                            visible: test_visible,
                            sizing_pass: test_sizing_pass,
                            parent_max_rect: form_max_rect,
                            parent_clip_rect: form_clip_rect,
                            inner_rect: form_output.inner_rect,
                            content_size: form_output.content_size,
                            offset: form_output.state.offset,
                        },
                        visible: test_visible,
                        sizing_pass: test_sizing_pass,
                        primary_rect,
                        cancel_rect,
                        window_rect: egui::Rect::NOTHING,
                    });
                }
                #[cfg(not(test))]
                let _ = (
                    form_max_rect,
                    form_clip_rect,
                    form_output,
                    primary_rect,
                    cancel_rect,
                );
            });

        #[cfg(test)]
        {
            if let Some(mut layout) = test_layout {
                layout.window_rect = window_output
                    .as_ref()
                    .map_or(egui::Rect::NOTHING, |window| window.response.rect);
                self.test_form_layout = Some(layout);
            }
        }
        #[cfg(not(test))]
        let _ = window_output;

        if should_close {
            self.open = false;
        }
    }
}

#[cfg(test)]
#[derive(Clone, Copy)]
pub(crate) struct AddActionLayoutObservation {
    pub(crate) scroll: ScrollLayoutObservation,
    pub(crate) visible: bool,
    pub(crate) sizing_pass: bool,
    pub(crate) primary_rect: egui::Rect,
    pub(crate) cancel_rect: egui::Rect,
    pub(crate) window_rect: egui::Rect,
}

fn action_footer_reserved_height(ui: &mut egui::Ui, labels: &[&str]) -> f32 {
    let spacing = ui.spacing().clone();
    let style = ui.style().clone();
    let font_id = style
        .override_font_id
        .clone()
        .or_else(|| {
            style
                .override_text_style
                .as_ref()
                .map(|text_style| text_style.resolve(&style))
        })
        .unwrap_or_else(|| egui::TextStyle::Body.resolve(&style));
    let button_frame = style
        .button_style(
            &egui::widget_style::Classes::default(),
            egui::widget_style::WidgetState::Inactive,
        )
        .frame;
    let button_height = (ui.fonts_mut(|fonts| fonts.row_height(&font_id))
        + spacing.extra_text_line_spacing
        + button_frame.total_margin().sum().y)
        .max(spacing.interact_size.y);
    let text_color = ui.visuals().text_color();
    let available_width = ui.available_width().max(1.0);
    let pixels_per_point = ui.ctx().pixels_per_point().max(f32::EPSILON);
    let mut row_width = 0.0;
    let mut row_count = 1_usize;
    for label in labels {
        let text_width = ui.fonts_mut(|fonts| {
            fonts
                .layout_no_wrap((*label).to_owned(), font_id.clone(), text_color)
                .size()
                .x
        });
        let button_width =
            (text_width + button_frame.total_margin().sum().x).max(spacing.interact_size.x);
        if row_width > 0.0 && row_width + spacing.item_spacing.x + button_width > available_width {
            row_count += 1;
            row_width = button_width;
        } else {
            if row_width > 0.0 {
                row_width += spacing.item_spacing.x;
            }
            row_width += button_width;
        }
    }
    let separator_height = style
        .separator_style(
            &egui::widget_style::Classes::default(),
            egui::widget_style::WidgetState::Noninteractive,
        )
        .spacing;
    let height = row_count as f32 * button_height
        + row_count.saturating_sub(1) as f32 * spacing.item_spacing.y
        + 2.0 * spacing.item_spacing.y
        + separator_height;
    (height * pixels_per_point).ceil() / pixels_per_point
}
