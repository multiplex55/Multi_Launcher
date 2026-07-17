use crate::file_search::model::{
    ContentFileResult, ContentMatch, ContentMatchRange, FileKind, FilenameRank, FilenameResult,
};
use std::path::{Path, PathBuf};

pub fn filename_result(path: impl Into<PathBuf>, rank: FilenameRank) -> FilenameResult {
    let path = path.into();
    FilenameResult {
        file_name: path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string()),
        parent_directory: path.parent().map(Path::to_path_buf),
        path,
        kind: FileKind::File,
        size: None,
        modified: None,
        rank,
        match_quality: rank,
        filename_match_ranges: Vec::new(),
        path_match_ranges: Vec::new(),
        arrival_index: 0,
    }
}

pub fn content_match(
    line_number: usize,
    line: impl Into<String>,
    start: usize,
    end: usize,
) -> ContentMatch {
    ContentMatch {
        line_number,
        column: Some(start),
        line: line.into(),
        byte_start: start,
        byte_end: end,
        ranges: vec![ContentMatchRange {
            byte_start: start,
            byte_end: end,
        }],
    }
}

pub fn content_file_result(
    path: impl Into<PathBuf>,
    arrival_index: usize,
    total_matches: usize,
) -> ContentFileResult {
    let path = path.into();
    ContentFileResult {
        file_name: path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string()),
        path,
        modified: None,
        filename_relevance: None,
        arrival_index,
        total_matches,
        matches: vec![content_match(arrival_index + 1, "needle", 0, 6)],
        truncated: false,
    }
}
