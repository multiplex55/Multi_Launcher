use crate::multi_manager::model::{new_workspace_id, MmWorkspace};
use anyhow::{Context, Result};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

pub fn load_workspaces(path: &Path) -> Result<Vec<MmWorkspace>> {
    let content = fs::read_to_string(path).with_context(|| {
        format!(
            "failed to read MultiManager workspaces from {}",
            path.display()
        )
    })?;
    let mut workspaces: Vec<MmWorkspace> = serde_json::from_str(&content).with_context(|| {
        format!(
            "failed to parse MultiManager workspaces from {}",
            path.display()
        )
    })?;
    normalize_workspaces(&mut workspaces);
    Ok(workspaces)
}

pub fn save_workspaces(path: &Path, workspaces: &[MmWorkspace]) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create MultiManager directory {}",
                parent.display()
            )
        })?;
    }
    let tmp_path = tmp_path_for(path);
    let json = serde_json::to_vec_pretty(workspaces)
        .context("failed to serialize MultiManager workspaces")?;
    {
        let mut file = File::create(&tmp_path).with_context(|| {
            format!(
                "failed to create temporary workspace file {}",
                tmp_path.display()
            )
        })?;
        file.write_all(&json).with_context(|| {
            format!(
                "failed to write temporary workspace file {}",
                tmp_path.display()
            )
        })?;
        file.flush().with_context(|| {
            format!(
                "failed to flush temporary workspace file {}",
                tmp_path.display()
            )
        })?;
        file.sync_all().with_context(|| {
            format!(
                "failed to sync temporary workspace file {}",
                tmp_path.display()
            )
        })?;
    }
    fs::rename(&tmp_path, path).with_context(|| {
        format!(
            "failed to atomically replace {} with {}",
            path.display(),
            tmp_path.display()
        )
    })?;
    Ok(())
}

pub fn load_or_default(path: &Path) -> Vec<MmWorkspace> {
    load_workspaces(path).unwrap_or_default()
}

pub fn import_old_manager_workspaces(path: &Path) -> Result<Vec<MmWorkspace>> {
    load_workspaces(path)
}

fn normalize_workspaces(workspaces: &mut [MmWorkspace]) {
    for workspace in workspaces {
        if workspace.id.trim().is_empty() {
            workspace.id = new_workspace_id();
        }
        workspace.rotation_offset = 0;
    }
}

fn tmp_path_for(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_else(|| "multi_manager_workspaces.json".into());
    name.push(".tmp");
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::multi_manager::model::{MmHotkey, MmRect, MmWindow};

    #[test]
    fn old_tuple_rect_json_loads_correctly() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workspaces.json");
        fs::write(&path, r#"[{"id":"ws","name":"A","home_rect":[1,2,3,4],"windows":[{"target_rect":[5,6,7,8]}]}]"#).unwrap();
        let loaded = load_workspaces(&path).unwrap();
        assert_eq!(
            loaded[0].home_rect,
            Some(MmRect {
                x: 1,
                y: 2,
                w: 3,
                h: 4
            })
        );
        assert_eq!(
            loaded[0].windows[0].target_rect,
            Some(MmRect {
                x: 5,
                y: 6,
                w: 7,
                h: 8
            })
        );
    }

    #[test]
    fn old_workspace_json_loads_with_safe_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workspaces.json");
        fs::write(
            &path,
            r#"[{"name":"Legacy","windows":[{"title":"Notepad","home_rect":[0,0,640,480]}]}]"#,
        )
        .unwrap();

        let loaded = load_workspaces(&path).unwrap();

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "Legacy");
        assert!(!loaded[0].id.is_empty());
        assert!(loaded[0].valid);
        assert!(!loaded[0].disabled);
        assert_eq!(loaded[0].windows[0].title, "Notepad");
        assert!(loaded[0].windows[0].valid);
    }

    #[test]
    fn named_rect_json_loads_correctly() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workspaces.json");
        fs::write(
            &path,
            r#"[{"id":"ws","target_rect":{"x":9,"y":10,"w":11,"h":12}}]"#,
        )
        .unwrap();
        let loaded = load_workspaces(&path).unwrap();
        assert_eq!(
            loaded[0].target_rect,
            Some(MmRect {
                x: 9,
                y: 10,
                w: 11,
                h: 12
            })
        );
    }

    #[test]
    fn missing_workspace_ids_are_generated() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workspaces.json");
        fs::write(&path, r#"[{"name":"Missing"},{"id":"","name":"Blank"}]"#).unwrap();
        let loaded = load_workspaces(&path).unwrap();
        assert!(loaded.iter().all(|workspace| !workspace.id.is_empty()));
        assert_ne!(loaded[0].id, loaded[1].id);
    }

    #[test]
    fn duplicate_workspace_names_receive_distinct_generated_ids() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workspaces.json");
        fs::write(
            &path,
            r#"[{"name":"New Workspace"},{"name":"New Workspace"},{"name":"New Workspace"}]"#,
        )
        .unwrap();

        let loaded = load_workspaces(&path).unwrap();

        assert_eq!(loaded.len(), 3);
        assert!(loaded
            .iter()
            .all(|workspace| workspace.name == "New Workspace" && !workspace.id.is_empty()));
        assert_ne!(loaded[0].id, loaded[1].id);
        assert_ne!(loaded[0].id, loaded[2].id);
        assert_ne!(loaded[1].id, loaded[2].id);
    }

    #[test]
    fn save_load_roundtrip_preserves_user_facing_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workspaces.json");
        let workspace = MmWorkspace {
            id: "stable-id".into(),
            name: "Main".into(),
            hotkey: Some(MmHotkey {
                key: "F9".into(),
                ctrl: true,
                shift: false,
                alt: true,
                win: false,
            }),
            aliases: vec!["main".into(), "work".into()],
            windows: vec![MmWindow {
                alias: "Editor".into(),
                title: "Editor".into(),
                executable: "code.exe".into(),
                class_name: "Chrome_WidgetWin_1".into(),
                process_path: "C:/Code/code.exe".into(),
                home_rect: Some(MmRect {
                    x: 1,
                    y: 2,
                    w: 3,
                    h: 4,
                }),
                target_rect: Some(MmRect {
                    x: 5,
                    y: 6,
                    w: 7,
                    h: 8,
                }),
                disabled: true,
                valid: false,
                hwnd: 0,
            }],
            home_rect: Some(MmRect {
                x: 10,
                y: 20,
                w: 30,
                h: 40,
            }),
            target_rect: Some(MmRect {
                x: 50,
                y: 60,
                w: 70,
                h: 80,
            }),
            disabled: true,
            valid: false,
            rotate: true,
            rotation_offset: 99,
        };
        save_workspaces(&path, std::slice::from_ref(&workspace)).unwrap();
        let loaded = load_workspaces(&path).unwrap();
        assert_eq!(loaded[0].id, workspace.id);
        assert_eq!(loaded[0].name, workspace.name);
        assert_eq!(loaded[0].hotkey, workspace.hotkey);
        assert_eq!(loaded[0].aliases, workspace.aliases);
        assert_eq!(loaded[0].windows, workspace.windows);
        assert_eq!(loaded[0].home_rect, workspace.home_rect);
        assert_eq!(loaded[0].target_rect, workspace.target_rect);
        assert_eq!(loaded[0].disabled, workspace.disabled);
        assert_eq!(loaded[0].valid, workspace.valid);
        assert_eq!(loaded[0].rotate, workspace.rotate);
    }

    #[test]
    fn rotation_offset_is_not_persisted_and_resets_on_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workspaces.json");
        let workspace = MmWorkspace {
            id: "ws".into(),
            rotation_offset: 42,
            ..MmWorkspace::default()
        };
        save_workspaces(&path, &[workspace]).unwrap();
        let raw = fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("rotation_offset"));
        let loaded = load_workspaces(&path).unwrap();
        assert_eq!(loaded[0].rotation_offset, 0);
    }

    #[test]
    fn atomic_save_leaves_valid_final_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workspaces.json");
        save_workspaces(
            &path,
            &[MmWorkspace {
                id: "first".into(),
                name: "First".into(),
                ..MmWorkspace::default()
            }],
        )
        .unwrap();
        save_workspaces(
            &path,
            &[MmWorkspace {
                id: "second".into(),
                name: "Second".into(),
                windows: vec![MmWindow {
                    title: "Window".into(),
                    ..MmWindow::default()
                }],
                ..MmWorkspace::default()
            }],
        )
        .unwrap();

        let raw = fs::read_to_string(&path).unwrap();
        let parsed: Vec<MmWorkspace> = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].id, "second");
        assert_eq!(load_workspaces(&path).unwrap()[0].name, "Second");
        assert!(!tmp_path_for(&path).exists());
    }

    #[test]
    fn invalid_or_missing_optional_fields_use_safe_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workspaces.json");
        fs::write(
            &path,
            r#"[{"id":"ws","home_rect":"bad","windows":[{"target_rect":{"bad":true}}]}]"#,
        )
        .unwrap();
        let loaded = load_workspaces(&path).unwrap();
        assert_eq!(loaded[0].home_rect, None);
        assert_eq!(loaded[0].windows[0].target_rect, None);
        assert!(loaded[0].valid);
        assert!(!loaded[0].disabled);
    }
}
