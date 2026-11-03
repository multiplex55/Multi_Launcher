//! Bounded CommonMark and exact-range Mermaid preview descriptors.

use std::{fmt, ops::Range, sync::Arc};

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

use crate::diagram::{
    DiagramDocumentError, DiagramExportError, DiagramImageSnapshot, DiagramSession,
    DiagramSourceCopy, MermaidDiagnostic, MermaidDiagnosticKind, PreviewState, SceneMetrics,
};

use super::authoring::{
    MAX_MARKDOWN_SOURCE_BYTES, MarkdownEditError, PreparedMarkdownEdit, dominant_newline,
    validate_source,
};
use super::{MarkdownAnalysis, analyze_markdown};

pub const MAX_MERMAID_PREVIEW_FENCES: usize = 32;
pub const MAX_MERMAID_FENCE_BYTES: usize = 64 * 1024;
pub const MAX_MERMAID_PREVIEW_AGGREGATE_BYTES: usize = 512 * 1024;
pub const MAX_MERMAID_PREVIEW_SVG_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_MARKDOWN_PREVIEW_LINES: usize = 65_536;
const MAX_MARKDOWN_EVENTS: usize = 1_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkdownPreviewLimit {
    SourceBytes,
    SourceLines,
    ParserEvents,
    FenceCount,
    AggregateDiagramBytes,
    MermaidSvgBytes,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MarkdownPreviewStatus {
    Complete,
    LimitExceeded(MarkdownPreviewLimit),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MermaidFenceStatus {
    Current,
    Invalid(MermaidDiagnostic),
    Unsupported(MermaidDiagnostic),
    Unclosed,
    TooLarge { max_bytes: usize },
    AggregateLimitExceeded { max_bytes: usize },
    SvgAggregateLimitExceeded { max_bytes: usize },
    PreviewEncodingFailed(String),
}

#[derive(Clone, Debug)]
pub struct MermaidFencePreview {
    source: Arc<str>,
    full_range: Range<usize>,
    body_range: Range<usize>,
    info: String,
    indent: usize,
    marker: char,
    marker_len: usize,
    closed: bool,
    status: MermaidFenceStatus,
    svg: Option<String>,
}

impl MermaidFencePreview {
    pub fn full_range(&self) -> Range<usize> {
        self.full_range.clone()
    }

    pub fn body_range(&self) -> Range<usize> {
        self.body_range.clone()
    }

    pub fn raw_fence(&self) -> &str {
        &self.source[self.full_range.clone()]
    }

    pub fn raw_source(&self) -> &str {
        &self.source[self.body_range.clone()]
    }

    pub fn info(&self) -> &str {
        &self.info
    }

    pub fn status(&self) -> &MermaidFenceStatus {
        &self.status
    }

    pub fn is_closed(&self) -> bool {
        self.closed
    }

    pub fn source_snapshot(&self) -> &str {
        &self.source
    }

    pub fn to_diagram_source_copy(&self) -> Result<DiagramSourceCopy, DiagramDocumentError> {
        DiagramSourceCopy::new(self.raw_source())
    }

    pub(crate) fn trusted_svg(&self) -> Option<&str> {
        self.svg.as_deref()
    }

    pub(crate) fn delimiter(&self) -> (usize, char, usize) {
        (self.indent, self.marker, self.marker_len)
    }
}

#[derive(Clone, Debug)]
pub struct MarkdownPreview {
    source: Arc<str>,
    analysis: Option<MarkdownAnalysis>,
    status: MarkdownPreviewStatus,
    mermaid_fences: Vec<MermaidFencePreview>,
}

impl MarkdownPreview {
    pub fn analyze(source: &str) -> Result<Self, MarkdownPreviewError> {
        validate_source(source).map_err(|_| MarkdownPreviewError::SourceTooLarge {
            max_bytes: MAX_MARKDOWN_SOURCE_BYTES,
        })?;
        let newline_count = source.bytes().filter(|byte| *byte == b'\n').count();
        let physical_line_count =
            newline_count + usize::from(!source.is_empty() && !source.ends_with('\n'));
        if physical_line_count > MAX_MARKDOWN_PREVIEW_LINES {
            return Ok(Self {
                source: Arc::from(source),
                analysis: None,
                status: MarkdownPreviewStatus::LimitExceeded(MarkdownPreviewLimit::SourceLines),
                mermaid_fences: Vec::new(),
            });
        }
        let analysis = Some(analyze_markdown(source));
        let options = markdown_options();
        let mut codeblocks = Vec::new();
        let mut pending: Option<(String, Range<usize>)> = None;
        let mut events = 0usize;
        for (event, range) in Parser::new_ext(source, options).into_offset_iter() {
            events = events.saturating_add(1);
            if events > MAX_MARKDOWN_EVENTS {
                return Ok(Self {
                    source: Arc::from(source),
                    analysis,
                    status: MarkdownPreviewStatus::LimitExceeded(
                        MarkdownPreviewLimit::ParserEvents,
                    ),
                    mermaid_fences: Vec::new(),
                });
            }
            match event {
                Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) => {
                    pending = Some((info.to_string(), range));
                }
                Event::End(TagEnd::CodeBlock) => {
                    if let Some((info, start_range)) = pending.take() {
                        let full_range =
                            start_range.start.min(range.start)..start_range.end.max(range.end);
                        if is_mermaid_info(&info) {
                            if codeblocks.len() == MAX_MERMAID_PREVIEW_FENCES {
                                return Ok(Self {
                                    source: Arc::from(source),
                                    analysis,
                                    status: MarkdownPreviewStatus::LimitExceeded(
                                        MarkdownPreviewLimit::FenceCount,
                                    ),
                                    mermaid_fences: Vec::new(),
                                });
                            }
                            if let Some(details) = inspect_fence(source, full_range.clone()) {
                                codeblocks.push((info, full_range, details));
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        let source: Arc<str> = Arc::from(source);
        let mut fences = Vec::with_capacity(codeblocks.len());
        let mut aggregate_bytes = 0usize;
        let mut aggregate_svg_bytes = 0usize;
        let mut status = MarkdownPreviewStatus::Complete;
        for (info, full_range, details) in codeblocks {
            let body_len = details
                .body_range
                .end
                .saturating_sub(details.body_range.start);
            let mut fence = MermaidFencePreview {
                source: Arc::clone(&source),
                full_range,
                body_range: details.body_range,
                info,
                indent: details.indent,
                marker: details.marker,
                marker_len: details.marker_len,
                closed: details.closed,
                status: MermaidFenceStatus::Current,
                svg: None,
            };
            if !fence.closed {
                fence.status = MermaidFenceStatus::Unclosed;
            } else if body_len > MAX_MERMAID_FENCE_BYTES {
                fence.status = MermaidFenceStatus::TooLarge {
                    max_bytes: MAX_MERMAID_FENCE_BYTES,
                };
            } else if aggregate_bytes.saturating_add(body_len) > MAX_MERMAID_PREVIEW_AGGREGATE_BYTES
            {
                fence.status = MermaidFenceStatus::AggregateLimitExceeded {
                    max_bytes: MAX_MERMAID_PREVIEW_AGGREGATE_BYTES,
                };
                status = MarkdownPreviewStatus::LimitExceeded(
                    MarkdownPreviewLimit::AggregateDiagramBytes,
                );
            } else {
                aggregate_bytes += body_len;
                if !prepare_diagram(&mut fence, &mut aggregate_svg_bytes) {
                    status =
                        MarkdownPreviewStatus::LimitExceeded(MarkdownPreviewLimit::MermaidSvgBytes);
                }
            }
            fences.push(fence);
        }
        Ok(Self {
            source,
            analysis,
            status,
            mermaid_fences: fences,
        })
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn analysis(&self) -> Option<&MarkdownAnalysis> {
        self.analysis.as_ref()
    }

    pub fn status(&self) -> &MarkdownPreviewStatus {
        &self.status
    }

    pub fn mermaid_fences(&self) -> &[MermaidFencePreview] {
        &self.mermaid_fences
    }

    /// Prepare an exact-range body replacement. The returned edit is bound to
    /// this full source snapshot and fails if the Markdown changed before apply.
    /// When copied Mermaid text could close the surrounding fence, only that
    /// fence's delimiters are lengthened; all bytes outside the fence survive.
    pub fn prepare_fence_replacement(
        &self,
        fence_index: usize,
        new_mermaid_source: &str,
    ) -> Result<PreparedMarkdownEdit, MarkdownEditError> {
        if new_mermaid_source.len() > MAX_MERMAID_FENCE_BYTES {
            return Err(MarkdownEditError::SnippetTooLarge {
                max_bytes: MAX_MERMAID_FENCE_BYTES,
            });
        }
        let fence = self
            .mermaid_fences
            .get(fence_index)
            .ok_or(MarkdownEditError::InvalidRange)?;
        if !fence.closed {
            return Err(MarkdownEditError::InvalidParameter(
                "cannot replace the body of an unclosed Mermaid fence",
            ));
        }
        let newline = dominant_newline(fence.raw_fence());
        let normalized_source = normalize_line_endings(new_mermaid_source, newline);
        let (_, marker, marker_len) = fence.delimiter();
        if !contains_closing_delimiter(&normalized_source, marker, marker_len) {
            let mut replacement = normalized_source;
            if !replacement.is_empty() && !replacement.ends_with(newline) {
                replacement.push_str(newline);
            }
            return PreparedMarkdownEdit::prepare_with_replacement_limit(
                &self.source,
                fence.body_range(),
                replacement,
                MAX_MERMAID_FENCE_BYTES.saturating_mul(2).saturating_add(2),
            );
        }

        let delimiter_char = if longest_fence_run(&normalized_source, '`')
            <= longest_fence_run(&normalized_source, '~')
        {
            '`'
        } else {
            '~'
        };
        let delimiter_len = longest_fence_run(&normalized_source, delimiter_char)
            .saturating_add(1)
            .max(3);
        let (indent, _, _) = fence.delimiter();
        let mut replacement = String::new();
        replacement.push_str(&" ".repeat(indent));
        replacement.extend(std::iter::repeat_n(delimiter_char, delimiter_len));
        replacement.push_str(fence.info());
        replacement.push_str(newline);
        replacement.push_str(&normalized_source);
        if !normalized_source.is_empty() && !normalized_source.ends_with(newline) {
            replacement.push_str(newline);
        }
        replacement.push_str(&" ".repeat(indent));
        replacement.extend(std::iter::repeat_n(delimiter_char, delimiter_len));
        if fence.raw_fence().ends_with(['\r', '\n']) {
            replacement.push_str(newline);
        }
        PreparedMarkdownEdit::prepare_with_replacement_limit(
            &self.source,
            fence.full_range(),
            replacement,
            MAX_MERMAID_FENCE_BYTES
                .saturating_mul(4)
                .saturating_add(256),
        )
    }
}

fn normalize_line_endings(source: &str, newline: &str) -> String {
    let mut normalized = String::with_capacity(source.len());
    let mut characters = source.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '\r' => {
                if characters.peek() == Some(&'\n') {
                    characters.next();
                }
                normalized.push_str(newline);
            }
            '\n' => normalized.push_str(newline),
            character => normalized.push(character),
        }
    }
    normalized
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MarkdownPreviewError {
    SourceTooLarge { max_bytes: usize },
}

impl fmt::Display for MarkdownPreviewError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceTooLarge { max_bytes } => {
                write!(
                    f,
                    "Markdown source exceeds the {max_bytes}-byte preview limit"
                )
            }
        }
    }
}

impl std::error::Error for MarkdownPreviewError {}

fn markdown_options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES
}

fn is_mermaid_info(info: &str) -> bool {
    info.split_ascii_whitespace()
        .next()
        .is_some_and(|language| language.eq_ignore_ascii_case("mermaid"))
}

fn contains_closing_delimiter(source: &str, marker: char, marker_len: usize) -> bool {
    source.lines().any(|line| {
        let indent = line.bytes().take_while(|byte| *byte == b' ').count();
        let bytes = line.as_bytes();
        let run = bytes[indent..]
            .iter()
            .take_while(|byte| **byte as char == marker)
            .count();
        indent <= 3
            && run >= marker_len
            && bytes[indent + run..]
                .iter()
                .all(|byte| matches!(byte, b' ' | b'\t'))
    })
}

fn longest_fence_run(source: &str, marker: char) -> usize {
    source.lines().fold(0usize, |max_run, line| {
        let indent = line.bytes().take_while(|byte| *byte == b' ').count();
        let run = if indent <= 3 {
            line.as_bytes()[indent..]
                .iter()
                .take_while(|byte| **byte as char == marker)
                .count()
        } else {
            0
        };
        max_run.max(run)
    })
}

#[derive(Clone, Debug)]
struct FenceDetails {
    body_range: Range<usize>,
    indent: usize,
    marker: char,
    marker_len: usize,
    closed: bool,
}

fn inspect_fence(source: &str, full_range: Range<usize>) -> Option<FenceDetails> {
    if full_range.start > full_range.end || full_range.end > source.len() {
        return None;
    }
    if !source.is_char_boundary(full_range.start) || !source.is_char_boundary(full_range.end) {
        return None;
    }
    let block = &source[full_range.clone()];
    let first_line_end = block.find('\n').map_or(block.len(), |index| index + 1);
    let opener = block[..first_line_end].trim_end_matches(['\r', '\n']);
    let spaces = opener.bytes().take_while(|byte| *byte == b' ').count();
    if spaces > 3 {
        return None;
    }
    let marker = *opener.as_bytes().get(spaces)? as char;
    if !matches!(marker, '`' | '~') {
        return None;
    }
    let marker_len = opener.as_bytes()[spaces..]
        .iter()
        .take_while(|byte| **byte as char == marker)
        .count();
    if marker_len < 3 {
        return None;
    }
    let body_start = full_range.start + first_line_end;
    let last_end = block.trim_end_matches(['\r', '\n']).len();
    let last_start = block[..last_end].rfind('\n').map_or(0, |index| index + 1);
    let last_line = block[last_start..last_end].trim_end_matches('\r');
    let leading = last_line.bytes().take_while(|byte| *byte == b' ').count();
    let run = last_line.as_bytes()[leading..]
        .iter()
        .take_while(|byte| **byte as char == marker)
        .count();
    let closed = full_range.start + last_start >= body_start
        && full_range.start + last_start > full_range.start
        && leading <= 3
        && run >= marker_len
        && last_line.as_bytes()[leading + run..]
            .iter()
            .all(|byte| matches!(byte, b' ' | b'\t'));
    let body_end = if closed {
        full_range.start + last_start
    } else {
        full_range.end
    };
    Some(FenceDetails {
        body_range: body_start.min(body_end)..body_end,
        indent: spaces,
        marker,
        marker_len,
        closed,
    })
}

fn prepare_diagram(fence: &mut MermaidFencePreview, aggregate_svg_bytes: &mut usize) -> bool {
    let source = fence.raw_source();
    let session = match DiagramSession::from_fresh_source(source) {
        Ok(session) => session,
        Err(error) => {
            fence.status = MermaidFenceStatus::PreviewEncodingFailed(error.to_string());
            return true;
        }
    };
    let status = session.status();
    if let Some(diagnostic) = status.diagnostic {
        fence.status = match diagnostic.kind {
            MermaidDiagnosticKind::Unsupported => MermaidFenceStatus::Unsupported(diagnostic),
            MermaidDiagnosticKind::Invalid | MermaidDiagnosticKind::LimitExceeded => {
                MermaidFenceStatus::Invalid(diagnostic)
            }
        };
        return true;
    }
    if status.preview != PreviewState::Current {
        fence.status = MermaidFenceStatus::PreviewEncodingFailed(
            "Diagram parser did not produce a current preview".to_owned(),
        );
        return true;
    }
    let snapshot = match DiagramImageSnapshot::capture(&session, SceneMetrics::default()) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            fence.status = MermaidFenceStatus::PreviewEncodingFailed(export_error_text(error));
            return true;
        }
    };
    match snapshot.encode_svg(1.0) {
        Ok(artifact) => match String::from_utf8(artifact.bytes().to_vec()) {
            Ok(svg) => {
                if aggregate_svg_bytes.saturating_add(svg.len()) > MAX_MERMAID_PREVIEW_SVG_BYTES {
                    fence.status = MermaidFenceStatus::SvgAggregateLimitExceeded {
                        max_bytes: MAX_MERMAID_PREVIEW_SVG_BYTES,
                    };
                    return false;
                }
                *aggregate_svg_bytes += svg.len();
                fence.svg = Some(svg);
                fence.status = MermaidFenceStatus::Current;
                true
            }
            Err(_) => {
                fence.status = MermaidFenceStatus::PreviewEncodingFailed(
                    "Diagram SVG encoder returned invalid UTF-8".to_owned(),
                );
                true
            }
        },
        Err(error) => {
            fence.status = MermaidFenceStatus::PreviewEncodingFailed(export_error_text(error));
            true
        }
    }
}

fn export_error_text(error: DiagramExportError) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_preview_keeps_full_unicode_source_and_exact_mermaid_body_offsets() {
        let source =
            "# 日記\r\n\r\n```mermaid title\r\nflowchart TD\r\nA[\"Café\"] --> B\r\n```\r\n";
        let preview = MarkdownPreview::analyze(source).unwrap();
        assert_eq!(preview.source(), source);
        assert_eq!(preview.status(), &MarkdownPreviewStatus::Complete);
        let fence = &preview.mermaid_fences()[0];
        assert_eq!(fence.raw_source(), "flowchart TD\r\nA[\"Café\"] --> B\r\n");
        assert_eq!(&source[fence.full_range()], fence.raw_fence());
        assert_eq!(&source[fence.body_range()], fence.raw_source());
        assert!(matches!(fence.status(), MermaidFenceStatus::Current));
        assert!(fence.trusted_svg().unwrap().contains("<svg"));
        let analysis = preview.analysis().unwrap();
        assert_eq!(analysis.headings[0].title, "日記");
    }

    #[test]
    fn markdown_preview_retains_invalid_unclosed_and_oversized_fences() {
        let invalid = "```mermaid\nflowchart TD\nA -->\n```\n";
        let invalid_preview = MarkdownPreview::analyze(invalid).unwrap();
        assert!(matches!(
            invalid_preview.mermaid_fences()[0].status(),
            MermaidFenceStatus::Invalid(_) | MermaidFenceStatus::Unsupported(_)
        ));
        assert!(
            invalid_preview.mermaid_fences()[0]
                .raw_source()
                .contains("A -->")
        );

        let unclosed = "```mermaid\nflowchart TD\nA --> B\n";
        let preview = MarkdownPreview::analyze(unclosed).unwrap();
        assert_eq!(
            preview.mermaid_fences()[0].status(),
            &MermaidFenceStatus::Unclosed
        );
        assert_eq!(preview.source(), unclosed);

        let oversized = format!("```mermaid\n{}\n```\n", "x".repeat(MAX_MERMAID_FENCE_BYTES));
        let preview = MarkdownPreview::analyze(&oversized).unwrap();
        assert!(matches!(
            preview.mermaid_fences()[0].status(),
            MermaidFenceStatus::TooLarge { .. }
        ));
        assert_eq!(
            preview.mermaid_fences()[0].raw_source().len(),
            MAX_MERMAID_FENCE_BYTES + 1
        );
    }

    #[test]
    fn markdown_preview_bounds_source_and_fence_count_without_losing_raw_source() {
        assert!(matches!(
            MarkdownPreview::analyze(&"x".repeat(MAX_MARKDOWN_SOURCE_BYTES + 1)),
            Err(MarkdownPreviewError::SourceTooLarge { .. })
        ));
        let source = (0..=MAX_MERMAID_PREVIEW_FENCES)
            .map(|index| format!("```mermaid\nflowchart TD\nA --> B{index}\n```\n"))
            .collect::<String>();
        let preview = MarkdownPreview::analyze(&source).unwrap();
        assert_eq!(preview.source(), source);
        assert_eq!(
            preview.status(),
            &MarkdownPreviewStatus::LimitExceeded(MarkdownPreviewLimit::FenceCount)
        );
        assert!(preview.mermaid_fences().is_empty());

        let many_lines = "\n".repeat(MAX_MARKDOWN_PREVIEW_LINES + 1);
        let preview = MarkdownPreview::analyze(&many_lines).unwrap();
        assert_eq!(preview.source(), many_lines);
        assert_eq!(preview.analysis(), None);
        assert_eq!(
            preview.status(),
            &MarkdownPreviewStatus::LimitExceeded(MarkdownPreviewLimit::SourceLines)
        );
    }

    #[test]
    fn markdown_preview_prepares_exact_fence_replacement_and_rejects_stale_source() {
        let source = "before\r\n~~~mermaid\r\nflowchart TD\r\nA --> B\r\n~~~\r\nafter\r\n";
        let preview = MarkdownPreview::analyze(source).unwrap();
        let copy = preview.mermaid_fences()[0]
            .to_diagram_source_copy()
            .unwrap();
        assert_eq!(copy.source(), "flowchart TD\r\nA --> B\r\n");
        let edit = preview
            .prepare_fence_replacement(0, "flowchart TD\r\n~~~\r\nA --> B\r\n")
            .unwrap();
        let applied = edit.apply(source).unwrap();
        assert!(applied.source.starts_with("before\r\n"));
        assert!(applied.source.ends_with("after\r\n"));
        let updated = MarkdownPreview::analyze(&applied.source).unwrap();
        assert_eq!(
            updated.mermaid_fences()[0].raw_source(),
            "flowchart TD\r\n~~~\r\nA --> B\r\n"
        );
        assert_eq!(
            edit.apply("changed\r\n~~~mermaid\r\nflowchart TD\r\nA --> B\r\n~~~\r\nafter\r\n"),
            Err(MarkdownEditError::SourceChanged)
        );
    }

    #[test]
    fn markdown_preview_replacement_without_final_newline_keeps_the_fence_closed() {
        let source = "before\r\n```mermaid\r\nold\r\n```\r\nafter\r\n";
        let preview = MarkdownPreview::analyze(source).unwrap();
        let edit = preview
            .prepare_fence_replacement(0, "flowchart TD\nA --> B")
            .unwrap();
        let applied = edit.apply(source).unwrap();
        assert!(applied.source.starts_with("before\r\n```mermaid\r\n"));
        assert!(applied.source.ends_with("```\r\nafter\r\n"));
        let updated = MarkdownPreview::analyze(&applied.source).unwrap();
        assert_eq!(updated.mermaid_fences().len(), 1);
        assert!(updated.mermaid_fences()[0].is_closed());
        assert_eq!(
            updated.mermaid_fences()[0].raw_source(),
            "flowchart TD\r\nA --> B\r\n"
        );
        assert_eq!(
            applied.source,
            "before\r\n```mermaid\r\nflowchart TD\r\nA --> B\r\n```\r\nafter\r\n"
        );
    }
}
