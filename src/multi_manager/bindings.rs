use crate::multi_manager::identity;
use crate::multi_manager::model::{MmWindow, MmWorkspace};
use crate::multi_manager::reconnect;
use crate::multi_manager::win::{self, EnumeratedWindow};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::ErrorKind;
use std::io::Write;
use std::path::{Path, PathBuf};

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
    #[serde(rename = "title")]
    pub captured_title: String,
    pub alias: Option<String>,
    pub hwnd: usize,
    pub executable: Option<String>,
    pub class_name: Option<String>,
    pub process_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateHwnd {
    pub hwnd: usize,
    pub locations: Vec<(usize, usize)>,
}

pub fn save_bindings(path: &Path, workspaces: &[MmWorkspace]) -> Result<()> {
    save_bindings_with_validator(path, workspaces, win::is_valid_window)
}

fn save_bindings_with_validator(
    path: &Path,
    workspaces: &[MmWorkspace],
    is_valid_window: impl Fn(usize) -> bool,
) -> Result<()> {
    let snapshots = binding_snapshots_with_validator(workspaces, is_valid_window);
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let tmp_path = tmp_path_for(path);
    let json = serde_json::to_vec_pretty(&snapshots)?;
    {
        let mut file = File::create(&tmp_path)
            .with_context(|| format!("create temporary binding file {}", tmp_path.display()))?;
        file.write_all(&json)
            .with_context(|| format!("write temporary binding file {}", tmp_path.display()))?;
        file.flush()
            .with_context(|| format!("flush temporary binding file {}", tmp_path.display()))?;
        file.sync_all()
            .with_context(|| format!("sync temporary binding file {}", tmp_path.display()))?;
    }
    fs::rename(&tmp_path, path).with_context(|| {
        format!(
            "atomically replace {} with {}",
            path.display(),
            tmp_path.display()
        )
    })
}

fn binding_snapshots_with_validator(
    workspaces: &[MmWorkspace],
    is_valid_window: impl Fn(usize) -> bool,
) -> Vec<WorkspaceBindingSnapshot> {
    workspaces
        .iter()
        .map(|workspace| WorkspaceBindingSnapshot {
            workspace_id: (!workspace.id.is_empty()).then(|| workspace.id.clone()),
            workspace_name: workspace.name.clone(),
            windows: workspace
                .windows
                .iter()
                .enumerate()
                .filter(|(_, window)| window.hwnd != 0 && is_valid_window(window.hwnd))
                .map(|(index, window)| WindowBindingSnapshot {
                    window_id: index,
                    window_index: index,
                    captured_title: window.captured_title.clone(),
                    alias: (!window.alias.is_empty()).then(|| window.alias.clone()),
                    hwnd: window.hwnd,
                    executable: non_empty_string(&window.executable),
                    class_name: non_empty_string(&window.class_name),
                    process_path: non_empty_string(&window.process_path),
                })
                .collect(),
        })
        .collect()
}

fn tmp_path_for(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_else(|| "multi_manager_bindings.json".into());
    name.push(".tmp");
    path.with_file_name(name)
}

pub fn load_bindings(path: &Path) -> Result<Vec<WorkspaceBindingSnapshot>> {
    let data = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&data).with_context(|| format!("parse {}", path.display()))
}

pub fn load_bindings_if_exists(path: &Path) -> Result<Option<Vec<WorkspaceBindingSnapshot>>> {
    match std::fs::read_to_string(path) {
        Ok(data) => serde_json::from_str(&data)
            .map(Some)
            .with_context(|| format!("parse {}", path.display())),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err).with_context(|| format!("read {}", path.display())),
    }
}

pub fn restore_bindings(workspaces: &mut [MmWorkspace], snapshots: &[WorkspaceBindingSnapshot]) {
    let mut live = win::enumerate_top_level_windows().unwrap_or_default();
    for window_snapshot in snapshots.iter().flat_map(|snapshot| &snapshot.windows) {
        if window_snapshot.hwnd == 0
            || live
                .iter()
                .any(|window| window.hwnd == window_snapshot.hwnd)
        {
            continue;
        }
        let direct = win::query_hwnd_identity(window_snapshot.hwnd);
        if direct.is_window {
            live.push(EnumeratedWindow {
                hwnd: direct.hwnd,
                title: direct.live_title,
                executable: direct.executable,
                class_name: direct.class_name,
                process_path: direct.process_path,
                rect: win::window_rect(window_snapshot.hwnd).unwrap_or(
                    crate::multi_manager::model::MmRect {
                        x: 0,
                        y: 0,
                        w: 0,
                        h: 0,
                    },
                ),
            });
        }
    }
    restore_bindings_with_windows(workspaces, snapshots, &live);
}

fn restore_bindings_with_windows(
    workspaces: &mut [MmWorkspace],
    snapshots: &[WorkspaceBindingSnapshot],
    live: &[EnumeratedWindow],
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
            let Some(window_index) = find_window_index(workspace, window_snapshot) else {
                continue;
            };
            let saved_window = window_from_snapshot(window_snapshot);
            let direct_match = live.iter().find(|candidate| {
                candidate.hwnd == window_snapshot.hwnd
                    && identity::stable_identity_matches_enumerated(&saved_window, candidate)
            });
            if let Some(candidate) = direct_match {
                workspace.windows[window_index].mark_reconnected(candidate.hwnd);
                workspace.windows[window_index].live_title = candidate.title.clone();
            } else if workspace.windows[window_index].hwnd == window_snapshot.hwnd {
                workspace.windows[window_index].mark_missing();
                workspace.windows[window_index].live_title.clear();
            }
        }
    }
}

fn find_window_index(workspace: &MmWorkspace, snapshot: &WindowBindingSnapshot) -> Option<usize> {
    if snapshot.window_id < workspace.windows.len() {
        return Some(snapshot.window_id);
    }
    if let Some(alias) = snapshot.alias.as_deref().filter(|alias| !alias.is_empty())
        && let Some(index) = workspace
            .windows
            .iter()
            .position(|window| window.alias == alias)
    {
        return Some(index);
    }
    if !snapshot.captured_title.is_empty()
        && let Some(index) = workspace
            .windows
            .iter()
            .position(|window| window.captured_title == snapshot.captured_title)
    {
        return Some(index);
    }
    (snapshot.window_index < workspace.windows.len()).then_some(snapshot.window_index)
}

fn non_empty_string(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.to_string())
}

fn window_from_snapshot(snapshot: &WindowBindingSnapshot) -> MmWindow {
    let mut window = MmWindow {
        alias: snapshot.alias.clone().unwrap_or_default(),
        captured_title: snapshot.captured_title.clone(),
        executable: snapshot.executable.clone().unwrap_or_default(),
        class_name: snapshot.class_name.clone().unwrap_or_default(),
        process_path: snapshot.process_path.clone().unwrap_or_default(),
        ..Default::default()
    };
    window.mark_bound(snapshot.hwnd);
    window
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
            if window.hwnd != 0
                && is_valid(window.hwnd)
                && let Some(new_title) = title(window.hwnd)
                && window.live_title != new_title
            {
                window.live_title = new_title;
                changed = true;
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
    fn win(alias: &str, captured_title: &str, hwnd: usize) -> MmWindow {
        MmWindow {
            alias: alias.into(),
            captured_title: captured_title.into(),
            hwnd,
            ..Default::default()
        }
    }

    fn live(
        hwnd: usize,
        captured_title: &str,
        executable: &str,
        class_name: &str,
        process_path: &str,
    ) -> EnumeratedWindow {
        EnumeratedWindow {
            hwnd,
            title: captured_title.into(),
            executable: executable.into(),
            class_name: class_name.into(),
            process_path: process_path.into(),
            rect: crate::multi_manager::model::MmRect {
                x: 0,
                y: 0,
                w: 100,
                h: 100,
            },
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
            ws("target", "same", vec![win("", "Doc", 0)]),
            ws("other", "old", vec![win("", "Doc", 0)]),
        ];
        let snapshots = vec![WorkspaceBindingSnapshot {
            workspace_id: Some("target".into()),
            workspace_name: "old".into(),
            windows: vec![WindowBindingSnapshot {
                hwnd: 7,
                captured_title: "Doc".into(),
                executable: Some("editor.exe".into()),
                class_name: Some("Editor".into()),
                process_path: Some("C:/Apps/editor.exe".into()),
                ..Default::default()
            }],
        }];
        restore_bindings_with_windows(
            &mut workspaces,
            &snapshots,
            &[live(7, "Doc", "editor.exe", "Editor", "C:/Apps/editor.exe")],
        );
        assert_eq!(workspaces[0].windows[0].hwnd, 7);
        assert_eq!(workspaces[1].windows[0].hwnd, 0);
    }

    #[test]
    fn restore_falls_back_to_name_when_id_absent() {
        let mut workspaces = vec![ws("id", "name", vec![win("", "Doc", 0)])];
        let snapshots = vec![WorkspaceBindingSnapshot {
            workspace_id: None,
            workspace_name: "name".into(),
            windows: vec![WindowBindingSnapshot {
                hwnd: 8,
                captured_title: "Doc".into(),
                executable: Some("editor.exe".into()),
                class_name: Some("Editor".into()),
                process_path: Some("C:/Apps/editor.exe".into()),
                ..Default::default()
            }],
        }];
        restore_bindings_with_windows(
            &mut workspaces,
            &snapshots,
            &[live(8, "Doc", "editor.exe", "Editor", "C:/Apps/editor.exe")],
        );
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
        restore_bindings_with_windows(&mut workspaces, &snapshots, &[]);
        assert_eq!(workspaces[0].windows[0].hwnd, 0);
    }

    #[test]
    fn old_snapshots_without_metadata_still_load() {
        let json = r#"[{
            "workspace_id": "id",
            "workspace_name": "name",
            "windows": [{
                "window_id": 0,
                "window_index": 0,
                "title": "Legacy",
                "alias": "legacy",
                "hwnd": 42
            }]
        }]"#;

        let snapshots: Vec<WorkspaceBindingSnapshot> = serde_json::from_str(json).unwrap();

        assert_eq!(snapshots[0].windows[0].executable, None);
        assert_eq!(snapshots[0].windows[0].class_name, None);
        assert_eq!(snapshots[0].windows[0].process_path, None);
    }

    #[test]
    fn reused_hwnd_with_mismatched_metadata_is_not_restored() {
        let mut workspaces = vec![ws("id", "name", vec![win("", "Doc", 44)])];
        let snapshots = vec![WorkspaceBindingSnapshot {
            workspace_id: Some("id".into()),
            workspace_name: "name".into(),
            windows: vec![WindowBindingSnapshot {
                hwnd: 44,
                captured_title: "Doc".into(),
                executable: Some("editor.exe".into()),
                class_name: Some("Editor".into()),
                process_path: Some("C:/Apps/editor.exe".into()),
                ..Default::default()
            }],
        }];

        restore_bindings_with_windows(
            &mut workspaces,
            &snapshots,
            &[live(44, "Other", "other.exe", "Other", "C:/Other.exe")],
        );

        assert_eq!(workspaces[0].windows[0].hwnd, 0);
        assert!(!workspaces[0].windows[0].valid);
    }

    #[test]
    fn direct_restore_accepts_saved_hwnd_when_metadata_matches_and_title_changed() {
        let mut workspaces = vec![ws("id", "name", vec![win("", "Doc", 44)])];
        let snapshots = vec![WorkspaceBindingSnapshot {
            workspace_id: Some("id".into()),
            workspace_name: "name".into(),
            windows: vec![WindowBindingSnapshot {
                hwnd: 44,
                captured_title: "Doc".into(),
                executable: Some("editor.exe".into()),
                class_name: Some("Editor".into()),
                process_path: Some("C:/Apps/editor.exe".into()),
                ..Default::default()
            }],
        }];

        restore_bindings_with_windows(
            &mut workspaces,
            &snapshots,
            &[live(
                44,
                "Renamed",
                "editor.exe",
                "Editor",
                "C:/Apps/editor.exe",
            )],
        );

        assert_eq!(workspaces[0].windows[0].hwnd, 44);
        assert!(workspaces[0].windows[0].valid);
        assert!(workspaces[0].windows[0].binding_verified);
        assert_eq!(workspaces[0].windows[0].live_title, "Renamed");
    }

    #[test]
    fn missing_binding_file_loads_as_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing-bindings.json");

        assert!(load_bindings_if_exists(&path).unwrap().is_none());
    }

    #[test]
    fn malformed_binding_json_is_parse_error_not_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bindings.json");
        std::fs::write(&path, "not json").unwrap();

        let err = load_bindings_if_exists(&path).expect_err("malformed JSON should be reported");

        assert!(err.to_string().contains("parse"));
    }

    #[test]
    fn fallback_reconnect_can_restore_safe_replacement() {
        let mut workspaces = vec![ws("id", "name", vec![win("", "Doc", 0)])];
        let snapshots = vec![WorkspaceBindingSnapshot {
            workspace_id: Some("id".into()),
            workspace_name: "name".into(),
            windows: vec![WindowBindingSnapshot {
                hwnd: 44,
                captured_title: "Doc".into(),
                executable: Some("editor.exe".into()),
                class_name: Some("Editor".into()),
                process_path: Some("C:/Apps/editor.exe".into()),
                ..Default::default()
            }],
        }];

        let live_windows = [
            live(44, "Other", "other.exe", "Other", "C:/Other.exe"),
            live(55, "Doc", "editor.exe", "Editor", "C:/Apps/editor.exe"),
        ];
        restore_bindings_with_windows(&mut workspaces, &snapshots, &live_windows);
        reconnect::reconnect_unresolved_workspaces_with_windows(&mut workspaces, &live_windows);

        assert_eq!(workspaces[0].windows[0].hwnd, 55);
        assert!(workspaces[0].windows[0].valid);
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
        assert_eq!(workspaces[0].windows[0].captured_title, "old");
        assert_eq!(workspaces[0].windows[0].live_title, "new");
        assert_eq!(workspaces[0].windows[0].alias, "alias");
    }
    #[test]
    fn cleared_hwnd_is_removed_from_next_snapshot() {
        let snapshots =
            binding_snapshots_with_validator(&[ws("id", "name", vec![win("a", "t", 0)])], |_| true);
        assert!(snapshots[0].windows.is_empty());
    }

    #[test]
    fn atomic_save_preserves_existing_snapshot_when_temp_create_fails() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bindings.json");
        std::fs::write(&path, "old complete snapshot").unwrap();
        std::fs::create_dir(path.with_file_name("bindings.json.tmp")).unwrap();

        let err = save_bindings_with_validator(
            &path,
            &[ws("id", "name", vec![win("a", "t", 123)])],
            |_| true,
        )
        .expect_err("temporary file creation should fail");

        assert!(err.to_string().contains("temporary binding file"));
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "old complete snapshot"
        );
    }
}
