//! Pure request and dry-run preview model for same-directory batch renames.
//!
//! This module consumes a caller-owned immutable file snapshot. Entry
//! references identify rows within that snapshot only; they are not native
//! filesystem identities and do not prove that a path is still current.
//! Preview generation performs no filesystem access or mutation.

use regex::Regex;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::{OsStr, OsString};
use std::ops::Range;
use std::path::{Component, Path, PathBuf};

pub const MAX_RENAME_RULES: usize = 32;
pub const MAX_SNAPSHOT_ENTRIES: usize = 10_000;
pub const MAX_SELECTED_ENTRIES: usize = 2_048;
pub const MAX_RULE_PARAMETER_BYTES: usize = 255;
pub const MAX_INTERMEDIATE_NAME_BYTES: usize = 1_024;
pub const MAX_WINDOWS_NAME_UTF16_UNITS: usize = 255;
/// Includes the terminating NUL, matching the Windows extended-path limit.
pub const MAX_WINDOWS_PATH_UTF16_UNITS_WITH_NUL: usize = 32_767;

/// The single path and filename equivalence policy used by preview collision
/// analysis and intended for the later executor: Windows ordinal comparison
/// with case ignored, applied component by component. Original spelling is
/// retained in every source and target path.
pub const PATH_EQUIVALENCE_POLICY: &str =
    "Windows ordinal, case-insensitive path-component comparison";

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ExplicitDirectoryRoot(PathBuf);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootError {
    Empty,
    MustBeAbsolute,
    ParentTraversal,
    PathTooLong,
}

impl ExplicitDirectoryRoot {
    /// Construct an explicit lexical root without probing the filesystem.
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, RootError> {
        let path = path.into();
        if path.as_os_str().is_empty() {
            return Err(RootError::Empty);
        }
        if !path.is_absolute() {
            return Err(RootError::MustBeAbsolute);
        }
        if path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(RootError::ParentTraversal);
        }
        if !path_fits_windows_limit(&path) {
            return Err(RootError::PathTooLong);
        }
        Ok(Self(path))
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

/// Stable row reference assigned by the snapshot producer. This is not a
/// filesystem identity and must not be used as a freshness or authorization
/// check by an executor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SnapshotEntryRef(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntryInclusion {
    Included,
    Excluded,
}

/// Snapshot row kind. Only regular files are rename candidates; every row
/// remains part of sibling-occupancy analysis regardless of kind or inclusion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SnapshotEntryKind {
    #[default]
    File,
    Directory,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameSnapshotEntry {
    entry_ref: SnapshotEntryRef,
    source_path: PathBuf,
    source_order: u64,
    inclusion: EntryInclusion,
    selected: bool,
    kind: SnapshotEntryKind,
}

impl RenameSnapshotEntry {
    pub fn new(
        entry_ref: SnapshotEntryRef,
        source_path: impl Into<PathBuf>,
        source_order: u64,
        inclusion: EntryInclusion,
        selected: bool,
    ) -> Self {
        Self {
            entry_ref,
            source_path: source_path.into(),
            source_order,
            inclusion,
            selected,
            kind: SnapshotEntryKind::File,
        }
    }

    pub fn with_kind(mut self, kind: SnapshotEntryKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn entry_ref(&self) -> SnapshotEntryRef {
        self.entry_ref
    }

    pub fn source_path(&self) -> &Path {
        &self.source_path
    }

    pub fn source_order(&self) -> u64 {
        self.source_order
    }

    pub fn inclusion(&self) -> EntryInclusion {
        self.inclusion
    }

    pub fn is_selected(&self) -> bool {
        self.selected
    }

    pub fn kind(&self) -> SnapshotEntryKind {
        self.kind
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SnapshotRevision(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SelectionRevision(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RulesRevision(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotEntryPathError {
    PathTooLong,
    MustBeAbsolute,
    ParentTraversal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotError {
    TooManyEntries {
        count: usize,
        limit: usize,
    },
    TooManySelected {
        count: usize,
        limit: usize,
    },
    DuplicateEntryReference(SnapshotEntryRef),
    MissingEntryReference(SnapshotEntryRef),
    UnsupportedEntryPath {
        entry_ref: SnapshotEntryRef,
        reason: SnapshotEntryPathError,
    },
    RevisionOverflow,
}

/// An immutable, caller-supplied view of root-relative file rows and all
/// sibling occupants needed to detect destination conflicts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameInputSnapshot {
    root: ExplicitDirectoryRoot,
    recursive: bool,
    revision: SnapshotRevision,
    selection_revision: SelectionRevision,
    entries: Vec<RenameSnapshotEntry>,
}

impl RenameInputSnapshot {
    /// Recursion is disabled by default.
    pub fn new(
        root: ExplicitDirectoryRoot,
        entries: Vec<RenameSnapshotEntry>,
    ) -> Result<Self, SnapshotError> {
        Self::validate_entries(&entries)?;
        Ok(Self {
            root,
            recursive: false,
            revision: SnapshotRevision(0),
            selection_revision: SelectionRevision(0),
            entries,
        })
    }

    fn validate_entries(entries: &[RenameSnapshotEntry]) -> Result<(), SnapshotError> {
        if entries.len() > MAX_SNAPSHOT_ENTRIES {
            return Err(SnapshotError::TooManyEntries {
                count: entries.len(),
                limit: MAX_SNAPSHOT_ENTRIES,
            });
        }
        let selected_count = entries.iter().filter(|entry| entry.selected).count();
        if selected_count > MAX_SELECTED_ENTRIES {
            return Err(SnapshotError::TooManySelected {
                count: selected_count,
                limit: MAX_SELECTED_ENTRIES,
            });
        }
        let mut references = HashSet::with_capacity(entries.len());
        for entry in entries {
            if !path_fits_windows_limit(&entry.source_path) {
                return Err(SnapshotError::UnsupportedEntryPath {
                    entry_ref: entry.entry_ref,
                    reason: SnapshotEntryPathError::PathTooLong,
                });
            }
            if !entry.source_path.is_absolute() {
                return Err(SnapshotError::UnsupportedEntryPath {
                    entry_ref: entry.entry_ref,
                    reason: SnapshotEntryPathError::MustBeAbsolute,
                });
            }
            if safe_path_components(&entry.source_path).is_none() {
                return Err(SnapshotError::UnsupportedEntryPath {
                    entry_ref: entry.entry_ref,
                    reason: SnapshotEntryPathError::ParentTraversal,
                });
            }
            if !references.insert(entry.entry_ref) {
                return Err(SnapshotError::DuplicateEntryReference(entry.entry_ref));
            }
        }
        Ok(())
    }

    pub fn root(&self) -> &ExplicitDirectoryRoot {
        &self.root
    }

    pub fn is_recursive(&self) -> bool {
        self.recursive
    }

    pub fn revision(&self) -> SnapshotRevision {
        self.revision
    }

    pub fn selection_revision(&self) -> SelectionRevision {
        self.selection_revision
    }

    pub fn entries(&self) -> &[RenameSnapshotEntry] {
        &self.entries
    }

    pub fn with_recursive(&self, recursive: bool) -> Result<Self, SnapshotError> {
        if self.recursive == recursive {
            return Ok(self.clone());
        }
        let mut updated = self.clone();
        updated.recursive = recursive;
        updated.revision = SnapshotRevision(
            self.revision
                .0
                .checked_add(1)
                .ok_or(SnapshotError::RevisionOverflow)?,
        );
        Ok(updated)
    }

    pub fn with_selection(
        &self,
        entry_ref: SnapshotEntryRef,
        selected: bool,
    ) -> Result<Self, SnapshotError> {
        self.with_entry_updates(&[(entry_ref, Some(selected), None)])
    }

    pub fn with_inclusion(
        &self,
        entry_ref: SnapshotEntryRef,
        inclusion: EntryInclusion,
    ) -> Result<Self, SnapshotError> {
        self.with_entry_updates(&[(entry_ref, None, Some(inclusion))])
    }

    /// Atomically update selection and preview inclusion for a batch of rows.
    /// The snapshot is cloned and validated once, and a single selection
    /// revision represents the complete update.
    pub fn with_entry_updates(
        &self,
        updates: &[(SnapshotEntryRef, Option<bool>, Option<EntryInclusion>)],
    ) -> Result<Self, SnapshotError> {
        if updates.is_empty() {
            return Ok(self.clone());
        }

        let indices: HashMap<_, _> = self
            .entries
            .iter()
            .enumerate()
            .map(|(index, entry)| (entry.entry_ref, index))
            .collect();
        let mut changes: HashMap<usize, (Option<bool>, Option<EntryInclusion>)> =
            HashMap::with_capacity(updates.len());
        for (entry_ref, selected, inclusion) in updates {
            let Some(index) = indices.get(entry_ref).copied() else {
                return Err(SnapshotError::MissingEntryReference(*entry_ref));
            };
            let change = changes.entry(index).or_default();
            if let Some(selected) = selected {
                change.0 = Some(*selected);
            }
            if let Some(inclusion) = inclusion {
                change.1 = Some(*inclusion);
            }
        }

        let mut selected_count = self.entries.iter().filter(|entry| entry.selected).count();
        let mut has_changes = false;
        for (index, (selected, inclusion)) in &changes {
            let entry = &self.entries[*index];
            if let Some(selected) = selected
                && entry.selected != *selected
            {
                selected_count = if *selected {
                    selected_count + 1
                } else {
                    selected_count - 1
                };
                has_changes = true;
            }
            if inclusion.is_some_and(|inclusion| entry.inclusion != inclusion) {
                has_changes = true;
            }
        }
        if selected_count > MAX_SELECTED_ENTRIES {
            return Err(SnapshotError::TooManySelected {
                count: selected_count,
                limit: MAX_SELECTED_ENTRIES,
            });
        }
        if !has_changes {
            return Ok(self.clone());
        }

        let next_revision = SelectionRevision(
            self.selection_revision
                .0
                .checked_add(1)
                .ok_or(SnapshotError::RevisionOverflow)?,
        );
        let mut updated = self.clone();
        for (index, (selected, inclusion)) in changes {
            if let Some(selected) = selected {
                updated.entries[index].selected = selected;
            }
            if let Some(inclusion) = inclusion {
                updated.entries[index].inclusion = inclusion;
            }
        }
        updated.selection_revision = next_revision;
        Self::validate_entries(&updated.entries)?;
        Ok(updated)
    }

    /// Set revisions supplied by the owning enumeration/UI state. Exact
    /// snapshot contents are still included in frozen-plan identity checks.
    pub fn with_revisions(
        mut self,
        revision: SnapshotRevision,
        selection_revision: SelectionRevision,
    ) -> Self {
        self.revision = revision;
        self.selection_revision = selection_revision;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ExtensionPolicy {
    #[default]
    Preserve,
    Edit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CaseConversion {
    Lowercase,
    Uppercase,
}

/// Ordered name transformations. Numbering inserts the selected row's
/// zero-padded sequence value at the beginning of the current stem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenameRule {
    Prefix(String),
    Suffix(String),
    Number {
        start: u64,
        step: u64,
        padding: usize,
    },
    LiteralReplace {
        find: String,
        replace_with: String,
    },
    RegexReplace {
        pattern: String,
        replace_with: String,
    },
    Case(CaseConversion),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameRuleSet {
    revision: RulesRevision,
    extension_policy: ExtensionPolicy,
    rules: Vec<RenameRule>,
}

impl Default for RenameRuleSet {
    fn default() -> Self {
        Self::new(ExtensionPolicy::Preserve, Vec::new())
    }
}

impl RenameRuleSet {
    pub fn new(extension_policy: ExtensionPolicy, rules: Vec<RenameRule>) -> Self {
        Self {
            revision: RulesRevision(0),
            extension_policy,
            rules,
        }
    }

    pub fn with_revision(mut self, revision: RulesRevision) -> Self {
        self.revision = revision;
        self
    }

    pub fn revision(&self) -> RulesRevision {
        self.revision
    }

    pub fn extension_policy(&self) -> ExtensionPolicy {
        self.extension_policy
    }

    pub fn rules(&self) -> &[RenameRule] {
        &self.rules
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameRequest {
    snapshot: RenameInputSnapshot,
    rules: RenameRuleSet,
}

impl RenameRequest {
    pub fn new(snapshot: RenameInputSnapshot, rules: RenameRuleSet) -> Self {
        Self { snapshot, rules }
    }

    pub fn snapshot(&self) -> &RenameInputSnapshot {
        &self.snapshot
    }

    pub fn rules(&self) -> &RenameRuleSet {
        &self.rules
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleValidationErrorKind {
    TooManyRules { count: usize, limit: usize },
    ParameterTooLong { bytes: usize, limit: usize },
    EmptyLiteralPattern,
    EmptyRegexPattern,
    InvalidRegex(String),
    NumberPaddingTooLarge { padding: usize, limit: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleValidationError {
    pub rule_index: Option<usize>,
    pub kind: RuleValidationErrorKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidReason {
    InvalidRules,
    SourceOutsideRoot,
    NestedSourceWhenRecursionDisabled,
    SourcePathTooLong,
    MissingFilename,
    UnsupportedFilenameEncoding,
    InvalidTargetLeaf(TargetNameError),
    TargetPathTooLong,
    NameGrowthLimit,
    NumberOverflow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetNameError {
    Empty,
    PathSeparator,
    ForbiddenCharacter(char),
    TrailingDotOrSpace,
    ReservedDeviceName,
    TooLong { utf16_units: usize, limit: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictReason {
    DuplicateSourcePath,
    DuplicateProposedTarget,
    DestinationOccupied { occupant: SnapshotEntryRef },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewRowStatus {
    Ready,
    NoOp,
    Invalid(InvalidReason),
    Conflict(ConflictReason),
    Unselected,
    Excluded,
    Ineligible,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenamePreviewRow {
    entry_ref: SnapshotEntryRef,
    source_path: PathBuf,
    source_name: OsString,
    proposed_name: Option<OsString>,
    proposed_path: Option<PathBuf>,
    status: PreviewRowStatus,
}

impl RenamePreviewRow {
    pub fn entry_ref(&self) -> SnapshotEntryRef {
        self.entry_ref
    }

    pub fn source_path(&self) -> &Path {
        &self.source_path
    }

    pub fn source_name(&self) -> &OsStr {
        &self.source_name
    }

    pub fn proposed_name(&self) -> Option<&OsStr> {
        self.proposed_name.as_deref()
    }

    pub fn proposed_path(&self) -> Option<&Path> {
        self.proposed_path.as_deref()
    }

    pub fn status(&self) -> &PreviewRowStatus {
        &self.status
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewReadiness {
    EmptySelection,
    AllNoOp,
    Ready,
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameAction {
    source_ref: SnapshotEntryRef,
    source_path: PathBuf,
    target_path: PathBuf,
}

impl RenameAction {
    pub fn source_ref(&self) -> SnapshotEntryRef {
        self.source_ref
    }

    pub fn source_path(&self) -> &Path {
        &self.source_path
    }

    pub fn target_path(&self) -> &Path {
        &self.target_path
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RequestIdentity {
    snapshot: RenameInputSnapshot,
    rules: RenameRuleSet,
}

/// An exact immutable candidate plan. It is neither authorization nor proof
/// that source paths remain fresh; later enumeration/apply stages must
/// revalidate their own filesystem preconditions and consume this plan
/// without recomputing UI rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenamePlan {
    root: ExplicitDirectoryRoot,
    recursive: bool,
    snapshot_revision: SnapshotRevision,
    selection_revision: SelectionRevision,
    rules_revision: RulesRevision,
    selected_refs: Vec<SnapshotEntryRef>,
    actions: Vec<RenameAction>,
    identity: RequestIdentity,
}

impl RenamePlan {
    pub fn root(&self) -> &ExplicitDirectoryRoot {
        &self.root
    }

    pub fn is_recursive(&self) -> bool {
        self.recursive
    }

    pub fn snapshot_revision(&self) -> SnapshotRevision {
        self.snapshot_revision
    }

    pub fn selection_revision(&self) -> SelectionRevision {
        self.selection_revision
    }

    pub fn rules_revision(&self) -> RulesRevision {
        self.rules_revision
    }

    /// Selected included source rows in deterministic preview order, including
    /// selected no-ops. actions() contains only actual path changes.
    pub fn selected_refs(&self) -> &[SnapshotEntryRef] {
        &self.selected_refs
    }

    pub fn actions(&self) -> &[RenameAction] {
        &self.actions
    }

    /// Compares the complete immutable request identity, not only revisions.
    /// This is a UI-state invalidation check and is not filesystem freshness.
    pub fn matches_request(&self, request: &RenameRequest) -> bool {
        self.identity.snapshot == request.snapshot && self.identity.rules == request.rules
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenamePreview {
    root: ExplicitDirectoryRoot,
    rows: Vec<RenamePreviewRow>,
    readiness: PreviewReadiness,
    rule_errors: Vec<RuleValidationError>,
    plan: Option<RenamePlan>,
}

impl RenamePreview {
    pub fn root(&self) -> &ExplicitDirectoryRoot {
        &self.root
    }

    pub fn rows(&self) -> &[RenamePreviewRow] {
        &self.rows
    }

    pub fn readiness(&self) -> PreviewReadiness {
        self.readiness
    }

    pub fn rule_errors(&self) -> &[RuleValidationError] {
        &self.rule_errors
    }

    pub fn plan(&self) -> Option<&RenamePlan> {
        self.plan.as_ref()
    }
}

#[derive(Debug)]
enum CompiledRule {
    Prefix(String),
    Suffix(String),
    Number {
        start: u64,
        step: u64,
        padding: usize,
    },
    LiteralReplace {
        find: String,
        replace_with: String,
    },
    RegexReplace {
        regex: Option<Regex>,
        replace_with: String,
    },
    Case(CaseConversion),
}

#[derive(Debug)]
struct WorkRow {
    entry_index: usize,
    preview_index: usize,
    proposed_path: Option<PathBuf>,
    status: PreviewRowStatus,
}

/// Compute a deterministic preview from explicit immutable input values.
/// This function does not read or write the filesystem.
pub fn preview_renames(request: &RenameRequest) -> RenamePreview {
    let ordered_indices = ordered_entry_indices(&request.snapshot.entries);
    let mut preview_index_by_entry = vec![0; request.snapshot.entries.len()];
    for (preview_index, &entry_index) in ordered_indices.iter().enumerate() {
        preview_index_by_entry[entry_index] = preview_index;
    }
    let mut rows = ordered_indices
        .iter()
        .map(|&index| {
            let entry = &request.snapshot.entries[index];
            RenamePreviewRow {
                entry_ref: entry.entry_ref,
                source_path: entry.source_path.clone(),
                source_name: entry
                    .source_path
                    .file_name()
                    .unwrap_or_else(|| OsStr::new(""))
                    .to_os_string(),
                proposed_name: None,
                proposed_path: None,
                status: match (entry.kind, entry.inclusion, entry.selected) {
                    (SnapshotEntryKind::Directory | SnapshotEntryKind::Other, _, _) => {
                        PreviewRowStatus::Ineligible
                    }
                    (_, EntryInclusion::Excluded, _) => PreviewRowStatus::Excluded,
                    (SnapshotEntryKind::File, EntryInclusion::Included, false) => {
                        PreviewRowStatus::Unselected
                    }
                    (SnapshotEntryKind::File, EntryInclusion::Included, true) => {
                        PreviewRowStatus::Invalid(InvalidReason::InvalidRules)
                    }
                },
            }
        })
        .collect::<Vec<_>>();

    let selected_included = ordered_indices
        .iter()
        .copied()
        .filter(|&index| {
            let entry = &request.snapshot.entries[index];
            entry.kind == SnapshotEntryKind::File
                && entry.inclusion == EntryInclusion::Included
                && entry.selected
        })
        .collect::<Vec<_>>();

    let (compiled_rules, rule_errors) = compile_rules(&request.rules);
    if !rule_errors.is_empty() {
        let work_rows = selected_included
            .iter()
            .map(|&entry_index| WorkRow {
                entry_index,
                preview_index: preview_index_by_entry[entry_index],
                proposed_path: None,
                status: PreviewRowStatus::Invalid(InvalidReason::InvalidRules),
            })
            .collect::<Vec<_>>();
        apply_work_statuses(&mut rows, &work_rows);
        return RenamePreview {
            root: request.snapshot.root.clone(),
            rows,
            readiness: PreviewReadiness::Invalid,
            rule_errors,
            plan: None,
        };
    }

    let mut work_rows = Vec::with_capacity(selected_included.len());
    let mut eligible_for_numbering = Vec::with_capacity(selected_included.len());
    for &entry_index in &selected_included {
        let preview_index = preview_index_by_entry[entry_index];
        let entry = &request.snapshot.entries[entry_index];
        let source_name = entry
            .source_path
            .file_name()
            .unwrap_or_else(|| OsStr::new(""));
        let initial_error = source_path_error(&request.snapshot, &entry.source_path);
        let unicode_name = source_name.to_str();
        let status = match (initial_error, unicode_name) {
            (Some(reason), _) => Some(PreviewRowStatus::Invalid(reason)),
            (None, None) => Some(PreviewRowStatus::Invalid(
                InvalidReason::UnsupportedFilenameEncoding,
            )),
            (None, Some(_)) => None,
        };
        if let Some(status) = status {
            work_rows.push(WorkRow {
                entry_index,
                preview_index,
                proposed_path: None,
                status,
            });
        } else {
            eligible_for_numbering.push((entry_index, preview_index));
        }
    }

    for (numbering_index, &(entry_index, preview_index)) in
        eligible_for_numbering.iter().enumerate()
    {
        let entry = &request.snapshot.entries[entry_index];
        let source_name = entry
            .source_path
            .file_name()
            .unwrap_or_else(|| OsStr::new(""));
        let source_name_str = source_name.to_str().unwrap_or_default();
        match transform_name(
            source_name_str,
            numbering_index,
            &compiled_rules,
            request.rules.extension_policy,
        ) {
            Ok(proposed_name) => {
                rows[preview_index].proposed_name = Some(OsString::from(&proposed_name));
                let status = match validate_target_leaf(&proposed_name) {
                    Err(reason) => {
                        PreviewRowStatus::Invalid(InvalidReason::InvalidTargetLeaf(reason))
                    }
                    Ok(()) => {
                        let parent = entry.source_path.parent().unwrap_or_else(|| Path::new(""));
                        let proposed_path = parent.join(&proposed_name);
                        rows[preview_index].proposed_path = Some(proposed_path.clone());
                        if !path_fits_windows_limit(&proposed_path) {
                            PreviewRowStatus::Invalid(InvalidReason::TargetPathTooLong)
                        } else if source_name == OsStr::new(&proposed_name) {
                            PreviewRowStatus::NoOp
                        } else {
                            PreviewRowStatus::Ready
                        }
                    }
                };
                work_rows.push(WorkRow {
                    entry_index,
                    preview_index,
                    proposed_path: rows[preview_index].proposed_path.clone(),
                    status,
                });
            }
            Err(reason) => work_rows.push(WorkRow {
                entry_index,
                preview_index,
                proposed_path: None,
                status: PreviewRowStatus::Invalid(reason),
            }),
        }
    }

    mark_duplicate_source_paths(&request.snapshot, &mut work_rows);
    mark_duplicate_targets(&mut work_rows);
    mark_occupied_targets(&request.snapshot, &mut work_rows);
    apply_work_statuses(&mut rows, &work_rows);

    let has_invalid = work_rows.iter().any(|row| {
        matches!(
            row.status,
            PreviewRowStatus::Invalid(_) | PreviewRowStatus::Conflict(_)
        )
    });
    let ready_actions = work_rows
        .iter()
        .filter(|row| row.status == PreviewRowStatus::Ready)
        .count();
    let readiness = if selected_included.is_empty() {
        PreviewReadiness::EmptySelection
    } else if has_invalid {
        PreviewReadiness::Invalid
    } else if ready_actions == 0 {
        PreviewReadiness::AllNoOp
    } else {
        PreviewReadiness::Ready
    };

    let plan = (readiness == PreviewReadiness::Ready).then(|| {
        let selected_refs = selected_included
            .iter()
            .map(|&index| request.snapshot.entries[index].entry_ref)
            .collect();
        let actions = work_rows
            .iter()
            .filter(|row| row.status == PreviewRowStatus::Ready)
            .filter_map(|row| {
                let entry = &request.snapshot.entries[row.entry_index];
                row.proposed_path.as_ref().map(|target_path| RenameAction {
                    source_ref: entry.entry_ref,
                    source_path: entry.source_path.clone(),
                    target_path: target_path.clone(),
                })
            })
            .collect();
        RenamePlan {
            root: request.snapshot.root.clone(),
            recursive: request.snapshot.recursive,
            snapshot_revision: request.snapshot.revision,
            selection_revision: request.snapshot.selection_revision,
            rules_revision: request.rules.revision,
            selected_refs,
            actions,
            identity: RequestIdentity {
                snapshot: request.snapshot.clone(),
                rules: request.rules.clone(),
            },
        }
    });

    RenamePreview {
        root: request.snapshot.root.clone(),
        rows,
        readiness,
        rule_errors,
        plan,
    }
}

fn ordered_entry_indices(entries: &[RenameSnapshotEntry]) -> Vec<usize> {
    let mut indices = (0..entries.len()).collect::<Vec<_>>();
    indices.sort_by(|left, right| {
        path_windows_ordinal_cmp(&entries[*left].source_path, &entries[*right].source_path)
            .then_with(|| entries[*left].source_path.cmp(&entries[*right].source_path))
            .then_with(|| {
                entries[*left]
                    .source_order
                    .cmp(&entries[*right].source_order)
            })
            .then_with(|| entries[*left].entry_ref.cmp(&entries[*right].entry_ref))
    });
    indices
}

fn compile_rules(rules: &RenameRuleSet) -> (Vec<CompiledRule>, Vec<RuleValidationError>) {
    if rules.rules.len() > MAX_RENAME_RULES {
        return (
            Vec::new(),
            vec![RuleValidationError {
                rule_index: None,
                kind: RuleValidationErrorKind::TooManyRules {
                    count: rules.rules.len(),
                    limit: MAX_RENAME_RULES,
                },
            }],
        );
    }
    let mut compiled = Vec::with_capacity(rules.rules.len());
    let mut errors = Vec::new();
    for (rule_index, rule) in rules.rules.iter().enumerate() {
        let mut parameter_error = None;
        let compiled_rule = match rule {
            RenameRule::Prefix(prefix) => {
                if prefix.len() > MAX_RULE_PARAMETER_BYTES {
                    parameter_error = Some(prefix.len());
                }
                CompiledRule::Prefix(prefix.clone())
            }
            RenameRule::Suffix(suffix) => {
                if suffix.len() > MAX_RULE_PARAMETER_BYTES {
                    parameter_error = Some(suffix.len());
                }
                CompiledRule::Suffix(suffix.clone())
            }
            RenameRule::Number {
                start,
                step,
                padding,
            } => {
                if *padding > MAX_WINDOWS_NAME_UTF16_UNITS {
                    errors.push(RuleValidationError {
                        rule_index: Some(rule_index),
                        kind: RuleValidationErrorKind::NumberPaddingTooLarge {
                            padding: *padding,
                            limit: MAX_WINDOWS_NAME_UTF16_UNITS,
                        },
                    });
                }
                CompiledRule::Number {
                    start: *start,
                    step: *step,
                    padding: *padding,
                }
            }
            RenameRule::LiteralReplace { find, replace_with } => {
                for value in [find, replace_with] {
                    if value.len() > MAX_RULE_PARAMETER_BYTES {
                        parameter_error = Some(parameter_error.unwrap_or(0).max(value.len()));
                    }
                }
                if find.is_empty() {
                    errors.push(RuleValidationError {
                        rule_index: Some(rule_index),
                        kind: RuleValidationErrorKind::EmptyLiteralPattern,
                    });
                }
                CompiledRule::LiteralReplace {
                    find: find.clone(),
                    replace_with: replace_with.clone(),
                }
            }
            RenameRule::RegexReplace {
                pattern,
                replace_with,
            } => {
                for value in [pattern, replace_with] {
                    if value.len() > MAX_RULE_PARAMETER_BYTES {
                        parameter_error = Some(parameter_error.unwrap_or(0).max(value.len()));
                    }
                }
                if pattern.is_empty() {
                    errors.push(RuleValidationError {
                        rule_index: Some(rule_index),
                        kind: RuleValidationErrorKind::EmptyRegexPattern,
                    });
                    CompiledRule::RegexReplace {
                        regex: None,
                        replace_with: replace_with.clone(),
                    }
                } else {
                    match Regex::new(pattern) {
                        Ok(regex) => CompiledRule::RegexReplace {
                            regex: Some(regex),
                            replace_with: replace_with.clone(),
                        },
                        Err(error) => {
                            errors.push(RuleValidationError {
                                rule_index: Some(rule_index),
                                kind: RuleValidationErrorKind::InvalidRegex(error.to_string()),
                            });
                            CompiledRule::RegexReplace {
                                regex: None,
                                replace_with: replace_with.clone(),
                            }
                        }
                    }
                }
            }
            RenameRule::Case(case) => CompiledRule::Case(*case),
        };
        if let Some(bytes) = parameter_error {
            errors.push(RuleValidationError {
                rule_index: Some(rule_index),
                kind: RuleValidationErrorKind::ParameterTooLong {
                    bytes,
                    limit: MAX_RULE_PARAMETER_BYTES,
                },
            });
        }
        compiled.push(compiled_rule);
    }
    (compiled, errors)
}

fn transform_name(
    original_name: &str,
    selected_index: usize,
    rules: &[CompiledRule],
    extension_policy: ExtensionPolicy,
) -> Result<String, InvalidReason> {
    let (stem, extension) = if extension_policy == ExtensionPolicy::Preserve {
        split_last_extension(original_name)
    } else {
        (original_name, "")
    };
    let mut transformed = stem.to_owned();
    let sequence_numbers = rules
        .iter()
        .filter_map(|rule| match rule {
            CompiledRule::Number { start, step, .. } => Some((*start, *step)),
            _ => None,
        })
        .map(|(start, step)| {
            let index = u64::try_from(selected_index).map_err(|_| InvalidReason::NumberOverflow)?;
            let offset = step
                .checked_mul(index)
                .ok_or(InvalidReason::NumberOverflow)?;
            start
                .checked_add(offset)
                .ok_or(InvalidReason::NumberOverflow)
        })
        .collect::<Result<Vec<_>, _>>()?;

    let mut number_rule_index = 0usize;
    for rule in rules {
        match rule {
            CompiledRule::Prefix(prefix) => transformed.insert_str(0, prefix),
            CompiledRule::Suffix(suffix) => transformed.push_str(suffix),
            CompiledRule::Number { padding, .. } => {
                let number = sequence_numbers[number_rule_index];
                number_rule_index += 1;
                let number = format!("{number:0width$}", width = *padding);
                transformed.insert_str(0, &number);
            }
            CompiledRule::LiteralReplace { find, replace_with } => {
                transformed = transformed.replace(find, replace_with);
            }
            CompiledRule::RegexReplace {
                regex,
                replace_with,
            } => {
                let Some(regex) = regex else {
                    return Err(InvalidReason::InvalidRules);
                };
                transformed = replace_regex_bounded(regex, replace_with, &transformed)?;
            }
            CompiledRule::Case(CaseConversion::Lowercase) => {
                transformed = transformed.to_lowercase();
            }
            CompiledRule::Case(CaseConversion::Uppercase) => {
                transformed = transformed.to_uppercase();
            }
        }
        if transformed.len() > MAX_INTERMEDIATE_NAME_BYTES {
            return Err(InvalidReason::NameGrowthLimit);
        }
    }
    let mut result = transformed;
    result.push_str(extension);
    if result.len() > MAX_INTERMEDIATE_NAME_BYTES {
        return Err(InvalidReason::NameGrowthLimit);
    }
    Ok(result)
}

fn split_last_extension(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(dot) if dot > 0 => (&name[..dot], &name[dot..]),
        _ => (name, ""),
    }
}

fn replace_regex_bounded(
    regex: &Regex,
    replacement: &str,
    input: &str,
) -> Result<String, InvalidReason> {
    let mut output = String::new();
    let mut last_end = 0;
    for captures in regex.captures_iter(input) {
        let Some(found) = captures.get(0) else {
            continue;
        };
        output.push_str(&input[last_end..found.start()]);
        captures.expand(replacement, &mut output);
        if output.len() > MAX_INTERMEDIATE_NAME_BYTES {
            return Err(InvalidReason::NameGrowthLimit);
        }
        last_end = found.end();
    }
    output.push_str(&input[last_end..]);
    if output.len() > MAX_INTERMEDIATE_NAME_BYTES {
        return Err(InvalidReason::NameGrowthLimit);
    }
    Ok(output)
}

fn source_path_error(snapshot: &RenameInputSnapshot, source_path: &Path) -> Option<InvalidReason> {
    if !path_fits_windows_limit(source_path) {
        return Some(InvalidReason::SourcePathTooLong);
    }
    let Some(relative_components) =
        path_relative_component_count(snapshot.root.as_path(), source_path)
    else {
        return Some(InvalidReason::SourceOutsideRoot);
    };
    if relative_components == 0 {
        return Some(InvalidReason::MissingFilename);
    }
    if !snapshot.recursive && relative_components != 1 {
        return Some(InvalidReason::NestedSourceWhenRecursionDisabled);
    }
    None
}

fn path_relative_component_count(root: &Path, path: &Path) -> Option<usize> {
    let root_components = safe_path_components(root)?;
    let path_components = safe_path_components(path)?;
    if path_components.len() < root_components.len()
        || !root_components
            .iter()
            .zip(path_components.iter())
            .all(|(root, path)| path_component_windows_eq(root, path))
    {
        return None;
    }
    Some(path_components.len() - root_components.len())
}

#[derive(Debug)]
enum SafePathComponent<'a> {
    Prefix(&'a OsStr),
    Root,
    Normal(&'a OsStr),
}

fn safe_path_components(path: &Path) -> Option<Vec<SafePathComponent<'_>>> {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => {
                components.push(SafePathComponent::Prefix(prefix.as_os_str()))
            }
            Component::RootDir => components.push(SafePathComponent::Root),
            Component::Normal(name) => components.push(SafePathComponent::Normal(name)),
            Component::CurDir => {}
            Component::ParentDir => return None,
        }
    }
    Some(components)
}

fn path_component_windows_eq(left: &SafePathComponent<'_>, right: &SafePathComponent<'_>) -> bool {
    match (left, right) {
        (SafePathComponent::Prefix(left), SafePathComponent::Prefix(right))
        | (SafePathComponent::Normal(left), SafePathComponent::Normal(right)) => {
            windows_ordinal_eq(left, right)
        }
        (SafePathComponent::Root, SafePathComponent::Root) => true,
        _ => false,
    }
}

fn path_windows_ordinal_cmp(left: &Path, right: &Path) -> Ordering {
    let Some(left_components) = safe_path_components(left) else {
        return left.as_os_str().cmp(right.as_os_str());
    };
    let Some(right_components) = safe_path_components(right) else {
        return left.as_os_str().cmp(right.as_os_str());
    };
    for (left, right) in left_components.iter().zip(right_components.iter()) {
        let ordering = match (left, right) {
            (SafePathComponent::Prefix(left), SafePathComponent::Prefix(right))
            | (SafePathComponent::Normal(left), SafePathComponent::Normal(right)) => {
                windows_ordinal_cmp(left, right)
            }
            (SafePathComponent::Root, SafePathComponent::Root) => Ordering::Equal,
            (SafePathComponent::Prefix(_), _) => Ordering::Less,
            (_, SafePathComponent::Prefix(_)) => Ordering::Greater,
            (SafePathComponent::Root, _) => Ordering::Less,
            (_, SafePathComponent::Root) => Ordering::Greater,
        };
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    left_components.len().cmp(&right_components.len())
}

/// Compare Windows paths using the same component-wise ordinal equivalence
/// policy used by preview conflict detection. Filesystem discovery shares this
/// helper so it cannot drift into a different case-folding rule.
pub(crate) fn compare_paths_windows_ordinal(left: &Path, right: &Path) -> Ordering {
    path_windows_ordinal_cmp(left, right)
}

/// Check path containment using the rename domain's Windows ordinal
/// component comparison. This is a lexical containment helper only; callers
/// that cross a filesystem boundary must also validate opened object IDs.
pub(crate) fn path_is_within_windows_ordinal(root: &Path, path: &Path) -> bool {
    let Some(root_components) = safe_path_components(root) else {
        return false;
    };
    let Some(path_components) = safe_path_components(path) else {
        return false;
    };
    path_components.len() >= root_components.len()
        && root_components
            .iter()
            .zip(path_components.iter())
            .all(|(root, path)| path_component_windows_eq(root, path))
}

fn windows_ordinal_eq(left: &OsStr, right: &OsStr) -> bool {
    windows_ordinal_cmp(left, right) == Ordering::Equal
}

#[cfg(windows)]
fn windows_ordinal_cmp(left: &OsStr, right: &OsStr) -> Ordering {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Foundation::BOOL;
    use windows::Win32::Globalization::{
        CSTR_EQUAL, CSTR_GREATER_THAN, CSTR_LESS_THAN, CompareStringOrdinal,
    };

    let left = left.encode_wide().collect::<Vec<_>>();
    let right = right.encode_wide().collect::<Vec<_>>();
    if left.is_empty() || right.is_empty() {
        return left.len().cmp(&right.len());
    }
    // SAFETY: the Windows wrapper passes UTF-16 slices and their bounded
    // lengths to a read-only string comparison API. No path is opened.
    match unsafe { CompareStringOrdinal(&left, &right, BOOL(1)) } {
        CSTR_LESS_THAN => Ordering::Less,
        CSTR_EQUAL => Ordering::Equal,
        CSTR_GREATER_THAN => Ordering::Greater,
        // CSTR_ERROR (or another unexpected API result) must not make two
        // different paths equivalent. Preserve deterministic raw UTF-16
        // ordering as a conservative failure fallback.
        _ => left.cmp(&right),
    }
}

#[cfg(not(windows))]
fn windows_ordinal_cmp(left: &OsStr, right: &OsStr) -> Ordering {
    // Non-Windows builds use an ASCII-insensitive development fallback. The
    // application and executor target Windows, where CompareStringOrdinal is
    // used above; non-ASCII Windows equivalence is verified on that target.
    match (left.to_str(), right.to_str()) {
        (Some(left), Some(right)) => left
            .bytes()
            .map(|byte| byte.to_ascii_lowercase())
            .cmp(right.bytes().map(|byte| byte.to_ascii_lowercase())),
        _ => left.cmp(right),
    }
}

fn path_fits_windows_limit(path: &Path) -> bool {
    os_str_utf16_len(path.as_os_str())
        .map(|units| units.saturating_add(1) <= MAX_WINDOWS_PATH_UTF16_UNITS_WITH_NUL)
        .unwrap_or(false)
}

fn os_str_utf16_len(value: &OsStr) -> Option<usize> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        Some(value.encode_wide().count())
    }
    #[cfg(not(windows))]
    {
        value.to_str().map(|value| value.encode_utf16().count())
    }
}

fn validate_target_leaf(name: &str) -> Result<(), TargetNameError> {
    if name.is_empty() {
        return Err(TargetNameError::Empty);
    }
    if name == "." || name == ".." {
        return Err(TargetNameError::PathSeparator);
    }
    for character in name.chars() {
        if matches!(character, '/' | '\\') {
            return Err(TargetNameError::PathSeparator);
        }
        if character <= '\u{001f}' || matches!(character, '<' | '>' | ':' | '"' | '|' | '?' | '*') {
            return Err(TargetNameError::ForbiddenCharacter(character));
        }
    }
    if name.ends_with('.') || name.ends_with(' ') {
        return Err(TargetNameError::TrailingDotOrSpace);
    }
    let utf16_units = name.encode_utf16().count();
    if utf16_units > MAX_WINDOWS_NAME_UTF16_UNITS {
        return Err(TargetNameError::TooLong {
            utf16_units,
            limit: MAX_WINDOWS_NAME_UTF16_UNITS,
        });
    }
    if is_reserved_device_name(name) {
        return Err(TargetNameError::ReservedDeviceName);
    }
    Ok(())
}

fn is_reserved_device_name(name: &str) -> bool {
    let base = name
        .split('.')
        .next()
        .unwrap_or(name)
        .trim_end_matches([' ', '.']);
    const RESERVED: &[&str] = &[
        "CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$", "CLOCK$", "COM1", "COM2", "COM3", "COM4",
        "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6",
        "LPT7", "LPT8", "LPT9", "COM¹", "COM²", "COM³", "LPT¹", "LPT²", "LPT³",
    ];
    RESERVED
        .iter()
        .any(|reserved| windows_ordinal_eq(OsStr::new(base), OsStr::new(reserved)))
}

fn mark_duplicate_source_paths(snapshot: &RenameInputSnapshot, work_rows: &mut [WorkRow]) {
    let mut ordered = snapshot
        .entries
        .iter()
        .enumerate()
        .map(|(index, entry)| (entry.source_path.clone(), index))
        .collect::<Vec<_>>();
    ordered.sort_by(|(left, left_index), (right, right_index)| {
        path_windows_ordinal_cmp(left, right)
            .then_with(|| left.cmp(right))
            .then_with(|| left_index.cmp(right_index))
    });
    let paths = ordered
        .iter()
        .map(|(path, _)| path.clone())
        .collect::<Vec<_>>();
    for group in equal_path_group_ranges(&paths) {
        if group.len() < 2 {
            continue;
        }
        for ordered_index in group {
            let entry_index = ordered[ordered_index].1;
            if let Some(row) = work_rows
                .iter_mut()
                .find(|row| row.entry_index == entry_index)
                && matches!(row.status, PreviewRowStatus::Ready | PreviewRowStatus::NoOp)
            {
                row.status = PreviewRowStatus::Conflict(ConflictReason::DuplicateSourcePath);
            }
        }
    }
}

fn mark_duplicate_targets(work_rows: &mut [WorkRow]) {
    let mut ordered = work_rows
        .iter()
        .enumerate()
        .filter_map(|(index, row)| {
            (row.status == PreviewRowStatus::Ready).then_some((row.proposed_path.clone()?, index))
        })
        .collect::<Vec<_>>();
    ordered.sort_by(|(left, left_index), (right, right_index)| {
        path_windows_ordinal_cmp(left, right)
            .then_with(|| left.cmp(right))
            .then_with(|| left_index.cmp(right_index))
    });
    let paths = ordered
        .iter()
        .map(|(path, _)| path.clone())
        .collect::<Vec<_>>();
    for group in equal_path_group_ranges(&paths) {
        if group.len() < 2 {
            continue;
        }
        for ordered_index in group {
            let row_index = ordered[ordered_index].1;
            work_rows[row_index].status =
                PreviewRowStatus::Conflict(ConflictReason::DuplicateProposedTarget);
        }
    }
}

fn equal_path_group_ranges(paths: &[PathBuf]) -> Vec<Range<usize>> {
    let mut groups = Vec::new();
    let mut start = 0;
    while start < paths.len() {
        let mut end = start + 1;
        while end < paths.len()
            && path_windows_ordinal_cmp(&paths[start], &paths[end]) == Ordering::Equal
        {
            end += 1;
        }
        groups.push(start..end);
        start = end;
    }
    groups
}

fn mark_occupied_targets(snapshot: &RenameInputSnapshot, work_rows: &mut [WorkRow]) {
    let mut occupants = snapshot
        .entries
        .iter()
        .map(|entry| (entry.source_path.clone(), entry.entry_ref))
        .collect::<Vec<_>>();
    occupants.sort_by(|(left, left_ref), (right, right_ref)| {
        path_windows_ordinal_cmp(left, right)
            .then_with(|| left.cmp(right))
            .then_with(|| left_ref.cmp(right_ref))
    });
    let row_by_ref = work_rows
        .iter()
        .enumerate()
        .map(|(index, row)| (snapshot.entries[row.entry_index].entry_ref, index))
        .collect::<HashMap<_, _>>();
    let mut reverse_dependencies = vec![Vec::new(); work_rows.len()];
    let mut direct_blockers = vec![None; work_rows.len()];

    // Build the moving-source dependency graph once. A ready rename depends
    // on every selected ready source whose current path equals its target.
    // Fixed occupants (including no-ops, invalid/conflicted rows, excluded
    // entries, and directories) block it immediately.
    for index in 0..work_rows.len() {
        if work_rows[index].status != PreviewRowStatus::Ready {
            continue;
        }
        let Some(target_path) = work_rows[index].proposed_path.as_ref() else {
            continue;
        };
        let source_ref = snapshot.entries[work_rows[index].entry_index].entry_ref;
        let start = occupants.partition_point(|(path, _)| {
            path_windows_ordinal_cmp(path, target_path) == Ordering::Less
        });
        for (_, occupied_ref) in occupants[start..]
            .iter()
            .take_while(|(path, _)| path_windows_ordinal_cmp(path, target_path) == Ordering::Equal)
        {
            if *occupied_ref == source_ref {
                continue;
            }
            let moving_row = row_by_ref
                .get(occupied_ref)
                .copied()
                .filter(|&row_index| work_rows[row_index].status == PreviewRowStatus::Ready);
            if let Some(moving_row) = moving_row {
                reverse_dependencies[moving_row].push(index);
            } else {
                direct_blockers[index] = Some(*occupied_ref);
                break;
            }
        }
    }

    // A blocked mover cannot vacate its source. Propagate that fact backward
    // through dependents in O(rows + dependency edges), preserving unblocked
    // cycles as valid staging candidates.
    let mut blocked_queue = VecDeque::new();
    for (index, blocker) in direct_blockers.into_iter().enumerate() {
        if let Some(occupant) = blocker {
            work_rows[index].status =
                PreviewRowStatus::Conflict(ConflictReason::DestinationOccupied { occupant });
            blocked_queue.push_back(index);
        }
    }
    while let Some(blocked_row) = blocked_queue.pop_front() {
        let blocked_ref = snapshot.entries[work_rows[blocked_row].entry_index].entry_ref;
        for &dependent in &reverse_dependencies[blocked_row] {
            if work_rows[dependent].status == PreviewRowStatus::Ready {
                work_rows[dependent].status =
                    PreviewRowStatus::Conflict(ConflictReason::DestinationOccupied {
                        occupant: blocked_ref,
                    });
                blocked_queue.push_back(dependent);
            }
        }
    }
}

fn apply_work_statuses(rows: &mut [RenamePreviewRow], work_rows: &[WorkRow]) {
    for work_row in work_rows {
        rows[work_row.preview_index].status = work_row.status.clone();
        if work_row.proposed_path.is_some() {
            rows[work_row.preview_index].proposed_path = work_row.proposed_path.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> ExplicitDirectoryRoot {
        ExplicitDirectoryRoot::new(r"C:\rename-test").unwrap()
    }

    fn entry(reference: u64, path: &str, source_order: u64, selected: bool) -> RenameSnapshotEntry {
        RenameSnapshotEntry::new(
            SnapshotEntryRef(reference),
            path,
            source_order,
            EntryInclusion::Included,
            selected,
        )
    }

    fn snapshot(entries: Vec<RenameSnapshotEntry>) -> RenameInputSnapshot {
        RenameInputSnapshot::new(root(), entries).unwrap()
    }

    fn request(entries: Vec<RenameSnapshotEntry>, rules: Vec<RenameRule>) -> RenameRequest {
        RenameRequest::new(
            snapshot(entries),
            RenameRuleSet::new(ExtensionPolicy::Preserve, rules),
        )
    }

    fn row(preview: &RenamePreview, reference: u64) -> &RenamePreviewRow {
        preview
            .rows()
            .iter()
            .find(|row| row.entry_ref() == SnapshotEntryRef(reference))
            .unwrap()
    }

    fn proposed_name(preview: &RenamePreview, reference: u64) -> &str {
        row(preview, reference)
            .proposed_name()
            .unwrap()
            .to_str()
            .unwrap()
    }

    #[test]
    fn m8c_bulk_updates_validate_once_and_reject_over_limit_atomically() {
        let entries = (0..MAX_SNAPSHOT_ENTRIES)
            .map(|index| {
                RenameSnapshotEntry::new(
                    SnapshotEntryRef(index as u64),
                    format!(r"C:\rename-test\file-{index:05}.txt"),
                    index as u64,
                    EntryInclusion::Included,
                    false,
                )
            })
            .collect();
        let snapshot = snapshot(entries);
        let selected = (0..MAX_SELECTED_ENTRIES)
            .map(|index| (SnapshotEntryRef(index as u64), Some(true), None))
            .collect::<Vec<_>>();
        let selected_snapshot = snapshot.with_entry_updates(&selected).unwrap();
        assert_eq!(selected_snapshot.selection_revision(), SelectionRevision(1));
        assert_eq!(
            selected_snapshot
                .entries()
                .iter()
                .filter(|entry| entry.is_selected())
                .count(),
            MAX_SELECTED_ENTRIES
        );
        let unchanged = selected_snapshot.with_entry_updates(&selected).unwrap();
        assert_eq!(unchanged, selected_snapshot);
        assert_eq!(unchanged.selection_revision(), SelectionRevision(1));

        let rejected = selected_snapshot.with_entry_updates(&[
            (SnapshotEntryRef(MAX_SELECTED_ENTRIES as u64), Some(true), None),
            (
                SnapshotEntryRef((MAX_SELECTED_ENTRIES + 1) as u64),
                None,
                Some(EntryInclusion::Excluded),
            ),
        ]);
        assert_eq!(
            rejected,
            Err(SnapshotError::TooManySelected {
                count: MAX_SELECTED_ENTRIES + 1,
                limit: MAX_SELECTED_ENTRIES,
            })
        );
        assert!(!selected_snapshot.entries()[MAX_SELECTED_ENTRIES].is_selected());
        assert_eq!(
            selected_snapshot.entries()[MAX_SELECTED_ENTRIES + 1].inclusion(),
            EntryInclusion::Included
        );
        assert_eq!(selected_snapshot.selection_revision(), SelectionRevision(1));
    }

    #[test]
    fn ordered_rules_compose_unicode_name_and_preserve_final_extension() {
        let request = request(
            vec![entry(1, r"C:\rename-test\café.tar.gz", 0, true)],
            vec![
                RenameRule::Number {
                    start: 4,
                    step: 1,
                    padding: 3,
                },
                RenameRule::Prefix("pre-".into()),
                RenameRule::Suffix("-done".into()),
                RenameRule::LiteralReplace {
                    find: "café".into(),
                    replace_with: "cafe".into(),
                },
                RenameRule::Case(CaseConversion::Uppercase),
            ],
        );
        let before = request.clone();
        let preview = preview_renames(&request);

        assert_eq!(proposed_name(&preview, 1), "PRE-004CAFE.TAR-DONE.gz");
        assert_eq!(row(&preview, 1).status(), &PreviewRowStatus::Ready);
        assert_eq!(preview.readiness(), PreviewReadiness::Ready);
        assert_eq!(preview.plan().unwrap().actions().len(), 1);
        assert_eq!(request, before);
    }

    #[test]
    fn default_extension_policy_handles_dotfiles_and_multidot_names() {
        let preview = preview_renames(&request(
            vec![
                entry(1, r"C:\rename-test\.env", 0, true),
                entry(2, r"C:\rename-test\archive.tar.gz", 1, true),
            ],
            vec![RenameRule::Suffix(".bak".into())],
        ));

        assert_eq!(proposed_name(&preview, 1), ".env.bak");
        assert_eq!(proposed_name(&preview, 2), "archive.tar.bak.gz");
        assert_eq!(row(&preview, 1).status(), &PreviewRowStatus::Ready);
        assert_eq!(row(&preview, 2).status(), &PreviewRowStatus::Ready);
    }

    #[test]
    fn explicit_extension_editing_applies_rules_to_the_complete_name() {
        let snapshot = snapshot(vec![entry(1, r"C:\rename-test\archive.tar.gz", 0, true)]);
        let rules = RenameRuleSet::new(
            ExtensionPolicy::Edit,
            vec![RenameRule::Suffix(".bak".into())],
        );
        let preview = preview_renames(&RenameRequest::new(snapshot, rules));

        assert_eq!(proposed_name(&preview, 1), "archive.tar.gz.bak");
        assert_eq!(row(&preview, 1).status(), &PreviewRowStatus::Ready);
    }

    #[test]
    fn numbering_uses_stable_selected_full_path_order_and_skips_unselected_rows() {
        let preview = preview_renames(&request(
            vec![
                entry(20, r"C:\rename-test\b.txt", 0, true),
                entry(30, r"C:\rename-test\skip.txt", 1, false),
                entry(10, r"C:\rename-test\a.txt", 2, true),
            ],
            vec![RenameRule::Number {
                start: 7,
                step: 2,
                padding: 3,
            }],
        ));

        assert_eq!(proposed_name(&preview, 10), "007a.txt");
        assert_eq!(proposed_name(&preview, 20), "009b.txt");
        assert_eq!(row(&preview, 30).status(), &PreviewRowStatus::Unselected);
        assert_eq!(
            preview
                .plan()
                .unwrap()
                .selected_refs()
                .iter()
                .map(|reference| reference.0)
                .collect::<Vec<_>>(),
            [10, 20]
        );
    }

    #[test]
    fn numbering_overflow_invalidates_only_the_unrepresentable_row_and_plan() {
        let preview = preview_renames(&request(
            vec![
                entry(1, r"C:\rename-test\a.txt", 0, true),
                entry(2, r"C:\rename-test\b.txt", 1, true),
            ],
            vec![RenameRule::Number {
                start: u64::MAX,
                step: 1,
                padding: 0,
            }],
        ));

        assert_eq!(row(&preview, 1).status(), &PreviewRowStatus::Ready);
        assert_eq!(
            row(&preview, 2).status(),
            &PreviewRowStatus::Invalid(InvalidReason::NumberOverflow)
        );
        assert_eq!(preview.readiness(), PreviewReadiness::Invalid);
        assert!(preview.plan().is_none());
    }

    #[test]
    fn regex_capture_substitution_uses_the_ordered_rule_output() {
        let preview = preview_renames(&request(
            vec![entry(1, r"C:\rename-test\chapter_12.txt", 0, true)],
            vec![RenameRule::RegexReplace {
                pattern: r"^([a-z]+)_([0-9]+)$".into(),
                replace_with: "$2-$1".into(),
            }],
        ));

        assert_eq!(proposed_name(&preview, 1), "12-chapter.txt");
        assert_eq!(row(&preview, 1).status(), &PreviewRowStatus::Ready);
    }

    #[test]
    fn windows_leaf_validation_rejects_reserved_ads_and_trailing_dot_names() {
        let reserved = preview_renames(&request(
            vec![entry(1, r"C:\rename-test\source.txt", 0, true)],
            vec![RenameRule::LiteralReplace {
                find: "source".into(),
                replace_with: "con".into(),
            }],
        ));
        assert_eq!(
            row(&reserved, 1).status(),
            &PreviewRowStatus::Invalid(InvalidReason::InvalidTargetLeaf(
                TargetNameError::ReservedDeviceName
            ))
        );

        let ads = preview_renames(&request(
            vec![entry(2, r"C:\rename-test\source.txt", 0, true)],
            vec![RenameRule::Suffix(":stream".into())],
        ));
        assert!(matches!(
            row(&ads, 2).status(),
            PreviewRowStatus::Invalid(InvalidReason::InvalidTargetLeaf(
                TargetNameError::ForbiddenCharacter(':')
            ))
        ));

        let trailing_dot = preview_renames(&request(
            vec![entry(3, r"C:\rename-test\source", 0, true)],
            vec![RenameRule::Suffix(".".into())],
        ));
        assert_eq!(
            row(&trailing_dot, 3).status(),
            &PreviewRowStatus::Invalid(InvalidReason::InvalidTargetLeaf(
                TargetNameError::TrailingDotOrSpace
            ))
        );
    }

    #[test]
    fn invalid_regex_is_reported_and_cannot_leave_a_candidate_plan() {
        let preview = preview_renames(&request(
            vec![entry(1, r"C:\rename-test\source.txt", 0, true)],
            vec![RenameRule::RegexReplace {
                pattern: "[".into(),
                replace_with: "replacement".into(),
            }],
        ));

        assert_eq!(preview.readiness(), PreviewReadiness::Invalid);
        assert_eq!(preview.rule_errors().len(), 1);
        assert!(matches!(
            preview.rule_errors()[0].kind,
            RuleValidationErrorKind::InvalidRegex(_)
        ));
        assert_eq!(
            row(&preview, 1).status(),
            &PreviewRowStatus::Invalid(InvalidReason::InvalidRules)
        );
        assert!(preview.plan().is_none());
    }

    #[test]
    fn duplicate_targets_and_unselected_occupants_are_conflicts() {
        let duplicate_targets = preview_renames(&request(
            vec![
                entry(1, r"C:\rename-test\alpha.txt", 0, true),
                entry(2, r"C:\rename-test\beta.txt", 1, true),
            ],
            vec![RenameRule::RegexReplace {
                pattern: r".+".into(),
                replace_with: "same".into(),
            }],
        ));
        assert_eq!(
            row(&duplicate_targets, 1).status(),
            &PreviewRowStatus::Conflict(ConflictReason::DuplicateProposedTarget)
        );
        assert_eq!(
            row(&duplicate_targets, 2).status(),
            &PreviewRowStatus::Conflict(ConflictReason::DuplicateProposedTarget)
        );
        assert!(duplicate_targets.plan().is_none());

        let occupied = preview_renames(&request(
            vec![
                entry(3, r"C:\rename-test\source.txt", 0, true),
                entry(4, r"C:\rename-test\occupied.txt", 1, false),
            ],
            vec![RenameRule::LiteralReplace {
                find: "source".into(),
                replace_with: "occupied".into(),
            }],
        ));
        assert_eq!(
            row(&occupied, 3).status(),
            &PreviewRowStatus::Conflict(ConflictReason::DestinationOccupied {
                occupant: SnapshotEntryRef(4)
            })
        );
        assert!(occupied.plan().is_none());
    }

    #[test]
    fn directories_and_other_ineligible_rows_remain_occupancy_blockers() {
        let directory = RenameSnapshotEntry::new(
            SnapshotEntryRef(2),
            r"C:\rename-test\occupied.txt",
            1,
            EntryInclusion::Excluded,
            false,
        )
        .with_kind(SnapshotEntryKind::Directory);
        let preview = preview_renames(&request(
            vec![entry(1, r"C:\rename-test\source.txt", 0, true), directory],
            vec![RenameRule::LiteralReplace {
                find: "source".into(),
                replace_with: "occupied".into(),
            }],
        ));

        assert_eq!(row(&preview, 2).status(), &PreviewRowStatus::Ineligible);
        assert_eq!(
            row(&preview, 1).status(),
            &PreviewRowStatus::Conflict(ConflictReason::DestinationOccupied {
                occupant: SnapshotEntryRef(2)
            })
        );
        assert!(preview.plan().is_none());
    }

    #[test]
    fn snapshot_rejects_unsupported_unselected_occupant_paths_before_preview() {
        let malformed_occupant = RenameSnapshotEntry::new(
            SnapshotEntryRef(2),
            root().as_path().join("sub").join("..").join("occupied.txt"),
            1,
            EntryInclusion::Excluded,
            false,
        );
        let invalid_shape = RenameInputSnapshot::new(
            root(),
            vec![
                entry(1, r"C:\rename-test\source.txt", 0, true),
                malformed_occupant,
            ],
        );
        assert_eq!(
            invalid_shape,
            Err(SnapshotError::UnsupportedEntryPath {
                entry_ref: SnapshotEntryRef(2),
                reason: SnapshotEntryPathError::ParentTraversal,
            })
        );

        let overlong_occupant = RenameSnapshotEntry::new(
            SnapshotEntryRef(3),
            root()
                .as_path()
                .join("x".repeat(MAX_WINDOWS_PATH_UTF16_UNITS_WITH_NUL)),
            2,
            EntryInclusion::Excluded,
            false,
        );
        let overlong = RenameInputSnapshot::new(root(), vec![overlong_occupant]);
        assert!(matches!(
            overlong,
            Err(SnapshotError::UnsupportedEntryPath {
                entry_ref: SnapshotEntryRef(3),
                reason: SnapshotEntryPathError::PathTooLong,
            })
        ));
    }

    #[test]
    fn no_op_remains_an_occupant_for_another_selected_destination() {
        let preview = preview_renames(&request(
            vec![
                entry(1, r"C:\rename-test\source.txt", 0, true),
                entry(2, r"C:\rename-test\occupied.txt", 1, true),
            ],
            vec![RenameRule::LiteralReplace {
                find: "source".into(),
                replace_with: "occupied".into(),
            }],
        ));

        assert_eq!(
            row(&preview, 1).status(),
            &PreviewRowStatus::Conflict(ConflictReason::DestinationOccupied {
                occupant: SnapshotEntryRef(2)
            })
        );
        assert_eq!(row(&preview, 2).status(), &PreviewRowStatus::NoOp);
        assert_eq!(preview.readiness(), PreviewReadiness::Invalid);
    }

    #[test]
    fn case_only_change_and_two_or_three_file_cycles_are_ready() {
        let case_only = preview_renames(&request(
            vec![entry(1, r"C:\rename-test\Readme.txt", 0, true)],
            vec![RenameRule::Case(CaseConversion::Lowercase)],
        ));
        assert_eq!(proposed_name(&case_only, 1), "readme.txt");
        assert_eq!(row(&case_only, 1).status(), &PreviewRowStatus::Ready);

        let swap = preview_renames(&request(
            vec![
                entry(2, r"C:\rename-test\alpha.txt", 0, true),
                entry(3, r"C:\rename-test\beta.txt", 1, true),
            ],
            vec![
                RenameRule::LiteralReplace {
                    find: "alpha".into(),
                    replace_with: "temporary".into(),
                },
                RenameRule::LiteralReplace {
                    find: "beta".into(),
                    replace_with: "alpha".into(),
                },
                RenameRule::LiteralReplace {
                    find: "temporary".into(),
                    replace_with: "beta".into(),
                },
            ],
        ));
        assert_eq!(swap.readiness(), PreviewReadiness::Ready);
        assert_eq!(swap.plan().unwrap().actions().len(), 2);

        let cycle = preview_renames(&request(
            vec![
                entry(4, r"C:\rename-test\a.txt", 0, true),
                entry(5, r"C:\rename-test\b.txt", 1, true),
                entry(6, r"C:\rename-test\c.txt", 2, true),
            ],
            vec![
                RenameRule::LiteralReplace {
                    find: "a".into(),
                    replace_with: "temporary".into(),
                },
                RenameRule::LiteralReplace {
                    find: "b".into(),
                    replace_with: "a".into(),
                },
                RenameRule::LiteralReplace {
                    find: "c".into(),
                    replace_with: "b".into(),
                },
                RenameRule::LiteralReplace {
                    find: "temporary".into(),
                    replace_with: "c".into(),
                },
            ],
        ));
        assert_eq!(cycle.readiness(), PreviewReadiness::Ready);
        assert_eq!(cycle.plan().unwrap().actions().len(), 3);
    }

    #[test]
    fn dependency_conflicts_propagate_through_invalid_selected_sources() {
        let preview = preview_renames(&request(
            vec![
                entry(1, r"C:\rename-test\a.txt", 0, true),
                entry(2, r"C:\rename-test\b.txt", 1, true),
                entry(3, r"C:\rename-test\c.txt", 2, true),
            ],
            vec![
                RenameRule::LiteralReplace {
                    find: "a".into(),
                    replace_with: "temporary-a".into(),
                },
                RenameRule::LiteralReplace {
                    find: "b".into(),
                    replace_with: "temporary-b".into(),
                },
                RenameRule::LiteralReplace {
                    find: "c".into(),
                    replace_with: "con".into(),
                },
                RenameRule::LiteralReplace {
                    find: "temporary-a".into(),
                    replace_with: "b".into(),
                },
                RenameRule::LiteralReplace {
                    find: "temporary-b".into(),
                    replace_with: "c".into(),
                },
            ],
        ));

        assert_eq!(
            row(&preview, 3).status(),
            &PreviewRowStatus::Invalid(InvalidReason::InvalidTargetLeaf(
                TargetNameError::ReservedDeviceName
            ))
        );
        assert!(matches!(
            row(&preview, 2).status(),
            PreviewRowStatus::Conflict(ConflictReason::DestinationOccupied {
                occupant: SnapshotEntryRef(3)
            })
        ));
        assert!(matches!(
            row(&preview, 1).status(),
            PreviewRowStatus::Conflict(ConflictReason::DestinationOccupied {
                occupant: SnapshotEntryRef(2)
            })
        ));
        assert_eq!(preview.readiness(), PreviewReadiness::Invalid);
        assert!(preview.plan().is_none());
    }

    #[test]
    fn long_forward_dependency_chain_propagates_from_its_blocked_endpoint() {
        const CHAIN_LENGTH: usize = 128;
        let mut entries = (0..CHAIN_LENGTH)
            .map(|index| {
                RenameSnapshotEntry::new(
                    SnapshotEntryRef(1_000 + index as u64),
                    root()
                        .as_path()
                        .join(format!("{}.txt", "a".repeat(index + 1))),
                    index as u64,
                    EntryInclusion::Included,
                    true,
                )
            })
            .collect::<Vec<_>>();
        entries.push(RenameSnapshotEntry::new(
            SnapshotEntryRef(9_999),
            root()
                .as_path()
                .join(format!("{}.txt", "a".repeat(CHAIN_LENGTH + 1))),
            CHAIN_LENGTH as u64,
            EntryInclusion::Included,
            false,
        ));
        let preview = preview_renames(&request(
            entries,
            vec![RenameRule::RegexReplace {
                pattern: "^(a*)$".into(),
                replace_with: "${1}a".into(),
            }],
        ));

        assert_eq!(preview.readiness(), PreviewReadiness::Invalid);
        assert_eq!(preview.plan(), None);
        for index in 0..CHAIN_LENGTH - 1 {
            assert_eq!(
                row(&preview, 1_000 + index as u64).status(),
                &PreviewRowStatus::Conflict(ConflictReason::DestinationOccupied {
                    occupant: SnapshotEntryRef(1_001 + index as u64),
                })
            );
        }
        assert_eq!(
            row(&preview, 1_000 + (CHAIN_LENGTH - 1) as u64).status(),
            &PreviewRowStatus::Conflict(ConflictReason::DestinationOccupied {
                occupant: SnapshotEntryRef(9_999),
            })
        );
    }

    #[test]
    fn generated_name_and_rule_counts_are_bounded_without_truncation() {
        let long_prefix = "x".repeat(MAX_WINDOWS_NAME_UTF16_UNITS - 4);
        let long_name = preview_renames(&request(
            vec![entry(1, r"C:\rename-test\a.txt", 0, true)],
            vec![RenameRule::Prefix(long_prefix)],
        ));
        assert_eq!(
            row(&long_name, 1).status(),
            &PreviewRowStatus::Invalid(InvalidReason::InvalidTargetLeaf(
                TargetNameError::TooLong {
                    utf16_units: MAX_WINDOWS_NAME_UTF16_UNITS + 1,
                    limit: MAX_WINDOWS_NAME_UTF16_UNITS,
                }
            ))
        );
        assert!(row(&long_name, 1).proposed_path().is_none());

        let too_many_rules = preview_renames(&request(
            vec![entry(2, r"C:\rename-test\a.txt", 0, true)],
            (0..=MAX_RENAME_RULES)
                .map(|_| RenameRule::Prefix(String::new()))
                .collect(),
        ));
        assert_eq!(too_many_rules.readiness(), PreviewReadiness::Invalid);
        assert!(matches!(
            too_many_rules.rule_errors()[0].kind,
            RuleValidationErrorKind::TooManyRules { .. }
        ));
        assert!(too_many_rules.plan().is_none());
    }

    #[test]
    fn readiness_distinguishes_empty_all_no_op_and_invalid_previews() {
        let empty = preview_renames(&request(Vec::new(), Vec::new()));
        assert_eq!(empty.readiness(), PreviewReadiness::EmptySelection);
        assert!(empty.plan().is_none());

        let no_op = preview_renames(&request(
            vec![entry(1, r"C:\rename-test\same.txt", 0, true)],
            Vec::new(),
        ));
        assert_eq!(no_op.readiness(), PreviewReadiness::AllNoOp);
        assert_eq!(row(&no_op, 1).status(), &PreviewRowStatus::NoOp);
        assert!(no_op.plan().is_none());

        let invalid = preview_renames(&request(
            vec![entry(2, r"C:\rename-test\same", 0, true)],
            vec![RenameRule::Suffix(".".into())],
        ));
        assert_eq!(invalid.readiness(), PreviewReadiness::Invalid);
        assert!(invalid.plan().is_none());
    }

    #[test]
    fn frozen_plan_identity_changes_with_root_rules_or_selection() {
        let original = request(
            vec![entry(1, r"C:\rename-test\source.txt", 0, true)],
            vec![RenameRule::Prefix("new-".into())],
        );
        let plan = preview_renames(&original).plan().unwrap().clone();
        assert!(plan.matches_request(&original));

        let changed_rules = RenameRequest::new(
            original.snapshot().clone(),
            RenameRuleSet::new(
                ExtensionPolicy::Preserve,
                vec![RenameRule::Prefix("other-".into())],
            ),
        );
        assert!(!plan.matches_request(&changed_rules));

        let changed_selection = RenameRequest::new(
            original
                .snapshot()
                .with_selection(SnapshotEntryRef(1), false)
                .unwrap(),
            original.rules().clone(),
        );
        assert!(!plan.matches_request(&changed_selection));

        let other_root = ExplicitDirectoryRoot::new(r"C:\other-root").unwrap();
        let changed_root = RenameRequest::new(
            RenameInputSnapshot::new(
                other_root,
                vec![entry(1, r"C:\other-root\source.txt", 0, true)],
            )
            .unwrap(),
            original.rules().clone(),
        );
        assert!(!plan.matches_request(&changed_root));
    }

    #[test]
    fn snapshot_enforces_recursion_off_by_default_and_bounded_counts() {
        let nested = snapshot(vec![entry(1, r"C:\rename-test\child\source.txt", 0, true)]);
        assert!(!nested.is_recursive());
        let rejected = preview_renames(&RenameRequest::new(
            nested.clone(),
            RenameRuleSet::new(
                ExtensionPolicy::Preserve,
                vec![RenameRule::Prefix("new-".into())],
            ),
        ));
        assert_eq!(
            row(&rejected, 1).status(),
            &PreviewRowStatus::Invalid(InvalidReason::NestedSourceWhenRecursionDisabled)
        );
        let recursive = nested.with_recursive(true).unwrap();
        assert!(recursive.is_recursive());
        let accepted = preview_renames(&RenameRequest::new(
            recursive,
            RenameRuleSet::new(
                ExtensionPolicy::Preserve,
                vec![RenameRule::Prefix("new-".into())],
            ),
        ));
        assert_eq!(accepted.readiness(), PreviewReadiness::Ready);

        let too_many = (0..=MAX_SELECTED_ENTRIES)
            .map(|index| {
                RenameSnapshotEntry::new(
                    SnapshotEntryRef(index as u64),
                    format!(r"C:\rename-test\{index}.txt"),
                    index as u64,
                    EntryInclusion::Included,
                    true,
                )
            })
            .collect();
        assert!(matches!(
            RenameInputSnapshot::new(root(), too_many),
            Err(SnapshotError::TooManySelected { .. })
        ));
    }

    #[cfg(windows)]
    #[test]
    fn non_unicode_windows_filename_is_refused_without_lossy_rewriting() {
        use std::os::windows::ffi::OsStringExt;

        let non_unicode = OsString::from_wide(&[0xd800]);
        let path = root().as_path().join(non_unicode);
        let snapshot = RenameInputSnapshot::new(
            root(),
            vec![RenameSnapshotEntry::new(
                SnapshotEntryRef(1),
                path,
                0,
                EntryInclusion::Included,
                true,
            )],
        )
        .unwrap();
        let preview = preview_renames(&RenameRequest::new(snapshot, RenameRuleSet::default()));

        assert_eq!(
            row(&preview, 1).status(),
            &PreviewRowStatus::Invalid(InvalidReason::UnsupportedFilenameEncoding)
        );
        assert!(row(&preview, 1).proposed_name().is_none());
        assert!(preview.plan().is_none());
    }
}
