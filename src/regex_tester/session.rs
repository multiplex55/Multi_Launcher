//! Deterministic, non-blocking scheduling for one editable tester document.

use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{
    EvaluationPolicy, EvaluationResult, ExplanationResult, RegexDraft, RegexFlags, RegexMatch,
    SubstitutionEvaluationResult, evaluate_substitution_with_policy, evaluate_with_policy, explain,
};

pub const EVALUATION_DEBOUNCE: Duration = Duration::from_millis(150);

#[derive(Default)]
pub struct RegexSession {
    pub draft: RegexDraft,
    policy: EvaluationPolicy,
    revision: u64,
    evaluated_revision: Option<u64>,
    pending_deadline: Option<Instant>,
    pending_matching: bool,
    result: Option<EvaluationResult>,
    selected_index: Option<usize>,
    evaluated_source: Option<Arc<str>>,
    explanation_cache: Option<ExplanationCache>,
    substitution_enabled: bool,
    substitution_result: Option<SubstitutionEvaluationResult>,
}

struct ExplanationCache {
    pattern: String,
    flags: RegexFlags,
    result: Arc<ExplanationResult>,
}

impl ExplanationCache {
    fn matches(&self, draft: &RegexDraft) -> bool {
        self.pattern == draft.pattern && self.flags == draft.flags
    }
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
        self.substitution_result = None;
        match self
            .policy
            .check_inputs(&self.draft.pattern, &self.draft.test_text)
        {
            Ok(()) => {
                self.pending_matching = true;
                self.result = None;
                self.pending_deadline = Some(now + EVALUATION_DEBOUNCE);
            }
            Err(suspension) => {
                self.pending_matching = false;
                if self.substitution_enabled {
                    self.substitution_result =
                        Some(SubstitutionEvaluationResult::Suspended(suspension));
                }
                self.result = Some(EvaluationResult::Suspended(suspension));
                self.pending_deadline = None;
                self.selected_index = None;
            }
        }
    }

    /// Returns true only when queued matching or substitution work completes.
    pub fn tick(&mut self, now: Instant) -> bool {
        if !self
            .pending_deadline
            .is_some_and(|deadline| now >= deadline)
        {
            return false;
        }
        self.pending_deadline = None;
        if std::mem::take(&mut self.pending_matching) {
            self.result = Some(evaluate_with_policy(
                &self.draft.pattern,
                &self.draft.flags,
                &self.draft.test_text,
                &self.policy,
            ));
            self.evaluated_revision = Some(self.revision);
            self.evaluated_source = matches!(self.result, Some(EvaluationResult::Success { .. }))
                .then(|| Arc::from(self.draft.test_text.as_str()));
            if matches!(self.result, Some(EvaluationResult::Success { .. }))
                && !self
                    .explanation_cache
                    .as_ref()
                    .is_some_and(|cache| cache.matches(&self.draft))
            {
                self.explanation_cache = Some(ExplanationCache {
                    pattern: self.draft.pattern.clone(),
                    flags: self.draft.flags,
                    result: Arc::new(explain(&self.draft.pattern, &self.draft.flags)),
                });
            }
            let count = self.matches().len();
            self.selected_index =
                (count > 0).then(|| self.selected_index.unwrap_or(0).min(count - 1));
        }
        if self.substitution_enabled {
            self.substitution_result = match self.result.as_ref() {
                Some(EvaluationResult::Success { .. }) => Some(evaluate_substitution_with_policy(
                    &self.draft.pattern,
                    &self.draft.flags,
                    &self.draft.test_text,
                    &self.draft.replacement,
                    &self.policy,
                )),
                Some(EvaluationResult::InvalidPattern(error)) => {
                    Some(SubstitutionEvaluationResult::InvalidPattern(error.clone()))
                }
                Some(EvaluationResult::Suspended(reason)) => {
                    Some(SubstitutionEvaluationResult::Suspended(*reason))
                }
                None => None,
            };
        }
        true
    }

    pub fn substitution_enabled(&self) -> bool {
        self.substitution_enabled
    }
    pub fn substitution_result(&self) -> Option<&SubstitutionEvaluationResult> {
        self.substitution_result.as_ref()
    }

    pub fn set_substitution_enabled(&mut self, enabled: bool, now: Instant) {
        if self.substitution_enabled == enabled {
            return;
        }
        self.substitution_enabled = enabled;
        if enabled {
            self.ensure_initial_evaluation(now);
            self.mark_replacement_changed(now);
        } else {
            self.substitution_result = None;
            if !self.pending_matching {
                self.pending_deadline = None;
            }
        }
    }

    /// Replacement edits preserve accepted source ranges, navigation, and syntax
    /// analysis. Only replacement work shares/coalesces the existing deadline.
    pub fn mark_replacement_changed(&mut self, now: Instant) {
        self.substitution_result = None;
        if !self.substitution_enabled {
            return;
        }
        if let Err(reason) = super::policy::check_limit(
            super::EvaluationLimit::ReplacementBytes,
            self.policy.replacement_bytes,
            self.draft.replacement.len(),
        ) {
            self.substitution_result = Some(SubstitutionEvaluationResult::Suspended(reason));
            if !self.pending_matching {
                self.pending_deadline = None;
            }
            return;
        }
        match self.result.as_ref() {
            Some(EvaluationResult::InvalidPattern(error)) => {
                self.substitution_result =
                    Some(SubstitutionEvaluationResult::InvalidPattern(error.clone()));
            }
            Some(EvaluationResult::Suspended(reason)) => {
                self.substitution_result = Some(SubstitutionEvaluationResult::Suspended(*reason));
            }
            _ => {
                self.pending_deadline = Some(now + EVALUATION_DEBOUNCE);
            }
        }
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

    /// Expose analysis only for the currently accepted document. The bounded
    /// pattern/flag cache survives text edits, but pending and invalid revisions
    /// never expose old syntax descriptions.
    pub fn explanation(&self) -> Option<&ExplanationResult> {
        if self.evaluated_revision != Some(self.revision)
            || !matches!(self.result, Some(EvaluationResult::Success { .. }))
        {
            return None;
        }
        self.explanation_cache
            .as_ref()
            .filter(|cache| cache.matches(&self.draft))
            .map(|cache| cache.result.as_ref())
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
    fn substitution_replacement_edits_coalesce_without_rerunning_accepted_matching() {
        let now = Instant::now();
        let mut session = RegexSession::default();
        session.draft.pattern = "a".into();
        session.draft.test_text = "aaa".into();
        session.mark_changed(now);
        session.tick(now + EVALUATION_DEBOUNCE);
        session.select_match(2);
        let source = session.evaluated_source.as_ref().unwrap().clone();
        let explanation = session.explanation_cache.as_ref().unwrap().result.clone();
        let revision = session.revision();
        session.set_substitution_enabled(true, now);
        session.draft.replacement = "X".into();
        session.mark_replacement_changed(now + Duration::from_millis(100));
        assert!(session.substitution_result().is_none());
        assert!(!session.tick(now + Duration::from_millis(200)));
        assert!(session.tick(now + Duration::from_millis(250)));
        let Some(SubstitutionEvaluationResult::Success(result)) = session.substitution_result()
        else {
            panic!("expected substitution");
        };
        assert_eq!(result.output, "XXX");
        assert_eq!(session.selected_index(), Some(2));
        assert_eq!(session.revision(), revision);
        assert!(Arc::ptr_eq(
            &source,
            session.evaluated_source.as_ref().unwrap()
        ));
        assert!(Arc::ptr_eq(
            &explanation,
            &session.explanation_cache.as_ref().unwrap().result
        ));
        assert!(!session.tick(now + Duration::from_secs(1)));
        session.draft.replacement = "Y".into();
        session.mark_replacement_changed(now);
        session.set_substitution_enabled(false, now);
        assert!(session.pending_delay(now).is_none());
        assert!(session.substitution_result().is_none());
        assert_eq!(session.matches().len(), 3);
        session.set_substitution_enabled(true, now);
        session.draft.test_text = "a".into();
        session.mark_changed(now);
        session.draft.replacement = "Z".into();
        session.mark_replacement_changed(now + Duration::from_millis(100));
        assert!(!session.tick(now + Duration::from_millis(200)));
        session.set_substitution_enabled(false, now + Duration::from_millis(200));
        assert!(session.tick(now + Duration::from_millis(250)));
        assert_eq!(session.matches().len(), 1);
        assert!(session.substitution_result().is_none());
        assert!(session.pending_delay(now).is_none());
    }

    #[test]
    fn substitution_semantics_invalid_and_budget_transitions_never_leave_stale_output() {
        let now = Instant::now();
        let mut session = RegexSession::default();
        session.draft.pattern = "(é)(?P<animal>🦀)".into();
        session.draft.test_text = "é🦀".into();
        session.draft.replacement = "$2:${animal}:$1:$$".into();
        session.set_substitution_enabled(true, now);
        session.tick(now + EVALUATION_DEBOUNCE);
        let Some(SubstitutionEvaluationResult::Success(result)) = session.substitution_result()
        else {
            panic!("expected substitution");
        };
        assert_eq!(result.output, "🦀:🦀:é:$");
        session.draft.pattern.clear();
        session.draft.test_text = "é".into();
        session.draft.replacement = "X".into();
        session.mark_changed(now);
        assert!(session.substitution_result().is_none());
        session.tick(now + EVALUATION_DEBOUNCE);
        let Some(SubstitutionEvaluationResult::Success(result)) = session.substitution_result()
        else {
            panic!("expected zero-width substitution");
        };
        assert_eq!(result.output, "XéX");
        assert_eq!(result.replacements_made, 2);
        session.draft.pattern = "[".into();
        session.mark_changed(now);
        session.tick(now + EVALUATION_DEBOUNCE);
        assert!(matches!(
            session.substitution_result(),
            Some(SubstitutionEvaluationResult::InvalidPattern(_))
        ));
        assert!(session.pending_delay(now).is_none());
        session.draft.pattern = "é".into();
        session.mark_changed(now);
        session.tick(now + EVALUATION_DEBOUNCE);
        assert!(matches!(
            session.substitution_result(),
            Some(SubstitutionEvaluationResult::Success(_))
        ));
        session.policy.replacement_output_bytes = 1;
        session.draft.replacement = "too long".into();
        session.mark_replacement_changed(now);
        assert!(session.substitution_result().is_none());
        session.tick(now + EVALUATION_DEBOUNCE);
        assert!(
            matches!(session.substitution_result(), Some(SubstitutionEvaluationResult::Suspended(reason)) if reason.limit == super::super::EvaluationLimit::ReplacementOutputBytes)
        );
        assert!(session.pending_delay(now).is_none());
        assert!(!session.tick(now + Duration::from_secs(1)));
        session.draft.replacement = "🦀".repeat(session.policy().replacement_bytes);
        let retained = session.draft.replacement.clone();
        session.mark_replacement_changed(now);
        assert!(
            matches!(session.substitution_result(), Some(SubstitutionEvaluationResult::Suspended(reason)) if reason.limit == super::super::EvaluationLimit::ReplacementBytes)
        );
        assert_eq!(session.draft.replacement, retained);
        assert!(session.pending_delay(now).is_none());
        assert_eq!(session.matches().len(), 1);
        session.draft.pattern = "x".repeat(session.policy().pattern_bytes + 1);
        session.mark_changed(now);
        assert!(
            matches!(session.substitution_result(), Some(SubstitutionEvaluationResult::Suspended(reason)) if reason.limit == super::super::EvaluationLimit::PatternBytes)
        );
        assert!(!session.pending_matching);
        session.set_substitution_enabled(false, now);
        session.ensure_initial_evaluation(now);
        assert!(session.pending_delay(now).is_none());
        assert!(!session.tick(now + Duration::from_secs(1)));
        session.draft.pattern = "é".into();
        session.mark_changed(now);
        session.ensure_initial_evaluation(now);
        assert!(session.tick(now + EVALUATION_DEBOUNCE));
        assert_eq!(session.matches().len(), 1);
        assert!(session.substitution_result().is_none());
    }

    #[test]
    fn explanation_cache_reuses_text_edits_and_idle_but_replaces_pattern_and_flags() {
        let now = Instant::now();
        let mut session = RegexSession::default();
        session.draft.pattern = "(?i:é)+".into();
        session.mark_changed(now);
        assert!(session.explanation().is_none());
        session.tick(now + EVALUATION_DEBOUNCE);
        let cached = session.explanation_cache.as_ref().unwrap().result.clone();
        let ExplanationResult::Success { explanations } = session.explanation().unwrap() else {
            panic!("valid syntax must have structured explanations");
        };
        assert!(explanations.iter().any(|entry| entry.token == "é"));
        assert!(
            explanations
                .iter()
                .any(|entry| entry.label == "Scoped flags")
        );
        for entry in explanations {
            assert_eq!(
                &session.draft.pattern[entry.span.start_byte()..entry.span.end_byte()],
                entry.token
            );
        }
        session.draft.test_text = "Éé".into();
        session.mark_changed(now);
        assert!(session.explanation().is_none());
        session.tick(now + EVALUATION_DEBOUNCE);
        assert!(Arc::ptr_eq(
            &cached,
            &session.explanation_cache.as_ref().unwrap().result
        ));
        assert!(!session.tick(now + Duration::from_secs(1)));
        assert!(Arc::ptr_eq(
            &cached,
            &session.explanation_cache.as_ref().unwrap().result
        ));
        session.draft.flags.multi_line = true;
        session.mark_changed(now);
        assert!(session.explanation().is_none());
        session.tick(now + EVALUATION_DEBOUNCE);
        let changed_flags = session.explanation_cache.as_ref().unwrap().result.clone();
        assert!(!Arc::ptr_eq(&cached, &changed_flags));
        session.draft.pattern = "🦀".into();
        session.mark_changed(now);
        session.tick(now + EVALUATION_DEBOUNCE);
        assert!(!Arc::ptr_eq(
            &changed_flags,
            &session.explanation_cache.as_ref().unwrap().result
        ));
    }

    #[test]
    fn invalid_pending_and_suspended_revisions_never_expose_stale_explanations() {
        let now = Instant::now();
        let mut session = RegexSession::default();
        session.draft.pattern = "a".into();
        session.mark_changed(now);
        session.tick(now + EVALUATION_DEBOUNCE);
        let cached = session.explanation_cache.as_ref().unwrap().result.clone();
        session.draft.pattern = "[".into();
        session.mark_changed(now);
        assert!(session.explanation().is_none());
        session.tick(now + EVALUATION_DEBOUNCE);
        assert!(matches!(
            session.result(),
            Some(EvaluationResult::InvalidPattern(_))
        ));
        assert!(session.explanation().is_none());
        assert!(Arc::ptr_eq(
            &cached,
            &session.explanation_cache.as_ref().unwrap().result
        ));
        session.draft.pattern = "x".repeat(session.policy().pattern_bytes + 1);
        session.mark_changed(now);
        assert!(!session.tick(now + EVALUATION_DEBOUNCE));
        assert!(session.explanation().is_none());
        assert!(Arc::ptr_eq(
            &cached,
            &session.explanation_cache.as_ref().unwrap().result
        ));
        session.draft.pattern = "a".into();
        session.mark_changed(now);
        assert!(session.explanation().is_none());
        session.tick(now + EVALUATION_DEBOUNCE);
        assert!(session.explanation().is_some());
        assert!(Arc::ptr_eq(
            &cached,
            &session.explanation_cache.as_ref().unwrap().result
        ));
    }

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
