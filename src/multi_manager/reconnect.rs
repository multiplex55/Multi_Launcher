use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use crate::multi_manager::model::{MmBindingStatus, MmWindow, MmWorkspace};
use crate::multi_manager::win::{self, EnumeratedWindow, WindowIdentitySnapshot};

pub type DirectHwndIdentity = WindowIdentitySnapshot;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconnectFingerprint {
    pub captured_title: String,
    pub executable: String,
    pub class_name: String,
    pub process_path: String,
    pub disabled: bool,
    pub hwnd: usize,
    pub valid: bool,
    pub binding_status: MmBindingStatus,
    pub binding_verified: bool,
}

impl From<&MmWindow> for ReconnectFingerprint {
    fn from(window: &MmWindow) -> Self {
        Self {
            captured_title: window.captured_title.clone(),
            executable: window.executable.clone(),
            class_name: window.class_name.clone(),
            process_path: window.process_path.clone(),
            disabled: window.disabled,
            hwnd: window.hwnd,
            valid: window.valid,
            binding_status: window.binding_status,
            binding_verified: window.binding_verified,
        }
    }
}

impl ReconnectFingerprint {
    fn fallback_title(&self) -> &str {
        self.captured_title.trim()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconnectSnapshotWindow {
    pub workspace_id: String,
    pub window_index: usize,
    pub fingerprint: ReconnectFingerprint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconnectSnapshot {
    pub windows: Vec<ReconnectSnapshotWindow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconnectBindingPatch {
    pub hwnd: usize,
    pub valid: bool,
    pub binding_status: MmBindingStatus,
    pub binding_verified: bool,
    pub live_title: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconnectPatch {
    pub workspace_id: String,
    pub window_index: usize,
    pub original: ReconnectFingerprint,
    pub outcome: ReconnectOutcome,
    pub binding: ReconnectBindingPatch,
}

pub fn collect_reconnect_snapshot(workspaces: &[MmWorkspace]) -> ReconnectSnapshot {
    ReconnectSnapshot {
        windows: workspaces
            .iter()
            .filter(|workspace| !workspace.disabled)
            .flat_map(|workspace| {
                workspace
                    .windows
                    .iter()
                    .enumerate()
                    .map(|(window_index, window)| ReconnectSnapshotWindow {
                        workspace_id: workspace.id.clone(),
                        window_index,
                        fingerprint: ReconnectFingerprint::from(window),
                    })
            })
            .collect(),
    }
}

pub fn collect_reconnect_snapshot_from_mutex(
    workspaces: &Arc<Mutex<Vec<MmWorkspace>>>,
) -> ReconnectSnapshot {
    let guard = workspaces.lock().unwrap();
    collect_reconnect_snapshot(&guard)
}

pub struct ReconnectDeps<'a> {
    pub is_window: &'a (dyn Fn(usize) -> bool + 'a),
    pub query_identity: &'a (dyn Fn(usize) -> Option<DirectHwndIdentity> + 'a),
    pub enumerate_top_level_windows: &'a (dyn Fn() -> Vec<EnumeratedWindow> + 'a),
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ReconnectSummary {
    pub already_valid: usize,
    pub reconnected: usize,
    pub missing: usize,
    pub ambiguous: usize,
    pub metadata_mismatch: usize,
    pub stale_results_discarded: usize,
    pub binding_snapshot_changed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconnectOutcome {
    AlreadyValid,
    Reconnected,
    Missing,
    Ambiguous,
    Invalidated,
    MetadataMismatch,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MatchCandidate {
    index: usize,
}

fn normalized(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn normalized_path(value: &str) -> String {
    value.trim().replace('/', "\\").to_ascii_lowercase()
}

fn same_title(value: &str, live: &str) -> bool {
    let stored = normalized(value);
    !stored.is_empty() && stored == normalized(live)
}

fn can_validate_stable_metadata(window: &ReconnectFingerprint, live: &EnumeratedWindow) -> bool {
    stable_identity_matches_enumerated(window, live)
}

fn direct_identity_matches(window: &ReconnectFingerprint, live: &DirectHwndIdentity) -> bool {
    stable_identity_matches(window, live)
}

fn has_stable_metadata(window: &ReconnectFingerprint) -> bool {
    !window.process_path.trim().is_empty()
        || !window.executable.trim().is_empty()
        || !window.class_name.trim().is_empty()
}

fn stable_identity_matches(window: &ReconnectFingerprint, live: &DirectHwndIdentity) -> bool {
    let live = EnumeratedWindow {
        hwnd: live.hwnd,
        title: live.live_title.clone(),
        executable: live.executable.clone(),
        class_name: live.class_name.clone(),
        process_path: live.process_path.clone(),
        rect: crate::multi_manager::model::MmRect {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
        },
    };
    stable_identity_matches_enumerated(window, &live)
}

fn stable_identity_matches_enumerated(
    window: &ReconnectFingerprint,
    live: &EnumeratedWindow,
) -> bool {
    let stored_process_path = window.process_path.trim();
    let stored_class_name = window.class_name.trim();
    let stored_executable = window.executable.trim();

    if stored_process_path.is_empty()
        && stored_class_name.is_empty()
        && stored_executable.is_empty()
    {
        return false;
    }

    if !stored_process_path.is_empty()
        && normalized_path(stored_process_path) != normalized_path(&live.process_path)
    {
        return false;
    }

    if !stored_class_name.is_empty()
        && normalized(stored_class_name) != normalized(&live.class_name)
    {
        return false;
    }

    if stored_process_path.is_empty()
        && !stored_executable.is_empty()
        && normalized(stored_executable) != normalized(&live.executable)
    {
        return false;
    }

    true
}

fn viable_exact_title_candidates(
    window: &ReconnectFingerprint,
    live: &[EnumeratedWindow],
    assigned: &HashSet<usize>,
) -> (usize, Vec<MatchCandidate>) {
    let exact_title: Vec<(usize, &EnumeratedWindow)> = live
        .iter()
        .enumerate()
        .filter(|(_, candidate)| !assigned.contains(&candidate.hwnd))
        .filter(|(_, candidate)| same_title(window.fallback_title(), &candidate.title))
        .collect();

    let exact_count = exact_title.len();
    if has_stable_metadata(window) {
        let viable = exact_title
            .into_iter()
            .filter(|(_, candidate)| can_validate_stable_metadata(window, candidate))
            .map(|(index, _)| MatchCandidate { index })
            .collect();
        (exact_count, viable)
    } else {
        let viable = exact_title
            .into_iter()
            .map(|(index, _)| MatchCandidate { index })
            .collect();
        (exact_count, viable)
    }
}

fn match_unbound_fallback(
    window: &ReconnectFingerprint,
    live: &[EnumeratedWindow],
    assigned: &HashSet<usize>,
) -> (ReconnectOutcome, Option<usize>) {
    let (exact_count, viable) = viable_exact_title_candidates(window, live, assigned);
    if exact_count == 0 {
        return (ReconnectOutcome::Missing, None);
    }
    if viable.is_empty() {
        return (ReconnectOutcome::MetadataMismatch, None);
    }
    if viable.len() > 1 {
        return (ReconnectOutcome::Ambiguous, None);
    }
    (
        ReconnectOutcome::Reconnected,
        Some(live[viable[0].index].hwnd),
    )
}

fn match_restored_hwnd(
    window: &ReconnectFingerprint,
    deps: &ReconnectDeps<'_>,
) -> (ReconnectOutcome, Option<usize>) {
    if !(deps.is_window)(window.hwnd) {
        return (ReconnectOutcome::Closed, None);
    }

    match (deps.query_identity)(window.hwnd) {
        Some(identity) if direct_identity_matches(window, &identity) => {
            (ReconnectOutcome::AlreadyValid, Some(window.hwnd))
        }
        _ => (ReconnectOutcome::MetadataMismatch, None),
    }
}

pub fn match_saved_window_against_candidates(
    window: &ReconnectFingerprint,
    live: &[EnumeratedWindow],
) -> (ReconnectOutcome, Option<usize>) {
    match_unbound_fallback(window, live, &HashSet::new())
}

pub fn match_saved_workspace_against_candidates(
    workspace: &MmWorkspace,
    live: &[EnumeratedWindow],
) -> Vec<(ReconnectOutcome, Option<usize>)> {
    let mut assigned = HashSet::new();
    workspace
        .windows
        .iter()
        .map(|window| {
            let fingerprint = ReconnectFingerprint::from(window);
            let result = match_unbound_fallback(&fingerprint, live, &assigned);
            if let Some(hwnd) = result.1 {
                assigned.insert(hwnd);
            }
            result
        })
        .collect()
}

pub fn reconnect_workspaces(workspaces: &mut [MmWorkspace]) -> ReconnectSummary {
    let is_window = |hwnd| win::is_valid_window(hwnd);
    let query_identity = |hwnd| Some(win::query_hwnd_identity(hwnd));
    let enumerate = || win::enumerate_top_level_windows().unwrap_or_default();
    reconnect_workspaces_with_deps(
        workspaces,
        ReconnectDeps {
            is_window: &is_window,
            query_identity: &query_identity,
            enumerate_top_level_windows: &enumerate,
        },
    )
}

pub fn reconnect_workspaces_with_windows(
    workspaces: &mut [MmWorkspace],
    live: &[EnumeratedWindow],
) -> ReconnectSummary {
    let is_window = |hwnd| live.iter().any(|candidate| candidate.hwnd == hwnd);
    let query_identity = |hwnd| {
        live.iter()
            .find(|candidate| candidate.hwnd == hwnd)
            .map(|candidate| DirectHwndIdentity {
                hwnd: candidate.hwnd,
                is_window: true,
                live_title: candidate.title.clone(),
                process_path: candidate.process_path.clone(),
                executable: candidate.executable.clone(),
                class_name: candidate.class_name.clone(),
            })
    };
    let enumerate = || live.to_vec();
    reconnect_workspaces_with_deps(
        workspaces,
        ReconnectDeps {
            is_window: &is_window,
            query_identity: &query_identity,
            enumerate_top_level_windows: &enumerate,
        },
    )
}

fn patch_for(
    window: &ReconnectSnapshotWindow,
    outcome: ReconnectOutcome,
    hwnd: usize,
    live_title: String,
) -> ReconnectPatch {
    let binding = match outcome {
        ReconnectOutcome::AlreadyValid => ReconnectBindingPatch {
            hwnd,
            valid: hwnd != 0,
            binding_status: if hwnd == 0 {
                MmBindingStatus::Missing
            } else {
                MmBindingStatus::Bound
            },
            binding_verified: hwnd != 0,
            live_title,
        },
        ReconnectOutcome::Reconnected => ReconnectBindingPatch {
            hwnd,
            valid: hwnd != 0,
            binding_status: if hwnd == 0 {
                MmBindingStatus::Missing
            } else {
                MmBindingStatus::Reconnected
            },
            binding_verified: hwnd != 0,
            live_title,
        },
        ReconnectOutcome::Closed => ReconnectBindingPatch {
            hwnd: 0,
            valid: false,
            binding_status: MmBindingStatus::Closed,
            binding_verified: false,
            live_title: String::new(),
        },
        ReconnectOutcome::Missing => ReconnectBindingPatch {
            hwnd: 0,
            valid: false,
            binding_status: MmBindingStatus::Missing,
            binding_verified: false,
            live_title: String::new(),
        },
        ReconnectOutcome::Ambiguous => ReconnectBindingPatch {
            hwnd: 0,
            valid: false,
            binding_status: MmBindingStatus::Ambiguous,
            binding_verified: false,
            live_title: String::new(),
        },
        ReconnectOutcome::MetadataMismatch => ReconnectBindingPatch {
            hwnd: 0,
            valid: false,
            binding_status: MmBindingStatus::MetadataMismatch,
            binding_verified: false,
            live_title: String::new(),
        },
        ReconnectOutcome::Invalidated => ReconnectBindingPatch {
            hwnd: 0,
            valid: false,
            binding_status: MmBindingStatus::Closed,
            binding_verified: false,
            live_title: String::new(),
        },
    };

    ReconnectPatch {
        workspace_id: window.workspace_id.clone(),
        window_index: window.window_index,
        original: window.fingerprint.clone(),
        outcome,
        binding,
    }
}

pub fn build_reconnect_patches(
    snapshot: &ReconnectSnapshot,
    deps: ReconnectDeps<'_>,
) -> (ReconnectSummary, Vec<ReconnectPatch>) {
    let mut summary = ReconnectSummary::default();
    let mut patches = Vec::new();
    let mut assigned = HashSet::new();
    let mut live_cache: Option<Vec<EnumeratedWindow>> = None;

    for window in &snapshot.windows {
        let fingerprint = &window.fingerprint;
        if fingerprint.disabled {
            continue;
        }

        if fingerprint.hwnd != 0 && fingerprint.binding_verified {
            if (deps.is_window)(fingerprint.hwnd) {
                summary.already_valid += 1;
                assigned.insert(fingerprint.hwnd);
                patches.push(patch_for(
                    window,
                    ReconnectOutcome::AlreadyValid,
                    fingerprint.hwnd,
                    String::new(),
                ));
                continue;
            }
        } else if fingerprint.hwnd != 0 {
            let (outcome, hwnd) = match_restored_hwnd(fingerprint, &deps);
            match outcome {
                ReconnectOutcome::AlreadyValid => {
                    let hwnd = hwnd.unwrap_or(fingerprint.hwnd);
                    summary.already_valid += 1;
                    assigned.insert(hwnd);
                    patches.push(patch_for(
                        window,
                        ReconnectOutcome::AlreadyValid,
                        hwnd,
                        String::new(),
                    ));
                    continue;
                }
                ReconnectOutcome::Closed => {}
                ReconnectOutcome::MetadataMismatch => {
                    summary.metadata_mismatch += 1;
                    patches.push(patch_for(
                        window,
                        ReconnectOutcome::MetadataMismatch,
                        0,
                        String::new(),
                    ));
                    continue;
                }
                _ => unreachable!(),
            }
        }

        let live = live_cache.get_or_insert_with(|| (deps.enumerate_top_level_windows)());
        let (outcome, hwnd) = match_unbound_fallback(fingerprint, live, &assigned);
        match outcome {
            ReconnectOutcome::Reconnected => {
                summary.reconnected += 1;
                let hwnd = hwnd.unwrap_or_default();
                let live_title = live
                    .iter()
                    .find(|candidate| candidate.hwnd == hwnd)
                    .map(|candidate| candidate.title.clone())
                    .unwrap_or_default();
                assigned.insert(hwnd);
                patches.push(patch_for(window, outcome, hwnd, live_title));
            }
            ReconnectOutcome::Ambiguous => {
                summary.ambiguous += 1;
                patches.push(patch_for(window, outcome, 0, String::new()));
            }
            ReconnectOutcome::MetadataMismatch => {
                summary.metadata_mismatch += 1;
                patches.push(patch_for(window, outcome, 0, String::new()));
            }
            ReconnectOutcome::Missing => {
                summary.missing += 1;
                patches.push(patch_for(window, outcome, 0, String::new()));
            }
            _ => unreachable!(),
        }
    }

    summary.binding_snapshot_changed = patches.iter().any(|patch| {
        patch.original.hwnd != patch.binding.hwnd
            || patch.original.binding_verified != patch.binding.binding_verified
    });
    (summary, patches)
}

fn count_applied_outcome(summary: &mut ReconnectSummary, outcome: ReconnectOutcome) {
    match outcome {
        ReconnectOutcome::AlreadyValid => summary.already_valid += 1,
        ReconnectOutcome::Reconnected => summary.reconnected += 1,
        ReconnectOutcome::Missing | ReconnectOutcome::Closed | ReconnectOutcome::Invalidated => {
            summary.missing += 1;
        }
        ReconnectOutcome::Ambiguous => summary.ambiguous += 1,
        ReconnectOutcome::MetadataMismatch => summary.metadata_mismatch += 1,
    }
}

fn binding_fields_changed(window: &MmWindow, binding: &ReconnectBindingPatch) -> bool {
    window.hwnd != binding.hwnd
        || window.valid != binding.valid
        || window.binding_status != binding.binding_status
        || window.binding_verified != binding.binding_verified
        || window.live_title != binding.live_title
}

fn current_assigned_hwnds(workspaces: &[MmWorkspace]) -> HashSet<usize> {
    workspaces
        .iter()
        .filter(|workspace| !workspace.disabled)
        .flat_map(|workspace| &workspace.windows)
        .filter(|window| !window.disabled && window.hwnd != 0 && window.valid)
        .map(|window| window.hwnd)
        .collect()
}

pub fn apply_reconnect_patches(
    workspaces: &mut [MmWorkspace],
    patches: &[ReconnectPatch],
) -> ReconnectSummary {
    let mut summary = ReconnectSummary::default();
    let mut assigned = current_assigned_hwnds(workspaces);

    for patch in patches {
        let Some(workspace_index) = workspaces
            .iter()
            .position(|workspace| workspace.id == patch.workspace_id)
        else {
            summary.stale_results_discarded += 1;
            continue;
        };

        let Some(window) = workspaces[workspace_index]
            .windows
            .get_mut(patch.window_index)
        else {
            summary.stale_results_discarded += 1;
            continue;
        };

        let current = ReconnectFingerprint::from(&*window);
        if current != patch.original {
            summary.stale_results_discarded += 1;
            continue;
        }

        if patch.outcome == ReconnectOutcome::Reconnected {
            let candidate = patch.binding.hwnd;
            if candidate != 0 && assigned.contains(&candidate) {
                summary.stale_results_discarded += 1;
                continue;
            }
        }

        if binding_fields_changed(window, &patch.binding) {
            summary.binding_snapshot_changed = true;
        }
        window.hwnd = patch.binding.hwnd;
        window.valid = patch.binding.valid;
        window.binding_status = patch.binding.binding_status;
        window.binding_verified = patch.binding.binding_verified;
        window.live_title = patch.binding.live_title.clone();

        if patch.binding.hwnd != 0 && patch.binding.valid {
            assigned.insert(patch.binding.hwnd);
        }
        count_applied_outcome(&mut summary, patch.outcome);
    }

    summary
}

pub fn reconnect_workspaces_with_deps(
    workspaces: &mut [MmWorkspace],
    deps: ReconnectDeps<'_>,
) -> ReconnectSummary {
    let snapshot = collect_reconnect_snapshot(workspaces);
    let (_worker_summary, patches) = build_reconnect_patches(&snapshot, deps);
    apply_reconnect_patches(workspaces, &patches)
}

pub fn reconnect_shared_workspaces_with_deps(
    workspaces: &Arc<Mutex<Vec<MmWorkspace>>>,
    deps: ReconnectDeps<'_>,
) -> ReconnectSummary {
    let snapshot = collect_reconnect_snapshot_from_mutex(workspaces);
    let (_worker_summary, patches) = build_reconnect_patches(&snapshot, deps);
    let mut guard = workspaces.lock().unwrap();
    apply_reconnect_patches(&mut guard, &patches)
}

/// Runs exact-title reconnect fallback only for unresolved enabled windows, preserving
/// already-bound valid HWNDs without revalidating or comparing their metadata.
pub fn reconnect_unresolved_workspaces_with_windows(
    workspaces: &mut [MmWorkspace],
    live: &[EnumeratedWindow],
) -> ReconnectSummary {
    let before: Vec<(usize, bool)> = workspaces
        .iter()
        .flat_map(|workspace| &workspace.windows)
        .map(|window| (window.hwnd, window.binding_verified))
        .collect();
    let mut summary = ReconnectSummary::default();
    let mut assigned: HashSet<usize> = workspaces
        .iter()
        .filter(|workspace| !workspace.disabled)
        .flat_map(|workspace| &workspace.windows)
        .filter(|window| !window.disabled && window.hwnd != 0 && window.valid)
        .map(|window| window.hwnd)
        .collect();

    for window in workspaces
        .iter_mut()
        .filter(|workspace| !workspace.disabled)
        .flat_map(|workspace| &mut workspace.windows)
    {
        if window.disabled || (window.hwnd != 0 && window.valid) {
            continue;
        }

        let fingerprint = ReconnectFingerprint::from(&*window);
        let (outcome, hwnd) = match_unbound_fallback(&fingerprint, live, &assigned);
        match outcome {
            ReconnectOutcome::Reconnected => {
                summary.reconnected += 1;
                if let Some(hwnd) = hwnd {
                    window.mark_reconnected(hwnd);
                    if let Some(live_window) = live.iter().find(|candidate| candidate.hwnd == hwnd)
                    {
                        window.live_title = live_window.title.clone();
                    }
                    assigned.insert(hwnd);
                }
            }
            ReconnectOutcome::Ambiguous => {
                summary.ambiguous += 1;
                window.mark_ambiguous();
                window.live_title.clear();
            }
            ReconnectOutcome::MetadataMismatch => {
                summary.metadata_mismatch += 1;
                window.mark_metadata_mismatch();
                window.live_title.clear();
            }
            ReconnectOutcome::Missing => {
                summary.missing += 1;
                window.mark_missing();
                window.live_title.clear();
            }
            _ => unreachable!(),
        }
    }

    let after: Vec<(usize, bool)> = workspaces
        .iter()
        .flat_map(|workspace| &workspace.windows)
        .map(|window| (window.hwnd, window.binding_verified))
        .collect();
    summary.binding_snapshot_changed = before != after;
    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::multi_manager::model::MmRect;
    use std::cell::Cell;
    use std::sync::{Arc, Mutex};

    fn rect() -> MmRect {
        MmRect {
            x: 0,
            y: 0,
            w: 100,
            h: 100,
        }
    }

    fn live(
        hwnd: usize,
        title: &str,
        executable: &str,
        class_name: &str,
        process_path: &str,
    ) -> EnumeratedWindow {
        EnumeratedWindow {
            hwnd,
            title: title.into(),
            executable: executable.into(),
            class_name: class_name.into(),
            process_path: process_path.into(),
            rect: rect(),
        }
    }

    fn identity_from(candidate: &EnumeratedWindow) -> DirectHwndIdentity {
        DirectHwndIdentity {
            hwnd: candidate.hwnd,
            is_window: true,
            live_title: candidate.title.clone(),
            process_path: candidate.process_path.clone(),
            executable: candidate.executable.clone(),
            class_name: candidate.class_name.clone(),
        }
    }

    fn workspace(window: MmWindow) -> Vec<MmWorkspace> {
        vec![MmWorkspace {
            windows: vec![window],
            ..MmWorkspace::default()
        }]
    }

    #[test]
    fn valid_hwnd_survives_changed_title() {
        let mut workspaces = workspace(MmWindow {
            hwnd: 5,
            binding_verified: true,
            live_title: "Old".into(),
            ..MmWindow::default()
        });
        let summary = reconnect_workspaces_with_deps(
            &mut workspaces,
            ReconnectDeps {
                is_window: &|hwnd| hwnd == 5,
                query_identity: &|_| panic!("identity should not be queried"),
                enumerate_top_level_windows: &|| panic!("should not enumerate"),
            },
        );
        assert_eq!(summary.already_valid, 1);
        assert_eq!(workspaces[0].windows[0].hwnd, 5);
        assert!(workspaces[0].windows[0].binding_verified);
    }

    #[test]
    fn valid_hwnd_does_not_invoke_enumeration() {
        let enumerated = Cell::new(false);
        let mut workspaces = workspace(MmWindow {
            hwnd: 5,
            binding_verified: true,
            ..MmWindow::default()
        });
        reconnect_workspaces_with_deps(
            &mut workspaces,
            ReconnectDeps {
                is_window: &|_| true,
                query_identity: &|_| None,
                enumerate_top_level_windows: &|| {
                    enumerated.set(true);
                    Vec::new()
                },
            },
        );
        assert!(!enumerated.get());
    }

    #[test]
    fn invalid_hwnd_is_cleared() {
        let mut workspaces = workspace(MmWindow {
            hwnd: 5,
            binding_verified: true,
            captured_title: "Doc".into(),
            ..MmWindow::default()
        });
        let summary = reconnect_workspaces_with_deps(
            &mut workspaces,
            ReconnectDeps {
                is_window: &|_| false,
                query_identity: &|_| None,
                enumerate_top_level_windows: &|| Vec::new(),
            },
        );
        assert_eq!(summary.missing, 1);
        assert_eq!(workspaces[0].windows[0].hwnd, 0);
        assert!(!workspaces[0].windows[0].binding_verified);
    }

    #[test]
    fn invalid_hwnd_reconnects_to_one_exact_title_candidate() {
        let candidate = live(9, "Doc", "edit.exe", "Editor", "C:/Apps/edit.exe");
        let mut workspaces = workspace(MmWindow {
            hwnd: 5,
            binding_verified: true,
            captured_title: "Doc".into(),
            executable: "edit.exe".into(),
            ..MmWindow::default()
        });
        let summary = reconnect_workspaces_with_deps(
            &mut workspaces,
            ReconnectDeps {
                is_window: &|hwnd| hwnd == 9,
                query_identity: &|_| None,
                enumerate_top_level_windows: &|| vec![candidate.clone()],
            },
        );
        assert_eq!(summary.missing, 0);
        assert_eq!(summary.reconnected, 1);
        assert_eq!(workspaces[0].windows[0].hwnd, 9);
        assert!(workspaces[0].windows[0].binding_verified);
    }

    #[test]
    fn stale_unverified_invalid_hwnd_reconnects_to_one_exact_title_candidate() {
        let candidate = live(42, "Notes", "app.exe", "AppClass", "C:/app.exe");
        let mut workspaces = workspace(MmWindow {
            hwnd: 7,
            valid: false,
            binding_verified: false,
            captured_title: "Notes".into(),
            executable: "app.exe".into(),
            class_name: "AppClass".into(),
            process_path: "C:/app.exe".into(),
            ..MmWindow::default()
        });
        let summary = reconnect_workspaces_with_deps(
            &mut workspaces,
            ReconnectDeps {
                is_window: &|hwnd| hwnd == 42,
                query_identity: &|_| None,
                enumerate_top_level_windows: &|| vec![candidate.clone()],
            },
        );
        assert_eq!(summary.missing, 0);
        assert_eq!(summary.reconnected, 1);
        assert_eq!(workspaces[0].windows[0].hwnd, 42);
        assert!(workspaces[0].windows[0].binding_verified);
    }

    #[test]
    fn different_title_with_identical_executable_does_not_reconnect() {
        let mut workspaces = workspace(MmWindow {
            captured_title: "Doc".into(),
            executable: "edit.exe".into(),
            ..MmWindow::default()
        });
        let summary = reconnect_workspaces_with_windows(
            &mut workspaces,
            &[live(10, "Other", "edit.exe", "", "")],
        );
        assert_eq!(summary.missing, 1);
        assert_eq!(workspaces[0].windows[0].hwnd, 0);
    }

    #[test]
    fn duplicate_exact_title_candidates_are_ambiguous() {
        let mut workspaces = workspace(MmWindow {
            captured_title: "Doc".into(),
            ..MmWindow::default()
        });
        let summary = reconnect_workspaces_with_windows(
            &mut workspaces,
            &[live(10, "Doc", "", "", ""), live(11, " doc ", "", "", "")],
        );
        assert_eq!(summary.ambiguous, 1);
        assert_eq!(workspaces[0].windows[0].hwnd, 0);
    }

    #[test]
    fn stable_metadata_disambiguates_duplicate_exact_titles() {
        let mut workspaces = workspace(MmWindow {
            captured_title: "Doc".into(),
            process_path: "C:/Apps/edit.exe".into(),
            ..MmWindow::default()
        });
        let summary = reconnect_workspaces_with_windows(
            &mut workspaces,
            &[
                live(10, "Doc", "edit.exe", "", "C:/Other/edit.exe"),
                live(11, "Doc", "edit.exe", "", "C:/Apps/edit.exe"),
            ],
        );
        assert_eq!(summary.reconnected, 1);
        assert_eq!(workspaces[0].windows[0].hwnd, 11);
    }

    #[test]
    fn exact_title_candidate_with_metadata_mismatch_is_rejected() {
        let mut workspaces = workspace(MmWindow {
            captured_title: "Doc".into(),
            process_path: "C:/Apps/edit.exe".into(),
            ..MmWindow::default()
        });
        let summary = reconnect_workspaces_with_windows(
            &mut workspaces,
            &[live(10, "Doc", "edit.exe", "", "C:/Other/edit.exe")],
        );
        assert_eq!(summary.metadata_mismatch, 1);
        assert_eq!(workspaces[0].windows[0].hwnd, 0);
    }

    #[test]
    fn legacy_title_only_entry_reconnects_when_exactly_one_candidate_exists() {
        let mut workspaces = workspace(MmWindow {
            captured_title: "Legacy App".into(),
            ..MmWindow::default()
        });
        let summary = reconnect_workspaces_with_windows(
            &mut workspaces,
            &[live(77, "legacy app", "app.exe", "Class", "C:/app.exe")],
        );
        assert_eq!(summary.reconnected, 1);
        assert_eq!(workspaces[0].windows[0].hwnd, 77);
    }

    #[test]
    fn one_hwnd_cannot_be_assigned_to_two_entries() {
        let mut workspaces = vec![MmWorkspace {
            windows: vec![
                MmWindow {
                    captured_title: "Doc".into(),
                    ..MmWindow::default()
                },
                MmWindow {
                    captured_title: "Doc".into(),
                    ..MmWindow::default()
                },
            ],
            ..MmWorkspace::default()
        }];
        let summary = reconnect_workspaces_with_windows(
            &mut workspaces,
            &[live(8, "Doc", "app.exe", "", "")],
        );
        assert_eq!(summary.reconnected, 1);
        assert_eq!(summary.missing, 1);
        assert_eq!(workspaces[0].windows[0].hwnd, 8);
        assert_eq!(workspaces[0].windows[1].hwnd, 0);
    }

    #[test]
    fn bound_hwnd_is_never_replaced_by_better_looking_title_match() {
        let mut workspaces = workspace(MmWindow {
            hwnd: 5,
            binding_verified: true,
            captured_title: "Doc".into(),
            executable: "edit.exe".into(),
            ..MmWindow::default()
        });
        let summary = reconnect_workspaces_with_deps(
            &mut workspaces,
            ReconnectDeps {
                is_window: &|hwnd| hwnd == 5,
                query_identity: &|_| None,
                enumerate_top_level_windows: &|| vec![live(9, "Doc", "edit.exe", "", "")],
            },
        );
        assert_eq!(summary.already_valid, 1);
        assert_eq!(workspaces[0].windows[0].hwnd, 5);
    }

    #[test]
    fn verified_valid_hwnd_is_preserved_without_metadata_even_when_fallback_matches_other_hwnd() {
        let mut workspaces = workspace(MmWindow {
            hwnd: 5,
            binding_verified: true,
            captured_title: "Doc".into(),
            executable: "edit.exe".into(),
            class_name: "Editor".into(),
            process_path: "C:/Apps/edit.exe".into(),
            ..MmWindow::default()
        });
        let summary = reconnect_workspaces_with_deps(
            &mut workspaces,
            ReconnectDeps {
                is_window: &|hwnd| hwnd == 5,
                query_identity: &|_| panic!("verified HWND preservation must only call IsWindow"),
                enumerate_top_level_windows: &|| {
                    vec![live(9, "Doc", "edit.exe", "Editor", "C:/Apps/edit.exe")]
                },
            },
        );
        assert_eq!(summary.already_valid, 1);
        assert!(!summary.binding_snapshot_changed);
        assert_eq!(workspaces[0].windows[0].hwnd, 5);
        assert!(workspaces[0].windows[0].binding_verified);
    }

    #[test]
    fn closed_unverified_hwnd_runs_exact_title_fallback() {
        let candidate = live(42, "Doc", "edit.exe", "Editor", "C:/Apps/edit.exe");
        let mut workspaces = workspace(MmWindow {
            hwnd: 5,
            binding_verified: false,
            valid: true,
            captured_title: "Doc".into(),
            executable: "edit.exe".into(),
            class_name: "Editor".into(),
            process_path: "C:/Apps/edit.exe".into(),
            ..MmWindow::default()
        });
        let summary = reconnect_workspaces_with_deps(
            &mut workspaces,
            ReconnectDeps {
                is_window: &|hwnd| hwnd == 42,
                query_identity: &|_| panic!("closed HWND identity should not be queried"),
                enumerate_top_level_windows: &|| vec![candidate.clone()],
            },
        );
        assert_eq!(summary.missing, 0);
        assert_eq!(summary.reconnected, 1);
        assert!(summary.binding_snapshot_changed);
        assert_eq!(workspaces[0].windows[0].hwnd, 42);
    }

    #[test]
    fn reconnect_builds_and_applies_patch_for_exact_title_reconnect() {
        let candidate = live(9, "Doc", "edit.exe", "Editor", "C:/Apps/edit.exe");
        let mut workspaces = workspace(MmWindow {
            hwnd: 5,
            binding_verified: true,
            captured_title: "Doc".into(),
            executable: "edit.exe".into(),
            ..MmWindow::default()
        });
        workspaces[0].id = "workspace-a".into();
        let snapshot = collect_reconnect_snapshot(&workspaces);
        let (summary, patches) = build_reconnect_patches(
            &snapshot,
            ReconnectDeps {
                is_window: &|hwnd| hwnd == 9,
                query_identity: &|_| None,
                enumerate_top_level_windows: &|| vec![candidate.clone()],
            },
        );

        assert_eq!(summary.missing, 0);
        assert_eq!(summary.reconnected, 1);
        assert_eq!(patches.len(), 1);
        assert_eq!(patches[0].workspace_id, "workspace-a");
        assert_eq!(patches[0].window_index, 0);
        assert_eq!(patches[0].original.hwnd, 5);
        assert_eq!(patches[0].outcome, ReconnectOutcome::Reconnected);
        assert_eq!(patches[0].binding.hwnd, 9);
        assert_eq!(
            patches[0].binding.binding_status,
            MmBindingStatus::Reconnected
        );

        let applied = apply_reconnect_patches(&mut workspaces, &patches);
        assert_eq!(applied.reconnected, 1);
        assert_eq!(workspaces[0].windows[0].hwnd, 9);
        assert_eq!(workspaces[0].windows[0].live_title, "Doc");
        assert!(workspaces[0].windows[0].binding_verified);
    }

    #[test]
    fn shared_reconnect_releases_workspace_mutex_before_enumeration() {
        let workspaces = Arc::new(Mutex::new(workspace(MmWindow {
            captured_title: "Doc".into(),
            ..MmWindow::default()
        })));
        workspaces.lock().unwrap()[0].id = "workspace-a".into();
        let observed_unlocked = Cell::new(false);
        let observed_workspaces = Arc::clone(&workspaces);

        let summary = reconnect_shared_workspaces_with_deps(
            &workspaces,
            ReconnectDeps {
                is_window: &|_| false,
                query_identity: &|_| None,
                enumerate_top_level_windows: &|| {
                    observed_unlocked.set(observed_workspaces.try_lock().is_ok());
                    vec![live(7, "Doc", "", "", "")]
                },
            },
        );

        assert!(observed_unlocked.get());
        assert_eq!(summary.reconnected, 1);
        assert_eq!(workspaces.lock().unwrap()[0].windows[0].hwnd, 7);
    }

    #[test]
    fn restored_unverified_hwnd_validates_stable_metadata_without_title() {
        let candidate = live(5, "Changed", "edit.exe", "Editor", "C:/Apps/edit.exe");
        let mut workspaces = workspace(MmWindow {
            hwnd: 5,
            binding_verified: false,
            captured_title: "Doc".into(),
            process_path: "C:/Apps/edit.exe".into(),
            class_name: "Editor".into(),
            ..MmWindow::default()
        });
        let summary = reconnect_workspaces_with_deps(
            &mut workspaces,
            ReconnectDeps {
                is_window: &|_| true,
                query_identity: &|_| Some(identity_from(&candidate)),
                enumerate_top_level_windows: &|| panic!("should not enumerate"),
            },
        );
        assert_eq!(summary.already_valid, 1);
        assert_eq!(workspaces[0].windows[0].hwnd, 5);
        assert!(workspaces[0].windows[0].binding_verified);
    }

    #[test]
    fn recapture_during_reconnect_discards_stale_patch() {
        let mut workspaces = workspace(MmWindow {
            captured_title: "Doc".into(),
            executable: "old.exe".into(),
            alias: "keep".into(),
            ..MmWindow::default()
        });
        workspaces[0].id = "workspace-a".into();
        let snapshot = collect_reconnect_snapshot(&workspaces);
        let (_worker, patches) = build_reconnect_patches(
            &snapshot,
            ReconnectDeps {
                is_window: &|_| false,
                query_identity: &|_| None,
                enumerate_top_level_windows: &|| vec![live(10, "Doc", "old.exe", "", "")],
            },
        );

        workspaces[0].windows[0].captured_title = "New Doc".into();
        workspaces[0].windows[0].executable = "new.exe".into();
        workspaces[0].windows[0].mark_bound(55);
        let applied = apply_reconnect_patches(&mut workspaces, &patches);

        assert_eq!(applied.stale_results_discarded, 1);
        assert_eq!(applied.reconnected, 0);
        assert_eq!(workspaces[0].windows[0].hwnd, 55);
        assert_eq!(workspaces[0].windows[0].captured_title, "New Doc");
        assert_eq!(workspaces[0].windows[0].executable, "new.exe");
    }

    #[test]
    fn deleted_workspace_discards_stale_patch_safely() {
        let mut workspaces = workspace(MmWindow {
            captured_title: "Doc".into(),
            ..MmWindow::default()
        });
        workspaces[0].id = "workspace-a".into();
        let snapshot = collect_reconnect_snapshot(&workspaces);
        let (_worker, patches) = build_reconnect_patches(
            &snapshot,
            ReconnectDeps {
                is_window: &|_| false,
                query_identity: &|_| None,
                enumerate_top_level_windows: &|| vec![live(10, "Doc", "", "", "")],
            },
        );
        workspaces.clear();

        let applied = apply_reconnect_patches(&mut workspaces, &patches);

        assert_eq!(applied.stale_results_discarded, 1);
        assert!(workspaces.is_empty());
    }

    #[test]
    fn reordered_windows_do_not_receive_incorrect_patches() {
        let mut workspaces = vec![MmWorkspace {
            id: "workspace-a".into(),
            windows: vec![
                MmWindow {
                    captured_title: "One".into(),
                    ..MmWindow::default()
                },
                MmWindow {
                    captured_title: "Two".into(),
                    ..MmWindow::default()
                },
            ],
            ..MmWorkspace::default()
        }];
        let snapshot = collect_reconnect_snapshot(&workspaces);
        let (_worker, patches) = build_reconnect_patches(
            &snapshot,
            ReconnectDeps {
                is_window: &|_| false,
                query_identity: &|_| None,
                enumerate_top_level_windows: &|| {
                    vec![live(11, "One", "", "", ""), live(22, "Two", "", "", "")]
                },
            },
        );
        workspaces[0].windows.swap(0, 1);

        let applied = apply_reconnect_patches(&mut workspaces, &patches);

        assert_eq!(applied.stale_results_discarded, 2);
        assert_eq!(workspaces[0].windows[0].captured_title, "Two");
        assert_eq!(workspaces[0].windows[0].hwnd, 0);
        assert_eq!(workspaces[0].windows[1].captured_title, "One");
        assert_eq!(workspaces[0].windows[1].hwnd, 0);
    }

    #[test]
    fn candidate_hwnd_already_assigned_elsewhere_is_rejected_at_apply() {
        let mut workspaces = vec![MmWorkspace {
            id: "workspace-a".into(),
            windows: vec![
                MmWindow {
                    captured_title: "Doc".into(),
                    ..MmWindow::default()
                },
                MmWindow {
                    captured_title: "Other".into(),
                    ..MmWindow::default()
                },
            ],
            ..MmWorkspace::default()
        }];
        let snapshot = collect_reconnect_snapshot(&workspaces);
        let (_worker, patches) = build_reconnect_patches(
            &snapshot,
            ReconnectDeps {
                is_window: &|_| false,
                query_identity: &|_| None,
                enumerate_top_level_windows: &|| vec![live(77, "Doc", "", "", "")],
            },
        );
        workspaces[0].windows[1].mark_bound(77);

        let applied = apply_reconnect_patches(&mut workspaces, &patches);

        assert_eq!(applied.stale_results_discarded, 1);
        assert_eq!(applied.reconnected, 0);
        assert_eq!(workspaces[0].windows[0].hwnd, 0);
        assert_eq!(workspaces[0].windows[1].hwnd, 77);
    }

    #[test]
    fn final_summary_counts_applied_patches_not_discarded_worker_results() {
        let mut workspaces = workspace(MmWindow {
            captured_title: "Doc".into(),
            ..MmWindow::default()
        });
        workspaces[0].id = "workspace-a".into();
        let snapshot = collect_reconnect_snapshot(&workspaces);
        let (worker, patches) = build_reconnect_patches(
            &snapshot,
            ReconnectDeps {
                is_window: &|_| false,
                query_identity: &|_| None,
                enumerate_top_level_windows: &|| vec![live(10, "Doc", "", "", "")],
            },
        );
        assert_eq!(worker.reconnected, 1);
        workspaces[0].windows[0].captured_title = "Changed".into();

        let applied = apply_reconnect_patches(&mut workspaces, &patches);

        assert_eq!(applied.reconnected, 0);
        assert_eq!(applied.stale_results_discarded, 1);
        assert!(!applied.binding_snapshot_changed);
    }

    #[test]
    fn patching_only_updates_binding_fields() {
        let mut workspaces = workspace(MmWindow {
            alias: "Alias".into(),
            captured_title: "Doc".into(),
            executable: "app.exe".into(),
            class_name: "Class".into(),
            process_path: "C:/app.exe".into(),
            home_rect: Some(rect()),
            target_rect: Some(MmRect {
                x: 1,
                y: 2,
                w: 3,
                h: 4,
            }),
            disabled: false,
            ..MmWindow::default()
        });
        workspaces[0].id = "workspace-a".into();
        workspaces[0].name = "Workspace".into();
        workspaces[0].disabled = false;
        let before_window = workspaces[0].windows[0].clone();
        let before_workspace_name = workspaces[0].name.clone();
        let before_workspace_disabled = workspaces[0].disabled;
        let snapshot = collect_reconnect_snapshot(&workspaces);
        let (_worker, patches) = build_reconnect_patches(
            &snapshot,
            ReconnectDeps {
                is_window: &|_| false,
                query_identity: &|_| None,
                enumerate_top_level_windows: &|| {
                    vec![live(88, "Doc", "app.exe", "Class", "C:/app.exe")]
                },
            },
        );

        let applied = apply_reconnect_patches(&mut workspaces, &patches);

        assert_eq!(applied.reconnected, 1);
        assert!(applied.binding_snapshot_changed);
        let window = &workspaces[0].windows[0];
        assert_eq!(window.alias, before_window.alias);
        assert_eq!(window.home_rect, before_window.home_rect);
        assert_eq!(window.target_rect, before_window.target_rect);
        assert_eq!(window.captured_title, before_window.captured_title);
        assert_eq!(window.executable, before_window.executable);
        assert_eq!(window.class_name, before_window.class_name);
        assert_eq!(window.process_path, before_window.process_path);
        assert_eq!(window.disabled, before_window.disabled);
        assert_eq!(workspaces[0].name, before_workspace_name);
        assert_eq!(workspaces[0].disabled, before_workspace_disabled);
        assert_eq!(window.hwnd, 88);
    }
}
