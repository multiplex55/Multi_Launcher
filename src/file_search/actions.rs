use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, Context};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::file_search::model::SearchKind;

pub const OPEN_ACTION: &str = "file_search:open";
pub const MODE_PREFIX: &str = "file_search:mode:";
pub const START_PREFIX: &str = "file_search:start:";
pub const CANCEL_ACTION: &str = "file_search:cancel";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileSearchModePayload {
    pub kind: FileSearchKindPayload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileSearchStartPayload {
    pub kind: FileSearchKindPayload,
    pub root: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileSearchKindPayload {
    File,
    Content,
}

impl From<SearchKind> for FileSearchKindPayload {
    fn from(kind: SearchKind) -> Self {
        match kind {
            SearchKind::Filename => Self::File,
            SearchKind::Content => Self::Content,
        }
    }
}

impl From<FileSearchKindPayload> for SearchKind {
    fn from(kind: FileSearchKindPayload) -> Self {
        match kind {
            FileSearchKindPayload::File => Self::Filename,
            FileSearchKindPayload::Content => Self::Content,
        }
    }
}

pub fn mode_action_payload(kind: SearchKind) -> FileSearchModePayload {
    FileSearchModePayload { kind: kind.into() }
}

pub fn start_action_payload(
    kind: SearchKind,
    root: Option<String>,
    text: String,
) -> FileSearchStartPayload {
    FileSearchStartPayload {
        kind: kind.into(),
        root,
        text,
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

impl FileSearchModePayload {
    pub fn validate(&self) -> Result<(), String> {
        Ok(())
    }
    pub fn search_kind(&self) -> SearchKind {
        self.kind.into()
    }
}

impl FileSearchStartPayload {
    pub fn validate(&self) -> Result<(), String> {
        if self.text.trim().is_empty() {
            return Err("File search text cannot be empty".to_string());
        }
        if self
            .root
            .as_deref()
            .is_some_and(|root| root.trim().is_empty())
        {
            return Err("File search root cannot be empty".to_string());
        }
        Ok(())
    }

    pub fn search_kind(&self) -> SearchKind {
        self.kind.into()
    }

    pub fn root_path(&self) -> Option<PathBuf> {
        self.root.as_ref().map(PathBuf::from)
    }
}

pub fn nested_search_root(path: &Path, is_directory: bool) -> Option<PathBuf> {
    if is_directory {
        Some(path.to_path_buf())
    } else {
        path.parent().map(Path::to_path_buf)
    }
}

pub fn containing_directory(path: &Path) -> Option<PathBuf> {
    path.parent().map(Path::to_path_buf)
}

pub fn copied_filename(path: &Path) -> Option<String> {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
}

pub fn windows_reveal_args(path: &Path) -> Vec<String> {
    vec![format!("/select,{}", path.display())]
}

pub fn open_path(path: &Path) -> anyhow::Result<()> {
    open::that(path).with_context(|| format!("open {}", path.display()))
}

pub fn reveal_path(path: &Path) -> anyhow::Result<()> {
    #[cfg(target_os = "windows")]
    {
        let status = Command::new("explorer")
            .args(windows_reveal_args(path))
            .spawn()
            .with_context(|| format!("reveal {} in Explorer", path.display()))?;
        let _ = status;
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let target = path.parent().unwrap_or(path);
        open::that(target).with_context(|| format!("reveal {}", path.display()))
    }
}

pub fn open_terminal_in_directory(dir: &Path) -> anyhow::Result<()> {
    if !dir.is_dir() {
        return Err(anyhow!("{} is not a directory", dir.display()));
    }
    let mut command = if crate::plugins::shell::use_wezterm() {
        let mut c = Command::new("wezterm");
        c.arg("start");
        c
    } else if cfg!(target_os = "windows") {
        Command::new("cmd")
    } else {
        let mut c = Command::new("sh");
        c.arg("-lc").arg("${TERMINAL:-x-terminal-emulator}");
        c
    };
    command
        .current_dir(dir)
        .spawn()
        .with_context(|| format!("open terminal in {}", dir.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_request_payload_round_trips() {
        let request = start_action_payload(
            SearchKind::Content,
            Some("/tmp/project".to_string()),
            "needle".to_string(),
        );
        let encoded = encode_action_payload(&request).unwrap();
        let decoded: FileSearchStartPayload = decode_action_payload(&encoded).unwrap();
        decoded.validate().unwrap();
        assert_eq!(decoded, request);
    }

    #[test]
    fn windows_paths_are_preserved() {
        let request = start_action_payload(
            SearchKind::Filename,
            Some(r"C:\Users\alice\Project".to_string()),
            "main.rs".to_string(),
        );
        let encoded = encode_action_payload(&request).unwrap();
        let decoded: FileSearchStartPayload = decode_action_payload(&encoded).unwrap();
        assert_eq!(decoded.root.as_deref(), Some(r"C:\Users\alice\Project"));
    }

    #[test]
    fn malformed_base64_is_rejected() {
        let err =
            decode_action_payload::<FileSearchStartPayload>("not valid base64!!!").unwrap_err();
        assert!(err.contains("base64"));
    }

    #[test]
    fn malformed_json_is_rejected() {
        let encoded = URL_SAFE_NO_PAD.encode(b"{not-json");
        let err = decode_action_payload::<FileSearchStartPayload>(&encoded).unwrap_err();
        assert!(err.contains("JSON"));
    }

    #[test]
    fn decoded_request_validation_errors_are_surfaced() {
        let request =
            start_action_payload(SearchKind::Content, Some("".to_string()), "".to_string());
        let encoded = encode_action_payload(&request).unwrap();
        let decoded: FileSearchStartPayload = decode_action_payload(&encoded).unwrap();
        let err = decoded.validate().unwrap_err();
        assert!(err.contains("text"));
    }

    #[test]
    fn file_result_uses_parent_for_nested_search() {
        assert_eq!(
            nested_search_root(Path::new("/tmp/project/src/main.rs"), false),
            Some(PathBuf::from("/tmp/project/src"))
        );
    }

    #[test]
    fn directory_result_uses_itself_for_nested_search() {
        assert_eq!(
            nested_search_root(Path::new("/tmp/project/src"), true),
            Some(PathBuf::from("/tmp/project/src"))
        );
    }

    #[test]
    fn containing_directory_action_uses_parent() {
        assert_eq!(
            containing_directory(Path::new("/tmp/project/src/main.rs")),
            Some(PathBuf::from("/tmp/project/src"))
        );
    }

    #[test]
    fn copied_filename_extracts_leaf_name() {
        assert_eq!(
            copied_filename(Path::new("/tmp/project/src/main.rs")).as_deref(),
            Some("main.rs")
        );
    }

    #[test]
    fn windows_reveal_argument_construction_selects_path() {
        assert_eq!(
            windows_reveal_args(Path::new(r"C:\Users\alice\file.txt")),
            vec![r"/select,C:\Users\alice\file.txt".to_string()]
        );
    }
}
