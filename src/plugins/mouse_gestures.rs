use crate::actions::Action;
use crate::common::json_watch::watch_json;
use crate::common::strip_prefix_ci;
use crate::mouse_gestures::db::{
    format_gesture_label, format_search_result_label, load_gestures, BindingMatchContext,
    SharedGestureDb, GESTURES_FILE,
};
use crate::mouse_gestures::service::{
    with_service as with_gesture_service, CancelBehavior, MouseGestureConfig, NoMatchBehavior,
    WheelCycleGate,
};
use crate::plugin::Plugin;
use eframe::egui;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

const PLUGIN_NAME: &str = "mouse_gestures";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MouseGestureSettings {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_debug_logging")]
    pub debug_logging: bool,
    #[serde(default = "default_show_trail")]
    pub show_trail: bool,
    #[serde(default = "default_trail_color")]
    pub trail_color: [u8; 4],
    #[serde(default = "default_trail_width")]
    pub trail_width: f32,
    #[serde(default = "default_trail_start_move_px")]
    pub trail_start_move_px: f32,
    #[serde(default = "default_show_hint")]
    pub show_hint: bool,
    #[serde(default = "default_hint_offset")]
    pub hint_offset: (f32, f32),
    #[serde(default = "default_cancel_behavior")]
    pub cancel_behavior: CancelBehavior,
    #[serde(default = "default_no_match_behavior")]
    pub no_match_behavior: NoMatchBehavior,
    #[serde(default = "default_wheel_cycle_gate")]
    pub wheel_cycle_gate: WheelCycleGate,
    #[serde(default = "default_practice_mode")]
    pub practice_mode: bool,
    #[serde(default)]
    pub ignore_window_titles: Vec<String>,
}

impl Default for MouseGestureSettings {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            debug_logging: default_debug_logging(),
            show_trail: default_show_trail(),
            trail_color: default_trail_color(),
            trail_width: default_trail_width(),
            trail_start_move_px: default_trail_start_move_px(),
            show_hint: default_show_hint(),
            hint_offset: default_hint_offset(),
            cancel_behavior: default_cancel_behavior(),
            no_match_behavior: default_no_match_behavior(),
            wheel_cycle_gate: default_wheel_cycle_gate(),
            practice_mode: default_practice_mode(),
            ignore_window_titles: Vec::new(),
        }
    }
}

fn default_enabled() -> bool {
    true
}

fn default_debug_logging() -> bool {
    false
}

fn default_show_trail() -> bool {
    true
}

fn default_trail_color() -> [u8; 4] {
    [0xff, 0x00, 0x00, 0xff]
}

fn default_trail_width() -> f32 {
    2.0
}

fn default_trail_start_move_px() -> f32 {
    8.0
}

fn default_show_hint() -> bool {
    true
}

fn default_hint_offset() -> (f32, f32) {
    (16.0, 16.0)
}

fn default_cancel_behavior() -> CancelBehavior {
    CancelBehavior::DoNothing
}

fn default_no_match_behavior() -> NoMatchBehavior {
    NoMatchBehavior::PassThroughClick
}

fn default_wheel_cycle_gate() -> WheelCycleGate {
    WheelCycleGate::Deadzone
}

fn default_practice_mode() -> bool {
    false
}

pub fn normalize_ignore_window_titles(values: &mut Vec<String>) -> bool {
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    for value in values.iter() {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        let key = trimmed.to_lowercase();
        if seen.insert(key) {
            normalized.push(trimmed.to_string());
        }
    }
    let changed = normalized != *values;
    if changed {
        *values = normalized;
    }
    changed
}

pub fn add_ignore_window_title(values: &mut Vec<String>, title: &str) -> bool {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return false;
    }
    if values
        .iter()
        .any(|entry| entry.trim().eq_ignore_ascii_case(trimmed))
    {
        return false;
    }
    values.push(trimmed.to_string());
    normalize_ignore_window_titles(values);
    true
}

pub fn collect_visible_window_titles() -> anyhow::Result<Vec<String>> {
    #[cfg(windows)]
    {
        use crate::windows_layout::{collect_layout_windows, LayoutWindowOptions};
        let windows = collect_layout_windows(LayoutWindowOptions::default())?;
        let mut seen = HashSet::new();
        let mut titles = Vec::new();
        for window in windows {
            let Some(title) = window.matcher.title else {
                continue;
            };
            let trimmed = title.trim();
            if trimmed.is_empty() {
                continue;
            }
            let key = trimmed.to_lowercase();
            if seen.insert(key) {
                titles.push(trimmed.to_string());
            }
        }
        titles.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
        Ok(titles)
    }
    #[cfg(not(windows))]
    {
        Ok(Vec::new())
    }
}

#[derive(Debug)]
struct MouseGestureRuntime {
    settings: MouseGestureSettings,
    plugin_enabled: bool,
    draw_suspend_count: usize,
    db: SharedGestureDb,
    #[allow(dead_code)]
    watcher: Option<crate::common::json_watch::JsonWatcher>,
}

impl Default for MouseGestureRuntime {
    fn default() -> Self {
        let db = Arc::new(Mutex::new(load_gestures(GESTURES_FILE).unwrap_or_default()));
        let db_for_watcher = Arc::clone(&db);

        let watcher = watch_json(GESTURES_FILE, move || {
            if let Ok(new_db) = load_gestures(GESTURES_FILE) {
                if let Ok(mut guard) = db_for_watcher.lock() {
                    *guard = new_db;
                }
            }
        })
        .ok();

        Self {
            settings: MouseGestureSettings::default(),
            plugin_enabled: true,
            draw_suspend_count: 0,
            db,
            watcher,
        }
    }
}

impl MouseGestureRuntime {
    fn update_settings(&mut self, settings: MouseGestureSettings) {
        self.settings = settings;
        self.apply();
    }

    fn set_plugin_enabled(&mut self, enabled: bool) {
        self.plugin_enabled = enabled;
        self.apply();
    }

    fn suspend_for_draw(&mut self) -> SuspendToken {
        self.draw_suspend_count = self.draw_suspend_count.saturating_add(1);
        self.apply();
        SuspendToken { _private: () }
    }

    fn resume_from_draw(&mut self, _token: SuspendToken) {
        if self.draw_suspend_count == 0 {
            return;
        }
        self.draw_suspend_count -= 1;
        self.apply();
    }

    fn effective_enabled(&self) -> bool {
        self.settings.enabled && self.plugin_enabled && self.draw_suspend_count == 0
    }

    fn base_enabled_without_draw_suspend(&self) -> bool {
        self.settings.enabled && self.plugin_enabled
    }

    fn draw_effective_enabled(&self) -> bool {
        self.effective_enabled()
    }

    fn set_draw_mode_active(&mut self, active: bool) {
        self.draw_suspend_count = usize::from(active);
        self.apply();
    }

    fn restore_draw_prior_effective_state(&mut self, prior_effective_state: bool) {
        let base_enabled = self.base_enabled_without_draw_suspend();
        self.draw_suspend_count = if prior_effective_state || !base_enabled {
            0
        } else {
            1
        };
        self.apply();
    }

    fn apply(&self) {
        let mut config = MouseGestureConfig::default();
        config.enabled = self.effective_enabled();
        config.debug_logging = self.settings.debug_logging;
        config.trail_start_move_px = self.settings.trail_start_move_px;
        config.show_trail = self.settings.show_trail;
        config.trail_color = self.settings.trail_color;
        config.trail_width = self.settings.trail_width;
        config.show_hint = self.settings.show_hint;
        config.hint_offset = self.settings.hint_offset;
        config.cancel_behavior = self.settings.cancel_behavior;
        config.no_match_behavior = self.settings.no_match_behavior;
        config.wheel_cycle_gate = self.settings.wheel_cycle_gate;
        config.practice_mode = self.settings.practice_mode;
        config.ignore_window_titles = self.settings.ignore_window_titles.clone();
        with_gesture_service(|svc| {
            svc.update_config(config);
            svc.update_db(Some(self.db.clone()));
        });
    }
}

static SERVICE: OnceCell<Mutex<MouseGestureRuntime>> = OnceCell::new();

fn with_service<F>(f: F)
where
    F: FnOnce(&mut MouseGestureRuntime),
{
    let service = SERVICE.get_or_init(|| Mutex::new(MouseGestureRuntime::default()));
    match service.lock() {
        Ok(mut guard) => f(&mut guard),
        Err(e) => tracing::error!(?e, "failed to lock mouse gestures runtime"),
    }
}

pub fn apply_runtime_settings(settings: MouseGestureSettings) {
    with_service(|svc| svc.update_settings(settings));
}

pub fn sync_enabled_plugins(enabled_plugins: Option<&HashSet<String>>) {
    let enabled = enabled_plugins
        .map(|set| set.contains(PLUGIN_NAME))
        .unwrap_or(true);
    with_service(|svc| svc.set_plugin_enabled(enabled));
}

pub fn draw_effective_enabled() -> bool {
    let mut enabled = false;
    with_service(|svc| {
        enabled = svc.draw_effective_enabled();
    });
    enabled
}

pub fn set_draw_mode_active(active: bool) {
    with_service(|svc| svc.set_draw_mode_active(active));
}

pub fn restore_draw_prior_effective_state(prior_effective_state: bool) {
    with_service(|svc| svc.restore_draw_prior_effective_state(prior_effective_state));
}

pub struct SuspendToken {
    _private: (),
}

pub fn suspend_for_draw() -> SuspendToken {
    let mut token = None;
    with_service(|svc| {
        token = Some(svc.suspend_for_draw());
    });
    token.unwrap_or(SuspendToken { _private: () })
}

pub fn resume_from_draw(token: SuspendToken) {
    with_service(|svc| svc.resume_from_draw(token));
}

pub struct MouseGesturesPlugin {
    settings: MouseGestureSettings,
    ignore_input: String,
    window_picker_open: bool,
    window_picker_titles: Vec<String>,
    window_picker_error: Option<String>,
}

impl Default for MouseGesturesPlugin {
    fn default() -> Self {
        Self {
            settings: MouseGestureSettings::default(),
            ignore_input: String::new(),
            window_picker_open: false,
            window_picker_titles: Vec::new(),
            window_picker_error: None,
        }
    }
}

impl MouseGesturesPlugin {
    fn command_actions() -> Vec<Action> {
        vec![
            Action {
                label: "mg".into(),
                desc: "Mouse gestures".into(),
                action: "query:mg ".into(),
                args: None,
            },
            Action {
                label: "mg settings".into(),
                desc: "Mouse gestures".into(),
                action: "mg:dialog:settings".into(),
                args: None,
            },
            Action {
                label: "mg edit".into(),
                desc: "Mouse gestures".into(),
                action: "mg:dialog".into(),
                args: None,
            },
            Action {
                label: "mg add".into(),
                desc: "Mouse gestures".into(),
                action: "mg:dialog:binding".into(),
                args: None,
            },
            Action {
                label: "mg list".into(),
                desc: "Mouse gestures".into(),
                action: "query:mg list".into(),
                args: None,
            },
            Action {
                label: "mg find".into(),
                desc: "Mouse gestures".into(),
                action: "query:mg find ".into(),
                args: None,
            },
            Action {
                label: "mg where".into(),
                desc: "Mouse gestures".into(),
                action: "query:mg where ".into(),
                args: None,
            },
            Action {
                label: "mg conflicts".into(),
                desc: "Mouse gestures".into(),
                action: "query:mg conflicts".into(),
                args: None,
            },
        ]
    }

    fn list_gestures(filter: &str) -> Vec<Action> {
        let db = load_gestures(GESTURES_FILE).unwrap_or_default();
        let matcher = SkimMatcherV2::default();
        let filter = filter.trim().to_lowercase();
        db.gestures
            .iter()
            .filter(|gesture| {
                if filter.is_empty() {
                    return true;
                }
                let label = format_gesture_label(gesture).to_lowercase();
                matcher.fuzzy_match(&label, &filter).is_some()
            })
            .map(|gesture| Action {
                label: format_gesture_label(gesture),
                desc: "Mouse gestures".into(),
                action: "mg:dialog".into(),
                args: None,
            })
            .collect()
    }

    fn format_match_desc(context: &BindingMatchContext) -> String {
        if context.fields.is_empty() {
            return "Mouse gestures".into();
        }
        let labels: Vec<&str> = context
            .fields
            .iter()
            .map(|field| match field {
                crate::mouse_gestures::db::BindingMatchField::GestureLabel => "gesture label",
                crate::mouse_gestures::db::BindingMatchField::Tokens => "tokens",
                crate::mouse_gestures::db::BindingMatchField::BindingLabel => "binding label",
                crate::mouse_gestures::db::BindingMatchField::Action => "action",
                crate::mouse_gestures::db::BindingMatchField::Args => "args",
            })
            .collect();
        format!("Mouse gestures (matches: {})", labels.join(", "))
    }
}

impl Plugin for MouseGesturesPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        if trimmed.eq_ignore_ascii_case("mg") {
            return Self::command_actions();
        }
        if strip_prefix_ci(trimmed, "mg settings").is_some()
            || strip_prefix_ci(trimmed, "mg setting").is_some()
        {
            return vec![Action {
                label: "Open mouse gesture settings".into(),
                desc: "Mouse gestures".into(),
                action: "mg:dialog:settings".into(),
                args: None,
            }];
        }
        if strip_prefix_ci(trimmed, "mg edit").is_some() {
            return vec![Action {
                label: "Edit mouse gestures".into(),
                desc: "Mouse gestures".into(),
                action: "mg:dialog".into(),
                args: None,
            }];
        }
        if strip_prefix_ci(trimmed, "mg add").is_some() {
            return vec![Action {
                label: "Add mouse gesture binding".into(),
                desc: "Mouse gestures".into(),
                action: "mg:dialog:binding".into(),
                args: None,
            }];
        }
        if let Some(rest) = strip_prefix_ci(trimmed, "mg find") {
            let query = rest.trim();
            let db = load_gestures(GESTURES_FILE).unwrap_or_default();
            return db
                .search_bindings(query)
                .into_iter()
                .map(|(gesture, binding, context)| Action {
                    label: format_search_result_label(&gesture, &binding),
                    desc: Self::format_match_desc(&context),
                    action: "mg:dialog".into(),
                    args: None,
                })
                .collect();
        }
        if let Some(rest) = strip_prefix_ci(trimmed, "mg where") {
            let action_prefix = rest.trim();
            let db = load_gestures(GESTURES_FILE).unwrap_or_default();
            return db
                .find_by_action(action_prefix)
                .into_iter()
                .map(|(gesture, binding)| Action {
                    label: format_search_result_label(&gesture, &binding),
                    desc: "Mouse gestures".into(),
                    action: "mg:dialog".into(),
                    args: None,
                })
                .collect();
        }
        if let Some(rest) = strip_prefix_ci(trimmed, "mg conflicts") {
            if !rest.trim().is_empty() {
                return Vec::new();
            }
            let db = load_gestures(GESTURES_FILE).unwrap_or_default();
            let mut actions = Vec::new();
            for conflict in db.find_conflicts() {
                let conflict_desc = match conflict.kind {
                    crate::mouse_gestures::db::GestureConflictKind::DuplicateTokens => {
                        "Mouse gestures (conflict: duplicate tokens)"
                    }
                    crate::mouse_gestures::db::GestureConflictKind::PrefixOverlap => {
                        "Mouse gestures (conflict: prefix overlap)"
                    }
                };
                for gesture in conflict.gestures {
                    for binding in gesture
                        .bindings
                        .iter()
                        .filter(|binding| binding.enabled)
                        .cloned()
                    {
                        actions.push(Action {
                            label: format_search_result_label(&gesture, &binding),
                            desc: conflict_desc.into(),
                            action: "mg:dialog".into(),
                            args: None,
                        });
                    }
                }
            }
            return actions;
        }
        if let Some(rest) = strip_prefix_ci(trimmed, "mg list") {
            return Self::list_gestures(rest);
        }
        if let Some(rest) = strip_prefix_ci(trimmed, "mg ") {
            return Self::list_gestures(rest);
        }
        Vec::new()
    }

    fn name(&self) -> &str {
        PLUGIN_NAME
    }

    fn description(&self) -> &str {
        "Handle mouse gestures (prefix: `mg`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        Self::command_actions()
    }

    fn default_settings(&self) -> Option<serde_json::Value> {
        serde_json::to_value(MouseGestureSettings::default()).ok()
    }

    fn apply_settings(&mut self, value: &serde_json::Value) {
        if let Ok(settings) = serde_json::from_value::<MouseGestureSettings>(value.clone()) {
            self.settings = settings.clone();
            apply_runtime_settings(settings);
        }
    }

    fn settings_ui(&mut self, ui: &mut egui::Ui, value: &mut serde_json::Value) {
        let mut cfg =
            serde_json::from_value::<MouseGestureSettings>(value.clone()).unwrap_or_default();

        let mut changed = false;

        changed |= ui
            .checkbox(&mut cfg.enabled, "Enable mouse gestures")
            .changed();
        changed |= ui
            .checkbox(&mut cfg.debug_logging, "Enable debug logging")
            .changed();

        changed |= ui
            .checkbox(&mut cfg.show_trail, "Show trail overlay")
            .changed();

        ui.horizontal(|ui| {
            ui.label("Trail color");
            let mut color = egui::Color32::from_rgba_unmultiplied(
                cfg.trail_color[0],
                cfg.trail_color[1],
                cfg.trail_color[2],
                cfg.trail_color[3],
            );

            let resp = ui.color_edit_button_srgba(&mut color);
            if resp.changed() {
                cfg.trail_color = [color.r(), color.g(), color.b(), color.a()];
                changed = true;
            }
        });

        ui.horizontal(|ui| {
            ui.label("Trail width");
            changed |= ui
                .add(
                    egui::DragValue::new(&mut cfg.trail_width)
                        .clamp_range(1.0..=20.0)
                        .speed(0.5),
                )
                .changed();
        });

        changed |= ui
            .checkbox(&mut cfg.show_hint, "Show hint overlay")
            .changed();

        ui.horizontal(|ui| {
            ui.label("Hint offset (x, y)");
            changed |= ui
                .add(
                    egui::DragValue::new(&mut cfg.hint_offset.0)
                        .clamp_range(-200.0..=200.0)
                        .speed(1.0),
                )
                .changed();
            changed |= ui
                .add(
                    egui::DragValue::new(&mut cfg.hint_offset.1)
                        .clamp_range(-200.0..=200.0)
                        .speed(1.0),
                )
                .changed();
        });

        ui.horizontal(|ui| {
            ui.label("Cancel behavior");
            egui::ComboBox::from_id_source("mg_cancel_behavior")
                .selected_text(cancel_behavior_label(cfg.cancel_behavior))
                .show_ui(ui, |ui| {
                    changed |= ui
                        .selectable_value(
                            &mut cfg.cancel_behavior,
                            CancelBehavior::DoNothing,
                            "Do nothing",
                        )
                        .changed();
                    changed |= ui
                        .selectable_value(
                            &mut cfg.cancel_behavior,
                            CancelBehavior::PassThroughClick,
                            "Pass through right-click",
                        )
                        .changed();
                });
        });

        ui.horizontal(|ui| {
            ui.label("No-match behavior");
            egui::ComboBox::from_id_source("mg_no_match_behavior")
                .selected_text(no_match_behavior_label(cfg.no_match_behavior))
                .show_ui(ui, |ui| {
                    changed |= ui
                        .selectable_value(
                            &mut cfg.no_match_behavior,
                            NoMatchBehavior::DoNothing,
                            "Do nothing",
                        )
                        .changed();
                    changed |= ui
                        .selectable_value(
                            &mut cfg.no_match_behavior,
                            NoMatchBehavior::PassThroughClick,
                            "Pass through right-click",
                        )
                        .changed();
                    changed |= ui
                        .selectable_value(
                            &mut cfg.no_match_behavior,
                            NoMatchBehavior::ShowNoMatchHint,
                            "Show no-match hint",
                        )
                        .changed();
                });
        });
        ui.small(
            "Fallback runs when a gesture does not match; default is pass-through right-click.",
        );

        ui.separator();
        ui.heading("Ignore windows (disable gestures)");
        ui.small(
            "Gestures will be ignored when the active window title contains one of these entries.",
        );

        let mut remove_index: Option<usize> = None;
        if cfg.ignore_window_titles.is_empty() {
            ui.label("No ignored windows.");
        } else {
            for (index, title) in cfg.ignore_window_titles.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(title);
                    if ui.button("Remove").clicked() {
                        remove_index = Some(index);
                    }
                });
            }
        }
        if let Some(index) = remove_index {
            cfg.ignore_window_titles.remove(index);
            changed = true;
        }

        ui.horizontal(|ui| {
            let response = ui.text_edit_singleline(&mut self.ignore_input);
            let mut add_now = ui.button("Add").clicked();
            if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                add_now = true;
            }
            if add_now {
                if add_ignore_window_title(&mut cfg.ignore_window_titles, &self.ignore_input) {
                    changed = true;
                }
                self.ignore_input.clear();
            }
        });

        let window_button = ui.add_enabled(cfg!(windows), egui::Button::new("Select window..."));
        if !cfg!(windows) {
            window_button.on_hover_text("Window picker is only available on Windows.");
        } else if window_button.clicked() {
            self.window_picker_error = None;
            match collect_visible_window_titles() {
                Ok(titles) => {
                    self.window_picker_titles = titles;
                    self.window_picker_open = true;
                }
                Err(err) => {
                    self.window_picker_error = Some(format!("Failed to enumerate windows: {err}"));
                    self.window_picker_titles.clear();
                    self.window_picker_open = true;
                }
            }
        }

        if self.window_picker_open {
            let mut open = self.window_picker_open;
            egui::Window::new("Select window to ignore")
                .open(&mut open)
                .resizable(true)
                .show(ui.ctx(), |ui| {
                    if let Some(err) = &self.window_picker_error {
                        ui.colored_label(egui::Color32::RED, err);
                        ui.separator();
                    }

                    if self.window_picker_titles.is_empty() {
                        ui.label("No windows found.");
                    } else {
                        egui::ScrollArea::vertical()
                            .max_height(220.0)
                            .show(ui, |ui| {
                                for title in self.window_picker_titles.clone() {
                                    ui.horizontal(|ui| {
                                        ui.label(&title);
                                        if ui.button("Add").clicked() {
                                            if add_ignore_window_title(
                                                &mut cfg.ignore_window_titles,
                                                &title,
                                            ) {
                                                changed = true;
                                            }
                                        }
                                    });
                                }
                            });
                    }
                });
            self.window_picker_open = open;
        }

        if normalize_ignore_window_titles(&mut cfg.ignore_window_titles) {
            changed = true;
        }

        // Only write+apply when something changed.
        if changed {
            match serde_json::to_value(&cfg) {
                Ok(v) => *value = v,
                Err(e) => tracing::error!(?e, "failed to serialize mouse gesture settings"),
            }
            self.settings = cfg.clone();
            apply_runtime_settings(cfg);
        }
    }
}

fn cancel_behavior_label(value: CancelBehavior) -> &'static str {
    match value {
        CancelBehavior::DoNothing => "Do nothing",
        CancelBehavior::PassThroughClick => "Pass through right-click",
    }
}

fn no_match_behavior_label(value: NoMatchBehavior) -> &'static str {
    match value {
        NoMatchBehavior::DoNothing => "Do nothing",
        NoMatchBehavior::PassThroughClick => "Pass through right-click",
        NoMatchBehavior::ShowNoMatchHint => "Show no-match hint",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suspend_disables_effective_runtime_without_changing_saved_settings() {
        let mut runtime = MouseGestureRuntime::default();
        runtime.update_settings(MouseGestureSettings {
            enabled: true,
            ..MouseGestureSettings::default()
        });

        let _token = runtime.suspend_for_draw();

        assert!(runtime.settings.enabled);
        assert!(!runtime.effective_enabled());
    }

    #[test]
    fn resume_restores_exact_prior_effective_state() {
        let mut runtime = MouseGestureRuntime::default();
        runtime.update_settings(MouseGestureSettings {
            enabled: true,
            ..MouseGestureSettings::default()
        });
        runtime.set_plugin_enabled(false);
        let before = runtime.effective_enabled();

        let token = runtime.suspend_for_draw();
        runtime.resume_from_draw(token);

        assert_eq!(before, runtime.effective_enabled());
    }

    #[test]
    fn double_suspend_single_resume_remains_suspended() {
        let mut runtime = MouseGestureRuntime::default();
        runtime.update_settings(MouseGestureSettings {
            enabled: true,
            ..MouseGestureSettings::default()
        });

        let _token1 = runtime.suspend_for_draw();
        let token2 = runtime.suspend_for_draw();
        runtime.resume_from_draw(token2);

        assert!(!runtime.effective_enabled());
        assert_eq!(runtime.draw_suspend_count, 1);
    }

    #[test]
    fn resume_without_prior_suspend_is_noop() {
        let mut runtime = MouseGestureRuntime::default();
        runtime.update_settings(MouseGestureSettings {
            enabled: true,
            ..MouseGestureSettings::default()
        });

        runtime.resume_from_draw(SuspendToken { _private: () });

        assert!(runtime.effective_enabled());
        assert_eq!(runtime.draw_suspend_count, 0);
    }
}
