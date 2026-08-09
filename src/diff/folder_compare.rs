//! Folder comparison model. Paths used as keys are validated relative paths;
//! operation paths and metadata are deliberately retained per side.
use crate::diff::text_compare::{CompiledRules, TextComparisonRules, project};
use crate::diff::text_file::{LoadedContent, load_text_file};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntryKind {
    File,
    Directory,
    Symlink,
    Other,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EntryMetadata {
    pub kind: EntryKind,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub identity: Option<(u64, u64)>,
}
impl EntryMetadata {
    pub fn from_fs(m: &fs::Metadata, kind: EntryKind) -> Self {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            Self {
                kind,
                size: m.len(),
                modified: m.modified().ok(),
                identity: Some((m.dev(), m.ino())),
            }
        }
        #[cfg(not(unix))]
        {
            Self {
                kind,
                size: m.len(),
                modified: m.modified().ok(),
                identity: None,
            }
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntrySide {
    pub path: PathBuf,
    pub metadata: Option<EntryMetadata>,
    pub error: Option<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FolderStatus {
    Identical,
    Different,
    LeftOnly,
    RightOnly,
    LeftNewer,
    RightNewer,
    PendingContentComparison,
    Unreadable,
    Error,
}
impl FolderStatus {
    pub fn is_different(self) -> bool {
        !matches!(self, Self::Identical)
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderEntry {
    pub relative_path: PathBuf,
    pub left: Option<EntrySide>,
    pub right: Option<EntrySide>,
    /// Result based only on stat data. `Identical` here means quick/metadata identical.
    pub metadata_status: FolderStatus,
    /// Content-refined status, when checked. Never overwrites the raw status.
    pub effective_status: FolderStatus,
    pub content_checked: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathKeyPolicy {
    Platform,
    Sensitive,
    Insensitive,
}
pub fn normalized_relative(path: &Path) -> Result<PathBuf, String> {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::Normal(x) => out.push(x),
            Component::CurDir => {}
            _ => {
                return Err(format!(
                    "path must be relative and cannot traverse parents: {}",
                    path.display()
                ));
            }
        }
    }
    Ok(out)
}
pub fn path_key(path: &Path, policy: PathKeyPolicy) -> Result<String, String> {
    let p = normalized_relative(path)?;
    let s = p
        .iter()
        .map(|x| x.to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    let insensitive = matches!(policy, PathKeyPolicy::Insensitive)
        || matches!(policy, PathKeyPolicy::Platform) && cfg!(windows);
    Ok(if insensitive { s.to_lowercase() } else { s })
}
pub fn fast_status(
    l: Option<&EntrySide>,
    r: Option<&EntrySide>,
    tolerance: Duration,
) -> FolderStatus {
    let (l, r) = match (l, r) {
        (Some(l), Some(r)) => (l, r),
        (Some(_), None) => return FolderStatus::LeftOnly,
        (None, Some(_)) => return FolderStatus::RightOnly,
        (None, None) => return FolderStatus::Error,
    };
    if l.error.is_some() || r.error.is_some() {
        return FolderStatus::Unreadable;
    }
    let (lm, rm) = match (&l.metadata, &r.metadata) {
        (Some(l), Some(r)) => (l, r),
        _ => return FolderStatus::Unreadable,
    };
    if lm.kind != rm.kind || lm.size != rm.size {
        return FolderStatus::Different;
    }
    match (lm.modified, rm.modified) {
        (Some(a), Some(b)) => {
            let d = a
                .duration_since(b)
                .or_else(|_| b.duration_since(a))
                .unwrap_or_default();
            if d <= tolerance {
                if lm.kind == EntryKind::File {
                    FolderStatus::PendingContentComparison
                } else {
                    FolderStatus::Identical
                }
            } else if a > b {
                FolderStatus::LeftNewer
            } else {
                FolderStatus::RightNewer
            }
        }
        _ => {
            if lm.kind == EntryKind::File {
                FolderStatus::PendingContentComparison
            } else {
                FolderStatus::Identical
            }
        }
    }
}
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FolderModel {
    pub entries: BTreeMap<String, FolderEntry>,
    pub revision: u64,
}
impl FolderModel {
    pub fn upsert(
        &mut self,
        relative: &Path,
        side: EntrySide,
        left: bool,
        policy: PathKeyPolicy,
        tolerance: Duration,
    ) -> Result<(), String> {
        let rel = normalized_relative(relative)?;
        let key = path_key(&rel, policy)?;
        let e = self.entries.entry(key).or_insert(FolderEntry {
            relative_path: rel,
            left: None,
            right: None,
            metadata_status: FolderStatus::Error,
            effective_status: FolderStatus::Error,
            content_checked: false,
        });
        if left {
            e.left = Some(side)
        } else {
            e.right = Some(side)
        };
        e.metadata_status = fast_status(e.left.as_ref(), e.right.as_ref(), tolerance);
        e.effective_status = e.metadata_status;
        e.content_checked = false;
        self.revision += 1;
        Ok(())
    }
    pub fn visible(&self, filter: DisplayFilter, search: &str) -> Vec<&FolderEntry> {
        let q = search.replace('\\', "/").to_lowercase();
        self.entries
            .values()
            .filter(|e| {
                e.relative_path
                    .to_string_lossy()
                    .replace('\\', "/")
                    .to_lowercase()
                    .contains(&q)
                    && filter.matches(e.effective_status)
            })
            .collect()
    }
}

/// One row in the presentation tree.  This is deliberately a value projection:
/// changing view options cannot mutate, or cause work to be scheduled on, the model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderProjectionRow {
    pub path: PathBuf,
    pub depth: usize,
    pub has_children: bool,
}

/// Builds a tree from explicit parent/child links and then flattens its visible
/// portion. This avoids deriving nesting from incidental ordering of path strings.
pub fn project_folder_rows(
    model: &FolderModel,
    filter: crate::diff::model::FolderDisplayFilter,
    path_filter: &str,
    expanded: &BTreeSet<PathBuf>,
    descending: bool,
) -> Vec<FolderProjectionRow> {
    use crate::diff::model::FolderDisplayFilter as F;
    let query = path_filter.replace('\\', "/").to_lowercase();
    let mut by_path = BTreeMap::<PathBuf, &FolderEntry>::new();
    for entry in model.entries.values() {
        by_path.insert(entry.relative_path.clone(), entry);
    }
    let mut children = BTreeMap::<Option<PathBuf>, Vec<PathBuf>>::new();
    for path in by_path.keys() {
        let parent = path.parent().filter(|p| !p.as_os_str().is_empty());
        let explicit_parent = parent
            .filter(|p| by_path.contains_key(*p))
            .map(Path::to_path_buf);
        children
            .entry(explicit_parent)
            .or_default()
            .push(path.clone());
    }
    fn matches(f: F, status: FolderStatus) -> bool {
        match f {
            F::All => true,
            F::Differences => status.is_different(),
            F::Identical => status == FolderStatus::Identical,
            F::LeftOnly => status == FolderStatus::LeftOnly,
            F::RightOnly => status == FolderStatus::RightOnly,
            F::LeftNewer => status == FolderStatus::LeftNewer,
            F::RightNewer => status == FolderStatus::RightNewer,
            F::Errors => matches!(status, FolderStatus::Unreadable | FolderStatus::Error),
        }
    }
    fn walk(
        parent: Option<&Path>,
        depth: usize,
        by_path: &BTreeMap<PathBuf, &FolderEntry>,
        children: &BTreeMap<Option<PathBuf>, Vec<PathBuf>>,
        expanded: &BTreeSet<PathBuf>,
        filter: F,
        query: &str,
        descending: bool,
        out: &mut Vec<FolderProjectionRow>,
    ) {
        let key = parent.map(Path::to_path_buf);
        let mut paths = children.get(&key).cloned().unwrap_or_default();
        // Sorting siblings keeps a directory immediately before its subtree.
        paths.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
        if descending {
            paths.reverse();
        }
        for path in paths {
            let entry = by_path[&path];
            let has_children = children.contains_key(&Some(path.clone()));
            let path_matches = path
                .to_string_lossy()
                .replace('\\', "/")
                .to_lowercase()
                .contains(query);
            if path_matches && matches(filter.clone(), entry.effective_status) {
                out.push(FolderProjectionRow {
                    path: path.clone(),
                    depth,
                    has_children,
                });
            }
            if has_children && expanded.contains(&path) {
                walk(
                    Some(&path),
                    depth + 1,
                    by_path,
                    children,
                    expanded,
                    filter.clone(),
                    query,
                    descending,
                    out,
                );
            }
        }
    }
    let mut rows = Vec::new();
    walk(
        None, 0, &by_path, &children, expanded, filter, &query, descending, &mut rows,
    );
    rows
}

/// Expands every retained directory entry above `target`.
pub fn expand_ancestors(model: &FolderModel, target: &Path, expanded: &mut BTreeSet<PathBuf>) {
    let paths: BTreeSet<_> = model
        .entries
        .values()
        .map(|e| e.relative_path.as_path())
        .collect();
    let mut parent = target.parent();
    while let Some(path) = parent.filter(|p| !p.as_os_str().is_empty()) {
        if paths.contains(path) {
            expanded.insert(path.to_path_buf());
        }
        parent = path.parent();
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayFilter {
    All,
    Differences,
    Identical,
    LeftOnly,
    RightOnly,
    LeftNewer,
    RightNewer,
}
impl DisplayFilter {
    pub fn matches(self, s: FolderStatus) -> bool {
        match self {
            Self::All => true,
            Self::Differences => s.is_different(),
            Self::Identical => s == FolderStatus::Identical,
            Self::LeftOnly => s == FolderStatus::LeftOnly,
            Self::RightOnly => s == FolderStatus::RightOnly,
            Self::LeftNewer => s == FolderStatus::LeftNewer,
            Self::RightNewer => s == FolderStatus::RightNewer,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey(PathBuf, PathBuf, EntryMetadata, EntryMetadata, u64);
#[derive(Default)]
pub struct ContentCache {
    values: HashMap<CacheKey, FolderStatus>,
}
impl ContentCache {
    pub fn refine(
        &mut self,
        e: &mut FolderEntry,
        rules: &TextComparisonRules,
    ) -> Result<FolderStatus, String> {
        let (l, r) = match (&e.left, &e.right) {
            (Some(l), Some(r)) => (l, r),
            _ => return Ok(e.effective_status),
        };
        let (lm, rm) = match (&l.metadata, &r.metadata) {
            (Some(lm), Some(rm)) => (lm, rm),
            _ => return Ok(FolderStatus::Unreadable),
        };
        if lm.kind != EntryKind::File || rm.kind != EntryKind::File {
            return Ok(e.effective_status);
        }
        let k = CacheKey(
            l.path.clone(),
            r.path.clone(),
            lm.clone(),
            rm.clone(),
            rules.revision,
        );
        if let Some(v) = self.values.get(&k) {
            e.effective_status = *v;
            e.content_checked = true;
            return Ok(*v);
        }
        let a = load_text_file(&l.path).map_err(|x| x.to_string())?;
        let b = load_text_file(&r.path).map_err(|x| x.to_string())?;
        let equal = match (&a.content, &b.content) {
            (LoadedContent::Text(a), LoadedContent::Text(b)) => {
                let c = CompiledRules::compile(rules).map_err(|x| x.join("; "))?;
                project(a, &c)
                    .lines
                    .iter()
                    .map(|x| &x.significance_key)
                    .eq(project(b, &c).lines.iter().map(|x| &x.significance_key))
            }
            (LoadedContent::Binary { digest: a }, LoadedContent::Binary { digest: b }) => {
                lm.size == rm.size && a == b
            }
            _ => false,
        };
        let v = if equal {
            FolderStatus::Identical
        } else {
            FolderStatus::Different
        };
        self.values.insert(k, v);
        e.effective_status = v;
        e.content_checked = true;
        Ok(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn side(kind: EntryKind, size: u64, modified: u64) -> EntrySide {
        EntrySide {
            path: PathBuf::from("item"),
            metadata: Some(EntryMetadata {
                kind,
                size,
                modified: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(modified)),
                identity: None,
            }),
            error: None,
        }
    }

    #[test]
    fn metadata_status_matrix_and_tolerance() {
        let file = side(EntryKind::File, 10, 100);
        assert_eq!(
            fast_status(Some(&file), Some(&file), Duration::ZERO),
            FolderStatus::PendingContentComparison
        );
        assert_eq!(
            fast_status(
                Some(&file),
                Some(&side(EntryKind::File, 11, 100)),
                Duration::ZERO
            ),
            FolderStatus::Different
        );
        assert_eq!(
            fast_status(Some(&file), None, Duration::ZERO),
            FolderStatus::LeftOnly
        );
        assert_eq!(
            fast_status(None, Some(&file), Duration::ZERO),
            FolderStatus::RightOnly
        );
        assert_eq!(
            fast_status(
                Some(&side(EntryKind::File, 10, 103)),
                Some(&file),
                Duration::from_secs(2)
            ),
            FolderStatus::LeftNewer
        );
        assert_eq!(
            fast_status(
                Some(&file),
                Some(&side(EntryKind::File, 10, 103)),
                Duration::from_secs(2)
            ),
            FolderStatus::RightNewer
        );
        assert_eq!(
            fast_status(
                Some(&file),
                Some(&side(EntryKind::File, 10, 101)),
                Duration::from_secs(2)
            ),
            FolderStatus::PendingContentComparison
        );
        assert_eq!(
            fast_status(
                Some(&file),
                Some(&side(EntryKind::Directory, 10, 100)),
                Duration::ZERO
            ),
            FolderStatus::Different
        );
        let unreadable = EntrySide {
            path: "bad".into(),
            metadata: None,
            error: Some("denied".into()),
        };
        assert_eq!(
            fast_status(Some(&unreadable), Some(&file), Duration::ZERO),
            FolderStatus::Unreadable
        );
    }

    #[test]
    fn insensitive_pairing_and_large_upserts_are_lossless() {
        let mut model = FolderModel::default();
        model
            .upsert(
                Path::new("Dir/FILE.txt"),
                side(EntryKind::File, 1, 1),
                true,
                PathKeyPolicy::Insensitive,
                Duration::ZERO,
            )
            .unwrap();
        model
            .upsert(
                Path::new("dir/file.TXT"),
                side(EntryKind::File, 1, 1),
                false,
                PathKeyPolicy::Insensitive,
                Duration::ZERO,
            )
            .unwrap();
        assert_eq!(model.entries.len(), 1);
        for i in 0..10_000 {
            let path = PathBuf::from(format!("batch/{i}.txt"));
            model
                .upsert(
                    &path,
                    side(EntryKind::File, i, 1),
                    true,
                    PathKeyPolicy::Sensitive,
                    Duration::ZERO,
                )
                .unwrap();
        }
        assert_eq!(model.entries.len(), 10_001);
        assert_eq!(
            model
                .entries
                .values()
                .map(|e| &e.relative_path)
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            10_001
        );
    }
}
