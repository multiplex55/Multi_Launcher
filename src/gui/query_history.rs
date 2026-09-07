use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QueryHistoryDirection {
    Older,
    Newer,
}

/// Transient shell-style navigation state for executed launcher queries.
///
/// The persisted history remains owned by `history`; this type takes one lazy
/// snapshot when traversal starts and holds it only for that traversal.
#[derive(Debug, Default)]
pub(crate) struct QueryHistoryNavigator {
    entries: Vec<String>,
    cursor: Option<usize>,
    draft: String,
    expected_query: Option<String>,
}

impl QueryHistoryNavigator {
    pub(crate) fn synchronize(&mut self, actual_query: &str) {
        if self
            .expected_query
            .as_deref()
            .is_some_and(|expected| expected != actual_query)
        {
            self.reset();
        }
    }

    pub(crate) fn older<I>(
        &mut self,
        current_query: &str,
        snapshot: impl FnOnce() -> I,
    ) -> Option<String>
    where
        I: IntoIterator<Item = String>,
    {
        self.synchronize(current_query);

        if let Some(cursor) = self.cursor {
            let next_cursor = cursor + 1;
            if next_cursor >= self.entries.len() {
                return None;
            }
            self.cursor = Some(next_cursor);
            let query = self.entries[next_cursor].clone();
            self.expected_query = Some(query.clone());
            return Some(query);
        }

        let mut seen = HashSet::new();
        let entries = snapshot()
            .into_iter()
            .filter(|query| !query.trim().is_empty())
            .filter(|query| seen.insert(query.clone()))
            .collect::<Vec<_>>();
        let query = entries.first()?.clone();

        self.entries = entries;
        self.cursor = Some(0);
        self.draft = current_query.to_owned();
        self.expected_query = Some(query.clone());
        Some(query)
    }

    pub(crate) fn newer(&mut self, current_query: &str) -> Option<String> {
        self.synchronize(current_query);
        let cursor = self.cursor?;
        if cursor > 0 {
            let next_cursor = cursor - 1;
            self.cursor = Some(next_cursor);
            let query = self.entries[next_cursor].clone();
            self.expected_query = Some(query.clone());
            Some(query)
        } else {
            let draft = self.draft.clone();
            self.reset();
            Some(draft)
        }
    }

    pub(crate) fn reset(&mut self) {
        self.entries.clear();
        self.cursor = None;
        self.draft.clear();
        self.expected_query = None;
    }

    #[cfg(test)]
    fn is_active(&self) -> bool {
        self.cursor.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn snapshot<'a>(values: &'a [&'a str]) -> impl Iterator<Item = String> + 'a {
        values.iter().map(|value| (*value).to_owned())
    }

    #[test]
    fn query_history_walks_older_then_restores_exact_draft_without_wrapping() {
        let mut navigator = QueryHistoryNavigator::default();

        assert_eq!(
            navigator.older("  mkma ", || snapshot(&["mkmacro", "note roadmap"])),
            Some("mkmacro".into())
        );
        assert_eq!(
            navigator.older("mkmacro", std::iter::empty),
            Some("note roadmap".into())
        );
        assert_eq!(navigator.older("note roadmap", std::iter::empty), None);
        assert_eq!(navigator.newer("note roadmap"), Some("mkmacro".into()));
        assert_eq!(navigator.newer("mkmacro"), Some("  mkma ".into()));
        assert!(!navigator.is_active());
        assert_eq!(navigator.newer("  mkma "), None);
    }

    #[test]
    fn query_history_restores_an_empty_draft() {
        let mut navigator = QueryHistoryNavigator::default();
        assert_eq!(
            navigator.older("", || snapshot(&["last query"])),
            Some("last query".into())
        );
        assert_eq!(navigator.newer("last query"), Some(String::new()));
        assert!(!navigator.is_active());
    }

    #[test]
    fn query_history_filters_blanks_and_exact_duplicates_keeping_newest() {
        let mut navigator = QueryHistoryNavigator::default();
        assert_eq!(
            navigator.older("draft", || snapshot(&[
                "",
                "  ",
                "MiXeD arg",
                "older",
                "MiXeD arg",
                "mixed arg"
            ])),
            Some("MiXeD arg".into())
        );
        assert_eq!(
            navigator.older("MiXeD arg", std::iter::empty),
            Some("older".into())
        );
        assert_eq!(
            navigator.older("older", std::iter::empty),
            Some("mixed arg".into())
        );
    }

    #[test]
    fn query_history_empty_snapshot_is_inactive_and_does_not_change_query() {
        let mut navigator = QueryHistoryNavigator::default();
        assert_eq!(navigator.older("draft", || snapshot(&["", " \t"])), None);
        assert!(!navigator.is_active());
        assert_eq!(navigator.newer("draft"), None);
    }

    #[test]
    fn query_history_snapshot_is_lazy_and_taken_once_per_traversal() {
        let calls = Cell::new(0);
        let mut navigator = QueryHistoryNavigator::default();
        assert_eq!(
            navigator.older("draft", || {
                calls.set(calls.get() + 1);
                snapshot(&["new", "old"])
            }),
            Some("new".into())
        );
        assert_eq!(
            navigator.older("new", || {
                calls.set(calls.get() + 1);
                snapshot(&["unexpected"])
            }),
            Some("old".into())
        );
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn query_history_divergence_abandons_snapshot_and_uses_edited_draft() {
        let mut navigator = QueryHistoryNavigator::default();
        assert_eq!(
            navigator.older("first draft", || snapshot(&["first history"])),
            Some("first history".into())
        );

        navigator.synchronize("manually edited");
        assert!(!navigator.is_active());
        assert_eq!(
            navigator.older("manually edited", || snapshot(&["new history"])),
            Some("new history".into())
        );
        assert_eq!(
            navigator.newer("new history"),
            Some("manually edited".into())
        );
    }

    #[test]
    fn query_history_reset_forces_a_fresh_snapshot() {
        let mut navigator = QueryHistoryNavigator::default();
        assert_eq!(
            navigator.older("draft", || snapshot(&["before action"])),
            Some("before action".into())
        );
        navigator.reset();
        assert_eq!(
            navigator.older("after action", || snapshot(&["newly recorded"])),
            Some("newly recorded".into())
        );
    }
}
