use std::collections::HashSet;

use crate::multi_manager::identity;
use crate::multi_manager::model::{MmWindow, MmWorkspace};
use crate::multi_manager::win::{self, EnumeratedWindow, WindowIdentitySnapshot};

pub type DirectHwndIdentity = WindowIdentitySnapshot;

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
    pub invalidated: usize,
    pub metadata_mismatch: usize,
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

fn same_title(value: &str, live: &str) -> bool {
    let stored = normalized(value);
    !stored.is_empty() && stored == normalized(live)
}

fn can_validate_stable_metadata(window: &MmWindow, live: &EnumeratedWindow) -> bool {
    identity::stable_identity_matches_enumerated(window, live)
}

fn direct_identity_matches(window: &MmWindow, live: &DirectHwndIdentity) -> bool {
    identity::stable_identity_matches(window, live)
}

fn viable_exact_title_candidates(
    window: &MmWindow,
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
    if identity::has_stable_metadata(window) {
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
    window: &MmWindow,
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
    window: &MmWindow,
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
    window: &MmWindow,
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
            let result = match_unbound_fallback(window, live, &assigned);
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

pub fn reconnect_workspaces_with_deps(
    workspaces: &mut [MmWorkspace],
    deps: ReconnectDeps<'_>,
) -> ReconnectSummary {
    let before: Vec<(usize, bool)> = workspaces
        .iter()
        .flat_map(|workspace| &workspace.windows)
        .map(|window| (window.hwnd, window.binding_verified))
        .collect();
    let mut summary = ReconnectSummary::default();
    let mut assigned = HashSet::new();
    let mut live_cache: Option<Vec<EnumeratedWindow>> = None;

    for window in workspaces
        .iter_mut()
        .flat_map(|workspace| &mut workspace.windows)
    {
        if window.disabled {
            continue;
        }

        if window.hwnd != 0 && window.binding_verified {
            if (deps.is_window)(window.hwnd) {
                summary.already_valid += 1;
                assigned.insert(window.hwnd);
                continue;
            }
            summary.invalidated += 1;
            window.mark_closed();
            window.live_title.clear();
        } else if window.hwnd != 0 {
            let (outcome, hwnd) = match_restored_hwnd(window, &deps);
            match outcome {
                ReconnectOutcome::AlreadyValid => {
                    summary.already_valid += 1;
                    window.mark_bound(hwnd.unwrap_or(window.hwnd));
                    assigned.insert(window.hwnd);
                    continue;
                }
                ReconnectOutcome::Closed => {
                    summary.invalidated += 1;
                    let was_already_invalid = !window.valid;
                    window.mark_closed();
                    window.live_title.clear();
                    if !was_already_invalid {
                        continue;
                    }
                }
                ReconnectOutcome::MetadataMismatch => {
                    summary.metadata_mismatch += 1;
                    window.mark_metadata_mismatch();
                    window.live_title.clear();
                    continue;
                }
                _ => unreachable!(),
            }
        }

        let live = live_cache.get_or_insert_with(|| (deps.enumerate_top_level_windows)());
        let (outcome, hwnd) = match_unbound_fallback(window, live, &assigned);
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

pub fn needs_reconnect(workspaces: &[MmWorkspace]) -> bool {
    workspaces
        .iter()
        .flat_map(|workspace| &workspace.windows)
        .any(|window| window.hwnd == 0 || !window.valid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::multi_manager::model::MmRect;
    use std::cell::Cell;

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
        assert_eq!(summary.invalidated, 1);
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
        assert_eq!(summary.invalidated, 1);
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
        assert_eq!(summary.invalidated, 1);
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
}
