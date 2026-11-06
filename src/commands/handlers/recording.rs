use crate::commands::{
    CommandError, CommandOutcome, HistoryPolicy, RecordingCommand, RecordingCommandHost,
    VisibilityPolicy,
};

pub(crate) fn handle_recording<H>(
    host: &mut H,
    command: RecordingCommand,
) -> Result<CommandOutcome, CommandError>
where
    H: RecordingCommandHost + ?Sized,
{
    host.execute_recording_command(command)
        .map_err(|error| CommandError::new("recording", error).with_refocus_policy())?;
    Ok(CommandOutcome {
        visibility: VisibilityPolicy::Keep,
        history: HistoryPolicy::Skip,
        ..CommandOutcome::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Host {
        commands: Vec<RecordingCommand>,
    }

    impl RecordingCommandHost for Host {
        fn execute_recording_command(&mut self, command: RecordingCommand) -> Result<(), String> {
            self.commands.push(command);
            Ok(())
        }
    }

    #[test]
    fn open_and_stop_share_the_typed_recording_host_without_starting_on_open() {
        let mut host = Host::default();
        let opened = handle_recording(&mut host, RecordingCommand::Open).unwrap();
        assert_eq!(host.commands, [RecordingCommand::Open]);
        assert_eq!(opened.visibility, VisibilityPolicy::Keep);
        assert_eq!(opened.history, HistoryPolicy::Skip);

        let stopped = handle_recording(&mut host, RecordingCommand::Stop).unwrap();
        assert_eq!(
            host.commands,
            [RecordingCommand::Open, RecordingCommand::Stop]
        );
        assert_eq!(stopped.visibility, VisibilityPolicy::Keep);
        assert_eq!(stopped.history, HistoryPolicy::Skip);
    }
}
