use super::{Command, CommandError, CommandHost, CommandInvocation, CommandOutcome};
use crate::commands::handlers::{handle_launcher, handle_query};

#[derive(Debug, Default)]
pub struct CommandBus;

impl CommandBus {
    pub fn dispatch(
        &self,
        invocation: &CommandInvocation,
        host: &mut dyn CommandHost,
    ) -> Result<CommandOutcome, CommandError> {
        tracing::debug!(
            domain = invocation.domain(),
            kind = invocation.kind_name(),
            source = invocation.source.label(),
            "dispatching typed command"
        );
        match &invocation.command {
            Command::Launcher(command) => Ok(handle_launcher(host, command)),
            Command::Query(command) => Ok(handle_query(command, invocation.source)),
            // Temporary bridge: milestones 5-14 migrate these enum families.
            _ => host.execute_legacy_command(invocation),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::Action;
    use crate::commands::{
        ActivationSource, LauncherCommand, LauncherCommandHost, LegacyCommandHost, QueryCommand,
        QueryPolicy, VisibilityPolicy,
    };

    #[derive(Default)]
    struct FakeHost {
        visible: bool,
        legacy_calls: usize,
    }

    impl LauncherCommandHost for FakeHost {
        fn launcher_is_visible(&self) -> bool {
            self.visible
        }
    }

    impl LegacyCommandHost for FakeHost {
        fn execute_legacy_command(
            &mut self,
            _: &CommandInvocation,
        ) -> Result<CommandOutcome, CommandError> {
            self.legacy_calls += 1;
            Ok(CommandOutcome::default())
        }
    }

    fn invocation(command: Command) -> CommandInvocation {
        CommandInvocation {
            command,
            original_action: Action {
                label: "x".into(),
                desc: "x".into(),
                action: "x".into(),
                args: None,
            },
            query_override: None,
            source: ActivationSource::Dashboard,
        }
    }

    #[test]
    fn command_bus_routes_launcher_and_query_without_legacy_bridge() {
        let mut host = FakeHost::default();
        let launcher = CommandBus
            .dispatch(
                &invocation(Command::Launcher(LauncherCommand::Show { query: None })),
                &mut host,
            )
            .unwrap();
        assert_eq!(launcher.visibility, VisibilityPolicy::Show);

        let query = CommandBus
            .dispatch(
                &invocation(Command::Query(QueryCommand::Set {
                    query: "abc".into(),
                    argument: None,
                })),
                &mut host,
            )
            .unwrap();
        assert_eq!(query.query, QueryPolicy::Set("abc".into()));
        assert_eq!(host.legacy_calls, 0);
    }
}
