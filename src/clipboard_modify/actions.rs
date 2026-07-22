use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use super::catalog::canonical_command;
use super::model::StageSpec;
use super::parser::ModifySection;

pub const OPEN_PREFIX: &str = "clipboard_modify:open:";
pub const EXECUTE_PREFIX: &str = "clipboard_modify:execute:";
pub const UNDO_PREFIX: &str = "clipboard_modify:undo:";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ClipboardModifyActionPayload {
    OpenDialogSection {
        section: ClipboardModifySectionPayload,
    },
    ExecuteAdHocStages {
        canonical_command: String,
        stages: Vec<StageSpec>,
    },
    ExecuteTemplate {
        canonical_command: String,
        name: String,
    },
    ExecuteSavedPipeline {
        canonical_command: String,
        name: String,
    },
    Undo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClipboardModifySectionPayload {
    Modify,
    Templates,
    SavedPipelines,
    ManageTemplates,
    ManagePipelines,
    Help,
}

impl From<ModifySection> for ClipboardModifySectionPayload {
    fn from(section: ModifySection) -> Self {
        match section {
            ModifySection::Modify => Self::Modify,
            ModifySection::Templates => Self::Templates,
            ModifySection::SavedPipelines => Self::SavedPipelines,
            ModifySection::ManageTemplates => Self::ManageTemplates,
            ModifySection::ManagePipelines => Self::ManagePipelines,
            ModifySection::Help => Self::Help,
        }
    }
}

pub fn encode_action_payload<T: Serialize>(payload: &T) -> Result<String, String> {
    let json = serde_json::to_vec(payload).map_err(|err| format!("serialize payload: {err}"))?;
    Ok(URL_SAFE_NO_PAD.encode(json))
}

pub fn decode_action_payload<T: DeserializeOwned>(encoded: &str) -> Result<T, String> {
    let bytes = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|err| format!("invalid base64 payload: {err}"))?;
    serde_json::from_slice(&bytes).map_err(|err| format!("invalid JSON payload: {err}"))
}

pub fn encoded_action(prefix: &str, payload: ClipboardModifyActionPayload) -> String {
    format!(
        "{prefix}{}",
        encode_action_payload(&payload).unwrap_or_default()
    )
}

pub fn open_dialog_payload(section: ModifySection) -> ClipboardModifyActionPayload {
    ClipboardModifyActionPayload::OpenDialogSection {
        section: section.into(),
    }
}

pub fn execute_stages_payload(stages: Vec<StageSpec>) -> ClipboardModifyActionPayload {
    let canonical_command = canonical_stages_command(&stages);
    ClipboardModifyActionPayload::ExecuteAdHocStages {
        canonical_command,
        stages,
    }
}

pub fn execute_template_payload(name: String) -> ClipboardModifyActionPayload {
    ClipboardModifyActionPayload::ExecuteTemplate {
        canonical_command: format!("cm template {name}"),
        name,
    }
}

pub fn execute_saved_pipeline_payload(name: String) -> ClipboardModifyActionPayload {
    ClipboardModifyActionPayload::ExecuteSavedPipeline {
        canonical_command: format!("cm apply {name}"),
        name,
    }
}

fn canonical_stages_command(stages: &[StageSpec]) -> String {
    let parts: Vec<String> = stages
        .iter()
        .map(|stage| {
            let mut part = canonical_command(stage.operation).to_string();
            if let Some(name) = stage.arguments.name.as_ref() {
                part.push(' ');
                part.push_str(name);
            } else if let Some(language) = stage.arguments.language.as_ref() {
                part.push(' ');
                part.push_str(language);
            } else if let (Some(prefix), Some(suffix)) = (
                stage.arguments.prefix.as_ref(),
                stage.arguments.suffix.as_ref(),
            ) {
                part.push(' ');
                part.push_str(prefix);
                part.push(' ');
                part.push_str(suffix);
            }
            part
        })
        .collect();
    format!("cm {}", parts.join(" | "))
}

pub fn undo_payload() -> ClipboardModifyActionPayload {
    ClipboardModifyActionPayload::Undo
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clipboard_modify::model::{OperationId, StageArguments};

    fn round_trip(payload: ClipboardModifyActionPayload) {
        let encoded = encode_action_payload(&payload).unwrap();
        assert!(!encoded.contains('+'));
        assert!(!encoded.contains('/'));
        assert!(!encoded.contains('='));
        let decoded: ClipboardModifyActionPayload = decode_action_payload(&encoded).unwrap();
        assert_eq!(decoded, payload);
    }

    #[test]
    fn encoded_payload_round_trips_for_every_variant() {
        for section in [
            ModifySection::Modify,
            ModifySection::Templates,
            ModifySection::SavedPipelines,
            ModifySection::ManageTemplates,
            ModifySection::ManagePipelines,
            ModifySection::Help,
        ] {
            round_trip(open_dialog_payload(section));
        }
        let stages = execute_stages_payload(vec![StageSpec {
            operation: OperationId::CamelCase,
            arguments: StageArguments::default(),
        }]);
        assert_canonical_command(&stages, "cm camel-case");
        round_trip(stages);
        let template = execute_template_payload("email".into());
        assert_canonical_command(&template, "cm template email");
        round_trip(template);
        let pipeline = execute_saved_pipeline_payload("cleanup".into());
        assert_canonical_command(&pipeline, "cm apply cleanup");
        round_trip(pipeline);
        round_trip(undo_payload());
    }

    fn assert_canonical_command(payload: &ClipboardModifyActionPayload, expected: &str) {
        let actual = match payload {
            ClipboardModifyActionPayload::ExecuteAdHocStages {
                canonical_command, ..
            }
            | ClipboardModifyActionPayload::ExecuteTemplate {
                canonical_command, ..
            }
            | ClipboardModifyActionPayload::ExecuteSavedPipeline {
                canonical_command, ..
            } => canonical_command,
            _ => panic!("execution payload expected"),
        };
        assert_eq!(actual, expected);
    }

    #[test]
    fn malformed_and_unknown_payload_fields_fail_to_decode() {
        let missing = URL_SAFE_NO_PAD.encode(r#"{"type":"execute-template","name":"email"}"#);
        assert!(decode_action_payload::<ClipboardModifyActionPayload>(&missing).is_err());
        let unknown = URL_SAFE_NO_PAD.encode(r#"{"type":"execute-template","canonical_command":"cm template email","name":"email","source":"secret"}"#);
        assert!(decode_action_payload::<ClipboardModifyActionPayload>(&unknown).is_err());
        assert!(decode_action_payload::<ClipboardModifyActionPayload>("not-base64").is_err());
    }

    #[test]
    fn payloads_do_not_include_clipboard_source_text() {
        let source = "SECRET_CLIPBOARD_SOURCE";
        let encoded =
            encode_action_payload(&execute_template_payload("template-name".into())).unwrap();
        let json = String::from_utf8(URL_SAFE_NO_PAD.decode(&encoded).unwrap()).unwrap();
        assert!(!json.contains(source));
        assert!(!encoded.contains(source));
    }
}
