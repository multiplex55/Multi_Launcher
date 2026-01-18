use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const MOUSE_GESTURES_FILE: &str = "mouse_gestures.json";
const CURRENT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MouseGestureBinding {
    pub gesture_id: String,
    #[serde(default)]
    pub label: String,
    pub action: String,
    #[serde(default)]
    pub args: Option<String>,
    #[serde(default)]
    pub priority: i32,
    #[serde(default = "default_binding_enabled")]
    pub enabled: bool,
}

fn default_binding_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MouseGestureRuleField {
    Exe,
    Class,
    Title,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MouseGestureRuleType {
    Contains,
    StartsWith,
    Regex,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MouseGestureProfileRule {
    pub field: MouseGestureRuleField,
    pub matcher: MouseGestureRuleType,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ForegroundWindowInfo {
    pub exe: Option<String>,
    pub class: Option<String>,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MouseGestureProfile {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub rules: Vec<MouseGestureProfileRule>,
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

impl MouseGestureProfileRule {
    pub fn matches(&self, window: &ForegroundWindowInfo) -> bool {
        let target = match self.field {
            MouseGestureRuleField::Exe => window.exe.as_deref(),
            MouseGestureRuleField::Class => window.class.as_deref(),
            MouseGestureRuleField::Title => window.title.as_deref(),
        };
        let Some(target) = target else {
            return false;
        };

        match self.matcher {
            MouseGestureRuleType::Contains => target.contains(&self.value),
            MouseGestureRuleType::StartsWith => target.starts_with(&self.value),
            MouseGestureRuleType::Regex => Regex::new(&self.value)
                .map(|pattern| pattern.is_match(target))
                .unwrap_or(false),
        }
    }
}

pub fn profile_matches(profile: &MouseGestureProfile, window: &ForegroundWindowInfo) -> bool {
    if !profile.enabled {
        return false;
    }
    if profile.rules.is_empty() {
        return true;
    }
    profile.rules.iter().all(|rule| rule.matches(window))
}

pub fn select_profile<'a>(
    db: &'a MouseGestureDb,
    window: &ForegroundWindowInfo,
) -> Option<&'a MouseGestureProfile> {
    let mut best: Option<(usize, &MouseGestureProfile)> = None;
    for (idx, profile) in db.profiles.iter().enumerate() {
        if !profile_matches(profile, window) {
            continue;
        }
        match best {
            None => best = Some((idx, profile)),
            Some((best_idx, best_profile)) => {
                if profile.priority > best_profile.priority
                    || (profile.priority == best_profile.priority && idx < best_idx)
                {
                    best = Some((idx, profile));
                }
            }
        }
    }
    best.map(|(_, profile)| profile)
}

#[derive(Debug, Clone, PartialEq)]
pub struct BindingMatch<'a> {
    pub binding: &'a MouseGestureBinding,
    pub distance: f32,
    pub index: usize,
}

pub fn select_binding<'a>(
    profile: &'a MouseGestureProfile,
    gesture_distances: &HashMap<String, f32>,
    max_distance: f32,
) -> Option<BindingMatch<'a>> {
    let mut best: Option<BindingMatch<'a>> = None;
    for (index, binding) in profile.bindings.iter().enumerate() {
        if !binding.enabled {
            continue;
        }
        let Some(&distance) = gesture_distances.get(&binding.gesture_id) else {
            continue;
        };
        if !distance.is_finite() || distance > max_distance {
            continue;
        }
        match &best {
            None => {
                best = Some(BindingMatch {
                    binding,
                    distance,
                    index,
                });
            }
            Some(current) => {
                if distance < current.distance
                    || (distance == current.distance
                        && (binding.priority > current.binding.priority
                            || (binding.priority == current.binding.priority
                                && index < current.index)))
                {
                    best = Some(BindingMatch {
                        binding,
                        distance,
                        index,
                    });
                }
            }
        }
    }
    best
}
