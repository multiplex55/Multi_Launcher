use std::collections::HashSet;

use crate::multi_manager::model::{MmWindow, MmWorkspace};
use crate::multi_manager::win::{self, EnumeratedWindow};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ReconnectSummary {
    pub already_valid: usize,
    pub reconnected: usize,
    pub missing: usize,
    pub ambiguous: usize,
    pub invalidated: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconnectOutcome {
    AlreadyValid,
    Reconnected,
    Missing,
    Ambiguous,
    Invalidated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MatchCandidate {
    index: usize,
    score: u8,
}

fn normalized(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn same_stored(value: &str, live: &str) -> bool {
    let stored = normalized(value);
    !stored.is_empty() && stored == normalized(live)
}

fn has_only_title_metadata(window: &MmWindow) -> bool {
    !window.title.trim().is_empty()
        && window.executable.trim().is_empty()
        && window.class_name.trim().is_empty()
        && window.process_path.trim().is_empty()
}

fn identity_score(window: &MmWindow, live: &EnumeratedWindow) -> Option<u8> {
    let title = same_stored(&window.title, &live.title);
    let process_path = same_stored(&window.process_path, &live.process_path);
    let executable = same_stored(&window.executable, &live.executable);
    let class_name = same_stored(&window.class_name, &live.class_name);

    if process_path && class_name && title {
        Some(100)
    } else if process_path && title {
        Some(90)
    } else if executable && class_name && title {
        Some(80)
    } else if executable && title {
        Some(70)
    } else if class_name && title {
        Some(60)
    } else if title {
        Some(40)
    } else {
        None
    }
}

fn matching_existing_hwnd<'a>(
    window: &MmWindow,
    live: &'a [EnumeratedWindow],
    assigned: &HashSet<usize>,
) -> Option<&'a EnumeratedWindow> {
    if window.hwnd == 0 || assigned.contains(&window.hwnd) {
        return None;
    }

    live.iter().find(|candidate| {
        candidate.hwnd == window.hwnd && identity_score(window, candidate).is_some()
    })
}

fn best_match(
    window: &MmWindow,
    live: &[EnumeratedWindow],
    assigned: &HashSet<usize>,
) -> Result<Option<MatchCandidate>, ()> {
    let mut matches: Vec<MatchCandidate> = live
        .iter()
        .enumerate()
        .filter(|(_, candidate)| !assigned.contains(&candidate.hwnd))
        .filter_map(|(index, candidate)| {
            identity_score(window, candidate).map(|score| MatchCandidate { index, score })
        })
        .collect();

    if matches.is_empty() {
        return Ok(None);
    }

    matches.sort_by_key(|candidate| std::cmp::Reverse(candidate.score));
    let best_score = matches[0].score;
    let best_count = matches
        .iter()
        .filter(|candidate| candidate.score == best_score)
        .count();

    if best_score == 40 && has_only_title_metadata(window) && best_count != 1 {
        return Err(());
    }

    if best_count == 1 {
        Ok(Some(matches[0]))
    } else {
        Err(())
    }
}

fn match_saved_window(
    window: &MmWindow,
    live: &[EnumeratedWindow],
    assigned: &HashSet<usize>,
) -> (ReconnectOutcome, Option<usize>) {
    if let Some(candidate) = matching_existing_hwnd(window, live, assigned) {
        return (ReconnectOutcome::AlreadyValid, Some(candidate.hwnd));
    }

    let stale_rejected = window.hwnd != 0;
    match best_match(window, live, assigned) {
        Ok(Some(candidate)) => {
            let outcome = if stale_rejected {
                ReconnectOutcome::Invalidated
            } else {
                ReconnectOutcome::Reconnected
            };
            (outcome, Some(live[candidate.index].hwnd))
        }
        Ok(None) => {
            let outcome = if stale_rejected {
                ReconnectOutcome::Invalidated
            } else {
                ReconnectOutcome::Missing
            };
            (outcome, None)
        }
        Err(()) => (ReconnectOutcome::Ambiguous, None),
    }
}

pub fn match_saved_window_against_candidates(
    window: &MmWindow,
    live: &[EnumeratedWindow],
) -> (ReconnectOutcome, Option<usize>) {
    match_saved_window(window, live, &HashSet::new())
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
            let result = match_saved_window(window, live, &assigned);
            if let Some(hwnd) = result.1 {
                assigned.insert(hwnd);
            }
            result
        })
        .collect()
}

pub fn reconnect_workspaces(workspaces: &mut [MmWorkspace]) -> ReconnectSummary {
    let live = win::enumerate_top_level_windows().unwrap_or_default();
    reconnect_workspaces_with_windows(workspaces, &live)
}

pub fn reconnect_workspaces_with_windows(
    workspaces: &mut [MmWorkspace],
    live: &[EnumeratedWindow],
) -> ReconnectSummary {
    let mut summary = ReconnectSummary::default();
    let mut assigned = HashSet::new();

    for window in workspaces
        .iter_mut()
        .flat_map(|workspace| &mut workspace.windows)
    {
        let (outcome, hwnd) = match_saved_window(window, live, &assigned);

        match outcome {
            ReconnectOutcome::AlreadyValid => {
                summary.already_valid += 1;
                window.valid = true;
            }
            ReconnectOutcome::Reconnected => {
                summary.reconnected += 1;
                window.valid = true;
            }
            ReconnectOutcome::Invalidated => {
                summary.invalidated += 1;
                if hwnd.is_some() {
                    summary.reconnected += 1;
                    window.valid = true;
                } else {
                    summary.missing += 1;
                    window.valid = false;
                }
            }
            ReconnectOutcome::Ambiguous => {
                summary.ambiguous += 1;
                window.valid = false;
            }
            ReconnectOutcome::Missing => {
                summary.missing += 1;
                window.valid = false;
            }
        }

        window.hwnd = hwnd.unwrap_or(0);
        if window.hwnd != 0 {
            assigned.insert(window.hwnd);
        }
    }

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

    fn workspace(window: MmWindow) -> Vec<MmWorkspace> {
        vec![MmWorkspace {
            windows: vec![window],
            ..MmWorkspace::default()
        }]
    }

    #[test]
    fn unique_title_only_legacy_reconnect() {
        let mut workspaces = workspace(MmWindow {
            title: " Notes ".into(),
            valid: false,
            ..MmWindow::default()
        });
        let summary =
            reconnect_workspaces_with_windows(&mut workspaces, &[live(10, "notes", "", "", "")]);
        assert_eq!(summary.reconnected, 1);
        assert_eq!(workspaces[0].windows[0].hwnd, 10);
        assert!(workspaces[0].windows[0].valid);
    }

    #[test]
    fn duplicate_title_only_candidates_are_ambiguous() {
        let mut workspaces = workspace(MmWindow {
            title: "Notes".into(),
            ..MmWindow::default()
        });
        let summary = reconnect_workspaces_with_windows(
            &mut workspaces,
            &[
                live(10, "Notes", "", "", ""),
                live(11, " notes ", "", "", ""),
            ],
        );
        assert_eq!(summary.ambiguous, 1);
        assert_eq!(workspaces[0].windows[0].hwnd, 0);
        assert!(!workspaces[0].windows[0].valid);
    }

    #[test]
    fn process_path_class_title_strong_match() {
        let mut workspaces = workspace(MmWindow {
            title: "Doc".into(),
            class_name: "Editor".into(),
            process_path: "C:/Apps/Edit.exe".into(),
            ..MmWindow::default()
        });
        let live_windows = [
            live(10, "Doc", "other.exe", "Other", "C:/Other.exe"),
            live(12, " doc ", "edit.exe", "editor", "c:/apps/edit.exe"),
        ];
        let summary = reconnect_workspaces_with_windows(&mut workspaces, &live_windows);
        assert_eq!(summary.reconnected, 1);
        assert_eq!(workspaces[0].windows[0].hwnd, 12);
    }

    #[test]
    fn stale_hwnd_invalidated() {
        let mut workspaces = workspace(MmWindow {
            hwnd: 5,
            title: "Doc".into(),
            executable: "edit.exe".into(),
            ..MmWindow::default()
        });
        let summary = reconnect_workspaces_with_windows(
            &mut workspaces,
            &[live(5, "Other", "edit.exe", "", "")],
        );
        assert_eq!(summary.invalidated, 1);
        assert_eq!(summary.missing, 1);
        assert_eq!(workspaces[0].windows[0].hwnd, 0);
        assert!(!workspaces[0].windows[0].valid);
    }

    #[test]
    fn valid_matching_hwnd_preserved() {
        let mut workspaces = workspace(MmWindow {
            hwnd: 5,
            title: "Doc".into(),
            executable: "Edit.EXE".into(),
            ..MmWindow::default()
        });
        let summary = reconnect_workspaces_with_windows(
            &mut workspaces,
            &[live(5, " doc ", "edit.exe", "", "")],
        );
        assert_eq!(summary.already_valid, 1);
        assert_eq!(workspaces[0].windows[0].hwnd, 5);
        assert!(workspaces[0].windows[0].valid);
    }

    #[test]
    fn same_hwnd_cannot_be_assigned_twice() {
        let mut workspaces = vec![MmWorkspace {
            windows: vec![
                MmWindow {
                    title: "Doc".into(),
                    ..MmWindow::default()
                },
                MmWindow {
                    title: "Doc".into(),
                    ..MmWindow::default()
                },
            ],
            ..MmWorkspace::default()
        }];
        let summary =
            reconnect_workspaces_with_windows(&mut workspaces, &[live(8, "Doc", "", "", "")]);
        assert_eq!(summary.reconnected, 1);
        assert_eq!(summary.missing, 1);
        assert_eq!(workspaces[0].windows[0].hwnd, 8);
        assert_eq!(workspaces[0].windows[1].hwnd, 0);
    }

    #[test]
    fn legacy_saved_window_with_only_title_still_reconnects() {
        let mut workspaces = workspace(MmWindow {
            alias: "Old".into(),
            title: "Legacy App".into(),
            ..MmWindow::default()
        });
        let summary = reconnect_workspaces_with_windows(
            &mut workspaces,
            &[live(77, "legacy app", "app.exe", "Class", "C:/app.exe")],
        );
        assert_eq!(summary.reconnected, 1);
        assert_eq!(workspaces[0].windows[0].hwnd, 77);
    }
}
