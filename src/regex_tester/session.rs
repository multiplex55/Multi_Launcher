//! Deterministic, non-blocking scheduling for one editable tester document.

use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{EvaluationPolicy, EvaluationResult, RegexDraft, RegexMatch, evaluate_with_policy};

pub const EVALUATION_DEBOUNCE: Duration = Duration::from_millis(150);

#[derive(Default)]
pub struct RegexSession {
    pub draft: RegexDraft,
    policy: EvaluationPolicy,
    revision: u64,
    evaluated_revision: Option<u64>,
    pending_deadline: Option<Instant>,
    result: Option<EvaluationResult>,
    selected_index: Option<usize>,
    evaluated_source: Option<Arc<str>>,
}

/// Borrowed presentation data never clones matches or capture text.
pub struct EvaluatedText<'a> {
    pub revision: u64,
    pub evaluated_revision: Option<u64>,
    pub source: Option<&'a str>,
    pub matches: &'a [RegexMatch],
    pub selected_index: Option<usize>,
}

impl EvaluatedText<'_> {
    pub fn applies_to(&self, text: &str) -> bool {
        self.evaluated_revision == Some(self.revision) && self.source == Some(text)
    }
}

impl RegexSession {
    pub fn ensure_initial_evaluation(&mut self, now: Instant) {
        if self.result.is_none() && self.pending_deadline.is_none() {
            self.mark_changed(now);
        }
    }

    /// Call only after a pattern, flag, or source-text edit. Stale results are
    /// unavailable immediately; successive edits replace the same deadline.
    pub fn mark_changed(&mut self, now: Instant) {
        self.revision = self.revision.wrapping_add(1);
        self.evaluated_revision = None;
        self.evaluated_source = None;
        match self
            .policy
            .check_inputs(&self.draft.pattern, &self.draft.test_text)
        {
            Ok(()) => {
                self.result = None;
                self.pending_deadline = Some(now + EVALUATION_DEBOUNCE);
            }
            Err(suspension) => {
                self.result = Some(EvaluationResult::Suspended(suspension));
                self.pending_deadline = None;
                self.selected_index = None;
            }
        }
    }

    /// Returns true only when this call evaluates the latest document revision.
    pub fn tick(&mut self, now: Instant) -> bool {
        if !self
            .pending_deadline
            .is_some_and(|deadline| now >= deadline)
        {
            return false;
        }
        self.pending_deadline = None;
        self.result = Some(evaluate_with_policy(
            &self.draft.pattern,
            &self.draft.flags,
            &self.draft.test_text,
            &self.policy,
        ));
        self.evaluated_revision = Some(self.revision);
        self.evaluated_source = matches!(self.result, Some(EvaluationResult::Success { .. }))
            .then(|| Arc::from(self.draft.test_text.as_str()));
        let count = self.matches().len();
        self.selected_index = (count > 0).then(|| self.selected_index.unwrap_or(0).min(count - 1));
        true
    }

    pub fn pending_delay(&self, now: Instant) -> Option<Duration> {
        self.pending_deadline
            .map(|deadline| deadline.saturating_duration_since(now))
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn evaluated_revision(&self) -> Option<u64> {
        self.evaluated_revision
    }
    pub fn result(&self) -> Option<&EvaluationResult> {
        self.result.as_ref()
    }
    pub fn policy(&self) -> &EvaluationPolicy {
        &self.policy
    }

    pub fn matches(&self) -> &[RegexMatch] {
        match self.result.as_ref() {
            Some(EvaluationResult::Success { matches, .. }) => matches,
            _ => &[],
        }
    }

    pub fn selected_index(&self) -> Option<usize> {
        self.selected_index
            .filter(|index| *index < self.matches().len())
    }

    pub fn select_match(&mut self, index: usize) {
        let count = self.matches().len();
        self.selected_index = (count > 0).then(|| index.min(count - 1));
    }

    pub fn next_match(&mut self) -> bool {
        let count = self.matches().len();
        if count == 0 {
            return false;
        }
        self.selected_index = Some((self.selected_index().unwrap_or(0) + 1) % count);
        true
    }

    pub fn previous_match(&mut self) -> bool {
        let count = self.matches().len();
        if count == 0 {
            return false;
        }
        self.selected_index = Some((self.selected_index().unwrap_or(0) + count - 1) % count);
        true
    }

    pub fn text_edit_parts(&mut self) -> (&mut String, EvaluatedText<'_>) {
        let matches = match self.result.as_ref() {
            Some(EvaluationResult::Success { matches, .. }) => matches.as_slice(),
            _ => &[],
        };
        let view = EvaluatedText {
            revision: self.revision,
            evaluated_revision: self.evaluated_revision,
            source: self.evaluated_source.as_deref(),
            matches,
            selected_index: self.selected_index.filter(|index| *index < matches.len()),
        };
        (&mut self.draft.test_text, view)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_wraps_and_never_changes_source_or_pending_state() {
        let now = Instant::now();
        let mut session = RegexSession::default();
        assert!(!session.next_match());
        assert!(!session.previous_match());
        session.draft.pattern = "a".into();
        session.draft.test_text = "aaa".into();
        session.mark_changed(now);
        assert!(!session.next_match());
        session.tick(now + EVALUATION_DEBOUNCE);
        let draft = session.draft.clone();
        assert!(session.previous_match());
        assert_eq!(session.selected_index(), Some(2));
        assert!(session.next_match());
        assert_eq!(session.selected_index(), Some(0));
        session.next_match();
        assert_eq!(session.selected_index(), Some(1));
        assert_eq!(session.draft, draft);
        session.draft.pattern = "[".into();
        session.mark_changed(now);
        session.tick(now + EVALUATION_DEBOUNCE);
        assert!(!session.previous_match());
        session.draft.pattern = "z".into();
        session.mark_changed(now);
        session.tick(now + EVALUATION_DEBOUNCE);
        assert!(!session.next_match());
    }

    #[test]
    fn edits_coalesce_and_evaluate_latest_revision_once() {
        let now = Instant::now();
        let mut session = RegexSession::default();
        session.draft.pattern = "a".into();
        session.draft.test_text = "aaa".into();
        session.ensure_initial_evaluation(now);
        session.ensure_initial_evaluation(now);
        assert_eq!(session.revision(), 1);
        assert!(!session.tick(now + Duration::from_millis(100)));
        session.draft.pattern = "aa".into();
        session.mark_changed(now + Duration::from_millis(100));
        assert!(!session.tick(now + Duration::from_millis(200)));
        assert!(session.tick(now + Duration::from_millis(250)));
        assert_eq!(session.matches().len(), 1);
        assert_eq!(session.evaluated_revision(), Some(2));
        assert!(!session.tick(now + Duration::from_secs(1)));
        session.ensure_initial_evaluation(now + Duration::from_secs(1));
        assert!(session.pending_delay(now).is_none());
        assert_eq!(session.evaluated_source.as_deref(), Some("aaa"));
        session.mark_changed(now);
        assert!(session.evaluated_source.is_none());
    }

    #[test]
    fn flags_text_invalid_and_no_match_transitions_clear_stale_results() {
        let now = Instant::now();
        let mut session = RegexSession::default();
        session.draft.pattern = "a".into();
        session.draft.test_text = "A".into();
        session.mark_changed(now);
        session.tick(now + EVALUATION_DEBOUNCE);
        assert!(session.matches().is_empty());
        session.draft.flags.case_insensitive = true;
        session.mark_changed(now);
        assert!(session.result().is_none());
        session.tick(now + EVALUATION_DEBOUNCE);
        assert_eq!(session.matches().len(), 1);
        session.draft.pattern = "[".into();
        session.mark_changed(now);
        session.tick(now + EVALUATION_DEBOUNCE);
        assert!(matches!(
            session.result(),
            Some(EvaluationResult::InvalidPattern(_))
        ));
        assert!(session.pending_delay(now).is_none());
        session.draft.pattern = "a".into();
        session.draft.test_text = "bbb".into();
        session.mark_changed(now);
        assert!(session.result().is_none());
        session.tick(now + EVALUATION_DEBOUNCE);
        assert!(matches!(
            session.result(),
            Some(EvaluationResult::Success { .. })
        ));
        assert!(session.matches().is_empty());
        assert_eq!(session.selected_index(), None);
    }

    #[test]
    fn selection_is_unavailable_while_pending_and_clamped_after_evaluation() {
        let now = Instant::now();
        let mut session = RegexSession::default();
        session.draft.pattern = "a".into();
        session.draft.test_text = "aaa".into();
        session.mark_changed(now);
        session.tick(now + EVALUATION_DEBOUNCE);
        session.select_match(2);
        assert_eq!(session.selected_index(), Some(2));
        session.draft.test_text = "a".into();
        session.mark_changed(now);
        assert_eq!(session.selected_index(), None);
        session.tick(now + EVALUATION_DEBOUNCE);
        assert_eq!(session.selected_index(), Some(0));
        session.select_match(100);
        assert_eq!(session.selected_index(), Some(0));
    }

    #[test]
    fn oversized_inputs_suspend_without_pending_work_and_recover_after_edit() {
        let now = Instant::now();
        let mut session = RegexSession::default();
        session.draft.test_text = "x".repeat(session.policy().text_bytes + 1);
        session.mark_changed(now);
        assert!(matches!(
            session.result(),
            Some(EvaluationResult::Suspended(_))
        ));
        assert!(session.pending_delay(now).is_none());
        assert!(!session.tick(now + Duration::from_secs(5)));
        session.draft.test_text.clear();
        session.mark_changed(now);
        assert!(session.tick(now + EVALUATION_DEBOUNCE));
        assert!(matches!(
            session.result(),
            Some(EvaluationResult::Success { .. })
        ));
    }
}
