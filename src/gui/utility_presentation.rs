//! Egui context binding and repaint policy for remembered utility presentation.
use crate::settings::{RememberedPresentation, UtilityToolKey};
use crate::settings_save::SettingsSaveWorkerRegistry;
use crate::utility_presentation::UtilityPresentationStore;
use eframe::egui::{self, Context};
use std::time::Duration;

const CAPABILITY: &str = "multi_launcher.utility_presentation_capability";

#[derive(Clone)]
pub(crate) struct UtilityPresentationManager(UtilityPresentationStore);

impl UtilityPresentationManager {
    pub(crate) fn new(
        path: String,
        workers: SettingsSaveWorkerRegistry,
        saved: crate::settings::UtilityPresentations,
    ) -> Self {
        Self(UtilityPresentationStore::new(path, workers, saved))
    }

    pub(crate) fn install_in_context(&self, ctx: &Context) {
        ctx.data_mut(|data| data.insert_temp(egui::Id::new(CAPABILITY), self.clone()));
    }

    pub(crate) fn for_context(ctx: &Context) -> Option<Self> {
        ctx.data(|data| data.get_temp::<Self>(egui::Id::new(CAPABILITY)))
    }

    pub(crate) fn rebind(&self, path: String, workers: SettingsSaveWorkerRegistry) {
        self.0.rebind(path, workers);
    }

    pub(crate) fn preference(&self, key: UtilityToolKey) -> Option<RememberedPresentation> {
        self.0.preference(key)
    }

    pub(crate) fn remember(
        &self,
        ctx: &Context,
        key: UtilityToolKey,
        value: RememberedPresentation,
    ) {
        match self.0.remember(key, value) {
            crate::utility_presentation::UtilityPresentationUpdate::Unchanged => {}
            crate::utility_presentation::UtilityPresentationUpdate::Pending => {
                ctx.request_repaint_after(Duration::from_millis(16));
            }
            crate::utility_presentation::UtilityPresentationUpdate::ImmediateFailure => {
                ctx.request_repaint();
            }
        }
    }

    pub(crate) fn begin_frame(&self, ctx: &Context) -> Vec<(UtilityToolKey, String)> {
        let failures = self.0.poll();
        if self.0.has_pending() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        failures
    }
}
