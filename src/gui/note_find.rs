//! Transient, per-panel literal Find state for unsaved Markdown content.

use regex::{Regex, RegexBuilder};
use std::ops::Range;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NoteFindMatch {
    pub(crate) byte_range: Range<usize>,
    pub(crate) char_range: Range<usize>,
}

#[derive(Default)]
pub(crate) struct NoteFindState {
    open: bool,
    query: String,
    compiled_query: Option<Regex>,
    query_revision: u64,
    content_revision: u64,
    indexed_revisions: Option<(u64, u64)>,
    matches: Vec<NoteFindMatch>,
    current_index: Option<usize>,
    #[cfg(test)]
    compile_count: usize,
    #[cfg(test)]
    recompute_count: usize,
}

impl NoteFindState {
    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn open(&mut self) {
        self.open = true;
    }

    pub(crate) fn close(&mut self) {
        self.open = false;
    }

    pub(crate) fn query(&self) -> &str {
        &self.query
    }

    /// Returns whether the query changed. Literal regex compilation happens
    /// only at this boundary, never while the panel is merely rendering.
    pub(crate) fn set_query(&mut self, query: impl Into<String>) -> bool {
        let query = query.into();
        if self.query == query {
            return false;
        }

        self.query = query;
        self.query_revision = self.query_revision.wrapping_add(1);
        self.compiled_query = if self.query.is_empty() {
            None
        } else {
            #[cfg(test)]
            {
                self.compile_count = self.compile_count.saturating_add(1);
            }
            RegexBuilder::new(&regex::escape(&self.query))
                .case_insensitive(true)
                .build()
                .ok()
        };
        self.invalidate_index();
        true
    }

    /// Call after any mutation of this panel's source, including external
    /// replacement with text that happens to have the same length.
    pub(crate) fn content_changed(&mut self) {
        self.content_revision = self.content_revision.wrapping_add(1);
        self.invalidate_index();
    }

    pub(crate) fn reset_for_new_note(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn results(&mut self, content: &str) -> &[NoteFindMatch] {
        self.ensure_indexed(content);
        &self.matches
    }

    pub(crate) fn current_index(&mut self, content: &str) -> Option<usize> {
        self.ensure_indexed(content);
        self.current_index
    }

    pub(crate) fn current_match(&mut self, content: &str) -> Option<&NoteFindMatch> {
        self.ensure_indexed(content);
        self.current_index.and_then(|index| self.matches.get(index))
    }

    pub(crate) fn next(&mut self, content: &str) -> Option<&NoteFindMatch> {
        self.ensure_indexed(content);
        let count = self.matches.len();
        if count == 0 {
            self.current_index = None;
            return None;
        }
        let next = self.current_index.map_or(0, |index| (index + 1) % count);
        self.current_index = Some(next);
        self.matches.get(next)
    }

    pub(crate) fn previous(&mut self, content: &str) -> Option<&NoteFindMatch> {
        self.ensure_indexed(content);
        let count = self.matches.len();
        if count == 0 {
            self.current_index = None;
            return None;
        }
        let previous = self
            .current_index
            .map_or(count - 1, |index| (index + count - 1) % count);
        self.current_index = Some(previous);
        self.matches.get(previous)
    }

    #[cfg(test)]
    fn compile_count(&self) -> usize {
        self.compile_count
    }

    #[cfg(test)]
    fn recompute_count(&self) -> usize {
        self.recompute_count
    }

    fn invalidate_index(&mut self) {
        self.indexed_revisions = None;
        self.matches.clear();
        self.current_index = None;
    }

    fn ensure_indexed(&mut self, content: &str) {
        let revisions = (self.query_revision, self.content_revision);
        if self.indexed_revisions == Some(revisions) {
            return;
        }

        self.matches.clear();
        if let Some(regex) = &self.compiled_query {
            let mut chars = content.char_indices().peekable();
            let mut char_index = 0;
            // Regex::find_iter is leftmost-first and non-overlapping, which is
            // the conventional behavior for literal Find results.
            for found in regex.find_iter(content) {
                while chars
                    .peek()
                    .is_some_and(|(byte_index, _)| *byte_index < found.start())
                {
                    chars.next();
                    char_index += 1;
                }
                let char_start = char_index;
                while chars
                    .peek()
                    .is_some_and(|(byte_index, _)| *byte_index < found.end())
                {
                    chars.next();
                    char_index += 1;
                }
                self.matches.push(NoteFindMatch {
                    byte_range: found.start()..found.end(),
                    char_range: char_start..char_index,
                });
            }
        }

        self.current_index = (!self.matches.is_empty()).then_some(0);
        self.indexed_revisions = Some(revisions);
        #[cfg(test)]
        {
            self.recompute_count = self.recompute_count.saturating_add(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_find_punctuation_is_literal_and_ranges_are_non_overlapping() {
        let mut find = NoteFindState::default();
        find.set_query("[a+b]?");

        let content = "x [a+b]? [a+b]?";
        let results = find.results(content);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].byte_range, 2..8);
        assert_eq!(results[1].byte_range, 9..15);

        find.set_query("ana");
        let results = find.results("banana");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].byte_range, 1..4);
    }

    #[test]
    fn note_find_unicode_case_insensitivity_keeps_original_utf8_and_crlf_ranges() {
        let mut find = NoteFindState::default();
        find.set_query("K");

        let content = "K\r\nk";
        let results = find.results(content);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].byte_range, 0..3);
        assert_eq!(results[0].char_range, 0..1);
        assert_eq!(results[1].byte_range, 5..6);
        assert_eq!(results[1].char_range, 3..4);
    }

    #[test]
    fn note_find_empty_and_missing_queries_have_no_ranges() {
        let mut find = NoteFindState::default();
        assert!(find.results("anything").is_empty());
        find.set_query("missing");
        assert!(find.results("anything").is_empty());
        assert_eq!(find.current_index("anything"), None);
        assert!(find.next("anything").is_none());
        assert!(find.previous("anything").is_none());
    }

    #[test]
    fn note_find_navigation_wraps_and_content_changes_reset_to_first() {
        let mut find = NoteFindState::default();
        find.set_query("x");
        let content = "x--x--x";
        assert_eq!(find.current_index(content), Some(0));
        assert_eq!(find.next(content).unwrap().byte_range, 3..4);
        assert_eq!(find.next(content).unwrap().byte_range, 6..7);
        assert_eq!(find.next(content).unwrap().byte_range, 0..1);
        assert_eq!(find.previous(content).unwrap().byte_range, 6..7);

        find.content_changed();
        assert_eq!(find.current_index("new x"), Some(0));
        assert_eq!(find.current_match("new x").unwrap().byte_range, 4..5);
    }

    #[test]
    fn note_find_unchanged_revisions_reuse_compiled_query_and_ranges() {
        let mut find = NoteFindState::default();
        assert!(find.set_query("needle"));
        assert!(!find.set_query("needle"));
        assert_eq!(find.results("needle needle").len(), 2);
        assert_eq!(find.results("needle needle").len(), 2);
        assert_eq!(find.compile_count(), 1);
        assert_eq!(find.recompute_count(), 1);

        find.content_changed();
        assert_eq!(find.results("needle needle").len(), 2);
        assert_eq!(find.compile_count(), 1);
        assert_eq!(find.recompute_count(), 2);
    }
}
