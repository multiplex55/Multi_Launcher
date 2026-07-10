use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::Serialize;

use crate::actions::Action;
use crate::file_search::model::SearchKind;
use crate::file_search::query::{FileSearchCommand, SearchRequestDraft};
use crate::plugin::Plugin;

pub struct FileSearchPlugin;

#[derive(Debug, Serialize)]
struct ModePayload {
    kind: &'static str,
}

#[derive(Debug, Serialize)]
struct StartPayload {
    kind: &'static str,
    root: Option<String>,
    text: String,
}

impl Plugin for FileSearchPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        match crate::file_search::query::parse_file_search_query(query) {
            None => Vec::new(),
            Some(FileSearchCommand::OpenWindow) => vec![open_action()],
            Some(FileSearchCommand::OpenWindowWithMode { kind }) => vec![mode_action(kind)],
            Some(FileSearchCommand::StartSearch(request)) => vec![start_action(request)],
            Some(FileSearchCommand::RequestDirectory { kind, search_text }) => {
                vec![request_directory_action(kind, search_text)]
            }
            Some(FileSearchCommand::Error(error)) => vec![error_action(error.to_string())],
        }
    }

    fn name(&self) -> &str {
        "file_search"
    }

    fn description(&self) -> &str {
        "Opens local filename/content search with prefix `fs`"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            command_action("fs", "Open local file search"),
            command_action("fs file", "Search local filenames"),
            command_action("fs content", "Search local file contents"),
            command_action("fs here file", "Search filenames in a selected folder"),
            command_action("fs here content", "Search contents in a selected folder"),
        ]
    }

    fn query_prefixes(&self) -> &[&str] {
        &["fs"]
    }
}

fn open_action() -> Action {
    Action {
        label: "Open file search".into(),
        desc: "Open local filename/content search".into(),
        action: "file_search:open".into(),
        args: None,
    }
}

fn mode_action(kind: SearchKind) -> Action {
    Action {
        label: format!("Open file search ({})", kind_label(kind)),
        desc: format!("Open local search with {} mode selected", kind_label(kind)),
        action: format!(
            "file_search:mode:{}",
            encode_payload(&ModePayload {
                kind: kind_payload(kind)
            })
        ),
        args: None,
    }
}

fn start_action(request: SearchRequestDraft) -> Action {
    let root = request
        .root
        .as_ref()
        .map(|path| path.to_string_lossy().into_owned());
    let payload = StartPayload {
        kind: kind_payload(request.kind),
        root,
        text: request.search_text.clone(),
    };
    Action {
        label: format!("Start {} search", kind_label(request.kind)),
        desc: request.search_text,
        action: format!("file_search:start:{}", encode_payload(&payload)),
        args: None,
    }
}

fn request_directory_action(kind: SearchKind, search_text: String) -> Action {
    let payload = StartPayload {
        kind: kind_payload(kind),
        root: None,
        text: search_text.clone(),
    };
    Action {
        label: format!("Choose folder for {} search", kind_label(kind)),
        desc: search_text,
        action: format!("file_search:mode:{}", encode_payload(&payload)),
        args: None,
    }
}

fn error_action(message: String) -> Action {
    Action {
        label: "Invalid file search query".into(),
        desc: message,
        action: "file_search:open".into(),
        args: None,
    }
}

fn command_action(query: &str, desc: &str) -> Action {
    Action {
        label: query.into(),
        desc: desc.into(),
        action: format!("query:{query}"),
        args: None,
    }
}

fn kind_payload(kind: SearchKind) -> &'static str {
    match kind {
        SearchKind::Filename => "file",
        SearchKind::Content => "content",
    }
}

fn kind_label(kind: SearchKind) -> &'static str {
    match kind {
        SearchKind::Filename => "filename",
        SearchKind::Content => "content",
    }
}

fn encode_payload<T: Serialize>(payload: &T) -> String {
    let json = serde_json::to_vec(payload).expect("file search action payload should serialize");
    URL_SAFE_NO_PAD.encode(json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
    use serde_json::Value;

    fn plugin() -> FileSearchPlugin {
        FileSearchPlugin
    }

    fn decode_payload(action: &str, prefix: &str) -> Value {
        let encoded = action
            .strip_prefix(prefix)
            .expect("action should have expected prefix");
        let bytes = URL_SAFE_NO_PAD
            .decode(encoded)
            .expect("payload should be URL-safe base64");
        serde_json::from_slice(&bytes).expect("payload should be JSON")
    }

    #[test]
    fn non_fs_queries_return_no_results() {
        assert!(plugin().search("note hello").is_empty());
        assert!(plugin().search("").is_empty());
    }

    #[test]
    fn fs_opens_search_window() {
        let actions = plugin().search("fs");
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].action, "file_search:open");
    }

    #[test]
    fn mode_commands_open_window_preselected() {
        let file = plugin().search("fs file");
        let file_payload = decode_payload(&file[0].action, "file_search:mode:");
        assert_eq!(file_payload["kind"], "file");

        let content = plugin().search("fs content");
        let content_payload = decode_payload(&content[0].action, "file_search:mode:");
        assert_eq!(content_payload["kind"], "content");
    }

    #[test]
    fn fully_specified_searches_produce_encoded_start_actions() {
        let temp = tempfile::tempdir().unwrap();
        let actions = plugin().search(&format!("fs content needle {}", temp.path().display()));
        assert_eq!(actions.len(), 1);
        assert!(actions[0].action.starts_with("file_search:start:"));

        let payload = decode_payload(&actions[0].action, "file_search:start:");
        assert_eq!(payload["kind"], "content");
        assert_eq!(payload["text"], "needle");
        assert_eq!(payload["root"], temp.path().to_string_lossy());
    }

    #[test]
    fn malformed_queries_do_not_emit_unsafe_start_actions() {
        let unterminated = plugin().search("fs file \"unterminated");
        assert_eq!(unterminated.len(), 1);
        assert_eq!(unterminated[0].action, "file_search:open");
        assert!(unterminated[0].label.contains("Invalid"));

        let missing_dir = plugin().search("fs file README ./definitely-not-a-directory");
        assert_eq!(missing_dir.len(), 1);
        assert!(!missing_dir[0].action.starts_with("file_search:start:"));
    }
}
