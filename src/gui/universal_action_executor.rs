use std::sync::atomic::Ordering;

use crate::actions::Action;
use crate::commands::{
    ActivationSource, CommandInvocation, CommandOutcome, FavoriteLogPolicy, HistoryPolicy,
    PendingQueryPolicy, QueryPolicy, ResultsPolicy, ToastPolicy, VisibilityPolicy,
};
use crate::history::{self, HISTORY_PINS_FILE, HistoryPin};
use crate::universal_actions::{
    ActionSafety, ActionSurface, NoteExternalEditor, UniversalAction, UniversalActionOperation,
    UniversalUiIntent, action_ids,
};

use super::{
    DestructiveAction, LauncherApp, NoteExternalOpen, PendingUniversalActionInvocation,
    spawn_external,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UniversalActionExecution {
    Executed,
    ConfirmationRequired,
    Unavailable,
}

impl LauncherApp {
    /// Execute a surface-independent action while retaining the input source
    /// and presentation surface as distinct invocation context.
    pub(crate) fn execute_universal_action(
        &mut self,
        action: UniversalAction,
        surface: ActionSurface,
        source: ActivationSource,
    ) -> UniversalActionExecution {
        if let Some(reason) = action.availability.disabled_reason() {
            self.report_error_message("universal_action", reason);
            return UniversalActionExecution::Unavailable;
        }

        let before = self.launcher_interaction_snapshot();
        if self.require_confirm_destructive && action.safety == ActionSafety::Destructive {
            let Some(kind) = DestructiveAction::from_universal_action(&action) else {
                self.report_error_message(
                    "universal_action",
                    format!("Missing confirmation metadata for {}", action.id),
                );
                return UniversalActionExecution::Unavailable;
            };
            self.pending_universal_confirm = Some(PendingUniversalActionInvocation {
                action,
                surface,
                source,
            });
            self.confirm_modal.open_for_source(kind, Some(source));
            self.restore_for_new_launcher_interaction(&before);
            return UniversalActionExecution::ConfirmationRequired;
        }

        self.execute_universal_action_confirmed(action, surface, source);
        self.restore_for_new_launcher_interaction(&before);
        UniversalActionExecution::Executed
    }

    pub(super) fn resolve_pending_universal_action_confirmation(
        &mut self,
        confirmed: bool,
    ) -> bool {
        let Some(pending) = self.pending_universal_confirm.take() else {
            return false;
        };
        if confirmed {
            let before = self.launcher_interaction_snapshot();
            self.execute_universal_action_confirmed(
                pending.action,
                pending.surface,
                pending.source,
            );
            self.restore_for_new_launcher_interaction(&before);
        }
        true
    }

    fn execute_universal_action_confirmed(
        &mut self,
        action: UniversalAction,
        _surface: ActionSurface,
        source: ActivationSource,
    ) {
        let action_id = action.id.clone();
        match action.operation {
            UniversalActionOperation::InvokePrimary(action) => {
                // This is intentionally the exact legacy primary activation path.
                self.activate_action(action, None, source);
            }
            UniversalActionOperation::Command {
                command,
                original_action,
            } => {
                self.dispatch_universal_secondary_command(
                    action_id.as_str(),
                    CommandInvocation {
                        command,
                        original_action,
                        query_override: None,
                        source,
                    },
                );
            }
            UniversalActionOperation::UiIntent(intent) => {
                self.execute_universal_ui_intent(action_id.as_str(), intent)
            }
        }
    }

    fn dispatch_universal_secondary_command(
        &mut self,
        action_id: &str,
        invocation: CommandInvocation,
    ) {
        let bus = std::sync::Arc::clone(&self.command_bus);
        match bus.dispatch(&invocation, self) {
            Ok(outcome) => {
                let outcome = normalize_secondary_outcome(
                    action_id,
                    &invocation.original_action,
                    self.query.trim(),
                    outcome,
                );
                self.apply_command_outcome(outcome, &invocation);
            }
            Err(error) => {
                if let Some(favorite) = error.favorite.as_ref() {
                    tracing::error!(fav = %favorite, error = %error.message, "failed to run favorite");
                }
                let message = error.message;
                self.report_error_message(error.domain, message.clone());
                if error.toast {
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

    fn execute_universal_ui_intent(&mut self, action_id: &str, intent: UniversalUiIntent) {
        match intent {
            UniversalUiIntent::EditCustomAction { index } => {
                if let Some(action) = self.actions.get(index).cloned() {
                    self.editor.open_edit(index, &action);
                    self.show_editor = true;
                } else {
                    self.report_error_message("universal_action", "Custom action not found");
                }
            }
            UniversalUiIntent::OpenFolderAlias { path } => self.alias_dialog.open(&path),
            UniversalUiIntent::OpenBookmarkAlias { url } => self.bookmark_alias_dialog.open(&url),
            UniversalUiIntent::EditSnippet { alias } => self.snippet_dialog.open_edit(&alias),
            UniversalUiIntent::OpenTempfileAlias { path } => self.tempfile_alias_dialog.open(&path),
            UniversalUiIntent::EditNote { slug } => self.open_note_panel(&slug, None),
            UniversalUiIntent::OpenNoteExternal { slug, editor } => match editor {
                NoteExternalEditor::Notepad => match crate::plugins::note::load_notes() {
                    Ok(notes) => match notes.iter().find(|note| note.slug == slug) {
                        Some(note) => {
                            if let Err(error) = std::process::Command::new("notepad.exe")
                                .arg(&note.path)
                                .spawn()
                            {
                                self.report_error_message("launcher", error.to_string());
                            }
                        }
                        None => self.report_error_message("launcher", "Note not found"),
                    },
                    Err(error) => self.report_error_message("launcher", error.to_string()),
                },
                NoteExternalEditor::Neovim => {
                    self.open_note_in_neovim(&slug, crate::plugins::note::load_notes, |path| {
                        spawn_external(path, NoteExternalOpen::Wezterm)
                    });
                }
            },
            UniversalUiIntent::EditClipboardEntry { index } => {
                self.clipboard_dialog.open_edit(index)
            }
            UniversalUiIntent::RemoveClipboardEntry { index, label } => {
                match crate::plugins::clipboard::remove_entry(
                    crate::plugins::clipboard::CLIPBOARD_FILE,
                    index,
                ) {
                    Ok(()) => {
                        self.last_results_valid = false;
                        self.search();
                        self.focus_input();
                        self.add_success_toast(format!("Removed entry {label}"));
                    }
                    Err(error) => self.report_error_message(
                        "launcher",
                        format!("Failed to remove entry: {error}"),
                    ),
                }
            }
            UniversalUiIntent::EditTodo { index } => self.todo_view_dialog.open_edit(index),
            UniversalUiIntent::AddFavorite { action } => {
                self.fav_dialog.open_prefilled_add(&action)
            }
            UniversalUiIntent::PinResult { action, query }
            | UniversalUiIntent::ReplacePin { action, query } => {
                let pin = history_pin(&action, &query, chrono::Utc::now().timestamp());
                match history::upsert_pin(HISTORY_PINS_FILE, &pin) {
                    Ok(_) => {
                        self.add_success_toast(if action_id == action_ids::RESULT_PIN.as_str() {
                            format!("Pinned {}", action.label)
                        } else {
                            format!("Updated pin for {}", action.label)
                        })
                    }
                    Err(error) => self
                        .report_error_message("launcher", format!("Failed to pin result: {error}")),
                }
            }
            UniversalUiIntent::UnpinResult { action } => {
                match history::remove_pin(HISTORY_PINS_FILE, &action.action, action.args.as_deref())
                {
                    Ok(_) => self.add_success_toast(format!("Unpinned {}", action.label)),
                    Err(error) => self.report_error_message(
                        "launcher",
                        format!("Failed to unpin result: {error}"),
                    ),
                }
            }
            UniversalUiIntent::RecomputePins => {
                match history::recompute_pins(HISTORY_PINS_FILE, |pin| self.resolve_pin_action(pin))
                {
                    Ok(report) => {
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
                        if report.missing > 0 {
                            self.add_warning_toast(text);
                        } else {
                            self.add_success_toast(text);
                        }
                    }
                    Err(error) => self.report_error_message(
                        "launcher",
                        format!("Failed to recompute pins: {error}"),
                    ),
                }
            }
            UniversalUiIntent::CopyStopwatchTime { id } => {
                if let Some(time) = crate::plugins::stopwatch::format_elapsed(id) {
                    match crate::actions::clipboard::set_text(&time) {
                        Ok(()) => self.add_success_toast(format!("Copied {time}")),
                        Err(error) => self.report_error_message(
                            "launcher",
                            format!("Failed to copy time: {error}"),
                        ),
                    }
                }
            }
            UniversalUiIntent::OpenMkMacro { id } => {
                self.mkmacro_dialog.open();
                self.mkmacro_dialog.set_selected_macro(Some(id));
            }
        }
    }
}

fn history_pin(action: &Action, query: &str, timestamp: i64) -> HistoryPin {
    HistoryPin {
        action_id: action.action.clone(),
        label: action.label.clone(),
        desc: action.desc.clone(),
        args: action.args.clone(),
        query: query.to_string(),
        timestamp,
    }
}

fn normalize_secondary_outcome(
    action_id: &str,
    original_action: &Action,
    query: &str,
    outcome: CommandOutcome,
) -> CommandOutcome {
    let mut normalized = CommandOutcome {
        query: QueryPolicy::Keep,
        pending_query: PendingQueryPolicy::Keep,
        results: ResultsPolicy::Keep,
        visibility: VisibilityPolicy::Keep,
        history: HistoryPolicy::Skip,
        favorite_log: FavoriteLogPolicy::None,
        toasts: outcome
            .toasts
            .into_iter()
            .filter(|toast| !matches!(toast, ToastPolicy::Launched(_)))
            .collect(),
        ..CommandOutcome::default()
    };

    let (refresh, message) = match action_id {
        value if value == action_ids::FOLDER_REMOVE.as_str() => (
            true,
            Some(format!("Removed folder {}", original_action.label)),
        ),
        value if value == action_ids::BOOKMARK_REMOVE.as_str() => (
            true,
            Some(format!("Removed bookmark {}", original_action.label)),
        ),
        value if value == action_ids::SNIPPET_REMOVE.as_str() => {
            // The command handler already supplies this domain-specific toast.
            (true, None)
        }
        value if value == action_ids::TEMPFILE_DELETE.as_str() => (
            true,
            Some(format!("Removed file {}", original_action.label)),
        ),
        value if value == action_ids::TIMER_PAUSE.as_str() && query.starts_with("timer list") => (
            true,
            Some(format!("Paused timer {}", original_action.label)),
        ),
        value if value == action_ids::TIMER_RESUME.as_str() && query.starts_with("timer list") => (
            true,
            Some(format!("Resumed timer {}", original_action.label)),
        ),
        value if value == action_ids::TIMER_CANCEL.as_str() && query.starts_with("timer list") => (
            true,
            Some(format!("Removed timer {}", original_action.label)),
        ),
        value if value == action_ids::STOPWATCH_PAUSE.as_str() && query.starts_with("sw list") => (
            true,
            Some(format!("Paused stopwatch {}", original_action.label)),
        ),
        value if value == action_ids::STOPWATCH_RESUME.as_str() && query.starts_with("sw list") => {
            (
                true,
                Some(format!("Resumed stopwatch {}", original_action.label)),
            )
        }
        value if value == action_ids::STOPWATCH_STOP.as_str() && query.starts_with("sw list") => (
            true,
            Some(format!("Stopped stopwatch {}", original_action.label)),
        ),
        _ => (false, None),
    };
    if refresh {
        normalized.search = true;
        normalized.invalidate_results = true;
        normalized.focus = true;
    }
    if let Some(message) = message {
        normalized.toasts.push(ToastPolicy::Success(message));
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{Command, SystemCommand, TodoCommand};
    use crate::universal_actions::{
        ActionAvailability, ActionGroup, ActionIconKey, ActionPresentation, ActionPriority,
        ActionTarget,
    };

    fn action() -> Action {
        Action {
            label: "Project".into(),
            desc: "Folder".into(),
            action: r#"C:\work\project"#.into(),
            args: Some("--reuse-window".into()),
        }
    }

    fn universal(
        id: crate::universal_actions::ActionId,
        target: ActionTarget,
        safety: ActionSafety,
        operation: UniversalActionOperation,
    ) -> UniversalAction {
        UniversalAction {
            id,
            target,
            presentation: ActionPresentation {
                label: "Test action".into(),
                short_label: None,
                description: None,
                icon: Some(ActionIconKey::Open),
                group: ActionGroup::Other,
                priority: ActionPriority::Normal,
                visible: true,
                surface_overrides: Default::default(),
            },
            availability: ActionAvailability::Available,
            safety,
            operation,
        }
    }

    #[test]
    fn pin_bridge_preserves_existing_schema_fields() {
        let action = action();
        let pin = history_pin(&action, "f project", 42);
        assert_eq!(pin.action_id, action.action);
        assert_eq!(pin.label, action.label);
        assert_eq!(pin.desc, action.desc);
        assert_eq!(pin.args, action.args);
        assert_eq!(pin.query, "f project");
        assert_eq!(pin.timestamp, 42);
    }

    #[test]
    fn secondary_normalization_strips_primary_side_effects() {
        let outcome = CommandOutcome {
            query: QueryPolicy::Set(String::new()),
            search: true,
            invalidate_results: true,
            visibility: VisibilityPolicy::Hide,
            focus: true,
            history: HistoryPolicy::Record,
            toasts: vec![ToastPolicy::Launched("Project".into())],
            favorite_log: FavoriteLogPolicy::Ran {
                label: "Project".into(),
                command: "run".into(),
            },
            ..CommandOutcome::default()
        };

        let normalized = normalize_secondary_outcome(
            action_ids::WINDOW_ACTIVATE.as_str(),
            &action(),
            "keep me",
            outcome,
        );

        assert_eq!(normalized.query, QueryPolicy::Keep);
        assert!(!normalized.search);
        assert!(!normalized.invalidate_results);
        assert_eq!(normalized.visibility, VisibilityPolicy::Keep);
        assert!(!normalized.focus);
        assert_eq!(normalized.history, HistoryPolicy::Skip);
        assert!(normalized.toasts.is_empty());
        assert_eq!(normalized.favorite_log, FavoriteLogPolicy::None);
    }

    #[test]
    fn legacy_list_actions_retain_refresh_focus_and_specific_toast() {
        let normalized = normalize_secondary_outcome(
            action_ids::TIMER_CANCEL.as_str(),
            &action(),
            "timer list",
            CommandOutcome::default(),
        );
        assert!(normalized.search);
        assert!(normalized.invalidate_results);
        assert!(normalized.focus);
        assert_eq!(
            normalized.toasts,
            vec![ToastPolicy::Success("Removed timer Project".into())]
        );

        let resumed = normalize_secondary_outcome(
            action_ids::TIMER_RESUME.as_str(),
            &action(),
            "timer list",
            CommandOutcome::default(),
        );
        assert!(resumed.search && resumed.invalidate_results && resumed.focus);
        assert_eq!(
            resumed.toasts,
            vec![ToastPolicy::Success("Resumed timer Project".into())]
        );
    }

    #[test]
    fn primary_universal_action_delegates_to_exact_activation_path() {
        let ctx = eframe::egui::Context::default();
        let mut app = crate::gui::actions::tests::new_app(&ctx);
        let primary = Action {
            label: "Help".into(),
            desc: "Test".into(),
            action: "help:show".into(),
            args: None,
        };

        let result = app.execute_universal_action(
            universal(
                action_ids::RESULT_EXECUTE,
                ActionTarget::Generic {
                    action: primary.clone(),
                },
                ActionSafety::Normal,
                UniversalActionOperation::InvokePrimary(primary.clone()),
            ),
            ActionSurface::ActionSheet,
            ActivationSource::Enter,
        );

        assert_eq!(result, UniversalActionExecution::Executed);
        assert_eq!(
            app.test_activation_trace.last(),
            Some(&(primary, ActivationSource::Enter))
        );
        assert!(app.help_window.open);
    }

    #[test]
    fn secondary_command_keeps_launcher_query_visibility_history_and_generic_toasts() {
        let ctx = eframe::egui::Context::default();
        let mut app = crate::gui::actions::tests::new_app(&ctx);
        app.query = "keep this query".into();
        app.hide_after_run = true;
        app.clear_query_after_run = true;
        app.enable_toasts = true;
        app.visible_flag.store(true, Ordering::SeqCst);
        let original = Action {
            label: "Todo".into(),
            desc: "Todo".into(),
            action: "todo:done:0".into(),
            args: None,
        };

        app.execute_universal_action(
            universal(
                action_ids::TODO_EDIT,
                ActionTarget::Todo { index: 0 },
                ActionSafety::Normal,
                UniversalActionOperation::Command {
                    command: Command::Todo(TodoCommand::Edit { index: 0 }),
                    original_action: original,
                },
            ),
            ActionSurface::ContextMenu,
            ActivationSource::Click,
        );

        assert_eq!(app.query, "keep this query");
        assert!(app.visible_flag.load(Ordering::SeqCst));
        assert!(app.test_recorded_history_queries.is_empty());
        assert!(app.test_toast_messages.is_empty());
    }

    #[test]
    fn destructive_confirmation_retains_and_resumes_universal_invocation_context() {
        let ctx = eframe::egui::Context::default();
        let mut app = crate::gui::actions::tests::new_app(&ctx);
        app.query = "window query".into();
        app.require_confirm_destructive = true;
        app.visible_flag.store(true, Ordering::SeqCst);
        let original = Action {
            label: "Editor".into(),
            desc: "Window".into(),
            action: "window:close:0".into(),
            args: None,
        };
        let action = universal(
            action_ids::WINDOW_CLOSE,
            ActionTarget::Window { hwnd: 0 },
            ActionSafety::Destructive,
            UniversalActionOperation::Command {
                command: Command::System(SystemCommand::WindowClose(0)),
                original_action: original,
            },
        );

        assert_eq!(
            app.execute_universal_action(
                action.clone(),
                ActionSurface::RadialMenu,
                ActivationSource::Gesture,
            ),
            UniversalActionExecution::ConfirmationRequired
        );
        let pending = app
            .pending_universal_confirm
            .as_ref()
            .expect("pending universal action");
        assert_eq!(pending.action, action);
        assert_eq!(pending.surface, ActionSurface::RadialMenu);
        assert_eq!(pending.source, ActivationSource::Gesture);

        app.resolve_pending_confirmation(true);
        assert!(app.pending_universal_confirm.is_none());
        assert_eq!(app.query, "window query");
        assert!(app.visible_flag.load(Ordering::SeqCst));
        assert!(app.test_recorded_history_queries.is_empty());
    }
}
