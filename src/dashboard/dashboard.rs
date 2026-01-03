use crate::dashboard::config::DashboardConfig;
use crate::dashboard::layout::{normalize_slots, NormalizedSlot};
use crate::dashboard::widgets::{WidgetAction, WidgetRegistry};
use crate::{actions::Action, common::json_watch::JsonWatcher};
use eframe::egui;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DashboardEvent {
    Reloaded,
}

/// Source of a widget activation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WidgetActivation {
    Click,
    Keyboard,
}

/// Context shared with widgets at render time.
pub struct DashboardContext<'a> {
    pub actions: &'a [Action],
    pub usage: &'a std::collections::HashMap<String, u32>,
    pub plugins: &'a crate::plugin::PluginManager,
    pub default_location: Option<&'a str>,
}

pub struct Dashboard {
    config_path: PathBuf,
    pub config: DashboardConfig,
    pub slots: Vec<NormalizedSlot>,
    registry: WidgetRegistry,
    watcher: Option<JsonWatcher>,
    pub warnings: Vec<String>,
    event_cb: Option<std::sync::Arc<dyn Fn(DashboardEvent) + Send + Sync>>,
}

impl Dashboard {
    pub fn new(
        config_path: impl AsRef<Path>,
        registry: WidgetRegistry,
        event_cb: Option<std::sync::Arc<dyn Fn(DashboardEvent) + Send + Sync>>,
    ) -> Self {
        let path = config_path.as_ref().to_path_buf();
        let (config, slots, warnings) = Self::load_internal(&path, &registry);
        Self {
            config_path: path,
            config,
            slots,
            registry,
            watcher: None,
            warnings,
            event_cb,
        }
    }

    fn load_internal(
        path: &Path,
        registry: &WidgetRegistry,
    ) -> (DashboardConfig, Vec<NormalizedSlot>, Vec<String>) {
        let cfg = DashboardConfig::load(path, registry).unwrap_or_default();
        let (slots, mut warnings) = normalize_slots(&cfg, registry);
        if slots.is_empty() {
            warnings.push("dashboard has no valid slots".into());
        }
        (cfg, slots, warnings)
    }

    pub fn reload(&mut self) {
        let (cfg, slots, warnings) = Self::load_internal(&self.config_path, &self.registry);
        self.config = cfg;
        self.slots = slots;
        self.warnings = warnings;
    }

    pub fn set_path(&mut self, path: impl AsRef<Path>) {
        self.config_path = path.as_ref().to_path_buf();
        self.reload();
        self.attach_watcher();
    }

    pub fn attach_watcher(&mut self) {
        let path = self.config_path.clone();
        let tx = self.event_cb.clone();
        self.watcher = crate::common::json_watch::watch_json(path.clone(), move || {
            tracing::info!("dashboard config changed");
            if let Some(tx) = &tx {
                (tx)(DashboardEvent::Reloaded);
            }
        })
        .ok();
    }

    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        let mut clicked = None;
        let grid_cols = self.config.grid.cols.max(1) as usize;
        let col_width = ui.available_width() / grid_cols.max(1) as f32;

        for slot in &self.slots {
            let rect = egui::Rect::from_min_size(
                ui.min_rect().min
                    + egui::vec2(
                        col_width * slot.col as f32,
                        (slot.row as f32) * 100.0, // coarse row height
                    ),
                egui::vec2(
                    col_width * slot.col_span as f32,
                    90.0 * slot.row_span as f32,
                ),
            );
            let mut child = ui.child_ui(rect, egui::Layout::left_to_right(egui::Align::TOP));
            if let Some(action) = self.render_slot(slot, &mut child, ctx, activation) {
                clicked = Some(action);
            }
        }

        clicked
    }

    fn render_slot(
        &self,
        slot: &NormalizedSlot,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        ui.group(|ui| {
            ui.vertical(|ui| {
                let heading = slot.id.as_deref().unwrap_or(&slot.widget);
                ui.heading(heading);
                self.registry
                    .create(&slot.widget, &slot.settings)
                    .and_then(|mut w| w.render(ui, ctx, activation))
            })
            .inner
        })
        .inner
    }

    pub fn registry(&self) -> &WidgetRegistry {
        &self.registry
    }
}
