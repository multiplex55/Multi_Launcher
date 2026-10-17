//! Explicit-root, read-only text search snapshots used by the replacement
//! workflow. This module owns search inputs, bounded discovery, and immutable
//! match evidence; it deliberately contains no replacement or write path.

use crate::batch_rename::{self, ExplicitDirectoryRoot, SnapshotEntryKind};
use crate::file_operations::{
    self, NativeFileIdentity, RenameScanGeneration, RenameScanIssueKind, RenameScanRequest,
    RenameScanResult, RenameScanRow, RenameScanStatus, ScanCancellation, VerifiedReadFailureKind,
    VerifiedReadSnapshot,
};
use regex::{Regex, RegexBuilder};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::ops::Range;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::SystemTime;

pub const MAX_SEARCH_ROOTS: usize = 32;
pub const MAX_SEARCH_PATTERN_BYTES: usize = 8 * 1024;
pub const MAX_SEARCH_GLOBS: usize = 64;
pub const MAX_SEARCH_GLOB_BYTES: usize = 512;
pub const MAX_SEARCH_EXTENSION_BYTES: usize = 64;
pub const MAX_SEARCH_ISSUE_SAMPLES: usize = 64;

const MAX_HARD_FILE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_HARD_TOTAL_BYTES: u64 = 128 * 1024 * 1024;
const MAX_HARD_FILES: usize = 10_000;
const MAX_HARD_MATCHES_TOTAL: usize = 100_000;
const MAX_HARD_MATCHES_PER_FILE: usize = 10_000;
const MAX_HARD_PREVIEW_CHARS: usize = 512;
const MAX_HARD_CONTEXT_CHARS: usize = 256;
const REGEX_SIZE_LIMIT: usize = 2 * 1024 * 1024;
const REGEX_NEST_LIMIT: u32 = 128;
const MAX_DISPLAY_PATH_CHARS: usize = 1_024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SearchRevision(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SearchGeneration(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SearchPatternMode {
    Literal,
    RustRegex,
}

#[derive(Debug, Clone)]
pub struct SearchPattern {
    source: String,
    mode: SearchPatternMode,
    case_sensitive: bool,
    compiled: Arc<Regex>,
}

impl SearchPattern {
    pub fn compile(
        source: impl Into<String>,
        mode: SearchPatternMode,
        case_sensitive: bool,
    ) -> Result<Self, SearchRequestError> {
        let source = source.into();
        if source.is_empty() {
            return Err(SearchRequestError::new(
                SearchRequestErrorKind::EmptyPattern,
                "the search pattern must not be empty",
            ));
        }
        if source.len() > MAX_SEARCH_PATTERN_BYTES {
            return Err(SearchRequestError::new(
                SearchRequestErrorKind::PatternTooLarge,
                format!("the search pattern exceeds {MAX_SEARCH_PATTERN_BYTES} bytes"),
            ));
        }
        let regex_source = match mode {
            SearchPatternMode::Literal => regex::escape(&source),
            SearchPatternMode::RustRegex => source.clone(),
        };
        let compiled = RegexBuilder::new(&regex_source)
            .case_insensitive(!case_sensitive)
            .size_limit(REGEX_SIZE_LIMIT)
            .dfa_size_limit(REGEX_SIZE_LIMIT)
            .nest_limit(REGEX_NEST_LIMIT)
            .build()
            .map_err(|error| {
                SearchRequestError::new(
                    SearchRequestErrorKind::InvalidPattern,
                    format!("invalid Rust regular expression: {error}"),
                )
            })?;
        Ok(Self {
            source,
            mode,
            case_sensitive,
            compiled: Arc::new(compiled),
        })
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn mode(&self) -> SearchPatternMode {
        self.mode
    }

    pub fn case_sensitive(&self) -> bool {
        self.case_sensitive
    }

    pub(crate) fn compiled(&self) -> &Regex {
        &self.compiled
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchFilterSpec {
    pub include_extensions: Vec<String>,
    pub include_path_globs: Vec<String>,
    pub exclude_paths: Vec<PathBuf>,
    pub exclude_path_globs: Vec<String>,
}

#[derive(Debug, Clone)]
struct CompiledSearchFilters {
    source: SearchFilterSpec,
    include_extensions: Vec<String>,
    include_path_globs: Vec<CompiledPathGlob>,
    exclude_paths: Vec<PathBuf>,
    exclude_path_globs: Vec<CompiledPathGlob>,
}

impl CompiledSearchFilters {
    fn compile(spec: SearchFilterSpec) -> Result<Self, SearchRequestError> {
        if spec.include_extensions.len() > MAX_SEARCH_GLOBS
            || spec.include_path_globs.len() > MAX_SEARCH_GLOBS
            || spec.exclude_paths.len() > MAX_SEARCH_GLOBS
            || spec.exclude_path_globs.len() > MAX_SEARCH_GLOBS
        {
            return Err(SearchRequestError::new(
                SearchRequestErrorKind::TooManyFilters,
                format!("no more than {MAX_SEARCH_GLOBS} filters of each kind are allowed"),
            ));
        }

        let source = spec.clone();
        let mut include_extensions = Vec::with_capacity(spec.include_extensions.len());
        for extension in spec.include_extensions {
            let extension = extension.trim().trim_start_matches('.');
            if extension.is_empty() || extension.len() > MAX_SEARCH_EXTENSION_BYTES {
                return Err(SearchRequestError::new(
                    SearchRequestErrorKind::InvalidFilter,
                    "extensions must be non-empty and within the configured size limit",
                ));
            }
            include_extensions.push(extension.to_owned());
        }
        include_extensions.sort_by_key(|extension| extension.to_ascii_lowercase());
        include_extensions.dedup_by(|left, right| left.eq_ignore_ascii_case(right));

        let include_path_globs = spec
            .include_path_globs
            .into_iter()
            .map(CompiledPathGlob::compile)
            .collect::<Result<Vec<_>, _>>()?;
        let exclude_path_globs = spec
            .exclude_path_globs
            .into_iter()
            .map(CompiledPathGlob::compile)
            .collect::<Result<Vec<_>, _>>()?;
        let mut exclude_paths = Vec::with_capacity(spec.exclude_paths.len());
        for path in spec.exclude_paths {
            if path.is_absolute()
                || path
                    .components()
                    .any(|component| !matches!(component, Component::Normal(_)))
                || path.as_os_str().is_empty()
            {
                return Err(SearchRequestError::new(
                    SearchRequestErrorKind::InvalidFilter,
                    "excluded paths must be non-empty root-relative paths without traversal",
                ));
            }
            exclude_paths.push(path);
        }
        exclude_paths.sort();
        exclude_paths.dedup();

        Ok(Self {
            source,
            include_extensions,
            include_path_globs,
            exclude_paths,
            exclude_path_globs,
        })
    }

    fn has_path_globs(&self) -> bool {
        !self.include_path_globs.is_empty() || !self.exclude_path_globs.is_empty()
    }

    fn matches(&self, relative_path: &Path) -> Result<bool, FilterPathError> {
        if !self.include_extensions.is_empty() {
            let Some(extension) = relative_path.extension() else {
                return Ok(false);
            };
            let extension = extension.to_str().ok_or(FilterPathError::NonUnicode)?;
            if !self
                .include_extensions
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(extension))
            {
                return Ok(false);
            }
        }
        if self
            .exclude_paths
            .iter()
            .any(|excluded| batch_rename::path_is_within_windows_ordinal(excluded, relative_path))
        {
            return Ok(false);
        }

        if self.has_path_globs() {
            let path = relative_path.to_str().ok_or(FilterPathError::NonUnicode)?;
            let normalized = path.replace('\\', "/");
            if !self.include_path_globs.is_empty()
                && !self
                    .include_path_globs
                    .iter()
                    .any(|glob| glob.matches(&normalized))
            {
                return Ok(false);
            }
            if self
                .exclude_path_globs
                .iter()
                .any(|glob| glob.matches(&normalized))
            {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

#[derive(Debug, Clone)]
struct CompiledPathGlob {
    regex: Regex,
}

impl CompiledPathGlob {
    fn compile(pattern: String) -> Result<Self, SearchRequestError> {
        if pattern.is_empty() || pattern.len() > MAX_SEARCH_GLOB_BYTES {
            return Err(SearchRequestError::new(
                SearchRequestErrorKind::InvalidFilter,
                format!(
                    "path globs must be non-empty and no longer than {MAX_SEARCH_GLOB_BYTES} bytes"
                ),
            ));
        }
        let normalized = pattern.replace('\\', "/");
        let regex_source = glob_to_regex(&normalized)?;
        let regex = RegexBuilder::new(&regex_source)
            .case_insensitive(cfg!(windows))
            .size_limit(REGEX_SIZE_LIMIT)
            .dfa_size_limit(REGEX_SIZE_LIMIT)
            .nest_limit(REGEX_NEST_LIMIT)
            .build()
            .map_err(|error| {
                SearchRequestError::new(
                    SearchRequestErrorKind::InvalidFilter,
                    format!("invalid path glob '{pattern}': {error}"),
                )
            })?;
        Ok(Self { regex })
    }

    fn matches(&self, path: &str) -> bool {
        self.regex.is_match(path)
    }
}

fn glob_to_regex(pattern: &str) -> Result<String, SearchRequestError> {
    let chars: Vec<char> = pattern.chars().collect();
    let mut regex = String::from("^");
    let mut index = 0;
    while index < chars.len() {
        match chars[index] {
            '*' if chars.get(index + 1) == Some(&'*') => {
                index += 2;
                if chars.get(index) == Some(&'/') {
                    regex.push_str("(?:.*/)?");
                    index += 1;
                } else {
                    regex.push_str(".*");
                }
            }
            '*' => {
                regex.push_str("[^/]*");
                index += 1;
            }
            '?' => {
                regex.push_str("[^/]");
                index += 1;
            }
            '[' | ']' | '{' | '}' => {
                return Err(SearchRequestError::new(
                    SearchRequestErrorKind::InvalidFilter,
                    "path globs support '*', '**', and '?' wildcards; character classes and brace expansion are unsupported",
                ));
            }
            value => {
                regex.push_str(&regex::escape(&value.to_string()));
                index += 1;
            }
        }
    }
    regex.push('$');
    Ok(regex)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchLimits {
    max_file_bytes: u64,
    max_total_bytes: u64,
    max_files: usize,
    max_matches_total: usize,
    max_matches_per_file: usize,
    max_preview_chars: usize,
    max_context_chars_per_side: usize,
}

impl SearchLimits {
    pub fn new(
        max_file_bytes: u64,
        max_total_bytes: u64,
        max_files: usize,
        max_matches_total: usize,
        max_matches_per_file: usize,
        max_preview_chars: usize,
        max_context_chars_per_side: usize,
    ) -> Result<Self, SearchRequestError> {
        let limits = Self {
            max_file_bytes,
            max_total_bytes,
            max_files,
            max_matches_total,
            max_matches_per_file,
            max_preview_chars,
            max_context_chars_per_side,
        };
        limits.validate()?;
        Ok(limits)
    }

    fn validate(self) -> Result<(), SearchRequestError> {
        if self.max_file_bytes == 0
            || self.max_file_bytes > MAX_HARD_FILE_BYTES
            || self.max_total_bytes == 0
            || self.max_total_bytes > MAX_HARD_TOTAL_BYTES
            || self.max_files == 0
            || self.max_files > MAX_HARD_FILES
            || self.max_matches_total == 0
            || self.max_matches_total > MAX_HARD_MATCHES_TOTAL
            || self.max_matches_per_file == 0
            || self.max_matches_per_file > MAX_HARD_MATCHES_PER_FILE
            || self.max_preview_chars == 0
            || self.max_preview_chars > MAX_HARD_PREVIEW_CHARS
            || self.max_context_chars_per_side > MAX_HARD_CONTEXT_CHARS
        {
            return Err(SearchRequestError::new(
                SearchRequestErrorKind::InvalidLimits,
                "search limits must be positive and within the hard resource bounds",
            ));
        }
        Ok(())
    }

    pub fn max_file_bytes(self) -> u64 {
        self.max_file_bytes
    }

    pub fn max_total_bytes(self) -> u64 {
        self.max_total_bytes
    }

    pub fn max_files(self) -> usize {
        self.max_files
    }

    pub fn max_matches_total(self) -> usize {
        self.max_matches_total
    }

    pub fn max_matches_per_file(self) -> usize {
        self.max_matches_per_file
    }

    pub fn max_preview_chars(self) -> usize {
        self.max_preview_chars
    }

    pub fn max_context_chars_per_side(self) -> usize {
        self.max_context_chars_per_side
    }
}

impl Default for SearchLimits {
    fn default() -> Self {
        Self {
            max_file_bytes: 4 * 1024 * 1024,
            max_total_bytes: 32 * 1024 * 1024,
            max_files: 5_000,
            max_matches_total: 25_000,
            max_matches_per_file: 5_000,
            max_preview_chars: 192,
            max_context_chars_per_side: 80,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TextSearchRequest {
    roots: Vec<ExplicitDirectoryRoot>,
    recursive: bool,
    filters: CompiledSearchFilters,
    pattern: SearchPattern,
    revision: SearchRevision,
    generation: SearchGeneration,
    limits: SearchLimits,
}

impl TextSearchRequest {
    pub fn new(
        roots: Vec<ExplicitDirectoryRoot>,
        filters: SearchFilterSpec,
        pattern: SearchPattern,
        revision: SearchRevision,
        generation: SearchGeneration,
        limits: SearchLimits,
    ) -> Result<Self, SearchRequestError> {
        if roots.is_empty() {
            return Err(SearchRequestError::new(
                SearchRequestErrorKind::NoExplicitRoots,
                "choose at least one explicit search root",
            ));
        }
        if roots.len() > MAX_SEARCH_ROOTS {
            return Err(SearchRequestError::new(
                SearchRequestErrorKind::TooManyRoots,
                format!("no more than {MAX_SEARCH_ROOTS} roots may be searched at once"),
            ));
        }
        limits.validate()?;
        let mut roots = roots;
        roots.sort_by(|left, right| {
            batch_rename::compare_paths_windows_ordinal(left.as_path(), right.as_path())
                .then_with(|| left.as_path().cmp(right.as_path()))
        });
        roots.dedup_by(|left, right| {
            batch_rename::compare_paths_windows_ordinal(left.as_path(), right.as_path())
                == std::cmp::Ordering::Equal
        });
        Ok(Self {
            roots,
            recursive: false,
            filters: CompiledSearchFilters::compile(filters)?,
            pattern,
            revision,
            generation,
            limits,
        })
    }

    pub fn roots(&self) -> &[ExplicitDirectoryRoot] {
        &self.roots
    }

    pub fn recursive(&self) -> bool {
        self.recursive
    }

    pub fn with_recursive(mut self, recursive: bool) -> Self {
        self.recursive = recursive;
        self
    }

    pub fn filters(&self) -> &SearchFilterSpec {
        &self.filters.source
    }

    pub fn revision(&self) -> SearchRevision {
        self.revision
    }

    pub fn generation(&self) -> SearchGeneration {
        self.generation
    }

    pub fn pattern(&self) -> &SearchPattern {
        &self.pattern
    }

    pub fn limits(&self) -> SearchLimits {
        self.limits
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchRequestErrorKind {
    NoExplicitRoots,
    TooManyRoots,
    EmptyPattern,
    PatternTooLarge,
    InvalidPattern,
    TooManyFilters,
    InvalidFilter,
    InvalidLimits,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchRequestError {
    kind: SearchRequestErrorKind,
    message: String,
}

impl SearchRequestError {
    fn new(kind: SearchRequestErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn kind(&self) -> SearchRequestErrorKind {
        self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for SearchRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for SearchRequestError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchStatus {
    Complete,
    Partial,
    Cancelled,
    ResourceLimit,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SkippedFileReason {
    ReparsePoint,
    Unreadable,
    ChangedDuringRead,
    TooLarge,
    FileLimitReached,
    AggregateByteLimitReached,
    MatchLimitReached,
    Binary,
    UnsupportedEncoding,
    InvalidUtf8,
    NonUnicodeFilterPath,
    InvalidNativeEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchIssue {
    path: Option<PathBuf>,
    reason: SkippedFileReason,
    message: String,
}

impl SearchIssue {
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn reason(&self) -> SkippedFileReason {
        self.reason
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchSummary {
    roots_scanned: usize,
    roots_unfinished: usize,
    files_discovered: usize,
    files_examined: usize,
    files_searched: usize,
    files_matched: usize,
    matches_found: usize,
    files_skipped: usize,
    files_unfinished: usize,
    bytes_read: u64,
    match_list_complete: bool,
    skipped_by_reason: BTreeMap<SkippedFileReason, usize>,
    issue_count: usize,
    issues_truncated: bool,
    issues: Vec<SearchIssue>,
}

impl SearchSummary {
    pub fn roots_scanned(&self) -> usize {
        self.roots_scanned
    }

    pub fn roots_unfinished(&self) -> usize {
        self.roots_unfinished
    }

    pub fn files_discovered(&self) -> usize {
        self.files_discovered
    }

    pub fn files_examined(&self) -> usize {
        self.files_examined
    }

    pub fn files_searched(&self) -> usize {
        self.files_searched
    }

    pub fn files_matched(&self) -> usize {
        self.files_matched
    }

    pub fn matches_found(&self) -> usize {
        self.matches_found
    }

    pub fn files_skipped(&self) -> usize {
        self.files_skipped
    }

    pub fn files_unfinished(&self) -> usize {
        self.files_unfinished
    }

    pub fn bytes_read(&self) -> u64 {
        self.bytes_read
    }

    pub fn match_list_complete(&self) -> bool {
        self.match_list_complete
    }

    pub fn skipped_by_reason(&self) -> &BTreeMap<SkippedFileReason, usize> {
        &self.skipped_by_reason
    }

    pub fn issue_count(&self) -> usize {
        self.issue_count
    }

    pub fn issues_truncated(&self) -> bool {
        self.issues_truncated
    }

    pub fn issues(&self) -> &[SearchIssue] {
        &self.issues
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MatchId {
    snapshot_path: Arc<PathBuf>,
    snapshot_sha256: [u8; 32],
    search_revision: SearchRevision,
    search_generation: SearchGeneration,
    raw_byte_range: Range<usize>,
    ordinal: u64,
}

impl MatchId {
    pub fn snapshot_path(&self) -> &Path {
        self.snapshot_path.as_path()
    }

    pub fn snapshot_sha256(&self) -> [u8; 32] {
        self.snapshot_sha256
    }

    pub fn search_revision(&self) -> SearchRevision {
        self.search_revision
    }

    /// Generation of the exact immutable scan that produced this identity.
    pub fn search_generation(&self) -> SearchGeneration {
        self.search_generation
    }

    pub fn raw_byte_range(&self) -> Range<usize> {
        self.raw_byte_range.clone()
    }

    pub fn ordinal(&self) -> u64 {
        self.ordinal
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchSpan {
    id: MatchId,
    raw_byte_range: Range<usize>,
    first_line: usize,
    last_line: usize,
    before_context: String,
    matched_preview: String,
    after_context: String,
    preview_truncated: bool,
}

impl MatchSpan {
    pub fn id(&self) -> &MatchId {
        &self.id
    }

    pub fn raw_byte_range(&self) -> Range<usize> {
        self.raw_byte_range.clone()
    }

    pub fn first_line(&self) -> usize {
        self.first_line
    }

    pub fn last_line(&self) -> usize {
        self.last_line
    }

    pub fn before_context(&self) -> &str {
        &self.before_context
    }

    pub fn matched_preview(&self) -> &str {
        &self.matched_preview
    }

    pub fn after_context(&self) -> &str {
        &self.after_context
    }

    pub fn preview_truncated(&self) -> bool {
        self.preview_truncated
    }
}

/// Matches are emitted in source order and do not overlap, matching the
/// standard Rust regex iterator's replacement-safe span semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchPolicy {
    NonOverlappingSourceOrder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextEncoding {
    Utf8,
    Utf8Bom,
}

#[derive(Debug, Clone)]
pub struct SearchFileSnapshot {
    root: PathBuf,
    relative_path: PathBuf,
    path: PathBuf,
    display_path: String,
    root_identity: NativeFileIdentity,
    file_identity: NativeFileIdentity,
    parent_identity: NativeFileIdentity,
    length: u64,
    modified: SystemTime,
    raw_bytes: Arc<[u8]>,
    content_sha256: [u8; 32],
    encoding: TextEncoding,
    bom_offset: usize,
    matches: Vec<MatchSpan>,
}

impl SearchFileSnapshot {
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn relative_path(&self) -> &Path {
        &self.relative_path
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn display_path(&self) -> &str {
        &self.display_path
    }

    pub fn root_identity(&self) -> NativeFileIdentity {
        self.root_identity
    }

    pub fn file_identity(&self) -> NativeFileIdentity {
        self.file_identity
    }

    pub fn parent_identity(&self) -> NativeFileIdentity {
        self.parent_identity
    }

    pub fn length(&self) -> u64 {
        self.length
    }

    pub fn modified(&self) -> SystemTime {
        self.modified
    }

    /// Original complete file bytes, including any UTF-8 BOM.
    pub fn raw_bytes(&self) -> &Arc<[u8]> {
        &self.raw_bytes
    }

    pub fn content_sha256(&self) -> [u8; 32] {
        self.content_sha256
    }

    pub fn encoding(&self) -> TextEncoding {
        self.encoding
    }

    pub fn bom_offset(&self) -> usize {
        self.bom_offset
    }

    pub fn matches(&self) -> &[MatchSpan] {
        &self.matches
    }
}

#[derive(Debug, Clone)]
pub struct TextSearchResult {
    request: TextSearchRequest,
    status: SearchStatus,
    summary: SearchSummary,
    matched_files: Vec<SearchFileSnapshot>,
}

impl TextSearchResult {
    pub fn revision(&self) -> SearchRevision {
        self.request.revision
    }

    pub fn generation(&self) -> SearchGeneration {
        self.request.generation
    }

    pub fn status(&self) -> SearchStatus {
        self.status
    }

    pub fn summary(&self) -> &SearchSummary {
        &self.summary
    }

    pub fn matched_files(&self) -> &[SearchFileSnapshot] {
        &self.matched_files
    }

    pub fn pattern(&self) -> &SearchPattern {
        &self.request.pattern
    }

    pub fn request(&self) -> &TextSearchRequest {
        &self.request
    }

    pub fn match_policy(&self) -> MatchPolicy {
        MatchPolicy::NonOverlappingSourceOrder
    }

    /// Compute an immutable preview from this exact scan snapshot. The
    /// candidate contains the original native evidence and complete output
    /// bytes; this operation does not access the filesystem.
    pub fn preview_replacement(
        &self,
        replacement: &CompiledReplacement,
        selected_match_ids: &[MatchId],
    ) -> Result<ReplacementCandidate, ReplacementPreviewError> {
        build_replacement_candidate(self, replacement, selected_match_ids)
    }

    /// Accept only the result whose cancel generation is still current.
    pub fn accept(self, current_generation: SearchGeneration) -> Result<Self, StaleSearchResult> {
        let result_generation = self.generation();
        if result_generation == current_generation {
            Ok(self)
        } else {
            Err(StaleSearchResult {
                result_generation,
                current_generation,
            })
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaleSearchResult {
    result_generation: SearchGeneration,
    current_generation: SearchGeneration,
}

impl StaleSearchResult {
    pub fn result_generation(self) -> SearchGeneration {
        self.result_generation
    }

    pub fn current_generation(self) -> SearchGeneration {
        self.current_generation
    }
}

/// A replacement edit is versioned independently from the original search
/// revision, so editing replacement text preserves the captured A match IDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ReplacementRevision(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReplacementMode {
    /// Every byte of the authored UTF-8 string is literal replacement text.
    Literal,
    /// Use the existing Rust regex crate's `$0`, `$1`, `${name}` and `$$`
    /// capture replacement syntax against the original matched text.
    RustRegexCaptures,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplacementSpec {
    text: String,
    mode: ReplacementMode,
    revision: ReplacementRevision,
}

impl ReplacementSpec {
    pub fn new(
        text: impl Into<String>,
        mode: ReplacementMode,
        revision: ReplacementRevision,
    ) -> Self {
        Self {
            text: text.into(),
            mode,
            revision,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn mode(&self) -> ReplacementMode {
        self.mode
    }

    pub fn revision(&self) -> ReplacementRevision {
        self.revision
    }

    /// Validate capture references once against the captured search pattern.
    /// Unknown references are diagnostics instead of silently expanding to an
    /// empty string, while unmatched optional groups retain Rust regex's empty
    /// string behavior when a particular match is expanded.
    pub fn compile(
        &self,
        pattern: &SearchPattern,
    ) -> Result<CompiledReplacement, ReplacementDiagnostic> {
        let text: Arc<str> = Arc::from(self.text.as_str());
        let parts = match self.mode {
            ReplacementMode::Literal => vec![ReplacementPart::Literal(0..text.len())],
            ReplacementMode::RustRegexCaptures => {
                parse_replacement_template(&text, pattern.compiled(), self.revision)?
            }
        };
        Ok(CompiledReplacement {
            text,
            mode: self.mode,
            revision: self.revision,
            pattern_source: pattern.source.clone(),
            pattern_mode: pattern.mode,
            pattern_case_sensitive: pattern.case_sensitive,
            parts,
        })
    }
}

#[derive(Debug, Clone)]
pub struct CompiledReplacement {
    text: Arc<str>,
    mode: ReplacementMode,
    revision: ReplacementRevision,
    pattern_source: String,
    pattern_mode: SearchPatternMode,
    pattern_case_sensitive: bool,
    parts: Vec<ReplacementPart>,
}

impl CompiledReplacement {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn mode(&self) -> ReplacementMode {
        self.mode
    }

    pub fn revision(&self) -> ReplacementRevision {
        self.revision
    }
}

#[derive(Debug, Clone)]
enum ReplacementPart {
    Literal(Range<usize>),
    EscapedDollar,
    Capture(CaptureReference),
}

#[derive(Debug, Clone)]
enum CaptureReference {
    Index(usize),
    Name(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplacementDiagnosticKind {
    UnknownCapture,
    InvalidCaptureIndex,
    OutputLimitExceeded,
    AllocationFailed,
    CapturedMatchMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplacementDiagnostic {
    revision: ReplacementRevision,
    kind: ReplacementDiagnosticKind,
    message: String,
}

impl ReplacementDiagnostic {
    fn new(
        revision: ReplacementRevision,
        kind: ReplacementDiagnosticKind,
        message: impl Into<String>,
    ) -> Self {
        Self {
            revision,
            kind,
            message: message.into(),
        }
    }

    pub fn revision(&self) -> ReplacementRevision {
        self.revision
    }

    pub fn kind(&self) -> ReplacementDiagnosticKind {
        self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ReplacementDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ReplacementDiagnostic {}

fn parse_replacement_template(
    text: &str,
    pattern: &Regex,
    revision: ReplacementRevision,
) -> Result<Vec<ReplacementPart>, ReplacementDiagnostic> {
    let bytes = text.as_bytes();
    let mut parts = Vec::new();
    let mut literal_start = 0;
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'$' {
            index += 1;
            continue;
        }
        if bytes.get(index + 1) == Some(&b'$') {
            if literal_start < index {
                parts.push(ReplacementPart::Literal(literal_start..index));
            }
            parts.push(ReplacementPart::EscapedDollar);
            index += 2;
            literal_start = index;
            continue;
        }

        let (reference_end, reference) = if bytes.get(index + 1) == Some(&b'{') {
            let Some(relative_end) = bytes[index + 2..].iter().position(|byte| *byte == b'}')
            else {
                index += 1;
                continue;
            };
            let end = index + 2 + relative_end;
            (end + 1, Some(&text[index + 2..end]))
        } else {
            let mut end = index + 1;
            while bytes.get(end).is_some_and(|byte| {
                byte.is_ascii_alphanumeric() || *byte == b'_'
            }) {
                end += 1;
            }
            if end == index + 1 {
                index += 1;
                continue;
            }
            (end, Some(&text[index + 1..end]))
        };
        let Some(reference) = reference else {
            index += 1;
            continue;
        };
        if literal_start < index {
            parts.push(ReplacementPart::Literal(literal_start..index));
        }
        parts.push(ReplacementPart::Capture(validate_capture_reference(
            reference, pattern, revision,
        )?));
        index = reference_end;
        literal_start = index;
    }
    if literal_start < text.len() {
        parts.push(ReplacementPart::Literal(literal_start..text.len()));
    }
    Ok(parts)
}

fn validate_capture_reference(
    reference: &str,
    pattern: &Regex,
    revision: ReplacementRevision,
) -> Result<CaptureReference, ReplacementDiagnostic> {
    // Match the regex crate's own replacement parser: braced references are
    // parsed as a usize first, which intentionally accepts spellings such as
    // `${+1}` as capture index 1.
    if let Ok(index) = reference.parse::<usize>() {
        if index < pattern.captures_len() {
            return Ok(CaptureReference::Index(index));
        }
        return Err(ReplacementDiagnostic::new(
            revision,
            ReplacementDiagnosticKind::InvalidCaptureIndex,
            format!("replacement capture '${reference}' does not exist in this pattern"),
        ));
    }
    if pattern
        .capture_names()
        .flatten()
        .any(|name| name == reference)
    {
        Ok(CaptureReference::Name(reference.to_owned()))
    } else {
        Err(ReplacementDiagnostic::new(
            revision,
            ReplacementDiagnosticKind::UnknownCapture,
            format!("replacement capture '${{{reference}}}' does not exist in this pattern"),
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplacementPreviewErrorKind {
    PatternMismatch,
    DuplicateSelection,
    StaleSelection,
    ForeignSelection,
    InconsistentSnapshot,
    InvalidMatchRange,
    OverlappingMatches,
    DuplicateInterval,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplacementPreviewError {
    kind: ReplacementPreviewErrorKind,
    message: String,
}

impl ReplacementPreviewError {
    fn new(kind: ReplacementPreviewErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn kind(&self) -> ReplacementPreviewErrorKind {
        self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ReplacementPreviewError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ReplacementPreviewError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplacementCandidateReadiness {
    Ready,
    NoOp,
    IncompleteDiscovery,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplacementFileReadiness {
    Ready,
    NoOp,
    Error,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReplacementSummary {
    affected_files: usize,
    approved_matches: usize,
    changed_matches: usize,
    selected_noop_matches: usize,
    failed_files: usize,
    /// Sum of original match bytes plus replacement bytes for changed selected
    /// matches. Unchanged selected matches and the preserved BOM are excluded.
    estimated_changed_bytes: u64,
}

impl ReplacementSummary {
    pub fn affected_files(&self) -> usize {
        self.affected_files
    }

    /// Number of explicitly selected MatchIds, including those whose
    /// replacement is byte-identical to their original match.
    pub fn approved_matches(&self) -> usize {
        self.approved_matches
    }

    pub fn changed_matches(&self) -> usize {
        self.changed_matches
    }

    pub fn selected_noop_matches(&self) -> usize {
        self.selected_noop_matches
    }

    pub fn failed_files(&self) -> usize {
        self.failed_files
    }

    pub fn estimated_changed_bytes(&self) -> u64 {
        self.estimated_changed_bytes
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplacementDiffContext {
    original_excerpt: String,
    candidate_excerpt: String,
    first_change_byte_offset: usize,
    truncated: bool,
}

impl ReplacementDiffContext {
    pub fn original_excerpt(&self) -> &str {
        &self.original_excerpt
    }

    pub fn candidate_excerpt(&self) -> &str {
        &self.candidate_excerpt
    }

    pub fn first_change_byte_offset(&self) -> usize {
        self.first_change_byte_offset
    }

    /// Indicates that the bounded excerpts omit some original or candidate
    /// text. Full candidate bytes remain separately available and exact.
    pub fn truncated(&self) -> bool {
        self.truncated
    }
}

#[derive(Debug, Clone)]
pub struct ReplacementFileCandidate {
    original_snapshot: SearchFileSnapshot,
    output_bytes: Arc<[u8]>,
    output_sha256: [u8; 32],
    replacement_revision: ReplacementRevision,
    selected_match_ids: Vec<MatchId>,
    readiness: ReplacementFileReadiness,
    diagnostic: Option<ReplacementDiagnostic>,
    changed_matches: Option<usize>,
    selected_noop_matches: Option<usize>,
    estimated_changed_bytes: Option<u64>,
    diff_context: Option<ReplacementDiffContext>,
}

impl ReplacementFileCandidate {
    /// Original bytes, hashes, paths, and native root/file/parent identities
    /// remain attached to the candidate for the later confirmation boundary.
    pub fn original_snapshot(&self) -> &SearchFileSnapshot {
        &self.original_snapshot
    }

    pub fn output_bytes(&self) -> &Arc<[u8]> {
        &self.output_bytes
    }

    pub fn output_sha256(&self) -> [u8; 32] {
        self.output_sha256
    }

    pub fn replacement_revision(&self) -> ReplacementRevision {
        self.replacement_revision
    }

    pub fn selected_match_ids(&self) -> &[MatchId] {
        &self.selected_match_ids
    }

    pub fn readiness(&self) -> ReplacementFileReadiness {
        self.readiness
    }

    pub fn diagnostic(&self) -> Option<&ReplacementDiagnostic> {
        self.diagnostic.as_ref()
    }

    /// `None` means an output-limit or allocation failure prevented a complete
    /// per-file computation; no partial output is exposed as a candidate.
    pub fn changed_matches(&self) -> Option<usize> {
        self.changed_matches
    }

    pub fn selected_noop_matches(&self) -> Option<usize> {
        self.selected_noop_matches
    }

    pub fn estimated_changed_bytes(&self) -> Option<u64> {
        self.estimated_changed_bytes
    }

    pub fn diff_context(&self) -> Option<&ReplacementDiffContext> {
        self.diff_context.as_ref()
    }
}

#[derive(Debug, Clone)]
pub struct ReplacementCandidate {
    search_revision: SearchRevision,
    search_generation: SearchGeneration,
    replacement_revision: ReplacementRevision,
    discovery_complete: bool,
    selected_match_ids: Vec<MatchId>,
    files: Vec<ReplacementFileCandidate>,
    summary: ReplacementSummary,
    readiness: ReplacementCandidateReadiness,
}

impl ReplacementCandidate {
    pub fn search_revision(&self) -> SearchRevision {
        self.search_revision
    }

    pub fn search_generation(&self) -> SearchGeneration {
        self.search_generation
    }

    pub fn replacement_revision(&self) -> ReplacementRevision {
        self.replacement_revision
    }

    pub fn discovery_complete(&self) -> bool {
        self.discovery_complete
    }

    pub fn selected_match_ids(&self) -> &[MatchId] {
        &self.selected_match_ids
    }

    pub fn files(&self) -> &[ReplacementFileCandidate] {
        &self.files
    }

    pub fn summary(&self) -> &ReplacementSummary {
        &self.summary
    }

    pub fn readiness(&self) -> ReplacementCandidateReadiness {
        self.readiness
    }
}

struct ComputedFileOutput {
    bytes: Arc<[u8]>,
    sha256: [u8; 32],
    changed_matches: usize,
    selected_noop_matches: usize,
    estimated_changed_bytes: u64,
    diff_context: Option<ReplacementDiffContext>,
}

fn build_replacement_candidate(
    result: &TextSearchResult,
    replacement: &CompiledReplacement,
    selected_match_ids: &[MatchId],
) -> Result<ReplacementCandidate, ReplacementPreviewError> {
    if replacement.pattern_source != result.request.pattern.source
        || replacement.pattern_mode != result.request.pattern.mode
        || replacement.pattern_case_sensitive != result.request.pattern.case_sensitive
    {
        return Err(ReplacementPreviewError::new(
            ReplacementPreviewErrorKind::PatternMismatch,
            "the replacement was compiled for a different search pattern",
        ));
    }

    let mut requested = HashSet::with_capacity(selected_match_ids.len());
    for id in selected_match_ids {
        if !requested.insert(id.clone()) {
            return Err(ReplacementPreviewError::new(
                ReplacementPreviewErrorKind::DuplicateSelection,
                "the selected match list contains a duplicate identity",
            ));
        }
    }

    let mut known = HashSet::new();
    let mut path_set = HashSet::new();
    let mut match_count = 0usize;
    for file in &result.matched_files {
        path_set.insert(file.path.clone());
        validate_snapshot_for_preview(file, result)?;
        for span in &file.matches {
            if !known.insert(span.id.clone()) {
                return Err(ReplacementPreviewError::new(
                    ReplacementPreviewErrorKind::InconsistentSnapshot,
                    "the captured scan contains duplicate match identities",
                ));
            }
            match_count = match_count.saturating_add(1);
        }
    }
    if match_count != result.summary.matches_found {
        return Err(ReplacementPreviewError::new(
            ReplacementPreviewErrorKind::InconsistentSnapshot,
            "the captured match count does not agree with its search summary",
        ));
    }

    for id in &requested {
        if known.contains(id) {
            continue;
        }
        let same_path = path_set.contains(id.snapshot_path.as_ref());
        let current_scan_identity = id.search_revision == result.revision()
            && id.search_generation == result.generation();
        let (kind, message) = if same_path || !current_scan_identity {
            (
                ReplacementPreviewErrorKind::StaleSelection,
                "a selected match belongs to an older search or source snapshot",
            )
        } else {
            (
                ReplacementPreviewErrorKind::ForeignSelection,
                "a selected match does not belong to this captured search result",
            )
        };
        return Err(ReplacementPreviewError::new(kind, message));
    }

    // Canonicalize selection into the immutable source order. UI selection
    // order therefore cannot affect candidate bytes, IDs, summaries, or hash.
    let mut canonical_selection = Vec::with_capacity(requested.len());
    for file in &result.matched_files {
        for span in &file.matches {
            if requested.contains(&span.id) {
                canonical_selection.push(span.id.clone());
            }
        }
    }

    let max_file_output = usize::try_from(result.request.limits.max_file_bytes)
        .unwrap_or(usize::MAX)
        .min(usize::try_from(MAX_HARD_FILE_BYTES).unwrap_or(usize::MAX));
    let mut total_output_remaining = usize::try_from(result.request.limits.max_total_bytes)
        .unwrap_or(usize::MAX)
        .min(usize::try_from(MAX_HARD_TOTAL_BYTES).unwrap_or(usize::MAX));
    let mut files = Vec::with_capacity(result.matched_files.len());
    let mut summary = ReplacementSummary {
        approved_matches: canonical_selection.len(),
        ..ReplacementSummary::default()
    };

    for snapshot in &result.matched_files {
        let file_selection = snapshot
            .matches
            .iter()
            .filter(|span| requested.contains(&span.id))
            .map(|span| span.id.clone())
            .collect::<Vec<_>>();
        let output_limit = max_file_output.min(total_output_remaining);
        let computed = compute_file_output(
            snapshot,
            &file_selection,
            &result.request.pattern,
            replacement,
            output_limit,
            result.request.limits.max_context_chars_per_side,
            result.request.limits.max_preview_chars,
        );
        let file_candidate = match computed {
            Ok(computed) => {
                let changed = computed.bytes.as_ref() != snapshot.raw_bytes.as_ref();
                if changed {
                    summary.affected_files = summary.affected_files.saturating_add(1);
                }
                summary.changed_matches = summary
                    .changed_matches
                    .saturating_add(computed.changed_matches);
                summary.selected_noop_matches = summary
                    .selected_noop_matches
                    .saturating_add(computed.selected_noop_matches);
                summary.estimated_changed_bytes = summary
                    .estimated_changed_bytes
                    .saturating_add(computed.estimated_changed_bytes);
                total_output_remaining = total_output_remaining
                    .saturating_sub(computed.bytes.len());
                ReplacementFileCandidate {
                    original_snapshot: snapshot.clone(),
                    output_bytes: computed.bytes,
                    output_sha256: computed.sha256,
                    replacement_revision: replacement.revision,
                    selected_match_ids: file_selection,
                    readiness: if changed {
                        ReplacementFileReadiness::Ready
                    } else {
                        ReplacementFileReadiness::NoOp
                    },
                    diagnostic: None,
                    changed_matches: Some(computed.changed_matches),
                    selected_noop_matches: Some(computed.selected_noop_matches),
                    estimated_changed_bytes: Some(computed.estimated_changed_bytes),
                    diff_context: computed.diff_context,
                }
            }
            Err(diagnostic) => {
                summary.failed_files = summary.failed_files.saturating_add(1);
                total_output_remaining = total_output_remaining
                    .saturating_sub(snapshot.raw_bytes.len());
                ReplacementFileCandidate {
                    original_snapshot: snapshot.clone(),
                    output_bytes: Arc::clone(&snapshot.raw_bytes),
                    output_sha256: snapshot.content_sha256,
                    replacement_revision: replacement.revision,
                    selected_match_ids: file_selection,
                    readiness: ReplacementFileReadiness::Error,
                    diagnostic: Some(diagnostic),
                    changed_matches: None,
                    selected_noop_matches: None,
                    estimated_changed_bytes: None,
                    diff_context: None,
                }
            }
        };
        files.push(file_candidate);
    }

    let discovery_complete = result.summary.match_list_complete;
    let readiness = if !discovery_complete {
        ReplacementCandidateReadiness::IncompleteDiscovery
    } else if summary.failed_files > 0 {
        ReplacementCandidateReadiness::Blocked
    } else if summary.affected_files == 0 {
        ReplacementCandidateReadiness::NoOp
    } else {
        ReplacementCandidateReadiness::Ready
    };
    Ok(ReplacementCandidate {
        search_revision: result.revision(),
        search_generation: result.generation(),
        replacement_revision: replacement.revision,
        discovery_complete,
        selected_match_ids: canonical_selection,
        files,
        summary,
        readiness,
    })
}

fn validate_snapshot_for_preview(
    snapshot: &SearchFileSnapshot,
    result: &TextSearchResult,
) -> Result<(), ReplacementPreviewError> {
    let raw = snapshot.raw_bytes.as_ref();
    if u64::try_from(raw.len()).ok() != Some(snapshot.length)
        || <[u8; 32]>::from(Sha256::digest(raw)) != snapshot.content_sha256
    {
        return Err(ReplacementPreviewError::new(
            ReplacementPreviewErrorKind::InconsistentSnapshot,
            "the captured source bytes do not agree with their original length or hash",
        ));
    }
    let Some((encoding, bom_offset, text)) = decode_supported_utf8(raw) else {
        return Err(ReplacementPreviewError::new(
            ReplacementPreviewErrorKind::InconsistentSnapshot,
            "the captured source bytes are no longer valid supported UTF-8",
        ));
    };
    if encoding != snapshot.encoding || bom_offset != snapshot.bom_offset {
        return Err(ReplacementPreviewError::new(
            ReplacementPreviewErrorKind::InconsistentSnapshot,
            "the captured source encoding does not agree with its original metadata",
        ));
    }

    let mut seen_intervals = HashSet::with_capacity(snapshot.matches.len());
    let mut prior: Option<Range<usize>> = None;
    for (index, span) in snapshot.matches.iter().enumerate() {
        let range = span.raw_byte_range();
        let id = &span.id;
        if id.snapshot_path() != snapshot.path
            || id.snapshot_sha256 != snapshot.content_sha256
            || id.search_revision != result.revision()
            || id.search_generation != result.generation()
            || id.raw_byte_range != range
            || usize::try_from(id.ordinal).ok() != Some(index)
        {
            return Err(ReplacementPreviewError::new(
                ReplacementPreviewErrorKind::InconsistentSnapshot,
                "a captured match identity does not agree with its source snapshot",
            ));
        }
        if range.start < bom_offset
            || range.start > range.end
            || range.end > raw.len()
            || text
                .get(range.start.saturating_sub(bom_offset)..range.end.saturating_sub(bom_offset))
                .is_none()
        {
            return Err(ReplacementPreviewError::new(
                ReplacementPreviewErrorKind::InvalidMatchRange,
                "a captured match contains an invalid or non-UTF-8-boundary byte range",
            ));
        }
        if !seen_intervals.insert(range.clone()) {
            return Err(ReplacementPreviewError::new(
                ReplacementPreviewErrorKind::DuplicateInterval,
                "the captured scan contains duplicate match intervals",
            ));
        }
        if let Some(previous) = &prior {
            if range.start < previous.end
                || (previous.start == previous.end && range.start == previous.start)
            {
                return Err(ReplacementPreviewError::new(
                    ReplacementPreviewErrorKind::OverlappingMatches,
                    "captured match intervals overlap or repeat a zero-width insertion point",
                ));
            }
        }
        prior = Some(range);
    }
    Ok(())
}

fn compute_file_output(
    snapshot: &SearchFileSnapshot,
    selected_match_ids: &[MatchId],
    pattern: &SearchPattern,
    replacement: &CompiledReplacement,
    output_limit: usize,
    max_context_chars: usize,
    max_preview_chars: usize,
) -> Result<ComputedFileOutput, ReplacementDiagnostic> {
    let revision = replacement.revision;
    let raw = snapshot.raw_bytes.as_ref();
    if selected_match_ids.is_empty() {
        if raw.len() > output_limit {
            return Err(output_limit_diagnostic(revision, output_limit));
        }
        return Ok(ComputedFileOutput {
            bytes: Arc::clone(&snapshot.raw_bytes),
            sha256: snapshot.content_sha256,
            changed_matches: 0,
            selected_noop_matches: 0,
            estimated_changed_bytes: 0,
            diff_context: None,
        });
    }

    let body = std::str::from_utf8(&raw[snapshot.bom_offset..]).map_err(|_| {
        ReplacementDiagnostic::new(
            revision,
            ReplacementDiagnosticKind::CapturedMatchMismatch,
            "the captured source body is not valid UTF-8",
        )
    })?;
    let selected = selected_match_ids.iter().collect::<HashSet<_>>();
    let mut output = Vec::new();
    output
        .try_reserve_exact(raw.len().min(output_limit))
        .map_err(|_| {
            ReplacementDiagnostic::new(
                revision,
                ReplacementDiagnosticKind::AllocationFailed,
                "unable to reserve memory for the exact replacement candidate",
            )
        })?;
    let mut cursor = 0;
    let mut changed_matches = 0usize;
    let mut selected_noop_matches = 0usize;
    let mut estimated_changed_bytes = 0u64;

    for span in &snapshot.matches {
        if !selected.contains(&span.id) {
            continue;
        }
        let range = span.raw_byte_range();
        append_output(&mut output, &raw[cursor..range.start], output_limit, revision)?;
        let relative_start = range.start.checked_sub(snapshot.bom_offset).ok_or_else(|| {
            ReplacementDiagnostic::new(
                revision,
                ReplacementDiagnosticKind::CapturedMatchMismatch,
                "a selected match offset precedes the original UTF-8 body",
            )
        })?;
        let relative_end = range.end.checked_sub(snapshot.bom_offset).ok_or_else(|| {
            ReplacementDiagnostic::new(
                revision,
                ReplacementDiagnosticKind::CapturedMatchMismatch,
                "a selected match end precedes the original UTF-8 body",
            )
        })?;
        let captures = pattern
            .compiled()
            .captures_at(body, relative_start)
            .ok_or_else(|| captured_match_mismatch(revision))?;
        let Some(full_match) = captures.get(0) else {
            return Err(captured_match_mismatch(revision));
        };
        if full_match.start() != relative_start || full_match.end() != relative_end {
            return Err(captured_match_mismatch(revision));
        }
        let matched_text = &body[relative_start..relative_end];
        let replacement_start = output.len();
        append_expansion(&mut output, &captures, replacement, output_limit)?;
        let inserted = std::str::from_utf8(&output[replacement_start..]).map_err(|_| {
            ReplacementDiagnostic::new(
                revision,
                ReplacementDiagnosticKind::AllocationFailed,
                "the replacement expansion produced invalid UTF-8",
            )
        })?;
        if inserted == matched_text {
            selected_noop_matches = selected_noop_matches.saturating_add(1);
        } else {
            changed_matches = changed_matches.saturating_add(1);
            let replaced_bytes = u64::try_from(matched_text.len()).unwrap_or(u64::MAX);
            let inserted_bytes = u64::try_from(inserted.len()).unwrap_or(u64::MAX);
            estimated_changed_bytes = estimated_changed_bytes
                .saturating_add(replaced_bytes)
                .saturating_add(inserted_bytes);
        }
        cursor = range.end;
    }
    append_output(&mut output, &raw[cursor..], output_limit, revision)?;

    if output.as_slice() == raw {
        return Ok(ComputedFileOutput {
            bytes: Arc::clone(&snapshot.raw_bytes),
            sha256: snapshot.content_sha256,
            changed_matches,
            selected_noop_matches,
            estimated_changed_bytes,
            diff_context: None,
        });
    }

    let output_sha256 = <[u8; 32]>::from(Sha256::digest(&output));
    let output_body = std::str::from_utf8(&output[snapshot.bom_offset..]).map_err(|_| {
        ReplacementDiagnostic::new(
            revision,
            ReplacementDiagnosticKind::AllocationFailed,
            "the exact replacement candidate is not valid UTF-8",
        )
    })?;
    let diff_context = make_diff_context(
        body,
        output_body,
        snapshot.bom_offset,
        max_context_chars,
        max_preview_chars,
    );
    Ok(ComputedFileOutput {
        bytes: Arc::from(output),
        sha256: output_sha256,
        changed_matches,
        selected_noop_matches,
        estimated_changed_bytes,
        diff_context,
    })
}

fn append_expansion(
    output: &mut Vec<u8>,
    captures: &regex::Captures<'_>,
    replacement: &CompiledReplacement,
    output_limit: usize,
) -> Result<(), ReplacementDiagnostic> {
    for part in &replacement.parts {
        match part {
            ReplacementPart::Literal(range) => append_output(
                output,
                replacement.text[range.clone()].as_bytes(),
                output_limit,
                replacement.revision,
            )?,
            ReplacementPart::EscapedDollar => {
                append_output(output, b"$", output_limit, replacement.revision)?;
            }
            ReplacementPart::Capture(reference) => {
                let value = match reference {
                    CaptureReference::Index(index) => captures.get(*index),
                    CaptureReference::Name(name) => captures.name(name),
                };
                if let Some(value) = value {
                    append_output(
                        output,
                        value.as_str().as_bytes(),
                        output_limit,
                        replacement.revision,
                    )?;
                }
            }
        }
    }
    Ok(())
}

fn append_output(
    output: &mut Vec<u8>,
    bytes: &[u8],
    output_limit: usize,
    revision: ReplacementRevision,
) -> Result<(), ReplacementDiagnostic> {
    let new_len = output.len().checked_add(bytes.len()).ok_or_else(|| {
        output_limit_diagnostic(revision, output_limit)
    })?;
    if new_len > output_limit {
        return Err(output_limit_diagnostic(revision, output_limit));
    }
    output.try_reserve(bytes.len()).map_err(|_| {
        ReplacementDiagnostic::new(
            revision,
            ReplacementDiagnosticKind::AllocationFailed,
            "unable to grow memory for the exact replacement candidate",
        )
    })?;
    output.extend_from_slice(bytes);
    Ok(())
}

fn output_limit_diagnostic(
    revision: ReplacementRevision,
    output_limit: usize,
) -> ReplacementDiagnostic {
    ReplacementDiagnostic::new(
        revision,
        ReplacementDiagnosticKind::OutputLimitExceeded,
        format!("the replacement output exceeds the {output_limit}-byte candidate limit"),
    )
}

fn captured_match_mismatch(revision: ReplacementRevision) -> ReplacementDiagnostic {
    ReplacementDiagnostic::new(
        revision,
        ReplacementDiagnosticKind::CapturedMatchMismatch,
        "the captured source bytes no longer contain the selected full match at its original range",
    )
}

fn make_diff_context(
    original: &str,
    candidate: &str,
    bom_offset: usize,
    context_chars: usize,
    preview_chars: usize,
) -> Option<ReplacementDiffContext> {
    if original == candidate {
        return None;
    }
    let mut prefix = original
        .as_bytes()
        .iter()
        .zip(candidate.as_bytes())
        .take_while(|(left, right)| left == right)
        .count();
    while !original.is_char_boundary(prefix) || !candidate.is_char_boundary(prefix) {
        prefix = prefix.saturating_sub(1);
    }
    let start = char_start_before(original, prefix, context_chars);
    let excerpt_chars = context_chars.saturating_add(preview_chars);
    let original_end = char_end_after(original, prefix, excerpt_chars);
    let candidate_end = char_end_after(candidate, prefix.min(candidate.len()), excerpt_chars);
    Some(ReplacementDiffContext {
        original_excerpt: original[start..original_end].to_owned(),
        candidate_excerpt: candidate[start.min(candidate.len())..candidate_end].to_owned(),
        first_change_byte_offset: bom_offset.saturating_add(prefix),
        truncated: start > 0 || original_end < original.len() || candidate_end < candidate.len(),
    })
}

fn char_start_before(text: &str, offset: usize, max_chars: usize) -> usize {
    if max_chars == 0 || offset == 0 {
        return offset;
    }
    text[..offset]
        .char_indices()
        .rev()
        .nth(max_chars.saturating_sub(1))
        .map_or(0, |(index, _)| index)
}

fn char_end_after(text: &str, offset: usize, max_chars: usize) -> usize {
    if offset >= text.len() {
        return text.len();
    }
    text[offset..]
        .char_indices()
        .nth(max_chars)
        .map_or(text.len(), |(index, _)| offset + index)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchProgressPhase {
    Queued,
    Discovering,
    Reading,
    Matching,
    Complete,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchProgress {
    generation: SearchGeneration,
    phase: SearchProgressPhase,
    roots_completed: usize,
    roots_total: usize,
    files_examined: usize,
    bytes_read: u64,
    matches_found: usize,
    current_root: Option<PathBuf>,
}

impl SearchProgress {
    pub fn generation(&self) -> SearchGeneration {
        self.generation
    }

    pub fn phase(&self) -> SearchProgressPhase {
        self.phase
    }

    pub fn roots_completed(&self) -> usize {
        self.roots_completed
    }

    pub fn roots_total(&self) -> usize {
        self.roots_total
    }

    pub fn files_examined(&self) -> usize {
        self.files_examined
    }

    pub fn bytes_read(&self) -> u64 {
        self.bytes_read
    }

    pub fn matches_found(&self) -> usize {
        self.matches_found
    }

    pub fn current_root(&self) -> Option<&Path> {
        self.current_root.as_deref()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FilterPathError {
    NonUnicode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MatchBuildStatus {
    Complete,
    Cancelled,
    LimitReached,
}

struct BuiltMatches {
    matches: Vec<MatchSpan>,
    status: MatchBuildStatus,
}

fn build_matches(
    text: &str,
    raw_length: usize,
    bom_offset: usize,
    path: &Path,
    content_sha256: [u8; 32],
    revision: SearchRevision,
    generation: SearchGeneration,
    pattern: &SearchPattern,
    limits: SearchLimits,
    remaining_total_matches: usize,
    cancellation: &ScanCancellation,
) -> BuiltMatches {
    let line_starts = logical_line_starts(text);
    let snapshot_path = Arc::new(path.to_path_buf());
    let mut matches = Vec::new();
    let mut status = MatchBuildStatus::Complete;
    for (ordinal, found) in pattern.compiled().find_iter(text).enumerate() {
        if cancellation.is_cancelled() {
            status = MatchBuildStatus::Cancelled;
            break;
        }
        if matches.len() >= limits.max_matches_per_file || matches.len() >= remaining_total_matches
        {
            status = MatchBuildStatus::LimitReached;
            break;
        }
        let Some(start) = bom_offset.checked_add(found.start()) else {
            status = MatchBuildStatus::LimitReached;
            break;
        };
        let Some(end) = bom_offset.checked_add(found.end()) else {
            status = MatchBuildStatus::LimitReached;
            break;
        };
        if end > raw_length || start > end {
            status = MatchBuildStatus::LimitReached;
            break;
        }
        let raw_byte_range = start..end;
        let first_line = line_for_offset(&line_starts, found.start());
        let last_offset = if found.end() > found.start() {
            found.end().saturating_sub(1)
        } else {
            found.start()
        };
        let last_line = line_for_offset(&line_starts, last_offset);
        let before_context = bounded_before(text, found.start(), limits.max_context_chars_per_side);
        let after_context = bounded_after(text, found.end(), limits.max_context_chars_per_side);
        let (matched_preview, preview_truncated) =
            bounded_text(&text[found.start()..found.end()], limits.max_preview_chars);
        let ordinal = u64::try_from(ordinal).unwrap_or(u64::MAX);
        let id = MatchId {
            snapshot_path: Arc::clone(&snapshot_path),
            snapshot_sha256: content_sha256,
            search_revision: revision,
            search_generation: generation,
            raw_byte_range: raw_byte_range.clone(),
            ordinal,
        };
        matches.push(MatchSpan {
            id,
            raw_byte_range,
            first_line,
            last_line,
            before_context,
            matched_preview,
            after_context,
            preview_truncated,
        });
    }
    BuiltMatches { matches, status }
}

fn logical_line_starts(text: &str) -> Vec<usize> {
    let bytes = text.as_bytes();
    let mut starts = vec![0];
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\r' if bytes.get(index + 1) == Some(&b'\n') => {
                index += 2;
                starts.push(index);
            }
            b'\r' | b'\n' => {
                index += 1;
                starts.push(index);
            }
            _ => index += 1,
        }
    }
    starts
}

fn line_for_offset(starts: &[usize], offset: usize) -> usize {
    let insertion = starts.partition_point(|start| *start <= offset);
    insertion.max(1)
}

fn bounded_before(text: &str, end: usize, max_chars: usize) -> String {
    if max_chars == 0 || end == 0 {
        return String::new();
    }
    let before = &text[..end];
    let start = before
        .char_indices()
        .rev()
        .nth(max_chars.saturating_sub(1))
        .map_or(0, |(index, _)| index);
    before[start..].to_owned()
}

fn bounded_after(text: &str, start: usize, max_chars: usize) -> String {
    if max_chars == 0 || start >= text.len() {
        return String::new();
    }
    let after = &text[start..];
    let end = after
        .char_indices()
        .nth(max_chars)
        .map_or(after.len(), |(index, _)| index);
    after[..end].to_owned()
}

fn bounded_text(text: &str, max_chars: usize) -> (String, bool) {
    if text.chars().count() <= max_chars {
        return (text.to_owned(), false);
    }
    let end = text
        .char_indices()
        .nth(max_chars)
        .map_or(text.len(), |(index, _)| index);
    (text[..end].to_owned(), true)
}

fn bounded_display_path(path: &Path) -> String {
    let display = path.display().to_string();
    let (mut display, truncated) = bounded_text(&display, MAX_DISPLAY_PATH_CHARS);
    if truncated {
        display.push('…');
    }
    display
}

fn record_skip(
    summary: &mut SearchSummary,
    path: Option<&Path>,
    reason: SkippedFileReason,
    message: impl Into<String>,
) {
    summary.files_skipped = summary.files_skipped.saturating_add(1);
    let count = summary.skipped_by_reason.entry(reason).or_default();
    *count = count.saturating_add(1);
    record_issue(summary, path, reason, message);
}

fn record_issue(
    summary: &mut SearchSummary,
    path: Option<&Path>,
    reason: SkippedFileReason,
    message: impl Into<String>,
) {
    summary.issue_count = summary.issue_count.saturating_add(1);
    if summary.issues.len() >= MAX_SEARCH_ISSUE_SAMPLES {
        summary.issues_truncated = true;
        return;
    }
    let (message, _) = bounded_text(&message.into(), 512);
    summary.issues.push(SearchIssue {
        path: path.map(Path::to_path_buf),
        reason,
        message,
    });
}

fn execute_search(
    request: TextSearchRequest,
    cancellation: &ScanCancellation,
    mut report_progress: impl FnMut(SearchProgress),
) -> TextSearchResult {
    let mut result = TextSearchResult {
        request: request.clone(),
        status: SearchStatus::Complete,
        summary: SearchSummary {
            match_list_complete: true,
            ..SearchSummary::default()
        },
        matched_files: Vec::new(),
    };
    let roots_total = request.roots.len();
    let mut seen_paths = Vec::<PathBuf>::new();

    for (root_index, root) in request.roots.iter().enumerate() {
        if cancellation.is_cancelled() {
            result.status = SearchStatus::Cancelled;
            result.summary.match_list_complete = false;
            result.summary.roots_unfinished = result
                .summary
                .roots_unfinished
                .saturating_add(roots_total.saturating_sub(root_index));
            break;
        }
        report_progress(SearchProgress {
            generation: request.generation,
            phase: SearchProgressPhase::Discovering,
            roots_completed: root_index.saturating_sub(result.summary.roots_unfinished),
            roots_total,
            files_examined: result.summary.files_examined,
            bytes_read: result.summary.bytes_read,
            matches_found: result.summary.matches_found,
            current_root: Some(root.as_path().to_path_buf()),
        });
        let scan = file_operations::scan_rename_root(
            RenameScanRequest::new(
                root.clone(),
                request.recursive,
                RenameScanGeneration(request.generation.0),
            ),
            cancellation,
        );
        result.summary.roots_scanned = result.summary.roots_scanned.saturating_add(1);
        result.summary.files_discovered = result
            .summary
            .files_discovered
            .saturating_add(count_file_rows(scan.rows()));

        if scan.status() != RenameScanStatus::Complete {
            result.summary.match_list_complete = false;
            result.status = merge_status(result.status, search_status_from_scan(scan.status()));
            if matches!(scan.status(), RenameScanStatus::Cancelled) {
                result.summary.roots_unfinished = result
                    .summary
                    .roots_unfinished
                    .saturating_add(roots_total.saturating_sub(root_index));
                result.summary.files_unfinished = result
                    .summary
                    .files_unfinished
                    .saturating_add(count_file_rows(scan.rows()));
                record_scan_issues(&mut result.summary, &scan);
                break;
            }
            result.summary.files_unfinished = result
                .summary
                .files_unfinished
                .saturating_add(count_file_rows(scan.rows()));
            record_scan_issues(&mut result.summary, &scan);
            result.summary.roots_unfinished = result.summary.roots_unfinished.saturating_add(1);
            if scan.status() == RenameScanStatus::ResourceLimit {
                result.summary.roots_unfinished = result
                    .summary
                    .roots_unfinished
                    .saturating_add(roots_total.saturating_sub(root_index + 1));
                break;
            }
            continue;
        }

        for (row_index, row) in scan.rows().iter().enumerate() {
            if cancellation.is_cancelled() {
                result.status = SearchStatus::Cancelled;
                result.summary.match_list_complete = false;
                result.summary.files_unfinished = result
                    .summary
                    .files_unfinished
                    .saturating_add(count_file_rows(&scan.rows()[row_index..]));
                result.summary.roots_unfinished = result
                    .summary
                    .roots_unfinished
                    .saturating_add(roots_total.saturating_sub(root_index));
                break;
            }
            if row.kind() != SnapshotEntryKind::File {
                if row.is_reparse_point() {
                    record_skip(
                        &mut result.summary,
                        Some(row.path()),
                        SkippedFileReason::ReparsePoint,
                        "reparse points are excluded from text search",
                    );
                }
                continue;
            }

            match request.filters.matches(row.relative_path()) {
                Ok(false) => continue,
                Err(FilterPathError::NonUnicode) => {
                    record_skip(
                        &mut result.summary,
                        Some(row.path()),
                        SkippedFileReason::NonUnicodeFilterPath,
                        "the configured extension or path glob cannot be evaluated for a non-Unicode path",
                    );
                    continue;
                }
                Ok(true) => {}
            }

            if !insert_path_windows_ordinal(&mut seen_paths, row.path().to_path_buf()) {
                continue;
            }
            if result.summary.files_examined >= request.limits.max_files {
                result.status = SearchStatus::ResourceLimit;
                result.summary.match_list_complete = false;
                result.summary.files_unfinished = result
                    .summary
                    .files_unfinished
                    .saturating_add(count_file_rows(&scan.rows()[row_index..]));
                result.summary.roots_unfinished = result
                    .summary
                    .roots_unfinished
                    .saturating_add(roots_total.saturating_sub(root_index));
                record_issue(
                    &mut result.summary,
                    Some(row.path()),
                    SkippedFileReason::FileLimitReached,
                    format!("file limit of {} reached", request.limits.max_files),
                );
                break;
            }
            let expected_length = row.length().unwrap_or_default();
            if expected_length > request.limits.max_file_bytes {
                record_skip(
                    &mut result.summary,
                    Some(row.path()),
                    SkippedFileReason::TooLarge,
                    format!(
                        "file length {expected_length} exceeds the {}-byte per-file limit",
                        request.limits.max_file_bytes
                    ),
                );
                continue;
            }
            if result
                .summary
                .bytes_read
                .checked_add(expected_length)
                .is_none_or(|total| total > request.limits.max_total_bytes)
            {
                result.status = SearchStatus::ResourceLimit;
                result.summary.match_list_complete = false;
                result.summary.files_unfinished = result
                    .summary
                    .files_unfinished
                    .saturating_add(count_file_rows(&scan.rows()[row_index..]));
                result.summary.roots_unfinished = result
                    .summary
                    .roots_unfinished
                    .saturating_add(roots_total.saturating_sub(root_index));
                record_issue(
                    &mut result.summary,
                    Some(row.path()),
                    SkippedFileReason::AggregateByteLimitReached,
                    format!(
                        "aggregate search byte limit of {} reached",
                        request.limits.max_total_bytes
                    ),
                );
                break;
            }
            result.summary.files_examined = result.summary.files_examined.saturating_add(1);
            report_progress(SearchProgress {
                generation: request.generation,
                phase: SearchProgressPhase::Reading,
                roots_completed: root_index.saturating_sub(result.summary.roots_unfinished),
                roots_total,
                files_examined: result.summary.files_examined,
                bytes_read: result.summary.bytes_read,
                matches_found: result.summary.matches_found,
                current_root: Some(root.as_path().to_path_buf()),
            });
            let read = match file_operations::read_verified_scan_file(
                &scan,
                row,
                request.limits.max_file_bytes,
                cancellation,
            ) {
                Ok(read) => read,
                Err(error) => {
                    if error.kind() == VerifiedReadFailureKind::Cancelled {
                        result.status = SearchStatus::Cancelled;
                        result.summary.match_list_complete = false;
                        result.summary.files_unfinished = result
                            .summary
                            .files_unfinished
                            .saturating_add(count_file_rows(&scan.rows()[row_index..]));
                        result.summary.roots_unfinished = result
                            .summary
                            .roots_unfinished
                            .saturating_add(roots_total.saturating_sub(root_index));
                        break;
                    }
                    record_skip(
                        &mut result.summary,
                        Some(row.path()),
                        skipped_reason_from_read(error.kind()),
                        error.message(),
                    );
                    continue;
                }
            };
            result.summary.bytes_read = result.summary.bytes_read.saturating_add(read.length());

            let Some((encoding, bom_offset, text)) = decode_supported_utf8(read.bytes()) else {
                record_skip(
                    &mut result.summary,
                    Some(read.path()),
                    classify_unsupported_bytes(read.bytes()),
                    "file is binary, invalid UTF-8, or uses an unsupported encoding",
                );
                continue;
            };
            result.summary.files_searched = result.summary.files_searched.saturating_add(1);
            report_progress(SearchProgress {
                generation: request.generation,
                phase: SearchProgressPhase::Matching,
                roots_completed: root_index.saturating_sub(result.summary.roots_unfinished),
                roots_total,
                files_examined: result.summary.files_examined,
                bytes_read: result.summary.bytes_read,
                matches_found: result.summary.matches_found,
                current_root: Some(root.as_path().to_path_buf()),
            });
            let remaining_matches = request
                .limits
                .max_matches_total
                .saturating_sub(result.summary.matches_found);
            let built = build_matches(
                text,
                read.bytes().len(),
                bom_offset,
                read.path(),
                read.content_sha256(),
                request.revision,
                request.generation,
                &request.pattern,
                request.limits,
                remaining_matches,
                cancellation,
            );
            let BuiltMatches {
                matches,
                status: built_status,
            } = built;
            if !matches.is_empty() {
                result.summary.files_matched = result.summary.files_matched.saturating_add(1);
                result.summary.matches_found =
                    result.summary.matches_found.saturating_add(matches.len());
                result.matched_files.push(SearchFileSnapshot {
                    root: read.canonical_root().to_path_buf(),
                    relative_path: row.relative_path().to_path_buf(),
                    path: read.path().to_path_buf(),
                    display_path: bounded_display_path(read.path()),
                    root_identity: read.root_identity(),
                    file_identity: read.identity(),
                    parent_identity: read.parent_identity(),
                    length: read.length(),
                    modified: read.modified(),
                    raw_bytes: Arc::clone(read.bytes()),
                    content_sha256: read.content_sha256(),
                    encoding,
                    bom_offset,
                    matches,
                });
            }
            match built_status {
                MatchBuildStatus::Complete => {}
                MatchBuildStatus::Cancelled => {
                    result.status = SearchStatus::Cancelled;
                    result.summary.match_list_complete = false;
                    result.summary.files_unfinished = result
                        .summary
                        .files_unfinished
                        .saturating_add(1)
                        .saturating_add(count_file_rows(&scan.rows()[row_index + 1..]));
                    result.summary.roots_unfinished = result
                        .summary
                        .roots_unfinished
                        .saturating_add(roots_total.saturating_sub(root_index));
                    break;
                }
                MatchBuildStatus::LimitReached => {
                    result.status = SearchStatus::ResourceLimit;
                    result.summary.match_list_complete = false;
                    result.summary.files_unfinished = result
                        .summary
                        .files_unfinished
                        .saturating_add(1)
                        .saturating_add(count_file_rows(&scan.rows()[row_index + 1..]));
                    result.summary.roots_unfinished = result
                        .summary
                        .roots_unfinished
                        .saturating_add(roots_total.saturating_sub(root_index));
                    record_issue(
                        &mut result.summary,
                        Some(row.path()),
                        SkippedFileReason::MatchLimitReached,
                        "the configured match limit was reached; this match list is incomplete",
                    );
                    break;
                }
            }
        }

        if matches!(
            result.status,
            SearchStatus::Cancelled | SearchStatus::ResourceLimit
        ) {
            break;
        }
        result.summary.roots_scanned = result.summary.roots_scanned.max(root_index + 1);
    }

    if result.status == SearchStatus::Complete && cancellation.is_cancelled() {
        result.status = SearchStatus::Cancelled;
        result.summary.match_list_complete = false;
    }
    if result.status != SearchStatus::Complete {
        result.summary.match_list_complete = false;
    }
    report_progress(SearchProgress {
        generation: request.generation,
        phase: if result.status == SearchStatus::Cancelled {
            SearchProgressPhase::Cancelled
        } else {
            SearchProgressPhase::Complete
        },
        roots_completed: roots_total.saturating_sub(result.summary.roots_unfinished),
        roots_total,
        files_examined: result.summary.files_examined,
        bytes_read: result.summary.bytes_read,
        matches_found: result.summary.matches_found,
        current_root: None,
    });
    result
}

fn count_file_rows(rows: &[RenameScanRow]) -> usize {
    rows.iter()
        .filter(|row| row.kind() == SnapshotEntryKind::File)
        .count()
}

fn insert_path_windows_ordinal(paths: &mut Vec<PathBuf>, candidate: PathBuf) -> bool {
    match paths
        .binary_search_by(|path| batch_rename::compare_paths_windows_ordinal(path, &candidate))
    {
        Ok(_) => false,
        Err(index) => {
            paths.insert(index, candidate);
            true
        }
    }
}

fn search_status_from_scan(status: RenameScanStatus) -> SearchStatus {
    match status {
        RenameScanStatus::Complete => SearchStatus::Complete,
        RenameScanStatus::Cancelled => SearchStatus::Cancelled,
        RenameScanStatus::ResourceLimit => SearchStatus::ResourceLimit,
        RenameScanStatus::Unsupported => SearchStatus::Unsupported,
        RenameScanStatus::Partial
        | RenameScanStatus::InvalidRoot
        | RenameScanStatus::Inaccessible
        | RenameScanStatus::ChangedDuringScan => SearchStatus::Partial,
    }
}

fn merge_status(current: SearchStatus, next: SearchStatus) -> SearchStatus {
    use SearchStatus::*;
    match (current, next) {
        (Cancelled, _) | (_, Cancelled) => Cancelled,
        (ResourceLimit, _) | (_, ResourceLimit) => ResourceLimit,
        (Partial, _) | (_, Partial) => Partial,
        (Unsupported, Complete) | (Complete, Unsupported) | (Unsupported, Unsupported) => {
            Unsupported
        }
        _ => Complete,
    }
}

fn record_scan_issues(summary: &mut SearchSummary, scan: &RenameScanResult) {
    for issue in scan.summary().issues() {
        let reason = match issue.kind() {
            RenameScanIssueKind::Inaccessible => SkippedFileReason::Unreadable,
            RenameScanIssueKind::ReparsePointExcluded => SkippedFileReason::ReparsePoint,
            _ => SkippedFileReason::InvalidNativeEvidence,
        };
        record_issue(summary, issue.path(), reason, issue.message());
    }
}

fn skipped_reason_from_read(kind: VerifiedReadFailureKind) -> SkippedFileReason {
    match kind {
        VerifiedReadFailureKind::TooLarge => SkippedFileReason::TooLarge,
        VerifiedReadFailureKind::Changed | VerifiedReadFailureKind::IncompleteScan => {
            SkippedFileReason::ChangedDuringRead
        }
        VerifiedReadFailureKind::Inaccessible => SkippedFileReason::Unreadable,
        VerifiedReadFailureKind::Unsupported | VerifiedReadFailureKind::InvalidEvidence => {
            SkippedFileReason::InvalidNativeEvidence
        }
        VerifiedReadFailureKind::NotRegularFile => SkippedFileReason::ReparsePoint,
        VerifiedReadFailureKind::Cancelled => SkippedFileReason::ChangedDuringRead,
    }
}

fn decode_supported_utf8(bytes: &[u8]) -> Option<(TextEncoding, usize, &str)> {
    if bytes.starts_with(&[0x00, 0x00, 0xFE, 0xFF])
        || bytes.starts_with(&[0xFF, 0xFE, 0x00, 0x00])
        || bytes.starts_with(&[0xFF, 0xFE])
        || bytes.starts_with(&[0xFE, 0xFF])
    {
        return None;
    }
    let (encoding, bom_offset) = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        (TextEncoding::Utf8Bom, 3)
    } else {
        (TextEncoding::Utf8, 0)
    };
    let text_bytes = bytes.get(bom_offset..)?;
    if text_bytes.contains(&0) {
        return None;
    }
    let text = std::str::from_utf8(text_bytes).ok()?;
    Some((encoding, bom_offset, text))
}

fn classify_unsupported_bytes(bytes: &[u8]) -> SkippedFileReason {
    if bytes.starts_with(&[0x00, 0x00, 0xFE, 0xFF])
        || bytes.starts_with(&[0xFF, 0xFE, 0x00, 0x00])
        || bytes.starts_with(&[0xFF, 0xFE])
        || bytes.starts_with(&[0xFE, 0xFF])
    {
        SkippedFileReason::UnsupportedEncoding
    } else if bytes.contains(&0) {
        SkippedFileReason::Binary
    } else {
        SkippedFileReason::InvalidUtf8
    }
}

struct SearchWorkerJob {
    request: TextSearchRequest,
    cancellation: ScanCancellation,
    reply: mpsc::Sender<TextSearchResult>,
}

#[derive(Clone)]
struct ActiveSearch {
    generation: SearchGeneration,
    cancellation: ScanCancellation,
}

struct SearchWorkerState {
    latest_generation: Option<SearchGeneration>,
    pending: Option<SearchWorkerJob>,
    active: Option<ActiveSearch>,
    shutdown: bool,
}

#[derive(Default)]
struct SearchRuntime {
    latest_generation: Option<SearchGeneration>,
    progress: Option<SearchProgress>,
}

/// One bounded worker owns all replacement-search discovery. Submitting a
/// newer generation cancels active work and coalesces pending work to one job.
pub struct SearchCoordinator {
    state: Arc<(Mutex<SearchWorkerState>, Condvar)>,
    runtime: Arc<Mutex<SearchRuntime>>,
    worker: Option<JoinHandle<()>>,
}

impl SearchCoordinator {
    pub fn new() -> std::io::Result<Self> {
        let state = Arc::new((
            Mutex::new(SearchWorkerState {
                latest_generation: None,
                pending: None,
                active: None,
                shutdown: false,
            }),
            Condvar::new(),
        ));
        let runtime = Arc::new(Mutex::new(SearchRuntime::default()));
        let worker_state = Arc::clone(&state);
        let worker_runtime = Arc::clone(&runtime);
        let worker = thread::Builder::new()
            .name("text-search-worker".into())
            .spawn(move || search_worker_loop(worker_state, worker_runtime))?;
        Ok(Self {
            state,
            runtime,
            worker: Some(worker),
        })
    }

    pub fn submit(&self, request: TextSearchRequest) -> std::io::Result<SearchTicket> {
        let (reply, receiver) = mpsc::channel();
        let generation = request.generation;
        let (lock, wake) = &*self.state;
        let mut state = lock.lock().map_err(|_| search_worker_poisoned())?;
        if state.shutdown {
            return Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "text search worker is shutting down",
            ));
        }
        if let Some(latest) = state.latest_generation
            && generation <= latest
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "search generation {} must be newer than the latest generation {}",
                    generation.0, latest.0
                ),
            ));
        }
        if let Some(active) = &state.active {
            active.cancellation.cancel();
        }
        if let Some(pending) = state.pending.take() {
            pending.cancellation.cancel();
        }
        state.latest_generation = Some(generation);
        let cancellation = ScanCancellation::new();
        state.pending = Some(SearchWorkerJob {
            request,
            cancellation: cancellation.clone(),
            reply,
        });
        if let Ok(mut runtime) = self.runtime.lock() {
            runtime.latest_generation = Some(generation);
            runtime.progress = Some(SearchProgress {
                generation,
                phase: SearchProgressPhase::Queued,
                roots_completed: 0,
                roots_total: 0,
                files_examined: 0,
                bytes_read: 0,
                matches_found: 0,
                current_root: None,
            });
        }
        wake.notify_one();
        Ok(SearchTicket {
            generation,
            cancellation,
            receiver,
        })
    }

    pub fn cancel_generation(&self, generation: SearchGeneration) {
        let (lock, _) = &*self.state;
        if let Ok(mut state) = lock.lock() {
            if let Some(active) = &state.active
                && active.generation == generation
            {
                active.cancellation.cancel();
            }
            if state
                .pending
                .as_ref()
                .is_some_and(|pending| pending.request.generation == generation)
                && let Some(pending) = state.pending.take()
            {
                pending.cancellation.cancel();
            }
        }
        if let Ok(mut runtime) = self.runtime.lock()
            && runtime.latest_generation == Some(generation)
            && let Some(progress) = runtime.progress.as_mut()
        {
            progress.phase = SearchProgressPhase::Cancelled;
        }
    }

    /// Return the one coalesced progress snapshot only for the current search.
    pub fn progress(&self, generation: SearchGeneration) -> Option<SearchProgress> {
        self.runtime.lock().ok().and_then(|runtime| {
            (runtime.latest_generation == Some(generation))
                .then(|| runtime.progress.clone())
                .flatten()
        })
    }

    /// Request nonblocking shutdown, then poll `try_reap` or call `shutdown`
    /// away from the UI frame path to join the worker.
    pub fn request_shutdown(&self) {
        self.signal_shutdown();
    }

    pub fn try_reap(&mut self) -> std::io::Result<bool> {
        let Some(worker) = self.worker.as_ref() else {
            return Ok(true);
        };
        if !worker.is_finished() {
            return Ok(false);
        }
        let Some(worker) = self.worker.take() else {
            return Ok(true);
        };
        worker
            .join()
            .map_err(|_| std::io::Error::other("text search worker panicked"))?;
        Ok(true)
    }

    pub fn shutdown(mut self) -> std::io::Result<()> {
        self.signal_shutdown();
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| std::io::Error::other("text search worker panicked"))?;
        }
        Ok(())
    }

    fn signal_shutdown(&self) {
        let (lock, wake) = &*self.state;
        if let Ok(mut state) = lock.lock() {
            state.shutdown = true;
            if let Some(active) = &state.active {
                active.cancellation.cancel();
            }
            if let Some(pending) = state.pending.take() {
                pending.cancellation.cancel();
            }
            wake.notify_all();
        }
    }
}

impl Drop for SearchCoordinator {
    fn drop(&mut self) {
        self.signal_shutdown();
        // As with the rename worker, Drop signals cancellation without joining
        // a potentially blocked native filesystem call on the caller's path.
    }
}

pub struct SearchTicket {
    generation: SearchGeneration,
    cancellation: ScanCancellation,
    receiver: mpsc::Receiver<TextSearchResult>,
}

impl SearchTicket {
    pub fn generation(&self) -> SearchGeneration {
        self.generation
    }

    pub fn cancel(&self) {
        self.cancellation.cancel();
    }

    pub fn try_recv(&self) -> Result<Option<TextSearchResult>, mpsc::TryRecvError> {
        match self.receiver.try_recv() {
            Ok(result) => Ok(Some(result)),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(error @ mpsc::TryRecvError::Disconnected) => Err(error),
        }
    }
}

fn search_worker_loop(
    worker_state: Arc<(Mutex<SearchWorkerState>, Condvar)>,
    runtime: Arc<Mutex<SearchRuntime>>,
) {
    loop {
        let (lock, wake) = &*worker_state;
        let mut state = match lock.lock() {
            Ok(state) => state,
            Err(_) => return,
        };
        while state.pending.is_none() && !state.shutdown {
            state = match wake.wait(state) {
                Ok(state) => state,
                Err(_) => return,
            };
        }
        if state.shutdown {
            return;
        }
        let Some(job) = state.pending.take() else {
            continue;
        };
        let generation = job.request.generation;
        state.active = Some(ActiveSearch {
            generation,
            cancellation: job.cancellation.clone(),
        });
        drop(state);

        let progress_runtime = Arc::clone(&runtime);
        let result = execute_search(job.request, &job.cancellation, |progress| {
            if let Ok(mut runtime) = progress_runtime.lock()
                && runtime.latest_generation == Some(progress.generation)
            {
                runtime.progress = Some(progress);
            }
        });
        let _ = job.reply.send(result);

        let (lock, _) = &*worker_state;
        if let Ok(mut state) = lock.lock()
            && state
                .active
                .as_ref()
                .is_some_and(|active| active.generation == generation)
        {
            state.active = None;
        }
    }
}

fn search_worker_poisoned() -> std::io::Error {
    std::io::Error::other("text search worker state was poisoned")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn small_limits() -> SearchLimits {
        SearchLimits::new(128, 4_096, 64, 64, 32, 32, 16).expect("valid limits")
    }

    #[test]
    fn match_model_uses_raw_offsets_and_bounds_zero_width_results() {
        let raw = b"\xEF\xBB\xBFHello \xF0\x9F\x99\x82\r\nhello\nHELLO";
        let text = std::str::from_utf8(&raw[3..]).expect("UTF-8 after BOM");
        let pattern = SearchPattern::compile("hello", SearchPatternMode::Literal, false)
            .expect("literal pattern");
        let cancellation = ScanCancellation::new();
        let first = build_matches(
            text,
            raw.len(),
            3,
            Path::new("sample.txt"),
            [9; 32],
            SearchRevision(7),
            SearchGeneration(11),
            &pattern,
            small_limits(),
            32,
            &cancellation,
        );
        let second = build_matches(
            text,
            raw.len(),
            3,
            Path::new("sample.txt"),
            [9; 32],
            SearchRevision(7),
            SearchGeneration(11),
            &pattern,
            small_limits(),
            32,
            &cancellation,
        );
        assert_eq!(first.status, MatchBuildStatus::Complete);
        assert_eq!(first.matches, second.matches);
        assert!(Arc::ptr_eq(
            &first.matches[0].id.snapshot_path,
            &first.matches[1].id.snapshot_path,
        ));
        assert_eq!(
            first
                .matches
                .iter()
                .map(MatchSpan::raw_byte_range)
                .collect::<Vec<_>>(),
            [3..8, 15..20, 21..26]
        );
        assert_eq!(
            first
                .matches
                .iter()
                .map(MatchSpan::first_line)
                .collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert_eq!(&raw[first.matches[0].raw_byte_range()], b"Hello");

        let zero_width = SearchPattern::compile("(?m)^", SearchPatternMode::RustRegex, false)
            .expect("zero-width regex");
        let limited =
            SearchLimits::new(128, 4_096, 64, 2, 2, 32, 16).expect("bounded match limits");
        let zero_matches = build_matches(
            text,
            raw.len(),
            3,
            Path::new("sample.txt"),
            [9; 32],
            SearchRevision(7),
            SearchGeneration(11),
            &zero_width,
            limited,
            2,
            &cancellation,
        );
        assert_eq!(zero_matches.matches.len(), 2);
        assert_eq!(zero_matches.status, MatchBuildStatus::LimitReached);
        assert!(zero_matches.matches.iter().all(|span| {
            let range = span.raw_byte_range();
            range.start == range.end
        }));
        assert_eq!(zero_matches.matches[0].raw_byte_range(), 3..3);
        assert_eq!(zero_matches.matches[1].raw_byte_range(), 15..15);
    }

    fn in_memory_search_result(
        raw: &[u8],
        pattern_source: &str,
        pattern_mode: SearchPatternMode,
        generation: SearchGeneration,
        file_name: &str,
    ) -> TextSearchResult {
        let root = ExplicitDirectoryRoot::new(std::env::temp_dir()).expect("absolute test root");
        let path = root.as_path().join(file_name);
        let revision = SearchRevision(8);
        let pattern = SearchPattern::compile(pattern_source, pattern_mode, true)
            .expect("valid test pattern");
        let (encoding, bom_offset, text) = decode_supported_utf8(raw).expect("UTF-8 fixture");
        let cancellation = ScanCancellation::new();
        let limits = small_limits();
        let content_sha256 = <[u8; 32]>::from(Sha256::digest(raw));
        let built = build_matches(
            text,
            raw.len(),
            bom_offset,
            &path,
            content_sha256,
            revision,
            generation,
            &pattern,
            limits,
            limits.max_matches_total(),
            &cancellation,
        );
        assert_eq!(built.status, MatchBuildStatus::Complete);
        let match_count = built.matches.len();
        let matched_files = if built.matches.is_empty() {
            Vec::new()
        } else {
            vec![SearchFileSnapshot {
                root: root.as_path().to_path_buf(),
                relative_path: PathBuf::from(file_name),
                path,
                display_path: file_name.to_owned(),
                root_identity: NativeFileIdentity::default(),
                file_identity: NativeFileIdentity::default(),
                parent_identity: NativeFileIdentity::default(),
                length: u64::try_from(raw.len()).expect("fixture length fits u64"),
                modified: SystemTime::UNIX_EPOCH,
                raw_bytes: Arc::from(raw.to_vec()),
                content_sha256,
                encoding,
                bom_offset,
                matches: built.matches,
            }]
        };
        let request = TextSearchRequest::new(
            vec![root],
            SearchFilterSpec::default(),
            pattern,
            revision,
            generation,
            limits,
        )
        .expect("valid in-memory search request");
        TextSearchResult {
            request,
            status: SearchStatus::Complete,
            summary: SearchSummary {
                files_examined: 1,
                files_searched: 1,
                files_matched: if match_count > 0 { 1 } else { 0 },
                matches_found: match_count,
                bytes_read: u64::try_from(raw.len()).expect("fixture length fits u64"),
                match_list_complete: true,
                ..SearchSummary::default()
            },
            matched_files,
        }
    }

    #[test]
    fn replacement_templates_preserve_literal_text_and_validate_rust_captures() {
        let generation = SearchGeneration(21);
        let result = in_memory_search_result(
            b"cat-7 dog",
            r"(?<word>[a-z]+)(?:-(?<number>[0-9]+))?",
            SearchPatternMode::RustRegex,
            generation,
            "template.txt",
        );
        let matches = result.matched_files()[0].matches();

        let literal = ReplacementSpec::new(
            "$1\\path\r\n🌐\n",
            ReplacementMode::Literal,
            ReplacementRevision(1),
        )
        .compile(result.pattern())
        .expect("literal replacement compiles without interpolation");
        let literal_candidate = result
            .preview_replacement(&literal, &[matches[0].id().clone()])
            .expect("literal preview");
        assert_eq!(
            literal_candidate.files()[0].output_bytes().as_ref(),
            "$1\\path\r\n🌐\n dog".as_bytes()
        );

        let captures = ReplacementSpec::new(
            "$0|$1|${word}|${number}|$$",
            ReplacementMode::RustRegexCaptures,
            ReplacementRevision(2),
        )
        .compile(result.pattern())
        .expect("known Rust capture references compile");
        let capture_candidate = result
            .preview_replacement(
                &captures,
                &[matches[0].id().clone(), matches[1].id().clone()],
            )
            .expect("capture preview");
        assert_eq!(
            capture_candidate.files()[0].output_bytes().as_ref(),
            b"cat-7|cat|cat|7|$ dog|dog|dog||$"
        );

        let braced_numeric = ReplacementSpec::new(
            "${+1}",
            ReplacementMode::RustRegexCaptures,
            ReplacementRevision(5),
        )
        .compile(result.pattern())
        .expect("braced usize references follow Rust regex parsing");
        let braced_numeric_candidate = result
            .preview_replacement(
                &braced_numeric,
                &[matches[0].id().clone(), matches[1].id().clone()],
            )
            .expect("braced numeric capture preview");
        let source = "cat-7 dog";
        let mut expanded_by_regex = String::new();
        let mut source_cursor = 0;
        for captures in result.pattern().compiled().captures_iter(source) {
            let full_match = captures.get(0).expect("full match");
            expanded_by_regex.push_str(&source[source_cursor..full_match.start()]);
            captures.expand("${+1}", &mut expanded_by_regex);
            source_cursor = full_match.end();
        }
        expanded_by_regex.push_str(&source[source_cursor..]);
        assert_eq!(expanded_by_regex, "cat dog");
        assert_eq!(
            braced_numeric_candidate.files()[0].output_bytes().as_ref(),
            expanded_by_regex.as_bytes()
        );

        let unknown = ReplacementSpec::new(
            "$missing",
            ReplacementMode::RustRegexCaptures,
            ReplacementRevision(3),
        )
        .compile(result.pattern())
        .expect_err("unknown names are diagnosed before preview");
        assert_eq!(unknown.revision(), ReplacementRevision(3));
        assert_eq!(unknown.kind(), ReplacementDiagnosticKind::UnknownCapture);
        let invalid_index = ReplacementSpec::new(
            "$9",
            ReplacementMode::RustRegexCaptures,
            ReplacementRevision(4),
        )
        .compile(result.pattern())
        .expect_err("unknown numeric groups are diagnosed before preview");
        assert_eq!(
            invalid_index.kind(),
            ReplacementDiagnosticKind::InvalidCaptureIndex
        );
    }

    #[test]
    fn selective_preview_is_snapshot_bound_bounded_and_byte_faithful() {
        let generation = SearchGeneration(31);
        let raw = b"\xEF\xBB\xBFhit\r\nhit\nlast";
        let result = in_memory_search_result(
            raw,
            "hit",
            SearchPatternMode::Literal,
            generation,
            "mixed-lines.txt",
        );
        let matches = result.matched_files()[0].matches();
        let second_id = matches[1].id().clone();
        assert_eq!(second_id.ordinal(), 1);
        let replacement = ReplacementSpec::new(
            "hit!",
            ReplacementMode::Literal,
            ReplacementRevision(10),
        )
        .compile(result.pattern())
        .expect("literal replacement");
        let candidate = result
            .preview_replacement(&replacement, std::slice::from_ref(&second_id))
            .expect("selected second-match preview");
        assert_eq!(candidate.readiness(), ReplacementCandidateReadiness::Ready);
        assert_eq!(candidate.search_generation(), generation);
        assert_eq!(candidate.selected_match_ids()[0].ordinal(), 1);
        assert_eq!(
            candidate.files()[0].output_bytes().as_ref(),
            b"\xEF\xBB\xBFhit\r\nhit!\nlast"
        );
        assert!(candidate.files()[0].diff_context().is_some());
        assert_eq!(candidate.summary().affected_files(), 1);
        assert_eq!(candidate.summary().approved_matches(), 1);
        assert_eq!(candidate.summary().changed_matches(), 1);
        assert_eq!(candidate.summary().estimated_changed_bytes(), 7);

        let no_op = ReplacementSpec::new(
            "hit",
            ReplacementMode::Literal,
            ReplacementRevision(11),
        )
        .compile(result.pattern())
        .expect("no-op replacement");
        let no_op_candidate = result
            .preview_replacement(&no_op, std::slice::from_ref(&second_id))
            .expect("no-op preview");
        assert_eq!(no_op_candidate.readiness(), ReplacementCandidateReadiness::NoOp);
        assert!(Arc::ptr_eq(
            no_op_candidate.files()[0].output_bytes(),
            result.matched_files()[0].raw_bytes()
        ));
        assert_eq!(no_op_candidate.summary().selected_noop_matches(), 1);
        assert_eq!(no_op_candidate.summary().affected_files(), 0);

        assert_eq!(
            result
                .preview_replacement(&replacement, &[second_id.clone(), second_id.clone()])
                .expect_err("duplicate selected IDs are rejected")
                .kind(),
            ReplacementPreviewErrorKind::DuplicateSelection
        );
        let newer_scan = in_memory_search_result(
            raw,
            "hit",
            SearchPatternMode::Literal,
            SearchGeneration(32),
            "mixed-lines.txt",
        );
        let newer_replacement = ReplacementSpec::new(
            "hit!",
            ReplacementMode::Literal,
            ReplacementRevision(12),
        )
        .compile(newer_scan.pattern())
        .expect("replacement for newer scan");
        assert_eq!(
            newer_scan
                .preview_replacement(&newer_replacement, &[second_id.clone()])
                .expect_err("an older scan selection is stale")
                .kind(),
            ReplacementPreviewErrorKind::StaleSelection
        );
        let other_file = in_memory_search_result(
            raw,
            "hit",
            SearchPatternMode::Literal,
            generation,
            "other.txt",
        );
        let other_replacement = ReplacementSpec::new(
            "hit!",
            ReplacementMode::Literal,
            ReplacementRevision(13),
        )
        .compile(other_file.pattern())
        .expect("replacement for other file");
        assert_eq!(
            other_file
                .preview_replacement(&other_replacement, &[second_id.clone()])
                .expect_err("an ID from another file is foreign")
                .kind(),
            ReplacementPreviewErrorKind::ForeignSelection
        );

        let mut incomplete = result.clone();
        incomplete.summary.match_list_complete = false;
        let incomplete_candidate = incomplete
            .preview_replacement(&replacement, std::slice::from_ref(&second_id))
            .expect("partial results can show a blocked preview");
        assert_eq!(
            incomplete_candidate.readiness(),
            ReplacementCandidateReadiness::IncompleteDiscovery
        );
        assert!(!incomplete_candidate.discovery_complete());

        let adjacent = in_memory_search_result(
            b"aa",
            "a",
            SearchPatternMode::Literal,
            SearchGeneration(33),
            "adjacent.txt",
        );
        let adjacent_replacement = ReplacementSpec::new(
            "X",
            ReplacementMode::Literal,
            ReplacementRevision(14),
        )
        .compile(adjacent.pattern())
        .expect("adjacent replacement");
        let adjacent_ids = adjacent.matched_files()[0]
            .matches()
            .iter()
            .map(|span| span.id().clone())
            .collect::<Vec<_>>();
        assert_eq!(
            adjacent
                .preview_replacement(&adjacent_replacement, &adjacent_ids)
                .expect("adjacent matches remain valid")
                .files()[0]
                .output_bytes()
                .as_ref(),
            b"XX"
        );

        let zero_width = in_memory_search_result(
            b"a\nb",
            "(?m)^",
            SearchPatternMode::RustRegex,
            SearchGeneration(34),
            "zero-width.txt",
        );
        let zero_width_replacement = ReplacementSpec::new(
            ">",
            ReplacementMode::Literal,
            ReplacementRevision(15),
        )
        .compile(zero_width.pattern())
        .expect("zero-width replacement");
        let first_zero_width = zero_width.matched_files()[0].matches()[0].id().clone();
        assert_eq!(
            zero_width
                .preview_replacement(&zero_width_replacement, &[first_zero_width])
                .expect("zero-width insertion applies once")
                .files()[0]
                .output_bytes()
                .as_ref(),
            b">a\nb"
        );

        let limited_replacement = ReplacementSpec::new(
            "x".repeat(129),
            ReplacementMode::Literal,
            ReplacementRevision(16),
        )
        .compile(adjacent.pattern())
        .expect("oversized output is diagnosed during preview");
        let limited_candidate = adjacent
            .preview_replacement(&limited_replacement, &[adjacent_ids[0].clone()])
            .expect("output-limit failures remain visible in the candidate");
        assert_eq!(limited_candidate.readiness(), ReplacementCandidateReadiness::Blocked);
        assert_eq!(
            limited_candidate.files()[0]
                .diagnostic()
                .expect("file carries output-limit diagnostic")
                .kind(),
            ReplacementDiagnosticKind::OutputLimitExceeded
        );

        // The aggregate output budget applies to candidate bytes. A later
        // source may be larger than the remaining budget and still fit after
        // its selected matches shrink.
        let first_expanding = in_memory_search_result(
            &vec![b'b'; 30],
            "aaaa|b",
            SearchPatternMode::RustRegex,
            SearchGeneration(35),
            "expanding.txt",
        );
        let second_shrinking = in_memory_search_result(
            &vec![b'a'; 100],
            "aaaa|b",
            SearchPatternMode::RustRegex,
            SearchGeneration(35),
            "shrinking.txt",
        );
        let mut aggregate = first_expanding.clone();
        aggregate
            .matched_files
            .extend(second_shrinking.matched_files.iter().cloned());
        aggregate.summary.files_examined = 2;
        aggregate.summary.files_searched = 2;
        aggregate.summary.files_matched = 2;
        aggregate.summary.matches_found = 55;
        aggregate.summary.bytes_read = 130;
        aggregate.request.limits.max_total_bytes = 140;
        let aggregate_selection = aggregate
            .matched_files()
            .iter()
            .flat_map(|file| file.matches().iter().map(|span| span.id().clone()))
            .collect::<Vec<_>>();
        let aggregate_replacement = ReplacementSpec::new(
            "XX",
            ReplacementMode::Literal,
            ReplacementRevision(17),
        )
        .compile(aggregate.pattern())
        .expect("aggregate replacement");
        let aggregate_candidate = aggregate
            .preview_replacement(&aggregate_replacement, &aggregate_selection)
            .expect("shrinking output fits the aggregate budget");
        assert_eq!(aggregate_candidate.readiness(), ReplacementCandidateReadiness::Ready);
        assert_eq!(
            aggregate_candidate
                .files()
                .iter()
                .map(|file| file.output_bytes().len())
                .sum::<usize>(),
            110
        );

        let mut invalid_range = result.clone();
        let invalid_id = {
            let span = &mut invalid_range.matched_files[0].matches[0];
            span.raw_byte_range = 3..raw.len() + 1;
            span.id.raw_byte_range = span.raw_byte_range.clone();
            span.id.clone()
        };
        assert_eq!(
            invalid_range
                .preview_replacement(&replacement, &[invalid_id])
                .expect_err("invalid original ranges are rejected")
                .kind(),
            ReplacementPreviewErrorKind::InvalidMatchRange
        );
    }

    #[cfg(windows)]
    #[test]
    fn native_search_is_root_bound_filtered_bounded_and_read_only() {
        use std::fs;
        use tempfile::Builder;

        let fixture_parent = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("productivity_expansion");
        fs::create_dir_all(&fixture_parent).expect("create fixture parent inside target");
        let temp = Builder::new()
            .prefix("m9a-search-")
            .tempdir_in(&fixture_parent)
            .expect("create scoped fixture");
        let root_path = temp.path().join("root");
        let nested_path = root_path.join("nested");
        let excluded_directory = root_path.join("excluded_dir");
        fs::create_dir_all(&nested_path).expect("create fixture roots");
        fs::create_dir_all(&excluded_directory).expect("create excluded directory");

        let original = b"\xEF\xBB\xBFneedle one\r\nneedle two\nlast".to_vec();
        let nested = b"needle nested".to_vec();
        let binary = b"needle\0binary".to_vec();
        let utf16 = [
            0xFF, 0xFE, b'n', 0, b'e', 0, b'e', 0, b'd', 0, b'l', 0, b'e', 0,
        ];
        fs::write(root_path.join("matches.txt"), &original).expect("write match fixture");
        fs::write(root_path.join("excluded.txt"), b"needle excluded")
            .expect("write excluded fixture");
        let excluded_descendant = excluded_directory.join("child.txt");
        fs::write(&excluded_descendant, b"needle in excluded subtree")
            .expect("write excluded subtree fixture");
        fs::write(root_path.join("other.rs"), b"needle rust")
            .expect("write extension-filter fixture");
        fs::write(root_path.join("binary.txt"), binary).expect("write binary fixture");
        fs::write(root_path.join("utf16.txt"), utf16).expect("write UTF-16 fixture");
        fs::write(root_path.join("large.txt"), vec![b'x'; 256]).expect("write oversized fixture");
        fs::write(nested_path.join("nested.txt"), &nested).expect("write nested fixture");

        let roots = vec![
            ExplicitDirectoryRoot::new(root_path.clone()).expect("explicit root"),
            ExplicitDirectoryRoot::new(nested_path.clone()).expect("overlapping explicit root"),
        ];
        let pattern = SearchPattern::compile("needle", SearchPatternMode::Literal, true)
            .expect("search pattern");
        let filters = SearchFilterSpec {
            include_extensions: vec!["txt".to_owned()],
            include_path_globs: vec!["**/*.txt".to_owned()],
            exclude_paths: vec![PathBuf::from("excluded.txt"), PathBuf::from("excluded_dir")],
            exclude_path_globs: Vec::new(),
        };
        let request = TextSearchRequest::new(
            roots,
            filters,
            pattern,
            SearchRevision(21),
            SearchGeneration(41),
            small_limits(),
        )
        .expect("valid scoped request")
        .with_recursive(true);
        let result = execute_search(request.clone(), &ScanCancellation::new(), |_| {});
        assert_eq!(result.status(), SearchStatus::Complete);
        assert!(result.summary().match_list_complete());
        assert_eq!(result.summary().matches_found(), 3);
        assert_eq!(result.summary().files_matched(), 2);
        assert!(
            !result
                .matched_files()
                .iter()
                .any(|snapshot| snapshot.path() == excluded_descendant)
        );
        assert!(
            result
                .summary()
                .skipped_by_reason()
                .contains_key(&SkippedFileReason::Binary)
        );
        assert!(
            result
                .summary()
                .skipped_by_reason()
                .contains_key(&SkippedFileReason::UnsupportedEncoding)
        );
        assert!(
            result
                .summary()
                .skipped_by_reason()
                .contains_key(&SkippedFileReason::TooLarge)
        );
        let root_snapshot = result
            .matched_files()
            .iter()
            .find(|snapshot| snapshot.path() == root_path.join("matches.txt"))
            .expect("root match snapshot");
        assert_eq!(root_snapshot.encoding(), TextEncoding::Utf8Bom);
        assert_eq!(root_snapshot.bom_offset(), 3);
        assert_eq!(root_snapshot.raw_bytes().as_ref(), &original);
        assert_eq!(root_snapshot.matches()[0].first_line(), 1);
        assert_eq!(root_snapshot.matches()[1].first_line(), 2);
        assert_eq!(
            fs::read(root_path.join("matches.txt")).expect("source bytes"),
            original
        );
        assert_eq!(
            fs::read(nested_path.join("nested.txt")).expect("nested source bytes"),
            nested
        );
        assert!(result.clone().accept(SearchGeneration(42)).is_err());

        let cancellation = ScanCancellation::new();
        cancellation.cancel();
        let cancelled = execute_search(request, &cancellation, |_| {});
        assert_eq!(cancelled.status(), SearchStatus::Cancelled);
        assert!(!cancelled.summary().match_list_complete());
    }
}
