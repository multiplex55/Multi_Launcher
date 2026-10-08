use super::snippet_template::{ParsedTemplate, TemplateError, parse_template};
use crate::actions::Action;
use crate::common::json_watch::{JsonWatcher, watch_json};
use crate::common::persistence::{LoadState, PersistenceError, load_json, save_json_atomic};
use crate::plugin::Plugin;
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex, MutexGuard,
    atomic::{AtomicU64, Ordering},
};

pub const SNIPPETS_FILE: &str = "snippets.json";

const SNIPPET_RUN_PREFIX: &str = "snippet:run:";

static SNIPPETS_VERSION: AtomicU64 = AtomicU64::new(0);
static SNIPPETS_TRANSACTION: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));
static VERSIONED_SNIPPETS: Lazy<Mutex<Option<(PathBuf, Vec<SnippetEntry>)>>> =
    Lazy::new(|| Mutex::new(None));
static LIVE_SNIPPETS: Lazy<super::live_snapshot::LiveSnapshotRegistry<SnippetEntry>> =
    Lazy::new(super::live_snapshot::LiveSnapshotRegistry::new);

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SnippetInputKind {
    #[default]
    SingleLine,
    Multiline,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct SnippetFieldDefinition {
    /// Placeholder key used in the snippet text, for example `name` in `{{name}}`.
    pub name: String,
    /// Empty persisted labels are displayed using a readable version of `name`.
    #[serde(default)]
    pub label: String,
    #[serde(default, rename = "default")]
    pub default_value: String,
    #[serde(default = "required_by_default")]
    pub required: bool,
    #[serde(default)]
    pub input_kind: SnippetInputKind,
}

impl SnippetFieldDefinition {
    /// Create a new authored field with the standard required, single-line defaults.
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            label: readable_field_label(&name),
            name,
            default_value: String::new(),
            required: true,
            input_kind: SnippetInputKind::SingleLine,
        }
    }

    /// Return the configured label or a readable fallback derived from the key.
    pub fn display_label(&self) -> Cow<'_, str> {
        if self.label.is_empty() {
            Cow::Owned(readable_field_label(&self.name))
        } else {
            Cow::Borrowed(&self.label)
        }
    }
}

/// Reconcile configured fields against unique, discovery-ordered placeholder keys.
///
/// Existing definitions are retained by key, newly discovered keys receive
/// standard defaults, and definitions for missing keys are omitted. This is a
/// pure candidate-building operation; it does not mutate persisted metadata.
pub(crate) fn reconcile_snippet_fields(
    existing: &[SnippetFieldDefinition],
    discovered_keys: &[String],
) -> Vec<SnippetFieldDefinition> {
    discovered_keys
        .iter()
        .map(|key| {
            existing
                .iter()
                .find(|field| field.name == *key)
                .cloned()
                .unwrap_or_else(|| SnippetFieldDefinition::new(key.clone()))
        })
        .collect()
}

#[derive(Debug)]
pub(crate) enum SnippetPreparationError {
    InvalidTemplate(TemplateError),
    NoFields,
    DuplicateFieldDefinitions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreparedSnippetTemplate {
    pub(crate) parsed: ParsedTemplate,
    pub(crate) fields: Vec<SnippetFieldDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SnippetRunMode {
    Plain,
    Prompted(PreparedSnippetTemplate),
}

impl fmt::Display for SnippetPreparationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTemplate(error) => write!(formatter, "invalid template: {error}"),
            Self::NoFields => {
                formatter.write_str("prompted snippets must contain at least one valid placeholder")
            }
            Self::DuplicateFieldDefinitions => {
                formatter.write_str("prompted snippet has duplicate configured field definitions")
            }
        }
    }
}

impl std::error::Error for SnippetPreparationError {}

/// Parse and build an effective field list without changing persisted metadata.
/// Missing definitions use authored-field defaults and orphan definitions are omitted.
pub(crate) fn prepare_prompted_template(
    text: &str,
    configured_fields: &[SnippetFieldDefinition],
) -> Result<PreparedSnippetTemplate, SnippetPreparationError> {
    let parsed = parse_template(text).map_err(SnippetPreparationError::InvalidTemplate)?;
    if parsed.field_keys.is_empty() {
        return Err(SnippetPreparationError::NoFields);
    }

    let mut known_keys = HashSet::new();
    if configured_fields
        .iter()
        .any(|field| !known_keys.insert(field.name.as_str()))
    {
        return Err(SnippetPreparationError::DuplicateFieldDefinitions);
    }

    let fields = reconcile_snippet_fields(configured_fields, &parsed.field_keys);
    Ok(PreparedSnippetTemplate { parsed, fields })
}

/// Make the single domain-level decision about how a resolved snippet runs.
/// Plain text bypasses template parsing; prompted text is fully prepared before
/// either the GUI or headless adapter chooses its execution behavior.
pub(crate) fn prepare_snippet_run(
    entry: &SnippetEntry,
) -> Result<SnippetRunMode, SnippetPreparationError> {
    if !entry.prompt_for_fields {
        return Ok(SnippetRunMode::Plain);
    }
    prepare_prompted_template(&entry.text, &entry.fields).map(SnippetRunMode::Prompted)
}

/// Build a text-edit candidate, parsing and reconciling only opted-in snippets.
/// The returned entry is detached from the persisted snapshot until the caller commits it.
pub(crate) fn prepare_snippet_text(
    entry: &SnippetEntry,
    text: &str,
) -> Result<SnippetEntry, SnippetPreparationError> {
    let mut candidate = entry.clone();
    candidate.text = text.to_owned();
    if !entry.prompt_for_fields {
        return Ok(candidate);
    }

    candidate.fields = prepare_prompted_template(text, &entry.fields)?.fields;
    Ok(candidate)
}

fn readable_field_label(name: &str) -> String {
    let label = name
        .split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut characters = part.chars();
            characters.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(characters).collect()
            })
        })
        .collect::<Vec<_>>()
        .join(" ");
    if label.is_empty() {
        name.to_owned()
    } else {
        label
    }
}

fn required_by_default() -> bool {
    true
}

/// Construct the stable, payload-free command used by snippet search results.
///
/// Percent-encoding every byte outside the URI unreserved set keeps action
/// strings safe for the launcher's delimiter-based protocols while preserving
/// aliases exactly when decoded.
pub fn snippet_run_action(alias: &str) -> String {
    let mut action = String::with_capacity(SNIPPET_RUN_PREFIX.len() + alias.len());
    action.push_str(SNIPPET_RUN_PREFIX);
    for byte in alias.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            action.push(char::from(byte));
        } else {
            use std::fmt::Write as _;
            let _ = write!(action, "%{byte:02X}");
        }
    }
    action
}

/// Decode a canonical snippet-run action, rejecting malformed or empty aliases.
pub fn decode_snippet_run_action(action: &str) -> Option<String> {
    decode_snippet_run_alias(action.strip_prefix(SNIPPET_RUN_PREFIX)?)
}

fn decode_snippet_run_alias(encoded: &str) -> Option<String> {
    if encoded.is_empty() {
        return None;
    }

    let bytes = encoded.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' => {
                let high = *bytes.get(index + 1)?;
                let low = *bytes.get(index + 2)?;
                decoded.push((hex_value(high)? << 4) | hex_value(low)?);
                index += 3;
            }
            byte if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') => {
                decoded.push(byte);
                index += 1;
            }
            _ => return None,
        }
    }

    String::from_utf8(decoded)
        .ok()
        .filter(|alias| !alias.is_empty())
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnippetResolutionError {
    Unavailable,
    MissingOrAmbiguous,
}

impl fmt::Display for SnippetResolutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable => formatter.write_str("saved snippets could not be read"),
            Self::MissingOrAmbiguous => {
                formatter.write_str("snippet is missing or has an ambiguous alias")
            }
        }
    }
}

impl std::error::Error for SnippetResolutionError {}

/// Resolve the current persisted entry by exact, case-sensitive alias.
/// The file is read under the mutation transaction so stale search results
/// cannot execute an old body or prompt configuration.
pub fn resolve_snippet(alias: &str) -> Result<SnippetEntry, SnippetResolutionError> {
    resolve_snippet_from(SNIPPETS_FILE, alias)
}

pub(crate) fn resolve_snippet_from(
    path: &str,
    alias: &str,
) -> Result<SnippetEntry, SnippetResolutionError> {
    let _transaction = snippets_transaction_guard();
    let snippets = load_snippets(path).map_err(|_| SnippetResolutionError::Unavailable)?;
    let mut matching = snippets.iter().filter(|entry| entry.alias == alias);
    let Some(entry) = matching.next() else {
        return Err(SnippetResolutionError::MissingOrAmbiguous);
    };
    if matching.next().is_some() {
        return Err(SnippetResolutionError::MissingOrAmbiguous);
    }
    Ok(entry.clone())
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct SnippetEntry {
    pub alias: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub hide_contents: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub prompt_for_fields: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<SnippetFieldDefinition>,
}

/// Produce a safe, single-line body preview without changing the saved text.
pub fn snippet_preview_text(entry: &SnippetEntry) -> String {
    if entry.hide_contents {
        return "******".to_owned();
    }

    let mut preview = String::with_capacity(entry.text.len());
    let mut space_pending = false;
    for character in entry.text.chars() {
        if character.is_whitespace() || character.is_control() {
            space_pending = !preview.is_empty();
        } else {
            if space_pending {
                preview.push(' ');
                space_pending = false;
            }
            preview.push(character);
        }
    }
    preview
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// Load all snippets from the JSON file at `path`.
pub fn load_snippets(path: &str) -> anyhow::Result<Vec<SnippetEntry>> {
    match load_snippets_typed(path)? {
        LoadState::Missing | LoadState::Empty => Ok(Vec::new()),
        LoadState::Loaded(snippets) => Ok(snippets),
    }
}

pub fn load_snippets_typed(
    path: impl AsRef<Path>,
) -> Result<LoadState<Vec<SnippetEntry>>, PersistenceError> {
    load_json(path)
}

pub(crate) fn load_snippets_for_reload(
    path: impl AsRef<Path>,
) -> Result<LoadState<Vec<SnippetEntry>>, PersistenceError> {
    let _transaction = snippets_transaction_guard();
    load_snippets_typed(path)
}

/// Persist `snippets` to `path`.
pub fn save_snippets(path: &str, snippets: &[SnippetEntry]) -> anyhow::Result<()> {
    replace_snippets(path, snippets.to_vec()).map(|_| ())
}

pub fn replace_snippets(
    path: &str,
    replacement: Vec<SnippetEntry>,
) -> anyhow::Result<Vec<SnippetEntry>> {
    update_snippets(path, move |snippets| {
        *snippets = replacement;
        Ok(true)
    })
}

pub fn update_snippets(
    path: &str,
    mutate: impl FnOnce(&mut Vec<SnippetEntry>) -> anyhow::Result<bool>,
) -> anyhow::Result<Vec<SnippetEntry>> {
    update_snippets_with_save(path, mutate, |path, snippets| {
        save_json_atomic(path, snippets).map_err(Into::into)
    })
}

fn update_snippets_with_save(
    path: &str,
    mutate: impl FnOnce(&mut Vec<SnippetEntry>) -> anyhow::Result<bool>,
    save: impl FnOnce(&str, &[SnippetEntry]) -> anyhow::Result<()>,
) -> anyhow::Result<Vec<SnippetEntry>> {
    let _transaction = snippets_transaction_guard();
    let mut snippets = load_snippets(path)?;
    if mutate(&mut snippets)? {
        save(path, &snippets)?;
        LIVE_SNIPPETS.publish(path, &snippets);
        record_versioned_snippets(path, &snippets);
    }
    Ok(snippets)
}

fn snippets_transaction_guard() -> MutexGuard<'static, ()> {
    SNIPPETS_TRANSACTION
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Append or update a snippet entry identified by `alias`.
pub fn append_snippet(path: &str, alias: &str, text: &str) -> anyhow::Result<()> {
    let alias = alias.to_owned();
    let text = text.to_owned();
    update_snippets(path, move |list| {
        if let Some(index) = list.iter().position(|entry| entry.alias == alias) {
            if list[index].text == text {
                return Ok(false);
            }
            let candidate = prepare_snippet_text(&list[index], &text)?;
            list[index] = candidate;
        } else {
            list.push(SnippetEntry {
                alias,
                text,
                hide_contents: false,
                prompt_for_fields: false,
                fields: Vec::new(),
            });
        }
        Ok(true)
    })?;
    Ok(())
}

/// Remove the snippet identified by `alias`.
pub fn remove_snippet(path: &str, alias: &str) -> anyhow::Result<()> {
    let alias = alias.to_owned();
    update_snippets(path, move |list| {
        let Some(pos) = list.iter().position(|entry| entry.alias == alias) else {
            return Ok(false);
        };
        list.remove(pos);
        Ok(true)
    })?;
    Ok(())
}

pub fn snippets_version() -> u64 {
    SNIPPETS_VERSION.load(Ordering::SeqCst)
}

fn bump_snippets_version() {
    SNIPPETS_VERSION.fetch_add(1, Ordering::SeqCst);
}

fn record_versioned_snippets(path: &str, snippets: &[SnippetEntry]) {
    *VERSIONED_SNIPPETS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) =
        Some((PathBuf::from(path), snippets.to_vec()));
    bump_snippets_version();
}

fn bump_for_external_snippets(path: &str, snippets: &[SnippetEntry]) {
    let mut versioned = VERSIONED_SNIPPETS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if versioned
        .as_ref()
        .is_some_and(|(saved_path, saved)| saved_path == Path::new(path) && saved == snippets)
    {
        return;
    }
    *versioned = Some((PathBuf::from(path), snippets.to_vec()));
    bump_snippets_version();
}

pub struct SnippetsPlugin {
    matcher: SkimMatcherV2,
    data: Arc<Mutex<Vec<SnippetEntry>>>,
    #[allow(dead_code)]
    watcher: Option<JsonWatcher>,
}

impl SnippetsPlugin {
    /// Create a new snippets plugin instance.
    pub fn new() -> Self {
        Self::new_for_path(SNIPPETS_FILE)
    }

    fn new_for_path(path: &str) -> Self {
        let data = {
            let _transaction = snippets_transaction_guard();
            let startup = match load_snippets(path) {
                Ok(snippets) => Some(snippets),
                Err(error) => {
                    tracing::error!(%error, "snippet startup retained invalid persisted file");
                    None
                }
            };
            let data = LIVE_SNIPPETS.get_or_create(path, startup.clone());
            if let Some(startup) = startup {
                *VERSIONED_SNIPPETS
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) =
                    Some((PathBuf::from(path), startup));
            }
            data
        };
        let data_clone = data.clone();
        let path = path.to_string();
        let watcher = watch_json(&path, {
            let path = path.clone();
            move || {
                if let Err(error) = reload_snippet_snapshot(&path, &data_clone) {
                    tracing::error!(%error, "invalid snippet reload retained last-good state");
                }
            }
        })
        .ok();
        Self {
            matcher: SkimMatcherV2::default(),
            data,
            watcher,
        }
    }
}

fn reload_snippet_snapshot(path: &str, data: &Arc<Mutex<Vec<SnippetEntry>>>) -> anyhow::Result<()> {
    let _transaction = snippets_transaction_guard();
    let snippets = match load_snippets_typed(path)? {
        LoadState::Missing => anyhow::bail!("snippets file was removed; retaining last-good state"),
        LoadState::Empty => Vec::new(),
        LoadState::Loaded(snippets) => snippets,
    };
    let mut current = data
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if *current == snippets {
        return Ok(());
    }
    *current = snippets.clone();
    drop(current);
    bump_for_external_snippets(path, &snippets);
    Ok(())
}

impl Default for SnippetsPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for SnippetsPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "cs")
            && rest.is_empty()
        {
            return vec![Action {
                label: "cs: edit snippets".into(),
                desc: "Snippet".into(),
                action: "snippet:dialog".into(),
                args: None,
            }];
        }
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "cs rm") {
            let filter = rest.trim();
            let guard = match self.data.lock() {
                Ok(g) => g,
                Err(_) => return Vec::new(),
            };
            return guard
                .iter()
                .filter(|s| {
                    filter.is_empty()
                        || self.matcher.fuzzy_match(&s.alias, filter).is_some()
                        || self.matcher.fuzzy_match(&s.text, filter).is_some()
                })
                .map(|s| Action {
                    label: format!("Remove snippet {}", s.alias.clone()),
                    desc: "Snippet".into(),
                    action: format!("snippet:remove:{}", s.alias.clone()),
                    args: None,
                })
                .collect();
        }

        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "cs add ") {
            let mut parts = rest.trim().splitn(2, ' ');
            let alias = parts.next().unwrap_or("").trim();
            let text = parts.next().unwrap_or("").trim();
            if !alias.is_empty() && !text.is_empty() {
                return vec![Action {
                    label: format!("Add snippet {alias}"),
                    desc: "Snippet".into(),
                    action: format!("snippet:add:{alias}|{text}"),
                    args: None,
                }];
            }
        }

        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "cs edit") {
            let rest = rest.trim();
            if let Some((alias, text)) = rest.split_once(' ') {
                let alias = alias.trim();
                let text = text.trim();
                if !alias.is_empty() && !text.is_empty() {
                    return vec![Action {
                        label: format!("Edit snippet {alias}"),
                        desc: "Snippet".into(),
                        action: format!("snippet:add:{alias}|{text}"),
                        args: None,
                    }];
                }
            }
            let filter = rest;
            let guard = match self.data.lock() {
                Ok(g) => g,
                Err(_) => return Vec::new(),
            };
            return guard
                .iter()
                .filter(|s| {
                    filter.is_empty()
                        || self.matcher.fuzzy_match(&s.alias, filter).is_some()
                        || self.matcher.fuzzy_match(&s.text, filter).is_some()
                })
                .map(|s| Action {
                    label: format!("Edit snippet {}", s.alias.clone()),
                    desc: "Snippet".into(),
                    action: format!("snippet:edit:{}", s.alias.clone()),
                    args: None,
                })
                .collect();
        }

        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "cs list") {
            let filter = rest.trim();
            let guard = match self.data.lock() {
                Ok(g) => g,
                Err(_) => return Vec::new(),
            };
            return guard
                .iter()
                .filter(|s| {
                    self.matcher.fuzzy_match(&s.alias, filter).is_some()
                        || self.matcher.fuzzy_match(&s.text, filter).is_some()
                })
                .map(|s| Action {
                    label: s.alias.clone(),
                    desc: "Snippet".into(),
                    action: snippet_run_action(&s.alias),
                    args: None,
                })
                .collect();
        }

        if let Some(filter) = crate::common::strip_prefix_ci(trimmed, "cs") {
            let filter = filter.trim();
            let guard = match self.data.lock() {
                Ok(g) => g,
                Err(_) => return Vec::new(),
            };
            return guard
                .iter()
                .filter(|s| {
                    self.matcher.fuzzy_match(&s.alias, filter).is_some()
                        || self.matcher.fuzzy_match(&s.text, filter).is_some()
                })
                .map(|s| Action {
                    label: s.alias.clone(),
                    desc: "Snippet".into(),
                    action: snippet_run_action(&s.alias),
                    args: None,
                })
                .collect();
        }
        Vec::new()
    }

    fn name(&self) -> &str {
        "snippets"
    }

    fn description(&self) -> &str {
        "Search saved text snippets (prefix: `cs`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "cs".into(),
                desc: "Snippet".into(),
                action: "query:cs".into(),
                args: None,
            },
            Action {
                label: "cs add".into(),
                desc: "Snippet".into(),
                action: "query:cs add ".into(),
                args: None,
            },
            Action {
                label: "cs rm".into(),
                desc: "Snippet".into(),
                action: "query:cs rm ".into(),
                args: None,
            },
            Action {
                label: "cs list".into(),
                desc: "Snippet".into(),
                action: "query:cs list".into(),
                args: None,
            },
            Action {
                label: "cs edit".into(),
                desc: "Snippet".into(),
                action: "query:cs edit".into(),
                args: None,
            },
        ]
    }
}

#[cfg(test)]
mod persistence_tests {
    use super::*;
    use std::sync::Barrier;

    static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    fn snippet(alias: &str, text: &str) -> SnippetEntry {
        SnippetEntry {
            alias: alias.into(),
            text: text.into(),
            hide_contents: false,
            prompt_for_fields: false,
            fields: Vec::new(),
        }
    }

    fn prompted_snippet(alias: &str, text: &str) -> SnippetEntry {
        SnippetEntry {
            alias: alias.into(),
            text: text.into(),
            hide_contents: true,
            prompt_for_fields: true,
            fields: vec![SnippetFieldDefinition {
                name: "name".into(),
                label: "Preferred name".into(),
                default_value: "Ada".into(),
                required: false,
                input_kind: SnippetInputKind::Multiline,
            }],
        }
    }

    #[test]
    fn preview_masks_hidden_body_without_changing_saved_text() {
        let entry = SnippetEntry {
            alias: "private".into(),
            text: "private body\n秘密 🧪".into(),
            hide_contents: true,
            prompt_for_fields: false,
            fields: Vec::new(),
        };

        assert_eq!(snippet_preview_text(&entry), "******");
        assert_eq!(entry.text, "private body\n秘密 🧪");
    }

    #[test]
    fn snippet_run_action_round_trips_alias_bytes_and_rejects_invalid_encoding() {
        for alias in [
            "simple",
            " leading and trailing ",
            "a:b|c%d",
            "line\nnext\r\t\0秘密🧪",
        ] {
            let action = snippet_run_action(alias);
            assert!(action.bytes().all(|byte| {
                byte.is_ascii_alphanumeric()
                    || matches!(byte, b'-' | b'_' | b'.' | b'~' | b':' | b'%')
            }));
            assert_eq!(decode_snippet_run_action(&action).as_deref(), Some(alias));
        }
        for malformed in [
            "snippet:run:",
            "snippet:run:%",
            "snippet:run:%0",
            "snippet:run:%GG",
            "snippet:run:%C3%28",
            "snippet:run:literal:colon",
            "snippet:run:raw space",
        ] {
            assert_eq!(decode_snippet_run_action(malformed), None, "{malformed}");
        }
    }

    #[test]
    fn run_resolution_uses_exact_alias_and_current_file_snapshot() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let path = path.to_str().unwrap();

        save_snippets(path, &[snippet("Case", "old body")]).unwrap();
        assert_eq!(
            resolve_snippet_from(path, "case"),
            Err(SnippetResolutionError::MissingOrAmbiguous)
        );
        assert_eq!(resolve_snippet_from(path, "Case").unwrap().text, "old body");

        save_snippets(path, &[snippet("Case", "new body")]).unwrap();
        assert_eq!(resolve_snippet_from(path, "Case").unwrap().text, "new body");

        save_snippets(path, &[snippet("Case", "one"), snippet("Case", "two")]).unwrap();
        assert_eq!(
            resolve_snippet_from(path, "Case"),
            Err(SnippetResolutionError::MissingOrAmbiguous)
        );
    }

    #[test]
    fn preview_normalizes_whitespace_and_controls_without_changing_saved_text() {
        let original = "  first\t\r\nsecond\u{0000}\u{001b}   third\u{0085} ";
        let entry = snippet("normal", original);

        assert_eq!(snippet_preview_text(&entry), "first second third");
        assert_eq!(entry.text, original);
    }

    #[test]
    fn typed_load_and_pretty_schema_cover_all_file_states() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("missing.json");
        assert_eq!(load_snippets_typed(&missing).unwrap(), LoadState::Missing);
        let empty = directory.path().join("empty.json");
        std::fs::write(&empty, " \r\n\t").unwrap();
        assert_eq!(load_snippets_typed(&empty).unwrap(), LoadState::Empty);
        let expected = vec![snippet("multi", "first\nsecond")];
        let valid = directory.path().join("valid.json");
        std::fs::write(&valid, serde_json::to_vec(&expected).unwrap()).unwrap();
        assert_eq!(
            load_snippets_typed(&valid).unwrap(),
            LoadState::Loaded(expected.clone())
        );
        let saved = directory.path().join("saved.json");
        save_snippets(saved.to_str().unwrap(), &expected).unwrap();
        assert_eq!(
            std::fs::read_to_string(saved).unwrap(),
            serde_json::to_string_pretty(&expected).unwrap()
        );
        let malformed = directory.path().join("malformed.json");
        std::fs::write(&malformed, "{broken").unwrap();
        assert!(matches!(
            load_snippets_typed(&malformed).unwrap_err(),
            PersistenceError::MalformedJson { .. }
        ));
        assert!(matches!(
            load_snippets_typed(directory.path()).unwrap_err(),
            PersistenceError::Read { .. }
        ));
    }

    #[test]
    fn hide_contents_defaults_for_legacy_and_false_is_omitted() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let legacy = directory.path().join("legacy.json");
        std::fs::write(&legacy, r#"[{"alias":"a","text":"first"}]"#).unwrap();
        assert_eq!(
            load_snippets(legacy.to_str().unwrap()).unwrap(),
            vec![snippet("a", "first")]
        );

        let explicit_false = directory.path().join("explicit-false.json");
        std::fs::write(
            &explicit_false,
            r#"[{"alias":"a","text":"first","hide_contents":false}]"#,
        )
        .unwrap();
        assert_eq!(
            load_snippets(explicit_false.to_str().unwrap()).unwrap(),
            vec![snippet("a", "first")]
        );

        let saved = directory.path().join("saved-unmasked.json");
        let expected = vec![snippet("a", "first")];
        save_snippets(saved.to_str().unwrap(), &expected).unwrap();
        let serialized: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&saved).unwrap()).unwrap();
        assert_eq!(serialized.as_array().unwrap().len(), 1);
        assert_eq!(
            serialized[0],
            serde_json::json!({"alias": "a", "text": "first"})
        );
        assert_eq!(load_snippets(saved.to_str().unwrap()).unwrap(), expected);
    }

    #[test]
    fn legacy_and_opt_out_metadata_default_without_rewriting_input() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let legacy = directory.path().join("legacy.json");
        let legacy_bytes = br#"[{"alias":"a","text":"first","future_entry_option":true}]"#;
        std::fs::write(&legacy, legacy_bytes).unwrap();

        let loaded = load_snippets(legacy.to_str().unwrap()).unwrap();
        assert_eq!(loaded, vec![snippet("a", "first")]);
        assert!(!loaded[0].prompt_for_fields);
        assert!(loaded[0].fields.is_empty());
        assert_eq!(std::fs::read(&legacy).unwrap(), legacy_bytes);

        let opt_out = directory.path().join("opt-out.json");
        std::fs::write(
            &opt_out,
            r#"[{"alias":"a","text":"first","prompt_for_fields":false,"fields":[{"name":"name","label":"Name","future_field_option":"ignored"}]}]"#,
        )
        .unwrap();
        let loaded_opt_out = load_snippets(opt_out.to_str().unwrap()).unwrap();
        assert!(!loaded_opt_out[0].prompt_for_fields);
        assert_eq!(
            loaded_opt_out[0].fields[0],
            SnippetFieldDefinition::new("name")
        );

        let saved_opt_out = directory.path().join("opt-out-saved.json");
        save_snippets(saved_opt_out.to_str().unwrap(), &loaded_opt_out).unwrap();
        assert_eq!(
            load_snippets(saved_opt_out.to_str().unwrap()).unwrap(),
            loaded_opt_out
        );
    }

    #[test]
    fn prompted_metadata_round_trips_with_defaults_for_omitted_options_and_unknown_fields() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("prompted-source.json");
        std::fs::write(
            &source,
            r#"[{"alias":"reply","text":"Hi {{name}}: {{details}}","prompt_for_fields":true,"fields":[{"name":"name","label":"Name","future_field_option":"ignored"},{"name":"details","label":"Details","default":"line 1\nline 2","required":false,"input_kind":"multiline","future_field_option":"ignored"},{"name":"missing_label","future_field_option":"ignored"},{"name":"_"}],"future_entry_option":"ignored"}]"#,
        )
        .unwrap();

        let loaded = load_snippets(source.to_str().unwrap()).unwrap();
        let first = &loaded[0].fields[0];
        assert_eq!(first.name, "name");
        assert_eq!(first.label, "Name");
        assert!(first.default_value.is_empty());
        assert!(first.required);
        assert_eq!(first.input_kind, SnippetInputKind::SingleLine);

        let second = &loaded[0].fields[1];
        assert_eq!(second.name, "details");
        assert_eq!(second.default_value, "line 1\nline 2");
        assert!(!second.required);
        assert_eq!(second.input_kind, SnippetInputKind::Multiline);

        let missing_label = &loaded[0].fields[2];
        assert!(missing_label.label.is_empty());
        assert_eq!(missing_label.display_label(), "Missing Label");
        let underscore_label = &loaded[0].fields[3];
        assert!(underscore_label.label.is_empty());
        assert_eq!(underscore_label.display_label(), "_");

        let saved = directory.path().join("prompted-saved.json");
        save_snippets(saved.to_str().unwrap(), &loaded).unwrap();
        let serialized: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&saved).unwrap()).unwrap();
        assert_eq!(serialized[0]["prompt_for_fields"].as_bool(), Some(true));
        assert_eq!(serialized[0]["fields"][0]["input_kind"], "single_line");
        assert_eq!(serialized[0]["fields"][1]["input_kind"], "multiline");
        assert_eq!(load_snippets(saved.to_str().unwrap()).unwrap(), loaded);
    }

    #[test]
    fn field_constructor_and_entry_equality_include_prompt_configuration() {
        let field = SnippetFieldDefinition::new("ticket_id");
        assert_eq!(field.name, "ticket_id");
        assert_eq!(field.label, "Ticket Id");
        assert_eq!(field.display_label(), "Ticket Id");
        assert!(field.default_value.is_empty());
        assert!(field.required);
        assert_eq!(field.input_kind, SnippetInputKind::SingleLine);

        let plain = snippet("reply", "Hi {{name}}");
        let mut prompted = plain.clone();
        prompted.prompt_for_fields = true;
        prompted.fields.push(SnippetFieldDefinition::new("name"));
        assert_ne!(plain, prompted);

        let mut differently_configured = prompted.clone();
        differently_configured.fields[0].label = "Your name".into();
        assert_ne!(prompted, differently_configured);
    }

    #[test]
    fn field_reconciliation_preserves_matching_options_orders_new_keys_and_omits_orphans() {
        let configured = SnippetFieldDefinition {
            name: "name".into(),
            label: "Preferred name".into(),
            default_value: "Ada".into(),
            required: false,
            input_kind: SnippetInputKind::Multiline,
        };
        let orphan = SnippetFieldDefinition::new("removed");
        let existing = vec![configured.clone(), orphan.clone()];
        let discovered_keys = vec!["ticket_id".to_owned(), "name".to_owned()];

        let reconciled = reconcile_snippet_fields(&existing, &discovered_keys);

        assert_eq!(reconciled[0], SnippetFieldDefinition::new("ticket_id"));
        assert_eq!(reconciled[1], configured);
        assert!(!reconciled.iter().any(|field| field.name == "removed"));
        assert_eq!(existing, vec![reconciled[1].clone(), orphan]);
    }

    #[test]
    fn runtime_preparation_normalizes_discovered_fields_without_mutating_config() {
        let configured_name = SnippetFieldDefinition {
            name: "name".into(),
            label: "Preferred name".into(),
            default_value: "Ada".into(),
            required: false,
            input_kind: SnippetInputKind::Multiline,
        };
        let configured = vec![
            configured_name.clone(),
            SnippetFieldDefinition::new("orphan"),
        ];

        let prepared = prepare_prompted_template("{{new_key}}/{{name}}", &configured).unwrap();

        assert_eq!(prepared.parsed.field_keys, vec!["new_key", "name"]);
        assert_eq!(
            prepared.fields,
            vec![SnippetFieldDefinition::new("new_key"), configured_name]
        );
        assert_eq!(configured[0].label, "Preferred name");
        assert_eq!(configured[0].default_value, "Ada");
        assert_eq!(configured[0].input_kind, SnippetInputKind::Multiline);
        assert_eq!(configured[1].name, "orphan");
    }

    #[test]
    fn runtime_preparation_rejects_duplicate_config_before_reconciliation() {
        let duplicate = vec![
            SnippetFieldDefinition::new("name"),
            SnippetFieldDefinition {
                name: "name".into(),
                label: "Second name".into(),
                default_value: String::new(),
                required: false,
                input_kind: SnippetInputKind::Multiline,
            },
        ];

        assert!(matches!(
            prepare_prompted_template("{{new_key}}", &duplicate),
            Err(SnippetPreparationError::DuplicateFieldDefinitions)
        ));
    }

    #[test]
    fn prompted_append_reconciles_fields_only_for_the_committed_text() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let path = path.to_str().unwrap();
        let configured_name = SnippetFieldDefinition {
            name: "name".into(),
            label: "Preferred name".into(),
            default_value: "Ada".into(),
            required: false,
            input_kind: SnippetInputKind::Multiline,
        };
        let original = SnippetEntry {
            alias: "configured".into(),
            text: "Hello {{name}} and {{removed}}".into(),
            hide_contents: true,
            prompt_for_fields: true,
            fields: vec![
                configured_name.clone(),
                SnippetFieldDefinition::new("removed"),
            ],
        };
        save_snippets(path, std::slice::from_ref(&original)).unwrap();
        let original_bytes = std::fs::read(path).unwrap();
        let version = snippets_version();

        append_snippet(path, "configured", "{{new_key}} then {{name}}").unwrap();

        let committed = load_snippets(path).unwrap();
        assert_eq!(committed[0].text, "{{new_key}} then {{name}}");
        assert!(committed[0].hide_contents);
        assert!(committed[0].prompt_for_fields);
        assert_eq!(
            committed[0].fields,
            vec![SnippetFieldDefinition::new("new_key"), configured_name]
        );
        assert_ne!(std::fs::read(path).unwrap(), original_bytes);
        assert_eq!(snippets_version(), version + 1);
    }

    #[test]
    fn invalid_prompted_append_preserves_file_snapshot_and_version() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let path = path.to_str().unwrap();
        let original = prompted_snippet("configured", "Hello {{name}}");
        save_snippets(path, std::slice::from_ref(&original)).unwrap();
        let original_bytes = std::fs::read(path).unwrap();
        let version = snippets_version();

        for invalid_text in ["PRIVATE {{", "literal only", "unexpected }}"] {
            let error = append_snippet(path, "configured", invalid_text).unwrap_err();
            assert!(!error.to_string().contains(invalid_text));
            assert_eq!(std::fs::read(path).unwrap(), original_bytes);
            assert_eq!(load_snippets(path).unwrap(), vec![original.clone()]);
            assert_eq!(snippets_version(), version);
        }
    }

    #[test]
    fn duplicate_prompted_field_definitions_are_not_dropped_during_reconciliation() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let path = path.to_str().unwrap();
        let mut original = prompted_snippet("configured", "Hello {{name}}");
        original.fields.push(SnippetFieldDefinition {
            name: "name".into(),
            label: "Second definition".into(),
            default_value: "ignored".into(),
            required: true,
            input_kind: SnippetInputKind::SingleLine,
        });
        save_snippets(path, std::slice::from_ref(&original)).unwrap();
        let original_bytes = std::fs::read(path).unwrap();
        let version = snippets_version();

        assert!(append_snippet(path, "configured", "Updated {{name}}").is_err());

        assert_eq!(std::fs::read(path).unwrap(), original_bytes);
        assert_eq!(load_snippets(path).unwrap(), vec![original]);
        assert_eq!(snippets_version(), version);
    }

    #[test]
    fn unprompted_new_and_updated_snippets_treat_braces_as_literal_text() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let path = path.to_str().unwrap();
        let literal = r"{{invalid key}} }} \{{unfinished";

        append_snippet(path, "plain", literal).unwrap();
        let after_first_write = std::fs::read(path).unwrap();
        let version = snippets_version();
        append_snippet(path, "plain", literal).unwrap();

        let saved = load_snippets(path).unwrap();
        assert_eq!(saved, vec![snippet("plain", literal)]);
        assert!(!saved[0].prompt_for_fields);
        assert!(saved[0].fields.is_empty());
        assert_eq!(std::fs::read(path).unwrap(), after_first_write);
        assert_eq!(snippets_version(), version);
    }

    #[test]
    fn hidden_multiline_unicode_snippet_round_trips_through_save_and_load() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let expected = vec![SnippetEntry {
            alias: "résumé-茶☕".into(),
            text: "first line\r\n第二行\nemoji 🧪 and café".into(),
            hide_contents: true,
            prompt_for_fields: false,
            fields: Vec::new(),
        }];

        save_snippets(path.to_str().unwrap(), &expected).unwrap();

        let serialized: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(serialized.as_array().unwrap().len(), 1);
        assert_eq!(
            serialized[0]["alias"].as_str(),
            Some(expected[0].alias.as_str())
        );
        assert_eq!(
            serialized[0]["text"].as_str(),
            Some(expected[0].text.as_str())
        );
        assert_eq!(serialized[0]["hide_contents"].as_bool(), Some(true));
        assert_eq!(load_snippets(path.to_str().unwrap()).unwrap(), expected);
    }

    #[test]
    fn malformed_file_rejects_add_edit_remove_and_replacement_unchanged() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let invalid = b"not snippets JSON";
        std::fs::write(&path, invalid).unwrap();
        let path = path.to_str().unwrap();
        for result in [
            append_snippet(path, "new", "text"),
            append_snippet(path, "existing", "edited"),
            remove_snippet(path, "existing"),
            save_snippets(path, &[snippet("replacement", "lost")]),
        ] {
            assert!(result.is_err());
            assert_eq!(std::fs::read(path).unwrap(), invalid);
        }
    }

    #[test]
    fn same_content_append_preserves_file_bytes_and_version() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let original_bytes = br#"[ { "alias": "configured", "text": "Hello {{name}}", "hide_contents": true, "prompt_for_fields": true, "fields": [{ "name": "name", "label": "Name", "default": "Ada", "required": true, "input_kind": "single_line" }] } ]"#;
        std::fs::write(&path, original_bytes).unwrap();
        let path = path.to_str().unwrap();
        let expected = load_snippets(path).unwrap();
        let version = snippets_version();

        append_snippet(path, "configured", "Hello {{name}}").unwrap();

        assert_eq!(std::fs::read(path).unwrap(), original_bytes);
        assert_eq!(snippets_version(), version);
        assert_eq!(load_snippets(path).unwrap(), expected);
    }

    #[test]
    fn metadata_only_update_publishes_and_reload_does_not_double_bump() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let path_text = path.to_str().unwrap();
        let initial = vec![prompted_snippet("configured", "Hello {{name}}")];
        std::fs::write(&path, serde_json::to_vec_pretty(&initial).unwrap()).unwrap();
        let stale_data = Arc::new(Mutex::new(initial.clone()));
        let plugin = SnippetsPlugin::new_for_path(path_text);
        let mut updated = initial.clone();
        updated[0].fields[0].label = "Display name".into();
        let before_update = snippets_version();

        save_snippets(path_text, &updated).unwrap();

        let after_update = snippets_version();
        assert_eq!(after_update, before_update + 1);
        assert_eq!(load_snippets(path_text).unwrap(), updated);
        assert_eq!(*plugin.data.lock().unwrap(), updated);
        reload_snippet_snapshot(path_text, &stale_data).unwrap();
        assert_eq!(*stale_data.lock().unwrap(), updated);
        assert_eq!(snippets_version(), after_update);
    }

    #[test]
    fn concurrent_mutations_both_survive() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = Arc::new(
            directory
                .path()
                .join("snippets.json")
                .to_string_lossy()
                .into_owned(),
        );
        let configured = prompted_snippet("configured", "original");
        save_snippets(&path, std::slice::from_ref(&configured)).unwrap();
        let barrier = Arc::new(Barrier::new(3));
        let first = {
            let path = Arc::clone(&path);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                append_snippet(&path, "first", "one").unwrap();
            })
        };
        let second = {
            let path = Arc::clone(&path);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                append_snippet(&path, "second", "two").unwrap();
            })
        };
        barrier.wait();
        first.join().unwrap();
        second.join().unwrap();
        let committed = load_snippets(&path).unwrap();
        assert!(committed.contains(&configured));
        assert!(committed.contains(&snippet("first", "one")));
        assert!(committed.contains(&snippet("second", "two")));
    }

    #[test]
    fn failed_save_retains_destination_and_version() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let path_text = path.to_str().unwrap();
        let original = vec![prompted_snippet("saved", "value")];
        std::fs::write(&path, serde_json::to_vec_pretty(&original).unwrap()).unwrap();
        let plugin = SnippetsPlugin::new_for_path(path_text);
        let version = snippets_version();
        let result = update_snippets_with_save(
            path_text,
            |snippets| {
                snippets.push(prompted_snippet("lost", "value"));
                Ok(true)
            },
            |_path, _snippets| anyhow::bail!("deterministic replacement failure"),
        );
        assert!(result.is_err());
        assert_eq!(
            std::fs::read(path_text).unwrap(),
            serde_json::to_vec_pretty(&original).unwrap()
        );
        assert_eq!(load_snippets(path_text).unwrap(), original);
        assert_eq!(*plugin.data.lock().unwrap(), original);
        assert_eq!(snippets_version(), version);
    }

    #[test]
    fn watcher_retains_invalid_recovers_and_does_not_double_bump_local_save() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let path_text = path.to_str().unwrap();
        let initial = vec![snippet("initial", "value")];
        std::fs::write(&path, serde_json::to_vec_pretty(&initial).unwrap()).unwrap();
        let data = Arc::new(Mutex::new(initial.clone()));
        let local = vec![snippet("local", "value")];
        save_snippets(path_text, &local).unwrap();
        let after_local = snippets_version();
        reload_snippet_snapshot(path_text, &data).unwrap();
        assert_eq!(*data.lock().unwrap(), local);
        assert_eq!(snippets_version(), after_local);

        std::fs::write(&path, "invalid").unwrap();
        assert!(reload_snippet_snapshot(path_text, &data).is_err());
        assert_eq!(*data.lock().unwrap(), local);
        assert_eq!(snippets_version(), after_local);

        std::fs::remove_file(&path).unwrap();
        assert!(reload_snippet_snapshot(path_text, &data).is_err());
        assert_eq!(*data.lock().unwrap(), local);
        assert_eq!(snippets_version(), after_local);

        let mut external = local.clone();
        external[0].prompt_for_fields = true;
        external[0].fields = vec![SnippetFieldDefinition {
            name: "external".into(),
            label: "External edit".into(),
            default_value: "configured default".into(),
            required: false,
            input_kind: SnippetInputKind::Multiline,
        }];
        std::fs::write(&path, serde_json::to_vec_pretty(&external).unwrap()).unwrap();
        reload_snippet_snapshot(path_text, &data).unwrap();
        assert_eq!(*data.lock().unwrap(), external);
        assert_eq!(snippets_version(), after_local + 1);
    }

    #[test]
    fn committed_mutation_is_visible_to_all_instances_without_watcher_delivery() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let path = path.to_str().unwrap();
        save_snippets(path, &[]).unwrap();
        let first = SnippetsPlugin::new_for_path(path);
        let second = SnippetsPlugin::new_for_path(path);

        append_snippet(path, "immediate", "published text").unwrap();

        for plugin in [&first, &second] {
            assert!(plugin.search("cs immediate").iter().any(|action| {
                action.label == "immediate" && action.action == snippet_run_action("immediate")
            }));
        }
    }
}
