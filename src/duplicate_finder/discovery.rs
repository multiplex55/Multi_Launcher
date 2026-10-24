use super::model::{
    DiscoveryDiagnostic, DiscoveryProgress, DiscoverySnapshot, DiscoveryStage, DiscoveryStatus,
    FileAlias, IdentityUnverifiedPath, KnownFile, KnownFileIdentity, RootScan, RootScanStatus,
    ScanCounts, SizeGroup,
};
use crate::batch_rename::{self, ExplicitDirectoryRoot, RootError, SnapshotEntryKind};
use crate::file_operations::{
    self, DuplicateDiscoveryRootIdentity, DuplicateDiscoveryScanLimits, RenameScanGeneration,
    RenameScanIssue, RenameScanStatus, ScanCancellation,
};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

pub(super) const MAX_SELECTED_ROOTS: usize = 32;
pub(super) const MAX_FILES: usize = 250_000;
pub(super) const MAX_ENTRIES: usize = 350_000;
pub(super) const MAX_DIRECTORIES: usize = 100_000;
pub(super) const MAX_DIRECTORY_DEPTH: usize = 256;
pub(super) const MAX_PATH_STORAGE_BYTES: usize = 64 * 1024 * 1024;
pub(super) const MAX_ROOT_SELECTION_PATH_BYTES: usize = 2 * 1024 * 1024;
pub(super) const MAX_DIAGNOSTICS: usize = 64;
pub(super) const MAX_DIAGNOSTIC_MESSAGE_CHARS: usize = 512;

const MAX_WINDOWS_PATH_UTF16_BYTES: usize = 32_766 * 2;
const RESERVED_DIAGNOSTIC_PATH_BYTES: usize = MAX_DIAGNOSTICS * MAX_WINDOWS_PATH_UTF16_BYTES;

#[derive(Clone, Copy, Debug)]
pub(super) struct DiscoveryLimits {
    pub(super) max_roots: usize,
    pub(super) max_files: usize,
    pub(super) max_entries: usize,
    pub(super) max_directories: usize,
    pub(super) max_depth: usize,
    pub(super) max_path_bytes: usize,
    pub(super) max_diagnostics: usize,
}

impl Default for DiscoveryLimits {
    fn default() -> Self {
        Self {
            max_roots: MAX_SELECTED_ROOTS,
            max_files: MAX_FILES,
            max_entries: MAX_ENTRIES,
            max_directories: MAX_DIRECTORIES,
            max_depth: MAX_DIRECTORY_DEPTH,
            max_path_bytes: MAX_PATH_STORAGE_BYTES,
            max_diagnostics: MAX_DIAGNOSTICS,
        }
    }
}

pub(super) trait DiscoveryJob: Send + Sync + 'static {
    fn scan(
        &self,
        selected_roots: Vec<PathBuf>,
        generation: u64,
        limits: DiscoveryLimits,
        cancellation: &ScanCancellation,
        progress: &mut dyn FnMut(DiscoveryProgress),
    ) -> DiscoverySnapshot;
}

pub(super) struct NativeDiscoveryJob;

#[derive(Clone)]
struct SelectedRoot {
    input_index: usize,
    display_path: PathBuf,
    normalized_path: PathBuf,
    invalid_reason: Option<RootError>,
}

struct AdmittedRoot {
    input_index: usize,
    display_path: PathBuf,
    canonical_path: PathBuf,
    identity: KnownFileIdentity,
    root_identity: DuplicateDiscoveryRootIdentity,
}

#[derive(Default)]
struct SizeGroupBuilder {
    known_files: BTreeMap<KnownFileIdentity, KnownFile>,
    unverified_paths: Vec<IdentityUnverifiedPath>,
}

#[derive(Default)]
struct DiscoveryAccumulator {
    groups: BTreeMap<u64, SizeGroupBuilder>,
    identity_sizes: HashMap<KnownFileIdentity, u64>,
    invalid_identities: HashSet<KnownFileIdentity>,
}

impl DiscoveryAccumulator {
    fn observe_file(
        &mut self,
        root_index: usize,
        path: PathBuf,
        identity: crate::file_operations::NativeFileIdentity,
        size_bytes: u64,
        modified: Option<SystemTime>,
    ) -> Option<String> {
        if identity.file_id() == [0; 16] {
            self.groups
                .entry(size_bytes)
                .or_default()
                .unverified_paths
                .push(IdentityUnverifiedPath {
                    root_index,
                    size_bytes,
                    observed_identity: identity,
                    modified,
                    path,
                });
            return None;
        }

        let identity = KnownFileIdentity {
            volume_serial_number: identity.volume_serial_number(),
            file_id: identity.file_id(),
        };
        if self.invalid_identities.contains(&identity) {
            return None;
        }

        if let Some(previous_size) = self.identity_sizes.get(&identity).copied() {
            let Some(existing) = self
                .groups
                .get_mut(&previous_size)
                .and_then(|group| group.known_files.get_mut(&identity))
            else {
                self.invalid_identities.insert(identity);
                self.identity_sizes.remove(&identity);
                return Some("a previously observed file identity could not be reconciled".into());
            };
            if previous_size != size_bytes || existing.modified != modified {
                if let Some(group) = self.groups.get_mut(&previous_size) {
                    group.known_files.remove(&identity);
                }
                self.identity_sizes.remove(&identity);
                self.invalid_identities.insert(identity);
                return Some(
                    "one native file identity changed size or modification time during discovery; it was excluded"
                        .into(),
                );
            }
            if !existing.aliases.iter().any(|alias| {
                batch_rename::compare_paths_windows_ordinal(&alias.path, &path)
                    == std::cmp::Ordering::Equal
            }) {
                existing.aliases.push(FileAlias { root_index, path });
            }
            return None;
        }

        self.identity_sizes.insert(identity, size_bytes);
        self.groups
            .entry(size_bytes)
            .or_default()
            .known_files
            .insert(
                identity,
                KnownFile {
                    identity,
                    size_bytes,
                    modified,
                    aliases: vec![FileAlias { root_index, path }],
                },
            );
        None
    }

    fn into_size_groups(self) -> Vec<SizeGroup> {
        self.groups
            .into_iter()
            .map(|(size_bytes, group)| SizeGroup {
                size_bytes,
                known_files: group.known_files.into_values().collect(),
                unverified_paths: group.unverified_paths,
            })
            .collect()
    }
}

impl DiscoveryJob for NativeDiscoveryJob {
    fn scan(
        &self,
        selected_roots: Vec<PathBuf>,
        generation: u64,
        limits: DiscoveryLimits,
        cancellation: &ScanCancellation,
        progress: &mut dyn FnMut(DiscoveryProgress),
    ) -> DiscoverySnapshot {
        let roots_total = selected_roots.len();
        let selected_path_bytes = selected_roots
            .iter()
            .fold(0usize, |total, path| total.saturating_add(path_bytes(path)));
        let selection_is_bounded = roots_total <= limits.max_roots
            && selected_path_bytes <= MAX_ROOT_SELECTION_PATH_BYTES
            && selected_path_bytes <= limits.max_path_bytes;
        let mut snapshot = if selection_is_bounded {
            DiscoverySnapshot::scanning(generation, selected_roots.clone())
        } else {
            let mut snapshot = DiscoverySnapshot::idle(generation);
            snapshot.status = DiscoveryStatus::Failed;
            push_diagnostic(
                &mut snapshot,
                limits.max_diagnostics,
                DiscoveryDiagnostic {
                    root_index: None,
                    path: None,
                    message: "selected roots exceed the bounded root count or path-list size"
                        .into(),
                },
            );
            return snapshot;
        };
        let mut accumulator = DiscoveryAccumulator::default();
        let mut has_partial_root = false;
        let mut counts = ScanCounts::default();

        let mut selected = match normalize_selected_roots(selected_roots, limits) {
            Ok(selected) => selected,
            Err(message) => {
                push_diagnostic(
                    &mut snapshot,
                    limits.max_diagnostics,
                    DiscoveryDiagnostic {
                        root_index: None,
                        path: None,
                        message,
                    },
                );
                snapshot.status = DiscoveryStatus::Failed;
                return snapshot;
            }
        };
        selected.sort_by(|left, right| {
            component_count(&left.normalized_path)
                .cmp(&component_count(&right.normalized_path))
                .then_with(|| {
                    batch_rename::compare_paths_windows_ordinal(
                        &left.normalized_path,
                        &right.normalized_path,
                    )
                })
                .then_with(|| left.input_index.cmp(&right.input_index))
        });

        let mut lexical_roots: Vec<SelectedRoot> = Vec::new();
        for root in selected {
            if cancellation.is_cancelled() {
                snapshot.status = DiscoveryStatus::Cancelled;
                return snapshot;
            }
            if let Some(error) = root.invalid_reason {
                has_partial_root = true;
                mark_root_status(&mut snapshot, root.input_index, map_root_error(error));
                push_diagnostic(
                    &mut snapshot,
                    limits.max_diagnostics,
                    DiscoveryDiagnostic {
                        root_index: Some(root.input_index),
                        path: Some(root.display_path),
                        message: format!("selected root is not representable: {error:?}"),
                    },
                );
                continue;
            }
            if let Some(parent) = lexical_roots.iter().find(|parent| {
                batch_rename::path_is_within_windows_ordinal(
                    &parent.normalized_path,
                    &root.normalized_path,
                )
            }) {
                let status = if batch_rename::compare_paths_windows_ordinal(
                    &parent.normalized_path,
                    &root.normalized_path,
                ) == std::cmp::Ordering::Equal
                {
                    RootScanStatus::SkippedEquivalent
                } else {
                    RootScanStatus::SkippedOverlap
                };
                mark_skipped_root(&mut snapshot, root.input_index, status);
                push_diagnostic(
                    &mut snapshot,
                    limits.max_diagnostics,
                    DiscoveryDiagnostic {
                        root_index: Some(root.input_index),
                        path: Some(root.display_path),
                        message: if status == RootScanStatus::SkippedEquivalent {
                            "this selected root is equivalent to an earlier root and will not be scanned twice".into()
                        } else {
                            format!(
                                "this selected root is inside an earlier selected root ({}) and will not be scanned twice",
                                parent.display_path.display()
                            )
                        },
                    },
                );
            } else {
                lexical_roots.push(root);
            }
        }

        let mut admitted = Vec::new();
        let mut roots_preflighted = 0usize;
        for root in lexical_roots {
            if cancellation.is_cancelled() {
                snapshot.status = DiscoveryStatus::Cancelled;
                return snapshot;
            }
            let explicit_root = match ExplicitDirectoryRoot::new(root.normalized_path.clone()) {
                Ok(root_path) => root_path,
                Err(error) => {
                    has_partial_root = true;
                    mark_root_status(&mut snapshot, root.input_index, map_root_error(error));
                    push_diagnostic(
                        &mut snapshot,
                        limits.max_diagnostics,
                        DiscoveryDiagnostic {
                            root_index: Some(root.input_index),
                            path: Some(root.display_path),
                            message: format!("selected root is not representable: {error:?}"),
                        },
                    );
                    continue;
                }
            };
            match file_operations::inspect_duplicate_discovery_root(
                explicit_root,
                RenameScanGeneration(generation),
            ) {
                Ok(root_identity) => {
                    let identity = known_identity(root_identity.identity);
                    if let Some(identity) = identity {
                        admitted.push(AdmittedRoot {
                            input_index: root.input_index,
                            display_path: root.display_path,
                            canonical_path: root_identity.canonical_root.clone(),
                            identity,
                            root_identity,
                        });
                    } else {
                        has_partial_root = true;
                        mark_root_status(
                            &mut snapshot,
                            root.input_index,
                            RootScanStatus::UnsupportedIdentity,
                        );
                        push_diagnostic(
                            &mut snapshot,
                            limits.max_diagnostics,
                            DiscoveryDiagnostic {
                                root_index: Some(root.input_index),
                                path: Some(root.display_path),
                                message: "selected root has an unsupported all-zero file identity and was not scanned".into(),
                            },
                        );
                    }
                }
                Err((status, summary)) => {
                    has_partial_root = true;
                    mark_root_status(&mut snapshot, root.input_index, map_scan_status(status));
                    append_issues(
                        &mut snapshot,
                        limits.max_diagnostics,
                        root.input_index,
                        summary.issues(),
                        summary.issues_truncated(),
                    );
                }
            }
            roots_preflighted = roots_preflighted.saturating_add(1);
            publish_progress(
                progress,
                generation,
                roots_total,
                roots_preflighted,
                0,
                None,
                counts,
            );
        }

        admitted.sort_by(|left, right| {
            component_count(&left.canonical_path)
                .cmp(&component_count(&right.canonical_path))
                .then_with(|| {
                    batch_rename::compare_paths_windows_ordinal(
                        &left.canonical_path,
                        &right.canonical_path,
                    )
                })
                .then_with(|| left.input_index.cmp(&right.input_index))
        });

        let mut distinct_roots: Vec<AdmittedRoot> = Vec::new();
        for root in admitted {
            if let Some(parent) = distinct_roots.iter().find(|parent| {
                parent.identity == root.identity
                    || batch_rename::path_is_within_windows_ordinal(
                        &parent.canonical_path,
                        &root.canonical_path,
                    )
            }) {
                let equivalent = parent.identity == root.identity
                    || batch_rename::compare_paths_windows_ordinal(
                        &parent.canonical_path,
                        &root.canonical_path,
                    ) == std::cmp::Ordering::Equal;
                let status = if equivalent {
                    RootScanStatus::SkippedEquivalent
                } else {
                    RootScanStatus::SkippedOverlap
                };
                mark_skipped_root(&mut snapshot, root.input_index, status);
                push_diagnostic(
                    &mut snapshot,
                    limits.max_diagnostics,
                    DiscoveryDiagnostic {
                        root_index: Some(root.input_index),
                        path: Some(root.display_path),
                        message: if equivalent {
                            "this selected root resolves to an already admitted directory and will not be scanned twice".into()
                        } else {
                            format!(
                                "this selected root resolves inside an already admitted root ({}) and will not be scanned twice",
                                parent.display_path.display()
                            )
                        },
                    },
                );
            } else {
                distinct_roots.push(root);
            }
        }

        // The display paths live in the published root rows while the worker
        // also retains the normalized and display spellings in its root list.
        // Charge all three copies before entering the traversal.
        // Keep a conservative reserve for the worker's normalized/display
        // root list. Canonical spellings are counted separately below because
        // they can be longer than the user supplied spelling on Windows.
        let selected_work_path_bytes = selected_path_bytes.saturating_mul(4);
        let canonical_work_path_bytes = distinct_roots.iter().fold(0usize, |total, root| {
            total
                .saturating_add(path_bytes(&root.canonical_path))
                .saturating_add(path_bytes(&root.root_identity.canonical_root))
        });
        let mut retained_path_bytes = selected_work_path_bytes
            .saturating_add(root_record_path_bytes(&snapshot.roots))
            .saturating_add(canonical_work_path_bytes);
        let reserved_diagnostic_bytes =
            RESERVED_DIAGNOSTIC_PATH_BYTES.min(limits.max_path_bytes / 4);
        let root_count = distinct_roots.len();
        let mut roots_completed = 0usize;
        let mut directories_admitted = 0usize;
        for (root_position, root) in distinct_roots.into_iter().enumerate() {
            if cancellation.is_cancelled() {
                snapshot.status = DiscoveryStatus::Cancelled;
                return snapshot;
            }
            let remaining_entries = limits.max_entries.saturating_sub(counts.entries_seen);
            let remaining_files = limits.max_files.saturating_sub(counts.files_seen);
            let roots_reserved_for_later = root_count.saturating_sub(root_position + 1);
            let remaining_directories =
                file_operations::remaining_duplicate_discovery_directory_budget(
                    limits.max_directories,
                    directories_admitted,
                    roots_reserved_for_later,
                );
            let remaining_path_bytes = limits
                .max_path_bytes
                .saturating_sub(retained_path_bytes)
                .saturating_sub(reserved_diagnostic_bytes);
            if remaining_entries == 0 || remaining_files == 0 || remaining_directories == 0 {
                has_partial_root = true;
                mark_root_status(
                    &mut snapshot,
                    root.input_index,
                    RootScanStatus::ResourceLimit,
                );
                push_diagnostic(
                    &mut snapshot,
                    limits.max_diagnostics,
                    DiscoveryDiagnostic {
                        root_index: Some(root.input_index),
                        path: Some(root.display_path),
                        message: "the aggregate discovery limit was reached before this root could be scanned".into(),
                    },
                );
                break;
            }
            if remaining_path_bytes == 0 {
                has_partial_root = true;
                mark_root_status(
                    &mut snapshot,
                    root.input_index,
                    RootScanStatus::ResourceLimit,
                );
                push_diagnostic(
                    &mut snapshot,
                    limits.max_diagnostics,
                    DiscoveryDiagnostic {
                        root_index: Some(root.input_index),
                        path: Some(root.display_path),
                        message: "the aggregate path-storage limit was reached before this root could be scanned".into(),
                    },
                );
                break;
            }

            mark_root_status(&mut snapshot, root.input_index, RootScanStatus::Scanning);
            let display_path = root.display_path.clone();
            let root_identity = root.root_identity.clone();
            let base_counts = counts;
            let roots_preflighted_for_progress = roots_preflighted;
            let roots_scanned_total = distinct_root_count(&snapshot.roots);
            let mut root_progress = |summary: &crate::file_operations::RenameScanSummary| {
                let current_counts = counts_from_summary(summary);
                publish_progress(
                    progress,
                    generation,
                    roots_total,
                    roots_preflighted_for_progress.min(roots_scanned_total),
                    roots_completed,
                    Some(display_path.clone()),
                    add_counts(base_counts, current_counts),
                );
            };
            let explicit_canonical_root = match ExplicitDirectoryRoot::new(
                root.canonical_path.clone(),
            ) {
                Ok(root) => root,
                Err(error) => {
                    has_partial_root = true;
                    mark_root_status(&mut snapshot, root.input_index, RootScanStatus::InvalidRoot);
                    push_diagnostic(
                        &mut snapshot,
                        limits.max_diagnostics,
                        DiscoveryDiagnostic {
                            root_index: Some(root.input_index),
                            path: Some(root.display_path),
                            message: format!(
                                "the admitted canonical root could not be represented: {error:?}"
                            ),
                        },
                    );
                    continue;
                }
            };
            let mut result = file_operations::scan_duplicate_discovery_root_with_progress(
                explicit_canonical_root,
                root_identity,
                RenameScanGeneration(generation),
                DuplicateDiscoveryScanLimits {
                    max_entries: remaining_entries,
                    max_files: remaining_files,
                    max_directories: remaining_directories,
                    max_depth: limits.max_depth,
                    max_path_bytes: remaining_path_bytes,
                },
                cancellation,
                &mut root_progress,
            );
            let root_counts = counts_from_summary(&result.summary);
            counts = add_counts(counts, root_counts);
            directories_admitted =
                directories_admitted.saturating_add(result.summary.directories_admitted());
            roots_completed += usize::from(result.status != RenameScanStatus::Cancelled);
            let root_status = map_scan_status(result.status);
            if !matches!(root_status, RootScanStatus::Complete) {
                has_partial_root = true;
            }
            let root_index = root.input_index;
            snapshot.roots[root_index] = RootScan {
                selected_path: root.display_path,
                canonical_root: result.canonical_root.take(),
                identity: result.root_identity.and_then(known_identity),
                status: root_status,
                counts: root_counts,
            };
            append_issues(
                &mut snapshot,
                limits.max_diagnostics,
                root_index,
                result.summary.issues(),
                result.summary.issues_truncated(),
            );
            if result.summary.issue_count() > result.summary.issues().len() {
                snapshot.diagnostics_truncated = true;
            }
            for row in result.rows.drain(..) {
                if row.kind != SnapshotEntryKind::File {
                    continue;
                }
                let Some(size_bytes) = row.length else {
                    has_partial_root = true;
                    push_diagnostic(
                        &mut snapshot,
                        limits.max_diagnostics,
                        DiscoveryDiagnostic {
                            root_index: Some(root_index),
                            path: Some(row.path),
                            message: "file size was unavailable, so this entry was not grouped"
                                .into(),
                        },
                    );
                    continue;
                };
                if row.cloud_placeholder {
                    continue;
                }
                let diagnostic_path = row.path.clone();
                if let Some(message) = accumulator.observe_file(
                    root_index,
                    row.path,
                    row.identity,
                    size_bytes,
                    row.modified,
                ) {
                    has_partial_root = true;
                    push_diagnostic(
                        &mut snapshot,
                        limits.max_diagnostics,
                        DiscoveryDiagnostic {
                            root_index: Some(root_index),
                            path: Some(diagnostic_path),
                            message,
                        },
                    );
                }
            }
            // The row paths in the accumulator are the retained result. The
            // adapter has already charged its canonical root and in-flight
            // proof/queue paths against this root's remaining path budget.
            retained_path_bytes = selected_work_path_bytes
                .saturating_add(root_record_path_bytes(&snapshot.roots))
                .saturating_add(canonical_work_path_bytes)
                .saturating_add(accumulator_path_bytes(&accumulator));
            if result.status == RenameScanStatus::ResourceLimit {
                break;
            }
            if result.status == RenameScanStatus::Cancelled || cancellation.is_cancelled() {
                snapshot.status = DiscoveryStatus::Cancelled;
                return snapshot;
            }
            publish_progress(
                progress,
                generation,
                roots_total,
                distinct_root_count(&snapshot.roots),
                roots_completed,
                None,
                counts,
            );
        }

        snapshot.counts = counts;
        snapshot.size_groups = accumulator.into_size_groups();
        snapshot.status = if cancellation.is_cancelled() {
            DiscoveryStatus::Cancelled
        } else if has_partial_root {
            DiscoveryStatus::Partial
        } else {
            DiscoveryStatus::Complete
        };
        snapshot.progress = Some(DiscoveryProgress {
            generation,
            stage: DiscoveryStage::Traversing,
            roots_total: snapshot.roots.len(),
            roots_preflighted: snapshot
                .roots
                .iter()
                .filter(|root| root.canonical_root.is_some())
                .count(),
            roots_completed,
            current_root: None,
            counts,
            content: None,
        });
        if cancellation.is_cancelled() {
            snapshot.status = DiscoveryStatus::Cancelled;
            return snapshot;
        }
        if snapshot.status != DiscoveryStatus::Cancelled {
            super::content::verify_discovered_content(
                &mut snapshot,
                limits,
                cancellation,
                progress,
            );
        }
        snapshot
    }
}

fn normalize_selected_roots(
    roots: Vec<PathBuf>,
    limits: DiscoveryLimits,
) -> Result<Vec<SelectedRoot>, String> {
    if roots.is_empty() {
        return Err("select at least one directory root before scanning".into());
    }
    if roots.len() > limits.max_roots {
        return Err(format!(
            "select no more than {} roots per scan",
            limits.max_roots
        ));
    }
    let total_path_bytes = roots
        .iter()
        .fold(0usize, |total, path| total.saturating_add(path_bytes(path)));
    if total_path_bytes > MAX_ROOT_SELECTION_PATH_BYTES {
        return Err("selected root paths exceed the bounded path-list size".into());
    }
    roots
        .into_iter()
        .enumerate()
        .map(|(input_index, display_path)| {
            let explicit = ExplicitDirectoryRoot::new(display_path.clone());
            let invalid_reason = explicit.as_ref().err().copied();
            let normalized_path = explicit
                .as_ref()
                .map(|explicit| normalize_path(explicit.as_path()))
                .unwrap_or_else(|_| display_path.clone());
            Ok(SelectedRoot {
                input_index,
                display_path,
                normalized_path,
                invalid_reason,
            })
        })
        .collect()
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
            Component::ParentDir => unreachable!("explicit roots reject parent traversal"),
        }
    }
    normalized
}

fn component_count(path: &Path) -> usize {
    path.components()
        .filter(|component| !matches!(component, Component::CurDir | Component::RootDir))
        .count()
}

fn path_bytes(path: &Path) -> usize {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        path.as_os_str().encode_wide().count().saturating_mul(2)
    }
    #[cfg(not(windows))]
    {
        path.as_os_str().as_encoded_bytes().len()
    }
}

fn known_identity(
    identity: crate::file_operations::NativeFileIdentity,
) -> Option<KnownFileIdentity> {
    let file_id = identity.file_id();
    (file_id != [0; 16]).then_some(KnownFileIdentity {
        volume_serial_number: identity.volume_serial_number(),
        file_id,
    })
}

fn map_root_error(error: batch_rename::RootError) -> RootScanStatus {
    match error {
        batch_rename::RootError::Empty
        | batch_rename::RootError::MustBeAbsolute
        | batch_rename::RootError::ParentTraversal
        | batch_rename::RootError::PathTooLong => RootScanStatus::InvalidRoot,
    }
}

fn map_scan_status(status: RenameScanStatus) -> RootScanStatus {
    match status {
        RenameScanStatus::Complete => RootScanStatus::Complete,
        RenameScanStatus::Partial => RootScanStatus::Partial,
        RenameScanStatus::InvalidRoot => RootScanStatus::InvalidRoot,
        RenameScanStatus::Inaccessible => RootScanStatus::Inaccessible,
        RenameScanStatus::Unsupported => RootScanStatus::UnsupportedIdentity,
        RenameScanStatus::Cancelled => RootScanStatus::Cancelled,
        RenameScanStatus::ResourceLimit => RootScanStatus::ResourceLimit,
        RenameScanStatus::ChangedDuringScan => RootScanStatus::ChangedDuringScan,
    }
}

fn counts_from_summary(summary: &crate::file_operations::RenameScanSummary) -> ScanCounts {
    ScanCounts {
        directories_scanned: summary.directories_scanned(),
        entries_seen: summary.entries_seen(),
        files_seen: summary.files_seen(),
        directories_seen: summary.directories_seen(),
        other_entries_seen: summary.other_entries_seen(),
        reparse_entries_excluded: summary.reparse_entries_excluded(),
        cloud_placeholders_excluded: summary.cloud_placeholders_excluded(),
        inaccessible_entries: summary.inaccessible_entries(),
    }
}

fn add_counts(left: ScanCounts, right: ScanCounts) -> ScanCounts {
    ScanCounts {
        directories_scanned: left
            .directories_scanned
            .saturating_add(right.directories_scanned),
        entries_seen: left.entries_seen.saturating_add(right.entries_seen),
        files_seen: left.files_seen.saturating_add(right.files_seen),
        directories_seen: left.directories_seen.saturating_add(right.directories_seen),
        other_entries_seen: left
            .other_entries_seen
            .saturating_add(right.other_entries_seen),
        reparse_entries_excluded: left
            .reparse_entries_excluded
            .saturating_add(right.reparse_entries_excluded),
        cloud_placeholders_excluded: left
            .cloud_placeholders_excluded
            .saturating_add(right.cloud_placeholders_excluded),
        inaccessible_entries: left
            .inaccessible_entries
            .saturating_add(right.inaccessible_entries),
    }
}

fn append_issues(
    snapshot: &mut DiscoverySnapshot,
    maximum: usize,
    root_index: usize,
    issues: &[RenameScanIssue],
    truncated: bool,
) {
    for issue in issues {
        push_diagnostic(
            snapshot,
            maximum,
            DiscoveryDiagnostic {
                root_index: Some(root_index),
                path: issue.path().map(Path::to_path_buf),
                message: truncate_message(issue.message()),
            },
        );
    }
    snapshot.diagnostics_truncated |= truncated;
}

fn push_diagnostic(
    snapshot: &mut DiscoverySnapshot,
    maximum: usize,
    mut diagnostic: DiscoveryDiagnostic,
) {
    if snapshot.diagnostics.len() >= maximum {
        snapshot.diagnostics_truncated = true;
        return;
    }
    diagnostic.message = truncate_message(&diagnostic.message);
    snapshot.diagnostics.push(diagnostic);
}

fn truncate_message(message: &str) -> String {
    message.chars().take(MAX_DIAGNOSTIC_MESSAGE_CHARS).collect()
}

fn mark_root_status(snapshot: &mut DiscoverySnapshot, root_index: usize, status: RootScanStatus) {
    if let Some(root) = snapshot.roots.get_mut(root_index) {
        root.status = status;
    }
}

fn mark_skipped_root(snapshot: &mut DiscoverySnapshot, root_index: usize, status: RootScanStatus) {
    mark_root_status(snapshot, root_index, status);
}

fn root_record_path_bytes(roots: &[RootScan]) -> usize {
    roots.iter().fold(0usize, |total, root| {
        total
            .saturating_add(path_bytes(&root.selected_path))
            .saturating_add(root.canonical_root.as_deref().map_or(0, path_bytes))
    })
}

fn accumulator_path_bytes(accumulator: &DiscoveryAccumulator) -> usize {
    accumulator.groups.values().fold(0usize, |total, group| {
        let known = group
            .known_files
            .values()
            .fold(0usize, |group_total, file| {
                group_total.saturating_add(
                    file.aliases.iter().fold(0usize, |alias_total, alias| {
                        alias_total.saturating_add(path_bytes(&alias.path))
                    }),
                )
            });
        let unknown = group
            .unverified_paths
            .iter()
            .fold(0usize, |group_total, file| {
                group_total.saturating_add(path_bytes(&file.path))
            });
        total.saturating_add(known).saturating_add(unknown)
    })
}

fn distinct_root_count(roots: &[RootScan]) -> usize {
    roots
        .iter()
        .filter(|root| {
            !matches!(
                root.status,
                RootScanStatus::SkippedOverlap | RootScanStatus::SkippedEquivalent
            )
        })
        .count()
}

fn publish_progress(
    progress: &mut dyn FnMut(DiscoveryProgress),
    generation: u64,
    roots_total: usize,
    roots_preflighted: usize,
    roots_completed: usize,
    current_root: Option<PathBuf>,
    counts: ScanCounts,
) {
    progress(DiscoveryProgress {
        generation,
        stage: DiscoveryStage::Traversing,
        roots_total,
        roots_preflighted,
        roots_completed,
        current_root,
        counts,
        content: None,
    });
}

#[cfg(test)]
mod tests {
    use super::{DiscoveryAccumulator, DiscoveryJob, DiscoveryLimits, NativeDiscoveryJob};
    use crate::file_operations::{NativeFileIdentity, ScanCancellation};
    use std::path::PathBuf;
    use std::time::SystemTime;

    #[cfg(windows)]
    use crate::duplicate_finder::{DiscoverySnapshot, DiscoveryStatus, RootScanStatus};

    fn identity(volume_serial_number: u64, file_id: [u8; 16]) -> NativeFileIdentity {
        serde_json::from_value(serde_json::json!({
            "volume_serial_number": volume_serial_number,
            "file_id": file_id,
        }))
        .expect("deserialize opaque native identity fixture")
    }

    #[test]
    fn dff_discovery_groups_known_identity_aliases_and_keeps_zero_ids_unverified() {
        let mut accumulator = DiscoveryAccumulator::default();
        let known = identity(7, [9; 16]);
        let unknown = identity(7, [0; 16]);
        let modified = Some(SystemTime::UNIX_EPOCH);

        assert_eq!(
            accumulator.observe_file(0, PathBuf::from("root/first.bin"), known, 64, modified,),
            None
        );
        assert_eq!(
            accumulator.observe_file(1, PathBuf::from("other/alias.bin"), known, 64, modified,),
            None
        );
        assert_eq!(
            accumulator.observe_file(
                0,
                PathBuf::from("root/unknown-a.bin"),
                unknown,
                64,
                modified,
            ),
            None
        );
        assert_eq!(
            accumulator.observe_file(
                0,
                PathBuf::from("root/unknown-b.bin"),
                unknown,
                64,
                modified,
            ),
            None
        );

        let groups = accumulator.into_size_groups();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].known_files.len(), 1);
        assert_eq!(groups[0].known_files[0].identity.file_id, [9; 16]);
        assert_eq!(groups[0].known_files[0].aliases.len(), 2);
        assert_eq!(groups[0].unverified_paths.len(), 2);
        assert_ne!(
            groups[0].unverified_paths[0].path,
            groups[0].unverified_paths[1].path
        );
    }

    #[cfg(windows)]
    fn scan_native(roots: Vec<PathBuf>) -> DiscoverySnapshot {
        let cancellation = ScanCancellation::new();
        NativeDiscoveryJob.scan(
            roots,
            41,
            DiscoveryLimits::default(),
            &cancellation,
            &mut |_| {},
        )
    }

    #[cfg(windows)]
    #[test]
    fn dff_discovery_hard_link_aliases_are_one_native_file() {
        let temp = tempfile::tempdir().expect("temporary discovery root");
        let first = temp.path().join("first.bin");
        let alias = temp.path().join("alias.bin");
        std::fs::write(&first, b"same native object").expect("write generated fixture");
        std::fs::hard_link(&first, &alias).expect("create generated hard link");

        let snapshot = scan_native(vec![temp.path().to_path_buf()]);
        assert_eq!(snapshot.status, DiscoveryStatus::Complete);
        assert_eq!(snapshot.counts.files_seen, 2);
        let group = snapshot
            .size_groups
            .iter()
            .find(|group| group.size_bytes == b"same native object".len() as u64)
            .expect("size group for generated files");
        assert_eq!(group.known_files.len(), 1);
        assert_eq!(group.known_files[0].aliases.len(), 2);
        assert!(group.unverified_paths.is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn dff_discovery_equivalent_and_nested_roots_are_not_scanned_twice() {
        let temp = tempfile::tempdir().expect("temporary discovery root");
        let nested = temp.path().join("nested");
        std::fs::create_dir(&nested).expect("create nested root");
        std::fs::write(temp.path().join("top.bin"), b"top").expect("write top file");
        std::fs::write(nested.join("child.bin"), b"child").expect("write child file");
        let dotted = temp.path().join(".");

        let snapshot = scan_native(vec![temp.path().to_path_buf(), dotted, nested]);
        assert_eq!(snapshot.status, DiscoveryStatus::Complete);
        assert_eq!(snapshot.roots.len(), 3);
        assert_eq!(snapshot.roots[0].status, RootScanStatus::Complete);
        assert_eq!(snapshot.roots[1].status, RootScanStatus::SkippedEquivalent);
        assert_eq!(snapshot.roots[2].status, RootScanStatus::SkippedOverlap);
        assert_eq!(snapshot.counts.files_seen, 2);
    }
}
