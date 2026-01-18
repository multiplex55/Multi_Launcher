use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const MOUSE_GESTURES_FILE: &str = "mouse_gestures.json";
const CURRENT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MouseGestureBinding {
    pub gesture_id: String,
    pub action: String,
    #[serde(default)]
    pub args: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MouseGestureProfile {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub bindings: Vec<MouseGestureBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MouseGestureDb {
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default)]
    pub gestures: Vec<String>,
    #[serde(default)]
    pub profiles: Vec<MouseGestureProfile>,
    #[serde(default)]
    pub bindings: HashMap<String, String>,
}

impl Default for MouseGestureDb {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            gestures: Vec::new(),
            profiles: Vec::new(),
            bindings: HashMap::new(),
        }
    }
}

pub fn load_gestures(path: &str) -> anyhow::Result<MouseGestureDb> {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    if content.trim().is_empty() {
        return Ok(MouseGestureDb::default());
    }
    let mut db: MouseGestureDb = serde_json::from_str(&content)?;
    if db.schema_version != CURRENT_SCHEMA_VERSION {
        db = MouseGestureDb::default();
    }
    Ok(db)
}

pub fn save_gestures(path: &str, db: &MouseGestureDb) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(db)?;
    std::fs::write(path, json)?;
    Ok(())
}
