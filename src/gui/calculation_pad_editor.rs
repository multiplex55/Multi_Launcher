//! Position mapping and literal search over the Pad's authoritative source.

use std::ops::Range;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FoldRegion {
    pub header: usize,
    pub hidden: Range<usize>,
}

/// Find physical loop and explicit comment-region bodies without changing the source.
pub(super) fn fold_regions(source: &str) -> Vec<FoldRegion> {
    let mut lines = Vec::new();
    let mut offset = 0;
    for raw in source.split_inclusive('\n') {
        let content = raw
            .strip_suffix('\n')
            .unwrap_or(raw)
            .strip_suffix('\r')
            .unwrap_or(raw.strip_suffix('\n').unwrap_or(raw));
        lines.push((offset, offset + raw.len(), content));
        offset += raw.len();
    }
    if source.is_empty() || source.ends_with('\n') {
        lines.push((offset, offset, ""));
    }
    let mut regions = Vec::new();
    let mut stack = Vec::new();
    for (index, &(start, end, text)) in lines.iter().enumerate() {
        let trimmed = text.trim();
        if trimmed == "# region" || trimmed.starts_with("# region ") {
            stack.push((index, end));
        } else if trimmed == "# endregion" {
            if let Some((header, hidden_start)) = stack.pop()
                && hidden_start < end
            {
                regions.push(FoldRegion {
                    header,
                    hidden: hidden_start..end,
                });
            }
        }
        if crate::calculation_pad::editor_loop_header(trimmed) {
            let indent = text.len() - text.trim_start().len();
            let mut body_end = end;
            let mut has_body = false;
            for &(line_start, line_end, line) in lines.iter().skip(index + 1) {
                let content = line.trim();
                if content.is_empty() || content.starts_with('#') || content.starts_with("//") {
                    body_end = line_end;
                    continue;
                }
                let next_indent = line.len() - line.trim_start().len();
                if next_indent <= indent {
                    body_end = line_start;
                    break;
                }
                has_body = true;
                body_end = line_end;
            }
            if has_body && end < body_end {
                regions.push(FoldRegion {
                    header: index,
                    hidden: end..body_end,
                });
            }
        }
        let _ = start;
    }
    regions.sort_by_key(|region| (region.header, region.hidden.end));
    regions
}

#[derive(Clone, Debug)]
pub(super) struct Projection {
    pub visible: String,
    /// Corresponding byte ranges of visible source pieces.
    pieces: Vec<(Range<usize>, Range<usize>)>,
    pub gaps: Vec<(usize, Range<usize>)>,
    pub visible_lines: Vec<usize>,
}

impl Projection {
    pub fn byte_at_character(&self, character: usize) -> Option<usize> {
        self.visible
            .char_indices()
            .map(|(byte, _)| byte)
            .chain(std::iter::once(self.visible.len()))
            .nth(character)
    }

    /// Apply one contiguous visible edit while retaining every hidden byte.
    pub fn apply_edit(&self, source: &str, edited: &str) -> Option<String> {
        let mut prefix = 0;
        for (left, right) in self.visible.chars().zip(edited.chars()) {
            if left != right {
                break;
            }
            prefix += left.len_utf8();
        }
        let mut old_end = self.visible.len();
        let mut new_end = edited.len();
        while old_end > prefix && new_end > prefix {
            let left = self.visible[..old_end].chars().next_back()?;
            let right = edited[..new_end].chars().next_back()?;
            if left != right {
                break;
            }
            old_end -= left.len_utf8();
            new_end -= right.len_utf8();
        }
        if self
            .gaps
            .iter()
            .any(|(at, _)| (prefix < *at && *at < old_end) || (prefix == old_end && prefix == *at))
        {
            return None;
        }
        let start = self.source_byte(prefix)?;
        let end = self.pieces.iter().find_map(|(view, source)| {
            (view.start <= old_end && old_end <= view.end)
                .then(|| source.start + old_end - view.start)
        })?;
        let mut result = source.to_owned();
        result.replace_range(start..end, edited.get(prefix..new_end)?);
        Some(result)
    }
    pub fn new(source: &str, hidden: &[Range<usize>]) -> Self {
        let mut ranges = hidden.to_vec();
        ranges.sort_by_key(|range| (range.start, range.end));
        let mut merged: Vec<Range<usize>> = Vec::new();
        for range in ranges {
            if let Some(last) = merged.last_mut()
                && range.start <= last.end
            {
                last.end = last.end.max(range.end);
            } else {
                merged.push(range);
            }
        }
        let mut visible = String::new();
        let mut pieces = Vec::new();
        let mut gaps = Vec::new();
        let mut cursor = 0;
        for range in &merged {
            let start = visible.len();
            visible.push_str(&source[cursor..range.start]);
            pieces.push((start..visible.len(), cursor..range.start));
            gaps.push((visible.len(), range.clone()));
            cursor = range.end;
        }
        let start = visible.len();
        visible.push_str(&source[cursor..]);
        pieces.push((start..visible.len(), cursor..source.len()));
        let mut visible_lines = Vec::new();
        let mut source_offset = 0;
        for (index, raw) in source.split_inclusive('\n').enumerate() {
            if !merged
                .iter()
                .any(|range| range.start <= source_offset && source_offset < range.end)
            {
                visible_lines.push(index);
            }
            source_offset += raw.len();
        }
        if source.is_empty() || source.ends_with('\n') {
            visible_lines.push(source.bytes().filter(|&b| b == b'\n').count());
        }
        Self {
            visible,
            pieces,
            gaps,
            visible_lines,
        }
    }

    pub fn source_byte(&self, visible_byte: usize) -> Option<usize> {
        if !self.visible.is_char_boundary(visible_byte) {
            return None;
        }
        self.pieces.iter().rev().find_map(|(view, source)| {
            (view.start <= visible_byte && visible_byte <= view.end)
                .then(|| source.start + visible_byte - view.start)
        })
    }

    pub fn visible_byte(&self, source_byte: usize) -> Option<usize> {
        self.pieces.iter().find_map(|(view, source)| {
            (source.start <= source_byte && source_byte <= source.end)
                .then(|| view.start + source_byte - source.start)
        })
    }

    pub fn source_character(&self, source: &str, visible_char: usize) -> Option<usize> {
        let byte = self
            .visible
            .char_indices()
            .map(|(byte, _)| byte)
            .chain(std::iter::once(self.visible.len()))
            .nth(visible_char)?;
        Some(source.get(..self.source_byte(byte)?)?.chars().count())
    }

    pub fn visible_character(&self, source: &str, source_char: usize) -> Option<usize> {
        let byte = source
            .char_indices()
            .map(|(byte, _)| byte)
            .chain(std::iter::once(source.len()))
            .nth(source_char)?;
        Some(
            self.visible
                .get(..self.visible_byte(byte)?)?
                .chars()
                .count(),
        )
    }

    pub fn visible_character_clamped(&self, source: &str, source_char: usize) -> Option<usize> {
        let byte = source
            .char_indices()
            .map(|(byte, _)| byte)
            .chain(std::iter::once(source.len()))
            .nth(source_char)?;
        let view = self.visible_byte(byte).or_else(|| {
            self.gaps
                .iter()
                .find_map(|(view, range)| (range.start < byte && byte < range.end).then_some(*view))
        })?;
        Some(self.visible.get(..view)?.chars().count())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SyntaxKind {
    Text,
    Number,
    Name,
    Call,
    Keyword,
    Constant,
    Operator,
    Comment,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SyntaxSpan {
    pub range: Range<usize>,
    pub kind: SyntaxKind,
}

pub(super) fn syntax_spans(source: &str) -> Vec<SyntaxSpan> {
    let mut spans = Vec::new();
    let mut offset = 0;
    for line in source.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') || trimmed.starts_with("//") {
            spans.push(SyntaxSpan {
                range: offset..offset + line.len(),
                kind: SyntaxKind::Comment,
            });
            offset += line.len();
            continue;
        }
        let mut pos = 0;
        while pos < line.len() {
            let start = pos;
            let ch = line[pos..].chars().next().unwrap();
            pos += ch.len_utf8();
            let kind = if ch == '{' {
                if let Some(end) = line[pos..].find('}') {
                    pos += end + 1;
                }
                SyntaxKind::Name
            } else if ch.is_ascii_digit()
                || ch == '.' && line[pos..].starts_with(|c: char| c.is_ascii_digit())
            {
                while let Some(next) = line[pos..].chars().next() {
                    if !(next.is_ascii_alphanumeric() || matches!(next, '.' | '_')) {
                        break;
                    }
                    pos += next.len_utf8();
                }
                SyntaxKind::Number
            } else if ch.is_alphabetic() || ch == '_' {
                while let Some(next) = line[pos..].chars().next() {
                    if !(next.is_alphanumeric() || next == '_') {
                        break;
                    }
                    pos += next.len_utf8();
                }
                let token = &line[start..pos];
                if matches!(token, "for" | "in") {
                    SyntaxKind::Keyword
                } else if matches!(token, "true" | "false" | "PI" | "E") {
                    SyntaxKind::Constant
                } else if matches!(token, "and" | "or" | "xor" | "shl" | "shr" | "mod")
                    && !line[pos..].trim_start().starts_with('(')
                {
                    SyntaxKind::Operator
                } else if line[pos..].trim_start().starts_with('(')
                    && crate::calculation_pad::function_signature_help(token).is_some()
                {
                    SyntaxKind::Call
                } else {
                    SyntaxKind::Name
                }
            } else if "+-*/%^=<>!&|~".contains(ch) {
                SyntaxKind::Operator
            } else {
                SyntaxKind::Text
            };
            spans.push(SyntaxSpan {
                range: offset + start..offset + pos,
                kind,
            });
        }
        offset += line.len();
    }
    spans
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Suggestion {
    pub label: String,
    pub insertion: String,
    pub replacement: Range<usize>,
    pub cursor: usize,
}

pub(super) fn suggestions(source: &str, caret: usize) -> Vec<Suggestion> {
    let Some(prefix) = source.get(..caret) else {
        return Vec::new();
    };
    let line_start = prefix.rfind('\n').map_or(0, |index| index + 1);
    let line = &prefix[line_start..];
    if line.trim_start().starts_with('#') || line.trim_start().starts_with("//") {
        return Vec::new();
    }
    let braced = line
        .rfind('{')
        .filter(|start| !line[*start..].contains('}'));
    let start = braced.map_or_else(
        || {
            line.char_indices()
                .rev()
                .find_map(|(index, ch)| {
                    (!(ch.is_alphanumeric() || ch == '_')).then_some(index + ch.len_utf8())
                })
                .unwrap_or(0)
        },
        |index| index,
    );
    let fragment = &line[start..];
    if fragment.is_empty() {
        return Vec::new();
    }
    let end = if braced.is_some() {
        source[caret..]
            .find('}')
            .filter(|end| !source[caret..caret + end].contains('\n'))
            .map_or(caret, |end| caret + end + 1)
    } else {
        caret
            + source[caret..]
                .chars()
                .take_while(|ch| ch.is_alphanumeric() || *ch == '_')
                .map(char::len_utf8)
                .sum::<usize>()
    };
    let replacement = line_start + start..end;
    let mut output = Vec::new();
    for name in crate::calculation_pad::editor_bindings(source, caret) {
        if name.starts_with(fragment) && name != fragment {
            let cursor = replacement.start + name.len();
            output.push(Suggestion {
                label: name.clone(),
                insertion: name,
                replacement: replacement.clone(),
                cursor,
            });
        }
    }
    if braced.is_none() {
        for function in crate::calculation_pad::FUNCTIONS {
            let name = function.name;
            if name.starts_with(fragment) && name != fragment {
                let suffix = source[caret..]
                    .chars()
                    .take_while(|ch| ch.is_alphanumeric() || *ch == '_')
                    .map(char::len_utf8)
                    .sum::<usize>();
                let end = caret + suffix;
                let has_call = source[end..].trim_start().starts_with('(');
                let insertion = if has_call {
                    name.to_owned()
                } else {
                    format!("{name}()")
                };
                let cursor = replacement.start + name.len() + usize::from(!has_call);
                output.push(Suggestion {
                    label: name.to_owned(),
                    insertion,
                    replacement: replacement.start..end,
                    cursor,
                });
            }
        }
    }
    output.truncate(8);
    output
}

pub(super) fn call_hint(source: &str, caret: usize) -> Option<(String, usize)> {
    let prefix = source.get(..caret)?;
    let line = prefix.rsplit('\n').next()?;
    if line.trim_start().starts_with('#') || line.trim_start().starts_with("//") {
        return None;
    }
    let mut stack: Vec<(Option<String>, usize)> = Vec::new();
    let mut braces = false;
    for (index, ch) in line.char_indices() {
        if braces {
            if ch == '}' {
                braces = false;
            }
            continue;
        }
        if ch == '{' {
            braces = true;
            continue;
        }
        match ch {
            '(' => {
                let before = line[..index].trim_end();
                let start = before
                    .char_indices()
                    .rev()
                    .find_map(|(pos, c)| {
                        (!(c.is_alphanumeric() || c == '_')).then_some(pos + c.len_utf8())
                    })
                    .unwrap_or(0);
                let name = &before[start..];
                stack.push((
                    crate::calculation_pad::function_signature_help(name).map(str::to_owned),
                    0,
                ));
            }
            '[' => stack.push((None, 0)),
            ')' | ']' => {
                stack.pop();
            }
            ',' => {
                if let Some((_, index)) = stack.last_mut() {
                    *index += 1;
                }
            }
            _ => {}
        }
    }
    stack
        .into_iter()
        .rev()
        .find_map(|(signature, index)| signature.map(|signature| (signature, index)))
}

#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct CompletionState {
    pub selected: usize,
    pub visible: bool,
    pub dismissed: Option<(u64, usize)>,
}

#[derive(Clone, Default)]
pub(super) struct SyntaxCache {
    pub source: String,
    pub spans: Vec<SyntaxSpan>,
}

#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct EditorTools {
    pub open: bool,
    pub replace: bool,
    pub goto: bool,
    pub query: String,
    pub replacement: String,
    pub case_sensitive: bool,
    pub line_input: String,
    pub message: Option<String>,
    pub matches: Arc<[Range<usize>]>,
    pub current: Option<usize>,
    indexed_revision: Option<u64>,
    indexed_query: String,
    indexed_case: bool,
}

impl EditorTools {
    pub fn current_match(&self, revision: u64) -> Option<&Range<usize>> {
        (self.indexed_revision == Some(revision))
            .then(|| self.current.and_then(|index| self.matches.get(index)))
            .flatten()
    }

    pub fn refresh(&mut self, source: &str, revision: u64) {
        if self.indexed_revision == Some(revision)
            && self.indexed_query == self.query
            && self.indexed_case == self.case_sensitive
        {
            return;
        }
        self.matches = Arc::default();
        self.current = None;
        if !self.query.is_empty()
            && let Ok(regex) = regex::RegexBuilder::new(&regex::escape(&self.query))
                .case_insensitive(!self.case_sensitive)
                .size_limit(1024 * 1024)
                .build()
        {
            self.matches = regex
                .find_iter(source)
                .map(|found| found.range())
                .collect::<Vec<_>>()
                .into();
        }
        self.indexed_revision = Some(revision);
        self.indexed_query.clone_from(&self.query);
        self.indexed_case = self.case_sensitive;
    }

    pub fn navigate(&mut self, forward: bool) -> Option<Range<usize>> {
        let len = self.matches.len();
        if len == 0 {
            self.current = None;
            return None;
        }
        let index = match (self.current, forward) {
            (Some(index), true) => (index + 1) % len,
            (Some(index), false) => (index + len - 1) % len,
            (None, true) => 0,
            (None, false) => len - 1,
        };
        self.current = Some(index);
        self.matches.get(index).cloned()
    }

    pub fn replacement(&self, source: &str, all: bool) -> Option<String> {
        let ranges = if all {
            self.matches.as_ref()
        } else {
            std::slice::from_ref(self.matches.get(self.current?)?)
        };
        if ranges.is_empty() {
            return None;
        }
        let removed = ranges
            .iter()
            .try_fold(0usize, |sum, range| sum.checked_add(range.len()))?;
        let added = self.replacement.len().checked_mul(ranges.len())?;
        let size = source.len().checked_sub(removed)?.checked_add(added)?;
        if size > crate::calculation_pad::MAX_DOCUMENT_BYTES {
            return None;
        }
        let mut result = String::with_capacity(size);
        let mut cursor = 0;
        for range in ranges {
            result.push_str(source.get(cursor..range.start)?);
            result.push_str(&self.replacement);
            cursor = range.end;
        }
        result.push_str(source.get(cursor..)?);
        if result.bytes().filter(|byte| *byte == b'\n').count() + 1
            > crate::calculation_pad::MAX_SOURCE_LINES
        {
            return None;
        }
        Some(result)
    }
}

pub(super) struct SourcePositions<'a> {
    source: &'a str,
}

/// Return the matching parenthesis/bracket at the caret, excluding comments and braced names.
pub(super) fn matching_delimiters(source: &str, caret: usize) -> Option<(usize, usize)> {
    if !source.is_char_boundary(caret) {
        return None;
    }
    let candidate = source
        .get(caret..)?
        .chars()
        .next()
        .filter(|character| matches!(character, '(' | ')' | '[' | ']'))
        .map(|_| caret)
        .or_else(|| {
            source[..caret]
                .char_indices()
                .next_back()
                .filter(|(_, character)| matches!(character, '(' | ')' | '[' | ']'))
                .map(|(byte, _)| byte)
        })?;
    let mut stack = Vec::new();
    let mut pairs = Vec::new();
    let mut braces = false;
    let mut comment = false;
    let mut line_start = true;
    for (byte, character) in source.char_indices() {
        if character == '\n' {
            comment = false;
            braces = false;
            line_start = true;
            continue;
        }
        if comment {
            continue;
        }
        if line_start && character.is_whitespace() {
            continue;
        }
        if line_start && (source[byte..].starts_with('#') || source[byte..].starts_with("//")) {
            comment = true;
            continue;
        }
        line_start = false;
        match character {
            '{' if !braces => braces = true,
            '}' if braces => braces = false,
            '(' | '[' if !braces => stack.push((character, byte)),
            ')' | ']' if !braces => {
                if let Some((open, start)) = stack.pop()
                    && matches!((open, character), ('(', ')') | ('[', ']'))
                {
                    pairs.push((start, byte));
                }
            }
            _ => {}
        }
    }
    pairs
        .into_iter()
        .find(|(left, right)| *left == candidate || *right == candidate)
}

pub(super) fn smart_edit(
    source: &str,
    selection: Range<usize>,
    character: char,
) -> Option<(String, usize)> {
    let selected = source.get(selection.clone())?;
    let closing = match character {
        '(' => ')',
        '[' => ']',
        _ => return None,
    };
    if source.len().checked_add(2)? > crate::calculation_pad::MAX_DOCUMENT_BYTES {
        return None;
    }
    let mut result = String::with_capacity(source.len().checked_add(2)?);
    result.push_str(&source[..selection.start]);
    result.push(character);
    result.push_str(selected);
    result.push(closing);
    result.push_str(&source[selection.end..]);
    Some((
        result,
        selection.start + character.len_utf8() + selected.len(),
    ))
}
impl<'a> SourcePositions<'a> {
    pub fn new(source: &'a str) -> Self {
        Self { source }
    }

    pub fn line_range(&self, one_based: usize) -> Option<Range<usize>> {
        if one_based == 0 {
            return None;
        }
        let mut start = 0;
        for (index, line) in self.source.split('\n').enumerate() {
            let end = start + line.strip_suffix('\r').unwrap_or(line).len();
            if index + 1 == one_based {
                return Some(start..end);
            }
            start += line.len() + 1;
        }
        None
    }

    pub fn line_at(&self, byte: usize) -> Option<usize> {
        Some(
            self.source
                .get(..byte)?
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count(),
        )
    }

    pub fn character(&self, byte: usize) -> Option<usize> {
        Some(self.source.get(..byte)?.chars().count())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn projection_maps_nested_regions_loops_and_unicode_without_changing_source() {
        let source = "# region values\r\nα = 1\r\n# endregion\r\nfor x in [1]:\r\n    雪 = x\r\n雪";
        let regions = fold_regions(source);
        assert_eq!(regions.len(), 2);
        let projection = Projection::new(
            source,
            &regions
                .iter()
                .map(|region| region.hidden.clone())
                .collect::<Vec<_>>(),
        );
        assert_eq!(projection.visible, "# region values\r\nfor x in [1]:\r\n雪");
        assert_eq!(projection.visible_lines, [0, 3, 5]);
        let snow = source.rfind('雪').unwrap();
        let view = projection.visible_byte(snow).unwrap();
        assert_eq!(projection.source_byte(view), Some(snow));
        assert!(
            projection
                .visible_byte(source.find("α =").unwrap() + 2)
                .is_none()
        );
    }

    #[test]
    fn projection_edits_adjacent_visible_pieces_without_deleting_hidden_bytes() {
        let source = "A\nhidden\nB";
        let projection = Projection::new(source, &[2..9]);
        assert_eq!(projection.visible, "A\nB");
        assert_eq!(
            projection.apply_edit(source, "A\n"),
            Some("A\nhidden\n".into())
        );
        assert_eq!(
            projection.apply_edit(source, "AB"),
            Some("Ahidden\nB".into())
        );
        assert_eq!(projection.apply_edit(source, "A\nXB"), None);
    }
    #[test]
    fn smart_pairs_wrap_unicode_and_ignore_comment_delimiters() {
        let (source, caret) = smart_edit("雪 + 2", 0..3, '(').unwrap();
        assert_eq!(source, "(雪) + 2");
        assert_eq!(caret, 4);
        assert_eq!(matching_delimiters(&source, 0), Some((0, 4)));
        assert_eq!(matching_delimiters("# (ignore)\n{a[b]}\n[2]", 2), None);
        assert_eq!(matching_delimiters("# (ignore)\n{a[b]}\n[2]", 13), None);
    }
    #[test]
    fn completion_respects_loop_scope_and_braced_names() {
        let source = "{gross total} = 3\nfor item in range(3):\n  inner = item\noutside = 2\n  it";
        assert!(
            suggestions(source, source.len())
                .iter()
                .all(|item| item.label != "item")
        );
        let inside = "{gross total} = 3\nfor item in range(3):\n  it";
        assert!(
            suggestions(inside, inside.len())
                .iter()
                .any(|item| item.label == "item")
        );
        let braced = "{gross total} = 3\n{gross";
        assert!(
            suggestions(braced, braced.len())
                .iter()
                .any(|item| item.insertion == "{gross total}")
        );
        assert!(suggestions("# squ", 5).is_empty());
    }

    #[test]
    fn completion_replaces_suffix_and_preserves_existing_parentheses() {
        let source = "squareroot(9)";
        let result = suggestions(source, 2);
        let sqrt = result.iter().find(|item| item.label == "sqrt").unwrap();
        assert_eq!(sqrt.replacement, 0..10);
        assert_eq!(sqrt.insertion, "sqrt");
        let source = "for\t{item in stock} in range(2):\n  {item";
        assert!(
            suggestions(source, source.len())
                .iter()
                .any(|item| item.label == "{item in stock}")
        );
        assert_eq!(
            crate::calculation_pad::function_signature_help("EXTRACT"),
            None
        );
        assert_eq!(
            crate::calculation_pad::function_signature_help("FLOOR"),
            Some("floor(value)")
        );
        assert_eq!(
            crate::calculation_pad::function_signature_help("DOT"),
            Some("dot(left, right)")
        );
    }

    #[test]
    fn nested_hints_and_syntax_use_shared_signatures() {
        let source = "round(extract(u8(3), 1, ";
        assert_eq!(
            call_hint(source, source.len()),
            Some(("extract(value, start, count)".into(), 2))
        );
        let source = "round(3) #";
        assert!(syntax_spans(source).iter().any(|span| span.kind == SyntaxKind::Call && &source[span.range.clone()] == "round"));
        assert_eq!(
            crate::calculation_pad::function_signature_help("round"),
            Some("round(value)")
        );
        assert_eq!(
            crate::calculation_pad::function_signature_help("extract"),
            Some("extract(value, start, count)")
        );
    }

    #[test]
    fn literal_matches_replacements_and_unicode_positions() {
        let source = "雪A\r\n雪a\r\n";
        let mut tools = EditorTools {
            query: "雪a".into(),
            replacement: "λ".into(),
            ..Default::default()
        };
        tools.refresh(source, 1);
        assert_eq!(tools.matches.len(), 2);
        assert_eq!(
            tools.replacement(source, true).as_deref(),
            Some("λ\r\nλ\r\n")
        );
        tools.case_sensitive = true;
        tools.refresh(source, 1);
        assert_eq!(tools.matches.len(), 1);
        assert_eq!(tools.navigate(true), Some(6..10));
        let positions = SourcePositions::new(source);
        assert_eq!(positions.line_range(2), Some(6..10));
        assert_eq!(positions.line_at(6), Some(1));
        assert_eq!(positions.character(6), Some(4));
        assert_eq!(positions.line_range(3), Some(12..12));
        assert_eq!(positions.line_range(4), None);
    }
}
