use crate::actions::clipboard;
use crate::file_search::model::{
    ContentFileResult, ContentMatch, FilenameMatchQuality, FilenameResult,
};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const FILENAME_TSV_HEADER: &[&str] = &[
    "path",
    "file name",
    "directory",
    "size",
    "modified time",
    "match quality",
];
const CONTENT_TSV_HEADER: &[&str] = &[
    "path",
    "file name",
    "directory",
    "line number",
    "line preview",
    "modified time",
    "match quality",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilenameExportRow {
    pub path: PathBuf,
    pub file_name: String,
    pub directory: String,
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
    pub modified: Option<SystemTime>,
    pub match_quality: Option<FilenameMatchQuality>,
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

pub fn filename_export_row(result: &FilenameResult) -> FilenameExportRow {
    FilenameExportRow {
        path: result.path.clone(),
        file_name: result.file_name.clone(),
        directory: result
            .parent_directory
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
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
            modified: result.modified,
            match_quality: result.filename_relevance,
        })
        .collect()
}

pub fn filename_results_tsv<'a>(rows: impl IntoIterator<Item = &'a FilenameExportRow>) -> String {
    let mut lines = vec![tsv_row(FILENAME_TSV_HEADER)];
    lines.extend(rows.into_iter().map(|row| {
        tsv_row(&[
            row.path.display().to_string(),
            row.file_name.clone(),
            row.directory.clone(),
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
            row.file_name.clone(),
            row.directory.clone(),
            row.line_number.to_string(),
            row.line_preview.clone(),
            format_system_time(row.modified),
            row.match_quality
                .map(|q| format!("{q:?}"))
                .unwrap_or_default(),
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
    fn filename_export_columns_are_stable() {
        let row = filename_export_row(&filename_result());
        let tsv = filename_results_tsv([&row]);
        let lines: Vec<_> = tsv.lines().collect();
        assert_eq!(
            lines[0],
            "path\tfile name\tdirectory\tsize\tmodified time\tmatch quality"
        );
        assert_eq!(
            lines[1],
            "/tmp/project/src/main.rs\tmain.rs\t/tmp/project/src\t42\t\tExactFilename"
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
            total_matches: 1,
            matches: vec![ContentMatch::new(7, "needle line".into(), 0, 6)],
            truncated: false,
        };
        let rows = content_export_rows(&result);
        let tsv = content_results_tsv(rows.iter());
        let lines: Vec<_> = tsv.lines().collect();
        assert_eq!(
            lines[0],
            "path\tfile name\tdirectory\tline number\tline preview\tmodified time\tmatch quality"
        );
        assert_eq!(
            lines[1],
            "/tmp/project/src/lib.rs\tlib.rs\t/tmp/project/src\t7\tneedle line\t\tFilenameContains"
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
    fn empty_result_export_has_header_only_and_empty_copy_payload() {
        assert_eq!(all_visible_filename_results([].iter()), "");
        assert_eq!(
            filename_results_tsv([].iter()),
            FILENAME_TSV_HEADER.join("\t")
        );
    }
}
