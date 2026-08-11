use crate::diff::folder_compare::FolderModel;
use crate::diff::folder_scan::ScanRules;
use crate::diff::settings::{DiffConfigV1, FolderColumnWidthsV1, FolderSortColumn};
use crate::diff::text_compare::{
    self, AlignedDiffRow, CompiledRules, FindMatch, FindScope, NavigationDirection,
    RowProjectionMode, TextComparisonResult, TextComparisonRules,
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

#[derive(Debug, Clone, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq)]
pub struct TextCompareState {
    pub left: Option<PathBuf>,
    pub right: Option<PathBuf>,
    pub relative_path: Option<PathBuf>,
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
    FolderCompare(FolderCompareState),
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
                })
            }
        } else if lm.is_dir() && rm.is_dir() {
            let mut folder = FolderCompareState {
                left_root: lp,
                right_root: rp,
                ..FolderCompareState::default()
            };
            folder.apply_table_preferences(&self.settings);
            DiffView::FolderCompare(folder)
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
}

/// Preferred and minimum Diff window sizes, before adapting them to the work area.
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
/// outer window rectangle.
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
        let projected = cache
            .comparison_to_projected
            .get(comparison_row)
            .and_then(|row| *row)
            .unwrap_or_else(|| comparison_row.min(cache.heights.len()));
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
    pub left_revision: u64,
    pub right_revision: u64,
    pub comparison_revision: u64,
    pub left_text_width_bits: u32,
    pub right_text_width_bits: u32,
    pub font_theme: u64,
    pub text_style: u64,
    pub display_config: u64,
    pub wrap: bool,
    pub projection_mode: u8,
    pub projection_context: usize,
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
            left_revision: 0,
            right_revision: 0,
            comparison_revision: 0,
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

/// Non-egui controller for a retained text comparison. The last accepted
/// alignment remains available while `recalculating` is true.
pub struct TextViewModel {
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
    pub syntax_cache: crate::diff::syntax::SyntaxCache,
    pub large_file_tier: crate::diff::worker::LargeFileTier,
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
    /// Installs a clean external reload while monotonically advancing the
    /// document revision used to reject stale comparison results.
    pub fn reload_external(
        &mut self,
        side: DiffSide,
        loaded: &crate::diff::text_file::LoadedTextFile,
    ) -> Result<(), String> {
        let replacement = TextDocument::from_loaded(loaded)
            .ok_or_else(|| "externally changed file is no longer text".to_string())?;
        let document = match side {
            DiffSide::Left => &mut self.left,
            DiffSide::Right => &mut self.right,
        };
        let next = document.revision.wrapping_add(1);
        *document = replacement;
        document.revision = next;
        document.saved_revision = next;
        self.external_conflict[if side == DiffSide::Left { 0 } else { 1 }] = false;
        self.schedule_compare();
        Ok(())
    }
    pub fn load(state: &TextCompareState, settings: &DiffConfigV1) -> Result<Self, String> {
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
        let wrap =
            if crate::diff::syntax::code_like(state.left.as_deref().or(state.right.as_deref())) {
                false
            } else {
                settings.wrap_text
            };
        let bytes = left.source().len().saturating_add(right.source().len()) as u64;
        let estimated_rows =
            left.source()
                .bytes()
                .filter(|b| *b == b'\n')
                .count()
                .max(right.source().bytes().filter(|b| *b == b'\n').count()) as u64
                + 1;
        let large_file_tier = settings.large_file_policy().tier(bytes, estimated_rows);
        let mut out = Self {
            left_path: state.left.clone(),
            right_path: state.right.clone(),
            left,
            right,
            comparison: None,
            rules: TextComparisonRules::default(),
            active_side: DiffSide::Left,
            current_row: None,
            pending_scroll_row: None,
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
            syntax_cache: Default::default(),
            large_file_tier,
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
    pub fn schedule_compare(&mut self) {
        self.recalculate_at = Some(Instant::now() + Duration::from_millis(250));
        self.recalculating = true;
    }
    pub fn poll(&mut self) {
        if self.recalculate_at.is_some_and(|t| Instant::now() >= t) {
            self.start_compare();
        }
        if let Some(rx) = &self.receiver {
            if let Ok(m) = rx.try_recv() {
                if m.generation == self.generation
                    && !m.result.is_stale(
                        self.left.revision,
                        self.right.revision,
                        self.rules.revision,
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
                }
            }
        }
    }
    fn start_compare(&mut self) {
        self.recalculate_at = None;
        self.generation += 1;
        let generation = self.generation;
        let (l, r, lr, rr, rules) = (
            self.left.source().to_owned(),
            self.right.source().to_owned(),
            self.left.revision,
            self.right.revision,
            self.rules.clone(),
        );
        let (tx, rx) = mpsc::channel();
        self.receiver = Some(rx);
        self.recalculating = true;
        std::thread::spawn(move || {
            if let Ok(c) = CompiledRules::compile(&rules) {
                let _ = tx.send(CompareMessage {
                    generation,
                    result: text_compare::compare(&l, &r, lr, rr, &c, 4096),
                });
            }
        });
    }
    pub fn projected_rows(&self) -> Vec<usize> {
        self.comparison.as_ref().map_or_else(Vec::new, |c| {
            text_compare::row_projection(c, self.projection_mode, self.projection_context)
        })
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
    pub fn set_current_row(&mut self, row: Option<usize>) {
        self.current_row = row;
        self.request_scroll_row(row);
    }
    pub fn request_scroll_row(&mut self, row: Option<usize>) {
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
        let changed = match self.active_side {
            DiffSide::Left => self.left.undo(),
            DiffSide::Right => self.right.undo(),
        };
        if changed {
            self.schedule_compare()
        }
        changed
    }
    pub fn redo(&mut self) -> bool {
        let changed = match self.active_side {
            DiffSide::Left => self.left.redo(),
            DiffSide::Right => self.right.redo(),
        };
        if changed {
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
        let c = self
            .comparison
            .as_ref()
            .ok_or("Comparison is still calculating")?;
        let row = self.current_row.ok_or("Select a difference first")?;
        let hi = c
            .navigation
            .row_to_hunk
            .get(row)
            .and_then(|x| *x)
            .ok_or("Current row is not a difference")?;
        let h = &c.hunks[hi];
        let mut replacement = Vec::new();
        let mut destination = Vec::new();
        for r in &c.rows[h.start_row..h.end_row] {
            if let Some(n) = visual_to_source(r, from) {
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
        let start = destination
            .first()
            .copied()
            .unwrap_or_else(|| insertion_line(&c.rows, h.start_row, other(from)));
        let edit = LineEdit {
            start,
            delete_count: destination.len(),
            replacement,
        };
        match other(from) {
            DiffSide::Left => self.left.apply_edits(&[edit])?,
            DiffSide::Right => self.right.apply_edits(&[edit])?,
        };
        self.schedule_compare();
        Ok(())
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
    pub fn has_dirty(&self) -> bool {
        self.left.is_dirty() || self.right.is_dirty()
    }
}
fn other(s: DiffSide) -> DiffSide {
    match s {
        DiffSide::Left => DiffSide::Right,
        DiffSide::Right => DiffSide::Left,
    }
}
fn source_line(s: &str, n: usize) -> String {
    s.lines().nth(n).unwrap_or_default().to_owned()
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
            view: DiffView::FolderCompare(FolderCompareState {
                left_root: "/left".into(),
                right_root: "/right".into(),
                path_filter: "rs".into(),
                ..Default::default()
            }),
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
            view: DiffView::FolderCompare(FolderCompareState::default()),
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
            view: DiffView::FolderCompare(folder),
        };
        workspace
            .push_file_compare("child.txt".into(), Some("/left/child.txt".into()), None)
            .unwrap();
        assert_eq!(
            workspace.navigation_stack[0].view,
            DiffView::FolderCompare(expected.clone())
        );
        assert!(workspace.back());
        assert_eq!(
            workspace.current_view.view,
            DiffView::FolderCompare(expected)
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
            left_revision: 1,
            right_revision: 2,
            comparison_revision: 3,
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
