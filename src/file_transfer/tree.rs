//! Bounded source-tree observations for a single transfer review.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::file_operations::ScanCancellation;

use super::{
    AdmissionError, EntryKind, EntrySnapshot, check_cancel, inspect_children, inspect_source,
};
use std::os::windows::ffi::OsStrExt;

const MAX_ENTRIES: usize = 10_000;
const MAX_METADATA_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SourceNodeId(usize);

impl SourceNodeId {
    pub(super) fn index(self) -> usize {
        self.0
    }
}

#[derive(Debug)]
pub enum SourceObservation {
    Ready(EntrySnapshot),
    Unavailable(AdmissionError),
}

#[derive(Debug)]
pub struct SourceTreeEntry {
    id: SourceNodeId,
    parent: Option<SourceNodeId>,
    path: PathBuf,
    observation: SourceObservation,
}

impl SourceTreeEntry {
    pub fn id(&self) -> SourceNodeId {
        self.id
    }

    pub fn parent(&self) -> Option<SourceNodeId> {
        self.parent
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn observation(&self) -> &SourceObservation {
        &self.observation
    }
}

#[derive(Debug)]
pub struct SourceTree {
    entries: Vec<SourceTreeEntry>,
}

impl SourceTree {
    pub fn entries(&self) -> &[SourceTreeEntry] {
        &self.entries
    }

    pub fn get(&self, id: SourceNodeId) -> Option<&SourceTreeEntry> {
        self.entries.get(id.0)
    }
}

pub(super) fn singleton_file(source: EntrySnapshot) -> Result<SourceTree, AdmissionError> {
    if source.metadata().kind != EntryKind::File {
        return Err(AdmissionError::InvalidPath);
    }
    Ok(SourceTree {
        entries: vec![SourceTreeEntry {
            id: SourceNodeId(0),
            parent: None,
            path: source.path(),
            observation: SourceObservation::Ready(source),
        }],
    })
}

#[derive(Debug)]
pub enum SourceTreeError {
    EmptySelection,
    Limit,
    Cancelled,
    AliasedOrOverlapping(PathBuf),
    Changed(PathBuf),
}

impl std::fmt::Display for SourceTreeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptySelection => f.write_str("Choose at least one transfer source."),
            Self::Limit => f.write_str("The selection exceeds the transfer review limits."),
            Self::Cancelled => f.write_str("Transfer review cancelled."),
            Self::AliasedOrOverlapping(path) => write!(
                f,
                "The selection contains duplicate, linked, or overlapping sources ({path:?})."
            ),
            Self::Changed(path) => write!(
                f,
                "The source changed during review ({path:?}). Review the selection again."
            ),
        }
    }
}

impl std::error::Error for SourceTreeError {}

struct Budget {
    entries: usize,
    bytes: usize,
}

impl Budget {
    fn reserve(
        &mut self,
        path: &Path,
        observation: &SourceObservation,
    ) -> Result<(), SourceTreeError> {
        let path_bytes = |path: &Path| path.as_os_str().encode_wide().count().saturating_mul(2);
        let evidence_bytes = match observation {
            SourceObservation::Ready(entry) => {
                let ancestors = entry.parent().ancestor_identities().count();
                path_bytes(entry.parent().path())
                    .saturating_add(
                        entry
                            .leaf()
                            .as_os_str()
                            .encode_wide()
                            .count()
                            .saturating_mul(2),
                    )
                    .saturating_add(
                        ancestors.saturating_mul(std::mem::size_of::<super::DirectoryStamp>()),
                    )
            }
            SourceObservation::Unavailable(_) => 1024,
        };
        let bytes = std::mem::size_of::<SourceTreeEntry>()
            .saturating_add(path_bytes(path))
            .saturating_add(evidence_bytes);
        self.entries = self.entries.checked_sub(1).ok_or(SourceTreeError::Limit)?;
        self.bytes = self
            .bytes
            .checked_sub(bytes)
            .ok_or(SourceTreeError::Limit)?;
        Ok(())
    }
}

fn observed(
    path: &Path,
    result: Result<EntrySnapshot, AdmissionError>,
) -> Result<SourceObservation, SourceTreeError> {
    match result {
        Ok(entry) => Ok(SourceObservation::Ready(entry)),
        Err(AdmissionError::Cancelled) => Err(SourceTreeError::Cancelled),
        Err(AdmissionError::Limit) => Err(SourceTreeError::Limit),
        Err(AdmissionError::StaleEntry | AdmissionError::StaleParent) => {
            Err(SourceTreeError::Changed(path.to_path_buf()))
        }
        Err(error) => Ok(SourceObservation::Unavailable(error)),
    }
}

struct PendingEntry {
    parent: Option<SourceNodeId>,
    path: PathBuf,
    observation: SourceObservation,
}

impl PendingEntry {
    fn from_observation(
        parent: Option<SourceNodeId>,
        path: &Path,
        observation: SourceObservation,
        budget: &mut Budget,
    ) -> Result<Self, SourceTreeError> {
        budget.reserve(path, &observation)?;
        let path = match &observation {
            SourceObservation::Ready(entry) => entry.path(),
            SourceObservation::Unavailable(_) => path.to_path_buf(),
        };
        Ok(Self {
            parent,
            path,
            observation,
        })
    }
}

/// Capture an ordered, read-only source tree. Unavailable entries remain
/// visible with their reason. Cancellation and limits invalidate the review.
pub fn inspect_source_tree(
    paths: &[PathBuf],
    cancellation: &ScanCancellation,
) -> Result<SourceTree, SourceTreeError> {
    inspect_with_budget(
        paths,
        cancellation,
        Budget {
            entries: MAX_ENTRIES,
            bytes: MAX_METADATA_BYTES,
        },
    )
}

/// Admit the reviewed roots before opening any of their descendants.
pub(crate) fn inspect_exact_source_tree(
    expected: &[EntrySnapshot],
    cancellation: &ScanCancellation,
) -> Result<SourceTree, SourceTreeError> {
    let paths: Vec<_> = expected.iter().map(EntrySnapshot::path).collect();
    inspect_expected_with_budget(
        &paths,
        Some(expected),
        cancellation,
        Budget {
            entries: MAX_ENTRIES,
            bytes: MAX_METADATA_BYTES,
        },
    )
}

fn inspect_with_budget(
    paths: &[PathBuf],
    cancellation: &ScanCancellation,
    budget: Budget,
) -> Result<SourceTree, SourceTreeError> {
    inspect_expected_with_budget(paths, None, cancellation, budget)
}

fn inspect_expected_with_budget(
    paths: &[PathBuf],
    expected: Option<&[EntrySnapshot]>,
    cancellation: &ScanCancellation,
    mut budget: Budget,
) -> Result<SourceTree, SourceTreeError> {
    if paths.is_empty() {
        return Err(SourceTreeError::EmptySelection);
    }
    if paths.len() > budget.entries {
        return Err(SourceTreeError::Limit);
    }

    let mut pending = Vec::with_capacity(paths.len());
    let mut roots = HashSet::new();
    for (index, path) in paths.iter().enumerate() {
        check_cancel(cancellation).map_err(|_| SourceTreeError::Cancelled)?;
        let entry = observed(path, inspect_source(path, cancellation))?;
        if let Some(expected) = expected
            && !matches!(&entry, SourceObservation::Ready(actual) if expected.get(index) == Some(actual))
        {
            return Err(SourceTreeError::Changed(path.clone()));
        }
        if let SourceObservation::Ready(entry) = &entry
            && !roots.insert(entry.identity())
        {
            return Err(SourceTreeError::AliasedOrOverlapping(path.clone()));
        }
        pending.push(PendingEntry::from_observation(
            None,
            path,
            entry,
            &mut budget,
        )?);
    }
    for root in &pending {
        if let SourceObservation::Ready(entry) = &root.observation
            && entry
                .parent()
                .ancestor_identities()
                .any(|id| roots.contains(&id))
        {
            return Err(SourceTreeError::AliasedOrOverlapping(root.path.clone()));
        }
    }
    pending.reverse();

    let mut entries = Vec::new();
    let mut seen = HashSet::new();
    while let Some(mut item) = pending.pop() {
        check_cancel(cancellation).map_err(|_| SourceTreeError::Cancelled)?;
        let id = SourceNodeId(entries.len());
        if let SourceObservation::Ready(source) = &item.observation {
            if !seen.insert(source.identity()) {
                return Err(SourceTreeError::AliasedOrOverlapping(item.path));
            }
            if source.metadata().kind == EntryKind::Directory {
                match inspect_children(source, budget.entries, budget.bytes, cancellation) {
                    Ok(children) => {
                        // Push in reverse so the admitted tree follows Windows
                        // ordinal name order with each parent before its children.
                        for child in children.into_iter().rev() {
                            let path = item.path.join(&child.name);
                            let observation = observed(&path, child.entry)?;
                            let child = PendingEntry::from_observation(
                                Some(id),
                                &path,
                                observation,
                                &mut budget,
                            )?;
                            pending.push(child);
                        }
                    }
                    Err(error) => {
                        item.observation = observed(&item.path, Err(error))?;
                    }
                }
            }
        }
        entries.push(SourceTreeEntry {
            id,
            parent: item.parent,
            path: item.path,
            observation: item.observation,
        });
    }
    Ok(SourceTree { entries })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downloads_triage_transfer_rejects_replaced_root_before_enumerating_children() {
        let root = tempfile::tempdir().unwrap();
        let selected = root.path().join("selected");
        std::fs::create_dir(&selected).unwrap();
        let cancellation = ScanCancellation::new();
        let expected = inspect_source(&selected, &cancellation).unwrap();
        std::fs::rename(&selected, root.path().join("old")).unwrap();
        std::fs::create_dir(&selected).unwrap();
        std::fs::write(selected.join("unrelated.txt"), b"foreign").unwrap();
        let result = inspect_expected_with_budget(
            std::slice::from_ref(&selected),
            Some(std::slice::from_ref(&expected)),
            &cancellation,
            Budget {
                entries: 1,
                bytes: MAX_METADATA_BYTES,
            },
        );
        assert!(matches!(result, Err(SourceTreeError::Changed(path)) if path == selected));
    }

    use std::fs;

    #[test]
    fn transfer_tree_orders_exact_sources_and_never_changes_them() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("picked");
        let nested = root.join("B-folder");
        fs::create_dir_all(&nested).unwrap();
        fs::write(root.join("z.txt"), b"last").unwrap();
        fs::write(root.join("A.txt"), b"first").unwrap();
        let nested_file = nested.join("inside.txt");
        fs::write(&nested_file, b"inside").unwrap();

        let tree = inspect_source_tree(&[root], &ScanCancellation::new()).unwrap();
        let entries = tree.entries();
        let names: Vec<_> = entries
            .iter()
            .map(|entry| entry.path().file_name().unwrap())
            .collect();
        assert_eq!(
            names,
            ["picked", "A.txt", "B-folder", "inside.txt", "z.txt"]
        );
        assert_eq!(entries[1].parent(), Some(entries[0].id()));
        assert_eq!(entries[3].parent(), Some(entries[2].id()));
        let SourceObservation::Ready(file) = entries[3].observation() else {
            panic!("ordinary child was unavailable");
        };
        assert_eq!(file.metadata().byte_len, Some(6));
        assert_eq!(fs::read(nested_file).unwrap(), b"inside");
    }

    #[test]
    fn transfer_tree_refuses_both_recursive_overlap_and_discovered_hard_links() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("picked");
        let nested = root.join("inside");
        fs::create_dir_all(&nested).unwrap();
        let control = ScanCancellation::new();
        assert!(matches!(
            inspect_source_tree(&[root.clone(), nested.clone()], &control),
            Err(SourceTreeError::AliasedOrOverlapping(_))
        ));

        let source = root.join("source.txt");
        fs::write(&source, b"alias evidence").unwrap();
        fs::hard_link(&source, nested.join("same-object.txt")).unwrap();
        assert!(matches!(
            inspect_source_tree(&[root], &control),
            Err(SourceTreeError::AliasedOrOverlapping(_))
        ));
    }

    #[test]
    fn transfer_tree_limits_and_cancellation_cannot_return_a_partial_review() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("picked");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("one.txt"), b"one").unwrap();
        fs::write(root.join("two.txt"), b"two").unwrap();
        let control = ScanCancellation::new();
        let roots = [root];
        assert!(matches!(
            inspect_with_budget(
                &roots,
                &control,
                Budget {
                    entries: 2,
                    bytes: MAX_METADATA_BYTES
                }
            ),
            Err(SourceTreeError::Limit)
        ));
        assert!(matches!(
            inspect_with_budget(
                &roots,
                &control,
                Budget {
                    entries: MAX_ENTRIES,
                    bytes: 1
                }
            ),
            Err(SourceTreeError::Limit)
        ));
        control.cancel();
        assert!(matches!(
            inspect_source_tree(&roots, &control),
            Err(SourceTreeError::Cancelled)
        ));
    }
}
