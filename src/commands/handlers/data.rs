use crate::commands::{
    CommandError, CommandOutcome, DataCommand, DataCommandHost, DataDialogFocus, ToastPolicy,
};

/// Route data-maintenance intent through the GUI host without allowing the
/// handler to inspect action protocol strings or own persistence/UI state.
pub(crate) fn handle_data<H>(
    host: &mut H,
    command: &DataCommand,
) -> Result<CommandOutcome, CommandError>
where
    H: DataCommandHost + ?Sized,
{
    let mut outcome = CommandOutcome::default();
    match command {
        DataCommand::Dialog => host.open_data_dialog(DataDialogFocus::Overview),
        DataCommand::Health => host.open_data_dialog(DataDialogFocus::Health),
        DataCommand::Backup => host.request_data_backup().map(|()| {
            outcome
                .toasts
                .push(ToastPolicy::Info("Backup requested".into()));
        }),
        DataCommand::OpenFolder => host.open_data_folder(),
        DataCommand::Recovery(recovery) => host.stage_data_recovery(recovery).map(|()| {
            outcome.toasts.push(ToastPolicy::Info(
                "Recovery staging requested; validation is running in Data & Recovery".into(),
            ));
        }),
        DataCommand::Invalid { error, .. } => Err(error.clone()),
    }
    .map_err(|error| CommandError::new("data", error).with_gui_failure_policy())?;

    outcome.focus = host.data_launcher_should_refocus();
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{
        DataRecoveryCommand, DataRecoveryConfirmation, HistoryPolicy, VisibilityPolicy,
    };
    use crate::persistence::PersistentStoreId;

    #[derive(Default)]
    struct Host {
        calls: Vec<String>,
        error: Option<String>,
        refocus: bool,
    }

    impl DataCommandHost for Host {
        fn open_data_dialog(&mut self, focus: DataDialogFocus) -> Result<(), String> {
            self.calls.push(match focus {
                DataDialogFocus::Overview => "dialog".into(),
                DataDialogFocus::Health => "dialog:health".into(),
            });
            self.result()
        }

        fn request_data_backup(&mut self) -> Result<(), String> {
            self.calls.push("backup".into());
            self.result()
        }

        fn open_data_folder(&mut self) -> Result<(), String> {
            self.calls.push("folder".into());
            self.result()
        }

        fn stage_data_recovery(&mut self, command: &DataRecoveryCommand) -> Result<(), String> {
            self.calls.push(format!("recovery:{command:?}"));
            self.result()
        }

        fn data_launcher_should_refocus(&self) -> bool {
            self.refocus
        }
    }

    impl Host {
        fn result(&self) -> Result<(), String> {
            self.error.clone().map_or(Ok(()), Err)
        }
    }

    #[test]
    fn non_destructive_commands_route_through_the_narrow_host_boundary() {
        for (command, expected) in [
            (DataCommand::Dialog, "dialog"),
            (DataCommand::Health, "dialog:health"),
            (DataCommand::Backup, "backup"),
            (DataCommand::OpenFolder, "folder"),
        ] {
            let mut host = Host::default();
            let outcome = handle_data(&mut host, &command).unwrap();
            assert_eq!(host.calls, [expected]);
            assert_eq!(outcome.history, HistoryPolicy::Skip);
            assert_eq!(outcome.visibility, VisibilityPolicy::Keep);
        }
    }

    #[test]
    fn recovery_requires_a_typed_confirmed_request_and_never_uses_a_raw_action() {
        let command = DataRecoveryCommand::Restore {
            store_id: PersistentStoreId::Settings,
            snapshot_id: "snapshot-1".into(),
            confirmation: DataRecoveryConfirmation::from_explicit_user_confirmation(),
        };
        assert_eq!(
            DataCommand::Recovery(command.clone()).kind_name(),
            "restore"
        );
        let mut host = Host::default();
        let outcome = handle_data(&mut host, &DataCommand::Recovery(command)).unwrap();
        assert!(host.calls[0].contains("Settings"));
        assert!(host.calls[0].contains("snapshot-1"));
        assert_eq!(
            outcome.toasts,
            [ToastPolicy::Info(
                "Recovery staging requested; validation is running in Data & Recovery".into()
            )]
        );
    }

    #[test]
    fn invalid_or_failed_requests_use_data_error_and_gui_failure_policy() {
        let mut host = Host::default();
        let error = handle_data(
            &mut host,
            &DataCommand::Invalid {
                raw: "data:restore".into(),
                error: "restore is UI-only".into(),
            },
        )
        .unwrap_err();
        assert_eq!(error.domain, "data");
        assert_eq!(error.message, "restore is UI-only");
        assert!(error.toast && error.refocus);

        host.error = Some("worker stopped".into());
        let error = handle_data(&mut host, &DataCommand::Backup).unwrap_err();
        assert_eq!(error.message, "worker stopped");
        assert!(error.toast && error.refocus);
    }
}
