use crate::commands::{
    CommandError, CommandOutcome, HistoryPolicy, ScreenDrawCommand, ScreenDrawCommandHost,
    VisibilityPolicy,
};

pub(crate) fn handle_screen_draw<H>(
    host: &mut H,
    command: &ScreenDrawCommand,
) -> Result<CommandOutcome, CommandError>
where
    H: ScreenDrawCommandHost + ?Sized,
{
    host.execute_screen_draw_command(*command)
        .map_err(|error| CommandError::new("screen_draw", error).with_refocus_policy())?;

    Ok(CommandOutcome {
        visibility: if matches!(
            command,
            ScreenDrawCommand::Start | ScreenDrawCommand::NewCapture
        ) {
            VisibilityPolicy::Hide
        } else {
            VisibilityPolicy::Keep
        },
        history: if matches!(
            command,
            ScreenDrawCommand::Start | ScreenDrawCommand::NewCapture
        ) {
            HistoryPolicy::Record
        } else {
            HistoryPolicy::Skip
        },
        ..CommandOutcome::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Host {
        commands: Vec<ScreenDrawCommand>,
        error: Option<String>,
    }

    impl ScreenDrawCommandHost for Host {
        fn execute_screen_draw_command(
            &mut self,
            command: ScreenDrawCommand,
        ) -> Result<(), String> {
            self.commands.push(command);
            self.error.clone().map_or(Ok(()), Err)
        }
    }

    #[test]
    fn start_stages_the_controller_then_hides_and_records() {
        let mut host = Host::default();
        let outcome = handle_screen_draw(&mut host, &ScreenDrawCommand::Start).unwrap();
        assert_eq!(host.commands, [ScreenDrawCommand::Start]);
        assert_eq!(outcome.visibility, VisibilityPolicy::Hide);
        assert_eq!(outcome.history, HistoryPolicy::Record);
    }

    #[test]
    fn new_capture_also_hides_and_records() {
        let mut host = Host::default();
        let outcome = handle_screen_draw(&mut host, &ScreenDrawCommand::NewCapture).unwrap();
        assert_eq!(host.commands, [ScreenDrawCommand::NewCapture]);
        assert_eq!(outcome.visibility, VisibilityPolicy::Hide);
        assert_eq!(outcome.history, HistoryPolicy::Record);
    }

    #[test]
    fn session_controls_keep_launcher_visibility_and_skip_launch_history() {
        for command in [
            ScreenDrawCommand::OpenToolbar,
            ScreenDrawCommand::Ghost,
            ScreenDrawCommand::Done,
            ScreenDrawCommand::Clear,
            ScreenDrawCommand::Close,
        ] {
            let mut host = Host::default();
            let outcome = handle_screen_draw(&mut host, &command).unwrap();
            assert_eq!(outcome.visibility, VisibilityPolicy::Keep);
            assert_eq!(outcome.history, HistoryPolicy::Skip);
        }
    }

    #[test]
    fn controller_errors_are_typed_and_request_refocus() {
        let mut host = Host {
            error: Some("no active session".into()),
            ..Host::default()
        };
        let error = handle_screen_draw(&mut host, &ScreenDrawCommand::Clear).unwrap_err();
        assert_eq!(error.domain, "screen_draw");
        assert_eq!(error.message, "no active session");
        assert!(error.refocus);
    }
}
