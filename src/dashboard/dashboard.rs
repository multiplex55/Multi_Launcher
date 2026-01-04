use crate::dashboard::config::{DashboardConfig, OverflowMode};
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

        let size = egui::vec2(ui.available_width(), ui.available_height());
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        let mut child = ui.child_ui(rect, egui::Layout::top_down(egui::Align::LEFT));
        let grid_cols = self.config.grid.cols.max(1) as usize;
        let grid_rows = self.config.grid.rows.max(1) as usize;
        let col_width = rect.width() / grid_cols.max(1) as f32;
        let row_height = rect.height() / grid_rows.max(1) as f32;

        for slot in &self.slots {
            let slot_rect = egui::Rect::from_min_size(
                rect.min + egui::vec2(col_width * slot.col as f32, row_height * slot.row as f32),
                egui::vec2(
                    col_width * slot.col_span as f32,
                    row_height * slot.row_span as f32,
                ),
            );
            let slot_clip = slot_rect.intersect(child.clip_rect());
            let response = child.allocate_ui_at_rect(slot_rect, |slot_ui| {
                slot_ui.set_clip_rect(slot_clip);
                slot_ui.set_min_size(slot_rect.size());
                self.render_slot(slot, slot_ui, ctx, activation)
            });
            clicked = clicked.or(response.inner);
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
        let heading = slot.id.as_deref().unwrap_or(&slot.widget);
        ui.set_clip_rect(ui.clip_rect().intersect(ui.max_rect()));
        ui.set_min_size(ui.available_size());
        egui::Frame::group(ui.style())
            .show(ui, |ui| {
                ui.set_min_size(ui.available_size());
                ui.vertical(|ui| {
                    ui.heading(heading);
                    let body_height =
                        (ui.available_height() - ui.spacing().item_spacing.y).max(0.0);
                match slot.overflow {
                    OverflowMode::Clip => {
                        ui.set_min_height(body_height.max(0.0));
                        self.registry
                            .create(&slot.widget, &slot.settings)
                            .and_then(|mut w| w.render(ui, ctx, activation))
                    }
                    OverflowMode::Auto | OverflowMode::Scroll => {
                        egui::ScrollArea::vertical()
                            .id_source(egui::Id::new((
                                "slot-scroll",
                                slot.id.as_deref().unwrap_or(&slot.widget),
                                slot.row,
                                slot.col,
                            )))
                            .auto_shrink([false; 2])
                            .show(ui, |ui| {
                                ui.set_min_height(body_height.max(0.0));
                                self.registry
                                    .create(&slot.widget, &slot.settings)
                                        .and_then(|mut w| w.render(ui, ctx, activation))
                                })
                                .inner
                        }
                    }
                })
                .inner
            })
            .inner
    }

    pub fn registry(&self) -> &WidgetRegistry {
        &self.registry
    }
}
