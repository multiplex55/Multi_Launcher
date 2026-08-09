use crate::diff::folder_compare::FolderModel;
use crate::diff::folder_scan::ScanRules;
use crate::diff::settings::DiffConfigV1;
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
}
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FolderSortState {
    pub column: String,
    pub descending: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContentComparisonMode {
    Metadata,
    #[default]
    OnDemand,
    Always,
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
    /// Rules that produced `model`; drafts never mutate these implicitly.
    pub applied_scan_rules: ScanRules,
    pub draft_include_rules: String,
    pub draft_exclude_rules: String,
    /// Rules used by asynchronous content refinement of text file pairs.
    pub text_rules: TextComparisonRules,
    pub content_comparison: ContentComparisonMode,
    pub timestamp_tolerance: Duration,
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
                column: "path".into(),
                descending: false,
            },
            applied_scan_rules: ScanRules::default(),
            draft_include_rules: String::new(),
            draft_exclude_rules: String::new(),
            text_rules: TextComparisonRules::default(),
            content_comparison: ContentComparisonMode::default(),
            timestamp_tolerance: Duration::from_secs(2),
            left_scan_complete: false,
            right_scan_complete: false,
            stale_paths: BTreeSet::new(),
        }
    }
}

impl FolderCompareState {
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
            Self::draft_lines(&self.draft_include_rules),
            Self::draft_lines(&self.draft_exclude_rules),
        )
    }

    pub fn rescan_required(&self) -> bool {
        self.validated_draft_scan_rules()
            .is_ok_and(|rules| rules != self.applied_scan_rules)
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
            DiffView::FolderCompare(FolderCompareState {
                left_root: lp,
                right_root: rp,
                ..FolderCompareState::default()
            })
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

/// Returns safe initial window geometry. A position is accepted when at least
/// part of the window intersects the available screen.
pub fn validated_window_geometry(
    persistence: &crate::diff::persistence::DiffPersistenceV1,
    screen: [f32; 4],
) -> ([f32; 2], Option<[f32; 2]>) {
    let size = persistence
        .window_size
        .filter(|s| s.iter().all(|v| v.is_finite()) && s[0] >= 600.0 && s[1] >= 350.0)
        .unwrap_or([900.0, 650.0]);
    let position = persistence.window_position.filter(|p| {
        p.iter().all(|v| v.is_finite())
            && p[0] + size[0] > screen[0]
            && p[1] + size[1] > screen[1]
            && p[0] < screen[2]
            && p[1] < screen[3]
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
    pub pane_width_bits: u32,
    pub font_theme: u64,
    pub wrap: bool,
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
    if heights.is_empty() {
        return 0..0;
    }
    let mut y = 0.0;
    let mut first = 0;
    while first < heights.len() && y + heights[first] <= scroll_y {
        y += heights[first];
        first += 1;
    }
    let mut end = first;
    let limit = scroll_y + viewport;
    while end < heights.len() && y < limit {
        y += heights[end];
        end += 1;
    }
    first.saturating_sub(overscan)..(end + overscan).min(heights.len())
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
            self.pending_scroll_row = Some(m.row);
        }
    }
    pub fn request_overview_scroll(&mut self, y: f32, height: f32) {
        if let Some(c) = &self.comparison {
            self.pending_scroll_row = text_compare::overview_row(y, height, c.rows.len());
        }
    }
    pub fn navigate(&mut self, direction: NavigationDirection) {
        if let Some(c) = &self.comparison {
            let row = c.navigate(self.current_row, direction, false, true);
            self.current_row = row;
            self.pending_scroll_row = row;
        }
    }
    pub fn set_current_row(&mut self, row: Option<usize>) {
        self.current_row = row;
        self.pending_scroll_row = row;
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
            ([900.0, 650.0], None)
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
                column: "size".into(),
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
        folder.draft_include_rules = "../invalid".into();
        assert!(folder.validated_draft_scan_rules().is_err());
        assert!(!folder.rescan_required());
        assert_eq!(folder.applied_scan_rules, applied);
        assert!(folder.model.entries.contains_key("kept"));

        folder.draft_include_rules = "*.tmp".into();
        folder.draft_exclude_rules = "target/\n.git/".into();
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
}
