pub mod callouts;
pub mod headings;
pub mod parse;
pub mod sections;
pub mod task_list;

pub use parse::analyze_markdown;

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
