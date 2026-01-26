use crate::actions::Action;
use crate::common::json_watch::watch_json;
use crate::common::strip_prefix_ci;
use crate::mouse_gestures::db::{
    format_gesture_label, load_gestures, SharedGestureDb, GESTURES_FILE,
};
use crate::mouse_gestures::service::{with_service as with_gesture_service, MouseGestureConfig};
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
    #[serde(default = "default_min_distance_px")]
    pub min_distance_px: f32,
    #[serde(default = "default_max_duration_ms")]
    pub max_duration_ms: u64,
    #[serde(default = "default_require_button")]
    pub require_button: bool,
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
}

impl Default for MouseGestureSettings {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            min_distance_px: default_min_distance_px(),
            max_duration_ms: default_max_duration_ms(),
            require_button: default_require_button(),
            show_trail: default_show_trail(),
            trail_color: default_trail_color(),
            trail_width: default_trail_width(),
            trail_start_move_px: default_trail_start_move_px(),
            show_hint: default_show_hint(),
            hint_offset: default_hint_offset(),
        }
    }
}

fn default_enabled() -> bool {
    true
}

fn default_min_distance_px() -> f32 {
    40.0
}

fn default_max_duration_ms() -> u64 {
    1500
}

fn default_require_button() -> bool {
    true
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
        let db_clone = db.clone();
        let watcher = watch_json(GESTURES_FILE, move || {
            if let Ok(next) = load_gestures(GESTURES_FILE) {
                if let Ok(mut guard) = db_clone.lock() {
                    *guard = next;
                }
            }
        })
        .ok();

        let mut runtime = Self {
            settings: MouseGestureSettings::default(),
            plugin_enabled: true,
            db,
            watcher,
        };

        // Critical: apply defaults once so mg starts without needing a settings.json touch.
        runtime.apply();

        runtime
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
        config.deadzone_px = self.settings.min_distance_px;
        config.trail_start_move_px = self.settings.trail_start_move_px;
        config.show_trail = self.settings.show_trail;
        config.trail_color = self.settings.trail_color;
        config.trail_width = self.settings.trail_width;
        config.show_hint = self.settings.show_hint;
        config.hint_offset = self.settings.hint_offset;
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
                action: "settings:dialog".into(),
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
}

impl Plugin for MouseGesturesPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        if trimmed.eq_ignore_ascii_case("mg") {
            return Self::command_actions();
        }
        if strip_prefix_ci(trimmed, "mg settings").is_some() {
            return vec![Action {
                label: "Open mouse gesture settings".into(),
                desc: "Mouse gestures".into(),
                action: "settings:dialog".into(),
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
            .checkbox(&mut cfg.require_button, "Require gesture button held")
            .changed();

        ui.horizontal(|ui| {
            ui.label("Minimum distance (px)");
            changed |= ui
                .add(
                    egui::DragValue::new(&mut cfg.min_distance_px)
                        .clamp_range(1.0..=500.0)
                        .speed(1.0),
                )
                .changed();
        });

        ui.horizontal(|ui| {
            ui.label("Max duration (ms)");
            changed |= ui
                .add(
                    egui::DragValue::new(&mut cfg.max_duration_ms)
                        .clamp_range(50..=10_000)
                        .speed(10),
                )
                .changed();
        });

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
