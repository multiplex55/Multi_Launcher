use crate::hotkey::parse_hotkey;
use crate::plugin::PluginManager;
use crate::settings::Settings;
use eframe::egui;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SettingsNavigationStage {
    MainControl,
    DeferredControl,
    DedicatedOwner,
}

/// A value-only one-shot handoff from the Settings search result to its owner.
/// It deliberately contains no widget response, geometry, or setting value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PendingSettingsNavigation {
    pub(crate) request_token: u64,
    pub(crate) control_id: String,
    pub(crate) owner: super::catalog::SettingsOwner,
    pub(crate) section: super::catalog::SettingsSection,
    pub(crate) catalog_revision: u64,
    pub(crate) settings_instance: Option<u64>,
    pub(crate) stage: SettingsNavigationStage,
    pub(crate) frames_remaining: u8,
    pub(crate) scroll_requested: bool,
    /// The dedicated window was opened by the navigation request.
    pub(crate) destination_observed: bool,
    /// Its exact native owner passed the current-owner guard at least once.
    /// A window can be open during egui's non-interactive sizing passes before
    /// it becomes the active native owner, so this is distinct from
    /// `destination_observed`.
    pub(crate) destination_owner_eligible: bool,
    /// The selected row can be unavailable while an exact built-in owner
    /// prerequisite is still safe to reveal. The original query target stays
    /// explicit so the prerequisite is never presented as that target.
    pub(crate) delivery: SettingsNavigationDelivery,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SettingsNavigationDelivery {
    ExactControl,
    BuiltinPrerequisite { requested_control_id: String },
    ConditionalPrerequisite { requested_control_id: String },
}

/// Short-lived, value-only provenance for drawing an outline around the real
/// control after its one-shot focus handoff has completed. It retains no
/// egui Response or geometry.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SettingsNavigationHighlight {
    pub(crate) request_token: u64,
    pub(crate) control_id: String,
    pub(crate) owner: super::catalog::SettingsOwner,
    pub(crate) section: super::catalog::SettingsSection,
    pub(crate) catalog_revision: u64,
    pub(crate) settings_instance: Option<u64>,
    pub(crate) delivery: SettingsNavigationDelivery,
    pub(crate) expires_at: f64,
}

/// QA-only evidence that the current Settings owner painted the accepted
/// navigation target in one specific egui pass. The retained navigation
/// highlight above intentionally remains geometry-free.
#[cfg(feature = "qa-framebuffer-capture")]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SettingsNavigationQaPaint {
    pub(crate) request_token: u64,
    pub(crate) control_id: String,
    pub(crate) owner: super::catalog::SettingsOwner,
    pub(crate) owner_instance: Option<u64>,
    pub(crate) catalog_revision: u64,
    pub(crate) response_id: egui::Id,
    pub(crate) rect: egui::Rect,
    pub(crate) egui_frame_nr: u64,
    pub(crate) egui_pass_index: usize,
}

impl SettingsNavigationHighlight {
    pub(crate) fn from_request(request: &PendingSettingsNavigation, now: f64) -> Self {
        Self {
            request_token: request.request_token,
            control_id: request.control_id.clone(),
            owner: request.owner.clone(),
            section: request.section.clone(),
            catalog_revision: request.catalog_revision,
            settings_instance: request.settings_instance,
            delivery: request.delivery.clone(),
            expires_at: now + 1.8,
        }
    }

    pub(crate) fn as_request(&self) -> PendingSettingsNavigation {
        PendingSettingsNavigation {
            request_token: self.request_token,
            control_id: self.control_id.clone(),
            owner: self.owner.clone(),
            section: self.section.clone(),
            catalog_revision: self.catalog_revision,
            settings_instance: self.settings_instance,
            stage: match &self.section {
                super::catalog::SettingsSection::Main(_)
                | super::catalog::SettingsSection::Notes
                | super::catalog::SettingsSection::Plugin { .. } => {
                    SettingsNavigationStage::DeferredControl
                }
                _ => SettingsNavigationStage::DedicatedOwner,
            },
            frames_remaining: 0,
            scroll_requested: false,
            destination_observed: true,
            destination_owner_eligible: true,
            delivery: self.delivery.clone(),
        }
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct TestNotesControlResponse {
    pub(crate) id: egui::Id,
    pub(crate) rect: egui::Rect,
    pub(crate) clip_rect: egui::Rect,
    pub(crate) visible: bool,
    pub(crate) ui_enabled: bool,
    pub(crate) response_enabled: bool,
    pub(crate) interaction_allowed: bool,
    pub(crate) has_focus: bool,
}

#[derive(Default)]
pub struct SettingsEditor {
    pub(crate) search_catalog: super::catalog::SettingsCatalog,
    pub(super) catalog_plugin_revision: Option<u64>,
    pub(crate) settings_search_query: String,
    pub(crate) settings_search_match_query: String,
    pub(crate) settings_search_match_revision: Option<u64>,
    pub(crate) settings_search_match_ids: Vec<String>,
    pub(crate) settings_search_selected_id: Option<String>,
    pub(crate) settings_search_page: usize,
    pub(crate) settings_search_response_owner_instance: Option<u64>,
    pub(crate) settings_search_query_response_id: Option<egui::Id>,
    pub(crate) settings_search_result_response_ids: Vec<egui::Id>,
    pub(crate) pending_settings_navigation: Option<PendingSettingsNavigation>,
    pub(crate) settings_navigation_highlight: Option<SettingsNavigationHighlight>,
    #[cfg(feature = "qa-framebuffer-capture")]
    pub(crate) settings_navigation_qa_paint: Option<SettingsNavigationQaPaint>,
    pub(crate) next_settings_navigation_token: u64,
    /// Keeps a locally handled search Enter from reaching controls on a
    /// retained egui pass after the search result has requested focus.
    pub(crate) search_enter_consumption_pending: bool,
    pub(crate) baseline_settings: Settings,
    pub(crate) paste_sequence: crate::settings::PasteSequencePreferences,
    pub(crate) paste_next_enabled: bool,
    pub(crate) paste_cancel_enabled: bool,
    pub(crate) hotkey: String,
    pub(crate) hotkey_valid: bool,
    pub(crate) last_valid_hotkey: String,
    pub(crate) quit_hotkey_enabled: bool,
    pub(crate) quit_hotkey: String,
    pub(crate) quit_hotkey_valid: bool,
    pub(crate) last_valid_quit_hotkey: String,
    pub(crate) help_hotkey_enabled: bool,
    pub(crate) help_hotkey: String,
    pub(crate) help_hotkey_valid: bool,
    pub(crate) last_valid_help_hotkey: String,
    pub(crate) radial_enabled: bool,
    pub(crate) radial_shared_tap_hold: bool,
    pub(crate) radial_hold_threshold_ms: u64,
    pub(crate) radial_tooltip_scope: crate::radial::model::TooltipScope,
    pub(crate) radial_tooltip_delay_ms: u64,
    pub(crate) radial_show_expected_layout_diagnostics: bool,
    pub(crate) radial_default_menu_id: String,
    pub(crate) radial_default_interaction: crate::radial::model::InteractionMode,
    pub(crate) radial_default_submenu_presentation: crate::radial::model::SubmenuPresentation,
    pub(crate) radial_safety_policy: crate::radial::model::RadialSafetyPolicy,
    pub(crate) radial_default_item_input_scope: crate::radial::model::TriggerScope,
    pub(crate) radial_global_item_inputs: bool,
    pub(crate) radial_designer: crate::settings::RadialDesignerPreferences,
    pub(crate) debug_logging: bool,
    pub(crate) show_toasts: bool,
    pub(crate) show_inline_errors: bool,
    pub(crate) show_error_toasts: bool,
    pub(crate) toast_duration: f32,
    pub(crate) offscreen_x: i32,
    pub(crate) offscreen_y: i32,
    pub(crate) window_w: i32,
    pub(crate) window_h: i32,
    pub(crate) note_panel_w: f32,
    pub(crate) note_panel_h: f32,
    pub(crate) note_save_on_close: bool,
    pub(crate) note_confirm_discard_unsaved_changes: bool,
    pub(crate) note_always_overwrite: bool,
    pub(crate) note_images_as_links: bool,
    pub(crate) note_show_details: bool,
    pub(crate) note_more_limit: usize,
    pub(crate) note_rich_markdown_enabled: bool,
    pub(crate) note_task_lists_enabled: bool,
    pub(crate) note_interactive_checkboxes_enabled: bool,
    pub(crate) note_collapsible_sections_enabled: bool,
    pub(crate) note_outline_sidebar_enabled: bool,
    pub(crate) note_outline_sidebar_default_open: bool,
    pub(crate) note_split_view_enabled: bool,
    pub(crate) note_default_view_mode: crate::settings::NoteViewMode,
    pub(crate) note_callouts_enabled: bool,
    pub(crate) note_backlinks_enabled: bool,
    pub(crate) note_aliases_enabled: bool,
    pub(crate) note_templates_enabled: bool,
    pub(crate) note_max_outline_depth: usize,
    pub(crate) note_collapsed_sections_persist: bool,
    pub(crate) query_scale: f32,
    pub(crate) list_scale: f32,
    pub(crate) history_limit: usize,
    pub(crate) clipboard_limit: usize,
    pub(crate) fuzzy_weight: f32,
    pub(crate) usage_weight: f32,
    pub(crate) match_exact: bool,
    pub(crate) page_jump: usize,
    pub(crate) query_results_layout_enabled: bool,
    pub(crate) query_results_layout_rows: usize,
    pub(crate) query_results_layout_cols: usize,
    pub(crate) query_results_layout_respect_plugin_capability: bool,
    pub(crate) query_results_layout_plugin_opt_out: String,
    pub(crate) follow_mouse: bool,
    pub(crate) static_enabled: bool,
    pub(crate) static_x: i32,
    pub(crate) static_y: i32,
    pub(crate) static_w: i32,
    pub(crate) static_h: i32,
    pub(crate) hide_after_run: bool,
    pub always_on_top: bool,
    pub(crate) show_launcher_icon: bool,
    pub(crate) show_launcher_title: bool,
    pub(crate) experimental_jump_mode_enabled: bool,
    pub(crate) jump_mode_activation_chord: String,
    pub(crate) jump_mode_hint_alphabet: String,
    pub(crate) timer_refresh: f32,
    pub(crate) disable_timer_updates: bool,
    pub(crate) preserve_command: bool,
    pub(crate) clear_query_after_run: bool,
    pub(crate) require_confirm_destructive: bool,
    pub(crate) query_autocomplete: bool,
    pub(crate) net_refresh: f32,
    pub(crate) net_unit: crate::settings::NetUnit,
    pub(crate) screenshot_dir: String,
    pub(crate) screenshot_save_file: bool,
    pub(crate) screenshot_auto_save: bool,
    pub(crate) screenshot_use_editor: bool,
    pub(crate) reduce_dashboard_work_when_unfocused: bool,
    pub(crate) show_dashboard_diagnostics: bool,
    pub(crate) dashboard_enabled: bool,
    pub(crate) dashboard_path: String,
    pub(crate) dashboard_default_location: String,
    pub(crate) dashboard_show_when_empty: bool,
    pub(crate) plugin_settings: std::collections::HashMap<String, serde_json::Value>,
    pub(crate) plugins_expanded: bool,
    pub(crate) expand_request: Option<bool>,
    pub(crate) confirm_clipboard_modify_factory_reset: bool,
    #[cfg(test)]
    pub(crate) test_search_query_response: Option<(egui::Id, egui::Rect)>,
    #[cfg(test)]
    pub(crate) test_search_query_clip_rect: Option<egui::Rect>,
    #[cfg(test)]
    pub(crate) test_settings_window_state: Option<(bool, bool, bool)>,
    #[cfg(test)]
    pub(crate) test_search_result_responses: Vec<(String, egui::Id, egui::Rect)>,
    #[cfg(test)]
    pub(crate) test_search_page_responses: Vec<(String, egui::Id, egui::Rect)>,
    #[cfg(test)]
    pub(crate) test_fuzzy_weight_response_ids: Option<(egui::Id, egui::Id)>,
    #[cfg(test)]
    pub(crate) test_last_navigation_focus: Option<(String, egui::Id)>,
    #[cfg(test)]
    pub(crate) test_last_dedicated_navigation_focus: Option<(String, egui::Id)>,
    #[cfg(test)]
    pub(crate) test_last_navigation_highlight: Option<(String, egui::Id)>,
    #[cfg(test)]
    pub(crate) test_last_navigation_diagnostic: Option<String>,
    #[cfg(test)]
    pub(crate) test_root_escape_preflight: Option<String>,
    #[cfg(test)]
    pub(crate) test_search_navigation_history: Vec<String>,
    #[cfg(test)]
    pub(crate) test_search_navigation_trace_started: bool,
    #[cfg(test)]
    pub(crate) test_notes_external_open_response: Option<TestNotesControlResponse>,
}

impl SettingsEditor {
    pub(crate) const SETTINGS_WINDOW_DEFAULT_WIDTH: f32 = 640.0;
    pub(crate) const SETTINGS_WINDOW_MAX_DEFAULT_HEIGHT: f32 = 720.0;
    pub(crate) const SETTINGS_WINDOW_MIN_HEIGHT: f32 = 360.0;

    /// Set the transient Settings search query from an explicit command.
    /// This intentionally changes no persisted setting or editor draft.
    pub(crate) fn set_search_query_from_command(&mut self, query: &str) {
        self.settings_search_query = if query.trim().is_empty() {
            String::new()
        } else {
            query.to_owned()
        };
        self.settings_search_match_query.clear();
        self.settings_search_match_revision = None;
        self.settings_search_match_ids.clear();
        self.settings_search_selected_id = None;
        self.settings_search_page = 0;
        self.pending_settings_navigation = None;
        self.settings_navigation_highlight = None;
        #[cfg(feature = "qa-framebuffer-capture")]
        {
            self.settings_navigation_qa_paint = None;
        }
        self.search_enter_consumption_pending = false;
        self.settings_search_response_owner_instance = None;
        self.settings_search_query_response_id = None;
        self.settings_search_result_response_ids.clear();
    }

    pub(crate) fn normalized_static_settings(
        follow_mouse: bool,
        static_enabled: bool,
        static_pos: (i32, i32),
        static_size: (i32, i32),
    ) -> (bool, Option<(i32, i32)>, Option<(i32, i32)>) {
        if follow_mouse {
            (false, None, None)
        } else {
            (static_enabled, Some(static_pos), Some(static_size))
        }
    }

    pub fn new(settings: &Settings) -> Self {
        Self::from_settings(settings)
    }

    pub fn new_with_plugins(settings: &Settings) -> Self {
        let mut s = Self::new(settings);
        s.sync_from_plugin_settings();
        s
    }

    pub(crate) fn settings_window_default_height(available_height: f32) -> f32 {
        (available_height * 0.5)
            .max(Self::SETTINGS_WINDOW_MIN_HEIGHT)
            .min(Self::SETTINGS_WINDOW_MAX_DEFAULT_HEIGHT)
    }

    pub(crate) fn settings_window_default_size(ctx: &egui::Context) -> [f32; 2] {
        [
            Self::SETTINGS_WINDOW_DEFAULT_WIDTH,
            Self::settings_window_default_height(ctx.content_rect().height()),
        ]
    }

    pub(crate) fn refresh_search_catalog(&mut self, plugins: &PluginManager) {
        if self.catalog_plugin_revision != Some(plugins.settings_catalog_revision())
            && let Some(snapshot) =
                plugins.settings_catalog_snapshot_if_changed(self.catalog_plugin_revision)
        {
            self.catalog_plugin_revision = Some(snapshot.revision);
            self.search_catalog = super::catalog::SettingsCatalog::from_plugin_snapshot(&snapshot);
        }
    }

    pub fn search_catalog(&self) -> &super::catalog::SettingsCatalog {
        &self.search_catalog
    }

    pub(crate) fn settings_content_height(
        available_height: f32,
        footer_reserved_height: f32,
    ) -> f32 {
        (available_height - footer_reserved_height).max(0.0)
    }

    pub(crate) fn jump_preferences(
        &self,
    ) -> Result<
        (
            crate::settings::JumpModeChord,
            crate::settings::HintAlphabet,
        ),
        String,
    > {
        let chord = crate::settings::JumpModeChord::parse(&self.jump_mode_activation_chord)
            .map_err(|error| format!("Jump activation chord: {error}"))?;
        let alphabet = crate::settings::HintAlphabet::parse(&self.jump_mode_hint_alphabet)
            .map_err(|error| format!("Jump hint alphabet: {error}"))?;
        if self.experimental_jump_mode_enabled {
            let bindings = [
                ("launcher hotkey", Some(self.hotkey.as_str())),
                (
                    "quit hotkey",
                    self.quit_hotkey_enabled
                        .then_some(self.quit_hotkey.as_str()),
                ),
                (
                    "help hotkey",
                    self.help_hotkey_enabled
                        .then_some(self.help_hotkey.as_str()),
                ),
            ];
            if let Some(message) = crate::hotkey::validation::jump_chord_conflict(
                chord,
                bindings.into_iter().filter_map(|(name, text)| {
                    text.and_then(parse_hotkey).map(|hotkey| (name, hotkey))
                }),
            ) {
                return Err(message);
            }
            if !self
                .baseline_settings
                .enabled_plugins
                .as_ref()
                .is_some_and(|plugins| !plugins.contains("paste_sequence"))
            {
                let preferences = self.paste_sequence_preferences();
                let defaults = crate::settings::PasteSequencePreferences::default();
                // The final resolver may omit optional defaults on conflict;
                // custom controls must be refused. Saved Starts are resolved
                // there from the current store, not from this editor draft.
                let controls = [
                    (
                        "Paste Sequence Next",
                        preferences.next_hotkey.as_deref(),
                        defaults.next_hotkey.as_deref(),
                    ),
                    (
                        "Paste Sequence Cancel",
                        preferences.cancel_hotkey.as_deref(),
                        defaults.cancel_hotkey.as_deref(),
                    ),
                ];
                if let Some(message) = crate::hotkey::validation::jump_chord_conflict(
                    chord,
                    controls
                        .into_iter()
                        .filter_map(|(name, configured, default)| {
                            let hotkey = configured.and_then(parse_hotkey)?;
                            (Some(hotkey) != default.and_then(parse_hotkey))
                                .then_some((name, hotkey))
                        }),
                ) {
                    return Err(message);
                }
            }
        }
        Ok((chord, alphabet))
    }

    pub(crate) fn reset_jump_preferences(&mut self) {
        self.experimental_jump_mode_enabled = false;
        self.jump_mode_activation_chord =
            crate::settings::JumpModeChord::default().canonical_text();
        self.jump_mode_hint_alphabet = crate::settings::HintAlphabet::default().canonical_text();
    }

    pub(crate) fn jump_hint_sample(&self) -> Result<Vec<String>, String> {
        use crate::gui::jump_mode::{
            HintMap, TargetGeneration, TargetId, TargetKind, TargetMetadata,
        };
        let alphabet = crate::settings::HintAlphabet::parse(&self.jump_mode_hint_alphabet)
            .map_err(|error| format!("Jump hint alphabet: {error}"))?;
        let targets = (0..5).map(|index| {
            TargetMetadata::new(
                TargetId::root_result(TargetGeneration(0), index),
                format!("Sample {}", index + 1),
                TargetKind::Button,
                index as u64,
            )
        });
        HintMap::assign_with_alphabet(targets, &alphabet)
            .map(|map| {
                map.assignments()
                    .iter()
                    .map(|assignment| assignment.hint().to_string())
                    .collect()
            })
            .map_err(|_| "Jump hint sample could not be assigned".to_owned())
    }

    pub(crate) fn validate_before_save(&mut self) -> Result<(), &'static str> {
        self.paste_sequence_preferences().validate()?;
        if parse_hotkey(&self.hotkey).is_none() {
            self.hotkey = self.last_valid_hotkey.clone();
            self.hotkey_valid = true;
            return Err("Failed to save settings: hotkey is invalid");
        }
        if self.quit_hotkey_enabled && parse_hotkey(&self.quit_hotkey).is_none() {
            self.quit_hotkey = self.last_valid_quit_hotkey.clone();
            self.quit_hotkey_valid = true;
            return Err("Failed to save settings: quit hotkey is invalid");
        }
        if self.help_hotkey_enabled && parse_hotkey(&self.help_hotkey).is_none() {
            self.help_hotkey = self.last_valid_help_hotkey.clone();
            self.help_hotkey_valid = true;
            return Err("Failed to save settings: help hotkey is invalid");
        }
        if self.radial_enabled
            && !(crate::radial::settings::MIN_HOLD_THRESHOLD_MS
                ..=crate::radial::settings::MAX_HOLD_THRESHOLD_MS)
                .contains(&self.radial_hold_threshold_ms)
        {
            return Err("Failed to save settings: radial hold threshold is outside 100-2000 ms");
        }
        if self.radial_enabled
            && !(crate::radial::settings::MIN_TOOLTIP_DELAY_MS
                ..=crate::radial::settings::MAX_TOOLTIP_DELAY_MS)
                .contains(&self.radial_tooltip_delay_ms)
        {
            return Err("Failed to save settings: radial tooltip delay is outside 0-5000 ms");
        }
        if self.radial_enabled
            && self.radial_default_item_input_scope == crate::radial::model::TriggerScope::Global
            && !self.radial_global_item_inputs
        {
            return Err(
                "Failed to save settings: global radial item scope requires global item inputs",
            );
        }

        self.last_valid_hotkey = self.hotkey.clone();
        if self.quit_hotkey_enabled {
            self.last_valid_quit_hotkey = self.quit_hotkey.clone();
        }
        if self.help_hotkey_enabled {
            self.last_valid_help_hotkey = self.help_hotkey.clone();
        }
        Ok(())
    }

    pub(crate) fn paste_sequence_preferences(&self) -> crate::settings::PasteSequencePreferences {
        let mut preferences = self.paste_sequence.clone();
        if !self.paste_next_enabled {
            preferences.next_hotkey = None;
        }
        if !self.paste_cancel_enabled {
            preferences.cancel_hotkey = None;
        }
        preferences
    }

    pub(crate) fn sync_paste_sequence_preferences(
        &mut self,
        preferences: &crate::settings::PasteSequencePreferences,
    ) {
        self.paste_sequence = preferences.clone();
        self.paste_next_enabled = preferences.next_hotkey.is_some();
        self.paste_cancel_enabled = preferences.cancel_hotkey.is_some();
    }

    pub(crate) fn retire_settings_navigation_for_section(
        &mut self,
        section: &super::catalog::SettingsSection,
    ) {
        if self
            .pending_settings_navigation
            .as_ref()
            .is_some_and(|request| &request.section == section)
        {
            self.pending_settings_navigation = None;
        }
        if self
            .settings_navigation_highlight
            .as_ref()
            .is_some_and(|highlight| &highlight.section == section)
        {
            self.settings_navigation_highlight = None;
            #[cfg(feature = "qa-framebuffer-capture")]
            {
                self.settings_navigation_qa_paint = None;
            }
        }
    }

    pub(crate) fn retire_plugin_settings_navigation(&mut self) {
        let is_plugin_editor = |section: &super::catalog::SettingsSection| {
            matches!(
                section,
                super::catalog::SettingsSection::PluginEditor
                    | super::catalog::SettingsSection::Plugin { .. }
            )
        };
        if self
            .pending_settings_navigation
            .as_ref()
            .is_some_and(|request| is_plugin_editor(&request.section))
        {
            self.pending_settings_navigation = None;
        }
        if self
            .settings_navigation_highlight
            .as_ref()
            .is_some_and(|highlight| is_plugin_editor(&highlight.section))
        {
            self.settings_navigation_highlight = None;
            #[cfg(feature = "qa-framebuffer-capture")]
            {
                self.settings_navigation_qa_paint = None;
            }
        }
    }

    pub(crate) fn retire_all_settings_navigation(&mut self) {
        self.pending_settings_navigation = None;
        self.settings_navigation_highlight = None;
        #[cfg(feature = "qa-framebuffer-capture")]
        {
            self.settings_navigation_qa_paint = None;
        }
    }

    pub(crate) fn expire_settings_navigation_highlight(&mut self, now: f64) {
        if self
            .settings_navigation_highlight
            .as_ref()
            .is_some_and(|highlight| now >= highlight.expires_at)
        {
            self.settings_navigation_highlight = None;
            #[cfg(feature = "qa-framebuffer-capture")]
            {
                self.settings_navigation_qa_paint = None;
            }
        }
    }

    pub(crate) fn radial_settings(&self) -> crate::radial::model::RadialFeatureSettings {
        crate::radial::model::RadialFeatureSettings {
            enabled: self.radial_enabled,
            shared_tap_hold: self.radial_shared_tap_hold,
            hold_threshold_ms: self.radial_hold_threshold_ms,
            default_menu_id: (!self.radial_default_menu_id.trim().is_empty())
                .then(|| crate::radial::model::MenuId::new(self.radial_default_menu_id.trim())),
            default_interaction: self.radial_default_interaction,
            default_submenu_presentation: self.radial_default_submenu_presentation,
            safety_policy: self.radial_safety_policy,
            default_item_input_scope: self.radial_default_item_input_scope,
            global_item_inputs: self.radial_global_item_inputs,
            tooltip_scope: self.radial_tooltip_scope,
            tooltip_delay_ms: self.radial_tooltip_delay_ms,
            show_expected_layout_diagnostics: self.radial_show_expected_layout_diagnostics,
        }
    }

    pub(crate) fn radial_reserved_hotkeys(&self) -> Vec<(String, String)> {
        let mut reserved = vec![("launcher toggle".into(), self.hotkey.clone())];
        if self.quit_hotkey_enabled {
            reserved.push(("quit launcher".into(), self.quit_hotkey.clone()));
        }
        if self.help_hotkey_enabled {
            reserved.push(("help launcher".into(), self.help_hotkey.clone()));
        }
        let screen_draw = self
            .plugin_settings
            .get("screen_draw")
            .cloned()
            .map(serde_json::from_value::<crate::screen_draw::ScreenDrawSettings>)
            .transpose()
            .ok()
            .flatten()
            .unwrap_or_default();
        if let Some(chord) = screen_draw.launch_hotkey {
            reserved.push(("Screen Draw launch".into(), chord.as_str().into()));
        }
        reserved.push((
            "Screen Draw emergency".into(),
            screen_draw.emergency_hotkey.as_str().into(),
        ));
        reserved
    }

    pub(crate) fn radial_validation_issues(
        &self,
        document: &crate::radial::model::RadialDocument,
    ) -> Vec<crate::radial::settings::RadialSettingsIssue> {
        crate::radial::settings::validate(
            &self.radial_settings(),
            document,
            &self.radial_reserved_hotkeys(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radial_threshold_and_scope_validation_explain_the_invalid_field() {
        let mut editor = SettingsEditor::new(&Settings::default());
        editor.radial_hold_threshold_ms = crate::radial::settings::MIN_HOLD_THRESHOLD_MS - 1;
        assert!(
            editor
                .validate_before_save()
                .unwrap_err()
                .contains("100-2000")
        );

        editor.radial_hold_threshold_ms = 350;
        editor.radial_default_item_input_scope = crate::radial::model::TriggerScope::Global;
        editor.radial_global_item_inputs = false;
        assert!(
            editor
                .validate_before_save()
                .unwrap_err()
                .contains("requires global item inputs")
        );
    }

    #[test]
    fn disabling_radial_allows_legacy_values_to_be_saved_for_later_repair() {
        let mut editor = SettingsEditor::new(&Settings::default());
        editor.radial_enabled = false;
        editor.radial_hold_threshold_ms = 0;
        editor.radial_default_item_input_scope = crate::radial::model::TriggerScope::Global;
        editor.radial_global_item_inputs = false;
        assert!(editor.validate_before_save().is_ok());
    }
}
