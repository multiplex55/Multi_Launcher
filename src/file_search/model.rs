use std::path::PathBuf;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilenameResult {
    pub path: PathBuf,
    pub file_name: String,
    pub kind: FileKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentFileResult {
    pub path: PathBuf,
    pub matches: Vec<ContentMatch>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentMatch {
    pub line_number: usize,
    pub line: String,
    pub byte_start: usize,
    pub byte_end: usize,
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
