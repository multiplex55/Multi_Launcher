use std::path::PathBuf;

use crate::batch_rename::ExplicitDirectoryRoot;
use crate::file_operations::{
    RenameScanGeneration, RenameScanRequest, ScanCancellation, scan_metadata_with_observer,
};

use super::model::{
    FolderSizeAccumulator, FolderSizeProgress, FolderSizeResult, MAX_FOLDER_SIZE_DEPTH,
    MAX_FOLDER_SIZE_DIRECTORIES, MAX_FOLDER_SIZE_ENTRIES, MAX_FOLDER_SIZE_PATH_BYTES,
};

const SCAN_PATH_BUDGET_BYTES: usize = MAX_FOLDER_SIZE_PATH_BYTES / 2;

pub(crate) fn folder_size_scan_bounds() -> crate::file_operations::MetadataScanBounds {
    crate::file_operations::MetadataScanBounds {
        max_entries: MAX_FOLDER_SIZE_ENTRIES,
        max_files: MAX_FOLDER_SIZE_ENTRIES,
        max_directories: MAX_FOLDER_SIZE_DIRECTORIES,
        max_depth: MAX_FOLDER_SIZE_DEPTH,
        max_path_bytes: SCAN_PATH_BUDGET_BYTES,
    }
}

pub(crate) fn scan_folder_size(
    root: ExplicitDirectoryRoot,
    recursive: bool,
    generation: u64,
    cancellation: &ScanCancellation,
    progress: &mut dyn FnMut(FolderSizeProgress),
) -> FolderSizeResult {
    scan_folder_size_with_bounds(
        root,
        recursive,
        generation,
        cancellation,
        progress,
        folder_size_scan_bounds(),
    )
}

fn scan_folder_size_with_bounds(
    root: ExplicitDirectoryRoot,
    recursive: bool,
    generation: u64,
    cancellation: &ScanCancellation,
    progress: &mut dyn FnMut(FolderSizeProgress),
    bounds: crate::file_operations::MetadataScanBounds,
) -> FolderSizeResult {
    let mut accumulator =
        FolderSizeAccumulator::new(PathBuf::from(root.as_path()), generation, progress);
    let mut report_progress = |_summary: &crate::file_operations::RenameScanSummary| {};
    let scanned = scan_metadata_with_observer(
        RenameScanRequest::new(root, recursive, RenameScanGeneration(generation)),
        bounds,
        cancellation,
        &mut report_progress,
        &mut accumulator,
    );
    accumulator.finish(scanned.status(), scanned.summary())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    use crate::folder_size::{FolderSizeEntryKind, FolderSizeStatus};
    #[cfg(windows)]
    use std::fs;

    #[cfg(windows)]
    #[test]
    fn folder_size_scan_native_counts_hard_link_names_and_empty_directories() {
        let temporary = crate::folder_size::disposable_scan_fixture_root();
        let root_path = temporary.path();
        let nested = root_path.join("nested");
        let empty = nested.join("empty");
        fs::create_dir(&nested).expect("nested fixture directory");
        fs::create_dir(&empty).expect("empty fixture directory");
        let original = root_path.join("payload.bin");
        let alias = root_path.join("payload-hard-link.bin");
        fs::write(&original, b"1234567").expect("fixture file");
        fs::hard_link(&original, &alias).expect("hard link in disposable fixture");

        let root = ExplicitDirectoryRoot::new(root_path.to_path_buf()).unwrap();
        let cancellation = ScanCancellation::new();
        let mut ignore_progress = |_progress| {};
        let result = scan_folder_size(root, true, 900, &cancellation, &mut ignore_progress);

        assert_eq!(
            result.status(),
            FolderSizeStatus::Complete,
            "summary={:?}, issues={:?}",
            result.summary(),
            result.issues()
        );
        assert_eq!(result.total_logical_bytes(), 14);
        assert_eq!(result.summary().files_seen(), 2);
        assert!(result.root_identity().is_some());
        let empty_entry = result
            .entries()
            .iter()
            .find(|entry| entry.path() == empty.as_path())
            .expect("empty directories remain part of the tree");
        assert_eq!(empty_entry.kind(), FolderSizeEntryKind::Directory);
        assert_eq!(empty_entry.subtree_logical_bytes(), 0);
        assert!(empty_entry.is_complete());
        assert!(
            result
                .entries()
                .iter()
                .any(|entry| entry.path() == original.as_path())
        );
        assert!(
            result
                .entries()
                .iter()
                .any(|entry| entry.path() == alias.as_path())
        );
    }

    #[cfg(windows)]
    #[test]
    fn folder_size_scan_native_preserves_root_subtotal_when_child_listing_exceeds_bound() {
        let temporary = crate::folder_size::disposable_scan_fixture_root();
        let root_path = temporary.path();
        let known = root_path.join("known.bin");
        let wide = root_path.join("wide");
        std::fs::write(&known, [0x41; 23]).expect("known root file");
        std::fs::create_dir(&wide).expect("wide child directory");
        for index in 0..4 {
            std::fs::write(wide.join(format!("item-{index}.bin")), [index as u8])
                .expect("wide child entry");
        }

        let root = ExplicitDirectoryRoot::new(root_path.to_path_buf()).unwrap();
        let cancellation = ScanCancellation::new();
        let mut ignore_progress = |_progress| {};
        let result = scan_folder_size_with_bounds(
            root,
            true,
            901,
            &cancellation,
            &mut ignore_progress,
            crate::file_operations::MetadataScanBounds {
                max_entries: 4,
                max_files: 10,
                max_directories: 10,
                max_depth: 64,
                max_path_bytes: SCAN_PATH_BUDGET_BYTES,
            },
        );

        assert_eq!(
            result.status(),
            FolderSizeStatus::Partial,
            "summary={:?}, issues={:?}",
            result.summary(),
            result.issues()
        );
        assert_eq!(result.total_logical_bytes(), 23);
        assert_eq!(result.summary().files_seen(), 1);
        let wide_entry = result
            .entries()
            .iter()
            .find(|entry| entry.path() == wide.as_path())
            .expect("the admitted child row survives its enumeration cap");
        assert_eq!(wide_entry.subtree_logical_bytes(), 0);
        assert!(!wide_entry.is_complete());
        assert!(!result.entries()[0].is_complete());
        assert!(result.issues().iter().any(|issue| {
            issue.path() == Some(wide.as_path())
                && issue.kind() == crate::folder_size::FolderSizeIssueKind::ResourceLimit
        }));
    }
}
