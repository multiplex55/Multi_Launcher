use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, Context};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::file_search::model::SearchKind;
use crate::file_search::settings::FileSearchSettings;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvocationTarget<'a> {
    pub file: &'a Path,
    pub line: Option<usize>,
    pub column: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandInvocation {
    pub executable: PathBuf,
    pub args: Vec<String>,
    pub working_dir: Option<PathBuf>,
}

fn path_entries() -> Vec<PathBuf> {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).collect())
        .unwrap_or_default()
}

pub fn configured_executable_available(executable: &str) -> bool {
    let trimmed = executable.trim();
    if trimmed.is_empty() {
        return false;
    }
    let path = Path::new(trimmed);
    if path.components().count() > 1 || path.is_absolute() {
        return path.is_file();
    }
    path_entries().into_iter().any(|dir| {
        let candidate = dir.join(trimmed);
        candidate.is_file()
            || (cfg!(target_os = "windows") && candidate.with_extension("exe").is_file())
    })
}

pub fn expand_invocation_template(
    template: &[String],
    target: &InvocationTarget<'_>,
) -> anyhow::Result<Vec<String>> {
    let parent = target.file.parent().unwrap_or(target.file);
    let line = target.line.map(|v| v.to_string()).unwrap_or_default();
    let column = target.column.map(|v| v.to_string()).unwrap_or_default();
    let mut args = Vec::new();
    for item in template {
        let pieces =
            shlex::split(item).ok_or_else(|| anyhow!("invalid argument template: {item}"))?;
        for piece in pieces {
            args.push(
                piece
                    .replace("{file}", &target.file.display().to_string())
                    .replace("{parent}", &parent.display().to_string())
                    .replace("{line}", &line)
                    .replace("{column}", &column),
            );
        }
    }
    Ok(args)
}

pub fn editor_invocation(
    settings: &FileSearchSettings,
    target: InvocationTarget<'_>,
) -> anyhow::Result<CommandInvocation> {
    let executable = settings.preferred_editor_command.trim();
    if executable.is_empty() {
        return Err(anyhow!(
            "configure a preferred editor executable before opening File Search results in an editor"
        ));
    }
    if !configured_executable_available(executable) {
        return Err(anyhow!(
            "configured editor executable '{}' is unavailable; update File Search settings to an installed editor path",
            executable
        ));
    }
    let template = if settings.preferred_editor_args.is_empty() {
        vec!["{file}".to_string()]
    } else {
        settings.preferred_editor_args.clone()
    };
    Ok(CommandInvocation {
        executable: PathBuf::from(executable),
        args: expand_invocation_template(&template, &target)?,
        working_dir: None,
    })
}

pub fn terminal_invocation(
    settings: &FileSearchSettings,
    dir: &Path,
) -> anyhow::Result<CommandInvocation> {
    if !dir.is_dir() {
        return Err(anyhow!("{} is not a directory", dir.display()));
    }
    let executable = settings.preferred_terminal_command.trim();
    if executable.is_empty() {
        let inv = if crate::plugins::shell::use_wezterm() {
            CommandInvocation {
                executable: "wezterm".into(),
                args: vec!["start".into()],
                working_dir: Some(dir.to_path_buf()),
            }
        } else if cfg!(target_os = "windows") {
            CommandInvocation {
                executable: "cmd".into(),
                args: Vec::new(),
                working_dir: Some(dir.to_path_buf()),
            }
        } else {
            CommandInvocation {
                executable: "sh".into(),
                args: vec!["-lc".into(), "${TERMINAL:-x-terminal-emulator}".into()],
                working_dir: Some(dir.to_path_buf()),
            }
        };
        return Ok(inv);
    }
    if !configured_executable_available(executable) {
        return Err(anyhow!(
            "configured terminal executable '{}' is unavailable; update File Search settings to an installed terminal path",
            executable
        ));
    }
    let target = InvocationTarget {
        file: dir,
        line: None,
        column: None,
    };
    Ok(CommandInvocation {
        executable: PathBuf::from(executable),
        args: expand_invocation_template(&settings.preferred_terminal_args, &target)?,
        working_dir: Some(dir.to_path_buf()),
    })
}

pub fn spawn_invocation(invocation: CommandInvocation) -> anyhow::Result<()> {
    let mut command = Command::new(&invocation.executable);
    command.args(&invocation.args);
    if let Some(dir) = &invocation.working_dir {
        command.current_dir(dir);
    }
    command
        .spawn()
        .with_context(|| format!("run {}", invocation.executable.display()))?;
    Ok(())
}

pub fn open_in_configured_editor(
    settings: &FileSearchSettings,
    target: InvocationTarget<'_>,
) -> anyhow::Result<()> {
    spawn_invocation(editor_invocation(settings, target)?)
}

pub fn open_configured_terminal_in_directory(
    settings: &FileSearchSettings,
    dir: &Path,
) -> anyhow::Result<()> {
    spawn_invocation(terminal_invocation(settings, dir)?)
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExplorerAction {
    OpenDirectory(PathBuf),
    RevealFile(PathBuf),
    Unsupported { path: PathBuf, reason: String },
}

pub fn resolve_explorer_action(path: &Path) -> anyhow::Result<ExplorerAction> {
    let metadata = std::fs::metadata(path)
        .with_context(|| format!("{} is missing or inaccessible", path.display()))?;
    if metadata.is_dir() {
        Ok(ExplorerAction::OpenDirectory(path.to_path_buf()))
    } else if metadata.is_file() {
        Ok(ExplorerAction::RevealFile(path.to_path_buf()))
    } else {
        Ok(ExplorerAction::Unsupported {
            path: path.to_path_buf(),
            reason: "unsupported filesystem object; expected a file or directory".to_string(),
        })
    }
}

pub fn execute_explorer_action(action: ExplorerAction) -> anyhow::Result<()> {
    match action {
        ExplorerAction::OpenDirectory(path) => open_path(&path),
        ExplorerAction::RevealFile(path) => reveal_path(&path),
        ExplorerAction::Unsupported { path, reason } => {
            Err(anyhow!("cannot open {}: {reason}", path.display()))
        }
    }
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
    return open_configured_terminal_in_directory(&FileSearchSettings::default(), dir);
}

pub fn legacy_open_terminal_in_directory(dir: &Path) -> anyhow::Result<()> {
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

    fn executable_settings(executable: &std::path::Path) -> FileSearchSettings {
        FileSearchSettings {
            preferred_editor_command: executable.display().to_string(),
            preferred_terminal_command: executable.display().to_string(),
            ..FileSearchSettings::default()
        }
    }

    #[test]
    fn placeholder_expansion_preserves_spaces_and_unicode() {
        let target = InvocationTarget {
            file: Path::new("/tmp/space dir/雪 file.txt"),
            line: Some(12),
            column: Some(4),
        };
        let args = expand_invocation_template(
            &["--goto '{file}:{line}:{column}' --parent {parent}".to_string()],
            &target,
        )
        .unwrap();
        assert_eq!(
            args,
            vec![
                "--goto",
                "/tmp/space dir/雪 file.txt:12:4",
                "--parent",
                "/tmp/space dir"
            ]
        );
    }

    #[test]
    fn missing_configured_editor_is_rejected() {
        let settings = FileSearchSettings {
            preferred_editor_command: "/definitely/missing/editor".to_string(),
            ..FileSearchSettings::default()
        };
        let err = editor_invocation(
            &settings,
            InvocationTarget {
                file: Path::new("/tmp/a.txt"),
                line: None,
                column: None,
            },
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("configured editor executable"));
    }

    #[test]
    fn file_result_editor_arguments_use_file_placeholders() {
        let exe = std::env::current_exe().unwrap();
        let mut settings = executable_settings(&exe);
        settings.preferred_editor_args = vec!["--open".into(), "{file}".into()];
        let invocation = editor_invocation(
            &settings,
            InvocationTarget {
                file: Path::new("/tmp/project/main.rs"),
                line: None,
                column: None,
            },
        )
        .unwrap();
        assert_eq!(invocation.args, vec!["--open", "/tmp/project/main.rs"]);
    }

    #[test]
    fn content_result_editor_arguments_include_line_and_column() {
        let exe = std::env::current_exe().unwrap();
        let mut settings = executable_settings(&exe);
        settings.preferred_editor_args = vec!["--goto".into(), "{file}:{line}:{column}".into()];
        let invocation = editor_invocation(
            &settings,
            InvocationTarget {
                file: Path::new("/tmp/project/main.rs"),
                line: Some(42),
                column: Some(9),
            },
        )
        .unwrap();
        assert_eq!(invocation.args, vec!["--goto", "/tmp/project/main.rs:42:9"]);
    }

    #[test]
    fn configured_terminal_uses_containing_directory_as_working_directory() {
        let exe = std::env::current_exe().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let mut settings = executable_settings(&exe);
        settings.preferred_terminal_args = vec!["--cwd".into(), "{file}".into()];
        let invocation = terminal_invocation(&settings, temp.path()).unwrap();
        assert_eq!(invocation.working_dir.as_deref(), Some(temp.path()));
        assert_eq!(
            invocation.args,
            vec!["--cwd".to_string(), temp.path().display().to_string()]
        );
    }
    #[test]
    fn file_resolves_to_reveal() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("space 雪.txt");
        std::fs::write(&path, "needle").unwrap();

        assert_eq!(
            resolve_explorer_action(&path).unwrap(),
            ExplorerAction::RevealFile(path)
        );
    }

    #[test]
    fn directory_resolves_to_open() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("dir 雪");
        std::fs::create_dir(&path).unwrap();

        assert_eq!(
            resolve_explorer_action(&path).unwrap(),
            ExplorerAction::OpenDirectory(path)
        );
    }

    #[test]
    fn missing_path_returns_error() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("missing.txt");

        let err = resolve_explorer_action(&path).unwrap_err().to_string();

        assert!(err.contains("missing or inaccessible"));
        assert!(err.contains("missing.txt"));
    }

    #[cfg(unix)]
    #[test]
    fn unsupported_entry_returns_clear_result() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("pipe");
        let status = Command::new("mkfifo")
            .arg(&path)
            .status()
            .expect("run mkfifo for unsupported-entry coverage");
        assert!(status.success(), "mkfifo should create a FIFO");

        match resolve_explorer_action(&path).unwrap() {
            ExplorerAction::Unsupported {
                path: actual,
                reason,
            } => {
                assert_eq!(actual, path);
                assert!(reason.contains("unsupported filesystem object"));
            }
            other => panic!("expected unsupported action, got {other:?}"),
        }
    }

    #[test]
    fn windows_reveal_args_preserves_spaces_and_unicode_in_one_argument() {
        let args = windows_reveal_args(Path::new(r"C:\Users\alice\space dir\雪 file.txt"));

        assert_eq!(args.len(), 1);
        assert_eq!(args[0], r"/select,C:\Users\alice\space dir\雪 file.txt");
    }
}
