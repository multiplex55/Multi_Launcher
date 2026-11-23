use crate::commands::{
    ActivationSource, CommandOutcome, LauncherCommand, LauncherCommandHost, QueryCommand,
    QueryPolicy, VisibilityPolicy,
};

pub(crate) fn handle_launcher<H: LauncherCommandHost + ?Sized>(
    host: &H,
    command: &LauncherCommand,
) -> CommandOutcome {
    match command {
        LauncherCommand::Toggle => CommandOutcome {
            visibility: VisibilityPolicy::Toggle,
            restore: !host.launcher_is_visible(),
            ..CommandOutcome::default()
        },
        LauncherCommand::Show { query } => CommandOutcome {
            query: query.clone().map_or(QueryPolicy::Keep, QueryPolicy::Set),
            search: query.is_some(),
            visibility: VisibilityPolicy::Show,
            restore: true,
            focus: query.is_some(),
            move_cursor_end: query.is_some(),
            ..CommandOutcome::default()
        },
        LauncherCommand::Hide => CommandOutcome {
            visibility: VisibilityPolicy::Hide,
            ..CommandOutcome::default()
        },
        LauncherCommand::Focus | LauncherCommand::Restore => CommandOutcome {
            visibility: VisibilityPolicy::Show,
            restore: true,
            ..CommandOutcome::default()
        },
    }
}

pub fn handle_query(command: &QueryCommand, source: ActivationSource) -> CommandOutcome {
    match command {
        QueryCommand::Set { query, argument } => {
            let query = argument
                .as_ref()
                .map(|argument| format!("{} {argument}", query.trim_end()))
                .unwrap_or_else(|| query.clone());
            CommandOutcome::query(query)
        }
        QueryCommand::ExecuteFirst { query } => CommandOutcome {
            activate_first_result: Some(source),
            ..CommandOutcome::query(query.clone())
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Host(bool);

    impl LauncherCommandHost for Host {
        fn launcher_is_visible(&self) -> bool {
            self.0
        }
    }

    #[test]
    fn launcher_toggle_only_restores_when_becoming_visible() {
        assert!(handle_launcher(&Host(false), &LauncherCommand::Toggle).restore);
        assert!(!handle_launcher(&Host(true), &LauncherCommand::Toggle).restore);
    }

    #[test]
    fn query_execute_first_preserves_source_in_outcome() {
        let outcome = handle_query(
            &QueryCommand::ExecuteFirst {
                query: "notes".into(),
            },
            ActivationSource::Macro,
        );
        assert_eq!(outcome.query, QueryPolicy::Set("notes".into()));
        assert_eq!(outcome.activate_first_result, Some(ActivationSource::Macro));
        assert!(!outcome.invalidate_results);
    }

    #[test]
    fn query_set_outcome_preserves_literal_and_json_argument_text() {
        let literal = handle_query(
            &QueryCommand::Set {
                query: "  literal  東京  ".into(),
                argument: None,
            },
            ActivationSource::Click,
        );
        assert_eq!(literal, CommandOutcome::query("  literal  東京  ".into()));

        let argument = handle_query(
            &QueryCommand::Set {
                query: "g05 outcome  ".into(),
                argument: Some("  second β  ".into()),
            },
            ActivationSource::Enter,
        );
        assert_eq!(
            argument,
            CommandOutcome::query("g05 outcome   second β  ".into())
        );
    }

    #[test]
    fn query_set_empty_text_and_argument_keep_the_handler_policy() {
        let empty = handle_query(
            &QueryCommand::Set {
                query: String::new(),
                argument: None,
            },
            ActivationSource::Click,
        );
        assert_eq!(empty, CommandOutcome::query(String::new()));

        let empty_argument = handle_query(
            &QueryCommand::Set {
                query: String::new(),
                argument: Some(String::new()),
            },
            ActivationSource::Click,
        );
        assert_eq!(empty_argument, CommandOutcome::query(" ".into()));
    }
}
