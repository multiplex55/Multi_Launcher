use std::path::{Path, PathBuf};

use crate::file_search::error::FileSearchError;
use crate::file_search::model::SearchKind;

/// Parsed command intent for the File Search launcher query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileSearchCommand {
    OpenWindow,
    OpenWindowWithMode {
        kind: SearchKind,
    },
    RequestDirectory {
        kind: SearchKind,
        search_text: String,
    },
    StartSearch(SearchRequestDraft),
    Error(FileSearchError),
}

/// User-provided search request before settings-derived options are applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchRequestDraft {
    pub kind: SearchKind,
    pub root: Option<PathBuf>,
    pub search_text: String,
}

/// Parses File Search launcher queries.
///
/// Returns `None` when the first token is not `fs` (case-insensitive).
pub fn parse_file_search_query(query: &str) -> Option<FileSearchCommand> {
    let tokens = match shlex::split(query) {
        Some(tokens) => tokens,
        None => {
            return Some(FileSearchCommand::Error(FileSearchError::InvalidQuery {
                message: "Could not parse quoted file search query".to_string(),
            }));
        }
    };

    let (first, rest) = tokens.split_first()?;
    if !first.eq_ignore_ascii_case("fs") {
        return None;
    }

    if rest.is_empty() {
        return Some(FileSearchCommand::OpenWindow);
    }

    let mut index = 0;
    let directory_scoped = rest[index].eq_ignore_ascii_case("here");
    if directory_scoped {
        index += 1;
    }

    let Some(kind) = rest.get(index).and_then(|token| parse_kind(token)) else {
        return Some(FileSearchCommand::OpenWindow);
    };
    index += 1;

    if index == rest.len() {
        return Some(FileSearchCommand::OpenWindowWithMode { kind });
    }

    let mut args = rest[index..].to_vec();
    let root = match take_trailing_directory(&mut args) {
        Ok(root) => root,
        Err(error) => return Some(FileSearchCommand::Error(error)),
    };
    let search_text = args.join(" ");

    if directory_scoped && root.is_none() {
        return Some(FileSearchCommand::RequestDirectory { kind, search_text });
    }

    Some(FileSearchCommand::StartSearch(SearchRequestDraft {
        kind,
        root,
        search_text,
    }))
}

fn parse_kind(token: &str) -> Option<SearchKind> {
    match token.to_ascii_lowercase().as_str() {
        "file" | "filename" | "name" => Some(SearchKind::Filename),
        "content" | "contents" | "text" => Some(SearchKind::Content),
        _ => None,
    }
}

fn take_trailing_directory(args: &mut Vec<String>) -> Result<Option<PathBuf>, FileSearchError> {
    let Some(last) = args.last() else {
        return Ok(None);
    };
    let path = Path::new(last);
    if path.is_dir() {
        let root = PathBuf::from(last);
        args.pop();
        return Ok(Some(root));
    }

    if is_path_like(last) {
        return Err(FileSearchError::InvalidDirectory {
            path: PathBuf::from(last),
            message: "The trailing path does not exist or is not a directory".to_string(),
        });
    }

    Ok(None)
}

fn is_path_like(value: &str) -> bool {
    if value.starts_with("./")
        || value.starts_with("../")
        || value.starts_with('/')
        || value.starts_with('~')
        || value.starts_with(r"\\")
        || value.contains('/')
        || value.contains('\\')
    {
        return true;
    }

    let bytes = value.as_bytes();
    bytes.len() >= 3
        && bytes[1] == b':'
        && (bytes[2] == b'/' || bytes[2] == b'\\')
        && bytes[0].is_ascii_alphabetic()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(query: &str) -> FileSearchCommand {
        parse_file_search_query(query).expect("query should be recognized as file search")
    }

    #[test]
    fn empty_fs_opens_window() {
        assert_eq!(parse("fs"), FileSearchCommand::OpenWindow);
        assert_eq!(parse("FS"), FileSearchCommand::OpenWindow);
    }

    #[test]
    fn filename_mode_opens_window_with_mode() {
        assert_eq!(
            parse("fs file"),
            FileSearchCommand::OpenWindowWithMode {
                kind: SearchKind::Filename
            }
        );
    }

    #[test]
    fn content_mode_opens_window_with_mode() {
        assert_eq!(
            parse("fs content"),
            FileSearchCommand::OpenWindowWithMode {
                kind: SearchKind::Content
            }
        );
    }

    #[test]
    fn here_without_root_requests_directory() {
        assert_eq!(
            parse("fs here file README"),
            FileSearchCommand::RequestDirectory {
                kind: SearchKind::Filename,
                search_text: "README".to_string()
            }
        );
    }

    #[test]
    fn aliases_are_recognized() {
        assert!(matches!(
            parse("fs filename readme"),
            FileSearchCommand::StartSearch(SearchRequestDraft {
                kind: SearchKind::Filename,
                ..
            })
        ));
        assert!(matches!(
            parse("fs name readme"),
            FileSearchCommand::StartSearch(SearchRequestDraft {
                kind: SearchKind::Filename,
                ..
            })
        ));
        assert!(matches!(
            parse("fs contents needle"),
            FileSearchCommand::StartSearch(SearchRequestDraft {
                kind: SearchKind::Content,
                ..
            })
        ));
        assert!(matches!(
            parse("fs text needle"),
            FileSearchCommand::StartSearch(SearchRequestDraft {
                kind: SearchKind::Content,
                ..
            })
        ));
    }

    #[test]
    fn quoted_search_terms_are_preserved() {
        assert_eq!(
            parse("fs file \"README file\""),
            FileSearchCommand::StartSearch(SearchRequestDraft {
                kind: SearchKind::Filename,
                root: None,
                search_text: "README file".to_string(),
            })
        );
    }

    #[test]
    fn quoted_windows_paths_preserve_backslashes() {
        assert!(matches!(
            parse(r#"fs content "launch_action" "D:\Projects\multi launcher""#),
            FileSearchCommand::Error(FileSearchError::InvalidDirectory { path, .. })
                if path == PathBuf::from(r"D:\Projects\multi launcher")
        ));
    }

    #[test]
    fn unc_paths_are_path_like_and_invalid_when_missing() {
        assert!(matches!(
            parse(r#"fs file README "\\server\share""#),
            FileSearchCommand::Error(FileSearchError::InvalidDirectory { path, .. })
                if path == PathBuf::from(r"\\server\share")
        ));
    }

    #[test]
    fn paths_containing_spaces_can_be_roots() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root with spaces");
        std::fs::create_dir(&root).unwrap();
        let query = format!("fs file README {:?}", root.display().to_string());

        assert_eq!(
            parse(&query),
            FileSearchCommand::StartSearch(SearchRequestDraft {
                kind: SearchKind::Filename,
                root: Some(root),
                search_text: "README".to_string(),
            })
        );
    }

    #[test]
    fn search_text_beginning_with_dash_is_not_an_external_argument() {
        assert_eq!(
            parse("fs file -README"),
            FileSearchCommand::StartSearch(SearchRequestDraft {
                kind: SearchKind::Filename,
                root: None,
                search_text: "-README".to_string(),
            })
        );
    }

    #[test]
    fn content_containing_colons_is_not_treated_as_path() {
        assert_eq!(
            parse("fs content key:value"),
            FileSearchCommand::StartSearch(SearchRequestDraft {
                kind: SearchKind::Content,
                root: None,
                search_text: "key:value".to_string(),
            })
        );
    }

    #[test]
    fn missing_directory_root_for_here_requests_directory() {
        assert_eq!(
            parse("fs here content launch_action"),
            FileSearchCommand::RequestDirectory {
                kind: SearchKind::Content,
                search_text: "launch_action".to_string(),
            }
        );
    }

    #[test]
    fn invalid_directory_roots_return_clear_error() {
        assert!(matches!(
            parse("fs file README ./definitely-not-a-directory"),
            FileSearchCommand::Error(FileSearchError::InvalidDirectory { path, message })
                if path == PathBuf::from("./definitely-not-a-directory")
                    && message.contains("does not exist")
        ));
    }
}
