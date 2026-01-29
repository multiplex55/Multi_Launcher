use crate::actions::Action;
use crate::mouse_gestures::engine::DirMode;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

pub const GESTURES_FILE: &str = "mouse_gestures.json";
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BindingEntry {
    pub label: String,
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GestureEntry {
    pub label: String,
    pub tokens: String,
    pub dir_mode: DirMode,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub bindings: Vec<BindingEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GestureDb {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub gestures: Vec<GestureEntry>,
}

pub type SharedGestureDb = Arc<Mutex<GestureDb>>;

impl Default for GestureDb {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            gestures: Vec::new(),
        }
    }
}

impl GestureDb {
    pub fn match_binding(
        &self,
        tokens: &str,
        dir_mode: DirMode,
    ) -> Option<(&GestureEntry, &BindingEntry)> {
        if tokens.is_empty() {
            return None;
        }
        self.gestures
            .iter()
            .filter(|gesture| gesture.enabled && gesture.dir_mode == dir_mode)
            .filter(|gesture| gesture.tokens == tokens)
            .find_map(|gesture| {
                gesture
                    .bindings
                    .iter()
                    .filter(|binding| binding.enabled)
                    .map(|binding| (gesture, binding))
                    .next()
            })
    }

    pub fn match_binding_owned(
        &self,
        tokens: &str,
        dir_mode: DirMode,
    ) -> Option<(String, BindingEntry)> {
        if tokens.is_empty() {
            return None;
        }
        for gesture in self
            .gestures
            .iter()
            .filter(|gesture| gesture.enabled && gesture.dir_mode == dir_mode)
        {
            if gesture.tokens != tokens {
                continue;
            }
            if let Some(binding) = gesture.bindings.iter().find(|binding| binding.enabled) {
                return Some((gesture.label.clone(), binding.clone()));
            }
        }
        None
    }

    pub fn match_bindings_owned(
        &self,
        tokens: &str,
        dir_mode: DirMode,
    ) -> Option<(String, Vec<BindingEntry>)> {
        if tokens.is_empty() {
            return None;
        }
        for gesture in self
            .gestures
            .iter()
            .filter(|gesture| gesture.enabled && gesture.dir_mode == dir_mode)
        {
            if gesture.tokens != tokens {
                continue;
            }
            let bindings: Vec<BindingEntry> = gesture
                .bindings
                .iter()
                .filter(|binding| binding.enabled)
                .cloned()
                .collect();
            if bindings.is_empty() {
                return None;
            }
            return Some((gesture.label.clone(), bindings));
        }
        None
    }
}

impl BindingEntry {
    pub fn to_action(&self, gesture_label: &str) -> Action {
        Action {
            label: self.label.clone(),
            desc: format!("Mouse gesture: {gesture_label}"),
            action: self.action.clone(),
            args: self.args.clone(),
        }
    }
}

pub fn load_gestures(path: &str) -> anyhow::Result<GestureDb> {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    if content.trim().is_empty() {
        return Ok(GestureDb::default());
    }
    let db: GestureDb = serde_json::from_str(&content)?;
    if db.schema_version != SCHEMA_VERSION {
        return Err(anyhow::anyhow!(
            "Unsupported gesture schema version {}",
            db.schema_version
        ));
    }
    Ok(db)
}

pub fn save_gestures(path: &str, db: &GestureDb) -> anyhow::Result<()> {
    let mut db = db.clone();
    db.schema_version = SCHEMA_VERSION;
    let json = serde_json::to_string_pretty(&db)?;
    std::fs::write(path, json)?;
    Ok(())
}

pub fn format_gesture_label(gesture: &GestureEntry) -> String {
    let tokens = if gesture.tokens.trim().is_empty() {
        "∅"
    } else {
        gesture.tokens.as_str()
    };
    let status = if gesture.enabled { "" } else { " (disabled)" };
    let binding_labels = format_binding_labels(&gesture.bindings);
    let base = format!("{}{} [{tokens}]", gesture.label, status);
    if binding_labels.is_empty() {
        base
    } else {
        format!("{base} → {}", binding_labels.join(", "))
    }
}

pub fn format_binding_labels(bindings: &[BindingEntry]) -> Vec<String> {
    bindings.iter().map(format_binding_label).collect()
}

fn format_binding_label(binding: &BindingEntry) -> String {
    if binding.enabled {
        binding.label.clone()
    } else {
        format!("{} (disabled)", binding.label)
    }
}

fn default_enabled() -> bool {
    true
}

fn default_schema_version() -> u32 {
    SCHEMA_VERSION
}
