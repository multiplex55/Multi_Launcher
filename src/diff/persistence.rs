//! Versioned persistence for durable diff preferences and named sessions only.
//! Runtime workers, cancellation/undo state, buffers, progress, selections, and
//! computed output are intentionally never serialized.

use crate::common::atomic_file::save_atomic;
use crate::diff::query::normalize_path;
use crate::diff::settings::{
    DIFF_CONFIG_VERSION, DiffConfigV1, FolderColumnWidthsV1, FolderSortStateV1, ReplacementRuleV1,
    UnimportantSectionRuleV1,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FolderAlignmentOverrideV1 {
    pub left_relative: PathBuf,
    pub right_relative: PathBuf,
}

impl From<&crate::diff::folder_compare::FolderAlignmentOverride> for FolderAlignmentOverrideV1 {
    fn from(v: &crate::diff::folder_compare::FolderAlignmentOverride) -> Self {
        Self {
            left_relative: v.left_relative.clone(),
            right_relative: v.right_relative.clone(),
        }
    }
}
impl From<&FolderAlignmentOverrideV1> for crate::diff::folder_compare::FolderAlignmentOverride {
    fn from(v: &FolderAlignmentOverrideV1) -> Self {
        Self {
            left_relative: v.left_relative.clone(),
            right_relative: v.right_relative.clone(),
        }
    }
}
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonModeV1 {
    #[default]
    Text,
    Folder,
    Binary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ContentComparisonModeV1 {
    Metadata,
    #[default]
    OnDemand,
    Always,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DisplayPathPairV1 {
    pub left: String,
    pub right: String,
    #[serde(default)]
    pub mode: ComparisonModeV1,
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
    #[serde(default)]
    pub comparison_mode: ComparisonModeV1,
    #[serde(default)]
    pub ignore_whitespace: bool,
    #[serde(default = "default_true")]
    pub case_sensitive: bool,
    #[serde(default)]
    pub replacement_rules: Vec<ReplacementRuleV1>,
    #[serde(default)]
    pub unimportant_section_rules: Vec<UnimportantSectionRuleV1>,
    #[serde(default)]
    pub folder_includes: Vec<String>,
    #[serde(default)]
    pub folder_excludes: Vec<String>,
    #[serde(default = "default_folder_display_filter")]
    pub folder_display_filter: String,
    #[serde(default)]
    pub content_comparison: ContentComparisonModeV1,
    #[serde(default)]
    pub folder_alignment_overrides: Vec<FolderAlignmentOverrideV1>,
    #[serde(default)]
    pub folder_column_widths: FolderColumnWidthsV1,
    #[serde(default)]
    pub folder_sort: FolderSortStateV1,
    #[serde(default = "default_true")]
    pub folder_compare_file_size: bool,
    #[serde(default = "default_true")]
    pub folder_compare_modified_timestamps: bool,
    #[serde(default = "default_tolerance")]
    pub folder_timestamp_tolerance_seconds: f64,
    #[serde(default = "default_true")]
    pub folder_use_text_compare_rules: bool,
    #[serde(default)]
    pub text_details_visible: bool,
    #[serde(default)]
    pub visible_whitespace: bool,
    #[serde(default = "default_true")]
    pub sync_vertical: bool,
    #[serde(default = "default_true")]
    pub sync_horizontal: bool,
    /// 0 = all rows, 1 = differences only. Unknown values restore to 0.
    #[serde(default)]
    pub projection_mode: u8,
}
impl Default for SavedDiffSessionV1 {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            left: String::new(),
            right: String::new(),
            pane_split: 0.5,
            wrap_text: false,
            syntax_highlighting: true,
            syntax_theme: "InspiredGitHub".into(),
            comparison_mode: Default::default(),
            ignore_whitespace: false,
            case_sensitive: true,
            replacement_rules: vec![],
            unimportant_section_rules: vec![],
            folder_includes: vec![],
            folder_excludes: vec![],
            folder_display_filter: "all".into(),
            content_comparison: Default::default(),
            folder_alignment_overrides: vec![],
            folder_column_widths: Default::default(),
            folder_sort: Default::default(),
            folder_compare_file_size: true,
            folder_compare_modified_timestamps: true,
            folder_timestamp_tolerance_seconds: default_tolerance(),
            folder_use_text_compare_rules: true,
            text_details_visible: false,
            visible_whitespace: false,
            sync_vertical: true,
            sync_horizontal: true,
            projection_mode: 0,
        }
    }
}
fn default_true() -> bool {
    true
}
fn default_folder_display_filter() -> String {
    "all".into()
}
fn default_tolerance() -> f64 {
    2.0
}

impl SavedDiffSessionV1 {
    pub fn validate(mut self) -> Self {
        self.pane_split = crate::diff::model::validated_splitter(self.pane_split);
        self.folder_column_widths = self.folder_column_widths.validated();
        if !self.folder_timestamp_tolerance_seconds.is_finite()
            || !(0.0..=86_400.0).contains(&self.folder_timestamp_tolerance_seconds)
        {
            self.folder_timestamp_tolerance_seconds = default_tolerance();
        }
        if self.projection_mode > 1 {
            self.projection_mode = 0;
        }
        self
    }
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
    serde_json::from_value::<DiffPersistenceV1>(value)
        .map(|mut persistence| {
            persistence.config.pane_split =
                crate::diff::model::validated_splitter(persistence.config.pane_split);
            persistence.config.folder_column_widths =
                persistence.config.folder_column_widths.validated();
            persistence.named_sessions = persistence
                .named_sessions
                .into_iter()
                .map(SavedDiffSessionV1::validate)
                .collect();
            Some(persistence)
        })
        .map_err(LoadError::Malformed)
}
pub fn save(path: &Path, value: &DiffPersistenceV1) -> anyhow::Result<()> {
    let data = serde_json::to_vec_pretty(value)?;
    save_atomic(path, &data)
}

pub fn record_recent(state: &mut DiffPersistenceV1, left: String, right: String) {
    record_recent_mode(state, left, right, ComparisonModeV1::Text)
}
/// Call only after `DiffWorkspace::open_paths` succeeds.
pub fn record_recent_mode(
    state: &mut DiffPersistenceV1,
    left: String,
    right: String,
    mode: ComparisonModeV1,
) {
    let identity = pair_identity(&left, &right, mode);
    state
        .recent_comparisons
        .retain(|p| pair_identity(&p.left, &p.right, p.mode) != identity);
    state
        .recent_comparisons
        .insert(0, DisplayPathPairV1 { left, right, mode });
    state
        .recent_comparisons
        .truncate(state.config.max_recent_comparisons);
}
pub fn clear_recents(state: &mut DiffPersistenceV1) {
    state.recent_comparisons.clear();
}
pub fn deduplicate_rule_ids(state: &mut DiffPersistenceV1) {
    fn retain_unique<T>(values: &mut Vec<T>, id: impl Fn(&T) -> &str) {
        let mut seen = HashSet::new();
        values.retain(|v| seen.insert(id(v).to_owned()));
    }
    retain_unique(&mut state.replacement_rules, |r| &r.id);
    retain_unique(&mut state.unimportant_section_rules, |r| &r.id);
}
fn pair_identity(
    left: &str,
    right: &str,
    mode: ComparisonModeV1,
) -> (String, String, ComparisonModeV1) {
    (
        normalize_path(left).to_string_lossy().to_lowercase(),
        normalize_path(right).to_string_lossy().to_lowercase(),
        mode,
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionError {
    DuplicateName(String),
    NotFound(String),
    InvalidRule(String),
    InvalidPath(String),
}
impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateName(n) => write!(f, "a session named '{n}' already exists"),
            Self::NotFound(id) => write!(f, "session '{id}' was not found"),
            Self::InvalidRule(e) => write!(f, "invalid rule: {e}"),
            Self::InvalidPath(e) => write!(f, "cannot reopen session: {e}"),
        }
    }
}
impl std::error::Error for SessionError {}

static NEXT_SESSION_ID: AtomicU64 = AtomicU64::new(1);
pub fn new_session_id() -> String {
    let n = NEXT_SESSION_ID.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("session-{nanos:032x}-{n:016x}")
}
pub fn validate_rules(
    replacements: &[ReplacementRuleV1],
    sections: &[UnimportantSectionRuleV1],
) -> Result<(), SessionError> {
    let mut ids = HashSet::new();
    for r in replacements {
        if !ids.insert(r.id.as_str()) {
            return Err(SessionError::InvalidRule(format!(
                "duplicate rule id '{}'",
                r.id
            )));
        }
        regex::Regex::new(&r.pattern)
            .map_err(|e| SessionError::InvalidRule(format!("{}: {e}", r.pattern)))?;
    }
    for r in sections {
        if !ids.insert(r.id.as_str()) {
            return Err(SessionError::InvalidRule(format!(
                "duplicate rule id '{}'",
                r.id
            )));
        }
        regex::Regex::new(&r.pattern)
            .map_err(|e| SessionError::InvalidRule(format!("{}: {e}", r.pattern)))?;
    }
    Ok(())
}
pub fn insert_session(
    state: &mut DiffPersistenceV1,
    mut session: SavedDiffSessionV1,
) -> Result<String, SessionError> {
    validate_rules(
        &session.replacement_rules,
        &session.unimportant_section_rules,
    )?;
    if state
        .named_sessions
        .iter()
        .any(|s| s.name.eq_ignore_ascii_case(&session.name))
    {
        return Err(SessionError::DuplicateName(session.name));
    }
    if session.id.is_empty() {
        session.id = new_session_id();
    }
    let id = session.id.clone();
    state.named_sessions.push(session);
    Ok(id)
}
pub fn rename_session(
    state: &mut DiffPersistenceV1,
    id: &str,
    name: String,
) -> Result<(), SessionError> {
    if state
        .named_sessions
        .iter()
        .any(|s| s.id != id && s.name.eq_ignore_ascii_case(&name))
    {
        return Err(SessionError::DuplicateName(name));
    }
    let s = state
        .named_sessions
        .iter_mut()
        .find(|s| s.id == id)
        .ok_or_else(|| SessionError::NotFound(id.into()))?;
    s.name = name;
    Ok(())
}
pub fn delete_session(
    state: &mut DiffPersistenceV1,
    id: &str,
) -> Result<SavedDiffSessionV1, SessionError> {
    let i = state
        .named_sessions
        .iter()
        .position(|s| s.id == id)
        .ok_or_else(|| SessionError::NotFound(id.into()))?;
    Ok(state.named_sessions.remove(i))
}
/// Apply and persist as one logical operation. The caller's state is replaced
/// only after the atomic file write succeeds.
pub fn update_atomic(
    path: &Path,
    state: &mut DiffPersistenceV1,
    change: impl FnOnce(&mut DiffPersistenceV1) -> Result<(), SessionError>,
) -> anyhow::Result<()> {
    let mut candidate = state.clone();
    change(&mut candidate).map_err(anyhow::Error::new)?;
    save(path, &candidate)?;
    *state = candidate;
    Ok(())
}
/// Validate sources at reopen time. Contents/results are absent from the
/// persisted type and therefore necessarily recomputed by the workspace.
pub fn reopen_recent(recent: &DisplayPathPairV1) -> Result<(String, String), SessionError> {
    validate_path_pair(&recent.left, &recent.right, recent.mode)
}

pub fn reopen_session(session: &SavedDiffSessionV1) -> Result<(String, String), SessionError> {
    validate_path_pair(&session.left, &session.right, session.comparison_mode)
}

fn validate_path_pair(
    left: &str,
    right: &str,
    mode: ComparisonModeV1,
) -> Result<(String, String), SessionError> {
    let l = normalize_path(left);
    let r = normalize_path(right);
    let lm = std::fs::metadata(&l)
        .map_err(|e| SessionError::InvalidPath(format!("{}: {e}", l.display())))?;
    let rm = std::fs::metadata(&r)
        .map_err(|e| SessionError::InvalidPath(format!("{}: {e}", r.display())))?;
    let valid = match mode {
        ComparisonModeV1::Text | ComparisonModeV1::Binary => lm.is_file() && rm.is_file(),
        ComparisonModeV1::Folder => lm.is_dir() && rm.is_dir(),
    };
    if !valid {
        return Err(SessionError::InvalidPath(
            "saved comparison mode no longer matches its paths".into(),
        ));
    }
    Ok((left.to_owned(), right.to_owned()))
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
    #[test]
    fn runtime_resources_are_absent_from_serialization() {
        let json = serde_json::to_string(&DiffPersistenceV1::default()).unwrap();
        for runtime_field in [
            "folder_runtimes",
            "scan_handle",
            "receiver",
            "active_operation",
            "comparison_queue",
            "comparison_cache",
        ] {
            assert!(!json.contains(runtime_field), "serialized {runtime_field}");
        }
    }
    #[test]
    fn old_session_defaults_alignment_and_new_session_roundtrips_it() {
        let old = r#"{"id":"1","name":"old","left":"l","right":"r","pane_split":0.5,"wrap_text":false,"syntax_highlighting":true,"syntax_theme":"x"}"#;
        let parsed: SavedDiffSessionV1 = serde_json::from_str(old).unwrap();
        assert!(parsed.folder_alignment_overrides.is_empty());
        let mut session = SavedDiffSessionV1::default();
        session
            .folder_alignment_overrides
            .push(FolderAlignmentOverrideV1 {
                left_relative: "old.txt".into(),
                right_relative: "new.txt".into(),
            });
        let json = serde_json::to_string(&session).unwrap();
        assert_eq!(
            serde_json::from_str::<SavedDiffSessionV1>(&json).unwrap(),
            session
        );
    }
    #[test]
    fn session_preferences_round_trip_and_corrupt_values_are_sanitized() {
        let mut session = SavedDiffSessionV1::default();
        session.visible_whitespace = true;
        session.sync_horizontal = false;
        session.projection_mode = 1;
        let decoded: SavedDiffSessionV1 =
            serde_json::from_str(&serde_json::to_string(&session).unwrap()).unwrap();
        assert_eq!(decoded, session);

        session.pane_split = f32::NAN;
        session.folder_column_widths.left_path = f32::INFINITY;
        session.folder_timestamp_tolerance_seconds = -1.0;
        session.projection_mode = 99;
        let safe = session.validate();
        assert_eq!(safe.pane_split, 0.5);
        assert!(safe.folder_column_widths.left_path.is_finite());
        assert_eq!(safe.folder_timestamp_tolerance_seconds, 2.0);
        assert_eq!(safe.projection_mode, 0);
    }

    #[test]
    fn named_sessions_do_not_serialize_ephemeral_view_state() {
        let json = serde_json::to_string(&SavedDiffSessionV1::default()).unwrap();
        for name in [
            "scroll_offset",
            "selected_paths",
            "current_row",
            "find_query",
            "pending_mutation",
            "worker",
        ] {
            assert!(!json.contains(name), "serialized ephemeral field {name}");
        }
    }
}
