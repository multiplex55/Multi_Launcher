//! Shared remembered presentation state for docked and independent utility hosts.
//!
//! This owner contains no frontend state. Adapters can read the effective
//! choice, accept a change, and poll the single settings writer for acknowledgments.
use crate::SettingsSaveWorkerRegistry;
use crate::settings::{RememberedPresentation, Settings, UtilityPresentations, UtilityToolKey};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UtilityPresentationUpdate {
    /// The accepted value already matched the effective preference.
    Unchanged,
    /// The shared settings writer accepted this revision.
    Pending,
    /// The shared writer could not queue this revision; `poll` returns its error once.
    ImmediateFailure,
}

#[derive(Clone)]
pub struct UtilityPresentationStore(Arc<Mutex<State>>);

struct State {
    path: String,
    workers: SettingsSaveWorkerRegistry,
    saved: UtilityPresentations,
    local: HashMap<UtilityToolKey, RememberedPresentation>,
    waiting: HashMap<UtilityToolKey, u64>,
    reported: HashMap<UtilityToolKey, u64>,
    immediate_failures: Vec<(UtilityToolKey, String)>,
}

impl UtilityPresentationStore {
    pub fn new(
        path: String,
        workers: SettingsSaveWorkerRegistry,
        saved: UtilityPresentations,
    ) -> Self {
        Self(Arc::new(Mutex::new(State {
            path,
            workers,
            saved,
            local: HashMap::new(),
            waiting: HashMap::new(),
            reported: HashMap::new(),
            immediate_failures: Vec::new(),
        })))
    }

    /// Bind the store to the current canonical settings file and writer.
    ///
    /// Rebinding to the same path keeps accepted local choices. A different
    /// path reloads its saved choices and discards local choices from the old
    /// data root. Acknowledgments from the previous writer are ignored.
    pub fn rebind(&self, path: String, workers: SettingsSaveWorkerRegistry) {
        let mut state = self.lock();
        if state.path != path {
            state.saved = Settings::load(&path)
                .map(|settings| settings.utility_presentations)
                .unwrap_or_default();
            state.local.clear();
            state.immediate_failures.clear();
        }
        state.waiting.clear();
        state.reported.clear();
        state.path = path;
        state.workers = workers;
    }

    /// Return the accepted in-session choice, falling back to the saved value.
    pub fn preference(&self, key: UtilityToolKey) -> Option<RememberedPresentation> {
        let state = self.lock();
        state
            .local
            .get(&key)
            .copied()
            .or_else(|| state.saved.get(key))
    }

    /// Accept an in-session choice and queue it on the shared settings writer.
    ///
    /// The local choice is committed before queueing, so it remains effective
    /// if the writer cannot start or the later persistence attempt fails.
    pub fn remember(
        &self,
        key: UtilityToolKey,
        value: RememberedPresentation,
    ) -> UtilityPresentationUpdate {
        let mut state = self.lock();
        if state
            .local
            .get(&key)
            .copied()
            .or_else(|| state.saved.get(key))
            == Some(value)
        {
            return UtilityPresentationUpdate::Unchanged;
        }

        // Keep local acceptance and writer revisions ordered when multiple
        // frontend adapters share this store.
        state.local.insert(key, value);
        match state
            .workers
            .queue_utility_presentation(state.path.clone(), key, value)
        {
            Ok(revision) => {
                state.waiting.insert(key, revision);
                UtilityPresentationUpdate::Pending
            }
            Err(error) => {
                state.immediate_failures.push((key, error));
                UtilityPresentationUpdate::ImmediateFailure
            }
        }
    }

    /// Apply current acknowledgments and return each current write failure once.
    pub fn poll(&self) -> Vec<(UtilityToolKey, String)> {
        let mut state = self.lock();
        let mut failures = std::mem::take(&mut state.immediate_failures);

        for (key, revision, value) in state.workers.drain_utility_presentation_acks() {
            if state.waiting.get(&key) == Some(&revision) {
                let _ = state.saved.set(key, value);
                state.waiting.remove(&key);
                state.reported.remove(&key);
            }
        }

        let waiting = state.waiting.clone();
        for (key, revision) in waiting {
            if let Some((failed, error)) = state.workers.utility_presentation_failure(key)
                && failed == revision
            {
                state.waiting.remove(&key);
                if state.reported.insert(key, revision) != Some(revision) {
                    failures.push((key, error));
                }
            }
        }

        failures
    }

    /// Whether an acknowledgment or an unreported immediate failure remains.
    pub fn has_pending(&self) -> bool {
        let state = self.lock();
        !state.waiting.is_empty() || !state.immediate_failures.is_empty()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path_string(path: &std::path::Path) -> String {
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn failed_presentation_write_keeps_local_choice_and_reports_once() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let value = serde_json::json!({"utility_presentations": {"version": 99, "entries": {}}});
        std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        let workers = SettingsSaveWorkerRegistry::default();
        let saved = serde_json::from_value(value["utility_presentations"].clone()).unwrap();
        let store = UtilityPresentationStore::new(path_string(&path), workers.clone(), saved);
        let key = UtilityToolKey::parse("batch-rename").unwrap();

        assert_eq!(
            store.remember(key, RememberedPresentation::Popout),
            UtilityPresentationUpdate::Pending
        );
        assert_eq!(store.preference(key), Some(RememberedPresentation::Popout));
        assert!(store.has_pending());

        workers.join_all();
        let failures = store.poll();
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].0, key);
        assert!(store.poll().is_empty());
        assert_eq!(
            store.remember(key, RememberedPresentation::Popout),
            UtilityPresentationUpdate::Unchanged,
            "an unchanged accepted choice does not retry each frame"
        );
        assert!(!store.has_pending());
        assert_eq!(store.preference(key), Some(RememberedPresentation::Popout));
    }

    #[test]
    fn presentation_acknowledgment_clears_pending_revision() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        Settings::default().save(&path_string(&path)).unwrap();
        let workers = SettingsSaveWorkerRegistry::default();
        let key = UtilityToolKey::parse("batch-rename").unwrap();
        let store = UtilityPresentationStore::new(
            path_string(&path),
            workers.clone(),
            UtilityPresentations::default(),
        );

        assert_eq!(
            store.remember(key, RememberedPresentation::Popout),
            UtilityPresentationUpdate::Pending
        );
        workers.join_all();
        assert!(store.has_pending());
        assert!(store.poll().is_empty());
        assert!(!store.has_pending());
        assert_eq!(store.preference(key), Some(RememberedPresentation::Popout));
    }

    #[test]
    fn rebind_keeps_local_choice_for_same_path_and_reloads_on_path_change() {
        let directory = tempfile::tempdir().unwrap();
        let path_a = directory.path().join("settings-a.json");
        let path_b = directory.path().join("settings-b.json");
        let key = UtilityToolKey::parse("batch-rename").unwrap();
        let mut saved_a = UtilityPresentations::default();
        saved_a.set(key, RememberedPresentation::Docked).unwrap();
        let mut saved_b = UtilityPresentations::default();
        saved_b.set(key, RememberedPresentation::Docked).unwrap();
        let mut settings_a = Settings::default();
        settings_a.utility_presentations = saved_a.clone();
        settings_a.save(&path_string(&path_a)).unwrap();
        let mut settings_b = Settings::default();
        settings_b.utility_presentations = saved_b;
        settings_b.save(&path_string(&path_b)).unwrap();

        let workers = SettingsSaveWorkerRegistry::default();
        let store = UtilityPresentationStore::new(path_string(&path_a), workers.clone(), saved_a);
        assert_eq!(
            store.remember(key, RememberedPresentation::Popout),
            UtilityPresentationUpdate::Pending
        );
        workers.join_all();

        store.rebind(path_string(&path_a), SettingsSaveWorkerRegistry::default());
        assert_eq!(
            store.preference(key),
            Some(RememberedPresentation::Popout),
            "same-path rebind retains an accepted local choice"
        );
        assert!(
            !store.has_pending(),
            "old-writer acknowledgments are discarded"
        );

        store.rebind(path_string(&path_b), SettingsSaveWorkerRegistry::default());
        assert_eq!(
            store.preference(key),
            Some(RememberedPresentation::Docked),
            "new-path rebind uses that file's saved choice"
        );
    }
}
