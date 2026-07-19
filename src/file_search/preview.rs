use crate::common::lru::LruCache;

use std::collections::VecDeque;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub const DEFAULT_MAX_BYTES_FULL_FILE_PREVIEW: usize = 5 * 1024 * 1024;
pub const DEFAULT_OVERSIZED_BEGINNING_PREVIEW_BYTES: usize = 64 * 1024;
pub const DEFAULT_MAX_LINES_AROUND_MATCH: usize = 3;
pub const DEFAULT_BINARY_SAMPLE_BYTES: usize = 8192;
pub const DEFAULT_CACHE_CAPACITY: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewLoadingState {
    Idle,
    Loading {
        request_id: u64,
    },
    Loaded {
        request_id: u64,
        preview: FilePreview,
    },
    Failed {
        request_id: u64,
        error: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewLoadOutcome {
    Loaded(FilePreview),
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewLoadStateMachine {
    next_request_id: u64,
    pub current_request_id: Option<u64>,
    pub state: PreviewLoadingState,
}

impl Default for PreviewLoadStateMachine {
    fn default() -> Self {
        Self {
            next_request_id: 0,
            current_request_id: None,
            state: PreviewLoadingState::Idle,
        }
    }
}

impl PreviewLoadStateMachine {
    pub fn begin_request(&mut self) -> u64 {
        self.next_request_id = self.next_request_id.saturating_add(1);
        let request_id = self.next_request_id;
        self.current_request_id = Some(request_id);
        self.state = PreviewLoadingState::Loading { request_id };
        request_id
    }

    pub fn complete_request(&mut self, request_id: u64, outcome: PreviewLoadOutcome) -> bool {
        if self.current_request_id != Some(request_id) {
            return false;
        }
        self.state = match outcome {
            PreviewLoadOutcome::Loaded(preview) => PreviewLoadingState::Loaded {
                request_id,
                preview,
            },
            PreviewLoadOutcome::Failed(error) => PreviewLoadingState::Failed { request_id, error },
        };
        true
    }

    pub fn clear(&mut self) {
        self.current_request_id = None;
        self.state = PreviewLoadingState::Idle;
    }

    pub fn is_loading(&self) -> bool {
        matches!(self.state, PreviewLoadingState::Loading { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PreviewRequest {
    pub path: PathBuf,
    pub line_range: Option<PreviewLineRange>,
    pub selected_match: Option<PreviewMatchSelection>,
    pub max_bytes_full_file_preview: usize,
    pub max_lines_around_match: usize,
    pub oversized_beginning_preview_bytes: usize,
}

impl PreviewRequest {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            line_range: None,
            selected_match: None,
            max_bytes_full_file_preview: DEFAULT_MAX_BYTES_FULL_FILE_PREVIEW,
            max_lines_around_match: DEFAULT_MAX_LINES_AROUND_MATCH,
            oversized_beginning_preview_bytes: DEFAULT_OVERSIZED_BEGINNING_PREVIEW_BYTES,
        }
    }

    pub fn for_match(path: impl Into<PathBuf>, line: usize, column: usize) -> Self {
        Self {
            selected_match: Some(PreviewMatchSelection {
                line,
                source_line: None,
                start_column: column,
                end_column: Some(column.saturating_add(1)),
                match_length: None,
            }),
            ..Self::new(path)
        }
    }

    fn effective_line_range(&self) -> Option<PreviewLineRange> {
        self.line_range.or_else(|| {
            self.selected_match.as_ref().map(|selected| {
                let start = selected
                    .line
                    .saturating_sub(self.max_lines_around_match)
                    .max(1);
                let end = selected.line.saturating_add(self.max_lines_around_match);
                PreviewLineRange { start, end }
            })
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PreviewLineRange {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PreviewMatchSelection {
    pub line: usize,
    pub source_line: Option<String>,
    pub start_column: usize,
    pub end_column: Option<usize>,
    pub match_length: Option<usize>,
}

impl PreviewMatchSelection {
    fn end_column(&self) -> usize {
        self.end_column
            .or_else(|| {
                self.match_length
                    .map(|length| self.start_column.saturating_add(length))
            })
            .unwrap_or_else(|| self.start_column.saturating_add(1))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePreview {
    pub path: PathBuf,
    pub kind: PreviewKind,
    pub lines: Vec<PreviewLine>,
    pub metadata: PreviewMetadata,
    pub binary_or_unsupported: Option<UnsupportedPreview>,
    pub error: Option<PreviewError>,
    pub coverage: PreviewCoverage,
    pub displayed_start_line: Option<usize>,
    pub displayed_end_line: Option<usize>,
    pub warnings: Vec<String>,
    pub lossy_utf8: bool,
    pub cache_hit: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewCoverage {
    Complete,
    BeginningOnly,
    MatchContextOnly,
    BinaryUnsupported,
    Unsupported,
    ReadError,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewKind {
    Text,
    Directory,
    Binary,
    Unsupported,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewLine {
    pub line_number: usize,
    pub text: String,
    pub match_ranges: Vec<PreviewMatchRange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreviewMatchRange {
    pub start_column: usize,
    pub end_column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewMetadata {
    pub is_directory: bool,
    pub len_bytes: u64,
    pub modified: Option<SystemTime>,
    pub readonly: bool,
    pub sampled_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsupportedPreview {
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewError {
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PreviewCacheKey {
    path: PathBuf,
    modified: Option<SystemTime>,
    len_bytes: u64,
    max_bytes_full_file_preview: usize,
    max_lines_around_match: usize,
    selected_line: Option<usize>,
    line_range: Option<PreviewLineRange>,
    oversized_beginning_preview_bytes: usize,
}

pub struct PreviewCache {
    entries: LruCache<PreviewCacheKey, FilePreview>,
}

impl Default for PreviewCache {
    fn default() -> Self {
        Self::new(DEFAULT_CACHE_CAPACITY)
    }
}

impl PreviewCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: LruCache::new(capacity),
        }
    }

    pub fn preview(&mut self, request: &PreviewRequest) -> FilePreview {
        let metadata = match fs::metadata(&request.path) {
            Ok(metadata) => metadata,
            Err(error) => return error_preview(&request.path, error),
        };
        let key = cache_key(request, &metadata);
        if let Some(cached) = self.entries.get(&key) {
            let mut preview = styled_preview(cached, request);
            preview.cache_hit = true;
            return preview;
        }

        let cache_request = cache_storage_request(request, &metadata);
        let mut preview = build_preview(&cache_request, metadata);
        clear_match_ranges(&mut preview);
        preview.cache_hit = false;
        self.entries.insert(key, preview.clone());
        styled_preview(&preview, request)
    }
}

pub fn preview_file(request: &PreviewRequest) -> FilePreview {
    build_preview(
        request,
        match fs::metadata(&request.path) {
            Ok(metadata) => metadata,
            Err(error) => return error_preview(&request.path, error),
        },
    )
}

fn cache_key(request: &PreviewRequest, metadata: &fs::Metadata) -> PreviewCacheKey {
    let context_only = metadata.len() > request.max_bytes_full_file_preview as u64
        && request.selected_match.is_some()
        && request.line_range.is_none();
    PreviewCacheKey {
        path: request.path.clone(),
        modified: metadata.modified().ok(),
        len_bytes: metadata.len(),
        max_bytes_full_file_preview: request.max_bytes_full_file_preview,
        max_lines_around_match: request.max_lines_around_match,
        selected_line: context_only
            .then(|| {
                request
                    .selected_match
                    .as_ref()
                    .map(|selected| selected.line)
            })
            .flatten(),
        line_range: request.line_range,
        oversized_beginning_preview_bytes: request.oversized_beginning_preview_bytes,
    }
}

fn cache_storage_request(request: &PreviewRequest, metadata: &fs::Metadata) -> PreviewRequest {
    let complete_selected_preview = metadata.len() <= request.max_bytes_full_file_preview as u64
        && request.selected_match.is_some()
        && request.line_range.is_none();
    if !complete_selected_preview {
        return request.clone();
    }

    let mut cache_request = request.clone();
    cache_request.selected_match = None;
    cache_request
}

fn styled_preview(preview: &FilePreview, request: &PreviewRequest) -> FilePreview {
    let mut preview = preview.clone();
    if preview.coverage == PreviewCoverage::Complete {
        apply_line_range(&mut preview, request.effective_line_range());
    }
    if preview.kind == PreviewKind::Text {
        apply_match_ranges(&mut preview, request.selected_match.as_ref());
    }
    preview
}

fn apply_line_range(preview: &mut FilePreview, range: Option<PreviewLineRange>) {
    if let Some(range) = range {
        preview
            .lines
            .retain(|line| line.line_number >= range.start && line.line_number <= range.end);
        preview.displayed_start_line = preview.lines.first().map(|line| line.line_number);
        preview.displayed_end_line = preview.lines.last().map(|line| line.line_number);
    }
}

fn clear_match_ranges(preview: &mut FilePreview) {
    for line in &mut preview.lines {
        line.match_ranges.clear();
    }
}

fn apply_match_ranges(preview: &mut FilePreview, selected: Option<&PreviewMatchSelection>) {
    for line in &mut preview.lines {
        line.match_ranges.clear();
        if let Some(selected) = selected
            && selected.line == line.line_number
        {
            line.match_ranges = match_ranges_for_text(&line.text, selected);
        }
    }
}

fn build_preview(request: &PreviewRequest, metadata: fs::Metadata) -> FilePreview {
    let preview_metadata = to_preview_metadata(&metadata, 0);
    if metadata.is_dir() {
        return FilePreview {
            path: request.path.clone(),
            kind: PreviewKind::Directory,
            lines: Vec::new(),
            metadata: preview_metadata,
            binary_or_unsupported: Some(UnsupportedPreview {
                reason: "Directories cannot be previewed as text".to_string(),
            }),
            error: None,
            coverage: PreviewCoverage::Unsupported,
            displayed_start_line: None,
            displayed_end_line: None,
            warnings: Vec::new(),
            lossy_utf8: false,
            cache_hit: false,
        };
    }

    if !metadata.is_file() {
        return unsupported_preview(
            request.path.clone(),
            preview_metadata,
            "Unsupported filesystem entry",
        );
    }

    let sample_limit = DEFAULT_BINARY_SAMPLE_BYTES.min(metadata.len() as usize);
    let (sample, _) = match read_bounded(&request.path, sample_limit) {
        Ok(sample) => sample,
        Err(error) => return error_preview(&request.path, error),
    };
    let sampled = sample.len();
    let preview_metadata = to_preview_metadata(&metadata, sampled);
    if is_likely_binary(&sample) {
        return FilePreview {
            path: request.path.clone(),
            kind: PreviewKind::Binary,
            lines: Vec::new(),
            metadata: preview_metadata,
            binary_or_unsupported: Some(UnsupportedPreview {
                reason: "File appears to be binary".to_string(),
            }),
            error: None,
            coverage: PreviewCoverage::BinaryUnsupported,
            displayed_start_line: None,
            displayed_end_line: None,
            warnings: Vec::new(),
            lossy_utf8: false,
            cache_hit: false,
        };
    }

    if metadata.len() <= request.max_bytes_full_file_preview as u64 {
        return match fs::read(&request.path) {
            Ok(bytes) => text_preview(
                request,
                preview_metadata,
                &bytes,
                PreviewCoverage::Complete,
                false,
            ),
            Err(error) => error_preview(&request.path, error),
        };
    }

    if request.selected_match.is_some() && request.line_range.is_none() {
        return oversized_match_preview(request, preview_metadata);
    }

    match read_bounded(&request.path, request.oversized_beginning_preview_bytes) {
        Ok((bytes, _)) => text_preview(
            request,
            preview_metadata,
            &bytes,
            PreviewCoverage::BeginningOnly,
            true,
        ),
        Err(error) => error_preview(&request.path, error),
    }
}

fn text_preview(
    request: &PreviewRequest,
    metadata: PreviewMetadata,
    bytes: &[u8],
    coverage: PreviewCoverage,
    beginning_only: bool,
) -> FilePreview {
    let text = String::from_utf8_lossy(bytes);
    let lossy = matches!(text, std::borrow::Cow::Owned(_));
    let range = request.effective_line_range();
    let selected = request.selected_match.as_ref();
    let mut lines = Vec::new();

    for (idx, line) in text.lines().enumerate() {
        let line_number = idx + 1;
        if let Some(range) = range {
            if line_number < range.start {
                continue;
            }
            if line_number > range.end {
                break;
            }
        }
        lines.push(preview_line(line_number, line.to_string(), selected));
    }

    let mut warnings = Vec::new();
    if lossy {
        warnings
            .push("File contains invalid UTF-8; replacement characters were inserted".to_string());
    }

    let reason = beginning_only.then(|| {
        "File is larger than the full-file preview limit; showing the beginning only".to_string()
    });
    let displayed_start_line = lines.first().map(|line| line.line_number);
    let displayed_end_line = lines.last().map(|line| line.line_number);

    FilePreview {
        path: request.path.clone(),
        kind: PreviewKind::Text,
        lines,
        metadata,
        binary_or_unsupported: reason.map(|reason| UnsupportedPreview { reason }),
        error: None,
        coverage,
        displayed_start_line,
        displayed_end_line,
        warnings,
        lossy_utf8: lossy,
        cache_hit: false,
    }
}

fn oversized_match_preview(request: &PreviewRequest, metadata: PreviewMetadata) -> FilePreview {
    let selected = request.selected_match.as_ref().expect("checked above");
    let end = selected.line.saturating_add(request.max_lines_around_match);
    let file = match File::open(&request.path) {
        Ok(file) => file,
        Err(error) => return error_preview(&request.path, error),
    };
    let mut reader = BufReader::new(file);
    let mut buf = Vec::new();
    let mut before: VecDeque<PreviewLine> = VecDeque::with_capacity(request.max_lines_around_match);
    let mut lines = Vec::new();
    let mut line_number = 0usize;
    let mut found = false;
    let mut lossy_utf8 = false;

    loop {
        buf.clear();
        let read = match reader.read_until(b'\n', &mut buf) {
            Ok(read) => read,
            Err(error) => return error_preview(&request.path, error),
        };
        if read == 0 {
            break;
        }
        line_number += 1;
        while matches!(buf.last(), Some(b'\n' | b'\r')) {
            buf.pop();
        }
        let text = String::from_utf8_lossy(&buf);
        lossy_utf8 |= matches!(text, std::borrow::Cow::Owned(_));
        let preview = preview_line(line_number, text.into_owned(), Some(selected));

        if !found {
            if line_number == selected.line {
                found = true;
                lines.extend(before.drain(..));
                lines.push(preview);
                if request.max_lines_around_match == 0 {
                    break;
                }
            } else {
                if before.len() == request.max_lines_around_match {
                    before.pop_front();
                }
                before.push_back(preview);
            }
            continue;
        }

        lines.push(preview);
        if line_number >= end {
            break;
        }
    }

    if !found {
        return FilePreview {
            path: request.path.clone(),
            kind: PreviewKind::Unsupported,
            lines: Vec::new(),
            metadata,
            binary_or_unsupported: Some(UnsupportedPreview {
                reason: "Selected match line no longer exists; the file may have changed"
                    .to_string(),
            }),
            error: None,
            coverage: PreviewCoverage::Unsupported,
            displayed_start_line: None,
            displayed_end_line: None,
            warnings: Vec::new(),
            lossy_utf8: false,
            cache_hit: false,
        };
    }

    let mut warnings = Vec::new();
    if lossy_utf8 {
        warnings
            .push("File contains invalid UTF-8; replacement characters were inserted".to_string());
    }

    FilePreview {
        path: request.path.clone(),
        kind: PreviewKind::Text,
        displayed_start_line: lines.first().map(|line| line.line_number),
        displayed_end_line: lines.last().map(|line| line.line_number),
        lines,
        metadata,
        binary_or_unsupported: Some(UnsupportedPreview {
            reason: "File is larger than the full-file preview limit; showing context around the selected match".to_string(),
        }),
        error: None,
        coverage: PreviewCoverage::MatchContextOnly,
        warnings,
        lossy_utf8,
        cache_hit: false,
    }
}

fn preview_line(
    line_number: usize,
    text: String,
    selected: Option<&PreviewMatchSelection>,
) -> PreviewLine {
    let match_ranges = selected
        .filter(|selected| selected.line == line_number)
        .map(|selected| match_ranges_for_text(&text, selected))
        .unwrap_or_default();
    PreviewLine {
        line_number,
        text,
        match_ranges,
    }
}

fn match_ranges_for_text(text: &str, selected: &PreviewMatchSelection) -> Vec<PreviewMatchRange> {
    let chars = text.chars().count();
    let start = selected.start_column.saturating_sub(1).min(chars);
    let end = selected
        .end_column()
        .saturating_sub(1)
        .min(chars)
        .max(start + 1)
        .min(chars);
    vec![PreviewMatchRange {
        start_column: start + 1,
        end_column: end + 1,
    }]
}

fn read_bounded(path: &Path, max_bytes: usize) -> io::Result<(Vec<u8>, bool)> {
    let mut file = File::open(path)?;
    let mut bytes = Vec::with_capacity(max_bytes.min(8192));
    let mut limited = file.by_ref().take(max_bytes as u64 + 1);
    limited.read_to_end(&mut bytes)?;
    let truncated = bytes.len() > max_bytes;
    if truncated {
        bytes.truncate(max_bytes);
    }
    Ok((bytes, truncated))
}

fn is_likely_binary(sample: &[u8]) -> bool {
    sample.contains(&0)
        || sample
            .iter()
            .filter(|byte| byte.is_ascii_control() && !matches!(byte, b'\n' | b'\r' | b'\t'))
            .count()
            > sample.len() / 10
}

fn to_preview_metadata(metadata: &fs::Metadata, sampled_bytes: usize) -> PreviewMetadata {
    PreviewMetadata {
        is_directory: metadata.is_dir(),
        len_bytes: metadata.len(),
        modified: metadata.modified().ok(),
        readonly: metadata.permissions().readonly(),
        sampled_bytes,
    }
}

fn unsupported_preview(path: PathBuf, metadata: PreviewMetadata, reason: &str) -> FilePreview {
    FilePreview {
        path,
        kind: PreviewKind::Unsupported,
        lines: Vec::new(),
        metadata,
        binary_or_unsupported: Some(UnsupportedPreview {
            reason: reason.to_string(),
        }),
        error: None,
        coverage: PreviewCoverage::Unsupported,
        displayed_start_line: None,
        displayed_end_line: None,
        warnings: Vec::new(),
        lossy_utf8: false,
        cache_hit: false,
    }
}

fn error_preview(path: &Path, error: io::Error) -> FilePreview {
    FilePreview {
        path: path.to_path_buf(),
        kind: PreviewKind::Error,
        lines: Vec::new(),
        metadata: PreviewMetadata {
            is_directory: false,
            len_bytes: 0,
            modified: None,
            readonly: false,
            sampled_bytes: 0,
        },
        binary_or_unsupported: None,
        error: Some(PreviewError {
            message: error.to_string(),
        }),
        coverage: PreviewCoverage::ReadError,
        displayed_start_line: None,
        displayed_end_line: None,
        warnings: vec![format!("Unable to read preview: {error}")],
        lossy_utf8: false,
        cache_hit: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::thread::sleep;
    use std::time::Duration;
    use tempfile::tempdir;

    fn write_file(path: &Path, bytes: &[u8]) {
        let mut file = File::create(path).unwrap();
        file.write_all(bytes).unwrap();
        file.sync_all().unwrap();
    }

    fn preview_for_state_machine(path: &str, text: &str) -> FilePreview {
        FilePreview {
            path: PathBuf::from(path),
            kind: PreviewKind::Text,
            lines: vec![PreviewLine {
                line_number: 1,
                text: text.to_string(),
                match_ranges: Vec::new(),
            }],
            metadata: PreviewMetadata {
                is_directory: false,
                len_bytes: text.len() as u64,
                modified: None,
                readonly: false,
                sampled_bytes: text.len(),
            },
            binary_or_unsupported: None,
            error: None,
            coverage: PreviewCoverage::Complete,
            displayed_start_line: Some(1),
            displayed_end_line: Some(1),
            warnings: Vec::new(),
            lossy_utf8: false,
            cache_hit: false,
        }
    }

    #[test]
    fn preview_state_machine_new_request_increments_request_id() {
        let mut state = PreviewLoadStateMachine::default();
        let first = state.begin_request();
        let second = state.begin_request();
        assert_eq!(first, 1);
        assert_eq!(second, 2);
        assert_eq!(state.current_request_id, Some(second));
        assert_eq!(
            state.state,
            PreviewLoadingState::Loading { request_id: second }
        );
    }

    #[test]
    fn preview_state_machine_ignores_stale_response() {
        let mut state = PreviewLoadStateMachine::default();
        let stale = state.begin_request();
        let current = state.begin_request();
        let accepted = state.complete_request(
            stale,
            PreviewLoadOutcome::Loaded(preview_for_state_machine("stale.txt", "stale")),
        );
        assert!(!accepted);
        assert_eq!(
            state.state,
            PreviewLoadingState::Loading {
                request_id: current
            }
        );
    }

    #[test]
    fn preview_state_machine_accepts_current_response() {
        let mut state = PreviewLoadStateMachine::default();
        let current = state.begin_request();
        let preview = preview_for_state_machine("current.txt", "current");
        let accepted = state.complete_request(current, PreviewLoadOutcome::Loaded(preview.clone()));
        assert!(accepted);
        assert_eq!(
            state.state,
            PreviewLoadingState::Loaded {
                request_id: current,
                preview
            }
        );
    }

    #[test]
    fn preview_state_machine_new_request_clears_loaded_result_until_response_arrives() {
        let mut state = PreviewLoadStateMachine::default();
        let first = state.begin_request();
        assert!(state.complete_request(
            first,
            PreviewLoadOutcome::Loaded(preview_for_state_machine("first.txt", "first")),
        ));
        let second = state.begin_request();
        assert_eq!(
            state.state,
            PreviewLoadingState::Loading { request_id: second }
        );
    }

    #[test]
    fn bounded_reads_mark_beginning_only_coverage() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("large.txt");
        write_file(&path, b"abcdef");
        let mut request = PreviewRequest::new(&path);
        request.max_bytes_full_file_preview = 3;
        request.oversized_beginning_preview_bytes = 3;
        let preview = preview_file(&request);
        assert_eq!(preview.coverage, PreviewCoverage::BeginningOnly);
        assert_eq!(preview.lines[0].text, "abc");
        assert_eq!(preview.displayed_start_line, Some(1));
        assert_eq!(preview.displayed_end_line, Some(1));
        assert_eq!(preview.metadata.len_bytes, 6);
    }

    #[test]
    fn large_file_does_not_fully_load() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("large.txt");
        write_file(&path, &vec![b'a'; 1024 * 1024]);
        let mut request = PreviewRequest::new(&path);
        request.max_bytes_full_file_preview = 128;
        request.oversized_beginning_preview_bytes = 128;
        let preview = preview_file(&request);
        assert_eq!(preview.coverage, PreviewCoverage::BeginningOnly);
        assert_eq!(preview.lines[0].text.len(), 128);
        assert_eq!(preview.metadata.len_bytes, 1024 * 1024);
    }

    #[test]
    fn detects_binary_files_from_bounded_sample() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("bin.dat");
        write_file(&path, b"abc\0def");
        let preview = preview_file(&PreviewRequest::new(&path));
        assert_eq!(preview.kind, PreviewKind::Binary);
        assert_eq!(preview.coverage, PreviewCoverage::BinaryUnsupported);
        assert!(preview.lines.is_empty());
        assert!(
            preview
                .binary_or_unsupported
                .unwrap()
                .reason
                .contains("binary")
        );
    }

    #[test]
    fn invalid_utf8_is_lossy_and_user_visible() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("bad.txt");
        write_file(&path, b"ok\xffbad");
        let preview = preview_file(&PreviewRequest::new(&path));
        assert_eq!(preview.kind, PreviewKind::Text);
        assert_eq!(preview.coverage, PreviewCoverage::Complete);
        assert!(preview.lossy_utf8);
        assert_eq!(preview.lines[0].text, "ok�bad");
        assert!(
            preview
                .warnings
                .iter()
                .any(|warning| warning.contains("replacement characters"))
        );
    }

    #[test]
    fn extracts_requested_line_range() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("lines.txt");
        write_file(&path, b"one\ntwo\nthree\nfour\n");
        let mut request = PreviewRequest::new(&path);
        request.line_range = Some(PreviewLineRange { start: 2, end: 3 });
        let preview = preview_file(&request);
        assert_eq!(
            preview
                .lines
                .iter()
                .map(|line| (line.line_number, line.text.as_str()))
                .collect::<Vec<_>>(),
            vec![(2, "two"), (3, "three")]
        );
    }

    #[test]
    fn content_match_returns_context_and_match_range() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("match.txt");
        write_file(&path, b"one\ntwo\nthree\nfour\nfive\n");
        let mut request = PreviewRequest::for_match(&path, 3, 2);
        request.max_lines_around_match = 1;
        let preview = preview_file(&request);
        assert_eq!(
            preview
                .lines
                .iter()
                .map(|line| line.line_number)
                .collect::<Vec<_>>(),
            vec![2, 3, 4]
        );
        assert_eq!(
            preview.lines[1].match_ranges,
            vec![PreviewMatchRange {
                start_column: 2,
                end_column: 3
            }]
        );
    }

    #[test]
    fn small_file_below_5_mib_loads_completely_even_with_selected_match() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("small.txt");
        write_file(
            &path,
            b"one
two match
three
",
        );
        let preview = preview_file(&PreviewRequest::for_match(&path, 2, 5));
        assert_eq!(preview.coverage, PreviewCoverage::Complete);
        assert_eq!(preview.lines.len(), 3);
        assert_eq!(preview.lines[1].text, "two match");
    }

    #[test]
    fn selected_match_in_middle_of_small_file_is_included() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("small-middle.txt");
        write_file(
            &path,
            b"one
two
needle here
four
five
",
        );
        let preview = preview_file(&PreviewRequest::for_match(&path, 3, 1));
        assert_eq!(
            preview
                .lines
                .iter()
                .map(|line| line.line_number)
                .collect::<Vec<_>>(),
            vec![1, 2, 3, 4, 5]
        );
        assert_eq!(
            preview.lines[2].match_ranges[0],
            PreviewMatchRange {
                start_column: 1,
                end_column: 2
            }
        );
    }

    #[test]
    fn oversized_file_returns_context_around_late_match() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("late.txt");
        let mut contents = String::new();
        for line in 1..=10_000 {
            contents.push_str(&format!(
                "line {line}
"
            ));
        }
        write_file(&path, contents.as_bytes());
        let mut request = PreviewRequest::for_match(&path, 9_000, 6);
        request.max_bytes_full_file_preview = 128;
        request.max_lines_around_match = 2;
        let preview = preview_file(&request);
        assert_eq!(preview.coverage, PreviewCoverage::MatchContextOnly);
        assert_eq!(
            preview
                .lines
                .iter()
                .map(|line| line.line_number)
                .collect::<Vec<_>>(),
            vec![8998, 8999, 9000, 9001, 9002]
        );
        assert_eq!(preview.lines[2].text, "line 9000");
    }

    #[test]
    fn oversized_context_extraction_does_not_retain_whole_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("bounded-context.txt");
        let mut contents = String::new();
        for line in 1..=20_000 {
            contents.push_str(&format!(
                "line {line}
"
            ));
        }
        write_file(&path, contents.as_bytes());
        let mut request = PreviewRequest::for_match(&path, 15_000, 6);
        request.max_bytes_full_file_preview = 256;
        request.max_lines_around_match = 3;
        let preview = preview_file(&request);
        assert_eq!(preview.lines.len(), 7);
        assert!(
            preview
                .lines
                .iter()
                .all(|line| line.line_number >= 14_997 && line.line_number <= 15_003)
        );
    }

    #[test]
    fn oversized_source_line_numbers_are_preserved() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("numbers.txt");
        let mut contents = String::new();
        for line in 1..=300 {
            contents.push_str(&format!(
                "line {line}
"
            ));
        }
        write_file(&path, contents.as_bytes());
        let mut request = PreviewRequest::for_match(&path, 250, 1);
        request.max_bytes_full_file_preview = 32;
        request.max_lines_around_match = 1;
        let preview = preview_file(&request);
        assert_eq!(
            preview
                .lines
                .iter()
                .map(|line| line.line_number)
                .collect::<Vec<_>>(),
            vec![249, 250, 251]
        );
    }

    #[test]
    fn oversized_preview_without_match_returns_bounded_beginning_plus_warning() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("beginning.txt");
        write_file(
            &path,
            b"abcdef
ghijkl
",
        );
        let mut request = PreviewRequest::new(&path);
        request.max_bytes_full_file_preview = 4;
        request.oversized_beginning_preview_bytes = 4;
        let preview = preview_file(&request);
        assert_eq!(preview.coverage, PreviewCoverage::BeginningOnly);
        assert_eq!(preview.lines[0].text, "abcd");
        assert!(
            preview
                .binary_or_unsupported
                .unwrap()
                .reason
                .contains("beginning only")
        );
    }

    #[test]
    fn selected_line_beyond_eof_reports_missing_line_state() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("changed.txt");
        write_file(
            &path,
            b"one
two
three
",
        );
        let mut request = PreviewRequest::for_match(&path, 10, 1);
        request.max_bytes_full_file_preview = 1;
        let preview = preview_file(&request);
        assert_eq!(preview.kind, PreviewKind::Unsupported);
        assert_eq!(preview.coverage, PreviewCoverage::Unsupported);
        assert!(preview.lines.is_empty());
        assert!(
            preview
                .binary_or_unsupported
                .unwrap()
                .reason
                .contains("no longer exists")
        );
    }

    #[test]
    fn missing_file_returns_visible_read_error_coverage() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("missing.txt");
        let preview = preview_file(&PreviewRequest::new(&path));
        assert_eq!(preview.kind, PreviewKind::Error);
        assert_eq!(preview.coverage, PreviewCoverage::ReadError);
        assert!(preview.error.is_some());
        assert!(
            preview
                .warnings
                .iter()
                .any(|warning| warning.contains("Unable to read preview"))
        );
    }

    #[test]
    fn cache_hit_returns_cached_preview() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("cache.txt");
        write_file(&path, b"one");
        let mut cache = PreviewCache::new(2);
        assert!(!cache.preview(&PreviewRequest::new(&path)).cache_hit);
        assert!(cache.preview(&PreviewRequest::new(&path)).cache_hit);
    }

    #[test]
    fn cache_invalidates_after_mtime_change() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("cache.txt");
        write_file(&path, b"one");
        let mut cache = PreviewCache::new(2);
        let request = PreviewRequest::new(&path);
        assert_eq!(cache.preview(&request).lines[0].text, "one");
        sleep(Duration::from_millis(20));
        write_file(&path, b"two");
        let preview = cache.preview(&request);
        assert!(!preview.cache_hit);
        assert_eq!(preview.lines[0].text, "two");
    }

    #[test]
    fn cache_distinguishes_large_file_selected_lines() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("large-selected-lines.txt");
        let mut contents = String::new();
        for line in 1..=100 {
            contents.push_str(&format!(
                "line {line}
"
            ));
        }
        write_file(&path, contents.as_bytes());
        let mut cache = PreviewCache::new(4);
        let mut first = PreviewRequest::for_match(&path, 40, 1);
        first.max_bytes_full_file_preview = 32;
        first.max_lines_around_match = 1;
        let mut second = PreviewRequest::for_match(&path, 80, 1);
        second.max_bytes_full_file_preview = 32;
        second.max_lines_around_match = 1;

        let first_preview = cache.preview(&first);
        let second_preview = cache.preview(&second);

        assert!(!first_preview.cache_hit);
        assert!(!second_preview.cache_hit);
        assert_eq!(first_preview.displayed_start_line, Some(39));
        assert_eq!(second_preview.displayed_start_line, Some(79));
    }

    #[test]
    fn cache_invalidates_after_file_length_change() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("cache-length.txt");
        write_file(&path, b"one");
        let mut cache = PreviewCache::new(2);
        let request = PreviewRequest::new(&path);
        assert_eq!(cache.preview(&request).metadata.len_bytes, 3);
        sleep(Duration::from_millis(20));
        write_file(&path, b"three");
        let preview = cache.preview(&request);
        assert!(!preview.cache_hit);
        assert_eq!(preview.metadata.len_bytes, 5);
        assert_eq!(preview.lines[0].text, "three");
    }

    #[test]
    fn full_preview_size_limit_affects_cache_identity() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("cache-limit.txt");
        write_file(
            &path, b"abcdef
",
        );
        let mut cache = PreviewCache::new(2);
        let full = PreviewRequest::new(&path);
        let mut beginning = PreviewRequest::new(&path);
        beginning.max_bytes_full_file_preview = 3;
        beginning.oversized_beginning_preview_bytes = 3;

        assert_eq!(cache.preview(&full).coverage, PreviewCoverage::Complete);
        let preview = cache.preview(&beginning);
        assert!(!preview.cache_hit);
        assert_eq!(preview.coverage, PreviewCoverage::BeginningOnly);
        assert_eq!(preview.lines[0].text, "abc");
    }

    #[test]
    fn context_line_count_affects_cache_identity() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("cache-context-count.txt");
        let mut contents = String::new();
        for line in 1..=50 {
            contents.push_str(&format!(
                "line {line}
"
            ));
        }
        write_file(&path, contents.as_bytes());
        let mut cache = PreviewCache::new(2);
        let mut one = PreviewRequest::for_match(&path, 25, 1);
        one.max_bytes_full_file_preview = 16;
        one.max_lines_around_match = 1;
        let mut two = PreviewRequest::for_match(&path, 25, 1);
        two.max_bytes_full_file_preview = 16;
        two.max_lines_around_match = 2;

        assert_eq!(cache.preview(&one).lines.len(), 3);
        let preview = cache.preview(&two);
        assert!(!preview.cache_hit);
        assert_eq!(preview.lines.len(), 5);
    }

    #[test]
    fn beginning_preview_byte_limit_affects_cache_identity() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("cache-beginning-limit.txt");
        write_file(&path, b"abcdef\n");
        let mut cache = PreviewCache::new(2);
        let mut short = PreviewRequest::new(&path);
        short.max_bytes_full_file_preview = 3;
        short.oversized_beginning_preview_bytes = 3;
        let mut longer = PreviewRequest::new(&path);
        longer.max_bytes_full_file_preview = 3;
        longer.oversized_beginning_preview_bytes = 5;

        assert_eq!(cache.preview(&short).lines[0].text, "abc");
        let preview = cache.preview(&longer);

        assert!(!preview.cache_hit);
        assert_eq!(preview.lines[0].text, "abcde");
    }

    #[test]
    fn small_complete_file_preview_reuses_cache_for_different_selected_match() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("cache-small-selected.txt");
        write_file(
            &path,
            b"alpha
beta
gamma
",
        );
        let mut cache = PreviewCache::new(2);
        let first = PreviewRequest::for_match(&path, 1, 1);
        let second = PreviewRequest::for_match(&path, 3, 1);

        let first_preview = cache.preview(&first);
        let second_preview = cache.preview(&second);

        assert!(!first_preview.cache_hit);
        assert!(second_preview.cache_hit);
        assert_eq!(
            first_preview
                .lines
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>(),
            second_preview
                .lines
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>()
        );
        assert!(first_preview.lines[0].match_ranges.len() == 1);
        assert!(second_preview.lines[2].match_ranges.len() == 1);
    }

    #[test]
    fn directory_preview_returns_metadata_without_text() {
        let dir = tempdir().unwrap();
        let preview = preview_file(&PreviewRequest::new(dir.path()));
        assert_eq!(preview.kind, PreviewKind::Directory);
        assert_eq!(preview.coverage, PreviewCoverage::Unsupported);
        assert!(preview.metadata.is_directory);
        assert!(preview.lines.is_empty());
    }
}
