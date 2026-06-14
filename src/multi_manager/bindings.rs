use crate::multi_manager::model::MmWorkspace;
use crate::multi_manager::win;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WorkspaceBindingSnapshot {
    pub workspace_id: Option<String>,
    pub workspace_name: String,
    #[serde(default)]
    pub windows: Vec<WindowBindingSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WindowBindingSnapshot {
    pub window_id: usize,
    pub window_index: usize,
    pub title: String,
    pub alias: Option<String>,
    pub hwnd: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateHwnd {
    pub hwnd: usize,
    pub locations: Vec<(usize, usize)>,
}

pub fn save_bindings(path: &Path, workspaces: &[MmWorkspace]) -> Result<()> {
    let snapshots = workspaces
        .iter()
        .map(|workspace| WorkspaceBindingSnapshot {
            workspace_id: (!workspace.id.is_empty()).then(|| workspace.id.clone()),
            workspace_name: workspace.name.clone(),
            windows: workspace
                .windows
                .iter()
                .enumerate()
                .filter(|(_, window)| window.hwnd != 0 && win::is_valid_window(window.hwnd))
                .map(|(index, window)| WindowBindingSnapshot {
                    window_id: index,
                    window_index: index,
                    title: window.title.clone(),
                    alias: (!window.alias.is_empty()).then(|| window.alias.clone()),
                    hwnd: window.hwnd,
                })
                .collect(),
        })
        .collect::<Vec<_>>();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    std::fs::write(path, serde_json::to_string_pretty(&snapshots)?)
        .with_context(|| format!("write {}", path.display()))
}

pub fn load_bindings(path: &Path) -> Result<Vec<WorkspaceBindingSnapshot>> {
    let data = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&data).with_context(|| format!("parse {}", path.display()))
}

pub fn restore_bindings(workspaces: &mut [MmWorkspace], snapshots: &[WorkspaceBindingSnapshot]) {
    restore_bindings_with_validator(workspaces, snapshots, win::is_valid_window);
}

fn restore_bindings_with_validator(
    workspaces: &mut [MmWorkspace],
    snapshots: &[WorkspaceBindingSnapshot],
    is_valid: impl Fn(usize) -> bool,
) {
    for snapshot in snapshots {
        let workspace_index = snapshot
            .workspace_id
            .as_deref()
            .and_then(|id| (!id.is_empty()).then_some(id))
            .and_then(|id| workspaces.iter().position(|workspace| workspace.id == id))
            .or_else(|| {
                workspaces
                    .iter()
                    .position(|workspace| workspace.name == snapshot.workspace_name)
            });
        let Some(workspace_index) = workspace_index else {
            continue;
        };
        let workspace = &mut workspaces[workspace_index];
        for window_snapshot in &snapshot.windows {
            if window_snapshot.hwnd == 0 || !is_valid(window_snapshot.hwnd) {
                continue;
            }
            if let Some(window_index) = find_window_index(workspace, window_snapshot) {
                workspace.windows[window_index].hwnd = window_snapshot.hwnd;
            }
        }
    }
}

fn find_window_index(workspace: &MmWorkspace, snapshot: &WindowBindingSnapshot) -> Option<usize> {
    if snapshot.window_id < workspace.windows.len() {
        return Some(snapshot.window_id);
    }
    if let Some(alias) = snapshot.alias.as_deref().filter(|alias| !alias.is_empty()) {
        if let Some(index) = workspace
            .windows
            .iter()
            .position(|window| window.alias == alias)
        {
            return Some(index);
        }
    }
    if !snapshot.title.is_empty() {
        if let Some(index) = workspace
            .windows
            .iter()
            .position(|window| window.title == snapshot.title)
        {
            return Some(index);
        }
    }
    (snapshot.window_index < workspace.windows.len()).then_some(snapshot.window_index)
}

pub fn duplicate_hwnds(workspaces: &[MmWorkspace]) -> Vec<DuplicateHwnd> {
    let mut map: HashMap<usize, Vec<(usize, usize)>> = HashMap::new();
    for (workspace_index, workspace) in workspaces.iter().enumerate() {
        for (window_index, window) in workspace.windows.iter().enumerate() {
            if window.hwnd != 0 {
                map.entry(window.hwnd)
                    .or_default()
                    .push((workspace_index, window_index));
            }
        }
    }
    let mut duplicates = map
        .into_iter()
        .filter_map(|(hwnd, locations)| {
            (locations.len() > 1).then_some(DuplicateHwnd { hwnd, locations })
        })
        .collect::<Vec<_>>();
    duplicates.sort_by_key(|duplicate| duplicate.hwnd);
    duplicates
}

pub fn refresh_titles_with(
    workspaces: &mut [MmWorkspace],
    is_valid: impl Fn(usize) -> bool,
    title: impl Fn(usize) -> Option<String>,
) -> bool {
    let mut changed = false;
    for workspace in workspaces {
        for window in &mut workspace.windows {
            if window.hwnd != 0 && is_valid(window.hwnd) {
                if let Some(new_title) = title(window.hwnd) {
                    if window.title != new_title {
                        window.title = new_title;
                        changed = true;
                    }
                }
            }
        }
    }
    changed
}

pub fn refresh_titles(workspaces: &mut [MmWorkspace]) -> bool {
    refresh_titles_with(workspaces, win::is_valid_window, win::window_title)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::multi_manager::model::{MmWindow, MmWorkspace};

    fn ws(id: &str, name: &str, windows: Vec<MmWindow>) -> MmWorkspace {
        MmWorkspace {
            id: id.into(),
            name: name.into(),
            windows,
            ..Default::default()
        }
    }
    fn win(alias: &str, title: &str, hwnd: usize) -> MmWindow {
        MmWindow {
            alias: alias.into(),
            title: title.into(),
            hwnd,
            ..Default::default()
        }
    }

    #[test]
    fn binding_save_omits_invalid_hwnds() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bindings.json");
        save_bindings(&path, &[ws("id", "name", vec![win("a", "t", 123)])]).unwrap();
        let loaded = load_bindings(&path).unwrap();
        assert!(loaded[0].windows.is_empty());
    }

    #[test]
    fn restore_prefers_workspace_id_over_name() {
        let mut workspaces = vec![
            ws("target", "same", vec![win("", "", 0)]),
            ws("other", "old", vec![win("", "", 0)]),
        ];
        let snapshots = vec![WorkspaceBindingSnapshot {
            workspace_id: Some("target".into()),
            workspace_name: "old".into(),
            windows: vec![WindowBindingSnapshot {
                hwnd: 7,
                ..Default::default()
            }],
        }];
        restore_bindings_with_validator(&mut workspaces, &snapshots, |_| true);
        assert_eq!(workspaces[0].windows[0].hwnd, 7);
        assert_eq!(workspaces[1].windows[0].hwnd, 0);
    }

    #[test]
    fn restore_falls_back_to_name_when_id_absent() {
        let mut workspaces = vec![ws("id", "name", vec![win("", "", 0)])];
        let snapshots = vec![WorkspaceBindingSnapshot {
            workspace_id: None,
            workspace_name: "name".into(),
            windows: vec![WindowBindingSnapshot {
                hwnd: 8,
                ..Default::default()
            }],
        }];
        restore_bindings_with_validator(&mut workspaces, &snapshots, |_| true);
        assert_eq!(workspaces[0].windows[0].hwnd, 8);
    }

    #[test]
    fn invalid_hwnds_are_not_restored() {
        let mut workspaces = vec![ws("id", "name", vec![win("", "", 0)])];
        let snapshots = vec![WorkspaceBindingSnapshot {
            workspace_id: Some("id".into()),
            workspace_name: "name".into(),
            windows: vec![WindowBindingSnapshot {
                hwnd: 9,
                ..Default::default()
            }],
        }];
        restore_bindings_with_validator(&mut workspaces, &snapshots, |_| false);
        assert_eq!(workspaces[0].windows[0].hwnd, 0);
    }

    #[test]
    fn duplicate_hwnd_detection_reports_all_duplicates() {
        let duplicates = duplicate_hwnds(&[
            ws("a", "a", vec![win("", "", 5)]),
            ws(
                "b",
                "b",
                vec![win("", "", 5), win("", "", 6), win("", "", 5)],
            ),
        ]);
        assert_eq!(duplicates[0].hwnd, 5);
        assert_eq!(duplicates[0].locations, vec![(0, 0), (1, 0), (1, 2)]);
    }

    #[test]
    fn title_refresh_preserves_aliases() {
        let mut workspaces = vec![ws("id", "name", vec![win("alias", "old", 4)])];
        assert!(refresh_titles_with(
            &mut workspaces,
            |_| true,
            |_| Some("new".into())
        ));
        assert_eq!(workspaces[0].windows[0].title, "new");
        assert_eq!(workspaces[0].windows[0].alias, "alias");
    }
}
