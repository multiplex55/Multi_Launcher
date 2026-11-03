//! Parser-offset Markdown table inspection and bounded contextual edits.

use std::{error::Error, fmt, ops::Range, sync::Arc};

use pulldown_cmark::{Alignment, Event, Options, Parser, Tag, TagEnd};

use super::authoring::{
    MAX_MARKDOWN_SOURCE_BYTES, MarkdownEditError, PreparedMarkdownEdit, dominant_newline,
    validate_source,
};

pub const MAX_MARKDOWN_TABLES: usize = 256;
pub const MAX_MARKDOWN_TABLE_CELLS: usize = 16_384;
const MAX_MARKDOWN_TABLE_EVENTS: usize = 1_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkdownTableAlignment {
    None,
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkdownTableRow {
    byte_range: Range<usize>,
    cell_ranges: Vec<Range<usize>>,
    pub is_header: bool,
}

impl MarkdownTableRow {
    pub fn byte_range(&self) -> Range<usize> {
        self.byte_range.clone()
    }

    pub fn cell_ranges(&self) -> &[Range<usize>] {
        &self.cell_ranges
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedMarkdownTable {
    source_snapshot: Arc<str>,
    byte_range: Range<usize>,
    alignments: Vec<MarkdownTableAlignment>,
    rows: Vec<MarkdownTableRow>,
    nested_container: bool,
}

impl ParsedMarkdownTable {
    pub fn byte_range(&self) -> Range<usize> {
        self.byte_range.clone()
    }

    pub fn alignments(&self) -> &[MarkdownTableAlignment] {
        &self.alignments
    }

    pub fn rows(&self) -> &[MarkdownTableRow] {
        &self.rows
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TableEditError {
    SourceTooLarge { max_bytes: usize },
    TooManyTables { max: usize },
    TooManyCells { max: usize },
    TooManyEvents { max: usize },
    NotFound,
    InvalidRow,
    InvalidColumn,
    InvalidCellText,
    MalformedTable,
    UnsupportedContainerContext,
    Edit(MarkdownEditError),
}

impl fmt::Display for TableEditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceTooLarge { max_bytes } => {
                write!(f, "Markdown source exceeds the {max_bytes}-byte limit")
            }
            Self::TooManyTables { max } => write!(f, "Markdown table count exceeds {max}"),
            Self::TooManyCells { max } => write!(f, "Markdown table cell count exceeds {max}"),
            Self::TooManyEvents { max } => write!(f, "Markdown table event count exceeds {max}"),
            Self::NotFound => f.write_str("no Markdown table contains that source position"),
            Self::InvalidRow => f.write_str("table row index is invalid"),
            Self::InvalidColumn => f.write_str("table column index is invalid"),
            Self::InvalidCellText => f.write_str("table cell text must be a single line"),
            Self::MalformedTable => f.write_str("Markdown table structure is incomplete"),
            Self::UnsupportedContainerContext => {
                f.write_str("structural edits inside blockquotes or lists are not supported")
            }
            Self::Edit(error) => error.fmt(f),
        }
    }
}

impl Error for TableEditError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Edit(error) => Some(error),
            _ => None,
        }
    }
}

impl From<MarkdownEditError> for TableEditError {
    fn from(error: MarkdownEditError) -> Self {
        Self::Edit(error)
    }
}

pub fn parse_markdown_tables(source: &str) -> Result<Vec<ParsedMarkdownTable>, TableEditError> {
    validate_source(source).map_err(map_source_error)?;
    let options = Options::ENABLE_TABLES;
    let mut tables = Vec::new();
    let source_snapshot: Arc<str> = Arc::from(source);
    let mut current: Option<TableBuilder> = None;
    let mut row: Option<RowBuilder> = None;
    let mut total_cells = 0usize;
    let mut event_count = 0usize;
    let mut nested_container_depth = 0usize;

    for (event, range) in Parser::new_ext(source, options).into_offset_iter() {
        event_count = event_count.saturating_add(1);
        if event_count > MAX_MARKDOWN_TABLE_EVENTS {
            return Err(TableEditError::TooManyEvents {
                max: MAX_MARKDOWN_TABLE_EVENTS,
            });
        }
        match event {
            Event::Start(Tag::BlockQuote(_) | Tag::List(_) | Tag::Item) => {
                nested_container_depth = nested_container_depth.saturating_add(1);
            }
            Event::End(TagEnd::BlockQuote(_) | TagEnd::List(_) | TagEnd::Item) => {
                nested_container_depth = nested_container_depth.saturating_sub(1);
            }
            Event::Start(Tag::Table(alignments)) => {
                if tables.len() >= MAX_MARKDOWN_TABLES {
                    return Err(TableEditError::TooManyTables {
                        max: MAX_MARKDOWN_TABLES,
                    });
                }
                current = Some(TableBuilder {
                    byte_range: range,
                    alignments: alignments.into_iter().map(map_alignment).collect(),
                    rows: Vec::new(),
                    nested_container: nested_container_depth > 0,
                });
            }
            Event::Start(Tag::TableHead) => {
                row = Some(RowBuilder {
                    byte_range: range,
                    cell_ranges: Vec::new(),
                    is_header: true,
                });
            }
            Event::Start(Tag::TableRow) => {
                row = Some(RowBuilder {
                    byte_range: range,
                    cell_ranges: Vec::new(),
                    is_header: false,
                });
            }
            Event::Start(Tag::TableCell) => {
                total_cells = total_cells.saturating_add(1);
                if total_cells > MAX_MARKDOWN_TABLE_CELLS {
                    return Err(TableEditError::TooManyCells {
                        max: MAX_MARKDOWN_TABLE_CELLS,
                    });
                }
                if let Some(active_row) = row.as_mut() {
                    active_row.cell_ranges.push(range);
                }
            }
            Event::End(TagEnd::TableHead | TagEnd::TableRow) => {
                if let Some(active_row) = row.take()
                    && let Some(table) = current.as_mut()
                {
                    let bounds = row_line_bounds(source, &active_row.cell_ranges)
                        .unwrap_or(active_row.byte_range);
                    table.rows.push(MarkdownTableRow {
                        byte_range: bounds,
                        cell_ranges: active_row.cell_ranges,
                        is_header: active_row.is_header,
                    });
                }
            }
            Event::End(TagEnd::Table) => {
                if let Some(mut table) = current.take() {
                    table.byte_range.start = range.start.min(table.byte_range.start);
                    table.byte_range.end = range.end.max(table.byte_range.end);
                    if !table.rows.is_empty() {
                        tables.push(ParsedMarkdownTable {
                            source_snapshot: Arc::clone(&source_snapshot),
                            byte_range: clamp_range(source, table.byte_range),
                            alignments: table.alignments,
                            rows: table.rows,
                            nested_container: table.nested_container,
                        });
                    }
                }
            }
            _ => {}
        }
    }
    Ok(tables)
}

pub fn table_at_byte(
    source: &str,
    byte_index: usize,
) -> Result<Option<ParsedMarkdownTable>, TableEditError> {
    if byte_index > source.len() || !source.is_char_boundary(byte_index) {
        return Err(TableEditError::InvalidRow);
    }
    Ok(parse_markdown_tables(source)?
        .into_iter()
        .find(|table| byte_index >= table.byte_range.start && byte_index <= table.byte_range.end))
}

pub fn prepare_insert_table(
    source: &str,
    byte_index: usize,
    header_cells: &[String],
) -> Result<PreparedMarkdownEdit, TableEditError> {
    validate_source(source).map_err(map_source_error)?;
    if byte_index > source.len() || !source.is_char_boundary(byte_index) {
        return Err(TableEditError::InvalidRow);
    }
    if header_cells.is_empty() || header_cells.len() > MAX_MARKDOWN_TABLE_CELLS {
        return Err(TableEditError::InvalidColumn);
    }
    validate_cells(header_cells)?;
    let newline = dominant_newline(source);
    let mut insertion = String::new();
    let prefix = &source[..byte_index];
    if !prefix.is_empty() {
        if !prefix.ends_with('\n') {
            insertion.push_str(newline);
        }
        if !prefix.ends_with(&format!("{newline}{newline}")) {
            insertion.push_str(newline);
        }
    }
    let escaped: Vec<_> = header_cells.iter().map(|cell| escape_cell(cell)).collect();
    write_row(&mut insertion, &escaped, newline);
    insertion.push_str(newline);
    write_separator(
        &mut insertion,
        &vec![MarkdownTableAlignment::None; header_cells.len()],
        newline,
    );
    insertion.push_str(newline);
    if byte_index < source.len() && !source[byte_index..].starts_with(newline) {
        insertion.push_str(newline);
    }
    if insertion.len() > MAX_MARKDOWN_SOURCE_BYTES {
        return Err(TableEditError::SourceTooLarge {
            max_bytes: MAX_MARKDOWN_SOURCE_BYTES,
        });
    }
    PreparedMarkdownEdit::prepare(source, byte_index..byte_index, insertion).map_err(Into::into)
}

pub fn prepare_insert_row(
    source: &str,
    table: &ParsedMarkdownTable,
    after_row: usize,
    cells: &[String],
) -> Result<PreparedMarkdownEdit, TableEditError> {
    validate_structural_edit_table(source, table)?;
    if after_row >= table.rows.len() {
        return Err(TableEditError::InvalidRow);
    }
    let columns = table.alignments.len();
    if cells.len() != columns {
        return Err(TableEditError::InvalidColumn);
    }
    validate_cells(cells)?;
    let mut rows = read_rows(source, table)?;
    rows.insert(
        after_row + 1,
        cells.iter().map(|cell| escape_cell(cell)).collect(),
    );
    prepare_rewrite(source, table, rows, table.alignments.clone())
}

pub fn prepare_remove_row(
    source: &str,
    table: &ParsedMarkdownTable,
    row_index: usize,
) -> Result<PreparedMarkdownEdit, TableEditError> {
    validate_structural_edit_table(source, table)?;
    if row_index == 0 || row_index >= table.rows.len() {
        return Err(TableEditError::InvalidRow);
    }
    let mut rows = read_rows(source, table)?;
    rows.remove(row_index);
    prepare_rewrite(source, table, rows, table.alignments.clone())
}

pub fn prepare_insert_column(
    source: &str,
    table: &ParsedMarkdownTable,
    column_index: usize,
    header: &str,
) -> Result<PreparedMarkdownEdit, TableEditError> {
    validate_structural_edit_table(source, table)?;
    if column_index > table.alignments.len() {
        return Err(TableEditError::InvalidColumn);
    }
    validate_cells(std::slice::from_ref(&header.to_owned()))?;
    let mut rows = read_rows(source, table)?;
    for (row_index, row) in rows.iter_mut().enumerate() {
        row.insert(
            column_index,
            if row_index == 0 {
                escape_cell(header)
            } else {
                String::new()
            },
        );
    }
    let mut alignments = table.alignments.clone();
    alignments.insert(column_index, MarkdownTableAlignment::None);
    prepare_rewrite(source, table, rows, alignments)
}

pub fn prepare_remove_column(
    source: &str,
    table: &ParsedMarkdownTable,
    column_index: usize,
) -> Result<PreparedMarkdownEdit, TableEditError> {
    validate_structural_edit_table(source, table)?;
    if column_index >= table.alignments.len() || table.alignments.len() <= 1 {
        return Err(TableEditError::InvalidColumn);
    }
    let mut rows = read_rows(source, table)?;
    for row in &mut rows {
        row.remove(column_index);
    }
    let mut alignments = table.alignments.clone();
    alignments.remove(column_index);
    prepare_rewrite(source, table, rows, alignments)
}

pub fn prepare_set_cell(
    source: &str,
    table: &ParsedMarkdownTable,
    row_index: usize,
    column_index: usize,
    value: &str,
) -> Result<PreparedMarkdownEdit, TableEditError> {
    validate_table_source(source, table)?;
    if value.contains(['\r', '\n']) || value.len() > super::authoring::MAX_MARKDOWN_SNIPPET_BYTES {
        return Err(TableEditError::InvalidCellText);
    }
    let row = table
        .rows
        .get(row_index)
        .ok_or(TableEditError::InvalidRow)?;
    let range = row
        .cell_ranges
        .get(column_index)
        .ok_or(TableEditError::InvalidColumn)?
        .clone();
    let cell_source = source
        .get(range.clone())
        .ok_or(TableEditError::MalformedTable)?;
    let content_start = cell_source.len() - cell_source.trim_start().len();
    let content_end = cell_source.trim_end().len().max(content_start);
    let content_range = range.start + content_start..range.start + content_end;
    PreparedMarkdownEdit::prepare(source, content_range, escape_cell(value)).map_err(Into::into)
}

fn validate_table_source(source: &str, table: &ParsedMarkdownTable) -> Result<(), TableEditError> {
    validate_source(source).map_err(map_source_error)?;
    if table.source_snapshot.as_ref() != source {
        return Err(TableEditError::Edit(MarkdownEditError::SourceChanged));
    }
    if table.byte_range.end > source.len()
        || !source.is_char_boundary(table.byte_range.start)
        || !source.is_char_boundary(table.byte_range.end)
        || table.rows.is_empty()
        || table.alignments.is_empty()
    {
        return Err(TableEditError::MalformedTable);
    }
    Ok(())
}

fn validate_structural_edit_table(
    source: &str,
    table: &ParsedMarkdownTable,
) -> Result<(), TableEditError> {
    validate_table_source(source, table)?;
    if table.nested_container {
        return Err(TableEditError::UnsupportedContainerContext);
    }
    Ok(())
}

fn read_rows(
    source: &str,
    table: &ParsedMarkdownTable,
) -> Result<Vec<Vec<String>>, TableEditError> {
    table
        .rows
        .iter()
        .map(|row| {
            if row.cell_ranges.len() != table.alignments.len() {
                return Err(TableEditError::MalformedTable);
            }
            row.cell_ranges
                .iter()
                .map(|range| {
                    source
                        .get(range.clone())
                        .map(|value| value.trim().to_owned())
                        .ok_or(TableEditError::MalformedTable)
                })
                .collect()
        })
        .collect()
}

fn prepare_rewrite(
    source: &str,
    table: &ParsedMarkdownTable,
    rows: Vec<Vec<String>>,
    alignments: Vec<MarkdownTableAlignment>,
) -> Result<PreparedMarkdownEdit, TableEditError> {
    if rows.is_empty() || rows.len() > MAX_MARKDOWN_TABLE_CELLS || alignments.is_empty() {
        return Err(TableEditError::MalformedTable);
    }
    if rows.iter().any(|row| row.len() != alignments.len()) {
        return Err(TableEditError::MalformedTable);
    }
    let raw = source
        .get(table.byte_range.clone())
        .ok_or(TableEditError::MalformedTable)?;
    let newline = dominant_newline(raw);
    let trailing_newline = raw.ends_with(['\r', '\n']);
    let mut replacement = String::new();
    write_row(&mut replacement, &rows[0], newline);
    replacement.push_str(newline);
    write_separator(&mut replacement, &alignments, newline);
    for row in rows.iter().skip(1) {
        replacement.push_str(newline);
        write_row(&mut replacement, row, newline);
    }
    if trailing_newline {
        replacement.push_str(newline);
    }
    if replacement.len() > MAX_MARKDOWN_SOURCE_BYTES {
        return Err(TableEditError::SourceTooLarge {
            max_bytes: MAX_MARKDOWN_SOURCE_BYTES,
        });
    }
    PreparedMarkdownEdit::prepare(source, table.byte_range.clone(), replacement).map_err(Into::into)
}

fn write_row(output: &mut String, cells: &[String], _newline: &str) {
    output.push('|');
    for cell in cells {
        output.push(' ');
        output.push_str(cell);
        output.push_str(" |");
    }
}

fn write_separator(output: &mut String, alignments: &[MarkdownTableAlignment], _newline: &str) {
    output.push('|');
    for alignment in alignments {
        let cell = match alignment {
            MarkdownTableAlignment::None => "---",
            MarkdownTableAlignment::Left => ":---",
            MarkdownTableAlignment::Center => ":---:",
            MarkdownTableAlignment::Right => "---:",
        };
        output.push(' ');
        output.push_str(cell);
        output.push_str(" |");
    }
}

fn validate_cells(cells: &[String]) -> Result<(), TableEditError> {
    if cells.len() > MAX_MARKDOWN_TABLE_CELLS {
        return Err(TableEditError::TooManyCells {
            max: MAX_MARKDOWN_TABLE_CELLS,
        });
    }
    let mut estimated_bytes = 0usize;
    for cell in cells {
        if cell.len() > super::authoring::MAX_MARKDOWN_SNIPPET_BYTES || cell.contains(['\r', '\n'])
        {
            return Err(TableEditError::InvalidCellText);
        }
        let cell_bytes = cell.chars().fold(0usize, |sum, character| {
            sum.saturating_add(escaped_cell_len(character))
        });
        estimated_bytes = estimated_bytes.saturating_add(cell_bytes);
        if estimated_bytes > MAX_MARKDOWN_SOURCE_BYTES {
            return Err(TableEditError::SourceTooLarge {
                max_bytes: MAX_MARKDOWN_SOURCE_BYTES,
            });
        }
    }
    Ok(())
}

fn escaped_cell_len(character: char) -> usize {
    match character {
        '&' => 5,
        '<' | '>' => 4,
        '|' => 6,
        _ => character.len_utf8(),
    }
}

fn escape_cell(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('|', "&#124;")
}

fn row_line_bounds(source: &str, cells: &[Range<usize>]) -> Option<Range<usize>> {
    let start = cells.iter().map(|range| range.start).min()?;
    let end = cells.iter().map(|range| range.end).max()?;
    let line_start = source[..start].rfind('\n').map_or(0, |index| index + 1);
    let line_end = source[end..]
        .find('\n')
        .map_or(source.len(), |index| end + index + 1);
    Some(line_start..line_end)
}

fn clamp_range(source: &str, range: Range<usize>) -> Range<usize> {
    let start = range.start.min(source.len());
    let end = range.end.min(source.len()).max(start);
    start..end
}

fn map_alignment(alignment: Alignment) -> MarkdownTableAlignment {
    match alignment {
        Alignment::None => MarkdownTableAlignment::None,
        Alignment::Left => MarkdownTableAlignment::Left,
        Alignment::Center => MarkdownTableAlignment::Center,
        Alignment::Right => MarkdownTableAlignment::Right,
    }
}

fn map_source_error(error: MarkdownEditError) -> TableEditError {
    match error {
        MarkdownEditError::SourceTooLarge { max_bytes } => {
            TableEditError::SourceTooLarge { max_bytes }
        }
        other => TableEditError::Edit(other),
    }
}

struct TableBuilder {
    byte_range: Range<usize>,
    alignments: Vec<MarkdownTableAlignment>,
    rows: Vec<MarkdownTableRow>,
    nested_container: bool,
}

struct RowBuilder {
    byte_range: Range<usize>,
    cell_ranges: Vec<Range<usize>>,
    is_header: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_offsets_and_contextual_edits_use_parser_cells() {
        let source =
            "Before\r\n| Name | Value |\r\n| :--- | ---: |\r\n| Café | `a\\|b` |\r\n\r\nAfter\r\n";
        let table = parse_markdown_tables(source).unwrap().remove(0);
        assert_eq!(
            table.alignments,
            vec![MarkdownTableAlignment::Left, MarkdownTableAlignment::Right]
        );
        assert_eq!(table.rows.len(), 2);
        assert!(source[table.rows[1].cell_ranges[1].clone()].contains('`'));

        let inserted = prepare_insert_column(source, &table, 1, "Extra")
            .unwrap()
            .apply(source)
            .unwrap();
        assert!(inserted.source.contains("| :--- | --- | ---: |"));
        assert!(inserted.source.contains("| Café |  | `a\\|b` |"));
        assert!(inserted.source.starts_with("Before\r\n"));
        assert!(inserted.source.ends_with("\r\nAfter\r\n"));

        let reparsed = parse_markdown_tables(&inserted.source).unwrap().remove(0);
        let added = prepare_insert_row(
            &inserted.source,
            &reparsed,
            1,
            &["next".into(), "with | pipe".into(), "日記".into()],
        )
        .unwrap()
        .apply(&inserted.source)
        .unwrap();
        assert!(added.source.contains("with &#124; pipe"));
        assert_eq!(
            parse_markdown_tables(&added.source).unwrap()[0].rows.len(),
            3
        );
    }

    #[test]
    fn table_cell_replacement_and_row_removal_are_source_bound() {
        let source = "| A | B |\n| --- | --- |\n| one | two |\n";
        let table = parse_markdown_tables(source).unwrap().remove(0);
        let changed = prepare_set_cell(source, &table, 1, 0, "new | value")
            .unwrap()
            .apply(source)
            .unwrap();
        assert!(changed.source.contains("new &#124; value"));
        assert_eq!(
            prepare_remove_row("different", &table, 1).unwrap_err(),
            TableEditError::Edit(MarkdownEditError::SourceChanged)
        );
        let reparsed = parse_markdown_tables(source).unwrap().remove(0);
        let removed = prepare_remove_row(source, &reparsed, 1)
            .unwrap()
            .apply(source)
            .unwrap();
        assert_eq!(
            parse_markdown_tables(&removed.source).unwrap()[0]
                .rows
                .len(),
            1
        );
    }

    #[test]
    fn table_insertion_is_bounded_and_preserves_surrounding_source() {
        let source = "Intro\r\n\r\nAfter\r\n";
        let insert_at = source.find("After").unwrap();
        let edit =
            prepare_insert_table(source, insert_at, &["Name".into(), "Value".into()]).unwrap();
        let updated = edit.apply(source).unwrap().source;
        assert!(
            updated.starts_with("Intro\r\n\r\n| Name | Value |\r\n| --- | --- |\r\n\r\nAfter\r\n")
        );
        let tables = parse_markdown_tables(&updated).unwrap();
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].alignments.len(), 2);
    }

    #[test]
    fn table_structural_edits_reject_nested_containers_without_flattening() {
        let quote = "> | A | B |\n> | --- | --- |\n> | x | y |\n";
        let quote_table = parse_markdown_tables(quote).unwrap().remove(0);
        assert_eq!(
            prepare_insert_row(quote, &quote_table, 1, &["z".into(), "q".into()]),
            Err(TableEditError::UnsupportedContainerContext)
        );
        let cell = prepare_set_cell(quote, &quote_table, 1, 0, "updated")
            .unwrap()
            .apply(quote)
            .unwrap();
        assert_eq!(
            cell.source,
            "> | A | B |\n> | --- | --- |\n> | updated | y |\n"
        );

        let list = "-\n  | A | B |\n  | --- | --- |\n  | x | y |\n";
        let list_table = parse_markdown_tables(list).unwrap().remove(0);
        assert_eq!(
            prepare_remove_column(list, &list_table, 1),
            Err(TableEditError::UnsupportedContainerContext)
        );
    }
}
