//! App-owned remembered presentation choices, separate from live host state.
use crate::settings::{RememberedPresentation, UtilityPresentations, UtilityToolKey};
use crate::settings_save::SettingsSaveWorkerRegistry;
use eframe::egui::{self, Context};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const CAPABILITY: &str = "multi_launcher.utility_presentation_capability";

#[derive(Clone)]
pub(crate) struct UtilityPresentationManager(Arc<Mutex<State>>);

struct State {
    path: String,
    workers: SettingsSaveWorkerRegistry,
    saved: UtilityPresentations,
    local: HashMap<UtilityToolKey, RememberedPresentation>,
    waiting: HashMap<UtilityToolKey, u64>,
    reported: HashMap<UtilityToolKey, u64>,
    immediate_failures: Vec<(UtilityToolKey, String)>,
}

impl UtilityPresentationManager {
    pub(crate) fn new(
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

    pub(crate) fn install_in_context(&self, ctx: &Context) {
        ctx.data_mut(|data| data.insert_temp(egui::Id::new(CAPABILITY), self.clone()));
    }

    pub(crate) fn for_context(ctx: &Context) -> Option<Self> {
        ctx.data(|data| data.get_temp::<Self>(egui::Id::new(CAPABILITY)))
    }

    pub(crate) fn rebind(&self, path: String, workers: SettingsSaveWorkerRegistry) {
        let mut state = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.path != path {
            state.saved = crate::settings::Settings::load(&path)
                .map(|settings| settings.utility_presentations)
                .unwrap_or_default();
            state.local.clear();
            state.immediate_failures.clear();
        }
        // Acknowledgements belong to the old registry and cannot arrive here.
        state.waiting.clear();
        state.reported.clear();
        state.path = path;
        state.workers = workers;
    }

    pub(crate) fn preference(&self, key: UtilityToolKey) -> Option<RememberedPresentation> {
        let state = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state
            .local
            .get(&key)
            .copied()
            .or_else(|| state.saved.get(key))
    }

    pub(crate) fn remember(
        &self,
        ctx: &Context,
        key: UtilityToolKey,
        value: RememberedPresentation,
    ) {
        let mut state = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state
            .local
            .get(&key)
            .copied()
            .or_else(|| state.saved.get(key))
            == Some(value)
        {
            return;
        }
        // Accepted in-session intent survives a failed or unsupported write.
        // Keep acceptance and submission ordered if multiple hosts share a key.
        state.local.insert(key, value);
        match state
            .workers
            .queue_utility_presentation(state.path.clone(), key, value)
        {
            Ok(revision) => {
                state.waiting.insert(key, revision);
                ctx.request_repaint_after(Duration::from_millis(16));
            }
            Err(error) => {
                state.immediate_failures.push((key, error));
                ctx.request_repaint();
            }
        }
    }

    pub(crate) fn begin_frame(&self, ctx: &Context) -> Vec<(UtilityToolKey, String)> {
        let mut state = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
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
            if let Some((failed, error)) = state.workers.utility_presentation_failure(key) {
                if failed == revision {
                    state.waiting.remove(&key);
                    if state.reported.insert(key, revision) != Some(revision) {
                        failures.push((key, error));
                    }
                }
            }
        }
        if !state.waiting.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        failures
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_presentation_write_keeps_local_choice_and_reports_once() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let value = serde_json::json!({"utility_presentations": {"version": 99, "entries": {}}});
        std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        let workers = SettingsSaveWorkerRegistry::default();
        let saved = serde_json::from_value(value["utility_presentations"].clone()).unwrap();
        let manager = UtilityPresentationManager::new(
            path.to_string_lossy().into_owned(),
            workers.clone(),
            saved,
        );
        let ctx = Context::default();
        let key = UtilityToolKey::parse("batch-rename").unwrap();
        manager.remember(&ctx, key, RememberedPresentation::Popout);
        assert_eq!(
            manager.preference(key),
            Some(RememberedPresentation::Popout)
        );
        workers.join_all();
        let failures = manager.begin_frame(&ctx);
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].0, key);
        assert!(manager.begin_frame(&ctx).is_empty());
        manager.remember(&ctx, key, RememberedPresentation::Popout);
        assert!(
            manager.begin_frame(&ctx).is_empty(),
            "unchanged accepted choice does not retry each frame"
        );
        assert_eq!(
            manager.preference(key),
            Some(RememberedPresentation::Popout)
        );
    }
}
