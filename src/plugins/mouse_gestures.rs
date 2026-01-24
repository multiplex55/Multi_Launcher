use crate::actions::Action;
use crate::plugin::Plugin;
use eframe::egui;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Mutex;

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
}

impl Default for MouseGestureSettings {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            min_distance_px: default_min_distance_px(),
            max_duration_ms: default_max_duration_ms(),
            require_button: default_require_button(),
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

#[derive(Debug)]
struct MouseGestureService {
    settings: MouseGestureSettings,
    plugin_enabled: bool,
    running: bool,
}

impl Default for MouseGestureService {
    fn default() -> Self {
        Self {
            settings: MouseGestureSettings::default(),
            plugin_enabled: true,
            running: false,
        }
    }
}

impl MouseGestureService {
    fn update_settings(&mut self, settings: MouseGestureSettings) {
        self.settings = settings;
        self.update_state();
    }

    fn set_plugin_enabled(&mut self, enabled: bool) {
        self.plugin_enabled = enabled;
        self.update_state();
    }

    fn update_state(&mut self) {
        let should_run = self.settings.enabled && self.plugin_enabled;
        if should_run {
            self.start();
        } else {
            self.stop();
        }
    }

    fn start(&mut self) {
        if !self.running {
            self.running = true;
            tracing::info!("mouse gestures service started");
        }
    }

    fn stop(&mut self) {
        if self.running {
            self.running = false;
            tracing::info!("mouse gestures service stopped");
        }
    }
}

static SERVICE: OnceCell<Mutex<MouseGestureService>> = OnceCell::new();

fn with_service<F>(f: F)
where
    F: FnOnce(&mut MouseGestureService),
{
    let service = SERVICE.get_or_init(|| Mutex::new(MouseGestureService::default()));
    match service.lock() {
        Ok(mut guard) => f(&mut guard),
        Err(e) => tracing::error!(?e, "failed to lock mouse gestures service"),
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
                action: "query:mg settings".into(),
                args: None,
            },
            Action {
                label: "mg edit".into(),
                desc: "Mouse gestures".into(),
                action: "query:mg edit".into(),
                args: None,
            },
            Action {
                label: "mg add".into(),
                desc: "Mouse gestures".into(),
                action: "query:mg add".into(),
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
}

impl Plugin for MouseGesturesPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        if trimmed.eq_ignore_ascii_case("mg") {
            return Self::command_actions();
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
        let mut cfg = serde_json::from_value::<MouseGestureSettings>(value.clone())
            .unwrap_or_default();
        ui.checkbox(&mut cfg.enabled, "Enable mouse gestures");
        ui.checkbox(&mut cfg.require_button, "Require gesture button held");
        ui.horizontal(|ui| {
            ui.label("Minimum distance (px)");
            ui.add(
                egui::DragValue::new(&mut cfg.min_distance_px)
                    .clamp_range(1.0..=500.0)
                    .speed(1.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Max duration (ms)");
            ui.add(
                egui::DragValue::new(&mut cfg.max_duration_ms)
                    .clamp_range(50..=10_000)
                    .speed(10),
            );
        });
        match serde_json::to_value(&cfg) {
            Ok(v) => *value = v,
            Err(e) => tracing::error!(?e, "failed to serialize mouse gesture settings"),
        }
        self.settings = cfg.clone();
        apply_runtime_settings(cfg);
    }
}
