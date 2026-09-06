use super::headless_gui::{favorite_log, is_external_favorite};
use crate::commands::{
    CommandError, CommandInvocation, CommandOutcome, HeadlessCommandHost, HistoryPolicy,
    LinkCommand, NoteCommand, NoteCommandHost, QueryPolicy, ToastPolicy, VisibilityPolicy,
};

pub(crate) fn handle_note<H>(
    host: &mut H,
    command: &NoteCommand,
    invocation: &CommandInvocation,
) -> Result<CommandOutcome, CommandError>
where
    H: NoteCommandHost + HeadlessCommandHost + ?Sized,
{
    match command {
        NoteCommand::Dialog => host.open_notes_dialog(),
        NoteCommand::GraphDialog { args } => host.open_note_graph_dialog(args.as_deref()),
        NoteCommand::UnusedAssets => host.open_unused_note_assets_dialog(),
        NoteCommand::Open { slug } => host.open_note_panel(slug, None),
        NoteCommand::New { slug, template } => host.open_note_panel(slug, template.as_deref()),
        NoteCommand::MalformedNew { raw, error, .. } => {
            return Err(malformed_note_error(raw, error));
        }
        NoteCommand::TemplatesDisabled => {
            return Err(
                CommandError::new("launcher", "Note templates are disabled in settings")
                    .with_refocus_policy(),
            );
        }
        NoteCommand::Tags => {
            host.open_note_tags();
            return Ok(CommandOutcome {
                focus: true,
                ..CommandOutcome::default()
            });
        }
        NoteCommand::OpenLink { link } => host.open_note_link(link),
        NoteCommand::WrapLinks { slug } => host.wrap_note_plain_links(slug),
        NoteCommand::Remove { slug } => host.delete_note(slug),
        NoteCommand::Reload => return reload(host, invocation),
    }
    Ok(CommandOutcome {
        focus: host.launcher_should_refocus(),
        ..CommandOutcome::default()
    })
}

pub(crate) fn handle_link<H>(
    host: &mut H,
    command: &LinkCommand,
) -> Result<CommandOutcome, CommandError>
where
    H: NoteCommandHost + HeadlessCommandHost + ?Sized,
{
    let LinkCommand::Open { id } = command;
    let parsed = crate::linking::parse_link_id(id).map_err(|_| {
        CommandError::new("launcher", format!("Invalid link id: {id}")).with_refocus_policy()
    })?;
    match parsed.target_type {
        crate::linking::LinkTarget::Note => {
            host.open_note_panel(&parsed.target_id, None);
            Ok(CommandOutcome {
                focus: host.launcher_should_refocus(),
                ..CommandOutcome::default()
            })
        }
        crate::linking::LinkTarget::Todo => Ok(CommandOutcome {
            query: QueryPolicy::Set(format!("todo links id:{}", parsed.target_id)),
            search: true,
            focus: host.launcher_should_refocus(),
            ..CommandOutcome::default()
        }),
        _ => Err(
            CommandError::new("launcher", format!("Unsupported link target: {id}"))
                .with_refocus_policy(),
        ),
    }
}

fn malformed_note_error(raw: &str, error: &str) -> CommandError {
    let message = if raw.starts_with(crate::plugins::note::NOTE_NEW_JSON_PREFIX) {
        let detail = error
            .strip_prefix("malformed new-note payload: ")
            .unwrap_or(error);
        format!("Malformed note action payload: {detail}")
    } else if error.starts_with("malformed action:") {
        format!("Malformed note action: {raw}")
    } else {
        "Malformed note action".to_string()
    };
    CommandError::new("launcher", message)
}

fn reload<H>(host: &mut H, invocation: &CommandInvocation) -> Result<CommandOutcome, CommandError>
where
    H: HeadlessCommandHost + ?Sized,
{
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
        search: true,
        invalidate_results: true,
        focus: true,
        history: HistoryPolicy::Record,
        favorite_log: favorite_log(invocation),
        toasts: vec![
            ToastPolicy::Launched(invocation.original_action.label.clone()),
            ToastPolicy::Success("Reloaded notes".into()),
        ],
        ..CommandOutcome::default()
    };
    if host.clear_query_after_run() {
        outcome.query = QueryPolicy::Set(String::new());
    }
    if host.hide_after_run() {
        outcome.visibility = VisibilityPolicy::Hide;
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
        opened: Vec<String>,
        executed: usize,
        clear: bool,
        hide: bool,
        refocus: bool,
        fail: bool,
    }

    impl NoteCommandHost for Host {
        fn open_notes_dialog(&mut self) {
            self.opened.push("dialog".into());
        }
        fn open_note_graph_dialog(&mut self, args: Option<&str>) {
            self.opened
                .push(format!("graph:{}", args.unwrap_or_default()));
        }
        fn open_unused_note_assets_dialog(&mut self) {
            self.opened.push("assets".into());
        }
        fn open_note_panel(&mut self, slug: &str, template: Option<&str>) {
            self.opened
                .push(format!("panel:{slug}:{}", template.unwrap_or_default()));
        }
        fn open_note_tags(&mut self) {
            self.opened.push("tags".into());
        }
        fn open_note_link(&mut self, link: &str) {
            self.opened.push(format!("url:{link}"));
        }
        fn wrap_note_plain_links(&mut self, slug: &str) {
            self.opened.push(format!("wrap:{slug}"));
        }
        fn delete_note(&mut self, slug: &str) {
            self.opened.push(format!("delete:{slug}"));
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
            false
        }
        fn current_query(&self) -> &str {
            "note reload"
        }
        fn launcher_should_refocus(&self) -> bool {
            self.refocus
        }
    }

    fn invocation(command: Command, raw: &str) -> CommandInvocation {
        CommandInvocation {
            command,
            original_action: Action {
                label: "Reload notes".into(),
                desc: "Note".into(),
                action: raw.into(),
                args: None,
            },
            query_override: None,
            source: ActivationSource::Enter,
        }
    }

    #[test]
    fn routes_note_panels_and_preserves_template_payload() {
        let mut host = Host::default();
        let command = NoteCommand::New {
            slug: "daily-note".into(),
            template: Some("fancy:name with spaces".into()),
        };
        handle_note(
            &mut host,
            &command,
            &invocation(Command::Note(command.clone()), "note:new:daily-note"),
        )
        .unwrap();
        assert_eq!(host.opened, ["panel:daily-note:fancy:name with spaces"]);
    }

    #[test]
    fn ordinary_note_success_refocuses_only_when_no_panel_remains_open() {
        let command = NoteCommand::WrapLinks {
            slug: "alpha".into(),
        };
        let mut host = Host {
            refocus: true,
            ..Host::default()
        };
        let outcome = handle_note(
            &mut host,
            &command,
            &invocation(Command::Note(command.clone()), "note:meta:wrap-links:alpha"),
        )
        .unwrap();
        assert!(outcome.focus);
    }

    #[test]
    fn malformed_new_retains_early_return_while_templates_disabled_refocuses() {
        let mut host = Host {
            refocus: true,
            ..Host::default()
        };
        let malformed = NoteCommand::MalformedNew {
            raw: "note:new:bad slug".into(),
            args: None,
            error: "malformed note action".into(),
        };
        let error = handle_note(
            &mut host,
            &malformed,
            &invocation(Command::Note(malformed.clone()), "note:new:bad slug"),
        )
        .unwrap_err();
        assert_eq!(error.message, "Malformed note action");
        assert!(!error.refocus && !error.toast);

        let disabled = NoteCommand::TemplatesDisabled;
        let error = handle_note(
            &mut host,
            &disabled,
            &invocation(Command::Note(disabled.clone()), "note:templates_disabled"),
        )
        .unwrap_err();
        assert!(error.refocus && !error.toast);
    }

    #[test]
    fn link_open_routes_note_and_todo_targets_and_refocuses() {
        let mut host = Host::default();
        let note = handle_link(
            &mut host,
            &LinkCommand::Open {
                id: "link://note/alpha".into(),
            },
        )
        .unwrap();
        assert_eq!(host.opened, ["panel:alpha:"]);
        assert!(!note.focus);

        host.refocus = true;
        let todo = handle_link(
            &mut host,
            &LinkCommand::Open {
                id: "link://todo/42".into(),
            },
        )
        .unwrap();
        assert_eq!(todo.query, QueryPolicy::Set("todo links id:42".into()));
        assert!(todo.search && todo.focus);
    }

    #[test]
    fn invalid_and_unsupported_links_refocus_without_extra_toast() {
        let mut host = Host {
            refocus: true,
            ..Host::default()
        };
        for id in ["invalid", "link://bookmark/7"] {
            let error = handle_link(&mut host, &LinkCommand::Open { id: id.into() }).unwrap_err();
            assert!(error.refocus && !error.toast);
        }
    }

    #[test]
    fn reload_keeps_generic_history_clear_hide_and_favorite_policy() {
        let command = Command::Note(NoteCommand::Reload);
        let mut call = invocation(command, "note:reload");
        call.original_action.desc = "Fav".into();
        call.original_action.label = "Favorite reload".into();
        let mut host = Host {
            clear: true,
            hide: true,
            ..Host::default()
        };
        let outcome = handle_note(&mut host, &NoteCommand::Reload, &call).unwrap();
        assert_eq!(host.executed, 1);
        assert_eq!(outcome.history, HistoryPolicy::Record);
        assert_eq!(outcome.query, QueryPolicy::Set(String::new()));
        assert_eq!(outcome.visibility, VisibilityPolicy::Hide);
        assert!(outcome.search && outcome.invalidate_results && outcome.focus);
        assert_eq!(outcome.toasts.len(), 2);
        assert!(
            matches!(outcome.favorite_log, FavoriteLogPolicy::Ran { ref label, .. } if label == "Favorite reload")
        );

        host.fail = true;
        let error = handle_note(&mut host, &NoteCommand::Reload, &call).unwrap_err();
        assert_eq!(error.favorite.as_deref(), Some("Favorite reload"));
        assert!(error.toast && error.refocus);
    }
}
