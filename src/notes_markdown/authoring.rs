//! Bounded, source-bound Markdown authoring operations shared by note tools.

use std::{error::Error, fmt, ops::Range};

use super::mutations;

pub const MAX_MARKDOWN_SOURCE_BYTES: usize = 1024 * 1024;
pub const MAX_MARKDOWN_SNIPPET_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MarkdownEditError {
    SourceTooLarge { max_bytes: usize },
    SnippetTooLarge { max_bytes: usize },
    InvalidRange,
    InvalidUtf8Boundary,
    SourceChanged,
    InvalidParameter(&'static str),
}

impl fmt::Display for MarkdownEditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceTooLarge { max_bytes } => {
                write!(f, "Markdown source exceeds the {max_bytes}-byte limit")
            }
            Self::SnippetTooLarge { max_bytes } => {
                write!(f, "Markdown edit exceeds the {max_bytes}-byte limit")
            }
            Self::InvalidRange => f.write_str("selection is outside the Markdown source"),
            Self::InvalidUtf8Boundary => {
                f.write_str("selection is not on UTF-8 character boundaries")
            }
            Self::SourceChanged => {
                f.write_str("Markdown source changed after this edit was prepared")
            }
            Self::InvalidParameter(message) => f.write_str(message),
        }
    }
}

impl Error for MarkdownEditError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

/// A replacement bound to the exact source snapshot and UTF-8 byte selection
/// from which it was prepared. Applying it to another source is a conflict.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedMarkdownEdit {
    expected_source: String,
    range: Range<usize>,
    replacement: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppliedMarkdownEdit {
    pub source: String,
    pub selection: Range<usize>,
}

impl PreparedMarkdownEdit {
    pub fn prepare(
        source: &str,
        range: Range<usize>,
        replacement: impl Into<String>,
    ) -> Result<Self, MarkdownEditError> {
        Self::prepare_with_replacement_limit(source, range, replacement, MAX_MARKDOWN_SNIPPET_BYTES)
    }

    pub(crate) fn prepare_with_replacement_limit(
        source: &str,
        range: Range<usize>,
        replacement: impl Into<String>,
        max_replacement_bytes: usize,
    ) -> Result<Self, MarkdownEditError> {
        validate_source(source)?;
        validate_range(source, &range)?;
        let replacement = replacement.into();
        if replacement.len() > max_replacement_bytes {
            return Err(MarkdownEditError::SnippetTooLarge {
                max_bytes: max_replacement_bytes,
            });
        }
        let output_len = source
            .len()
            .saturating_sub(range.end - range.start)
            .checked_add(replacement.len())
            .ok_or(MarkdownEditError::SourceTooLarge {
                max_bytes: MAX_MARKDOWN_SOURCE_BYTES,
            })?;
        if output_len > MAX_MARKDOWN_SOURCE_BYTES {
            return Err(MarkdownEditError::SourceTooLarge {
                max_bytes: MAX_MARKDOWN_SOURCE_BYTES,
            });
        }
        Ok(Self {
            expected_source: source.to_owned(),
            range,
            replacement,
        })
    }

    pub fn expected_source(&self) -> &str {
        &self.expected_source
    }

    pub fn range(&self) -> Range<usize> {
        self.range.clone()
    }

    pub fn replacement(&self) -> &str {
        &self.replacement
    }

    pub fn apply(&self, current_source: &str) -> Result<AppliedMarkdownEdit, MarkdownEditError> {
        validate_source(current_source)?;
        if current_source != self.expected_source {
            return Err(MarkdownEditError::SourceChanged);
        }
        validate_range(current_source, &self.range)?;
        let end = self.range.start + self.replacement.len();
        let mut source = String::with_capacity(
            current_source.len() - (self.range.end - self.range.start) + self.replacement.len(),
        );
        source.push_str(&current_source[..self.range.start]);
        source.push_str(&self.replacement);
        source.push_str(&current_source[self.range.end..]);
        Ok(AppliedMarkdownEdit {
            source,
            selection: self.range.start..end,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MarkdownInlineFormat {
    Strong,
    Emphasis,
    Strike,
    Code,
    Link { destination: String },
    Image { destination: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MarkdownBlockSnippet {
    Heading { level: u8 },
    Quote,
    Bullet,
    Task { checked: bool },
    Callout { kind: String, title: String },
    CodeFence { language: String },
    WikiLink,
    TodoMarker { id: String },
}

pub fn prepare_inline_format(
    source: &str,
    selection: Range<usize>,
    format: MarkdownInlineFormat,
) -> Result<PreparedMarkdownEdit, MarkdownEditError> {
    validate_source(source)?;
    validate_range(source, &selection)?;
    let selected = &source[selection.clone()];
    let replacement = match format {
        MarkdownInlineFormat::Strong => format!("**{selected}**"),
        MarkdownInlineFormat::Emphasis => format!("*{selected}*"),
        MarkdownInlineFormat::Strike => format!("~~{selected}~~"),
        MarkdownInlineFormat::Code => inline_code(selected),
        MarkdownInlineFormat::Link { destination } => {
            validate_destination(&destination)?;
            format!("[{}](<{}>)", escape_link_label(selected), destination)
        }
        MarkdownInlineFormat::Image { destination } => {
            validate_destination(&destination)?;
            format!("![{}](<{}>)", escape_link_label(selected), destination)
        }
    };
    PreparedMarkdownEdit::prepare(source, selection, replacement)
}

pub fn prepare_block_snippet(
    source: &str,
    selection: Range<usize>,
    snippet: MarkdownBlockSnippet,
) -> Result<PreparedMarkdownEdit, MarkdownEditError> {
    validate_source(source)?;
    validate_range(source, &selection)?;
    let selected = &source[selection.clone()];
    let newline = dominant_newline(source);
    let replacement = match snippet {
        MarkdownBlockSnippet::Heading { level } => {
            if !(1..=6).contains(&level) {
                return Err(MarkdownEditError::InvalidParameter(
                    "heading level must be between 1 and 6",
                ));
            }
            format!("{} {}{newline}", "#".repeat(level as usize), selected)
        }
        MarkdownBlockSnippet::Quote => {
            if selected.is_empty() {
                "> ".to_owned()
            } else {
                selected
                    .split_inclusive('\n')
                    .map(|line| format!("> {line}"))
                    .collect::<String>()
            }
        }
        MarkdownBlockSnippet::Bullet => {
            if selected.is_empty() {
                "- ".to_owned()
            } else {
                selected
                    .split_inclusive('\n')
                    .map(|line| format!("- {line}"))
                    .collect::<String>()
            }
        }
        MarkdownBlockSnippet::Task { checked } => {
            format!(
                "- [{}] {selected}{newline}",
                if checked { "x" } else { " " }
            )
        }
        MarkdownBlockSnippet::Callout { kind, title } => {
            let wrapped =
                mutations::wrap_selection_in_callout(selected, 0..selected.len(), &kind, &title)
                    .map_err(|_| MarkdownEditError::InvalidRange)?;
            if newline == "\r\n" {
                wrapped.replace('\n', "\r\n")
            } else {
                wrapped
            }
        }
        MarkdownBlockSnippet::CodeFence { language } => {
            validate_fence_language(&language)?;
            format_fenced_code(selected, &language, newline)
        }
        MarkdownBlockSnippet::WikiLink => format!("[[{}]]", selected.trim()),
        MarkdownBlockSnippet::TodoMarker { id } => {
            if id.is_empty()
                || id.len() > 128
                || !id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b":_-".contains(&byte))
            {
                return Err(MarkdownEditError::InvalidParameter(
                    "todo marker ID must use at most 128 ASCII letters, digits, :, _ or -",
                ));
            }
            format!("{selected} <!-- ml:todo:{id} -->")
        }
    };
    PreparedMarkdownEdit::prepare(source, selection, replacement)
}

pub fn validate_source(source: &str) -> Result<(), MarkdownEditError> {
    if source.len() > MAX_MARKDOWN_SOURCE_BYTES {
        Err(MarkdownEditError::SourceTooLarge {
            max_bytes: MAX_MARKDOWN_SOURCE_BYTES,
        })
    } else {
        Ok(())
    }
}

pub(crate) fn validate_range(source: &str, range: &Range<usize>) -> Result<(), MarkdownEditError> {
    if range.start > range.end || range.end > source.len() {
        return Err(MarkdownEditError::InvalidRange);
    }
    if !source.is_char_boundary(range.start) || !source.is_char_boundary(range.end) {
        return Err(MarkdownEditError::InvalidUtf8Boundary);
    }
    Ok(())
}

pub(crate) fn dominant_newline(source: &str) -> &'static str {
    if source.as_bytes().windows(2).any(|pair| pair == b"\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

fn validate_destination(destination: &str) -> Result<(), MarkdownEditError> {
    if destination.is_empty()
        || destination.len() > 2048
        || destination.chars().any(|character| {
            character.is_control() || character.is_whitespace() || matches!(character, '<' | '>')
        })
    {
        return Err(MarkdownEditError::InvalidParameter(
            "link destination must be nonempty and contain no whitespace or controls",
        ));
    }
    Ok(())
}

fn validate_fence_language(language: &str) -> Result<(), MarkdownEditError> {
    if language.len() > 64
        || language.chars().any(|character| {
            character.is_whitespace() || character.is_control() || matches!(character, '`' | '~')
        })
    {
        return Err(MarkdownEditError::InvalidParameter(
            "code fence language must be a short single token",
        ));
    }
    Ok(())
}

fn escape_link_label(label: &str) -> String {
    label
        .replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]")
}

fn inline_code(selected: &str) -> String {
    let longest_run = longest_run(selected, '`');
    let delimiter = "`".repeat(longest_run + 1);
    let needs_padding = (selected.starts_with('`') || selected.ends_with('`'))
        || (selected.starts_with(' ') && selected.ends_with(' ') && !selected.trim().is_empty());
    if needs_padding {
        format!("{delimiter} {selected} {delimiter}")
    } else {
        format!("{delimiter}{selected}{delimiter}")
    }
}

fn format_fenced_code(selected: &str, language: &str, newline: &str) -> String {
    let max_ticks = longest_run(selected, '`');
    let max_tildes = longest_run(selected, '~');
    let marker = if max_ticks <= max_tildes { '`' } else { '~' };
    let delimiter = marker.to_string().repeat(3 + longest_run(selected, marker));
    let mut content = selected.to_owned();
    if !content.is_empty() && !content.ends_with(['\r', '\n']) {
        content.push_str(newline);
    }
    format!("{delimiter}{language}{newline}{content}{delimiter}{newline}")
}

fn longest_run(text: &str, needle: char) -> usize {
    text.chars()
        .fold((0usize, 0usize), |(best, current), character| {
            if character == needle {
                (best.max(current + 1), current + 1)
            } else {
                (best, 0)
            }
        })
        .0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_authoring_edits_are_utf8_safe_and_source_bound() {
        let source = "Before 🌱 after\r\n";
        let range = source.find('🌱').unwrap()..source.find('🌱').unwrap() + "🌱".len();
        let edit = prepare_inline_format(source, range, MarkdownInlineFormat::Strong).unwrap();
        let applied = edit.apply(source).unwrap();
        assert_eq!(applied.source, "Before **🌱** after\r\n");
        assert_eq!(&applied.source[applied.selection], "**🌱**");
        assert_eq!(
            edit.apply("Before 🌼 after\r\n"),
            Err(MarkdownEditError::SourceChanged)
        );
        assert_eq!(
            PreparedMarkdownEdit::prepare(source, 8..9, "x").unwrap_err(),
            MarkdownEditError::InvalidUtf8Boundary
        );
    }

    #[test]
    fn markdown_authoring_snippets_preserve_newline_style_and_safe_fences() {
        let source = "A\r\nB\r\n";
        let selected = 3..4;
        let task = prepare_block_snippet(
            source,
            selected.clone(),
            MarkdownBlockSnippet::Task { checked: false },
        )
        .unwrap()
        .apply(source)
        .unwrap();
        assert_eq!(task.source, "A\r\n- [ ] B\r\n\r\n");

        let code = prepare_block_snippet(
            "x\n",
            0..1,
            MarkdownBlockSnippet::CodeFence {
                language: "text".into(),
            },
        )
        .unwrap()
        .apply("x\n")
        .unwrap();
        assert_eq!(code.source, "```text\nx\n```\n\n");
        assert!(
            prepare_inline_format(
                "label",
                0..5,
                MarkdownInlineFormat::Link {
                    destination: "javascript:alert(1)".into(),
                }
            )
            .is_ok()
        ); // HTML export applies the separate navigation policy.

        let empty_quote = prepare_block_snippet("", 0..0, MarkdownBlockSnippet::Quote)
            .unwrap()
            .apply("")
            .unwrap();
        assert_eq!(empty_quote.source, "> ");
        assert_eq!(empty_quote.selection, 0..2);
        let empty_bullet = prepare_block_snippet("", 0..0, MarkdownBlockSnippet::Bullet)
            .unwrap()
            .apply("")
            .unwrap();
        assert_eq!(empty_bullet.source, "- ");
        assert_eq!(empty_bullet.selection, 0..2);
    }

    #[test]
    fn markdown_authoring_rejects_output_growth_past_bound() {
        let source = "x".repeat(MAX_MARKDOWN_SOURCE_BYTES);
        assert!(matches!(
            PreparedMarkdownEdit::prepare(&source, source.len()..source.len(), "y"),
            Err(MarkdownEditError::SourceTooLarge { .. })
        ));
    }
}
