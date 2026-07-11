use crate::file_search::coordinator::{CancellationToken, SearchExecutor};
use crate::file_search::error::FileSearchError;
use crate::file_search::model::{
    ContentFileResult, ContentFileResultBuilder, ContentMatch, SearchEvent, SearchId, SearchKind,
    SearchProgress, SearchRequest, SearchResult, SearchScope, SearchStatus,
};
use crate::file_search::settings::FileSearchSettings;
use serde_json::Value;
use std::collections::BTreeMap;
use std::env;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct RipgrepSearchExecutor {
    settings: FileSearchSettings,
}

impl RipgrepSearchExecutor {
    pub fn new(settings: FileSearchSettings) -> Self {
        Self { settings }
    }
}

impl SearchExecutor for RipgrepSearchExecutor {
    fn execute(
        &self,
        id: SearchId,
        request: SearchRequest,
        token: CancellationToken,
        events: mpsc::Sender<SearchEvent>,
    ) {
        match search_content_with_ripgrep(request, &self.settings, &token) {
            Ok(summary) => {
                for result in summary.results {
                    if events
                        .send(SearchEvent::Result {
                            id,
                            result: SearchResult::ContentFile(result),
                        })
                        .is_err()
                    {
                        return;
                    }
                }
                let status = if summary.cancelled {
                    SearchStatus::Cancelled
                } else {
                    SearchStatus::Completed
                };
                let _ = events.send(SearchEvent::Progress {
                    id,
                    progress: SearchProgress {
                        files_scanned: summary.files_scanned,
                        directories_scanned: 0,
                        results_found: summary.results_found,
                        status,
                    },
                });
                let _ = if summary.cancelled {
                    events.send(SearchEvent::Cancelled { id })
                } else {
                    events.send(SearchEvent::Completed { id })
                };
            }
            Err(error) => {
                let _ = events.send(SearchEvent::Failed {
                    id,
                    error: error.to_string(),
                });
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RipgrepSearchSummary {
    pub results: Vec<ContentFileResult>,
    pub results_found: usize,
    pub files_scanned: u64,
    pub cancelled: bool,
    pub stderr: String,
}

pub fn search_content_with_ripgrep(
    request: SearchRequest,
    settings: &FileSearchSettings,
    cancellation: &CancellationToken,
) -> Result<RipgrepSearchSummary, FileSearchError> {
    if request.kind != SearchKind::Content {
        return Err(FileSearchError::InvalidQuery {
            message: "ripgrep backend only supports content searches".to_owned(),
        });
    }
    if request.text.is_empty() {
        return Err(FileSearchError::InvalidQuery {
            message: "content search query cannot be empty".to_owned(),
        });
    }

    let executable = detect_ripgrep_executable(&settings.ripgrep_executable_path)?;
    let roots = search_roots(&request, settings)?;
    let mut command = build_ripgrep_command(&executable, &request, settings, &roots);
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| FileSearchError::ProcessLaunchFailure {
            executable: executable.clone(),
            message: error.to_string(),
        })?;

    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");
    let stdout_handle = thread::spawn(move || read_to_string(stdout));
    let stderr_handle = thread::spawn(move || read_to_string(stderr));
    let mut cancelled = false;
    let status = loop {
        if cancellation.is_cancelled() {
            cancelled = true;
            let _ = child.kill();
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(error) => {
                let _ = child.kill();
                return Err(FileSearchError::ProcessLaunchFailure {
                    executable: executable.clone(),
                    message: error.to_string(),
                });
            }
        }
    };
    let _ = child.wait();

    let stdout = stdout_handle.join().unwrap_or_default();
    let stderr = stderr_handle.join().unwrap_or_default();
    let per_file_match_limit = per_file_match_limit(&request, settings);
    if cancelled {
        let mut summary =
            parse_ripgrep_json_limited(&stdout, per_file_match_limit, request.max_results)?;
        summary.cancelled = true;
        summary.stderr = stderr;
        return Ok(summary);
    }
    handle_exit_status(status, &stderr)?;
    let mut summary =
        parse_ripgrep_json_limited(&stdout, per_file_match_limit, request.max_results)?;
    summary.stderr = stderr;
    Ok(summary)
}

fn per_file_match_limit(request: &SearchRequest, settings: &FileSearchSettings) -> usize {
    match &request.scope {
        SearchScope::File { .. } => usize::MAX,
        _ => settings.max_matches_per_content_file,
    }
}

pub fn detect_ripgrep_executable(configured: &Path) -> Result<PathBuf, FileSearchError> {
    if !configured.as_os_str().is_empty() && configured.components().count() > 1 {
        if configured.is_file() {
            return Ok(configured.to_path_buf());
        }
        return Err(FileSearchError::BackendUnavailable {
            backend: "ripgrep".to_owned(),
            message: format!(
                "configured executable '{}' was not found",
                configured.display()
            ),
        });
    }
    let name = if configured.as_os_str().is_empty() {
        "rg".into()
    } else {
        configured.to_path_buf()
    };
    find_on_path(&name).ok_or_else(|| FileSearchError::BackendUnavailable {
        backend: "ripgrep".to_owned(),
        message: "ripgrep executable was not found in PATH".to_owned(),
    })
}

fn find_on_path(name: &Path) -> Option<PathBuf> {
    let paths = env::var_os("PATH")?;
    for dir in env::split_paths(&paths) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            let exe = candidate.with_extension("exe");
            if exe.is_file() {
                return Some(exe);
            }
        }
    }
    None
}

pub fn build_ripgrep_command(
    executable: &Path,
    request: &SearchRequest,
    settings: &FileSearchSettings,
    roots: &[PathBuf],
) -> Command {
    let mut command = Command::new(executable);
    command.arg("--json").arg("--fixed-strings");
    if request.case_sensitive {
        command.arg("--case-sensitive");
    } else {
        command.arg("--ignore-case");
    }
    if request.include_hidden_files {
        command.arg("--hidden");
    }
    command
        .arg("--max-filesize")
        .arg(request.max_file_size_bytes.to_string());
    for ext in &request.included_extensions {
        command
            .arg("--glob")
            .arg(format!("*.{}", normalize_ext(ext)));
    }
    for ext in &request.excluded_extensions {
        command
            .arg("--glob")
            .arg(format!("!*.{}", normalize_ext(ext)));
    }
    for dir in excluded_directory_names(request, settings) {
        command.arg("--glob").arg(format!("!**/{dir}/**"));
    }
    command.arg("--").arg(&request.text);
    for root in roots {
        command.arg(root);
    }
    command
}

fn normalize_ext(ext: &str) -> &str {
    ext.trim_start_matches('.')
}

fn excluded_directory_names(request: &SearchRequest, settings: &FileSearchSettings) -> Vec<String> {
    let mut names = settings.excluded_directory_names.clone();
    names.extend(request.excluded_directory_names.clone());
    names.sort();
    names.dedup();
    names
}

fn search_roots(
    request: &SearchRequest,
    settings: &FileSearchSettings,
) -> Result<Vec<PathBuf>, FileSearchError> {
    let roots = match &request.scope {
        SearchScope::Directory { root } => vec![root.clone()],
        SearchScope::File { path } => vec![path.clone()],
        SearchScope::Global => settings.global_content_search_roots.clone(),
    };
    for root in &roots {
        if !(root.is_dir() || root.is_file()) {
            return Err(FileSearchError::InvalidDirectory {
                path: root.clone(),
                message: "path is not a directory or file".to_owned(),
            });
        }
    }
    Ok(roots)
}

fn read_to_string(reader: impl Read) -> String {
    let mut output = String::new();
    let mut reader = BufReader::new(reader);
    let _ = reader.read_to_string(&mut output);
    output
}

fn handle_exit_status(status: ExitStatus, stderr: &str) -> Result<(), FileSearchError> {
    match status.code() {
        Some(0) | Some(1) => Ok(()),
        Some(2) if is_non_fatal_ripgrep_stderr(stderr) => Ok(()),
        _ => Err(FileSearchError::ProcessLaunchFailure {
            executable: PathBuf::from("rg"),
            message: if stderr.trim().is_empty() {
                format!("ripgrep exited with status {status}")
            } else {
                stderr.trim().to_owned()
            },
        }),
    }
}

fn is_non_fatal_ripgrep_stderr(stderr: &str) -> bool {
    let lower = stderr.to_lowercase();
    lower.contains("permission denied")
        || lower.contains("access is denied")
        || lower.contains("no such file")
        || lower.contains("not found")
}

pub fn parse_ripgrep_json(
    output: &str,
    max_matches_per_file: usize,
) -> Result<RipgrepSearchSummary, FileSearchError> {
    parse_ripgrep_json_limited(output, max_matches_per_file, usize::MAX)
}

fn parse_ripgrep_json_limited(
    output: &str,
    max_matches_per_file: usize,
    max_result_files: usize,
) -> Result<RipgrepSearchSummary, FileSearchError> {
    let mut files: BTreeMap<PathBuf, ContentFileResultBuilder> = BTreeMap::new();
    let mut files_scanned = 0_u64;
    for line in output.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(line).map_err(|error| {
            FileSearchError::ProcessOutputParseFailure {
                backend: "ripgrep".to_owned(),
                message: error.to_string(),
            }
        })?;
        match value.get("type").and_then(Value::as_str) {
            Some("match") => parse_match_event(&value, &mut files, max_matches_per_file)?,
            Some("summary") => {
                if let Some(searched) = value
                    .pointer("/data/stats/searches")
                    .and_then(Value::as_u64)
                {
                    files_scanned = searched;
                }
            }
            Some("begin") | Some("end") | Some("context") => {}
            _ => {}
        }
    }
    let results: Vec<_> = files
        .into_iter()
        .take(max_result_files)
        .map(|(_path, builder)| builder.finish())
        .collect();
    Ok(RipgrepSearchSummary {
        results_found: results.len(),
        files_scanned,
        results,
        cancelled: false,
        stderr: String::new(),
    })
}

fn parse_match_event(
    value: &Value,
    files: &mut BTreeMap<PathBuf, ContentFileResultBuilder>,
    max_matches_per_file: usize,
) -> Result<(), FileSearchError> {
    let data = &value["data"];
    let path = data
        .pointer("/path/text")
        .and_then(Value::as_str)
        .ok_or_else(|| FileSearchError::ProcessOutputParseFailure {
            backend: "ripgrep".to_owned(),
            message: "match event missing path text".to_owned(),
        })?;
    let line = data
        .pointer("/lines/text")
        .and_then(Value::as_str)
        .unwrap_or("");
    let line_number = data.get("line_number").and_then(Value::as_u64).unwrap_or(0) as usize;
    let normalized_line = line.trim_end_matches(['\r', '\n']).to_owned();
    let entry = files.entry(PathBuf::from(path)).or_insert_with(|| {
        ContentFileResultBuilder::new(PathBuf::from(path), max_matches_per_file)
    });
    if let Some(submatches) = data.get("submatches").and_then(Value::as_array) {
        for submatch in submatches {
            let byte_start = submatch.get("start").and_then(Value::as_u64).unwrap_or(0) as usize;
            let byte_end = submatch.get("end").and_then(Value::as_u64).unwrap_or(0) as usize;
            entry.push_match(ContentMatch::new(
                line_number,
                normalized_line.clone(),
                byte_start,
                byte_end,
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn req(root: PathBuf) -> SearchRequest {
        SearchRequest {
            kind: SearchKind::Content,
            scope: SearchScope::Directory { root },
            text: "needle".to_owned(),
            case_sensitive: false,
            include_hidden_files: false,
            max_results: 10,
            max_file_size_bytes: 1024,
            included_extensions: vec![],
            excluded_extensions: vec![],
            excluded_directory_names: vec![],
        }
    }

    #[test]
    fn parses_single_match() {
        let json = r#"{"type":"match","data":{"path":{"text":"a.txt"},"lines":{"text":"needle here\n"},"line_number":7,"submatches":[{"match":{"text":"needle"},"start":0,"end":6}]}}"#;
        let summary = parse_ripgrep_json(json, 25).unwrap();
        assert_eq!(summary.results[0].path, PathBuf::from("a.txt"));
        assert_eq!(summary.results[0].matches[0].line_number, 7);
    }

    #[test]
    fn parses_multiple_files() {
        let json = concat!(
            r#"{"type":"match","data":{"path":{"text":"a.txt"},"lines":{"text":"needle\n"},"line_number":1,"submatches":[{"start":0,"end":6}]}}"#,
            "\n",
            r#"{"type":"match","data":{"path":{"text":"b.txt"},"lines":{"text":"needle\n"},"line_number":2,"submatches":[{"start":0,"end":6}]}}"#
        );
        assert_eq!(parse_ripgrep_json(json, 25).unwrap().results.len(), 2);
    }

    #[test]
    fn parses_multiple_matches_in_one_file() {
        let json = r#"{"type":"match","data":{"path":{"text":"a.txt"},"lines":{"text":"needle needle\n"},"line_number":1,"submatches":[{"start":0,"end":6},{"start":7,"end":13}]}}"#;
        assert_eq!(
            parse_ripgrep_json(json, 25).unwrap().results[0]
                .matches
                .len(),
            2
        );
    }

    #[test]
    fn parses_unicode_paths() {
        let json = r#"{"type":"match","data":{"path":{"text":"資料/é.txt"},"lines":{"text":"needle\n"},"line_number":1,"submatches":[{"start":0,"end":6}]}}"#;
        assert_eq!(
            parse_ripgrep_json(json, 25).unwrap().results[0].path,
            PathBuf::from("資料/é.txt")
        );
    }

    #[test]
    fn malformed_json_is_error() {
        assert!(parse_ripgrep_json("{not json}", 25).is_err());
    }

    #[test]
    fn no_match_completion_is_empty_success() {
        let summary =
            parse_ripgrep_json(r#"{"type":"summary","data":{"stats":{"searches":3}}}"#, 25)
                .unwrap();
        assert!(summary.results.is_empty());
        assert_eq!(summary.files_scanned, 3);
    }

    #[test]
    fn backend_errors_are_distinct_from_no_matches() {
        assert!(handle_exit_status(fake_status(1), "").is_ok());
        assert!(handle_exit_status(fake_status(2), "unrecognized flag").is_err());
        assert!(handle_exit_status(fake_status(2), "Permission denied").is_ok());
    }

    #[cfg(unix)]
    fn fake_status(code: i32) -> ExitStatus {
        use std::os::unix::process::ExitStatusExt;
        ExitStatus::from_raw(code << 8)
    }

    #[cfg(windows)]
    fn fake_status(code: i32) -> ExitStatus {
        use std::os::windows::process::ExitStatusExt;
        ExitStatus::from_raw(code as u32)
    }

    #[test]
    fn truncates_match_storage() {
        let json = r#"{"type":"match","data":{"path":{"text":"a.txt"},"lines":{"text":"needle needle\n"},"line_number":1,"submatches":[{"start":0,"end":6},{"start":7,"end":13}]}}"#;
        assert_eq!(
            parse_ripgrep_json(json, 1).unwrap().results[0]
                .matches
                .len(),
            1
        );
    }

    #[test]
    fn grouped_results_count_total_matches_past_limit_and_truncate() {
        let json = r#"{"type":"match","data":{"path":{"text":"a.txt"},"lines":{"text":"needle needle\n"},"line_number":1,"submatches":[{"start":0,"end":6},{"start":7,"end":13}]}}"#;
        let result = parse_ripgrep_json(json, 1).unwrap().results.remove(0);
        assert_eq!(result.total_matches, 2);
        assert_eq!(result.matches.len(), 1);
        assert!(result.truncated);
    }

    #[test]
    fn grouped_results_keep_same_filename_in_different_directories_separate() {
        let json = concat!(
            r#"{"type":"match","data":{"path":{"text":"one/a.txt"},"lines":{"text":"needle\n"},"line_number":1,"submatches":[{"start":0,"end":6}]}}"#,
            "\n",
            r#"{"type":"match","data":{"path":{"text":"two/a.txt"},"lines":{"text":"needle\n"},"line_number":2,"submatches":[{"start":0,"end":6}]}}"#
        );
        let summary = parse_ripgrep_json(json, 25).unwrap();
        assert_eq!(summary.results.len(), 2);
        assert!(
            summary
                .results
                .iter()
                .any(|r| r.path == PathBuf::from("one/a.txt"))
        );
        assert!(
            summary
                .results
                .iter()
                .any(|r| r.path == PathBuf::from("two/a.txt"))
        );
    }

    #[test]
    fn query_starting_with_dash_is_after_separator() {
        let temp = tempfile::tempdir().unwrap();
        let request = SearchRequest {
            text: "-needle".to_owned(),
            ..req(temp.path().to_path_buf())
        };
        let command = build_ripgrep_command(
            Path::new("rg"),
            &request,
            &FileSearchSettings::default(),
            &[temp.path().to_path_buf()],
        );
        let args: Vec<_> = command
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        let sep = args.iter().position(|arg| arg == "--").unwrap();
        assert_eq!(args[sep + 1], "-needle");
    }

    #[test]
    fn optional_integration_runs_when_rg_is_detected() {
        let Ok(executable) = detect_ripgrep_executable(Path::new("rg")) else {
            return;
        };
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("file.txt"), "hello Needle\n").unwrap();
        let settings = FileSearchSettings {
            ripgrep_executable_path: executable,
            ..FileSearchSettings::default()
        };
        let (_tx, _rx) = mpsc::channel::<SearchEvent>();
        let summary = search_content_with_ripgrep(
            req(temp.path().to_path_buf()),
            &settings,
            &CancellationToken::new(),
        )
        .unwrap();
        assert_eq!(summary.results.len(), 1);
    }
}
