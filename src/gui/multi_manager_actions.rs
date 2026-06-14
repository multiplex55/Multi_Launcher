use super::*;
use crate::multi_manager::model::{MmWindow, PendingCaptureAction, RecaptureQueueItem};
use crate::multi_manager::win::{self, CaptureKeyAction};
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
        let workspace_ids = self.multi_manager.workspaces.lock().ok().map(|workspaces| {
            workspaces
                .iter()
                .map(|workspace| workspace.id.clone())
                .collect::<Vec<_>>()
        });
        let Some(workspace_ids) = workspace_ids else {
            self.report_error_message(
                "multi_manager.recapture",
                "Failed to lock MultiManager workspaces for recapture",
            );
            return;
        };

        if workspace_ids.is_empty() {
            self.report_error_message(
                "multi_manager.recapture",
                "No MultiManager workspaces to recapture",
            );
            return;
        }

        self.multi_manager.recapture_queue = workspace_ids
            .into_iter()
            .map(|workspace_id| RecaptureQueueItem { workspace_id })
            .collect();
        self.multi_manager.recapture_active = true;
        self.add_success_toast("Started MultiManager recapture for all workspaces");
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
            self.multi_manager.pending_capture = Some(PendingCaptureAction::CaptureWorkspace {
                workspace_id: workspace_id.to_string(),
            });
            self.multi_manager
                .runtime
                .control
                .capture_pending
                .store(true, Ordering::Relaxed);
            self.add_success_toast("Started MultiManager workspace capture");
        } else {
            self.report_error_message(
                "multi_manager.capture",
                format!(
                    "Failed to start MultiManager capture: workspace not found: {workspace_id}"
                ),
            );
        }
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
        self.multi_manager.pending_capture = None;
        self.multi_manager.recapture_active = false;
        self.multi_manager.recapture_queue.clear();
        self.multi_manager
            .runtime
            .control
            .capture_pending
            .store(false, Ordering::Relaxed);
    }

    pub fn multi_manager_poll_capture(&mut self, ctx: &eframe::egui::Context) {
        if self.multi_manager.pending_capture.is_none() && self.multi_manager.recapture_active {
            if let Some(item) = self.multi_manager.recapture_queue.pop_front() {
                self.multi_manager.pending_capture =
                    Some(PendingCaptureAction::RecaptureWorkspace {
                        workspace_id: item.workspace_id,
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
            return;
        }
        ctx.request_repaint();
        match win::poll_capture_keys() {
            Some(CaptureKeyAction::Cancel) => self.multi_manager_cancel_capture(),
            Some(CaptureKeyAction::Skip) => {
                if self.multi_manager.recapture_active {
                    self.multi_manager.pending_capture = None;
                }
            }
            Some(CaptureKeyAction::Confirm) => self.multi_manager_complete_capture(),
            None => {}
        }
    }

    fn multi_manager_complete_capture(&mut self) {
        let Some(action) = self.multi_manager.pending_capture.clone() else {
            return;
        };
        let Some(captured) = win::active_window() else {
            self.report_error_message("multi_manager.capture", "No active window to capture");
            return;
        };
        if self
            .multi_manager_settings
            .ignore_launcher_window_on_capture
            && self.launcher_hwnd == Some(captured.hwnd)
        {
            self.report_error_message(
                "multi_manager.capture",
                "Ignoring launcher window; focus another window and press Enter",
            );
            return;
        }
        match action {
            PendingCaptureAction::CaptureWorkspace { workspace_id }
            | PendingCaptureAction::RecaptureWorkspace { workspace_id } => {
                let window = MmWindow {
                    alias: captured.title.clone(),
                    title: captured.title,
                    hwnd: captured.hwnd,
                    home_rect: Some(captured.rect),
                    target_rect: Some(captured.rect),
                    ..Default::default()
                };
                self.multi_manager
                    .with_workspace_mut(&workspace_id, |workspace| {
                        workspace.windows.push(window);
                    });
            }
            PendingCaptureAction::CaptureWindow {
                workspace_id,
                window_id,
            } => {
                if let Ok(index) = window_id.parse::<usize>() {
                    self.multi_manager
                        .with_workspace_mut(&workspace_id, |workspace| {
                            if let Some(window) = workspace.windows.get_mut(index) {
                                window.title = captured.title.clone();
                                window.alias = captured.title;
                                window.hwnd = captured.hwnd;
                                window.home_rect = Some(captured.rect);
                                window.target_rect = Some(captured.rect);
                                window.valid = true;
                            }
                        });
                }
            }
        }
        self.multi_manager.pending_capture = None;
        if !self.multi_manager.recapture_active {
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
