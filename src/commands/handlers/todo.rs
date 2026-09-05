use super::headless_gui::{favorite_log, is_external_favorite};
use crate::commands::{
    CommandError, CommandInvocation, CommandOutcome, HeadlessCommandHost, HistoryPolicy,
    PendingQueryPolicy, QueryPolicy, ToastPolicy, TodoCommand, TodoCommandHost, VisibilityPolicy,
};

pub(crate) fn handle_todo<H>(
    host: &mut H,
    command: &TodoCommand,
    invocation: &CommandInvocation,
) -> Result<CommandOutcome, CommandError>
where
    H: TodoCommandHost + HeadlessCommandHost + ?Sized,
{
    match command {
        // The dialog command is consumed by the simple-dialog handler before this
        // handler. Keeping this arm side-effect free makes direct handler use safe.
        TodoCommand::Dialog => Ok(CommandOutcome::default()),
        TodoCommand::View => {
            host.open_todo_view();
            Ok(CommandOutcome {
                focus: host.launcher_should_refocus(),
                ..CommandOutcome::default()
            })
        }
        TodoCommand::Edit { index } => {
            host.open_todo_editor(*index);
            Ok(CommandOutcome {
                focus: host.launcher_should_refocus(),
                ..CommandOutcome::default()
            })
        }
        TodoCommand::Add { .. }
        | TodoCommand::SetPriority { .. }
        | TodoCommand::SetTags { .. }
        | TodoCommand::Remove { .. }
        | TodoCommand::Done { .. }
        | TodoCommand::Clear
        | TodoCommand::Export => execute(host, command, invocation),
    }
}

fn execute<H>(
    host: &mut H,
    command: &TodoCommand,
    invocation: &CommandInvocation,
) -> Result<CommandOutcome, CommandError>
where
    H: TodoCommandHost + HeadlessCommandHost + ?Sized,
{
    let current_query = host.current_query().to_owned();
    host.execute_headless_command(&invocation.command, &invocation.original_action)
        .map_err(|error| {
            let error =
                CommandError::new("launcher", format!("Failed: {error}")).with_gui_failure_policy();
            if is_external_favorite(invocation) {
                error.with_favorite(invocation.original_action.label.clone())
            } else {
                error
            }
        })?;

    let mut outcome = CommandOutcome {
        history: HistoryPolicy::Record,
        toasts: vec![ToastPolicy::Launched(
            invocation.original_action.label.clone(),
        )],
        favorite_log: favorite_log(invocation),
        ..CommandOutcome::default()
    };
    let mut command_changed_query = false;

    match command {
        TodoCommand::Add { toast_text, .. } => {
            outcome.query = QueryPolicy::Set(if host.preserve_command() {
                "todo add ".into()
            } else {
                String::new()
            });
            outcome.search = true;
            outcome.invalidate_results = true;
            outcome.focus = true;
            command_changed_query = true;
            outcome
                .toasts
                .push(ToastPolicy::Success(format!("Added todo {toast_text}")));
        }
        TodoCommand::Remove { .. } => {
            outcome.search = true;
            outcome.invalidate_results = true;
            outcome.focus = true;
            if current_query.starts_with("note list") {
                outcome.pending_query = PendingQueryPolicy::Set(current_query.clone());
                command_changed_query = true;
            }
            let label = invocation
                .original_action
                .label
                .strip_prefix("Remove todo ")
                .unwrap_or(&invocation.original_action.label);
            outcome
                .toasts
                .push(ToastPolicy::Success(format!("Removed todo {label}")));
        }
        TodoCommand::Done { .. } => {
            outcome.search = true;
            outcome.invalidate_results = true;
            outcome.focus = true;
            outcome.pending_query = PendingQueryPolicy::Set(current_query.clone());
            command_changed_query = true;
            let label = invocation
                .original_action
                .label
                .trim_start_matches("[x] ")
                .trim_start_matches("[ ] ");
            outcome
                .toasts
                .push(ToastPolicy::Success(format!("Toggled todo {label}")));
        }
        TodoCommand::SetPriority { .. } => {
            outcome.search = true;
            outcome.invalidate_results = true;
            outcome.focus = true;
            outcome
                .toasts
                .push(ToastPolicy::Success("Updated todo priority".into()));
        }
        TodoCommand::SetTags { .. } => {
            outcome.search = true;
            outcome.invalidate_results = true;
            outcome.focus = true;
            outcome
                .toasts
                .push(ToastPolicy::Success("Updated todo tags".into()));
        }
        TodoCommand::Clear => {
            outcome.search = true;
            outcome.invalidate_results = true;
            outcome.focus = true;
            outcome
                .toasts
                .push(ToastPolicy::Success("Cleared completed todos".into()));
        }
        TodoCommand::Export => {}
        TodoCommand::Dialog | TodoCommand::View | TodoCommand::Edit { .. } => unreachable!(),
    }

    if host.clear_query_after_run() && !command_changed_query {
        outcome.query = QueryPolicy::Set(String::new());
        outcome.search = true;
        outcome.invalidate_results = true;
        outcome.focus = true;
    }
    if host.hide_after_run() && !matches!(command, TodoCommand::Done { .. }) {
        outcome.visibility = VisibilityPolicy::Hide;
    }
    if !outcome.focus
        && outcome.visibility != VisibilityPolicy::Hide
        && host.launcher_should_refocus()
    {
        outcome.focus = true;
    }
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::Action;
    use crate::commands::{ActivationSource, Command, FavoriteLogPolicy};

    #[derive(Default)]
    struct Host {
        query: String,
        preserve: bool,
        clear: bool,
        hide: bool,
        refocus: bool,
        fail: bool,
        executed: usize,
        opened: Vec<String>,
    }

    impl TodoCommandHost for Host {
        fn open_todo_view(&mut self) {
            self.opened.push("view".into());
        }
        fn open_todo_editor(&mut self, index: usize) {
            self.opened.push(format!("edit:{index}"));
        }
    }

    impl HeadlessCommandHost for Host {
        fn execute_headless_command(&mut self, _: &Command, _: &Action) -> anyhow::Result<()> {
            self.executed += 1;
            if self.fail {
                anyhow::bail!("injected failure");
            }
            Ok(())
        }
        fn spawn_headless_command(&mut self, _: Command, _: Action) {}
        fn clear_query_after_run(&self) -> bool {
            self.clear
        }
        fn hide_after_run(&self) -> bool {
            self.hide
        }
        fn preserve_command(&self) -> bool {
            self.preserve
        }
        fn current_query(&self) -> &str {
            &self.query
        }
        fn launcher_should_refocus(&self) -> bool {
            self.refocus
        }
    }

    fn invocation(command: TodoCommand, raw: &str, label: &str) -> CommandInvocation {
        CommandInvocation {
            command: Command::Todo(command),
            original_action: Action {
                label: label.into(),
                desc: "Todo".into(),
                action: raw.into(),
                args: None,
            },
            query_override: None,
            source: ActivationSource::Enter,
        }
    }

    #[test]
    fn view_and_edit_use_ui_host_without_headless_execution_or_history() {
        let mut host = Host::default();
        let view = invocation(TodoCommand::View, "todo:view", "View todos");
        let edit = invocation(TodoCommand::Edit { index: 7 }, "todo:edit:7", "Edit todo");
        assert_eq!(
            handle_todo(&mut host, &TodoCommand::View, &view)
                .unwrap()
                .history,
            HistoryPolicy::Skip
        );
        assert_eq!(
            handle_todo(&mut host, &TodoCommand::Edit { index: 7 }, &edit)
                .unwrap()
                .history,
            HistoryPolicy::Skip
        );
        assert_eq!(host.opened, ["view", "edit:7"]);
        assert_eq!(host.executed, 0);
    }

    #[test]
    fn add_preserves_prefix_and_applies_generic_history() {
        let command = TodoCommand::Add {
            text: "ship|it".into(),
            priority: 3,
            tags: vec!["work".into()],
            refs: Vec::new(),
            toast_text: "ship".into(),
        };
        let invocation = invocation(command.clone(), "todo:add:ship|3|work", "Add todo");
        let mut host = Host {
            query: "todo add ship".into(),
            preserve: true,
            hide: true,
            ..Host::default()
        };
        let outcome = handle_todo(&mut host, &command, &invocation).unwrap();
        assert_eq!(outcome.query, QueryPolicy::Set("todo add ".into()));
        assert!(outcome.search && outcome.invalidate_results && outcome.focus);
        assert_eq!(outcome.visibility, VisibilityPolicy::Hide);
        assert_eq!(outcome.history, HistoryPolicy::Record);
        assert_eq!(
            outcome.toasts,
            vec![
                ToastPolicy::Launched("Add todo".into()),
                ToastPolicy::Success("Added todo ship".into()),
            ]
        );
    }

    #[test]
    fn encoded_add_uses_canonical_typed_compatibility_toast_text() {
        let payload = crate::plugins::todo::TodoAddActionPayload {
            text: "ship | release".into(),
            priority: 4,
            tags: vec!["team,alpha".into()],
            refs: Vec::new(),
        };
        let encoded = crate::plugins::todo::encode_todo_add_action_payload(&payload).unwrap();
        let raw = format!("todo:add:{encoded}");
        let parsed = crate::commands::parse_action(&Action {
            label: "Add encoded todo".into(),
            desc: "Todo".into(),
            action: raw.clone(),
            args: None,
        })
        .unwrap();
        let Command::Todo(command) = parsed else {
            panic!("expected typed Todo command");
        };
        let invocation = invocation(command.clone(), &raw, "Add encoded todo");
        let mut host = Host::default();
        let outcome = handle_todo(&mut host, &command, &invocation).unwrap();
        assert_eq!(
            outcome.toasts.last(),
            Some(&ToastPolicy::Success(format!("Added todo {encoded}")))
        );
        assert_eq!(host.executed, 1);
    }
    #[test]
    fn done_preserves_current_query_as_pending_and_is_hide_exempt() {
        let command = TodoCommand::Done { index: 2 };
        let invocation = invocation(command.clone(), "todo:done:2", "[ ] Ship it");
        let mut host = Host {
            query: "todo list".into(),
            clear: true,
            hide: true,
            ..Host::default()
        };
        let outcome = handle_todo(&mut host, &command, &invocation).unwrap();
        assert_eq!(outcome.query, QueryPolicy::Keep);
        assert_eq!(
            outcome.pending_query,
            PendingQueryPolicy::Set("todo list".into())
        );
        assert_eq!(outcome.visibility, VisibilityPolicy::Keep);
        assert_eq!(
            outcome.toasts.last(),
            Some(&ToastPolicy::Success("Toggled todo Ship it".into()))
        );
    }

    #[test]
    fn note_list_remove_preserves_pending_query_but_other_remove_obeys_clear() {
        let command = TodoCommand::Remove { index: 1 };
        let invocation = invocation(command.clone(), "todo:remove:1", "Remove todo Old task");
        let mut note_host = Host {
            query: "note list alpha".into(),
            clear: true,
            ..Host::default()
        };
        let note_outcome = handle_todo(&mut note_host, &command, &invocation).unwrap();
        assert_eq!(note_outcome.query, QueryPolicy::Keep);
        assert_eq!(
            note_outcome.pending_query,
            PendingQueryPolicy::Set("note list alpha".into())
        );

        let mut todo_host = Host {
            query: "todo remove".into(),
            clear: true,
            ..Host::default()
        };
        let todo_outcome = handle_todo(&mut todo_host, &command, &invocation).unwrap();
        assert_eq!(todo_outcome.query, QueryPolicy::Set(String::new()));
        assert_eq!(todo_outcome.pending_query, PendingQueryPolicy::Keep);
        assert_eq!(
            todo_outcome.toasts.last(),
            Some(&ToastPolicy::Success("Removed todo Old task".into()))
        );
    }

    #[test]
    fn priority_tags_clear_and_export_keep_exact_success_policy() {
        let cases = [
            (
                TodoCommand::SetPriority {
                    index: 1,
                    priority: 4,
                },
                Some("Updated todo priority"),
                true,
            ),
            (
                TodoCommand::SetTags {
                    index: 1,
                    tags: vec!["work".into()],
                },
                Some("Updated todo tags"),
                true,
            ),
            (TodoCommand::Clear, Some("Cleared completed todos"), true),
            (TodoCommand::Export, None, false),
        ];
        let mut host = Host::default();
        for (command, specific_toast, refreshes_results) in cases {
            let invocation = invocation(command.clone(), "typed", "Todo action");
            let outcome = handle_todo(&mut host, &command, &invocation).unwrap();
            assert_eq!(outcome.history, HistoryPolicy::Record);
            assert_eq!(outcome.search, refreshes_results);
            assert_eq!(outcome.invalidate_results, refreshes_results);
            assert_eq!(
                outcome.toasts.first(),
                Some(&ToastPolicy::Launched("Todo action".into()))
            );
            match specific_toast {
                Some(message) => assert_eq!(
                    outcome.toasts.last(),
                    Some(&ToastPolicy::Success(message.into()))
                ),
                None => assert_eq!(outcome.toasts.len(), 1),
            }
        }
        assert_eq!(host.executed, 4);
    }
    #[test]
    fn failures_skip_success_policy() {
        let command = TodoCommand::Clear;
        let invocation = invocation(command.clone(), "todo:clear", "Clear todos");
        let mut host = Host {
            fail: true,
            clear: true,
            hide: true,
            refocus: true,
            ..Host::default()
        };
        let error = handle_todo(&mut host, &command, &invocation).unwrap_err();
        assert_eq!(error.message, "Failed: injected failure");
        assert!(error.toast && error.refocus);
    }

    #[test]
    fn favorite_metadata_is_retained_for_todo_execution() {
        let command = TodoCommand::Export;
        let mut invocation = invocation(command.clone(), "todo:export", "Export");
        invocation.original_action.desc = "Fav".into();
        let mut host = Host::default();
        let outcome = handle_todo(&mut host, &command, &invocation).unwrap();
        assert_eq!(
            outcome.favorite_log,
            FavoriteLogPolicy::Ran {
                label: "Export".into(),
                command: "todo:export".into(),
            }
        );
    }
}
