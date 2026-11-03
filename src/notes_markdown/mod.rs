pub mod authoring;
pub mod callouts;
pub mod headings;
pub mod html;
pub mod links;
pub mod mutations;
pub mod parse;
pub mod preview;
pub mod sections;
pub mod tables;
pub mod task_list;

pub use authoring::{
    AppliedMarkdownEdit, MAX_MARKDOWN_SNIPPET_BYTES, MAX_MARKDOWN_SOURCE_BYTES,
    MarkdownBlockSnippet, MarkdownEditError, MarkdownInlineFormat, PreparedMarkdownEdit,
    prepare_block_snippet, prepare_inline_format,
};
pub use html::{
    LocalRasterAsset, LocalRasterFormat, LocalRasterResolver, MAX_LOCAL_RASTER_AGGREGATE_BYTES,
    MAX_LOCAL_RASTER_BYTES, MAX_MARKDOWN_HTML_BYTES, MarkdownExportArtifact, MarkdownExportFormat,
    MarkdownExportWarnings, MarkdownHtmlError, export_html, export_markdown,
};
pub use links::{PlainLinkConversionReport, WrapLinksReport, convert_plain_links, wrap_plain_urls};
pub use parse::{analyze_markdown, analyze_markdown_with_max_outline_depth};
pub use preview::{
    MAX_MARKDOWN_PREVIEW_LINES, MAX_MERMAID_FENCE_BYTES, MAX_MERMAID_PREVIEW_AGGREGATE_BYTES,
    MAX_MERMAID_PREVIEW_FENCES, MAX_MERMAID_PREVIEW_SVG_BYTES, MarkdownPreview,
    MarkdownPreviewError, MarkdownPreviewLimit, MarkdownPreviewStatus, MermaidFencePreview,
    MermaidFenceStatus,
};
pub use tables::{
    MAX_MARKDOWN_TABLE_CELLS, MAX_MARKDOWN_TABLES, MarkdownTableAlignment, MarkdownTableRow,
    ParsedMarkdownTable, TableEditError, parse_markdown_tables, prepare_insert_column,
    prepare_insert_row, prepare_insert_table, prepare_remove_column, prepare_remove_row,
    prepare_set_cell, table_at_byte,
};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MarkdownAnalysis {
    pub headings: Vec<MarkdownHeading>,
    pub sections: Vec<MarkdownSection>,
    pub task_items: Vec<MarkdownTaskItem>,
    pub callouts: Vec<MarkdownCallout>,
    pub outline: Vec<OutlineRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownHeading {
    pub level: u8,
    pub title: String,
    pub normalized_anchor: String,
    pub line_index: usize,
    pub byte_range: std::ops::Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownSection {
    pub heading: MarkdownHeading,
    pub body_line_range: std::ops::Range<usize>,
    pub body_byte_range: std::ops::Range<usize>,
    pub nested_heading_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownTaskItem {
    pub line_index: usize,
    pub line_byte_range: std::ops::Range<usize>,
    pub marker_byte_range: std::ops::Range<usize>,
    pub checked: bool,
    pub indent: usize,
    pub bullet_marker: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownCallout {
    pub kind: String,
    pub title: String,
    pub line_range: std::ops::Range<usize>,
    pub byte_range: std::ops::Range<usize>,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineRow {
    pub level: u8,
    pub title: String,
    pub normalized_anchor: String,
    pub line_index: usize,
}
