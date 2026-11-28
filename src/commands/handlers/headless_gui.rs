use super::super::{
    BrowserTabCommand, ClipboardCommand, Command, CommandError, CommandInvocation, CommandOutcome,
    ExternalCommand, ExternalCommandHost, ExternalNamespace, FavoriteLogPolicy,
    HeadlessCommandHost, HistoryPolicy, QueryPolicy, StorageCommand, TimerCommand, ToastPolicy,
    VisibilityPolicy,
};

pub(crate) fn handle_headless_gui<H: HeadlessCommandHost + ?Sized>(
    host: &mut H,
    invocation: &CommandInvocation,
) -> Result<CommandOutcome, CommandError> {
    handle_headless_gui_with_history_query(host, invocation, None)
}

pub(crate) fn handle_headless_gui_with_history_query<H: HeadlessCommandHost + ?Sized>(
    host: &mut H,
    invocation: &CommandInvocation,
    captured_history_query: Option<&str>,
) -> Result<CommandOutcome, CommandError> {
    if let Command::Clipboard(ClipboardCommand::CopyGeneratedPassword { length }) =
        &invocation.command
    {
        use crate::platform::protected_clipboard::CopyStatus;

        let status = host
            .copy_generated_password(*length)
            .map_err(CommandError::with_gui_failure_policy)?;
        let mut outcome = success_outcome(host, invocation);
        outcome.history = HistoryPolicy::Skip;
        outcome.favorite_log = FavoriteLogPolicy::None;
        outcome.toasts = vec![match status {
            CopyStatus::AcceptedAndPersisted => {
                ToastPolicy::Success("Copied a password privately.".into())
            }
            CopyStatus::AcceptedButNotPersisted => ToastPolicy::Info(
                "Windows accepted the private copy. Clipboard persistence could not be confirmed."
                    .into(),
            ),
        }];
        return Ok(outcome);
    }

    if let Command::Storage(StorageCommand::HistoryLaunch(index)) = &invocation.command
        && let Some(outcome) = handle_history_snippet_replay(
            host,
            invocation,
            *index,
            captured_history_query,
            crate::plugins::snippets::SNIPPETS_FILE,
        )?
    {
        return Ok(outcome);
    }

    if let Command::VirtualDesktop(command) = &invocation.command {
        let _ = command;
        let history_query = captured_history_query
            .map(str::to_owned)
            .unwrap_or_else(|| host.current_query().to_owned());
        host.spawn_virtual_desktop_command_with_history_query(invocation.clone(), history_query);
        return Ok(CommandOutcome::default());
    }
    if matches!(
        &invocation.command,
        Command::BrowserTab(BrowserTabCommand::Switch(_) | BrowserTabCommand::InvalidSwitch)
    ) {
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

    let result = host.execute_headless_command(&invocation.command, &invocation.original_action);
    finish_execution(invocation, result, || success_outcome(host, invocation))
}

pub(crate) fn handle_external<H: ExternalCommandHost + ?Sized>(
    host: &mut H,
    command: &ExternalCommand,
    invocation: &CommandInvocation,
) -> Result<CommandOutcome, CommandError> {
    let result = host.execute_external_command(command, &invocation.original_action);
    finish_execution(invocation, result, || {
        finish_success_outcome(host, invocation, initial_success_outcome(invocation), false)
    })
}

pub(crate) fn handle_checked_tempfile_mutation(
    preferences: super::super::LauncherRunPreferences,
    invocation: &CommandInvocation,
    result: anyhow::Result<()>,
) -> Result<CommandOutcome, CommandError> {
    finish_execution(invocation, result, || {
        let mut outcome = initial_success_outcome(invocation);
        outcome.search = true;
        outcome.invalidate_results = true;
        outcome.focus = true;
        finish_success_with_preferences(preferences, invocation, outcome, false)
    })
}

pub(crate) fn handle_checked_headless_effect(
    preferences: super::super::LauncherRunPreferences,
    invocation: &CommandInvocation,
    result: anyhow::Result<()>,
) -> Result<CommandOutcome, CommandError> {
    finish_execution(invocation, result, || {
        finish_success_with_preferences(preferences, invocation, initial_success_outcome(invocation), false)
    })
}

fn finish_execution(
    invocation: &CommandInvocation,
    result: anyhow::Result<()>,
    success: impl FnOnce() -> CommandOutcome,
) -> Result<CommandOutcome, CommandError> {
    if let Err(error) = result {
        let mut error =
            CommandError::new("launcher", format!("Failed: {error}")).with_gui_failure_policy();
        if is_external_favorite(invocation) {
            error = error.with_favorite(invocation.original_action.label.clone());
        }
        return Err(error);
    }

    Ok(success())
}

fn handle_history_snippet_replay<H: HeadlessCommandHost + ?Sized>(
    host: &mut H,
    history_invocation: &CommandInvocation,
    index: usize,
    captured_history_query: Option<&str>,
    snippets_path: &str,
) -> Result<Option<CommandOutcome>, CommandError> {
    let Some(entry) = host.history_entry(index) else {
        return Ok(None);
    };
    if entry.action.args.is_some() {
        return Ok(None);
    }
    let Ok(command) = crate::commands::parse_action(&entry.action) else {
        return Ok(None);
    };

    let (command, original_action) = match command {
        Command::Storage(StorageCommand::SnippetRun(alias)) => (
            Command::Storage(StorageCommand::SnippetRun(alias.clone())),
            crate::actions::Action {
                label: alias.clone(),
                desc: "Snippet".into(),
                action: crate::plugins::snippets::snippet_run_action(&alias),
                args: None,
            },
        ),
        Command::Storage(StorageCommand::InvalidSnippetRun) => (
            Command::Storage(StorageCommand::InvalidSnippetRun),
            crate::actions::Action {
                label: "Snippet".into(),
                desc: "Snippet".into(),
                action: "snippet:run:".into(),
                args: None,
            },
        ),
        _ => return Ok(None),
    };
    let invocation = CommandInvocation {
        command,
        original_action,
        query_override: None,
        source: history_invocation.source,
    };
    let history_query = captured_history_query
        .map(str::to_owned)
        .unwrap_or_else(|| host.current_query().to_owned());
    super::snippet_run::handle_snippet_run_from_path(
        host,
        &invocation,
        Some(&history_query),
        snippets_path,
    )
    .map(Some)
}

pub(crate) fn checked_storage_authoring_outcome(
    invocation: &CommandInvocation,
    preserve_command: bool,
    preferences: super::super::LauncherRunPreferences,
    result: anyhow::Result<()>,
) -> Result<CommandOutcome, CommandError> {
    let Some(outcome) = checked_authoring_success_outcome(invocation, preserve_command, preferences) else {
        return Err(CommandError::new("launcher", "Invalid checked storage authoring command"));
    };
    finish_execution(invocation, result, || outcome)
}

pub(crate) fn checked_timer_start_outcome(
    invocation: &CommandInvocation,
    raw_query: &str,
    preserve_command: bool,
    preferences: super::super::LauncherRunPreferences,
    result: anyhow::Result<()>,
) -> Result<CommandOutcome, CommandError> {
    let Some(outcome) = timer_start_success(invocation, raw_query, preserve_command, preferences) else {
        return Err(CommandError::new("launcher", "Invalid checked timer start command"));
    };
    finish_execution(invocation, result, || outcome)
}

fn timer_start_success(
    invocation: &CommandInvocation,
    raw_query: &str,
    preserve_command: bool,
    preferences: super::super::LauncherRunPreferences,
) -> Option<CommandOutcome> {
    if !matches!(&invocation.command, Command::Timer(TimerCommand::Start { .. } | TimerCommand::AlarmSet { .. })) {
        return None;
    }
    let mut outcome = initial_success_outcome(invocation);
    let changed = matches!(&invocation.command, Command::Timer(TimerCommand::Start { .. }))
        && raw_query.starts_with("timer add");
    if changed {
        outcome.query = QueryPolicy::Set(if preserve_command { "timer add ".into() } else { String::new() });
        outcome.focus = true;
    }
    Some(finish_success_with_preferences(preferences, invocation, outcome, changed))
}

fn checked_authoring_success_outcome(invocation: &CommandInvocation, preserve_command: bool,
    preferences: super::super::LauncherRunPreferences) -> Option<CommandOutcome> {
    let mut outcome = initial_success_outcome(invocation);
    let changed = match &invocation.command {
        Command::Storage(StorageCommand::FolderAdd(_) | StorageCommand::BookmarkAdd(_)) => {
            let prefix = if matches!(&invocation.command, Command::Storage(StorageCommand::FolderAdd(_))) { "f add " } else { "bm add " };
            outcome.query = QueryPolicy::Set(if preserve_command { prefix.into() } else { String::new() });
            outcome.search = true; outcome.invalidate_results = true; outcome.focus = true; true
        }
        Command::Storage(StorageCommand::FolderRemove(_) | StorageCommand::BookmarkRemove(_)) => {
            outcome.search = true; outcome.invalidate_results = true; outcome.focus = true; false
        }
        _ => return None,
    };
    Some(finish_success_with_preferences(preferences, invocation, outcome, changed))
}

pub(crate) fn success_outcome<H: HeadlessCommandHost + ?Sized>(
    host: &H,
    invocation: &CommandInvocation,
) -> CommandOutcome {
    if let Some(outcome) = timer_start_success(invocation, host.current_query(), host.preserve_command(), super::super::LauncherRunPreferences {
        clear_query_after_run: host.clear_query_after_run(), hide_after_run: host.hide_after_run(), refocus: host.launcher_should_refocus(),
    }) { return outcome; }
    if let Some(outcome) = checked_authoring_success_outcome(invocation, host.preserve_command(), super::super::LauncherRunPreferences {
        clear_query_after_run: host.clear_query_after_run(), hide_after_run: host.hide_after_run(), refocus: host.launcher_should_refocus(),
    }) { return outcome; }
    let mut outcome = initial_success_outcome(invocation);
    let mut command_changed_query = false;

    match &invocation.command {
        Command::Storage(
            StorageCommand::SnippetRemove(_)
            | StorageCommand::FavoriteAdd { .. }
            | StorageCommand::FavoriteRemove(_)
            | StorageCommand::TempfileRemove(_)
            | StorageCommand::TempfileAlias { .. }
            | StorageCommand::InvalidTempfileAlias,
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
        Command::Timer(TimerCommand::Cancel(_) | TimerCommand::InvalidCancel)
            if host.current_query().starts_with("timer rm") =>
        {
            outcome.search = true;
            outcome.invalidate_results = true;
            outcome.focus = true;
        }
        Command::Timer(TimerCommand::Pause(_) | TimerCommand::InvalidPause)
            if host.current_query().starts_with("timer pause") =>
        {
            outcome.search = true;
            outcome.invalidate_results = true;
            outcome.focus = true;
        }
        Command::Timer(TimerCommand::Resume(_) | TimerCommand::InvalidResume)
            if host.current_query().starts_with("timer resume") =>
        {
            outcome.search = true;
            outcome.invalidate_results = true;
            outcome.focus = true;
        }
        _ => {}
    }

    finish_success_outcome(host, invocation, outcome, command_changed_query)
}

fn initial_success_outcome(invocation: &CommandInvocation) -> CommandOutcome {
    CommandOutcome {
        history: HistoryPolicy::Record,
        toasts: success_toasts(invocation),
        favorite_log: favorite_log(invocation),
        ..CommandOutcome::default()
    }
}

fn finish_success_outcome<H: ExternalCommandHost + ?Sized>(
    host: &H,
    invocation: &CommandInvocation,
    outcome: CommandOutcome,
    command_changed_query: bool,
) -> CommandOutcome {
    finish_success_with_preferences(
        super::super::LauncherRunPreferences {
            clear_query_after_run: host.external_clear_query_after_run(),
            hide_after_run: host.external_hide_after_run(),
            refocus: host.external_launcher_should_refocus(),
        },
        invocation, outcome, command_changed_query,
    )
}

fn finish_success_with_preferences(
    preferences: super::super::LauncherRunPreferences,
    invocation: &CommandInvocation,
    mut outcome: CommandOutcome,
    command_changed_query: bool,
) -> CommandOutcome {
    if preferences.clear_query_after_run && !command_changed_query {
        outcome.query = QueryPolicy::Set(String::new());
        outcome.search = true;
        outcome.invalidate_results = true;
        outcome.focus = true;
    }

    if preferences.hide_after_run && !is_hide_exempt(&invocation.command) {
        outcome.visibility = VisibilityPolicy::Hide;
    }

    if !outcome.focus
        && outcome.visibility != VisibilityPolicy::Hide
        && preferences.refocus
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
        Command::Clipboard(_) | Command::Storage(StorageCommand::SnippetRun(_)) => {
            vec![ToastPolicy::Copied(
                invocation.original_action.label.clone(),
            )]
        }
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
            &invocation.command,
            Command::Storage(
                StorageCommand::FavoriteAdd { .. } | StorageCommand::FavoriteRemove(_)
            ) | Command::External(ExternalCommand {
                namespace: Some(ExternalNamespace::Favorite),
                ..
            })
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

    #[test]
    fn typed_external_entry_uses_shared_policy_and_preserves_launch_arguments() {
        #[derive(Default)]
        struct ExternalHost {
            seen: Option<(ExternalCommand, Action)>,
            fail: bool,
        }
        impl ExternalCommandHost for ExternalHost {
            fn execute_external_command(
                &mut self,
                command: &ExternalCommand,
                action: &Action,
            ) -> anyhow::Result<()> {
                self.seen = Some((command.clone(), action.clone()));
                if self.fail {
                    anyhow::bail!("injected failure")
                }
                Ok(())
            }
            fn external_clear_query_after_run(&self) -> bool {
                true
            }
            fn external_hide_after_run(&self) -> bool {
                true
            }
            fn external_launcher_should_refocus(&self) -> bool {
                true
            }
        }
        let command = ExternalCommand {
            target: "tool.exe".into(),
            args: Some("--example".into()),
            namespace: None,
        };
        let mut invocation = invocation(Command::External(command.clone()), "original wire value");
        invocation.original_action.desc = "Fav".into();
        invocation.original_action.args = Some("original arguments".into());
        invocation.source = ActivationSource::Click;
        let mut narrow = ExternalHost::default();
        let outcome = crate::commands::CommandBus
            .dispatch_external(
                &command,
                &invocation.original_action,
                invocation.source,
                Some("query"),
                &mut narrow,
            )
            .unwrap();
        let mut legacy = host();
        legacy.clear = true;
        legacy.hide = true;
        assert_eq!(
            outcome,
            handle_headless_gui(&mut legacy, &invocation).unwrap()
        );
        assert_eq!(
            narrow.seen,
            Some((command.clone(), invocation.original_action.clone()))
        );

        narrow.fail = true;
        legacy.fail = true;
        let error = crate::commands::CommandBus
            .dispatch_external(
                &command,
                &invocation.original_action,
                invocation.source,
                None,
                &mut narrow,
            )
            .unwrap_err();
        let legacy_error = handle_headless_gui(&mut legacy, &invocation).unwrap_err();
        assert_eq!(format!("{error:?}"), format!("{legacy_error:?}"));
    }

    struct FakeHost {
        query: String,
        clear: bool,
        hide: bool,
        preserve: bool,
        executed: usize,
        spawned: usize,
        fail: bool,
        history: Option<crate::history::HistoryEntry>,
        copied: Vec<String>,
        prompt: Option<crate::commands::SnippetPromptIntent>,
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
        fn history_entry(&self, index: usize) -> Option<crate::history::HistoryEntry> {
            if index == 0 {
                self.history.clone()
            } else {
                None
            }
        }
        fn copy_snippet_text(&mut self, text: &str) -> anyhow::Result<()> {
            self.copied.push(text.to_owned());
            Ok(())
        }
        fn request_snippet_prompt(
            &mut self,
            intent: crate::commands::SnippetPromptIntent,
        ) -> Result<(), String> {
            self.prompt = Some(intent);
            Ok(())
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
            history: None,
            copied: Vec::new(),
            prompt: None,
        }
    }

    fn history_entry(action: &str, label: &str) -> crate::history::HistoryEntry {
        crate::history::HistoryEntry {
            query: "old query".into(),
            query_lc: "old query".into(),
            action: Action {
                label: label.into(),
                desc: "Snippet".into(),
                action: action.into(),
                args: None,
            },
            source: None,
            timestamp: 1,
        }
    }

    fn history_launch(source: ActivationSource) -> CommandInvocation {
        let mut invocation = invocation(
            Command::Storage(StorageCommand::HistoryLaunch(0)),
            "history:0",
        );
        invocation.source = source;
        invocation
    }

    fn write_test_snippets(
        directory: &tempfile::TempDir,
        entries: &[crate::plugins::snippets::SnippetEntry],
    ) -> String {
        let path = directory.path().join("snippets.json");
        let path = path.to_str().unwrap().to_owned();
        crate::plugins::snippets::save_snippets(&path, entries).unwrap();
        path
    }

    #[test]
    fn history_snippet_replay_uses_current_run_mode_and_captured_activation_context() {
        use crate::plugins::snippets::{SnippetEntry, SnippetFieldDefinition, snippet_run_action};

        let directory = tempfile::tempdir().unwrap();
        let plain_text = "literal {{ braces }}\r\nremain exact";
        let plain = SnippetEntry {
            alias: "plain".into(),
            text: plain_text.into(),
            hide_contents: false,
            prompt_for_fields: false,
            fields: Vec::new(),
            tags: Vec::new(),
        };
        let prompted = SnippetEntry {
            alias: "ticket:current".into(),
            text: "Hello {{name}}".into(),
            hide_contents: true,
            prompt_for_fields: true,
            fields: vec![SnippetFieldDefinition::new("name")],
            tags: Vec::new(),
        };
        let path = write_test_snippets(&directory, &[plain, prompted]);
        let invocation = history_launch(ActivationSource::Dashboard);

        let mut plain_host = host();
        plain_host.history = Some(history_entry(&snippet_run_action("plain"), "old plain"));
        let plain_outcome = handle_history_snippet_replay(
            &mut plain_host,
            &invocation,
            0,
            Some("captured plain query"),
            &path,
        )
        .unwrap()
        .expect("canonical snippet history must use the typed handler");
        assert_eq!(plain_host.copied, [plain_text]);
        assert_eq!(plain_outcome.history, HistoryPolicy::Record);
        assert_eq!(plain_outcome.toasts, [ToastPolicy::Copied("plain".into())]);
        assert_eq!(plain_host.executed, 0);

        let mut prompted_host = host();
        prompted_host.query = "ambient query must not replace captured history".into();
        prompted_host.history = Some(history_entry(
            &snippet_run_action("ticket:current"),
            "stale display label",
        ));
        let prompted_outcome = handle_history_snippet_replay(
            &mut prompted_host,
            &invocation,
            0,
            Some("captured prompted query"),
            &path,
        )
        .unwrap()
        .expect("prompted history must hand off to the existing form owner");
        assert!(prompted_host.copied.is_empty());
        assert_eq!(prompted_outcome.history, HistoryPolicy::Skip);
        assert!(prompted_outcome.toasts.is_empty());
        assert_eq!(prompted_host.executed, 0);
        let pending = prompted_host.prompt.expect("prompt intent");
        assert_eq!(pending.alias, "ticket:current");
        assert_eq!(pending.source, ActivationSource::Dashboard);
        assert_eq!(pending.history_query, "captured prompted query");
        assert_eq!(pending.safe_action.label, "ticket:current");
        assert!(!pending.safe_action.action.contains("Hello"));
    }

    #[test]
    fn invalid_or_opaque_history_actions_never_infer_snippet_from_description() {
        use crate::plugins::snippets::{SnippetEntry, snippet_run_action};

        let directory = tempfile::tempdir().unwrap();
        let path = write_test_snippets(
            &directory,
            &[SnippetEntry {
                alias: "available".into(),
                text: "literal".into(),
                hide_contents: false,
                prompt_for_fields: false,
                fields: Vec::new(),
                tags: Vec::new(),
            }],
        );
        let invocation = history_launch(ActivationSource::Enter);
        let mut missing_host = host();
        missing_host.history = Some(history_entry(&snippet_run_action("gone"), "gone"));
        let error = handle_history_snippet_replay(&mut missing_host, &invocation, 0, None, &path)
            .unwrap_err();
        assert!(error.message.contains("missing or has an ambiguous alias"));
        assert!(missing_host.copied.is_empty());
        assert!(missing_host.prompt.is_none());

        let mut malformed_host = host();
        malformed_host.history = Some(history_entry("snippet:run:%GG", "malformed"));
        let malformed =
            handle_history_snippet_replay(&mut malformed_host, &invocation, 0, None, &path)
                .unwrap_err();
        assert!(malformed.message.contains("invalid snippet run action"));
        assert!(malformed_host.copied.is_empty());
        assert!(malformed_host.prompt.is_none());

        let mut opaque_host = host();
        opaque_host.history = Some(history_entry(
            "clipboard:literal {{ equal-looking text }}",
            "old opaque snippet",
        ));
        assert!(
            handle_history_snippet_replay(&mut opaque_host, &invocation, 0, None, &path,)
                .unwrap()
                .is_none(),
            "opaque clipboard history must remain on generic replay"
        );
        let outer = handle_headless_gui(&mut opaque_host, &invocation).unwrap();
        assert_eq!(opaque_host.executed, 1);
        assert!(opaque_host.copied.is_empty());
        assert_eq!(outer.history, HistoryPolicy::Record);
        assert!(opaque_host.prompt.is_none());
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
                    namespace: None,
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
                    namespace: None,
                }),
                "tool",
            ),
        )
        .unwrap_err();
        assert_eq!(error.message, "Failed: injected failure");
        assert!(error.toast && error.refocus);
    }

    #[test]
    fn malformed_timer_and_storage_apply_post_policy_only_after_success() {
        let mut timer_host = host();
        timer_host.query = "timer rm broken".into();
        timer_host.hide = true;
        let outcome = handle_headless_gui(
            &mut timer_host,
            &invocation(
                Command::Timer(TimerCommand::InvalidCancel),
                "timer:cancel:bad",
            ),
        )
        .unwrap();
        assert!(outcome.search && outcome.invalidate_results && outcome.focus);
        assert_eq!(outcome.visibility, VisibilityPolicy::Hide);

        let mut storage_host = host();
        let outcome = handle_headless_gui(
            &mut storage_host,
            &invocation(
                Command::Storage(StorageCommand::InvalidTempfileAlias),
                "tempfile:alias:bad",
            ),
        )
        .unwrap();
        assert!(outcome.search && outcome.invalidate_results && outcome.focus);

        timer_host.fail = true;
        let error = handle_headless_gui(
            &mut timer_host,
            &invocation(
                Command::Timer(TimerCommand::InvalidPause),
                "timer:pause:bad",
            ),
        )
        .unwrap_err();
        assert_eq!(error.message, "Failed: injected failure");
    }

    #[test]
    fn every_raw_favorite_protocol_is_excluded_from_favorite_logging() {
        let mut invocation = invocation(
            Command::External(super::super::super::ExternalCommand {
                target: "fav:future:payload".into(),
                args: None,
                namespace: Some(ExternalNamespace::Favorite),
            }),
            "fav:future:payload",
        );
        invocation.original_action.desc = "Fav".into();
        assert!(!is_external_favorite(&invocation));
        assert_eq!(favorite_log(&invocation), FavoriteLogPolicy::None);
    }
    #[test]
    fn ordinary_external_favorite_is_logged_from_typed_external_semantics() {
        let mut invocation = invocation(
            Command::External(ExternalCommand {
                target: "tool".into(),
                args: None,
                namespace: None,
            }),
            "tool",
        );
        invocation.original_action.desc = "Fav".into();
        assert!(is_external_favorite(&invocation));
        assert_eq!(
            favorite_log(&invocation),
            FavoriteLogPolicy::Ran {
                label: "Example".into(),
                command: "tool".into(),
            }
        );
    }

    #[test]
    fn browser_switch_is_async_without_generic_clear_or_hide() {
        for (command, action) in [
            (
                Command::BrowserTab(BrowserTabCommand::Switch(vec![1])),
                "tab:switch:1",
            ),
            (
                Command::BrowserTab(BrowserTabCommand::InvalidSwitch),
                "tab:switch:bad",
            ),
        ] {
            let mut host = host();
            host.clear = true;
            host.hide = true;
            let result = handle_headless_gui(&mut host, &invocation(command, action)).unwrap();
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

    #[test]
    fn virtual_desktop_commands_are_dispatched_off_the_gui_thread() {
        let mut host = host();
        let invocation = invocation(
            Command::VirtualDesktop(super::super::super::VirtualDesktopCommand::Launch(
                super::super::super::VirtualDesktopLaunchPayload {
                    target: "1".into(),
                    application: "app.exe".into(),
                    args: None,
                    follow: false,
                    timeout_ms: 10_000,
                },
            )),
            "vd:launch",
        );
        let result = handle_headless_gui(&mut host, &invocation).unwrap();
        assert_eq!(host.spawned, 1);
        assert_eq!(host.executed, 0);
        assert_eq!(result.history, HistoryPolicy::Skip);
        assert!(result.toasts.is_empty());
        let completed = success_outcome(&host, &invocation);
        assert_eq!(completed.history, HistoryPolicy::Record);
        assert_eq!(completed.toasts, [ToastPolicy::Launched("Example".into())]);
    }
}
