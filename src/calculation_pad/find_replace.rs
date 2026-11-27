//! Renderer-neutral literal Find and Replace state for Calculation Pad.
use std::{ops::Range, sync::Arc};

#[derive(Clone, Default, PartialEq, Eq)]
pub struct FindReplace {
    pub query: String,
    pub replacement: String,
    pub case_sensitive: bool,
    pub matches: Arc<[Range<usize>]>,
    pub current: Option<usize>,
    indexed_revision: Option<u64>,
    indexed_query: String,
    indexed_case: bool,
}

impl FindReplace {
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
            self.matches = regex.find_iter(source).map(|found| found.range()).collect::<Vec<_>>().into();
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
        if self.replacement.len() > crate::calculation_pad::MAX_ADMITTED_DOCUMENT_BYTES {
            return None;
        }
        let ranges = if all { self.matches.as_ref() } else { std::slice::from_ref(self.matches.get(self.current?)?) };
        if ranges.is_empty() { return None; }
        let removed = ranges.iter().try_fold(0usize, |sum, range| sum.checked_add(range.len()))?;
        let added = self.replacement.len().checked_mul(ranges.len())?;
        let size = source.len().checked_sub(removed)?.checked_add(added)?;
        if size > crate::calculation_pad::MAX_ADMITTED_DOCUMENT_BYTES { return None; }
        let mut result = String::with_capacity(size);
        let mut cursor = 0;
        for range in ranges {
            result.push_str(source.get(cursor..range.start)?);
            result.push_str(&self.replacement);
            cursor = range.end;
        }
        result.push_str(source.get(cursor..)?);
        if !crate::calculation_pad::source_admitted(&result) { return None; }
        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::FindReplace;

    #[test]
    fn literal_navigation_wraps_and_invalidates_on_query_case_and_revision() {
        let source = "雪A\r\n雪a\r\n";
        let mut find = FindReplace { query: "雪a".into(), ..Default::default() };
        find.refresh(source, 1);
        assert_eq!(find.navigate(false), Some(6..10));
        assert_eq!(find.navigate(true), Some(0..4));
        assert_eq!(find.current_match(2), None);
        find.case_sensitive = true;
        find.refresh(source, 1);
        assert_eq!(find.current, None);
        assert_eq!(find.navigate(true), Some(6..10));
        find.query = ".".into();
        find.refresh(source, 1);
        assert!(find.matches.is_empty());
        find.query = "雪".into();
        find.refresh(source, 2);
        assert_eq!(find.navigate(true), Some(0..3));
    }
}
