use crate::diff::settings::DiffConfigV1;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
fn id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffStatus {
    Identical,
    Modified,
    LeftOnly,
    RightOnly,
    Error,
}
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum FolderDisplayFilter {
    #[default]
    All,
    Changed,
    Identical,
    OneSided,
}
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FolderSortState {
    pub column: String,
    pub descending: bool,
}
#[derive(Debug, Clone, PartialEq)]
pub struct FolderCompareState {
    pub selected_relative_path: Option<PathBuf>,
    pub expanded_nodes: BTreeSet<PathBuf>,
    pub scroll_anchor: Option<PathBuf>,
    pub display_filter: FolderDisplayFilter,
    pub path_filter: String,
    pub sort: FolderSortState,
    pub content_statuses: BTreeMap<PathBuf, DiffStatus>,
}
impl Default for FolderCompareState {
    fn default() -> Self {
        Self {
            selected_relative_path: None,
            expanded_nodes: BTreeSet::new(),
            scroll_anchor: None,
            display_filter: FolderDisplayFilter::All,
            path_filter: String::new(),
            sort: FolderSortState {
                column: "path".into(),
                descending: false,
            },
            content_statuses: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum FileComparisonKind {
    Text,
    Binary,
}
#[derive(Debug, Clone, PartialEq)]
pub struct TextCompareState {
    pub left: Option<PathBuf>,
    pub right: Option<PathBuf>,
    pub relative_path: Option<PathBuf>,
    pub kind: FileComparisonKind,
}
#[derive(Debug, Clone, PartialEq)]
pub enum DiffView {
    Start,
    TextCompare(TextCompareState),
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
            DiffView::TextCompare(TextCompareState {
                left: Some(lp.clone()),
                right: Some(rp.clone()),
                relative_path: None,
                kind: detect_kind(&lp, &rp),
            })
        } else if lm.is_dir() && rm.is_dir() {
            DiffView::FolderCompare(FolderCompareState::default())
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
        let kind = match (&left, &right) {
            (Some(l), Some(r)) => detect_kind(l, r),
            _ => FileComparisonKind::Text,
        };
        let next = RetainedView {
            id: id(),
            view: DiffView::TextCompare(TextCompareState {
                left,
                right,
                relative_path: Some(relative_path),
                kind,
            }),
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
fn metadata(path: &Path, side: &str) -> Result<fs::Metadata, String> {
    fs::metadata(path).map_err(|e| format!("{side} path '{}': {e}", path.display()))
}
fn detect_kind(left: &Path, right: &Path) -> FileComparisonKind {
    let binary = |p: &Path| {
        fs::read(p)
            .ok()
            .is_some_and(|b| b.iter().take(8192).any(|v| *v == 0))
    };
    if binary(left) || binary(right) {
        FileComparisonKind::Binary
    } else {
        FileComparisonKind::Text
    }
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
    fn push_back_restores_same_folder_state() {
        let mut w = DiffWorkspace::default();
        w.current_view = RetainedView {
            id: 99,
            view: DiffView::FolderCompare(FolderCompareState {
                path_filter: "rs".into(),
                ..Default::default()
            }),
        };
        w.push_file_compare("a".into(), Some("a".into()), None)
            .unwrap();
        assert!(w.back());
        assert_eq!(w.current_view.id, 99);
        assert!(matches!(&w.current_view.view,DiffView::FolderCompare(s) if s.path_filter=="rs"));
    }
}
