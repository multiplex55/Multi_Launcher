mod model;
mod scan;
mod session;
#[allow(dead_code)]
pub(crate) mod treemap;

#[cfg(test)]
pub(crate) use model::FolderSizeAccumulator;
#[cfg(test)]
pub(crate) use scan::folder_size_scan_bounds;

#[cfg(test)]
pub(crate) fn disposable_scan_fixture_root() -> tempfile::TempDir {
    let fixture_parent = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("productivity_expansion");
    std::fs::create_dir_all(&fixture_parent).expect("disposable fixture parent");
    tempfile::tempdir_in(fixture_parent)
        .expect("temporary root under the accessible repository workspace")
}

pub use model::{
    FolderSizeEntry, FolderSizeEntryKind, FolderSizeIssue, FolderSizeIssueKind, FolderSizeOmission,
    FolderSizeProgress, FolderSizeProgressEntry, FolderSizeProgressRows, FolderSizeResult,
    FolderSizeStatus, FolderSizeSummary, MAX_FOLDER_SIZE_DEPTH, MAX_FOLDER_SIZE_DIRECTORIES,
    MAX_FOLDER_SIZE_ENTRIES, MAX_FOLDER_SIZE_ISSUE_EXAMPLES, MAX_FOLDER_SIZE_PATH_BYTES,
};
pub use session::{
    FolderSizeSession, FolderSizeSessionError, FolderSizeSessionStatus, FolderSizeSessionView,
};
