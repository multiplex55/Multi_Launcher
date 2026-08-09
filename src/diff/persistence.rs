//! Versioned persistence for durable diff preferences and named sessions only.
//! Runtime workers, cancellation/undo state, buffers, progress, selections, and
//! computed output are intentionally never serialized.

use crate::common::atomic_file::save_atomic;
use crate::diff::query::normalize_path;
use crate::diff::settings::{
    DIFF_CONFIG_VERSION, DiffConfigV1, ReplacementRuleV1, UnimportantSectionRuleV1,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::ErrorKind;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DisplayPathPairV1 {
    pub left: String,
    pub right: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedDiffSessionV1 {
    pub id: String,
    pub name: String,
    pub left: String,
    pub right: String,
    pub pane_split: f32,
    pub wrap_text: bool,
    pub syntax_highlighting: bool,
    pub syntax_theme: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DiffPersistenceV1 {
    pub version: u32,
    pub config: DiffConfigV1,
    pub recent_comparisons: Vec<DisplayPathPairV1>,
    pub named_sessions: Vec<SavedDiffSessionV1>,
    pub replacement_rules: Vec<ReplacementRuleV1>,
    pub unimportant_section_rules: Vec<UnimportantSectionRuleV1>,
    pub window_size: Option<[f32; 2]>,
    pub window_position: Option<[f32; 2]>,
}
impl Default for DiffPersistenceV1 {
    fn default() -> Self {
        Self {
            version: DIFF_CONFIG_VERSION,
            config: Default::default(),
            recent_comparisons: vec![],
            named_sessions: vec![],
            replacement_rules: vec![],
            unimportant_section_rules: vec![],
            window_size: None,
            window_position: None,
        }
    }
}

#[derive(Debug)]
pub enum LoadError {
    Io(std::io::Error),
    Malformed(serde_json::Error),
    UnsupportedVersion(u64),
}
impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "read diff settings: {e}"),
            Self::Malformed(e) => write!(f, "malformed diff settings: {e}"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported diff settings version {v}"),
        }
    }
}
impl std::error::Error for LoadError {}

pub fn load(path: &Path) -> Result<Option<DiffPersistenceV1>, LoadError> {
    let bytes = match std::fs::read(path) {
        Ok(v) => v,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(LoadError::Io(e)),
    };
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(LoadError::Malformed)?;
    let version = value.get("version").and_then(|v| v.as_u64()).unwrap_or(0);
    if version != DIFF_CONFIG_VERSION as u64 {
        return Err(LoadError::UnsupportedVersion(version));
    }
    serde_json::from_value(value)
        .map(Some)
        .map_err(LoadError::Malformed)
}
pub fn save(path: &Path, value: &DiffPersistenceV1) -> anyhow::Result<()> {
    let data = serde_json::to_vec_pretty(value)?;
    save_atomic(path, &data)
}

pub fn record_recent(state: &mut DiffPersistenceV1, left: String, right: String) {
    let identity = pair_identity(&left, &right);
    state
        .recent_comparisons
        .retain(|p| pair_identity(&p.left, &p.right) != identity);
    state
        .recent_comparisons
        .insert(0, DisplayPathPairV1 { left, right });
    state
        .recent_comparisons
        .truncate(state.config.max_recent_comparisons);
}
pub fn deduplicate_rule_ids(state: &mut DiffPersistenceV1) {
    fn retain_unique<T>(values: &mut Vec<T>, id: impl Fn(&T) -> &str) {
        let mut seen = HashSet::new();
        values.retain(|v| seen.insert(id(v).to_owned()));
    }
    retain_unique(&mut state.replacement_rules, |r| &r.id);
    retain_unique(&mut state.unimportant_section_rules, |r| &r.id);
}
fn pair_identity(left: &str, right: &str) -> (String, String) {
    (
        normalize_path(left).to_string_lossy().to_lowercase(),
        normalize_path(right).to_string_lossy().to_lowercase(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip_and_unknown_fields() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("d.json");
        let s = DiffPersistenceV1::default();
        save(&p, &s).unwrap();
        assert_eq!(load(&p).unwrap(), Some(s));
        let mut v: serde_json::Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
        v["future"] = true.into();
        std::fs::write(&p, serde_json::to_vec(&v).unwrap()).unwrap();
        assert!(load(&p).unwrap().is_some());
    }
    #[test]
    fn missing_and_malformed() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("x");
        assert!(load(&p).unwrap().is_none());
        std::fs::write(&p, "{").unwrap();
        assert!(matches!(load(&p), Err(LoadError::Malformed(_))));
        assert_eq!(std::fs::read_to_string(p).unwrap(), "{");
    }
    #[test]
    fn recent_is_bounded_and_deduped() {
        let mut s = DiffPersistenceV1::default();
        s.config.max_recent_comparisons = 1;
        record_recent(&mut s, "a".into(), "b".into());
        record_recent(&mut s, "./a".into(), "./b".into());
        assert_eq!(s.recent_comparisons.len(), 1);
        assert_eq!(s.recent_comparisons[0].left, "./a");
    }
}
