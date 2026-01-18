use crate::actions::Action;
use crate::plugin::Plugin;

pub struct MouseGesturesPlugin;

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
}
