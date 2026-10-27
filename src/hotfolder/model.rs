use crate::universal_actions::{ActionId, PersistedUniversalActionRef};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::path::{Component, Path, PathBuf};

pub const HOT_FOLDER_RULES_FILE: &str = "hotfolder_rules.json";
pub const LEGACY_HOT_FOLDER_RULES_FORMAT_VERSION: u32 = 1;
pub const HOT_FOLDER_RULES_FORMAT_VERSION: u32 = 2;
pub const MAX_CONTENT_LITERAL_BYTES: usize = 4096;

pub const FILE_CONDITIONS_NOT_EVALUATED: &str =
    "file conditions are not evaluated by this Hot Folder runtime version";

pub const MAX_RULES: usize = 256;
pub const MAX_QUARANTINED_ROWS: usize = 256;
pub const MAX_DOCUMENT_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_RULE_NAME_CHARS: usize = 128;
pub const MAX_FILENAME_GLOB_CHARS: usize = 128;
pub const MAX_EXTENSIONS: usize = 64;
pub const MAX_EXTENSION_CHARS: usize = 32;
pub const MAX_RENAME_PART_CHARS: usize = 64;
pub const MAX_EXTERNAL_ACTION_TEXT_CHARS: usize = 4096;
pub const MAX_RULE_PATH_CHARS: usize = 4096;
pub const MAX_WATCHED_ROOTS: usize = MAX_RULES;
pub const MAX_WATCH_INGRESS_EVENTS: usize = 512;
pub const MAX_EVENT_PATHS: usize = 64;
pub const MAX_EVENT_PATH_BYTES: usize = 32 * 1024;
pub const MAX_PENDING_FILES: usize = 1024;
pub const MAX_READY_CANDIDATES: usize = 256;
pub const MAX_RUNTIME_DIAGNOSTICS: usize = 128;
pub const MAX_DUE_OBSERVATIONS_PER_TURN: usize = 32;
pub const READY_LEASE_TIMEOUT_MS: u64 = 30_000;

pub const MIN_SETTLE_INTERVAL_MS: u64 = 50;
pub const MAX_SETTLE_INTERVAL_MS: u64 = 5_000;
pub const MIN_SETTLE_OBSERVATIONS: u8 = 2;
pub const MAX_SETTLE_OBSERVATIONS: u8 = 10;
pub const MIN_SETTLE_TIMEOUT_MS: u64 = 100;
pub const MAX_SETTLE_TIMEOUT_MS: u64 = 30_000;

/// Stable identity for a persisted rule. New IDs are random 128-bit values
/// rendered as 32 lowercase hexadecimal characters.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RuleId(String);

impl RuleId {
    pub fn generate() -> Self {
        Self(format!("{:032x}", rand::random::<u128>()))
    }

    pub fn parse(value: impl Into<String>) -> Result<Self, &'static str> {
        let id = Self(value.into());
        if is_lower_hex(&id.0, 32) {
            Ok(id)
        } else {
            Err("rule ID must contain exactly 32 lowercase hexadecimal characters")
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn is_valid(&self) -> bool {
        is_lower_hex(&self.0, 32)
    }
}

impl fmt::Debug for RuleId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("RuleId").field(&self.0).finish()
    }
}

/// Digest of the currently selected launcher or MkMacro configuration. The
/// digest is rechecked against the injected current catalog before capability
/// is reported; it is never treated as cached authorization.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ConfigurationFingerprint(String);

impl ConfigurationFingerprint {
    pub(crate) fn of_serializable<T: Serialize>(value: &T) -> Result<Self, serde_json::Error> {
        let bytes = serde_json::to_vec(value)?;
        if bytes.len() > MAX_DOCUMENT_BYTES {
            return Err(serde_json::Error::io(std::io::Error::other(
                "configuration exceeds the supported fingerprint size",
            )));
        }
        Ok(Self(hex::encode(Sha256::digest(bytes))))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn is_valid(&self) -> bool {
        is_lower_hex(&self.0, 64)
    }
}

impl fmt::Debug for ConfigurationFingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ConfigurationFingerprint([redacted])")
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleTrigger {
    #[default]
    NewFile,
    ModifiedFile,
}

/// A reported filesystem event. This small value is shared by watcher
/// admission and the read-only rule preview so both paths evaluate the same
/// trigger semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotFolderEventKind {
    Created,
    Modified,
    Removed,
    RenamedFrom,
    RenamedTo,
}

/// Inclusive lower and upper bounds in the unit named by the containing
/// field. At least one bound must be present.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InclusiveU64Range {
    #[serde(default)]
    pub min: Option<u64>,
    #[serde(default)]
    pub max: Option<u64>,
}

impl InclusiveU64Range {
    pub fn validate(self) -> Result<(), &'static str> {
        match (self.min, self.max) {
            (None, None) => Err("a file-condition range needs at least one bound"),
            (Some(min), Some(max)) if min > max => {
                Err("a file-condition minimum cannot exceed its maximum")
            }
            _ => Ok(()),
        }
    }

    fn contains(self, value: u64) -> bool {
        self.min.is_none_or(|minimum| value >= minimum)
            && self.max.is_none_or(|maximum| value <= maximum)
    }

    fn contains_duration(self, value: std::time::Duration) -> bool {
        self.min
            .is_none_or(|minimum| value >= std::time::Duration::from_secs(minimum))
            && self
                .max
                .is_none_or(|maximum| value <= std::time::Duration::from_secs(maximum))
    }
}

/// A literal condition applied later by the bounded content reader. The
/// literal is deliberately omitted from Debug output and diagnostics.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "mode",
    content = "literal",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum LiteralContentCondition {
    Contains(String),
    DoesNotContain(String),
}

impl LiteralContentCondition {
    fn literal(&self) -> &str {
        match self {
            Self::Contains(literal) | Self::DoesNotContain(literal) => literal,
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        let literal = self.literal();
        if literal.is_empty() {
            return Err("content condition literal cannot be empty");
        }
        if literal.len() > MAX_CONTENT_LITERAL_BYTES {
            return Err("content condition literal exceeds the supported UTF-8 byte limit");
        }
        if literal.contains('\0') {
            return Err("content condition literal cannot contain NUL");
        }
        Ok(())
    }
}

impl fmt::Debug for LiteralContentCondition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mode = match self {
            Self::Contains(_) => "contains",
            Self::DoesNotContain(_) => "does_not_contain",
        };
        formatter
            .debug_struct("LiteralContentCondition")
            .field("mode", &mode)
            .field("literal", &"[redacted]")
            .finish()
    }
}

/// Optional file conditions. Size is measured in bytes and age bounds are
/// expressed in seconds; content matching is a separate, bounded streaming
/// stage.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HotFolderConditions {
    #[serde(default)]
    pub size_bytes: Option<InclusiveU64Range>,
    #[serde(default)]
    pub modified_age_seconds: Option<InclusiveU64Range>,
    #[serde(default)]
    pub content: Option<LiteralContentCondition>,
}

impl HotFolderConditions {
    pub fn is_configured(&self) -> bool {
        self.size_bytes.is_some() || self.modified_age_seconds.is_some() || self.content.is_some()
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if let Some(range) = self.size_bytes {
            range.validate()?;
        }
        if let Some(range) = self.modified_age_seconds {
            range.validate()?;
        }
        if let Some(content) = &self.content {
            content.validate()?;
        }
        Ok(())
    }

    pub fn evaluate_metadata(
        &self,
        metadata: FileMetadata,
        captured_now: std::time::SystemTime,
    ) -> MetadataConditionResult {
        if let Err(reason) = self.validate() {
            return MetadataConditionResult::InvalidConfiguration { reason };
        }
        if self
            .size_bytes
            .is_some_and(|range| !range.contains(metadata.size_bytes))
        {
            return MetadataConditionResult::NoMatch {
                reason: "file size is outside the configured range",
            };
        }
        if let Some(range) = self.modified_age_seconds {
            let Some(modified) = metadata.modified_at else {
                return MetadataConditionResult::NoMatch {
                    reason: "file modification time is unavailable",
                };
            };
            let Ok(age) = captured_now.duration_since(modified) else {
                return MetadataConditionResult::NoMatch {
                    reason: "file modification time is in the future",
                };
            };
            if !range.contains_duration(age) {
                return MetadataConditionResult::NoMatch {
                    reason: "file age is outside the configured range",
                };
            }
        }
        if self.content.is_some() {
            return MetadataConditionResult::NotEvaluated {
                reason: "content condition requires the bounded streaming evaluator",
            };
        }
        MetadataConditionResult::Match
    }
}

impl fmt::Debug for HotFolderConditions {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HotFolderConditions")
            .field("size_bytes", &self.size_bytes)
            .field("modified_age_seconds", &self.modified_age_seconds)
            .field("content", &self.content.as_ref().map(|_| "[redacted]"))
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileMetadata {
    pub size_bytes: u64,
    pub modified_at: Option<std::time::SystemTime>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetadataConditionResult {
    Match,
    NoMatch { reason: &'static str },
    NotEvaluated { reason: &'static str },
    InvalidConfiguration { reason: &'static str },
}

/// Purely evaluate one already-rooted relative filename against the stored
/// rule conditions. The caller remains responsible for proving that `path`
/// was reported beneath the current canonical watched root.
pub fn evaluate_rule_conditions(
    rule: &HotFolderRule,
    event: HotFolderEventKind,
    path: &Path,
) -> Result<(), &'static str> {
    if rule.conditions.is_configured() {
        return Err(FILE_CONDITIONS_NOT_EVALUATED);
    }
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("sample path is not a root-relative file path");
    }
    let trigger_matches = matches!(
        (rule.trigger, event),
        (
            RuleTrigger::NewFile,
            HotFolderEventKind::Created | HotFolderEventKind::RenamedTo
        ) | (RuleTrigger::ModifiedFile, HotFolderEventKind::Modified)
    );
    if !trigger_matches {
        return Err("event does not match the selected trigger");
    }
    let depth = path.components().count();
    if !rule.include_subfolders && depth != 1 {
        return Err("subfolder events are excluded by this rule");
    }
    let Some(filename) = path.file_name() else {
        return Err("sample path has no filename");
    };
    if !rule.include_temporary_files && is_temporary_download_filename(filename) {
        return Err("temporary download filenames are excluded by this rule");
    }
    let filename_matches = match filename.to_str() {
        Some(filename) => rule.filter.matches_filename(filename),
        None => rule.filter.glob.is_none() && rule.filter.extensions.is_empty(),
    };
    if !filename_matches {
        return Err("filename does not match this rule's filters");
    }
    Ok(())
}

pub(crate) fn is_temporary_download_filename(filename: &std::ffi::OsStr) -> bool {
    filename.to_str().is_some_and(|filename| {
        [".part", ".crdownload"].iter().any(|suffix| {
            filename
                .get(filename.len().saturating_sub(suffix.len())..)
                .is_some_and(|tail| tail.eq_ignore_ascii_case(suffix))
        })
    })
}

/// Bounded file-stability policy stored with a rule for the future watcher.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettlePolicy {
    pub sample_interval_ms: u64,
    pub stable_observations: u8,
    pub timeout_ms: u64,
}

impl Default for SettlePolicy {
    fn default() -> Self {
        Self {
            sample_interval_ms: 200,
            stable_observations: 3,
            timeout_ms: 15_000,
        }
    }
}

impl SettlePolicy {
    pub fn validate(self) -> Result<(), &'static str> {
        if !(MIN_SETTLE_INTERVAL_MS..=MAX_SETTLE_INTERVAL_MS).contains(&self.sample_interval_ms) {
            return Err("settle sample interval is outside the supported bounds");
        }
        if !(MIN_SETTLE_OBSERVATIONS..=MAX_SETTLE_OBSERVATIONS).contains(&self.stable_observations)
        {
            return Err("stable observation count is outside the supported bounds");
        }
        if !(MIN_SETTLE_TIMEOUT_MS..=MAX_SETTLE_TIMEOUT_MS).contains(&self.timeout_ms) {
            return Err("settle timeout is outside the supported bounds");
        }
        let minimum_window = self
            .sample_interval_ms
            .saturating_mul(u64::from(self.stable_observations));
        if self.timeout_ms < minimum_window {
            return Err("settle timeout cannot contain the requested stable observations");
        }
        Ok(())
    }
}

/// Filename-only filters. When both a glob and extensions are provided, both
/// constraints must match. Extension matching is ASCII case-insensitive.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FilenameFilter {
    #[serde(default)]
    pub glob: Option<String>,
    #[serde(default)]
    pub extensions: Vec<String>,
}

impl fmt::Debug for FilenameFilter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FilenameFilter")
            .field("has_glob", &self.glob.is_some())
            .field("extension_count", &self.extensions.len())
            .finish()
    }
}

impl FilenameFilter {
    pub fn validate(&self) -> Result<(), &'static str> {
        if let Some(glob) = &self.glob {
            validate_filename_glob(glob)?;
        }
        if self.extensions.len() > MAX_EXTENSIONS {
            return Err("too many extension filters");
        }
        let mut seen = std::collections::HashSet::new();
        for extension in &self.extensions {
            validate_extension(extension)?;
            if !seen.insert(extension.to_ascii_lowercase()) {
                return Err("extension filters must be unique");
            }
        }
        Ok(())
    }

    pub fn matches_filename(&self, filename: &str) -> bool {
        if filename.is_empty() || filename.contains(['/', '\\', '\0']) {
            return false;
        }
        if let Some(glob) = &self.glob
            && !filename_glob_matches(glob, filename)
        {
            return false;
        }
        if self.extensions.is_empty() {
            return true;
        }
        Path::new(filename)
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|actual| {
                self.extensions
                    .iter()
                    .any(|expected| expected.eq_ignore_ascii_case(actual))
            })
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RuleAction {
    Move {
        destination: PathBuf,
    },
    Copy {
        destination: PathBuf,
    },
    Rename {
        prefix: String,
        suffix: String,
    },
    RegisteredLauncherAction {
        reference: PersistedUniversalActionRef,
        configuration_fingerprint: ConfigurationFingerprint,
    },
    MkMacro {
        macro_id: u64,
        configuration_fingerprint: ConfigurationFingerprint,
    },
}

impl fmt::Debug for RuleAction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self {
            Self::Move { .. } => "Move",
            Self::Copy { .. } => "Copy",
            Self::Rename { .. } => "Rename",
            Self::RegisteredLauncherAction { .. } => "RegisteredLauncherAction",
            Self::MkMacro { .. } => "MkMacro",
        };
        formatter.debug_tuple("RuleAction").field(&kind).finish()
    }
}

impl RuleAction {
    pub fn validate(&self) -> Result<(), &'static str> {
        match self {
            Self::Move { destination } | Self::Copy { destination } => {
                validate_explicit_directory(destination, "action destination")
            }
            Self::Rename { prefix, suffix } => {
                validate_rename_part(prefix)?;
                validate_rename_part(suffix)?;
                if prefix.is_empty() && suffix.is_empty() {
                    return Err("rename prefix and suffix cannot both be empty");
                }
                Ok(())
            }
            Self::RegisteredLauncherAction {
                reference,
                configuration_fingerprint,
            } => {
                if reference.action_id.as_str().is_empty()
                    || reference.action_id.as_str().len() > 128
                    || reference.target.is_none()
                {
                    return Err("launcher action reference is not a stable target reference");
                }
                validate_persisted_target(reference)?;
                if !configuration_fingerprint.is_valid() {
                    return Err("launcher action configuration fingerprint is malformed");
                }
                Ok(())
            }
            Self::MkMacro {
                macro_id,
                configuration_fingerprint,
            } => {
                if *macro_id == 0 {
                    return Err("MkMacro ID must be non-zero");
                }
                if !configuration_fingerprint.is_valid() {
                    return Err("MkMacro configuration fingerprint is malformed");
                }
                Ok(())
            }
        }
    }

    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Move { .. } => "move",
            Self::Copy { .. } => "copy",
            Self::Rename { .. } => "rename",
            Self::RegisteredLauncherAction { .. } => "registered_launcher_action",
            Self::MkMacro { .. } => "mkmacro",
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HotFolderRule {
    pub id: RuleId,
    pub name: String,
    pub watched_root: PathBuf,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub trigger: RuleTrigger,
    #[serde(default)]
    pub filter: FilenameFilter,
    #[serde(default)]
    pub include_subfolders: bool,
    /// Allow known temporary download suffixes such as `.part` and
    /// `.crdownload` through the event filter. Defaults to false.
    #[serde(default)]
    pub include_temporary_files: bool,
    #[serde(default)]
    pub conditions: HotFolderConditions,
    #[serde(default)]
    pub settle: SettlePolicy,
    pub action: RuleAction,
}

impl fmt::Debug for HotFolderRule {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HotFolderRule")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("enabled", &self.enabled)
            .field("trigger", &self.trigger)
            .field("include_subfolders", &self.include_subfolders)
            .field("conditions", &self.conditions)
            .field("action", &self.action)
            .finish_non_exhaustive()
    }
}

impl HotFolderRule {
    pub fn new(
        name: impl Into<String>,
        watched_root: impl Into<PathBuf>,
        action: RuleAction,
    ) -> Self {
        Self {
            id: RuleId::generate(),
            name: name.into(),
            watched_root: watched_root.into(),
            enabled: false,
            trigger: RuleTrigger::NewFile,
            filter: FilenameFilter::default(),
            include_subfolders: false,
            include_temporary_files: false,
            conditions: HotFolderConditions::default(),
            settle: SettlePolicy::default(),
            action,
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if !self.id.is_valid() {
            return Err("rule ID is malformed");
        }
        validate_name(&self.name)?;
        validate_explicit_directory(&self.watched_root, "watched root")?;
        self.filter.validate()?;
        self.conditions.validate()?;
        self.settle.validate()?;
        self.action.validate()
    }
}

fn validate_name(name: &str) -> Result<(), &'static str> {
    if name.trim().is_empty() {
        return Err("rule name cannot be empty");
    }
    if name.chars().count() > MAX_RULE_NAME_CHARS || name.chars().any(char::is_control) {
        return Err("rule name is outside the supported bounds");
    }
    Ok(())
}

fn validate_explicit_directory(path: &Path, label: &'static str) -> Result<(), &'static str> {
    if path.as_os_str().is_empty() || !path.is_absolute() {
        return Err(if label == "watched root" {
            "watched root must be an explicit absolute path"
        } else {
            "action destination must be an explicit absolute path"
        });
    }
    let display = path.as_os_str().to_string_lossy();
    if display.chars().count() > MAX_RULE_PATH_CHARS || display.chars().any(char::is_control) {
        return Err(if label == "watched root" {
            "watched root is outside the supported path bounds"
        } else {
            "action destination is outside the supported path bounds"
        });
    }
    if path
        .components()
        .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        return Err(if label == "watched root" {
            "watched root cannot contain current- or parent-directory components"
        } else {
            "action destination cannot contain current- or parent-directory components"
        });
    }
    if !path
        .components()
        .any(|component| matches!(component, Component::Normal(_)))
    {
        return Err(if label == "watched root" {
            "watched root cannot be a drive or filesystem root"
        } else {
            "action destination cannot be a drive or filesystem root"
        });
    }
    Ok(())
}

fn validate_filename_glob(glob: &str) -> Result<(), &'static str> {
    if glob.is_empty() || glob.chars().count() > MAX_FILENAME_GLOB_CHARS {
        return Err("filename glob is empty or exceeds the supported length");
    }
    if glob.contains(['/', '\\', '\0', '[', ']']) || glob.chars().any(char::is_control) {
        return Err("filename glob accepts filename-only literals, * and ? patterns");
    }
    RegexGlob::compile(glob)?;
    Ok(())
}

fn validate_extension(extension: &str) -> Result<(), &'static str> {
    if extension.is_empty()
        || extension.len() > MAX_EXTENSION_CHARS
        || extension.starts_with('.')
        || !extension
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("extension filters must be short filename extensions without a leading dot");
    }
    Ok(())
}

fn validate_rename_part(value: &str) -> Result<(), &'static str> {
    if value.chars().count() > MAX_RENAME_PART_CHARS
        || value
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\' | ':'))
    {
        return Err("rename prefix and suffix must be bounded filename text");
    }
    Ok(())
}

fn validate_persisted_target(reference: &PersistedUniversalActionRef) -> Result<(), &'static str> {
    use crate::universal_actions::PersistableActionTargetRef as Target;
    match reference.target.as_ref() {
        Some(Target::LegacyAction { action } | Target::CustomAction { action }) => {
            if action.label.trim().is_empty()
                || action.action.trim().is_empty()
                || action.action.len() > MAX_EXTERNAL_ACTION_TEXT_CHARS
                || action.args.as_ref().is_some_and(|args| {
                    args.len() > MAX_EXTERNAL_ACTION_TEXT_CHARS
                        || args.chars().any(char::is_control)
                })
            {
                return Err(
                    "saved launcher action parameters are empty or outside the supported bounds",
                );
            }
            Ok(())
        }
        Some(Target::Folder { path } | Target::Tempfile { path }) => {
            if path.trim().is_empty() || path.len() > MAX_EXTERNAL_ACTION_TEXT_CHARS {
                return Err("saved launcher target is empty or outside the supported bounds");
            }
            Ok(())
        }
        Some(Target::Bookmark { url }) => {
            if url.trim().is_empty() || url.len() > MAX_EXTERNAL_ACTION_TEXT_CHARS {
                return Err("saved launcher target is empty or outside the supported bounds");
            }
            Ok(())
        }
        Some(Target::Snippet { alias } | Target::Note { slug: alias }) => {
            if alias.trim().is_empty() || alias.len() > MAX_EXTERNAL_ACTION_TEXT_CHARS {
                return Err("saved launcher target is empty or outside the supported bounds");
            }
            Ok(())
        }
        Some(Target::MkMacro { id }) if *id > 0 => Ok(()),
        Some(Target::MkMacro { .. }) => Err("MkMacro ID must be non-zero"),
        None => Err("launcher action reference has no stable target"),
    }
}

/// Compile the intentionally small filename glob grammar using the existing
/// regex dependency. Only `*` and `?` are metacharacters; all other characters
/// are escaped. Path separators and character classes are rejected above.
struct RegexGlob(regex::Regex);

impl RegexGlob {
    fn compile(glob: &str) -> Result<Self, &'static str> {
        let mut expression = String::from("\\A");
        for character in glob.chars() {
            match character {
                '*' => expression.push_str(".*"),
                '?' => expression.push('.'),
                literal => expression.push_str(&regex::escape(&literal.to_string())),
            }
        }
        expression.push_str("\\z");
        regex::RegexBuilder::new(&expression)
            .case_insensitive(true)
            .build()
            .map(Self)
            .map_err(|_| "filename glob is invalid")
    }

    fn matches(&self, filename: &str) -> bool {
        self.0.is_match(filename)
    }
}

fn filename_glob_matches(glob: &str, filename: &str) -> bool {
    RegexGlob::compile(glob).is_ok_and(|compiled| compiled.matches(filename))
}

fn is_lower_hex(value: &str, expected_len: usize) -> bool {
    value.len() == expected_len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_rule() -> HotFolderRule {
        HotFolderRule::new(
            "Images",
            std::env::temp_dir().join("target/productivity_expansion/watch/images"),
            RuleAction::Copy {
                destination: std::env::temp_dir()
                    .join("target/productivity_expansion/app_data/copied"),
            },
        )
    }

    #[test]
    fn model_defaults_require_explicit_bounded_inputs_and_filter_filenames() {
        let rule = valid_rule();
        assert!(!rule.enabled);
        assert_eq!(rule.trigger, RuleTrigger::NewFile);
        assert!(!rule.include_subfolders);
        assert!(!rule.include_temporary_files);
        assert_eq!(rule.settle, SettlePolicy::default());
        assert!(rule.validate().is_ok());

        let mut legacy = serde_json::to_value(&rule).unwrap();
        legacy
            .as_object_mut()
            .unwrap()
            .remove("include_temporary_files");
        let restored: HotFolderRule = serde_json::from_value(legacy).unwrap();
        assert!(!restored.include_temporary_files);

        let mut invalid = rule.clone();
        invalid.watched_root = PathBuf::new();
        assert!(invalid.validate().is_err());
        invalid.watched_root = PathBuf::from("relative/watch");
        assert!(invalid.validate().is_err());
        invalid.watched_root = Path::new("/").to_path_buf();
        assert!(invalid.validate().is_err());

        let filter = FilenameFilter {
            glob: Some("camera-??-*.JPG".into()),
            extensions: vec!["jpg".into()],
        };
        assert!(filter.validate().is_ok());
        assert!(filter.matches_filename("camera-01-front.jpg"));
        assert!(!filter.matches_filename("camera-1-front.jpg"));
        assert!(!filter.matches_filename("camera-01-front.png"));
        assert!(
            FilenameFilter {
                glob: Some("../*.jpg".into()),
                extensions: Vec::new(),
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn fingerprint_is_stable_and_changes_with_selected_configuration() {
        let first = crate::actions::Action {
            label: "Editor".into(),
            desc: "Application".into(),
            action: "C:\\Tools\\editor.exe".into(),
            args: Some("--open".into()),
        };
        let same = first.clone();
        let mut changed = first.clone();
        changed.args = Some("--safe".into());

        let fingerprint = ConfigurationFingerprint::of_serializable(&first).unwrap();
        assert_eq!(
            fingerprint,
            ConfigurationFingerprint::of_serializable(&same).unwrap()
        );
        assert_ne!(
            fingerprint,
            ConfigurationFingerprint::of_serializable(&changed).unwrap()
        );
        assert!(fingerprint.is_valid());
        assert_eq!(
            RuleId::parse("0123456789abcdef0123456789abcdef")
                .unwrap()
                .as_str(),
            "0123456789abcdef0123456789abcdef"
        );

        let rule = valid_rule();
        let original = ConfigurationFingerprint::of_serializable(&rule).unwrap();
        let mut with_size_condition = rule.clone();
        with_size_condition.conditions.size_bytes = Some(InclusiveU64Range {
            min: Some(0),
            max: None,
        });
        assert_ne!(
            original,
            ConfigurationFingerprint::of_serializable(&with_size_condition).unwrap(),
            "condition edits participate in the existing whole-rule fingerprint"
        );
    }

    #[test]
    fn metadata_conditions_use_inclusive_bounds_and_one_captured_time() {
        use std::time::{Duration, UNIX_EPOCH};

        let now = UNIX_EPOCH + Duration::from_secs(10_000);
        let conditions = HotFolderConditions {
            size_bytes: Some(InclusiveU64Range {
                min: Some(10),
                max: Some(20),
            }),
            modified_age_seconds: Some(InclusiveU64Range {
                min: Some(5),
                max: Some(10),
            }),
            content: None,
        };
        assert_eq!(
            conditions.evaluate_metadata(
                FileMetadata {
                    size_bytes: 10,
                    modified_at: Some(now - Duration::from_secs(5)),
                },
                now,
            ),
            MetadataConditionResult::Match,
            "both lower endpoints are inclusive"
        );
        assert_eq!(
            conditions.evaluate_metadata(
                FileMetadata {
                    size_bytes: 20,
                    modified_at: Some(now - Duration::from_secs(10)),
                },
                now,
            ),
            MetadataConditionResult::Match,
            "both upper endpoints are inclusive"
        );
        assert!(matches!(
            conditions.evaluate_metadata(
                FileMetadata {
                    size_bytes: 9,
                    modified_at: Some(now - Duration::from_secs(5)),
                },
                now,
            ),
            MetadataConditionResult::NoMatch { .. }
        ));

        let age_zero = HotFolderConditions {
            modified_age_seconds: Some(InclusiveU64Range {
                min: Some(0),
                max: None,
            }),
            ..HotFolderConditions::default()
        };
        for modified_at in [None, Some(now + Duration::from_secs(1))] {
            assert!(matches!(
                age_zero.evaluate_metadata(
                    FileMetadata {
                        size_bytes: 0,
                        modified_at,
                    },
                    now,
                ),
                MetadataConditionResult::NoMatch { .. }
            ));
        }
        assert_eq!(
            HotFolderConditions::default().evaluate_metadata(
                FileMetadata {
                    size_bytes: u64::MAX,
                    modified_at: None,
                },
                now,
            ),
            MetadataConditionResult::Match,
            "disabled predicates do not require optional metadata"
        );
    }

    #[test]
    fn modified_age_bounds_preserve_subsecond_precision() {
        use std::time::{Duration, UNIX_EPOCH};

        let now = UNIX_EPOCH + Duration::from_secs(10_000);
        let maximum_ten_seconds = HotFolderConditions {
            modified_age_seconds: Some(InclusiveU64Range {
                min: None,
                max: Some(10),
            }),
            ..HotFolderConditions::default()
        };
        assert_eq!(
            maximum_ten_seconds.evaluate_metadata(
                FileMetadata {
                    size_bytes: 0,
                    modified_at: Some(now - Duration::from_secs(10)),
                },
                now,
            ),
            MetadataConditionResult::Match,
            "the configured maximum remains inclusive at the exact second"
        );
        assert!(matches!(
            maximum_ten_seconds.evaluate_metadata(
                FileMetadata {
                    size_bytes: 0,
                    modified_at: Some(now - Duration::from_secs(10) - Duration::from_millis(1)),
                },
                now,
            ),
            MetadataConditionResult::NoMatch { .. }
        ));

        let maximum_zero_seconds = HotFolderConditions {
            modified_age_seconds: Some(InclusiveU64Range {
                min: None,
                max: Some(0),
            }),
            ..HotFolderConditions::default()
        };
        assert!(matches!(
            maximum_zero_seconds.evaluate_metadata(
                FileMetadata {
                    size_bytes: 0,
                    modified_at: Some(now - Duration::from_millis(1)),
                },
                now,
            ),
            MetadataConditionResult::NoMatch { .. }
        ));
    }

    #[test]
    fn file_conditions_validate_ranges_bound_utf8_and_defer_content() {
        use std::time::{Duration, UNIX_EPOCH};

        assert!(
            InclusiveU64Range {
                min: Some(0),
                max: Some(0),
            }
            .validate()
            .is_ok()
        );
        assert!(
            InclusiveU64Range {
                min: None,
                max: None
            }
            .validate()
            .is_err()
        );
        assert!(
            InclusiveU64Range {
                min: Some(2),
                max: Some(1),
            }
            .validate()
            .is_err()
        );

        let exact_utf8_limit = "é".repeat(MAX_CONTENT_LITERAL_BYTES / 2);
        assert_eq!(exact_utf8_limit.len(), MAX_CONTENT_LITERAL_BYTES);
        assert!(
            LiteralContentCondition::Contains(exact_utf8_limit)
                .validate()
                .is_ok()
        );
        assert!(
            LiteralContentCondition::Contains(String::new())
                .validate()
                .is_err()
        );
        assert!(
            LiteralContentCondition::DoesNotContain("bad\0text".into())
                .validate()
                .is_err()
        );
        assert!(
            LiteralContentCondition::Contains("é".repeat(MAX_CONTENT_LITERAL_BYTES / 2 + 1))
                .validate()
                .is_err()
        );

        let sentinel = "PRIVATE_CONTENT_SENTINEL";
        let content = LiteralContentCondition::Contains(sentinel.into());
        assert!(!format!("{content:?}").contains(sentinel));
        let serialized = serde_json::to_string(&content).unwrap();
        assert!(serialized.contains("\"mode\":\"contains\""));
        assert!(serialized.contains(sentinel));

        let conditions = HotFolderConditions {
            size_bytes: Some(InclusiveU64Range {
                min: Some(1),
                max: Some(5),
            }),
            modified_age_seconds: Some(InclusiveU64Range {
                min: Some(0),
                max: None,
            }),
            content: Some(content),
        };
        let now = UNIX_EPOCH + Duration::from_secs(10);
        assert_eq!(
            conditions.evaluate_metadata(
                FileMetadata {
                    size_bytes: 3,
                    modified_at: Some(now),
                },
                now,
            ),
            MetadataConditionResult::NotEvaluated {
                reason: "content condition requires the bounded streaming evaluator",
            },
            "metadata success cannot claim a content match before M1-B"
        );

        let mut configured = valid_rule();
        configured.conditions = conditions;
        assert!(!format!("{configured:?}").contains(sentinel));
        assert_eq!(
            evaluate_rule_conditions(
                &configured,
                HotFolderEventKind::Created,
                Path::new("image.png")
            ),
            Err(FILE_CONDITIONS_NOT_EVALUATED),
            "the old filename-only runtime must not accept a partial condition match"
        );
    }

    #[test]
    fn condition_bearing_v2_json_rejects_negative_and_overflow_unsigned_bounds() {
        let mut value = serde_json::to_value(valid_rule()).unwrap();
        value["conditions"] = serde_json::json!({
            "size_bytes": { "min": -1, "max": null },
            "modified_age_seconds": null,
            "content": null
        });
        assert!(serde_json::from_value::<HotFolderRule>(value).is_err());
        assert!(
            serde_json::from_str::<InclusiveU64Range>(r#"{"min":18446744073709551616,"max":null}"#)
                .is_err()
        );
    }
}
