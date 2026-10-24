//! Transient, per-panel literal Find state for unsaved Markdown content.

use regex::{Regex, RegexBuilder};
use std::ops::Range;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NoteFindMatch {
    pub(crate) byte_range: Range<usize>,
    pub(crate) char_range: Range<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SelectedMatchIdentity {
    index: usize,
    matched: NoteFindMatch,
}

/// A literal replacement computed against one exact Find owner/query/content
/// revision and, for Replace Current, selected match.
///
/// This is intentionally an in-memory transaction. The owning NotePanel still
/// installs the returned source and owns dirty state and persistence.
#[derive(Clone, Debug)]
pub(crate) struct PreparedNoteReplacement {
    owner_token: Arc<()>,
    note_generation: u64,
    query_revision: u64,
    content_revision: u64,
    navigation_revision: u64,
    selected_match: Option<SelectedMatchIdentity>,
    content: String,
    changed: bool,
    caret_char: usize,
    next_search_from_byte: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StaleNoteReplacement {
    DifferentOwner,
    NoteGeneration,
    QueryRevision,
    ContentRevision,
    SelectedMatch,
    AlreadyCommitted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NoteReplacementOutcome {
    pub(crate) content: String,
    pub(crate) changed: bool,
    /// Character offset immediately after the replacement text.
    pub(crate) caret_char: usize,
    /// The next match after the inserted text, if present.
    pub(crate) next_match: Option<NoteFindMatch>,
    owner_token: Arc<()>,
    note_generation: u64,
    query_revision: u64,
    content_revision: u64,
    navigation_revision: u64,
    selected_match: Option<SelectedMatchIdentity>,
    next_search_from_byte: Option<usize>,
    committed: bool,
}

pub(crate) struct NoteFindState {
    owner_token: Arc<()>,
    note_generation: u64,
    open: bool,
    query: String,
    compiled_query: Option<Regex>,
    query_revision: u64,
    content_revision: u64,
    indexed_revisions: Option<(u64, u64)>,
    matches: Vec<NoteFindMatch>,
    current_index: Option<usize>,
    navigation_revision: u64,
    navigation_anchor: Option<usize>,
    anchored_selection: Option<NoteFindMatch>,
    #[cfg(test)]
    compile_count: usize,
    #[cfg(test)]
    recompute_count: usize,
}

impl Default for NoteFindState {
    fn default() -> Self {
        Self {
            owner_token: Arc::new(()),
            note_generation: 0,
            open: false,
            query: String::new(),
            compiled_query: None,
            query_revision: 0,
            content_revision: 0,
            indexed_revisions: None,
            matches: Vec::new(),
            current_index: None,
            navigation_revision: 0,
            navigation_anchor: None,
            anchored_selection: None,
            #[cfg(test)]
            compile_count: 0,
            #[cfg(test)]
            recompute_count: 0,
        }
    }
}

impl NoteFindState {
    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn content_revision(&self) -> u64 {
        self.content_revision
    }

    pub(crate) fn query_revision(&self) -> u64 {
        self.query_revision
    }

    pub(crate) fn navigation_revision(&self) -> u64 {
        self.navigation_revision
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
        self.note_generation = self.note_generation.wrapping_add(1);
        self.query.clear();
        self.compiled_query = None;
        self.query_revision = 0;
        self.content_revision = 0;
        self.indexed_revisions = None;
        self.matches.clear();
        self.current_index = None;
        self.navigation_revision = self.navigation_revision.wrapping_add(1);
        self.navigation_anchor = None;
        self.anchored_selection = None;
        self.open = false;
        #[cfg(test)]
        {
            self.compile_count = 0;
            self.recompute_count = 0;
        }
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
        self.restore_global_index_for_navigation(content, SearchDirection::Next);
        self.ensure_indexed(content);
        self.navigation_revision = self.navigation_revision.wrapping_add(1);
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
        self.restore_global_index_for_navigation(content, SearchDirection::Previous);
        self.ensure_indexed(content);
        self.navigation_revision = self.navigation_revision.wrapping_add(1);
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

    pub(crate) fn prepare_replace_current(
        &mut self,
        content: &str,
        replacement: &str,
    ) -> Option<PreparedNoteReplacement> {
        self.ensure_indexed(content);
        let index = self.current_index?;
        let matched = self.matches.get(index)?.clone();
        let (new_content, caret_char) =
            replace_ranges_once(content, std::slice::from_ref(&matched), replacement);
        let next_search_from_byte =
            Some(matched.byte_range.start.saturating_add(replacement.len()));
        Some(PreparedNoteReplacement {
            owner_token: Arc::clone(&self.owner_token),
            note_generation: self.note_generation,
            query_revision: self.query_revision,
            content_revision: self.content_revision,
            navigation_revision: self.navigation_revision,
            selected_match: Some(SelectedMatchIdentity { index, matched }),
            changed: new_content != content,
            content: new_content,
            caret_char,
            next_search_from_byte,
        })
    }

    pub(crate) fn prepare_replace_all(
        &mut self,
        content: &str,
        replacement: &str,
    ) -> Option<PreparedNoteReplacement> {
        let matches = self
            .compiled_query
            .as_ref()
            .map(|regex| find_matches_from(regex, content, 0))
            .unwrap_or_default();
        if matches.is_empty() {
            return None;
        }
        let (new_content, caret_char) = replace_ranges_once(content, &matches, replacement);
        Some(PreparedNoteReplacement {
            owner_token: Arc::clone(&self.owner_token),
            note_generation: self.note_generation,
            query_revision: self.query_revision,
            content_revision: self.content_revision,
            navigation_revision: self.navigation_revision,
            selected_match: None,
            changed: new_content != content,
            content: new_content,
            caret_char,
            next_search_from_byte: None,
        })
    }

    pub(crate) fn apply_prepared_replacement(
        &mut self,
        current_content: &str,
        prepared: PreparedNoteReplacement,
    ) -> Result<NoteReplacementOutcome, StaleNoteReplacement> {
        if !Arc::ptr_eq(&self.owner_token, &prepared.owner_token) {
            return Err(StaleNoteReplacement::DifferentOwner);
        }
        if self.note_generation != prepared.note_generation {
            return Err(StaleNoteReplacement::NoteGeneration);
        }
        if self.query_revision != prepared.query_revision {
            return Err(StaleNoteReplacement::QueryRevision);
        }
        if self.content_revision != prepared.content_revision {
            return Err(StaleNoteReplacement::ContentRevision);
        }
        if prepared.selected_match.is_some()
            && self.navigation_revision != prepared.navigation_revision
        {
            return Err(StaleNoteReplacement::SelectedMatch);
        }

        self.ensure_indexed(current_content);
        if let Some(selected) = &prepared.selected_match
            && (self.current_index != Some(selected.index)
                || self.matches.get(selected.index) != Some(&selected.matched))
        {
            return Err(StaleNoteReplacement::SelectedMatch);
        }

        Ok(NoteReplacementOutcome {
            content: prepared.content,
            changed: prepared.changed,
            caret_char: prepared.caret_char,
            next_match: None,
            owner_token: prepared.owner_token,
            note_generation: prepared.note_generation,
            query_revision: prepared.query_revision,
            content_revision: prepared.content_revision,
            navigation_revision: prepared.navigation_revision,
            selected_match: prepared.selected_match,
            next_search_from_byte: prepared.next_search_from_byte,
            committed: false,
        })
    }

    /// Complete the pure replacement after the NotePanel has installed
    /// `outcome.content` through its normal mutation path. A changed source
    /// must have crossed `content_changed()` exactly once before this call.
    pub(crate) fn commit_replacement_navigation(
        &mut self,
        current_content: &str,
        outcome: &mut NoteReplacementOutcome,
    ) -> Result<(), StaleNoteReplacement> {
        if outcome.committed {
            return Err(StaleNoteReplacement::AlreadyCommitted);
        }
        if !Arc::ptr_eq(&self.owner_token, &outcome.owner_token) {
            return Err(StaleNoteReplacement::DifferentOwner);
        }
        if self.note_generation != outcome.note_generation {
            return Err(StaleNoteReplacement::NoteGeneration);
        }
        if self.query_revision != outcome.query_revision {
            return Err(StaleNoteReplacement::QueryRevision);
        }
        let expected_content_revision = if outcome.changed {
            outcome.content_revision.wrapping_add(1)
        } else {
            outcome.content_revision
        };
        if self.content_revision != expected_content_revision || current_content != outcome.content
        {
            return Err(StaleNoteReplacement::ContentRevision);
        }
        if outcome.selected_match.is_some()
            && self.navigation_revision != outcome.navigation_revision
        {
            return Err(StaleNoteReplacement::SelectedMatch);
        }

        if let Some(search_from_byte) = outcome.next_search_from_byte {
            self.reindex_from_byte(current_content, search_from_byte);
            self.anchored_selection = self
                .current_index
                .and_then(|index| self.matches.get(index))
                .cloned();
        } else if outcome.changed {
            self.ensure_global_indexed(current_content);
            // Replace All consumes the original result set. Do not immediately
            // select a match that exists only in newly inserted replacement text.
            self.current_index = None;
        }

        outcome.next_match = self
            .current_index
            .and_then(|index| self.matches.get(index))
            .cloned();
        outcome.committed = true;
        if outcome.selected_match.is_some() || outcome.changed {
            self.navigation_revision = self.navigation_revision.wrapping_add(1);
        }
        Ok(())
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
        self.navigation_anchor = None;
        self.anchored_selection = None;
    }

    fn ensure_global_indexed(&mut self, content: &str) {
        if self.navigation_anchor.is_none() {
            self.ensure_indexed(content);
            return;
        }

        let anchored_selection = self.anchored_selection.clone();
        self.navigation_anchor = None;
        self.anchored_selection = None;
        self.indexed_revisions = None;
        self.ensure_indexed(content);
        self.current_index = anchored_selection
            .as_ref()
            .and_then(|selected| self.matches.iter().position(|matched| matched == selected))
            .or_else(|| {
                anchored_selection.as_ref().and_then(|selected| {
                    self.matches
                        .iter()
                        .position(|matched| matched.byte_range.start >= selected.byte_range.start)
                })
            })
            .or_else(|| (!self.matches.is_empty()).then_some(0));
    }

    fn restore_global_index_for_navigation(&mut self, content: &str, direction: SearchDirection) {
        if self.navigation_anchor.is_none() {
            return;
        }
        let selected = self.anchored_selection.clone();
        self.navigation_anchor = None;
        self.anchored_selection = None;
        self.indexed_revisions = None;
        self.ensure_indexed(content);

        let count = self.matches.len();
        if count == 0 {
            self.current_index = None;
            return;
        }
        self.current_index = match direction {
            SearchDirection::Next => selected
                .and_then(|selected| {
                    self.matches
                        .iter()
                        .position(|matched| matched.byte_range.start >= selected.byte_range.end)
                })
                .map(|next| (next + count - 1) % count)
                .or(Some(count - 1)),
            SearchDirection::Previous => selected
                .and_then(|selected| {
                    self.matches
                        .iter()
                        .rposition(|matched| matched.byte_range.end <= selected.byte_range.start)
                })
                .map(|previous| (previous + 1) % count)
                .or(Some(0)),
        };
    }

    fn reindex_from_byte(&mut self, content: &str, search_from_byte: usize) {
        self.ensure_global_indexed(content);
        let anchor = search_from_byte.min(content.len());
        let prefix = self
            .matches
            .iter()
            .filter(|matched| matched.byte_range.end <= anchor)
            .cloned()
            .collect::<Vec<_>>();
        let suffix = self
            .compiled_query
            .as_ref()
            .map(|regex| find_matches_from(regex, content, anchor))
            .unwrap_or_default();

        self.matches = suffix;
        let has_forward_match = !self.matches.is_empty();
        self.matches.extend(prefix);
        // Prefix ranges remain available for the result count, but automatic
        // post-replacement selection must never jump back into inserted text.
        self.current_index = has_forward_match.then_some(0);
        self.indexed_revisions = Some((self.query_revision, self.content_revision));
        self.navigation_anchor = Some(anchor);
        #[cfg(test)]
        {
            self.recompute_count = self.recompute_count.saturating_add(1);
        }
    }

    fn ensure_indexed(&mut self, content: &str) {
        let revisions = (self.query_revision, self.content_revision);
        if self.indexed_revisions == Some(revisions) {
            return;
        }

        self.matches = self
            .compiled_query
            .as_ref()
            .map(|regex| find_matches_from(regex, content, 0))
            .unwrap_or_default();

        self.current_index = (!self.matches.is_empty()).then_some(0);
        self.indexed_revisions = Some(revisions);
        #[cfg(test)]
        {
            self.recompute_count = self.recompute_count.saturating_add(1);
        }
    }
}

#[derive(Clone, Copy)]
enum SearchDirection {
    Next,
    Previous,
}

fn find_matches_from(regex: &Regex, content: &str, start_byte: usize) -> Vec<NoteFindMatch> {
    let mut matches = Vec::new();
    let mut chars = content.char_indices().peekable();
    let mut char_index = 0;
    while chars
        .peek()
        .is_some_and(|(byte_index, _)| *byte_index < start_byte)
    {
        chars.next();
        char_index += 1;
    }

    let mut cursor = start_byte;
    while cursor <= content.len() {
        let Some(found) = regex.find_at(content, cursor) else {
            break;
        };
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
        matches.push(NoteFindMatch {
            byte_range: found.start()..found.end(),
            char_range: char_start..char_index,
        });
        // Find queries are nonempty literals, so each result advances the
        // cursor and preserves non-overlapping ranges.
        cursor = found.end();
    }

    matches
}

fn replace_ranges_once(
    content: &str,
    matches: &[NoteFindMatch],
    replacement: &str,
) -> (String, usize) {
    let replacement_char_count = replacement.chars().count();
    let capacity = content
        .len()
        .saturating_add(replacement.len().saturating_mul(matches.len()));
    let mut result = String::with_capacity(capacity);
    let mut source_byte_cursor = 0;
    let mut result_char_count = 0;
    let mut caret_char = 0;

    for matched in matches {
        let original_prefix = &content[source_byte_cursor..matched.byte_range.start];
        result.push_str(original_prefix);
        result_char_count += original_prefix.chars().count();

        result.push_str(replacement);
        result_char_count += replacement_char_count;
        caret_char = result_char_count;
        source_byte_cursor = matched.byte_range.end;
    }
    result.push_str(&content[source_byte_cursor..]);

    (result, caret_char)
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

    #[test]
    fn note_replace_current_uses_literal_text_and_skips_matches_inside_inserted_text() {
        let mut find = NoteFindState::default();
        find.set_query("needle");
        let original = "needle\r\nneedle";
        let replacement = "$1\\needle\r\n猫!";

        let prepared = find.prepare_replace_current(original, replacement).unwrap();
        let mut outcome = find.apply_prepared_replacement(original, prepared).unwrap();

        assert!(outcome.changed);
        assert_eq!(outcome.content, "$1\\needle\r\n猫!\r\nneedle");
        assert_eq!(outcome.caret_char, replacement.chars().count());
        assert_eq!(find.content_revision, 0);
        find.content_changed();
        let installed_content = outcome.content.clone();
        find.commit_replacement_navigation(&installed_content, &mut outcome)
            .unwrap();
        assert_eq!(find.current_index(&outcome.content), Some(0));
        assert_eq!(
            outcome.next_match,
            Some(NoteFindMatch {
                byte_range: 17..23,
                char_range: 15..21,
            })
        );
        assert_eq!(find.content_revision, 1);

        // The cached anchored result is actionable by another Replace Current.
        let second = find
            .prepare_replace_current(&outcome.content, "tail")
            .unwrap();
        let second = find
            .apply_prepared_replacement(&outcome.content, second)
            .unwrap();
        assert_eq!(second.content, "$1\\needle\r\n猫!\r\ntail");
    }

    #[test]
    fn note_replace_all_is_one_pass_and_does_not_select_inserted_matches() {
        let mut find = NoteFindState::default();
        find.set_query("ab");
        let original = "ab ab";
        let replacement = "ab$\\\n";

        let prepared = find.prepare_replace_all(original, replacement).unwrap();
        let mut outcome = find.apply_prepared_replacement(original, prepared).unwrap();

        assert!(outcome.changed);
        assert_eq!(outcome.content, "ab$\\\n ab$\\\n");
        assert_eq!(outcome.caret_char, outcome.content.chars().count());
        find.content_changed();
        let installed_content = outcome.content.clone();
        find.commit_replacement_navigation(&installed_content, &mut outcome)
            .unwrap();
        assert!(outcome.next_match.is_none());
        assert_eq!(find.current_index(&outcome.content), None);
        // The inserted text remains searchable, but it was not recursively
        // replaced during this transaction or selected as its next result.
        assert_eq!(find.results(&outcome.content).len(), 2);
    }

    #[test]
    fn note_replace_all_preserves_unicode_source_ranges_and_crlf_with_empty_text() {
        let mut find = NoteFindState::default();
        find.set_query("K");
        let original = "K\r\nk";
        assert_eq!(find.results(original)[0].byte_range, 0..3);
        assert_eq!(find.results(original)[1].byte_range, 5..6);

        let prepared = find.prepare_replace_all(original, "").unwrap();
        let mut outcome = find.apply_prepared_replacement(original, prepared).unwrap();

        assert!(outcome.changed);
        assert_eq!(outcome.content, "\r\n");
        assert_eq!(outcome.caret_char, 2);
        find.content_changed();
        let installed_content = outcome.content.clone();
        find.commit_replacement_navigation(&installed_content, &mut outcome)
            .unwrap();
        assert!(outcome.next_match.is_none());
        assert!(find.results(&outcome.content).is_empty());
    }

    #[test]
    fn note_replace_empty_missing_and_identical_replacements_are_noops() {
        let mut find = NoteFindState::default();
        assert!(find.prepare_replace_current("anything", "x").is_none());
        assert!(find.prepare_replace_all("anything", "x").is_none());

        find.set_query("missing");
        assert!(find.prepare_replace_current("anything", "x").is_none());
        assert!(find.prepare_replace_all("anything", "x").is_none());

        find.set_query("needle");
        let original = "Needle needle";
        let prepared = find.prepare_replace_current(original, "Needle").unwrap();
        let mut outcome = find.apply_prepared_replacement(original, prepared).unwrap();

        assert!(!outcome.changed);
        assert_eq!(outcome.content, original);
        assert_eq!(outcome.caret_char, "Needle".chars().count());
        let installed_content = outcome.content.clone();
        find.commit_replacement_navigation(&installed_content, &mut outcome)
            .unwrap();
        assert_eq!(find.content_revision, 0);
        assert_eq!(find.current_index(original), Some(0));
        assert_eq!(outcome.next_match.unwrap().char_range, 7..13);

        let mut replace_all = NoteFindState::default();
        replace_all.set_query("needle");
        let original = "needle needle";
        let prepared = replace_all.prepare_replace_all(original, "needle").unwrap();
        let mut outcome = replace_all
            .apply_prepared_replacement(original, prepared)
            .unwrap();
        assert!(!outcome.changed);
        replace_all
            .commit_replacement_navigation(original, &mut outcome)
            .unwrap();
        assert_eq!(outcome.content, original);
        assert_eq!(replace_all.content_revision, 0);
        assert_eq!(replace_all.current_index(original), Some(0));
    }

    #[test]
    fn note_replace_rejects_stale_query_content_and_selected_match() {
        let original = "x x";

        let mut query_changed = NoteFindState::default();
        query_changed.set_query("x");
        let prepared = query_changed
            .prepare_replace_current(original, "y")
            .unwrap();
        query_changed.set_query("y");
        assert_eq!(
            query_changed.apply_prepared_replacement(original, prepared),
            Err(StaleNoteReplacement::QueryRevision)
        );

        let mut content_changed = NoteFindState::default();
        content_changed.set_query("x");
        let prepared = content_changed
            .prepare_replace_current(original, "y")
            .unwrap();
        content_changed.content_changed();
        assert_eq!(
            content_changed.apply_prepared_replacement(original, prepared),
            Err(StaleNoteReplacement::ContentRevision)
        );

        let mut selection_changed = NoteFindState::default();
        selection_changed.set_query("x");
        let prepared = selection_changed
            .prepare_replace_current(original, "y")
            .unwrap();
        assert_eq!(selection_changed.next(original).unwrap().byte_range, 2..3);
        assert_eq!(
            selection_changed.apply_prepared_replacement(original, prepared),
            Err(StaleNoteReplacement::SelectedMatch)
        );
    }

    #[test]
    fn note_replace_uses_suffix_matches_when_replacement_creates_a_new_adjacency() {
        let mut find = NoteFindState::default();
        find.set_query("aa");
        let original = "aaaa";

        let prepared = find.prepare_replace_current(original, "a").unwrap();
        let mut outcome = find.apply_prepared_replacement(original, prepared).unwrap();
        assert_eq!(outcome.content, "aaa");
        find.content_changed();
        let installed_content = outcome.content.clone();
        find.commit_replacement_navigation(&installed_content, &mut outcome)
            .unwrap();

        let selected = find.current_match(&outcome.content).unwrap().clone();
        assert_eq!(selected.byte_range, 1..3);
        assert_eq!(selected.char_range, 1..3);
        let prepared_next = find.prepare_replace_current(&outcome.content, "z").unwrap();
        let next = find
            .apply_prepared_replacement(&outcome.content, prepared_next)
            .unwrap();
        assert_eq!(next.content, "az");

        // Ordinary navigation restores the regular global cache and remaps
        // from the anchored range rather than reusing its old numeric index.
        assert_eq!(find.next(&outcome.content).unwrap().byte_range, 0..2);
    }

    #[test]
    fn note_replace_current_wraps_to_inserted_match_only_after_explicit_next() {
        let mut find = NoteFindState::default();
        find.set_query("x");
        let prepared = find.prepare_replace_current("x", "xx").unwrap();
        let mut outcome = find.apply_prepared_replacement("x", prepared).unwrap();
        assert_eq!(outcome.content, "xx");

        find.content_changed();
        let installed_content = outcome.content.clone();
        find.commit_replacement_navigation(&installed_content, &mut outcome)
            .unwrap();

        // Both matches are retained for the result count, but neither lies
        // beyond the insertion point, so replacement does not auto-select one.
        assert_eq!(find.results(&outcome.content).len(), 2);
        assert_eq!(find.current_index(&outcome.content), None);
        assert!(outcome.next_match.is_none());

        // Ordinary navigation may wrap to an inserted-text match explicitly.
        assert_eq!(find.next(&outcome.content).unwrap().byte_range, 0..1);
    }

    #[test]
    fn note_replace_tokens_are_rejected_after_note_reset_or_on_another_owner() {
        let original = "x";
        let mut find = NoteFindState::default();
        find.set_query("x");
        find.content_changed();
        let prepared = find.prepare_replace_all(original, "y").unwrap();

        find.reset_for_new_note();
        find.set_query("x");
        find.content_changed();
        assert_eq!(find.query_revision, prepared.query_revision);
        assert_eq!(find.content_revision, prepared.content_revision);
        assert_eq!(
            find.apply_prepared_replacement(original, prepared),
            Err(StaleNoteReplacement::NoteGeneration)
        );

        let mut source = NoteFindState::default();
        source.set_query("x");
        let prepared = source.prepare_replace_all(original, "y").unwrap();
        let mut other_owner = NoteFindState::default();
        other_owner.set_query("x");
        assert_eq!(
            other_owner.apply_prepared_replacement(original, prepared),
            Err(StaleNoteReplacement::DifferentOwner)
        );
    }
}
