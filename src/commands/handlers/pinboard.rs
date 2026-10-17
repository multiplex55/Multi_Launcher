use crate::commands::{CommandError, CommandOutcome, PinboardCommand, PinboardCommandHost};

pub(crate) fn handle_pinboard<H>(
    host: &mut H,
    command: &PinboardCommand,
) -> Result<CommandOutcome, CommandError>
where
    H: PinboardCommandHost + ?Sized,
{
    match command {
        PinboardCommand::Capture => match host.start_pinboard_capture() {
            Ok(true) => Ok(CommandOutcome::default()),
            Ok(false) => Err(CommandError::new(
                "pinboard",
                "A screenshot pin capture is already in progress",
            )
            .with_gui_failure_policy()),
            Err(error) => Err(CommandError::new("pinboard", error).with_gui_failure_policy()),
        },
        PinboardCommand::List => host
            .open_pinboard_list()
            .map(|()| CommandOutcome::default())
            .map_err(|error| CommandError::new("pinboard", error).with_gui_failure_policy()),
        PinboardCommand::Unknown { raw } => Err(CommandError::new(
            "pinboard",
            format!("Unknown Pinboard command: {raw}"),
        )
        .with_gui_failure_policy()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{HistoryPolicy, PendingQueryPolicy, QueryPolicy, VisibilityPolicy};

    #[derive(Default)]
    struct Host {
        captures: usize,
        lists: usize,
    }

    impl PinboardCommandHost for Host {
        fn start_pinboard_capture(&mut self) -> Result<bool, String> {
            self.captures += 1;
            Ok(true)
        }

        fn open_pinboard_list(&mut self) -> Result<(), String> {
            self.lists += 1;
            Ok(())
        }
    }

    #[test]
    fn pinboard_typed_actions_reach_the_gui_host_without_generic_visibility_or_focus_policy() {
        let mut host = Host::default();
        let capture = handle_pinboard(&mut host, &PinboardCommand::Capture).unwrap();
        let list = handle_pinboard(&mut host, &PinboardCommand::List).unwrap();

        assert_eq!((host.captures, host.lists), (1, 1));
        for outcome in [capture, list] {
            assert_eq!(outcome.query, QueryPolicy::Keep);
            assert_eq!(outcome.pending_query, PendingQueryPolicy::Keep);
            assert_eq!(outcome.visibility, VisibilityPolicy::Keep);
            assert!(!outcome.restore && !outcome.focus && !outcome.search);
            assert_eq!(outcome.history, HistoryPolicy::Skip);
        }
    }
}
