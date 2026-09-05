use super::{Command, CommandError, CommandInvocation, CommandOutcome};
use crate::actions::Action;

pub trait LauncherCommandHost {
    fn launcher_is_visible(&self) -> bool;
}

pub trait HeadlessCommandHost {
    fn execute_headless_command(
        &mut self,
        command: &Command,
        original_action: &Action,
    ) -> anyhow::Result<()>;

    fn spawn_headless_command(&mut self, command: Command, original_action: Action);
    fn clear_query_after_run(&self) -> bool;
    fn hide_after_run(&self) -> bool;
    fn preserve_command(&self) -> bool;
    fn current_query(&self) -> &str;
    fn launcher_should_refocus(&self) -> bool;
}

/// Transitional bridge removed after all command families have typed handlers.
pub trait LegacyCommandHost {
    fn execute_legacy_command(
        &mut self,
        invocation: &CommandInvocation,
    ) -> Result<CommandOutcome, CommandError>;
}

pub trait CommandHost: LauncherCommandHost + HeadlessCommandHost + LegacyCommandHost {}

impl<T> CommandHost for T where T: LauncherCommandHost + HeadlessCommandHost + LegacyCommandHost {}
