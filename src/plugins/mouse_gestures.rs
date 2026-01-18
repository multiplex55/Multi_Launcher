pub mod db;
pub mod engine;
pub mod settings;

use crate::actions::Action;
use crate::common::json_watch::{watch_json, JsonWatcher};
use crate::plugin::Plugin;
use crate::mouse_gestures::mouse_gesture_service;
use crate::plugins::mouse_gestures::db::{load_gestures, MOUSE_GESTURES_FILE};
use crate::plugins::mouse_gestures::settings::MouseGesturePluginSettings;

pub struct MouseGesturesPlugin {
    settings: MouseGesturePluginSettings,
    #[allow(dead_code)]
    watcher: Option<JsonWatcher>,
}

impl MouseGesturesPlugin {
    pub fn new() -> Self {
        let db = load_gestures(MOUSE_GESTURES_FILE).unwrap_or_default();
        let service = mouse_gesture_service();
        service.update_db(db.clone());
        service.update_settings(MouseGesturePluginSettings::default());
        let watch_path = MOUSE_GESTURES_FILE.to_string();
        let watch_path_clone = watch_path.clone();
        let watcher = watch_json(&watch_path, move || {
            if let Ok(db) = load_gestures(&watch_path_clone) {
                mouse_gesture_service().update_db(db);
            }
        })
        .ok();
        Self {
            settings: MouseGesturePluginSettings::default(),
            watcher,
        }
    }
}

impl Default for MouseGesturesPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl MouseGesturesPlugin {
    fn base_actions() -> Vec<Action> {
        vec![
            Action {
                label: "Mouse gesture settings".into(),
                desc: "Mouse Gestures".into(),
                action: "mg:settings".into(),
                args: None,
            },
            Action {
                label: "Mouse gesture recorder".into(),
                desc: "Mouse Gestures".into(),
                action: "mg:gesture_recorder".into(),
                args: None,
            },
            Action {
                label: "Add mouse gesture binding".into(),
                desc: "Mouse Gestures".into(),
                action: "mg:add_binding".into(),
                args: None,
            },
        ]
    }

    fn action_for(query: &str) -> Option<Action> {
        if crate::common::strip_prefix_ci(query, "setting").is_some()
            || crate::common::strip_prefix_ci(query, "settings").is_some()
        {
            return Some(Action {
                label: "Mouse gesture settings".into(),
                desc: "Mouse Gestures".into(),
                action: "mg:settings".into(),
                args: None,
            });
        }
        if crate::common::strip_prefix_ci(query, "gesture").is_some() {
            return Some(Action {
                label: "Mouse gesture recorder".into(),
                desc: "Mouse Gestures".into(),
                action: "mg:gesture_recorder".into(),
                args: None,
            });
        }
        if crate::common::strip_prefix_ci(query, "add").is_some() {
            return Some(Action {
                label: "Add mouse gesture binding".into(),
                desc: "Mouse Gestures".into(),
                action: "mg:add_binding".into(),
                args: None,
            });
        }
        None
    }
}

impl Plugin for MouseGesturesPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "mg") {
            let rest = rest.trim();
            if rest.is_empty() {
                return Self::base_actions();
            }
            return Self::action_for(rest).into_iter().collect();
        }
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "mouse") {
            let rest = rest.trim();
            if rest.is_empty() {
                return Self::base_actions();
            }
            return Self::action_for(rest).into_iter().collect();
        }
        Vec::new()
    }

    fn name(&self) -> &str {
        "mouse_gestures"
    }

    fn description(&self) -> &str {
        "Configure mouse gesture bindings (prefix: `mg` or `mouse`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "mg".into(),
                desc: "Mouse Gestures".into(),
                action: "query:mg".into(),
                args: None,
            },
            Action {
                label: "mouse".into(),
                desc: "Mouse Gestures".into(),
                action: "query:mouse".into(),
                args: None,
            },
        ]
    }

    fn default_settings(&self) -> Option<serde_json::Value> {
        serde_json::to_value(&self.settings).ok()
    }

    fn apply_settings(&mut self, value: &serde_json::Value) {
        if let Ok(cfg) = serde_json::from_value::<MouseGesturePluginSettings>(value.clone()) {
            self.settings = cfg;
            mouse_gesture_service().update_settings(self.settings.clone());
        }
    }
}
