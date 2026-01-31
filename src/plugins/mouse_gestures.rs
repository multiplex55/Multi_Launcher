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

#[derive(Debug)]
struct MouseGestureRuntime {
    settings: MouseGestureSettings,
    plugin_enabled: bool,
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

    fn apply(&self) {
        let mut config = MouseGestureConfig::default();
        config.enabled = self.settings.enabled && self.plugin_enabled;
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

#[derive(Default)]
pub struct MouseGesturesPlugin {
    settings: MouseGestureSettings,
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
        ui.small("Fallback runs when a gesture does not match; default is pass-through right-click.");

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
