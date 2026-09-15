use crate::commands::{
    CommandError, CommandOutcome, HistoryPolicy, RadialCommand, RadialCommandHost,
};
use crate::radial::control::{RadialControlRequest, RadialMenuSelector};

pub(crate) fn handle_radial<H>(
    host: &mut H,
    command: &RadialCommand,
) -> Result<CommandOutcome, CommandError>
where
    H: RadialCommandHost + ?Sized,
{
    match command {
        RadialCommand::ShowDefault | RadialCommand::Show(_) if !host.radial_is_enabled() => {
            return Err(
                CommandError::new("radial", "radial menus are disabled in settings")
                    .with_refocus_policy(),
            );
        }
        RadialCommand::ShowDefault => {
            host.request_radial_control(RadialControlRequest::Show(RadialMenuSelector::Default))
        }
        RadialCommand::Show(target) => host.request_radial_control(RadialControlRequest::Show(
            RadialMenuSelector::IdOrName(target.clone()),
        )),
        RadialCommand::Close => host.request_radial_control(RadialControlRequest::Close),
        RadialCommand::Edit => {
            host.open_radial_editor(false);
            Ok(())
        }
        RadialCommand::Skins => {
            host.open_radial_editor(true);
            Ok(())
        }
    }
    .map_err(|error| CommandError::new("radial", error).with_refocus_policy())?;

    Ok(CommandOutcome {
        history: HistoryPolicy::Record,
        ..CommandOutcome::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Host {
        enabled: bool,
        requests: Vec<RadialControlRequest>,
        editor: Option<bool>,
    }

    impl RadialCommandHost for Host {
        fn radial_is_enabled(&self) -> bool {
            self.enabled
        }

        fn request_radial_control(&mut self, request: RadialControlRequest) -> Result<(), String> {
            self.requests.push(request);
            Ok(())
        }

        fn open_radial_editor(&mut self, skins: bool) {
            self.editor = Some(skins);
        }
    }

    #[test]
    fn show_and_close_issue_one_typed_request_and_preserve_root_state() {
        let mut host = Host {
            enabled: true,
            ..Host::default()
        };
        for (command, expected) in [
            (
                RadialCommand::ShowDefault,
                RadialControlRequest::Show(RadialMenuSelector::Default),
            ),
            (
                RadialCommand::Show("tools".into()),
                RadialControlRequest::Show(RadialMenuSelector::IdOrName("tools".into())),
            ),
            (RadialCommand::Close, RadialControlRequest::Close),
        ] {
            let before = host.requests.len();
            let outcome = handle_radial(&mut host, &command).unwrap();
            assert_eq!(host.requests.len(), before + 1);
            assert_eq!(host.requests.last(), Some(&expected));
            assert_eq!(
                outcome,
                CommandOutcome {
                    history: HistoryPolicy::Record,
                    ..CommandOutcome::default()
                }
            );
        }
    }

    #[test]
    fn disabled_show_fails_without_controller_request_but_editor_remains_available() {
        let mut host = Host::default();
        assert!(handle_radial(&mut host, &RadialCommand::ShowDefault).is_err());
        assert!(host.requests.is_empty());
        handle_radial(&mut host, &RadialCommand::Edit).unwrap();
        assert_eq!(host.editor, Some(false));
        handle_radial(&mut host, &RadialCommand::Skins).unwrap();
        assert_eq!(host.editor, Some(true));
    }
}
