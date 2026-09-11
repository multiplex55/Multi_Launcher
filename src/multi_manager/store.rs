use crate::common::persistence::{self, LoadState, PersistenceError};
use crate::multi_manager::model::{MmWorkspace, new_workspace_id};
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use std::path::Path;
use std::sync::Mutex;

static WORKSPACE_TRANSACTION: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

pub fn load_workspace_state(
    path: &Path,
) -> std::result::Result<LoadState<Vec<MmWorkspace>>, PersistenceError> {
    let mut state = persistence::load_json::<Vec<MmWorkspace>>(path)?;
    if let LoadState::Loaded(workspaces) = &mut state {
        normalize_workspaces(workspaces);
    }
    Ok(state)
}

pub fn load_workspaces(path: &Path) -> Result<Vec<MmWorkspace>> {
    match load_workspace_state(path)? {
        LoadState::Missing | LoadState::Empty => Ok(Vec::new()),
        LoadState::Loaded(workspaces) => Ok(workspaces),
    }
}

pub fn save_workspaces(path: &Path, workspaces: &[MmWorkspace]) -> Result<()> {
    replace_workspaces(path, workspaces).map(|_| ())
}

/// Replace a workspace document only when the current on-disk document is
/// missing, empty, or valid. This prevents a stale editor/autosave snapshot
/// from silently overwriting corruption.
pub fn replace_workspaces(path: &Path, workspaces: &[MmWorkspace]) -> Result<Vec<MmWorkspace>> {
    let _transaction = WORKSPACE_TRANSACTION
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _ = load_workspace_state(path).with_context(|| {
        format!(
            "refusing to replace unreadable MultiManager workspaces at {}",
            path.display()
        )
    })?;
    let mut committed = workspaces.to_vec();
    normalize_workspaces(&mut committed);
    persistence::save_json_atomic(path, &committed)?;
    Ok(committed)
}

/// Serialize a read-modify-write transaction against the latest valid disk
/// document. Domain validation and mutation remain owned by MultiManager.
pub fn update_workspaces<R>(
    path: &Path,
    change: impl FnOnce(&mut Vec<MmWorkspace>) -> Result<R>,
) -> Result<(Vec<MmWorkspace>, R)> {
    let _transaction = WORKSPACE_TRANSACTION
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut workspaces = match load_workspace_state(path)? {
        LoadState::Missing | LoadState::Empty => Vec::new(),
        LoadState::Loaded(workspaces) => workspaces,
    };
    let result = change(&mut workspaces)?;
    normalize_workspaces(&mut workspaces);
    persistence::save_json_atomic(path, &workspaces)?;
    Ok((workspaces, result))
}

pub fn import_old_manager_workspaces(path: &Path) -> Result<Vec<MmWorkspace>> {
    match load_workspace_state(path)? {
        LoadState::Loaded(workspaces) => Ok(workspaces),
        LoadState::Missing => anyhow::bail!(
            "MultiManager workspace import source {} does not exist",
            path.display()
        ),
        LoadState::Empty => anyhow::bail!(
            "MultiManager workspace import source {} is empty",
            path.display()
        ),
    }
}

fn normalize_workspaces(workspaces: &mut [MmWorkspace]) {
    for workspace in workspaces {
        if workspace.id.trim().is_empty() {
            workspace.id = new_workspace_id();
        }
        workspace.rotation_offset = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::multi_manager::model::{MmHotkey, MmRect, MmWindow};

    #[test]
    fn old_tuple_rect_json_loads_correctly() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workspaces.json");
        std::fs::write(&path, r#"[{"id":"ws","name":"A","home_rect":[1,2,3,4],"windows":[{"target_rect":[5,6,7,8]}]}]"#).unwrap();
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
        std::fs::write(
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
        assert_eq!(loaded[0].windows[0].captured_title, "Notepad");
        assert!(loaded[0].windows[0].valid);
    }

    #[test]
    fn named_rect_json_loads_correctly() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workspaces.json");
        std::fs::write(
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
        std::fs::write(&path, r#"[{"name":"Missing"},{"id":"","name":"Blank"}]"#).unwrap();
        let loaded = load_workspaces(&path).unwrap();
        assert!(loaded.iter().all(|workspace| !workspace.id.is_empty()));
        assert_ne!(loaded[0].id, loaded[1].id);
    }

    #[test]
    fn duplicate_workspace_names_receive_distinct_generated_ids() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workspaces.json");
        std::fs::write(
            &path,
            r#"[{"name":"New Workspace"},{"name":"New Workspace"},{"name":"New Workspace"}]"#,
        )
        .unwrap();

        let loaded = load_workspaces(&path).unwrap();

        assert_eq!(loaded.len(), 3);
        assert!(
            loaded
                .iter()
                .all(|workspace| workspace.name == "New Workspace" && !workspace.id.is_empty())
        );
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
                captured_title: "Editor".into(),
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
                ..MmWindow::default()
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
            virtual_desktop: None,
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
        let raw = std::fs::read_to_string(&path).unwrap();
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
                    captured_title: "Window".into(),
                    ..MmWindow::default()
                }],
                ..MmWorkspace::default()
            }],
        )
        .unwrap();

        let raw = std::fs::read_to_string(&path).unwrap();
        let parsed: Vec<MmWorkspace> = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].id, "second");
        assert_eq!(load_workspaces(&path).unwrap()[0].name, "Second");
    }

    #[test]
    fn invalid_or_missing_optional_fields_use_safe_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workspaces.json");
        std::fs::write(
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

    #[test]
    fn typed_load_distinguishes_missing_empty_valid_malformed_and_unreadable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workspaces.json");
        assert_eq!(load_workspace_state(&path).unwrap(), LoadState::Missing);

        std::fs::write(&path, " \r\n\t").unwrap();
        assert_eq!(load_workspace_state(&path).unwrap(), LoadState::Empty);

        std::fs::write(&path, r#"[{"id":"valid","name":"Valid"}]"#).unwrap();
        assert!(matches!(
            load_workspace_state(&path).unwrap(),
            LoadState::Loaded(workspaces) if workspaces[0].id == "valid"
        ));

        std::fs::write(&path, "{broken").unwrap();
        assert!(matches!(
            load_workspace_state(&path),
            Err(PersistenceError::MalformedJson { .. })
        ));
        assert!(matches!(
            load_workspace_state(dir.path()),
            Err(PersistenceError::Read { .. })
        ));
    }

    #[test]
    fn malformed_document_rejects_replacement_and_retains_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workspaces.json");
        let original = b"{broken workspace bytes";
        std::fs::write(&path, original).unwrap();

        let result = replace_workspaces(
            &path,
            &[MmWorkspace {
                id: "replacement".into(),
                ..Default::default()
            }],
        );

        assert!(result.is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }

    #[test]
    fn missing_and_empty_documents_accept_first_replacement_and_create_parents() {
        for empty in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("nested/workspaces.json");
            if empty {
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(&path, "  \n").unwrap();
            }
            let committed = replace_workspaces(
                &path,
                &[MmWorkspace {
                    id: "first".into(),
                    name: "First".into(),
                    ..Default::default()
                }],
            )
            .unwrap();
            assert_eq!(committed[0].id, "first");
            assert_eq!(load_workspaces(&path).unwrap(), committed);
        }
    }

    #[test]
    fn concurrent_updates_preserve_non_conflicting_additions() {
        let dir = tempfile::tempdir().unwrap();
        let path = std::sync::Arc::new(dir.path().join("workspaces.json"));
        let mut threads = Vec::new();
        for id in ["one", "two"] {
            let path = std::sync::Arc::clone(&path);
            threads.push(std::thread::spawn(move || {
                update_workspaces(&path, |workspaces| {
                    workspaces.push(MmWorkspace {
                        id: id.into(),
                        name: id.into(),
                        ..Default::default()
                    });
                    Ok(())
                })
                .unwrap();
            }));
        }
        for thread in threads {
            thread.join().unwrap();
        }
        let loaded = load_workspaces(&path).unwrap();
        assert_eq!(loaded.len(), 2);
        assert!(loaded.iter().any(|workspace| workspace.id == "one"));
        assert!(loaded.iter().any(|workspace| workspace.id == "two"));
    }
}
