use crate::actions::clipboard;
use crate::file_search::model::{
    ContentFileResult, ContentMatch, FileKind, FilenameMatchQuality, FilenameResult,
};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const FILENAME_TSV_HEADER: &[&str] = &[
    "Name",
    "Directory",
    "Full Path",
    "Type",
    "Size",
    "Modified",
    "Match Quality",
];
const CONTENT_TSV_HEADER: &[&str] = &[
    "Full Path",
    "Line",
    "Column",
    "Matching Text",
    "Total Matches In File",
    "File Truncated",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilenameExportRow {
    pub path: PathBuf,
    pub file_name: String,
    pub directory: String,
    pub kind: FileKind,
    pub size: Option<u64>,
    pub modified: Option<SystemTime>,
    pub match_quality: Option<FilenameMatchQuality>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentExportRow {
    pub path: PathBuf,
    pub file_name: String,
    pub directory: String,
    pub line_number: usize,
    pub column: Option<usize>,
    pub line_preview: String,
    pub total_matches_in_file: usize,
    pub file_truncated: bool,
}

pub fn selected_filename_result_path(result: &FilenameResult) -> String {
    result.path.display().to_string()
}

pub fn selected_content_match_line(content_match: &ContentMatch) -> String {
    content_match.line.clone()
}

pub fn all_visible_filename_results<'a>(
    rows: impl IntoIterator<Item = &'a FilenameExportRow>,
) -> String {
    rows.into_iter()
        .map(|row| row.path.display().to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn all_visible_content_results<'a>(
    rows: impl IntoIterator<Item = &'a ContentExportRow>,
) -> String {
    rows.into_iter()
        .map(|row| match row.column {
            Some(column) => format!(
                "{}:{}:{}: {}",
                row.path.display(),
                row.line_number,
                column.saturating_add(1),
                row.line_preview
            ),
            None => format!(
                "{}:{}: {}",
                row.path.display(),
                row.line_number,
                row.line_preview
            ),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn visible_full_paths<'a>(paths: impl IntoIterator<Item = &'a Path>) -> Result<String, String> {
    let mut seen = std::collections::HashSet::new();
    let mut lines = Vec::new();
    for path in paths {
        let key = crate::file_search::sorting::path_identity(path);
        if seen.insert(key) {
            lines.push(path.display().to_string());
        }
    }
    non_empty_export(lines.join("\n"))
}

pub fn non_empty_export(payload: String) -> Result<String, String> {
    if payload.is_empty() {
        Err("There are no visible file-search results to export.".to_string())
    } else {
        Ok(payload)
    }
}

pub fn filename_export_row(result: &FilenameResult) -> FilenameExportRow {
    FilenameExportRow {
        path: result.path.clone(),
        file_name: result.file_name.clone(),
        directory: result
            .parent_directory
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
        kind: result.kind,
        size: result.size,
        modified: result.modified,
        match_quality: Some(result.match_quality),
    }
}

pub fn content_export_rows(result: &ContentFileResult) -> Vec<ContentExportRow> {
    let directory = result
        .path
        .parent()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    result
        .matches
        .iter()
        .map(|m| ContentExportRow {
            path: result.path.clone(),
            file_name: result.file_name.clone(),
            directory: directory.clone(),
            line_number: m.line_number,
            column: m.column,
            line_preview: m.line.clone(),
            total_matches_in_file: result.total_matches,
            file_truncated: result.truncated,
        })
        .collect()
}

pub fn filename_results_tsv<'a>(rows: impl IntoIterator<Item = &'a FilenameExportRow>) -> String {
    let mut lines = vec![tsv_row(FILENAME_TSV_HEADER)];
    lines.extend(rows.into_iter().map(|row| {
        tsv_row(&[
            row.file_name.clone(),
            row.directory.clone(),
            row.path.display().to_string(),
            format!("{:?}", row.kind),
            row.size.map(|s| s.to_string()).unwrap_or_default(),
            format_system_time(row.modified),
            row.match_quality
                .map(|q| format!("{q:?}"))
                .unwrap_or_default(),
        ])
    }));
    lines.join("\n")
}

pub fn content_results_tsv<'a>(rows: impl IntoIterator<Item = &'a ContentExportRow>) -> String {
    let mut lines = vec![tsv_row(CONTENT_TSV_HEADER)];
    lines.extend(rows.into_iter().map(|row| {
        tsv_row(&[
            row.path.display().to_string(),
            row.line_number.to_string(),
            row.column
                .map(|column| column.saturating_add(1).to_string())
                .unwrap_or_default(),
            row.line_preview.clone(),
            row.total_matches_in_file.to_string(),
            row.file_truncated.to_string(),
        ])
    }));
    lines.join("\n")
}

pub fn tsv_row(fields: &[impl AsRef<str>]) -> String {
    fields
        .iter()
        .map(|f| escape_tsv_field(f.as_ref()))
        .collect::<Vec<_>>()
        .join("\t")
}

pub fn escape_tsv_field(field: &str) -> String {
    field.replace(['\t', '\n', '\r'], " ")
}

fn format_system_time(time: Option<SystemTime>) -> String {
    time.map(|t| {
        let dt: chrono::DateTime<chrono::Local> = t.into();
        dt.format("%Y-%m-%d %H:%M:%S").to_string()
    })
    .unwrap_or_default()
}

pub fn file_name_from_path(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string())
}

pub fn set_clipboard_text(text: &str) -> anyhow::Result<()> {
    clipboard::set_text(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_search::model::{FileKind, FilenameRank, TextMatchRange};

    fn filename_result() -> FilenameResult {
        FilenameResult {
            path: PathBuf::from("/tmp/project/src/main.rs"),
            file_name: "main.rs".into(),
            parent_directory: Some(PathBuf::from("/tmp/project/src")),
            kind: FileKind::File,
            size: Some(42),
            modified: None,
            rank: FilenameRank::ExactFilename,
            match_quality: FilenameRank::ExactFilename,
            filename_match_ranges: vec![TextMatchRange {
                byte_start: 0,
                byte_end: 4,
            }],
            path_match_ranges: Vec::new(),
            arrival_index: 0,
        }
    }

    #[test]
    fn tsv_field_escaping_replaces_control_separators() {
        assert_eq!(escape_tsv_field("a\tb\nc\rd"), "a b c d");
        assert_eq!(tsv_row(&["a\tb", "c\nd"]), "a b\tc d");
    }

    #[test]
    fn tsv_field_escaping_replaces_tabs() {
        assert_eq!(escape_tsv_field("left\tright"), "left right");
    }

    #[test]
    fn tsv_field_escaping_replaces_newlines() {
        assert_eq!(escape_tsv_field("top\nbottom\rmore"), "top bottom more");
    }

    #[test]
    fn filename_export_columns_are_stable() {
        let row = filename_export_row(&filename_result());
        let tsv = filename_results_tsv([&row]);
        let lines: Vec<_> = tsv.lines().collect();
        assert_eq!(
            lines[0],
            "Name\tDirectory\tFull Path\tType\tSize\tModified\tMatch Quality"
        );
        assert_eq!(
            lines[1],
            "main.rs\t/tmp/project/src\t/tmp/project/src/main.rs\tFile\t42\t\tExactFilename"
        );
    }

    #[test]
    fn content_export_columns_are_stable() {
        let result = ContentFileResult {
            path: PathBuf::from("/tmp/project/src/lib.rs"),
            file_name: "lib.rs".into(),
            modified: None,
            filename_relevance: Some(FilenameRank::FilenameContains),
            arrival_index: 0,
            total_matches: 3,
            matches: vec![ContentMatch::new(7, "needle line".into(), 0, 6)],
            truncated: true,
        };
        let rows = content_export_rows(&result);
        let tsv = content_results_tsv(rows.iter());
        let lines: Vec<_> = tsv.lines().collect();
        assert_eq!(
            lines[0],
            "Full Path\tLine\tColumn\tMatching Text\tTotal Matches In File\tFile Truncated"
        );
        assert_eq!(
            lines[1],
            "/tmp/project/src/lib.rs\t7\t1\tneedle line\t3\ttrue"
        );
    }

    #[test]
    fn copy_selected_filename_payload_is_path() {
        assert_eq!(
            selected_filename_result_path(&filename_result()),
            "/tmp/project/src/main.rs"
        );
    }

    #[test]
    fn copy_selected_content_payload_is_line() {
        let m = ContentMatch::new(1, "hello needle".into(), 6, 12);
        assert_eq!(selected_content_match_line(&m), "hello needle");
    }

    #[test]
    fn visible_full_paths_are_deduplicated_in_order() {
        let a = PathBuf::from("/tmp/a.txt");
        let b = PathBuf::from("/tmp/b.txt");
        assert_eq!(
            visible_full_paths([a.as_path(), b.as_path(), a.as_path()]).unwrap(),
            "/tmp/a.txt\n/tmp/b.txt"
        );
    }

    #[test]
    fn empty_result_export_has_header_only_and_empty_copy_payload() {
        assert_eq!(all_visible_filename_results([].iter()), "");
        assert_eq!(
            non_empty_export(String::new()).unwrap_err(),
            "There are no visible file-search results to export."
        );
        assert_eq!(
            filename_results_tsv([].iter()),
            FILENAME_TSV_HEADER.join("\t")
        );
    }
}
