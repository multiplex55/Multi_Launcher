use super::{CommandError, CommandInvocation, CommandOutcome};

pub trait LauncherCommandHost {
    fn launcher_is_visible(&self) -> bool;
}

/// Transitional bridge removed after all command families have typed handlers.
pub trait LegacyCommandHost {
    fn execute_legacy_command(
        &mut self,
        invocation: &CommandInvocation,
    ) -> Result<CommandOutcome, CommandError>;
}

pub trait CommandHost: LauncherCommandHost + LegacyCommandHost {}

impl<T> CommandHost for T where T: LauncherCommandHost + LegacyCommandHost {}
