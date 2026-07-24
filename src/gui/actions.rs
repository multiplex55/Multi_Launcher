use super::*;
use crate::gui::note_mutation::{NoteMutationOutcome, NoteMutationOutput, NoteMutationResult};

fn pluralize<'a>(count: usize, singular: &'a str, plural: &'a str) -> &'a str {
    if count == 1 { singular } else { plural }
}

fn format_wrap_links_toast(result: NoteMutationResult) -> String {
    format!(
        "Wrapped {} {}; skipped {} existing {}.",
        result.wrapped_links,
        pluralize(result.wrapped_links, "link", "links"),
        result.skipped_existing_links,
        pluralize(result.skipped_existing_links, "link", "links"),
    )
}

fn validate_note_new_payload(slug: &str, template: Option<&str>) -> Result<(), String> {
    let slug_has_whitespace = slug.chars().any(char::is_whitespace);
    let slug_has_delimiter = slug.contains(':');
    let template_invalid = template.map(|tpl| tpl.trim().is_empty()).unwrap_or(false);
    if slug.is_empty() || slug_has_whitespace || slug_has_delimiter || template_invalid {
        return Err("Malformed note action".to_string());
    }
    Ok(())
}

impl LauncherApp {
    pub fn activate_action(
        &mut self,
        a: Action,
        query_override: Option<String>,
        source: ActivationSource,
    ) {
        if self.maybe_confirm_destructive_action(&a, query_override.clone(), source) {
            return;
        }
        self.activate_action_confirmed(a, query_override, source);
    }

    fn maybe_confirm_destructive_action(
        &mut self,
        a: &Action,
        query_override: Option<String>,
        source: ActivationSource,
    ) -> bool {
        if !self.require_confirm_destructive {
            return false;
        }
        if let Some(kind) = DestructiveAction::from_action(a) {
            self.pending_confirm = Some(PendingConfirmAction {
                action: a.clone(),
                query_override,
                source,
            });
            self.confirm_modal.open_for_source(kind, Some(source));
            return true;
        }
        false
    }

    pub(crate) fn activate_action_confirmed(
        &mut self,
        a: Action,
        query_override: Option<String>,
        source: ActivationSource,
    ) {
        if self.handle_clipboard_modify_action(&a, source) {
            return;
        }
        if self.handle_file_search_action(&a.action) {
            return;
        }
        if let Some(new_query) = query_override {
            self.query = new_query;
            self.last_timer_query =
                self.query.starts_with("timer list") || self.query.starts_with("alarm list");
            self.search();
        }
        let mut focus_after_launcher = false;
        if a.action == "launcher:show"
            && let Some(query) = a.args.as_ref()
        {
            self.query = query.to_string();
            self.last_timer_query =
                query.starts_with("timer list") || query.starts_with("alarm list");
            self.search();
            self.move_cursor_end = true;
            focus_after_launcher = true;
        }
        if self.handle_launcher_action(&a.action) {
            if focus_after_launcher {
                self.focus_input();
            }
            return;
        }
        let current = self.query.clone();
        let mut refresh = false;
        let mut set_focus = false;
        let mut command_changed_query = false;
        if let Some(new_q) = a.action.strip_prefix("queryexec:") {
            tracing::debug!("queryexec action via activation: {new_q}");
            self.query = new_q.to_string();
            self.last_timer_query =
                new_q.starts_with("timer list") || new_q.starts_with("alarm list");
            self.search();
            self.move_cursor_end = true;
            if let Some(action) = self.results.first().cloned() {
                self.activate_action(action, None, source);
            }
            return;
        } else if let Some(new_q) = a.action.strip_prefix("query:") {
            tracing::debug!("query action via activation: {new_q}");
            self.query = if let Some(query_arg) = a
                .args
                .as_deref()
                .and_then(|args| serde_json::from_str::<serde_json::Value>(args).ok())
                .and_then(|value| {
                    value
                        .get("query")
                        .and_then(|query| query.as_str())
                        .map(str::to_string)
                }) {
                format!("{} {}", new_q.trim_end(), query_arg)
            } else {
                new_q.to_string()
            };
            self.last_timer_query =
                new_q.starts_with("timer list") || new_q.starts_with("alarm list");
            self.search();
            self.move_cursor_end = true;
            self.focus_input();
            return;
        } else if a.action == "help:show" {
            self.help_window.open = true;
        } else if a.action == "timer:dialog:timer" {
            self.timer_dialog.open_timer();
        } else if a.action == "timer:dialog:alarm" {
            self.timer_dialog.open_alarm();
        } else if a.action == "calendar:open" || a.action.starts_with("calendar:open:") {
            let view = a.action.strip_prefix("calendar:open:").unwrap_or("default");
            let now = chrono::Local::now().naive_local();
            let mut state =
                crate::plugins::calendar::load_state(crate::plugins::calendar::CALENDAR_STATE_FILE)
                    .unwrap_or_default();
            state.last_opened = Some(now);
            state.last_viewed_day = Some(now.date());
            if let Err(err) = crate::plugins::calendar::save_state(
                crate::plugins::calendar::CALENDAR_STATE_FILE,
                &state,
            ) {
                self.add_error_toast(format!("Calendar state error: {err}"));
            }
            if self.dashboard_enabled {
                self.query.clear();
                command_changed_query = true;
                refresh = true;
                set_focus = true;
            }
            self.open_calendar_popover(Some(now.date()));
            if self.enable_toasts {
                let label = if view == "default" {
                    "Opened calendar".to_string()
                } else {
                    format!("Opened calendar ({view} view)")
                };
                push_toast(
                    &mut self.toasts,
                    Toast {
                        text: label.into(),
                        kind: ToastKind::Success,
                        options: ToastOptions::default()
                            .duration_in_seconds(self.toast_duration as f64),
                    },
                );
            }
        } else if let Some(reference) = a.action.strip_prefix("calendar:jump:") {
            let now = chrono::Local::now().naive_local();
            match crate::plugins::calendar::parse_date_reference(reference, now.date()) {
                Some(date) => {
                    let mut state = crate::plugins::calendar::load_state(
                        crate::plugins::calendar::CALENDAR_STATE_FILE,
                    )
                    .unwrap_or_default();
                    state.last_opened = Some(now);
                    state.last_viewed_day = Some(date);
                    if let Err(err) = crate::plugins::calendar::save_state(
                        crate::plugins::calendar::CALENDAR_STATE_FILE,
                        &state,
                    ) {
                        self.add_error_toast(format!("Calendar state error: {err}"));
                    }
                    if self.dashboard_enabled {
                        self.query.clear();
                        command_changed_query = true;
                        refresh = true;
                        set_focus = true;
                    }
                    if self.enable_toasts {
                        push_toast(
                            &mut self.toasts,
                            Toast {
                                text: format!("Jumped to {}", date.format("%Y-%m-%d")).into(),
                                kind: ToastKind::Success,
                                options: ToastOptions::default()
                                    .duration_in_seconds(self.toast_duration as f64),
                            },
                        );
                    }
                }
                None => {
                    self.add_error_toast(format!("Invalid date reference: {reference}"));
                }
            }
        } else if let Some(input) = a.action.strip_prefix("calendar:add:") {
            let now = chrono::Local::now().naive_local();
            match crate::plugins::calendar::parse_calendar_add(input, now) {
                Ok(request) => match crate::plugins::calendar::add_event(request, now) {
                    Ok(event) => {
                        self.dashboard_data_cache.refresh_calendar();
                        if self.preserve_command {
                            self.query = "cal add ".into();
                        } else {
                            self.query.clear();
                        }
                        command_changed_query = true;
                        refresh = true;
                        set_focus = true;
                        if self.enable_toasts {
                            push_toast(
                                &mut self.toasts,
                                Toast {
                                    text: format!("Added {}", event.title).into(),
                                    kind: ToastKind::Success,
                                    options: ToastOptions::default()
                                        .duration_in_seconds(self.toast_duration as f64),
                                },
                            );
                        }
                    }
                    Err(err) => {
                        self.add_error_toast(format!("Calendar add failed: {err}"));
                    }
                },
                Err(err) => {
                    self.add_error_toast(err);
                }
            }
        } else if let Some(input) = a.action.strip_prefix("calendar:search:") {
            match crate::plugins::calendar::parse_calendar_search(input) {
                Ok(request) => {
                    let results = crate::plugins::calendar::search_events(&request);
                    let actions: Vec<Action> = results
                        .into_iter()
                        .map(|event| Action {
                            label: crate::plugins::calendar::format_event_label(&event),
                            desc: "Calendar".into(),
                            action: format!("calendar:jump:{}", event.start.format("%Y-%m-%d")),
                            args: None,
                        })
                        .collect();
                    self.query = format!("cal find {input}");
                    self.results = actions;
                    self.selected = None;
                    self.last_search_query = self.query.clone();
                    self.last_results_valid = true;
                    self.update_suggestions();
                    command_changed_query = true;
                    set_focus = true;
                    if self.enable_toasts {
                        push_toast(
                            &mut self.toasts,
                            Toast {
                                text: format!("Found {} events", self.results.len()).into(),
                                kind: ToastKind::Info,
                                options: ToastOptions::default()
                                    .duration_in_seconds(self.toast_duration as f64),
                            },
                        );
                    }
                }
                Err(err) => {
                    self.add_error_toast(err);
                }
            }
        } else if a.action == "calendar:upcoming" {
            let now = chrono::Local::now().naive_local();
            let events = crate::plugins::calendar::CALENDAR_DATA
                .read()
                .map(|d| d.clone())
                .unwrap_or_default();
            let until = now + chrono::Duration::days(7);
            let instances = crate::plugins::calendar::expand_instances(&events, now, until, 50);
            let titles: std::collections::HashMap<_, _> =
                events.into_iter().map(|e| (e.id, e.title)).collect();
            self.query = "cal upcoming".into();
            self.results = instances
                .into_iter()
                .map(|instance| {
                    let title = titles
                        .get(&instance.source_event_id)
                        .cloned()
                        .unwrap_or_else(|| "Calendar event".to_string());
                    let label = if instance.all_day {
                        format!("{} ({} all-day)", title, instance.start.format("%Y-%m-%d"))
                    } else {
                        format!(
                            "{} ({} {})",
                            title,
                            instance.start.format("%Y-%m-%d"),
                            instance.start.format("%H:%M")
                        )
                    };
                    Action {
                        label,
                        desc: "Calendar".into(),
                        action: format!("calendar:jump:{}", instance.start.format("%Y-%m-%d")),
                        args: None,
                    }
                })
                .collect();
            self.selected = None;
            self.last_search_query = self.query.clone();
            self.last_results_valid = true;
            self.update_suggestions();
            command_changed_query = true;
            set_focus = true;
        } else if let Some(input) = a.action.strip_prefix("calendar:snooze:") {
            let mut parts = input.split_whitespace();
            if let (Some(duration_str), Some(event_id)) = (parts.next(), parts.next()) {
                if let Some(duration) = crate::plugins::calendar::parse_duration_spec(duration_str)
                {
                    match crate::plugins::calendar::snooze_event(event_id, duration) {
                        Ok(true) => {
                            self.dashboard_data_cache.refresh_calendar();
                            if self.enable_toasts {
                                push_toast(
                                    &mut self.toasts,
                                    Toast {
                                        text: format!("Snoozed event {event_id}").into(),
                                        kind: ToastKind::Success,
                                        options: ToastOptions::default()
                                            .duration_in_seconds(self.toast_duration as f64),
                                    },
                                );
                            }
                        }
                        Ok(false) => {
                            self.add_error_toast(format!("Event not found: {event_id}"));
                        }
                        Err(err) => {
                            self.add_error_toast(format!("Snooze failed: {err}"));
                        }
                    }
                } else {
                    self.add_error_toast("Invalid snooze duration (use 10m, 1h, 2d)");
                }
            } else {
                self.add_error_toast("Provide a duration and event id to snooze");
            }
        } else if a.action == "shell:dialog" {
            self.shell_cmd_dialog.open();
        } else if a.action == "note:dialog" {
            self.notes_dialog.open();
        } else if a.action == "note:graph_dialog" {
            self.note_graph_dialog.open_with_args(a.args.as_deref());
        } else if a.action == "note:unused_assets" {
            self.unused_assets_dialog.open();
        } else if a.action == "bookmark:dialog" {
            self.add_bookmark_dialog.open();
        } else if a.action == "snippet:dialog" {
            self.snippet_dialog.open();
        } else if let Some(alias) = a.action.strip_prefix("snippet:edit:") {
            self.snippet_dialog.open_edit(alias);
        } else if a.action == "macro:dialog" {
            self.macro_dialog.open();
        } else if a.action == "mg:dialog" {
            self.mouse_gestures_dialog.open();
        } else if a.action == "mg:dialog:add" {
            self.mouse_gestures_dialog.open_add();
        } else if a.action == "mg:dialog:binding" {
            self.mouse_gestures_dialog.open_binding_editor();
        } else if a.action == "mg:dialog:focus" {
            if let Some(args) = a
                .args
                .as_deref()
                .and_then(|raw| serde_json::from_str::<GestureFocusArgs>(raw).ok())
            {
                self.mouse_gestures_dialog
                    .open_focus(&args.label, &args.tokens, args.dir_mode);
            } else {
                self.mouse_gestures_dialog.open();
            }
        } else if a.action == "mg:dialog:settings" {
            self.open_mouse_gesture_settings_dialog();
        } else if a.action == "mg:toggle" {
            if let Some(args) = a
                .args
                .as_deref()
                .and_then(|raw| serde_json::from_str::<GestureToggleArgs>(raw).ok())
            {
                let mut db = load_gestures(GESTURES_FILE).unwrap_or_default();
                if let Some(gesture) = db.gestures.iter_mut().find(|gesture| {
                    gesture.label == args.label
                        && gesture.tokens == args.tokens
                        && gesture.dir_mode == args.dir_mode
                }) {
                    gesture.enabled = args.enabled;
                    if let Err(err) = save_gestures(GESTURES_FILE, &db) {
                        self.report_error_message(
                            "launcher",
                            format!("Failed to save mouse gestures: {err}"),
                        );
                    } else {
                        self.dashboard_data_cache.refresh_gestures();
                    }
                }
            }
        } else if let Some(label) = a.action.strip_prefix("fav:dialog:") {
            if label.is_empty() {
                self.fav_dialog.open();
            } else {
                self.fav_dialog.open_edit(label);
            }
        } else if a.action == "todo:dialog" {
            self.todo_dialog.open();
        } else if a.action == "todo:view" {
            self.todo_view_dialog.open();
        } else if let Some(idx) = a.action.strip_prefix("todo:edit:") {
            if let Ok(i) = idx.parse::<usize>() {
                self.todo_view_dialog.open_edit(i);
            }
        } else if a.action == "clipboard:dialog" {
            self.clipboard_dialog.open();
        } else if let Some(slug) = a.action.strip_prefix("note:open:") {
            let slug = slug.to_string();
            self.open_note_panel(&slug, None);
        } else if let Some(encoded) = a
            .action
            .strip_prefix(crate::plugins::note::NOTE_NEW_JSON_PREFIX)
        {
            let payload = match crate::plugins::note::decode_note_new_payload(encoded) {
                Ok(payload) => payload,
                Err(err) => {
                    self.report_error_message(
                        "launcher",
                        format!("Malformed note action payload: {err}"),
                    );
                    return;
                }
            };
            if let Err(err) = validate_note_new_payload(&payload.slug, payload.template.as_deref())
            {
                self.report_error_message("launcher", err);
                return;
            }
            self.open_note_panel(&payload.slug, payload.template.as_deref());
        } else if let Some(rest) = a.action.strip_prefix("note:new:") {
            let slug = match urlencoding::decode(rest.trim()) {
                Ok(decoded) => decoded.into_owned(),
                Err(_) => {
                    self.report_error_message(
                        "launcher",
                        format!("Malformed note action: {}", a.action),
                    );
                    return;
                }
            };
            let template = a.args.as_deref().and_then(|args| {
                serde_json::from_str::<serde_json::Value>(args)
                    .ok()
                    .and_then(|value| {
                        value
                            .get("template")
                            .and_then(|template| template.as_str())
                            .map(str::to_string)
                    })
            });
            if let Err(err) = validate_note_new_payload(&slug, template.as_deref()) {
                self.report_error_message("launcher", err);
                return;
            }
            self.open_note_panel(&slug, template.as_deref());
        } else if a.action == "note:templates_disabled" {
            self.report_error_message("launcher", "Note templates are disabled in settings");
        } else if a.action == "note:tags" {
            self.open_note_tags();
            set_focus = true;
        } else if let Some(link) = a.action.strip_prefix("note:link:") {
            self.open_note_link(link);
        } else if let Some(slug) = a.action.strip_prefix("note:meta:wrap-links:") {
            self.wrap_note_plain_links(slug);
        } else if let Some(link_id) = a.action.strip_prefix("link:open:") {
            if let Ok(parsed) = crate::linking::parse_link_id(link_id) {
                match parsed.target_type {
                    crate::linking::LinkTarget::Note => {
                        self.open_note_panel(&parsed.target_id, None);
                    }
                    crate::linking::LinkTarget::Todo => {
                        self.query = format!("todo links id:{}", parsed.target_id);
                        self.search();
                    }
                    _ => {
                        self.report_error_message(
                            "launcher",
                            format!("Unsupported link target: {}", link_id),
                        );
                    }
                }
            } else {
                self.report_error_message("launcher", format!("Invalid link id: {}", link_id));
            }
        } else if let Some(slug) = a.action.strip_prefix("note:remove:") {
            self.delete_note(slug);
        } else if a.action == "convert:panel" {
            self.convert_panel.open();
        } else if a.action == "tempfile:dialog" {
            self.tempfile_dialog.open();
        } else if a.action == "settings:dialog" {
            self.open_settings_dialog();
        } else if a.action == "dashboard:settings" {
            let registry = self.dashboard.registry().clone();
            self.dashboard_editor.open(&self.dashboard_path, &registry);
            self.show_dashboard_editor = true;
        } else if a.action == "theme:dialog" {
            self.open_theme_settings_dialog();
        } else if a.action == "volume:dialog" {
            self.volume_dialog.open();
        } else if a.action == "brightness:dialog" {
            self.brightness_dialog.open();
        } else if let Some(n) = a.action.strip_prefix("sysinfo:cpu_list:") {
            if let Ok(count) = n.parse::<usize>() {
                self.cpu_list_dialog.open(count);
            }
        } else if a.action.starts_with("tab:switch:") {
            if self.enable_toasts {
                push_toast(
                    &mut self.toasts,
                    Toast {
                        text: format!("Switching to {}", a.label).into(),
                        kind: ToastKind::Info,
                        options: ToastOptions::default()
                            .duration_in_seconds(self.toast_duration as f64),
                    },
                );
            }
            let act = a.clone();
            std::thread::spawn(move || {
                if let Err(e) = launch_action(&act) {
                    tracing::error!(?e, "failed to switch tab");
                }
            });
            if a.action != "help:show" {
                self.record_history_usage(&a, &current, source);
            }
        } else if a.action == "mm:open" {
            self.open_multi_manager();
        } else if a.action == "mm:settings" {
            self.open_multi_manager_settings();
        } else if a.action == "mm:save" {
            self.multi_manager_save();
        } else if a.action == "mm:reload" {
            self.multi_manager_reload();
        } else if a.action == "mm:send-all-home" {
            self.multi_manager_send_all_home();
        } else if a.action == "mm:reconnect" {
            self.multi_manager_start_manual_reconnect();
        } else if a.action == "mm:save-bindings" {
            self.multi_manager_save_bindings();
        } else if a.action == "mm:restore-bindings" {
            self.multi_manager_restore_bindings();
        } else if a.action == "mm:import" {
            self.multi_manager_import();
        } else if a.action == "mm:recapture-all" {
            self.multi_manager_start_recapture_all();
        } else if let Some(workspace_id) = a.action.strip_prefix("mm:toggle:") {
            self.multi_manager_toggle_workspace(workspace_id);
        } else if let Some(workspace_id) = a.action.strip_prefix("mm:home:") {
            self.multi_manager_send_home(workspace_id);
        } else if let Some(workspace_id) = a.action.strip_prefix("mm:target:") {
            self.multi_manager_send_target(workspace_id);
        } else if let Some(workspace_id) = a.action.strip_prefix("mm:capture:") {
            self.multi_manager_start_capture(workspace_id);
        } else if let Some(workspace_id) = a.action.strip_prefix("mm:disable:") {
            self.multi_manager_set_workspace_disabled(workspace_id, true);
        } else if let Some(workspace_id) = a.action.strip_prefix("mm:enable:") {
            self.multi_manager_set_workspace_disabled(workspace_id, false);
        } else if let Some(mode) = a.action.strip_prefix("screenshot:") {
            use crate::actions::screenshot::Mode as ScreenshotMode;
            let (mode, clip, tool) = match mode {
                "window" => (ScreenshotMode::Window, false, MarkupTool::Rectangle),
                "region" => (ScreenshotMode::Region, false, MarkupTool::Rectangle),
                "region_markup" => (ScreenshotMode::Region, false, MarkupTool::Pen),
                "desktop" => (ScreenshotMode::Desktop, false, MarkupTool::Rectangle),
                "window_clip" => (ScreenshotMode::Window, true, MarkupTool::Rectangle),
                "region_clip" => (ScreenshotMode::Region, true, MarkupTool::Rectangle),
                "desktop_clip" => (ScreenshotMode::Desktop, true, MarkupTool::Rectangle),
                _ => (ScreenshotMode::Desktop, false, MarkupTool::Rectangle),
            };
            let screenshot_result =
                crate::plugins::screenshot::launch_editor(self, mode, clip, tool);
            if self.handle_screenshot_launch_result(screenshot_result) && a.action != "help:show" {
                self.record_history_usage(&a, &current, source);
            }
        } else if let Err(e) = execute_action(&a) {
            if a.desc == "Fav" && !a.action.starts_with("fav:") {
                tracing::error!(?e, fav=%a.label, "failed to run favorite");
            }
            self.report_error_message("launcher", format!("Failed: {e}"));
            self.add_error_toast(format!("Failed: {e}"));
        } else {
            if a.desc == "Fav" && !a.action.starts_with("fav:") {
                tracing::info!(fav=%a.label, command=%a.action, "ran favorite");
            }
            if self.enable_toasts && a.action != "recycle:clean" {
                let msg = if a.action.starts_with("clipboard:") {
                    format!("Copied {}", a.label)
                } else {
                    format!("Launched {}", a.label)
                };
                push_toast(
                    &mut self.toasts,
                    Toast {
                        text: msg.into(),
                        kind: ToastKind::Success,
                        options: ToastOptions::default()
                            .duration_in_seconds(self.toast_duration as f64),
                    },
                );
            }
            if a.action != "help:show" {
                self.record_history_usage(&a, &current, source);
            }
            if a.action == "note:reload" {
                refresh = true;
                set_focus = true;
                if self.enable_toasts {
                    push_toast(
                        &mut self.toasts,
                        Toast {
                            text: "Reloaded notes".into(),
                            kind: ToastKind::Success,
                            options: ToastOptions::default()
                                .duration_in_seconds(self.toast_duration as f64),
                        },
                    );
                }
            } else if a.action.starts_with("bookmark:add:") {
                if self.preserve_command {
                    self.query = "bm add ".into();
                } else {
                    self.query.clear();
                }
                command_changed_query = true;
                refresh = true;
                set_focus = true;
            } else if a.action.starts_with("bookmark:remove:") {
                refresh = true;
                set_focus = true;
            } else if a.action.starts_with("folder:add:") {
                if self.preserve_command {
                    self.query = "f add ".into();
                } else {
                    self.query.clear();
                }
                command_changed_query = true;
                refresh = true;
                set_focus = true;
            } else if a.action.starts_with("folder:remove:") {
                refresh = true;
                set_focus = true;
            } else if a.action.starts_with("fav:add:") {
                refresh = true;
                set_focus = true;
            } else if a.action.starts_with("fav:remove:") {
                refresh = true;
                set_focus = true;
            } else if a.action.starts_with("todo:add:") {
                if self.preserve_command {
                    self.query = "todo add ".into();
                } else {
                    self.query.clear();
                }
                command_changed_query = true;
                refresh = true;
                set_focus = true;
                if self.enable_toasts
                    && let Some(text) = a
                        .action
                        .strip_prefix("todo:add:")
                        .and_then(|r| r.split('|').next())
                {
                    push_toast(
                        &mut self.toasts,
                        Toast {
                            text: format!("Added todo {text}").into(),
                            kind: ToastKind::Success,
                            options: ToastOptions::default()
                                .duration_in_seconds(self.toast_duration as f64),
                        },
                    );
                }
            } else if a.action.starts_with("todo:remove:") {
                refresh = true;
                set_focus = true;
                if current.starts_with("note list") {
                    self.pending_query = Some(current.clone());
                    command_changed_query = true;
                }
                if self.enable_toasts {
                    let label = a.label.strip_prefix("Remove todo ").unwrap_or(&a.label);
                    push_toast(
                        &mut self.toasts,
                        Toast {
                            text: format!("Removed todo {label}").into(),
                            kind: ToastKind::Success,
                            options: ToastOptions::default()
                                .duration_in_seconds(self.toast_duration as f64),
                        },
                    );
                }
            } else if a.action.starts_with("todo:done:") {
                refresh = true;
                set_focus = true;
                self.pending_query = Some(current.clone());
                command_changed_query = true;
                if self.enable_toasts {
                    let label = a
                        .label
                        .trim_start_matches("[x] ")
                        .trim_start_matches("[ ] ");
                    push_toast(
                        &mut self.toasts,
                        Toast {
                            text: format!("Toggled todo {label}").into(),
                            kind: ToastKind::Success,
                            options: ToastOptions::default()
                                .duration_in_seconds(self.toast_duration as f64),
                        },
                    );
                }
            } else if a.action.starts_with("todo:pset:") {
                refresh = true;
                set_focus = true;
                if self.enable_toasts {
                    push_toast(
                        &mut self.toasts,
                        Toast {
                            text: "Updated todo priority".into(),
                            kind: ToastKind::Success,
                            options: ToastOptions::default()
                                .duration_in_seconds(self.toast_duration as f64),
                        },
                    );
                }
            } else if a.action.starts_with("todo:tag:") {
                refresh = true;
                set_focus = true;
                if self.enable_toasts {
                    push_toast(
                        &mut self.toasts,
                        Toast {
                            text: "Updated todo tags".into(),
                            kind: ToastKind::Success,
                            options: ToastOptions::default()
                                .duration_in_seconds(self.toast_duration as f64),
                        },
                    );
                }
            } else if a.action == "todo:clear" {
                refresh = true;
                set_focus = true;
                if self.enable_toasts {
                    push_toast(
                        &mut self.toasts,
                        Toast {
                            text: "Cleared completed todos".into(),
                            kind: ToastKind::Success,
                            options: ToastOptions::default()
                                .duration_in_seconds(self.toast_duration as f64),
                        },
                    );
                }
            } else if a.action.starts_with("snippet:remove:") {
                refresh = true;
                set_focus = true;
                if self.enable_toasts {
                    push_toast(
                        &mut self.toasts,
                        Toast {
                            text: format!("Removed snippet {}", a.label).into(),
                            kind: ToastKind::Success,
                            options: ToastOptions::default()
                                .duration_in_seconds(self.toast_duration as f64),
                        },
                    );
                }
            } else if a.action.starts_with("tempfile:remove:") {
                refresh = true;
                set_focus = true;
            } else if a.action.starts_with("tempfile:alias:") {
                refresh = true;
                set_focus = true;
            } else if a.action == "tempfile:new" || a.action.starts_with("tempfile:new:") {
                if self.preserve_command {
                    self.query = "tmp new ".into();
                } else {
                    self.query.clear();
                }
                command_changed_query = true;
                set_focus = true;
            } else if a.action.starts_with("timer:cancel:") && current.starts_with("timer rm") {
                refresh = true;
                set_focus = true;
            } else if a.action.starts_with("timer:pause:") && current.starts_with("timer pause") {
                refresh = true;
                set_focus = true;
            } else if a.action.starts_with("timer:resume:") && current.starts_with("timer resume") {
                refresh = true;
                set_focus = true;
            } else if a.action.starts_with("timer:start:") && current.starts_with("timer add") {
                if self.preserve_command {
                    self.query = "timer add ".into();
                } else {
                    self.query.clear();
                }
                command_changed_query = true;
                set_focus = true;
            }
            if self.clear_query_after_run && !command_changed_query {
                self.query.clear();
                refresh = true;
                set_focus = true;
            }
            if self.hide_after_run
                && !a.action.starts_with("bookmark:add:")
                && !a.action.starts_with("bookmark:remove:")
                && !a.action.starts_with("folder:add:")
                && !a.action.starts_with("folder:remove:")
                && !a.action.starts_with("snippet:remove:")
                && !a.action.starts_with("fav:add:")
                && !a.action.starts_with("fav:remove:")
                && !a.action.starts_with("screenshot:")
                && !a.action.starts_with("calc:")
                && !a.action.starts_with("todo:done:")
            {
                self.visible_flag.store(false, Ordering::SeqCst);
            }
        }
        if refresh {
            self.last_results_valid = false;
            self.search();
        }
        let _ = command_changed_query;
        if set_focus {
            self.focus_input();
        } else if self.visible_flag.load(Ordering::SeqCst) && !self.any_panel_open() {
            self.focus_input();
        }
    }

    pub(crate) fn handle_clipboard_modify_action(
        &mut self,
        action: &Action,
        source: ActivationSource,
    ) -> bool {
        use crate::clipboard_modify::actions::{
            ClipboardModifyActionPayload, ClipboardModifySectionPayload, EXECUTE_PREFIX,
            OPEN_PREFIX, UNDO_PREFIX, decode_action_payload,
        };
        use crate::clipboard_modify::parser::ClipboardModifyIntent;

        if action.action.starts_with("query:") {
            return false;
        }
        let is_clipboard_modify = action.action.starts_with("clipboard_modify:");
        if !is_clipboard_modify {
            return false;
        }

        let payload = action
            .args
            .as_deref()
            .and_then(|args| decode_action_payload::<ClipboardModifyActionPayload>(args).ok());

        if action.action.starts_with(OPEN_PREFIX) || action.action == "clipboard_modify:open" {
            let section = match payload {
                Some(ClipboardModifyActionPayload::OpenDialogSection { section }) => section,
                _ if action.action.ends_with(":templates") => {
                    ClipboardModifySectionPayload::Templates
                }
                _ if action.action.ends_with(":saved-pipelines") => {
                    ClipboardModifySectionPayload::SavedPipelines
                }
                _ if action.action.ends_with(":manage-templates") => {
                    ClipboardModifySectionPayload::ManageTemplates
                }
                _ if action.action.ends_with(":manage-pipelines") => {
                    ClipboardModifySectionPayload::ManagePipelines
                }
                _ if action.action.ends_with(":help") => ClipboardModifySectionPayload::Help,
                _ => ClipboardModifySectionPayload::Modify,
            };
            let section = match section {
                ClipboardModifySectionPayload::Modify => ClipboardModifyDialogSection::Modify,
                ClipboardModifySectionPayload::Templates => ClipboardModifyDialogSection::Templates,
                ClipboardModifySectionPayload::SavedPipelines => {
                    ClipboardModifyDialogSection::SavedPipelines
                }
                ClipboardModifySectionPayload::ManageTemplates => {
                    ClipboardModifyDialogSection::ManageTemplates
                }
                ClipboardModifySectionPayload::ManagePipelines => {
                    ClipboardModifyDialogSection::ManagePipelines
                }
                ClipboardModifySectionPayload::Help => ClipboardModifyDialogSection::Help,
            };
            self.clipboard_modify_dialog.open_section(
                section,
                &crate::clipboard_modify::runtime::clipboard_service(),
            );
            return true;
        }

        if action.action.starts_with(UNDO_PREFIX) || action.action == "clipboard_modify:undo" {
            match crate::clipboard_modify::runtime::undo() {
                Ok(()) => {
                    self.handle_clipboard_modify_gui_event(
                        ClipboardModifyGuiEvent::ImmediateOperationComplete,
                    );
                    self.visible_flag.store(false, Ordering::SeqCst);
                    if self.enable_toasts {
                        push_toast(
                            &mut self.toasts,
                            Toast {
                                text: "Undid Clipboard Modify".into(),
                                kind: ToastKind::Success,
                                options: ToastOptions::default()
                                    .duration_in_seconds(self.toast_duration as f64),
                            },
                        );
                    }
                }
                Err(err) => self.report_clipboard_modify_action_error(err.to_string()),
            }
            return true;
        }

        if action.action.starts_with(EXECUTE_PREFIX) || action.action == "clipboard_modify:execute"
        {
            let payload = match payload {
                Some(payload) => payload,
                None => {
                    self.report_clipboard_modify_action_error("missing execute payload".into());
                    return true;
                }
            };
            let (intent, canonical_command) = match payload {
                ClipboardModifyActionPayload::ExecuteAdHocStages {
                    canonical_command,
                    stages,
                } => (ClipboardModifyIntent::Stages(stages), canonical_command),
                ClipboardModifyActionPayload::ExecuteTemplate {
                    canonical_command,
                    name,
                } => (
                    ClipboardModifyIntent::ApplyTemplate { name },
                    canonical_command,
                ),
                ClipboardModifyActionPayload::ExecuteSavedPipeline {
                    canonical_command,
                    name,
                } => (
                    ClipboardModifyIntent::ApplySavedPipeline { name },
                    canonical_command,
                ),
                _ => {
                    self.report_clipboard_modify_action_error("unexpected execute payload".into());
                    return true;
                }
            };
            let meta = ImmediateRequestMetadata {
                action: action.clone(),
                query: canonical_command,
                source,
            };
            match self.clipboard_modify_immediate.start(
                intent,
                self.clipboard_modify_runtime.catalog_snapshot(),
                meta.clone(),
            ) {
                Ok(id) => {
                    self.pending_clipboard_modify_immediate.insert(id.0, meta);
                }
                Err(err) => {
                    self.report_clipboard_modify_action_error(err.message);
                    self.visible_flag.store(true, Ordering::SeqCst);
                    self.move_cursor_end = true;
                    self.focus_input();
                }
            }
            return true;
        }

        if action.action == "clipboard_modify:error" {
            self.report_clipboard_modify_action_error(action.desc.clone());
            return true;
        }

        false
    }

    pub(crate) fn drain_clipboard_modify_immediate(&mut self) {
        let mut typed_events = Vec::new();
        for ev in self.clipboard_modify_immediate.drain_completions() {
            let meta = self
                .pending_clipboard_modify_immediate
                .remove(&ev.request_id.0);
            match ev.result {
                Ok(()) => {
                    typed_events.push(WatchEvent::ClipboardModify(
                        ClipboardModifyGuiEvent::ImmediateOperationComplete,
                    ));
                    if let Some(meta) = meta.as_ref() {
                        self.record_history_usage(&meta.action, &meta.query, meta.source);
                    }
                    self.visible_flag.store(false, Ordering::SeqCst);
                    if self.enable_toasts {
                        push_toast(
                            &mut self.toasts,
                            Toast {
                                text: format!("{} complete", ev.display_label).into(),
                                kind: ToastKind::Success,
                                options: ToastOptions::default()
                                    .duration_in_seconds(self.toast_duration as f64),
                            },
                        );
                    }
                }
                Err(ref err) => {
                    typed_events.push(WatchEvent::ClipboardModify(
                        ClipboardModifyGuiEvent::ImmediateOperationFailed,
                    ));
                    if let Some(meta) = meta {
                        self.query = meta.query;
                        self.last_results_valid = false;
                        self.search();
                    }
                    self.visible_flag.store(true, Ordering::SeqCst);
                    self.move_cursor_end = true;
                    self.focus_input();
                    self.report_error_message("clipboard_modify", err.message.clone());
                }
            }
            self.clipboard_modify_events.push(ev);
        }
        for ev in typed_events {
            self.handle_clipboard_modify_gui_event(match ev {
                WatchEvent::ClipboardModify(e) => e,
                _ => unreachable!(),
            });
        }
    }

    pub(crate) fn handle_clipboard_modify_gui_event(&mut self, ev: ClipboardModifyGuiEvent) {
        match ev {
            ClipboardModifyGuiEvent::ImmediateOperationComplete
            | ClipboardModifyGuiEvent::ImmediateOperationFailed => {}
            ClipboardModifyGuiEvent::ConfigurationReloadSuccess => {
                self.clipboard_modify_config_diagnostic = self
                    .clipboard_modify_runtime
                    .diagnostic
                    .read()
                    .unwrap()
                    .clone();
                self.last_results_valid = false;
                if self
                    .query
                    .trim_start()
                    .to_ascii_lowercase()
                    .starts_with("cm")
                {
                    self.search();
                }
                self.update_command_cache();
            }
            ClipboardModifyGuiEvent::ConfigurationReloadFailure(err) => {
                self.clipboard_modify_config_diagnostic = Some(err.clone());
                self.report_error_message("clipboard_modify.config.reload", err);
            }
            ClipboardModifyGuiEvent::StartupDiagnosticChanged(diagnostic) => {
                self.clipboard_modify_config_diagnostic = diagnostic;
            }
        }
    }

    pub(crate) fn refresh_clipboard_modify_catalog(
        &mut self,
        catalog: crate::clipboard_modify::model::ClipboardModifierCatalog,
        static_commands_changed: bool,
    ) {
        self.clipboard_modify_runtime.replace_catalog(catalog);
        self.clipboard_modify_config_diagnostic = self
            .clipboard_modify_runtime
            .diagnostic
            .read()
            .unwrap()
            .clone();
        self.last_results_valid = false;
        if self
            .query
            .trim_start()
            .to_ascii_lowercase()
            .starts_with("cm")
        {
            self.search();
        }
        if static_commands_changed {
            self.update_command_cache();
        } else {
            self.update_suggestions();
        }
    }

    fn report_clipboard_modify_action_error(&mut self, err: String) {
        let msg = format!("Invalid clipboard modify action: {err}");
        self.set_inline_error(msg.clone());
        self.add_error_toast(msg);
    }

    fn handle_file_search_action(&mut self, action: &str) -> bool {
        use crate::file_search::actions::{
            CANCEL_ACTION, FileSearchModePayload, FileSearchStartPayload, MODE_PREFIX, OPEN_ACTION,
            START_PREFIX, decode_action_payload,
        };

        if action == OPEN_ACTION {
            self.file_search_dialog.open();
            return true;
        }
        if action == CANCEL_ACTION {
            self.file_search_dialog
                .cancel_search(&mut self.file_search_coordinator);
            return true;
        }
        if let Some(encoded) = action.strip_prefix(MODE_PREFIX) {
            self.file_search_dialog.open();
            match decode_action_payload::<FileSearchModePayload>(encoded).and_then(|payload| {
                payload.validate()?;
                Ok(payload)
            }) {
                Ok(payload) => {
                    let mode = match payload.search_kind() {
                        crate::file_search::model::SearchKind::Filename => {
                            crate::gui::FileSearchMode::Filename
                        }
                        crate::file_search::model::SearchKind::Content => {
                            crate::gui::FileSearchMode::Content
                        }
                    };
                    self.file_search_dialog.open_with_mode(mode);
                }
                Err(err) => self.report_file_search_action_error(err),
            }
            return true;
        }
        if let Some(encoded) = action.strip_prefix(START_PREFIX) {
            self.file_search_dialog.open();
            match decode_action_payload::<FileSearchStartPayload>(encoded).and_then(|payload| {
                payload.validate()?;
                Ok(payload)
            }) {
                Ok(payload) => {
                    let mode = match payload.search_kind() {
                        crate::file_search::model::SearchKind::Filename => {
                            crate::gui::FileSearchMode::Filename
                        }
                        crate::file_search::model::SearchKind::Content => {
                            crate::gui::FileSearchMode::Content
                        }
                    };
                    let root = payload.root_path();
                    self.file_search_dialog.open_and_start(
                        mode,
                        root,
                        payload.text,
                        &mut self.file_search_coordinator,
                    );
                }
                Err(err) => self.report_file_search_action_error(err),
            }
            return true;
        }
        false
    }

    fn report_file_search_action_error(&mut self, err: String) {
        let msg = format!("Invalid file search action: {err}");
        self.set_inline_error(msg.clone());
        self.add_error_toast(msg);
    }

    fn record_history_usage(&mut self, action: &Action, query: &str, source: ActivationSource) {
        let _ = history::append_history(
            HistoryEntry {
                query: query.to_string(),
                query_lc: String::new(),
                action: action.clone(),
                source: Some(source.label().to_string()),
                timestamp: 0,
            },
            self.history_limit,
        );
        let count = self.usage.entry(action.action.clone()).or_insert(0);
        *count += 1;
    }

    fn wrap_note_plain_links(&mut self, slug: &str) {
        let outcome = self.mutate_note_by_slug(slug, |content| {
            let report = crate::notes_markdown::links::wrap_plain_urls(content);
            NoteMutationOutput::changed(
                report.content,
                NoteMutationResult {
                    wrapped_links: report.wrapped,
                    skipped_existing_links: report.skipped_existing,
                },
            )
        });

        let Ok(outcome) = outcome else {
            return;
        };

        let message = match outcome {
            NoteMutationOutcome::Changed(result) => format_wrap_links_toast(result),
            NoteMutationOutcome::Unchanged(_) => "No unwrapped links found.".to_string(),
        };

        if self.enable_toasts {
            push_toast(
                &mut self.toasts,
                Toast {
                    text: message.into(),
                    kind: ToastKind::Success,
                    options: ToastOptions::default()
                        .duration_in_seconds(self.toast_duration as f64),
                },
            );
        }
    }

    fn handle_launcher_action(&mut self, action: &str) -> bool {
        match action {
            "launcher:toggle" => {
                let next = !self.visible_flag.load(Ordering::SeqCst);
                self.visible_flag.store(next, Ordering::SeqCst);
                if next {
                    self.restore_flag.store(true, Ordering::SeqCst);
                }
                true
            }
            "launcher:show" => {
                self.visible_flag.store(true, Ordering::SeqCst);
                self.restore_flag.store(true, Ordering::SeqCst);
                true
            }
            "launcher:hide" => {
                self.visible_flag.store(false, Ordering::SeqCst);
                true
            }
            "launcher:focus" | "launcher:restore" => {
                self.visible_flag.store(true, Ordering::SeqCst);
                self.restore_flag.store(true, Ordering::SeqCst);
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        common::slug::reset_slug_lookup,
        history,
        plugin::PluginManager,
        plugins::note::{Note, append_note, load_notes, save_note, save_notes},
        settings::Settings,
    };
    use eframe::egui;
    use once_cell::sync::Lazy;
    use std::sync::{Arc, Mutex, atomic::AtomicBool};
    use tempfile::tempdir;

    static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    pub(super) fn new_app(ctx: &egui::Context) -> LauncherApp {
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

    fn note(title: &str, slug: &str, content: &str) -> Note {
        Note {
            title: title.into(),
            path: Default::default(),
            content: content.into(),
            tags: Vec::new(),
            links: Vec::new(),
            slug: slug.into(),
            alias: None,
            aliases: Vec::new(),
            entity_refs: Vec::new(),
        }
    }

    fn setup_notes_app() -> (
        tempfile::TempDir,
        std::path::PathBuf,
        egui::Context,
        LauncherApp,
    ) {
        let dir = tempdir().unwrap();
        let notes_dir = dir.path().join("notes");
        std::fs::create_dir_all(&notes_dir).unwrap();
        unsafe { std::env::set_var("ML_NOTES_DIR", &notes_dir) };
        unsafe { std::env::set_var("HOME", dir.path()) };
        save_notes(&[]).unwrap();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.enable_toasts = true;
        (dir, original_dir, ctx, app)
    }

    fn activate_wrap_links(app: &mut LauncherApp, slug: &str) {
        app.activate_action_confirmed(
            Action {
                label: "Wrap links".into(),
                desc: "Notes".into(),
                action: format!("note:meta:wrap-links:{slug}"),
                args: None,
            },
            None,
            ActivationSource::Enter,
        );
    }

    #[test]
    fn wrap_links_action_changes_correct_note_and_reports_counts() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let (_dir, original_dir, _ctx, mut app) = setup_notes_app();
        let mut alpha = note(
            "Alpha",
            "alpha",
            "See https://example.com and [kept](https://kept.example).",
        );
        let mut beta = note("Beta", "beta", "See https://beta.example.");
        save_note(&mut alpha, true).unwrap();
        save_note(&mut beta, true).unwrap();

        activate_wrap_links(&mut app, "alpha");

        let notes = load_notes().unwrap();
        let alpha = notes.iter().find(|note| note.slug == "alpha").unwrap();
        let beta = notes.iter().find(|note| note.slug == "beta").unwrap();
        assert!(
            alpha
                .content
                .contains("[https://example.com](https://example.com)")
        );
        assert!(alpha.content.contains("[kept](https://kept.example)"));
        assert!(beta.content.contains("https://beta.example"));
        assert!(
            !beta
                .content
                .contains("[https://beta.example](https://beta.example)")
        );
        let log = std::fs::read_to_string(crate::toast_log::TOAST_LOG_FILE).unwrap();
        assert!(log.contains("Wrapped 1 link; skipped 1 existing link."));
        std::env::set_current_dir(original_dir).unwrap();
    }

    #[test]
    fn wrap_links_toast_formats_plural_counts() {
        assert_eq!(
            format_wrap_links_toast(NoteMutationResult {
                wrapped_links: 1,
                skipped_existing_links: 0,
            }),
            "Wrapped 1 link; skipped 0 existing links."
        );
        assert_eq!(
            format_wrap_links_toast(NoteMutationResult {
                wrapped_links: 2,
                skipped_existing_links: 1,
            }),
            "Wrapped 2 links; skipped 1 existing link."
        );
    }

    #[test]
    fn wrap_links_noop_shows_message_and_does_not_rewrite_note() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let (_dir, original_dir, _ctx, mut app) = setup_notes_app();
        let mut alpha = note("Alpha", "alpha", "Already <https://example.com>.");
        save_note(&mut alpha, true).unwrap();
        let path = load_notes().unwrap()[0].path.clone();
        let before = std::fs::metadata(&path).unwrap().modified().unwrap();

        activate_wrap_links(&mut app, "alpha");

        let after = std::fs::metadata(&path).unwrap().modified().unwrap();
        assert_eq!(before, after);
        assert_eq!(app.note_mutation_refresh_count, 0);
        let log = std::fs::read_to_string(crate::toast_log::TOAST_LOG_FILE).unwrap();
        assert!(log.contains("No unwrapped links found."));
        std::env::set_current_dir(original_dir).unwrap();
    }

    #[test]
    fn wrap_links_action_mutates_open_dirty_panel_content() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let (_dir, original_dir, _ctx, mut app) = setup_notes_app();
        let mut alpha = note("Alpha", "alpha", "disk https://disk.example");
        save_note(&mut alpha, true).unwrap();
        let mut panel = NotePanel::from_note(alpha);
        panel.replace_content_after_external_mutation("dirty https://dirty.example".into());
        app.note_panels.push(panel);

        activate_wrap_links(&mut app, "alpha");

        assert_eq!(
            app.note_panels[0].note_content(),
            "dirty [https://dirty.example](https://dirty.example)"
        );
        let saved = load_notes().unwrap().remove(0);
        assert!(
            saved
                .content
                .contains("dirty [https://dirty.example](https://dirty.example)")
        );
        assert!(!saved.content.contains("disk"));
        std::env::set_current_dir(original_dir).unwrap();
    }

    #[test]
    fn wrap_links_action_loads_mutates_and_saves_closed_note() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let (_dir, original_dir, _ctx, mut app) = setup_notes_app();
        let mut alpha = note("Alpha", "alpha", "closed https://closed.example");
        save_note(&mut alpha, true).unwrap();

        activate_wrap_links(&mut app, "alpha");

        let saved = load_notes().unwrap().remove(0);
        assert!(
            saved
                .content
                .contains("closed [https://closed.example](https://closed.example)")
        );
        assert_eq!(app.note_mutation_refresh_count, 1);
        std::env::set_current_dir(original_dir).unwrap();
    }

    #[test]
    fn destructive_confirmation_supports_queue_confirm_and_cancel_paths() {
        let dir = tempfile::tempdir().unwrap();
        let notes_dir = dir.path().join("notes");
        std::fs::create_dir_all(&notes_dir).unwrap();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        unsafe { std::env::set_var("ML_NOTES_DIR", &notes_dir) };
        save_notes(&[]).unwrap();
        reset_slug_lookup();
        append_note("alpha", "# alpha\n\nbody").unwrap();

        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.require_confirm_destructive = true;
        let delete = Action {
            label: "Delete note".into(),
            desc: "Notes".into(),
            action: "note:remove:alpha".into(),
            args: None,
        };

        app.activate_action(delete.clone(), None, ActivationSource::Click);
        assert!(app.pending_confirm.is_some());
        assert_eq!(load_notes().unwrap().len(), 1);

        app.pending_confirm = None;
        assert_eq!(load_notes().unwrap().len(), 1);

        app.activate_action(delete, None, ActivationSource::Dashboard);
        let pending = app
            .pending_confirm
            .take()
            .expect("queued destructive action");
        assert_eq!(pending.source, ActivationSource::Dashboard);
        app.activate_action_confirmed(pending.action, pending.query_override, pending.source);
        assert!(load_notes().unwrap().is_empty());

        std::env::set_current_dir(original_dir).unwrap();
    }

    #[test]
    fn action_execution_errors_flow_through_unified_ui_reporting() {
        let dir = tempfile::tempdir().unwrap();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();

        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.enable_toasts = true;
        app.show_error_toasts = true;
        app.show_inline_errors = true;

        set_execute_action_hook(Some(Box::new(|_| Err(anyhow::anyhow!("injected failure")))));
        app.activate_action(
            Action {
                label: "Broken".into(),
                desc: "Test".into(),
                action: "exec:broken".into(),
                args: None,
            },
            None,
            ActivationSource::Enter,
        );

        assert!(
            app.error
                .as_deref()
                .is_some_and(|msg| msg.contains("injected failure"))
        );
        let log = std::fs::read_to_string(crate::toast_log::TOAST_LOG_FILE).unwrap();
        assert!(log.contains("[error:launcher] Failed: injected failure"));
        assert!(log.contains("Failed: injected failure"));

        set_execute_action_hook(None);
        std::env::set_current_dir(original_dir).unwrap();
    }

    #[test]
    fn activation_source_and_usage_are_recorded_for_successful_actions() {
        let dir = tempfile::tempdir().unwrap();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();

        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let action = Action {
            label: "Track Me".into(),
            desc: "Test".into(),
            action: "exec:track".into(),
            args: None,
        };
        let before_len = history::get_history().len();

        set_execute_action_hook(Some(Box::new(|_| Ok(()))));
        app.query = "track me".into();
        app.activate_action(action.clone(), None, ActivationSource::Gesture);

        assert_eq!(app.usage.get(&action.action), Some(&1));
        let history_entries = history::get_history();
        assert!(history_entries.len() > before_len);
        let latest = history_entries.front().expect("latest history entry");
        assert_eq!(latest.action.action, action.action);
        assert_eq!(latest.query, "track me");
        assert_eq!(latest.source.as_deref(), Some("gesture"));

        set_execute_action_hook(None);
        std::env::set_current_dir(original_dir).unwrap();
    }
}

#[cfg(test)]
mod clipboard_modify_gui_action_tests {
    use super::*;
    use crate::clipboard_modify::actions::{
        encode_action_payload, execute_stages_payload, open_dialog_payload, undo_payload,
    };
    use crate::clipboard_modify::model::{OperationId, StageArguments, StageSpec};
    use crate::clipboard_modify::parser::ModifySection;

    fn action(action: &str, args: Option<String>) -> Action {
        Action {
            label: "Test".into(),
            desc: "Test".into(),
            action: action.into(),
            args,
        }
    }

    #[test]
    fn query_clipboard_modify_completion_is_not_claimed() {
        let ctx = egui::Context::default();
        let mut app = super::tests::new_app(&ctx);
        assert!(!app.handle_clipboard_modify_action(
            &action("query:cm camel-case", None),
            ActivationSource::Enter
        ));
    }

    #[test]
    fn execute_without_payload_reports_missing_and_does_not_start() {
        let ctx = egui::Context::default();
        let mut app = super::tests::new_app(&ctx);
        assert!(app.handle_clipboard_modify_action(
            &action("clipboard_modify:execute", None),
            ActivationSource::Enter
        ));
        assert!(app.pending_clipboard_modify_immediate.is_empty());
        assert!(
            app.error
                .as_deref()
                .unwrap_or_default()
                .contains("missing execute payload")
        );
    }

    #[test]
    fn execute_rejects_open_and_undo_payloads() {
        let ctx = egui::Context::default();
        for payload in [open_dialog_payload(ModifySection::Help), undo_payload()] {
            let mut app = super::tests::new_app(&ctx);
            let args = encode_action_payload(&payload).unwrap();
            assert!(app.handle_clipboard_modify_action(
                &action("clipboard_modify:execute", Some(args)),
                ActivationSource::Enter
            ));
            assert!(app.pending_clipboard_modify_immediate.is_empty());
            assert!(
                app.error
                    .as_deref()
                    .unwrap_or_default()
                    .contains("unexpected execute payload")
            );
        }
    }

    #[test]
    fn typed_execute_starts_immediate_with_canonical_command_metadata() {
        let ctx = egui::Context::default();
        let mut app = super::tests::new_app(&ctx);
        app.query = "not canonical".into();
        let args = encode_action_payload(&execute_stages_payload(vec![StageSpec {
            operation: OperationId::CamelCase,
            arguments: StageArguments::default(),
        }]))
        .unwrap();
        let action = action("clipboard_modify:execute", Some(args));
        assert!(app.handle_clipboard_modify_action(&action, ActivationSource::Enter));
        let meta = app
            .pending_clipboard_modify_immediate
            .values()
            .next()
            .expect("pending immediate metadata");
        assert_eq!(meta.query, "cm camel-case");
        assert_eq!(meta.action.action, "clipboard_modify:execute");
    }

    #[test]
    fn clipboard_modify_handler_claims_only_clipboard_modify_actions() {
        let ctx = egui::Context::default();
        let mut app = super::tests::new_app(&ctx);
        assert!(!app.handle_clipboard_modify_action(
            &action("clipboard:upper", None),
            ActivationSource::Enter
        ));
        assert!(app.handle_clipboard_modify_action(
            &action("clipboard_modify:error", None),
            ActivationSource::Enter
        ));
    }

    #[test]
    fn dialog_open_payload_selects_requested_section() {
        let ctx = egui::Context::default();
        let mut app = super::tests::new_app(&ctx);
        let args = encode_action_payload(&open_dialog_payload(ModifySection::Templates)).unwrap();
        assert!(app.handle_clipboard_modify_action(
            &action("clipboard_modify:open:templates", Some(args)),
            ActivationSource::Click
        ));
        assert!(app.clipboard_modify_dialog.open);
        assert_eq!(
            app.clipboard_modify_dialog.section,
            ClipboardModifyDialogSection::Templates
        );
    }

    #[test]
    fn dialog_open_payload_routes_all_clipboard_modify_sections() {
        let ctx = egui::Context::default();
        let mut app = super::tests::new_app(&ctx);
        for (modify_section, dialog_section) in [
            (ModifySection::Modify, ClipboardModifyDialogSection::Modify),
            (
                ModifySection::Templates,
                ClipboardModifyDialogSection::Templates,
            ),
            (
                ModifySection::SavedPipelines,
                ClipboardModifyDialogSection::SavedPipelines,
            ),
            (
                ModifySection::ManageTemplates,
                ClipboardModifyDialogSection::ManageTemplates,
            ),
            (
                ModifySection::ManagePipelines,
                ClipboardModifyDialogSection::ManagePipelines,
            ),
            (ModifySection::Help, ClipboardModifyDialogSection::Help),
        ] {
            let args = encode_action_payload(&open_dialog_payload(modify_section)).unwrap();
            assert!(app.handle_clipboard_modify_action(
                &action("clipboard_modify:open", Some(args)),
                ActivationSource::Click
            ));
            assert_eq!(app.clipboard_modify_dialog.section, dialog_section);
        }
    }

    #[test]
    fn dialog_open_legacy_suffix_routes_new_sections() {
        let ctx = egui::Context::default();
        let mut app = super::tests::new_app(&ctx);
        for (suffix, dialog_section) in [
            (
                "manage-templates",
                ClipboardModifyDialogSection::ManageTemplates,
            ),
            (
                "manage-pipelines",
                ClipboardModifyDialogSection::ManagePipelines,
            ),
            ("help", ClipboardModifyDialogSection::Help),
        ] {
            assert!(app.handle_clipboard_modify_action(
                &action(&format!("clipboard_modify:open:{suffix}"), None),
                ActivationSource::Click
            ));
            assert_eq!(app.clipboard_modify_dialog.section, dialog_section);
        }
    }

    #[test]
    fn concurrent_activation_shows_already_running_and_leaves_launcher_visible() {
        let ctx = egui::Context::default();
        let mut app = super::tests::new_app(&ctx);
        app.visible_flag.store(true, Ordering::SeqCst);
        let args = encode_action_payload(&execute_stages_payload(vec![StageSpec {
            operation: OperationId::Uppercase,
            arguments: StageArguments::default(),
        }]))
        .unwrap();
        let act = action("clipboard_modify:execute", Some(args));
        assert!(app.handle_clipboard_modify_action(&act, ActivationSource::Enter));
        let query_before = app.query.clone();
        assert!(app.handle_clipboard_modify_action(&act, ActivationSource::Enter));
        assert!(app.visible_flag.load(Ordering::SeqCst));
        assert_eq!(app.query, query_before);
        assert!(app.move_cursor_end);
        assert!(
            app.error
                .as_deref()
                .unwrap_or_default()
                .contains("Clipboard Modify operation already running")
        );
    }

    #[test]
    fn immediate_failure_restores_canonical_query_and_focus_cursor_flags() {
        let ctx = egui::Context::default();
        let mut app = super::tests::new_app(&ctx);
        let meta = ImmediateRequestMetadata {
            action: action("clipboard_modify:execute", None),
            query: "cm uppercase".into(),
            source: ActivationSource::Enter,
        };
        app.pending_clipboard_modify_immediate
            .insert(7, meta.clone());
        app.query = "changed".into();
        app.clipboard_modify_immediate.inject_completion_for_test(
            meta.clone(),
            crate::clipboard_modify::coordinator::ImmediateCompletionEvent {
                request_id: crate::clipboard_modify::coordinator::OperationId(7),
                display_label: "Test".into(),
                character_count: 0,
                line_count: 0,
                undo_available: false,
                result: Err(
                    crate::clipboard_modify::coordinator::StructuredClipboardModifyError {
                        message: "boom".into(),
                    },
                ),
            },
        );
        app.drain_clipboard_modify_immediate();
        assert_eq!(app.query, "cm uppercase");
        assert!(app.visible_flag.load(Ordering::SeqCst));
        assert!(app.move_cursor_end);
        assert!(app.error.as_deref().unwrap_or_default().contains("boom"));
        assert!(app.usage.get(&meta.action.action).is_none());
    }

    #[test]
    fn immediate_success_records_canonical_usage_history_and_hides_launcher() {
        let dir = tempfile::tempdir().unwrap();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        let ctx = egui::Context::default();
        let mut app = super::tests::new_app(&ctx);
        app.visible_flag.store(true, Ordering::SeqCst);
        let meta = ImmediateRequestMetadata {
            action: action("clipboard_modify:execute", None),
            query: "cm uppercase".into(),
            source: ActivationSource::Gesture,
        };
        app.pending_clipboard_modify_immediate
            .insert(8, meta.clone());
        let before_len = history::get_history().len();
        app.clipboard_modify_immediate.inject_completion_for_test(
            meta.clone(),
            crate::clipboard_modify::coordinator::ImmediateCompletionEvent {
                request_id: crate::clipboard_modify::coordinator::OperationId(8),
                display_label: "Test".into(),
                character_count: 1,
                line_count: 1,
                undo_available: true,
                result: Ok(()),
            },
        );
        app.drain_clipboard_modify_immediate();
        assert!(!app.visible_flag.load(Ordering::SeqCst));
        assert_eq!(app.usage.get(&meta.action.action), Some(&1));
        assert!(history::get_history().len() > before_len);
        assert_eq!(
            history::get_history().front().unwrap().query,
            "cm uppercase"
        );
        std::env::set_current_dir(original_dir).unwrap();
    }

    #[test]
    fn malformed_payload_failure_leaves_coordinator_ready_for_valid_command() {
        let ctx = egui::Context::default();
        let mut app = super::tests::new_app(&ctx);
        assert!(app.handle_clipboard_modify_action(
            &action("clipboard_modify:execute", Some("not-json".into())),
            ActivationSource::Enter
        ));
        assert!(!app.clipboard_modify_immediate.has_pending());
        let args = encode_action_payload(&execute_stages_payload(vec![StageSpec {
            operation: OperationId::Uppercase,
            arguments: StageArguments::default(),
        }]))
        .unwrap();
        assert!(app.handle_clipboard_modify_action(
            &action("clipboard_modify:execute", Some(args)),
            ActivationSource::Enter
        ));
        assert!(app.clipboard_modify_immediate.has_pending());
    }

    #[test]
    fn catalog_replacement_refreshes_cm_results_without_rebuilding_plugin_manager() {
        let ctx = egui::Context::default();
        let mut app = super::tests::new_app(&ctx);
        app.plugins.register(Box::new(
            crate::plugins::clipboard_modify::ClipboardModifyPlugin::new(
                app.plugins.clipboard_modifier_catalog(),
            ),
        ));
        app.update_command_cache();
        let names = app.plugins.plugin_names();
        app.query = "cm".into();
        app.refresh_clipboard_modify_catalog(crate::clipboard_modify::default_catalog(), false);
        assert_eq!(app.plugins.plugin_names(), names);
        assert!(app.last_results_valid);
        assert!(
            app.results
                .iter()
                .any(|a| a.action.starts_with("clipboard_modify:"))
        );
    }
}
