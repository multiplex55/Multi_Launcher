//! Position mapping and literal search over the Pad's authoritative source.

use std::ops::Range;
use std::sync::Arc;

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
