use super::state::SettingsEditor;
use crate::gui::LauncherApp;
use crate::gui::jump_mode::{
    TargetKind,
    ui::{JumpModeScope, RegistrationResult},
};
use crate::plugins::note::{NoteExternalOpen, NotePluginSettings};
use crate::settings::NoteViewMode;
use eframe::egui;

const RADIAL_TAP_HOLD_EXPLANATION: &str = "Release before the threshold to toggle the launcher. Crossing the threshold opens the radial menu; releasing afterward never also toggles the launcher.";
const RADIAL_LEGACY_TIMING_EXPLANATION: &str = "Legacy timing: the launcher hotkey toggles immediately; named radial commands and configured direct triggers remain available.";

#[derive(Clone, Copy)]
enum TopLevelSettingsSection {
    Hotkeys,
    PasteSequences,
    LauncherWindow,
    SearchResults,
    ActionsFeedback,
    Dashboard,
    Radial,
    Plugins,
}

impl TopLevelSettingsSection {
    const ALL: [Self; 8] = [
        Self::Hotkeys,
        Self::PasteSequences,
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
            Self::PasteSequences => "settings_section_paste_sequences",
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
            Self::PasteSequences => "Paste Sequences",
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
            Self::PasteSequences => {
                "Configure playback controls. Saved sequence start shortcuts are edited in Paste Sequences."
            }
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

/// Optional, frame-local adapter for the native controls owned by this editor.
/// It registers only the real `Response`s returned by the existing widgets.
struct SettingsJumpTargets {
    scope: Option<JumpModeScope>,
    next_visual_order: u64,
    first_ui_pass: bool,
    activation_guarded: bool,
}

impl SettingsJumpTargets {
    fn new(scope: Option<JumpModeScope>, first_ui_pass: bool, activation_guarded: bool) -> Self {
        Self {
            scope,
            next_visual_order: 0,
            first_ui_pass,
            activation_guarded,
        }
    }

    fn register(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        identity: &'static str,
        label: impl Into<String>,
        kind: TargetKind,
    ) {
        let visual_order = self.next_visual_order;
        self.next_visual_order = self.next_visual_order.saturating_add(1);
        self.register_at(ui, response, identity, label, kind, visual_order);
    }

    fn reserve_order(&mut self) -> u64 {
        let visual_order = self.next_visual_order;
        self.next_visual_order = self.next_visual_order.saturating_add(1);
        visual_order
    }

    fn register_at(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        identity: &'static str,
        label: impl Into<String>,
        kind: TargetKind,
        visual_order: u64,
    ) {
        self.register_at_with_paint_label_rect(
            ui,
            response,
            identity,
            label,
            kind,
            visual_order,
            None,
        );
    }

    fn register_at_with_paint_label_rect(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        identity: &'static str,
        label: impl Into<String>,
        kind: TargetKind,
        visual_order: u64,
        paint_label_rect: Option<egui::Rect>,
    ) {
        let Some(scope) = self.scope.as_ref() else {
            return;
        };
        let label = label.into();
        let registration = match paint_label_rect {
            Some(rect) => scope.register_with_paint_label_rect(
                ui,
                response,
                identity,
                label,
                kind,
                visual_order,
                rect,
            ),
            None => scope.register(ui, response, identity, label, kind, visual_order),
        };
        if let RegistrationResult::Registered {
            directive: Some(directive),
            ..
        } = registration
        {
            directive.apply_to_response(response);
        }
    }

    fn checkbox(
        &mut self,
        ui: &mut egui::Ui,
        value: &mut bool,
        identity: &'static str,
        label: &str,
    ) -> egui::Response {
        let response = ui.checkbox(value, label);
        self.register(ui, &response, identity, label, TargetKind::Checkbox);
        response
    }

    fn text_edit_singleline(
        &mut self,
        ui: &mut egui::Ui,
        value: &mut String,
        identity: &'static str,
        label: &str,
    ) -> egui::Response {
        let response = ui.text_edit_singleline(value);
        self.register(ui, &response, identity, label, TargetKind::TextEntry);
        response
    }

    fn response(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        identity: &'static str,
        label: &str,
        kind: TargetKind,
    ) {
        self.register(ui, response, identity, label, kind);
    }

    fn slider_with_numeric_editor(
        &mut self,
        ui: &mut egui::Ui,
        value: &mut f32,
        range: std::ops::RangeInclusive<f32>,
        identity: &'static str,
        label: &'static str,
        numeric_identity: &'static str,
        numeric_label: &'static str,
    ) -> egui::Response {
        let slider = ui.add(
            egui::Slider::new(value, range.clone())
                .text("")
                .show_value(false),
        );
        self.response(ui, &slider, identity, label, TargetKind::Slider);

        let numeric = ui.add(egui::DragValue::new(value).range(range).speed(0.1));
        self.response(
            ui,
            &numeric,
            numeric_identity,
            numeric_label,
            TargetKind::TextEntry,
        );
        slider | numeric
    }

    fn combo_popup_scope(&self, identity: &'static str) -> Option<JumpModeScope> {
        self.scope
            .as_ref()
            .map(|scope| scope.combo_popup_scope(identity))
    }

    fn popup_option(
        &mut self,
        popup_scope: Option<&JumpModeScope>,
        ui: &egui::Ui,
        response: &egui::Response,
        identity: &'static str,
        label: &'static str,
    ) {
        let order = self.reserve_order();
        let Some(scope) = popup_scope else {
            return;
        };
        if let RegistrationResult::Registered {
            directive: Some(directive),
            ..
        } = scope.register(ui, response, identity, label, TargetKind::Selectable, order)
        {
            directive.apply_to_response(response);
        }
    }

    fn button(
        &mut self,
        ui: &mut egui::Ui,
        identity: &'static str,
        label: &'static str,
    ) -> egui::Response {
        let response = ui.button(label);
        self.register(ui, &response, identity, label, TargetKind::Button);
        response
    }

    fn clicked(&self, response: &egui::Response) -> bool {
        response.clicked() && (!self.activation_guarded || self.first_ui_pass)
    }
}

fn show_settings_section(
    ui: &mut egui::Ui,
    section: TopLevelSettingsSection,
    forced_open: Option<bool>,
    targets: &mut SettingsJumpTargets,
    add_contents: impl FnOnce(&mut egui::Ui, &mut SettingsJumpTargets),
) -> egui::Rect {
    let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
        ui.ctx(),
        section.egui_id(ui),
        section.default_open(),
    );
    if let Some(open) = forced_open {
        state.set_open(open);
    }
    let header_visual_order = targets.reserve_order();
    let mut title_rect = egui::Rect::NOTHING;
    let header = state.show_header(ui, |ui| {
        title_rect = ui.heading(section.title()).rect;
    });
    let (toggle_response, header, _) = header.body(|ui| {
        crate::gui::theme::widgets::secondary_help(ui, section.description());
        add_contents(ui, targets);
    });
    targets.register_at_with_paint_label_rect(
        ui,
        &toggle_response,
        section.persistent_id(),
        section.title(),
        TargetKind::Button,
        header_visual_order,
        Some(title_rect),
    );
    header.response.rect
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
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        app: &mut LauncherApp,
        jump_mode_scope: Option<JumpModeScope>,
        first_ui_pass: bool,
    ) {
        let mut jump_targets =
            SettingsJumpTargets::new(jump_mode_scope, first_ui_pass, app.jump_mode_enabled());
        let mut open = app.show_settings;
        let mut cancel = false;
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
                                let expand =
                                    jump_targets.button(ui, "sections.expand_all", "Expand all");
                                if jump_targets.clicked(&expand) {
                                    request = Some(true);
                                }
                                let collapse = jump_targets.button(
                                    ui,
                                    "sections.collapse_all",
                                    "Collapse all",
                                );
                                if jump_targets.clicked(&collapse) {
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
                            &mut jump_targets,
                            |ui, targets| self.render_hotkey_section(ui, targets),
                        );
                        let reveal_paste = std::mem::take(&mut self.reveal_paste_sequence);
                        let paste_header = show_settings_section(
                            ui,
                            TopLevelSettingsSection::PasteSequences,
                            if reveal_paste {
                                Some(true)
                            } else {
                                top_level_expand_request
                            },
                            &mut jump_targets,
                            |ui, targets| {
                                ui.label(app.paste_sequence_shortcut_summary());
                                ui.separator();
                                self.render_paste_sequence_section(ui, targets);
                            },
                        );
                        if reveal_paste {
                            ui.scroll_to_rect(paste_header, Some(egui::Align::Min));
                        }
                        show_settings_section(
                            ui,
                            TopLevelSettingsSection::LauncherWindow,
                            top_level_expand_request,
                            &mut jump_targets,
                            |ui, targets| self.render_launcher_window_section(ui, app, targets),
                        );
                        show_settings_section(
                            ui,
                            TopLevelSettingsSection::SearchResults,
                            top_level_expand_request,
                            &mut jump_targets,
                            |ui, targets| self.render_search_results_section(ui, targets),
                        );
                        show_settings_section(
                            ui,
                            TopLevelSettingsSection::ActionsFeedback,
                            top_level_expand_request,
                            &mut jump_targets,
                            |ui, targets| self.render_actions_safety_feedback_section(ui, targets),
                        );
                        show_settings_section(
                            ui,
                            TopLevelSettingsSection::Dashboard,
                            top_level_expand_request,
                            &mut jump_targets,
                            |ui, targets| self.render_dashboard_section(ui, app, targets),
                        );
                        show_settings_section(
                            ui,
                            TopLevelSettingsSection::Radial,
                            top_level_expand_request,
                            &mut jump_targets,
                            |ui, targets| self.render_radial_section(ui, app, targets),
                        );
                        show_settings_section(
                            ui,
                            TopLevelSettingsSection::Plugins,
                            top_level_expand_request,
                            &mut jump_targets,
                            |ui, targets| self.render_plugin_sections(ui, app, targets),
                        );
                        self.expand_request = None;
                    });

                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    let pending = app.settings_save_pending();
                    let save = ui.add_enabled(!pending, egui::Button::new("Save"));
                    let save = crate::gui::theme::widgets::with_button_emphasis(
                        ui,
                        save,
                        crate::gui::theme::widgets::ButtonEmphasis::Primary,
                    );
                    jump_targets.response(ui, &save, "footer.save", "Save", TargetKind::Button);
                    if jump_targets.clicked(&save) {
                        self.save_settings(ctx, app);
                    }
                    let cancel_button = ui.add_enabled(!pending, egui::Button::new("Cancel"));
                    jump_targets.register(
                        ui,
                        &cancel_button,
                        "footer.cancel",
                        "Cancel",
                        TargetKind::Button,
                    );
                    if jump_targets.clicked(&cancel_button) {
                        cancel = true;
                    }
                    if app.settings_save_reconciliation_required() {
                        ui.label("Hotkey state needs reconciliation; saving is disabled.");
                    } else if pending {
                        ui.label("Saving settings…");
                    }
                });
            });
        if cancel {
            *self = Self::new_with_plugins(&self.baseline_settings);
            open = false;
        }
        app.show_settings = open;
    }

    fn render_paste_sequence_section(
        &mut self,
        ui: &mut egui::Ui,
        targets: &mut SettingsJumpTargets,
    ) {
        render_paste_shortcut(
            ui,
            "Next",
            "paste.next.enabled",
            "paste.next.hotkey",
            &mut self.paste_next_enabled,
            &mut self.paste_sequence.next_hotkey,
            targets,
        );
        render_paste_shortcut(
            ui,
            "Cancel",
            "paste.cancel.enabled",
            "paste.cancel.hotkey",
            &mut self.paste_cancel_enabled,
            &mut self.paste_sequence.cancel_hotkey,
            targets,
        );
        targets.checkbox(
            ui,
            &mut self.paste_sequence.escape_cancel_enabled,
            "paste.escape_cancel",
            "Escape cancels active playback",
        );
        targets.checkbox(
            ui,
            &mut self.paste_sequence.keep_dialog_visible_during_playback,
            "paste.keep_visible",
            "Keep Paste Sequences dialog visible during playback",
        );
        crate::gui::theme::widgets::secondary_help(
            ui,
            "When started from this dialog, leave it open while you paste into another application.",
        );
        crate::gui::theme::widgets::secondary_help(
            ui,
            "Empty shortcuts are disabled. Shortcut conflicts are checked on Save. Changes take effect after Save succeeds.",
        );
    }

    fn render_hotkey_section(&mut self, ui: &mut egui::Ui, targets: &mut SettingsJumpTargets) {
        crate::gui::theme::widgets::labeled_form_row(ui, "Launcher hotkey", |ui| {
            let resp = targets.text_edit_singleline(
                ui,
                &mut self.hotkey,
                "hotkeys.launcher",
                "Launcher hotkey",
            );
            if resp.changed() {
                self.hotkey_valid = crate::hotkey::parse_hotkey(&self.hotkey).is_some();
                if self.hotkey_valid {
                    self.last_valid_hotkey = self.hotkey.clone();
                }
            }
            hotkey_indicator(ui, self.hotkey_valid);
            resp
        });
        self.render_optional_hotkey(
            ui,
            targets,
            "Enable quit hotkey",
            "Quit hotkey",
            HotkeyKind::Quit,
        );
        self.render_optional_hotkey(
            ui,
            targets,
            "Enable help hotkey",
            "Help hotkey",
            HotkeyKind::Help,
        );
    }

    fn render_radial_section(
        &mut self,
        ui: &mut egui::Ui,
        app: &mut LauncherApp,
        targets: &mut SettingsJumpTargets,
    ) {
        use crate::commands::RadialCommandHost;
        use crate::radial::model::{
            InteractionMode, RadialSafetyPolicy, SubmenuPresentation, TooltipScope, TriggerScope,
        };

        targets.checkbox(
            ui,
            &mut self.radial_enabled,
            "radial.enabled",
            "Enable radial menus",
        );
        ui.add_enabled_ui(self.radial_enabled, |ui| {
            targets.checkbox(
                ui,
                &mut self.radial_shared_tap_hold,
                "radial.shared_tap_hold",
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
            targets.checkbox(
                ui,
                &mut self.radial_show_expected_layout_diagnostics,
                "radial.expected_layout_diagnostics",
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
            let global_inputs = targets.checkbox(
                ui,
                &mut self.radial_global_item_inputs,
                "radial.global_item_inputs",
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
            let edit_menus = targets.button(ui, "radial.edit_menus", "Edit menus...");
            if targets.clicked(&edit_menus) {
                RadialCommandHost::open_radial_editor(app, false);
            }
            let edit_skins = targets.button(ui, "radial.edit_skins", "Edit skins...");
            if targets.clicked(&edit_skins) {
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
                    let restore = targets.button(
                        ui,
                        "radial.restore_submenu_presentation",
                        "Restore previous submenu presentation",
                    );
                    if targets.clicked(&restore) {
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
        targets: &mut SettingsJumpTargets,
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
        let (toggle_identity, field_identity) = match kind {
            HotkeyKind::Quit => ("hotkeys.quit.enabled", "hotkeys.quit.value"),
            HotkeyKind::Help => ("hotkeys.help.enabled", "hotkeys.help.value"),
        };
        targets.checkbox(ui, enabled, toggle_identity, toggle_label);
        if *enabled {
            crate::gui::theme::widgets::labeled_form_row(ui, field_label, |ui| {
                let resp = targets.text_edit_singleline(ui, value, field_identity, field_label);
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

    fn render_actions_safety_feedback_section(
        &mut self,
        ui: &mut egui::Ui,
        targets: &mut SettingsJumpTargets,
    ) {
        ui.label(egui::RichText::new("After running an action").strong());
        ui.horizontal_wrapped(|ui| {
            targets.checkbox(
                ui,
                &mut self.hide_after_run,
                "actions.hide_after_run",
                "Hide window after running action",
            );
            targets.checkbox(
                ui,
                &mut self.preserve_command,
                "actions.preserve_command",
                "Preserve command after run",
            );
            targets.checkbox(
                ui,
                &mut self.clear_query_after_run,
                "actions.clear_query_after_run",
                "Clear query after run",
            );
        });

        ui.label(egui::RichText::new("Safety").strong());
        targets.checkbox(
            ui,
            &mut self.require_confirm_destructive,
            "actions.confirm_destructive",
            "Require confirm for destructive actions",
        );

        ui.label(egui::RichText::new("Notifications and errors").strong());
        targets.checkbox(
            ui,
            &mut self.show_toasts,
            "feedback.toasts",
            "Enable toast notifications",
        );
        if self.show_toasts {
            crate::gui::theme::widgets::labeled_form_row(ui, "Toast duration (s)", |ui| {
                targets.slider_with_numeric_editor(
                    ui,
                    &mut self.toast_duration,
                    0.1..=5.0,
                    "feedback.toast_duration",
                    "Toast duration",
                    "feedback.toast_duration/value",
                    "Toast duration value",
                )
            });
        }
        ui.horizontal_wrapped(|ui| {
            targets.checkbox(
                ui,
                &mut self.show_inline_errors,
                "feedback.inline_errors",
                "Show inline errors",
            );
            targets.checkbox(
                ui,
                &mut self.show_error_toasts,
                "feedback.error_toasts",
                "Show error toasts",
            );
        });

        ui.label(egui::RichText::new("Diagnostics and refresh").strong());
        ui.horizontal(|ui| {
            let combo_identity = "feedback.debug_logging";
            let combo_order = targets.reserve_order();
            let popup_scope = targets.combo_popup_scope(combo_identity);
            let combo = egui::ComboBox::from_label("Debug logging")
                .selected_text(if self.debug_logging {
                    "Enabled"
                } else {
                    "Disabled"
                })
                .show_ui(ui, |ui| {
                    let disabled = ui.selectable_value(&mut self.debug_logging, false, "Disabled");
                    targets.popup_option(
                        popup_scope.as_ref(),
                        ui,
                        &disabled,
                        "option/disabled",
                        "Debug logging: Disabled",
                    );
                    let enabled = ui.selectable_value(&mut self.debug_logging, true, "Enabled");
                    targets.popup_option(
                        popup_scope.as_ref(),
                        ui,
                        &enabled,
                        "option/enabled",
                        "Debug logging: Enabled",
                    );
                });
            targets.register_at(
                ui,
                &combo.response,
                combo_identity,
                "Debug logging",
                TargetKind::ComboBox,
                combo_order,
            );
        });
        targets.checkbox(
            ui,
            &mut self.disable_timer_updates,
            "feedback.disable_timer_updates",
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

    fn render_launcher_window_section(
        &mut self,
        ui: &mut egui::Ui,
        app: &mut LauncherApp,
        targets: &mut SettingsJumpTargets,
    ) {
        targets.checkbox(
            ui,
            &mut self.always_on_top,
            "launcher.always_on_top",
            "Always on top",
        );
        ui.horizontal_wrapped(|ui| {
            targets.checkbox(
                ui,
                &mut self.show_launcher_icon,
                "launcher.show_icon",
                "Show Launcher Icon",
            );
            targets.checkbox(
                ui,
                &mut self.show_launcher_title,
                "launcher.show_title",
                "Show Launcher Title",
            );
        });
        targets.checkbox(
            ui,
            &mut self.experimental_jump_mode_enabled,
            "launcher.experimental_jump_mode",
            "Experimental Jump Mode",
        )
        .on_hover_text(
            "Shift+Space displays hints; typing a hint selects, Enter activates. Available in supported screens only.",
        );
        crate::gui::theme::widgets::labeled_form_row(ui, "Off-screen position", |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("X");
                ui.add(egui::DragValue::new(&mut self.offscreen_x));
                ui.label("Y");
                ui.add(egui::DragValue::new(&mut self.offscreen_y))
            })
            .inner
        });
        let follow_mouse_resp = targets.checkbox(
            ui,
            &mut self.follow_mouse,
            "launcher.follow_mouse",
            "Follow mouse",
        );
        if follow_mouse_resp.changed() && self.follow_mouse {
            self.static_enabled = false;
        }
        ui.add_enabled_ui(!self.follow_mouse, |ui| {
            targets.checkbox(
                ui,
                &mut self.static_enabled,
                "launcher.static_position",
                "Use static position",
            );
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
                    let snapshot = targets.button(ui, "launcher.snapshot", "Snapshot");
                    if targets.clicked(&snapshot) {
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
            targets.slider_with_numeric_editor(
                ui,
                &mut self.query_scale,
                0.5..=5.0,
                "launcher.query_scale",
                "Query scale",
                "launcher.query_scale/value",
                "Query scale value",
            )
        });
        crate::gui::theme::widgets::labeled_form_row(ui, "List scale", |ui| {
            targets.slider_with_numeric_editor(
                ui,
                &mut self.list_scale,
                0.5..=5.0,
                "launcher.list_scale",
                "List scale",
                "launcher.list_scale/value",
                "List scale value",
            )
        });
        let theme_settings =
            targets.button(ui, "launcher.open_theme_settings", "Open Theme Settings...");
        if targets.clicked(&theme_settings) {
            app.open_theme_settings_dialog();
        }
    }

    fn render_search_results_section(
        &mut self,
        ui: &mut egui::Ui,
        targets: &mut SettingsJumpTargets,
    ) {
        targets.checkbox(
            ui,
            &mut self.query_autocomplete,
            "search.autocomplete",
            "Enable query autocomplete",
        );
        crate::gui::theme::widgets::labeled_form_row(ui, "Fuzzy weight", |ui| {
            targets.slider_with_numeric_editor(
                ui,
                &mut self.fuzzy_weight,
                0.0..=5.0,
                "search.fuzzy_weight",
                "Fuzzy weight",
                "search.fuzzy_weight/value",
                "Fuzzy weight value",
            )
        });
        crate::gui::theme::widgets::labeled_form_row(ui, "Usage weight", |ui| {
            targets.slider_with_numeric_editor(
                ui,
                &mut self.usage_weight,
                0.0..=5.0,
                "search.usage_weight",
                "Usage weight",
                "search.usage_weight/value",
                "Usage weight value",
            )
        });
        targets.checkbox(
            ui,
            &mut self.match_exact,
            "search.match_exact",
            "Match exact",
        );
        crate::gui::theme::widgets::labeled_form_row(ui, "Page jump", |ui| {
            ui.add(
                egui::DragValue::new(&mut self.page_jump)
                    .range(1..=100)
                    .speed(1),
            )
        });
        targets.checkbox(
            ui,
            &mut self.query_results_layout_enabled,
            "search.grid_layout",
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
            targets.checkbox(
                ui,
                &mut self.query_results_layout_respect_plugin_capability,
                "search.respect_plugin_capability",
                "Respect plugin list/grid capability",
            );
            ui.horizontal(|ui| {
                ui.label("Force list for plugins (comma separated)");
                targets.text_edit_singleline(
                    ui,
                    &mut self.query_results_layout_plugin_opt_out,
                    "search.plugin_opt_out",
                    "Force list for plugins",
                );
            });
        });
    }

    fn render_dashboard_section(
        &mut self,
        ui: &mut egui::Ui,
        app: &mut LauncherApp,
        targets: &mut SettingsJumpTargets,
    ) {
        targets.checkbox(
            ui,
            &mut self.dashboard_enabled,
            "dashboard.enabled",
            "Enable dashboard when query is empty",
        );
        ui.horizontal(|ui| {
            ui.label("Dashboard config path");
            targets.text_edit_singleline(
                ui,
                &mut self.dashboard_path,
                "dashboard.path",
                "Dashboard config path",
            );
        });
        ui.horizontal(|ui| {
            ui.label("Default location");
            targets.text_edit_singleline(
                ui,
                &mut self.dashboard_default_location,
                "dashboard.default_location",
                "Default location",
            );
        });
        targets.checkbox(
            ui,
            &mut self.dashboard_show_when_empty,
            "dashboard.show_when_empty",
            "Show dashboard when the search box is blank",
        );
        targets.checkbox(
            ui,
            &mut self.reduce_dashboard_work_when_unfocused,
            "dashboard.reduce_work_unfocused",
            "Reduce dashboard work when not focused",
        );
        if self.debug_logging || cfg!(debug_assertions) {
            targets.checkbox(
                ui,
                &mut self.show_dashboard_diagnostics,
                "dashboard.diagnostics",
                "Show dashboard diagnostics (dev)",
            );
        }
        let customize = targets.button(ui, "dashboard.customize", "Customize Dashboard...");
        if targets.clicked(&customize) {
            app.show_dashboard_editor = true;
        }
    }

    fn render_plugin_sections(
        &mut self,
        ui: &mut egui::Ui,
        app: &mut LauncherApp,
        targets: &mut SettingsJumpTargets,
    ) {
        ui.separator();
        let toggle_sections = targets.button(
            ui,
            "plugins.toggle_sections",
            if self.plugins_expanded {
                "Collapse plugin sections"
            } else {
                "Expand plugin sections"
            },
        );
        if targets.clicked(&toggle_sections) {
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

fn render_paste_shortcut(
    ui: &mut egui::Ui,
    name: &str,
    enabled_identity: &'static str,
    value_identity: &'static str,
    enabled: &mut bool,
    value: &mut Option<String>,
    targets: &mut SettingsJumpTargets,
) {
    ui.horizontal_wrapped(|ui| {
        let enable_label = format!("Enable {name} shortcut");
        let enabled_response = ui.checkbox(enabled, &enable_label);
        targets.response(
            ui,
            &enabled_response,
            enabled_identity,
            &enable_label,
            TargetKind::Checkbox,
        );
        ui.add_enabled_ui(*enabled, |ui| {
            let text = value.get_or_insert_with(String::new);
            let response =
                ui.add(egui::TextEdit::singleline(text).hint_text(format!("{name} shortcut")));
            targets.response(
                ui,
                &response,
                value_identity,
                match name {
                    "Next" => "Next shortcut",
                    "Cancel" => "Cancel shortcut",
                    _ => "Paste Sequence shortcut",
                },
                TargetKind::TextEntry,
            );
        });
        if !*enabled {
            ui.label("Disabled");
        } else {
            match crate::settings::PasteSequencePreferences::parse_optional_hotkey(value.as_deref())
            {
                Ok(None) => {
                    ui.label("Disabled (empty shortcut)");
                }
                Ok(Some(_)) => hotkey_indicator(ui, true),
                Err(_) => hotkey_indicator(ui, false),
            }
        }
    });
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

    fn paste_settings_frame(
        ctx: &egui::Context,
        app: &mut LauncherApp,
        events: Vec<egui::Event>,
    ) -> Vec<(String, egui::Rect)> {
        ctx.begin_pass(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000.0, 900.0),
            )),
            events,
            ..Default::default()
        });
        let mut editor = std::mem::take(&mut app.settings_editor);
        editor.ui(ctx, app, None, true);
        app.settings_editor = editor;
        let output = ctx.end_pass();
        fn collect(shape: &egui::epaint::Shape, labels: &mut Vec<(String, egui::Rect)>) {
            match shape {
                egui::epaint::Shape::Text(text) => labels.push((
                    text.galley.text().to_owned(),
                    text.galley.rect.translate(text.pos.to_vec2()),
                )),
                egui::epaint::Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect(shape, labels);
                    }
                }
                _ => {}
            }
        }
        let mut labels = Vec::new();
        for clipped in &output.shapes {
            collect(&clipped.shape, &mut labels);
        }
        output.drop_without_applying_deltas();
        labels
    }

    fn paste_settings_click(ctx: &egui::Context, app: &mut LauncherApp, label: &str) {
        let labels = paste_settings_frame(ctx, app, vec![]);
        let label_rect = labels
            .iter()
            .find(|(text, _)| text == label)
            .unwrap_or_else(|| panic!("missing {label}"))
            .1;
        let mut pos = label_rect.center();
        if label == "Search & Results" {
            // Custom Settings headers paint their title as a label; egui's
            // collapsing interaction is the narrow disclosure control just
            // to its left.
            pos.x = label_rect.left() - 8.0;
        }
        for pressed in [true, false] {
            paste_settings_frame(
                ctx,
                app,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
    }

    fn settings_test_app(
        ctx: &egui::Context,
        directory: &tempfile::TempDir,
        mut settings: crate::settings::Settings,
    ) -> LauncherApp {
        use std::sync::{Arc, atomic::AtomicBool};

        settings.index_paths = Some(Vec::new());
        settings.plugin_dirs = Some(Vec::new());
        settings.enabled_plugins = Some(std::collections::HashSet::new());
        settings.paste_sequence.next_hotkey = None;
        settings.paste_sequence.cancel_hotkey = None;
        settings
            .save(directory.path().join("settings.json").to_str().unwrap())
            .unwrap();

        LauncherApp::new(
            ctx,
            Arc::new(Vec::new()),
            0,
            crate::plugin::PluginManager::new_inert_for_test(),
            directory
                .path()
                .join("actions.json")
                .to_string_lossy()
                .into_owned(),
            directory
                .path()
                .join("settings.json")
                .to_string_lossy()
                .into_owned(),
            settings,
            None,
            None,
            Some(std::collections::HashSet::new()),
            Some(std::collections::HashMap::new()),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        )
    }

    fn settings_jump_frame(
        ctx: &egui::Context,
        app: &mut LauncherApp,
        jump: &mut crate::gui::jump_mode::ui::JumpModeUi,
        events: Vec<egui::Event>,
        focus_target: Option<crate::gui::jump_mode::TargetId>,
        first_ui_pass: bool,
    ) -> Vec<crate::gui::jump_mode::ui::RegisteredTarget> {
        use crate::gui::jump_mode::ui::NavigationOwner;
        use crate::gui::jump_mode::{SurfaceId, TargetGeneration};

        ctx.begin_pass(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1100.0, 900.0),
            )),
            focused: true,
            events,
            ..Default::default()
        });
        jump.begin_frame();
        let owner = NavigationOwner::Settings {
            instance: jump.settings_instance(),
        };
        assert_eq!(jump.update_navigation_owner(ctx, owner, false), owner);
        if let Some(target) = focus_target {
            assert!(jump.queue_focus_intent(target, ctx.viewport_id(), jump.navigation_epoch()));
        }
        jump.begin_ui_pass();
        let scope = jump
            .scope_for_navigation_owner(
                SurfaceId::Settings {
                    instance: jump.settings_instance(),
                },
                TargetGeneration(jump.settings_instance()),
                "settings",
                ctx.viewport_id(),
                owner,
                false,
            )
            .expect("enabled Settings owner creates a collection scope");
        let mut editor = std::mem::take(&mut app.settings_editor);
        editor.ui(ctx, app, Some(scope), first_ui_pass);
        app.settings_editor = editor;
        jump.finish_ui_pass();
        let targets = jump
            .pass_registry()
            .expect("Settings pass is collected")
            .targets()
            .to_vec();
        let output = ctx.end_pass();
        assert!(jump.prepare_pass_for_navigation_owner(ctx, owner, false, true));
        jump.complete_frame_for_navigation_owner(ctx, owner, false, true);
        output.drop_without_applying_deltas();
        targets
    }

    fn settings_jump_discard_frame(
        ctx: &egui::Context,
        app: &mut LauncherApp,
        jump: &mut crate::gui::jump_mode::ui::JumpModeUi,
        events: Vec<egui::Event>,
    ) -> (
        Vec<crate::gui::jump_mode::ui::RegisteredTarget>,
        Vec<SettingsDiscardPassState>,
        usize,
    ) {
        use crate::gui::jump_mode::ui::NavigationOwner;
        use crate::gui::jump_mode::{SurfaceId, TargetGeneration};

        let owner = NavigationOwner::Settings {
            instance: jump.settings_instance(),
        };
        jump.begin_frame();
        assert_eq!(jump.update_navigation_owner(ctx, owner, false), owner);
        let mut state_after_pass = Vec::new();
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 900.0),
                )),
                focused: true,
                events,
                ..Default::default()
            },
            |ui| {
                let first_ui_pass = ui.output(|output| output.num_completed_passes == 0);
                jump.begin_ui_pass();
                let scope = jump
                    .scope_for_navigation_owner(
                        SurfaceId::Settings {
                            instance: jump.settings_instance(),
                        },
                        TargetGeneration(jump.settings_instance()),
                        "settings",
                        ctx.viewport_id(),
                        owner,
                        false,
                    )
                    .expect("enabled Settings owner creates a collection scope");
                let mut editor = std::mem::take(&mut app.settings_editor);
                editor.ui(ctx, app, Some(scope), first_ui_pass);
                app.settings_editor = editor;
                jump.finish_ui_pass();
                let hotkey_cursor = jump
                    .pass_registry()
                    .and_then(|registry| {
                        registry
                            .targets()
                            .iter()
                            .find(|target| target.metadata().label() == "Launcher hotkey")
                    })
                    .and_then(|target| egui::TextEdit::load_state(ctx, target.response_id()))
                    .and_then(|state| state.cursor.char_range().map(|range| range.primary.index.0));
                let paste_body_visible = jump.pass_registry().is_some_and(|registry| {
                    registry
                        .targets()
                        .iter()
                        .any(|target| target.metadata().label() == "Enable Next shortcut")
                });
                state_after_pass.push(SettingsDiscardPassState {
                    quit_hotkey_enabled: app.settings_editor.quit_hotkey_enabled,
                    hotkey: app.settings_editor.hotkey.clone(),
                    hotkey_cursor,
                    paste_body_visible,
                });
                if first_ui_pass {
                    ui.ctx()
                        .request_discard("Settings native-control state test");
                }
            },
        );
        let targets = jump
            .pass_registry()
            .expect("retained Settings pass is collected")
            .targets()
            .to_vec();
        assert!(jump.prepare_pass_for_navigation_owner(ctx, owner, false, true));
        jump.complete_frame_for_navigation_owner(ctx, owner, false, true);
        let passes = output.platform_output.num_completed_passes;
        output.drop_without_applying_deltas();
        (targets, state_after_pass, passes)
    }

    fn target_by_label<'a>(
        targets: &'a [crate::gui::jump_mode::ui::RegisteredTarget],
        label: &str,
    ) -> &'a crate::gui::jump_mode::ui::RegisteredTarget {
        targets
            .iter()
            .find(|target| target.metadata().label() == label)
            .unwrap_or_else(|| {
                let available = targets
                    .iter()
                    .map(|target| target.metadata().label())
                    .collect::<Vec<_>>()
                    .join(", ");
                panic!("missing Settings Jump target {label}; available: {available}")
            })
    }

    fn settings_scroll_until_target(
        ctx: &egui::Context,
        app: &mut LauncherApp,
        jump: &mut crate::gui::jump_mode::ui::JumpModeUi,
        mut targets: Vec<crate::gui::jump_mode::ui::RegisteredTarget>,
        wanted_label: &str,
    ) -> Vec<crate::gui::jump_mode::ui::RegisteredTarget> {
        let scroll_position = target_by_label(&targets, "Expand all").rect().center();
        for _ in 0..16 {
            if targets
                .iter()
                .any(|target| target.metadata().label() == wanted_label)
            {
                return targets;
            }
            targets = settings_jump_frame(
                ctx,
                app,
                jump,
                vec![
                    egui::Event::PointerMoved(scroll_position),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0.0, -180.0),
                        phase: egui::TouchPhase::Move,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                None,
                true,
            );
        }
        target_by_label(&targets, wanted_label);
        targets
    }

    struct SettingsDiscardPassState {
        quit_hotkey_enabled: bool,
        hotkey: String,
        hotkey_cursor: Option<usize>,
        paste_body_visible: bool,
    }

    #[test]
    fn paste_settings_navigation_controls_and_cancel_preserve_committed_state() {
        use std::sync::{Arc, atomic::AtomicBool};
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let mut settings = crate::settings::Settings::default();
        settings.enable_toasts = false;
        settings.dashboard.enabled = false;
        settings.paste_sequence.cancel_hotkey = Some("Ctrl+F10".into());
        settings.save(path.to_str().unwrap()).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let ctx = egui::Context::default();
        ctx.global_style_mut(|style| style.animation_time = 0.0);
        let mut app = LauncherApp::new(
            &ctx,
            Arc::new(Vec::new()),
            0,
            crate::plugin::PluginManager::new_inert_for_test(),
            directory
                .path()
                .join("actions.json")
                .to_string_lossy()
                .into_owned(),
            path.to_string_lossy().into_owned(),
            settings.clone(),
            None,
            None,
            Some(std::collections::HashSet::new()),
            Some(std::collections::HashMap::new()),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        );
        app.open_paste_sequence_settings_dialog();
        assert!(app.show_settings && app.settings_editor.reveal_paste_sequence);
        app.settings_editor.paste_sequence.next_hotkey = Some("Ctrl+F9".into());
        app.open_paste_sequence_settings_dialog();
        assert_eq!(
            app.settings_editor.paste_sequence.next_hotkey.as_deref(),
            Some("Ctrl+F9")
        );
        for _ in 0..3 {
            paste_settings_frame(&ctx, &mut app, vec![]);
        }
        assert!(!app.settings_editor.reveal_paste_sequence);
        paste_settings_click(&ctx, &mut app, "Enable Cancel shortcut");
        assert!(!app.settings_editor.paste_cancel_enabled);
        assert!(
            app.settings_editor
                .to_settings(&settings)
                .paste_sequence
                .cancel_hotkey
                .is_none()
        );
        paste_settings_click(&ctx, &mut app, "Enable Cancel shortcut");
        assert_eq!(
            app.settings_editor
                .to_settings(&settings)
                .paste_sequence
                .cancel_hotkey
                .as_deref(),
            Some("Ctrl+F10")
        );
        paste_settings_click(&ctx, &mut app, "Escape cancels active playback");
        assert!(!app.settings_editor.paste_sequence.escape_cancel_enabled);
        paste_settings_click(
            &ctx,
            &mut app,
            "Keep Paste Sequences dialog visible during playback",
        );
        assert!(
            app.settings_editor
                .paste_sequence
                .keep_dialog_visible_during_playback
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(
            app.settings_editor.baseline_settings.paste_sequence,
            settings.paste_sequence
        );
        paste_settings_click(&ctx, &mut app, "Cancel");
        assert!(!app.show_settings);
        assert!(!app.settings_save_pending());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        app.open_paste_sequence_settings_dialog();
        assert_eq!(app.settings_editor.paste_sequence, settings.paste_sequence);
    }

    #[test]
    fn jump_mode_focuses_real_settings_controls_and_later_egui_keys_activate_them() {
        use crate::gui::jump_mode::ui::{JumpModeUi, NavigationOwner};

        let directory = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        ctx.global_style_mut(|style| style.animation_time = 0.0);
        let mut settings = crate::settings::Settings::default();
        settings.experimental_jump_mode_enabled = true;
        let mut app = settings_test_app(&ctx, &directory, settings);
        app.show_settings = true;
        for _ in 0..3 {
            paste_settings_frame(&ctx, &mut app, Vec::new());
        }
        paste_settings_click(&ctx, &mut app, "Expand all");

        let mut jump = JumpModeUi::new(true);
        let settings_owner = NavigationOwner::Settings { instance: 1 };
        let setup_output = ctx.run_ui(
            egui::RawInput {
                focused: true,
                ..Default::default()
            },
            |ui| {
                jump.begin_frame();
                assert!(jump.observe_settings_visibility(true));
                assert_eq!(
                    jump.update_navigation_owner(&ctx, settings_owner, true),
                    NavigationOwner::Blocked,
                    "the newly opened Settings owner waits for retained window ordering"
                );
                ui.label("Settings owner setup");
            },
        );
        setup_output.drop_without_applying_deltas();

        let mut targets = settings_jump_frame(&ctx, &mut app, &mut jump, Vec::new(), None, true);
        let hotkeys_header = target_by_label(&targets, "Hotkeys");
        let hotkeys_title_rect = hotkeys_header
            .paint_label_rect()
            .expect("the collapsible section registers its actual painted heading rect");
        assert!(
            hotkeys_title_rect.left() >= hotkeys_header.rect().right(),
            "the Settings heading is painted to the right of its native collapse response"
        );
        app.settings_save_reconciliation_required = true;
        let disabled_targets =
            settings_jump_frame(&ctx, &mut app, &mut jump, Vec::new(), None, true);
        assert!(
            disabled_targets
                .iter()
                .all(|target| !matches!(target.metadata().label(), "Save" | "Cancel")),
            "disabled footer controls are excluded from the registry"
        );
        app.settings_save_reconciliation_required = false;
        assert!(app.jump_mode_enabled());
        let collapse = target_by_label(&targets, "Collapse all").clone();
        let focused = settings_jump_frame(
            &ctx,
            &mut app,
            &mut jump,
            Vec::new(),
            Some(collapse.id().clone()),
            true,
        );
        assert!(
            target_by_label(&focused, "Launcher hotkey").eligible(),
            "focus handoff never activates Expand/Collapse"
        );
        assert_eq!(
            ctx.memory(|memory| memory.focused()),
            Some(collapse.response_id()),
            "the one-shot intent focuses the real Settings button response"
        );

        let enter = egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let collapsed =
            settings_jump_frame(&ctx, &mut app, &mut jump, vec![enter.clone()], None, true);
        assert!(
            collapsed
                .iter()
                .all(|target| target.metadata().label() != "Launcher hotkey"),
            "a later native Enter activates Collapse all"
        );
        let expand = target_by_label(&collapsed, "Expand all").clone();
        settings_jump_frame(
            &ctx,
            &mut app,
            &mut jump,
            Vec::new(),
            Some(expand.id().clone()),
            true,
        );
        targets = settings_jump_frame(&ctx, &mut app, &mut jump, vec![enter.clone()], None, true);
        assert!(target_by_label(&targets, "Launcher hotkey").eligible());

        let checkbox = target_by_label(&targets, "Enable quit hotkey").clone();
        let quit_hotkey_before = app.settings_editor.quit_hotkey_enabled;
        settings_jump_frame(
            &ctx,
            &mut app,
            &mut jump,
            Vec::new(),
            Some(checkbox.id().clone()),
            true,
        );
        assert_eq!(app.settings_editor.quit_hotkey_enabled, quit_hotkey_before);
        assert_eq!(
            ctx.memory(|memory| memory.focused()),
            Some(checkbox.response_id()),
            "checkbox intent uses its actual egui response"
        );
        let space = egui::Event::Key {
            key: egui::Key::Space,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let (_, checkbox_values, checkbox_passes) =
            settings_jump_discard_frame(&ctx, &mut app, &mut jump, vec![space]);
        assert_eq!(checkbox_passes, 2, "egui retains its second pass");
        assert_eq!(checkbox_values.len(), checkbox_passes);
        assert_ne!(
            checkbox_values[0].quit_hotkey_enabled, quit_hotkey_before,
            "the first native pass applies the Space activation"
        );
        assert_eq!(
            checkbox_values[1].quit_hotkey_enabled, checkbox_values[0].quit_hotkey_enabled,
            "the actual discarded pass retains the first pass's checkbox state"
        );

        let pointer_checkbox = target_by_label(&targets, "Enable quit hotkey").clone();
        let pointer_click =
            std::iter::once(egui::Event::PointerMoved(pointer_checkbox.rect().center()))
                .chain(
                    [true, false]
                        .into_iter()
                        .map(|pressed| egui::Event::PointerButton {
                            pos: pointer_checkbox.rect().center(),
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        }),
                )
                .collect();
        let before_pointer = app.settings_editor.quit_hotkey_enabled;
        let (_, pointer_values, pointer_passes) =
            settings_jump_discard_frame(&ctx, &mut app, &mut jump, pointer_click);
        assert_eq!(
            pointer_passes, 2,
            "pointer activation also requests a retry"
        );
        assert_eq!(pointer_values[0].quit_hotkey_enabled, !before_pointer);
        assert_eq!(
            pointer_values[1].quit_hotkey_enabled,
            pointer_values[0].quit_hotkey_enabled
        );

        let paste_header = target_by_label(&targets, "Paste Sequences").clone();
        settings_jump_frame(
            &ctx,
            &mut app,
            &mut jump,
            Vec::new(),
            Some(paste_header.id().clone()),
            true,
        );
        let (collapsed_targets, header_values, header_passes) =
            settings_jump_discard_frame(&ctx, &mut app, &mut jump, vec![enter.clone()]);
        assert_eq!(header_passes, 2, "section toggles survive the retry");
        assert!(target_by_label(&targets, "Enable Next shortcut").eligible());
        assert!(
            collapsed_targets
                .iter()
                .all(|target| target.metadata().label() != "Enable Next shortcut")
        );
        assert!(header_values.iter().all(|pass| !pass.paste_body_visible));

        let launcher_hotkey = target_by_label(&targets, "Launcher hotkey").clone();
        settings_jump_frame(
            &ctx,
            &mut app,
            &mut jump,
            Vec::new(),
            Some(launcher_hotkey.id().clone()),
            true,
        );
        let original_hotkey = app.settings_editor.hotkey.clone();
        let (_, hotkey_values, hotkey_passes) = settings_jump_discard_frame(
            &ctx,
            &mut app,
            &mut jump,
            vec![egui::Event::Text("q".into())],
        );
        assert_eq!(hotkey_passes, 2, "text entry is checked in a retained pass");
        assert_eq!(hotkey_values[0].hotkey, format!("{original_hotkey}q"));
        assert_eq!(hotkey_values[1].hotkey, hotkey_values[0].hotkey);
        assert_ne!(app.settings_editor.hotkey, original_hotkey);
        assert!(app.settings_editor.hotkey.contains('q'));
        assert_eq!(
            hotkey_values[1].hotkey_cursor,
            Some(original_hotkey.chars().count() + 1),
            "the retained TextEdit pass keeps its caret after the inserted character"
        );

        targets = settings_scroll_until_target(
            &ctx,
            &mut app,
            &mut jump,
            targets,
            "Experimental Jump Mode",
        );
        let jump_mode_toggle = target_by_label(&targets, "Experimental Jump Mode").clone();
        settings_jump_frame(
            &ctx,
            &mut app,
            &mut jump,
            Vec::new(),
            Some(jump_mode_toggle.id().clone()),
            true,
        );
        settings_jump_frame(
            &ctx,
            &mut app,
            &mut jump,
            vec![egui::Event::Key {
                key: egui::Key::Space,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            None,
            true,
        );
        assert!(!app.settings_editor.experimental_jump_mode_enabled);
        assert!(app.jump_mode_enabled(), "draft edits do not change runtime");

        let cancel = target_by_label(&targets, "Cancel").clone();
        settings_jump_frame(
            &ctx,
            &mut app,
            &mut jump,
            Vec::new(),
            Some(cancel.id().clone()),
            true,
        );
        settings_jump_frame(&ctx, &mut app, &mut jump, vec![enter], None, true);
        assert!(!app.show_settings);
        assert!(
            app.jump_mode_enabled(),
            "Cancel keeps the committed runtime flag"
        );
        assert!(app.settings_editor.experimental_jump_mode_enabled);
    }

    #[test]
    fn jump_mode_settings_slider_and_value_editor_are_distinct_native_targets() {
        use crate::gui::jump_mode::ui::{JumpModeUi, NavigationOwner};

        let directory = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        ctx.global_style_mut(|style| style.animation_time = 0.0);
        let mut settings = crate::settings::Settings::default();
        settings.experimental_jump_mode_enabled = true;
        let mut app = settings_test_app(&ctx, &directory, settings);
        app.show_settings = true;
        for _ in 0..3 {
            paste_settings_frame(&ctx, &mut app, Vec::new());
        }
        paste_settings_click(&ctx, &mut app, "Expand all");
        paste_settings_click(&ctx, &mut app, "Collapse all");
        paste_settings_click(&ctx, &mut app, "Search & Results");

        let mut jump = JumpModeUi::new(true);
        let settings_owner = NavigationOwner::Settings { instance: 1 };
        let setup_output = ctx.run_ui(
            egui::RawInput {
                focused: true,
                ..Default::default()
            },
            |ui| {
                jump.begin_frame();
                assert!(jump.observe_settings_visibility(true));
                assert_eq!(
                    jump.update_navigation_owner(&ctx, settings_owner, true),
                    NavigationOwner::Blocked,
                    "the newly opened Settings owner waits for retained window ordering"
                );
                ui.label("Settings slider owner setup");
            },
        );
        setup_output.drop_without_applying_deltas();

        let targets = settings_jump_frame(&ctx, &mut app, &mut jump, Vec::new(), None, true);
        let slider = target_by_label(&targets, "Fuzzy weight").clone();
        assert!(
            slider.clipped_rect().contains_rect(slider.rect()),
            "the actual Settings slider is fully visible before focus handoff"
        );
        let numeric = target_by_label(&targets, "Fuzzy weight value").clone();
        assert!(slider.eligible());
        assert!(numeric.eligible());
        assert_eq!(
            slider.metadata().kind(),
            crate::gui::jump_mode::TargetKind::Slider
        );
        assert_eq!(
            numeric.metadata().kind(),
            crate::gui::jump_mode::TargetKind::TextEntry
        );
        assert_ne!(
            slider.id(),
            numeric.id(),
            "semantic IDs distinguish the slider thumb from its numeric editor"
        );
        assert_ne!(
            slider.response_id(),
            numeric.response_id(),
            "the actual egui slider and DragValue responses have distinct native IDs"
        );

        let original = app.settings_editor.fuzzy_weight;
        let focused = settings_jump_frame(
            &ctx,
            &mut app,
            &mut jump,
            Vec::new(),
            Some(slider.id().clone()),
            true,
        );
        assert_eq!(
            target_by_label(&focused, "Fuzzy weight").response_id(),
            slider.response_id()
        );
        assert_eq!(
            target_by_label(&focused, "Fuzzy weight").rect(),
            slider.rect(),
            "the Settings slider geometry remains stationary through focus handoff"
        );
        assert_eq!(
            ctx.memory(|memory| memory.focused()),
            Some(slider.response_id())
        );
        assert_eq!(
            app.settings_editor.fuzzy_weight, original,
            "focusing either control does not change the setting"
        );
        settings_jump_frame(
            &ctx,
            &mut app,
            &mut jump,
            vec![egui::Event::Key {
                key: egui::Key::ArrowRight,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            None,
            true,
        );
        assert!(app.settings_editor.fuzzy_weight > original);
        assert!(app.settings_editor.fuzzy_weight <= 5.0);
        assert_eq!(
            ctx.memory(|memory| memory.focused()),
            Some(slider.response_id()),
            "the first native slider arrow keeps focus on the actual thumb response"
        );
        let after_first_arrow = app.settings_editor.fuzzy_weight;
        settings_jump_frame(
            &ctx,
            &mut app,
            &mut jump,
            vec![egui::Event::Key {
                key: egui::Key::ArrowRight,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            None,
            true,
        );
        assert!(app.settings_editor.fuzzy_weight > after_first_arrow);
        assert_eq!(
            ctx.memory(|memory| memory.focused()),
            Some(slider.response_id()),
            "subsequent native slider arrows remain focused without another hint"
        );

        let before_numeric = app.settings_editor.fuzzy_weight;
        settings_jump_frame(
            &ctx,
            &mut app,
            &mut jump,
            Vec::new(),
            Some(numeric.id().clone()),
            true,
        );
        assert_eq!(
            ctx.memory(|memory| memory.focused()),
            Some(numeric.response_id())
        );
        settings_jump_frame(
            &ctx,
            &mut app,
            &mut jump,
            vec![egui::Event::Key {
                key: egui::Key::ArrowUp,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            None,
            true,
        );
        assert!(app.settings_editor.fuzzy_weight > before_numeric);
        assert!(app.settings_editor.fuzzy_weight <= 5.0);

        app.settings_editor.fuzzy_weight = 5.0;
        settings_jump_frame(
            &ctx,
            &mut app,
            &mut jump,
            vec![egui::Event::Key {
                key: egui::Key::ArrowUp,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            None,
            true,
        );
        assert_eq!(
            app.settings_editor.fuzzy_weight, 5.0,
            "the actual Settings DragValue does not step above its configured maximum"
        );
    }

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
                let mut targets = SettingsJumpTargets::new(None, true, false);
                for section in TopLevelSettingsSection::ALL {
                    let id = section.egui_id(ui);
                    show_settings_section(ui, section, None, &mut targets, |ui, _| {
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
            vec![true, false, true, true, false, false, false, false],
            "the established defaults remain intact beside the new collapsed Paste section"
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
