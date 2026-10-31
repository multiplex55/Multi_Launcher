use crate::diff::folder_compare::{FolderAlignmentOverride, FolderModel};
use crate::diff::folder_scan::ScanRules;
use crate::diff::settings::{DiffConfigV1, FolderColumnWidthsV1, FolderSortColumn};
use crate::diff::text_compare::{
    self, AlignedDiffRow, CompiledRules, FindMatch, FindScope, IntralineLocation,
    ManualTextAlignment, NavigationDirection, RowProjectionMode, TextComparisonResult,
    TextComparisonRules,
};
use crate::diff::text_file::{LineEdit, TextDocument, load_text_file};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
fn id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum FolderStatusFilter {
    Identical,
    Differences,
    LeftOnly,
    RightOnly,
    LeftNewer,
    RightNewer,
    Errors,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum FolderDisplayFilter {
    #[default]
    All,
    Differences,
    Identical,
    LeftOnly,
    RightOnly,
    LeftNewer,
    RightNewer,
    Errors,
    LeftChanges,
    RightChanges,
    Orphans,
    Changes,
    /// Union of independently selected status categories.
    Combined(BTreeSet<FolderStatusFilter>),
}
impl FolderDisplayFilter {
    pub fn matches(&self, status: crate::diff::folder_compare::FolderStatus) -> bool {
        use crate::diff::folder_compare::FolderStatus as S;
        use FolderDisplayFilter as F;
        use FolderStatusFilter as C;
        let criterion = |c: &C| match c {
            C::Identical => status == S::Identical,
            C::Differences => status.is_different(),
            C::LeftOnly => status == S::LeftOnly,
            C::RightOnly => status == S::RightOnly,
            C::LeftNewer => status == S::LeftNewer,
            C::RightNewer => status == S::RightNewer,
            C::Errors => matches!(status, S::Unreadable | S::Error),
        };
        match self {
            F::All => true,
            F::Differences | F::Changes => status.is_different(),
            F::Identical => status == S::Identical,
            F::LeftOnly => status == S::LeftOnly,
            F::RightOnly => status == S::RightOnly,
            F::LeftNewer => status == S::LeftNewer,
            F::RightNewer => status == S::RightNewer,
            F::Errors => matches!(status, S::Unreadable | S::Error),
            F::LeftChanges => matches!(status, S::LeftOnly | S::LeftNewer),
            F::RightChanges => matches!(status, S::RightOnly | S::RightNewer),
            F::Orphans => matches!(status, S::LeftOnly | S::RightOnly),
            F::Combined(criteria) => criteria.is_empty() || criteria.iter().any(criterion),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FolderSortState {
    pub column: FolderSortColumn,
    pub descending: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContentComparisonMode {
    Metadata,
    #[default]
    OnDemand,
    Always,
}

/// Complete, editable folder comparison configuration.  This is deliberately
/// separate from the fields which produced the current model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderRulesDraft {
    pub compare_file_size: bool,
    pub compare_modified_timestamps: bool,
    /// Text is retained while editing so NaN, infinity and negative input can
    /// be reported without ever entering the applied configuration.
    pub timestamp_tolerance_seconds: String,
    pub content_comparison: ContentComparisonMode,
    pub use_text_compare_rules: bool,
    pub text_rules: TextComparisonRules,
    pub include_rules: String,
    pub exclude_rules: String,
}

impl Default for FolderRulesDraft {
    fn default() -> Self {
        Self {
            compare_file_size: true,
            compare_modified_timestamps: true,
            timestamp_tolerance_seconds: "2".into(),
            content_comparison: ContentComparisonMode::default(),
            use_text_compare_rules: true,
            text_rules: TextComparisonRules::default(),
            include_rules: String::new(),
            exclude_rules: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FolderCompareState {
    pub left_root: PathBuf,
    pub right_root: PathBuf,
    pub model: FolderModel,
    pub selected_paths: BTreeSet<PathBuf>,
    pub primary_selection: Option<PathBuf>,
    pub expanded_nodes: BTreeSet<PathBuf>,
    pub scroll_anchor: Option<PathBuf>,
    pub display_filter: FolderDisplayFilter,
    pub path_filter: String,
    pub sort: FolderSortState,
    pub column_widths: FolderColumnWidthsV1,
    /// Rules that produced `model`; drafts never mutate these implicitly.
    pub applied_scan_rules: ScanRules,
    pub draft_rules: FolderRulesDraft,
    /// Rules used by asynchronous content refinement of text file pairs.
    pub text_rules: TextComparisonRules,
    pub content_comparison: ContentComparisonMode,
    pub timestamp_tolerance: Duration,
    pub compare_file_size: bool,
    pub compare_modified_timestamps: bool,
    pub use_text_compare_rules: bool,
    pub folder_rules_open: bool,
    pub folder_rules_cancel_snapshot: Option<FolderRulesDraft>,
    pub left_scan_complete: bool,
    pub right_scan_complete: bool,
    /// Children changed by an editor and awaiting a targeted metadata refresh.
    pub stale_paths: BTreeSet<PathBuf>,
    pub alignment_overrides: Vec<FolderAlignmentOverride>,
    /// First endpoint selected by the two-step `Align With…` interaction.
    pub pending_alignment: Option<(bool, PathBuf)>,
}
impl Default for FolderCompareState {
    fn default() -> Self {
        Self {
            left_root: PathBuf::new(),
            right_root: PathBuf::new(),
            model: FolderModel::default(),
            selected_paths: BTreeSet::new(),
            primary_selection: None,
            expanded_nodes: BTreeSet::new(),
            scroll_anchor: None,
            display_filter: FolderDisplayFilter::All,
            path_filter: String::new(),
            sort: FolderSortState {
                column: FolderSortColumn::Path,
                descending: false,
            },
            column_widths: FolderColumnWidthsV1::default(),
            applied_scan_rules: ScanRules::default(),
            draft_rules: FolderRulesDraft::default(),
            text_rules: TextComparisonRules::default(),
            content_comparison: ContentComparisonMode::default(),
            timestamp_tolerance: Duration::from_secs(2),
            compare_file_size: true,
            compare_modified_timestamps: true,
            use_text_compare_rules: true,
            folder_rules_open: false,
            folder_rules_cancel_snapshot: None,
            left_scan_complete: false,
            right_scan_complete: false,
            stale_paths: BTreeSet::new(),
            alignment_overrides: vec![],
            pending_alignment: None,
        }
    }
}

impl FolderCompareState {
    pub fn apply_table_preferences(&mut self, settings: &DiffConfigV1) {
        self.sort = FolderSortState {
            column: settings.folder_sort.column,
            descending: settings.folder_sort.descending,
        };
        self.column_widths = settings.folder_column_widths.validated();
    }
    fn draft_lines(value: &str) -> Vec<String> {
        value
            .lines()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_owned)
            .collect()
    }

    pub fn validated_draft_scan_rules(&self) -> Result<ScanRules, String> {
        ScanRules::validated(
            Self::draft_lines(&self.draft_rules.include_rules),
            Self::draft_lines(&self.draft_rules.exclude_rules),
        )
    }

    pub fn validated_draft_tolerance(&self) -> Result<Duration, String> {
        const MAX_SECONDS: f64 = 86_400.0;
        let value: f64 = self
            .draft_rules
            .timestamp_tolerance_seconds
            .trim()
            .parse()
            .map_err(|_| "Timestamp tolerance must be a number".to_owned())?;
        if !value.is_finite() {
            return Err("Timestamp tolerance must be finite".into());
        }
        if value < 0.0 {
            return Err("Timestamp tolerance cannot be negative".into());
        }
        if value > MAX_SECONDS {
            return Err("Timestamp tolerance cannot exceed 24 hours".into());
        }
        Ok(Duration::from_secs_f64(value))
    }

    pub fn validate_draft(&self) -> Result<(ScanRules, Duration), String> {
        Ok((
            self.validated_draft_scan_rules()?,
            self.validated_draft_tolerance()?,
        ))
    }

    pub fn apply_draft(&mut self) -> Result<(), String> {
        let (rules, tolerance) = self.validate_draft()?;
        self.applied_scan_rules = rules;
        self.timestamp_tolerance = tolerance;
        self.content_comparison = self.draft_rules.content_comparison;
        self.text_rules = self.draft_rules.text_rules.clone();
        self.compare_file_size = self.draft_rules.compare_file_size;
        self.compare_modified_timestamps = self.draft_rules.compare_modified_timestamps;
        self.use_text_compare_rules = self.draft_rules.use_text_compare_rules;
        Ok(())
    }

    pub fn rescan_required(&self) -> bool {
        self.validate_draft().is_ok_and(|(rules, tolerance)| {
            rules != self.applied_scan_rules
                || tolerance != self.timestamp_tolerance
                || self.draft_rules.content_comparison != self.content_comparison
                || self.draft_rules.text_rules != self.text_rules
                || self.draft_rules.compare_file_size != self.compare_file_size
                || self.draft_rules.compare_modified_timestamps != self.compare_modified_timestamps
                || self.draft_rules.use_text_compare_rules != self.use_text_compare_rules
        })
    }
}

#[cfg(test)]
mod folder_rule_tests {
    use super::*;
    #[test]
    fn invalid_draft_is_rejected_without_model_or_applied_mutation() {
        let mut state = FolderCompareState::default();
        state.model.revision = 7;
        let applied = state.applied_scan_rules.clone();
        for invalid in ["NaN", "inf", "-1", "86401"] {
            state.draft_rules.timestamp_tolerance_seconds = invalid.into();
            assert!(state.apply_draft().is_err());
            assert_eq!(state.model.revision, 7);
            assert_eq!(state.applied_scan_rules, applied);
        }
        state.draft_rules.timestamp_tolerance_seconds = "2".into();
        state.draft_rules.exclude_rules = "../secret".into();
        assert!(state.apply_draft().is_err());
    }

    #[test]
    fn successful_apply_and_rescan_required_transitions_are_atomic() {
        let mut state = FolderCompareState::default();
        assert!(!state.rescan_required());
        state.draft_rules.exclude_rules = "*.tmp".into();
        assert!(state.rescan_required());
        assert!(state.applied_scan_rules.excludes.is_empty());
        state.apply_draft().unwrap();
        assert_eq!(state.applied_scan_rules.excludes, ["*.tmp"]);
        assert!(!state.rescan_required());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextCompareOrigin {
    FileBacked,
    Memory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextCompareState {
    pub left: Option<PathBuf>,
    pub right: Option<PathBuf>,
    pub relative_path: Option<PathBuf>,
    /// `None` paths may denote a missing folder-child side. Origin identifies
    /// memory text explicitly instead of inferring it from absent paths.
    pub origin: TextCompareOrigin,
}
#[derive(Debug, Clone, PartialEq)]
pub struct BinaryCompareState {
    pub left: Option<PathBuf>,
    pub right: Option<PathBuf>,
    pub relative_path: Option<PathBuf>,
}
#[derive(Debug, Clone, PartialEq)]
pub enum DiffView {
    Start,
    TextCompare(TextCompareState),
    BinaryCompare(BinaryCompareState),
    FolderCompare(Box<FolderCompareState>),
}
#[derive(Debug, Clone, PartialEq)]
pub struct RetainedView {
    pub id: u64,
    pub view: DiffView,
}

#[derive(Debug, Clone)]
pub struct DiffWorkspace {
    pub left_visible: String,
    pub right_visible: String,
    pub left_normalized: Option<PathBuf>,
    pub right_normalized: Option<PathBuf>,
    pub workspace_id: u64,
    pub current_view: RetainedView,
    pub navigation_stack: Vec<RetainedView>,
    pub settings: DiffConfigV1,
    pub current_job_generation: u64,
    pub progress: Option<crate::diff::worker::DiffProgress>,
    pub status: Option<String>,
    pub error: Option<String>,
    pub focus_left_requested: bool,
}

impl Default for DiffWorkspace {
    fn default() -> Self {
        Self::new(DiffConfigV1::default())
    }
}
impl DiffWorkspace {
    /// Swap the semantic endpoints of the active comparison.  When a folder
    /// child is open its retained parent is swapped as well, so Back continues
    /// to describe (and reconcile against) the same pair of roots.
    pub fn swap_sides(&mut self) -> Result<(), String> {
        fn swap_view(view: &mut DiffView) {
            match view {
                DiffView::TextCompare(s) => std::mem::swap(&mut s.left, &mut s.right),
                DiffView::BinaryCompare(s) => std::mem::swap(&mut s.left, &mut s.right),
                DiffView::FolderCompare(s) => {
                    std::mem::swap(&mut s.left_root, &mut s.right_root);
                    s.model = Default::default();
                    s.left_scan_complete = false;
                    s.right_scan_complete = false;
                    for a in &mut s.alignment_overrides {
                        std::mem::swap(&mut a.left_relative, &mut a.right_relative);
                    }
                    s.pending_alignment = s.pending_alignment.take().map(|(left, p)| (!left, p));
                }
                DiffView::Start => {}
            }
        }
        if matches!(self.current_view.view, DiffView::Start) {
            return Err("Open a comparison before swapping sides".into());
        }
        swap_view(&mut self.current_view.view);
        if matches!(
            self.current_view.view,
            DiffView::TextCompare(TextCompareState {
                relative_path: Some(_),
                ..
            }) | DiffView::BinaryCompare(BinaryCompareState {
                relative_path: Some(_),
                ..
            })
        ) && let Some(parent) = self.navigation_stack.last_mut()
        {
            swap_view(&mut parent.view);
        }
        std::mem::swap(&mut self.left_visible, &mut self.right_visible);
        std::mem::swap(&mut self.left_normalized, &mut self.right_normalized);
        self.current_job_generation = self.current_job_generation.wrapping_add(1);
        Ok(())
    }
    pub fn new(settings: DiffConfigV1) -> Self {
        Self {
            left_visible: String::new(),
            right_visible: String::new(),
            left_normalized: None,
            right_normalized: None,
            workspace_id: id(),
            current_view: RetainedView {
                id: id(),
                view: DiffView::Start,
            },
            navigation_stack: vec![],
            settings,
            current_job_generation: 0,
            progress: None,
            status: None,
            error: None,
            focus_left_requested: true,
        }
    }
    pub fn open_invocation(
        &mut self,
        left: Option<String>,
        right: Option<String>,
    ) -> Result<(), String> {
        match (left, right) {
            (None, None) => {
                self.focus_left_requested = true;
                Ok(())
            }
            (Some(l), None) => {
                self.left_visible = l.clone();
                self.left_normalized = Some(crate::diff::query::normalize_path(&l));
                self.focus_left_requested = false;
                Ok(())
            }
            (l, r) => self.open_paths(l.unwrap_or_default(), r.unwrap_or_default()),
        }
    }
    pub fn open_paths(&mut self, left: String, right: String) -> Result<(), String> {
        self.left_visible = left;
        self.right_visible = right;
        self.focus_left_requested = false;
        let lp = crate::diff::query::normalize_path(&self.left_visible);
        let rp = crate::diff::query::normalize_path(&self.right_visible);
        self.left_normalized = Some(lp.clone());
        self.right_normalized = Some(rp.clone());
        let lm = metadata(&lp, "Left");
        let rm = metadata(&rp, "Right");
        let (lm, rm) = match (lm, rm) {
            (Ok(l), Ok(r)) => (l, r),
            (l, r) => {
                let msg = [l.err(), r.err()]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join("; ");
                self.error = Some(msg.clone());
                return Err(msg);
            }
        };
        let view = if lm.is_file() && rm.is_file() {
            if paths_are_binary(Some(&lp), Some(&rp)) {
                DiffView::BinaryCompare(BinaryCompareState {
                    left: Some(lp.clone()),
                    right: Some(rp.clone()),
                    relative_path: None,
                })
            } else {
                DiffView::TextCompare(TextCompareState {
                    left: Some(lp.clone()),
                    right: Some(rp.clone()),
                    relative_path: None,
                    origin: TextCompareOrigin::FileBacked,
                })
            }
        } else if lm.is_dir() && rm.is_dir() {
            let mut folder = FolderCompareState {
                left_root: lp,
                right_root: rp,
                ..FolderCompareState::default()
            };
            folder.apply_table_preferences(&self.settings);
            DiffView::FolderCompare(Box::new(folder))
        } else {
            let msg = "Left and right paths must both be files or both be directories".to_string();
            self.error = Some(msg.clone());
            return Err(msg);
        };
        self.navigation_stack.clear();
        self.current_view = RetainedView { id: id(), view };
        self.current_job_generation += 1;
        self.error = None;
        self.status = Some("Comparison ready".into());
        Ok(())
    }
    /// Assigns a picker result without starting or disturbing a comparison.
    pub fn assign_selected_path(&mut self, side: DiffSide, selected: Option<PathBuf>) {
        let Some(path) = selected else { return };
        let display = path.as_os_str().to_string_lossy().into_owned();
        match side {
            DiffSide::Left => self.left_visible = display,
            DiffSide::Right => self.right_visible = display,
        }
    }
    /// Opens a folder child; either side may be absent without weakening root validation.
    pub fn push_file_compare(
        &mut self,
        relative_path: PathBuf,
        left: Option<PathBuf>,
        right: Option<PathBuf>,
    ) -> Result<(), String> {
        if left.is_none() && right.is_none() {
            return Err("A folder child must exist on at least one side".into());
        }
        let next = RetainedView {
            id: id(),
            view: if paths_are_binary(left.as_deref(), right.as_deref()) {
                DiffView::BinaryCompare(BinaryCompareState {
                    left,
                    right,
                    relative_path: Some(relative_path),
                })
            } else {
                DiffView::TextCompare(TextCompareState {
                    left,
                    right,
                    relative_path: Some(relative_path),
                    origin: TextCompareOrigin::FileBacked,
                })
            },
        };
        let old = std::mem::replace(&mut self.current_view, next);
        self.navigation_stack.push(old);
        Ok(())
    }
    pub fn back(&mut self) -> bool {
        if let Some(previous) = self.navigation_stack.pop() {
            self.current_view = previous;
            true
        } else {
            false
        }
    }

    /// Pushes a new memory comparison while retaining the current view and its
    /// model identity for Back navigation.
    pub fn push_memory_text_compare(&mut self) -> u64 {
        let next = RetainedView {
            id: id(),
            view: DiffView::TextCompare(TextCompareState {
                left: None,
                right: None,
                relative_path: None,
                origin: TextCompareOrigin::Memory,
            }),
        };
        let next_id = next.id;
        let old = std::mem::replace(&mut self.current_view, next);
        self.navigation_stack.push(old);
        self.current_job_generation = self.current_job_generation.wrapping_add(1);
        next_id
    }

    /// Replaces a folder child with the adjacent differing file from the
    /// retained parent's current visible projection. The parent itself is not
    /// rebuilt or popped, so selection, expansion, filters and scan state stay intact.
    pub fn navigate_different_file(&mut self, next: bool) -> Result<bool, String> {
        let current = match &self.current_view.view {
            DiffView::TextCompare(s) => s.relative_path.as_ref(),
            DiffView::BinaryCompare(s) => s.relative_path.as_ref(),
            _ => None,
        }
        .ok_or("Current comparison has no Folder Compare parent")?
        .clone();
        let parent = self
            .navigation_stack
            .last()
            .and_then(|v| match &v.view {
                DiffView::FolderCompare(s) => Some(s),
                _ => None,
            })
            .ok_or("Current comparison has no Folder Compare parent")?;
        let query = parent.path_filter.to_lowercase();
        let mut entries: Vec<_> = parent
            .model
            .with_alignment_overrides(&parent.alignment_overrides)
            .entries
            .into_values()
            .filter(|entry| {
                parent.display_filter.matches(entry.effective_status)
                    && entry.effective_status.is_different()
                    && !matches!(
                        entry.effective_status,
                        crate::diff::folder_compare::FolderStatus::Unreadable
                            | crate::diff::folder_compare::FolderStatus::Error
                    )
                    && (query.is_empty()
                        || entry
                            .relative_path
                            .to_string_lossy()
                            .to_lowercase()
                            .contains(&query))
                    && entry
                        .left
                        .as_ref()
                        .or(entry.right.as_ref())
                        .and_then(|s| s.metadata.as_ref())
                        .is_some_and(|m| m.kind == crate::diff::folder_compare::EntryKind::File)
                    && entry.left.as_ref().is_none_or(|s| s.error.is_none())
                    && entry.right.as_ref().is_none_or(|s| s.error.is_none())
            })
            .collect();
        entries.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
        if parent.sort.descending {
            entries.reverse();
        }
        let Some(index) = entries.iter().position(|e| e.relative_path == current) else {
            return Ok(false);
        };
        let target = if next {
            entries.get(index + 1)
        } else {
            index.checked_sub(1).and_then(|i| entries.get(i))
        };
        let Some(target) = target else {
            return Ok(false);
        };
        let relative_path = target.relative_path.clone();
        let left = target.left.as_ref().map(|s| s.path.clone());
        let right = target.right.as_ref().map(|s| s.path.clone());
        self.current_view = RetainedView {
            id: id(),
            view: if paths_are_binary(left.as_deref(), right.as_deref()) {
                DiffView::BinaryCompare(BinaryCompareState {
                    left,
                    right,
                    relative_path: Some(relative_path),
                })
            } else {
                DiffView::TextCompare(TextCompareState {
                    left,
                    right,
                    relative_path: Some(relative_path),
                    origin: TextCompareOrigin::FileBacked,
                })
            },
        };
        Ok(true)
    }
}

/// Preferred and minimum Diff inner viewport sizes, before adapting them to the work area.
pub const DIFF_DEFAULT_SIZE: [f32; 2] = [900.0, 650.0];
pub const DIFF_MIN_SIZE: [f32; 2] = [400.0, 250.0];

/// Returns the minimum Diff window size that fits in the current work area.
pub fn diff_window_min_size(screen: [f32; 4]) -> [f32; 2] {
    let screen_width = (screen[2] - screen[0]).max(1.0);
    let screen_height = (screen[3] - screen[1]).max(1.0);
    [
        DIFF_MIN_SIZE[0].min(screen_width),
        DIFF_MIN_SIZE[1].min(screen_height),
    ]
}

/// Returns safe initial window geometry for the current work area.
///
/// This is deliberately the only policy used when restoring a diff window.
/// Content and comparison mode are not inputs: they must never influence the
/// inner viewport size or remembered outer position.
pub fn validated_window_geometry(
    persistence: &crate::diff::persistence::DiffPersistenceV1,
    screen: [f32; 4],
) -> ([f32; 2], Option<[f32; 2]>) {
    let screen_width = (screen[2] - screen[0]).max(1.0);
    let screen_height = (screen[3] - screen[1]).max(1.0);
    let effective_min = diff_window_min_size(screen);
    let default = [
        DIFF_DEFAULT_SIZE[0].min(screen_width),
        DIFF_DEFAULT_SIZE[1].min(screen_height),
    ];
    let size = persistence
        .window_size
        .filter(|s| {
            s.iter().all(|v| v.is_finite() && *v > 0.0)
                && s[0] >= effective_min[0]
                && s[1] >= effective_min[1]
        })
        .map(|s| {
            [
                s[0].clamp(effective_min[0], screen_width),
                s[1].clamp(effective_min[1], screen_height),
            ]
        })
        .unwrap_or(default);
    let position = persistence.window_position.and_then(|p| {
        p.iter().all(|v| v.is_finite()).then(|| {
            [
                p[0].clamp(screen[0], screen[2] - size[0]),
                p[1].clamp(screen[1], screen[3] - size[1]),
            ]
        })
    });
    (size, position)
}
fn metadata(path: &Path, side: &str) -> Result<fs::Metadata, String> {
    fs::metadata(path).map_err(|e| format!("{side} path '{}': {e}", path.display()))
}
fn paths_are_binary(left: Option<&Path>, right: Option<&Path>) -> bool {
    let binary = |p: &Path| {
        const ARCHIVES: &[&str] = &[
            "zip", "7z", "rar", "tar", "gz", "tgz", "bz2", "xz", "zst", "lz", "jar", "war", "apk",
            "cab", "arj", "iso", "dmg", "deb", "rpm",
        ];
        if p.extension()
            .and_then(|v| v.to_str())
            .is_some_and(|v| ARCHIVES.iter().any(|x| v.eq_ignore_ascii_case(x)))
        {
            return true;
        }
        let mut bytes = [0; 8192];
        use std::io::Read;
        fs::File::open(p)
            .ok()
            .and_then(|mut f| f.read(&mut bytes).ok())
            .is_some_and(|n| bytes[..n].contains(&0) || std::str::from_utf8(&bytes[..n]).is_err())
    };
    left.is_some_and(binary) || right.is_some_and(binary)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiffSide {
    Left,
    Right,
}

/// Retained scroll controller shared by both text panes. Pixel positions are
/// deliberately transient; only the two user preferences are serializable.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct TextScrollState {
    pub sync_vertical: bool,
    pub sync_horizontal: bool,
    #[serde(skip)]
    pub left_vertical: f32,
    #[serde(skip)]
    pub right_vertical: f32,
    #[serde(skip)]
    pub left_horizontal: f32,
    #[serde(skip)]
    pub right_horizontal: f32,
    #[serde(skip)]
    pub active_driver: DiffSide,
    /// Aligned comparison row and the fractional visual position within it.
    #[serde(skip)]
    pub aligned_row: usize,
    #[serde(skip)]
    pub within_row: f32,
    /// Retained X positions are hidden, rather than destroyed, while wrapped.
    #[serde(skip)]
    pub wrapped: bool,
}

impl Default for TextScrollState {
    fn default() -> Self {
        Self {
            sync_vertical: true,
            sync_horizontal: true,
            left_vertical: 0.0,
            right_vertical: 0.0,
            left_horizontal: 0.0,
            right_horizontal: 0.0,
            active_driver: DiffSide::Left,
            aligned_row: 0,
            within_row: 0.0,
            wrapped: false,
        }
    }
}

impl TextScrollState {
    pub fn offsets(&self, side: DiffSide) -> (f32, f32) {
        match side {
            DiffSide::Left => (self.left_horizontal, self.left_vertical),
            DiffSide::Right => (self.right_horizontal, self.right_vertical),
        }
    }

    pub fn set_driver(&mut self, side: DiffSide) {
        self.active_driver = side;
    }

    /// Records a user-driven offset and applies synchronization in aligned
    /// visual coordinates (row plus within-row pixels), including blank rows.
    pub fn drive(&mut self, side: DiffSide, x: f32, y: f32, row_height: f32, wrapped: bool) {
        self.active_driver = side;
        self.wrapped = wrapped;
        let h = row_height.max(1.0);
        self.aligned_row = (y / h).floor().max(0.0) as usize;
        self.within_row = y.rem_euclid(h);
        let aligned_y = self.aligned_row as f32 * h + self.within_row;
        match side {
            DiffSide::Left => {
                self.left_vertical = y.max(0.0);
                if !wrapped {
                    self.left_horizontal = x.max(0.0);
                }
                if self.sync_vertical {
                    self.right_vertical = aligned_y;
                }
                if self.sync_horizontal && !wrapped {
                    self.right_horizontal = self.left_horizontal;
                }
            }
            DiffSide::Right => {
                self.right_vertical = y.max(0.0);
                if !wrapped {
                    self.right_horizontal = x.max(0.0);
                }
                if self.sync_vertical {
                    self.left_vertical = aligned_y;
                }
                if self.sync_horizontal && !wrapped {
                    self.left_horizontal = self.right_horizontal;
                }
            }
        }
    }

    pub fn set_sync(&mut self, vertical: bool, horizontal: bool) {
        let enabling_v = vertical && !self.sync_vertical;
        let enabling_h = horizontal && !self.sync_horizontal;
        self.sync_vertical = vertical;
        self.sync_horizontal = horizontal;
        let (x, y) = self.offsets(self.active_driver);
        if enabling_v || enabling_h {
            self.drive(self.active_driver, x, y, 1.0, self.wrapped);
        }
    }

    pub fn target_aligned_row(&mut self, row: usize, row_height: f32) {
        self.aligned_row = row;
        self.within_row = 0.0;
        let y = row as f32 * row_height.max(1.0);
        self.left_vertical = y;
        self.right_vertical = y;
    }

    pub fn drive_measured(&mut self, side: DiffSide, x: f32, y: f32, cache: &RowMeasurementCache) {
        let (projected, within) = cache.row_at_offset(y as f64);
        self.drive(side, x, y, MIN_VISUAL_LINE_HEIGHT, true);
        self.aligned_row = cache
            .projected_to_comparison
            .get(projected)
            .copied()
            .unwrap_or(0);
        self.within_row = within;
    }

    pub fn target_measured_row(&mut self, comparison_row: usize, cache: &RowMeasurementCache) {
        let Some(projected) = cache
            .comparison_to_projected
            .get(comparison_row)
            .and_then(|row| *row)
        else {
            self.aligned_row = comparison_row;
            self.within_row = 0.0;
            return;
        };
        self.aligned_row = comparison_row;
        self.within_row = 0.0;
        let y = cache.offset_for_row(projected) as f32;
        self.left_vertical = y;
        self.right_vertical = y;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourcePosition {
    pub side: DiffSide,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollAnchor {
    pub side: DiffSide,
    pub source_line: usize,
    pub offset: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseChoice {
    Save,
    Discard,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowMeasureKey {
    pub left_source_id: u64,
    pub right_source_id: u64,
    pub left_revision: u64,
    pub right_revision: u64,
    pub comparison_revision: u64,
    pub annotation_revision: u64,
    pub left_text_width_bits: u32,
    pub right_text_width_bits: u32,
    pub font_theme: u64,
    pub text_style: u64,
    pub display_config: u64,
    pub wrap: bool,
    pub projection_mode: u8,
    pub projection_context: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingScrollRequest {
    row: usize,
    generation: u64,
}

impl PendingScrollRequest {
    pub fn row(self) -> usize {
        self.row
    }
}

pub const MIN_VISUAL_LINE_HEIGHT: f32 = 20.0;

/// Retained geometry for the current projected comparison. `offsets` is kept
/// in sync with `heights`, making all viewport and navigation queries O(log n).
#[derive(Debug, Clone)]
pub struct RowMeasurementCache {
    pub key: Option<RowMeasureKey>,
    pub heights: Vec<f32>,
    /// Double precision prevents prefix accumulation from losing individual
    /// pixels once very large comparisons exceed f32's exact-integer range.
    pub offsets: Vec<f64>,
    pub projected_to_comparison: Vec<usize>,
    pub comparison_to_projected: Vec<Option<usize>>,
}

impl Default for RowMeasurementCache {
    fn default() -> Self {
        Self {
            key: None,
            heights: vec![],
            offsets: vec![0.0],
            projected_to_comparison: vec![],
            comparison_to_projected: vec![],
        }
    }
}

impl RowMeasurementCache {
    pub fn rebuild(
        &mut self,
        key: RowMeasureKey,
        projected: Vec<usize>,
        comparison_rows: usize,
        measured: impl IntoIterator<Item = f32>,
    ) {
        self.key = Some(key);
        self.projected_to_comparison = projected;
        self.comparison_to_projected = vec![None; comparison_rows];
        for (p, &c) in self.projected_to_comparison.iter().enumerate() {
            if let Some(slot) = self.comparison_to_projected.get_mut(c) {
                *slot = Some(p);
            }
        }
        self.heights = measured
            .into_iter()
            .take(self.projected_to_comparison.len())
            .map(Self::valid_height)
            .collect();
        self.heights
            .resize(self.projected_to_comparison.len(), MIN_VISUAL_LINE_HEIGHT);
        self.rebuild_offsets();
    }

    pub fn set_height(&mut self, row: usize, height: f32) {
        if let Some(v) = self.heights.get_mut(row) {
            *v = Self::valid_height(height);
            self.rebuild_offsets();
        }
    }
    fn valid_height(height: f32) -> f32 {
        if height.is_finite() {
            height.max(MIN_VISUAL_LINE_HEIGHT)
        } else {
            MIN_VISUAL_LINE_HEIGHT
        }
    }
    fn rebuild_offsets(&mut self) {
        self.offsets.clear();
        self.offsets.reserve(self.heights.len() + 1);
        self.offsets.push(0.0);
        for &height in &self.heights {
            self.offsets.push(
                self.offsets.last().copied().unwrap_or(0.0) + Self::valid_height(height) as f64,
            );
        }
    }
    pub fn total_height(&self) -> f64 {
        self.offsets.last().copied().unwrap_or(0.0)
    }
    pub fn offset_for_row(&self, row: usize) -> f64 {
        self.offsets
            .get(row.min(self.heights.len()))
            .copied()
            .unwrap_or(0.0)
    }
    pub fn row_at_offset(&self, y: f64) -> (usize, f32) {
        if self.heights.is_empty() {
            return (0, 0.0);
        }
        let y = if y.is_finite() {
            y.max(0.0).min(self.total_height())
        } else {
            0.0
        };
        let row = self
            .offsets
            .partition_point(|&offset| offset <= y)
            .saturating_sub(1)
            .min(self.heights.len() - 1);
        (
            row,
            (y - self.offsets[row])
                .max(0.0)
                .min(self.heights[row] as f64) as f32,
        )
    }
    pub fn visible_range(
        &self,
        scroll_y: f32,
        viewport: f32,
        overscan: usize,
    ) -> std::ops::Range<usize> {
        if self.heights.is_empty() {
            return 0..0;
        }
        let first = self.row_at_offset(scroll_y as f64).0;
        let end_y = ((scroll_y.max(0.0) + viewport.max(0.0)) as f64).min(self.total_height());
        let end = self
            .offsets
            .partition_point(|&offset| offset < end_y)
            .max(first + 1)
            .min(self.heights.len());
        first.saturating_sub(overscan)..(end + overscan).min(self.heights.len())
    }
}

/// Clamp persisted values as they cross the model boundary; NaN and corrupt
/// values intentionally restore the documented 50/50 layout.
pub fn validated_splitter(value: f32) -> f32 {
    if value.is_finite() && (0.15..=0.85).contains(&value) {
        value
    } else {
        0.5
    }
}

pub fn visible_row_range(
    heights: &[f32],
    scroll_y: f32,
    viewport: f32,
    overscan: usize,
) -> std::ops::Range<usize> {
    let mut cache = RowMeasurementCache::default();
    cache.rebuild(
        RowMeasureKey {
            left_source_id: 0,
            right_source_id: 0,
            left_revision: 0,
            right_revision: 0,
            comparison_revision: 0,
            annotation_revision: 0,
            left_text_width_bits: 0,
            right_text_width_bits: 0,
            font_theme: 0,
            text_style: 0,
            display_config: 0,
            wrap: true,
            projection_mode: 0,
            projection_context: 0,
        },
        (0..heights.len()).collect(),
        heights.len(),
        heights.iter().copied(),
    );
    cache.visible_range(scroll_y, viewport, overscan)
}

pub fn visual_to_source(row: &AlignedDiffRow, side: DiffSide) -> Option<usize> {
    match side {
        DiffSide::Left => row.left,
        DiffSide::Right => row.right,
    }
}

pub fn restore_anchor(rows: &[AlignedDiffRow], anchor: ScrollAnchor) -> usize {
    rows.iter()
        .position(|r| visual_to_source(r, anchor.side) == Some(anchor.source_line))
        .or_else(|| {
            rows.iter().position(|r| {
                visual_to_source(r, anchor.side).is_some_and(|n| n >= anchor.source_line)
            })
        })
        .unwrap_or_else(|| rows.len().saturating_sub(1))
}

struct CompareMessage {
    generation: u64,
    result: TextComparisonResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceLineSelection {
    pub source_id: u64,
    pub revision: u64,
    pub side: DiffSide,
    pub anchor_line: usize,
    pub focus_line: usize,
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, PartialEq, Eq)]
pub struct SelectedSourceText {
    pub source_id: u64,
    pub revision: u64,
    pub side: DiffSide,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingTextAlignment {
    pub source_id: u64,
    pub revision: u64,
    pub side: DiffSide,
    pub source_line: usize,
}

/// Non-egui controller for a retained text comparison. The last accepted
/// alignment remains available while `recalculating` is true.
pub struct TextViewModel {
    pub origin: TextCompareOrigin,
    pub left_path: Option<PathBuf>,
    pub right_path: Option<PathBuf>,
    pub left: TextDocument,
    pub right: TextDocument,
    pub comparison: Option<TextComparisonResult>,
    pub rules: TextComparisonRules,
    pub active_side: DiffSide,
    pub current_row: Option<usize>,
    /// A model-row navigation target waiting to be consumed by the view.
    pub pending_scroll_row: Option<usize>,
    pending_scroll_generation: u64,
    pub scroll: TextScrollState,
    pub row_measurements: RowMeasurementCache,
    pub projection_mode: RowProjectionMode,
    pub projection_context: usize,
    pub find_open: bool,
    pub find_query: String,
    pub find_scope: FindScope,
    pub find_case_sensitive: bool,
    pub find_projection_only: bool,
    pub find_matches: Vec<FindMatch>,
    pub current_find_match: Option<usize>,
    pub wrap: bool,
    pub syntax: bool,
    pub visible_whitespace: bool,
    pub visible_line_endings: bool,
    pub text_details_open: bool,
    pub text_details_height: f32,
    pub manual_alignments: Vec<ManualTextAlignment>,
    pub pending_alignment: Option<PendingTextAlignment>,
    pub alignment_error: Option<String>,
    pub source_line_selection: Option<SourceLineSelection>,
    pub annotation_revision: u64,
    pub annotation_notice: Option<String>,
    pub projection_notice: Option<String>,
    manual_alignment_source_ids: (u64, u64),
    manual_alignment_source_revisions: (u64, u64),
    pub current_intraline: Option<IntralineLocation>,
    pub syntax_cache: crate::diff::syntax::SyntaxCache,
    pub large_file_tier: crate::diff::worker::LargeFileTier,
    large_file_policy: crate::diff::worker::LargeFilePolicy,
    pub splitter: f32,
    pub recalculate_at: Option<Instant>,
    pub recalculating: bool,
    pub left_error: Option<String>,
    pub right_error: Option<String>,
    pub external_conflict: [bool; 2],
    pub cursor: Option<SourcePosition>,
    /// Set only after a successful write, so returning without saving is inert.
    pub saved_filesystem_mutation: bool,
    generation: u64,
    receiver: Option<Receiver<CompareMessage>>,
}

impl TextViewModel {
    #[cfg(test)]
    pub(crate) fn from_test_text(left: String, right: String) -> Self {
        let mut model = Self::new_memory(left, right, &DiffConfigV1::default()).unwrap();
        model.wrap = false;
        model.scroll.wrapped = false;
        for _ in 0..100 {
            model.poll();
            if model.comparison.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(model.comparison.is_some(), "in-memory comparison completed");
        model
    }

    pub fn swap_sides(&mut self) {
        std::mem::swap(&mut self.left_path, &mut self.right_path);
        std::mem::swap(&mut self.left, &mut self.right);
        std::mem::swap(&mut self.left_error, &mut self.right_error);
        self.external_conflict.swap(0, 1);
        std::mem::swap(
            &mut self.scroll.left_horizontal,
            &mut self.scroll.right_horizontal,
        );
        std::mem::swap(
            &mut self.scroll.left_vertical,
            &mut self.scroll.right_vertical,
        );
        self.active_side = other(self.active_side);
        self.source_line_selection = self.source_line_selection.map(|mut selection| {
            selection.side = other(selection.side);
            selection
        });
        if let Some(cursor) = &mut self.cursor {
            cursor.side = other(cursor.side);
        }
        for anchor in &mut self.manual_alignments {
            std::mem::swap(&mut anchor.left_line, &mut anchor.right_line);
        }
        self.pending_alignment = self.pending_alignment.map(|mut anchor| {
            anchor.side = other(anchor.side);
            anchor
        });
        self.manual_alignment_source_ids = (
            self.manual_alignment_source_ids.1,
            self.manual_alignment_source_ids.0,
        );
        self.manual_alignment_source_revisions = (
            self.manual_alignment_source_revisions.1,
            self.manual_alignment_source_revisions.0,
        );
        self.current_intraline = None;
        self.comparison = None;
        self.row_measurements = Default::default();
        self.annotation_revision = self.annotation_revision.wrapping_add(1);
        self.schedule_compare();
    }

    /// Reload is intentionally all-or-nothing with respect to dirty buffers.
    pub fn reload_files(&mut self) -> Result<(), String> {
        if self.origin == TextCompareOrigin::Memory {
            return Err(
                "In-memory text has no files to reload; use Recompare or Edit Inputs".into(),
            );
        }
        if self.has_dirty() {
            return Err(
                "Reload would discard unsaved edits; save or discard them explicitly first".into(),
            );
        }
        let load = |path: Option<&PathBuf>| -> Result<TextDocument, String> {
            path.map(|p| {
                load_text_file(p)
                    .map_err(|e| format!("{}: {e}", p.display()))
                    .and_then(|f| {
                        TextDocument::from_loaded(&f)
                            .ok_or_else(|| "binary content is not editable".into())
                    })
            })
            .unwrap_or_else(|| Ok(TextDocument::empty()))
        };
        let left = load(self.left_path.as_ref())?;
        let right = load(self.right_path.as_ref())?;
        let cleared_bookmarks =
            !self.left.bookmarks().is_empty() || !self.right.bookmarks().is_empty();
        let cleared_annotations = !self.left.ignored_line_ranges().is_empty()
            || !self.right.ignored_line_ranges().is_empty()
            || !self.manual_alignments.is_empty()
            || self.source_line_selection.is_some()
            || self.pending_alignment.is_some();
        self.left = left;
        self.right = right;
        self.external_conflict = [false; 2];
        self.manual_alignments.clear();
        self.pending_alignment = None;
        self.source_line_selection = None;
        self.manual_alignment_source_ids = (self.left.source_id(), self.right.source_id());
        self.manual_alignment_source_revisions = (self.left.revision, self.right.revision);
        self.annotation_revision = self.annotation_revision.wrapping_add(1);
        if cleared_annotations || cleared_bookmarks {
            self.annotation_notice = Some(
                "Reload cleared source annotations, bookmarks, selections, or anchors for the replaced sources."
                    .into(),
            );
        }
        self.schedule_compare();
        Ok(())
    }

    pub fn recompare(&mut self) {
        self.schedule_compare();
    }
    /// Installs a clean external reload while monotonically advancing the
    /// document revision used to reject stale comparison results.
    pub fn reload_external(
        &mut self,
        side: DiffSide,
        loaded: &crate::diff::text_file::LoadedTextFile,
    ) -> Result<(), String> {
        let replacement = TextDocument::from_loaded(loaded)
            .ok_or_else(|| "externally changed file is no longer text".to_string())?;
        let had_ignored = match side {
            DiffSide::Left => !self.left.ignored_line_ranges().is_empty(),
            DiffSide::Right => !self.right.ignored_line_ranges().is_empty(),
        };
        let had_bookmarks = match side {
            DiffSide::Left => !self.left.bookmarks().is_empty(),
            DiffSide::Right => !self.right.bookmarks().is_empty(),
        };
        let had_selection = self
            .source_line_selection
            .is_some_and(|selection| selection.side == side)
            || self
                .pending_alignment
                .is_some_and(|pending| pending.side == side);
        let document = match side {
            DiffSide::Left => &mut self.left,
            DiffSide::Right => &mut self.right,
        };
        let next = document.revision.wrapping_add(1);
        *document = replacement;
        document.revision = next;
        document.saved_revision = next;
        self.manual_alignments.clear();
        if self
            .pending_alignment
            .is_some_and(|pending| pending.side == side)
        {
            self.pending_alignment = None;
        }
        self.alignment_error = Some("Reload cleared anchors tied to the replaced source.".into());
        if self
            .source_line_selection
            .is_some_and(|selection| selection.side == side)
        {
            self.source_line_selection = None;
        }
        self.manual_alignment_source_ids = (self.left.source_id(), self.right.source_id());
        self.manual_alignment_source_revisions = (self.left.revision, self.right.revision);
        self.annotation_revision = self.annotation_revision.wrapping_add(1);
        if had_ignored || had_selection || had_bookmarks {
            self.annotation_notice = Some(
                "Reload cleared ignored-line ranges, bookmarks, or source selection for the reloaded source."
                    .into(),
            );
        }
        self.external_conflict[if side == DiffSide::Left { 0 } else { 1 }] = false;
        self.schedule_compare();
        Ok(())
    }
    pub fn load(state: &TextCompareState, settings: &DiffConfigV1) -> Result<Self, String> {
        if state.origin == TextCompareOrigin::Memory {
            return Err("Memory comparisons must be created from explicit text input".into());
        }
        fn doc(path: Option<&PathBuf>) -> Result<TextDocument, String> {
            path.map(|p| {
                load_text_file(p)
                    .map_err(|e| format!("{}: {e}", p.display()))
                    .and_then(|f| {
                        TextDocument::from_loaded(&f)
                            .ok_or_else(|| "binary content is not editable".into())
                    })
            })
            .unwrap_or_else(|| Ok(TextDocument::empty()))
        }
        let left = doc(state.left.as_ref())?;
        let right = doc(state.right.as_ref())?;
        Self::from_documents(state, settings, left, right)
    }

    /// Creates a memory-backed comparison using the same comparison model and
    /// worker as file-backed text views.
    pub fn new_memory(
        left: String,
        right: String,
        settings: &DiffConfigV1,
    ) -> Result<Self, String> {
        validate_memory_text(&left)?;
        validate_memory_text(&right)?;
        let state = TextCompareState {
            left: None,
            right: None,
            relative_path: None,
            origin: TextCompareOrigin::Memory,
        };
        Self::from_documents(
            &state,
            settings,
            TextDocument::from_memory_source(left),
            TextDocument::from_memory_source(right),
        )
    }

    fn from_documents(
        state: &TextCompareState,
        settings: &DiffConfigV1,
        left: TextDocument,
        right: TextDocument,
    ) -> Result<Self, String> {
        let manual_alignment_source_ids = (left.source_id(), right.source_id());
        let manual_alignment_source_revisions = (left.revision, right.revision);
        let wrap =
            if crate::diff::syntax::code_like(state.left.as_deref().or(state.right.as_deref())) {
                false
            } else {
                settings.wrap_text
            };
        let bytes = left.source().len().saturating_add(right.source().len()) as u64;
        let estimated_rows = if state.origin == TextCompareOrigin::Memory {
            memory_text_line_count(left.source()).max(memory_text_line_count(right.source())) as u64
        } else {
            left.source()
                .bytes()
                .filter(|b| *b == b'\n')
                .count()
                .max(right.source().bytes().filter(|b| *b == b'\n').count()) as u64
                + 1
        };
        let large_file_tier = settings.large_file_policy().tier(bytes, estimated_rows);
        let mut out = Self {
            left_path: state.left.clone(),
            right_path: state.right.clone(),
            origin: state.origin,
            left,
            right,
            comparison: None,
            rules: TextComparisonRules::default(),
            active_side: DiffSide::Left,
            current_row: None,
            pending_scroll_row: None,
            pending_scroll_generation: 0,
            scroll: TextScrollState::default(),
            row_measurements: RowMeasurementCache::default(),
            projection_mode: RowProjectionMode::All,
            projection_context: 3,
            find_open: false,
            find_query: String::new(),
            find_scope: FindScope::Both,
            find_case_sensitive: false,
            find_projection_only: false,
            find_matches: vec![],
            current_find_match: None,
            wrap,
            syntax: settings.syntax_highlighting && large_file_tier.syntax_enabled(),
            visible_whitespace: false,
            visible_line_endings: false,
            text_details_open: false,
            text_details_height: 140.0,
            manual_alignments: vec![],
            pending_alignment: None,
            alignment_error: None,
            source_line_selection: None,
            annotation_revision: 0,
            annotation_notice: None,
            projection_notice: None,
            manual_alignment_source_ids,
            manual_alignment_source_revisions,
            current_intraline: None,
            syntax_cache: Default::default(),
            large_file_tier,
            large_file_policy: settings.large_file_policy(),
            splitter: validated_splitter(settings.pane_split),
            recalculate_at: Some(Instant::now()),
            recalculating: true,
            left_error: None,
            right_error: None,
            external_conflict: [false, false],
            cursor: None,
            saved_filesystem_mutation: false,
            generation: 0,
            receiver: None,
        };
        out.start_compare();
        Ok(out)
    }

    /// Commits edited memory inputs through each document's revision/undo
    /// owner. Validation and read-only checks run for both sides before either
    /// source changes.
    pub fn replace_memory_sources(&mut self, left: String, right: String) -> Result<bool, String> {
        if self.origin != TextCompareOrigin::Memory {
            return Err("Only an in-memory comparison can replace text inputs".into());
        }
        validate_memory_text(&left)?;
        validate_memory_text(&right)?;
        if self.left.read_only || self.right.read_only {
            return Err("In-memory text documents must be editable".into());
        }

        let mut next_left = self.left.clone();
        let mut next_right = self.right.clone();
        let had_alignments = !self.manual_alignments.is_empty();
        let left_changed = next_left.replace_source(left)?;
        let right_changed = next_right.replace_source(right)?;
        if !left_changed && !right_changed {
            return Ok(false);
        }
        let cleared_ignored = (left_changed && !self.left.ignored_line_ranges().is_empty())
            || (right_changed && !self.right.ignored_line_ranges().is_empty());
        let cleared_bookmarks = (left_changed && !self.left.bookmarks().is_empty())
            || (right_changed && !self.right.bookmarks().is_empty());
        let cleared_selection = self.source_line_selection.is_some_and(|selection| {
            (left_changed && selection.side == DiffSide::Left)
                || (right_changed && selection.side == DiffSide::Right)
        }) || self.pending_alignment.is_some_and(|pending| {
            (left_changed && pending.side == DiffSide::Left)
                || (right_changed && pending.side == DiffSide::Right)
        });

        next_left.take_bookmark_invalidation();
        next_right.take_bookmark_invalidation();

        self.left = next_left;
        self.right = next_right;
        self.manual_alignments.clear();
        if left_changed
            && self
                .pending_alignment
                .is_some_and(|pending| pending.side == DiffSide::Left)
            || right_changed
                && self
                    .pending_alignment
                    .is_some_and(|pending| pending.side == DiffSide::Right)
        {
            self.pending_alignment = None;
        }
        if left_changed
            && self
                .source_line_selection
                .is_some_and(|selection| selection.side == DiffSide::Left)
            || right_changed
                && self
                    .source_line_selection
                    .is_some_and(|selection| selection.side == DiffSide::Right)
        {
            self.source_line_selection = None;
        }
        if had_alignments {
            self.alignment_error = Some(
                "Replacing a source cleared manual alignments tied to the previous text.".into(),
            );
        } else {
            self.alignment_error = None;
        }
        self.manual_alignment_source_ids = (self.left.source_id(), self.right.source_id());
        self.manual_alignment_source_revisions = (self.left.revision, self.right.revision);
        self.annotation_revision = self.annotation_revision.wrapping_add(1);
        let mut notices = Vec::new();
        if cleared_ignored || cleared_selection {
            notices.push(
                "Replacing text inputs cleared ignored ranges or source selection for the changed source.",
            );
        }
        if cleared_bookmarks {
            notices.push("Replacing text inputs cleared bookmarks for the changed source.");
        }
        if !notices.is_empty() {
            self.annotation_notice = Some(notices.join(" "));
        }
        self.current_intraline = None;
        self.cursor = None;
        self.current_row = None;
        self.pending_scroll_row = None;
        self.row_measurements = Default::default();
        self.find_matches.clear();
        self.current_find_match = None;
        self.comparison = None;
        self.generation = self.generation.wrapping_add(1);
        self.receiver = None;
        self.large_file_tier = self.large_file_policy.tier(
            self.left
                .source()
                .len()
                .saturating_add(self.right.source().len()) as u64,
            memory_text_line_count(self.left.source())
                .max(memory_text_line_count(self.right.source())) as u64,
        );
        self.schedule_compare();
        Ok(true)
    }
    pub fn schedule_compare(&mut self) {
        self.recalculate_at = Some(Instant::now() + Duration::from_millis(250));
        self.recalculating = true;
    }
    pub fn poll(&mut self) {
        if self.recalculate_at.is_some_and(|t| Instant::now() >= t) {
            self.start_compare();
        }
        if let Some(rx) = &self.receiver
            && let Ok(m) = rx.try_recv()
            && m.generation == self.generation
            && !m.result.is_stale(
                self.left.source_id(),
                self.right.source_id(),
                self.left.revision,
                self.right.revision,
                self.rules.revision,
                self.annotation_revision,
            )
        {
            let anchor = self.cursor.map(|p| ScrollAnchor {
                side: p.side,
                source_line: p.line,
                offset: 0.0,
            });
            self.current_row = anchor
                .map(|a| restore_anchor(&m.result.rows, a))
                .or(self.current_row);
            self.comparison = Some(m.result);
            self.recalculating = false;
            if let Some(row) = self.pending_scroll_row {
                self.request_scroll_row(Some(row));
            }
        }
    }
    fn start_compare(&mut self) {
        self.recalculate_at = None;
        let source_ids = (self.left.source_id(), self.right.source_id());
        let source_revisions = (self.left.revision, self.right.revision);
        if self.manual_alignment_source_ids != source_ids
            || self.manual_alignment_source_revisions != source_revisions
        {
            self.manual_alignments.clear();
            self.pending_alignment = None;
            self.manual_alignment_source_ids = source_ids;
            self.manual_alignment_source_revisions = source_revisions;
            self.alignment_error = Some(
                "Manual alignments were cleared because their source documents changed.".into(),
            );
        }
        self.generation += 1;
        let generation = self.generation;
        let (
            l,
            r,
            left_id,
            right_id,
            lr,
            rr,
            rules,
            anchors,
            left_ignored,
            right_ignored,
            annotation_revision,
        ) = (
            self.left.source().to_owned(),
            self.right.source().to_owned(),
            self.left.source_id(),
            self.right.source_id(),
            self.left.revision,
            self.right.revision,
            self.rules.clone(),
            self.manual_alignments.clone(),
            self.left.ignored_line_ranges().to_vec(),
            self.right.ignored_line_ranges().to_vec(),
            self.annotation_revision,
        );
        let (tx, rx) = mpsc::channel();
        self.receiver = Some(rx);
        self.recalculating = true;
        std::thread::spawn(move || {
            if let Ok(c) = CompiledRules::compile(&rules)
                && let Ok(result) = text_compare::compare_with_annotations(
                    &l,
                    &r,
                    left_id,
                    right_id,
                    lr,
                    rr,
                    &c,
                    4096,
                    &anchors,
                    &left_ignored,
                    &right_ignored,
                    annotation_revision,
                )
            {
                let _ = tx.send(CompareMessage { generation, result });
            }
        });
    }
    pub fn projected_rows(&self) -> Vec<usize> {
        self.comparison.as_ref().map_or_else(Vec::new, |c| {
            text_compare::row_projection(c, self.projection_mode, self.projection_context)
        })
    }
    pub fn set_row_projection(&mut self, mode: RowProjectionMode, context: usize) {
        let context = context.min(text_compare::MAX_ROW_PROJECTION_CONTEXT);
        if self.projection_mode == mode && self.projection_context == context {
            return;
        }
        self.projection_mode = mode;
        self.projection_context = context;
        self.row_measurements = Default::default();
        if self.find_projection_only {
            self.refresh_find();
        }
        if let Some(row) = self.pending_scroll_row {
            self.request_scroll_row(Some(row));
        }
    }
    pub fn refresh_find(&mut self) {
        let projection = self.find_projection_only.then(|| self.projected_rows());
        self.find_matches = self.comparison.as_ref().map_or_else(Vec::new, |c| {
            text_compare::find_matches(
                c,
                self.left.source(),
                self.right.source(),
                &self.find_query,
                self.find_scope,
                self.active_side,
                self.find_case_sensitive,
                projection.as_deref(),
            )
        });
        self.current_find_match = None;
    }
    pub fn navigate_find(&mut self, forward: bool) {
        self.current_find_match =
            text_compare::navigate_match(&self.find_matches, self.current_find_match, forward);
        if let Some(m) = self
            .current_find_match
            .and_then(|i| self.find_matches.get(i).copied())
        {
            self.active_side = m.side;
            self.request_scroll_row(Some(m.row));
        }
    }
    pub fn request_overview_scroll(&mut self, y: f32, height: f32) {
        if let Some(c) = &self.comparison {
            let row = text_compare::overview_row(y, height, c.rows.len());
            self.request_scroll_row(row);
        }
    }
    pub fn navigate(&mut self, direction: NavigationDirection) {
        if let Some(c) = &self.comparison {
            let row = c.navigate(self.current_row, direction, false, true);
            self.current_row = row;
            self.request_scroll_row(row);
        }
    }
    pub fn navigate_intraline(&mut self, direction: NavigationDirection) {
        if let Some(c) = &self.comparison {
            self.current_intraline = c.navigate_intraline(self.current_intraline, direction, false);
            if let Some(location) = self.current_intraline {
                self.active_side = location.side;
                self.current_row = Some(location.row);
                self.request_scroll_row(Some(location.row));
            }
        }
    }
    pub fn select_source_line(&mut self, side: DiffSide, line: usize, extend: bool) -> bool {
        let document = match side {
            DiffSide::Left => &self.left,
            DiffSide::Right => &self.right,
        };
        let count =
            text_compare::source_line_count(document.source(), self.rules.line_ending_equivalence);
        if line >= count {
            return false;
        }
        let source_id = document.source_id();
        let revision = document.revision;
        let anchor_line = if extend {
            self.source_line_selection
                .filter(|selection| {
                    selection.side == side
                        && selection.source_id == source_id
                        && selection.revision == revision
                })
                .map_or(line, |selection| selection.anchor_line)
        } else {
            line
        };
        self.source_line_selection = Some(SourceLineSelection {
            source_id,
            revision,
            side,
            anchor_line,
            focus_line: line,
            start: anchor_line.min(line),
            end: anchor_line.max(line) + 1,
        });
        self.active_side = side;
        true
    }

    pub fn move_source_selection(&mut self, side: DiffSide, delta: isize, extend: bool) -> bool {
        let existing = self.source_line_selection.filter(|selection| {
            let document = match side {
                DiffSide::Left => &self.left,
                DiffSide::Right => &self.right,
            };
            selection.side == side
                && selection.source_id == document.source_id()
                && selection.revision == document.revision
        });
        let current = existing.map(|selection| selection.focus_line);
        let current = current.or_else(|| {
            self.current_row
                .and_then(|row| self.comparison.as_ref()?.rows.get(row))
                .and_then(|row| visual_to_source(row, side))
        });
        let document = match side {
            DiffSide::Left => &self.left,
            DiffSide::Right => &self.right,
        };
        let count =
            text_compare::source_line_count(document.source(), self.rules.line_ending_equivalence);
        if count == 0 {
            return false;
        }
        let next = current.map_or(0, |line| {
            if delta < 0 {
                line.saturating_sub(delta.unsigned_abs())
            } else {
                line.saturating_add(delta as usize).min(count - 1)
            }
        });
        self.select_source_line(side, next, extend)
    }

    pub fn source_line_is_ignored(&self, side: DiffSide, line: usize) -> bool {
        let document = match side {
            DiffSide::Left => &self.left,
            DiffSide::Right => &self.right,
        };
        text_compare::line_is_ignored(document.ignored_line_ranges(), line)
    }

    pub fn has_fresh_source_selection(&self) -> bool {
        self.fresh_source_selection().is_ok()
    }

    /// Captures the selected source lines exactly as stored, including original
    /// Unicode bytes, delimiters, and any ignored source lines in the interval.
    pub fn selected_raw_source_text(&self) -> Result<SelectedSourceText, String> {
        let selection = self.fresh_source_selection()?;
        let document = match selection.side {
            DiffSide::Left => &self.left,
            DiffSide::Right => &self.right,
        };
        let text = document
            .raw_line_range(selection.start, selection.end)
            .ok_or_else(|| "Source selection no longer maps to raw text".to_string())?;
        Ok(SelectedSourceText {
            source_id: selection.source_id,
            revision: selection.revision,
            side: selection.side,
            text: text.to_owned(),
        })
    }

    pub fn toggle_bookmark_at_selection(&mut self) -> Result<bool, String> {
        let selection = self.fresh_source_selection()?;
        let document = match selection.side {
            DiffSide::Left => &mut self.left,
            DiffSide::Right => &mut self.right,
        };
        Ok(document.toggle_bookmark(selection.focus_line))
    }

    /// Navigates bookmarks in comparison-row order, left before right on ties.
    /// It only updates source selection and scroll state, leaving the active
    /// difference/hunk unchanged.
    pub fn navigate_bookmark(&mut self, forward: bool) -> Result<(), String> {
        let has_selection = self.source_line_selection.is_some();
        let selected = if has_selection {
            Some(self.fresh_source_selection()?)
        } else {
            None
        };
        let (bookmarks, current) = {
            let comparison = self.fresh_comparison()?;
            let mut bookmarks = Vec::new();
            for (row_index, row) in comparison.rows.iter().enumerate() {
                if let Some(line) = row.left
                    && self.left.is_bookmarked(line)
                {
                    bookmarks.push((row_index, DiffSide::Left, line));
                }
                if let Some(line) = row.right
                    && self.right.is_bookmarked(line)
                {
                    bookmarks.push((row_index, DiffSide::Right, line));
                }
            }
            let current = selected
                .map(|selection| {
                    let row = comparison
                        .rows
                        .iter()
                        .position(|row| {
                            visual_to_source(row, selection.side) == Some(selection.focus_line)
                        })
                        .ok_or_else(|| {
                            "Focused source line has no current comparison row".to_string()
                        })?;
                    Ok::<_, String>((row, selection.side))
                })
                .transpose()?;
            (bookmarks, current)
        };
        let target = match (current, forward) {
            (Some((row, side)), true) => bookmarks
                .iter()
                .find(|(candidate_row, candidate_side, _)| {
                    (*candidate_row, bookmark_side_order(*candidate_side))
                        > (row, bookmark_side_order(side))
                })
                .copied(),
            (Some((row, side)), false) => bookmarks
                .iter()
                .rev()
                .find(|(candidate_row, candidate_side, _)| {
                    (*candidate_row, bookmark_side_order(*candidate_side))
                        < (row, bookmark_side_order(side))
                })
                .copied(),
            (None, true) => bookmarks.first().copied(),
            (None, false) => bookmarks.last().copied(),
        }
        .ok_or_else(|| {
            if bookmarks.is_empty() {
                "No source bookmarks are available".to_string()
            } else if forward {
                "There is no later source bookmark".to_string()
            } else {
                "There is no earlier source bookmark".to_string()
            }
        })?;
        let (row, side, line) = target;
        if !self.select_source_line(side, line, false) {
            return Err("Bookmark no longer maps to a source line".into());
        }
        self.request_scroll_row(Some(row));
        Ok(())
    }

    fn fresh_source_selection(&self) -> Result<SourceLineSelection, String> {
        let selection = self
            .source_line_selection
            .ok_or("Select one or more source lines first")?;
        let document = match selection.side {
            DiffSide::Left => &self.left,
            DiffSide::Right => &self.right,
        };
        if selection.source_id != document.source_id() || selection.revision != document.revision {
            return Err("Source selection is stale; select the lines again".into());
        }
        Ok(selection)
    }

    pub fn ignore_selected_lines(&mut self) -> Result<(), String> {
        let selection = self.fresh_source_selection()?;
        let document = match selection.side {
            DiffSide::Left => &mut self.left,
            DiffSide::Right => &mut self.right,
        };
        let line_count =
            text_compare::source_line_count(document.source(), self.rules.line_ending_equivalence);
        let mut ranges = document.ignored_line_ranges().to_vec();
        ranges.push(text_compare::IgnoredLineRange {
            start: selection.start,
            end: selection.end,
        });
        let ranges = text_compare::normalize_ignored_ranges(&ranges, line_count)?;
        if !document.set_ignored_line_ranges(ranges)? {
            return Ok(());
        }
        let mut removed_anchor = false;
        self.manual_alignments.retain(|anchor| {
            let endpoint_is_selected = match selection.side {
                DiffSide::Left => (selection.start..selection.end).contains(&anchor.left_line),
                DiffSide::Right => (selection.start..selection.end).contains(&anchor.right_line),
            };
            removed_anchor |= endpoint_is_selected;
            !endpoint_is_selected
        });
        if self.pending_alignment.is_some_and(|pending| {
            pending.side == selection.side
                && (selection.start..selection.end).contains(&pending.source_line)
        }) {
            self.pending_alignment = None;
            removed_anchor = true;
        }
        if removed_anchor {
            self.alignment_error =
                Some("Ignored lines cannot remain manual-alignment endpoints; restore a line before aligning it.".into());
        }
        self.annotation_revision = self.annotation_revision.wrapping_add(1);
        self.row_measurements = Default::default();
        self.schedule_compare();
        Ok(())
    }

    pub fn restore_selected_ignored_lines(&mut self) -> Result<(), String> {
        let selection = self.fresh_source_selection()?;
        let document = match selection.side {
            DiffSide::Left => &mut self.left,
            DiffSide::Right => &mut self.right,
        };
        let mut changed = false;
        let mut restored = Vec::new();
        for range in document.ignored_line_ranges() {
            if range.end <= selection.start || range.start >= selection.end {
                restored.push(*range);
                continue;
            }
            changed = true;
            if range.start < selection.start {
                restored.push(text_compare::IgnoredLineRange {
                    start: range.start,
                    end: selection.start,
                });
            }
            if range.end > selection.end {
                restored.push(text_compare::IgnoredLineRange {
                    start: selection.end,
                    end: range.end,
                });
            }
        }
        if !changed {
            return Err("The selected source lines are not ignored".into());
        }
        document.set_ignored_line_ranges(restored)?;
        self.annotation_revision = self.annotation_revision.wrapping_add(1);
        self.row_measurements = Default::default();
        self.schedule_compare();
        Ok(())
    }

    pub fn clear_ignored_ranges(&mut self) -> bool {
        let mut changed = false;
        changed |= self.left.set_ignored_line_ranges(vec![]).unwrap_or(false);
        changed |= self.right.set_ignored_line_ranges(vec![]).unwrap_or(false);
        if changed {
            self.annotation_revision = self.annotation_revision.wrapping_add(1);
            self.row_measurements = Default::default();
            self.schedule_compare();
        }
        changed
    }

    pub fn begin_manual_alignment(&mut self, side: DiffSide, source_line: usize) {
        let document = match side {
            DiffSide::Left => &self.left,
            DiffSide::Right => &self.right,
        };
        if text_compare::line_is_ignored(document.ignored_line_ranges(), source_line) {
            self.alignment_error = Some(
                "Ignored source lines cannot be manual-alignment endpoints; restore the line first."
                    .into(),
            );
            return;
        }
        self.pending_alignment = Some(PendingTextAlignment {
            source_id: document.source_id(),
            revision: document.revision,
            side,
            source_line,
        });
    }

    pub fn set_manual_alignments(
        &mut self,
        anchors: Vec<ManualTextAlignment>,
    ) -> Result<(), String> {
        let left_lines =
            text_compare::source_line_count(self.left.source(), self.rules.line_ending_equivalence);
        let right_lines = text_compare::source_line_count(
            self.right.source(),
            self.rules.line_ending_equivalence,
        );
        let anchors = text_compare::validate_manual_alignments(&anchors, left_lines, right_lines)?;
        if anchors.iter().any(|anchor| {
            self.source_line_is_ignored(DiffSide::Left, anchor.left_line)
                || self.source_line_is_ignored(DiffSide::Right, anchor.right_line)
        }) {
            return Err(
                "Manual alignment endpoints cannot be ignored lines; restore them before aligning."
                    .into(),
            );
        }
        self.manual_alignments = anchors;
        self.manual_alignment_source_ids = (self.left.source_id(), self.right.source_id());
        self.manual_alignment_source_revisions = (self.left.revision, self.right.revision);
        self.alignment_error = None;
        self.annotation_revision = self.annotation_revision.wrapping_add(1);
        self.row_measurements = Default::default();
        self.pending_scroll_row = self.current_row.or(Some(self.scroll.aligned_row));
        self.schedule_compare();
        Ok(())
    }
    pub fn add_manual_alignment(&mut self, anchor: ManualTextAlignment) -> Result<(), String> {
        let mut anchors = self.manual_alignments.clone();
        anchors.push(anchor);
        self.set_manual_alignments(anchors)
    }
    pub fn remove_manual_alignment(&mut self, anchor: ManualTextAlignment) -> bool {
        let before = self.manual_alignments.len();
        self.manual_alignments.retain(|a| *a != anchor);
        if self.manual_alignments.len() != before {
            self.annotation_revision = self.annotation_revision.wrapping_add(1);
            self.row_measurements = Default::default();
            self.schedule_compare();
            true
        } else {
            false
        }
    }
    pub fn set_current_row(&mut self, row: Option<usize>) {
        self.current_row = row;
        self.request_scroll_row(row);
    }
    pub fn pending_scroll_request(&self) -> Option<PendingScrollRequest> {
        self.pending_scroll_row.map(|row| PendingScrollRequest {
            row,
            generation: self.pending_scroll_generation,
        })
    }
    pub fn accept_pending_scroll_request(
        &mut self,
        request: PendingScrollRequest,
        measurement_key: &RowMeasureKey,
    ) -> bool {
        if self.pending_scroll_row != Some(request.row)
            || self.pending_scroll_generation != request.generation
            || self.row_measurements.key.as_ref() != Some(measurement_key)
            || self
                .row_measurements
                .comparison_to_projected
                .get(request.row)
                .and_then(|projected| *projected)
                .is_none()
        {
            return false;
        }
        self.pending_scroll_row = None;
        true
    }
    pub fn request_scroll_row(&mut self, row: Option<usize>) {
        self.pending_scroll_generation = self.pending_scroll_generation.wrapping_add(1);
        if let Some(row) = row
            && let Some(comparison) = self.comparison.as_ref()
        {
            if row >= comparison.rows.len() {
                self.projection_notice =
                    Some("The requested row no longer exists in this comparison.".into());
                self.pending_scroll_row = None;
                return;
            }
            let projected = text_compare::row_projection(
                comparison,
                self.projection_mode,
                self.projection_context,
            );
            if projected.binary_search(&row).is_err() {
                self.projection_mode = RowProjectionMode::All;
                self.row_measurements = Default::default();
                self.projection_notice =
                    Some("Showing all rows to reveal the requested source line or match.".into());
                if self.find_projection_only {
                    self.refresh_find();
                }
            }
        }
        self.pending_scroll_row = row;
        if let Some(row) = row {
            self.scroll.target_measured_row(row, &self.row_measurements);
        }
    }
    pub fn set_ignore_all_whitespace(&mut self, value: bool) {
        if self.rules.ignore_all_whitespace != value {
            self.rules.ignore_all_whitespace = value;
            self.rules.revision = self.rules.revision.wrapping_add(1);
            self.schedule_compare();
        }
    }
    pub fn set_rules(&mut self, mut rules: TextComparisonRules) {
        rules.revision = self.rules.revision;
        if rules != self.rules {
            rules.revision = self.rules.revision.wrapping_add(1);
            self.rules = rules;
            self.schedule_compare();
        }
    }
    pub fn undo(&mut self) -> bool {
        let changed_side = self.active_side;
        let changed = match changed_side {
            DiffSide::Left => self.left.undo(),
            DiffSide::Right => self.right.undo(),
        };
        if changed {
            self.annotation_revision = self.annotation_revision.wrapping_add(1);
            if self
                .source_line_selection
                .is_some_and(|selection| selection.side == changed_side)
            {
                self.source_line_selection = None;
            }
            if self
                .pending_alignment
                .is_some_and(|pending| pending.side == changed_side)
            {
                self.pending_alignment = None;
            }
            if !self.manual_alignments.is_empty() {
                self.manual_alignments.clear();
                self.alignment_error =
                    Some("Undo changed source coordinates; manual alignments were cleared.".into());
            }
            self.annotation_notice =
                Some("Undo restored the source and its ignored-line ranges.".into());
            self.row_measurements = Default::default();
            self.schedule_compare()
        }
        changed
    }
    pub fn redo(&mut self) -> bool {
        let changed_side = self.active_side;
        let changed = match changed_side {
            DiffSide::Left => self.left.redo(),
            DiffSide::Right => self.right.redo(),
        };
        if changed {
            self.annotation_revision = self.annotation_revision.wrapping_add(1);
            if self
                .source_line_selection
                .is_some_and(|selection| selection.side == changed_side)
            {
                self.source_line_selection = None;
            }
            if self
                .pending_alignment
                .is_some_and(|pending| pending.side == changed_side)
            {
                self.pending_alignment = None;
            }
            if !self.manual_alignments.is_empty() {
                self.manual_alignments.clear();
                self.alignment_error =
                    Some("Redo changed source coordinates; manual alignments were cleared.".into());
            }
            self.annotation_notice =
                Some("Redo restored the source and its ignored-line ranges.".into());
            self.row_measurements = Default::default();
            self.schedule_compare()
        }
        changed
    }
    pub fn copy_hunk(&mut self, from: DiffSide) -> Result<(), String> {
        if !self.large_file_tier.editable() {
            return Err(self.large_file_tier.explanation().into());
        }
        if self.external_conflict != [false, false] {
            return Err("Resolve external-change conflict before merging".into());
        }
        let c = self.fresh_comparison()?;
        let row = self.current_row.ok_or("Select a difference first")?;
        let hi = c
            .navigation
            .row_to_hunk
            .get(row)
            .and_then(|x| *x)
            .ok_or("Current row is not a difference")?;
        let h = &c.hunks[hi];
        let mut replacement = Vec::new();
        let mut replacement_lines = Vec::new();
        let mut destination = Vec::new();
        for r in &c.rows[h.start_row..h.end_row] {
            if let Some(n) = visual_to_source(r, from) {
                replacement_lines.push(n);
                replacement.push(source_line(
                    match from {
                        DiffSide::Left => self.left.source(),
                        DiffSide::Right => self.right.source(),
                    },
                    n,
                ));
            }
            if let Some(n) = visual_to_source(r, other(from)) {
                destination.push(n);
            }
        }
        if !source_lines_contiguous(&replacement_lines) || !source_lines_contiguous(&destination) {
            return Err("This difference is not a contiguous source-line hunk".into());
        }
        let start = destination
            .first()
            .copied()
            .unwrap_or_else(|| insertion_line(&c.rows, h.start_row, other(from)));
        let edit = LineEdit {
            start,
            delete_count: destination.len(),
            replacement,
        };
        let before_ignored = match other(from) {
            DiffSide::Left => self.left.ignored_line_ranges().to_vec(),
            DiffSide::Right => self.right.ignored_line_ranges().to_vec(),
        };
        let source_selection_cleared = self
            .source_line_selection
            .is_some_and(|selection| selection.side == other(from));
        match other(from) {
            DiffSide::Left => self.left.apply_edits(&[edit.clone()])?,
            DiffSide::Right => self.right.apply_edits(&[edit.clone()])?,
        };
        let target_document = match other(from) {
            DiffSide::Left => &mut self.left,
            DiffSide::Right => &mut self.right,
        };
        let invalidated = target_document.take_ignored_range_invalidation();
        let bookmarks_invalidated = target_document.take_bookmark_invalidation();
        let ignored_changed = before_ignored != target_document.ignored_line_ranges();
        if ignored_changed {
            self.annotation_revision = self.annotation_revision.wrapping_add(1);
        }
        if invalidated {
            self.annotation_notice = Some(
                "Ignored ranges intersecting the copied edit were cleared; unaffected ranges were retained."
                    .into(),
            );
        }
        if source_selection_cleared {
            self.annotation_notice =
                Some("The copied edit cleared selection on the changed source side.".into());
        }
        if bookmarks_invalidated {
            self.annotation_notice = Some(
                "The copied edit replaced bookmarked source lines; those bookmarks were cleared."
                    .into(),
            );
        }
        self.map_manual_alignments_after_edit(other(from), &edit);
        if self
            .source_line_selection
            .is_some_and(|selection| selection.side == other(from))
        {
            self.source_line_selection = None;
        }
        if self
            .pending_alignment
            .is_some_and(|pending| pending.side == other(from))
        {
            self.pending_alignment = None;
        }
        self.row_measurements = Default::default();
        self.schedule_compare();
        Ok(())
    }

    fn fresh_comparison(&self) -> Result<&TextComparisonResult, String> {
        if !self.comparison_is_fresh() {
            return Err("Comparison or ignored-line projection is still calculating".into());
        }
        self.comparison
            .as_ref()
            .ok_or_else(|| "Comparison is still calculating".to_string())
    }

    pub fn comparison_is_fresh(&self) -> bool {
        if self.recalculating || self.recalculate_at.is_some() {
            return false;
        }
        let Some(comparison) = self.comparison.as_ref() else {
            return false;
        };
        !comparison.is_stale(
            self.left.source_id(),
            self.right.source_id(),
            self.left.revision,
            self.right.revision,
            self.rules.revision,
            self.annotation_revision,
        )
    }

    fn map_manual_alignments_after_edit(&mut self, side: DiffSide, edit: &LineEdit) {
        let mut invalidated = false;
        self.manual_alignments.retain_mut(|anchor| {
            let endpoint = match side {
                DiffSide::Left => &mut anchor.left_line,
                DiffSide::Right => &mut anchor.right_line,
            };
            match map_source_line(*endpoint, edit) {
                Some(mapped) => {
                    *endpoint = mapped;
                    true
                }
                None => {
                    invalidated = true;
                    false
                }
            }
        });
        if invalidated {
            self.alignment_error =
                Some("Manual alignments intersecting the copied edit were cleared.".into());
        }
        self.manual_alignment_source_revisions = (self.left.revision, self.right.revision);
    }
    pub fn save(&mut self, side: DiffSide) -> bool {
        let (doc, path, err) = match side {
            DiffSide::Left => (
                &mut self.left,
                self.left_path.as_deref(),
                &mut self.left_error,
            ),
            DiffSide::Right => (
                &mut self.right,
                self.right_path.as_deref(),
                &mut self.right_error,
            ),
        };
        let was_dirty = doc.is_dirty();
        let result = path
            .ok_or_else(|| anyhow::anyhow!("Choose a path before saving"))
            .and_then(|p| doc.save(p));
        *err = result.as_ref().err().map(ToString::to_string);
        if result.is_ok() && was_dirty {
            self.saved_filesystem_mutation = true;
        }
        result.is_ok()
    }

    /// Saves one memory side to an explicitly chosen destination while
    /// retaining its memory origin and pathless document identity.
    pub fn save_as_memory(&mut self, side: DiffSide, path: &Path) -> bool {
        if self.origin != TextCompareOrigin::Memory {
            let error = "Save As is only available for in-memory text".to_string();
            match side {
                DiffSide::Left => self.left_error = Some(error),
                DiffSide::Right => self.right_error = Some(error),
            }
            return false;
        }
        let (document, error) = match side {
            DiffSide::Left => (&mut self.left, &mut self.left_error),
            DiffSide::Right => (&mut self.right, &mut self.right_error),
        };
        let result = document.save_as_exact_utf8(path);
        *error = result.as_ref().err().map(ToString::to_string);
        result.is_ok()
    }
    pub fn has_dirty(&self) -> bool {
        self.left.is_dirty() || self.right.is_dirty()
    }
}

pub const MEMORY_TEXT_MAX_BYTES: usize = 8 * 1024 * 1024;
pub const MEMORY_TEXT_MAX_LINES: usize = 100_000;

/// Rejects oversized input before it enters a retained comparison. Content is
/// never truncated or silently normalized.
pub fn validate_memory_text(source: &str) -> Result<(), String> {
    if source.len() > MEMORY_TEXT_MAX_BYTES {
        return Err(format!(
            "Text input exceeds the {} MiB per-side limit",
            MEMORY_TEXT_MAX_BYTES / (1024 * 1024)
        ));
    }
    let lines = memory_text_line_count(source);
    if lines > MEMORY_TEXT_MAX_LINES {
        return Err(format!(
            "Text input exceeds the {MEMORY_TEXT_MAX_LINES}-line per-side limit"
        ));
    }
    Ok(())
}

fn memory_text_line_count(source: &str) -> usize {
    if source.is_empty() {
        return 0;
    }
    let bytes = source.as_bytes();
    let mut lines = 1usize;
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'\r' => {
                lines = lines.saturating_add(1);
                if bytes.get(index + 1) == Some(&b'\n') {
                    index += 1;
                }
            }
            b'\n' => lines = lines.saturating_add(1),
            _ => {}
        }
        index += 1;
    }
    lines
}
fn other(s: DiffSide) -> DiffSide {
    match s {
        DiffSide::Left => DiffSide::Right,
        DiffSide::Right => DiffSide::Left,
    }
}

fn bookmark_side_order(side: DiffSide) -> u8 {
    match side {
        DiffSide::Left => 0,
        DiffSide::Right => 1,
    }
}
fn source_line(s: &str, n: usize) -> String {
    s.lines().nth(n).unwrap_or_default().to_owned()
}
fn source_lines_contiguous(lines: &[usize]) -> bool {
    lines.windows(2).all(|pair| pair[1] == pair[0] + 1)
}
fn map_source_line(line: usize, edit: &LineEdit) -> Option<usize> {
    let end = edit.start.checked_add(edit.delete_count)?;
    if edit.delete_count == 0 {
        if edit.start <= line {
            line.checked_add(edit.replacement.len())
        } else {
            Some(line)
        }
    } else if line < edit.start {
        Some(line)
    } else if line >= end {
        line.checked_add(edit.replacement.len())?
            .checked_sub(edit.delete_count)
    } else {
        None
    }
}
fn insertion_line(rows: &[AlignedDiffRow], at: usize, side: DiffSide) -> usize {
    rows[..at]
        .iter()
        .rev()
        .find_map(|r| visual_to_source(r, side))
        .map_or(0, |n| n + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wait_for_text_comparison(model: &mut TextViewModel) {
        for _ in 0..500 {
            model.poll();
            if model.comparison.is_some() && !model.recalculating {
                return;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(model.comparison.is_some(), "text comparison completed");
    }

    #[test]
    fn memory_text_constructor_preserves_exact_sources_and_uses_text_engine() {
        let left = "α\0\u{200b}\r\nsame\r\n\nlast\r".to_string();
        let right = "α\0\u{200b}\r\nsame\r\n\nchanged\r".to_string();
        let mut model =
            TextViewModel::new_memory(left.clone(), right.clone(), &DiffConfigV1::default())
                .unwrap();
        assert_eq!(model.origin, TextCompareOrigin::Memory);
        assert_eq!(model.left_path, None);
        assert_eq!(model.right_path, None);
        assert_eq!(model.left.source(), left);
        assert_eq!(model.right.source(), right);
        wait_for_text_comparison(&mut model);
        assert!(!model.comparison.as_ref().unwrap().hunks.is_empty());

        let empty =
            TextViewModel::new_memory(String::new(), String::new(), &Default::default()).unwrap();
        assert_eq!(empty.left.source(), "");
        assert_eq!(empty.right.source(), "");

        model.swap_sides();
        assert_eq!(model.origin, TextCompareOrigin::Memory);
        assert_eq!(model.left.source(), right);
        assert_eq!(model.right.source(), left);
    }

    #[test]
    fn memory_text_limits_reject_without_truncating_or_mutating_existing_sources() {
        let mut model =
            TextViewModel::new_memory("left".into(), "right".into(), &Default::default()).unwrap();
        let before = (
            model.left.source().to_owned(),
            model.right.source().to_owned(),
            model.left.revision,
            model.right.revision,
        );
        let oversized = "x".repeat(MEMORY_TEXT_MAX_BYTES + 1);
        assert!(validate_memory_text(&oversized).is_err());
        assert!(validate_memory_text(&"\n".repeat(MEMORY_TEXT_MAX_LINES)).is_err());
        assert!(
            model
                .replace_memory_sources("accepted".into(), oversized)
                .is_err()
        );
        assert_eq!(model.left.source(), before.0);
        assert_eq!(model.right.source(), before.1);
        assert_eq!(model.left.revision, before.2);
        assert_eq!(model.right.revision, before.3);
    }

    #[test]
    fn memory_text_replacement_rejects_old_comparison_and_invalidates_alignment() {
        let mut model =
            TextViewModel::new_memory("a\nb".into(), "a\nb".into(), &Default::default()).unwrap();
        wait_for_text_comparison(&mut model);
        model
            .set_manual_alignments(vec![ManualTextAlignment {
                left_line: 0,
                right_line: 0,
            }])
            .unwrap();
        let previous_revision = model.left.revision;
        let old_generation = model.generation;
        assert!(
            model
                .replace_memory_sources("changed\nb".into(), "a\nb".into())
                .unwrap()
        );
        assert!(model.generation > old_generation);
        assert_eq!(model.left.revision, previous_revision + 1);
        assert!(model.manual_alignments.is_empty());
        assert!(model.comparison.is_none());
        wait_for_text_comparison(&mut model);
        let result = model.comparison.as_ref().unwrap();
        assert_eq!(result.left_revision, model.left.revision);
        assert_eq!(result.right_revision, model.right.revision);
        assert!(!model.saved_filesystem_mutation);
    }

    #[test]
    fn memory_text_hunk_copy_and_undo_use_document_revision_owner() {
        let original_left = "prefix\r\nleft\nsuffix\r";
        let original_right = "prefix\r\nright\nsuffix\r";
        let mut model = TextViewModel::new_memory(
            original_left.into(),
            original_right.into(),
            &Default::default(),
        )
        .unwrap();
        wait_for_text_comparison(&mut model);
        let row = model
            .comparison
            .as_ref()
            .unwrap()
            .rows
            .iter()
            .position(|row| {
                matches!(
                    row.kind,
                    crate::diff::text_compare::DiffRowKind::Modified
                        | crate::diff::text_compare::DiffRowKind::Deleted
                        | crate::diff::text_compare::DiffRowKind::Inserted
                )
            })
            .unwrap();
        model.current_row = Some(row);
        model.copy_hunk(DiffSide::Left).unwrap();
        assert_eq!(model.right.source(), original_left);
        assert!(model.right.is_dirty());
        model.active_side = DiffSide::Right;
        assert!(model.undo());
        assert_eq!(model.right.source(), original_right);
    }

    #[test]
    fn ignored_lines_restore_raw_text_and_exclude_only_the_selected_side() {
        let original_left = "keep\nextra\nend";
        let original_right = "keep\nend";
        let mut model = TextViewModel::new_memory(
            original_left.into(),
            original_right.into(),
            &Default::default(),
        )
        .unwrap();
        wait_for_text_comparison(&mut model);
        model.select_source_line(DiffSide::Left, 1, false);
        model.ignore_selected_lines().unwrap();
        assert_eq!(model.left.source(), original_left);
        assert_eq!(model.right.source(), original_right);
        wait_for_text_comparison(&mut model);
        let comparison = model.comparison.as_ref().unwrap();
        assert!(!comparison.raw_equal);
        assert!(comparison.equal_under_rules);
        assert!(comparison.rows.iter().any(|row| {
            row.kind == crate::diff::text_compare::DiffRowKind::Ignored
                && row.left == Some(1)
                && row.right.is_none()
        }));
        assert!(comparison.navigation.all_difference_rows.is_empty());
        assert!(!model.left.is_dirty());
        assert!(!model.right.is_dirty());
        assert!(!model.saved_filesystem_mutation);

        model.restore_selected_ignored_lines().unwrap();
        assert_eq!(model.left.source(), original_left);
        assert_eq!(model.right.source(), original_right);
        wait_for_text_comparison(&mut model);
        assert!(!model.comparison.as_ref().unwrap().equal_under_rules);
        model.select_source_line(DiffSide::Right, 0, false);
        model.ignore_selected_lines().unwrap();
        assert!(model.clear_ignored_ranges());
        assert!(model.left.ignored_line_ranges().is_empty());
        assert!(model.right.ignored_line_ranges().is_empty());
    }

    #[test]
    fn ignored_lines_merge_adjacent_ranges_and_restore_a_middle_selection_as_a_split() {
        let mut model = TextViewModel::new_memory(
            "a\nb\nc\nd\ne".into(),
            "a\nb\nc\nd\ne".into(),
            &Default::default(),
        )
        .unwrap();
        wait_for_text_comparison(&mut model);
        model.select_source_line(DiffSide::Left, 1, false);
        model.ignore_selected_lines().unwrap();
        model.select_source_line(DiffSide::Left, 2, false);
        model.ignore_selected_lines().unwrap();
        model.select_source_line(DiffSide::Left, 3, false);
        model.ignore_selected_lines().unwrap();
        assert_eq!(
            model.left.ignored_line_ranges(),
            &[crate::diff::text_compare::IgnoredLineRange { start: 1, end: 4 }]
        );
        let source = model.left.source().to_owned();
        model.select_source_line(DiffSide::Left, 2, false);
        model.restore_selected_ignored_lines().unwrap();
        assert_eq!(model.left.source(), source);
        assert_eq!(
            model.left.ignored_line_ranges(),
            &[
                crate::diff::text_compare::IgnoredLineRange { start: 1, end: 2 },
                crate::diff::text_compare::IgnoredLineRange { start: 3, end: 4 },
            ]
        );
    }

    #[test]
    fn ignored_lines_mouse_and_keyboard_selection_use_source_coordinates() {
        let mut model = TextViewModel::new_memory(
            "one\ntwo\nthree\nfour".into(),
            "one\ntwo\nthree\nfour".into(),
            &Default::default(),
        )
        .unwrap();
        wait_for_text_comparison(&mut model);
        assert!(model.select_source_line(DiffSide::Left, 1, false));
        assert!(model.move_source_selection(DiffSide::Left, 2, true));
        assert_eq!(
            model.source_line_selection,
            Some(SourceLineSelection {
                source_id: model.left.source_id(),
                revision: model.left.revision,
                side: DiffSide::Left,
                anchor_line: 1,
                focus_line: 3,
                start: 1,
                end: 4,
            })
        );
        assert!(model.move_source_selection(DiffSide::Left, -1, true));
        assert_eq!(
            model.source_line_selection.unwrap().start..model.source_line_selection.unwrap().end,
            1..3
        );
        model.select_source_line(DiffSide::Right, 2, false);
        assert_eq!(model.source_line_selection.unwrap().side, DiffSide::Right);
        assert_eq!(
            model.source_line_selection.unwrap().source_id,
            model.right.source_id()
        );
    }

    #[test]
    fn m3c_selected_raw_source_text_keeps_exact_bytes_and_ignored_lines() {
        let original = "α\r\nignored\r\nlast\n";
        let mut model =
            TextViewModel::new_memory(original.into(), "right".into(), &DiffConfigV1::default())
                .unwrap();
        wait_for_text_comparison(&mut model);
        model.select_source_line(DiffSide::Left, 1, false);
        model.ignore_selected_lines().unwrap();
        let selected = model.selected_raw_source_text().unwrap();
        assert_eq!(selected.side, DiffSide::Left);
        assert_eq!(selected.source_id, model.left.source_id());
        assert_eq!(selected.revision, model.left.revision);
        assert_eq!(selected.text, "ignored\r\n");
        assert_eq!(model.left.source(), original);

        model
            .left
            .replace_source("changed\r\nignored\r\nlast\n".into())
            .unwrap();
        assert!(
            model
                .selected_raw_source_text()
                .err()
                .unwrap()
                .contains("selection is stale")
        );
    }

    #[test]
    fn m3c_bookmarks_navigate_by_row_and_side_without_moving_the_active_hunk() {
        let mut model = TextViewModel::new_memory(
            "same\nleft change".into(),
            "same\nright change".into(),
            &DiffConfigV1::default(),
        )
        .unwrap();
        wait_for_text_comparison(&mut model);
        let difference_row = model
            .comparison
            .as_ref()
            .unwrap()
            .navigation
            .all_difference_rows[0];
        model.current_row = Some(difference_row);
        model.projection_mode = RowProjectionMode::DifferencesOnly;
        model.projection_context = 0;

        let annotation_revision = model.annotation_revision;
        let left_revision = model.left.revision;
        let right_revision = model.right.revision;
        model.select_source_line(DiffSide::Left, 0, false);
        assert!(model.toggle_bookmark_at_selection().unwrap());
        model.select_source_line(DiffSide::Right, 0, false);
        assert!(model.toggle_bookmark_at_selection().unwrap());
        assert_eq!(model.annotation_revision, annotation_revision);
        assert_eq!(model.left.revision, left_revision);
        assert_eq!(model.right.revision, right_revision);
        assert!(!model.left.is_dirty() && !model.right.is_dirty());
        model.source_line_selection = None;

        model.navigate_bookmark(true).unwrap();
        assert_eq!(
            model.source_line_selection.unwrap().side,
            DiffSide::Left,
            "left wins a same-row tie"
        );
        assert_eq!(model.source_line_selection.unwrap().focus_line, 0);
        assert_eq!(model.current_row, Some(difference_row));
        assert_eq!(model.projection_mode, RowProjectionMode::All);
        assert!(model.projection_notice.is_some());

        model.navigate_bookmark(true).unwrap();
        assert_eq!(model.source_line_selection.unwrap().side, DiffSide::Right);
        assert_eq!(model.current_row, Some(difference_row));
        assert!(model.navigate_bookmark(true).is_err());
        model.navigate_bookmark(false).unwrap();
        assert_eq!(model.source_line_selection.unwrap().side, DiffSide::Left);
        assert_eq!(model.current_row, Some(difference_row));
    }

    #[test]
    fn m3c_bookmarks_follow_swaps_and_source_replacement() {
        let mut model = TextViewModel::new_memory(
            "left\nsecond".into(),
            "right\nsecond".into(),
            &DiffConfigV1::default(),
        )
        .unwrap();
        model.left.toggle_bookmark(0);
        model.right.toggle_bookmark(1);
        model.swap_sides();
        assert!(model.left.is_bookmarked(1));
        assert!(model.right.is_bookmarked(0));

        let preserved_right = model.right.source().to_owned();
        model
            .replace_memory_sources("replacement left".into(), preserved_right)
            .unwrap();
        assert!(model.left.bookmarks().is_empty());
        assert!(model.right.is_bookmarked(0));
        assert!(
            model
                .annotation_notice
                .as_deref()
                .is_some_and(|notice| notice.contains("bookmarks"))
        );

        let directory = tempfile::tempdir().unwrap();
        let left_path = directory.path().join("left.txt");
        let right_path = directory.path().join("right.txt");
        let replacement_path = directory.path().join("replacement.txt");
        fs::write(&left_path, "left\nline").unwrap();
        fs::write(&right_path, "right\nline").unwrap();
        fs::write(&replacement_path, "reloaded\ntext").unwrap();
        let mut file_model = TextViewModel::load(
            &TextCompareState {
                left: Some(left_path),
                right: Some(right_path),
                relative_path: None,
                origin: TextCompareOrigin::FileBacked,
            },
            &DiffConfigV1::default(),
        )
        .unwrap();
        file_model.left.toggle_bookmark(0);
        file_model.right.toggle_bookmark(1);
        let reloaded = crate::diff::text_file::load_text_file(replacement_path).unwrap();
        file_model
            .reload_external(DiffSide::Left, &reloaded)
            .unwrap();
        assert!(file_model.left.bookmarks().is_empty());
        assert!(file_model.right.is_bookmarked(1));
        assert!(
            file_model
                .annotation_notice
                .as_deref()
                .is_some_and(|notice| notice.contains("bookmarks"))
        );
    }

    #[test]
    fn m3c_projection_changes_are_presentation_only_and_refresh_limited_find() {
        let mut model = TextViewModel::new_memory(
            "same\nleft change".into(),
            "same\nright change".into(),
            &DiffConfigV1::default(),
        )
        .unwrap();
        wait_for_text_comparison(&mut model);
        model.find_query = "same".into();
        model.find_projection_only = true;
        model.refresh_find();
        assert!(!model.find_matches.is_empty());
        let row_ids: Vec<_> = model
            .comparison
            .as_ref()
            .unwrap()
            .rows
            .iter()
            .map(|row| row.id)
            .collect();
        let hunk_ids: Vec<_> = model
            .comparison
            .as_ref()
            .unwrap()
            .hunks
            .iter()
            .map(|hunk| hunk.id)
            .collect();
        let rules_revision = model.rules.revision;
        let annotation_revision = model.annotation_revision;

        model.set_row_projection(RowProjectionMode::DifferencesOnly, 0);
        assert!(model.find_matches.is_empty());
        model.set_row_projection(RowProjectionMode::IdenticalOnly, 0);
        assert!(!model.find_matches.is_empty());
        model.set_row_projection(RowProjectionMode::All, usize::MAX);
        assert_eq!(
            model.projection_context,
            text_compare::MAX_ROW_PROJECTION_CONTEXT
        );
        assert_eq!(model.rules.revision, rules_revision);
        assert_eq!(model.annotation_revision, annotation_revision);
        assert!(!model.recalculating);
        assert_eq!(
            model
                .comparison
                .as_ref()
                .unwrap()
                .rows
                .iter()
                .map(|row| row.id)
                .collect::<Vec<_>>(),
            row_ids
        );
        assert_eq!(
            model
                .comparison
                .as_ref()
                .unwrap()
                .hunks
                .iter()
                .map(|hunk| hunk.id)
                .collect::<Vec<_>>(),
            hunk_ids
        );

        model.find_projection_only = false;
        model.set_row_projection(RowProjectionMode::DifferencesOnly, 0);
        model.refresh_find();
        model.navigate_find(true);
        assert_eq!(model.projection_mode, RowProjectionMode::All);
        assert!(model.projection_notice.is_some());
    }

    #[test]
    fn m3c_scroll_reveal_requires_exact_measurements_and_preserves_newer_requests() {
        let mut model = TextViewModel::new_memory(
            "same\nleft change".into(),
            "same\nright change".into(),
            &DiffConfigV1::default(),
        )
        .unwrap();
        wait_for_text_comparison(&mut model);
        let comparison = model.comparison.as_ref().unwrap();
        let target = comparison
            .rows
            .iter()
            .position(|row| row.kind == text_compare::DiffRowKind::Equal)
            .unwrap();
        let row_count = comparison.rows.len();
        model.set_row_projection(RowProjectionMode::DifferencesOnly, 0);
        assert!(!model.projected_rows().contains(&target));
        model.scroll.left_vertical = 321.0;
        model.request_scroll_row(Some(target));
        assert_eq!(model.projection_mode, RowProjectionMode::All);
        assert_eq!(model.pending_scroll_row, Some(target));
        assert_eq!(model.scroll.left_vertical, 321.0);

        let key = measure_key(false, 100.0, 100.0);
        let projected = model.projected_rows();
        model.row_measurements.rebuild(
            key.clone(),
            projected.clone(),
            row_count,
            vec![20.0; projected.len()],
        );
        let first = model.pending_scroll_request().unwrap();
        assert!(model.accept_pending_scroll_request(first, &key));
        assert_eq!(model.pending_scroll_row, None);

        model.request_scroll_row(Some(target));
        let captured = model.pending_scroll_request().unwrap();
        model.request_scroll_row(Some(target));
        let newer = model.pending_scroll_request().unwrap();
        assert_ne!(captured, newer, "even the same-row re-request is distinct");
        assert!(!model.accept_pending_scroll_request(captured, &key));
        assert_eq!(model.pending_scroll_request(), Some(newer));
        assert!(model.accept_pending_scroll_request(newer, &key));

        model.request_scroll_row(Some(row_count + 1));
        assert_eq!(model.pending_scroll_row, None);
        assert!(
            model
                .projection_notice
                .as_deref()
                .is_some_and(|notice| notice.contains("no longer exists"))
        );
    }

    #[test]
    fn ignored_lines_preserve_gap_during_final_hunk_copy_and_undo() {
        let left = "keep\nnoise\nleft";
        let right = "keep\nother\nright";
        let mut model =
            TextViewModel::new_memory(left.into(), right.into(), &Default::default()).unwrap();
        wait_for_text_comparison(&mut model);
        for side in [DiffSide::Left, DiffSide::Right] {
            model.select_source_line(side, 1, false);
            model.ignore_selected_lines().unwrap();
        }
        wait_for_text_comparison(&mut model);
        let final_row = model
            .comparison
            .as_ref()
            .unwrap()
            .rows
            .iter()
            .position(|row| row.left == Some(2) && row.right == Some(2))
            .unwrap();
        model.current_row = Some(final_row);
        model.copy_hunk(DiffSide::Left).unwrap();
        assert_eq!(model.right.source(), "keep\nother\nleft");
        assert_eq!(model.left.source(), left);
        assert_eq!(
            model.right.ignored_line_ranges(),
            &[crate::diff::text_compare::IgnoredLineRange { start: 1, end: 2 }]
        );

        let mixed_left = "keep\r\nnoise\nleft\r";
        let mixed_right = "keep\r\nother\nright\r";
        let mut mixed =
            TextViewModel::new_memory(mixed_left.into(), mixed_right.into(), &Default::default())
                .unwrap();
        wait_for_text_comparison(&mut mixed);
        for side in [DiffSide::Left, DiffSide::Right] {
            mixed.select_source_line(side, 1, false);
            mixed.ignore_selected_lines().unwrap();
        }
        wait_for_text_comparison(&mut mixed);
        let final_row = mixed
            .comparison
            .as_ref()
            .unwrap()
            .rows
            .iter()
            .position(|row| row.left == Some(2) && row.right == Some(2))
            .unwrap();
        mixed.current_row = Some(final_row);
        mixed.copy_hunk(DiffSide::Left).unwrap();
        assert_eq!(mixed.right.source(), "keep\r\nother\nleft\r");
        assert_eq!(mixed.left.source(), mixed_left);
        assert_eq!(
            mixed.right.ignored_line_ranges(),
            &[crate::diff::text_compare::IgnoredLineRange { start: 1, end: 2 }]
        );
        model.active_side = DiffSide::Right;
        assert!(model.undo());
        assert_eq!(model.right.source(), right);
        assert_eq!(
            model.right.ignored_line_ranges(),
            &[crate::diff::text_compare::IgnoredLineRange { start: 1, end: 2 }]
        );
    }

    #[test]
    fn ignored_lines_follow_sources_on_swap_and_preserve_unchanged_side_selection() {
        let mut model = TextViewModel::new_memory(
            "left\nnoise\nend".into(),
            "right\nnoise\nend".into(),
            &Default::default(),
        )
        .unwrap();
        wait_for_text_comparison(&mut model);
        model.select_source_line(DiffSide::Left, 1, false);
        model.ignore_selected_lines().unwrap();
        model
            .set_manual_alignments(vec![ManualTextAlignment {
                left_line: 0,
                right_line: 0,
            }])
            .unwrap();
        model.swap_sides();
        assert_eq!(model.right.ignored_line_ranges().len(), 1);
        assert_eq!(model.source_line_selection.unwrap().side, DiffSide::Right);
        assert_eq!(model.manual_alignments[0].left_line, 0);
        assert_eq!(model.manual_alignments[0].right_line, 0);

        wait_for_text_comparison(&mut model);
        model.select_source_line(DiffSide::Right, 1, false);
        let preserved_id = model.right.source_id();
        let preserved_revision = model.right.revision;
        let right_source = model.right.source().to_owned();
        model
            .replace_memory_sources("replacement\nleft".into(), right_source)
            .unwrap();
        assert_eq!(model.source_line_selection.unwrap().side, DiffSide::Right);
        assert_eq!(model.right.source_id(), preserved_id);
        assert_eq!(model.right.revision, preserved_revision);
        assert_eq!(model.right.ignored_line_ranges().len(), 1);
        assert!(model.manual_alignments.is_empty());
        wait_for_text_comparison(&mut model);

        model.select_source_line(DiffSide::Left, 0, false);
        model.ignore_selected_lines().unwrap();
        let right_ranges = model.right.ignored_line_ranges().to_vec();
        let preserved_source = model.right.source().to_owned();
        model
            .replace_memory_sources("fully replaced left".into(), preserved_source)
            .unwrap();
        assert_eq!(model.right.source_id(), preserved_id);
        assert_eq!(model.right.revision, preserved_revision);
        assert!(model.source_line_selection.is_none());
        assert!(model.left.ignored_line_ranges().is_empty());
        assert_eq!(model.right.ignored_line_ranges(), right_ranges);
    }

    #[test]
    fn ignored_lines_reject_manual_anchors_and_stale_completions() {
        let mut model =
            TextViewModel::new_memory("a\nb\nc".into(), "a\nb\nc".into(), &Default::default())
                .unwrap();
        let initial_revision = model.annotation_revision;
        model.select_source_line(DiffSide::Left, 1, false);
        model.ignore_selected_lines().unwrap();
        assert!(model.annotation_revision > initial_revision);
        assert!(
            model
                .set_manual_alignments(vec![ManualTextAlignment {
                    left_line: 1,
                    right_line: 1,
                }])
                .unwrap_err()
                .contains("cannot be ignored")
        );
        assert!(
            model
                .add_manual_alignment(ManualTextAlignment {
                    left_line: 0,
                    right_line: 0,
                })
                .is_ok()
        );
        model.select_source_line(DiffSide::Left, 0, false);
        model.ignore_selected_lines().unwrap();
        assert!(model.manual_alignments.is_empty());
        assert!(
            model
                .alignment_error
                .as_deref()
                .is_some_and(|message| message.contains("cannot remain"))
        );

        wait_for_text_comparison(&mut model);
        assert!(model.comparison_is_fresh());
        assert_eq!(
            model.comparison.as_ref().unwrap().annotation_revision,
            model.annotation_revision
        );
        let previous = model.comparison.as_ref().unwrap().clone();
        let current_right = model.right.source().to_owned();
        model
            .replace_memory_sources("replacement\na\nc".into(), current_right)
            .unwrap();
        assert!(!model.comparison_is_fresh());
        assert!(previous.is_stale(
            model.left.source_id(),
            model.right.source_id(),
            model.left.revision,
            model.right.revision,
            model.rules.revision,
            model.annotation_revision,
        ));
        wait_for_text_comparison(&mut model);
        assert!(model.comparison_is_fresh());
        assert_eq!(
            model.comparison.as_ref().unwrap().left_source_id,
            model.left.source_id()
        );
    }

    #[test]
    fn memory_text_hunk_copy_at_eof_preserves_empty_and_terminal_newline_policy() {
        for (left, right, expected) in [
            ("a\nb", "a", "a\nb"),
            ("b", "", "b"),
            ("a\nb\n", "a\n", "a\nb\n"),
        ] {
            let mut model =
                TextViewModel::new_memory(left.into(), right.into(), &Default::default()).unwrap();
            wait_for_text_comparison(&mut model);
            let row = model
                .comparison
                .as_ref()
                .unwrap()
                .rows
                .iter()
                .position(|row| {
                    matches!(
                        row.kind,
                        crate::diff::text_compare::DiffRowKind::Modified
                            | crate::diff::text_compare::DiffRowKind::Deleted
                            | crate::diff::text_compare::DiffRowKind::Inserted
                    )
                })
                .unwrap();
            model.current_row = Some(row);
            model.copy_hunk(DiffSide::Left).unwrap();
            assert_eq!(model.right.source(), expected);
        }
    }

    #[test]
    fn memory_text_reload_is_inert_and_save_as_keeps_memory_origin() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("saved text.txt");
        let mut model = TextViewModel::new_memory(
            "left\r\n".into(),
            "right\r\n\r\n".into(),
            &Default::default(),
        )
        .unwrap();
        model.left.replace_source("edited\r\n".into()).unwrap();
        let before = (model.left.source().to_owned(), model.left.revision);
        assert!(model.reload_files().is_err());
        assert_eq!(model.left.source(), before.0);
        assert_eq!(model.left.revision, before.1);
        assert!(model.save_as_memory(DiffSide::Left, &path));
        assert_eq!(fs::read(&path).unwrap(), b"edited\r\n");
        assert_eq!(model.origin, TextCompareOrigin::Memory);
        assert_eq!(model.left_path, None);
        assert!(!model.saved_filesystem_mutation);
        assert!(model.left_error.is_none());
        assert!(!model.has_dirty());

        model.left.replace_source("still edited".into()).unwrap();
        let saved_revision = model.left.saved_revision;
        let parent_file = directory.path().join("not a directory");
        fs::write(&parent_file, b"block").unwrap();
        assert!(!model.save_as_memory(DiffSide::Left, &parent_file.join("output.txt")));
        assert_eq!(model.left.saved_revision, saved_revision);
        assert!(model.left.is_dirty());
        assert_eq!(model.left.source(), "still edited");
    }

    #[test]
    fn memory_text_workspace_push_and_back_retain_previous_view_identity() {
        let mut workspace = DiffWorkspace::default();
        workspace.current_view = RetainedView {
            id: 761,
            view: DiffView::TextCompare(TextCompareState {
                left: None,
                right: Some("right".into()),
                relative_path: Some("child.txt".into()),
                origin: TextCompareOrigin::FileBacked,
            }),
        };
        let memory_id = workspace.push_memory_text_compare();
        assert_eq!(workspace.current_view.id, memory_id);
        assert!(matches!(
            &workspace.current_view.view,
            DiffView::TextCompare(state)
                if state.origin == TextCompareOrigin::Memory
                    && state.left.is_none()
                    && state.right.is_none()
        ));
        assert!(workspace.back());
        assert_eq!(workspace.current_view.id, 761);
        assert!(matches!(
            &workspace.current_view.view,
            DiffView::TextCompare(state)
                if state.origin == TextCompareOrigin::FileBacked
                    && state.left.is_none()
                    && state.right.as_deref() == Some(Path::new("right"))
                    && state.relative_path.as_deref() == Some(Path::new("child.txt"))
        ));
    }

    #[test]
    fn memory_text_missing_folder_child_side_remains_file_backed() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("left.txt");
        fs::write(&path, "left").unwrap();
        let state = TextCompareState {
            left: Some(path),
            right: None,
            relative_path: Some("child.txt".into()),
            origin: TextCompareOrigin::FileBacked,
        };
        let mut model = TextViewModel::load(&state, &Default::default()).unwrap();
        assert_eq!(model.origin, TextCompareOrigin::FileBacked);
        assert_eq!(model.left.source(), "left");
        assert_eq!(model.right.source(), "");
        assert!(model.reload_files().is_ok());
        assert_eq!(model.origin, TextCompareOrigin::FileBacked);
    }
    #[test]
    fn swap_sides_round_trips_text_binary_and_folder() {
        for view in [
            DiffView::TextCompare(TextCompareState {
                left: Some("l".into()),
                right: Some("r".into()),
                relative_path: None,
                origin: TextCompareOrigin::FileBacked,
            }),
            DiffView::BinaryCompare(BinaryCompareState {
                left: Some("l".into()),
                right: Some("r".into()),
                relative_path: None,
            }),
            DiffView::FolderCompare(Box::new(FolderCompareState {
                left_root: "l".into(),
                right_root: "r".into(),
                ..Default::default()
            })),
        ] {
            let mut w = DiffWorkspace::default();
            w.left_visible = "left".into();
            w.right_visible = "right".into();
            w.current_view.view = view.clone();
            w.swap_sides().unwrap();
            w.swap_sides().unwrap();
            assert_eq!(w.current_view.view, view);
            assert_eq!(w.left_visible, "left");
            assert_eq!(w.right_visible, "right");
        }
    }
    #[test]
    fn validates_and_retains_inputs() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("f");
        fs::write(&f, "x").unwrap();
        let mut w = DiffWorkspace::default();
        assert!(
            w.open_paths(f.display().to_string(), d.path().display().to_string())
                .is_err()
        );
        assert_eq!(w.left_visible, f.display().to_string());
    }
    #[test]
    fn picker_assignment_is_side_specific_exact_and_cancellation_is_noop() {
        let mut w = DiffWorkspace::default();
        w.left_visible = "left old".into();
        w.right_visible = "right old".into();
        w.left_normalized = Some("normalized-left".into());
        let view_id = w.current_view.id;
        w.assign_selected_path(DiffSide::Left, Some(PathBuf::from("folder/a file.txt")));
        assert_eq!(
            w.left_visible,
            PathBuf::from("folder/a file.txt").to_string_lossy()
        );
        assert_eq!(w.right_visible, "right old");
        w.assign_selected_path(DiffSide::Right, Some(PathBuf::from(r"\\server\share\a b")));
        assert_eq!(w.right_visible, r"\\server\share\a b");
        let snapshot = (
            w.left_visible.clone(),
            w.right_visible.clone(),
            w.left_normalized.clone(),
            w.current_view.id,
        );
        w.assign_selected_path(DiffSide::Left, None);
        assert_eq!(
            snapshot,
            (w.left_visible, w.right_visible, w.left_normalized, view_id)
        );
    }
    #[test]
    fn validates_file_file_folder_folder_and_rejects_mixed() {
        let d = tempfile::tempdir().unwrap();
        let dirs = [d.path().join("a"), d.path().join("b")];
        fs::create_dir_all(&dirs[0]).unwrap();
        fs::create_dir_all(&dirs[1]).unwrap();
        let files = [d.path().join("a.txt"), d.path().join("b.txt")];
        fs::write(&files[0], "a").unwrap();
        fs::write(&files[1], "b").unwrap();
        let mut w = DiffWorkspace::default();
        assert!(
            w.open_paths(
                files[0].to_string_lossy().into(),
                files[1].to_string_lossy().into()
            )
            .is_ok()
        );
        assert!(
            w.open_paths(
                dirs[0].to_string_lossy().into(),
                dirs[1].to_string_lossy().into()
            )
            .is_ok()
        );
        assert!(
            w.open_paths(
                files[0].to_string_lossy().into(),
                dirs[0].to_string_lossy().into()
            )
            .is_err()
        );
    }
    #[test]
    fn persisted_geometry_round_trips_and_corruption_falls_back() {
        let mut p = crate::diff::persistence::DiffPersistenceV1::default();
        p.window_size = Some([1000.0, 700.0]);
        p.window_position = Some([20.0, 30.0]);
        assert_eq!(
            validated_window_geometry(&p, [0.0, 0.0, 1920.0, 1080.0]),
            ([1000.0, 700.0], Some([20.0, 30.0]))
        );
        p.window_size = Some([f32::NAN, 1.0]);
        p.window_position = Some([5000.0, 5000.0]);
        assert_eq!(
            validated_window_geometry(&p, [0.0, 0.0, 1920.0, 1080.0]),
            ([900.0, 650.0], Some([1020.0, 430.0]))
        );
    }
    #[test]
    fn geometry_accepts_nominal_minimum_and_rejects_invalid_dimensions() {
        let screen = [0.0, 0.0, 1920.0, 1080.0];
        for size in [DIFF_MIN_SIZE, [800.0, 250.0], [1000.0, 700.0]] {
            let mut p = crate::diff::persistence::DiffPersistenceV1::default();
            p.window_size = Some(size);
            p.window_position = Some([30.0, 40.0]);
            assert_eq!(
                validated_window_geometry(&p, screen),
                (size, Some([30.0, 40.0]))
            );
        }
        for invalid in [
            [f32::NAN, 250.0],
            [f32::INFINITY, 250.0],
            [0.0, 250.0],
            [-1.0, 250.0],
            [800.0, 72.0],
            [800.0, 249.0],
            [399.0, 250.0],
        ] {
            let mut p = crate::diff::persistence::DiffPersistenceV1::default();
            p.window_size = Some(invalid);
            assert_eq!(validated_window_geometry(&p, screen).0, DIFF_DEFAULT_SIZE);
        }
    }

    #[test]
    fn geometry_rejects_non_finite_positions_and_clamps_oversized_sizes() {
        let screen = [10.0, 20.0, 1210.0, 820.0];
        let mut persistence = crate::diff::persistence::DiffPersistenceV1::default();
        persistence.window_size = Some([2000.0, 1000.0]);
        for position in [[f32::NAN, 30.0], [30.0, f32::INFINITY]] {
            persistence.window_position = Some(position);
            assert_eq!(
                validated_window_geometry(&persistence, screen),
                ([1200.0, 800.0], None)
            );
        }
    }
    #[test]
    fn geometry_corrects_offscreen_and_clamps_to_small_monitor() {
        let mut p = crate::diff::persistence::DiffPersistenceV1::default();
        p.window_size = Some([900.0, 650.0]);
        p.window_position = Some([5000.0, -5000.0]);
        assert_eq!(
            validated_window_geometry(&p, [10.0, 20.0, 310.0, 220.0]),
            ([300.0, 200.0], Some([10.0, 20.0]))
        );
        p.window_size = Some([300.0, 200.0]);
        p.window_position = Some([10.0, 20.0]);
        assert_eq!(
            validated_window_geometry(&p, [10.0, 20.0, 310.0, 220.0]),
            ([300.0, 200.0], Some([10.0, 20.0]))
        );
    }
    #[test]
    fn push_back_restores_same_folder_state() {
        let mut w = DiffWorkspace::default();
        w.current_view = RetainedView {
            id: 99,
            view: DiffView::FolderCompare(Box::new(FolderCompareState {
                left_root: "/left".into(),
                right_root: "/right".into(),
                path_filter: "rs".into(),
                ..Default::default()
            })),
        };
        w.push_file_compare("a".into(), Some("a".into()), None)
            .unwrap();
        assert!(w.back());
        assert_eq!(w.current_view.id, 99);
        assert!(matches!(&w.current_view.view,DiffView::FolderCompare(s)
            if s.path_filter=="rs" && s.left_root == Path::new("/left") && s.right_root == Path::new("/right")));
    }
    #[test]
    fn paired_binary_child_routes_to_binary_compare() {
        let temp = tempfile::tempdir().unwrap();
        let left = temp.path().join("left.bin");
        let right = temp.path().join("right.bin");
        fs::write(&left, [0, 1, 2]).unwrap();
        fs::write(&right, [3, 0, 4]).unwrap();
        let mut workspace = DiffWorkspace::default();
        workspace.current_view = RetainedView {
            id: 100,
            view: DiffView::FolderCompare(Box::default()),
        };
        workspace
            .push_file_compare("pair.bin".into(), Some(left.clone()), Some(right.clone()))
            .unwrap();
        assert!(matches!(
            &workspace.current_view.view,
            DiffView::BinaryCompare(state)
                if state.left.as_ref() == Some(&left) && state.right.as_ref() == Some(&right)
        ));
    }

    #[test]
    fn zip_routes_to_binary_without_content_inspection() {
        let temp = tempfile::tempdir().unwrap();
        let left = temp.path().join("left.zip");
        let right = temp.path().join("right.zip");
        fs::write(&left, "plain but archive-named").unwrap();
        fs::write(&right, "also plain").unwrap();
        let mut workspace = DiffWorkspace::default();
        workspace
            .open_paths(
                left.to_string_lossy().into(),
                right.to_string_lossy().into(),
            )
            .unwrap();
        assert!(matches!(
            workspace.current_view.view,
            DiffView::BinaryCompare(_)
        ));
    }

    #[test]
    fn complete_folder_state_is_retained_unchanged_across_child_back() {
        use crate::diff::folder_compare::{FolderEntry, FolderStatus};
        let mut folder = FolderCompareState {
            left_root: "/left".into(),
            right_root: "/right".into(),
            path_filter: "needle".into(),
            display_filter: FolderDisplayFilter::RightOnly,
            sort: FolderSortState {
                column: FolderSortColumn::LeftSize,
                descending: true,
            },
            ..Default::default()
        };
        folder.expanded_nodes.insert("dir".into());
        folder
            .selected_paths
            .extend([PathBuf::from("a"), PathBuf::from("b")]);
        folder.primary_selection = Some("b".into());
        folder.scroll_anchor = Some("b".into());
        folder.applied_scan_rules.includes = vec!["*.txt".into()];
        folder.applied_scan_rules.excludes = vec!["target/".into()];
        folder.model.entries.insert(
            "a".into(),
            FolderEntry {
                relative_path: "a".into(),
                left: None,
                right: None,
                metadata_status: FolderStatus::LeftOnly,
                effective_status: FolderStatus::LeftOnly,
                content_checked: true,
            },
        );
        let expected = folder.clone();
        let mut workspace = DiffWorkspace::default();
        workspace.current_view = RetainedView {
            id: 101,
            view: DiffView::FolderCompare(Box::new(folder)),
        };
        workspace
            .push_file_compare("child.txt".into(), Some("/left/child.txt".into()), None)
            .unwrap();
        assert_eq!(
            workspace.navigation_stack[0].view,
            DiffView::FolderCompare(Box::new(expected.clone()))
        );
        assert!(workspace.back());
        assert_eq!(
            workspace.current_view.view,
            DiffView::FolderCompare(Box::new(expected))
        );
    }

    #[test]
    fn scan_rule_drafts_are_validated_without_mutating_applied_rules_or_model() {
        use crate::diff::folder_compare::{FolderEntry, FolderStatus};
        let mut folder = FolderCompareState::default();
        folder.applied_scan_rules.excludes.push("target/".into());
        folder.model.entries.insert(
            "kept".into(),
            FolderEntry {
                relative_path: "kept".into(),
                left: None,
                right: None,
                metadata_status: FolderStatus::Identical,
                effective_status: FolderStatus::Identical,
                content_checked: false,
            },
        );
        let applied = folder.applied_scan_rules.clone();
        folder.draft_rules.include_rules = "../invalid".into();
        assert!(folder.validated_draft_scan_rules().is_err());
        assert!(!folder.rescan_required());
        assert_eq!(folder.applied_scan_rules, applied);
        assert!(folder.model.entries.contains_key("kept"));

        folder.draft_rules.include_rules = "*.tmp".into();
        folder.draft_rules.exclude_rules = "target/\n.git/".into();
        assert!(folder.rescan_required());
        assert_eq!(folder.applied_scan_rules, applied);
    }
    #[test]
    fn splitter_and_virtual_range_are_bounded() {
        assert_eq!(validated_splitter(f32::NAN), 0.5);
        assert_eq!(validated_splitter(0.02), 0.5);
        assert_eq!(validated_splitter(0.6), 0.6);
        let heights = vec![20.0; 100_000];
        let range = visible_row_range(&heights, 20_000.0, 400.0, 3);
        assert!(
            range.len() <= 27,
            "only visible rows plus overscan are laid out"
        );
        assert!(range.start > 0);
    }

    fn measure_key(wrap: bool, left_width: f32, right_width: f32) -> RowMeasureKey {
        RowMeasureKey {
            left_source_id: 0,
            right_source_id: 0,
            left_revision: 1,
            right_revision: 2,
            comparison_revision: 3,
            annotation_revision: 0,
            left_text_width_bits: left_width.to_bits(),
            right_text_width_bits: right_width.to_bits(),
            font_theme: 4,
            text_style: 5,
            display_config: 6,
            wrap,
            projection_mode: 0,
            projection_context: 3,
        }
    }

    #[test]
    fn measurement_cache_prefix_boundaries_within_rows_and_overscan() {
        let mut cache = RowMeasurementCache::default();
        cache.rebuild(
            measure_key(true, 100.0, 200.0),
            vec![2, 4, 7],
            8,
            [20.0, 30.0, 40.0],
        );
        assert_eq!(cache.offsets, [0.0, 20.0, 50.0, 90.0]);
        assert_eq!(cache.row_at_offset(0.0), (0, 0.0));
        assert_eq!(cache.row_at_offset(19.0), (0, 19.0));
        assert_eq!(cache.row_at_offset(20.0), (1, 0.0));
        assert_eq!(cache.row_at_offset(55.0), (2, 5.0));
        assert_eq!(cache.offset_for_row(2), 50.0);
        assert_eq!(cache.visible_range(50.0, 1.0, 1), 1..3);
        assert_eq!(cache.comparison_to_projected[4], Some(1));
    }

    #[test]
    fn measurement_cache_is_safe_for_empty_and_corrupt_input() {
        let mut empty = RowMeasurementCache::default();
        empty.rebuild(measure_key(true, 1.0, 1.0), vec![], 0, []);
        assert_eq!(empty.offsets, [0.0]);
        assert_eq!(empty.visible_range(100.0, 20.0, 4), 0..0);
        assert_eq!(empty.row_at_offset(f64::NAN), (0, 0.0));

        let mut corrupt = RowMeasurementCache::default();
        corrupt.rebuild(
            measure_key(true, 1.0, 1.0),
            vec![0, 1, 2, 3],
            4,
            [f32::NAN, -3.0, f32::INFINITY, 25.0],
        );
        assert_eq!(corrupt.heights, [20.0, 20.0, 20.0, 25.0]);
        assert!(corrupt.offsets.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn measurement_keys_invalidate_only_layout_inputs() {
        let base = measure_key(false, 300.0, 700.0);
        assert_ne!(base, measure_key(true, 300.0, 700.0));
        assert_ne!(base, measure_key(false, 301.0, 700.0));
        assert_ne!(base, measure_key(false, 300.0, 701.0));
        // Paint-only state has deliberately no representation in RowMeasureKey.
        let hover_or_selection_changed = base.clone();
        assert_eq!(base, hover_or_selection_changed);
    }

    #[test]
    fn very_large_cache_queries_and_navigation_remain_exact() {
        let count = 1_000_000;
        let mut cache = RowMeasurementCache::default();
        cache.rebuild(
            measure_key(true, 80.0, 500.0),
            (0..count).collect(),
            count,
            std::iter::repeat_n(20.0, count),
        );
        assert_eq!(cache.offset_for_row(900_000), 18_000_000.0);
        assert_eq!(cache.row_at_offset(18_000_007.0), (900_000, 7.0));
        assert!(cache.visible_range(18_000_000.0, 400.0, 4).len() <= 29);

        cache.set_height(0, 100.0);
        let mut scroll = TextScrollState::default();
        scroll.target_measured_row(2, &cache);
        assert_eq!(scroll.left_vertical, 120.0);
        scroll.drive_measured(DiffSide::Left, 0.0, 105.0, &cache);
        assert_eq!((scroll.aligned_row, scroll.within_row), (1, 5.0));
    }

    #[test]
    fn wrap_toggle_changes_geometry_key_and_preserves_horizontal_offsets() {
        let mut scroll = TextScrollState::default();
        scroll.set_sync(true, false);
        scroll.drive(DiffSide::Left, 123.0, 0.0, 20.0, false);
        let off = measure_key(false, 400.0, 400.0);
        let on = measure_key(true, 400.0, 400.0);
        assert_ne!(off, on);
        scroll.drive(DiffSide::Left, 0.0, 45.0, 20.0, true);
        scroll.drive(DiffSide::Left, 123.0, 45.0, 20.0, false);
        assert_eq!(scroll.left_horizontal, 123.0);
        assert_eq!(off, measure_key(false, 400.0, 400.0));
    }
    #[test]
    fn placeholder_mapping_and_anchor_use_source_coordinates() {
        use crate::diff::text_compare::{AlignedDiffRow, ChangeImportance, DiffRowKind};
        let row = AlignedDiffRow {
            id: 1,
            left: Some(4),
            right: None,
            kind: DiffRowKind::Deleted,
            importance: ChangeImportance::Important,
            left_ranges: vec![],
            right_ranges: vec![],
        };
        assert_eq!(visual_to_source(&row, DiffSide::Left), Some(4));
        assert_eq!(visual_to_source(&row, DiffSide::Right), None);
        assert_eq!(
            restore_anchor(
                &[row],
                ScrollAnchor {
                    side: DiffSide::Left,
                    source_line: 4,
                    offset: 0.0
                }
            ),
            0
        );
    }

    #[test]
    fn text_scroll_defaults_and_bidirectional_drivers() {
        let mut scroll = TextScrollState::default();
        assert!(scroll.sync_vertical && scroll.sync_horizontal);
        scroll.drive(DiffSide::Left, 12.0, 45.0, 20.0, false);
        assert_eq!(scroll.active_driver, DiffSide::Left);
        assert_eq!(scroll.offsets(DiffSide::Right), (12.0, 45.0));
        assert_eq!((scroll.aligned_row, scroll.within_row), (2, 5.0));
        scroll.drive(DiffSide::Right, 31.0, 87.0, 20.0, false);
        assert_eq!(scroll.active_driver, DiffSide::Right);
        assert_eq!(scroll.offsets(DiffSide::Left), (31.0, 87.0));
    }

    #[test]
    fn aligned_blank_rows_use_comparison_coordinates() {
        let rows = [AlignedDiffRow {
            id: 7,
            left: Some(3),
            right: None,
            kind: crate::diff::text_compare::DiffRowKind::Deleted,
            importance: crate::diff::text_compare::ChangeImportance::Important,
            left_ranges: vec![],
            right_ranges: vec![],
        }];
        let mut scroll = TextScrollState::default();
        scroll.drive(DiffSide::Left, 0.0, 13.0, 20.0, false);
        assert_eq!(scroll.aligned_row, 0);
        assert_eq!(
            visual_to_source(&rows[scroll.aligned_row], DiffSide::Right),
            None
        );
        assert_eq!(scroll.right_vertical, 13.0);
    }

    #[test]
    fn disabled_sync_preserves_independent_offsets_and_reenable_uses_driver() {
        let mut scroll = TextScrollState::default();
        scroll.set_sync(false, false);
        scroll.drive(DiffSide::Left, 10.0, 20.0, 20.0, false);
        scroll.drive(DiffSide::Right, 70.0, 80.0, 20.0, false);
        assert_eq!(scroll.offsets(DiffSide::Left), (10.0, 20.0));
        assert_eq!(scroll.offsets(DiffSide::Right), (70.0, 80.0));
        scroll.set_sync(true, true);
        assert_eq!(scroll.offsets(DiffSide::Left), (70.0, 80.0));
    }

    #[test]
    fn horizontal_sync_only_applies_unwrapped_and_wrap_preserves_x() {
        let mut scroll = TextScrollState::default();
        scroll.drive(DiffSide::Left, 44.0, 0.0, 20.0, false);
        scroll.drive(DiffSide::Left, 0.0, 20.0, 34.0, true);
        assert_eq!(scroll.left_horizontal, 44.0);
        assert_eq!(scroll.right_horizontal, 44.0);
        scroll.drive(DiffSide::Right, 91.0, 20.0, 20.0, false);
        assert_eq!(scroll.left_horizontal, 91.0);
    }

    #[test]
    fn x_offsets_remain_independent_and_follow_the_selected_driver_when_resynced() {
        let mut scroll = TextScrollState::default();
        scroll.set_sync(true, false);
        scroll.drive(DiffSide::Left, 125.0, 20.0, 20.0, false);
        scroll.drive(DiffSide::Right, 875.0, 40.0, 20.0, false);
        assert_eq!(
            (scroll.left_horizontal, scroll.right_horizontal),
            (125.0, 875.0)
        );
        assert_eq!(scroll.active_driver, DiffSide::Right);

        scroll.set_sync(true, true);
        assert_eq!(
            (scroll.left_horizontal, scroll.right_horizontal),
            (875.0, 875.0)
        );
        scroll.set_sync(true, false);
        scroll.set_driver(DiffSide::Left);
        scroll.drive(DiffSide::Left, 240.0, 60.0, 20.0, false);
        scroll.set_sync(true, true);
        assert_eq!(
            (scroll.left_horizontal, scroll.right_horizontal),
            (240.0, 240.0)
        );
    }

    #[test]
    fn wrapping_hides_x_offsets_and_unwrapping_restores_both_sides() {
        let mut scroll = TextScrollState::default();
        scroll.set_sync(true, false);
        scroll.drive(DiffSide::Left, 111.0, 0.0, 20.0, false);
        scroll.drive(DiffSide::Right, 999.0, 0.0, 20.0, false);
        scroll.drive(DiffSide::Left, 0.0, 34.0, 34.0, true);
        scroll.drive(DiffSide::Right, 0.0, 68.0, 34.0, true);
        assert!(scroll.wrapped);
        assert_eq!(
            (scroll.left_horizontal, scroll.right_horizontal),
            (111.0, 999.0)
        );
        scroll.drive(DiffSide::Right, 999.0, 68.0, 20.0, false);
        assert!(!scroll.wrapped);
        assert_eq!(
            (scroll.left_horizontal, scroll.right_horizontal),
            (111.0, 999.0)
        );
    }

    #[test]
    fn all_navigation_sources_share_an_aligned_target() {
        let mut scroll = TextScrollState::default();
        for target in [3, 17, 42] {
            // Difference, Find, and overview respectively.
            scroll.target_aligned_row(target, 20.0);
            assert_eq!(scroll.aligned_row, target);
            assert_eq!(scroll.left_vertical, scroll.right_vertical);
        }
    }

    #[test]
    fn serialized_scroll_state_excludes_transient_offsets() {
        let mut scroll = TextScrollState::default();
        scroll.drive(DiffSide::Right, 123.0, 456.0, 20.0, false);
        let json = serde_json::to_value(&scroll).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "sync_vertical": true,
                "sync_horizontal": true
            })
        );
    }
}
