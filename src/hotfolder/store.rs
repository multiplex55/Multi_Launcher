use super::model::{
    HOT_FOLDER_RULES_FILE, HOT_FOLDER_RULES_FORMAT_VERSION, HotFolderRule, MAX_DOCUMENT_BYTES,
    MAX_QUARANTINED_ROWS, MAX_RULES,
};
use crate::{common::atomic_file::save_atomic, platform::app_data::AppDataRoot};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fmt,
    fs::File,
    io::{self, Read},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

const MAX_QUARANTINE_DIAGNOSTIC_CHARS: usize = 512;

/// One unrecognized or invalid source row retained for user recovery.
///
/// The raw value is intentionally omitted from Debug because it may include
/// launcher parameters or user paths.
#[derive(Clone, PartialEq)]
pub struct QuarantinedRule {
    pub raw: Value,
    pub diagnostic: String,
}

impl fmt::Debug for QuarantinedRule {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("QuarantinedRule")
            .field("raw", &"[redacted]")
            .field("diagnostic", &self.diagnostic)
            .finish()
    }
}

#[derive(Clone, PartialEq)]
pub struct HotFolderRulesSnapshot {
    pub revision: u64,
    pub rules: Vec<HotFolderRule>,
    pub quarantined_rows: Vec<QuarantinedRule>,
    write_block_reason: Option<String>,
}

impl HotFolderRulesSnapshot {
    fn empty() -> Self {
        Self {
            revision: 0,
            rules: Vec::new(),
            quarantined_rows: Vec::new(),
            write_block_reason: None,
        }
    }

    pub(crate) fn for_preview(rule: HotFolderRule) -> Self {
        Self {
            revision: 0,
            rules: vec![rule],
            quarantined_rows: Vec::new(),
            write_block_reason: None,
        }
    }

    pub fn is_write_blocked(&self) -> bool {
        self.write_block_reason.is_some()
    }

    pub fn write_block_reason(&self) -> Option<&str> {
        self.write_block_reason.as_deref()
    }
}

impl fmt::Debug for HotFolderRulesSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HotFolderRulesSnapshot")
            .field("revision", &self.revision)
            .field("rule_count", &self.rules.len())
            .field("quarantined_count", &self.quarantined_rows.len())
            .field("write_blocked", &self.is_write_blocked())
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotFolderLoadStatus {
    Missing,
    Loaded,
    LoadedWithQuarantinedRows,
    EmptyDocumentRetained,
    MalformedDocumentRetained,
    UnsupportedVersionRetained,
    OversizedDocumentRetained,
    UnavailableRetained,
}

#[derive(Clone, Debug)]
pub struct HotFolderLoadReport {
    pub status: HotFolderLoadStatus,
    pub snapshot: HotFolderRulesSnapshot,
    pub diagnostic: Option<String>,
}

#[derive(Debug)]
pub enum HotFolderStoreError {
    Io {
        operation: &'static str,
        source: io::Error,
    },
    NotLoaded,
    StaleRevision {
        expected: u64,
        actual: u64,
    },
    DiskChanged,
    WriteBlocked(String),
    InvalidRule {
        index: usize,
        reason: &'static str,
    },
    DuplicateRuleId,
    TooManyRules,
    TooManyQuarantinedRows,
    DocumentTooLarge,
    RevisionExhausted,
    RuleNotFound,
    Serialize(String),
}

impl fmt::Display for HotFolderStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { operation, source } => write!(
                formatter,
                "failed to {operation} Hot Folder rules: {source}"
            ),
            Self::NotLoaded => {
                formatter.write_str("Hot Folder rules must be loaded before they can be saved")
            }
            Self::StaleRevision { expected, actual } => write!(
                formatter,
                "Hot Folder rules revision changed (expected {expected}, current {actual})"
            ),
            Self::DiskChanged => {
                formatter.write_str("Hot Folder rules changed on disk; reload before saving")
            }
            Self::WriteBlocked(reason) => {
                write!(formatter, "Hot Folder rules cannot be saved: {reason}")
            }
            Self::InvalidRule { index, reason } => write!(
                formatter,
                "Hot Folder rule at index {index} is invalid: {reason}"
            ),
            Self::DuplicateRuleId => formatter.write_str("Hot Folder rule IDs must be unique"),
            Self::TooManyRules => {
                formatter.write_str("Hot Folder rule count exceeds the supported limit")
            }
            Self::TooManyQuarantinedRows => {
                formatter.write_str("Hot Folder quarantine count exceeds the supported limit")
            }
            Self::DocumentTooLarge => {
                formatter.write_str("Hot Folder rules document exceeds the supported size")
            }
            Self::RevisionExhausted => {
                formatter.write_str("Hot Folder rules revision is exhausted")
            }
            Self::RuleNotFound => formatter.write_str("Hot Folder rule no longer exists"),
            Self::Serialize(message) => {
                write!(formatter, "failed to serialize Hot Folder rules: {message}")
            }
        }
    }
}

impl std::error::Error for HotFolderStoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DiskBaseline {
    Unknown,
    Missing,
    Bytes([u8; 32]),
}

struct StoreState {
    snapshot: HotFolderRulesSnapshot,
    status: Option<HotFolderLoadStatus>,
    baseline: DiskBaseline,
    last_good_raw: Option<Vec<u8>>,
    last_observed_raw: Option<Vec<u8>>,
}

/// Read-only until one of the explicit CRUD methods is called. Construction
/// records the app-data path only; it never reads, creates, or activates files.
pub struct HotFolderRulesStore {
    path: PathBuf,
    state: Mutex<StoreState>,
}

impl HotFolderRulesStore {
    pub fn new(root: &AppDataRoot) -> Self {
        Self {
            path: root.path().join(HOT_FOLDER_RULES_FILE),
            state: Mutex::new(StoreState {
                snapshot: HotFolderRulesSnapshot::empty(),
                status: None,
                baseline: DiskBaseline::Unknown,
                last_good_raw: None,
                last_observed_raw: None,
            }),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Inspect current bytes without creating or rewriting the store. Invalid
    /// whole documents preserve the in-memory last-good snapshot and block
    /// writes until a valid version is explicitly loaded.
    pub fn load(&self) -> HotFolderLoadReport {
        let transaction = path_transaction(&self.path);
        let _transaction = transaction
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match read_limited(&self.path) {
            Ok(ReadDocument::Missing) => self.record_missing(),
            Ok(ReadDocument::Bytes(bytes)) => self.record_bytes(bytes),
            Ok(ReadDocument::Oversized) => self.record_whole_document_error(
                HotFolderLoadStatus::OversizedDocumentRetained,
                "Hot Folder rules document exceeds the supported size".into(),
                None,
            ),
            Err(source) => self.record_whole_document_error(
                HotFolderLoadStatus::UnavailableRetained,
                "Hot Folder rules could not be read".into(),
                Some(HotFolderStoreError::Io {
                    operation: "read",
                    source,
                }),
            ),
        }
    }

    pub fn snapshot(&self) -> HotFolderRulesSnapshot {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .snapshot
            .clone()
    }

    pub fn load_status(&self) -> Option<HotFolderLoadStatus> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .status
    }

    pub fn save_rules(
        &self,
        expected_revision: u64,
        rules: Vec<HotFolderRule>,
    ) -> Result<HotFolderRulesSnapshot, HotFolderStoreError> {
        self.commit_rules(expected_revision, rules)
    }

    pub fn add_rule(
        &self,
        expected_revision: u64,
        mut rule: HotFolderRule,
    ) -> Result<HotFolderRulesSnapshot, HotFolderStoreError> {
        rule.enabled = false;
        let mut rules = self.rules_for_revision(expected_revision)?;
        if rules.iter().any(|existing| existing.id == rule.id) {
            return Err(HotFolderStoreError::DuplicateRuleId);
        }
        rules.push(rule);
        self.commit_rules(expected_revision, rules)
    }

    pub fn replace_rule(
        &self,
        expected_revision: u64,
        rule: HotFolderRule,
    ) -> Result<HotFolderRulesSnapshot, HotFolderStoreError> {
        let mut rules = self.rules_for_revision(expected_revision)?;
        let existing = rules
            .iter_mut()
            .find(|existing| existing.id == rule.id)
            .ok_or(HotFolderStoreError::RuleNotFound)?;
        *existing = rule;
        self.commit_rules(expected_revision, rules)
    }

    pub fn remove_rule(
        &self,
        expected_revision: u64,
        id: &super::model::RuleId,
    ) -> Result<HotFolderRulesSnapshot, HotFolderStoreError> {
        let mut rules = self.rules_for_revision(expected_revision)?;
        let old_len = rules.len();
        rules.retain(|rule| &rule.id != id);
        if rules.len() == old_len {
            return Err(HotFolderStoreError::RuleNotFound);
        }
        self.commit_rules(expected_revision, rules)
    }

    fn rules_for_revision(
        &self,
        expected_revision: u64,
    ) -> Result<Vec<HotFolderRule>, HotFolderStoreError> {
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        ensure_revision(&state.snapshot, expected_revision)?;
        ensure_writable(&state)?;
        Ok(state.snapshot.rules.clone())
    }

    fn commit_rules(
        &self,
        expected_revision: u64,
        mut rules: Vec<HotFolderRule>,
    ) -> Result<HotFolderRulesSnapshot, HotFolderStoreError> {
        let transaction = path_transaction(&self.path);
        let _transaction = transaction
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        ensure_revision(&state.snapshot, expected_revision)?;
        ensure_writable(&state)?;
        if state.snapshot.quarantined_rows.len() > MAX_QUARANTINED_ROWS {
            return Err(HotFolderStoreError::TooManyQuarantinedRows);
        }
        let existing_rules = state
            .snapshot
            .rules
            .iter()
            .map(|rule| (rule.id.as_str().to_owned(), rule.clone()))
            .collect::<HashMap<_, _>>();
        let quarantined_ids = state
            .snapshot
            .quarantined_rows
            .iter()
            .filter_map(|row| {
                row.raw
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .collect::<std::collections::HashSet<_>>();
        for rule in &mut rules {
            if quarantined_ids.contains(rule.id.as_str()) {
                return Err(HotFolderStoreError::DuplicateRuleId);
            }
            let Some(existing) = existing_rules.get(rule.id.as_str()) else {
                rule.enabled = false;
                continue;
            };
            let mut existing_definition = existing.clone();
            existing_definition.enabled = false;
            let mut candidate_definition = rule.clone();
            candidate_definition.enabled = false;
            if candidate_definition != existing_definition {
                rule.enabled = false;
            }
        }
        validate_rules(&rules)?;
        match (&state.baseline, read_limited(&self.path)) {
            (DiskBaseline::Missing, Ok(ReadDocument::Missing)) => {}
            (DiskBaseline::Bytes(expected), Ok(ReadDocument::Bytes(current)))
                if *expected == digest(&current) => {}
            (DiskBaseline::Unknown, _) => return Err(HotFolderStoreError::NotLoaded),
            (_, Err(source)) => {
                return Err(HotFolderStoreError::Io {
                    operation: "check before save",
                    source,
                });
            }
            (_, Ok(ReadDocument::Oversized)) => return Err(HotFolderStoreError::DiskChanged),
            _ => return Err(HotFolderStoreError::DiskChanged),
        }

        let revision = state
            .snapshot
            .revision
            .checked_add(1)
            .ok_or(HotFolderStoreError::RevisionExhausted)?;
        let document = StoredDocument {
            format_version: HOT_FOLDER_RULES_FORMAT_VERSION,
            revision,
            rules: rules.clone(),
            quarantined_rows: state
                .snapshot
                .quarantined_rows
                .iter()
                .map(|row| QuarantinedRuleDocument {
                    raw: row.raw.clone(),
                    diagnostic: row.diagnostic.clone(),
                })
                .collect(),
        };
        let bytes = serde_json::to_vec_pretty(&document)
            .map_err(|error| HotFolderStoreError::Serialize(error.to_string()))?;
        if bytes.len() > MAX_DOCUMENT_BYTES {
            return Err(HotFolderStoreError::DocumentTooLarge);
        }
        save_atomic(&self.path, &bytes).map_err(|source| HotFolderStoreError::Io {
            operation: "atomically save",
            source: io::Error::other(source.to_string()),
        })?;

        let snapshot = HotFolderRulesSnapshot {
            revision,
            rules,
            quarantined_rows: state.snapshot.quarantined_rows.clone(),
            write_block_reason: None,
        };
        state.snapshot = snapshot.clone();
        state.status = Some(if snapshot.quarantined_rows.is_empty() {
            HotFolderLoadStatus::Loaded
        } else {
            HotFolderLoadStatus::LoadedWithQuarantinedRows
        });
        state.baseline = DiskBaseline::Bytes(digest(&bytes));
        state.last_good_raw = Some(bytes.clone());
        state.last_observed_raw = Some(bytes);
        Ok(snapshot)
    }

    fn record_missing(&self) -> HotFolderLoadReport {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.last_good_raw.is_none()
            && state.last_observed_raw.is_none()
            && state.snapshot.revision == 0
        {
            state.snapshot = HotFolderRulesSnapshot::empty();
            state.snapshot.write_block_reason = None;
            state.baseline = DiskBaseline::Missing;
            state.status = Some(HotFolderLoadStatus::Missing);
            state.last_observed_raw = None;
            return HotFolderLoadReport {
                status: HotFolderLoadStatus::Missing,
                snapshot: state.snapshot.clone(),
                diagnostic: None,
            };
        }
        let report = retain_last_good(
            &mut state,
            HotFolderLoadStatus::UnavailableRetained,
            "Hot Folder rules file disappeared after a valid snapshot was loaded".into(),
            None,
        );
        report
    }

    fn record_bytes(&self, bytes: Vec<u8>) -> HotFolderLoadReport {
        let decoded = decode_document(&bytes);
        match decoded {
            Ok(decoded) => {
                let mut state = self
                    .state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let status = if decoded.quarantined_rows.is_empty() {
                    HotFolderLoadStatus::Loaded
                } else {
                    HotFolderLoadStatus::LoadedWithQuarantinedRows
                };
                state.snapshot = HotFolderRulesSnapshot {
                    revision: decoded.revision,
                    rules: decoded.rules,
                    quarantined_rows: decoded.quarantined_rows,
                    write_block_reason: None,
                };
                state.status = Some(status);
                state.baseline = DiskBaseline::Bytes(digest(&bytes));
                state.last_good_raw = Some(bytes.clone());
                state.last_observed_raw = Some(bytes);
                HotFolderLoadReport {
                    status,
                    snapshot: state.snapshot.clone(),
                    diagnostic: None,
                }
            }
            Err(decode_error) => self.record_whole_document_error(
                decode_error.status(),
                decode_error.message(),
                None,
            ),
        }
    }

    fn record_whole_document_error(
        &self,
        status: HotFolderLoadStatus,
        diagnostic: String,
        error: Option<HotFolderStoreError>,
    ) -> HotFolderLoadReport {
        let retained_raw = error.is_none();
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if retained_raw {
            state.last_observed_raw = read_limited(&self.path).ok().and_then(|read| match read {
                ReadDocument::Bytes(bytes) => Some(bytes),
                ReadDocument::Missing | ReadDocument::Oversized => None,
            });
        }
        let report = retain_last_good(&mut state, status, diagnostic, error);
        report
    }
}

fn retain_last_good(
    state: &mut StoreState,
    status: HotFolderLoadStatus,
    diagnostic: String,
    _error: Option<HotFolderStoreError>,
) -> HotFolderLoadReport {
    let message = truncate_diagnostic(diagnostic);
    state.snapshot.write_block_reason = Some(message.clone());
    state.status = Some(status);
    HotFolderLoadReport {
        status,
        snapshot: state.snapshot.clone(),
        diagnostic: Some(message),
    }
}

fn ensure_revision(
    snapshot: &HotFolderRulesSnapshot,
    expected: u64,
) -> Result<(), HotFolderStoreError> {
    if expected == snapshot.revision {
        Ok(())
    } else {
        Err(HotFolderStoreError::StaleRevision {
            expected,
            actual: snapshot.revision,
        })
    }
}

fn ensure_writable(state: &StoreState) -> Result<(), HotFolderStoreError> {
    if let Some(reason) = &state.snapshot.write_block_reason {
        return Err(HotFolderStoreError::WriteBlocked(reason.clone()));
    }
    if state.baseline == DiskBaseline::Unknown {
        return Err(HotFolderStoreError::NotLoaded);
    }
    Ok(())
}

fn validate_rules(rules: &[HotFolderRule]) -> Result<(), HotFolderStoreError> {
    if rules.len() > MAX_RULES {
        return Err(HotFolderStoreError::TooManyRules);
    }
    let mut ids = std::collections::HashSet::with_capacity(rules.len());
    for (index, rule) in rules.iter().enumerate() {
        rule.validate()
            .map_err(|reason| HotFolderStoreError::InvalidRule { index, reason })?;
        if !ids.insert(rule.id.as_str()) {
            return Err(HotFolderStoreError::DuplicateRuleId);
        }
    }
    Ok(())
}

#[derive(Serialize)]
struct StoredDocument {
    format_version: u32,
    revision: u64,
    quarantined_rows: Vec<QuarantinedRuleDocument>,
    rules: Vec<HotFolderRule>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct QuarantinedRuleDocument {
    raw: Value,
    diagnostic: String,
}

struct DecodedDocument {
    revision: u64,
    rules: Vec<HotFolderRule>,
    quarantined_rows: Vec<QuarantinedRule>,
}

#[derive(Debug)]
enum DecodeError {
    Empty,
    Malformed(String),
    UnsupportedVersion(u64),
}

impl DecodeError {
    fn status(&self) -> HotFolderLoadStatus {
        match self {
            Self::Empty => HotFolderLoadStatus::EmptyDocumentRetained,
            Self::Malformed(_) => HotFolderLoadStatus::MalformedDocumentRetained,
            Self::UnsupportedVersion(_) => HotFolderLoadStatus::UnsupportedVersionRetained,
        }
    }

    fn message(&self) -> String {
        match self {
            Self::Empty => "Hot Folder rules document is empty".into(),
            Self::Malformed(message) => message.clone(),
            Self::UnsupportedVersion(version) => {
                format!("Hot Folder rules format version {version} is unsupported")
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HotFolderDocumentProbe {
    Supported,
    UnsupportedVersion(u64),
    Invalid,
}

/// Probe Hot Folder bytes using the same bounded decoder used by the store.
/// Documents with any quarantine are deliberately not recovery-eligible.
pub(crate) fn probe_document_bytes(bytes: &[u8]) -> HotFolderDocumentProbe {
    match decode_document(bytes) {
        Ok(document) if document.quarantined_rows.is_empty() => HotFolderDocumentProbe::Supported,
        Ok(_) | Err(DecodeError::Empty | DecodeError::Malformed(_)) => {
            HotFolderDocumentProbe::Invalid
        }
        Err(DecodeError::UnsupportedVersion(version)) => {
            HotFolderDocumentProbe::UnsupportedVersion(version)
        }
    }
}

fn decode_document(bytes: &[u8]) -> Result<DecodedDocument, DecodeError> {
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Err(DecodeError::Empty);
    }
    if bytes.len() > MAX_DOCUMENT_BYTES {
        return Err(DecodeError::Malformed(
            "Hot Folder rules document exceeds the supported size".into(),
        ));
    }
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|_| DecodeError::Malformed("Hot Folder rules JSON is malformed".into()))?;
    let object = value.as_object().ok_or_else(|| {
        DecodeError::Malformed("Hot Folder rules document must be a JSON object".into())
    })?;
    if object.keys().any(|key| {
        !matches!(
            key.as_str(),
            "format_version" | "revision" | "rules" | "quarantined_rows"
        )
    }) {
        return Err(DecodeError::Malformed(
            "Hot Folder rules document has unknown top-level fields".into(),
        ));
    }
    let version = object
        .get("format_version")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            DecodeError::Malformed("Hot Folder rules version is missing or invalid".into())
        })?;
    if version != u64::from(HOT_FOLDER_RULES_FORMAT_VERSION) {
        return Err(DecodeError::UnsupportedVersion(version));
    }
    let revision = object
        .get("revision")
        .and_then(Value::as_u64)
        .filter(|revision| *revision > 0)
        .ok_or_else(|| {
            DecodeError::Malformed("Hot Folder rules revision is missing or invalid".into())
        })?;
    let source_rows = object
        .get("rules")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            DecodeError::Malformed("Hot Folder rules list is missing or invalid".into())
        })?;
    let source_quarantine = match object.get("quarantined_rows") {
        Some(value) => value.as_array().map(Vec::as_slice).ok_or_else(|| {
            DecodeError::Malformed("Hot Folder quarantine list is invalid".into())
        })?,
        None => &[],
    };
    if source_rows.len() > MAX_RULES || source_quarantine.len() > MAX_QUARANTINED_ROWS {
        return Err(DecodeError::Malformed(
            "Hot Folder rules or quarantine count exceeds the supported limit".into(),
        ));
    }

    let mut id_counts = HashMap::<String, usize>::new();
    for raw in source_rows
        .iter()
        .chain(source_quarantine.iter().filter_map(|row| row.get("raw")))
    {
        if let Some(id) = raw.get("id").and_then(Value::as_str) {
            *id_counts.entry(id.to_owned()).or_default() += 1;
        }
    }

    let mut quarantined_rows = Vec::with_capacity(source_quarantine.len());
    for row in source_quarantine {
        let entry = serde_json::from_value::<QuarantinedRuleDocument>(row.clone())
            .map_err(|_| DecodeError::Malformed("Hot Folder quarantine row is malformed".into()))?;
        if entry.diagnostic.chars().count() > MAX_QUARANTINE_DIAGNOSTIC_CHARS {
            return Err(DecodeError::Malformed(
                "Hot Folder quarantine diagnostic exceeds the supported limit".into(),
            ));
        }
        quarantined_rows.push(QuarantinedRule {
            raw: entry.raw,
            diagnostic: entry.diagnostic,
        });
    }

    let mut rules = Vec::new();
    for raw in source_rows {
        if raw
            .get("id")
            .and_then(Value::as_str)
            .and_then(|id| id_counts.get(id))
            .is_some_and(|count| *count > 1)
        {
            quarantined_rows.push(quarantine(
                raw,
                "duplicate rule ID makes this row ambiguous",
            ));
            continue;
        }
        match serde_json::from_value::<HotFolderRule>(raw.clone()) {
            Ok(rule) => match rule.validate() {
                Ok(()) => rules.push(rule),
                Err(reason) => quarantined_rows.push(quarantine(raw, reason)),
            },
            Err(error) => quarantined_rows.push(quarantine(raw, error.to_string())),
        }
    }
    if quarantined_rows.len() > MAX_QUARANTINED_ROWS {
        return Err(DecodeError::Malformed(
            "Hot Folder quarantine count exceeds the supported limit".into(),
        ));
    }
    Ok(DecodedDocument {
        revision,
        rules,
        quarantined_rows,
    })
}

fn quarantine(raw: &Value, diagnostic: impl AsRef<str>) -> QuarantinedRule {
    QuarantinedRule {
        raw: raw.clone(),
        diagnostic: truncate_diagnostic(diagnostic.as_ref().to_owned()),
    }
}

fn truncate_diagnostic(mut diagnostic: String) -> String {
    if diagnostic.chars().count() > MAX_QUARANTINE_DIAGNOSTIC_CHARS {
        diagnostic = diagnostic
            .chars()
            .take(MAX_QUARANTINE_DIAGNOSTIC_CHARS)
            .collect();
    }
    diagnostic
}

enum ReadDocument {
    Missing,
    Bytes(Vec<u8>),
    Oversized,
}

fn read_limited(path: &Path) -> io::Result<ReadDocument> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(ReadDocument::Missing);
        }
        Err(error) => return Err(error),
    };
    let mut bytes = Vec::new();
    file.take((MAX_DOCUMENT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_DOCUMENT_BYTES {
        return Ok(ReadDocument::Oversized);
    }
    Ok(ReadDocument::Bytes(bytes))
}

fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

static PATH_TRANSACTIONS: OnceLock<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>> = OnceLock::new();

fn path_transaction(path: &Path) -> Arc<Mutex<()>> {
    let path = path.to_path_buf();
    let transactions = PATH_TRANSACTIONS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut transactions = transactions
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    transactions
        .entry(path)
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hotfolder::{RuleAction, RuleId};

    fn fixture() -> (tempfile::TempDir, AppDataRoot, HotFolderRulesStore) {
        let directory = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_path(
            directory
                .path()
                .join("target/productivity_expansion/app_data"),
        );
        let store = HotFolderRulesStore::new(&root);
        (directory, root, store)
    }

    fn rule(name: &str) -> HotFolderRule {
        HotFolderRule::new(
            name,
            std::env::temp_dir().join("target/productivity_expansion/watch/incoming"),
            RuleAction::Copy {
                destination: std::env::temp_dir()
                    .join("target/productivity_expansion/app_data/out"),
            },
        )
    }

    #[test]
    fn explicit_store_crud_quarantines_bad_neighbors_and_rejects_stale_writes() {
        let (_directory, root, store) = fixture();
        let missing = store.load();
        assert_eq!(missing.status, HotFolderLoadStatus::Missing);
        assert!(!store.path().exists());
        let first = store.add_rule(0, rule("first")).unwrap();
        assert_eq!(first.revision, 1);
        assert!(!first.rules[0].enabled);
        let catalog = crate::persistence::PersistenceCatalog::new(
            &root,
            &crate::settings::Settings::default(),
        );
        let descriptor = catalog.get(crate::persistence::PersistentStoreId::HotFolderRules);
        assert_eq!(descriptor.probe(), crate::persistence::StoreHealth::Healthy);
        assert!(matches!(
            store.add_rule(0, rule("stale")),
            Err(HotFolderStoreError::StaleRevision { .. })
        ));
        let enabled = store
            .replace_rule(first.revision, {
                let mut unchanged = first.rules[0].clone();
                unchanged.enabled = true;
                unchanged
            })
            .unwrap();
        assert!(enabled.rules[0].enabled);
        let changed_definition = store
            .replace_rule(enabled.revision, {
                let mut changed = enabled.rules[0].clone();
                changed.action = RuleAction::Move {
                    destination: std::env::temp_dir()
                        .join("target/productivity_expansion/app_data/out"),
                };
                changed.enabled = true;
                changed
            })
            .unwrap();
        assert!(!changed_definition.rules[0].enabled);

        let original = std::fs::read(store.path()).unwrap();
        let mut value: Value = serde_json::from_slice(&original).unwrap();
        value["rules"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "id": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "name": "invalid row",
                "watched_root": "relative/path",
                "action": { "kind": "future_action", "payload": "preserve-me" }
            }));
        let damaged_neighbor = serde_json::to_vec_pretty(&value).unwrap();
        std::fs::write(store.path(), &damaged_neighbor).unwrap();
        let report = store.load();
        assert!(matches!(
            descriptor.probe(),
            crate::persistence::StoreHealth::Malformed { .. }
        ));
        assert_eq!(
            report.status,
            HotFolderLoadStatus::LoadedWithQuarantinedRows
        );
        assert_eq!(report.snapshot.rules.len(), 1);
        assert_eq!(report.snapshot.quarantined_rows.len(), 1);
        assert_eq!(
            report.snapshot.quarantined_rows[0].raw["action"]["payload"],
            "preserve-me"
        );
        let saved = store
            .replace_rule(report.snapshot.revision, {
                let mut replacement = report.snapshot.rules[0].clone();
                replacement.name = "updated".into();
                replacement
            })
            .unwrap();
        let persisted: Value =
            serde_json::from_slice(&std::fs::read(store.path()).unwrap()).unwrap();
        assert_eq!(saved.rules[0].name, "updated");
        assert_eq!(
            persisted["quarantined_rows"][0]["raw"]["action"]["payload"],
            "preserve-me"
        );
        assert!(matches!(
            descriptor.probe(),
            crate::persistence::StoreHealth::Malformed { .. }
        ));
        let bytes_before_conflicting_add = std::fs::read(store.path()).unwrap();
        let mut conflicting = rule("quarantined ID collision");
        conflicting.id = RuleId::parse("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap();
        assert!(matches!(
            store.add_rule(saved.revision, conflicting),
            Err(HotFolderStoreError::DuplicateRuleId)
        ));
        assert_eq!(
            std::fs::read(store.path()).unwrap(),
            bytes_before_conflicting_add
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&std::fs::read(store.path()).unwrap()).unwrap()["revision"],
            saved.revision
        );
        let before_read_only_load = std::fs::read(store.path()).unwrap();
        let reloaded_store = HotFolderRulesStore::new(&root);
        let reloaded = reloaded_store.load();
        assert_eq!(reloaded.snapshot.revision, saved.revision);
        assert_eq!(reloaded.snapshot.rules, saved.rules);
        assert_eq!(std::fs::read(store.path()).unwrap(), before_read_only_load);

        assert_eq!(descriptor.path, root.path().join(HOT_FOLDER_RULES_FILE));
        assert_eq!(
            descriptor.privacy,
            crate::persistence::StorePrivacy::Sensitive
        );
        assert_eq!(
            descriptor.backup_policy,
            crate::persistence::BackupPolicy::Include
        );
        assert!(matches!(
            descriptor.probe(),
            crate::persistence::StoreHealth::Malformed { .. }
        ));
        assert_eq!(std::fs::read(store.path()).unwrap(), before_read_only_load);

        let mut duplicate_id_document: Value =
            serde_json::from_slice(&before_read_only_load).unwrap();
        let duplicate = duplicate_id_document["rules"][0].clone();
        duplicate_id_document["rules"]
            .as_array_mut()
            .unwrap()
            .push(duplicate);
        std::fs::write(
            store.path(),
            serde_json::to_vec(&duplicate_id_document).unwrap(),
        )
        .unwrap();
        let duplicates = store.load();
        assert!(duplicates.snapshot.rules.is_empty());
        assert_eq!(duplicates.snapshot.quarantined_rows.len(), 3);
        assert!(
            duplicates.snapshot.quarantined_rows[1..]
                .iter()
                .all(|row| row.diagnostic.contains("duplicate rule ID"))
        );

        let mut without_quarantine: Value = serde_json::from_slice(&before_read_only_load).unwrap();
        without_quarantine.as_object_mut().unwrap().remove("quarantined_rows");
        let without_quarantine_bytes = serde_json::to_vec(&without_quarantine).unwrap();
        std::fs::write(store.path(), &without_quarantine_bytes).unwrap();
        assert_eq!(descriptor.probe(), crate::persistence::StoreHealth::Healthy);

        let mut malformed_quarantine = without_quarantine.clone();
        malformed_quarantine["quarantined_rows"] = serde_json::json!([{}]);
        std::fs::write(
            store.path(),
            serde_json::to_vec(&malformed_quarantine).unwrap(),
        )
        .unwrap();
        assert!(matches!(
            descriptor.probe(),
            crate::persistence::StoreHealth::Malformed { .. }
        ));

        let mut zero_revision = without_quarantine.clone();
        zero_revision["revision"] = serde_json::json!(0);
        std::fs::write(store.path(), serde_json::to_vec(&zero_revision).unwrap()).unwrap();
        assert!(matches!(
            descriptor.probe(),
            crate::persistence::StoreHealth::Malformed { .. }
        ));

        let mut future = without_quarantine;
        future["format_version"] = serde_json::json!(HOT_FOLDER_RULES_FORMAT_VERSION + 1);
        std::fs::write(store.path(), serde_json::to_vec(&future).unwrap()).unwrap();
        assert!(matches!(
            descriptor.probe(),
            crate::persistence::StoreHealth::UnsupportedSchema { .. }
        ));
        std::fs::write(store.path(), before_read_only_load).unwrap();
    }

    #[test]
    fn malformed_oversized_or_concurrently_changed_store_retains_last_good_and_blocks_save() {
        let (_directory, _root, store) = fixture();
        store.load();
        let saved = store.add_rule(0, rule("good")).unwrap();
        let last_good_bytes = std::fs::read(store.path()).unwrap();

        std::fs::write(store.path(), b"{truncated").unwrap();
        let malformed = store.load();
        assert_eq!(
            malformed.status,
            HotFolderLoadStatus::MalformedDocumentRetained
        );
        assert_eq!(malformed.snapshot.revision, saved.revision);
        assert_eq!(malformed.snapshot.rules, saved.rules);
        assert!(malformed.snapshot.is_write_blocked());
        assert!(matches!(
            store.add_rule(saved.revision, rule("blocked")),
            Err(HotFolderStoreError::WriteBlocked(_))
        ));

        std::fs::write(store.path(), &last_good_bytes).unwrap();
        let restored = store.load();
        assert_eq!(restored.status, HotFolderLoadStatus::Loaded);
        assert!(!restored.snapshot.is_write_blocked());
        let future = serde_json::json!({
            "format_version": HOT_FOLDER_RULES_FORMAT_VERSION + 1,
            "revision": restored.snapshot.revision + 1,
            "rules": [],
            "quarantined_rows": []
        });
        std::fs::write(store.path(), serde_json::to_vec(&future).unwrap()).unwrap();
        let unsupported = store.load();
        assert_eq!(
            unsupported.status,
            HotFolderLoadStatus::UnsupportedVersionRetained
        );
        assert_eq!(unsupported.snapshot.revision, restored.snapshot.revision);
        assert!(unsupported.snapshot.is_write_blocked());
        std::fs::write(store.path(), &last_good_bytes).unwrap();
        let restored = store.load();
        assert!(!restored.snapshot.is_write_blocked());
        let other_store = HotFolderRulesStore::new(&AppDataRoot::from_path(
            store.path().parent().unwrap().to_path_buf(),
        ));
        let other = other_store.load();
        assert_eq!(other.snapshot.revision, restored.snapshot.revision);
        let external = serde_json::json!({
            "format_version": HOT_FOLDER_RULES_FORMAT_VERSION,
            "revision": restored.snapshot.revision + 1,
            "rules": [],
            "quarantined_rows": []
        });
        std::fs::write(store.path(), serde_json::to_vec(&external).unwrap()).unwrap();
        assert!(matches!(
            store.add_rule(restored.snapshot.revision, rule("stale disk")),
            Err(HotFolderStoreError::DiskChanged)
        ));

        std::fs::write(store.path(), vec![b' '; MAX_DOCUMENT_BYTES + 1]).unwrap();
        let oversized = store.load();
        assert_eq!(
            oversized.status,
            HotFolderLoadStatus::OversizedDocumentRetained
        );
        assert!(oversized.snapshot.is_write_blocked());
    }
}
