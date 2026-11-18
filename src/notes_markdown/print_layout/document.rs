//! Bounded, device-independent pagination of Markdown text and tables.
use std::{collections::HashMap, sync::Arc};
use unicode_segmentation::UnicodeSegmentation;

use super::markdown::{self, ColumnAlignment, NodeKind, ParsedMarkdown};
use super::{
    MAX_PAGES, PageGeometry, Point, PrintSettings, Rect, ShapedText, TextRole, line_height,
    render_template, shape_line,
};

const MAX_DRAWABLES: usize = 120_000;
const MAX_SHAPES: usize = 100_000;
const MAX_WARNINGS: usize = 1024;
const MAX_COLUMNS: usize = 128;
const GAP: f32 = 4.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrintError {
    Cancelled,
    InvalidSettings(String),
    InvalidMarkdown,
    Limit(&'static str),
    Unplaceable(&'static str),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrintWarning {
    pub message: String,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextStyle {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub enum LinkDestination {
    External(Arc<str>),
    Internal { page: usize, point: Point },
}
#[derive(Clone, Debug, PartialEq)]
pub enum Drawable {
    /// Baseline is page-relative, top-left origin and positive Y down; glyph offsets remain font-up.
    Text {
        text: Arc<str>,
        shape: Arc<ShapedText>,
        baseline: Point,
        style: TextStyle,
    },
    Line {
        from: Point,
        to: Point,
        width: f32,
    },
    Link {
        bounds: Rect,
        destination: LinkDestination,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub struct Page {
    pub geometry: PageGeometry,
    pub drawables: Vec<Drawable>,
    pub painted_bounds: Option<Rect>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PaginatedDocument {
    pub pages: Vec<Page>,
    pub warnings: Vec<PrintWarning>,
}

#[derive(Clone)]
struct Span {
    text: String,
    style: TextStyle,
    link: Option<Arc<str>>,
}
#[derive(Clone)]
struct Fragment {
    text: Arc<str>,
    style: TextStyle,
    link: Option<Arc<str>>,
    shape: Arc<ShapedText>,
}
#[derive(Clone, Default)]
struct Line {
    fragments: Vec<Fragment>,
    width: f32,
    left: f32,
    right: f32,
    ascent: f32,
    descent: f32,
}
struct Cell {
    lines: Vec<Line>,
}
struct Table {
    alignment: Vec<ColumnAlignment>,
    rows: Vec<Vec<Vec<Span>>>,
}
enum Block {
    Text {
        spans: Vec<Span>,
        indent: f32,
        heading: Option<String>,
        size: f32,
        keep_next: bool,
    },
    Table {
        table: Table,
        indent: f32,
    },
    Rule {
        indent: f32,
    },
    Break,
}

struct Builder<'a, F: FnMut() -> bool> {
    settings: &'a PrintSettings,
    geometry: PageGeometry,
    cancelled: F,
    pages: Vec<Page>,
    y: f32,
    work: usize,
    shapes: usize,
    glyph_work: usize,
    warnings: Vec<PrintWarning>,
    anchors: HashMap<String, (usize, Point)>,
    links: Vec<(usize, usize, Arc<str>)>,
}
impl<F: FnMut() -> bool> Builder<'_, F> {
    fn check(&mut self) -> Result<(), PrintError> {
        if (self.cancelled)() {
            return Err(PrintError::Cancelled);
        }
        self.work += 1;
        if self.work > MAX_SHAPES {
            return Err(PrintError::Limit("print layout work limit exceeded"));
        }
        Ok(())
    }
    fn warn(&mut self, message: impl Into<String>) {
        if self.warnings.len() < MAX_WARNINGS {
            self.warnings.push(PrintWarning {
                message: message.into(),
            });
        }
    }
    fn page(&mut self) -> Result<(), PrintError> {
        self.check()?;
        if self.pages.len() >= MAX_PAGES {
            return Err(PrintError::Limit("print page limit exceeded"));
        }
        self.pages.push(Page {
            geometry: self.geometry,
            drawables: Vec::new(),
            painted_bounds: None,
        });
        self.y = self.geometry.content.y;
        Ok(())
    }
    fn push(&mut self, drawable: Drawable) -> Result<usize, PrintError> {
        self.shapes += 1;
        if self.shapes > MAX_DRAWABLES {
            return Err(PrintError::Limit("print drawable limit exceeded"));
        }
        let page = self.pages.last_mut().ok_or(PrintError::InvalidMarkdown)?;
        let index = page.drawables.len();
        page.drawables.push(drawable);
        Ok(index)
    }
    fn shape(
        &mut self,
        text: &str,
        style: TextStyle,
        size: f32,
    ) -> Result<Arc<ShapedText>, PrintError> {
        self.check()?;
        let shape = shape_line(
            text,
            if style.code {
                TextRole::Code
            } else {
                TextRole::Body
            },
            size,
        )
        .map_err(|e| PrintError::Unplaceable(e))?;
        self.glyph_work = self
            .glyph_work
            .checked_add(shape.runs.iter().map(|r| r.glyphs.len()).sum::<usize>())
            .ok_or(PrintError::Limit("print glyph work limit exceeded"))?;
        if self.glyph_work > 1_000_000 {
            return Err(PrintError::Limit("print glyph work limit exceeded"));
        }
        if !shape.unsupported.is_empty() {
            self.warn("Some characters are unavailable in the embedded fonts.");
        }
        Ok(Arc::new(shape))
    }
    fn fit(&mut self, height: f32) -> Result<(), PrintError> {
        if height > self.geometry.content.height + 0.001 {
            return Err(PrintError::Unplaceable(
                "A line is taller than the printable area.",
            ));
        }
        if self.y + height > self.geometry.content.y + self.geometry.content.height + 0.001 {
            self.page()?;
        }
        Ok(())
    }
    fn paint_line(
        &mut self,
        line: &Line,
        x: f32,
        top: f32,
        height: f32,
        align_width: f32,
        align: ColumnAlignment,
    ) -> Result<(), PrintError> {
        let extent = line.right - line.left;
        let offset = match align {
            ColumnAlignment::Right => (align_width - extent).max(0.0),
            ColumnAlignment::Center => ((align_width - extent) / 2.0).max(0.0),
            _ => 0.0,
        };
        let mut cursor = x + offset - line.left;
        for f in &line.fragments {
            self.check()?;
            let baseline = Point {
                x: cursor,
                y: top + line.ascent,
            };
            let index = self.push(Drawable::Text {
                text: f.text.clone(),
                shape: f.shape.clone(),
                baseline,
                style: f.style,
            })?;
            if let Some(link) = &f.link {
                self.links.push((self.pages.len() - 1, index, link.clone()));
            }
            if f.style.strike {
                self.push(Drawable::Line {
                    from: Point {
                        x: cursor,
                        y: top + height * 0.5,
                    },
                    to: Point {
                        x: cursor + f.shape.width,
                        y: top + height * 0.5,
                    },
                    width: 0.6,
                })?;
            }
            cursor += f.shape.width;
        }
        Ok(())
    }
    fn wrap(&mut self, spans: &[Span], width: f32, size: f32) -> Result<Vec<Line>, PrintError> {
        if width <= 0.0 {
            return Err(PrintError::Unplaceable("No horizontal space for text."));
        }
        let mut lines = vec![Line::default()];
        for span in spans {
            let initial_lines = lines.len();
            let expanded = span.text.replace('\t', "    ");
            for piece in expanded.split_word_bounds() {
                self.check()?;
                if piece.contains('\n') || piece.contains('\r') {
                    for (i, part) in piece.split('\n').enumerate() {
                        if i > 0 {
                            lines.push(Line::default());
                        }
                        let part = part.trim_end_matches('\r');
                        if !part.is_empty() {
                            self.wrap_piece(&mut lines, part, span, width, size)?;
                        }
                    }
                } else {
                    self.wrap_piece(&mut lines, piece, span, width, size)?;
                }
            }
            if lines.len() > initial_lines
                && span
                    .text
                    .chars()
                    .any(|c| matches!(c as u32, 0x202a..=0x202e | 0x2066..=0x2069))
            {
                self.warn(
                    "Bidirectional controls across wrapped lines may not retain visual order.",
                );
            }
        }
        // Shape each contiguous style/link run once per completed line, so bidi
        // controls and word shaping survive across token boundaries.
        for line in &mut lines {
            let old = std::mem::take(&mut line.fragments);
            let mut merged: Vec<(String, TextStyle, Option<Arc<str>>)> = Vec::new();
            for fragment in old {
                if let Some((text, style, link)) = merged.last_mut()
                    && *style == fragment.style
                    && *link == fragment.link
                {
                    text.push_str(&fragment.text);
                } else {
                    merged.push((fragment.text.to_string(), fragment.style, fragment.link));
                }
            }
            *line = Line::default();
            let mixed = merged.len() > 1;
            for (text, style, link) in merged {
                let shape = self.shape(
                    &text,
                    style,
                    if style.code {
                        self.settings.code_size
                    } else {
                        size
                    },
                )?;
                if mixed
                    && text.chars().any(
                        |c| matches!(c as u32, 0x0590..=0x08ff | 0x202a..=0x202e | 0x2066..=0x2069),
                    )
                {
                    self.warn(
                        "Bidirectional text across mixed styles may not retain visual order.",
                    );
                }
                line_add(line, &shape);
                line.fragments.push(Fragment {
                    text: Arc::from(text),
                    style,
                    link,
                    shape,
                });
            }
            if line.right - line.left > width + 0.001 {
                return Err(PrintError::Unplaceable(
                    "A shaped text line exceeds its available width.",
                ));
            }
            if line.ascent + line.descent == 0.0 {
                line.ascent = line_height(TextRole::Body, size).map_err(PrintError::Unplaceable)?;
            }
        }
        Ok(lines)
    }
    fn wrap_piece(
        &mut self,
        lines: &mut Vec<Line>,
        piece: &str,
        span: &Span,
        width: f32,
        size: f32,
    ) -> Result<(), PrintError> {
        let size = if span.style.code {
            self.settings.code_size
        } else {
            size
        };
        if piece.len() > 4096 {
            for g in piece.graphemes(true) {
                let shape = self.shape(g, span.style, size)?;
                self.add_piece(lines, g, span, shape, width)?;
            }
            return Ok(());
        }
        let shape = self.shape(piece, span.style, size)?;
        if shape_extent(&shape) > width {
            for g in piece.graphemes(true) {
                let shape = self.shape(g, span.style, size)?;
                self.add_piece(lines, g, span, shape, width)?;
            }
        } else {
            self.add_piece(lines, piece, span, shape, width)?;
        }
        Ok(())
    }
    fn add_piece(
        &mut self,
        lines: &mut Vec<Line>,
        piece: &str,
        span: &Span,
        shape: Arc<ShapedText>,
        width: f32,
    ) -> Result<(), PrintError> {
        if shape_extent(&shape) > width {
            return Err(PrintError::Unplaceable(
                "A grapheme is wider than the printable area.",
            ));
        }
        let previous = lines.last().unwrap();
        let mut candidate = Line {
            width: previous.width,
            left: previous.left,
            right: previous.right,
            ascent: previous.ascent,
            descent: previous.descent,
            fragments: Vec::new(),
        };
        line_add(&mut candidate, &shape);
        if candidate.right - candidate.left > width && !previous.fragments.is_empty() {
            lines.push(Line::default());
        }
        let line = lines.last_mut().unwrap();
        line_add(line, &shape);
        line.fragments.push(Fragment {
            text: Arc::from(piece),
            style: span.style,
            link: span.link.clone(),
            shape,
        });
        Ok(())
    }
}

fn shape_extent(shape: &ShapedText) -> f32 {
    let (left, right) = shape.ink.as_ref().map_or((0.0, shape.width), |r| {
        (r.x.min(0.0), (r.x + r.width).max(shape.width))
    });
    right - left
}
fn line_add(line: &mut Line, shape: &ShapedText) {
    if let Some(ink) = &shape.ink {
        line.left = line.left.min(line.width + ink.x);
        line.right = line.right.max(line.width + ink.x + ink.width);
        line.ascent = line.ascent.max(ink.y + ink.height);
        line.descent = line.descent.max(-ink.y);
    }
    for run in &shape.runs {
        line.ascent = line.ascent.max(run.ascent);
        line.descent = line.descent.max(-run.descent);
    }
    line.width += shape.width;
    line.right = line.right.max(line.width);
}

fn inline(
    parsed: &ParsedMarkdown,
    index: usize,
    style: TextStyle,
    link: Option<Arc<str>>,
    skip: Option<&std::ops::Range<usize>>,
    out: &mut Vec<Span>,
    work: &mut usize,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), PrintError> {
    if cancelled() {
        return Err(PrintError::Cancelled);
    }
    *work += 1;
    if *work > MAX_SHAPES {
        return Err(PrintError::Limit("print content limit exceeded"));
    }
    let node = &parsed.nodes[index];
    if skip.is_some_and(|s| node.source.start >= s.start && node.source.end <= s.end) {
        return Ok(());
    }
    let mut next = style;
    let mut target = link;
    match &node.kind {
        NodeKind::Text(text) | NodeKind::Unsupported(text) => out.push(Span {
            text: text.clone(),
            style,
            link: target.clone(),
        }),
        NodeKind::Code(text) => {
            next.code = true;
            out.push(Span {
                text: text.clone(),
                style: next,
                link: target.clone(),
            });
        }
        NodeKind::SoftBreak => out.push(Span {
            text: " ".into(),
            style,
            link: target.clone(),
        }),
        NodeKind::HardBreak => out.push(Span {
            text: "\n".into(),
            style,
            link: target.clone(),
        }),
        NodeKind::Task(done) => out.push(Span {
            text: if *done { "[x] " } else { "[ ] " }.into(),
            style,
            link: target.clone(),
        }),
        NodeKind::FootnoteReference(label) => out.push(Span {
            text: format!("[{label}]"),
            style,
            link: target.clone(),
        }),
        NodeKind::Emphasis => next.italic = true,
        NodeKind::Strong => next.bold = true,
        NodeKind::Strike => next.strike = true,
        NodeKind::Link { destination, .. } => {
            target = (destination.len() <= 8192).then(|| Arc::from(destination.as_str()))
        }
        NodeKind::Image { .. } => {
            out.push(Span {
                text: "[Image: ".into(),
                style,
                link: None,
            });
            for child in &node.children {
                inline(parsed, *child, style, None, skip, out, work, cancelled)?;
            }
            out.push(Span {
                text: "]".into(),
                style,
                link: None,
            });
            return Ok(());
        }
        NodeKind::Ignored => return Ok(()),
        _ => {}
    }
    for child in &node.children {
        inline(
            parsed,
            *child,
            next,
            target.clone(),
            skip,
            out,
            work,
            cancelled,
        )?;
    }
    Ok(())
}
fn collect(
    parsed: &ParsedMarkdown,
    index: usize,
    indent: f32,
    skip: Option<&std::ops::Range<usize>>,
    blocks: &mut Vec<Block>,
    work: &mut usize,
    body: f32,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), PrintError> {
    if cancelled() {
        return Err(PrintError::Cancelled);
    }
    *work += 1;
    if *work > MAX_SHAPES {
        return Err(PrintError::Limit("print content limit exceeded"));
    }
    let node = &parsed.nodes[index];
    match &node.kind {
        NodeKind::Paragraph
        | NodeKind::Heading { .. }
        | NodeKind::CodeBlock { .. }
        | NodeKind::Unsupported(_) => {
            let mut spans = Vec::new();
            let mut style = TextStyle::default();
            let (heading, size, keep_next) = match &node.kind {
                NodeKind::Heading { level, anchor } => {
                    style.bold = true;
                    (
                        anchor.clone(),
                        (body * (1.6 - (*level as f32 - 1.0) * 0.1)).clamp(6.0, 48.0),
                        true,
                    )
                }
                NodeKind::CodeBlock { .. } => {
                    style.code = true;
                    (None, body, false)
                }
                _ => (None, body, false),
            };
            inline(
                parsed, index, style, None, skip, &mut spans, work, cancelled,
            )?;
            if !spans.is_empty() {
                blocks.push(Block::Text {
                    spans,
                    indent,
                    heading,
                    size,
                    keep_next,
                });
            }
        }
        NodeKind::Rule => blocks.push(Block::Rule { indent }),
        NodeKind::PageBreak => blocks.push(Block::Break),
        NodeKind::Quote { callout } => {
            let skip = if let Some(header) = callout {
                let title = if header.title.is_empty() {
                    header.kind.to_uppercase()
                } else {
                    format!("{}: {}", header.kind.to_uppercase(), header.title)
                };
                blocks.push(Block::Text {
                    spans: vec![Span {
                        text: title,
                        style: TextStyle {
                            bold: true,
                            ..Default::default()
                        },
                        link: None,
                    }],
                    indent: indent + 12.0,
                    heading: None,
                    size: body,
                    keep_next: true,
                });
                Some(&header.source)
            } else {
                skip
            };
            for child in &node.children {
                collect(
                    parsed,
                    *child,
                    indent + 12.0,
                    skip,
                    blocks,
                    work,
                    body,
                    cancelled,
                )?;
            }
        }
        NodeKind::List { first } => {
            for (i, child) in node.children.iter().enumerate() {
                let marker = first.map_or_else(
                    || "• ".to_owned(),
                    |n| format!("{}. ", n.saturating_add(i as u64)),
                );
                let item = &parsed.nodes[*child];
                let mut pending = Some(marker);
                let mut spans = Vec::new();
                for &part in &item.children {
                    match parsed.nodes[part].kind {
                        NodeKind::Paragraph => {
                            if !spans.is_empty() {
                                blocks.push(Block::Text {
                                    spans: std::mem::take(&mut spans),
                                    indent: indent + 12.0,
                                    heading: None,
                                    size: body,
                                    keep_next: false,
                                });
                            }
                            if let Some(marker) = pending.take() {
                                spans.push(Span {
                                    text: marker,
                                    style: TextStyle::default(),
                                    link: None,
                                });
                            }
                            inline(
                                parsed,
                                part,
                                TextStyle::default(),
                                None,
                                skip,
                                &mut spans,
                                work,
                                cancelled,
                            )?;
                            blocks.push(Block::Text {
                                spans: std::mem::take(&mut spans),
                                indent: indent + 12.0,
                                heading: None,
                                size: body,
                                keep_next: false,
                            });
                        }
                        NodeKind::List { .. }
                        | NodeKind::Quote { .. }
                        | NodeKind::Table { .. }
                        | NodeKind::CodeBlock { .. }
                        | NodeKind::Heading { .. }
                        | NodeKind::Rule => {
                            if let Some(marker) = pending.take() {
                                spans.push(Span {
                                    text: marker,
                                    style: TextStyle::default(),
                                    link: None,
                                });
                            }
                            if !spans.is_empty() {
                                blocks.push(Block::Text {
                                    spans: std::mem::take(&mut spans),
                                    indent: indent + 12.0,
                                    heading: None,
                                    size: body,
                                    keep_next: false,
                                });
                            }
                            collect(
                                parsed,
                                part,
                                indent + 12.0,
                                skip,
                                blocks,
                                work,
                                body,
                                cancelled,
                            )?;
                        }
                        _ => {
                            if let Some(marker) = pending.take() {
                                spans.push(Span {
                                    text: marker,
                                    style: TextStyle::default(),
                                    link: None,
                                });
                            }
                            inline(
                                parsed,
                                part,
                                TextStyle::default(),
                                None,
                                skip,
                                &mut spans,
                                work,
                                cancelled,
                            )?;
                        }
                    }
                }
                if let Some(marker) = pending {
                    spans.push(Span {
                        text: marker,
                        style: TextStyle::default(),
                        link: None,
                    });
                }
                if !spans.is_empty() {
                    blocks.push(Block::Text {
                        spans,
                        indent: indent + 12.0,
                        heading: None,
                        size: body,
                        keep_next: false,
                    });
                }
            }
        }
        NodeKind::Table { alignments } => {
            if alignments.len() > MAX_COLUMNS {
                return Err(PrintError::Limit("table column limit exceeded"));
            }
            let mut rows = Vec::new();
            for &row in &node.children {
                let mut cells = Vec::new();
                for &cell in &parsed.nodes[row].children {
                    let mut spans = Vec::new();
                    inline(
                        parsed,
                        cell,
                        TextStyle::default(),
                        None,
                        skip,
                        &mut spans,
                        work,
                        cancelled,
                    )?;
                    cells.push(spans);
                }
                rows.push(cells);
            }
            blocks.push(Block::Table {
                table: Table {
                    alignment: alignments.clone(),
                    rows,
                },
                indent,
            });
        }
        NodeKind::FootnoteDefinition(label) => {
            blocks.push(Block::Text {
                spans: vec![Span {
                    text: format!("[{label}]"),
                    style: TextStyle::default(),
                    link: None,
                }],
                indent,
                heading: None,
                size: body,
                keep_next: false,
            });
            for child in &node.children {
                collect(
                    parsed,
                    *child,
                    indent + 8.0,
                    skip,
                    blocks,
                    work,
                    body,
                    cancelled,
                )?;
            }
        }
        NodeKind::Ignored => {}
        _ => {
            for child in &node.children {
                collect(parsed, *child, indent, skip, blocks, work, body, cancelled)?;
            }
        }
    }
    Ok(())
}

impl<F: FnMut() -> bool> Builder<'_, F> {
    fn text_block(
        &mut self,
        spans: &[Span],
        indent: f32,
        heading: Option<&str>,
        size: f32,
        next_height: Option<f32>,
    ) -> Result<(), PrintError> {
        let indent = indent.min((self.geometry.content.width - size * 4.0).max(0.0));
        let x = self.geometry.content.x + indent;
        let width = self.geometry.content.width - indent;
        let lines = self.wrap(spans, width, size)?;
        let minimum = line_height(TextRole::Body, size).map_err(PrintError::Unplaceable)?;
        let total: f32 = lines
            .iter()
            .map(|line| (line.ascent + line.descent).max(minimum))
            .sum();
        if let Some(next) = next_height
            && total + GAP + next <= self.geometry.content.height
        {
            self.fit(total + GAP + next)?;
        }
        for (index, line) in lines.iter().enumerate() {
            let height = (line.ascent + line.descent).max(minimum);
            self.fit(height)?;
            if index == 0
                && let Some(anchor) = heading
            {
                self.anchors
                    .entry(anchor.to_owned())
                    .or_insert((self.pages.len() - 1, Point { x, y: self.y }));
            }
            self.paint_line(line, x, self.y, height, width, ColumnAlignment::Left)?;
            self.y += height;
        }
        self.y += GAP;
        Ok(())
    }
    fn table(&mut self, table: &Table, indent: f32) -> Result<(), PrintError> {
        if table.rows.is_empty() || table.alignment.is_empty() {
            return Ok(());
        }
        let indent =
            indent.min((self.geometry.content.width - self.settings.body_size * 4.0).max(0.0));
        let count = table.alignment.len();
        let width = self.geometry.content.width - indent;
        let cell_width = width / count as f32;
        let size = self.settings.body_size;
        let height = line_height(TextRole::Body, size).map_err(PrintError::Unplaceable)?;
        if cell_width < size * 3.0 || 2.0 * height > self.geometry.content.height {
            return self.table_fallback(
                table,
                indent,
                size,
                "Table columns are too narrow; displayed as labeled rows.",
            );
        }
        let mut rows = Vec::new();
        for row in &table.rows {
            self.check()?;
            let mut cells = Vec::new();
            for column in 0..count {
                let lines = match self.wrap(
                    row.get(column).map_or(&[], Vec::as_slice),
                    cell_width - 4.0,
                    size,
                ) {
                    Ok(lines) => lines,
                    Err(PrintError::Unplaceable(_)) => {
                        return self.table_fallback(
                            table,
                            indent,
                            size,
                            "Table content is too wide; displayed as labeled rows.",
                        );
                    }
                    Err(error) => return Err(error),
                };
                cells.push(Cell { lines });
            }
            rows.push(cells);
        }
        let height = rows
            .iter()
            .flat_map(|row| row.iter())
            .flat_map(|cell| cell.lines.iter())
            .map(|line| line.ascent + line.descent)
            .fold(height, f32::max);
        let header_height = rows[0]
            .iter()
            .map(|c| c.lines.len())
            .max()
            .unwrap_or(1)
            .max(1) as f32
            * height;
        if header_height + height > self.geometry.content.height {
            return self.table_fallback(
                table,
                indent,
                size,
                "Table header is too tall to repeat; displayed as labeled rows.",
            );
        }
        if rows.len() == 1 {
            self.fit(header_height)?;
            self.paint_table_slice(
                &rows[0],
                table,
                indent,
                0,
                (header_height / height).round() as usize,
                height,
                cell_width,
            )?;
            return Ok(());
        }
        let mut need_header = true;
        for row in rows.iter().skip(1) {
            let total = row.iter().map(|c| c.lines.len()).max().unwrap_or(1).max(1);
            let mut step = 0;
            while step < total {
                self.check()?;
                let just_header = need_header;
                if need_header {
                    self.fit(header_height + height)?;
                    let header_lines = (header_height / height).round() as usize;
                    self.paint_table_slice(
                        &rows[0],
                        table,
                        indent,
                        0,
                        header_lines,
                        height,
                        cell_width,
                    )?;
                    need_header = false;
                }
                let available = ((self.geometry.content.y + self.geometry.content.height - self.y
                    + 0.001)
                    / height)
                    .floor() as usize;
                if available == 0 {
                    if just_header {
                        return Err(PrintError::Unplaceable(
                            "Table header and first row cannot fit together.",
                        ));
                    }
                    self.page()?;
                    need_header = true;
                    continue;
                }
                let count = (total - step).min(available);
                self.paint_table_slice(row, table, indent, step, count, height, cell_width)?;
                step += count;
                if step < total {
                    self.page()?;
                    need_header = true;
                }
            }
            if self.y + height > self.geometry.content.y + self.geometry.content.height {
                need_header = true;
            }
        }
        self.y += GAP;
        Ok(())
    }
    fn table_fallback(
        &mut self,
        table: &Table,
        indent: f32,
        size: f32,
        message: &str,
    ) -> Result<(), PrintError> {
        self.warn(message);
        if table.rows.len() == 1 {
            for cell in &table.rows[0] {
                self.text_block(cell, indent, None, size, None)?;
            }
        }
        for row in table.rows.iter().skip(1) {
            for (column, cell) in row.iter().enumerate() {
                self.check()?;
                let mut spans = table.rows[0].get(column).cloned().unwrap_or_default();
                spans.push(Span {
                    text: ": ".into(),
                    style: TextStyle::default(),
                    link: None,
                });
                spans.extend(cell.clone());
                self.text_block(&spans, indent, None, size, None)?;
            }
        }
        Ok(())
    }
    fn paint_table_slice(
        &mut self,
        cells: &[Cell],
        table: &Table,
        indent: f32,
        start: usize,
        count: usize,
        height: f32,
        cell_width: f32,
    ) -> Result<(), PrintError> {
        let slice_start = self.y;
        for step in start..start + count {
            let top = slice_start + (step - start) as f32 * height;
            for (column, cell) in cells.iter().enumerate() {
                if let Some(line) = cell.lines.get(step) {
                    self.paint_line(
                        line,
                        self.geometry.content.x + indent + column as f32 * cell_width + 2.0,
                        top,
                        height,
                        cell_width - 4.0,
                        table.alignment[column],
                    )?;
                }
            }
        }
        self.y = slice_start + count as f32 * height;
        Ok(())
    }
}

fn decode_fragment(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}
fn bounds_union(a: Option<Rect>, b: Rect) -> Option<Rect> {
    Some(match a {
        None => b,
        Some(a) => {
            let x = a.x.min(b.x);
            let y = a.y.min(b.y);
            Rect {
                x,
                y,
                width: (a.x + a.width).max(b.x + b.width) - x,
                height: (a.y + a.height).max(b.y + b.height) - y,
            }
        }
    })
}

/// Build an immutable page model; no printer, image decoding, or PDF work occurs here.
pub fn paginate(
    source: &str,
    settings: &PrintSettings,
    date: &str,
    mut cancelled: impl FnMut() -> bool,
) -> Result<PaginatedDocument, PrintError> {
    let geometry = settings
        .geometry()
        .map_err(|e| PrintError::InvalidSettings(e.into()))?;
    let parsed = markdown::parse(source, &mut cancelled).map_err(|e| match e {
        markdown::ParseError::Cancelled => PrintError::Cancelled,
        markdown::ParseError::SourceTooLarge
        | markdown::ParseError::EventLimit
        | markdown::ParseError::DepthLimit
        | markdown::ParseError::AllocationLimit => {
            PrintError::Limit("Markdown exceeds print limits")
        }
        _ => PrintError::InvalidMarkdown,
    })?;
    let mut blocks = Vec::new();
    let mut collect_work = 0;
    if !settings.title.is_empty() {
        blocks.push(Block::Text {
            spans: vec![Span {
                text: settings.title.clone(),
                style: TextStyle {
                    bold: true,
                    ..Default::default()
                },
                link: None,
            }],
            indent: 0.0,
            heading: None,
            size: (settings.body_size * 1.6).min(48.0),
            keep_next: true,
        });
    }
    for &root in &parsed.roots {
        if cancelled() {
            return Err(PrintError::Cancelled);
        }
        collect(
            &parsed,
            root,
            0.0,
            None,
            &mut blocks,
            &mut collect_work,
            settings.body_size,
            &mut cancelled,
        )?;
    }
    let mut b = Builder {
        settings,
        geometry,
        cancelled,
        pages: Vec::new(),
        y: 0.0,
        work: 0,
        shapes: 0,
        glyph_work: 0,
        warnings: Vec::new(),
        anchors: HashMap::new(),
        links: Vec::new(),
    };
    for warning in parsed.warnings {
        b.warn(warning.message);
    }
    if parsed.nodes.iter().any(
        |node| matches!(&node.kind, NodeKind::Link { destination, .. } if destination.len() > 8192),
    ) {
        b.warn("A link destination exceeds the supported length.");
    }
    if parsed.nodes.iter().any(|node| matches!(&node.kind, NodeKind::CodeBlock { language: Some(language) } if language.split_whitespace().next().is_some_and(|part| part.eq_ignore_ascii_case("mermaid")))) { b.warn("Mermaid diagrams are printed as source; diagram rendering is not yet available."); }
    if parsed
        .nodes
        .iter()
        .any(|node| matches!(node.kind, NodeKind::Image { .. }))
    {
        b.warn("Images are represented by their alt text; image rendering is not yet available.");
    }
    b.page()?;
    let mut blocks = blocks.into_iter().peekable();
    while let Some(block) = blocks.next() {
        b.check()?;
        match block {
            Block::Text {
                spans,
                indent,
                heading,
                size,
                keep_next,
            } => {
                let next_height = if keep_next {
                    if let Some(Block::Text {
                        spans,
                        indent,
                        size,
                        ..
                    }) = blocks.peek()
                    {
                        let indent = indent.min((geometry.content.width - size * 4.0).max(0.0));
                        let lines = b.wrap(spans, geometry.content.width - indent, *size)?;
                        lines.first().map(|line| {
                            (line.ascent + line.descent)
                                .max(line_height(TextRole::Body, *size).unwrap_or(0.0))
                        })
                    } else {
                        None
                    }
                } else {
                    None
                };
                b.text_block(&spans, indent, heading.as_deref(), size, next_height)?;
            }
            Block::Table { table, indent } => b.table(&table, indent)?,
            Block::Rule { indent } => {
                b.fit(8.0)?;
                b.push(Drawable::Line {
                    from: Point {
                        x: geometry.content.x
                            + indent
                                .min((geometry.content.width - settings.body_size * 4.0).max(0.0)),
                        y: b.y + 4.0,
                    },
                    to: Point {
                        x: geometry.content.x + geometry.content.width,
                        y: b.y + 4.0,
                    },
                    width: 0.5,
                })?;
                b.y += 8.0;
            }
            Block::Break => b.page()?,
        }
    }
    let page_count = b.pages.len();
    for page_index in 0..page_count {
        for (template, y) in [
            (&settings.header, geometry.header_y),
            (&settings.footer, geometry.footer_y),
        ] {
            if let Some(y) = y {
                let text =
                    render_template(template, &settings.title, date, page_index + 1, page_count)
                        .map_err(|e| PrintError::InvalidSettings(e.into()))?;
                let shape = b.shape(&text, TextStyle::default(), settings.body_size)?;
                let ink = shape.ink.as_ref();
                let ascent = shape
                    .runs
                    .iter()
                    .map(|r| r.ascent)
                    .fold(0.0_f32, f32::max)
                    .max(ink.map_or(0.0, |r| r.y + r.height));
                let baseline = Point {
                    x: geometry.content.x - ink.map_or(0.0, |r| r.x.min(0.0)),
                    y: y + ascent,
                };
                let band_height = settings.body_size * 1.5;
                if shape_extent(&shape) > geometry.content.width
                    || ink.is_some_and(|r| {
                        baseline.y - r.y - r.height < y - 0.001
                            || baseline.y - r.y > y + band_height + 0.001
                    })
                {
                    return Err(PrintError::Unplaceable(
                        "Header or footer does not fit its reserved line.",
                    ));
                }
                b.shapes += 1;
                if b.shapes > MAX_DRAWABLES {
                    return Err(PrintError::Limit("print drawable limit exceeded"));
                }
                b.pages[page_index].drawables.push(Drawable::Text {
                    text: Arc::from(text),
                    shape,
                    baseline,
                    style: TextStyle::default(),
                });
            }
        }
    }
    let mut resolved: HashMap<Arc<str>, Option<LinkDestination>> = HashMap::new();
    for (page, index, target) in std::mem::take(&mut b.links) {
        b.check()?;
        let destination = if let Some(cached) = resolved.get(&target) {
            cached.clone()
        } else {
            let result = if let Some(fragment) = target.strip_prefix('#') {
                decode_fragment(fragment).and_then(|fragment| {
                    let normalized = super::super::headings::normalize_anchor(&fragment);
                    b.anchors
                        .get(&normalized)
                        .map(|(page, point)| LinkDestination::Internal {
                            page: *page,
                            point: *point,
                        })
                })
            } else {
                super::super::html::safe_link_destination(&target)
                    .filter(|uri| {
                        uri.starts_with("http://")
                            || uri.starts_with("https://")
                            || uri.starts_with("mailto:")
                    })
                    .map(|uri| LinkDestination::External(Arc::from(uri)))
            };
            resolved.insert(target.clone(), result.clone());
            result
        };
        if let Some(destination) = destination {
            if let Drawable::Text {
                shape, baseline, ..
            } = &b.pages[page].drawables[index]
            {
                let ascent = shape.runs.iter().map(|r| r.ascent).fold(0.0_f32, f32::max);
                let descent = shape
                    .runs
                    .iter()
                    .map(|r| -r.descent)
                    .fold(0.0_f32, f32::max);
                let bounds = Rect {
                    x: baseline.x,
                    y: baseline.y - ascent,
                    width: shape.width,
                    height: ascent + descent,
                };
                b.shapes += 1;
                if b.shapes > MAX_DRAWABLES {
                    return Err(PrintError::Limit("print drawable limit exceeded"));
                }
                b.pages[page].drawables.push(Drawable::Link {
                    bounds,
                    destination,
                });
            }
        } else {
            b.warn("A link is unsafe or its target is unavailable.");
        }
    }
    for page in &mut b.pages {
        for drawable in &page.drawables {
            let rect = match drawable {
                Drawable::Text {
                    shape, baseline, ..
                } => shape.ink.as_ref().map(|r| Rect {
                    x: baseline.x + r.x,
                    y: baseline.y - r.y - r.height,
                    width: r.width,
                    height: r.height,
                }),
                Drawable::Line { from, to, width } => Some(Rect {
                    x: from.x.min(to.x),
                    y: from.y.min(to.y) - width / 2.0,
                    width: (to.x - from.x).abs(),
                    height: (to.y - from.y).abs() + width,
                }),
                Drawable::Link { .. } => None,
            };
            if let Some(rect) = rect {
                page.painted_bounds = bounds_union(page.painted_bounds, rect);
            }
        }
    }
    Ok(PaginatedDocument {
        pages: b.pages,
        warnings: b.warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn strings(page: &Page) -> String {
        page.drawables
            .iter()
            .filter_map(|item| match item {
                Drawable::Text { text, .. } => Some(text.as_ref()),
                _ => None,
            })
            .collect()
    }
    #[test]
    fn paginate_text_links_breaks_and_headers() {
        let mut settings = PrintSettings::default();
        settings.header = "{page}/{pages}".into();
        let doc = paginate(
            "# Heading\n\n[go](#heading) and **bold**.\n\n<!-- pagebreak -->\n\nend",
            &settings,
            "2026-01-01",
            || false,
        )
        .unwrap();
        assert_eq!(doc.pages.len(), 2);
        assert!(strings(&doc.pages[0]).contains("1/2"));
        assert!(strings(&doc.pages[1]).contains("2/2"));
        assert!(doc.pages[0].drawables.iter().any(|item| matches!(
            item,
            Drawable::Link {
                destination: LinkDestination::Internal { page: 0, .. },
                ..
            }
        )));
        assert!(
            doc.pages[0]
                .drawables
                .iter()
                .any(|item| matches!(item, Drawable::Text { style, .. } if style.bold))
        );
    }
    #[test]
    fn paginate_repeats_table_header_and_splits_long_rows() {
        let mut settings = PrintSettings::default();
        settings.paper = super::super::Paper::Custom {
            width: 240.0,
            height: 180.0,
        };
        settings.margins = super::super::Margins {
            top: 10.0,
            bottom: 10.0,
            left: 10.0,
            right: 10.0,
        };
        let source = format!(
            "| HEADER | VALUE |\n| --- | --- |\n| {} | data |\n",
            "long text ".repeat(300)
        );
        let doc = paginate(&source, &settings, "", || false).unwrap();
        assert!(doc.pages.len() > 1);
        for page in &doc.pages {
            assert!(strings(page).contains("HEADER"));
        }
    }
    #[test]
    fn paginate_ink_code_size_and_list_source_order() {
        let mut settings = PrintSettings::default();
        settings.code_size = 24.0;
        let doc = paginate(
            "j `j`\n\n- first\n\n  > quoted\n\n  later\n",
            &settings,
            "",
            || false,
        )
        .unwrap();
        let page = &doc.pages[0];
        for drawable in &page.drawables {
            if let Drawable::Text {
                shape,
                baseline,
                style,
                ..
            } = drawable
            {
                if let Some(ink) = &shape.ink {
                    assert!(baseline.x + ink.x >= page.geometry.content.x - 0.001);
                }
                if style.code {
                    assert!(shape.runs.iter().all(|run| (run.size - 24.0).abs() < 0.001));
                }
            }
        }
        let text = strings(page);
        assert!(text.find("first").unwrap() < text.find("quoted").unwrap());
        assert!(text.find("quoted").unwrap() < text.find("later").unwrap());
    }
    #[test]
    fn paginate_header_only_fallback_preserves_content_and_anchors_follow_actual_page() {
        let mut settings = PrintSettings::default();
        settings.paper = super::super::Paper::Custom {
            width: 72.0,
            height: 90.0,
        };
        settings.margins = super::super::Margins {
            top: 10.0,
            bottom: 10.0,
            left: 10.0,
            right: 10.0,
        };
        let doc = paginate("| Alpha | Beta | Gamma |\n| --- | --- | --- |\n\n<!-- pagebreak -->\n\n# Target\n\n[go](#target)", &settings, "", || false).unwrap();
        let all = doc.pages.iter().map(strings).collect::<String>();
        assert!(all.contains("Alpha") && all.contains("Beta") && all.contains("Gamma"));
        let (page, _) = doc
            .pages
            .iter()
            .enumerate()
            .find(|(_, page)| strings(page).contains("Target"))
            .unwrap();
        assert!(doc.pages.iter().flat_map(|page| &page.drawables).any(|item| matches!(item, Drawable::Link { destination: LinkDestination::Internal { page: target, .. }, .. } if *target == page)));
    }
    #[test]
    fn paginate_cancel_during_collection_and_large_link_is_bounded() {
        let source = format!(
            "[{}](https://example.test/{})",
            "word ".repeat(200),
            "x".repeat(9000)
        );
        let doc = paginate(&source, &PrintSettings::default(), "", || false).unwrap();
        assert!(
            doc.warnings
                .iter()
                .any(|warning| warning.message.contains("length"))
        );
        let mut calls = 0;
        let result = paginate(&source, &PrintSettings::default(), "", || {
            calls += 1;
            calls > 12
        });
        assert!(matches!(result, Err(PrintError::Cancelled)));
    }

    #[test]
    fn paginate_loose_item_rule_and_table_wide_glyph_fallback() {
        let rule = paginate(
            "- first\n\n  ---\n\n  last\n",
            &PrintSettings::default(),
            "",
            || false,
        )
        .unwrap();
        assert!(
            rule.pages
                .iter()
                .flat_map(|p| &p.drawables)
                .any(|d| matches!(d, Drawable::Line { .. }))
        );
        let mut settings = PrintSettings::default();
        settings.paper = super::super::Paper::Custom {
            width: 72.0,
            height: 180.0,
        };
        settings.margins = super::super::Margins {
            top: 10.0,
            bottom: 10.0,
            left: 10.0,
            right: 10.0,
        };
        settings.body_size = 6.0;
        settings.code_size = 48.0;
        let table = paginate(
            "| A | B | C | D |\n| --- | --- | --- | --- |\n| `W` | x | y | z |",
            &settings,
            "",
            || false,
        )
        .unwrap();
        assert!(
            table
                .warnings
                .iter()
                .any(|w| w.message.contains("labeled rows"))
        );
        assert!(
            table
                .pages
                .iter()
                .map(strings)
                .collect::<String>()
                .contains("W")
        );
    }

    #[test]
    fn paginate_cancel_and_page_budget() {
        assert!(matches!(
            paginate("test", &PrintSettings::default(), "", || true),
            Err(PrintError::Cancelled)
        ));
        let source = "<!-- pagebreak -->\n\n".repeat(MAX_PAGES);
        assert!(matches!(
            paginate(&source, &PrintSettings::default(), "", || false),
            Err(PrintError::Limit(_))
        ));
    }
}
