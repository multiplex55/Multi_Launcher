//! Position mapping and literal search over the Pad's authoritative source.

use std::ops::Range;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SyntaxKind { Text, Number, Name, Call, Keyword, Constant, Operator, Comment }

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
            spans.push(SyntaxSpan { range: offset..offset + line.len(), kind: SyntaxKind::Comment });
            offset += line.len();
            continue;
        }
        let mut pos = 0;
        while pos < line.len() {
            let start = pos;
            let ch = line[pos..].chars().next().unwrap();
            pos += ch.len_utf8();
            let kind = if ch == '{' {
                if let Some(end) = line[pos..].find('}') { pos += end + 1; }
                SyntaxKind::Name
            } else if ch.is_ascii_digit() || ch == '.' && line[pos..].starts_with(|c: char| c.is_ascii_digit()) {
                while let Some(next) = line[pos..].chars().next() {
                    if !(next.is_ascii_alphanumeric() || matches!(next, '.' | '_')) { break; }
                    pos += next.len_utf8();
                }
                SyntaxKind::Number
            } else if ch.is_alphabetic() || ch == '_' {
                while let Some(next) = line[pos..].chars().next() {
                    if !(next.is_alphanumeric() || next == '_') { break; }
                    pos += next.len_utf8();
                }
                let token = &line[start..pos];
                if matches!(token, "for" | "in") { SyntaxKind::Keyword }
                else if matches!(token, "true" | "false" | "PI" | "E") { SyntaxKind::Constant }
                else if matches!(token, "and" | "or" | "xor" | "shl" | "shr" | "mod")
                    && !line[pos..].trim_start().starts_with('(') { SyntaxKind::Operator }
                else if line[pos..].trim_start().starts_with('(')
                    && crate::calculation_pad::function_signature_help(token).is_some() { SyntaxKind::Call }
                else { SyntaxKind::Name }
            } else if "+-*/%^=<>!&|~".contains(ch) {
                SyntaxKind::Operator
            } else { SyntaxKind::Text };
            spans.push(SyntaxSpan { range: offset + start..offset + pos, kind });
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
    let Some(prefix) = source.get(..caret) else { return Vec::new(); };
    let line_start = prefix.rfind('\n').map_or(0, |index| index + 1);
    let line = &prefix[line_start..];
    if line.trim_start().starts_with('#') || line.trim_start().starts_with("//") { return Vec::new(); }
    let braced = line.rfind('{').filter(|start| !line[*start..].contains('}'));
    let start = braced.map_or_else(|| {
        line.char_indices().rev().find_map(|(index, ch)|
            (!(ch.is_alphanumeric() || ch == '_')).then_some(index + ch.len_utf8())
        ).unwrap_or(0)
    }, |index| index);
    let fragment = &line[start..];
    if fragment.is_empty() { return Vec::new(); }
    let end = if braced.is_some() {
        source[caret..].find('}').filter(|end| !source[caret..caret + end].contains('\n')).map_or(caret, |end| caret + end + 1)
    } else {
        caret + source[caret..].chars().take_while(|ch| ch.is_alphanumeric() || *ch == '_').map(char::len_utf8).sum::<usize>()
    };
    let replacement = line_start + start..end;
    let mut output = Vec::new();
    for name in crate::calculation_pad::editor_bindings(source, caret) {
        if name.starts_with(fragment) && name != fragment {
            let cursor = replacement.start + name.len();
            output.push(Suggestion { label: name.clone(), insertion: name, replacement: replacement.clone(), cursor });
        }
    }
    if braced.is_none() {
        for function in crate::calculation_pad::FUNCTIONS {
            let name = function.name;
            if name.starts_with(fragment) && name != fragment {
                let suffix = source[caret..].chars().take_while(|ch| ch.is_alphanumeric() || *ch == '_').map(char::len_utf8).sum::<usize>();
                let end = caret + suffix;
                let has_call = source[end..].trim_start().starts_with('(');
                let insertion = if has_call { name.to_owned() } else { format!("{name}()") };
                let cursor = replacement.start + name.len() + usize::from(!has_call);
                output.push(Suggestion { label: name.to_owned(), insertion, replacement: replacement.start..end, cursor });
            }
        }
    }
    output.truncate(8);
    output
}

pub(super) fn call_hint(source: &str, caret: usize) -> Option<(String, usize)> {
    let prefix = source.get(..caret)?;
    let line = prefix.rsplit('\n').next()?;
    if line.trim_start().starts_with('#') || line.trim_start().starts_with("//") { return None; }
    let mut stack: Vec<(Option<String>, usize)> = Vec::new();
    let mut braces = false;
    for (index, ch) in line.char_indices() {
        if braces { if ch == '}' { braces = false; } continue; }
        if ch == '{' { braces = true; continue; }
        match ch {
            '(' => {
                let before = line[..index].trim_end();
                let start = before.char_indices().rev().find_map(|(pos, c)|
                    (!(c.is_alphanumeric() || c == '_')).then_some(pos + c.len_utf8())
                ).unwrap_or(0);
                let name = &before[start..];
                stack.push((crate::calculation_pad::function_signature_help(name).map(str::to_owned), 0));
            }
            '[' => stack.push((None, 0)),
            ')' | ']' => { stack.pop(); }
            ',' => if let Some((_, index)) = stack.last_mut() { *index += 1; },
            _ => {}
        }
    }
    stack.into_iter().rev().find_map(|(signature, index)| signature.map(|signature| (signature, index)))
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
    fn completion_respects_loop_scope_and_braced_names() {
        let source = "{gross total} = 3\nfor item in range(3):\n  inner = item\noutside = 2\n  it";
        assert!(suggestions(source, source.len()).iter().all(|item| item.label != "item"));
        let inside = "{gross total} = 3\nfor item in range(3):\n  it";
        assert!(suggestions(inside, inside.len()).iter().any(|item| item.label == "item"));
        let braced = "{gross total} = 3\n{gross";
        assert!(suggestions(braced, braced.len()).iter().any(|item| item.insertion == "{gross total}"));
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
        assert!(suggestions(source, source.len()).iter().any(|item| item.label == "{item in stock}"));
        assert_eq!(crate::calculation_pad::function_signature_help("EXTRACT"), None);
        assert_eq!(crate::calculation_pad::function_signature_help("FLOOR"), Some("floor(value)"));
        assert_eq!(crate::calculation_pad::function_signature_help("DOT"), Some("dot(left, right)"));
    }

    #[test]
    fn nested_hints_and_syntax_use_shared_signatures() {
        let source = "round(extract(u8(3), 1, ";
        assert_eq!(call_hint(source, source.len()), Some(("extract(value, start, count)".into(), 2)));
        let source = "round(3) #";
        assert!(syntax_spans(source).iter().any(|span| span.kind == SyntaxKind::Call && &source[span.range.clone()] == "round"));
        assert_eq!(crate::calculation_pad::function_signature_help("round"), Some("round(value)"));
        assert_eq!(crate::calculation_pad::function_signature_help("extract"), Some("extract(value, start, count)"));
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
