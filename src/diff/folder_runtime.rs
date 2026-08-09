//! Ephemeral resources used while a retained folder comparison is alive.
//!
//! This type deliberately lives outside `FolderCompareState`: retained views
//! may be cloned or persisted, while receivers and operation handles may not.

use crate::diff::file_ops::OperationHandle;
use crate::diff::folder_compare::{ContentCache, FolderStatus};
use crate::diff::folder_scan::{RootIdentity, ScanHandle};
use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);

#[derive(Default)]
pub struct FolderRuntime {
    pub left_scan: Option<ScanHandle>,
    pub right_scan: Option<ScanHandle>,
    pub generation: u64,
    pub left_root: Option<RootIdentity>,
    pub right_root: Option<RootIdentity>,
    pub left_visited: u64,
    pub right_visited: u64,
    pub comparison_queue: VecDeque<PathBuf>,
    pub comparison_results: BTreeMap<PathBuf, FolderStatus>,
    pub content_cache: ContentCache,
    pub active_operation: Option<OperationHandle>,
    pub left_error: Option<String>,
    pub right_error: Option<String>,
    // Filesystem watchers belong here when live refresh is implemented.
}

impl FolderRuntime {
    pub fn next_generation() -> u64 {
        NEXT_GENERATION.fetch_add(1, Ordering::Relaxed)
    }

    pub fn is_active(&self) -> bool {
        self.left_scan.is_some() || self.right_scan.is_some() || !self.comparison_queue.is_empty()
    }
    pub fn cancel(&self) {
        if let Some(scan) = &self.left_scan {
            scan.cancel();
        }
        if let Some(scan) = &self.right_scan {
            scan.cancel();
        }
        if let Some(operation) = &self.active_operation {
            operation.cancel();
        }
    }
}

impl Drop for FolderRuntime {
    fn drop(&mut self) {
        self.cancel();
    }
}
