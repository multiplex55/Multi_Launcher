use crate::commands::{
    CommandError, CommandOutcome, CoordinateToolCommand, HistoryPolicy, LauncherCommandHost,
    ToastPolicy,
};

pub(crate) fn handle_coordinate_tool<H: LauncherCommandHost + ?Sized>(
    host: &mut H,
    command: &CoordinateToolCommand,
) -> Result<CommandOutcome, CommandError> {
    let copied = match command {
        CoordinateToolCommand::HudHelp => {
            return Ok(help_outcome(COORDINATE_HELP));
        }
        CoordinateToolCommand::CrosshairHelp => {
            return Ok(help_outcome(CROSSHAIR_HELP));
        }
        CoordinateToolCommand::Invalid { error, .. } => {
            let mut error = CommandError::new("coordinate_tool", error.clone());
            error.toast = true;
            return Err(error);
        }
        _ => host
            .execute_coordinate_tool_command(command)
            .map_err(|message| {
                let mut error = CommandError::new("coordinate_tool", message);
                error.toast = true;
                error
            })?,
    };

    Ok(CommandOutcome {
        history: HistoryPolicy::Skip,
        toasts: copied.into_iter().map(ToastPolicy::Copied).collect(),
        ..CommandOutcome::default()
    })
}

fn help_outcome(text: &str) -> CommandOutcome {
    CommandOutcome {
        history: HistoryPolicy::Skip,
        toasts: vec![ToastPolicy::Info(text.into())],
        ..CommandOutcome::default()
    }
}

const COORDINATE_HELP: &str = "Coordinate HUD: coord toggles the HUD; use coord on|off, coord space desktop|monitor|client, coord compact|detailed, or coord offset <signed-x> <signed-y> (-512..512). Use coord freeze|unfreeze; coord copy writes the displayed signed physical-pixel position as x,y. Desktop uses the signed virtual desktop, monitor is relative to the cursor monitor origin, and client is relative to the foreground client origin. While Multi Launcher is foreground, client coordinates use the last external active window. The crosshair is independent; see crosshair help.";

const CROSSHAIR_HELP: &str = "Crosshair controls: crosshair on|off; crosshair color #rrggbb; crosshair thickness 1..16; crosshair length 2..256; crosshair opacity 0.1..1.0; crosshair guides on|off; crosshair contrast on|off.";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::CoordinateToolCommand;

    #[derive(Default)]
    struct Host {
        calls: Vec<CoordinateToolCommand>,
        copy: Option<String>,
        error: Option<String>,
    }

    impl LauncherCommandHost for Host {
        fn launcher_is_visible(&self) -> bool {
            true
        }

        fn execute_coordinate_tool_command(
            &mut self,
            command: &CoordinateToolCommand,
        ) -> Result<Option<String>, String> {
            self.calls.push(command.clone());
            self.error.clone().map_or(Ok(self.copy.clone()), Err)
        }
    }

    #[test]
    fn passive_controls_dispatch_once_without_query_visibility_or_focus_effects() {
        let mut host = Host::default();
        let outcome = handle_coordinate_tool(&mut host, &CoordinateToolCommand::ToggleHud).unwrap();
        assert_eq!(host.calls, [CoordinateToolCommand::ToggleHud]);
        assert_eq!(outcome, CommandOutcome::default());
    }

    #[test]
    fn copy_feedback_is_reported_only_after_host_success() {
        let mut host = Host {
            copy: Some("-20,14".into()),
            ..Host::default()
        };
        let outcome = handle_coordinate_tool(&mut host, &CoordinateToolCommand::Copy).unwrap();
        assert_eq!(outcome.toasts, [ToastPolicy::Copied("-20,14".into())]);
        assert_eq!(outcome.history, HistoryPolicy::Skip);

        host.error = Some("clipboard unavailable".into());
        let error = handle_coordinate_tool(&mut host, &CoordinateToolCommand::Copy).unwrap_err();
        assert!(error.toast);
        assert!(!error.refocus);
    }

    #[test]
    fn help_is_pure_and_invalid_commands_do_not_reach_the_host() {
        let mut host = Host::default();
        let help = handle_coordinate_tool(&mut host, &CoordinateToolCommand::HudHelp).unwrap();
        assert!(
            matches!(help.toasts.as_slice(), [ToastPolicy::Info(text)] if text.contains("last external active window"))
        );
        assert_eq!(
            help,
            CommandOutcome {
                history: HistoryPolicy::Skip,
                toasts: help.toasts.clone(),
                ..CommandOutcome::default()
            }
        );
        assert!(host.calls.is_empty());

        let invalid = CoordinateToolCommand::Invalid {
            raw: "coord:offset:513:0".into(),
            error: "offset x must be in the range -512..512".into(),
        };
        let error = handle_coordinate_tool(&mut host, &invalid).unwrap_err();
        assert!(error.toast);
        assert!(!error.refocus);
        assert!(host.calls.is_empty());
    }
}
