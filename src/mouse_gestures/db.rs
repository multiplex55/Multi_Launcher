use crate::actions::Action;
use crate::mouse_gestures::engine::DirMode;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
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
    /// Normalized stroke points for previewing the gesture in the UI.
    ///
    /// Stored as signed 16-bit fixed-point coordinates in the range [-32767, 32767]
    /// where +/-32767 corresponds to +/-1.0 in normalized space. The UI scales these
    /// points into the current preview rectangle.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stroke: Vec<[i16; 2]>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GestureMatchType {
    Exact,
    Prefix,
    Fuzzy,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GestureCandidate {
    pub gesture_label: String,
    pub tokens: String,
    pub bindings: Vec<BindingEntry>,
    pub match_type: GestureMatchType,
    pub score: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BindingMatchField {
    GestureLabel,
    Tokens,
    BindingLabel,
    Action,
    Args,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingMatchContext {
    pub fields: Vec<BindingMatchField>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GestureConflictKind {
    DuplicateTokens,
    PrefixOverlap,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GestureConflict {
    pub tokens: String,
    pub dir_mode: DirMode,
    pub kind: GestureConflictKind,
    pub gestures: Vec<GestureEntry>,
}

impl GestureMatchType {
    fn rank(self) -> u8 {
        match self {
            GestureMatchType::Exact => 3,
            GestureMatchType::Prefix => 2,
            GestureMatchType::Fuzzy => 1,
        }
    }
}

impl Default for GestureDb {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            gestures: Vec::new(),
        }
    }
}

impl GestureDb {
    pub fn search_bindings(
        &self,
        query: &str,
    ) -> Vec<(GestureEntry, BindingEntry, BindingMatchContext)> {
        let query = query.trim();
        if query.is_empty() {
            return Vec::new();
        }
        let query_lower = query.to_lowercase();
        let mut results = Vec::new();

        for gesture in self.gestures.iter().filter(|gesture| gesture.enabled) {
            let mut gesture_fields = Vec::new();
            if gesture.label.to_lowercase().contains(&query_lower) {
                gesture_fields.push(BindingMatchField::GestureLabel);
            }
            if gesture.tokens.to_lowercase().contains(&query_lower) {
                gesture_fields.push(BindingMatchField::Tokens);
            }

            for binding in gesture.bindings.iter().filter(|binding| binding.enabled) {
                let mut fields = gesture_fields.clone();
                if binding.label.to_lowercase().contains(&query_lower) {
                    fields.push(BindingMatchField::BindingLabel);
                }
                if binding.action.to_lowercase().contains(&query_lower) {
                    fields.push(BindingMatchField::Action);
                }
                if binding
                    .args
                    .as_ref()
                    .map(|args| args.to_lowercase().contains(&query_lower))
                    .unwrap_or(false)
                {
                    fields.push(BindingMatchField::Args);
                }

                if !fields.is_empty() {
                    fields.sort();
                    fields.dedup();
                    results.push((
                        gesture.clone(),
                        binding.clone(),
                        BindingMatchContext { fields },
                    ));
                }
            }
        }

        results.sort_by(|a, b| {
            a.0.label
                .cmp(&b.0.label)
                .then_with(|| a.0.tokens.cmp(&b.0.tokens))
                .then_with(|| a.1.label.cmp(&b.1.label))
        });
        results
    }

    pub fn find_by_action(&self, action_prefix: &str) -> Vec<(GestureEntry, BindingEntry)> {
        let action_prefix = action_prefix.trim();
        if action_prefix.is_empty() {
            return Vec::new();
        }
        let action_prefix = action_prefix.to_lowercase();
        let mut results = Vec::new();

        for gesture in self.gestures.iter().filter(|gesture| gesture.enabled) {
            for binding in gesture.bindings.iter().filter(|binding| binding.enabled) {
                if binding
                    .action
                    .to_lowercase()
                    .starts_with(&action_prefix)
                {
                    results.push((gesture.clone(), binding.clone()));
                }
            }
        }

        results.sort_by(|a, b| {
            a.0.label
                .cmp(&b.0.label)
                .then_with(|| a.0.tokens.cmp(&b.0.tokens))
                .then_with(|| a.1.label.cmp(&b.1.label))
        });
        results
    }

    pub fn find_conflicts(&self) -> Vec<GestureConflict> {
        let gestures: Vec<&GestureEntry> = self.gestures.iter().filter(|g| g.enabled).collect();
        let mut duplicates: BTreeMap<(DirMode, String), Vec<GestureEntry>> = BTreeMap::new();
        for gesture in &gestures {
            duplicates
                .entry((gesture.dir_mode, gesture.tokens.clone()))
                .or_default()
                .push((*gesture).clone());
        }

        let mut conflicts = Vec::new();
        for ((dir_mode, tokens), mut grouped) in duplicates {
            if grouped.len() > 1 {
                grouped.sort_by(|a, b| a.label.cmp(&b.label));
                conflicts.push(GestureConflict {
                    tokens,
                    dir_mode,
                    kind: GestureConflictKind::DuplicateTokens,
                    gestures: grouped,
                });
            }
        }

        let mut prefix_groups: BTreeMap<(DirMode, String), BTreeSet<(String, String)>> =
            BTreeMap::new();
        for gesture in &gestures {
            if gesture.tokens.trim().is_empty() {
                continue;
            }
            for other in &gestures {
                if gesture.dir_mode != other.dir_mode || gesture.tokens == other.tokens {
                    continue;
                }
                if other.tokens.starts_with(&gesture.tokens) {
                    let key = (gesture.dir_mode, gesture.tokens.clone());
                    let entry = prefix_groups.entry(key).or_default();
                    entry.insert((gesture.label.clone(), gesture.tokens.clone()));
                    entry.insert((other.label.clone(), other.tokens.clone()));
                }
            }
        }

        for ((dir_mode, tokens), grouped) in prefix_groups {
            if grouped.len() <= 1 {
                continue;
            }
            let mut gesture_list: Vec<GestureEntry> = grouped
                .into_iter()
                .filter_map(|(label, tokens_match)| {
                    gestures
                        .iter()
                        .find(|g| g.label == label && g.tokens == tokens_match)
                        .cloned()
                })
                .collect();
            gesture_list.sort_by(|a, b| a.label.cmp(&b.label));
            conflicts.push(GestureConflict {
                tokens,
                dir_mode,
                kind: GestureConflictKind::PrefixOverlap,
                gestures: gesture_list,
            });
        }

        conflicts.sort_by(|a, b| {
            a.tokens
                .cmp(&b.tokens)
                .then_with(|| a.dir_mode.cmp(&b.dir_mode))
                .then_with(|| a.kind.cmp(&b.kind))
        });
        conflicts
    }

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

    pub fn candidate_matches(
        &self,
        tokens_prefix: &str,
        dir_mode: DirMode,
    ) -> Vec<GestureCandidate> {
        if tokens_prefix.is_empty() {
            return Vec::new();
        }
        let mut candidates = Vec::new();
        for gesture in self
            .gestures
            .iter()
            .filter(|gesture| gesture.enabled && gesture.dir_mode == dir_mode)
        {
            let bindings: Vec<BindingEntry> = gesture
                .bindings
                .iter()
                .filter(|binding| binding.enabled)
                .cloned()
                .collect();
            if bindings.is_empty() {
                continue;
            }

            if gesture.tokens == tokens_prefix {
                candidates.push(GestureCandidate {
                    gesture_label: gesture.label.clone(),
                    tokens: gesture.tokens.clone(),
                    bindings,
                    match_type: GestureMatchType::Exact,
                    score: 1.0,
                });
                continue;
            }

            if gesture.tokens.starts_with(tokens_prefix) {
                let score = tokens_prefix.len() as f32 / gesture.tokens.len() as f32;
                candidates.push(GestureCandidate {
                    gesture_label: gesture.label.clone(),
                    tokens: gesture.tokens.clone(),
                    bindings,
                    match_type: GestureMatchType::Prefix,
                    score,
                });
                continue;
            }

            if let Some(score) = fuzzy_score(tokens_prefix, &gesture.tokens) {
                candidates.push(GestureCandidate {
                    gesture_label: gesture.label.clone(),
                    tokens: gesture.tokens.clone(),
                    bindings,
                    match_type: GestureMatchType::Fuzzy,
                    score,
                });
            }
        }

        candidates.sort_by(|a, b| {
            b.match_type
                .rank()
                .cmp(&a.match_type.rank())
                .then_with(|| {
                    b.score
                        .partial_cmp(&a.score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| a.gesture_label.cmp(&b.gesture_label))
                .then_with(|| a.tokens.cmp(&b.tokens))
        });
        candidates
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
    let tokens = format_tokens(&gesture.tokens);
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

pub fn format_tokens(tokens: &str) -> String {
    if tokens.trim().is_empty() {
        "∅".into()
    } else {
        tokens.to_string()
    }
}

pub fn format_dir_mode_label(dir_mode: DirMode) -> &'static str {
    match dir_mode {
        DirMode::Four => "Four",
        DirMode::Eight => "Eight",
    }
}

pub fn format_search_result_label(gesture: &GestureEntry, binding: &BindingEntry) -> String {
    format!(
        "{} [{}] → {} (binding: {})",
        format_tokens(&gesture.tokens),
        format_dir_mode_label(gesture.dir_mode),
        gesture.label,
        binding.label
    )
}

fn default_enabled() -> bool {
    true
}

fn default_schema_version() -> u32 {
    SCHEMA_VERSION
}

fn fuzzy_score(needle: &str, haystack: &str) -> Option<f32> {
    let mut matched = 0_usize;
    let mut start_index = 0_usize;
    let hay_chars: Vec<char> = haystack.chars().collect();
    for ch in needle.chars() {
        if let Some((idx, _)) = hay_chars
            .iter()
            .enumerate()
            .skip(start_index)
            .find(|(_, candidate)| **candidate == ch)
        {
            matched += 1;
            start_index = idx + 1;
        }
    }

    if matched == 0 {
        return None;
    }

    Some(matched as f32 / hay_chars.len() as f32)
}
