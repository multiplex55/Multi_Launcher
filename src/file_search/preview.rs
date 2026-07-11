use crate::common::lru::LruCache;

use std::collections::hash_map::DefaultHasher;
use std::fs::{self, File};
use std::hash::{Hash, Hasher};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub const DEFAULT_MAX_BYTES_PER_PREVIEW: usize = 64 * 1024;
pub const DEFAULT_MAX_LINES_AROUND_MATCH: usize = 3;
pub const DEFAULT_BINARY_SAMPLE_BYTES: usize = 8192;
pub const DEFAULT_CACHE_CAPACITY: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PreviewRequest {
    pub path: PathBuf,
    pub line_range: Option<PreviewLineRange>,
    pub selected_match: Option<PreviewMatchSelection>,
    pub max_bytes_per_preview: usize,
    pub max_lines_around_match: usize,
}

impl PreviewRequest {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            line_range: None,
            selected_match: None,
            max_bytes_per_preview: DEFAULT_MAX_BYTES_PER_PREVIEW,
            max_lines_around_match: DEFAULT_MAX_LINES_AROUND_MATCH,
        }
    }

    pub fn for_match(path: impl Into<PathBuf>, line: usize, column: usize) -> Self {
        Self {
            selected_match: Some(PreviewMatchSelection { line, column }),
            ..Self::new(path)
        }
    }

    fn effective_line_range(&self) -> Option<PreviewLineRange> {
        self.line_range.or_else(|| {
            self.selected_match.map(|selected| {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PreviewMatchSelection {
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePreview {
    pub path: PathBuf,
    pub kind: PreviewKind,
    pub lines: Vec<PreviewLine>,
    pub metadata: PreviewMetadata,
    pub binary_or_unsupported: Option<UnsupportedPreview>,
    pub error: Option<PreviewError>,
    pub truncated: bool,
    pub cache_hit: bool,
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
    modified_hash: u64,
    line_range: Option<PreviewLineRange>,
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
        let key = cache_key(request, metadata.modified().ok());
        if let Some(cached) = self.entries.get(&key) {
            let mut preview = cached.clone();
            preview.cache_hit = true;
            return preview;
        }

        let mut preview = build_preview(request, metadata);
        preview.cache_hit = false;
        self.entries.insert(key, preview.clone());
        preview
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

fn cache_key(request: &PreviewRequest, modified: Option<SystemTime>) -> PreviewCacheKey {
    let mut hasher = DefaultHasher::new();
    modified.hash(&mut hasher);
    PreviewCacheKey {
        path: request.path.clone(),
        modified_hash: hasher.finish(),
        line_range: request.effective_line_range(),
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
            binary_or_unsupported: None,
            error: None,
            truncated: false,
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

    match read_bounded(&request.path, request.max_bytes_per_preview) {
        Ok((bytes, truncated)) => {
            let sampled = bytes.len().min(DEFAULT_BINARY_SAMPLE_BYTES);
            let metadata = to_preview_metadata(&metadata, sampled);
            if is_likely_binary(&bytes[..sampled]) {
                return FilePreview {
                    path: request.path.clone(),
                    kind: PreviewKind::Binary,
                    lines: Vec::new(),
                    metadata,
                    binary_or_unsupported: Some(UnsupportedPreview {
                        reason: "File appears to be binary".to_string(),
                    }),
                    error: None,
                    truncated,
                    cache_hit: false,
                };
            }
            text_preview(request, metadata, &bytes, truncated)
        }
        Err(error) => error_preview(&request.path, error),
    }
}

fn text_preview(
    request: &PreviewRequest,
    metadata: PreviewMetadata,
    bytes: &[u8],
    truncated: bool,
) -> FilePreview {
    let text = String::from_utf8_lossy(bytes);
    let lossy = matches!(text, std::borrow::Cow::Owned(_));
    let range = request.effective_line_range();
    let selected = request.selected_match;
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
        let mut match_ranges = Vec::new();
        if let Some(selected) = selected {
            if selected.line == line_number {
                let start = selected.column.saturating_sub(1).min(line.chars().count());
                let end = (start + 1).min(line.chars().count());
                match_ranges.push(PreviewMatchRange {
                    start_column: start + 1,
                    end_column: end + 1,
                });
            }
        }
        lines.push(PreviewLine {
            line_number,
            text: line.to_string(),
            match_ranges,
        });
    }

    FilePreview {
        path: request.path.clone(),
        kind: if lossy {
            PreviewKind::Unsupported
        } else {
            PreviewKind::Text
        },
        lines,
        metadata,
        binary_or_unsupported: lossy.then(|| UnsupportedPreview {
            reason: "File contains invalid UTF-8; preview uses replacement characters".to_string(),
        }),
        error: None,
        truncated,
        cache_hit: false,
    }
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
        truncated: false,
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
        truncated: false,
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

    #[test]
    fn bounded_reads_mark_preview_truncated() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("large.txt");
        write_file(&path, b"abcdef");
        let mut request = PreviewRequest::new(&path);
        request.max_bytes_per_preview = 3;
        let preview = preview_file(&request);
        assert!(preview.truncated);
        assert_eq!(preview.lines[0].text, "abc");
        assert_eq!(preview.metadata.len_bytes, 6);
    }

    #[test]
    fn large_file_does_not_fully_load() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("large.txt");
        write_file(&path, &vec![b'a'; 1024 * 1024]);
        let mut request = PreviewRequest::new(&path);
        request.max_bytes_per_preview = 128;
        let preview = preview_file(&request);
        assert!(preview.truncated);
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
        assert_eq!(preview.kind, PreviewKind::Unsupported);
        assert_eq!(preview.lines[0].text, "ok�bad");
        assert!(
            preview
                .binary_or_unsupported
                .unwrap()
                .reason
                .contains("invalid UTF-8")
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
    fn directory_preview_returns_metadata_without_text() {
        let dir = tempdir().unwrap();
        let preview = preview_file(&PreviewRequest::new(dir.path()));
        assert_eq!(preview.kind, PreviewKind::Directory);
        assert!(preview.metadata.is_directory);
        assert!(preview.lines.is_empty());
    }
}
