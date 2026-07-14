use crate::file_search::model::{
    ContentFileResult, ContentMatch, ContentMatchRange, FileKind, FilenameMatchQuality,
    FilenameRank, FilenameResult, TextMatchRange,
};
use crate::file_search::settings::{FileSearchColumn, FileSearchUiPreferences};
use eframe::egui::{self, text::LayoutJob, Color32, FontId, TextFormat, WidgetText};
use std::path::PathBuf;
use std::time::SystemTime;

pub type FilenameColumnVisibility = Vec<FileSearchColumn>;
pub type FilenameColumnWidths = std::collections::BTreeMap<FileSearchColumn, u32>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilenameResultRowPresentation {
    pub path: PathBuf,
    pub name: String,
    pub directory: String,
    pub kind: FileKind,
    pub size: Option<u64>,
    pub modified: Option<SystemTime>,
    pub rank: FilenameRank,
    pub match_quality: FilenameMatchQuality,
    pub filename_match_ranges: Vec<TextMatchRange>,
    pub path_match_ranges: Vec<TextMatchRange>,
    pub columns: Vec<RenderedColumn>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedColumn {
    pub column: FileSearchColumn,
    pub text: String,
    pub width: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentFileGroupPresentation {
    pub path: PathBuf,
    pub header: String,
    pub selectable: bool,
    pub rows: Vec<ContentLineRowPresentation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentLineRowPresentation {
    pub line_number: usize,
    pub line: String,
    pub ranges: Vec<ContentMatchRange>,
    pub selectable: bool,
}

pub fn filename_row_presentation(
    result: &FilenameResult,
    prefs: &FileSearchUiPreferences,
) -> FilenameResultRowPresentation {
    let directory = result
        .parent_directory
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let mut columns = Vec::new();
    for column in &prefs.visible_columns {
        let text = match column {
            FileSearchColumn::Name => result.file_name.clone(),
            FileSearchColumn::Directory => directory.clone(),
            FileSearchColumn::Kind => format!("{:?}", result.kind),
            FileSearchColumn::MatchQuality => format_match_quality(result.match_quality),
            FileSearchColumn::Path => result.path.display().to_string(),
            FileSearchColumn::Line | FileSearchColumn::MatchText => continue,
            FileSearchColumn::Size => result.size.map(format_size).unwrap_or_default(),
            FileSearchColumn::Modified => result.modified.map(format_modified).unwrap_or_default(),
        };
        columns.push(RenderedColumn {
            column: *column,
            text,
            width: prefs.column_widths.get(column).copied(),
        });
    }
    FilenameResultRowPresentation {
        path: result.path.clone(),
        name: result.file_name.clone(),
        directory,
        kind: result.kind,
        size: result.size,
        modified: result.modified,
        rank: result.rank,
        match_quality: result.match_quality,
        filename_match_ranges: result.filename_match_ranges.clone(),
        path_match_ranges: result.path_match_ranges.clone(),
        columns,
    }
}

pub fn content_group_presentation(result: &ContentFileResult) -> ContentFileGroupPresentation {
    ContentFileGroupPresentation {
        path: result.path.clone(),
        header: format!(
            "{} ({} match{})",
            result.path.display(),
            result.total_matches,
            if result.total_matches == 1 { "" } else { "es" }
        ),
        selectable: false,
        rows: result
            .matches
            .iter()
            .map(|m| ContentLineRowPresentation {
                line_number: m.line_number,
                line: m.line.clone(),
                ranges: m.ranges.clone(),
                selectable: true,
            })
            .collect(),
    }
}

pub fn format_match_quality(rank: FilenameMatchQuality) -> String {
    match rank {
        FilenameRank::ExactFilename => "Exact filename",
        FilenameRank::FilenameStartsWith => "Name starts with",
        FilenameRank::FilenameContains => "Name contains",
        FilenameRank::FullPathContains => "Path contains",
    }
    .to_owned()
}

fn format_size(size: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    if size >= MB {
        format!("{:.1} MB", size as f64 / MB as f64)
    } else if size >= KB {
        format!("{:.1} KB", size as f64 / KB as f64)
    } else {
        format!("{size} B")
    }
}

fn format_modified(time: SystemTime) -> String {
    let dt: chrono::DateTime<chrono::Local> = time.into();
    dt.format("%Y-%m-%d %H:%M").to_string()
}

pub fn highlighted_job(text: &str, ranges: &[TextMatchRange]) -> LayoutJob {
    let ranges: Vec<_> = ranges.iter().map(|r| (r.byte_start, r.byte_end)).collect();
    highlighted_job_from_byte_ranges(text, &ranges)
}

pub fn highlighted_content_job(text: &str, ranges: &[ContentMatchRange]) -> LayoutJob {
    let ranges: Vec<_> = ranges.iter().map(|r| (r.byte_start, r.byte_end)).collect();
    highlighted_job_from_byte_ranges(text, &ranges)
}

fn highlighted_job_from_byte_ranges(text: &str, ranges: &[(usize, usize)]) -> LayoutJob {
    let normal = TextFormat {
        font_id: FontId::proportional(14.0),
        color: Color32::WHITE,
        ..Default::default()
    };
    let highlight = TextFormat {
        font_id: FontId::proportional(14.0),
        color: Color32::BLACK,
        background: Color32::YELLOW,
        ..Default::default()
    };
    let mut job = LayoutJob::default();
    let mut cursor = 0;
    let mut sorted = ranges.to_vec();
    sorted.sort_unstable();
    for (start, end) in sorted {
        if start >= end
            || end > text.len()
            || !text.is_char_boundary(start)
            || !text.is_char_boundary(end)
            || start < cursor
        {
            continue;
        }
        if cursor < start {
            job.append(&text[cursor..start], 0.0, normal.clone());
        }
        job.append(&text[start..end], 0.0, highlight.clone());
        cursor = end;
    }
    if cursor < text.len() {
        job.append(&text[cursor..], 0.0, normal);
    }
    job
}

pub(super) fn non_wrapping_selectable_label(
    ui: &mut egui::Ui,
    selected: bool,
    text: impl Into<WidgetText>,
) -> egui::Response {
    let text = text.into();
    let button_padding = ui.spacing().button_padding;
    let total_extra = button_padding + button_padding;
    let galley = text.into_galley(ui, Some(false), f32::INFINITY, egui::TextStyle::Button);
    let mut desired_size = total_extra + galley.size();
    desired_size.y = desired_size.y.max(ui.spacing().interact_size.y);
    let (rect, response) = ui.allocate_at_least(desired_size, egui::Sense::click());

    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, selected, galley.text())
    });

    if ui.is_rect_visible(response.rect) {
        let text_pos = ui
            .layout()
            .align_size_within_rect(galley.size(), rect.shrink2(button_padding))
            .min;
        let visuals = ui.style().interact_selectable(&response, selected);
        if selected || response.hovered() || response.highlighted() || response.has_focus() {
            let rect = rect.expand(visuals.expansion);
            ui.painter().rect(
                rect,
                visuals.rounding,
                visuals.weak_bg_fill,
                visuals.bg_stroke,
            );
        }
        ui.painter().galley(text_pos, galley, visuals.text_color());
    }

    response
}

pub fn content_line_label(m: &ContentMatch, truncated: bool) -> WidgetText {
    let prefix = format!("{}: ", m.line_number);
    let mut job = LayoutJob::default();
    job.append(&prefix, 0.0, TextFormat::default());
    let highlighted = highlighted_content_job(&m.line, &m.ranges);
    for section in highlighted.sections {
        let text = &highlighted.text[section.byte_range];
        job.append(text, section.leading_space, section.format);
    }
    if truncated {
        job.append(" … truncated", 0.0, TextFormat::default());
    }
    WidgetText::LayoutJob(job)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_search::model::{ContentFileResult, FilenameRank, SearchResult};
    use crate::file_search::settings::FileSearchColumn;
    use std::collections::BTreeMap;
    use std::time::{Duration, UNIX_EPOCH};

    fn filename_result() -> FilenameResult {
        FilenameResult {
            path: PathBuf::from("/tmp/project/src/main.rs"),
            file_name: "main.rs".into(),
            parent_directory: Some(PathBuf::from("/tmp/project/src")),
            kind: FileKind::File,
            size: Some(42),
            modified: Some(UNIX_EPOCH + Duration::from_secs(60)),
            rank: FilenameRank::FilenameStartsWith,
            match_quality: FilenameRank::FilenameStartsWith,
            filename_match_ranges: vec![TextMatchRange {
                byte_start: 0,
                byte_end: 4,
            }],
            path_match_ranges: vec![TextMatchRange {
                byte_start: 13,
                byte_end: 16,
            }],
            arrival_index: 7,
        }
    }

    #[test]
    fn filename_row_preserves_size_modified_rank_and_match_ranges() {
        let prefs = FileSearchUiPreferences::default();
        let source = filename_result();
        let row = filename_row_presentation(&source, &prefs);

        assert_eq!(row.size, source.size);
        assert_eq!(row.modified, source.modified);
        assert_eq!(row.rank, source.rank);
        assert_eq!(row.match_quality, source.match_quality);
        assert_eq!(row.filename_match_ranges, source.filename_match_ranges);
        assert_eq!(row.path_match_ranges, source.path_match_ranges);
    }

    #[test]
    fn content_file_header_appears_once_per_grouped_file() {
        let file = ContentFileResult {
            path: PathBuf::from("/tmp/project/src/main.rs"),
            file_name: "main.rs".into(),
            modified: None,
            filename_relevance: None,
            arrival_index: 0,
            total_matches: 2,
            matches: vec![
                ContentMatch::new(1, "needle one".into(), 0, 6),
                ContentMatch::new(2, "needle two".into(), 0, 6),
            ],
            truncated: false,
        };
        let results = vec![SearchResult::ContentFile(file.clone())];
        let header_count = results
            .iter()
            .filter_map(|r| match r {
                SearchResult::ContentFile(content) => Some(content_group_presentation(content)),
                _ => None,
            })
            .filter(|group| group.header.contains("main.rs"))
            .count();

        assert_eq!(header_count, 1);
        assert!(!content_group_presentation(&file).selectable);
    }

    #[test]
    fn each_matching_line_has_distinct_selectable_row() {
        let file = ContentFileResult {
            path: PathBuf::from("/tmp/a.txt"),
            file_name: "a.txt".into(),
            modified: None,
            filename_relevance: None,
            arrival_index: 0,
            total_matches: 2,
            matches: vec![
                ContentMatch::new(3, "alpha needle".into(), 6, 12),
                ContentMatch::new(9, "beta needle".into(), 5, 11),
            ],
            truncated: false,
        };
        let group = content_group_presentation(&file);

        assert_eq!(group.rows.len(), 2);
        assert!(group.rows.iter().all(|row| row.selectable));
        assert_ne!(group.rows[0].line_number, group.rows[1].line_number);
    }

    #[test]
    fn highlighting_receives_correct_byte_ranges() {
        let job = highlighted_job(
            "café needle",
            &[TextMatchRange {
                byte_start: 6,
                byte_end: 12,
            }],
        );

        assert_eq!(job.text, "café needle");
        assert!(job.sections.iter().any(|section| {
            section.byte_range.start == 6
                && section.byte_range.end == 12
                && section.format.background == Color32::YELLOW
        }));
    }

    #[test]
    fn column_visibility_preferences_affect_rendered_rows() {
        let mut widths = BTreeMap::new();
        widths.insert(FileSearchColumn::Name, 120);
        let prefs = FileSearchUiPreferences {
            visible_columns: vec![FileSearchColumn::Name, FileSearchColumn::Size],
            column_widths: widths,
            ..Default::default()
        };
        let row = filename_row_presentation(&filename_result(), &prefs);
        let columns: Vec<_> = row.columns.iter().map(|c| c.column).collect();

        assert_eq!(
            columns,
            vec![FileSearchColumn::Name, FileSearchColumn::Size]
        );
        assert_eq!(row.columns[0].width, Some(120));
        assert_eq!(row.columns[1].text, "42 B");
    }
}
