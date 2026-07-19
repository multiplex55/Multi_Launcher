use super::*;

impl LauncherApp {
    pub(crate) fn launcher_query_keyboard_enabled(query_has_focus: bool) -> bool {
        query_has_focus
    }

    pub(crate) fn launcher_enter_activation_enabled(
        query_has_focus: bool,
        file_search_open: bool,
    ) -> bool {
        query_has_focus && !file_search_open
    }

    pub(crate) fn launcher_escape_handling_enabled(file_search_open: bool) -> bool {
        !file_search_open
    }

    pub(crate) fn launcher_query_focus_should_be_requested(
        just_became_visible: bool,
        focus_query: bool,
        file_search_open: bool,
    ) -> bool {
        (just_became_visible || focus_query) && !file_search_open
    }

    pub(crate) fn result_context_menu_kind(&self, action: &Action) -> ResultContextMenuKind {
        if self.folder_aliases.contains_key(&action.action) && !action.action.starts_with("folder:")
        {
            ResultContextMenuKind::Folder
        } else if self.bookmark_aliases.contains_key(&action.action) {
            ResultContextMenuKind::Bookmark
        } else if action.desc == "Timer" && action.action.starts_with("timer:show:") {
            action.action[11..]
                .parse::<u64>()
                .map(|id| ResultContextMenuKind::Timer { id })
                .unwrap_or(ResultContextMenuKind::Default)
        } else if action.desc == "Stopwatch" && action.action.starts_with("stopwatch:show:") {
            action.action["stopwatch:show:".len()..]
                .parse::<u64>()
                .map(|id| ResultContextMenuKind::Stopwatch { id })
                .unwrap_or(ResultContextMenuKind::Default)
        } else if action.desc == "Snippet" {
            ResultContextMenuKind::Snippet
        } else if action.desc == "Tempfile" && !action.action.starts_with("tempfile:") {
            ResultContextMenuKind::Tempfile
        } else if action.desc == "Note" && action.action.starts_with("note:open:") {
            let slug = action.action.rsplit(':').next().unwrap_or("").to_string();
            ResultContextMenuKind::Note { slug }
        } else if action.desc == "Clipboard" && action.action.starts_with("clipboard:copy:") {
            if let Ok(idx) = action
                .action
                .rsplit(':')
                .next()
                .unwrap_or("")
                .parse::<usize>()
            {
                ResultContextMenuKind::Clipboard {
                    idx,
                    label: action.label.clone(),
                }
            } else {
                ResultContextMenuKind::Default
            }
        } else if action.desc == "Todo" && action.action.starts_with("todo:done:") {
            action
                .action
                .rsplit(':')
                .next()
                .unwrap_or("")
                .parse::<usize>()
                .map(|idx| ResultContextMenuKind::Todo { idx })
                .unwrap_or(ResultContextMenuKind::Default)
        } else {
            ResultContextMenuKind::Default
        }
    }

    fn attach_result_context_menu(
        &mut self,
        action: &Action,
        menu_resp: egui::Response,
        refresh: &mut bool,
        set_focus: &mut bool,
    ) -> egui::Response {
        let custom_idx = self
            .actions
            .iter()
            .take(self.custom_len)
            .position(|act| act.action == action.action && act.label == action.label);
        let query = self.query.trim().to_string();

        match self.result_context_menu_kind(action) {
            ResultContextMenuKind::Folder => {
                menu_resp.clone().context_menu(|ui| {
                    if ui.button("Set Alias").clicked() {
                        self.alias_dialog.open(&action.action);
                        ui.close_menu();
                    }
                    if ui.button("Remove Folder").clicked() {
                        if let Err(e) = crate::plugins::folders::remove_folder(
                            crate::plugins::folders::FOLDERS_FILE,
                            &action.action,
                        ) {
                            self.report_error_message(
                                "launcher",
                                format!("Failed to remove folder: {e}"),
                            );
                        } else {
                            *refresh = true;
                            *set_focus = true;
                            if self.enable_toasts {
                                push_toast(
                                    &mut self.toasts,
                                    Toast {
                                        text: format!("Removed folder {}", action.label).into(),
                                        kind: ToastKind::Success,
                                        options: ToastOptions::default()
                                            .duration_in_seconds(self.toast_duration as f64),
                                    },
                                );
                            }
                        }
                        ui.close_menu();
                    }
                    if let Some(idx_act) = custom_idx
                        && ui.button("Edit App").clicked()
                    {
                        self.editor.open_edit(idx_act, &self.actions[idx_act]);
                        self.show_editor = true;
                        ui.close_menu();
                    }
                    self.pin_result_menu(ui, action);
                });
            }
            ResultContextMenuKind::Bookmark => {
                menu_resp.clone().context_menu(|ui| {
                    if ui.button("Set Alias").clicked() {
                        self.bookmark_alias_dialog.open(&action.action);
                        ui.close_menu();
                    }
                    if ui.button("Remove Bookmark").clicked() {
                        if let Err(e) = crate::plugins::bookmarks::remove_bookmark(
                            crate::plugins::bookmarks::BOOKMARKS_FILE,
                            &action.action,
                        ) {
                            self.report_error_message(
                                "launcher",
                                format!("Failed to remove bookmark: {e}"),
                            );
                        } else {
                            *refresh = true;
                            *set_focus = true;
                            if self.enable_toasts {
                                push_toast(
                                    &mut self.toasts,
                                    Toast {
                                        text: format!("Removed bookmark {}", action.label).into(),
                                        kind: ToastKind::Success,
                                        options: ToastOptions::default()
                                            .duration_in_seconds(self.toast_duration as f64),
                                    },
                                );
                            }
                        }
                        ui.close_menu();
                    }
                    if let Some(idx_act) = custom_idx
                        && ui.button("Edit App").clicked()
                    {
                        self.editor.open_edit(idx_act, &self.actions[idx_act]);
                        self.show_editor = true;
                        ui.close_menu();
                    }
                    self.pin_result_menu(ui, action);
                });
            }
            ResultContextMenuKind::Timer { id } => {
                menu_resp.clone().context_menu(|ui| {
                    if ui.button("Pause Timer").clicked() {
                        crate::plugins::timer::pause_timer(id);
                        if query.starts_with("timer list") {
                            *refresh = true;
                            *set_focus = true;
                            if self.enable_toasts {
                                push_toast(
                                    &mut self.toasts,
                                    Toast {
                                        text: format!("Paused timer {}", action.label).into(),
                                        kind: ToastKind::Success,
                                        options: ToastOptions::default()
                                            .duration_in_seconds(self.toast_duration as f64),
                                    },
                                );
                            }
                        }
                        ui.close_menu();
                    }
                    if ui.button("Remove Timer").clicked() {
                        crate::plugins::timer::cancel_timer(id);
                        if query.starts_with("timer list") {
                            *refresh = true;
                            *set_focus = true;
                            if self.enable_toasts {
                                push_toast(
                                    &mut self.toasts,
                                    Toast {
                                        text: format!("Removed timer {}", action.label).into(),
                                        kind: ToastKind::Success,
                                        options: ToastOptions::default()
                                            .duration_in_seconds(self.toast_duration as f64),
                                    },
                                );
                            }
                        }
                        ui.close_menu();
                    }
                    if let Some(idx_act) = custom_idx
                        && ui.button("Edit App").clicked()
                    {
                        self.editor.open_edit(idx_act, &self.actions[idx_act]);
                        self.show_editor = true;
                        ui.close_menu();
                    }
                    self.pin_result_menu(ui, action);
                });
            }
            ResultContextMenuKind::Stopwatch { id } => {
                menu_resp.clone().context_menu(|ui| {
                    if ui.button("Pause Stopwatch").clicked() {
                        crate::plugins::stopwatch::pause_stopwatch(id);
                        if query.starts_with("sw list") {
                            *refresh = true;
                            *set_focus = true;
                            if self.enable_toasts {
                                push_toast(
                                    &mut self.toasts,
                                    Toast {
                                        text: format!("Paused stopwatch {}", action.label).into(),
                                        kind: ToastKind::Success,
                                        options: ToastOptions::default()
                                            .duration_in_seconds(self.toast_duration as f64),
                                    },
                                );
                            }
                        }
                        ui.close_menu();
                    }
                    if ui.button("Resume Stopwatch").clicked() {
                        crate::plugins::stopwatch::resume_stopwatch(id);
                        if query.starts_with("sw list") {
                            *refresh = true;
                            *set_focus = true;
                            if self.enable_toasts {
                                push_toast(
                                    &mut self.toasts,
                                    Toast {
                                        text: format!("Resumed stopwatch {}", action.label).into(),
                                        kind: ToastKind::Success,
                                        options: ToastOptions::default()
                                            .duration_in_seconds(self.toast_duration as f64),
                                    },
                                );
                            }
                        }
                        ui.close_menu();
                    }
                    if ui.button("Stop Stopwatch").clicked() {
                        crate::plugins::stopwatch::stop_stopwatch(id);
                        if query.starts_with("sw list") {
                            *refresh = true;
                            *set_focus = true;
                            if self.enable_toasts {
                                push_toast(
                                    &mut self.toasts,
                                    Toast {
                                        text: format!("Stopped stopwatch {}", action.label).into(),
                                        kind: ToastKind::Success,
                                        options: ToastOptions::default()
                                            .duration_in_seconds(self.toast_duration as f64),
                                    },
                                );
                            }
                        }
                        ui.close_menu();
                    }
                    if ui.button("Copy Time").clicked() {
                        if let Some(time) = crate::plugins::stopwatch::format_elapsed(id) {
                            if let Err(e) = crate::actions::clipboard::set_text(&time) {
                                self.report_error_message(
                                    "launcher",
                                    format!("Failed to copy time: {e}"),
                                );
                            } else if self.enable_toasts {
                                push_toast(
                                    &mut self.toasts,
                                    Toast {
                                        text: format!("Copied {time}").into(),
                                        kind: ToastKind::Success,
                                        options: ToastOptions::default()
                                            .duration_in_seconds(self.toast_duration as f64),
                                    },
                                );
                            }
                        }
                        ui.close_menu();
                    }
                    if let Some(idx_act) = custom_idx
                        && ui.button("Edit App").clicked()
                    {
                        self.editor.open_edit(idx_act, &self.actions[idx_act]);
                        self.show_editor = true;
                        ui.close_menu();
                    }
                    self.pin_result_menu(ui, action);
                });
            }
            ResultContextMenuKind::Snippet => {
                menu_resp.clone().context_menu(|ui| {
                    if ui.button("Edit Snippet").clicked() {
                        self.snippet_dialog.open_edit(&action.label);
                        ui.close_menu();
                    }
                    if ui.button("Remove Snippet").clicked() {
                        if let Err(e) = remove_snippet(SNIPPETS_FILE, &action.label) {
                            self.report_error_message(
                                "launcher",
                                format!("Failed to remove snippet: {e}"),
                            );
                        } else {
                            *refresh = true;
                            *set_focus = true;
                            if self.enable_toasts {
                                push_toast(
                                    &mut self.toasts,
                                    Toast {
                                        text: format!("Removed snippet {}", action.label).into(),
                                        kind: ToastKind::Success,
                                        options: ToastOptions::default()
                                            .duration_in_seconds(self.toast_duration as f64),
                                    },
                                );
                            }
                        }
                        ui.close_menu();
                    }
                    if let Some(idx_act) = custom_idx
                        && ui.button("Edit App").clicked()
                    {
                        self.editor.open_edit(idx_act, &self.actions[idx_act]);
                        self.show_editor = true;
                        ui.close_menu();
                    }
                    self.pin_result_menu(ui, action);
                });
            }
            ResultContextMenuKind::Tempfile => {
                let file_path = action.action.clone();
                menu_resp.clone().context_menu(|ui| {
                    if ui.button("Set Alias").clicked() {
                        self.tempfile_alias_dialog.open(&file_path);
                        ui.close_menu();
                    }
                    if ui.button("Delete File").clicked() {
                        if let Err(e) =
                            crate::plugins::tempfile::remove_file(std::path::Path::new(&file_path))
                        {
                            self.report_error_message(
                                "launcher",
                                format!("Failed to delete file: {e}"),
                            );
                        } else {
                            *refresh = true;
                            *set_focus = true;
                            if self.enable_toasts {
                                push_toast(
                                    &mut self.toasts,
                                    Toast {
                                        text: format!("Removed file {}", action.label).into(),
                                        kind: ToastKind::Success,
                                        options: ToastOptions::default()
                                            .duration_in_seconds(self.toast_duration as f64),
                                    },
                                );
                            }
                        }
                        ui.close_menu();
                    }
                    if let Some(idx_act) = custom_idx
                        && ui.button("Edit App").clicked()
                    {
                        self.editor.open_edit(idx_act, &self.actions[idx_act]);
                        self.show_editor = true;
                        ui.close_menu();
                    }
                    self.pin_result_menu(ui, action);
                });
            }
            ResultContextMenuKind::Note { slug } => {
                menu_resp.clone().context_menu(|ui| {
                    if ui.button("Edit Note").clicked() {
                        self.open_note_panel(&slug, None);
                        ui.close_menu();
                    }
                    if ui.button("Open in Notepad").clicked() {
                        match crate::plugins::note::load_notes() {
                            Ok(notes) => {
                                if let Some(note) = notes.iter().find(|n| n.slug == slug) {
                                    if let Err(e) = std::process::Command::new("notepad.exe")
                                        .arg(&note.path)
                                        .spawn()
                                    {
                                        self.report_error_message("launcher", e.to_string());
                                    }
                                } else {
                                    self.report_error_message(
                                        "launcher",
                                        "Note not found".to_string(),
                                    );
                                }
                            }
                            Err(e) => {
                                self.report_error_message("launcher", e.to_string());
                            }
                        }
                        ui.close_menu();
                    }
                    if ui.button("Open in Neovim").clicked()
                        && self.open_note_in_neovim(
                            &slug,
                            crate::plugins::note::load_notes,
                            |path| spawn_external(path, NoteExternalOpen::Wezterm),
                        )
                    {
                        ui.close_menu();
                    }
                    if ui.button("Remove Note").clicked() {
                        self.delete_note(&slug);
                        *refresh = true;
                        *set_focus = true;
                        ui.close_menu();
                    }
                    if let Some(idx_act) = custom_idx
                        && ui.button("Edit App").clicked()
                    {
                        self.editor.open_edit(idx_act, &self.actions[idx_act]);
                        self.show_editor = true;
                        ui.close_menu();
                    }
                    self.pin_result_menu(ui, action);
                });
            }
            ResultContextMenuKind::Clipboard { idx, label } => {
                menu_resp.clone().context_menu(|ui| {
                    if ui.button("Edit Entry").clicked() {
                        self.clipboard_dialog.open_edit(idx);
                        ui.close_menu();
                    }
                    if ui.button("Remove Entry").clicked() {
                        if let Err(e) = crate::plugins::clipboard::remove_entry(
                            crate::plugins::clipboard::CLIPBOARD_FILE,
                            idx,
                        ) {
                            self.report_error_message(
                                "launcher",
                                format!("Failed to remove entry: {e}"),
                            );
                        } else {
                            *refresh = true;
                            *set_focus = true;
                            if self.enable_toasts {
                                push_toast(
                                    &mut self.toasts,
                                    Toast {
                                        text: format!("Removed entry {}", label).into(),
                                        kind: ToastKind::Success,
                                        options: ToastOptions::default()
                                            .duration_in_seconds(self.toast_duration as f64),
                                    },
                                );
                            }
                        }
                        ui.close_menu();
                    }
                    if let Some(idx_act) = custom_idx
                        && ui.button("Edit App").clicked()
                    {
                        self.editor.open_edit(idx_act, &self.actions[idx_act]);
                        self.show_editor = true;
                        ui.close_menu();
                    }
                    self.pin_result_menu(ui, action);
                });
            }
            ResultContextMenuKind::Todo { idx } => {
                menu_resp.clone().context_menu(|ui| {
                    if ui.button("Edit Todo").clicked() {
                        self.todo_view_dialog.open_edit(idx);
                        ui.close_menu();
                    }
                    if let Some(idx_act) = custom_idx
                        && ui.button("Edit App").clicked()
                    {
                        self.editor.open_edit(idx_act, &self.actions[idx_act]);
                        self.show_editor = true;
                        ui.close_menu();
                    }
                    self.pin_result_menu(ui, action);
                });
            }
            ResultContextMenuKind::Default => {
                menu_resp.clone().context_menu(|ui| {
                    if let Some(idx_act) = custom_idx
                        && ui.button("Edit App").clicked()
                    {
                        self.editor.open_edit(idx_act, &self.actions[idx_act]);
                        self.show_editor = true;
                        ui.close_menu();
                    }
                    self.pin_result_menu(ui, action);
                });
            }
        }

        menu_resp
    }

    fn pin_result_menu(&mut self, ui: &mut egui::Ui, action: &Action) {
        ui.separator();
        let pins = history::load_pins(HISTORY_PINS_FILE).unwrap_or_default();
        let is_pinned = pins.iter().any(|pin| pin.matches_action(action));
        let pin = HistoryPin {
            action_id: action.action.clone(),
            label: action.label.clone(),
            desc: action.desc.clone(),
            args: action.args.clone(),
            query: self.query.clone(),
            timestamp: chrono::Utc::now().timestamp(),
        };

        if !is_pinned {
            if ui.button("Pin current query result").clicked() {
                match history::upsert_pin(HISTORY_PINS_FILE, &pin) {
                    Ok(_) => {
                        if self.enable_toasts {
                            push_toast(
                                &mut self.toasts,
                                Toast {
                                    text: format!("Pinned {}", action.label).into(),
                                    kind: ToastKind::Success,
                                    options: ToastOptions::default()
                                        .duration_in_seconds(self.toast_duration as f64),
                                },
                            );
                        }
                    }
                    Err(e) => {
                        self.report_error_message("launcher", format!("Failed to pin result: {e}"));
                    }
                }
                ui.close_menu();
            }
        } else {
            if ui.button("Unpin result").clicked() {
                if let Err(e) =
                    history::remove_pin(HISTORY_PINS_FILE, &action.action, action.args.as_deref())
                {
                    self.report_error_message("launcher", format!("Failed to unpin result: {e}"));
                } else if self.enable_toasts {
                    push_toast(
                        &mut self.toasts,
                        Toast {
                            text: format!("Unpinned {}", action.label).into(),
                            kind: ToastKind::Success,
                            options: ToastOptions::default()
                                .duration_in_seconds(self.toast_duration as f64),
                        },
                    );
                }
                ui.close_menu();
            }
            if ui.button("Replace pin with current result").clicked() {
                match history::upsert_pin(HISTORY_PINS_FILE, &pin) {
                    Ok(_) => {
                        if self.enable_toasts {
                            push_toast(
                                &mut self.toasts,
                                Toast {
                                    text: format!("Updated pin for {}", action.label).into(),
                                    kind: ToastKind::Success,
                                    options: ToastOptions::default()
                                        .duration_in_seconds(self.toast_duration as f64),
                                },
                            );
                        }
                    }
                    Err(e) => {
                        self.report_error_message("launcher", format!("Failed to update pin: {e}"));
                    }
                }
                ui.close_menu();
            }
        }

        if ui.button("Recompute pinned results").clicked() {
            match history::recompute_pins(HISTORY_PINS_FILE, |pin| self.resolve_pin_action(pin)) {
                Ok(report) => {
                    if self.enable_toasts {
                        let text = if report.updated == 0 && report.missing == 0 {
                            "Pinned results are up to date.".to_string()
                        } else if report.updated > 0 && report.missing > 0 {
                            format!(
                                "Updated {} pinned results ({} missing).",
                                report.updated, report.missing
                            )
                        } else if report.updated > 0 {
                            format!("Updated {} pinned results.", report.updated)
                        } else {
                            format!("{} pinned results missing.", report.missing)
                        };
                        push_toast(
                            &mut self.toasts,
                            Toast {
                                text: text.into(),
                                kind: if report.missing > 0 {
                                    ToastKind::Warning
                                } else {
                                    ToastKind::Success
                                },
                                options: ToastOptions::default()
                                    .duration_in_seconds(self.toast_duration as f64),
                            },
                        );
                    }
                }
                Err(e) => {
                    self.report_error_message("launcher", format!("Failed to recompute pins: {e}"));
                }
            }
            ui.close_menu();
        }
    }
}

impl eframe::App for LauncherApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        use egui::*;

        // tracing::debug!("LauncherApp::update called");
        if let Some(hwnd) = crate::window_manager::get_hwnd(_frame) {
            self.launcher_hwnd = Some(hwnd.0 as usize);
        }
        self.multi_manager_drain_runtime_events();
        let _ = self.multi_manager.start_pending_automatic_reconnect();
        if self
            .multi_manager
            .reconnect_in_progress
            .load(Ordering::Acquire)
        {
            ctx.request_repaint_after(Duration::from_millis(150));
        }
        self.multi_manager.maybe_auto_save_bindings();
        if self.enable_toasts {
            self.toasts.show(ctx);
        }
        let frame_time = Duration::from_secs_f32(ctx.input(|i| i.unstable_dt).max(0.0));
        self.dashboard.update_frame_timing(frame_time);
        if let Some(pending) = self.pending_query.take() {
            self.query = pending;
            self.search();
            self.focus_input();
        }
        self.maybe_run_note_search_debounce();
        if let (Some(t), Some(_)) = (self.error_time, self.error.as_ref())
            && t.elapsed().as_secs_f32() >= 3.0
        {
            self.error = None;
            self.error_time = None;
        }
        if self
            .enabled_capabilities
            .as_ref()
            .and_then(|m| m.get("timer"))
            .map(|c| c.contains(&"completion_dialog".to_string()))
            .unwrap_or(true)
        {
            for msg in crate::plugins::timer::take_finished_messages() {
                self.completion_dialog.open_message(msg);
            }
        }
        for msg in crate::plugins::macros::take_step_messages() {
            if self.enable_toasts {
                push_toast(
                    &mut self.toasts,
                    Toast {
                        text: msg.into(),
                        kind: ToastKind::Info,
                        options: ToastOptions::default()
                            .duration_in_seconds(self.toast_duration as f64),
                    },
                );
            }
        }
        for msg in crate::plugins::browser_tabs::take_cache_messages() {
            if self.enable_toasts {
                push_toast(
                    &mut self.toasts,
                    Toast {
                        text: msg.into(),
                        kind: ToastKind::Info,
                        options: ToastOptions::default()
                            .duration_in_seconds(self.toast_duration as f64),
                    },
                );
            }
        }
        for err in crate::plugins::macros::take_error_messages() {
            tracing::debug!("{err}");
        }

        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        self.handle_dropped_files(dropped);
        if let Some(rect) = ctx.input(|i| i.viewport().inner_rect) {
            self.window_size = (rect.width() as i32, rect.height() as i32);
        }
        if let Some(rect) = ctx.input(|i| i.viewport().outer_rect) {
            self.window_pos = (rect.min.x as i32, rect.min.y as i32);
        }
        let do_restore = self.restore_flag.swap(false, Ordering::SeqCst);
        if self.visible_flag.load(Ordering::SeqCst) && self.help_flag.swap(false, Ordering::SeqCst)
        {
            self.help_window.overlay_open = !self.help_window.overlay_open;
        } else {
            // reset any queued toggle when window not visible
            self.help_flag.store(false, Ordering::SeqCst);
        }
        if do_restore {
            tracing::debug!("Restoring window on restore_flag");
            apply_visibility(
                true,
                ctx,
                self.offscreen_pos,
                self.follow_mouse,
                self.static_location_enabled,
                self.static_pos.map(|(x, y)| (x as f32, y as f32)),
                self.static_size.map(|(w, h)| (w as f32, h as f32)),
                (self.window_size.0 as f32, self.window_size.1 as f32),
            );
            if let Some(hwnd) = crate::window_manager::get_hwnd(_frame) {
                crate::window_manager::force_restore_and_foreground(hwnd);
            }
        }

        let should_be_visible = self.visible_flag.load(Ordering::SeqCst);
        let just_became_visible = !self.last_visible && should_be_visible;
        if self.last_visible != should_be_visible {
            tracing::debug!("gui thread -> visible: {}", should_be_visible);
            apply_visibility(
                should_be_visible,
                ctx,
                self.offscreen_pos,
                self.follow_mouse,
                self.static_location_enabled,
                self.static_pos.map(|(x, y)| (x as f32, y as f32)),
                self.static_size.map(|(w, h)| (w as f32, h as f32)),
                (self.window_size.0 as f32, self.window_size.1 as f32),
            );
            self.last_visible = should_be_visible;
        }

        TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    ui.menu_button("Apps", |ui| {
                        if ui.button("Edit Apps").clicked() {
                            self.show_editor = !self.show_editor;
                        }
                        if ui.button("Edit Plugins").clicked() {
                            self.show_plugins = !self.show_plugins;
                        }
                    });
                    if ui.button("Close Application").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        self.unregister_all_hotkeys();
                        self.visible_flag.store(false, Ordering::SeqCst);
                        #[allow(unused_assignments)]
                        {
                            self.last_visible = false;
                        }
                        #[cfg(not(test))]
                        std::process::exit(0);
                    }
                });
                ui.menu_button("Settings", |ui| {
                    if ui.button("Edit Settings").clicked() {
                        self.show_settings = !self.show_settings;
                    }
                });
                ui.menu_button("Help", |ui| {
                    if ui.button("Command List").clicked() {
                        self.help_window.open = true;
                    }
                    if ui.button("Linking Guide (todo/note/cal)").clicked() {
                        self.help_window.open = true;
                        self.help_window.filter = "todo note cal @note: @todo:".into();
                    }
                    if ui.button("Quick Help Overlay").clicked() {
                        self.help_window.overlay_open = true;
                    }
                    if ui.button("Open Toast Log").clicked() {
                        if std::fs::OpenOptions::new()
                            .create(true)
                            .write(true)
                            .open(TOAST_LOG_FILE)
                            .is_err()
                        {
                            self.report_error_message("launcher", "Failed to create log");
                        } else if let Err(e) = open::that(TOAST_LOG_FILE) {
                            self.report_error_message(
                                "launcher",
                                format!("Failed to open log: {e}"),
                            );
                        }
                    }
                    if ui.button("View Toast Log").clicked() {
                        self.toast_log_dialog.open();
                    }
                });
                for panel in self.pinned_panels.clone() {
                    let label = format!("{:?}", panel);
                    if ui.button(label).clicked() {
                        if self.panel_stack.last() == Some(&panel) {
                            self.toggle_pin(panel);
                        } else {
                            self.focus_panel(panel);
                        }
                    }
                }
            });
        });

        self.process_watch_events();

        let trimmed = self.query.trim().to_string();
        let use_dashboard = self.should_show_dashboard(trimmed.as_str());
        self.maybe_refresh_timer_list();
        self.maybe_refresh_stopwatch_list();
        if trimmed.eq_ignore_ascii_case("net")
            && self.last_net_update.elapsed().as_secs_f32() >= self.net_refresh
        {
            self.search();
            self.last_net_update = Instant::now();
        }

        CentralPanel::default().show(ctx, |ui| {
            ui.heading("🚀 Multi Lnchr");
            if self.should_render_inline_error()
                && let Some(err) = &self.error {
                    ui.colored_label(Color32::RED, err);
                }

            scale_ui(ui, self.query_scale, |ui| {
                let input_id = egui::Id::new("query_input");

                if self.move_cursor_end {
                    if ui.ctx().memory(|m| m.has_focus(input_id)) {
                        let len = self.query.chars().count();
                        tracing::debug!("moving cursor to end: {len}");
                        ui.ctx().data_mut(|data| {
                            let state = data
                                .get_persisted_mut_or_default::<egui::widgets::text_edit::TextEditState>(
                                    input_id,
                                );
                            state.cursor.set_char_range(Some(egui::text::CCursorRange::one(
                                egui::text::CCursor::new(len),
                            )));
                        });
                        crate::window_manager::send_end_key();
                        self.move_cursor_end = false;
                        tracing::debug!("move_cursor_end cleared after moving");
                    } else {
                        tracing::debug!("cursor not moved - input not focused");
                    }
                }

                let query_response = ui.add(
                    egui::TextEdit::singleline(&mut self.query)
                        .id_source(input_id)
                        .desired_width(f32::INFINITY),
                );
                if Self::launcher_query_focus_should_be_requested(
                    just_became_visible,
                    self.focus_query,
                    self.file_search_dialog.open,
                ) {
                    query_response.request_focus();
                    self.focus_query = false;
                }
                let query_has_focus = query_response.has_focus();

                if query_response.changed() {
                    self.autocomplete_index = 0;
                    if Self::is_note_search_query(&self.query) {
                        self.last_note_search_change = Some(Instant::now());
                    } else {
                        self.last_note_search_change = None;
                        self.search();
                    }
                }

                if self.query_autocomplete && !use_dashboard && !self.suggestions.is_empty() {
                    ui.vertical(|ui| {
                        for s in &self.suggestions {
                            ui.colored_label(Color32::GRAY, s);
                        }
                    });
                }

                if Self::launcher_escape_handling_enabled(self.file_search_dialog.open)
                    && ctx.input(|i| i.key_pressed(egui::Key::Escape))
                {
                    if self.any_panel_open() {
                        if self.close_front_dialog() {
                            ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
                        }
                    } else {
                        self.visible_flag.store(false, Ordering::SeqCst);
                    }
                }

                if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::W))
                    && self.any_panel_open()
                        && self.close_front_dialog() {
                            ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::W));
                        }

                if Self::launcher_query_keyboard_enabled(query_has_focus) {
                    for key in [
                        egui::Key::ArrowDown,
                        egui::Key::ArrowUp,
                        egui::Key::PageDown,
                        egui::Key::PageUp,
                        egui::Key::ArrowLeft,
                        egui::Key::ArrowRight,
                        egui::Key::Num8,
                        egui::Key::Num2,
                        egui::Key::Num4,
                        egui::Key::Num6,
                    ] {
                        if ctx.input(|i| i.key_pressed(key)) {
                            self.handle_key(key);
                        }
                    }
                }

                let tab = ctx.input(|i| i.key_pressed(egui::Key::Tab));
                let enter = ctx.input(|i| i.key_pressed(egui::Key::Enter));
                let mut accepted_suggestion = false;
                if Self::launcher_query_keyboard_enabled(query_has_focus)
                    && (tab || (enter && self.selected.is_none()))
                {
                    accepted_suggestion = self.accept_suggestion(tab);
                }
                if accepted_suggestion {
                    ctx.input_mut(|i| {
                        if tab {
                            i.consume_key(egui::Modifiers::NONE, egui::Key::Tab);
                        }
                        if enter {
                            i.consume_key(egui::Modifiers::NONE, egui::Key::Enter);
                        }
                    });
                }

                let mut launch_idx: Option<usize> = None;
                if !accepted_suggestion
                    && enter
                    && Self::launcher_enter_activation_enabled(
                        query_has_focus,
                        self.file_search_dialog.open,
                    )
                    && !self.bookmark_alias_dialog.open
                    && !self.tempfile_alias_dialog.open
                    && !self.tempfile_dialog.open
                    && !self.shell_cmd_dialog.open
                    && !self.notes_dialog.open
                    && !self.todo_dialog.open
                    && !self.todo_view_dialog.open
                    && self.note_panels.is_empty()
                    && self.image_panels.is_empty()
                {
                    launch_idx = self.handle_key(egui::Key::Enter);
                }

                if let Some(i) = launch_idx
                    && let Some(a) = self.results.get(i) {
                        let a = a.clone();
                        self.activate_action(a, None, ActivationSource::Enter);
                    }
            });

            if use_dashboard {
                self.dashboard_data_cache
                    .flush_refresh_requests(&self.plugins);
                if !self.suggestions.is_empty() {
                    self.autocomplete_index = 0;
                    self.suggestions.clear();
                }
                let dashboard_visible = self.visible_flag.load(Ordering::SeqCst);
                let dashboard_focused = ctx.input(|i| i.viewport().focused).unwrap_or(true);
                let has_diagnostics_widget = self.has_diagnostics_widget();
                let show_diagnostics_widget =
                    self.show_dashboard_diagnostics || has_diagnostics_widget;
                let diagnostics = if self.show_dashboard_diagnostics || has_diagnostics_widget {
                    Some(self.dashboard.diagnostics_snapshot())
                } else {
                    None
                };
                let dash_ctx = DashboardContext {
                    actions: &self.actions,
                    actions_by_id: &self.actions_by_id,
                    usage: &self.usage,
                    plugins: &self.plugins,
                    enabled_plugins: self.enabled_plugins.as_ref(),
                    default_location: self.dashboard_default_location.as_deref(),
                    data_cache: &self.dashboard_data_cache,
                    actions_version: crate::actions::actions_version(),
                    fav_version: crate::plugins::fav::fav_version(),
                    notes_version: crate::plugins::note::note_version(),
                    todo_version: crate::plugins::todo::todo_version(),
                    calendar_version: crate::plugins::calendar::calendar_version(),
                    clipboard_version: crate::plugins::clipboard::clipboard_version(),
                    snippets_version: crate::plugins::snippets::snippets_version(),
                    dashboard_visible,
                    dashboard_focused,
                    reduce_dashboard_work_when_unfocused: self
                        .reduce_dashboard_work_when_unfocused,
                    diagnostics,
                    show_diagnostics_widget,
                };
                ctx.request_repaint_after(Duration::from_millis(250));
                if let Some(action) = self.dashboard.ui(ui, &dash_ctx, WidgetActivation::Click) {
                    self.activate_action(action.action, action.query_override, ActivationSource::Dashboard);
                }
            } else {
                let area_height = ui.available_height();
                ScrollArea::vertical()
                    .max_height(area_height)
                    .show(ui, |ui| {
                        scale_ui(ui, self.list_scale, |ui| {
                            let mut refresh = false;
                            let mut set_focus = false;
                            let show_full = self
                                .enabled_capabilities
                                .as_ref()
                                .and_then(|m| m.get("folders"))
                                .map(|caps| caps.contains(&"show_full_path".to_string()))
                                .unwrap_or(false);
                            if self.resolved_grid_layout {
                                let cols = self.query_results_layout.cols.max(1);
                                let col_width = ((ui.available_width()
                                    - ((cols.saturating_sub(1)) as f32 * 8.0))
                                    / cols as f32)
                                    .max(160.0);
                                egui::Grid::new("query_results_grid")
                                    .num_columns(cols)
                                    .spacing([8.0, 6.0])
                                    .show(ui, |ui| {
                                        for idx in 0..self.results.len() {
                                            let action = self.results[idx].clone();
                                            let text = format!("{}\n{}", action.label, action.desc);
                                            let resp = ui.add_sized(
                                                [col_width, 44.0],
                                                egui::SelectableLabel::new(
                                                    self.selected == Some(idx),
                                                    text,
                                                ),
                                            );
                                            let menu_resp = self.attach_result_context_menu(
                                                &action,
                                                resp,
                                                &mut refresh,
                                                &mut set_focus,
                                            );
                                            if self.selected == Some(idx) {
                                                menu_resp.scroll_to_me(Some(egui::Align::Center));
                                            }
                                            if menu_resp.clicked() {
                                                self.selected = Some(idx);
                                                self.activate_action(
                                                    action,
                                                    None,
                                                    ActivationSource::Click,
                                                );
                                            }
                                            if (idx + 1) % cols == 0 {
                                                ui.end_row();
                                            }
                                        }
                                    });
                            } else {
                                for idx in 0..self.results.len() {
                                    let a = self.results[idx].clone();
                                    let aliased =
                                        self.folder_aliases.get(&a.action).and_then(|v| v.as_ref());
                                    let show_path = show_full || aliased.is_none();
                                    let text = if show_path {
                                        format!("{} : {}", a.label, a.desc)
                                    } else {
                                        a.label.clone()
                                    };
                                    let resp = ui.add_sized(
                                        [ui.available_width(), 0.0],
                                        egui::SelectableLabel::new(self.selected == Some(idx), text),
                                    );
                                    let tooltip = if a.desc == "Timer" && a.action.starts_with("timer:show:") {
                                        if let Ok(id) = a.action[11..].parse::<u64>() {
                                            if let Some(ts) = crate::plugins::timer::timer_start_ts(id) {
                                                format!("Started {}", crate::plugins::timer::format_ts(ts))
                                            } else {
                                                a.action.clone()
                                            }
                                        } else {
                                            a.action.clone()
                                        }
                                    } else {
                                        a.action.clone()
                                    };
                                    let menu_resp =
                                        self.attach_result_context_menu(&a, resp.on_hover_text(tooltip), &mut refresh, &mut set_focus);
                                    if self.selected == Some(idx) {
                                        menu_resp.scroll_to_me(Some(egui::Align::Center));
                                    }
                                    if menu_resp.clicked() {
                                        self.selected = Some(idx);
                                        self.activate_action(a.clone(), None, ActivationSource::Click);
                                    }
                                }
                            }
                            if refresh {
                                self.last_results_valid = false;
                                self.search();
                            }
                            if set_focus {
                                self.focus_input();
                            } else if self.visible_flag.load(Ordering::SeqCst) && !self.any_panel_open()
                            {
                                self.focus_input();
                            }
                        });
                    });
            }
        });
        let show_editor = self.show_editor;
        if show_editor {
            let mut editor = std::mem::take(&mut self.editor);
            editor.ui(ctx, self);
            self.editor = editor;
        }
        let show_settings = self.show_settings;
        if show_settings {
            let mut ed = std::mem::take(&mut self.settings_editor);
            ed.ui(ctx, self);
            self.settings_editor = ed;
        }
        let show_plugin = self.show_plugins;
        if show_plugin {
            let mut ed = std::mem::take(&mut self.plugin_editor);
            ed.ui(ctx, self);
            self.plugin_editor = ed;
        }
        if self.show_dashboard_editor && !self.dashboard_editor.open {
            let registry = self.dashboard.registry().clone();
            self.dashboard_editor.open(&self.dashboard_path, &registry);
        }
        if self.show_dashboard_editor {
            let registry = self.dashboard.registry().clone();
            let mut dlg = std::mem::take(&mut self.dashboard_editor);
            let plugin_infos = self.plugins.plugin_infos();
            let plugin_commands = self.plugins.commands();
            let settings_ctx = WidgetSettingsContext {
                plugins: Some(&self.plugins),
                plugin_infos: Some(&plugin_infos),
                plugin_commands: Some(&plugin_commands),
                actions: Some(self.actions.as_slice()),
                usage: Some(&self.usage),
                default_location: self.dashboard_default_location.as_deref(),
                enabled_plugins: self.enabled_plugins.as_ref(),
            };
            let reload = dlg.ui(
                ctx,
                &registry,
                settings_ctx,
                self.require_confirm_destructive,
            );
            self.show_dashboard_editor = dlg.open;
            self.dashboard_editor = dlg;
            if reload {
                self.dashboard.reload();
            }
        }

        let mut mm_dlg = std::mem::take(&mut self.multi_manager_dialog);
        mm_dlg.ui(ctx, self);
        self.multi_manager_dialog = mm_dlg;
        let mut mm_settings_dlg = std::mem::take(&mut self.multi_manager_settings_dialog);
        mm_settings_dlg.ui(ctx, self);
        self.multi_manager_settings_dialog = mm_settings_dlg;
        let mut dlg = std::mem::take(&mut self.alias_dialog);
        dlg.ui(ctx, self);
        self.alias_dialog = dlg;
        let mut bm_dlg = std::mem::take(&mut self.bookmark_alias_dialog);
        bm_dlg.ui(ctx, self);
        self.bookmark_alias_dialog = bm_dlg;
        let mut tf_dlg = std::mem::take(&mut self.tempfile_alias_dialog);
        tf_dlg.ui(ctx, self);
        self.tempfile_alias_dialog = tf_dlg;
        let mut create_tf = std::mem::take(&mut self.tempfile_dialog);
        create_tf.ui(ctx, self);
        self.tempfile_dialog = create_tf;
        let mut add_bm_dlg = std::mem::take(&mut self.add_bookmark_dialog);
        add_bm_dlg.ui(ctx, self);
        self.add_bookmark_dialog = add_bm_dlg;
        let mut help = std::mem::take(&mut self.help_window);
        help.ui(ctx, self);
        self.help_window = help;
        let mut timer_dlg = std::mem::take(&mut self.timer_dialog);
        timer_dlg.ui(ctx, self);
        self.timer_dialog = timer_dlg;
        let mut comp = std::mem::take(&mut self.completion_dialog);
        comp.ui(ctx);
        self.completion_dialog = comp;
        let mut shell_dlg = std::mem::take(&mut self.shell_cmd_dialog);
        shell_dlg.ui(ctx, self);
        self.shell_cmd_dialog = shell_dlg;
        let mut snip_dlg = std::mem::take(&mut self.snippet_dialog);
        snip_dlg.ui(ctx, self);
        self.snippet_dialog = snip_dlg;
        let mut macro_dlg = std::mem::take(&mut self.macro_dialog);
        macro_dlg.ui(ctx, self);
        self.macro_dialog = macro_dlg;
        let mut mg_dlg = std::mem::take(&mut self.mouse_gestures_dialog);
        mg_dlg.ui(ctx, self);
        self.mouse_gestures_dialog = mg_dlg;
        let mut mg_settings_dlg = std::mem::take(&mut self.mouse_gesture_settings_dialog);
        mg_settings_dlg.ui(ctx, self);
        self.mouse_gesture_settings_dialog = mg_settings_dlg;
        let mut theme_state = std::mem::take(&mut self.theme_settings_dialog);
        let mut theme_open = self.theme_settings_dialog_open;
        crate::gui::theme_settings_dialog::ui(ctx, self, &mut theme_open, &mut theme_state);
        self.theme_settings_dialog_open = theme_open;
        self.theme_settings_dialog = theme_state;
        let mut fav_dlg = std::mem::take(&mut self.fav_dialog);
        fav_dlg.ui(ctx, self);
        self.fav_dialog = fav_dlg;
        let file_search_was_open = self.file_search_dialog.open;
        let file_search_commands = self
            .file_search_dialog
            .ui(ctx, &mut self.file_search_coordinator);
        for command in file_search_commands {
            self.handle_file_search_ui_command(command);
        }
        if file_search_was_open && !self.file_search_dialog.open {
            self.save_file_search_ui_preferences_if_dirty();
        }
        self.file_search_dialog
            .preview_dialog
            .ui(ctx, &self.file_search_dialog.settings);
        let mut notes_dlg = std::mem::take(&mut self.notes_dialog);
        notes_dlg.ui(ctx, self);
        self.notes_dialog = notes_dlg;
        let mut graph_dlg = std::mem::take(&mut self.note_graph_dialog);
        let data_cache: *const DashboardDataCache = &self.dashboard_data_cache;
        // SAFETY: `data_cache` points to a stable field on `self` for this call. The dialog
        // only reads through `&DashboardDataCache` while `self` is mutably borrowed for app
        // actions; no mutation of `dashboard_data_cache` occurs here.
        let data_cache = unsafe { &*data_cache };
        graph_dlg.ui(ctx, self, data_cache, crate::plugins::note::note_version());
        self.note_graph_dialog = graph_dlg;
        let mut assets_dlg = std::mem::take(&mut self.unused_assets_dialog);
        assets_dlg.ui(ctx, self);
        self.unused_assets_dialog = assets_dlg;
        let mut i = 0;
        while i < self.note_panels.len() {
            let mut panel = self.note_panels.remove(i);
            panel.ui(ctx, self);
            if panel.open {
                self.note_panels.insert(i, panel);
                i += 1;
            }
        }
        let mut i = 0;
        while i < self.image_panels.len() {
            let mut panel = self.image_panels.remove(i);
            panel.ui(ctx);
            if panel.open {
                self.image_panels.insert(i, panel);
                i += 1;
            }
        }
        let mut i = 0;
        while i < self.screenshot_editors.len() {
            let mut editor = self.screenshot_editors.remove(i);
            editor.ui(ctx, self);
            if editor.open {
                self.screenshot_editors.insert(i, editor);
                i += 1;
            }
        }
        let mut todo_dlg = std::mem::take(&mut self.todo_dialog);
        todo_dlg.ui(ctx, self);
        self.todo_dialog = todo_dlg;
        let mut todo_view = std::mem::take(&mut self.todo_view_dialog);
        todo_view.ui(ctx, self);
        self.todo_view_dialog = todo_view;
        let mut cb_dlg = std::mem::take(&mut self.clipboard_dialog);
        cb_dlg.ui(ctx, self);
        self.clipboard_dialog = cb_dlg;
        let mut conv_panel = std::mem::take(&mut self.convert_panel);
        conv_panel.ui(ctx, self);
        self.convert_panel = conv_panel;
        let mut vol_dlg = std::mem::take(&mut self.volume_dialog);
        vol_dlg.ui(ctx, self);
        self.volume_dialog = vol_dlg;
        let mut bright_dlg = std::mem::take(&mut self.brightness_dialog);
        bright_dlg.ui(ctx, self);
        self.brightness_dialog = bright_dlg;
        let mut cpu_dlg = std::mem::take(&mut self.cpu_list_dialog);
        cpu_dlg.ui(ctx, self);
        self.cpu_list_dialog = cpu_dlg;
        let mut toast_dlg = std::mem::take(&mut self.toast_log_dialog);
        toast_dlg.ui(ctx, self);
        self.toast_log_dialog = toast_dlg;
        let mut calendar_popover = std::mem::take(&mut self.calendar_popover);
        calendar_popover.ui(ctx, self);
        self.calendar_popover = calendar_popover;
        let mut calendar_editor = std::mem::take(&mut self.calendar_event_editor);
        calendar_editor.ui(ctx, self);
        self.calendar_event_editor = calendar_editor;
        let mut calendar_details = std::mem::take(&mut self.calendar_event_details);
        calendar_details.ui(ctx, self);
        self.calendar_event_details = calendar_details;
        match self.confirm_modal.ui(ctx) {
            ConfirmationResult::Confirmed => {
                if let Some(pending) = self.pending_confirm.take() {
                    self.activate_action_confirmed(
                        pending.action,
                        pending.query_override,
                        pending.source,
                    );
                }
            }
            ConfirmationResult::Cancelled => {
                self.pending_confirm = None;
            }
            ConfirmationResult::None => {}
        }
        self.enforce_pinned();
        self.update_panel_stack();
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.multi_manager.shutdown();
        let multi_manager_save_on_exit = crate::settings::Settings::load(&self.settings_path)
            .map(|settings| settings.multi_manager.save_on_exit)
            .unwrap_or(true);
        if multi_manager_save_on_exit && let Err(err) = self.multi_manager.save() {
            self.report_error("multi_manager.save_on_exit", err);
        }
        if let Err(err) = self.multi_manager.flush_bindings_if_dirty() {
            self.report_error("multi_manager.bindings.save_on_exit", err);
        }
        self.unregister_all_hotkeys();
        self.visible_flag.store(false, Ordering::SeqCst);
        self.last_visible = false;
        self.save_file_search_ui_preferences_if_dirty();
        if let Ok(mut settings) = crate::settings::Settings::load(&self.settings_path) {
            settings.window_size = Some(self.window_size);
            settings.pinned_panels = self.pinned_panels.clone();
            let _ = settings.save(&self.settings_path);
        }
        let _ = usage::save_usage(USAGE_FILE, &self.usage);
        #[cfg(not(test))]
        std::process::exit(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{plugin::PluginManager, settings::Settings};
    use eframe::egui;
    use std::sync::{Arc, atomic::AtomicBool};

    fn new_app(ctx: &egui::Context) -> LauncherApp {
        LauncherApp::new(
            ctx,
            Arc::new(Vec::new()),
            0,
            PluginManager::new(),
            "actions.json".into(),
            "settings.json".into(),
            Settings::default(),
            None,
            None,
            None,
            None,
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        )
    }

    #[test]
    fn list_and_grid_modes_share_context_menu_resolution() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let actions = vec![
            (
                Action {
                    label: "Bookmark".into(),
                    desc: "Web".into(),
                    action: "https://example.com".into(),
                    args: None,
                },
                ResultContextMenuKind::Bookmark,
            ),
            (
                Action {
                    label: "Todo".into(),
                    desc: "Todo".into(),
                    action: "todo:done:7".into(),
                    args: None,
                },
                ResultContextMenuKind::Todo { idx: 7 },
            ),
            (
                Action {
                    label: "Clipboard entry".into(),
                    desc: "Clipboard".into(),
                    action: "clipboard:copy:2".into(),
                    args: None,
                },
                ResultContextMenuKind::Clipboard {
                    idx: 2,
                    label: "Clipboard entry".into(),
                },
            ),
        ];
        app.bookmark_aliases
            .insert("https://example.com".into(), Some("Docs".into()));

        for (action, expected) in actions {
            app.resolved_grid_layout = false;
            let list_kind = app.result_context_menu_kind(&action);
            app.resolved_grid_layout = true;
            let grid_kind = app.result_context_menu_kind(&action);
            assert_eq!(list_kind, expected);
            assert_eq!(grid_kind, expected);
        }
    }

    #[test]
    fn keyboard_navigation_is_consistent_between_grid_and_list_modes() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.results = (0..8)
            .map(|i| Action {
                label: format!("A{i}"),
                desc: "d".into(),
                action: format!("act:{i}"),
                args: None,
            })
            .collect();

        app.resolved_grid_layout = false;
        app.selected = Some(1);
        app.handle_key(egui::Key::ArrowDown);
        app.handle_key(egui::Key::ArrowUp);
        assert_eq!(app.selected, Some(1));
        app.handle_key(egui::Key::ArrowRight);
        assert_eq!(app.selected, Some(1));

        app.resolved_grid_layout = true;
        app.query_results_layout.cols = 3;
        app.selected = Some(4);
        app.handle_key(egui::Key::ArrowLeft);
        app.handle_key(egui::Key::ArrowRight);
        assert_eq!(app.selected, Some(4));
        app.handle_key(egui::Key::ArrowUp);
        assert_eq!(app.selected, Some(1));
        app.handle_key(egui::Key::ArrowDown);
        assert_eq!(app.selected, Some(4));
    }

    #[test]
    fn pinned_panels_prevent_close_until_unpinned() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.pinned_panels.push(Panel::ClipboardDialog);
        app.clipboard_dialog.open = true;
        app.update_panel_stack();

        assert!(!app.close_front_dialog());
        assert!(app.clipboard_dialog.open);

        app.toggle_pin(Panel::ClipboardDialog);
        app.clipboard_dialog.open = true;
        app.update_panel_stack();
        assert!(app.close_front_dialog());
        assert!(!app.clipboard_dialog.open);
    }
}
