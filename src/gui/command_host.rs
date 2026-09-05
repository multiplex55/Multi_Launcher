use std::sync::atomic::Ordering;

use crate::commands::{
    Command, CommandError, CommandInvocation, CommandOutcome, CropCommandHost, DialogCommandHost,
    FavoriteLogPolicy, HeadlessCommandHost, HistoryPolicy, LauncherCommandHost, LegacyCommandHost,
    QueryPolicy, ToastPolicy, VisibilityPolicy,
};

use super::{LauncherApp, Toast, ToastKind, ToastOptions, push_toast};

impl LauncherCommandHost for LauncherApp {
    fn launcher_is_visible(&self) -> bool {
        self.visible_flag.load(Ordering::SeqCst)
    }
}

impl DialogCommandHost for LauncherApp {
    fn open_help_dialog(&mut self) {
        self.help_window.open = true;
    }

    fn open_timer_dialog(&mut self) {
        self.timer_dialog.open_timer();
    }

    fn open_alarm_dialog(&mut self) {
        self.timer_dialog.open_alarm();
    }

    fn open_shell_dialog(&mut self) {
        self.shell_cmd_dialog.open();
    }

    fn open_bookmark_dialog(&mut self) {
        self.add_bookmark_dialog.open();
    }

    fn open_snippet_dialog(&mut self) {
        self.snippet_dialog.open();
    }

    fn open_snippet_editor(&mut self, alias: &str) {
        self.snippet_dialog.open_edit(alias);
    }

    fn open_favorite_dialog(&mut self, label: &str) {
        if label.is_empty() {
            self.fav_dialog.open();
        } else {
            self.fav_dialog.open_edit(label);
        }
    }

    fn open_legacy_macro_dialog(&mut self) {
        self.macro_dialog.open();
    }

    fn open_mkmacro_dialog(&mut self) {
        self.mkmacro_dialog.open();
    }

    fn open_todo_dialog(&mut self) {
        self.todo_dialog.open();
    }

    fn open_clipboard_dialog(&mut self) {
        self.clipboard_dialog.open();
    }

    fn open_convert_dialog(&mut self) {
        self.convert_panel.open();
    }

    fn open_tempfile_dialog(&mut self) {
        self.tempfile_dialog.open();
    }

    fn open_settings_dialog(&mut self) {
        LauncherApp::open_settings_dialog(self);
    }

    fn open_dashboard_settings_dialog(&mut self) {
        let registry = self.dashboard.registry().clone();
        self.dashboard_editor.open(&self.dashboard_path, &registry);
        self.show_dashboard_editor = true;
    }

    fn open_theme_dialog(&mut self) {
        self.open_theme_settings_dialog();
    }

    fn open_volume_dialog(&mut self) {
        self.volume_dialog.open();
    }

    fn open_brightness_dialog(&mut self) {
        self.brightness_dialog.open();
    }

    fn open_cpu_list_dialog(&mut self, count: usize) {
        self.cpu_list_dialog.open(count);
    }
}

impl CropCommandHost for LauncherApp {
    fn crop_image(&mut self) {
        self.handle_crop_image_action();
    }

    fn crop_screenshot(&mut self) {
        self.begin_crop_screenshot();
    }
}
impl HeadlessCommandHost for LauncherApp {
    fn execute_headless_command(
        &mut self,
        command: &Command,
        original_action: &crate::actions::Action,
    ) -> anyhow::Result<()> {
        super::execute_parsed_action(command, original_action)
    }

    fn spawn_headless_command(
        &mut self,
        command: Command,
        original_action: crate::actions::Action,
    ) {
        std::thread::spawn(move || {
            if let Err(error) = crate::commands::headless::execute(command, &original_action) {
                tracing::error!(?error, "failed to execute asynchronous command");
            }
        });
    }

    fn clear_query_after_run(&self) -> bool {
        self.clear_query_after_run
    }

    fn hide_after_run(&self) -> bool {
        self.hide_after_run
    }

    fn preserve_command(&self) -> bool {
        self.preserve_command
    }

    fn current_query(&self) -> &str {
        &self.query
    }

    fn launcher_should_refocus(&self) -> bool {
        self.visible_flag.load(Ordering::SeqCst) && !self.any_panel_open()
    }
}

impl LegacyCommandHost for LauncherApp {
    fn execute_legacy_command(
        &mut self,
        invocation: &CommandInvocation,
    ) -> Result<CommandOutcome, CommandError> {
        self.activate_action_legacy(invocation.original_action.clone(), invocation.source);
        Ok(CommandOutcome {
            history: HistoryPolicy::AlreadyApplied,
            ..CommandOutcome::default()
        })
    }
}

impl LauncherApp {
    pub(crate) fn dispatch_command_invocation(&mut self, invocation: CommandInvocation) {
        if let Some(query_override) = invocation.query_override.as_ref()
            && !matches!(
                &invocation.command,
                Command::ClipboardModify(_) | Command::FileSearch(_) | Command::Diff(_)
            )
        {
            self.apply_command_outcome(
                CommandOutcome {
                    query: QueryPolicy::Set(query_override.clone()),
                    search: true,
                    ..CommandOutcome::default()
                },
                &invocation,
            );
        }

        let bus = std::sync::Arc::clone(&self.command_bus);
        match bus.dispatch(&invocation, self) {
            Ok(outcome) => self.apply_command_outcome(outcome, &invocation),
            Err(error) => {
                if let Some(favorite) = error.favorite.as_ref() {
                    tracing::error!(fav = %favorite, error = %error.message, "failed to run favorite");
                }
                let message = error.message;
                self.report_error_message(error.domain, message.clone());
                if error.toast {
                    // The legacy generic failure path reported the error and also
                    // emitted its explicit failure toast.
                    self.add_error_toast(message);
                }
                if error.refocus
                    && self.visible_flag.load(Ordering::SeqCst)
                    && !self.any_panel_open()
                {
                    self.focus_input();
                }
            }
        }
    }

    fn apply_command_outcome(&mut self, outcome: CommandOutcome, invocation: &CommandInvocation) {
        let history_query = self.query.clone();

        if let QueryPolicy::Set(query) = outcome.query {
            self.last_timer_query =
                query.starts_with("timer list") || query.starts_with("alarm list");
            self.query = query;
        }
        if outcome.invalidate_results {
            self.last_results_valid = false;
        }
        if outcome.search {
            self.search();
        }
        if let Some(source) = outcome.activate_first_result
            && let Some(action) = self.results.first().cloned()
        {
            self.activate_action(action, None, source);
        }

        match outcome.visibility {
            VisibilityPolicy::Keep => {}
            VisibilityPolicy::Show => self.visible_flag.store(true, Ordering::SeqCst),
            VisibilityPolicy::Hide => self.visible_flag.store(false, Ordering::SeqCst),
            VisibilityPolicy::Toggle => {
                let next = !self.visible_flag.load(Ordering::SeqCst);
                self.visible_flag.store(next, Ordering::SeqCst);
            }
        }
        if outcome.restore {
            self.restore_flag.store(true, Ordering::SeqCst);
        }
        if outcome.move_cursor_end {
            self.move_cursor_end = true;
        }
        if outcome.focus {
            self.focus_input();
        }
        if let FavoriteLogPolicy::Ran { label, command } = outcome.favorite_log {
            tracing::info!(fav = %label, command = %command, "ran favorite");
        }
        if self.enable_toasts {
            for toast in outcome.toasts {
                let (text, kind) = match toast {
                    ToastPolicy::Launched(label) => {
                        (format!("Launched {label}"), ToastKind::Success)
                    }
                    ToastPolicy::Copied(label) => (format!("Copied {label}"), ToastKind::Success),
                    ToastPolicy::Info(message) => (message, ToastKind::Info),
                    ToastPolicy::Success(message) => (message, ToastKind::Success),
                };
                push_toast(
                    &mut self.toasts,
                    Toast {
                        text: text.into(),
                        kind,
                        options: ToastOptions::default()
                            .duration_in_seconds(self.toast_duration as f64),
                    },
                );
            }
        }
        if outcome.history == HistoryPolicy::Record {
            self.record_history_usage(
                &invocation.original_action,
                &history_query,
                invocation.source,
            );
        }
    }
}
