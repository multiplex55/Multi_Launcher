use crate::clipboard_modify::actions::ClipboardModifyActionPayload;
use crate::clipboard_modify::coordinator::ImmediateRequestMetadata;
use crate::clipboard_modify::parser::ClipboardModifyIntent;
use crate::commands::{
    ClipboardModifyCommand, ClipboardModifyCommandHost, CommandInvocation, CommandOutcome,
    ToastPolicy, VisibilityPolicy,
};

pub(crate) fn handle_clipboard_modify<H>(
    host: &mut H,
    command: &ClipboardModifyCommand,
    invocation: &CommandInvocation,
) -> CommandOutcome
where
    H: ClipboardModifyCommandHost + ?Sized,
{
    match command {
        ClipboardModifyCommand::Open { section } => {
            host.open_clipboard_modify(*section);
            CommandOutcome::default()
        }
        ClipboardModifyCommand::Undo { .. } => match host.undo_clipboard_modify() {
            Ok(()) => CommandOutcome {
                visibility: VisibilityPolicy::Hide,
                toasts: vec![ToastPolicy::Success("Undid Clipboard Modify".into())],
                ..CommandOutcome::default()
            },
            Err(error) => {
                host.report_clipboard_modify_action_error(error);
                CommandOutcome::default()
            }
        },
        ClipboardModifyCommand::Execute { payload, .. } => {
            let Some((intent, canonical_query, hide_launcher_on_success)) =
                execution_request(host, payload.as_ref())
            else {
                host.report_clipboard_modify_action_error(match payload {
                    None => "missing execute payload".into(),
                    Some(_) => "unexpected execute payload".into(),
                });
                return CommandOutcome::default();
            };
            let metadata = ImmediateRequestMetadata {
                action: invocation.original_action.clone(),
                query: canonical_query,
                source: invocation.source,
                hide_launcher_on_success,
            };
            match host.start_clipboard_modify(intent, metadata) {
                Ok(()) => CommandOutcome::default(),
                Err(error) => {
                    host.report_clipboard_modify_action_error(error);
                    CommandOutcome {
                        visibility: VisibilityPolicy::Show,
                        focus: true,
                        move_cursor_end: true,
                        ..CommandOutcome::default()
                    }
                }
            }
        }
        ClipboardModifyCommand::Error { message } => {
            host.report_clipboard_modify_action_error(message.clone());
            CommandOutcome::default()
        }
    }
}

fn execution_request<H>(
    host: &H,
    payload: Option<&ClipboardModifyActionPayload>,
) -> Option<(ClipboardModifyIntent, String, bool)>
where
    H: ClipboardModifyCommandHost + ?Sized,
{
    match payload? {
        ClipboardModifyActionPayload::ExecuteAdHocStages {
            canonical_command,
            stages,
        } => Some((
            ClipboardModifyIntent::Stages(stages.clone()),
            canonical_command.clone(),
            true,
        )),
        ClipboardModifyActionPayload::ExecuteTemplate {
            canonical_command,
            name,
        } => Some((
            ClipboardModifyIntent::ApplyTemplate { name: name.clone() },
            canonical_command.clone(),
            host.clipboard_modify_hide_launcher_after_apply(),
        )),
        ClipboardModifyActionPayload::ExecuteSavedPipeline {
            canonical_command,
            name,
        } => Some((
            ClipboardModifyIntent::ApplySavedPipeline { name: name.clone() },
            canonical_command.clone(),
            host.clipboard_modify_hide_launcher_after_apply(),
        )),
        ClipboardModifyActionPayload::Undo
        | ClipboardModifyActionPayload::OpenDialogSection { .. } => None,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::Action;
    use crate::clipboard_modify::actions::{
        ClipboardModifySectionPayload, execute_stages_payload, execute_template_payload,
        open_dialog_payload,
    };
    use crate::clipboard_modify::model::{OperationId, StageArguments, StageSpec};
    use crate::clipboard_modify::parser::ModifySection;
    use crate::commands::{ActivationSource, Command, HistoryPolicy, QueryPolicy};

    #[derive(Default)]
    struct Host {
        opened: Option<ClipboardModifySectionPayload>,
        started: Vec<(ClipboardModifyIntent, ImmediateRequestMetadata)>,
        error: Option<String>,
        undo_error: Option<String>,
        start_error: Option<String>,
        hide_after_apply: bool,
    }

    impl ClipboardModifyCommandHost for Host {
        fn open_clipboard_modify(&mut self, section: ClipboardModifySectionPayload) {
            self.opened = Some(section);
        }

        fn undo_clipboard_modify(&mut self) -> Result<(), String> {
            match self.undo_error.as_ref() {
                Some(error) => Err(error.clone()),
                None => Ok(()),
            }
        }

        fn start_clipboard_modify(
            &mut self,
            intent: ClipboardModifyIntent,
            metadata: ImmediateRequestMetadata,
        ) -> Result<(), String> {
            if let Some(error) = self.start_error.as_ref() {
                return Err(error.clone());
            }
            self.started.push((intent, metadata));
            Ok(())
        }

        fn clipboard_modify_hide_launcher_after_apply(&self) -> bool {
            self.hide_after_apply
        }

        fn report_clipboard_modify_action_error(&mut self, message: String) {
            self.error = Some(message);
        }
    }

    fn invocation(command: ClipboardModifyCommand) -> CommandInvocation {
        CommandInvocation {
            command: Command::ClipboardModify(command),
            original_action: Action {
                label: "Modify selection".into(),
                desc: "Clipboard Modify".into(),
                action: "clipboard_modify:execute".into(),
                args: Some("encoded".into()),
            },
            query_override: Some("must not replace canonical query".into()),
            source: ActivationSource::Gesture,
        }
    }

    #[test]
    fn open_routes_the_typed_section_without_generic_lifecycle() {
        let command = ClipboardModifyCommand::Open {
            section: ClipboardModifySectionPayload::ManageTemplates,
        };
        let invocation = invocation(command.clone());
        let mut host = Host::default();
        let outcome = handle_clipboard_modify(&mut host, &command, &invocation);

        assert_eq!(
            host.opened,
            Some(ClipboardModifySectionPayload::ManageTemplates)
        );
        assert_eq!(outcome, CommandOutcome::default());
    }

    #[test]
    fn execute_stores_original_action_source_canonical_query_and_hide_policy() {
        let payload = execute_stages_payload(vec![StageSpec {
            operation: OperationId::Uppercase,
            arguments: StageArguments::default(),
        }]);
        let command = ClipboardModifyCommand::Execute {
            payload: Some(payload),
            raw_argument: Some("legacy".into()),
            payload_error: None,
        };
        let invocation = invocation(command.clone());
        let mut host = Host::default();
        let outcome = handle_clipboard_modify(&mut host, &command, &invocation);

        assert_eq!(outcome, CommandOutcome::default());
        let (intent, metadata) = host.started.pop().unwrap();
        assert!(matches!(intent, ClipboardModifyIntent::Stages(stages) if stages.len() == 1));
        assert_eq!(metadata.action, invocation.original_action);
        assert_eq!(metadata.source, ActivationSource::Gesture);
        assert_eq!(metadata.query, "cm uppercase");
        assert!(metadata.hide_launcher_on_success);
    }

    #[test]
    fn template_snapshots_runtime_hide_preference() {
        let command = ClipboardModifyCommand::Execute {
            payload: Some(execute_template_payload("email".into())),
            raw_argument: None,
            payload_error: None,
        };
        let invocation = invocation(command.clone());
        let mut host = Host {
            hide_after_apply: false,
            ..Host::default()
        };
        handle_clipboard_modify(&mut host, &command, &invocation);

        let (intent, metadata) = host.started.pop().unwrap();
        assert!(matches!(intent, ClipboardModifyIntent::ApplyTemplate { name } if name == "email"));
        assert_eq!(metadata.query, "cm template email");
        assert!(!metadata.hide_launcher_on_success);
    }

    #[test]
    fn invalid_execute_payloads_report_legacy_messages_without_starting() {
        let cases = [
            (
                ClipboardModifyCommand::Execute {
                    payload: None,
                    raw_argument: Some("bad".into()),
                    payload_error: Some("invalid base64 payload".into()),
                },
                "missing execute payload",
            ),
            (
                ClipboardModifyCommand::Execute {
                    payload: Some(open_dialog_payload(ModifySection::Help)),
                    raw_argument: None,
                    payload_error: None,
                },
                "unexpected execute payload",
            ),
        ];
        for (command, expected) in cases {
            let invocation = invocation(command.clone());
            let mut host = Host::default();
            let outcome = handle_clipboard_modify(&mut host, &command, &invocation);
            assert_eq!(host.error.as_deref(), Some(expected));
            assert!(host.started.is_empty());
            assert_eq!(outcome, CommandOutcome::default());
        }
    }

    #[test]
    fn start_failure_restores_launcher_without_history_or_query_mutation() {
        let command = ClipboardModifyCommand::Execute {
            payload: Some(execute_template_payload("email".into())),
            raw_argument: None,
            payload_error: None,
        };
        let invocation = invocation(command.clone());
        let mut host = Host {
            start_error: Some("Clipboard Modify operation already running".into()),
            ..Host::default()
        };
        let outcome = handle_clipboard_modify(&mut host, &command, &invocation);

        assert_eq!(
            host.error.as_deref(),
            Some("Clipboard Modify operation already running")
        );
        assert_eq!(outcome.visibility, VisibilityPolicy::Show);
        assert!(outcome.focus && outcome.move_cursor_end);
        assert_eq!(outcome.history, HistoryPolicy::Skip);
        assert_eq!(outcome.query, QueryPolicy::Keep);
    }

    #[test]
    fn undo_success_and_failure_preserve_legacy_policies() {
        let command = ClipboardModifyCommand::Undo { raw_argument: None };
        let invocation = invocation(command.clone());
        let mut host = Host::default();
        let outcome = handle_clipboard_modify(&mut host, &command, &invocation);
        assert_eq!(outcome.visibility, VisibilityPolicy::Hide);
        assert_eq!(
            outcome.toasts,
            vec![ToastPolicy::Success("Undid Clipboard Modify".into())]
        );
        assert_eq!(outcome.history, HistoryPolicy::Skip);

        host.undo_error = Some("nothing to undo".into());
        let outcome = handle_clipboard_modify(&mut host, &command, &invocation);
        assert_eq!(host.error.as_deref(), Some("nothing to undo"));
        assert_eq!(outcome, CommandOutcome::default());
    }

    #[test]
    fn error_variant_reports_its_typed_message() {
        let command = ClipboardModifyCommand::Error {
            message: "unterminated quote".into(),
        };
        let invocation = invocation(command.clone());
        let mut host = Host::default();
        let outcome = handle_clipboard_modify(&mut host, &command, &invocation);
        assert_eq!(host.error.as_deref(), Some("unterminated quote"));
        assert_eq!(outcome, CommandOutcome::default());
    }
}
