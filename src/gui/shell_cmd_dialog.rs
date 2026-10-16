use crate::gui::LauncherApp;
use crate::gui::theme::widgets::{
    ButtonEmphasis, StatusTone, compact_section_heading, semantic_status, with_button_emphasis,
};
use crate::plugins::shell::{SHELL_CMDS_FILE, ShellCmdEntry, load_shell_cmds, replace_shell_cmds};
use eframe::egui;

#[derive(Default)]
pub struct ShellCmdDialog {
    pub open: bool,
    entries: Vec<ShellCmdEntry>,
    edit_idx: Option<usize>,
    name: String,
    args: String,
    keep_open: bool,
    load_error: Option<String>,
    #[cfg(test)]
    test_layout: Option<ShellCmdDialogLayout>,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug)]
struct ShellScrollLayout {
    parent_max_rect: egui::Rect,
    viewport_clip_rect: egui::Rect,
    content_size: egui::Vec2,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug)]
enum ShellBodyLayout {
    Editor {
        scroll: ShellScrollLayout,
        name_rect: egui::Rect,
        editor_clip_rect: egui::Rect,
        args_id: egui::Id,
        args_has_focus: bool,
        keep_open_rect: egui::Rect,
    },
    Browse {
        scroll: ShellScrollLayout,
        first_name_rect: egui::Rect,
        first_command_rect: egui::Rect,
        first_edit_rect: egui::Rect,
        first_remove_rect: egui::Rect,
    },
}

#[cfg(test)]
#[derive(Clone, Copy, Debug)]
struct ShellCmdDialogLayout {
    window_rect: egui::Rect,
    body: ShellBodyLayout,
    primary_rect: egui::Rect,
    secondary_rect: egui::Rect,
    footer_clip_rect: egui::Rect,
    close_clicked: bool,
    visible: bool,
    sizing_pass: bool,
}

impl ShellCmdDialog {
    /// Open the dialog and load persisted commands for an explicit open request.
    pub fn open(&mut self) {
        self.open_from(SHELL_CMDS_FILE);
    }

    fn open_from(&mut self, path: &str) {
        let _ = self.load_from(path);
        self.open = true;
        self.edit_idx = None;
        self.name.clear();
        self.args.clear();
        self.keep_open = false;
    }

    /// Load persisted commands only on a closed-to-open transition.
    /// Pinned panels call this from the per-frame open reconciliation path.
    pub fn ensure_open(&mut self) {
        self.ensure_open_from(SHELL_CMDS_FILE);
    }

    fn ensure_open_from(&mut self, path: &str) {
        if !self.open {
            self.open_from(path);
        }
    }

    fn load_from(&mut self, path: &str) -> anyhow::Result<()> {
        match load_shell_cmds(path) {
            Ok(entries) => {
                self.entries = entries;
                self.load_error = None;
                Ok(())
            }
            Err(error) => {
                self.load_error = Some(error.to_string());
                Err(error)
            }
        }
    }

    fn commit_entries(&mut self, path: &str, candidate: Vec<ShellCmdEntry>) -> anyhow::Result<()> {
        match replace_shell_cmds(path, candidate) {
            Ok(committed) => {
                self.entries = committed;
                self.load_error = None;
                Ok(())
            }
            Err(error) => {
                self.load_error = Some(error.to_string());
                Err(error)
            }
        }
    }

    fn save(&mut self, app: &mut LauncherApp, candidate: Vec<ShellCmdEntry>) -> bool {
        if let Err(e) = self.commit_entries(SHELL_CMDS_FILE, candidate) {
            app.report_error_message("ui operation", format!("Failed to save commands: {e}"));
            false
        } else {
            app.search();
            app.focus_input();
            true
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open {
            return;
        }

        let pinned = app
            .pinned_panels
            .contains(&crate::gui::Panel::ShellCmdDialog);
        let mut window_open = self.open;
        let mut close = false;
        let mut save_candidate = None;
        #[cfg(test)]
        let mut test_body = None;
        #[cfg(test)]
        let mut test_primary_rect = egui::Rect::NOTHING;
        #[cfg(test)]
        let mut test_secondary_rect = egui::Rect::NOTHING;
        #[cfg(test)]
        let mut test_footer_clip_rect = egui::Rect::NOTHING;
        #[cfg(test)]
        let mut test_close_clicked = false;
        #[cfg(test)]
        let mut test_visible = false;
        #[cfg(test)]
        let mut test_sizing_pass = false;
        #[cfg(test)]
        let window_id = egui::Id::new("Shell Commands");

        let window_output = egui::Window::new("Shell Commands")
            .resizable(true)
            .default_size((560.0, 400.0))
            .max_width(ctx.content_rect().width().max(1.0))
            .max_height(ctx.content_rect().height().max(1.0))
            .open(&mut window_open)
            .show(ctx, |ui| {
                #[cfg(test)]
                {
                    test_visible = ui.is_visible();
                    test_sizing_pass = ui.is_sizing_pass();
                }

                if let Some(error) = &self.load_error {
                    let footer_height = shell_footer_reserved_height(ui, &["Close"]);
                    let error_height = (ui.available_height() - footer_height).max(0.0);
                    egui::ScrollArea::vertical()
                        .id_salt("shell_cmd_load_error")
                        .auto_shrink([false, false])
                        .min_scrolled_height(0.0)
                        .max_height(error_height)
                        .show(ui, |ui| {
                            semantic_status(
                                ui,
                                StatusTone::Error,
                                "Shell commands are read-only because loading failed",
                                Some(error),
                            );
                        });
                    ui.separator();
                    ui.horizontal_wrapped(|ui| {
                        #[cfg(test)]
                        {
                            test_footer_clip_rect = ui.clip_rect();
                        }
                        let button = ui.button("Close");
                        let button = with_button_emphasis(ui, button, ButtonEmphasis::Primary);
                        #[cfg(test)]
                        {
                            test_primary_rect = button.rect;
                            test_close_clicked |= button.clicked();
                        }
                        if button.clicked() {
                            close = true;
                        }
                    });
                    return;
                }

                if let Some(idx) = self.edit_idx {
                    compact_section_heading(
                        ui,
                        if idx == self.entries.len() {
                            "New command"
                        } else {
                            "Edit command"
                        },
                        Some("Enter inserts a new line in the command text."),
                    );
                    let name_response = ui
                        .horizontal_wrapped(|ui| {
                            let label = ui.label("Name");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.name)
                                    .desired_width(ui.available_width().min(360.0).max(72.0)),
                            )
                            .labelled_by(label.id)
                        })
                        .inner;
                    let command_label = ui.label("Command");
                    let footer_height =
                        shell_editor_footer_reserved_height(ui, &["Save", "Cancel"]);
                    let editor_max_height = (ui.available_height() - footer_height).max(0.0);
                    #[cfg(test)]
                    let parent_max_rect = ui.max_rect();
                    #[cfg(test)]
                    let mut parent_clip_rect = egui::Rect::NOTHING;
                    #[cfg(test)]
                    let mut editor_clip_rect = egui::Rect::NOTHING;
                    #[cfg(test)]
                    let mut editor_args_id = None;
                    #[cfg(test)]
                    let mut editor_has_focus = false;
                    let _editor_output = egui::ScrollArea::vertical()
                        .id_salt("shell_cmd_args_scroll")
                        .auto_shrink([false, false])
                        .min_scrolled_height(0.0)
                        .max_height(editor_max_height)
                        .show(ui, |ui| {
                            #[cfg(test)]
                            {
                                parent_clip_rect = ui.clip_rect();
                            }
                            let output = egui::TextEdit::multiline(&mut self.args)
                                .id_salt("shell_cmd_args")
                                .desired_width(f32::INFINITY)
                                .desired_rows(12)
                                .show(ui);
                            #[cfg(test)]
                            {
                                editor_args_id = Some(output.response.id);
                                editor_clip_rect = output.text_clip_rect.intersect(ui.clip_rect());
                            }
                            let response = egui::Response::clone(&output.response)
                                .labelled_by(command_label.id);
                            #[cfg(test)]
                            {
                                editor_has_focus = response.has_focus();
                            }
                            if response.has_focus()
                                && ui.input(|input| input.key_pressed(egui::Key::Enter))
                            {
                                let modifiers = ui.input(|input| input.modifiers);
                                ui.input_mut(|input| {
                                    input.consume_key(modifiers, egui::Key::Enter)
                                });
                            }
                            response
                        });

                    let keep_open_response =
                        ui.checkbox(&mut self.keep_open, "Keep command prompt open after run");
                    ui.separator();
                    ui.horizontal_wrapped(|ui| {
                        #[cfg(test)]
                        {
                            test_footer_clip_rect = ui.clip_rect();
                        }
                        let save_button = ui.button("Save");
                        let save_button =
                            with_button_emphasis(ui, save_button, ButtonEmphasis::Primary);
                        #[cfg(test)]
                        {
                            test_primary_rect = save_button.rect;
                        }
                        if save_button.clicked() {
                            if self.name.trim().is_empty() || self.args.trim().is_empty() {
                                app.report_error_message("ui operation", "Both fields required");
                            } else {
                                let mut candidate = self.entries.clone();
                                if idx == candidate.len() {
                                    candidate.push(ShellCmdEntry {
                                        name: self.name.clone(),
                                        args: self.args.clone(),
                                        autocomplete: true,
                                        keep_open: self.keep_open,
                                    });
                                } else if let Some(entry) = candidate.get_mut(idx) {
                                    entry.name = self.name.clone();
                                    entry.args = self.args.clone();
                                    entry.keep_open = self.keep_open;
                                }
                                save_candidate = Some(candidate);
                            }
                        }
                        let cancel_button = ui.button("Cancel");
                        #[cfg(test)]
                        {
                            test_secondary_rect = cancel_button.rect;
                        }
                        if cancel_button.clicked() {
                            self.edit_idx = None;
                        }
                    });

                    #[cfg(test)]
                    {
                        test_body = Some(ShellBodyLayout::Editor {
                            scroll: ShellScrollLayout {
                                parent_max_rect,
                                viewport_clip_rect: parent_clip_rect,
                                content_size: _editor_output.content_size,
                            },
                            name_rect: name_response.rect,
                            editor_clip_rect,
                            args_id: editor_args_id.expect("the command editor should have an id"),
                            args_has_focus: editor_has_focus,
                            keep_open_rect: keep_open_response.rect,
                        });
                    }
                    #[cfg(not(test))]
                    let _ = (name_response, keep_open_response);
                } else {
                    compact_section_heading(ui, "Saved commands", None);
                    let footer_height = shell_footer_reserved_height(ui, &["Add Command", "Close"]);
                    let list_height = (ui.available_height() - footer_height).max(0.0);
                    let mut remove = None;
                    #[cfg(test)]
                    let parent_max_rect = ui.max_rect();
                    #[cfg(test)]
                    let mut parent_clip_rect = egui::Rect::NOTHING;
                    #[cfg(test)]
                    let mut first_name_rect = egui::Rect::NOTHING;
                    #[cfg(test)]
                    let mut first_command_rect = egui::Rect::NOTHING;
                    #[cfg(test)]
                    let mut first_edit_rect = egui::Rect::NOTHING;
                    #[cfg(test)]
                    let mut first_remove_rect = egui::Rect::NOTHING;
                    let list_output = egui::ScrollArea::vertical()
                        .id_salt("shell_cmd_list")
                        .auto_shrink([false, false])
                        .min_scrolled_height(0.0)
                        .max_height(list_height)
                        .show(ui, |ui| {
                            #[cfg(test)]
                            {
                                parent_clip_rect = ui.clip_rect();
                            }
                            for idx in 0..self.entries.len() {
                                let name = self.entries[idx].name.clone();
                                let args = self.entries[idx].args.clone();
                                let keep_open = self.entries[idx].keep_open;
                                let row = ui.push_id(("shell_cmd_row", idx), |ui| {
                                    ui.vertical(|ui| {
                                        let name_response =
                                            ui.label(egui::RichText::new(name).strong());
                                        let command_response =
                                            ui.add(egui::Label::new(args).wrap());
                                        let mut edit_clicked = false;
                                        let mut remove_clicked = false;
                                        ui.horizontal_wrapped(|ui| {
                                            let edit_button = ui.button("Edit");
                                            edit_clicked = edit_button.clicked();
                                            #[cfg(test)]
                                            if idx == 0 {
                                                first_edit_rect = edit_button.rect;
                                            }
                                            let remove_button = ui.button("Remove");
                                            let remove_button = with_button_emphasis(
                                                ui,
                                                remove_button,
                                                ButtonEmphasis::Destructive,
                                            );
                                            remove_clicked = remove_button.clicked();
                                            #[cfg(test)]
                                            if idx == 0 {
                                                first_remove_rect = remove_button.rect;
                                            }
                                        });
                                        ui.separator();
                                        (
                                            name_response,
                                            command_response,
                                            edit_clicked,
                                            remove_clicked,
                                        )
                                    })
                                    .inner
                                });
                                if idx == 0 {
                                    #[cfg(test)]
                                    {
                                        first_name_rect = row.inner.0.rect;
                                        first_command_rect = row.inner.1.rect;
                                    }
                                }
                                if row.inner.2 {
                                    self.edit_idx = Some(idx);
                                    self.name.clone_from(&self.entries[idx].name);
                                    self.args.clone_from(&self.entries[idx].args);
                                    self.keep_open = keep_open;
                                }
                                if row.inner.3 {
                                    remove = Some(idx);
                                }
                            }
                        });
                    if let Some(idx) = remove {
                        let mut candidate = self.entries.clone();
                        candidate.remove(idx);
                        save_candidate = Some(candidate);
                    }
                    ui.separator();
                    ui.horizontal_wrapped(|ui| {
                        #[cfg(test)]
                        {
                            test_footer_clip_rect = ui.clip_rect();
                        }
                        let add_button = ui.button("Add Command");
                        let add_button =
                            with_button_emphasis(ui, add_button, ButtonEmphasis::Primary);
                        #[cfg(test)]
                        {
                            test_primary_rect = add_button.rect;
                        }
                        if add_button.clicked() {
                            self.edit_idx = Some(self.entries.len());
                            self.name.clear();
                            self.args.clear();
                            self.keep_open = false;
                        }
                        let close_button = ui.button("Close");
                        #[cfg(test)]
                        {
                            test_secondary_rect = close_button.rect;
                            test_close_clicked |= close_button.clicked();
                        }
                        if close_button.clicked() {
                            close = true;
                        }
                    });
                    #[cfg(test)]
                    {
                        test_body = Some(ShellBodyLayout::Browse {
                            scroll: ShellScrollLayout {
                                parent_max_rect,
                                viewport_clip_rect: parent_clip_rect,
                                content_size: list_output.content_size,
                            },
                            first_name_rect,
                            first_command_rect,
                            first_edit_rect,
                            first_remove_rect,
                        });
                    }
                    #[cfg(not(test))]
                    let _ = list_output;
                }
            });

        self.open = window_open || pinned;
        #[cfg(test)]
        {
            self.test_layout = match (window_output.as_ref(), test_body) {
                (Some(window), Some(body)) => Some(ShellCmdDialogLayout {
                    window_rect: ctx
                        .memory(|memory| memory.area_rect(window_id))
                        .unwrap_or(window.response.rect),
                    body,
                    primary_rect: test_primary_rect,
                    secondary_rect: test_secondary_rect,
                    footer_clip_rect: test_footer_clip_rect,
                    close_clicked: test_close_clicked,
                    visible: test_visible,
                    sizing_pass: test_sizing_pass,
                }),
                _ => None,
            };
        }
        #[cfg(not(test))]
        let _ = window_output;

        if let Some(candidate) = save_candidate
            && self.save(app, candidate)
        {
            self.edit_idx = None;
            self.name.clear();
            self.args.clear();
            self.keep_open = false;
        }
        if close && !pinned {
            self.open = false;
        }
    }
}

fn shell_editor_footer_reserved_height(ui: &mut egui::Ui, labels: &[&str]) -> f32 {
    let spacing = ui.spacing().clone();
    let available_width = ui.available_width();
    let keep_open_label = "Keep command prompt open after run";
    let checkbox_style = ui.style().checkbox_style(
        &egui::widget_style::Classes::default(),
        egui::widget_style::WidgetState::Inactive,
    );
    let checkbox_min_height = spacing.interact_size.y.max(checkbox_style.checkbox_size);
    let checkbox_icon_height = checkbox_style.checkbox_size.max(checkbox_min_height);
    let checkbox_frame_margin = checkbox_style.frame.total_margin().sum();
    let label_width = (available_width
        - checkbox_frame_margin.x
        - checkbox_style.checkbox_size
        - spacing.icon_spacing)
        .max(1.0);
    let label_galley = egui::WidgetText::from(keep_open_label).into_galley(
        ui,
        Some(ui.wrap_mode()),
        label_width,
        checkbox_style.text_style.font_id,
    );
    let checkbox_height = (label_galley.size().y.max(checkbox_icon_height)
        + checkbox_frame_margin.y)
        .max(checkbox_min_height);
    let height =
        checkbox_height + spacing.item_spacing.y + shell_footer_reserved_height(ui, labels);
    let pixels_per_point = ui.pixels_per_point().max(f32::EPSILON);
    (height * pixels_per_point).ceil() / pixels_per_point
}

fn shell_footer_reserved_height(ui: &mut egui::Ui, labels: &[&str]) -> f32 {
    let spacing = ui.spacing().clone();
    let style = ui.style().clone();
    let native_button_style = style.button_style(
        &egui::widget_style::Classes::default(),
        egui::widget_style::WidgetState::Inactive,
    );
    let font_id = style
        .override_font_id
        .clone()
        .or_else(|| {
            style
                .override_text_style
                .clone()
                .map(|text_style| text_style.resolve(&style))
        })
        .unwrap_or_else(|| native_button_style.text_style.font_id.clone());
    let button_frame = native_button_style.frame;
    let button_height = (ui.fonts_mut(|fonts| fonts.row_height(&font_id))
        + spacing.extra_text_line_spacing
        + button_frame.total_margin().sum().y)
        .max(spacing.interact_size.y);
    let available_width = ui.available_width().max(1.0);
    let text_color = ui.visuals().text_color();
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
        let gap = if row_width == 0.0 {
            0.0
        } else {
            spacing.item_spacing.x
        };
        if row_width > 0.0 && row_width + gap + button_width > available_width {
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
    let pixels_per_point = ui.pixels_per_point().max(f32::EPSILON);
    (height * pixels_per_point).ceil() / pixels_per_point
}

#[cfg(test)]
mod tests {
    use super::{ShellBodyLayout, ShellCmdDialog, ShellCmdDialogLayout};
    use crate::gui::Panel;
    use crate::plugin::PluginManager;
    use crate::plugins::shell::{ShellCmdEntry, save_shell_cmds};
    use crate::settings::Settings;
    use eframe::egui;
    use std::sync::{Arc, atomic::AtomicBool};
    use tempfile::tempdir;

    fn new_app(ctx: &egui::Context, root: &std::path::Path) -> crate::gui::LauncherApp {
        crate::gui::LauncherApp::new(
            ctx,
            Arc::new(Vec::new()),
            0,
            PluginManager::new_inert_for_test(),
            root.join("actions.json").to_string_lossy().into_owned(),
            root.join("settings.json").to_string_lossy().into_owned(),
            Settings::default(),
            None,
            None,
            None,
            None,
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        )
    }

    fn test_input(screen_rect: egui::Rect, time: f64, events: Vec<egui::Event>) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(screen_rect),
            time: Some(time),
            focused: true,
            events,
            ..Default::default()
        }
    }

    fn render_frame(
        ctx: &egui::Context,
        dialog: &mut ShellCmdDialog,
        app: &mut crate::gui::LauncherApp,
        input: egui::RawInput,
    ) -> Option<ShellCmdDialogLayout> {
        let output = ctx.run_ui(input, |ctx| dialog.ui(ctx, app));
        let layout = dialog.test_layout;
        output.drop_without_applying_deltas();
        layout
    }

    fn click_widget(
        ctx: &egui::Context,
        dialog: &mut ShellCmdDialog,
        app: &mut crate::gui::LauncherApp,
        screen_rect: egui::Rect,
        rect: egui::Rect,
        time: f64,
    ) -> Option<ShellCmdDialogLayout> {
        let point = rect.center();
        let _ = render_frame(
            ctx,
            dialog,
            app,
            test_input(
                screen_rect,
                time,
                vec![
                    egui::Event::PointerMoved(point),
                    egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::default(),
                    },
                ],
            ),
        );
        render_frame(
            ctx,
            dialog,
            app,
            test_input(
                screen_rect,
                time + 0.1,
                vec![egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::default(),
                }],
            ),
        )
    }

    fn editor_args_details(dialog: &ShellCmdDialog) -> (egui::Id, bool) {
        match dialog
            .test_layout
            .expect("the command editor should render")
            .body
        {
            ShellBodyLayout::Editor {
                args_id,
                args_has_focus,
                ..
            } => (args_id, args_has_focus),
            body => panic!("expected the command editor layout, got {body:?}"),
        }
    }

    #[test]
    fn enter_inserts_newline() {
        let dir = tempdir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx, dir.path());
        let mut dlg = ShellCmdDialog::default();
        dlg.open();
        dlg.edit_idx = Some(0);
        dlg.args = "echo hi".into();

        ctx.begin_pass(Default::default());
        dlg.ui(&ctx, &mut app);
        ctx.end_pass().drop_without_applying_deltas();
        let (editor_id, _) = editor_args_details(&dlg);
        ctx.memory_mut(|memory| memory.request_focus(editor_id));

        ctx.begin_pass(egui::RawInput {
            events: vec![
                egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::default(),
                },
                egui::Event::ModifiersChanged(egui::Modifiers::default()),
            ],
            ..Default::default()
        });
        dlg.ui(&ctx, &mut app);
        ctx.end_pass().drop_without_applying_deltas();

        assert!(
            editor_args_details(&dlg).1,
            "the command editor should have focus"
        );
        assert_eq!(dlg.args, "echo hi\n");
    }

    #[test]
    fn shift_enter_inserts_newline() {
        let dir = tempdir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx, dir.path());
        let mut dlg = ShellCmdDialog::default();
        dlg.open();
        dlg.edit_idx = Some(0);
        dlg.args = "echo hi".into();

        ctx.begin_pass(Default::default());
        dlg.ui(&ctx, &mut app);
        ctx.end_pass().drop_without_applying_deltas();
        let (editor_id, _) = editor_args_details(&dlg);
        ctx.memory_mut(|memory| memory.request_focus(editor_id));

        let modifiers = egui::Modifiers {
            shift: true,
            ..Default::default()
        };
        ctx.begin_pass(egui::RawInput {
            events: vec![
                egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                },
                egui::Event::ModifiersChanged(modifiers),
            ],
            ..Default::default()
        });
        dlg.ui(&ctx, &mut app);
        ctx.end_pass().drop_without_applying_deltas();

        assert!(
            editor_args_details(&dlg).1,
            "the command editor should have focus"
        );
        assert_eq!(dlg.args, "echo hi\n");
    }

    #[test]
    fn open_resets_keep_open() {
        let mut dlg = ShellCmdDialog {
            keep_open: true,
            ..Default::default()
        };
        dlg.open();
        assert!(!dlg.keep_open);
    }

    #[test]
    fn opening_error_and_pinned_close_preserve_draft_until_reopen() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("commands.json");
        let path_text = path.to_str().unwrap();
        let initial = vec![ShellCmdEntry {
            name: "saved".into(),
            args: "echo saved".into(),
            autocomplete: true,
            keep_open: false,
        }];
        let changed_on_disk = vec![ShellCmdEntry {
            name: "updated".into(),
            args: "echo updated".into(),
            autocomplete: true,
            keep_open: true,
        }];
        save_shell_cmds(path_text, &initial).unwrap();
        let mut dialog = ShellCmdDialog {
            name: "stale name".into(),
            args: "stale command".into(),
            keep_open: true,
            ..Default::default()
        };
        dialog.ensure_open_from(path_text);
        assert!(dialog.open);
        assert_eq!(dialog.entries, initial);
        assert_eq!(dialog.name, "");
        assert_eq!(dialog.args, "");
        assert!(!dialog.keep_open);
        dialog.edit_idx = None;
        dialog.name = "unsaved pinned draft".into();
        dialog.args = "echo draft".into();
        dialog.keep_open = true;
        save_shell_cmds(path_text, &changed_on_disk).unwrap();

        let ctx = egui::Context::default();
        ctx.style_mut_of(ctx.theme(), |style| style.animation_time = 0.0);
        let mut app = new_app(&ctx, dir.path());
        app.pinned_panels.push(Panel::ShellCmdDialog);
        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(480.0, 340.0));
        let mut layout = None;
        for frame in 0..3 {
            layout = render_frame(
                &ctx,
                &mut dialog,
                &mut app,
                test_input(screen_rect, frame as f64, Vec::new()),
            );
        }
        let pinned_layout = layout.expect("the pinned command list should render");
        let close_rect = pinned_layout.secondary_rect;
        let pinned_close = click_widget(&ctx, &mut dialog, &mut app, screen_rect, close_rect, 4.0)
            .expect("the pinned close button should render");
        assert!(
            pinned_close.close_clicked,
            "the pinned Close click should register"
        );
        assert!(
            dialog.open,
            "a pinned Close click keeps Shell Commands open"
        );
        assert_eq!(dialog.entries, initial);
        assert_eq!(dialog.name, "unsaved pinned draft");
        assert_eq!(dialog.args, "echo draft");
        assert!(dialog.keep_open);

        dialog.ensure_open_from(path_text);
        assert_eq!(dialog.entries, initial);
        assert_eq!(dialog.name, "unsaved pinned draft");
        assert_eq!(dialog.args, "echo draft");

        app.pinned_panels
            .retain(|panel| *panel != Panel::ShellCmdDialog);
        let unpinned_layout = render_frame(
            &ctx,
            &mut dialog,
            &mut app,
            test_input(screen_rect, 5.0, Vec::new()),
        )
        .expect("the unpinned command list should render");
        let unpinned_close = click_widget(
            &ctx,
            &mut dialog,
            &mut app,
            screen_rect,
            unpinned_layout.secondary_rect,
            6.0,
        )
        .expect("the unpinned close button should render");
        assert!(
            unpinned_close.close_clicked,
            "the unpinned Close click should register"
        );
        assert!(!dialog.open);
        assert_eq!(dialog.name, "unsaved pinned draft");

        let invalid_bytes = b"invalid shell commands";
        std::fs::write(&path, invalid_bytes).unwrap();
        dialog.ensure_open_from(path_text);
        assert!(dialog.open);
        assert_eq!(dialog.entries, initial);
        assert!(dialog.load_error.is_some());
        assert_eq!(dialog.name, "");
        assert_eq!(dialog.args, "");
        assert!(!dialog.keep_open);
        assert_eq!(std::fs::read(&path).unwrap(), invalid_bytes);

        dialog.open = false;
        // Simulate external repair; the guarded store correctly rejects corrupt input.
        std::fs::write(&path, serde_json::to_vec(&changed_on_disk).unwrap()).unwrap();
        dialog.ensure_open_from(path_text);
        assert_eq!(dialog.entries, changed_on_disk);
        assert!(dialog.load_error.is_none());
    }

    #[test]
    fn long_commands_scroll_inside_bounded_editor_and_browse_list() {
        let dir = tempdir().unwrap();
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx, dir.path());
        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(480.0, 340.0));
        let args = (0..80)
            .map(|line| format!("echo {line} — café 🌱\n"))
            .collect::<String>();
        let mut dialog = ShellCmdDialog {
            open: true,
            edit_idx: Some(0),
            name: "draft command".into(),
            args: args.clone(),
            keep_open: true,
            ..Default::default()
        };
        let mut editor_layouts = Vec::new();
        for frame in 0..4 {
            if let Some(layout) = render_frame(
                &ctx,
                &mut dialog,
                &mut app,
                test_input(screen_rect, frame as f64, Vec::new()),
            ) {
                if layout.visible && !layout.sizing_pass {
                    editor_layouts.push(layout);
                }
            }
        }
        let editor = *editor_layouts
            .last()
            .expect("the Shell command editor should render");
        let ShellBodyLayout::Editor {
            scroll: editor_scroll,
            name_rect,
            editor_clip_rect,
            keep_open_rect,
            ..
        } = editor.body
        else {
            panic!("expected the command editor layout, got {:?}", editor.body);
        };
        assert!(screen_rect.contains_rect(editor.window_rect));
        assert!(editor_scroll.parent_max_rect.is_positive());
        assert!(editor_scroll.viewport_clip_rect.is_positive());
        assert!(
            editor_scroll
                .viewport_clip_rect
                .intersects(editor_clip_rect)
        );
        assert!(screen_rect.contains_rect(editor_clip_rect));
        assert!(editor_scroll.content_size.y > editor_scroll.viewport_clip_rect.height());
        assert!(screen_rect.contains_rect(name_rect));
        for action in [editor.primary_rect, editor.secondary_rect] {
            assert!(editor.window_rect.contains_rect(action));
            assert!(screen_rect.contains_rect(action));
            assert!(editor.footer_clip_rect.contains_rect(action));
        }
        assert!(editor.window_rect.contains_rect(keep_open_rect));
        assert!(screen_rect.contains_rect(keep_open_rect));
        assert!(editor_scroll.viewport_clip_rect.bottom() <= keep_open_rect.top());
        assert!(keep_open_rect.bottom() <= editor.primary_rect.top());
        assert_eq!(dialog.args, args);
        assert_eq!(dialog.name, "draft command");
        assert!(dialog.keep_open && dialog.open);

        let entries = (0..24)
            .map(|index| ShellCmdEntry {
                name: format!("Long command name {index}: {}", "deploy tool ".repeat(5)),
                args: format!("--option-{index} {}", "argument phrase ".repeat(10)),
                autocomplete: true,
                keep_open: index % 2 == 0,
            })
            .collect::<Vec<_>>();
        dialog.edit_idx = None;
        dialog.entries = entries.clone();
        let mut browse_layouts = Vec::new();
        for frame in 4..8 {
            if let Some(layout) = render_frame(
                &ctx,
                &mut dialog,
                &mut app,
                test_input(screen_rect, frame as f64, Vec::new()),
            ) {
                if layout.visible && !layout.sizing_pass {
                    browse_layouts.push(layout);
                }
            }
        }
        let browse = *browse_layouts
            .last()
            .expect("the saved command list should render");
        let ShellBodyLayout::Browse {
            scroll: list_scroll,
            first_name_rect,
            first_command_rect,
            first_edit_rect,
            first_remove_rect,
        } = browse.body
        else {
            panic!("expected the browse layout, got {:?}", browse.body);
        };
        assert!(screen_rect.contains_rect(browse.window_rect));
        assert!(list_scroll.parent_max_rect.is_positive());
        assert!(list_scroll.viewport_clip_rect.is_positive());
        assert!(list_scroll.content_size.y > list_scroll.viewport_clip_rect.height());
        assert!(
            list_scroll
                .viewport_clip_rect
                .contains_rect(first_name_rect)
        );
        assert!(
            list_scroll
                .viewport_clip_rect
                .contains_rect(first_command_rect)
        );
        let body_font_id = egui::TextStyle::Body.resolve(&ctx.style_of(ctx.theme()));
        let body_line_height = ctx.fonts_mut(|fonts| fonts.row_height(&body_font_id));
        assert!(first_name_rect.height() > body_line_height * 1.5);
        assert!(first_command_rect.height() > body_line_height * 1.5);
        for action in [first_edit_rect, first_remove_rect] {
            assert!(list_scroll.viewport_clip_rect.contains_rect(action));
        }
        for action in [browse.primary_rect, browse.secondary_rect] {
            assert!(browse.window_rect.contains_rect(action));
            assert!(screen_rect.contains_rect(action));
            assert!(browse.footer_clip_rect.contains_rect(action));
        }
        assert!(list_scroll.viewport_clip_rect.bottom() <= browse.primary_rect.top());
        assert_eq!(dialog.entries, entries);
        assert_eq!(dialog.name, "draft command");
        assert_eq!(dialog.args, args);
        assert!(dialog.keep_open && dialog.open);
    }

    #[test]
    fn invalid_reload_and_commit_keep_last_good_and_read_only() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("commands.json");
        let initial = vec![ShellCmdEntry {
            name: "saved".into(),
            args: "echo saved".into(),
            autocomplete: true,
            keep_open: false,
        }];
        save_shell_cmds(path.to_str().unwrap(), &initial).unwrap();
        let mut dialog = ShellCmdDialog::default();
        dialog.load_from(path.to_str().unwrap()).unwrap();
        let invalid = b"invalid commands";
        std::fs::write(&path, invalid).unwrap();
        assert!(dialog.load_from(path.to_str().unwrap()).is_err());
        assert_eq!(dialog.entries, initial);
        assert!(dialog.load_error.is_some());
        assert!(
            dialog
                .commit_entries(path.to_str().unwrap(), Vec::new())
                .is_err()
        );
        assert_eq!(dialog.entries, initial);
        assert_eq!(std::fs::read(path).unwrap(), invalid);
    }
}
