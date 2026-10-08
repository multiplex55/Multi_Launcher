use std::collections::HashMap;

use crate::actions::Action;
use crate::commands::{ActivationSource, SnippetPromptIntent};
use crate::plugins::snippet_template::{
    RenderErrorKind, RenderedPreview, TemplateRenderError, render_for_copy, render_preview,
};
use crate::plugins::snippets::{PreparedSnippetTemplate, SnippetEntry, SnippetRunMode};
use crate::universal_actions::RootLauncherPolicy;

/// Transient, in-memory state for one prompted snippet invocation or editor preview.
/// It intentionally has no `Debug` implementation because it owns user-entered values.
#[derive(Default)]
pub(crate) struct SnippetPromptDialog {
    session: Option<SnippetPromptSession>,
    generation: u64,
    open: bool,
    feedback: Option<SnippetPromptError>,
}

pub(crate) struct SnippetPromptSession {
    pub(crate) generation: u64,
    pub(crate) alias: String,
    pub(crate) prepared: PreparedSnippetTemplate,
    pub(crate) values: HashMap<String, String>,
    pub(crate) initial_focus: Option<String>,
    mode: SnippetPromptMode,
}

enum SnippetPromptMode {
    Execute {
        entry_snapshot: SnippetEntry,
        safe_action: Action,
        source: ActivationSource,
        history_query: String,
        root_policy: RootLauncherPolicy,
    },
    PreviewOnly {
        draft: SnippetEntry,
    },
}

/// Safe submit failures contain no field values, rendered output, or template excerpts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SnippetPromptError {
    NoActiveSession,
    PreviewOnly,
    InvalidConfiguration,
    MissingValue,
    RequiredValueEmpty,
    StaleTemplate,
    ClipboardUnavailable,
}

impl SnippetPromptError {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::NoActiveSession => "The snippet prompt is no longer open.",
            Self::PreviewOnly => "Preview mode cannot copy to the clipboard.",
            Self::InvalidConfiguration => "The snippet field configuration is invalid.",
            Self::MissingValue => "A snippet field value is missing.",
            Self::RequiredValueEmpty => "Fill in the required snippet fields before copying.",
            Self::StaleTemplate => {
                "This snippet changed while the prompt was open. Reopen it before copying."
            }
            Self::ClipboardUnavailable => "The snippet could not be copied to the clipboard.",
        }
    }
}

#[derive(Debug, PartialEq)]
pub(crate) struct SnippetPromptCompletion {
    pub(crate) alias: String,
    pub(crate) safe_action: Action,
    pub(crate) source: ActivationSource,
    pub(crate) history_query: String,
    pub(crate) root_policy: RootLauncherPolicy,
}

impl SnippetPromptDialog {
    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn ensure_open(&mut self) {
        self.open = true;
    }

    pub(crate) fn session(&self) -> Option<&SnippetPromptSession> {
        self.session.as_ref()
    }

    pub(crate) fn is_preview_only(&self) -> bool {
        self.session
            .as_ref()
            .is_some_and(|session| matches!(&session.mode, SnippetPromptMode::PreviewOnly { .. }))
    }

    pub(crate) fn feedback(&self) -> Option<SnippetPromptError> {
        self.feedback
    }

    pub(crate) fn begin_execution(&mut self, intent: SnippetPromptIntent) -> u64 {
        let session = SnippetPromptSession::new(
            intent.alias,
            intent.prepared,
            SnippetPromptMode::Execute {
                entry_snapshot: intent.entry_snapshot,
                safe_action: intent.safe_action,
                source: intent.source,
                history_query: intent.history_query,
                root_policy: intent.root_policy,
            },
        );
        self.replace_session(session)
    }

    pub(crate) fn begin_preview(&mut self, draft: SnippetEntry) -> Result<u64, SnippetPromptError> {
        let prepared = match crate::plugins::snippets::prepare_snippet_run(&draft) {
            Ok(SnippetRunMode::Prompted(prepared)) => prepared,
            Ok(SnippetRunMode::Plain) | Err(_) => {
                self.feedback = Some(SnippetPromptError::InvalidConfiguration);
                return Err(SnippetPromptError::InvalidConfiguration);
            }
        };
        let session = SnippetPromptSession::new(
            draft.alias.clone(),
            prepared,
            SnippetPromptMode::PreviewOnly { draft },
        );
        Ok(self.replace_session(session))
    }

    fn replace_session(&mut self, mut session: SnippetPromptSession) -> u64 {
        self.generation = self.generation.wrapping_add(1).max(1);
        session.generation = self.generation;
        self.session = Some(session);
        self.open = true;
        self.feedback = None;
        self.generation
    }

    /// Set only a configured field. The entered text remains in this session and is
    /// never copied into attribution or error state.
    pub(crate) fn set_value(&mut self, key: &str, value: String) -> bool {
        let Some(session) = self.session.as_mut() else {
            return false;
        };
        if !session
            .prepared
            .fields
            .iter()
            .any(|field| field.name == key)
        {
            return false;
        }
        session.values.insert(key.to_owned(), value);
        self.feedback = None;
        true
    }

    pub(crate) fn preview(&self) -> Result<RenderedPreview, TemplateRenderError> {
        let Some(session) = &self.session else {
            return Err(TemplateRenderError {
                kind: RenderErrorKind::InvalidConfiguration,
                issues: Vec::new(),
            });
        };
        render_preview(
            &session.prepared.parsed,
            &session.prepared.fields,
            &session.values,
        )
    }

    /// Try a final copy using injected resolution and clipboard operations. Both
    /// callbacks receive only the alias or rendered output; failures are replaced
    /// with safe local errors. Preview-only sessions are rejected before either runs.
    pub(crate) fn submit_with(
        &mut self,
        resolve_current: impl FnOnce(&str) -> Result<SnippetEntry, ()>,
        copy_text: impl FnOnce(&str) -> Result<(), ()>,
    ) -> Result<SnippetPromptCompletion, SnippetPromptError> {
        let result = (|| {
            let session = self
                .session
                .as_ref()
                .ok_or(SnippetPromptError::NoActiveSession)?;
            let SnippetPromptMode::Execute {
                entry_snapshot,
                safe_action,
                source,
                history_query,
                root_policy,
            } = &session.mode
            else {
                return Err(SnippetPromptError::PreviewOnly);
            };

            let rendered = render_for_copy(
                &session.prepared.parsed,
                &session.prepared.fields,
                &session.values,
            )
            .map_err(map_render_error)?;
            let current =
                resolve_current(&session.alias).map_err(|()| SnippetPromptError::StaleTemplate)?;
            if current != *entry_snapshot {
                return Err(SnippetPromptError::StaleTemplate);
            }

            copy_text(&rendered).map_err(|()| SnippetPromptError::ClipboardUnavailable)?;
            Ok(SnippetPromptCompletion {
                alias: session.alias.clone(),
                safe_action: safe_action.clone(),
                source: *source,
                history_query: history_query.clone(),
                root_policy: *root_policy,
            })
        })();

        match result {
            Ok(completion) => {
                self.session = None;
                self.open = false;
                self.feedback = None;
                Ok(completion)
            }
            Err(error) => {
                self.feedback = Some(error);
                Err(error)
            }
        }
    }

    /// Cancel and drop entered values. Preview mode returns the exact unsaved draft
    /// so the editor can resume with its pre-preview state.
    pub(crate) fn cancel(&mut self) -> Option<SnippetEntry> {
        self.open = false;
        self.feedback = None;
        match self.session.take().map(|session| session.mode) {
            Some(SnippetPromptMode::PreviewOnly { draft }) => Some(draft),
            Some(SnippetPromptMode::Execute { .. }) | None => None,
        }
    }

    pub(crate) fn shutdown(&mut self) {
        let _ = self.cancel();
    }
}

impl SnippetPromptSession {
    fn new(alias: String, prepared: PreparedSnippetTemplate, mode: SnippetPromptMode) -> Self {
        let values = prepared
            .fields
            .iter()
            .map(|field| (field.name.clone(), field.default_value.clone()))
            .collect();
        let initial_focus = prepared
            .fields
            .iter()
            .find(|field| field.required)
            .or_else(|| prepared.fields.first())
            .map(|field| field.name.clone());
        Self {
            generation: 0,
            alias,
            prepared,
            values,
            initial_focus,
            mode,
        }
    }
}

fn map_render_error(error: TemplateRenderError) -> SnippetPromptError {
    match error.kind {
        RenderErrorKind::InvalidConfiguration => SnippetPromptError::InvalidConfiguration,
        RenderErrorKind::MissingValues => SnippetPromptError::MissingValue,
        RenderErrorKind::RequiredValues => SnippetPromptError::RequiredValueEmpty,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::snippets::{SnippetFieldDefinition, SnippetInputKind};

    fn entry() -> SnippetEntry {
        SnippetEntry {
            alias: "ticketreply".into(),
            text: "Hello {{name}}, {{ticket}}".into(),
            hide_contents: false,
            prompt_for_fields: true,
            fields: vec![
                SnippetFieldDefinition {
                    name: "name".into(),
                    label: "Recipient".into(),
                    default_value: "Ada".into(),
                    required: true,
                    input_kind: SnippetInputKind::SingleLine,
                },
                SnippetFieldDefinition {
                    name: "ticket".into(),
                    label: "Ticket".into(),
                    default_value: String::new(),
                    required: false,
                    input_kind: SnippetInputKind::Multiline,
                },
            ],
        }
    }

    fn intent(entry: SnippetEntry) -> SnippetPromptIntent {
        let prepared = match crate::plugins::snippets::prepare_snippet_run(&entry).unwrap() {
            SnippetRunMode::Prompted(prepared) => prepared,
            SnippetRunMode::Plain => unreachable!(),
        };
        SnippetPromptIntent {
            alias: entry.alias.clone(),
            entry_snapshot: entry,
            prepared,
            safe_action: Action {
                label: "ticketreply".into(),
                desc: "Snippet".into(),
                action: crate::plugins::snippets::snippet_run_action("ticketreply"),
                args: None,
            },
            source: ActivationSource::Dashboard,
            history_query: "cs ticketreply".into(),
            root_policy: RootLauncherPolicy::PreserveOrdinaryState,
        }
    }

    #[test]
    fn execution_session_uses_defaults_and_focuses_first_required_field() {
        let mut dialog = SnippetPromptDialog::default();
        dialog.begin_execution(intent(entry()));

        let session = dialog.session().unwrap();
        assert_eq!(session.values.get("name").map(String::as_str), Some("Ada"));
        assert_eq!(session.values.get("ticket").map(String::as_str), Some(""));
        assert_eq!(session.initial_focus.as_deref(), Some("name"));
        assert_eq!(session.prepared.fields[0].name, "name");
        assert_eq!(session.prepared.fields[1].name, "ticket");
        assert!(dialog.is_open());
    }

    #[test]
    fn optional_only_form_focuses_first_available_field() {
        let mut saved = entry();
        for field in &mut saved.fields {
            field.required = false;
        }
        let mut dialog = SnippetPromptDialog::default();
        dialog.begin_execution(intent(saved));
        assert_eq!(
            dialog
                .session()
                .and_then(|session| session.initial_focus.as_deref()),
            Some("name")
        );
    }

    #[test]
    fn replacement_starts_fresh_generation_with_configured_defaults() {
        let mut dialog = SnippetPromptDialog::default();
        let first = dialog.begin_execution(intent(entry()));
        assert!(dialog.set_value("name", "filled sentinel".into()));
        let second = dialog.begin_execution(intent(entry()));

        let session = dialog.session().unwrap();
        assert_ne!(first, second);
        assert_eq!(session.generation, second);
        assert_eq!(session.values.get("name").map(String::as_str), Some("Ada"));
        assert_eq!(session.values.get("ticket").map(String::as_str), Some(""));
    }

    #[test]
    fn successful_submit_copies_once_and_returns_only_captured_safe_attribution() {
        let entry = entry();
        let mut dialog = SnippetPromptDialog::default();
        dialog.begin_execution(intent(entry.clone()));
        dialog.set_value("ticket", "42".into());
        let mut copied = Vec::new();
        let completion = dialog
            .submit_with(
                |_| Ok(entry.clone()),
                |text| {
                    copied.push(text.to_owned());
                    Ok(())
                },
            )
            .unwrap();

        assert_eq!(copied, ["Hello Ada, 42"]);
        assert_eq!(completion.alias, "ticketreply");
        assert_eq!(completion.safe_action.action, "snippet:run:ticketreply");
        assert_eq!(completion.safe_action.args, None);
        assert_eq!(completion.source, ActivationSource::Dashboard);
        assert_eq!(completion.history_query, "cs ticketreply");
        assert_eq!(
            completion.root_policy,
            RootLauncherPolicy::PreserveOrdinaryState
        );
        assert!(!dialog.is_open());
        assert!(dialog.session().is_none());

        let second = dialog.submit_with(|_| Ok(entry), |_| panic!("must not copy twice"));
        assert_eq!(second, Err(SnippetPromptError::NoActiveSession));
    }

    #[test]
    fn required_and_missing_values_fail_without_copy_and_keep_the_session() {
        let mut saved = entry();
        saved.fields[1].required = true;
        let mut dialog = SnippetPromptDialog::default();
        dialog.begin_execution(intent(saved.clone()));
        let mut writes = 0;
        let result = dialog.submit_with(
            |_| Ok(saved.clone()),
            |_| {
                writes += 1;
                Ok(())
            },
        );
        assert_eq!(result, Err(SnippetPromptError::RequiredValueEmpty));
        assert_eq!(writes, 0);
        assert!(dialog.is_open());
        assert_eq!(
            dialog.feedback(),
            Some(SnippetPromptError::RequiredValueEmpty)
        );

        dialog.session.as_mut().unwrap().values.remove("ticket");
        let result = dialog.submit_with(
            |_| Ok(saved),
            |_| {
                writes += 1;
                Ok(())
            },
        );
        assert_eq!(result, Err(SnippetPromptError::MissingValue));
        assert_eq!(writes, 0);
        assert!(dialog.is_open());
    }

    #[test]
    fn clipboard_failure_is_safe_and_retains_filled_values() {
        let entry = entry();
        let mut dialog = SnippetPromptDialog::default();
        dialog.begin_execution(intent(entry.clone()));
        dialog.set_value("name", "private form sentinel".into());
        let error = dialog.submit_with(|_| Ok(entry), |_| Err(())).unwrap_err();

        assert_eq!(error, SnippetPromptError::ClipboardUnavailable);
        assert!(!error.message().contains("private form sentinel"));
        assert!(dialog.is_open());
        assert_eq!(
            dialog
                .session()
                .and_then(|session| session.values.get("name"))
                .map(String::as_str),
            Some("private form sentinel")
        );
    }

    #[test]
    fn changed_deleted_or_ambiguous_current_entries_are_stale() {
        for current in [
            Some({
                let mut changed = entry();
                changed.text.push_str(" changed");
                changed
            }),
            None,
            None,
        ] {
            let snapshot = entry();
            let mut dialog = SnippetPromptDialog::default();
            dialog.begin_execution(intent(snapshot));
            let mut writes = 0;
            let result = dialog.submit_with(
                |_| current.clone().ok_or(()),
                |_| {
                    writes += 1;
                    Ok(())
                },
            );
            assert_eq!(result, Err(SnippetPromptError::StaleTemplate));
            assert_eq!(writes, 0);
            assert!(dialog.is_open());
        }
    }

    #[test]
    fn unchanged_persisted_entry_with_orphan_metadata_is_not_false_stale() {
        let mut saved = entry();
        let mut orphan = SnippetFieldDefinition::new("orphan");
        orphan.default_value = "persisted but unused".into();
        saved.fields.push(orphan);
        let mut dialog = SnippetPromptDialog::default();
        dialog.begin_execution(intent(saved.clone()));
        dialog.set_value("ticket", "42".into());

        let mut writes = Vec::new();
        let result = dialog.submit_with(
            |_| Ok(saved.clone()),
            |text| {
                writes.push(text.to_owned());
                Ok(())
            },
        );
        assert!(result.is_ok());
        assert_eq!(writes, ["Hello Ada, 42"]);
    }

    #[test]
    fn preview_cannot_resolve_or_copy_and_cancel_returns_exact_draft() {
        let mut draft = entry();
        draft.alias = "unsaved alias".into();
        draft.text = "Draft: {{name}} / {{ticket}}\r\n".into();
        draft.fields[0].default_value = "Draft default".into();
        let expected_draft = draft.clone();
        let mut dialog = SnippetPromptDialog::default();
        dialog.begin_preview(draft).unwrap();
        dialog.set_value("name", "preview only".into());

        let mut resolutions = 0;
        let mut writes = 0;
        let result = dialog.submit_with(
            |_| {
                resolutions += 1;
                Err(())
            },
            |_| {
                writes += 1;
                Ok(())
            },
        );
        assert_eq!(result, Err(SnippetPromptError::PreviewOnly));
        assert_eq!(resolutions, 0);
        assert_eq!(writes, 0);
        assert!(dialog.is_open());
        assert_eq!(dialog.preview().unwrap().text, "Draft: preview only / \r\n");

        assert_eq!(dialog.cancel(), Some(expected_draft));
        assert!(!dialog.is_open());
        assert!(dialog.session().is_none());
    }

    #[test]
    fn cancel_and_shutdown_clear_values_and_safe_feedback() {
        let mut dialog = SnippetPromptDialog::default();
        dialog.begin_execution(intent(entry()));
        dialog.set_value("name", "private sentinel".into());
        dialog.cancel();
        assert!(dialog.session().is_none());
        assert_eq!(dialog.feedback(), None);

        dialog.begin_execution(intent(entry()));
        dialog.set_value("name", "private sentinel".into());
        dialog.shutdown();
        assert!(dialog.session().is_none());
        assert!(!dialog.is_open());
    }

    #[test]
    fn session_and_attribution_do_not_format_entered_values() {
        let entry = entry();
        let mut dialog = SnippetPromptDialog::default();
        dialog.begin_execution(intent(entry.clone()));
        dialog.set_value("name", "private form sentinel".into());
        let completion = dialog.submit_with(|_| Ok(entry), |_| Ok(())).unwrap();
        assert_eq!(completion.safe_action.args, None);
        assert_eq!(completion.safe_action.action, "snippet:run:ticketreply");
        assert_eq!(completion.history_query, "cs ticketreply");
    }
}
