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
        self.focus_query = false;
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

    pub fn multi_manager_send_all_home(&mut self) {
        let workspaces = self
            .multi_manager
            .workspaces
            .lock()
            .ok()
            .map(|workspaces| workspaces.clone());
        let Some(workspaces) = workspaces else {
            self.report_error_message(
                "multi_manager.send_all_home",
                "Failed to lock MultiManager workspaces to send all windows home",
            );
            return;
        };

        crate::multi_manager::runtime::send_all_home(&workspaces);
        self.add_success_toast("Sent all MultiManager windows home");
    }

    pub fn multi_manager_reconnect_windows(&mut self) {
        let reconnect_result = self
            .multi_manager
            .workspaces
            .lock()
            .map(|mut workspaces| {
                crate::multi_manager::reconnect::reconnect_workspaces(&mut workspaces)
            })
            .map_err(|_| ());

        let summary = match reconnect_result {
            Ok(summary) => summary,
            Err(()) => {
                self.report_error_message(
                    "multi_manager.reconnect",
                    "Failed to lock MultiManager workspaces to reconnect windows",
                );
                return;
            }
        };

        self.multi_manager.mark_dirty();
        self.add_success_toast(format_reconnect_summary(summary));
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

        self.multi_manager_start_recapture_queue(queue);
        self.add_success_toast("Started MultiManager window recapture queue");
        self.multi_manager_validate_capture_state();
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
            self.multi_manager_queue_capture_action(PendingCaptureAction::CaptureOneWindow {
                workspace_id: workspace_id.to_string(),
            });
            self.add_success_toast("Started MultiManager active window capture");
            self.multi_manager_validate_capture_state();
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
        self.multi_manager_begin_capture_action(
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
        self.multi_manager_begin_capture_action(
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
        self.multi_manager_begin_capture_action(
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

    fn multi_manager_begin_capture_action(
        &mut self,
        workspace_id: &str,
        window_index: Option<usize>,
        action: PendingCaptureAction,
        ctx: &egui::Context,
        success_message: &str,
    ) {
        if !self.multi_manager_capture_target_exists(workspace_id, window_index) {
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

        self.multi_manager.queued_capture = None;
        self.multi_manager_pop_recapture_queue_front_for_action(&action);

        self.multi_manager.pending_capture = Some(action);
        self.multi_manager_start_capture_session(ctx);
        self.multi_manager
            .runtime
            .control
            .capture_pending
            .store(true, Ordering::Relaxed);
        self.add_success_toast(success_message);
        self.multi_manager_validate_capture_state();
    }

    fn multi_manager_pop_recapture_queue_front_for_action(
        &mut self,
        action: &PendingCaptureAction,
    ) {
        if let PendingCaptureAction::RecaptureWindow {
            workspace_id,
            window_index,
        } = action
            && self
                .multi_manager
                .recapture_queue
                .front()
                .is_some_and(|item| {
                    item.workspace_id == *workspace_id && item.window_index == *window_index
                })
        {
            self.multi_manager.recapture_queue.pop_front();
        }
    }

    fn multi_manager_capture_target_exists(
        &self,
        workspace_id: &str,
        window_index: Option<usize>,
    ) -> bool {
        self.multi_manager
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
            .unwrap_or(false)
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
        tracing::info!(
            pending_action = ?self.multi_manager.pending_capture,
            queued_action = ?self.multi_manager.queued_capture,
            recapture_active = self.multi_manager.recapture_active,
            "MultiManager capture canceled"
        );
        self.multi_manager.pending_capture = None;
        self.multi_manager.capture_session = None;
        self.multi_manager.recapture_queue.clear();
        self.multi_manager.recapture_active = false;
        self.multi_manager.queued_capture = None;
        self.multi_manager
            .runtime
            .control
            .capture_pending
            .store(false, Ordering::Relaxed);
        self.multi_manager_validate_capture_state();
    }

    fn launcher_capture_match_reason(&self, captured: &CapturedWindow) -> Option<&'static str> {
        if self.launcher_hwnd == Some(captured.hwnd) {
            return Some("hwnd");
        }

        if captured.title.contains("Multi Lnchr") {
            return Some("title contains Multi Lnchr");
        }

        if captured.title.contains("Multi Launcher") {
            return Some("title contains Multi Launcher");
        }

        None
    }

    fn is_launcher_capture(&self, captured: &CapturedWindow) -> bool {
        let match_reason = self.launcher_capture_match_reason(captured);
        tracing::debug!(
            hwnd = captured.hwnd,
            title = %captured.title,
            executable = %captured.executable,
            class_name = %captured.class_name,
            process_path = %captured.process_path,
            launcher_hwnd = ?self.launcher_hwnd,
            match_reason = ?match_reason,
            is_match = match_reason.is_some(),
            "Evaluated MultiManager launcher capture criteria"
        );
        match_reason.is_some()
    }

    pub fn multi_manager_poll_capture(&mut self, ctx: &eframe::egui::Context) {
        self.multi_manager_process_queued_capture(ctx);

        if self.multi_manager.pending_capture.is_none()
            && self.multi_manager.queued_capture.is_none()
            && self.multi_manager.recapture_active
        {
            self.multi_manager_start_next_recapture_item(ctx);
            ctx.request_repaint();
        }

        if !self.multi_manager_capture_listener_active() {
            return;
        }

        let mut events = Vec::new();
        if let Some(session) = self.multi_manager.capture_session.as_ref() {
            while let Ok(event) = session.rx.try_recv() {
                events.push(event);
            }
        }

        for event in events {
            self.handle_capture_event(ctx, event);
        }
    }

    fn multi_manager_process_queued_capture(&mut self, ctx: &egui::Context) {
        let Some(action) = self.multi_manager.queued_capture.take() else {
            return;
        };
        tracing::info!(?action, "MultiManager queued capture consumed");

        if !self.multi_manager_capture_action_workspace_exists(&action) {
            if self.multi_manager.recapture_active {
                self.multi_manager_start_next_recapture_item(ctx);
            } else {
                self.multi_manager
                    .runtime
                    .control
                    .capture_pending
                    .store(false, Ordering::Relaxed);
                self.report_error_message(
                    "multi_manager.capture",
                    "Failed to start MultiManager capture: target not found",
                );
                self.multi_manager_validate_capture_state();
            }
            ctx.request_repaint();
            return;
        }

        self.multi_manager_pop_recapture_queue_front_for_action(&action);

        self.multi_manager.pending_capture = Some(action);
        self.multi_manager_start_capture_session(ctx);
        self.multi_manager
            .runtime
            .control
            .capture_pending
            .store(true, Ordering::Relaxed);
        self.multi_manager_validate_capture_state();
        ctx.request_repaint();
    }

    fn multi_manager_capture_action_workspace_exists(&self, action: &PendingCaptureAction) -> bool {
        match action {
            PendingCaptureAction::CaptureOneWindow { workspace_id }
            | PendingCaptureAction::CaptureMultipleWindows { workspace_id } => {
                self.multi_manager_workspace_exists(workspace_id)
            }
            PendingCaptureAction::RecaptureWindow {
                workspace_id,
                window_index,
            } => self.multi_manager_capture_target_exists(workspace_id, Some(*window_index)),
        }
    }

    fn handle_capture_event(&mut self, ctx: &eframe::egui::Context, event: capture::CaptureEvent) {
        match event.action {
            CaptureKeyAction::Cancel => {
                self.multi_manager_cancel_capture();
                ctx.request_repaint();
            }
            CaptureKeyAction::Skip => {
                self.multi_manager_skip_capture(ctx);
                ctx.request_repaint();
            }
            CaptureKeyAction::Confirm => {
                self.multi_manager_complete_capture(ctx, event.captured);
                ctx.request_repaint();
            }
        }
        self.multi_manager_validate_capture_state();
    }

    fn multi_manager_finish_capture(&mut self) {
        self.multi_manager.pending_capture = None;
        self.multi_manager.capture_session = None;
        let recapture_has_more_work =
            self.multi_manager.recapture_active && !self.multi_manager.recapture_queue.is_empty();
        if recapture_has_more_work {
            self.multi_manager.queued_capture =
                self.multi_manager.recapture_queue.front().map(|item| {
                    PendingCaptureAction::RecaptureWindow {
                        workspace_id: item.workspace_id.clone(),
                        window_index: item.window_index,
                    }
                });
            if let Some(action) = &self.multi_manager.queued_capture {
                tracing::info!(?action, "MultiManager queued capture stored");
            }
        } else {
            self.multi_manager.recapture_active = false;
            self.multi_manager
                .runtime
                .control
                .capture_pending
                .store(false, Ordering::Relaxed);
        }
        self.multi_manager_validate_capture_state();
    }

    #[cfg(test)]
    fn multi_manager_finish_current_capture_item(&mut self) {
        self.multi_manager_finish_capture();
    }

    fn multi_manager_clear_current_capture_item(&mut self) {
        self.multi_manager.pending_capture = None;
        self.multi_manager.capture_session = None;
        self.multi_manager.queued_capture = None;
    }

    fn multi_manager_start_next_recapture_item(&mut self, ctx: &egui::Context) -> bool {
        while let Some(item) = self.multi_manager.recapture_queue.pop_front() {
            if !self
                .multi_manager_capture_target_exists(&item.workspace_id, Some(item.window_index))
            {
                tracing::info!(
                    workspace_id = %item.workspace_id,
                    window_index = item.window_index,
                    "MultiManager recapture item skipped"
                );
                self.report_error_message(
                    "multi_manager.recapture",
                    format!(
                        "Skipping MultiManager recapture item: window {} not found in workspace {}",
                        item.window_index + 1,
                        item.workspace_id
                    ),
                );
                continue;
            }

            tracing::info!(
                workspace_id = %item.workspace_id,
                window_index = item.window_index,
                "MultiManager recapture item started"
            );
            self.multi_manager.pending_capture = Some(PendingCaptureAction::RecaptureWindow {
                workspace_id: item.workspace_id,
                window_index: item.window_index,
            });
            self.multi_manager_start_capture_session(ctx);
            self.multi_manager
                .runtime
                .control
                .capture_pending
                .store(true, Ordering::Relaxed);
            self.multi_manager_validate_capture_state();
            return true;
        }

        self.multi_manager_finish_recapture_queue();
        false
    }

    fn multi_manager_skip_capture(&mut self, ctx: &egui::Context) {
        if self.multi_manager.recapture_active {
            tracing::info!(
                pending_action = ?self.multi_manager.pending_capture,
                "MultiManager recapture item skipped"
            );
            self.multi_manager_clear_current_capture_item();
            self.multi_manager_start_next_recapture_item(ctx);
            self.multi_manager_validate_capture_state();
            return;
        }

        if self.multi_manager.pending_capture.is_some() {
            self.multi_manager_restart_capture_listener(ctx);
        } else {
            self.multi_manager_finish_capture();
        }
        self.multi_manager_validate_capture_state();
    }

    fn multi_manager_start_capture_session(&mut self, ctx: &egui::Context) {
        self.multi_manager.capture_session = None;
        self.multi_manager.capture_session = Some(capture::start_capture_session(ctx.clone()));
    }

    fn multi_manager_restart_capture_listener(&mut self, ctx: &eframe::egui::Context) {
        self.multi_manager.capture_session = None;
        self.multi_manager.capture_session = Some(capture::start_capture_session(ctx.clone()));
        self.multi_manager
            .runtime
            .control
            .capture_pending
            .store(true, Ordering::Relaxed);
        self.multi_manager_validate_capture_state();
    }

    fn multi_manager_capture_listener_active(&self) -> bool {
        self.multi_manager.capture_session.is_some()
    }

    fn multi_manager_queue_capture_action(&mut self, action: PendingCaptureAction) {
        tracing::info!(?action, "MultiManager queued capture stored");
        self.multi_manager.queued_capture = Some(action);
        self.multi_manager
            .runtime
            .control
            .capture_pending
            .store(true, Ordering::Relaxed);
        self.multi_manager_validate_capture_state();
    }

    fn multi_manager_start_recapture_queue(&mut self, queue: Vec<RecaptureQueueItem>) {
        self.multi_manager.recapture_queue = queue.into();
        self.multi_manager.recapture_active = true;
        if let Some(item) = self.multi_manager.recapture_queue.front() {
            self.multi_manager_queue_capture_action(PendingCaptureAction::RecaptureWindow {
                workspace_id: item.workspace_id.clone(),
                window_index: item.window_index,
            });
        }
    }

    fn multi_manager_finish_recapture_queue(&mut self) {
        self.multi_manager.recapture_active = false;
        self.multi_manager
            .runtime
            .control
            .capture_pending
            .store(false, Ordering::Relaxed);
        self.multi_manager_validate_capture_state();
    }

    fn multi_manager_validate_capture_state(&self) {
        self.multi_manager.validate_capture_state_debug();
        debug_assert!(
            self.multi_manager.pending_capture.is_some()
                || self.multi_manager.capture_session.is_none(),
            "capture session must have a pending action"
        );
        debug_assert!(
            !self.multi_manager.recapture_active
                || self.multi_manager.pending_capture.is_some()
                || !self.multi_manager.recapture_queue.is_empty(),
            "active recapture must have a current item or queued item"
        );
        debug_assert!(
            self.multi_manager
                .runtime
                .control
                .capture_pending
                .load(Ordering::Relaxed)
                || (self.multi_manager.pending_capture.is_none()
                    && self.multi_manager.queued_capture.is_none()
                    && self.multi_manager.capture_session.is_none()
                    && !self.multi_manager.recapture_active),
            "capture_pending flag is false while capture state is active"
        );
    }

    fn multi_manager_complete_capture(
        &mut self,
        ctx: &eframe::egui::Context,
        captured: Option<CapturedWindow>,
    ) {
        let Some(action) = self.multi_manager.pending_capture.clone() else {
            tracing::warn!("MultiManager capture confirm ignored without a pending action");
            self.multi_manager_finish_capture();
            return;
        };
        let Some(captured) = captured else {
            tracing::warn!(
                ?action,
                "MultiManager capture confirm had no active foreground window"
            );
            self.report_error_message(
                "multi_manager.capture",
                "No active window to capture; focus a window and press Enter",
            );
            if self.multi_manager.pending_capture.is_some() {
                self.multi_manager_restart_capture_listener(ctx);
            }
            return;
        };
        if self
            .multi_manager_settings
            .ignore_launcher_window_on_capture
            && self.is_launcher_capture(&captured)
        {
            let match_reason = self.launcher_capture_match_reason(&captured);
            tracing::warn!(
                hwnd = captured.hwnd,
                title = %captured.title,
                executable = %captured.executable,
                class_name = %captured.class_name,
                process_path = %captured.process_path,
                launcher_hwnd = ?self.launcher_hwnd,
                match_reason = ?match_reason,
                ?action,
                "MultiManager launcher capture ignored"
            );
            self.report_error_message(
                "multi_manager.capture",
                "Ignoring launcher window; focus another window and press Enter",
            );
            self.multi_manager_restart_capture_listener(ctx);
            return;
        }

        let result = self
            .multi_manager
            .workspaces
            .lock()
            .map(|mut workspaces| {
                apply_capture::apply_capture_to_workspaces(
                    &mut workspaces,
                    &action,
                    captured.clone(),
                )
            })
            .unwrap_or(ApplyCaptureResult::MissingWorkspace);
        match result {
            ApplyCaptureResult::Applied => {
                log_applied_capture(&action, &captured);
                self.multi_manager.mark_dirty();
            }
            ApplyCaptureResult::MissingWorkspace => {
                self.report_error_message(
                    "multi_manager.capture",
                    "Failed to apply capture: workspace not found",
                );
                self.multi_manager_finish_capture();
                return;
            }
            ApplyCaptureResult::MissingWindow => {
                self.report_error_message(
                    "multi_manager.capture",
                    "Failed to apply capture: window not found",
                );
                self.multi_manager_finish_capture();
                return;
            }
        }

        match action {
            PendingCaptureAction::CaptureOneWindow { .. } => self.multi_manager_finish_capture(),
            PendingCaptureAction::CaptureMultipleWindows { .. } => {
                self.multi_manager_restart_capture_listener(ctx);
            }
            PendingCaptureAction::RecaptureWindow { .. } => {
                self.multi_manager_clear_current_capture_item();
                self.multi_manager_start_next_recapture_item(ctx);
            }
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

fn log_applied_capture(action: &PendingCaptureAction, captured: &CapturedWindow) {
    match action {
        PendingCaptureAction::CaptureOneWindow { workspace_id } => tracing::info!(
            workspace_id = %workspace_id,
            hwnd = captured.hwnd,
            title = %captured.title,
            executable = %captured.executable,
            class_name = %captured.class_name,
            process_path = %captured.process_path,
            "MultiManager one-shot capture applied"
        ),
        PendingCaptureAction::CaptureMultipleWindows { workspace_id } => tracing::info!(
            workspace_id = %workspace_id,
            hwnd = captured.hwnd,
            title = %captured.title,
            executable = %captured.executable,
            class_name = %captured.class_name,
            process_path = %captured.process_path,
            "MultiManager multi-capture applied and listener restarted"
        ),
        PendingCaptureAction::RecaptureWindow {
            workspace_id,
            window_index,
        } => tracing::info!(
            workspace_id = %workspace_id,
            window_index = *window_index,
            hwnd = captured.hwnd,
            title = %captured.title,
            executable = %captured.executable,
            class_name = %captured.class_name,
            process_path = %captured.process_path,
            "MultiManager recapture item applied"
        ),
    }
}

pub(crate) fn format_reconnect_summary(
    summary: crate::multi_manager::reconnect::ReconnectSummary,
) -> String {
    format!(
        "Reconnected MultiManager windows: {} reconnected, {} missing, {} ambiguous",
        summary.reconnected, summary.missing, summary.ambiguous
    )
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
                .filter(|&(_window_index, window)| {
                    window.hwnd == 0 || !win::is_valid_window(window.hwnd)
                })
                .map(|(window_index, _window)| RecaptureQueueItem {
                    workspace_id: workspace.id.clone(),
                    window_index,
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{build_recapture_queue, format_reconnect_summary};
    use crate::gui::LauncherApp;
    use crate::multi_manager::capture::CaptureEvent;
    use crate::multi_manager::model::{
        MmRect, MmWindow, MmWorkspace, PendingCaptureAction, RecaptureQueueItem,
    };
    use crate::multi_manager::win::{CaptureKeyAction, CapturedWindow};
    use crate::plugin::PluginManager;
    use crate::settings::Settings;
    use std::collections::VecDeque;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    #[test]
    fn reconnect_summary_format_includes_action_counts() {
        let summary = crate::multi_manager::reconnect::ReconnectSummary {
            reconnected: 2,
            missing: 3,
            ambiguous: 4,
            ..Default::default()
        };

        let text = format_reconnect_summary(summary);

        assert!(text.contains("2 reconnected"));
        assert!(text.contains("3 missing"));
        assert!(text.contains("4 ambiguous"));
    }

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

    fn captured(hwnd: usize, captured_title: &str) -> CapturedWindow {
        CapturedWindow {
            hwnd,
            title: captured_title.to_string(),
            rect: MmRect {
                x: 0,
                y: 0,
                w: 100,
                h: 100,
            },
            executable: format!("{captured_title}.exe"),
            class_name: format!("{captured_title}Class"),
            process_path: format!("C:/Apps/{captured_title}.exe"),
        }
    }

    fn recapture_all_workspace() -> MmWorkspace {
        MmWorkspace {
            id: "w".into(),
            windows: vec![
                MmWindow {
                    captured_title: "Old 1".into(),
                    hwnd: 0,
                    ..Default::default()
                },
                MmWindow {
                    captured_title: "Old 2".into(),
                    hwnd: 0,
                    ..Default::default()
                },
            ],
            ..Default::default()
        }
    }

    #[test]
    fn recapture_all_starts_first_listener() {
        let ctx = eframe::egui::Context::default();
        let mut app = test_app();
        set_workspaces(&mut app, vec![recapture_all_workspace()]);

        app.multi_manager_start_recapture_all();
        app.multi_manager_poll_capture(&ctx);

        assert_eq!(
            app.multi_manager.pending_capture,
            Some(PendingCaptureAction::RecaptureWindow {
                workspace_id: "w".into(),
                window_index: 0,
            })
        );
        assert!(app.multi_manager.capture_session.is_some());
        assert!(app.multi_manager.recapture_active);
        assert_eq!(app.multi_manager.recapture_queue.len(), 1);
    }

    #[test]
    fn recapture_all_advances_after_confirm() {
        let ctx = eframe::egui::Context::default();
        let mut app = test_app();
        set_workspaces(&mut app, vec![recapture_all_workspace()]);

        app.multi_manager_start_recapture_all();
        app.multi_manager_poll_capture(&ctx);
        app.multi_manager_complete_capture(&ctx, Some(captured(10, "New 1")));

        assert_eq!(
            app.multi_manager.pending_capture,
            Some(PendingCaptureAction::RecaptureWindow {
                workspace_id: "w".into(),
                window_index: 1,
            })
        );
        assert!(app.multi_manager.capture_session.is_some());
        assert!(app.multi_manager.recapture_queue.is_empty());
    }

    #[test]
    fn recapture_all_advances_after_skip() {
        let ctx = eframe::egui::Context::default();
        let mut app = test_app();
        set_workspaces(&mut app, vec![recapture_all_workspace()]);

        app.multi_manager_start_recapture_all();
        app.multi_manager_poll_capture(&ctx);
        app.multi_manager_skip_capture(&ctx);

        assert_eq!(
            app.multi_manager.pending_capture,
            Some(PendingCaptureAction::RecaptureWindow {
                workspace_id: "w".into(),
                window_index: 1,
            })
        );
        assert!(app.multi_manager.capture_session.is_some());
        assert!(app.multi_manager.recapture_queue.is_empty());
    }

    #[test]
    fn recapture_all_clears_state_when_queue_becomes_empty() {
        let ctx = eframe::egui::Context::default();
        let mut app = test_app();
        set_workspaces(&mut app, vec![recapture_all_workspace()]);

        app.multi_manager_start_recapture_all();
        app.multi_manager_poll_capture(&ctx);
        app.multi_manager_complete_capture(&ctx, Some(captured(10, "New 1")));
        app.multi_manager_complete_capture(&ctx, Some(captured(11, "New 2")));

        assert_eq!(app.multi_manager.pending_capture, None);
        assert!(app.multi_manager.capture_session.is_none());
        assert!(!app.multi_manager.recapture_active);
        assert!(app.multi_manager.recapture_queue.is_empty());
        assert!(
            !app.multi_manager
                .runtime
                .control
                .capture_pending
                .load(Ordering::Relaxed)
        );
    }

    #[test]
    fn recapture_all_cancel_clears_remaining_queue_entries() {
        let ctx = eframe::egui::Context::default();
        let mut app = test_app();
        set_workspaces(&mut app, vec![recapture_all_workspace()]);

        app.multi_manager_start_recapture_all();
        app.multi_manager_poll_capture(&ctx);
        app.multi_manager_cancel_capture();

        assert_eq!(app.multi_manager.pending_capture, None);
        assert!(app.multi_manager.capture_session.is_none());
        assert!(!app.multi_manager.recapture_active);
        assert!(app.multi_manager.recapture_queue.is_empty());
        assert_eq!(app.multi_manager.queued_capture, None);
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

    fn begin_one_window_capture(app: &mut LauncherApp, ctx: &eframe::egui::Context) {
        app.multi_manager_begin_capture_action(
            "w",
            None,
            PendingCaptureAction::CaptureOneWindow {
                workspace_id: "w".into(),
            },
            ctx,
            "Started test capture",
        );
    }

    fn begin_multi_window_capture(app: &mut LauncherApp, ctx: &eframe::egui::Context) {
        app.multi_manager_begin_capture_action(
            "w",
            None,
            PendingCaptureAction::CaptureMultipleWindows {
                workspace_id: "w".into(),
            },
            ctx,
            "Started test multi-capture",
        );
    }

    fn begin_recapture(app: &mut LauncherApp, ctx: &eframe::egui::Context, index: usize) {
        app.multi_manager_begin_capture_action(
            "w",
            Some(index),
            PendingCaptureAction::RecaptureWindow {
                workspace_id: "w".into(),
                window_index: index,
            },
            ctx,
            "Started test recapture",
        );
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
        begin_one_window_capture(&mut app, &eframe::egui::Context::default());

        app.multi_manager_complete_capture(
            &eframe::egui::Context::default(),
            Some(captured(7, "Editor")),
        );

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
        assert_eq!(workspaces[0].windows[0].captured_title, "Editor");
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
        begin_multi_window_capture(&mut app, &eframe::egui::Context::default());

        app.multi_manager_complete_capture(
            &eframe::egui::Context::default(),
            Some(captured(1, "One")),
        );
        app.multi_manager_complete_capture(
            &eframe::egui::Context::default(),
            Some(captured(2, "Two")),
        );

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
        assert_eq!(workspaces[0].windows[0].captured_title, "One");
        assert_eq!(workspaces[0].windows[1].captured_title, "Two");
    }

    #[test]
    fn confirm_event_in_one_shot_mode_clears_pending_capture() {
        let ctx = eframe::egui::Context::default();
        let mut app = test_app();
        set_workspaces(
            &mut app,
            vec![MmWorkspace {
                id: "w".into(),
                ..Default::default()
            }],
        );
        begin_one_window_capture(&mut app, &eframe::egui::Context::default());

        app.handle_capture_event(
            &ctx,
            CaptureEvent {
                action: CaptureKeyAction::Confirm,
                captured: Some(captured(7, "Editor")),
            },
        );

        assert_eq!(app.multi_manager.pending_capture, None);
        assert!(app.multi_manager.capture_session.is_none());
        assert!(
            !app.multi_manager
                .runtime
                .control
                .capture_pending
                .load(Ordering::Relaxed)
        );
        let workspaces = app.multi_manager.workspaces.lock().expect("workspaces");
        assert_eq!(workspaces[0].windows.len(), 1);
        assert_eq!(workspaces[0].windows[0].captured_title, "Editor");
    }

    #[test]
    fn confirm_event_in_multi_capture_mode_keeps_pending_capture() {
        let ctx = eframe::egui::Context::default();
        let mut app = test_app();
        set_workspaces(
            &mut app,
            vec![MmWorkspace {
                id: "w".into(),
                ..Default::default()
            }],
        );
        begin_multi_window_capture(&mut app, &eframe::egui::Context::default());

        app.handle_capture_event(
            &ctx,
            CaptureEvent {
                action: CaptureKeyAction::Confirm,
                captured: Some(captured(1, "One")),
            },
        );

        assert_eq!(
            app.multi_manager.pending_capture,
            Some(PendingCaptureAction::CaptureMultipleWindows {
                workspace_id: "w".into()
            })
        );
        assert!(app.multi_manager.capture_session.is_some());
        assert!(
            app.multi_manager
                .runtime
                .control
                .capture_pending
                .load(Ordering::Relaxed)
        );
    }

    #[test]
    fn confirm_event_without_foreground_window_restarts_listener() {
        let ctx = eframe::egui::Context::default();
        let mut app = test_app();
        set_workspaces(
            &mut app,
            vec![MmWorkspace {
                id: "w".into(),
                ..Default::default()
            }],
        );
        begin_one_window_capture(&mut app, &ctx);

        app.handle_capture_event(
            &ctx,
            CaptureEvent {
                action: CaptureKeyAction::Confirm,
                captured: None,
            },
        );

        assert_eq!(
            app.multi_manager.pending_capture,
            Some(PendingCaptureAction::CaptureOneWindow {
                workspace_id: "w".into()
            })
        );
        assert!(app.multi_manager.capture_session.is_some());
        assert!(
            app.multi_manager
                .runtime
                .control
                .capture_pending
                .load(Ordering::Relaxed)
        );
        let workspaces = app.multi_manager.workspaces.lock().expect("workspaces");
        assert!(workspaces[0].windows.is_empty());
    }

    #[test]
    fn ignored_launcher_capture_restarts_listener_without_applying() {
        let ctx = eframe::egui::Context::default();
        let mut app = test_app();
        app.launcher_hwnd = Some(7);
        app.multi_manager_settings.ignore_launcher_window_on_capture = true;
        set_workspaces(
            &mut app,
            vec![MmWorkspace {
                id: "w".into(),
                ..Default::default()
            }],
        );
        begin_one_window_capture(&mut app, &ctx);

        app.handle_capture_event(
            &ctx,
            CaptureEvent {
                action: CaptureKeyAction::Confirm,
                captured: Some(captured(7, "Multi Launcher")),
            },
        );

        assert_eq!(
            app.multi_manager.pending_capture,
            Some(PendingCaptureAction::CaptureOneWindow {
                workspace_id: "w".into()
            })
        );
        assert!(app.multi_manager.capture_session.is_some());
        let workspaces = app.multi_manager.workspaces.lock().expect("workspaces");
        assert!(workspaces[0].windows.is_empty());
    }

    #[test]
    fn ignored_launcher_title_during_multi_capture_restarts_without_appending() {
        let ctx = eframe::egui::Context::default();
        let mut app = test_app();
        app.multi_manager_settings.ignore_launcher_window_on_capture = true;
        set_workspaces(
            &mut app,
            vec![MmWorkspace {
                id: "w".into(),
                windows: vec![MmWindow {
                    captured_title: "Existing".into(),
                    hwnd: 1,
                    ..Default::default()
                }],
                ..Default::default()
            }],
        );
        begin_multi_window_capture(&mut app, &ctx);

        app.handle_capture_event(
            &ctx,
            CaptureEvent {
                action: CaptureKeyAction::Confirm,
                captured: Some(captured(99, "Multi Launcher")),
            },
        );

        assert_eq!(
            app.multi_manager.pending_capture,
            Some(PendingCaptureAction::CaptureMultipleWindows {
                workspace_id: "w".into()
            })
        );
        assert!(app.multi_manager.capture_session.is_some());
        assert!(
            app.multi_manager
                .runtime
                .control
                .capture_pending
                .load(Ordering::Relaxed)
        );
        let workspaces = app.multi_manager.workspaces.lock().expect("workspaces");
        assert_eq!(workspaces[0].windows.len(), 1);
        assert_eq!(workspaces[0].windows[0].captured_title, "Existing");
    }

    #[test]
    fn ignored_launcher_hwnd_during_recapture_restarts_without_updating_current_item() {
        let ctx = eframe::egui::Context::default();
        let mut app = test_app();
        app.launcher_hwnd = Some(55);
        app.multi_manager_settings.ignore_launcher_window_on_capture = true;
        set_workspaces(
            &mut app,
            vec![MmWorkspace {
                id: "w".into(),
                windows: vec![MmWindow {
                    captured_title: "Old".into(),
                    hwnd: 10,
                    ..Default::default()
                }],
                ..Default::default()
            }],
        );
        begin_recapture(&mut app, &ctx, 0);

        app.handle_capture_event(
            &ctx,
            CaptureEvent {
                action: CaptureKeyAction::Confirm,
                captured: Some(captured(55, "Settings")),
            },
        );

        assert_eq!(
            app.multi_manager.pending_capture,
            Some(PendingCaptureAction::RecaptureWindow {
                workspace_id: "w".into(),
                window_index: 0,
            })
        );
        assert!(app.multi_manager.capture_session.is_some());
        assert!(
            app.multi_manager
                .runtime
                .control
                .capture_pending
                .load(Ordering::Relaxed)
        );
        let workspaces = app.multi_manager.workspaces.lock().expect("workspaces");
        assert_eq!(workspaces[0].windows.len(), 1);
        assert_eq!(workspaces[0].windows[0].captured_title, "Old");
        assert_eq!(workspaces[0].windows[0].hwnd, 10);
    }

    #[test]
    fn recapture_confirm_advances_queue_and_restarts_listener() {
        let ctx = eframe::egui::Context::default();
        let mut app = test_app();
        set_workspaces(
            &mut app,
            vec![MmWorkspace {
                id: "w".into(),
                windows: vec![MmWindow::default(), MmWindow::default()],
                ..Default::default()
            }],
        );
        app.multi_manager_start_recapture_queue(vec![RecaptureQueueItem {
            workspace_id: "w".into(),
            window_index: 1,
        }]);
        begin_recapture(&mut app, &ctx, 0);

        app.handle_capture_event(
            &ctx,
            CaptureEvent {
                action: CaptureKeyAction::Confirm,
                captured: Some(captured(10, "New")),
            },
        );

        assert_eq!(
            app.multi_manager.pending_capture,
            Some(PendingCaptureAction::RecaptureWindow {
                workspace_id: "w".into(),
                window_index: 1,
            })
        );
        assert!(app.multi_manager.capture_session.is_some());
        assert!(app.multi_manager.recapture_queue.is_empty());
        let workspaces = app.multi_manager.workspaces.lock().expect("workspaces");
        assert_eq!(workspaces[0].windows[0].captured_title, "New");
    }

    #[test]
    fn skip_event_during_recapture_clears_only_current_item() {
        let ctx = eframe::egui::Context::default();
        let mut app = test_app();
        set_workspaces(
            &mut app,
            vec![MmWorkspace {
                id: "w".into(),
                windows: vec![MmWindow::default(), MmWindow::default()],
                ..Default::default()
            }],
        );
        app.multi_manager_start_recapture_queue(vec![RecaptureQueueItem {
            workspace_id: "w".into(),
            window_index: 1,
        }]);
        begin_recapture(&mut app, &ctx, 0);

        app.handle_capture_event(
            &ctx,
            CaptureEvent {
                action: CaptureKeyAction::Skip,
                captured: None,
            },
        );

        assert_eq!(
            app.multi_manager.pending_capture,
            Some(PendingCaptureAction::RecaptureWindow {
                workspace_id: "w".into(),
                window_index: 1,
            })
        );
        assert!(app.multi_manager.capture_session.is_some());
        assert!(app.multi_manager.recapture_active);
        assert!(app.multi_manager.recapture_queue.is_empty());
        assert!(
            app.multi_manager
                .runtime
                .control
                .capture_pending
                .load(Ordering::Relaxed)
        );
    }

    #[test]
    fn cancel_event_clears_queue_and_pending_session() {
        let ctx = eframe::egui::Context::default();
        let mut app = test_app();
        set_workspaces(
            &mut app,
            vec![MmWorkspace {
                id: "w".into(),
                windows: vec![MmWindow::default(), MmWindow::default()],
                ..Default::default()
            }],
        );
        app.multi_manager_start_recapture_queue(vec![RecaptureQueueItem {
            workspace_id: "w".into(),
            window_index: 1,
        }]);
        begin_recapture(&mut app, &ctx, 0);

        app.handle_capture_event(
            &ctx,
            CaptureEvent {
                action: CaptureKeyAction::Cancel,
                captured: None,
            },
        );

        assert_eq!(app.multi_manager.pending_capture, None);
        assert!(app.multi_manager.capture_session.is_none());
        assert!(!app.multi_manager.recapture_active);
        assert!(app.multi_manager.recapture_queue.is_empty());
        assert!(
            !app.multi_manager
                .runtime
                .control
                .capture_pending
                .load(Ordering::Relaxed)
        );
    }

    #[test]
    fn finish_current_capture_item_during_active_queue_preserves_remaining_queue() {
        let mut app = test_app();
        let ctx = eframe::egui::Context::default();
        set_workspaces(
            &mut app,
            vec![MmWorkspace {
                id: "w".into(),
                windows: vec![
                    MmWindow::default(),
                    MmWindow::default(),
                    MmWindow::default(),
                ],
                ..Default::default()
            }],
        );
        app.multi_manager_start_recapture_queue(vec![
            RecaptureQueueItem {
                workspace_id: "w".into(),
                window_index: 1,
            },
            RecaptureQueueItem {
                workspace_id: "w".into(),
                window_index: 2,
            },
        ]);
        begin_recapture(&mut app, &ctx, 0);

        app.multi_manager_finish_current_capture_item();

        assert_eq!(app.multi_manager.pending_capture, None);
        assert!(app.multi_manager.capture_session.is_none());
        assert!(app.multi_manager.recapture_active);
        assert_eq!(app.multi_manager.recapture_queue.len(), 2);
        assert!(
            app.multi_manager
                .runtime
                .control
                .capture_pending
                .load(Ordering::Relaxed)
        );
    }

    #[test]
    fn action_path_capture_queues_listener_without_pending_capture() {
        let mut app = test_app();
        set_workspaces(
            &mut app,
            vec![MmWorkspace {
                id: "w".into(),
                ..Default::default()
            }],
        );

        app.multi_manager_start_capture("w");

        assert_eq!(
            app.multi_manager.queued_capture,
            Some(PendingCaptureAction::CaptureOneWindow {
                workspace_id: "w".into()
            })
        );
        assert_eq!(app.multi_manager.pending_capture, None);
        assert!(app.multi_manager.capture_session.is_none());
        assert!(
            app.multi_manager
                .runtime
                .control
                .capture_pending
                .load(Ordering::Relaxed)
        );
    }

    #[test]
    fn queued_capture_starts_when_context_becomes_available() {
        let ctx = eframe::egui::Context::default();
        let mut app = test_app();
        set_workspaces(
            &mut app,
            vec![MmWorkspace {
                id: "w".into(),
                ..Default::default()
            }],
        );
        app.multi_manager_queue_capture_action(PendingCaptureAction::CaptureOneWindow {
            workspace_id: "w".into(),
        });

        app.multi_manager_poll_capture(&ctx);

        assert_eq!(app.multi_manager.queued_capture, None);
        assert_eq!(
            app.multi_manager.pending_capture,
            Some(PendingCaptureAction::CaptureOneWindow {
                workspace_id: "w".into()
            })
        );
        assert!(app.multi_manager.capture_session.is_some());
        assert!(
            app.multi_manager
                .runtime
                .control
                .capture_pending
                .load(Ordering::Relaxed)
        );

        app.multi_manager_cancel_capture();
    }

    #[test]
    fn invalid_queued_capture_workspace_is_cleared_safely() {
        let ctx = eframe::egui::Context::default();
        let mut app = test_app();
        set_workspaces(&mut app, Vec::new());
        app.multi_manager_queue_capture_action(PendingCaptureAction::CaptureOneWindow {
            workspace_id: "missing".into(),
        });

        app.multi_manager_poll_capture(&ctx);

        assert_eq!(app.multi_manager.queued_capture, None);
        assert_eq!(app.multi_manager.pending_capture, None);
        assert!(app.multi_manager.capture_session.is_none());
        assert!(
            !app.multi_manager
                .runtime
                .control
                .capture_pending
                .load(Ordering::Relaxed)
        );
    }

    #[test]
    fn begin_capture_sets_pending_replaces_listener_and_sets_runtime_flag() {
        let ctx = eframe::egui::Context::default();
        let mut app = test_app();
        set_workspaces(
            &mut app,
            vec![MmWorkspace {
                id: "w".into(),
                ..Default::default()
            }],
        );

        begin_one_window_capture(&mut app, &ctx);
        let first_session = app
            .multi_manager
            .capture_session
            .as_ref()
            .map(crate::multi_manager::capture::CaptureSession::test_id);
        assert_eq!(
            app.multi_manager.pending_capture,
            Some(PendingCaptureAction::CaptureOneWindow {
                workspace_id: "w".into()
            })
        );
        assert!(first_session.is_some());
        assert!(
            app.multi_manager
                .runtime
                .control
                .capture_pending
                .load(Ordering::Relaxed)
        );

        begin_multi_window_capture(&mut app, &ctx);
        let second_session = app
            .multi_manager
            .capture_session
            .as_ref()
            .map(crate::multi_manager::capture::CaptureSession::test_id);
        assert_eq!(
            app.multi_manager.pending_capture,
            Some(PendingCaptureAction::CaptureMultipleWindows {
                workspace_id: "w".into()
            })
        );
        assert!(second_session.is_some());
        assert_ne!(first_session, second_session);
        assert!(
            app.multi_manager
                .runtime
                .control
                .capture_pending
                .load(Ordering::Relaxed)
        );

        app.multi_manager_cancel_capture();
    }

    #[test]
    fn no_pending_capture_exists_without_active_or_queued_listener() {
        let mut app = test_app();

        assert_eq!(app.multi_manager.pending_capture, None);
        assert_eq!(app.multi_manager.queued_capture, None);
        assert!(app.multi_manager.capture_session.is_none());
        assert!(
            !app.multi_manager
                .runtime
                .control
                .capture_pending
                .load(Ordering::Relaxed)
        );

        app.multi_manager_cancel_capture();
        assert_eq!(app.multi_manager.pending_capture, None);
        assert_eq!(app.multi_manager.queued_capture, None);
        assert!(app.multi_manager.capture_session.is_none());
        assert!(!app.multi_manager.recapture_active);
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
                        captured_title: "Keep".into(),
                        alias: "Keep".into(),
                        hwnd: 10,
                        ..Default::default()
                    },
                    MmWindow {
                        captured_title: "Old".into(),
                        alias: "Old".into(),
                        hwnd: 11,
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
        );
        let ctx = eframe::egui::Context::default();
        begin_recapture(&mut app, &ctx, 1);

        app.multi_manager_complete_capture(&ctx, Some(captured(22, "New")));

        let workspaces = app.multi_manager.workspaces.lock().expect("workspaces");
        assert_eq!(workspaces[0].windows.len(), 2);
        assert_eq!(workspaces[0].windows[0].captured_title, "Keep");
        assert_eq!(workspaces[0].windows[1].captured_title, "New");
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
