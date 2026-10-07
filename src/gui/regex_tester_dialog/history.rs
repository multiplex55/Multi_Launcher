//! History integration delegates persistence and corruption protection to the store.

use crate::regex_tester::{EvaluationResult, HistoryEntry, HistoryStore, RegexSession};
use eframe::egui;

pub(super) struct HistoryState {
    pub store: HistoryStore,
    attempted: Option<HistoryEntry>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::regex_tester::session::EVALUATION_DEBOUNCE;
    use std::time::Instant;

    fn settle(session: &mut RegexSession) {
        let now = Instant::now();
        session.mark_changed(now);
        session.tick(now + EVALUATION_DEBOUNCE);
    }

    #[test]
    fn records_only_changed_accepted_nonempty_pairs_and_keeps_buffers_private() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.json");
        let mut history = HistoryState::open(&path);
        let mut session = RegexSession::default();
        settle(&mut session);
        history.record_success(&session);
        assert!(!path.exists());
        session.draft.pattern = "é".into();
        session.draft.test_text = "private text é".into();
        session.draft.replacement = "private replacement".into();
        session.mark_changed(Instant::now());
        history.record_success(&session);
        assert!(!path.exists());
        settle(&mut session);
        history.record_success(&session);
        let persisted = std::fs::read(&path).unwrap();
        let document: serde_json::Value = serde_json::from_slice(&persisted).unwrap();
        assert_eq!(document["entries"][0].as_object().unwrap().len(), 2);
        assert!(
            !String::from_utf8(persisted.clone())
                .unwrap()
                .contains("private")
        );
        // A redundant record would discover corruption. Remaining writable is
        // evidence that unchanged-pair text edits did not access the store.
        std::fs::write(&path, b"external corruption").unwrap();
        session.draft.test_text = "different private text".into();
        settle(&mut session);
        history.record_success(&session);
        session.draft.replacement = "different replacement".into();
        history.record_success(&session);
        assert!(history.store.is_writable());
        assert_eq!(std::fs::read(&path).unwrap(), b"external corruption");
        std::fs::write(&path, &persisted).unwrap();
        session.draft.flags.case_insensitive = true;
        settle(&mut session);
        history.record_success(&session);
        assert_eq!(history.store.entries().len(), 2);
        let accepted = std::fs::read(&path).unwrap();
        session.draft.pattern = "[".into();
        settle(&mut session);
        history.record_success(&session);
        session.draft.pattern = "x".repeat(session.policy().pattern_bytes + 1);
        settle(&mut session);
        history.record_success(&session);
        assert_eq!(std::fs::read(&path).unwrap(), accepted);
    }

    #[test]
    fn corruption_preserves_snapshot_and_bytes_until_explicit_reload_and_retry() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.json");
        let mut history = HistoryState::open(&path);
        let mut session = RegexSession::default();
        session.draft.pattern = "a".into();
        settle(&mut session);
        history.record_success(&session);
        let valid = std::fs::read(&path).unwrap();
        std::fs::write(&path, b"broken history").unwrap();
        session.draft.pattern = "b".into();
        settle(&mut session);
        history.record_success(&session);
        assert!(!history.store.is_writable());
        assert!(history.store.diagnostic().is_some());
        assert_eq!(history.store.entries()[0].pattern, "a");
        assert_eq!(std::fs::read(&path).unwrap(), b"broken history");
        std::fs::write(&path, &valid).unwrap();
        session.draft.test_text = "b".into();
        settle(&mut session);
        history.record_success(&session);
        assert_eq!(std::fs::read(&path).unwrap(), valid);
        assert!(!history.store.is_writable());
        history.store.reload().unwrap();
        history.retry(&session);
        assert_eq!(history.store.entries()[0].pattern, "b");
        assert!(history.store.diagnostic().is_none());
        assert_eq!(HistoryStore::open(&path).entries(), history.store.entries());
    }
}

pub(super) enum HistoryAction {
    Load(usize),
    Reload,
    Retry,
}

impl HistoryState {
    pub fn open(path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            store: HistoryStore::open(path),
            attempted: None,
        }
    }

    pub fn record_success(&mut self, session: &RegexSession) {
        if !matches!(session.result(), Some(EvaluationResult::Success { .. }))
            || session.evaluated_revision() != Some(session.revision())
            || session.draft.pattern.is_empty()
        {
            return;
        }
        let draft = &session.draft;
        if self
            .attempted
            .as_ref()
            .is_some_and(|entry| entry.pattern == draft.pattern && entry.flags == draft.flags)
        {
            return;
        }
        // Remember attempted pairs even on failure: frames and text-only edits
        // cannot repeatedly retry disk writes. Recovery is an explicit action.
        self.attempted = Some(HistoryEntry {
            pattern: draft.pattern.clone(),
            flags: draft.flags,
        });
        let _ = self.store.record(&draft.pattern, draft.flags);
    }

    pub fn retry(&mut self, session: &RegexSession) {
        self.attempted = None;
        self.record_success(session);
    }

    pub fn show(&self, ui: &mut egui::Ui, retryable: bool) -> Option<HistoryAction> {
        let mut action = None;
        egui::ScrollArea::vertical()
            .id_source("regex_history_scroll")
            .auto_shrink([false, false])
            .max_height(ui.available_height().max(1.0))
            .show(ui, |ui| {
                ui.set_max_width(ui.available_width());
                ui.style_mut().wrap = Some(true);
                if let Some(diagnostic) = self.store.diagnostic() {
                    ui.colored_label(ui.visuals().error_fg_color, &diagnostic.message);
                }
                if !self.store.is_writable() {
                    ui.weak("History writes are blocked. The existing file is preserved; repair it and reload to recover.");
                }
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Reload history").clicked() { action = Some(HistoryAction::Reload); }
                    if ui.add_enabled(retryable && self.store.is_writable(), egui::Button::new("Retry record")).clicked() {
                        action = Some(HistoryAction::Retry);
                    }
                });
                if self.store.entries().is_empty() { ui.weak("No recent expressions."); }
                for (index, entry) in self.store.entries().iter().enumerate() {
                    ui.push_id(("regex_history_entry", index), |ui| {
                        ui.strong(format!("Recent #{} · flags {}", index + 1, entry.flags.suffix()));
                        ui.label(egui::RichText::new(super::utf8_prefix(&entry.pattern, 160)).monospace());
                        if entry.pattern.len() > 160 { ui.weak("Pattern preview; Load restores the complete expression."); }
                        if ui.button("Load expression").clicked() { action = Some(HistoryAction::Load(index)); }
                        ui.separator();
                    });
                }
            });
        action
    }
}
