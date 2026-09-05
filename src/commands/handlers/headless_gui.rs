use super::super::{
    BrowserTabCommand, Command, CommandError, CommandInvocation, CommandOutcome, FavoriteLogPolicy,
    HeadlessCommandHost, HistoryPolicy, QueryPolicy, StorageCommand, TimerCommand, ToastPolicy,
    VisibilityPolicy,
};

pub(crate) fn handle_headless_gui<H: HeadlessCommandHost + ?Sized>(
    host: &mut H,
    invocation: &CommandInvocation,
) -> Result<CommandOutcome, CommandError> {
    if let Command::BrowserTab(BrowserTabCommand::Switch(_)) = &invocation.command {
        host.spawn_headless_command(
            invocation.command.clone(),
            invocation.original_action.clone(),
        );
        return Ok(CommandOutcome {
            focus: host.launcher_should_refocus(),
            history: HistoryPolicy::Record,
            toasts: vec![ToastPolicy::Info(format!(
                "Switching to {}",
                invocation.original_action.label
            ))],
            ..CommandOutcome::default()
        });
    }

    if let Err(error) =
        host.execute_headless_command(&invocation.command, &invocation.original_action)
    {
        let mut error =
            CommandError::new("launcher", format!("Failed: {error}")).with_gui_failure_policy();
        if is_external_favorite(invocation) {
            error = error.with_favorite(invocation.original_action.label.clone());
        }
        return Err(error);
    }

    Ok(success_outcome(host, invocation))
}

fn success_outcome<H: HeadlessCommandHost + ?Sized>(
    host: &H,
    invocation: &CommandInvocation,
) -> CommandOutcome {
    let mut outcome = CommandOutcome {
        history: HistoryPolicy::Record,
        toasts: success_toasts(invocation),
        favorite_log: favorite_log(invocation),
        ..CommandOutcome::default()
    };
    let mut command_changed_query = false;

    match &invocation.command {
        Command::Storage(StorageCommand::BookmarkAdd(_)) => {
            outcome.query = QueryPolicy::Set(if host.preserve_command() {
                "bm add ".into()
            } else {
                String::new()
            });
            outcome.search = true;
            outcome.invalidate_results = true;
            outcome.focus = true;
            command_changed_query = true;
        }
        Command::Storage(StorageCommand::FolderAdd(_)) => {
            outcome.query = QueryPolicy::Set(if host.preserve_command() {
                "f add ".into()
            } else {
                String::new()
            });
            outcome.search = true;
            outcome.invalidate_results = true;
            outcome.focus = true;
            command_changed_query = true;
        }
        Command::Storage(
            StorageCommand::BookmarkRemove(_)
            | StorageCommand::FolderRemove(_)
            | StorageCommand::SnippetRemove(_)
            | StorageCommand::FavoriteAdd { .. }
            | StorageCommand::FavoriteRemove(_)
            | StorageCommand::TempfileRemove(_)
            | StorageCommand::TempfileAlias { .. },
        ) => {
            outcome.search = true;
            outcome.invalidate_results = true;
            outcome.focus = true;
        }
        Command::Storage(StorageCommand::TempfileNew(_)) => {
            outcome.query = QueryPolicy::Set(if host.preserve_command() {
                "tmp new ".into()
            } else {
                String::new()
            });
            outcome.focus = true;
            command_changed_query = true;
        }
        Command::Timer(TimerCommand::Cancel(_)) if host.current_query().starts_with("timer rm") => {
            outcome.search = true;
            outcome.invalidate_results = true;
            outcome.focus = true;
        }
        Command::Timer(TimerCommand::Pause(_))
            if host.current_query().starts_with("timer pause") =>
        {
            outcome.search = true;
            outcome.invalidate_results = true;
            outcome.focus = true;
        }
        Command::Timer(TimerCommand::Resume(_))
            if host.current_query().starts_with("timer resume") =>
        {
            outcome.search = true;
            outcome.invalidate_results = true;
            outcome.focus = true;
        }
        Command::Timer(TimerCommand::Start { .. })
            if host.current_query().starts_with("timer add") =>
        {
            outcome.query = QueryPolicy::Set(if host.preserve_command() {
                "timer add ".into()
            } else {
                String::new()
            });
            outcome.focus = true;
            command_changed_query = true;
        }
        _ => {}
    }

    if host.clear_query_after_run() && !command_changed_query {
        outcome.query = QueryPolicy::Set(String::new());
        outcome.search = true;
        outcome.invalidate_results = true;
        outcome.focus = true;
    }

    if host.hide_after_run() && !is_hide_exempt(&invocation.command) {
        outcome.visibility = VisibilityPolicy::Hide;
    }

    if !outcome.focus
        && outcome.visibility != VisibilityPolicy::Hide
        && host.launcher_should_refocus()
    {
        outcome.focus = true;
    }
    outcome
}

fn is_hide_exempt(command: &Command) -> bool {
    matches!(
        command,
        Command::Calculator(_)
            | Command::Storage(
                StorageCommand::BookmarkAdd(_)
                    | StorageCommand::BookmarkRemove(_)
                    | StorageCommand::FolderAdd(_)
                    | StorageCommand::FolderRemove(_)
                    | StorageCommand::SnippetRemove(_)
                    | StorageCommand::FavoriteAdd { .. }
                    | StorageCommand::FavoriteRemove(_)
            )
    )
}

fn success_toasts(invocation: &CommandInvocation) -> Vec<ToastPolicy> {
    let mut toasts = match &invocation.command {
        Command::Storage(StorageCommand::RecycleClean) => Vec::new(),
        Command::Clipboard(_) => vec![ToastPolicy::Copied(
            invocation.original_action.label.clone(),
        )],
        _ => vec![ToastPolicy::Launched(
            invocation.original_action.label.clone(),
        )],
    };
    if matches!(
        invocation.command,
        Command::Storage(StorageCommand::SnippetRemove(_))
    ) {
        toasts.push(ToastPolicy::Success(format!(
            "Removed snippet {}",
            invocation.original_action.label
        )));
    }
    toasts
}

pub(super) fn is_external_favorite(invocation: &CommandInvocation) -> bool {
    invocation.original_action.desc == "Fav"
        && !matches!(
            invocation.command,
            Command::Storage(
                StorageCommand::FavoriteAdd { .. }
                    | StorageCommand::FavoriteRemove(_)
                    | StorageCommand::FavoriteDialog(_)
            )
        )
}

pub(super) fn favorite_log(invocation: &CommandInvocation) -> FavoriteLogPolicy {
    if is_external_favorite(invocation) {
        FavoriteLogPolicy::Ran {
            label: invocation.original_action.label.clone(),
            command: invocation.original_action.action.clone(),
        }
    } else {
        FavoriteLogPolicy::None
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{actions::Action, commands::ActivationSource};

    struct FakeHost {
        query: String,
        clear: bool,
        hide: bool,
        preserve: bool,
        executed: usize,
        spawned: usize,
        fail: bool,
    }

    impl HeadlessCommandHost for FakeHost {
        fn execute_headless_command(&mut self, _: &Command, _: &Action) -> anyhow::Result<()> {
            self.executed += 1;
            if self.fail {
                anyhow::bail!("injected failure")
            }
            Ok(())
        }
        fn spawn_headless_command(&mut self, _: Command, _: Action) {
            self.spawned += 1;
        }
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
            true
        }
    }

    fn invocation(command: Command, raw: &str) -> CommandInvocation {
        CommandInvocation {
            command,
            original_action: Action {
                label: "Example".into(),
                desc: String::new(),
                action: raw.into(),
                args: None,
            },
            query_override: None,
            source: ActivationSource::Enter,
        }
    }

    fn host() -> FakeHost {
        FakeHost {
            query: String::new(),
            clear: false,
            hide: false,
            preserve: false,
            executed: 0,
            spawned: 0,
            fail: false,
        }
    }

    #[test]
    fn bookmark_add_encodes_preserve_refresh_focus_and_hide_exemption() {
        let mut host = host();
        host.preserve = true;
        host.hide = true;
        let result = handle_headless_gui(
            &mut host,
            &invocation(
                Command::Storage(StorageCommand::BookmarkAdd("url".into())),
                "bookmark:add:url",
            ),
        )
        .unwrap();
        assert_eq!(result.query, QueryPolicy::Set("bm add ".into()));
        assert!(result.search && result.invalidate_results && result.focus);
        assert_eq!(result.visibility, VisibilityPolicy::Keep);
        assert_eq!(result.history, HistoryPolicy::Record);
    }

    #[test]
    fn generic_success_applies_clear_hide_history_and_toast() {
        let mut host = host();
        host.clear = true;
        host.hide = true;
        let result = handle_headless_gui(
            &mut host,
            &invocation(
                Command::External(super::super::super::ExternalCommand {
                    target: "tool".into(),
                    args: None,
                }),
                "tool",
            ),
        )
        .unwrap();
        assert_eq!(result.query, QueryPolicy::Set(String::new()));
        assert!(result.search && result.invalidate_results && result.focus);
        assert_eq!(result.visibility, VisibilityPolicy::Hide);
        assert_eq!(result.history, HistoryPolicy::Record);
        assert_eq!(result.toasts, vec![ToastPolicy::Launched("Example".into())]);
    }

    #[test]
    fn snippet_remove_preserves_generic_and_specific_success_toasts() {
        let mut host = host();
        let result = handle_headless_gui(
            &mut host,
            &invocation(
                Command::Storage(StorageCommand::SnippetRemove("alias".into())),
                "snippet:remove:alias",
            ),
        )
        .unwrap();
        assert_eq!(
            result.toasts,
            vec![
                ToastPolicy::Launched("Example".into()),
                ToastPolicy::Success("Removed snippet Example".into()),
            ]
        );
        assert!(result.search && result.invalidate_results && result.focus);
    }
    #[test]
    fn failures_do_not_apply_success_post_policy() {
        let mut host = host();
        host.fail = true;
        host.clear = true;
        host.hide = true;
        let error = handle_headless_gui(
            &mut host,
            &invocation(
                Command::External(super::super::super::ExternalCommand {
                    target: "tool".into(),
                    args: None,
                }),
                "tool",
            ),
        )
        .unwrap_err();
        assert_eq!(error.message, "Failed: injected failure");
        assert!(error.toast && error.refocus);
    }

    #[test]
    fn browser_switch_is_async_without_generic_clear_or_hide() {
        let mut host = host();
        host.clear = true;
        host.hide = true;
        let result = handle_headless_gui(
            &mut host,
            &invocation(
                Command::BrowserTab(BrowserTabCommand::Switch(vec![1])),
                "tab:switch:1",
            ),
        )
        .unwrap();
        assert_eq!(host.spawned, 1);
        assert_eq!(host.executed, 0);
        assert_eq!(result.query, QueryPolicy::Keep);
        assert_eq!(result.visibility, VisibilityPolicy::Keep);
        assert_eq!(result.history, HistoryPolicy::Record);
        assert_eq!(
            result.toasts,
            vec![ToastPolicy::Info("Switching to Example".into())]
        );
    }
}
