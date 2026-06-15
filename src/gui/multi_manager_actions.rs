use super::*;
use crate::multi_manager::apply_capture::{self, ApplyCaptureResult};
use crate::multi_manager::bindings;
use crate::multi_manager::capture;
use crate::multi_manager::model::{PendingCaptureAction, RecaptureQueueItem};
use crate::multi_manager::win::{self, CaptureKeyAction, CapturedWindow};
use std::sync::atomic::Ordering;

impl LauncherApp {
    pub fn open_multi_manager(&mut self) {
        self.multi_manager_dialog.open = true;
    }

    pub fn open_multi_manager_settings(&mut self) {
        self.multi_manager_settings_dialog.open = true;
    }

    pub fn multi_manager_save(&mut self) {
        match self.multi_manager.save() {
            Ok(()) => self.add_success_toast("Saved MultiManager workspaces"),
            Err(err) => self.report_error_message(
                "multi_manager.save",
                format!("Failed to save MultiManager workspaces: {err}"),
            ),
        }
    }

    pub fn multi_manager_reload(&mut self) {
        match self.multi_manager.reload() {
            Ok(()) => self.add_success_toast("Reloaded MultiManager workspaces"),
            Err(err) => self.report_error_message(
                "multi_manager.reload",
                format!("Failed to reload MultiManager workspaces: {err}"),
            ),
        }
    }

    pub fn multi_manager_import(&mut self) {
        self.report_error_message(
            "multi_manager.import",
            "MultiManager import needs a source path and is not wired to the launcher yet",
        );
    }

    pub fn multi_manager_start_recapture_all(&mut self) {
        let queue = self
            .multi_manager
            .workspaces
            .lock()
            .ok()
            .map(|workspaces| build_recapture_queue(&workspaces));
        let Some(queue) = queue else {
            self.report_error_message(
                "multi_manager.recapture",
                "Failed to lock MultiManager workspaces for recapture",
            );
            return;
        };

        if queue.is_empty() {
            self.report_error_message(
                "multi_manager.recapture",
                "No invalid or missing MultiManager window bindings to recapture",
            );
            return;
        }

        self.multi_manager.recapture_queue = queue.into();
        self.multi_manager.recapture_active = true;
        self.add_success_toast("Started MultiManager window recapture queue");
    }

    pub fn multi_manager_save_bindings(&mut self) {
        let result = self
            .multi_manager
            .workspaces
            .lock()
            .map_err(|_| anyhow::anyhow!("MultiManager workspace lock poisoned"))
            .and_then(|workspaces| {
                bindings::save_bindings(&self.multi_manager.bindings_path, &workspaces)
            });
        match result {
            Ok(()) => self.add_success_toast("Saved MultiManager window bindings"),
            Err(err) => self.report_error_message(
                "multi_manager.bindings",
                format!("Failed to save bindings: {err}"),
            ),
        }
    }

    pub fn multi_manager_restore_bindings(&mut self) {
        match bindings::load_bindings(&self.multi_manager.bindings_path) {
            Ok(snapshots) => {
                let restored = {
                    match self.multi_manager.workspaces.lock() {
                        Ok(mut workspaces) => {
                            bindings::restore_bindings(&mut workspaces, &snapshots);
                            Ok(())
                        }
                        Err(_) => Err(()),
                    }
                };
                match restored {
                    Ok(()) => {
                        self.multi_manager.mark_dirty();
                        self.add_success_toast("Restored MultiManager window bindings");
                    }
                    Err(()) => self.report_error_message(
                        "multi_manager.bindings",
                        "Failed to lock workspaces for binding restore",
                    ),
                }
            }
            Err(err) => self.report_error_message(
                "multi_manager.bindings",
                format!("Failed to load bindings: {err}"),
            ),
        }
    }

    pub fn multi_manager_refresh_titles(&mut self) {
        let changed = {
            match self.multi_manager.workspaces.lock() {
                Ok(mut workspaces) => Ok(bindings::refresh_titles(&mut workspaces)),
                Err(_) => Err(()),
            }
        };
        match changed {
            Ok(true) => {
                self.multi_manager.mark_dirty();
                self.add_success_toast("Refreshed MultiManager window titles");
            }
            Ok(false) => self.add_success_toast("MultiManager window titles already current"),
            Err(()) => self.report_error_message(
                "multi_manager.titles",
                "Failed to lock workspaces for title refresh",
            ),
        }
    }

    pub fn multi_manager_toggle_workspace(&mut self, workspace_id: &str) {
        if self
            .multi_manager
            .with_workspace_mut(
                workspace_id,
                crate::multi_manager::runtime::toggle_workspace,
            )
            .is_some()
        {
            self.add_success_toast("Toggled MultiManager workspace");
        } else {
            self.report_error_message(
                "multi_manager.toggle",
                format!("Failed to toggle MultiManager workspace: {workspace_id}"),
            );
        }
    }

    pub fn multi_manager_send_home(&mut self, workspace_id: &str) {
        self.multi_manager_move_workspace(workspace_id, true);
    }

    pub fn multi_manager_send_target(&mut self, workspace_id: &str) {
        self.multi_manager_move_workspace(workspace_id, false);
    }

    pub fn multi_manager_start_capture(&mut self, workspace_id: &str) {
        if self.multi_manager_workspace_exists(workspace_id) {
            self.multi_manager.pending_capture = Some(PendingCaptureAction::CaptureOneWindow {
                workspace_id: workspace_id.to_string(),
            });
            self.multi_manager.capture_session = None;
            self.multi_manager
                .runtime
                .control
                .capture_pending
                .store(true, Ordering::Relaxed);
            self.add_success_toast("Started MultiManager active window capture");
        } else {
            self.report_error_message(
                "multi_manager.capture",
                format!(
                    "Failed to start MultiManager capture: workspace not found: {workspace_id}"
                ),
            );
        }
    }

    pub fn multi_manager_start_capture_one(&mut self, workspace_id: &str, ctx: &egui::Context) {
        self.multi_manager_start_capture_session(
            workspace_id,
            None,
            PendingCaptureAction::CaptureOneWindow {
                workspace_id: workspace_id.to_string(),
            },
            ctx,
            "Started MultiManager active window capture",
        );
    }

    pub fn multi_manager_start_capture_multiple(
        &mut self,
        workspace_id: &str,
        ctx: &egui::Context,
    ) {
        self.multi_manager_start_capture_session(
            workspace_id,
            None,
            PendingCaptureAction::CaptureMultipleWindows {
                workspace_id: workspace_id.to_string(),
            },
            ctx,
            "Started MultiManager multi-window capture",
        );
    }

    pub fn multi_manager_start_recapture_window(
        &mut self,
        workspace_id: &str,
        index: usize,
        ctx: &egui::Context,
    ) {
        self.multi_manager_start_capture_session(
            workspace_id,
            Some(index),
            PendingCaptureAction::RecaptureWindow {
                workspace_id: workspace_id.to_string(),
                window_index: index,
            },
            ctx,
            "Started MultiManager window recapture",
        );
    }

    fn multi_manager_start_capture_session(
        &mut self,
        workspace_id: &str,
        window_index: Option<usize>,
        action: PendingCaptureAction,
        ctx: &egui::Context,
        success_message: &str,
    ) {
        let valid = self
            .multi_manager
            .workspaces
            .lock()
            .map(|workspaces| {
                workspaces
                    .iter()
                    .find(|workspace| workspace.id == workspace_id)
                    .is_some_and(|workspace| {
                        window_index.is_none_or(|index| index < workspace.windows.len())
                    })
            })
            .unwrap_or(false);

        if !valid {
            self.report_error_message(
                "multi_manager.capture",
                match window_index {
                    Some(index) => format!(
                        "Failed to start MultiManager recapture: window {} not found in workspace {workspace_id}",
                        index + 1
                    ),
                    None => format!(
                        "Failed to start MultiManager capture: workspace not found: {workspace_id}"
                    ),
                },
            );
            return;
        }

        self.multi_manager.pending_capture = Some(action);
        self.multi_manager.capture_session = None;
        self.multi_manager
            .runtime
            .control
            .capture_pending
            .store(true, Ordering::Relaxed);
        self.multi_manager.capture_session = Some(capture::start_capture_session(ctx.clone()));
        self.add_success_toast(success_message);
    }

    pub fn multi_manager_set_workspace_disabled(&mut self, workspace_id: &str, disabled: bool) {
        if self
            .multi_manager
            .with_workspace_mut(workspace_id, |workspace| workspace.disabled = disabled)
            .is_some()
        {
            let verb = if disabled { "Disabled" } else { "Enabled" };
            self.add_success_toast(format!("{verb} MultiManager workspace"));
        } else {
            self.report_error_message(
                "multi_manager.enable",
                format!("Failed to update MultiManager workspace: {workspace_id}"),
            );
        }
    }

    fn multi_manager_move_workspace(&mut self, workspace_id: &str, home: bool) {
        let workspace = match self.multi_manager.workspaces.lock() {
            Ok(workspaces) => workspaces
                .iter()
                .find(|workspace| workspace.id == workspace_id)
                .cloned(),
            Err(_) => None,
        };
        let Some(workspace) = workspace else {
            self.report_error_message(
                "multi_manager.move",
                format!("Failed to move MultiManager workspace: {workspace_id}"),
            );
            return;
        };
        if home {
            crate::multi_manager::runtime::send_workspace_home(&workspace);
            self.add_success_toast("Sent MultiManager workspace home");
        } else {
            crate::multi_manager::runtime::send_workspace_target(&workspace);
            self.add_success_toast("Sent MultiManager workspace to target");
        }
    }

    pub fn multi_manager_cancel_capture(&mut self) {
        self.multi_manager.capture_session = None;
        self.multi_manager.pending_capture = None;
        self.multi_manager.recapture_active = false;
        self.multi_manager.recapture_queue.clear();
        self.multi_manager
            .runtime
            .control
            .capture_pending
            .store(false, Ordering::Relaxed);
    }

    fn is_launcher_capture(&self, captured: &crate::multi_manager::win::CapturedWindow) -> bool {
        self.launcher_hwnd == Some(captured.hwnd)
            || captured.title.contains("Multi Lnchr")
            || captured.title.contains("Multi Launcher")
    }

    pub fn multi_manager_poll_capture(&mut self, ctx: &eframe::egui::Context) {
        if self.multi_manager.pending_capture.is_none() && self.multi_manager.recapture_active {
            if let Some(item) = self.multi_manager.recapture_queue.pop_front() {
                self.multi_manager.pending_capture = Some(PendingCaptureAction::RecaptureWindow {
                    workspace_id: item.workspace_id,
                    window_index: item.window_index,
                });
                self.multi_manager
                    .runtime
                    .control
                    .capture_pending
                    .store(true, Ordering::Relaxed);
            } else {
                self.multi_manager.recapture_active = false;
                self.multi_manager
                    .runtime
                    .control
                    .capture_pending
                    .store(false, Ordering::Relaxed);
            }
        }
        if self.multi_manager.pending_capture.is_none() {
            self.multi_manager.capture_session = None;
            return;
        }
        if self.multi_manager.capture_session.is_none() {
            self.multi_manager.capture_session = Some(capture::start_capture_session(ctx.clone()));
        }

        let event = self
            .multi_manager
            .capture_session
            .as_ref()
            .and_then(|session| session.rx.try_recv().ok());
        if let Some(event) = event {
            self.multi_manager.capture_session = None;
            match event.action {
                CaptureKeyAction::Cancel => self.multi_manager_cancel_capture(),
                CaptureKeyAction::Skip => {
                    if self.multi_manager.recapture_active {
                        self.multi_manager.pending_capture = None;
                    }
                }
                CaptureKeyAction::Confirm => self.multi_manager_complete_capture(event.captured),
            }
        }
    }

    fn multi_manager_complete_capture(&mut self, captured: Option<CapturedWindow>) {
        let Some(action) = self.multi_manager.pending_capture.clone() else {
            return;
        };
        let Some(captured) = captured else {
            self.report_error_message("multi_manager.capture", "No active window to capture");
            return;
        };
        if self
            .multi_manager_settings
            .ignore_launcher_window_on_capture
            && self.is_launcher_capture(&captured)
        {
            self.report_error_message(
                "multi_manager.capture",
                "Ignoring launcher window; focus another window and press Enter",
            );
            return;
        }
        let keep_pending = matches!(action, PendingCaptureAction::CaptureMultipleWindows { .. });
        let result = self
            .multi_manager
            .workspaces
            .lock()
            .map(|mut workspaces| {
                apply_capture::apply_capture_to_workspaces(&mut workspaces, &action, captured)
            })
            .unwrap_or(ApplyCaptureResult::MissingWorkspace);
        match result {
            ApplyCaptureResult::Applied => self.multi_manager.mark_dirty(),
            ApplyCaptureResult::MissingWorkspace => {
                self.report_error_message(
                    "multi_manager.capture",
                    "Failed to apply capture: workspace not found",
                );
                return;
            }
            ApplyCaptureResult::MissingWindow => {
                self.report_error_message(
                    "multi_manager.capture",
                    "Failed to apply capture: window not found",
                );
                return;
            }
        }
        self.multi_manager.capture_session = None;
        if !keep_pending {
            self.multi_manager.pending_capture = None;
        }
        if !self.multi_manager.recapture_active && !keep_pending {
            self.multi_manager
                .runtime
                .control
                .capture_pending
                .store(false, Ordering::Relaxed);
        }
    }

    fn multi_manager_workspace_exists(&self, workspace_id: &str) -> bool {
        self.multi_manager
            .workspaces
            .lock()
            .map(|workspaces| {
                workspaces
                    .iter()
                    .any(|workspace| workspace.id == workspace_id)
            })
            .unwrap_or(false)
    }

    pub(crate) fn add_success_toast(&mut self, msg: impl Into<String>) {
        if self.enable_toasts {
            self.add_toast(Toast {
                text: msg.into().into(),
                kind: ToastKind::Success,
                options: ToastOptions::default().duration_in_seconds(self.toast_duration as f64),
            });
        }
    }
}

pub(crate) fn build_recapture_queue(
    workspaces: &[crate::multi_manager::model::MmWorkspace],
) -> Vec<RecaptureQueueItem> {
    workspaces
        .iter()
        .flat_map(|workspace| {
            workspace
                .windows
                .iter()
                .enumerate()
                .filter_map(|(window_index, window)| {
                    (window.hwnd == 0 || !win::is_valid_window(window.hwnd)).then(|| {
                        RecaptureQueueItem {
                            workspace_id: workspace.id.clone(),
                            window_index,
                        }
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::build_recapture_queue;
    use crate::gui::LauncherApp;
    use crate::multi_manager::model::{MmRect, MmWindow, MmWorkspace, PendingCaptureAction};
    use crate::multi_manager::win::CapturedWindow;
    use crate::plugin::PluginManager;
    use crate::settings::Settings;
    use std::collections::VecDeque;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    fn test_app() -> LauncherApp {
        let ctx = eframe::egui::Context::default();
        let tempdir = tempfile::tempdir().expect("tempdir");
        let path = tempdir.path().join("actions.json");
        std::fs::write(&path, "[]").expect("write actions file");
        LauncherApp::new(
            &ctx,
            Arc::new(Vec::new()),
            0,
            PluginManager::new(),
            path.to_string_lossy().to_string(),
            tempdir
                .path()
                .join("settings.json")
                .to_string_lossy()
                .to_string(),
            Settings::default(),
            None,
            None,
            None,
            None,
            Arc::new(AtomicBool::new(true)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        )
    }

    fn captured(hwnd: usize, title: &str) -> CapturedWindow {
        CapturedWindow {
            hwnd,
            title: title.to_string(),
            rect: MmRect {
                x: 0,
                y: 0,
                w: 100,
                h: 100,
            },
        }
    }

    #[test]
    fn recapture_queue_skips_cancels_and_advances() {
        let workspaces = vec![MmWorkspace {
            id: "w".into(),
            windows: vec![
                MmWindow {
                    hwnd: 0,
                    ..Default::default()
                },
                MmWindow {
                    hwnd: 0,
                    ..Default::default()
                },
            ],
            ..Default::default()
        }];
        let mut queue: VecDeque<_> = build_recapture_queue(&workspaces).into();
        assert_eq!(queue.pop_front().unwrap().window_index, 0); // current item skipped
        assert_eq!(queue.pop_front().unwrap().window_index, 1); // queue advanced
        queue.clear(); // Escape cancels remaining queue
        assert!(queue.is_empty());
    }

    fn set_workspaces(app: &mut LauncherApp, workspaces: Vec<MmWorkspace>) {
        *app.multi_manager.workspaces.lock().expect("workspaces") = workspaces;
    }

    #[test]
    fn one_window_capture_clears_pending_state_after_success() {
        let mut app = test_app();
        set_workspaces(
            &mut app,
            vec![MmWorkspace {
                id: "w".into(),
                ..Default::default()
            }],
        );
        app.multi_manager.pending_capture = Some(PendingCaptureAction::CaptureOneWindow {
            workspace_id: "w".into(),
        });
        app.multi_manager
            .runtime
            .control
            .capture_pending
            .store(true, Ordering::Relaxed);

        app.multi_manager_complete_capture(Some(captured(7, "Editor")));

        assert_eq!(app.multi_manager.pending_capture, None);
        assert!(
            !app.multi_manager
                .runtime
                .control
                .capture_pending
                .load(Ordering::Relaxed)
        );
        let workspaces = app.multi_manager.workspaces.lock().expect("workspaces");
        assert_eq!(workspaces[0].windows.len(), 1);
        assert_eq!(workspaces[0].windows[0].title, "Editor");
    }

    #[test]
    fn multi_window_capture_preserves_pending_state_across_confirms() {
        let mut app = test_app();
        set_workspaces(
            &mut app,
            vec![MmWorkspace {
                id: "w".into(),
                ..Default::default()
            }],
        );
        app.multi_manager.pending_capture = Some(PendingCaptureAction::CaptureMultipleWindows {
            workspace_id: "w".into(),
        });
        app.multi_manager
            .runtime
            .control
            .capture_pending
            .store(true, Ordering::Relaxed);

        app.multi_manager_complete_capture(Some(captured(1, "One")));
        app.multi_manager_complete_capture(Some(captured(2, "Two")));

        assert_eq!(
            app.multi_manager.pending_capture,
            Some(PendingCaptureAction::CaptureMultipleWindows {
                workspace_id: "w".into()
            })
        );
        assert!(
            app.multi_manager
                .runtime
                .control
                .capture_pending
                .load(Ordering::Relaxed)
        );
        let workspaces = app.multi_manager.workspaces.lock().expect("workspaces");
        assert_eq!(workspaces[0].windows.len(), 2);
        assert_eq!(workspaces[0].windows[0].title, "One");
        assert_eq!(workspaces[0].windows[1].title, "Two");
    }

    #[test]
    fn recapture_updates_existing_index_without_appending() {
        let mut app = test_app();
        set_workspaces(
            &mut app,
            vec![MmWorkspace {
                id: "w".into(),
                windows: vec![
                    MmWindow {
                        title: "Keep".into(),
                        alias: "Keep".into(),
                        hwnd: 10,
                        ..Default::default()
                    },
                    MmWindow {
                        title: "Old".into(),
                        alias: "Old".into(),
                        hwnd: 11,
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
        );
        app.multi_manager.pending_capture = Some(PendingCaptureAction::RecaptureWindow {
            workspace_id: "w".into(),
            window_index: 1,
        });
        app.multi_manager
            .runtime
            .control
            .capture_pending
            .store(true, Ordering::Relaxed);

        app.multi_manager_complete_capture(Some(captured(22, "New")));

        let workspaces = app.multi_manager.workspaces.lock().expect("workspaces");
        assert_eq!(workspaces[0].windows.len(), 2);
        assert_eq!(workspaces[0].windows[0].title, "Keep");
        assert_eq!(workspaces[0].windows[1].title, "New");
        assert_eq!(workspaces[0].windows[1].hwnd, 22);
    }

    #[test]
    fn is_launcher_capture_matches_launcher_hwnd() {
        let mut app = test_app();
        app.launcher_hwnd = Some(42);

        assert!(app.is_launcher_capture(&captured(42, "Notepad")));
    }

    #[test]
    fn is_launcher_capture_matches_multi_lnchr_title() {
        let app = test_app();

        assert!(app.is_launcher_capture(&captured(100, "Multi Lnchr")));
    }

    #[test]
    fn is_launcher_capture_matches_multi_launcher_title() {
        let app = test_app();

        assert!(app.is_launcher_capture(&captured(100, "Multi Launcher")));
    }

    #[test]
    fn is_launcher_capture_ignores_unrelated_capture() {
        let mut app = test_app();
        app.launcher_hwnd = Some(42);

        assert!(!app.is_launcher_capture(&captured(100, "Notepad")));
    }
}
