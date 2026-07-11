use crate::file_search::coordinator::{CancellationToken, SearchExecutor};
use crate::file_search::model::{
    FileKind, FilenameRank, FilenameResult, SearchEvent, SearchId, SearchKind, SearchProgress,
    SearchRequest, SearchResult, SearchScope, SearchStatus,
};
use crate::file_search::settings::FileSearchSettings;
use crate::file_search::walkdir::rank_filename_match;
use std::collections::HashSet;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

const STDERR_LIMIT: usize = 8 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EverythingDiagnostic {
    pub enabled: bool,
    pub configured_path: Option<PathBuf>,
    pub detected_path: Option<PathBuf>,
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct EverythingSearchExecutor {
    settings: FileSearchSettings,
}

impl EverythingSearchExecutor {
    pub fn new(settings: FileSearchSettings) -> Self {
        Self { settings }
    }

    pub fn diagnostic(&self) -> EverythingDiagnostic {
        everything_diagnostic(&self.settings)
    }
}

impl SearchExecutor for EverythingSearchExecutor {
    fn execute(
        &self,
        id: SearchId,
        request: SearchRequest,
        token: CancellationToken,
        events: mpsc::Sender<SearchEvent>,
    ) {
        match search_with_everything(request, &self.settings, &token, &events, id) {
            Ok(()) => {}
            Err(error) if token.is_cancelled() => {
                let _ = events.send(SearchEvent::Cancelled { id });
                if !error.is_empty() {
                    tracing::debug!(%error, "Everything search cancelled");
                }
            }
            Err(error) => {
                let _ = events.send(SearchEvent::Failed { id, error });
            }
        }
    }
}

pub fn everything_diagnostic(settings: &FileSearchSettings) -> EverythingDiagnostic {
    let configured_path = configured_executable(&settings.everything_executable_path);
    let detected_path = if settings.everything_enabled {
        detect_everything_executable(settings)
    } else {
        None
    };
    let unavailable_reason = if !settings.everything_enabled {
        Some("Everything filename search is disabled in settings".to_owned())
    } else if detected_path.is_none() {
        Some(
            "Everything executable was not found; install Everything/ES.exe or configure its path"
                .to_owned(),
        )
    } else {
        None
    };

    EverythingDiagnostic {
        enabled: settings.everything_enabled,
        configured_path,
        detected_path,
        unavailable_reason,
    }
}

pub fn detect_everything_executable(settings: &FileSearchSettings) -> Option<PathBuf> {
    if !settings.everything_enabled {
        return None;
    }
    configured_executable(&settings.everything_executable_path)
        .or_else(|| find_on_path("es.exe"))
        .or_else(|| find_on_path("Everything.exe"))
        .or_else(find_common_windows_installation)
}

fn configured_executable(path: &Path) -> Option<PathBuf> {
    if path.as_os_str().is_empty() {
        return None;
    }
    if path.components().count() == 1 {
        find_on_path(path)
    } else if path.is_file() {
        Some(path.to_path_buf())
    } else {
        None
    }
}

fn find_on_path<S: AsRef<std::ffi::OsStr>>(name: S) -> Option<PathBuf> {
    let name = name.as_ref();
    env::var_os("PATH").and_then(|paths| {
        env::split_paths(&paths)
            .map(|dir| dir.join(Path::new(name)))
            .find(|candidate| candidate.is_file())
    })
}

fn find_common_windows_installation() -> Option<PathBuf> {
    let mut roots = Vec::new();
    roots.extend(env::var_os("ProgramFiles").map(PathBuf::from));
    roots.extend(env::var_os("ProgramFiles(x86)").map(PathBuf::from));
    roots.extend(env::var_os("LOCALAPPDATA").map(PathBuf::from));
    for root in roots {
        for rel in [
            "Everything/ES.exe",
            "Everything/es.exe",
            "Everything/Everything.exe",
        ] {
            let candidate = root.join(rel);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EverythingCommandSpec {
    pub executable: PathBuf,
    pub args: Vec<OsString>,
}

pub fn build_everything_command(
    executable: PathBuf,
    request: &SearchRequest,
    settings: &FileSearchSettings,
) -> EverythingCommandSpec {
    let mut args = Vec::new();
    args.push(OsString::from("-csv"));
    args.push(OsString::from("-n"));
    args.push(OsString::from(request.max_results.to_string()));
    if request.case_sensitive {
        args.push(OsString::from("-case"));
    }
    if request.include_hidden_files {
        args.push(OsString::from("-hidden"));
    }
    for ext in &request.included_extensions {
        args.push(OsString::from("-ext"));
        args.push(OsString::from(ext.trim_start_matches('.')));
    }
    for ext in &request.excluded_extensions {
        args.push(OsString::from("-exclude"));
        args.push(OsString::from(format!("*.{}", ext.trim_start_matches('.'))));
    }
    for dir in excluded_directory_names(request, settings) {
        args.push(OsString::from("-exclude"));
        args.push(OsString::from(format!("{}\\*", dir)));
    }
    args.push(OsString::from("-s"));
    args.push(OsString::from(&request.text));
    EverythingCommandSpec { executable, args }
}

pub fn search_with_everything(
    request: SearchRequest,
    settings: &FileSearchSettings,
    cancellation: &CancellationToken,
    event_sender: &mpsc::Sender<SearchEvent>,
    search_id: SearchId,
) -> Result<(), String> {
    if request.kind != SearchKind::Filename || request.scope != SearchScope::Global {
        return Err("Everything search only supports global filename requests".to_owned());
    }
    let executable = detect_everything_executable(settings).ok_or_else(|| {
        "Everything filename search is unavailable; install Everything/ES.exe or configure the Everything executable path in settings".to_owned()
    })?;
    let spec = build_everything_command(executable, &request, settings);
    let mut child = Command::new(&spec.executable)
        .args(&spec.args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            format!(
                "failed to start Everything search '{}': {e}",
                spec.executable.display()
            )
        })?;

    let stdout = child
        .stdout
        .take()
        .ok_or("failed to capture Everything stdout")?;
    let stderr = child
        .stderr
        .take()
        .ok_or("failed to capture Everything stderr")?;
    let (result_tx, result_rx) = mpsc::channel();
    let req_for_reader = request.clone();
    let stdout_thread = thread::spawn(move || read_stdout(stdout, req_for_reader, result_tx));
    let stderr_thread = thread::spawn(move || read_bounded(stderr, STDERR_LIMIT));

    let mut emitted = 0usize;
    loop {
        while let Ok(parsed) = result_rx.try_recv() {
            match parsed {
                Ok(result) => {
                    if emitted < request.max_results
                        && passes_post_filters(&result, &request, settings)
                        && event_sender
                            .send(SearchEvent::Result {
                                id: search_id,
                                result: SearchResult::Filename(result),
                            })
                            .is_ok()
                    {
                        emitted += 1;
                    }
                }
                Err(error) => return Err(error),
            }
        }
        if cancellation.is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_thread.join();
            let _ = stderr_thread.join();
            let _ = event_sender.send(SearchEvent::Cancelled { id: search_id });
            return Ok(());
        }
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            let stdout_result = stdout_thread
                .join()
                .unwrap_or_else(|_| Err("stdout reader panicked".to_owned()));
            while let Ok(parsed) = result_rx.try_recv() {
                match parsed {
                    Ok(result) => {
                        if emitted < request.max_results
                            && passes_post_filters(&result, &request, settings)
                        {
                            let _ = event_sender.send(SearchEvent::Result {
                                id: search_id,
                                result: SearchResult::Filename(result),
                            });
                            emitted += 1;
                        }
                    }
                    Err(error) => return Err(error),
                }
            }
            stdout_result?;
            let stderr = stderr_thread.join().unwrap_or_default();
            if !status.success() {
                return Err(format!(
                    "Everything search failed with status {status}: {}",
                    stderr.trim()
                ));
            }
            let _ = event_sender.send(SearchEvent::Progress {
                id: search_id,
                progress: SearchProgress {
                    files_scanned: 0,
                    directories_scanned: 0,
                    results_found: emitted,
                    status: SearchStatus::Completed,
                },
            });
            let _ = event_sender.send(SearchEvent::Completed { id: search_id });
            return Ok(());
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn read_stdout<R: Read>(
    reader: R,
    request: SearchRequest,
    tx: mpsc::Sender<Result<FilenameResult, String>>,
) -> Result<(), String> {
    for line in BufReader::new(reader).lines() {
        let line = line.map_err(|e| e.to_string())?;
        if line.trim().is_empty() {
            continue;
        }
        let result = parse_everything_line(&line, &request)?;
        if tx.send(Ok(result)).is_err() {
            break;
        }
    }
    Ok(())
}

fn read_bounded<R: Read>(mut reader: R, limit: usize) -> String {
    let mut captured = Vec::new();
    let mut chunk = [0_u8; 1024];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => {
                let remaining = limit.saturating_sub(captured.len());
                if remaining > 0 {
                    captured.extend_from_slice(&chunk[..read.min(remaining)]);
                }
            }
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&captured).into_owned()
}

pub fn parse_everything_output(
    output: &str,
    request: &SearchRequest,
) -> Result<Vec<FilenameResult>, String> {
    output
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| parse_everything_line(l, request))
        .collect()
}

pub fn parse_everything_line(
    line: &str,
    request: &SearchRequest,
) -> Result<FilenameResult, String> {
    let fields = parse_csv_line(line)?;
    let path_field = fields
        .iter()
        .find(|field| looks_like_path(field))
        .or_else(|| fields.first())
        .ok_or("Everything output row did not contain a path")?;
    let path = PathBuf::from(path_field);
    if path.as_os_str().is_empty() {
        return Err("Everything output row contained an empty path".to_owned());
    }
    let metadata = fs::metadata(&path).ok();
    let file_name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());
    let rank = rank_filename_match(&file_name, &path, &request.text, request.case_sensitive)
        .unwrap_or(FilenameRank::FullPathContains);
    Ok(FilenameResult {
        path: path.clone(),
        file_name,
        parent_directory: path.parent().map(Path::to_path_buf),
        kind: file_kind(metadata.as_ref()),
        size: metadata.as_ref().filter(|m| m.is_file()).map(|m| m.len()),
        modified: metadata.and_then(|m| m.modified().ok()),
        rank,
    })
}

fn parse_csv_line(line: &str) -> Result<Vec<String>, String> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut chars = line.chars().peekable();
    let mut in_quotes = false;
    while let Some(ch) = chars.next() {
        match ch {
            '"' if in_quotes && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => {
                fields.push(field);
                field = String::new();
            }
            _ => field.push(ch),
        }
    }
    if in_quotes {
        return Err("invalid Everything CSV output: unterminated quoted field".to_owned());
    }
    fields.push(field);
    Ok(fields)
}

fn looks_like_path(value: &str) -> bool {
    value.contains('\\') || value.contains('/') || Path::new(value).is_absolute()
}

fn file_kind(metadata: Option<&fs::Metadata>) -> FileKind {
    match metadata {
        Some(m) if m.is_file() => FileKind::File,
        Some(m) if m.is_dir() => FileKind::Directory,
        _ => FileKind::Other,
    }
}

fn passes_post_filters(
    result: &FilenameResult,
    request: &SearchRequest,
    settings: &FileSearchSettings,
) -> bool {
    if !request.include_hidden_files
        && result
            .path
            .components()
            .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
    {
        return false;
    }
    if !request.included_extensions.is_empty() {
        let ext = result
            .path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase());
        if !request
            .included_extensions
            .iter()
            .map(|e| e.trim_start_matches('.').to_lowercase())
            .any(|e| Some(e) == ext)
        {
            return false;
        }
    }
    if !request.excluded_extensions.is_empty() {
        let ext = result
            .path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase());
        if request
            .excluded_extensions
            .iter()
            .map(|e| e.trim_start_matches('.').to_lowercase())
            .any(|e| Some(e) == ext)
        {
            return false;
        }
    }
    if result.path.components().any(|c| {
        excluded_directory_names(request, settings)
            .contains(&c.as_os_str().to_string_lossy().to_string())
    }) {
        return false;
    }
    true
}

fn excluded_directory_names(
    request: &SearchRequest,
    settings: &FileSearchSettings,
) -> HashSet<String> {
    settings
        .excluded_directory_names
        .iter()
        .chain(request.excluded_directory_names.iter())
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn request(text: &str) -> SearchRequest {
        SearchRequest {
            kind: SearchKind::Filename,
            scope: SearchScope::Global,
            text: text.to_owned(),
            case_sensitive: false,
            include_hidden_files: false,
            max_results: 25,
            max_file_size_bytes: 0,
            included_extensions: vec![],
            excluded_extensions: vec![],
            excluded_directory_names: vec![],
        }
    }

    #[test]
    fn detects_configured_temp_fake_executable_path() {
        let temp = tempfile::tempdir().unwrap();
        let fake = temp.path().join("fake es.exe");
        fs::write(&fake, "").unwrap();
        let settings = FileSearchSettings {
            everything_enabled: true,
            everything_executable_path: fake.clone(),
            ..FileSearchSettings::default()
        };
        assert_eq!(detect_everything_executable(&settings), Some(fake.clone()));
        let diagnostic = everything_diagnostic(&settings);
        assert!(diagnostic.enabled);
        assert_eq!(diagnostic.configured_path, Some(fake.clone()));
        assert_eq!(diagnostic.detected_path, Some(fake));
        assert_eq!(diagnostic.unavailable_reason, None);
    }

    #[test]
    fn detects_path_fake_executable() {
        let temp = tempfile::tempdir().unwrap();
        let fake = temp.path().join("es.exe");
        fs::write(&fake, "").unwrap();
        let old = env::var_os("PATH");
        unsafe {
            env::set_var("PATH", temp.path());
        }
        let settings = FileSearchSettings {
            everything_enabled: true,
            everything_executable_path: PathBuf::from("missing.exe"),
            ..FileSearchSettings::default()
        };
        assert_eq!(detect_everything_executable(&settings), Some(fake));
        if let Some(old) = old {
            unsafe {
                env::set_var("PATH", old);
            }
        } else {
            unsafe {
                env::remove_var("PATH");
            }
        }
    }

    #[test]
    fn builds_argument_vector_with_filters_and_dash_query_as_value() {
        let mut req = request("-dash query");
        req.max_results = 7;
        req.case_sensitive = true;
        req.include_hidden_files = true;
        req.included_extensions = vec![".rs".into()];
        req.excluded_directory_names = vec!["target".into()];
        let spec = build_everything_command(
            PathBuf::from("C:/Program Files/Everything/es.exe"),
            &req,
            &FileSearchSettings::default(),
        );
        assert_eq!(
            spec.executable,
            PathBuf::from("C:/Program Files/Everything/es.exe")
        );
        assert!(spec.args.contains(&OsString::from("-csv")));
        assert!(spec
            .args
            .windows(2)
            .any(|w| w == [OsString::from("-n"), OsString::from("7")]));
        assert!(spec
            .args
            .windows(2)
            .any(|w| w == [OsString::from("-s"), OsString::from("-dash query")]));
        assert!(!spec
            .args
            .iter()
            .any(|a| a == "-dash query" && spec.args.first() == Some(a)));
    }

    #[test]
    fn parses_output_fixtures_empty_invalid_spaces_unicode_and_kinds() {
        let temp = tempfile::tempdir().unwrap();
        let spaced = temp.path().join("path with spaces.txt");
        let unicode = temp.path().join("ユニコード");
        fs::write(&spaced, "a").unwrap();
        fs::create_dir(&unicode).unwrap();
        let output = format!("\"{}\"\n\"{}\"\n", spaced.display(), unicode.display());
        let results = parse_everything_output(&output, &request("path")).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].kind, FileKind::File);
        assert_eq!(results[1].kind, FileKind::Directory);
        assert!(results[0]
            .path
            .to_string_lossy()
            .contains("path with spaces"));
        assert!(results[1].path.to_string_lossy().contains("ユニコード"));
        assert!(parse_everything_output("", &request("x"))
            .unwrap()
            .is_empty());
        assert!(parse_everything_output("\"unterminated", &request("x")).is_err());
    }

    #[test]
    fn nonzero_status_is_reported_with_bounded_stderr() {
        let temp = tempfile::tempdir().unwrap();
        #[cfg(unix)]
        let script = temp.path().join("es");
        #[cfg(windows)]
        let script = temp.path().join("es.cmd");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::write(
                &script,
                "#!/bin/sh
echo failure >&2
exit 3
",
            )
            .unwrap();
            let mut p = fs::metadata(&script).unwrap().permissions();
            p.set_mode(0o755);
            fs::set_permissions(&script, p).unwrap();
        }
        #[cfg(windows)]
        {
            fs::write(
                &script,
                "@echo off
echo failure 1>&2
exit /b 3
",
            )
            .unwrap();
        }
        let settings = FileSearchSettings {
            everything_enabled: true,
            everything_executable_path: script,
            ..FileSearchSettings::default()
        };
        let err = search_with_everything(
            request("x"),
            &settings,
            &CancellationToken::new(),
            &mpsc::channel().0,
            SearchId(1),
        )
        .unwrap_err();
        assert!(err.contains("Everything search failed"));
        assert!(err.contains("failure"));
    }

    #[test]
    fn optional_integration_runs_only_when_enabled_and_detected() {
        if env::var_os("MULTI_LAUNCHER_TEST_EVERYTHING").is_none() {
            return;
        }
        let settings = FileSearchSettings {
            everything_enabled: true,
            ..FileSearchSettings::default()
        };
        if detect_everything_executable(&settings).is_none() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        search_with_everything(
            request("definitely-not-a-real-file-search-fixture"),
            &settings,
            &CancellationToken::new(),
            &tx,
            SearchId(99),
        )
        .unwrap();
        assert!(rx
            .try_iter()
            .any(|event| matches!(event, SearchEvent::Completed { id: SearchId(99) })));
    }
}
