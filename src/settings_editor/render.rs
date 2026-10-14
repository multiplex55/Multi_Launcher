use super::state::SettingsEditor;
use crate::gui::LauncherApp;
use crate::plugins::note::{NoteExternalOpen, NotePluginSettings};
use crate::settings::NoteViewMode;
use eframe::egui;

const RADIAL_TAP_HOLD_EXPLANATION: &str = "Release before the threshold to toggle the launcher. Crossing the threshold opens the radial menu; releasing afterward never also toggles the launcher.";
const RADIAL_LEGACY_TIMING_EXPLANATION: &str = "Legacy timing: the launcher hotkey toggles immediately; named radial commands and configured direct triggers remain available.";

#[derive(Clone, Copy)]
enum TopLevelSettingsSection {
    Hotkeys,
    LauncherWindow,
    SearchResults,
    ActionsFeedback,
    Dashboard,
    Radial,
    Plugins,
}

impl TopLevelSettingsSection {
    const ALL: [Self; 7] = [
        Self::Hotkeys,
        Self::LauncherWindow,
        Self::SearchResults,
        Self::ActionsFeedback,
        Self::Dashboard,
        Self::Radial,
        Self::Plugins,
    ];

    const fn persistent_id(self) -> &'static str {
        match self {
            Self::Hotkeys => "settings_section_hotkeys",
            Self::LauncherWindow => "settings_section_launcher_window",
            Self::SearchResults => "settings_section_search_results",
            Self::ActionsFeedback => "settings_section_actions_feedback",
            Self::Dashboard => "settings_section_dashboard",
            Self::Radial => "settings_section_radial",
            Self::Plugins => "settings_section_plugins",
        }
    }

    const fn title(self) -> &'static str {
        match self {
            Self::Hotkeys => "Hotkeys",
            Self::LauncherWindow => "Launcher Window & Appearance",
            Self::SearchResults => "Search & Results",
            Self::ActionsFeedback => "Actions, Safety & Feedback",
            Self::Dashboard => "Dashboard",
            Self::Radial => "Radial Menus",
            Self::Plugins => "Plugin Settings",
        }
    }

    const fn description(self) -> &'static str {
        match self {
            Self::Hotkeys => "Set the launcher shortcut and optional quit/help shortcuts.",
            Self::LauncherWindow => "Choose window placement behavior and query/list text scales.",
            Self::SearchResults => "Tune matching and the way search results are presented.",
            Self::ActionsFeedback => "Control action completion, safety prompts, and feedback.",
            Self::Dashboard => "Configure the empty-query dashboard and its refresh behavior.",
            Self::Radial => "Set radial timing, input scope, safety, and menu defaults.",
            Self::Plugins => "Configure settings for enabled plugins.",
        }
    }

    const fn default_open(self) -> bool {
        matches!(
            self,
            Self::Hotkeys | Self::LauncherWindow | Self::SearchResults
        )
    }

    fn egui_id(self, ui: &egui::Ui) -> egui::Id {
        ui.make_persistent_id(self.persistent_id())
    }
}

fn show_settings_section(
    ui: &mut egui::Ui,
    section: TopLevelSettingsSection,
    forced_open: Option<bool>,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
        ui.ctx(),
        section.egui_id(ui),
        section.default_open(),
    );
    if let Some(open) = forced_open {
        state.set_open(open);
    }
    state
        .show_header(ui, |ui| {
            ui.heading(section.title());
        })
        .body(|ui| {
            crate::gui::theme::widgets::secondary_help(ui, section.description());
            add_contents(ui);
        });
}

fn set_all_settings_sections_open(ui: &egui::Ui, open: bool) {
    for section in TopLevelSettingsSection::ALL {
        let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
            ui.ctx(),
            section.egui_id(ui),
            section.default_open(),
        );
        state.set_open(open);
        state.store(ui.ctx());
    }
}

fn settings_footer_reserved_height(ui: &egui::Ui) -> f32 {
    let spacing = ui.spacing();
    let style = ui.style();
    let font_id = style
        .override_font_id
        .clone()
        .or_else(|| {
            style
                .override_text_style
                .as_ref()
                .map(|text_style| text_style.resolve(style))
        })
        .unwrap_or_else(|| egui::TextStyle::Body.resolve(style));
    let text_height =
        ui.fonts_mut(|fonts| fonts.row_height(&font_id)) + spacing.extra_text_line_spacing;
    let button_frame = style
        .button_style(
            &egui::widget_style::Classes::default(),
            egui::widget_style::WidgetState::Inactive,
        )
        .frame;
    let button_height =
        (text_height + button_frame.total_margin().sum().y).max(spacing.interact_size.y);
    let separator_height = style
        .separator_style(
            &egui::widget_style::Classes::default(),
            egui::widget_style::WidgetState::Noninteractive,
        )
        .spacing;
    button_height + 2.0 * spacing.item_spacing.y + separator_height
}

impl SettingsEditor {
    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        let mut open = app.show_settings;
        egui::Window::new("Settings")
            .resizable(true)
            .default_size(Self::settings_window_default_size(ctx))
            .min_height(Self::SETTINGS_WINDOW_MIN_HEIGHT)
            .open(&mut open)
            .show(ctx, |ui| {
                let settings_content_height = Self::settings_content_height(
                    ui.available_height(),
                    settings_footer_reserved_height(ui),
                );
                egui::ScrollArea::vertical()
                    .min_scrolled_height(0.0)
                    .max_height(settings_content_height)
                    .show(ui, |ui| {
                        let top_level_expand_request = ui
                            .horizontal(|ui| {
                                let mut request = None;
                                if ui.button("Expand all").clicked() {
                                    request = Some(true);
                                }
                                if ui.button("Collapse all").clicked() {
                                    request = Some(false);
                                }
                                request
                            })
                            .inner;
                        if let Some(open) = top_level_expand_request {
                            set_all_settings_sections_open(ui, open);
                        }

                        show_settings_section(
                            ui,
                            TopLevelSettingsSection::Hotkeys,
                            top_level_expand_request,
                            |ui| self.render_hotkey_section(ui),
                        );
                        show_settings_section(
                            ui,
                            TopLevelSettingsSection::LauncherWindow,
                            top_level_expand_request,
                            |ui| self.render_launcher_window_section(ui, app),
                        );
                        show_settings_section(
                            ui,
                            TopLevelSettingsSection::SearchResults,
                            top_level_expand_request,
                            |ui| self.render_search_results_section(ui),
                        );
                        show_settings_section(
                            ui,
                            TopLevelSettingsSection::ActionsFeedback,
                            top_level_expand_request,
                            |ui| self.render_actions_safety_feedback_section(ui),
                        );
                        show_settings_section(
                            ui,
                            TopLevelSettingsSection::Dashboard,
                            top_level_expand_request,
                            |ui| self.render_dashboard_section(ui, app),
                        );
                        show_settings_section(
                            ui,
                            TopLevelSettingsSection::Radial,
                            top_level_expand_request,
                            |ui| self.render_radial_section(ui, app),
                        );
                        show_settings_section(
                            ui,
                            TopLevelSettingsSection::Plugins,
                            top_level_expand_request,
                            |ui| self.render_plugin_sections(ui, app),
                        );
                        self.expand_request = None;
                    });

                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    let save = ui.button("Save");
                    if crate::gui::theme::widgets::with_button_emphasis(
                        ui,
                        save,
                        crate::gui::theme::widgets::ButtonEmphasis::Primary,
                    )
                    .clicked()
                    {
                        self.save_settings(ctx, app);
                    }
                });
            });
        app.show_settings = open;
    }

    fn render_hotkey_section(&mut self, ui: &mut egui::Ui) {
        crate::gui::theme::widgets::labeled_form_row(ui, "Launcher hotkey", |ui| {
            let resp = ui.text_edit_singleline(&mut self.hotkey);
            if resp.changed() {
                self.hotkey_valid = crate::hotkey::parse_hotkey(&self.hotkey).is_some();
                if self.hotkey_valid {
                    self.last_valid_hotkey = self.hotkey.clone();
                }
            }
            hotkey_indicator(ui, self.hotkey_valid);
            resp
        });
        self.render_optional_hotkey(ui, "Enable quit hotkey", "Quit hotkey", HotkeyKind::Quit);
        self.render_optional_hotkey(ui, "Enable help hotkey", "Help hotkey", HotkeyKind::Help);
    }

    fn render_radial_section(&mut self, ui: &mut egui::Ui, app: &mut LauncherApp) {
        use crate::commands::RadialCommandHost;
        use crate::radial::model::{
            InteractionMode, RadialSafetyPolicy, SubmenuPresentation, TooltipScope, TriggerScope,
        };

        ui.checkbox(&mut self.radial_enabled, "Enable radial menus");
        ui.add_enabled_ui(self.radial_enabled, |ui| {
            ui.checkbox(
                &mut self.radial_shared_tap_hold,
                "Share the launcher hotkey between tap and hold",
            );
            if self.radial_shared_tap_hold {
            crate::gui::theme::widgets::secondary_help(ui, RADIAL_TAP_HOLD_EXPLANATION);
            } else {
            crate::gui::theme::widgets::secondary_help(ui, RADIAL_LEGACY_TIMING_EXPLANATION);
            }
        crate::gui::theme::widgets::labeled_form_row(
            ui,
            "Hold threshold (ms)",
            |ui| {
                ui.add(
                    egui::DragValue::new(&mut self.radial_hold_threshold_ms)
                        .range(
                            crate::radial::settings::MIN_HOLD_THRESHOLD_MS
                                ..=crate::radial::settings::MAX_HOLD_THRESHOLD_MS,
                        )
                        .speed(10),
                )
            },
        );
            egui::ComboBox::from_label("Full-label tooltip scope")
                .selected_text(match self.radial_tooltip_scope {
                    TooltipScope::Off => "Off",
                    TooltipScope::TruncatedOnly => "Truncated labels only",
                    TooltipScope::AllCells => "All cells",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.radial_tooltip_scope,
                        TooltipScope::Off,
                        "Off",
                    );
                    ui.selectable_value(
                        &mut self.radial_tooltip_scope,
                        TooltipScope::TruncatedOnly,
                        "Truncated labels only",
                    );
                    ui.selectable_value(
                        &mut self.radial_tooltip_scope,
                        TooltipScope::AllCells,
                        "All cells",
                    );
                });
        crate::gui::theme::widgets::labeled_form_row(ui, "Tooltip delay (ms)", |ui| {
            ui.add(
                    egui::DragValue::new(&mut self.radial_tooltip_delay_ms)
                        .range(
                            crate::radial::settings::MIN_TOOLTIP_DELAY_MS
                                ..=crate::radial::settings::MAX_TOOLTIP_DELAY_MS,
                        )
                        .speed(25),
            )
        });
        crate::gui::theme::widgets::secondary_help(
            ui,
            "Tooltip delay is independent of the launcher hold threshold and item dwell delay.",
        );
            ui.checkbox(
                &mut self.radial_show_expected_layout_diagnostics,
                "Show expected label-layout diagnostics",
            );

            let document = crate::gui::radial_published_document();
            let effective_default = if self.radial_default_menu_id.trim().is_empty() {
                document.default_menu_id.as_str().to_owned()
            } else {
                self.radial_default_menu_id.trim().to_owned()
            };
            egui::ComboBox::from_label("Default menu")
                .selected_text(effective_default)
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.radial_default_menu_id,
                        String::new(),
                        format!("Document default ({})", document.default_menu_id),
                    );
                    for menu in &document.menus {
                        ui.selectable_value(
                            &mut self.radial_default_menu_id,
                            menu.id.as_str().to_owned(),
                            format!("{} ({})", menu.name, menu.id),
                        );
                    }
                });
            egui::ComboBox::from_label("New menu interaction")
                .selected_text(match self.radial_default_interaction {
                    InteractionMode::StickyClick => "Sticky click",
                    InteractionMode::ReleaseToSelect => "Release to select",
                    InteractionMode::HoldAndClick => "Hold and click",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.radial_default_interaction,
                        InteractionMode::StickyClick,
                        "Sticky click",
                    );
                    ui.selectable_value(
                        &mut self.radial_default_interaction,
                        InteractionMode::ReleaseToSelect,
                        "Release to select",
                    );
                    ui.selectable_value(
                        &mut self.radial_default_interaction,
                        InteractionMode::HoldAndClick,
                        "Hold and click",
                    );
                });
        crate::gui::theme::widgets::secondary_help(
            ui,
                "Sticky click keeps the tree available for repeated choices; release-to-select activates on release, and hold-and-click requires an explicit click.",
            );
            egui::ComboBox::from_label("New-menu default submenu behavior")
                .selected_text(match self.radial_default_submenu_presentation {
                    SubmenuPresentation::Cascade => "Cascade",
                    SubmenuPresentation::SameCenter => "Same center",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.radial_default_submenu_presentation,
                        SubmenuPresentation::Cascade,
                        "Cascade",
                    );
                    ui.selectable_value(
                        &mut self.radial_default_submenu_presentation,
                        SubmenuPresentation::SameCenter,
                        "Same center",
                    );
                });
            egui::ComboBox::from_label("Destructive action safety")
                .selected_text(match self.radial_safety_policy {
                    RadialSafetyPolicy::InheritLauncher => "Use launcher setting",
                    RadialSafetyPolicy::AlwaysConfirmDestructive => "Always confirm",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.radial_safety_policy,
                        RadialSafetyPolicy::InheritLauncher,
                        "Use launcher setting",
                    );
                    ui.selectable_value(
                        &mut self.radial_safety_policy,
                        RadialSafetyPolicy::AlwaysConfirmDestructive,
                        "Always confirm destructive radial actions",
                    );
                });
            let global_inputs = ui.checkbox(
                &mut self.radial_global_item_inputs,
                "Allow process-wide radial item shortcuts and hotstrings",
            );
            if global_inputs.changed() && !self.radial_global_item_inputs {
                self.radial_default_item_input_scope = TriggerScope::MenuLocal;
            }
            egui::ComboBox::from_label("New item input scope")
                .selected_text(match self.radial_default_item_input_scope {
                    TriggerScope::MenuLocal => "Current menu only",
                    TriggerScope::Global => "Process-wide",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.radial_default_item_input_scope,
                        TriggerScope::MenuLocal,
                        "Current menu only",
                    );
                    ui.add_enabled_ui(self.radial_global_item_inputs, |ui| {
                        ui.selectable_value(
                            &mut self.radial_default_item_input_scope,
                            TriggerScope::Global,
                            "Process-wide",
                        );
                    });
                });

        crate::gui::theme::widgets::secondary_help(ui, &format!(
                "Current shared trigger: {} ({})",
                self.hotkey,
                if self.radial_shared_tap_hold {
                    "tap launcher / hold radial"
                } else {
                    "legacy launcher timing"
                }
        ));
            let issues = self.radial_validation_issues(&document);
            if issues.is_empty() {
            crate::gui::theme::widgets::secondary_help(
                ui,
                "No conflicts with launcher, quit, help, Screen Draw, or global radial inputs.",
            );
            } else {
                for issue in issues {
                crate::gui::theme::widgets::semantic_status(
                    ui,
                    crate::gui::theme::widgets::StatusTone::Warning,
                    "Warning",
                    Some(&issue.message),
                );
                }
            }
        crate::gui::theme::widgets::secondary_help(
            ui,
            "Back returns from a submenu; Esc or `radial close` closes the radial tree.",
        );
        });
        ui.horizontal(|ui| {
            if ui.button("Edit menus...").clicked() {
                RadialCommandHost::open_radial_editor(app, false);
            }
            if ui.button("Edit skins...").clicked() {
                RadialCommandHost::open_radial_editor(app, true);
            }
        });
        if let Some(receipt) = app.radial_migration_receipt.as_ref() {
            match receipt.state {
                crate::settings::SubmenuMigrationState::Applied => {
                    crate::gui::theme::widgets::secondary_help(
                        ui,
                        "Existing submenu presentations were converted to Same center.",
                    );
                    if ui.button("Restore previous submenu presentation").clicked() {
                        app.request_radial_submenu_migration_restore();
                    }
                }
                crate::settings::SubmenuMigrationState::Prepared
                | crate::settings::SubmenuMigrationState::UndoPrepared => {
                    crate::gui::theme::widgets::secondary_help(
                        ui,
                        "Radial submenu migration recovery is pending.",
                    );
                }
                crate::settings::SubmenuMigrationState::Undone => {
                    crate::gui::theme::widgets::secondary_help(
                        ui,
                        "The one-time radial submenu migration has been restored.",
                    );
                }
                crate::settings::SubmenuMigrationState::Failed => {
                    crate::gui::theme::widgets::semantic_status(
                        ui,
                        crate::gui::theme::widgets::StatusTone::Error,
                        "Error",
                        Some(
                            receipt
                                .failure
                                .as_deref()
                                .unwrap_or("Radial migration failed."),
                        ),
                    );
                }
            }
        }
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
            crate::gui::theme::widgets::labeled_form_row(ui, field_label, |ui| {
                let resp = ui.text_edit_singleline(value);
                if resp.changed() {
                    *valid = crate::hotkey::parse_hotkey(value).is_some();
                    if *valid {
                        *last_valid = value.clone();
                    }
                }
                hotkey_indicator(ui, *valid);
                resp
            });
        }
    }

    fn render_actions_safety_feedback_section(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("After running an action").strong());
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut self.hide_after_run, "Hide window after running action");
            ui.checkbox(&mut self.preserve_command, "Preserve command after run");
            ui.checkbox(&mut self.clear_query_after_run, "Clear query after run");
        });

        ui.label(egui::RichText::new("Safety").strong());
        ui.checkbox(
            &mut self.require_confirm_destructive,
            "Require confirm for destructive actions",
        );

        ui.label(egui::RichText::new("Notifications and errors").strong());
        ui.checkbox(&mut self.show_toasts, "Enable toast notifications");
        if self.show_toasts {
            crate::gui::theme::widgets::labeled_form_row(ui, "Toast duration (s)", |ui| {
                ui.add(egui::Slider::new(&mut self.toast_duration, 0.1..=5.0).text(""))
            });
        }
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut self.show_inline_errors, "Show inline errors");
            ui.checkbox(&mut self.show_error_toasts, "Show error toasts");
        });

        ui.label(egui::RichText::new("Diagnostics and refresh").strong());
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
        ui.checkbox(
            &mut self.disable_timer_updates,
            "Disable timer auto refresh",
        );
        crate::gui::theme::widgets::labeled_form_row(ui, "Timer refresh rate (s)", |ui| {
            ui.add_enabled_ui(!self.disable_timer_updates, |ui| {
                ui.add(
                    egui::DragValue::new(&mut self.timer_refresh)
                        .range(0.1..=60.0)
                        .speed(0.1),
                )
            })
            .inner
        });
    }

    fn render_launcher_window_section(&mut self, ui: &mut egui::Ui, app: &mut LauncherApp) {
        ui.checkbox(&mut self.always_on_top, "Always on top");
        crate::gui::theme::widgets::labeled_form_row(ui, "Off-screen position", |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("X");
                ui.add(egui::DragValue::new(&mut self.offscreen_x));
                ui.label("Y");
                ui.add(egui::DragValue::new(&mut self.offscreen_y))
            })
            .inner
        });
        let follow_mouse_resp = ui.checkbox(&mut self.follow_mouse, "Follow mouse");
        if follow_mouse_resp.changed() && self.follow_mouse {
            self.static_enabled = false;
        }
        ui.add_enabled_ui(!self.follow_mouse, |ui| {
            ui.checkbox(&mut self.static_enabled, "Use static position");
        });
        if !self.follow_mouse && self.static_enabled {
            crate::gui::theme::widgets::labeled_form_row(ui, "Static X / Y / W / H", |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label("X");
                    ui.add(egui::DragValue::new(&mut self.static_x));
                    ui.label("Y");
                    ui.add(egui::DragValue::new(&mut self.static_y));
                    ui.label("W");
                    ui.add(egui::DragValue::new(&mut self.static_w));
                    ui.label("H");
                    ui.add(egui::DragValue::new(&mut self.static_h));
                    let snapshot = ui.button("Snapshot");
                    if snapshot.clicked() {
                        self.static_x = app.window_pos.0;
                        self.static_y = app.window_pos.1;
                        self.static_w = app.window_size.0;
                        self.static_h = app.window_size.1;
                    }
                    snapshot
                })
                .inner
            });
        }
        crate::gui::theme::widgets::labeled_form_row(ui, "Query scale", |ui| {
            ui.add(egui::Slider::new(&mut self.query_scale, 0.5..=5.0).text(""))
        });
        crate::gui::theme::widgets::labeled_form_row(ui, "List scale", |ui| {
            ui.add(egui::Slider::new(&mut self.list_scale, 0.5..=5.0).text(""))
        });
        if ui.button("Open Theme Settings...").clicked() {
            app.open_theme_settings_dialog();
        }
    }

    fn render_search_results_section(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.query_autocomplete, "Enable query autocomplete");
        crate::gui::theme::widgets::labeled_form_row(ui, "Fuzzy weight", |ui| {
            ui.add(egui::Slider::new(&mut self.fuzzy_weight, 0.0..=5.0).text(""))
        });
        crate::gui::theme::widgets::labeled_form_row(ui, "Usage weight", |ui| {
            ui.add(egui::Slider::new(&mut self.usage_weight, 0.0..=5.0).text(""))
        });
        ui.checkbox(&mut self.match_exact, "Match exact");
        crate::gui::theme::widgets::labeled_form_row(ui, "Page jump", |ui| {
            ui.add(
                egui::DragValue::new(&mut self.page_jump)
                    .range(1..=100)
                    .speed(1),
            )
        });
        ui.checkbox(
            &mut self.query_results_layout_enabled,
            "Display results in grid layout",
        );
        ui.add_enabled_ui(self.query_results_layout_enabled, |ui| {
            crate::gui::theme::widgets::labeled_form_row(ui, "Grid size", |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Rows");
                    ui.add(
                        egui::DragValue::new(&mut self.query_results_layout_rows)
                            .range(1..=100)
                            .speed(1),
                    );
                    ui.label("Columns");
                    ui.add(
                        egui::DragValue::new(&mut self.query_results_layout_cols)
                            .range(1..=100)
                            .speed(1),
                    )
                })
                .inner
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
    }

    fn render_dashboard_section(&mut self, ui: &mut egui::Ui, app: &mut LauncherApp) {
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

        for path in app.plugins.deferred_plugin_reload_paths() {
            ui.colored_label(
                egui::Color32::YELLOW,
                format!(
                    "Plugin reload pending for '{}'; close active plugin views and reload again.",
                    path.display()
                ),
            );
        }

        for name in app.plugins.plugin_names() {
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
            let Ok(mut plugin) = app.plugins.try_write_plugin(&name) else {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    format!("{name} settings are temporarily busy; retry next frame."),
                );
                continue;
            };
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
                            .range(1..=6)
                            .speed(1),
                    );
                });
                self.note_max_outline_depth = self.note_max_outline_depth.clamp(1, 6);

                ui.separator();
                ui.heading("Legacy note options");
                ui.checkbox(&mut self.note_save_on_close, "Save note on close (Esc)");
                ui.checkbox(
                    &mut self.note_confirm_discard_unsaved_changes,
                    "Confirm before discarding unsaved note changes",
                );
                ui.checkbox(
                    &mut self.note_always_overwrite,
                    "Always overwrite existing notes",
                );
                ui.checkbox(&mut self.note_images_as_links, "Display images as links");
                ui.checkbox(&mut self.note_show_details, "Show note details by default");
                ui.horizontal(|ui| {
                    ui.label("Tag/link preview limit");
                    ui.add(egui::DragValue::new(&mut self.note_more_limit).range(1..=usize::MAX));
                });
                ui.horizontal(|ui| {
                    ui.label("Note panel W");
                    ui.add(egui::DragValue::new(&mut self.note_panel_w).range(200.0..=2000.0));
                    ui.label("H");
                    ui.add(egui::DragValue::new(&mut self.note_panel_h).range(150.0..=1600.0));
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

#[cfg(test)]
mod radial_language_tests {
    use super::*;

    #[test]
    fn tap_hold_language_states_release_crossing_and_legacy_semantics() {
        assert!(RADIAL_TAP_HOLD_EXPLANATION.contains("Release before the threshold"));
        assert!(RADIAL_TAP_HOLD_EXPLANATION.contains("Crossing the threshold"));
        assert!(RADIAL_TAP_HOLD_EXPLANATION.contains("never also toggles"));
        assert!(RADIAL_LEGACY_TIMING_EXPLANATION.contains("toggles immediately"));
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
    let (tone, label) = if valid {
        (crate::gui::theme::widgets::StatusTone::Success, "Valid")
    } else {
        (crate::gui::theme::widgets::StatusTone::Error, "Invalid")
    };
    crate::gui::theme::widgets::semantic_status(ui, tone, label, None);
}

#[cfg(test)]
mod settings_shell_tests {
    use super::*;

    fn draw_sections(ctx: &egui::Context, expand_all: Option<bool>) -> (Vec<bool>, Vec<egui::Id>) {
        let mut open_states = Vec::new();
        let mut ids = Vec::new();
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(640.0, 520.0),
                )),
                ..Default::default()
            },
            |ui| {
                if let Some(open) = expand_all {
                    set_all_settings_sections_open(ui, open);
                }
                for section in TopLevelSettingsSection::ALL {
                    let id = section.egui_id(ui);
                    show_settings_section(ui, section, None, |ui| {
                        ui.label("section content");
                    });
                    open_states.push(
                        egui::collapsing_header::CollapsingState::load_with_default_open(
                            ctx,
                            id,
                            section.default_open(),
                        )
                        .is_open(),
                    );
                    ids.push(id);
                }
            },
        );
        output.drop_without_applying_deltas();
        (open_states, ids)
    }

    #[test]
    fn settings_sections_keep_ids_and_collapse_state_while_footer_reserves_space() {
        let ctx = egui::Context::default();
        let (defaults, ids) = draw_sections(&ctx, None);
        assert_eq!(
            defaults,
            vec![true, true, true, false, false, false, false],
            "the seven established default-open states remain intact"
        );

        let (expanded, expanded_ids) = draw_sections(&ctx, Some(true));
        assert!(expanded.iter().all(|open| *open));
        assert_eq!(expanded_ids, ids, "expand-all keeps persistent section IDs");

        let (collapsed, collapsed_ids) = draw_sections(&ctx, Some(false));
        assert!(collapsed.iter().all(|open| !open));
        assert_eq!(
            collapsed_ids, ids,
            "collapse-all keeps persistent section IDs"
        );

        let mut save_rect = egui::Rect::NOTHING;
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(640.0, 240.0),
                )),
                ..Default::default()
            },
            |ui| {
                let content_height = SettingsEditor::settings_content_height(
                    ui.available_height(),
                    settings_footer_reserved_height(ui),
                );
                egui::ScrollArea::vertical()
                    .min_scrolled_height(0.0)
                    .max_height(content_height)
                    .show(ui, |ui| ui.add_space(600.0));
                ui.separator();
                save_rect = ui.button("Save").rect;
            },
        );
        output.drop_without_applying_deltas();
        assert!(
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 240.0))
                .contains_rect(save_rect),
            "the Save control remains inside a short viewport"
        );
    }
}
