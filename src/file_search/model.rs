use std::path::PathBuf;
use std::time::SystemTime;

/// Describes whether a search should match file names or file contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SearchKind {
    Filename,
    Content,
}

/// Defines the roots a search backend should inspect.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SearchScope {
    Global,
    Directory { root: PathBuf },
    File { path: PathBuf },
}

/// Backend-ready request with all user and settings-derived search options resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchRequest {
    pub kind: SearchKind,
    pub scope: SearchScope,
    pub text: String,
    pub case_sensitive: bool,
    pub include_hidden_files: bool,
    pub max_results: usize,
    pub max_file_size_bytes: u64,
    pub included_extensions: Vec<String>,
    pub excluded_extensions: Vec<String>,
    pub excluded_directory_names: Vec<String>,
}

/// User-adjustable options that can be applied to a search request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchOptions {
    pub case_sensitive: bool,
    pub include_hidden_files: bool,
    pub max_results: usize,
    pub max_file_size_bytes: u64,
    pub included_extensions: Vec<String>,
    pub excluded_extensions: Vec<String>,
    pub excluded_directory_names: Vec<String>,
}

/// Stable identifier for a submitted search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SearchId(pub u64);

/// Search implementation selected to execute a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SearchBackend {
    Everything,
    Ripgrep,
    WalkDir,
    Native,
}

/// Lifecycle state for a search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SearchStatus {
    Pending,
    Running,
    Completed,
    Cancelled,
    Failed,
}

/// Incremental progress reported by a backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchProgress {
    pub files_scanned: u64,
    pub directories_scanned: u64,
    pub results_found: usize,
    pub status: SearchStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileKind {
    File,
    Directory,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FilenameRank {
    ExactFilename,
    FilenameStartsWith,
    FilenameContains,
    FullPathContains,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilenameResult {
    pub path: PathBuf,
    pub file_name: String,
    pub parent_directory: Option<PathBuf>,
    pub kind: FileKind,
    pub size: Option<u64>,
    pub modified: Option<SystemTime>,
    pub rank: FilenameRank,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentFileResult {
    pub path: PathBuf,
    pub total_matches: usize,
    pub matches: Vec<ContentMatch>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentMatch {
    pub line_number: usize,
    pub column: Option<usize>,
    pub line: String,
    pub byte_start: usize,
    pub byte_end: usize,
    pub ranges: Vec<ContentMatchRange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentMatchRange {
    pub byte_start: usize,
    pub byte_end: usize,
}

impl ContentMatch {
    pub fn new(line_number: usize, line: String, byte_start: usize, byte_end: usize) -> Self {
        Self {
            line_number,
            column: Some(byte_start),
            line,
            byte_start,
            byte_end,
            ranges: vec![ContentMatchRange {
                byte_start,
                byte_end,
            }],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentFileResultBuilder {
    result: ContentFileResult,
    display_limit: usize,
}

impl ContentFileResultBuilder {
    pub fn new(path: PathBuf, display_limit: usize) -> Self {
        Self {
            result: ContentFileResult {
                path,
                total_matches: 0,
                matches: Vec::new(),
                truncated: false,
            },
            display_limit,
        }
    }

    pub fn push_match(&mut self, content_match: ContentMatch) {
        self.result.total_matches = self.result.total_matches.saturating_add(1);
        if self.result.matches.len() < self.display_limit {
            self.result.matches.push(content_match);
        } else {
            self.result.truncated = true;
        }
    }

    pub fn finish(self) -> ContentFileResult {
        self.result
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchResult {
    Filename(FilenameResult),
    ContentFile(ContentFileResult),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchEvent {
    Started {
        id: SearchId,
        backend: SearchBackend,
    },
    Result {
        id: SearchId,
        result: SearchResult,
    },
    Progress {
        id: SearchId,
        progress: SearchProgress,
    },
    Completed {
        id: SearchId,
    },
    Cancelled {
        id: SearchId,
    },
    Failed {
        id: SearchId,
        error: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_match(line_number: usize) -> ContentMatch {
        ContentMatch::new(line_number, format!("line {line_number} needle"), 5, 11)
    }

    #[test]
    fn content_builder_groups_by_path() {
        let mut builder = ContentFileResultBuilder::new("src/lib.rs".into(), 10);
        builder.push_match(sample_match(1));
        builder.push_match(sample_match(2));
        let result = builder.finish();
        assert_eq!(result.path, PathBuf::from("src/lib.rs"));
        assert_eq!(result.matches.len(), 2);
    }

    #[test]
    fn content_builder_total_count_increments_past_display_limit() {
        let mut builder = ContentFileResultBuilder::new("src/lib.rs".into(), 1);
        builder.push_match(sample_match(1));
        builder.push_match(sample_match(2));
        let result = builder.finish();
        assert_eq!(result.total_matches, 2);
    }

    #[test]
    fn content_builder_enforces_per_file_display_limit() {
        let mut builder = ContentFileResultBuilder::new("src/lib.rs".into(), 1);
        builder.push_match(sample_match(1));
        builder.push_match(sample_match(2));
        let result = builder.finish();
        assert_eq!(result.matches.len(), 1);
        assert_eq!(result.matches[0].line_number, 1);
    }

    #[test]
    fn content_builder_sets_truncation_flag() {
        let mut builder = ContentFileResultBuilder::new("src/lib.rs".into(), 1);
        builder.push_match(sample_match(1));
        builder.push_match(sample_match(2));
        assert!(builder.finish().truncated);
    }

    #[test]
    fn content_match_preserves_display_data() {
        let content_match = ContentMatch::new(12, "abc needle".into(), 4, 10);
        assert_eq!(content_match.line_number, 12);
        assert_eq!(content_match.column, Some(4));
        assert_eq!(content_match.line, "abc needle");
        assert_eq!(content_match.ranges.len(), 1);
    }
}
