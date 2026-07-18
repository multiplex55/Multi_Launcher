use crate::file_search::coordinator::{CancellationToken, SearchExecutor};
pub use crate::file_search::discovery::{
    ExecutableSearchContext, detect_ripgrep_executable, resolve_ripgrep_executable,
    resolve_ripgrep_with_context,
};
use crate::file_search::error::FileSearchError;
use crate::file_search::matching::filename_highlight_match;
use crate::file_search::model::{
    BackendExecutionDetails, ContentFileResult, ContentFileResultBuilder, ContentMatch, FileKind,
    FileTypeFilter, FilenameResult, SearchBackend, SearchDiagnostic, SearchEvent, SearchId,
    SearchKind, SearchProgress, SearchRequest, SearchResult, SearchScope, SearchStatus,
};
use crate::file_search::settings::FileSearchSettings;
use crate::process::configure_background_command;
use serde_json::Value;
use std::collections::HashSet;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant, SystemTime};

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
        let result = match request.kind {
            SearchKind::Content => {
                execute_content_search(id, request, &self.settings, &token, &events)
            }
            SearchKind::Filename => {
                execute_filename_search(id, request, &self.settings, &token, &events)
            }
        };
        if let Err(error) = result {
            let _ = events.send(SearchEvent::Failed {
                id,
                error: error.to_string(),
            });
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
    pub global_truncated: bool,
    pub execution_details: Option<BackendExecutionDetails>,
}

fn execute_content_search(
    id: SearchId,
    request: SearchRequest,
    settings: &FileSearchSettings,
    token: &CancellationToken,
    events: &mpsc::Sender<SearchEvent>,
) -> Result<(), FileSearchError> {
    let summary = search_content_with_ripgrep(request, settings, token)?;
    if let Some(details) = summary.execution_details.clone() {
        let _ = events.send(SearchEvent::Diagnostic {
            id,
            diagnostic: SearchDiagnostic::BackendExecution(Box::new(details)),
        });
    }
    send_bounded_stderr_diagnostic(id, &summary.stderr, events);
    for result in summary.results {
        if events
            .send(SearchEvent::Result {
                id,
                result: SearchResult::ContentFile(result),
            })
            .is_err()
        {
            return Ok(());
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
            global_truncated: summary.global_truncated,
        },
    });
    let _ = if summary.cancelled {
        events.send(SearchEvent::Cancelled { id })
    } else {
        events.send(SearchEvent::Completed { id })
    };
    Ok(())
}

fn execute_filename_search(
    id: SearchId,
    request: SearchRequest,
    settings: &FileSearchSettings,
    token: &CancellationToken,
    events: &mpsc::Sender<SearchEvent>,
) -> Result<(), FileSearchError> {
    let summary = search_filenames_with_ripgrep(request, settings, token)?;
    if let Some(details) = summary.execution_details.clone() {
        let _ = events.send(SearchEvent::Diagnostic {
            id,
            diagnostic: SearchDiagnostic::BackendExecution(Box::new(details)),
        });
    }
    send_bounded_stderr_diagnostic(id, &summary.stderr, events);
    for result in summary.results {
        if events
            .send(SearchEvent::Result {
                id,
                result: SearchResult::Filename(result),
            })
            .is_err()
        {
            return Ok(());
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
            global_truncated: summary.global_truncated,
        },
    });
    let _ = if summary.cancelled {
        events.send(SearchEvent::Cancelled { id })
    } else {
        events.send(SearchEvent::Completed { id })
    };
    Ok(())
}

fn send_bounded_stderr_diagnostic(id: SearchId, stderr: &str, events: &mpsc::Sender<SearchEvent>) {
    if stderr.trim().is_empty() {
        return;
    }
    let snippet: String = stderr.chars().take(4096).collect();
    let _ = events.send(SearchEvent::Diagnostic {
        id,
        diagnostic: SearchDiagnostic::BackendStderr(snippet),
    });
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RipgrepFilenameSearchSummary {
    pub results: Vec<FilenameResult>,
    pub results_found: usize,
    pub files_scanned: u64,
    pub cancelled: bool,
    pub stderr: String,
    pub global_truncated: bool,
    pub execution_details: Option<BackendExecutionDetails>,
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

    let resolution = resolve_ripgrep_with_context(
        &settings.ripgrep_executable_path,
        &ExecutableSearchContext::from_process(),
    )?;
    let executable = resolution.path.clone();
    tracing::debug!(executable = %executable.display(), "starting ripgrep content search");
    let roots = search_roots(&request, settings)?;
    let mut command = build_ripgrep_command(&executable, &request, settings, &roots);
    let started_at = SystemTime::now();
    let started = Instant::now();
    let command_for_display = command_to_string(&command, false);
    let command_without_query = command_to_string(&command, true);
    let mut child = spawn_ripgrep_child(&mut command, &executable)?;
    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");
    let stderr_handle = thread::spawn(move || read_bounded_to_string(stderr, STDERR_CAPTURE_LIMIT));
    let per_file_match_limit = per_file_match_limit(&request, settings);
    let stdout_handle = thread::spawn(move || {
        parse_ripgrep_json_reader(
            BufReader::new(stdout),
            per_file_match_limit,
            request.max_results,
        )
    });
    let process = wait_for_child(&mut child, cancellation, None);
    let mut summary = stdout_handle.join().unwrap_or_else(|_| {
        Err(FileSearchError::ProcessOutputParseFailure {
            backend: "ripgrep".to_owned(),
            message: "stdout reader thread panicked".to_owned(),
        })
    })?;
    let stderr = stderr_handle.join().unwrap_or_default();
    summary.cancelled = process.cancelled;
    summary.stderr = stderr.clone();
    summary.execution_details = Some(BackendExecutionDetails {
        backend: SearchBackend::Ripgrep,
        executable_path: Some(executable.clone()),
        version: resolution.version,
        resolution_source: Some(format!("{:?}", resolution.source)),
        command_for_display: Some(command_for_display),
        command_without_query: Some(command_without_query),
        search_roots: roots,
        started_at,
        ended_at: started_at.checked_add(started.elapsed()),
        stderr: if stderr.trim().is_empty() {
            None
        } else {
            Some(stderr.clone())
        },
        fallback_reason: None,
        cancelled: process.cancelled,
    });
    if !process.cancelled
        && let Some(status) = process.status
    {
        handle_exit_status(status, &executable, &stderr)?;
    }
    Ok(summary)
}

#[derive(Debug)]
struct ChildProcessResult {
    status: Option<ExitStatus>,
    cancelled: bool,
}

const STDERR_CAPTURE_LIMIT: usize = 64 * 1024;

fn spawn_ripgrep_child(command: &mut Command, executable: &Path) -> Result<Child, FileSearchError> {
    command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| FileSearchError::ProcessLaunchFailure {
            executable: executable.to_path_buf(),
            message: error.to_string(),
        })
}

fn wait_for_child(
    child: &mut Child,
    cancellation: &CancellationToken,
    stop: Option<&mpsc::Receiver<()>>,
) -> ChildProcessResult {
    let mut killed = false;
    let mut cancelled = false;
    loop {
        if cancellation.is_cancelled() || stop.is_some_and(|rx| rx.try_recv().is_ok()) {
            cancelled = cancellation.is_cancelled();
            if !killed {
                let _ = child.kill();
                killed = true;
            }
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                let _ = child.wait();
                return ChildProcessResult {
                    status: Some(status),
                    cancelled,
                };
            }
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(_) => {
                if !killed {
                    let _ = child.kill();
                }
                let status = child.wait().ok();
                return ChildProcessResult {
                    status,
                    cancelled: true,
                };
            }
        }
    }
}

fn read_bounded_to_string(reader: impl Read, limit: usize) -> String {
    let mut reader = BufReader::new(reader);
    let mut output = Vec::new();
    let mut buffer = [0_u8; 4096];
    while output.len() < limit {
        let remaining = limit - output.len();
        let read_len = remaining.min(buffer.len());
        match reader.read(&mut buffer[..read_len]) {
            Ok(0) | Err(_) => break,
            Ok(n) => output.extend_from_slice(&buffer[..n]),
        }
    }
    String::from_utf8_lossy(&output).to_string()
}

fn command_to_string(command: &Command, redact_query: bool) -> String {
    let program = shell_quote_arg(&command.get_program().to_string_lossy());
    let mut redact_next = false;
    let args = command.get_args().map(|arg| {
        let text = arg.to_string_lossy();
        if redact_next {
            redact_next = false;
            return shell_quote_arg("<query>");
        }
        if redact_query && text == "-e" {
            redact_next = true;
        }
        shell_quote_arg(&text)
    });
    std::iter::once(program)
        .chain(args)
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_quote_arg(arg: &str) -> String {
    if arg.is_empty() {
        return "''".to_string();
    }
    if arg.chars().all(|ch| {
        ch.is_ascii_alphanumeric() || matches!(ch, '/' | '\\' | '.' | '_' | '-' | ':' | '=')
    }) {
        return arg.to_string();
    }
    format!("'{}'", arg.replace('\'', "'\\''"))
}

pub fn search_filenames_with_ripgrep(
    request: SearchRequest,
    settings: &FileSearchSettings,
    cancellation: &CancellationToken,
) -> Result<RipgrepFilenameSearchSummary, FileSearchError> {
    if request.kind != SearchKind::Filename {
        return Err(FileSearchError::InvalidQuery {
            message: "ripgrep filename search only supports filename requests".to_owned(),
        });
    }
    if request.text.is_empty() {
        return Err(FileSearchError::InvalidQuery {
            message: "filename search query cannot be empty".to_owned(),
        });
    }

    let resolution = resolve_ripgrep_with_context(
        &settings.ripgrep_executable_path,
        &ExecutableSearchContext::from_process(),
    )?;
    let executable = resolution.path.clone();
    let roots = search_roots(&request, settings)?;
    let mut command = build_ripgrep_files_command(&executable, &request, settings, &roots);
    let started_at = SystemTime::now();
    let started = Instant::now();
    let command_for_display = command_to_string(&command, false);
    let command_without_query = command_to_string(&command, true);
    let mut child = spawn_ripgrep_child(&mut command, &executable)?;
    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");
    let stderr_handle = thread::spawn(move || read_bounded_to_string(stderr, STDERR_CAPTURE_LIMIT));
    let (line_tx, line_rx) = mpsc::channel();
    let stdout_handle = thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if line_tx.send(line).is_err() {
                break;
            }
        }
    });

    let needle = if request.case_sensitive {
        request.text.clone()
    } else {
        request.text.to_lowercase()
    };
    let mut summary = RipgrepFilenameSearchSummary::default();
    let mut results = Vec::new();
    let mut seen = HashSet::new();
    let (stop_tx, stop_rx) = mpsc::channel();

    loop {
        if cancellation.is_cancelled() {
            summary.cancelled = true;
            let _ = stop_tx.send(());
            break;
        }
        match line_rx.recv_timeout(Duration::from_millis(10)) {
            Ok(line) => {
                summary.files_scanned = summary.files_scanned.saturating_add(1);
                if let Some(result) =
                    filename_result_from_path(&line, &needle, &request, results.len())
                {
                    let identity = crate::file_search::model::PathIdentity::from_path(&result.path);
                    if seen.insert(identity) {
                        results.push(result);
                        summary.results_found += 1;
                        if results.len() >= request.max_results {
                            summary.global_truncated = true;
                            let _ = stop_tx.send(());
                            break;
                        }
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if child.try_wait().ok().flatten().is_some() {
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    let process = wait_for_child(&mut child, cancellation, Some(&stop_rx));
    let _ = stdout_handle.join();
    let stderr = stderr_handle.join().unwrap_or_default();
    summary.cancelled |= process.cancelled;
    summary.stderr = stderr.clone();
    summary.execution_details = Some(BackendExecutionDetails {
        backend: SearchBackend::Ripgrep,
        executable_path: Some(executable.clone()),
        version: resolution.version,
        resolution_source: Some(format!("{:?}", resolution.source)),
        command_for_display: Some(command_for_display),
        command_without_query: Some(command_without_query),
        search_roots: roots,
        started_at,
        ended_at: started_at.checked_add(started.elapsed()),
        stderr: if stderr.trim().is_empty() {
            None
        } else {
            Some(stderr.clone())
        },
        fallback_reason: None,
        cancelled: summary.cancelled,
    });
    if !summary.cancelled
        && let Some(status) = process.status
    {
        handle_exit_status(status, &executable, &stderr)?;
    }
    while let Ok(line) = line_rx.try_recv() {
        if results.len() >= request.max_results {
            break;
        }
        summary.files_scanned = summary.files_scanned.saturating_add(1);
        if let Some(result) = filename_result_from_path(&line, &needle, &request, results.len()) {
            let identity = crate::file_search::model::PathIdentity::from_path(&result.path);
            if seen.insert(identity) {
                results.push(result);
                summary.results_found += 1;
            }
        }
    }
    summary.results = results;
    Ok(summary)
}

fn per_file_match_limit(request: &SearchRequest, settings: &FileSearchSettings) -> usize {
    match &request.scope {
        SearchScope::Files { .. } => usize::MAX,
        _ => settings.max_matches_per_content_file,
    }
}

#[cfg(test)]
use crate::file_search::discovery::find_on_path;

pub fn build_ripgrep_command(
    executable: &Path,
    request: &SearchRequest,
    settings: &FileSearchSettings,
    roots: &[PathBuf],
) -> Command {
    let mut command = Command::new(executable);
    configure_background_command(&mut command);
    command.arg("--json").arg("--no-ignore");
    if request.case_sensitive {
        command.arg("--case-sensitive");
    } else {
        command.arg("--ignore-case");
    }
    if request.include_hidden_files {
        command.arg("--hidden");
    }
    if request.whole_word {
        command.arg("--word-regexp");
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
    command.arg("--fixed-strings");
    match request.content_match_mode {
        crate::file_search::model::ContentMatchMode::ExactPhrase => {
            command.arg("-e").arg(&request.text);
        }
        crate::file_search::model::ContentMatchMode::AnyTerm => {
            for term in normalized_terms(&request.text, request.case_sensitive) {
                command.arg("-e").arg(term);
            }
        }
    }
    command.arg("--");
    for root in roots {
        command.arg(root);
    }
    command
}

pub fn build_ripgrep_files_command(
    executable: &Path,
    request: &SearchRequest,
    settings: &FileSearchSettings,
    roots: &[PathBuf],
) -> Command {
    let mut command = Command::new(executable);
    configure_background_command(&mut command);
    command.arg("--files").arg("--no-ignore");
    if request.include_hidden_files {
        command.arg("--hidden");
    }
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
    for root in roots {
        command.arg(root);
    }
    command
}

fn normalize_ext(ext: &str) -> &str {
    ext.trim_start_matches('.')
}

fn excluded_directory_names(
    request: &SearchRequest,
    _settings: &FileSearchSettings,
) -> Vec<String> {
    let mut names = request.excluded_directory_names.clone();
    names.sort();
    names.dedup();
    names
}

fn normalized_terms(query: &str, case_sensitive: bool) -> Vec<String> {
    query
        .split_whitespace()
        .map(|term| {
            if case_sensitive {
                term.to_owned()
            } else {
                term.to_lowercase()
            }
        })
        .filter(|term| !term.is_empty())
        .collect()
}

fn filename_result_from_path(
    line: &str,
    needle: &str,
    request: &SearchRequest,
    arrival_index: usize,
) -> Option<FilenameResult> {
    let path = PathBuf::from(line);
    let metadata = path.metadata().ok();
    if !matches_file_type_filter(metadata.as_ref(), request.file_type_filter) {
        return None;
    }
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| line.to_owned());
    let highlight = filename_highlight_match(
        &file_name,
        &path,
        needle,
        request.case_sensitive,
        request.filename_match_mode,
    )?;
    let rank = highlight.rank;
    Some(FilenameResult {
        path: path.clone(),
        file_name: file_name.clone(),
        parent_directory: path.parent().map(Path::to_path_buf),
        kind: file_kind(metadata.as_ref()),
        size: metadata.as_ref().filter(|m| m.is_file()).map(|m| m.len()),
        modified: metadata.and_then(|m| m.modified().ok()),
        rank,
        match_quality: rank,
        filename_match_ranges: highlight.filename_match_ranges,
        path_match_ranges: highlight.path_match_ranges,
        arrival_index,
    })
}

fn matches_file_type_filter(metadata: Option<&std::fs::Metadata>, filter: FileTypeFilter) -> bool {
    match filter {
        FileTypeFilter::FilesOnly => metadata.is_some_and(|m| m.is_file()),
        FileTypeFilter::DirectoriesOnly => metadata.is_some_and(|m| m.is_dir()),
        FileTypeFilter::FilesAndDirectories => true,
    }
}

fn search_roots(
    request: &SearchRequest,
    settings: &FileSearchSettings,
) -> Result<Vec<PathBuf>, FileSearchError> {
    let roots = match &request.scope {
        SearchScope::Roots { roots } if roots.is_empty() => settings.global_search_roots.clone(),
        SearchScope::Roots { roots } => roots.clone(),
        SearchScope::Files { files } => files.clone(),
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

fn file_kind(metadata: Option<&std::fs::Metadata>) -> FileKind {
    match metadata {
        Some(metadata) if metadata.is_file() => FileKind::File,
        Some(metadata) if metadata.is_dir() => FileKind::Directory,
        _ => FileKind::Other,
    }
}

fn handle_exit_status(
    status: ExitStatus,
    executable: &Path,
    stderr: &str,
) -> Result<(), FileSearchError> {
    match status.code() {
        Some(0) | Some(1) => Ok(()),
        Some(2) if is_non_fatal_ripgrep_stderr(stderr) => Ok(()),
        _ => Err(FileSearchError::ProcessFatalStatus {
            executable: executable.to_path_buf(),
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
    parse_ripgrep_json_reader(output.as_bytes(), max_matches_per_file, max_result_files)
}

fn parse_ripgrep_json_reader<R: Read>(
    reader: R,
    max_matches_per_file: usize,
    max_result_files: usize,
) -> Result<RipgrepSearchSummary, FileSearchError> {
    let mut current: Option<(PathBuf, ContentFileResultBuilder)> = None;
    let mut results = Vec::new();
    let mut files_scanned = 0_u64;
    let mut global_truncated = false;
    for line in BufReader::new(reader).lines() {
        let line = line.map_err(|error| FileSearchError::ProcessOutputParseFailure {
            backend: "ripgrep".to_owned(),
            message: error.to_string(),
        })?;
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(&line).map_err(|error| {
            FileSearchError::ProcessOutputParseFailure {
                backend: "ripgrep".to_owned(),
                message: error.to_string(),
            }
        })?;
        match value.get("type").and_then(Value::as_str) {
            Some("begin") => {
                if let Some(path) = event_path(&value) {
                    if let Some((_path, builder)) = current.take() {
                        push_content_result(
                            builder,
                            &mut results,
                            max_result_files,
                            &mut global_truncated,
                        );
                    }
                    current = Some((
                        path.clone(),
                        ContentFileResultBuilder::new(path, max_matches_per_file),
                    ));
                }
            }
            Some("match") => {
                let path = event_path(&value).ok_or_else(|| {
                    FileSearchError::ProcessOutputParseFailure {
                        backend: "ripgrep".to_owned(),
                        message: "match event missing path text".to_owned(),
                    }
                })?;
                if current.as_ref().map(|(p, _)| p != &path).unwrap_or(true) {
                    if let Some((_path, builder)) = current.take() {
                        push_content_result(
                            builder,
                            &mut results,
                            max_result_files,
                            &mut global_truncated,
                        );
                    }
                    current = Some((
                        path.clone(),
                        ContentFileResultBuilder::new(path.clone(), max_matches_per_file),
                    ));
                }
                if let Some((_path, builder)) = current.as_mut() {
                    parse_match_event_into_builder(&value, builder)?;
                }
            }
            Some("end") => {
                if let Some((_path, builder)) = current.take() {
                    push_content_result(
                        builder,
                        &mut results,
                        max_result_files,
                        &mut global_truncated,
                    );
                }
            }
            Some("summary") => {
                if let Some(searched) = value
                    .pointer("/data/stats/searches")
                    .and_then(Value::as_u64)
                {
                    files_scanned = searched;
                }
            }
            Some("context") | None => {}
            _ => {}
        }
    }
    if let Some((_path, builder)) = current.take() {
        push_content_result(
            builder,
            &mut results,
            max_result_files,
            &mut global_truncated,
        );
    }
    Ok(RipgrepSearchSummary {
        results_found: results.len(),
        files_scanned,
        results,
        cancelled: false,
        stderr: String::new(),
        global_truncated,
        execution_details: None,
    })
}

fn push_content_result(
    builder: ContentFileResultBuilder,
    results: &mut Vec<ContentFileResult>,
    max_result_files: usize,
    global_truncated: &mut bool,
) {
    let result = builder.finish();
    if result.total_matches == 0 {
        return;
    }
    if results.len() < max_result_files {
        results.push(result);
    } else {
        *global_truncated = true;
    }
}

fn event_path(value: &Value) -> Option<PathBuf> {
    value
        .pointer("/data/path/text")
        .and_then(Value::as_str)
        .map(PathBuf::from)
}

fn parse_match_event_into_builder(
    value: &Value,
    entry: &mut ContentFileResultBuilder,
) -> Result<(), FileSearchError> {
    let data = &value["data"];
    let line = data
        .pointer("/lines/text")
        .and_then(Value::as_str)
        .unwrap_or("");
    let line_number = data.get("line_number").and_then(Value::as_u64).unwrap_or(0) as usize;
    let normalized_line = line.trim_end_matches(['\r', '\n']).to_owned();
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

    fn req(root: PathBuf) -> SearchRequest {
        SearchRequest {
            kind: SearchKind::Content,
            scope: SearchScope::Roots { roots: vec![root] },
            text: "needle".to_owned(),
            case_sensitive: false,
            include_hidden_files: false,
            max_results: 10,
            max_file_size_bytes: 1024,
            included_extensions: vec![],
            excluded_extensions: vec![],
            excluded_directory_names: vec![],
            filename_match_mode: crate::file_search::model::FilenameMatchMode::RankedSubstring,
            content_match_mode: crate::file_search::model::ContentMatchMode::ExactPhrase,
            whole_word: false,
            file_type_filter: crate::file_search::model::FileTypeFilter::FilesAndDirectories,
        }
    }

    fn args(command: &Command) -> Vec<String> {
        command
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect()
    }

    #[test]
    fn commands_include_no_ignore() {
        let temp = tempfile::tempdir().unwrap();
        let request = req(temp.path().to_path_buf());
        assert!(
            args(&build_ripgrep_command(
                Path::new("rg"),
                &request,
                &FileSearchSettings::default(),
                &[temp.path().to_path_buf()]
            ))
            .contains(&"--no-ignore".to_owned())
        );
        assert!(
            args(&build_ripgrep_files_command(
                Path::new("rg"),
                &request,
                &FileSearchSettings::default(),
                &[temp.path().to_path_buf()]
            ))
            .contains(&"--no-ignore".to_owned())
        );
    }

    #[test]
    fn content_command_keeps_patterns_before_separator_and_roots_after() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let mut request = req(root.clone());
        request.content_match_mode = crate::file_search::model::ContentMatchMode::AnyTerm;
        request.text = "one two".to_owned();
        let args = args(&build_ripgrep_command(
            Path::new("rg"),
            &request,
            &FileSearchSettings::default(),
            std::slice::from_ref(&root),
        ));
        let sep = args.iter().position(|arg| arg == "--").unwrap();
        assert!(args[..sep].windows(2).any(|w| w == ["-e", "one"]));
        assert!(args[..sep].windows(2).any(|w| w == ["-e", "two"]));
        assert_eq!(args.last(), Some(&root.to_string_lossy().to_string()));
    }

    #[test]
    fn hidden_flag_only_when_enabled() {
        let temp = tempfile::tempdir().unwrap();
        let mut request = req(temp.path().to_path_buf());
        assert!(
            !args(&build_ripgrep_command(
                Path::new("rg"),
                &request,
                &FileSearchSettings::default(),
                &[temp.path().to_path_buf()]
            ))
            .contains(&"--hidden".to_owned())
        );
        request.include_hidden_files = true;
        assert!(
            args(&build_ripgrep_command(
                Path::new("rg"),
                &request,
                &FileSearchSettings::default(),
                &[temp.path().to_path_buf()]
            ))
            .contains(&"--hidden".to_owned())
        );
    }

    #[test]
    fn whole_word_flag_only_when_enabled() {
        let temp = tempfile::tempdir().unwrap();
        let mut request = req(temp.path().to_path_buf());
        assert!(
            !args(&build_ripgrep_command(
                Path::new("rg"),
                &request,
                &FileSearchSettings::default(),
                &[temp.path().to_path_buf()]
            ))
            .contains(&"--word-regexp".to_owned())
        );
        request.whole_word = true;
        assert!(
            args(&build_ripgrep_command(
                Path::new("rg"),
                &request,
                &FileSearchSettings::default(),
                &[temp.path().to_path_buf()]
            ))
            .contains(&"--word-regexp".to_owned())
        );
    }

    #[test]
    fn any_term_mode_creates_multiple_patterns() {
        let temp = tempfile::tempdir().unwrap();
        let mut request = req(temp.path().to_path_buf());
        request.content_match_mode = crate::file_search::model::ContentMatchMode::AnyTerm;
        request.text = "Alpha -beta".to_owned();
        let args = args(&build_ripgrep_command(
            Path::new("rg"),
            &request,
            &FileSearchSettings::default(),
            &[temp.path().to_path_buf()],
        ));
        assert_eq!(args.iter().filter(|arg| *arg == "-e").count(), 2);
        assert!(args.windows(2).any(|w| w == ["-e", "alpha"]));
        assert!(args.windows(2).any(|w| w == ["-e", "-beta"]));
    }

    #[test]
    fn globs_are_normalized_and_directory_exclusions_safe() {
        let temp = tempfile::tempdir().unwrap();
        let mut request = req(temp.path().to_path_buf());
        request.included_extensions = vec![".rs".to_owned()];
        request.excluded_extensions = vec!["tmp".to_owned()];
        request.excluded_directory_names = vec!["target".to_owned()];
        let args = args(&build_ripgrep_command(
            Path::new("rg"),
            &request,
            &FileSearchSettings::default(),
            &[temp.path().to_path_buf()],
        ));
        assert!(args.windows(2).any(|w| w == ["--glob", "*.rs"]));
        assert!(args.windows(2).any(|w| w == ["--glob", "!*.tmp"]));
        assert!(args.windows(2).any(|w| w == ["--glob", "!**/target/**"]));
    }

    #[test]
    fn incremental_json_parser_groups_begin_match_end_by_file() {
        let json = concat!(
            r#"{"type":"begin","data":{"path":{"text":"a.txt"}}}"#,
            "\n",
            r#"{"type":"match","data":{"path":{"text":"a.txt"},"lines":{"text":"needle\n"},"line_number":1,"submatches":[{"start":0,"end":6}]}}"#,
            "\n",
            r#"{"type":"end","data":{"path":{"text":"a.txt"}}}"#
        );
        let summary = parse_ripgrep_json(json, 25).unwrap();
        assert_eq!(summary.results.len(), 1);
        assert_eq!(summary.results[0].matches.len(), 1);
    }

    #[test]
    fn bounded_stderr_is_limited() {
        let data = vec![b'x'; STDERR_CAPTURE_LIMIT * 2];
        let captured = read_bounded_to_string(&data[..], STDERR_CAPTURE_LIMIT);
        assert_eq!(captured.len(), STDERR_CAPTURE_LIMIT);
    }

    #[test]
    fn result_limit_sets_global_truncation() {
        let json = concat!(
            r#"{"type":"match","data":{"path":{"text":"a.txt"},"lines":{"text":"needle\n"},"line_number":1,"submatches":[{"start":0,"end":6}]}}"#,
            "\n",
            r#"{"type":"end","data":{"path":{"text":"a.txt"}}}"#,
            "\n",
            r#"{"type":"match","data":{"path":{"text":"b.txt"},"lines":{"text":"needle\n"},"line_number":1,"submatches":[{"start":0,"end":6}]}}"#,
            "\n",
            r#"{"type":"end","data":{"path":{"text":"b.txt"}}}"#
        );
        let summary = parse_ripgrep_json_limited(json, 25, 1).unwrap();
        assert_eq!(summary.results.len(), 1);
        assert!(summary.global_truncated);
    }

    #[test]
    fn streaming_parser_finalizes_open_file_without_end_event() {
        let json = r#"{"type":"match","data":{"path":{"text":"open.txt"},"lines":{"text":"needle\n"},"line_number":1,"submatches":[{"start":0,"end":6}]}}"#;
        let summary = parse_ripgrep_json_reader(json.as_bytes(), 25, 10).unwrap();
        assert_eq!(summary.results.len(), 1);
        assert_eq!(summary.results[0].path, PathBuf::from("open.txt"));
    }

    #[cfg(unix)]
    #[test]
    fn cancellation_kills_child() {
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg("sleep 5")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().unwrap();
        let token = CancellationToken::new();
        token.cancel();
        let result = wait_for_child(&mut child, &token, None);
        assert!(result.cancelled);
        assert!(result.status.is_some());
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
        assert!(handle_exit_status(fake_status(1), Path::new("rg"), "").is_ok());
        assert!(handle_exit_status(fake_status(2), Path::new("rg"), "unrecognized flag").is_err());
        assert!(handle_exit_status(fake_status(2), Path::new("rg"), "Permission denied").is_ok());
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
    fn resolves_bare_rg_from_path() {
        let temp = tempfile::tempdir().unwrap();
        let executable = temp.path().join("rg");
        std::fs::write(&executable, "").unwrap();

        assert_eq!(
            find_on_path(Path::new("rg"), [temp.path().to_path_buf()]),
            Some(executable)
        );
    }

    #[cfg(windows)]
    #[test]
    fn resolves_bare_rg_exe_from_path() {
        let temp = tempfile::tempdir().unwrap();
        let executable = temp.path().join("rg.exe");
        std::fs::write(&executable, "").unwrap();

        assert_eq!(
            find_on_path(Path::new("rg.exe"), [temp.path().to_path_buf()]),
            Some(executable)
        );
    }

    #[test]
    fn nonexistent_absolute_path_error_contains_configured_path() {
        let configured = if cfg!(windows) {
            PathBuf::from(r"C:\definitely\missing\rg.exe")
        } else {
            PathBuf::from("/definitely/missing/rg")
        };
        let error = resolve_ripgrep_with_context(
            &configured,
            &ExecutableSearchContext {
                launcher_directory: tempfile::tempdir().unwrap().path().to_path_buf(),
                path_directories: Vec::new(),
            },
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains(&configured.display().to_string()), "{error}");
    }

    #[test]
    fn relative_path_with_directory_components_must_exist() {
        let configured = PathBuf::from("missing-dir/rg");
        let error = resolve_ripgrep_with_context(
            &configured,
            &ExecutableSearchContext {
                launcher_directory: tempfile::tempdir().unwrap().path().to_path_buf(),
                path_directories: Vec::new(),
            },
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains(&configured.display().to_string()), "{error}");
    }

    #[test]
    fn settings_diagnostics_and_runtime_use_same_resolver_output() {
        let Ok(resolved) = resolve_ripgrep_executable(Path::new("rg")) else {
            return;
        };
        let settings = FileSearchSettings {
            ripgrep_executable_path: PathBuf::from("rg"),
            ..FileSearchSettings::default()
        };
        assert_eq!(
            crate::file_search::settings::detect_ripgrep_executable(&settings),
            Some(resolved.clone())
        );
        assert_eq!(
            resolve_ripgrep_executable(&settings.ripgrep_executable_path).unwrap(),
            resolved
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
        let pattern = args
            .windows(2)
            .position(|window| window == ["-e", "-needle"])
            .unwrap();
        assert!(pattern < sep);
    }

    #[test]
    fn optional_integration_runs_when_rg_is_detected() {
        let Ok(executable) = resolve_ripgrep_executable(Path::new("rg")) else {
            return;
        };
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("file.txt");
        std::fs::write(&file, "hello needle\n").unwrap();
        let settings = FileSearchSettings {
            ripgrep_executable_path: executable,
            ..FileSearchSettings::default()
        };
        let mut coordinator =
            crate::file_search::coordinator::SearchCoordinator::from_settings(settings);
        coordinator.start_search(req(temp.path().to_path_buf()));
        let mut events = Vec::new();
        for _ in 0..200 {
            events.extend(coordinator.drain_current_events());
            if events.iter().any(|event| {
                matches!(
                    event,
                    SearchEvent::Completed { .. } | SearchEvent::Failed { .. }
                )
            }) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            events
                .iter()
                .any(|event| matches!(event, SearchEvent::Completed { .. })),
            "events: {events:?}"
        );
        assert!(
            events.iter().any(|event| matches!(
                event,
                SearchEvent::Result {
                    result: SearchResult::ContentFile(result),
                    ..
                } if result.path == file
            )),
            "events: {events:?}"
        );
    }
}
