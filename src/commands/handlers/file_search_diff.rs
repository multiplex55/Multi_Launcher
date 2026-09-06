use crate::commands::{
    CommandError, CommandOutcome, DiffCommand, DiffCommandHost, FileSearchCommand,
    FileSearchCommandHost,
};

pub(crate) fn handle_file_search<H>(host: &mut H, command: &FileSearchCommand) -> CommandOutcome
where
    H: FileSearchCommandHost + ?Sized,
{
    match command {
        FileSearchCommand::Open => host.open_file_search(),
        FileSearchCommand::Cancel => host.cancel_file_search(),
        FileSearchCommand::SetMode(payload) => host.set_file_search_mode(payload),
        FileSearchCommand::Start(payload) => host.start_file_search(payload),
        FileSearchCommand::Invalid { error, .. } => {
            // Legacy mode/start actions claimed the namespace and opened the
            // dialog before reporting malformed payloads.
            host.open_file_search();
            host.report_file_search_action_error(format!("Invalid file search action: {error}"));
        }
    }
    CommandOutcome::default()
}

pub(crate) fn handle_diff<H>(
    host: &mut H,
    command: &DiffCommand,
) -> Result<CommandOutcome, CommandError>
where
    H: DiffCommandHost + ?Sized,
{
    match command {
        DiffCommand::Open(payload) => host
            .open_diff(payload)
            .map_err(|error| CommandError::new("diff", error))?,
        DiffCommand::Invalid { error, .. } => {
            return Err(CommandError::new("diff", error.clone()));
        }
    }
    Ok(CommandOutcome::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::query::DiffOpenPayload;
    use crate::file_search::actions::{
        FileSearchKindPayload, FileSearchModePayload, FileSearchStartPayload,
    };

    #[derive(Default)]
    struct Host {
        file_search_operations: Vec<String>,
        file_search_error: Option<String>,
        diff_payload: Option<DiffOpenPayload>,
        diff_error: Option<String>,
    }

    impl FileSearchCommandHost for Host {
        fn open_file_search(&mut self) {
            self.file_search_operations.push("open".into());
        }

        fn cancel_file_search(&mut self) {
            self.file_search_operations.push("cancel".into());
        }

        fn set_file_search_mode(&mut self, payload: &FileSearchModePayload) {
            self.file_search_operations
                .push(format!("mode:{:?}", payload.kind));
        }

        fn start_file_search(&mut self, payload: &FileSearchStartPayload) {
            self.file_search_operations.push(format!(
                "start:{:?}:{}:{}",
                payload.kind,
                payload.root.as_deref().unwrap_or_default(),
                payload.text
            ));
        }

        fn report_file_search_action_error(&mut self, message: String) {
            self.file_search_error = Some(message);
        }
    }

    impl DiffCommandHost for Host {
        fn open_diff(&mut self, payload: &DiffOpenPayload) -> Result<(), String> {
            self.diff_payload = Some(payload.clone());
            match self.diff_error.as_ref() {
                Some(error) => Err(error.clone()),
                None => Ok(()),
            }
        }
    }

    #[test]
    fn every_file_search_variant_uses_typed_host_operations() {
        let cases = [
            (FileSearchCommand::Open, "open"),
            (FileSearchCommand::Cancel, "cancel"),
            (
                FileSearchCommand::SetMode(FileSearchModePayload {
                    kind: FileSearchKindPayload::Content,
                }),
                "mode:Content",
            ),
            (
                FileSearchCommand::Start(FileSearchStartPayload {
                    kind: FileSearchKindPayload::File,
                    root: Some(r"C:\work".into()),
                    text: "needle".into(),
                }),
                r"start:File:C:\work:needle",
            ),
        ];

        for (command, expected) in cases {
            let mut host = Host::default();
            let outcome = handle_file_search(&mut host, &command);
            assert_eq!(host.file_search_operations, [expected]);
            assert_eq!(outcome, CommandOutcome::default());
        }
    }

    #[test]
    fn malformed_file_search_is_claimed_opens_dialog_and_keeps_exact_message() {
        let mut host = Host::default();
        let outcome = handle_file_search(
            &mut host,
            &FileSearchCommand::Invalid {
                raw: "file_search:start:bad".into(),
                error: "invalid base64 payload: bad byte".into(),
            },
        );

        assert_eq!(host.file_search_operations, ["open"]);
        assert_eq!(
            host.file_search_error.as_deref(),
            Some("Invalid file search action: invalid base64 payload: bad byte")
        );
        assert_eq!(outcome, CommandOutcome::default());
    }

    #[test]
    fn diff_uses_typed_payload_and_preserves_error_domain_and_wording() {
        let payload = DiffOpenPayload {
            left: Some("left.txt".into()),
            right: Some("right.txt".into()),
        };
        let mut host = Host::default();
        assert_eq!(
            handle_diff(&mut host, &DiffCommand::Open(payload.clone())).unwrap(),
            CommandOutcome::default()
        );
        assert_eq!(host.diff_payload, Some(payload));

        host.diff_error = Some("could not open comparison".into());
        let error = handle_diff(
            &mut host,
            &DiffCommand::Open(DiffOpenPayload {
                left: None,
                right: None,
            }),
        )
        .unwrap_err();
        assert_eq!(error.domain, "diff");
        assert_eq!(error.message, "could not open comparison");
        assert!(!error.toast && !error.refocus);
    }

    #[test]
    fn malformed_diff_is_claimed_without_calling_the_host() {
        let mut host = Host::default();
        let error = handle_diff(
            &mut host,
            &DiffCommand::Invalid {
                raw: "diff:open:bad".into(),
                error: "invalid diff payload: Invalid symbol 98, offset 0.".into(),
            },
        )
        .unwrap_err();
        assert!(host.diff_payload.is_none());
        assert_eq!(error.domain, "diff");
        assert_eq!(
            error.message,
            "invalid diff payload: Invalid symbol 98, offset 0."
        );
    }
}
