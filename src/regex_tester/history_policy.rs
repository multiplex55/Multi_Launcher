//! Accepted pattern history policy shared by frontend owners.
use super::{EvaluationResult, HistoryEntry, RegexSession};

#[derive(Default)]
pub struct AcceptedHistory {
    attempted: Option<HistoryEntry>,
}
impl AcceptedHistory {
    /// Admit a current successful, nonempty exact pair once, including failures.
    pub fn admit(&mut self, session: &RegexSession) -> Option<HistoryEntry> {
        if !matches!(session.result(), Some(EvaluationResult::Success { .. }))
            || session.evaluated_revision() != Some(session.revision())
            || session.draft.pattern.is_empty()
        { return None; }
        let entry = HistoryEntry { pattern: session.draft.pattern.clone(), flags: session.draft.flags };
        if self.attempted.as_ref() == Some(&entry) { return None; }
        self.attempted = Some(entry.clone());
        Some(entry)
    }
    pub fn retry(&mut self) { self.attempted = None; }
}
#[cfg(test)]
mod tests {
    use super::AcceptedHistory;
    use crate::regex_tester::RegexSession;
    use std::time::{Duration, Instant};
    #[test]
    fn exact_accepted_pair_is_attempted_once_until_pair_changes() {
        let mut policy = AcceptedHistory::default();
        let mut session = RegexSession::default();
        let now = Instant::now();
        session.mark_changed(now); session.tick(now + Duration::from_millis(200));
        assert!(policy.admit(&session).is_none());
        session.draft.pattern = "é".into(); session.draft.test_text = "private text".into();
        session.mark_changed(now); assert!(policy.admit(&session).is_none());
        session.tick(now + Duration::from_millis(200));
        let first = policy.admit(&session).unwrap();
        assert_eq!(first.pattern, "é");
        session.draft.test_text = "another private value".into(); session.mark_changed(now); session.tick(now + Duration::from_millis(200));
        assert!(policy.admit(&session).is_none());
        session.draft.flags.case_insensitive = true; session.mark_changed(now); session.tick(now + Duration::from_millis(200));
        assert!(policy.admit(&session).unwrap().flags.case_insensitive);
        session.draft.pattern = "[".into(); session.mark_changed(now); session.tick(now + Duration::from_millis(200));
        assert!(policy.admit(&session).is_none());
        policy.retry(); session.draft.pattern = "é".into(); session.mark_changed(now); session.tick(now + Duration::from_millis(200));
        assert!(policy.admit(&session).is_some());
    }
}
