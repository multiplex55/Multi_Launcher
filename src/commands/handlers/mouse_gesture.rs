use crate::commands::{CommandError, CommandOutcome, MouseGestureCommand, MouseGestureCommandHost};

pub(crate) fn handle_mouse_gesture<H>(
    host: &mut H,
    command: &MouseGestureCommand,
) -> Result<CommandOutcome, CommandError>
where
    H: MouseGestureCommandHost + ?Sized,
{
    match command {
        MouseGestureCommand::Dialog => host.open_mouse_gesture_dialog(),
        MouseGestureCommand::Add => host.open_mouse_gesture_add_dialog(),
        MouseGestureCommand::Binding => host.open_mouse_gesture_binding_dialog(),
        MouseGestureCommand::Focus { args: Some(args) } => host.open_mouse_gesture_focus(args),
        MouseGestureCommand::Focus { args: None } => host.open_mouse_gesture_dialog(),
        MouseGestureCommand::Settings => host.open_mouse_gesture_settings_dialog(),
        MouseGestureCommand::Toggle { args: Some(args) } => {
            host.set_mouse_gesture_enabled(args).map_err(|error| {
                CommandError::new(
                    "launcher",
                    format!("Failed to save mouse gestures: {error}"),
                )
                .with_refocus_policy()
            })?
        }
        MouseGestureCommand::Toggle { args: None } => {}
    }

    Ok(CommandOutcome {
        focus: host.mouse_gesture_launcher_should_refocus(),
        ..CommandOutcome::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mouse_gestures::engine::DirMode;
    use crate::mouse_gestures::selection::{GestureFocusArgs, GestureToggleArgs};

    #[derive(Default)]
    struct Host {
        opened: Option<String>,
        toggle: Option<GestureToggleArgs>,
        save_error: Option<String>,
        refocus: bool,
    }

    impl MouseGestureCommandHost for Host {
        fn open_mouse_gesture_dialog(&mut self) {
            self.opened = Some("dialog".into());
        }

        fn open_mouse_gesture_add_dialog(&mut self) {
            self.opened = Some("add".into());
        }

        fn open_mouse_gesture_binding_dialog(&mut self) {
            self.opened = Some("binding".into());
        }

        fn open_mouse_gesture_focus(&mut self, args: &GestureFocusArgs) {
            self.opened = Some(format!("focus:{}:{}", args.label, args.tokens));
        }

        fn open_mouse_gesture_settings_dialog(&mut self) {
            self.opened = Some("settings".into());
        }

        fn set_mouse_gesture_enabled(&mut self, args: &GestureToggleArgs) -> Result<(), String> {
            self.toggle = Some(args.clone());
            match self.save_error.as_ref() {
                Some(error) => Err(error.clone()),
                None => Ok(()),
            }
        }

        fn mouse_gesture_launcher_should_refocus(&self) -> bool {
            self.refocus
        }
    }

    fn focus_args() -> GestureFocusArgs {
        GestureFocusArgs {
            label: "Back".into(),
            tokens: "L".into(),
            dir_mode: DirMode::Four,
            binding_idx: Some(2),
        }
    }

    fn toggle_args() -> GestureToggleArgs {
        GestureToggleArgs {
            label: "Back".into(),
            tokens: "L".into(),
            dir_mode: DirMode::Four,
            enabled: false,
        }
    }

    #[test]
    fn every_dialog_variant_routes_through_the_typed_host() {
        let cases = [
            (MouseGestureCommand::Dialog, "dialog"),
            (MouseGestureCommand::Add, "add"),
            (MouseGestureCommand::Binding, "binding"),
            (MouseGestureCommand::Settings, "settings"),
        ];
        for (command, expected) in cases {
            let mut host = Host::default();
            let outcome = handle_mouse_gesture(&mut host, &command).unwrap();
            assert_eq!(host.opened.as_deref(), Some(expected));
            assert_eq!(outcome, CommandOutcome::default());
        }
    }

    #[test]
    fn focus_uses_typed_payload_and_missing_payload_falls_back_to_dialog() {
        let mut host = Host::default();
        handle_mouse_gesture(
            &mut host,
            &MouseGestureCommand::Focus {
                args: Some(focus_args()),
            },
        )
        .unwrap();
        assert_eq!(host.opened.as_deref(), Some("focus:Back:L"));

        host.opened = None;
        handle_mouse_gesture(&mut host, &MouseGestureCommand::Focus { args: None }).unwrap();
        assert_eq!(host.opened.as_deref(), Some("dialog"));
    }

    #[test]
    fn toggle_uses_typed_payload_while_missing_payload_is_claimed_no_op() {
        let mut host = Host::default();
        let args = toggle_args();
        handle_mouse_gesture(
            &mut host,
            &MouseGestureCommand::Toggle {
                args: Some(args.clone()),
            },
        )
        .unwrap();
        assert_eq!(host.toggle, Some(args));

        host.toggle = None;
        handle_mouse_gesture(&mut host, &MouseGestureCommand::Toggle { args: None }).unwrap();
        assert_eq!(host.toggle, None);
    }

    #[test]
    fn toggle_save_error_keeps_exact_message_and_refocus_policy() {
        let mut host = Host {
            save_error: Some("disk full".into()),
            ..Host::default()
        };
        let error = handle_mouse_gesture(
            &mut host,
            &MouseGestureCommand::Toggle {
                args: Some(toggle_args()),
            },
        )
        .unwrap_err();
        assert_eq!(error.domain, "launcher");
        assert_eq!(error.message, "Failed to save mouse gestures: disk full");
        assert!(!error.toast);
        assert!(error.refocus);
    }

    #[test]
    fn mouse_gestures_skip_history_and_generic_clear_hide_policy() {
        let mut host = Host {
            refocus: true,
            ..Host::default()
        };
        let outcome = handle_mouse_gesture(&mut host, &MouseGestureCommand::Dialog).unwrap();
        assert_eq!(outcome.history, crate::commands::HistoryPolicy::Skip);
        assert_eq!(outcome.query, crate::commands::QueryPolicy::Keep);
        assert_eq!(outcome.visibility, crate::commands::VisibilityPolicy::Keep);
        assert!(outcome.focus);
    }
}
