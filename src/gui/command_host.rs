use std::sync::atomic::Ordering;

use crate::commands::{
    Command, CommandError, CommandInvocation, CommandOutcome, HistoryPolicy, LauncherCommandHost,
    LegacyCommandHost, QueryPolicy, VisibilityPolicy,
};

use super::LauncherApp;

impl LauncherCommandHost for LauncherApp {
    fn launcher_is_visible(&self) -> bool {
        self.visible_flag.load(Ordering::SeqCst)
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
            Err(error) => self.report_error_message(error.domain, error.message),
        }
    }

    fn apply_command_outcome(&mut self, outcome: CommandOutcome, invocation: &CommandInvocation) {
        let history_query = self.query.clone();

        if let QueryPolicy::Set(query) = outcome.query {
            self.last_timer_query =
                query.starts_with("timer list") || query.starts_with("alarm list");
            self.query = query;
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
        if outcome.history == HistoryPolicy::Record {
            self.record_history_usage(
                &invocation.original_action,
                &history_query,
                invocation.source,
            );
        }
    }
}
