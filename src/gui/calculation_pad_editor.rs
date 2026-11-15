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
    let analysis = crate::calculation_pad::SourceAnalysis::new(source);
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
        } else if trimmed == "# endregion"
            && let Some((header, hidden_start)) = stack.pop()
            && hidden_start < end
        {
            regions.push(FoldRegion {
                header,
                hidden: hidden_start..end,
            });
        }
        let Some(row) = analysis.rows.get(index) else { continue; };
        if row.owner.is_some() { continue; }
        let statement_end = analysis.rows.iter().enumerate().skip(index + 1)
            .take_while(|(_, next)| next.owner == Some(index))
            .map(|(index, _)| index)
            .last().unwrap_or(index);
        if crate::calculation_pad::editor_loop_header(&row.text) {
            let indent = text.len() - text.trim_start().len();
            let mut body_end = lines[statement_end].1;
            let mut has_body = false;
            for (body_index, &(line_start, line_end, line)) in lines.iter().enumerate().skip(statement_end + 1) {
                if analysis.rows.get(body_index).is_some_and(|row| row.owner.is_some()) {
                    body_end = line_end;
                    continue;
                }
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
        } else if statement_end > index && end < lines[statement_end].1 {
            regions.push(FoldRegion { header: index, hidden: end..lines[statement_end].1 });
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
    pub fn project_spans(&self, spans: &[SyntaxSpan]) -> Vec<SyntaxSpan> {
        let mut projected = Vec::new();
        for (view, source) in &self.pieces {
            for span in spans {
                let start = span.range.start.max(source.start);
                let end = span.range.end.min(source.end);
                if start < end {
                    projected.push(SyntaxSpan {
                        range: view.start + start - source.start..view.start + end - source.start,
                        kind: span.kind,
                    });
                }
            }
        }
        projected
    }
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
    let analysis = crate::calculation_pad::SourceAnalysis::new(source);
    let functions = crate::calculation_pad::editor_function_scope(source, source.len());
    let mut protected_spans = analysis.protected.iter().peekable();
    let mut offset = 0;
    for (line_index, line) in source.split_inclusive('\n').enumerate() {
        let mut pos = 0;
        while pos < line.len() {
            let start = pos;
            if protected_spans.peek().is_some_and(|span| span.range.start == offset + pos) {
                let protected = protected_spans.next().unwrap();
                pos = protected.range.end - offset;
                let kind = match protected.kind {
                    crate::calculation_pad::ProtectedKind::Comment => SyntaxKind::Comment,
                    crate::calculation_pad::ProtectedKind::Braced => SyntaxKind::Name,
                    crate::calculation_pad::ProtectedKind::Quoted => SyntaxKind::Text,
                };
                spans.push(SyntaxSpan { range: offset + start..offset + pos, kind });
                continue;
            }
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
                let token_range = offset + start..offset + pos;
                let physical_owner = analysis.rows.get(line_index).map_or(line_index, |row| row.owner.unwrap_or(line_index));
                let owner_line = functions.declarations.iter()
                    .find(|function| function.body_ranges.iter().any(|range| range.contains(&token_range.start)))
                    .map_or(physical_owner, |function| function.line);
                let contextual_control = analysis.rows.get(line_index).is_some_and(|row| {
                    row.owner.is_none()
                        && crate::calculation_pad::editor_control_keyword(&row.text) == Some(token)
                        && token_range.start == row.range.start + row.text.len() - row.text.trim_start().len()
                });
                if contextual_control || matches!(token, "for" | "in") || functions.declarations.iter().any(|function| function.keyword_range == token_range) {
                    SyntaxKind::Keyword
                } else if functions.declarations.iter().any(|function| function.parameter_ranges.contains(&token_range)) {
                    SyntaxKind::Name
                } else if matches!(token, "true" | "false" | "PI" | "E") {
                    SyntaxKind::Constant
                } else if matches!(token, "and" | "or" | "xor" | "shl" | "shr" | "mod")
                    && !line[pos..].trim_start().starts_with('(')
                {
                    SyntaxKind::Operator
                } else if functions.declarations.iter().any(|function| function.name_range == token_range)
                    || line[pos..].trim_start().starts_with('(')
                        && (crate::calculation_pad::function_signature_help(token).is_some()
                            || functions.declarations.iter().any(|function| function.valid && function.line < owner_line && function.name == token))
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
    let analysis = crate::calculation_pad::SourceAnalysis::new(source);
    let protected = analysis.protected.iter().find(|span| {
        span.range.start <= caret && (caret < span.range.end
            || caret == span.range.end && match span.kind {
                crate::calculation_pad::ProtectedKind::Comment => true,
                crate::calculation_pad::ProtectedKind::Braced => !source[span.range.clone()].ends_with('}'),
                crate::calculation_pad::ProtectedKind::Quoted => !source[span.range.clone()].ends_with('"'),
            })
    });
    if protected.is_some_and(|span| span.kind != crate::calculation_pad::ProtectedKind::Braced) {
        return Vec::new();
    }
    let line_start = prefix.rfind('\n').map_or(0, |index| index + 1);
    let line = &prefix[line_start..];
    if line.trim_start().starts_with('#') || line.trim_start().starts_with("//") {
        return Vec::new();
    }
    let braced = protected.filter(|span| span.kind == crate::calculation_pad::ProtectedKind::Braced)
        .map(|span| span.range.start - line_start);
    let start = braced.unwrap_or_else(|| {
            line.char_indices()
                .rev()
                .find_map(|(index, ch)| {
                    (!(ch.is_alphanumeric() || ch == '_')).then_some(index + ch.len_utf8())
                })
                .unwrap_or(0)
    });
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
    let scope = crate::calculation_pad::editor_function_scope(source, caret);
    for name in crate::calculation_pad::editor_bindings(source, caret, &scope) {
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
        for function in scope.declarations.iter().filter(|function| function.valid && function.line < scope.owner_line) {
            if function.name.starts_with(fragment) && function.name != fragment {
                let has_call = source[end..].trim_start().starts_with('(');
                let insertion = if has_call { function.name.clone() } else { format!("{}()", function.name) };
                let cursor = replacement.start + function.name.len() + usize::from(!has_call);
                output.push(Suggestion { label: format!("{}()", function.name), insertion, replacement: replacement.clone(), cursor });
            }
        }
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

#[derive(Debug, PartialEq, Eq)]
pub(super) struct CallHint {
    pub signature: String,
    pub argument: usize,
    pub parameter: Option<String>,
}

pub(super) fn call_hint(source: &str, caret: usize) -> Option<CallHint> {
    let prefix = source.get(..caret)?;
    let analysis = crate::calculation_pad::SourceAnalysis::new(prefix);
    if analysis.protected.iter().any(|span| span.kind == crate::calculation_pad::ProtectedKind::Comment && span.range.end == caret) {
        return None;
    }
    let last = analysis.rows.len().checked_sub(1)?;
    let row = &analysis.rows[analysis.rows[last].owner.unwrap_or(last)];
    let line = row.text.as_str();
    let scope = crate::calculation_pad::editor_function_scope(source, caret);
    if scope.in_declaration_header { return None; }
    let mut stack: Vec<(Option<(String, Option<Vec<String>>)>, usize)> = Vec::new();
    let mut protected_spans = analysis.protected.iter().peekable();
    for (index, ch) in line.char_indices() {
        let protected = row.offsets.get(index).and_then(|offset| *offset).is_some_and(|offset| {
            while protected_spans.peek().is_some_and(|span| span.range.end <= offset) {
                protected_spans.next();
            }
            protected_spans.peek().is_some_and(|span| span.range.contains(&offset))
        });
        if protected {
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
                let user = scope.declarations.iter().find(|function| function.valid && function.line < scope.owner_line && function.name == name);
                let signature = user.map(|function| (format!("{}({})", function.name, function.parameters.join(", ")), Some(function.parameters.clone())))
                    .or_else(|| crate::calculation_pad::function_signature_help(name).map(|signature| (signature.to_owned(), None)));
                stack.push((signature, 0));
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
        .find_map(|(signature, argument)| signature.map(|(signature, parameters)| {
            let parameter = if let Some(parameters) = parameters {
                parameters.get(argument).cloned()
            } else {
                let overload = if argument > 0 { signature.rsplit("  ·  ").next().unwrap_or(&signature) } else { signature.as_str() };
                overload.split_once('(').and_then(|(_, rest)| rest.split_once(')'))
                    .and_then(|(parameters, _)| parameters.split(',').nth(argument))
                    .map(|parameter| parameter.trim().trim_matches(['[', ']']).trim().to_owned())
            };
            CallHint { signature, argument, parameter }
        }))
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
    pub visible: String,
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
    crate::calculation_pad::SourceAnalysis::new(source).pairs
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
    fn multiline_lexing_hints_folds_and_projected_highlighting_use_original_source() {
        let source = "α = max(\r\n  1, // ignore ], (\r\n  2)\r\nβ = 3 # comment (\r\n";
        let open = source.find('(').unwrap();
        let close = source.find("2)").unwrap() + 1;
        assert_eq!(matching_delimiters(source, open), Some((open, close)));
        let ignored = source.find("],").unwrap();
        assert_eq!(matching_delimiters(source, ignored), None);
        let caret = source.find("2)").unwrap() + 1;
        assert_eq!(call_hint(source, caret).map(|hint| hint.argument), Some(1));
        assert!(suggestions(source, source.find("ignore").unwrap() + 3).is_empty());
        let regions = fold_regions(source);
        assert!(regions.iter().any(|region| region.header == 0));
        let projection = Projection::new(source, &[regions.iter().find(|region| region.header == 0).unwrap().hidden.clone()]);
        let spans = projection.project_spans(&syntax_spans(source));
        assert_eq!(spans.iter().map(|span| &projection.visible[span.range.clone()]).collect::<String>(), projection.visible);
        let comment = projection.visible.find("# comment").unwrap();
        assert!(spans.iter().any(|span| span.kind == SyntaxKind::Comment && span.range.contains(&comment)));
        let edited = projection.visible.replace("β = 3", "β = 4");
        let result = projection.apply_edit(source, &edited).unwrap();
        assert_eq!(result, source.replace("β = 3", "β = 4"));
    }

    #[test]
    fn multiline_header_fold_keeps_grouped_header_and_body_together() {
        let source = "for item in range(\r\n  1, 4):\r\n    n = (item +\r\n      1)\r\nafter = 3";
        let region = fold_regions(source).into_iter().find(|region| region.header == 0).unwrap();
        let projection = Projection::new(source, &[region.hidden]);
        assert_eq!(projection.visible, "for item in range(\r\nafter = 3");
    }

    #[test]
    fn conditional_header_fold_and_contextual_keywords_preserve_source() {
        let source = "if (\r\n  score > 80):\r\n    score += 10\r\nelse:\r\n    score = 0\r\nif = 2\r\nbreak += 1";
        let regions = fold_regions(source);
        let first = regions.iter().find(|region| region.header == 0).unwrap();
        let projection = Projection::new(source, &[first.hidden.clone()]);
        assert!(projection.visible.contains("else:\r\n"));
        assert!(!projection.visible.contains("score += 10"));
        let spans = syntax_spans(source);
        let keywords = spans.iter().filter(|span| span.kind == SyntaxKind::Keyword)
            .map(|span| &source[span.range.clone()]).collect::<Vec<_>>();
        assert_eq!(keywords, ["if", "else"]);
        assert_eq!(projection.apply_edit(source, &projection.visible).unwrap(), source);
    }

    #[test]
    fn completion_uses_owning_statement_scope_inside_dedented_continuation() {
        let source = "for item in range(3):\n    total = max(\n  it";
        assert!(suggestions(source, source.len()).iter().any(|item| item.label == "item"));
        let source = "for item in range(3):\n    total = max(\n  1, 2) + to";
        assert!(suggestions(source, source.len()).iter().all(|item| item.label != "total"));
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
    fn compound_operators_keep_source_spans_and_completion_names() {
        let source = "{顧客 名} = 1\r\n{顧客 額} = 3\r\n{顧客 名} += 2\r\n{顧";
        let result = suggestions(source, source.len());
        let names = result.iter().filter(|item| item.insertion.starts_with("{顧客"))
            .map(|item| item.insertion.as_str()).collect::<Vec<_>>();
        assert_eq!(names, ["{顧客 名}", "{顧客 額}"]);
        let operators = syntax_spans(source).into_iter()
            .filter(|span| span.kind == SyntaxKind::Operator)
            .map(|span| &source[span.range])
            .collect::<Vec<_>>();
        assert_eq!(operators, ["=", "=", "+", "="]);
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
            Some(CallHint { signature: "extract(value, start, count)".into(), argument: 2, parameter: Some("count".into()) })
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
    fn user_function_completion_and_hints_follow_local_scope_and_authored_names() {
        let prefix = concat!(
            "outside = 7\r\n",
            "fn earlier({gross, total}, {other)}) = {gross, total} + {other)}\r\n",
            "fn later({local}) = (\r\n  {lo",
        );
        let local_suggestions = suggestions(prefix, prefix.len());
        assert!(local_suggestions.iter().any(|item| item.insertion == "{local}"));
        assert!(local_suggestions.iter().all(|item| item.insertion != "outside" && item.insertion != "{gross, total}"));
        let source = concat!(
            "fn earlier({gross, total}, {other)}) = {gross, total} + {other)}\r\n",
            "value = earlier(1, ",
        );
        let hint = call_hint(source, source.len()).unwrap();
        assert_eq!(hint.signature, "earlier({gross, total}, {other)})");
        assert_eq!(hint.argument, 1);
        assert_eq!(hint.parameter.as_deref(), Some("{other)}"));
        let incomplete = "fn bad(value) = unknown\nvalue = ba";
        assert!(suggestions(incomplete, incomplete.len()).iter().all(|item| item.label != "bad()"));
        let prior = "fn earlier(value) = value\nfn later(local) = (\n  ear";
        assert!(suggestions(prior, prior.len()).iter().any(|item| item.label == "earlier()"));
        let own = "fn earlier(value) = value\nfn later(local) = (\n  lat";
        assert!(suggestions(own, own.len()).iter().all(|item| item.label != "later()"));
        let forward = "answer = lat\nfn later(value) = value";
        assert!(suggestions(forward, "answer = lat".len()).iter().all(|item| item.label != "later()"));
        let same_name = "fn price(value) = value\nprice = 2\npri";
        let names = suggestions(same_name, same_name.len());
        assert!(names.iter().any(|item| item.label == "price" && item.insertion == "price"));
        assert!(names.iter().any(|item| item.label == "price()" && item.insertion == "price()"));
        let header = "fn sqrt(value";
        assert!(call_hint(header, header.len()).is_none());
    }

    #[test]
    fn user_function_source_ranges_highlight_and_fold_original_crlf() {
        let source = "fn scaled({rate, gross}, x) = (\r\n  {rate, gross} * x)\r\nscaled(2, 3)";
        let scope = crate::calculation_pad::editor_function_scope(source, source.len());
        let function = scope.declarations.iter().find(|function| function.name == "scaled").unwrap();
        assert!(function.valid);
        assert_eq!(&source[function.keyword_range.clone()], "fn");
        assert_eq!(&source[function.name_range.clone()], "scaled");
        assert_eq!(&source[function.parameter_ranges[0].clone()], "{rate, gross}");
        assert!(function.body_ranges.iter().any(|range| source[range.clone()].contains("{rate, gross}")));
        let spans = syntax_spans(source);
        assert!(spans.iter().any(|span| span.kind == SyntaxKind::Keyword && &source[span.range.clone()] == "fn"));
        assert_eq!(spans.iter().filter(|span| span.kind == SyntaxKind::Call && &source[span.range.clone()] == "scaled").count(), 2);
        let region = fold_regions(source).into_iter().find(|region| region.header == 0).unwrap();
        let projection = Projection::new(source, &[region.hidden]);
        let projected = projection.project_spans(&spans);
        assert_eq!(projected.iter().map(|span| &projection.visible[span.range.clone()]).collect::<String>(), projection.visible);
        assert_eq!(projection.visible, "fn scaled({rate, gross}, x) = (\r\nscaled(2, 3)");
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
