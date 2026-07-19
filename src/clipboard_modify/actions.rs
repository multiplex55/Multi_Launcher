use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

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
        stages: Vec<StageSpec>,
    },
    ExecuteTemplate {
        name: String,
    },
    ExecuteSavedPipeline {
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
}

impl From<ModifySection> for ClipboardModifySectionPayload {
    fn from(section: ModifySection) -> Self {
        match section {
            ModifySection::Modify => Self::Modify,
            ModifySection::Templates => Self::Templates,
            ModifySection::SavedPipelines => Self::SavedPipelines,
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
    ClipboardModifyActionPayload::ExecuteAdHocStages { stages }
}

pub fn execute_template_payload(name: String) -> ClipboardModifyActionPayload {
    ClipboardModifyActionPayload::ExecuteTemplate { name }
}

pub fn execute_saved_pipeline_payload(name: String) -> ClipboardModifyActionPayload {
    ClipboardModifyActionPayload::ExecuteSavedPipeline { name }
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
        round_trip(open_dialog_payload(ModifySection::Modify));
        round_trip(execute_stages_payload(vec![StageSpec {
            operation: OperationId::Uppercase,
            arguments: StageArguments::default(),
        }]));
        round_trip(execute_template_payload("email".into()));
        round_trip(execute_saved_pipeline_payload("cleanup".into()));
        round_trip(undo_payload());
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
