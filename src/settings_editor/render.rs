use super::state::SettingsEditor;
use crate::gui::LauncherApp;
use crate::plugins::note::{NoteExternalOpen, NotePluginSettings};
use crate::settings::NoteViewMode;
use eframe::egui;

impl SettingsEditor {
    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        let mut open = app.show_settings;
        egui::Window::new("Settings")
            .resizable(true)
            .default_size(Self::settings_window_default_size(ctx))
            .min_height(Self::SETTINGS_WINDOW_MIN_HEIGHT)
            .open(&mut open)
            .show(ctx, |ui| {
                let settings_content_height = Self::settings_content_height(ui.available_height());
                egui::ScrollArea::vertical()
                    .max_height(settings_content_height)
                    .show(ui, |ui| {
                        self.render_hotkey_section(ui);
                        self.render_general_section(ui, app);
                        self.render_layout_section(ui, app);
                        self.render_dashboard_section(ui, app);
                        self.render_plugin_sections(ui, app);
                        self.expand_request = None;
                    });

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        self.save_settings(ctx, app);
                    }
                });
            });
        app.show_settings = open;
    }

    fn render_hotkey_section(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Launcher hotkey");
            let resp = ui.text_edit_singleline(&mut self.hotkey);
            if resp.changed() {
                self.hotkey_valid = crate::hotkey::parse_hotkey(&self.hotkey).is_some();
                if self.hotkey_valid {
                    self.last_valid_hotkey = self.hotkey.clone();
                }
            }
            hotkey_indicator(ui, self.hotkey_valid);
        });
        self.render_optional_hotkey(ui, "Enable quit hotkey", "Quit hotkey", HotkeyKind::Quit);
        self.render_optional_hotkey(ui, "Enable help hotkey", "Help hotkey", HotkeyKind::Help);
    }

    fn render_optional_hotkey(
        &mut self,
        ui: &mut egui::Ui,
        toggle_label: &str,
        field_label: &str,
        kind: HotkeyKind,
    ) {
        let (enabled, value, valid, last_valid) = match kind {
            HotkeyKind::Quit => (
                &mut self.quit_hotkey_enabled,
                &mut self.quit_hotkey,
                &mut self.quit_hotkey_valid,
                &mut self.last_valid_quit_hotkey,
            ),
            HotkeyKind::Help => (
                &mut self.help_hotkey_enabled,
                &mut self.help_hotkey,
                &mut self.help_hotkey_valid,
                &mut self.last_valid_help_hotkey,
            ),
        };
        ui.checkbox(enabled, toggle_label);
        if *enabled {
            ui.horizontal(|ui| {
                ui.label(field_label);
                let resp = ui.text_edit_singleline(value);
                if resp.changed() {
                    *valid = crate::hotkey::parse_hotkey(value).is_some();
                    if *valid {
                        *last_valid = value.clone();
                    }
                }
                hotkey_indicator(ui, *valid);
            });
        }
    }

    fn render_general_section(&mut self, ui: &mut egui::Ui, app: &mut LauncherApp) {
        ui.horizontal(|ui| {
            egui::ComboBox::from_label("Debug logging")
                .selected_text(if self.debug_logging {
                    "Enabled"
                } else {
                    "Disabled"
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.debug_logging, false, "Disabled");
                    ui.selectable_value(&mut self.debug_logging, true, "Enabled");
                });
        });
        ui.checkbox(&mut self.show_toasts, "Enable toast notifications");
        ui.checkbox(&mut self.show_inline_errors, "Show inline errors");
        ui.checkbox(&mut self.show_error_toasts, "Show error toasts");
        if self.show_toasts {
            ui.horizontal(|ui| {
                ui.label("Toast duration (s)");
                ui.add(egui::Slider::new(&mut self.toast_duration, 0.1..=5.0).text(""));
            });
        }
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut self.hide_after_run, "Hide window after running action");
            ui.checkbox(&mut self.preserve_command, "Preserve command after run");
            ui.checkbox(&mut self.clear_query_after_run, "Clear query after run");
            ui.checkbox(
                &mut self.require_confirm_destructive,
                "Require confirm for destructive actions",
            );
        });
        ui.checkbox(&mut self.always_on_top, "Always on top");
        ui.checkbox(&mut self.query_autocomplete, "Enable query autocomplete");
        ui.checkbox(
            &mut self.disable_timer_updates,
            "Disable timer auto refresh",
        );
        ui.horizontal(|ui| {
            ui.label("Timer refresh rate (s)");
            ui.add_enabled_ui(!self.disable_timer_updates, |ui| {
                ui.add(
                    egui::DragValue::new(&mut self.timer_refresh)
                        .clamp_range(0.1..=60.0)
                        .speed(0.1),
                );
            });
        });
        ui.horizontal(|ui| {
            ui.label("Query scale");
            ui.add(egui::Slider::new(&mut self.query_scale, 0.5..=5.0).text(""));
        });
        ui.horizontal(|ui| {
            ui.label("List scale");
            ui.add(egui::Slider::new(&mut self.list_scale, 0.5..=5.0).text(""));
        });
        if ui.button("Open Theme Settings...").clicked() {
            app.open_theme_settings_dialog();
        }
        ui.horizontal(|ui| {
            ui.label("Fuzzy weight");
            ui.add(egui::Slider::new(&mut self.fuzzy_weight, 0.0..=5.0).text(""));
        });
        ui.horizontal(|ui| {
            ui.label("Usage weight");
            ui.add(egui::Slider::new(&mut self.usage_weight, 0.0..=5.0).text(""));
        });
        ui.checkbox(&mut self.match_exact, "Match exact");
        ui.horizontal(|ui| {
            ui.label("Page jump");
            ui.add(
                egui::DragValue::new(&mut self.page_jump)
                    .clamp_range(1..=100)
                    .speed(1),
            );
        });
    }

    fn render_layout_section(&mut self, ui: &mut egui::Ui, app: &LauncherApp) {
        ui.checkbox(
            &mut self.query_results_layout_enabled,
            "Display results in grid layout",
        );
        ui.add_enabled_ui(self.query_results_layout_enabled, |ui| {
            ui.horizontal(|ui| {
                ui.label("Grid rows");
                ui.add(
                    egui::DragValue::new(&mut self.query_results_layout_rows)
                        .clamp_range(1..=100)
                        .speed(1),
                );
                ui.label("Columns");
                ui.add(
                    egui::DragValue::new(&mut self.query_results_layout_cols)
                        .clamp_range(1..=100)
                        .speed(1),
                );
            });
            self.query_results_layout_rows = self.query_results_layout_rows.max(1);
            self.query_results_layout_cols = self.query_results_layout_cols.max(1);
            ui.checkbox(
                &mut self.query_results_layout_respect_plugin_capability,
                "Respect plugin list/grid capability",
            );
            ui.horizontal(|ui| {
                ui.label("Force list for plugins (comma separated)");
                ui.text_edit_singleline(&mut self.query_results_layout_plugin_opt_out);
            });
        });
        ui.horizontal(|ui| {
            ui.label("Off-screen X");
            ui.add(egui::DragValue::new(&mut self.offscreen_x));
            ui.label("Y");
            ui.add(egui::DragValue::new(&mut self.offscreen_y));
        });
        let follow_mouse_resp = ui.checkbox(&mut self.follow_mouse, "Follow mouse");
        if follow_mouse_resp.changed() && self.follow_mouse {
            self.static_enabled = false;
        }
        ui.add_enabled_ui(!self.follow_mouse, |ui| {
            ui.checkbox(&mut self.static_enabled, "Use static position");
        });
        if !self.follow_mouse && self.static_enabled {
            ui.horizontal(|ui| {
                ui.label("X");
                ui.add(egui::DragValue::new(&mut self.static_x));
                ui.label("Y");
                ui.add(egui::DragValue::new(&mut self.static_y));
                ui.label("W");
                ui.add(egui::DragValue::new(&mut self.static_w));
                ui.label("H");
                ui.add(egui::DragValue::new(&mut self.static_h));
                if ui.button("Snapshot").clicked() {
                    self.static_x = app.window_pos.0;
                    self.static_y = app.window_pos.1;
                    self.static_w = app.window_size.0;
                    self.static_h = app.window_size.1;
                }
            });
        }
    }

    fn render_dashboard_section(&mut self, ui: &mut egui::Ui, app: &mut LauncherApp) {
        ui.separator();
        ui.heading("Dashboard");
        ui.checkbox(
            &mut self.dashboard_enabled,
            "Enable dashboard when query is empty",
        );
        ui.horizontal(|ui| {
            ui.label("Dashboard config path");
            ui.text_edit_singleline(&mut self.dashboard_path);
        });
        ui.horizontal(|ui| {
            ui.label("Default location");
            ui.text_edit_singleline(&mut self.dashboard_default_location);
        });
        ui.checkbox(
            &mut self.dashboard_show_when_empty,
            "Show dashboard when the search box is blank",
        );
        ui.checkbox(
            &mut self.reduce_dashboard_work_when_unfocused,
            "Reduce dashboard work when not focused",
        );
        if self.debug_logging || cfg!(debug_assertions) {
            ui.checkbox(
                &mut self.show_dashboard_diagnostics,
                "Show dashboard diagnostics (dev)",
            );
        }
        if ui.button("Customize Dashboard...").clicked() {
            app.show_dashboard_editor = true;
        }
    }

    fn render_plugin_sections(&mut self, ui: &mut egui::Ui, app: &mut LauncherApp) {
        ui.separator();
        if ui
            .button(if self.plugins_expanded {
                "Collapse plugin sections"
            } else {
                "Expand plugin sections"
            })
            .clicked()
        {
            self.plugins_expanded = !self.plugins_expanded;
            self.expand_request = Some(self.plugins_expanded);
        }
        let enabled_list = app.enabled_plugins_list();
        let mut open_mg_settings_dialog = false;
        let mut clipboard_modify_command = None;
        let clipboard_modify_status = app
            .clipboard_modify_config_diagnostic
            .clone()
            .unwrap_or_else(|| "valid".to_owned());

        for plugin in app.plugins.iter_mut() {
            let name = plugin.name().to_string();
            if name == "notes" {
                continue;
            }
            let enabled = match &enabled_list {
                Some(list) => list.contains(&name),
                None => true,
            };
            if !enabled {
                continue;
            }
            let has_settings =
                plugin.default_settings().is_some() || self.plugin_settings.contains_key(&name);
            if !has_settings {
                continue;
            }

            let entry = self
                .plugin_settings
                .entry(name.clone())
                .or_insert_with(|| plugin.default_settings().unwrap_or(serde_json::Value::Null));
            Self::render_plugin_section(
                ui,
                self.expand_request,
                &name,
                entry,
                plugin.as_mut(),
                &mut open_mg_settings_dialog,
                &mut clipboard_modify_command,
                &clipboard_modify_status,
                &mut self.confirm_clipboard_modify_factory_reset,
            );
        }

        if open_mg_settings_dialog {
            app.open_mouse_gesture_settings_dialog();
        }
        if let Some(command) = clipboard_modify_command {
            match command {
                ClipboardModifySettingsCommand::OpenDialog => app.open_clipboard_modify_dialog(),
                ClipboardModifySettingsCommand::OpenConfigFile => {
                    app.open_clipboard_modify_config_file()
                }
                ClipboardModifySettingsCommand::ReloadConfig => {
                    app.reload_clipboard_modify_config()
                }
                ClipboardModifySettingsCommand::ResetFactoryDefaults => {
                    app.reset_clipboard_modify_config_to_factory_defaults()
                }
            }
        }
        self.render_note_section(ui);
    }

    fn render_plugin_section(
        ui: &mut egui::Ui,
        expand_request: Option<bool>,
        name: &str,
        entry: &mut serde_json::Value,
        plugin: &mut dyn crate::plugin::Plugin,
        open_mg_settings_dialog: &mut bool,
        clipboard_modify_command: &mut Option<ClipboardModifySettingsCommand>,
        clipboard_modify_status: &str,
        confirm_factory_reset: &mut bool,
    ) {
        let id = ui.make_persistent_id(format!("plugin_{name}"));
        let mut state =
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);
        if let Some(open) = expand_request {
            state.set_open(open);
        }
        state
            .show_header(ui, |ui| {
                ui.label(format!("{name} settings"));
            })
            .body(|ui| {
                if name == "mouse_gestures" {
                    ui.label("Mouse gesture settings are managed in a dedicated dialog.");
                    ui.add_space(6.0);
                    if ui.button("Open Mouse Gesture Settings...").clicked() {
                        *open_mg_settings_dialog = true;
                    }
                    ui.add_space(4.0);
                    ui.small("Tip: you can also open this window via `mg settings`.");
                } else if name == "clipboard_modify" {
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Open Clipboard Modify").clicked() {
                            *clipboard_modify_command =
                                Some(ClipboardModifySettingsCommand::OpenDialog);
                        }
                        if ui.button("Open clipboard_modifiers.json").clicked() {
                            *clipboard_modify_command =
                                Some(ClipboardModifySettingsCommand::OpenConfigFile);
                        }
                        if ui.button("Reload configuration").clicked() {
                            *clipboard_modify_command =
                                Some(ClipboardModifySettingsCommand::ReloadConfig);
                        }
                        if ui.button("Reset to factory defaults").clicked() {
                            *confirm_factory_reset = true;
                        }
                    });
                    if *confirm_factory_reset {
                        ui.group(|ui| {
                            ui.label("Replace the configuration with factory defaults? A timestamped backup will be created first.");
                            ui.horizontal(|ui| {
                                if ui.button("Confirm reset").clicked() {
                                    *clipboard_modify_command = Some(
                                        ClipboardModifySettingsCommand::ResetFactoryDefaults,
                                    );
                                    *confirm_factory_reset = false;
                                }
                                if ui.button("Cancel").clicked() {
                                    *confirm_factory_reset = false;
                                }
                            });
                        });
                    }
                    ui.label(format!("Configuration status: {clipboard_modify_status}"));
                    ui.separator();
                    plugin.settings_ui(ui, entry);
                } else {
                    plugin.settings_ui(ui, entry);
                }
            });
    }

    fn render_note_section(&mut self, ui: &mut egui::Ui) {
        let id = ui.make_persistent_id("plugin_notes");
        let mut state =
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);
        if let Some(open) = self.expand_request {
            state.set_open(open);
        }
        state
            .show_header(ui, |ui| {
                ui.label("Note settings");
            })
            .body(|ui| {
                ui.heading("Notes / Markdown");
                ui.checkbox(&mut self.note_rich_markdown_enabled, "Enable rich Markdown");
                ui.checkbox(&mut self.note_task_lists_enabled, "Enable task lists");
                ui.checkbox(
                    &mut self.note_interactive_checkboxes_enabled,
                    "Enable interactive checkboxes",
                );
                ui.checkbox(
                    &mut self.note_collapsible_sections_enabled,
                    "Enable collapsible sections",
                );
                ui.checkbox(
                    &mut self.note_outline_sidebar_enabled,
                    "Enable outline sidebar",
                );
                ui.add_enabled_ui(self.note_outline_sidebar_enabled, |ui| {
                    ui.checkbox(
                        &mut self.note_outline_sidebar_default_open,
                        "Open outline sidebar by default",
                    );
                });
                ui.checkbox(&mut self.note_split_view_enabled, "Enable split view");
                egui::ComboBox::from_label("Default view mode")
                    .selected_text(match self.note_default_view_mode {
                        NoteViewMode::Edit => "Edit",
                        NoteViewMode::Preview => "Preview",
                        NoteViewMode::Split => "Split",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.note_default_view_mode,
                            NoteViewMode::Edit,
                            "Edit",
                        );
                        ui.selectable_value(
                            &mut self.note_default_view_mode,
                            NoteViewMode::Preview,
                            "Preview",
                        );
                        ui.add_enabled_ui(self.note_split_view_enabled, |ui| {
                            ui.selectable_value(
                                &mut self.note_default_view_mode,
                                NoteViewMode::Split,
                                "Split",
                            );
                        });
                    });
                ui.checkbox(&mut self.note_callouts_enabled, "Enable callouts");
                ui.checkbox(&mut self.note_backlinks_enabled, "Enable backlinks");
                ui.checkbox(&mut self.note_aliases_enabled, "Enable aliases");
                ui.checkbox(&mut self.note_templates_enabled, "Enable templates");
                ui.checkbox(
                    &mut self.note_collapsed_sections_persist,
                    "Remember collapsed sections",
                );
                ui.horizontal(|ui| {
                    ui.label("Max outline depth");
                    ui.add(
                        egui::DragValue::new(&mut self.note_max_outline_depth)
                            .clamp_range(1..=6)
                            .speed(1),
                    );
                });
                self.note_max_outline_depth = self.note_max_outline_depth.clamp(1, 6);

                ui.separator();
                ui.heading("Legacy note options");
                ui.checkbox(&mut self.note_save_on_close, "Save note on close (Esc)");
                ui.checkbox(
                    &mut self.note_always_overwrite,
                    "Always overwrite existing notes",
                );
                ui.checkbox(&mut self.note_images_as_links, "Display images as links");
                ui.checkbox(&mut self.note_show_details, "Show note details by default");
                ui.horizontal(|ui| {
                    ui.label("Tag/link preview limit");
                    ui.add(
                        egui::DragValue::new(&mut self.note_more_limit).clamp_range(1..=usize::MAX),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("Note panel W");
                    ui.add(
                        egui::DragValue::new(&mut self.note_panel_w).clamp_range(200.0..=2000.0),
                    );
                    ui.label("H");
                    ui.add(
                        egui::DragValue::new(&mut self.note_panel_h).clamp_range(150.0..=1600.0),
                    );
                });
                let mut cfg = self
                    .plugin_settings
                    .get("note")
                    .and_then(|v| serde_json::from_value::<NotePluginSettings>(v.clone()).ok())
                    .unwrap_or_default();
                egui::ComboBox::from_label("Open externally")
                    .selected_text(match cfg.external_open {
                        NoteExternalOpen::Neither => "Neither",
                        NoteExternalOpen::Powershell => "Powershell",
                        NoteExternalOpen::Notepad => "Notepad",
                        NoteExternalOpen::Wezterm => "WezTerm",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut cfg.external_open,
                            NoteExternalOpen::Neither,
                            "Neither",
                        );
                        ui.selectable_value(
                            &mut cfg.external_open,
                            NoteExternalOpen::Powershell,
                            "Powershell",
                        );
                        ui.selectable_value(
                            &mut cfg.external_open,
                            NoteExternalOpen::Notepad,
                            "Notepad",
                        );
                        ui.selectable_value(
                            &mut cfg.external_open,
                            NoteExternalOpen::Wezterm,
                            "WezTerm",
                        );
                    });
                self.plugin_settings.insert(
                    "note".into(),
                    serde_json::to_value(cfg).unwrap_or(serde_json::Value::Null),
                );
            });
    }
}

#[derive(Clone, Copy)]
enum HotkeyKind {
    Quit,
    Help,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ClipboardModifySettingsCommand {
    OpenDialog,
    OpenConfigFile,
    ReloadConfig,
    ResetFactoryDefaults,
}

fn hotkey_indicator(ui: &mut egui::Ui, valid: bool) {
    let color = if valid {
        egui::Color32::GREEN
    } else {
        egui::Color32::RED
    };
    ui.add(egui::Label::new(egui::RichText::new("●").color(color)));
}
