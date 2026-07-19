use crate::multi_manager::model::{MmWindow, MmWorkspace, PendingCaptureAction};
use crate::multi_manager::win::CapturedWindow;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyCaptureResult {
    Applied,
    MissingWorkspace,
    MissingWindow,
    DuplicateHwnd,
}

pub fn apply_capture_to_workspaces(
    workspaces: &mut [MmWorkspace],
    action: &PendingCaptureAction,
    captured: CapturedWindow,
) -> ApplyCaptureResult {
    match action {
        PendingCaptureAction::CaptureOneWindow { workspace_id }
        | PendingCaptureAction::CaptureMultipleWindows { workspace_id } => {
            let Some(workspace_index) = workspaces
                .iter()
                .position(|workspace| workspace.id == *workspace_id)
            else {
                return ApplyCaptureResult::MissingWorkspace;
            };

            if hwnd_is_in_use(workspaces, captured.hwnd, None) {
                return ApplyCaptureResult::DuplicateHwnd;
            }

            let mut window = MmWindow {
                alias: captured.title.clone(),
                captured_title: captured.title.clone(),
                live_title: captured.title,
                home_rect: Some(captured.rect),
                target_rect: Some(captured.rect),
                executable: captured.executable,
                class_name: captured.class_name,
                process_path: captured.process_path,
                ..Default::default()
            };
            window.mark_bound(captured.hwnd);
            workspaces[workspace_index].windows.push(window);
            ApplyCaptureResult::Applied
        }
        PendingCaptureAction::RecaptureWindow {
            workspace_id,
            window_index,
        } => {
            let Some(workspace_index) = workspaces
                .iter()
                .position(|workspace| workspace.id == *workspace_id)
            else {
                return ApplyCaptureResult::MissingWorkspace;
            };
            if workspaces[workspace_index]
                .windows
                .get(*window_index)
                .is_none()
            {
                return ApplyCaptureResult::MissingWindow;
            }
            if hwnd_is_in_use(
                workspaces,
                captured.hwnd,
                Some((workspace_index, *window_index)),
            ) {
                return ApplyCaptureResult::DuplicateHwnd;
            }

            let workspace = &mut workspaces[workspace_index];
            let Some(window) = workspace.windows.get_mut(*window_index) else {
                return ApplyCaptureResult::MissingWindow;
            };

            window.captured_title = captured.title.clone();
            window.live_title = captured.title.clone();
            window.executable = captured.executable;
            window.class_name = captured.class_name;
            window.process_path = captured.process_path;
            window.mark_bound(captured.hwnd);
            if window.alias.trim().is_empty() {
                window.alias = captured.title;
            }
            ApplyCaptureResult::Applied
        }
    }
}

fn hwnd_is_in_use(
    workspaces: &[MmWorkspace],
    hwnd: usize,
    allowed_location: Option<(usize, usize)>,
) -> bool {
    hwnd != 0
        && workspaces
            .iter()
            .enumerate()
            .flat_map(|(workspace_index, workspace)| {
                workspace
                    .windows
                    .iter()
                    .enumerate()
                    .map(move |(window_index, window)| (workspace_index, window_index, window))
            })
            .any(|(workspace_index, window_index, window)| {
                window.hwnd == hwnd && allowed_location != Some((workspace_index, window_index))
            })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::multi_manager::model::{MmBindingStatus, MmRect};

    fn workspace(id: &str) -> MmWorkspace {
        MmWorkspace {
            id: id.to_string(),
            ..Default::default()
        }
    }

    fn rect(x: i32, y: i32, w: i32, h: i32) -> MmRect {
        MmRect { x, y, w, h }
    }

    fn captured(hwnd: usize, captured_title: &str, rect: MmRect) -> CapturedWindow {
        captured_with_metadata(
            hwnd,
            captured_title,
            rect,
            &format!("{captured_title}.exe"),
            &format!("{captured_title}Class"),
            &format!("C:/Apps/{captured_title}.exe"),
        )
    }

    fn captured_with_metadata(
        hwnd: usize,
        captured_title: &str,
        rect: MmRect,
        executable: &str,
        class_name: &str,
        process_path: &str,
    ) -> CapturedWindow {
        CapturedWindow {
            hwnd,
            title: captured_title.to_string(),
            rect,
            executable: executable.to_string(),
            class_name: class_name.to_string(),
            process_path: process_path.to_string(),
        }
    }

    #[test]
    fn capture_one_window_appends_one_mm_window() {
        let capture_rect = rect(1, 2, 300, 400);
        let mut workspaces = vec![workspace("w")];
        let result = apply_capture_to_workspaces(
            &mut workspaces,
            &PendingCaptureAction::CaptureOneWindow {
                workspace_id: "w".into(),
            },
            captured(7, "Editor", capture_rect),
        );

        assert_eq!(result, ApplyCaptureResult::Applied);
        assert_eq!(workspaces[0].windows.len(), 1);
        let window = &workspaces[0].windows[0];
        assert_eq!(window.alias, "Editor");
        assert_eq!(window.captured_title, "Editor");
        assert_eq!(window.live_title, "Editor");
        assert_eq!(window.hwnd, 7);
        assert_eq!(window.binding_status, MmBindingStatus::Bound);
        assert!(window.binding_verified);
        assert!(window.can_activate());
        assert_eq!(window.home_rect, Some(capture_rect));
        assert_eq!(window.target_rect, Some(capture_rect));
        assert_eq!(window.executable, "Editor.exe");
        assert_eq!(window.class_name, "EditorClass");
        assert_eq!(window.process_path, "C:/Apps/Editor.exe");
        assert!(window.valid);
    }

    #[test]
    fn capture_multiple_windows_appends_one_mm_window_per_call() {
        let mut workspaces = vec![workspace("w")];
        let action = PendingCaptureAction::CaptureMultipleWindows {
            workspace_id: "w".into(),
        };

        assert_eq!(
            apply_capture_to_workspaces(
                &mut workspaces,
                &action,
                captured(1, "One", rect(0, 0, 10, 10))
            ),
            ApplyCaptureResult::Applied
        );
        assert_eq!(
            apply_capture_to_workspaces(
                &mut workspaces,
                &action,
                captured(2, "Two", rect(10, 10, 20, 20))
            ),
            ApplyCaptureResult::Applied
        );

        assert_eq!(workspaces[0].windows.len(), 2);
        assert_eq!(workspaces[0].windows[0].captured_title, "One");
        assert_eq!(workspaces[0].windows[1].captured_title, "Two");
    }

    #[test]
    fn recapturing_updates_hwnd_and_title() {
        let mut workspaces = vec![MmWorkspace {
            id: "w".into(),
            windows: vec![MmWindow {
                captured_title: "Old".into(),
                alias: "Alias".into(),
                hwnd: 1,
                valid: false,
                ..Default::default()
            }],
            ..Default::default()
        }];

        let result = apply_capture_to_workspaces(
            &mut workspaces,
            &PendingCaptureAction::RecaptureWindow {
                workspace_id: "w".into(),
                window_index: 0,
            },
            captured(99, "New", rect(0, 0, 50, 50)),
        );

        assert_eq!(result, ApplyCaptureResult::Applied);
        assert_eq!(workspaces[0].windows[0].hwnd, 99);
        assert_eq!(workspaces[0].windows[0].captured_title, "New");
        assert_eq!(workspaces[0].windows[0].live_title, "New");
        assert_eq!(
            workspaces[0].windows[0].binding_status,
            MmBindingStatus::Bound
        );
        assert!(workspaces[0].windows[0].binding_verified);
        assert!(workspaces[0].windows[0].can_activate());
        assert_eq!(workspaces[0].windows[0].executable, "New.exe");
        assert_eq!(workspaces[0].windows[0].class_name, "NewClass");
        assert_eq!(workspaces[0].windows[0].process_path, "C:/Apps/New.exe");
        assert!(workspaces[0].windows[0].valid);
    }

    #[test]
    fn recapturing_preserves_existing_home_and_target_rectangles() {
        let home = rect(1, 1, 100, 100);
        let target = rect(2, 2, 200, 200);
        let mut workspaces = vec![MmWorkspace {
            id: "w".into(),
            windows: vec![MmWindow {
                home_rect: Some(home),
                target_rect: Some(target),
                ..Default::default()
            }],
            ..Default::default()
        }];

        let result = apply_capture_to_workspaces(
            &mut workspaces,
            &PendingCaptureAction::RecaptureWindow {
                workspace_id: "w".into(),
                window_index: 0,
            },
            captured(99, "New", rect(9, 9, 900, 900)),
        );

        assert_eq!(result, ApplyCaptureResult::Applied);
        assert_eq!(workspaces[0].windows[0].home_rect, Some(home));
        assert_eq!(workspaces[0].windows[0].target_rect, Some(target));
    }

    #[test]
    fn recapturing_preserves_rectangles_but_updates_identity_metadata() {
        let home = rect(10, 20, 300, 400);
        let target = rect(50, 60, 700, 800);
        let mut workspaces = vec![MmWorkspace {
            id: "w".into(),
            windows: vec![MmWindow {
                alias: "Stable Alias".into(),
                captured_title: "Old Title".into(),
                hwnd: 11,
                executable: "old.exe".into(),
                class_name: "OldClass".into(),
                process_path: "C:/Old/old.exe".into(),
                home_rect: Some(home),
                target_rect: Some(target),
                ..Default::default()
            }],
            ..Default::default()
        }];

        let result = apply_capture_to_workspaces(
            &mut workspaces,
            &PendingCaptureAction::RecaptureWindow {
                workspace_id: "w".into(),
                window_index: 0,
            },
            captured_with_metadata(
                22,
                "New Title",
                rect(1, 2, 3, 4),
                "new.exe",
                "NewClass",
                "C:/New/new.exe",
            ),
        );

        let window = &workspaces[0].windows[0];
        assert_eq!(result, ApplyCaptureResult::Applied);
        assert_eq!(window.alias, "Stable Alias");
        assert_eq!(window.hwnd, 22);
        assert_eq!(window.captured_title, "New Title");
        assert_eq!(window.live_title, "New Title");
        assert_eq!(window.executable, "new.exe");
        assert_eq!(window.class_name, "NewClass");
        assert_eq!(window.process_path, "C:/New/new.exe");
        assert_eq!(window.home_rect, Some(home));
        assert_eq!(window.target_rect, Some(target));
        assert!(window.valid);
    }

    #[test]
    fn missing_workspace_returns_missing_workspace() {
        let mut workspaces = vec![workspace("other")];
        let result = apply_capture_to_workspaces(
            &mut workspaces,
            &PendingCaptureAction::CaptureOneWindow {
                workspace_id: "missing".into(),
            },
            captured(1, "Window", rect(0, 0, 1, 1)),
        );

        assert_eq!(result, ApplyCaptureResult::MissingWorkspace);
    }

    #[test]
    fn missing_recapture_index_returns_missing_window() {
        let mut workspaces = vec![workspace("w")];
        let result = apply_capture_to_workspaces(
            &mut workspaces,
            &PendingCaptureAction::RecaptureWindow {
                workspace_id: "w".into(),
                window_index: 1,
            },
            captured(1, "Window", rect(0, 0, 1, 1)),
        );

        assert_eq!(result, ApplyCaptureResult::MissingWindow);
    }

    #[test]
    fn recapture_only_fills_alias_when_existing_alias_is_blank() {
        let mut workspaces = vec![MmWorkspace {
            id: "w".into(),
            windows: vec![
                MmWindow {
                    alias: "Custom".into(),
                    ..Default::default()
                },
                MmWindow {
                    alias: " \t ".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }];

        apply_capture_to_workspaces(
            &mut workspaces,
            &PendingCaptureAction::RecaptureWindow {
                workspace_id: "w".into(),
                window_index: 0,
            },
            captured(1, "New A", rect(0, 0, 1, 1)),
        );
        apply_capture_to_workspaces(
            &mut workspaces,
            &PendingCaptureAction::RecaptureWindow {
                workspace_id: "w".into(),
                window_index: 1,
            },
            captured(2, "New B", rect(0, 0, 1, 1)),
        );

        assert_eq!(workspaces[0].windows[0].alias, "Custom");
        assert_eq!(workspaces[0].windows[1].alias, "New B");
    }

    #[test]
    fn capture_rejects_duplicate_hwnd_before_appending() {
        let mut workspaces = vec![MmWorkspace {
            id: "w".into(),
            windows: vec![MmWindow {
                hwnd: 7,
                captured_title: "Existing".into(),
                ..Default::default()
            }],
            ..Default::default()
        }];

        let result = apply_capture_to_workspaces(
            &mut workspaces,
            &PendingCaptureAction::CaptureOneWindow {
                workspace_id: "w".into(),
            },
            captured(7, "Duplicate", rect(0, 0, 1, 1)),
        );

        assert_eq!(result, ApplyCaptureResult::DuplicateHwnd);
        assert_eq!(workspaces[0].windows.len(), 1);
        assert_eq!(workspaces[0].windows[0].captured_title, "Existing");
    }

    #[test]
    fn recapture_rejects_hwnd_already_bound_to_another_window() {
        let mut workspaces = vec![MmWorkspace {
            id: "w".into(),
            windows: vec![
                MmWindow {
                    hwnd: 1,
                    captured_title: "First".into(),
                    ..Default::default()
                },
                MmWindow {
                    hwnd: 2,
                    captured_title: "Second".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }];

        let result = apply_capture_to_workspaces(
            &mut workspaces,
            &PendingCaptureAction::RecaptureWindow {
                workspace_id: "w".into(),
                window_index: 1,
            },
            captured(1, "Duplicate", rect(0, 0, 1, 1)),
        );

        assert_eq!(result, ApplyCaptureResult::DuplicateHwnd);
        assert_eq!(workspaces[0].windows[1].hwnd, 2);
        assert_eq!(workspaces[0].windows[1].captured_title, "Second");
    }
}
