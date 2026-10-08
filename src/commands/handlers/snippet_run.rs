use crate::actions::Action;

use super::super::{
    CommandError, CommandInvocation, CommandOutcome, HeadlessCommandHost, HistoryPolicy,
    StorageCommand, ToastPolicy,
};

pub(crate) fn handle_snippet_run<H: HeadlessCommandHost + ?Sized>(
    host: &mut H,
    invocation: &CommandInvocation,
    captured_history_query: Option<&str>,
) -> Result<CommandOutcome, CommandError> {
    handle_snippet_run_from_path(
        host,
        invocation,
        captured_history_query,
        crate::plugins::snippets::SNIPPETS_FILE,
    )
}

fn handle_snippet_run_from_path<H: HeadlessCommandHost + ?Sized>(
    host: &mut H,
    invocation: &CommandInvocation,
    captured_history_query: Option<&str>,
    path: &str,
) -> Result<CommandOutcome, CommandError> {
    let alias = match &invocation.command {
        crate::commands::Command::Storage(StorageCommand::SnippetRun(alias)) => alias,
        crate::commands::Command::Storage(StorageCommand::InvalidSnippetRun) => {
            return Err(snippet_error("invalid snippet run action"));
        }
        _ => unreachable!("snippet run handler received a different command"),
    };
    let entry = crate::plugins::snippets::resolve_snippet_from(path, alias)
        .map_err(|error| snippet_error(error.to_string()))?;
    handle_snippet_entry(host, invocation, captured_history_query, entry)
}

fn handle_snippet_entry<H: HeadlessCommandHost + ?Sized>(
    host: &mut H,
    invocation: &CommandInvocation,
    captured_history_query: Option<&str>,
    entry: crate::plugins::snippets::SnippetEntry,
) -> Result<CommandOutcome, CommandError> {
    let prepared = match crate::plugins::snippets::prepare_snippet_run(&entry)
        .map_err(|error| snippet_error(error.to_string()))?
    {
        crate::plugins::snippets::SnippetRunMode::Plain => {
            host.copy_snippet_text(&entry.text)
                .map_err(|error| snippet_error(format!("failed to copy snippet: {error}")))?;
            return Ok(super::success_outcome(host, invocation));
        }
        crate::plugins::snippets::SnippetRunMode::Prompted(prepared) => prepared,
    };
    let safe_action = Action {
        label: entry.alias.clone(),
        desc: "Snippet".into(),
        action: crate::plugins::snippets::snippet_run_action(&entry.alias),
        args: None,
    };
    let intent = super::super::SnippetPromptIntent {
        alias: entry.alias.clone(),
        entry_snapshot: entry,
        prepared,
        safe_action,
        source: invocation.source,
        history_query: captured_history_query
            .map(str::to_owned)
            .unwrap_or_else(|| host.current_query().to_owned()),
        root_policy: host.snippet_root_policy(),
    };
    host.request_snippet_prompt(intent)
        .map_err(|_| snippet_error("prompted snippet could not be queued"))?;

    Ok(CommandOutcome {
        toasts: vec![ToastPolicy::Info(
            "Snippet needs input before it can be copied".into(),
        )],
        history: HistoryPolicy::Skip,
        ..CommandOutcome::default()
    })
}

fn snippet_error(message: impl Into<String>) -> CommandError {
    CommandError::new("snippet", message).with_gui_failure_policy()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{
        ActivationSource, Command, CommandInvocation, HistoryPolicy, StorageCommand, ToastPolicy,
    };
    use crate::plugins::snippets::{
        SnippetEntry, SnippetFieldDefinition, SnippetInputKind, snippet_run_action,
    };
    use crate::universal_actions::RootLauncherPolicy;

    struct FakeHost {
        copied: Vec<String>,
        pending: Option<crate::commands::SnippetPromptIntent>,
        query: String,
        root_policy: RootLauncherPolicy,
        should_fail_copy: bool,
    }

    impl Default for FakeHost {
        fn default() -> Self {
            Self {
                copied: Vec::new(),
                pending: None,
                query: String::new(),
                root_policy: RootLauncherPolicy::Legacy,
                should_fail_copy: false,
            }
        }
    }

    impl crate::commands::HeadlessCommandHost for FakeHost {
        fn execute_headless_command(&mut self, _: &Command, _: &Action) -> anyhow::Result<()> {
            anyhow::bail!("unexpected generic command execution")
        }
        fn spawn_headless_command(&mut self, _: Command, _: Action) {}
        fn clear_query_after_run(&self) -> bool {
            false
        }
        fn hide_after_run(&self) -> bool {
            true
        }
        fn preserve_command(&self) -> bool {
            false
        }
        fn current_query(&self) -> &str {
            &self.query
        }
        fn launcher_should_refocus(&self) -> bool {
            true
        }
        fn copy_snippet_text(&mut self, text: &str) -> anyhow::Result<()> {
            if self.should_fail_copy {
                anyhow::bail!("injected clipboard failure")
            }
            self.copied.push(text.to_owned());
            Ok(())
        }
        fn request_snippet_prompt(
            &mut self,
            intent: crate::commands::SnippetPromptIntent,
        ) -> Result<(), String> {
            self.pending = Some(intent);
            Ok(())
        }
        fn snippet_root_policy(&self) -> RootLauncherPolicy {
            self.root_policy
        }
    }

    fn invocation(alias: &str) -> CommandInvocation {
        CommandInvocation {
            command: Command::Storage(StorageCommand::SnippetRun(alias.into())),
            original_action: Action {
                label: alias.into(),
                desc: "Snippet".into(),
                action: snippet_run_action(alias),
                args: None,
            },
            query_override: None,
            source: ActivationSource::Click,
        }
    }

    fn plain_entry(alias: &str, text: &str) -> SnippetEntry {
        SnippetEntry {
            alias: alias.into(),
            text: text.into(),
            hide_contents: false,
            prompt_for_fields: false,
            fields: Vec::new(),
        }
    }

    fn prompted_entry() -> SnippetEntry {
        SnippetEntry {
            alias: "ticket".into(),
            text: "Hello {{name}}\r\n{{ticket}}".into(),
            hide_contents: true,
            prompt_for_fields: true,
            fields: vec![
                SnippetFieldDefinition {
                    name: "name".into(),
                    label: "Preferred name".into(),
                    default_value: "Ada".into(),
                    required: false,
                    input_kind: SnippetInputKind::Multiline,
                },
                SnippetFieldDefinition {
                    name: "ticket".into(),
                    label: "Ticket".into(),
                    default_value: "INC-".into(),
                    required: true,
                    input_kind: SnippetInputKind::SingleLine,
                },
            ],
        }
    }

    #[test]
    fn plain_run_copies_exact_current_literal_and_uses_normal_success_policy() {
        let body = "first {{not-parsed}}\r\n秘密 🧪\nlast";
        let mut host = FakeHost::default();
        let invocation = invocation("plain");
        let outcome = handle_snippet_entry(
            &mut host,
            &invocation,
            Some("captured query"),
            plain_entry("plain", body),
        )
        .unwrap();

        assert_eq!(host.copied, vec![body]);
        assert_eq!(outcome.history, HistoryPolicy::Record);
        assert_eq!(outcome.visibility, crate::commands::VisibilityPolicy::Hide);
        assert_eq!(outcome.toasts, vec![ToastPolicy::Copied("plain".into())]);
        assert!(host.pending.is_none());
    }

    #[test]
    fn prompted_run_queues_safe_snapshot_without_copy_or_success_history() {
        let mut host = FakeHost {
            query: "current query".into(),
            root_policy: RootLauncherPolicy::PreserveOrdinaryState,
            ..FakeHost::default()
        };
        let invocation = invocation("ticket");
        let entry = prompted_entry();
        let outcome = handle_snippet_entry(
            &mut host,
            &invocation,
            Some("captured history query"),
            entry.clone(),
        )
        .unwrap();

        assert!(host.copied.is_empty());
        assert_eq!(outcome.history, HistoryPolicy::Skip);
        assert_eq!(outcome.visibility, crate::commands::VisibilityPolicy::Keep);
        assert_eq!(
            outcome.toasts,
            vec![ToastPolicy::Info(
                "Snippet needs input before it can be copied".into()
            )]
        );
        let pending = host.pending.unwrap();
        assert_eq!(pending.alias, "ticket");
        assert_eq!(pending.entry_snapshot, entry);
        assert_eq!(pending.prepared.fields.len(), 2);
        assert_eq!(pending.safe_action.action, snippet_run_action("ticket"));
        assert_eq!(pending.safe_action.args, None);
        assert_eq!(pending.source, ActivationSource::Click);
        assert_eq!(pending.history_query, "captured history query");
        assert_eq!(
            pending.root_policy,
            RootLauncherPolicy::PreserveOrdinaryState
        );
        assert!(!pending.safe_action.action.contains("Hello"));
    }

    #[test]
    fn invalid_prompt_configuration_and_copy_failures_do_not_claim_success() {
        let mut host = FakeHost::default();
        let invocation = invocation("empty");
        let empty_prompt = SnippetEntry {
            prompt_for_fields: true,
            ..plain_entry("empty", "literal only")
        };
        let error = handle_snippet_entry(&mut host, &invocation, None, empty_prompt).unwrap_err();
        assert!(error.message.contains("at least one valid placeholder"));
        assert!(host.pending.is_none());
        assert!(host.copied.is_empty());

        let malformed = SnippetEntry {
            prompt_for_fields: true,
            fields: vec![SnippetFieldDefinition::new("name")],
            ..plain_entry("empty", "{{unfinished")
        };
        let error = handle_snippet_entry(&mut host, &invocation, None, malformed).unwrap_err();
        assert!(error.message.contains("invalid template"));

        let duplicate = SnippetEntry {
            prompt_for_fields: true,
            fields: vec![
                SnippetFieldDefinition::new("name"),
                SnippetFieldDefinition::new("name"),
            ],
            ..plain_entry("empty", "{{name}}")
        };
        let error = handle_snippet_entry(&mut host, &invocation, None, duplicate).unwrap_err();
        assert!(
            error
                .message
                .contains("duplicate configured field definitions")
        );
        assert!(host.pending.is_none());
        assert!(host.copied.is_empty());

        let mut host = FakeHost {
            should_fail_copy: true,
            ..FakeHost::default()
        };
        let error =
            handle_snippet_entry(&mut host, &invocation, None, plain_entry("empty", "body"))
                .unwrap_err();
        assert!(error.message.contains("failed to copy snippet"));
        assert!(error.toast && error.refocus);
    }

    #[test]
    fn run_route_uses_current_unique_exact_alias_and_rejects_missing_or_duplicate_aliases() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let path = path.to_str().unwrap();
        crate::plugins::snippets::save_snippets(path, &[plain_entry("Alias", "first")]).unwrap();

        let mut host = FakeHost::default();
        let run_invocation = invocation("Alias");
        handle_snippet_run_from_path(&mut host, &run_invocation, None, path).unwrap();
        assert_eq!(host.copied, vec!["first"]);

        crate::plugins::snippets::save_snippets(path, &[plain_entry("Alias", "updated")]).unwrap();
        handle_snippet_run_from_path(&mut host, &run_invocation, None, path).unwrap();
        assert_eq!(host.copied, vec!["first", "updated"]);

        let missing = invocation("alias");
        let error = handle_snippet_run_from_path(&mut host, &missing, None, path).unwrap_err();
        assert!(error.message.contains("missing or has an ambiguous alias"));

        crate::plugins::snippets::save_snippets(
            path,
            &[plain_entry("Alias", "one"), plain_entry("Alias", "two")],
        )
        .unwrap();
        let error =
            handle_snippet_run_from_path(&mut host, &run_invocation, None, path).unwrap_err();
        assert!(error.message.contains("missing or has an ambiguous alias"));
        assert_eq!(host.copied, vec!["first", "updated"]);
    }
}
