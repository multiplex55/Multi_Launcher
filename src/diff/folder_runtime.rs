//! Ephemeral resources used while a retained folder comparison is alive.
//!
//! This type deliberately lives outside `FolderCompareState`: retained views
//! may be cloned or persisted, while receivers and operation handles may not.

use crate::diff::file_ops::OperationHandle;
use crate::diff::folder_scan::{RootIdentity, ScanHandle};
use crate::diff::model::DiffStatus;
use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;

#[derive(Default)]
pub struct FolderRuntime {
    pub left_scan: Option<ScanHandle>,
    pub right_scan: Option<ScanHandle>,
    pub generation: u64,
    pub left_root: Option<RootIdentity>,
    pub right_root: Option<RootIdentity>,
    pub discovered: u64,
    pub compared: u64,
    pub comparison_queue: VecDeque<PathBuf>,
    pub comparison_cache: BTreeMap<PathBuf, DiffStatus>,
    pub active_operation: Option<OperationHandle>,
    // Filesystem watchers belong here when live refresh is implemented.
}

impl FolderRuntime {
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
