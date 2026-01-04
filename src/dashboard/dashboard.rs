use crate::dashboard::config::{DashboardConfig, OverflowMode};
use crate::dashboard::layout::{normalize_slots, NormalizedSlot};
use crate::dashboard::widgets::{WidgetAction, WidgetRegistry};
use crate::{actions::Action, common::json_watch::JsonWatcher};
use eframe::egui;
#[cfg(test)]
use once_cell::sync::Lazy;
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

        let available_size = egui::vec2(ui.available_width(), ui.available_height());
        let grid_cols = self.config.grid.cols.max(1) as usize;
        let grid_rows = self.config.grid.rows.max(1) as usize;
        let col_width = available_size.x / grid_cols.max(1) as f32;
        let row_height = available_size.y / grid_rows.max(1) as f32;
        let (rect, _) = ui.allocate_exact_size(available_size, egui::Sense::hover());
        let mut child = ui.child_ui(rect, egui::Layout::top_down(egui::Align::LEFT));

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
                self.render_slot(slot, slot_rect, slot_clip, slot_ui, ctx, activation)
            });
            clicked = clicked.or(response.inner);
        }

        clicked
    }

    fn render_slot(
        &self,
        slot: &NormalizedSlot,
        slot_rect: egui::Rect,
        slot_clip: egui::Rect,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        let heading = slot.id.as_deref().unwrap_or(&slot.widget);
        let height_id = slot_height_id(slot);
        let previous_height = ui
            .ctx()
            .data(|d| d.get_temp::<f32>(height_id))
            .unwrap_or_default();

        ui.set_clip_rect(slot_clip);
        ui.set_min_size(slot_rect.size());
        egui::Frame::group(ui.style())
            .show(ui, |ui| {
                ui.set_min_size(ui.available_size());
                ui.vertical(|ui| {
                    let heading_rect = ui.heading(heading).rect;
                    let body_height =
                        (slot_rect.height() - heading_rect.height() - ui.spacing().item_spacing.y)
                            .max(0.0);
                    let overflow = match slot.overflow {
                        OverflowMode::Clip => OverflowPolicy::Clip,
                        OverflowMode::Scroll => OverflowPolicy::Scroll,
                        OverflowMode::Auto => {
                            if previous_height > body_height {
                                OverflowPolicy::Scroll
                            } else {
                                OverflowPolicy::Clip
                            }
                        }
                    };

                    let (action, content_height) = match overflow {
                        OverflowPolicy::Clip => {
                            self.render_clipped_widget(slot, ui, ctx, activation, body_height)
                        }
                        OverflowPolicy::Scroll => self.render_scrollable_widget(
                            slot,
                            ui,
                            ctx,
                            activation,
                            body_height,
                            slot.overflow,
                        ),
                    };

                    ui.ctx()
                        .data_mut(|d| d.insert_temp(height_id, content_height));

                    action
                })
                .inner
            })
            .inner
    }

    fn render_clipped_widget(
        &self,
        slot: &NormalizedSlot,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        activation: WidgetActivation,
        body_height: f32,
    ) -> (Option<WidgetAction>, f32) {
        ui.set_min_height(body_height);
        ui.set_max_height(body_height);
        self.render_widget_content(slot, ui, ctx, activation)
    }

    fn render_scrollable_widget(
        &self,
        slot: &NormalizedSlot,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        activation: WidgetActivation,
        body_height: f32,
        overflow: OverflowMode,
    ) -> (Option<WidgetAction>, f32) {
        let mut measured_height = 0.0;
        let scroll_id = egui::Id::new((
            "slot-scroll",
            slot.id.as_deref().unwrap_or(&slot.widget),
            slot.row,
            slot.col,
        ));
        let scroll_area = egui::ScrollArea::vertical()
            .id_source(scroll_id)
            .auto_shrink([false; 2])
            .max_height(body_height)
            .enable_scrolling(matches!(
                overflow,
                OverflowMode::Scroll | OverflowMode::Auto
            ));

        let output = scroll_area.show_viewport(ui, |ui, _viewport| {
            ui.set_min_height(body_height);
            let (action, height) = self.render_widget_content(slot, ui, ctx, activation);
            measured_height = height;
            action
        });

        let content_height = output.content_size.y.max(measured_height);
        (output.inner, content_height)
    }

    fn render_widget_content(
        &self,
        slot: &NormalizedSlot,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        activation: WidgetActivation,
    ) -> (Option<WidgetAction>, f32) {
        let start_cursor = ui.cursor().min.y;
        let action = self
            .registry
            .create(&slot.widget, &slot.settings)
            .and_then(|mut w| w.render(ui, ctx, activation));
        let end_cursor = ui.cursor().max.y;
        (action, (end_cursor - start_cursor).max(0.0))
    }

    pub fn registry(&self) -> &WidgetRegistry {
        &self.registry
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OverflowPolicy {
    Clip,
    Scroll,
}

fn slot_height_id(slot: &NormalizedSlot) -> egui::Id {
    egui::Id::new((
        "dashboard-slot-height",
        slot.id.as_deref().unwrap_or(&slot.widget),
        slot.row,
        slot.col,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboard::config::{GridConfig, SlotConfig};
    use crate::dashboard::widgets::{Widget, WidgetFactory};
    use crate::plugin::PluginManager;
    use once_cell::sync::Lazy;
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default, Serialize, Deserialize)]
    struct RecordingConfig;

    #[derive(Default)]
    struct RecordingWidget;

    #[derive(Clone, Copy)]
    struct SlotRecord {
        clip: egui::Rect,
        max_rect: egui::Rect,
    }

    static RECORDS: Lazy<Mutex<Vec<SlotRecord>>> = Lazy::new(|| Mutex::new(Vec::new()));

    impl Widget for RecordingWidget {
        fn render(
            &mut self,
            ui: &mut egui::Ui,
            _ctx: &DashboardContext<'_>,
            _activation: WidgetActivation,
        ) -> Option<WidgetAction> {
            let rect = ui.max_rect();
            let clip = ui.clip_rect();
            ui.painter().rect_filled(
                rect.expand2(egui::vec2(50.0, 50.0)),
                0.0,
                egui::Color32::RED,
            );
            RECORDS.lock().unwrap().push(SlotRecord {
                clip,
                max_rect: rect,
            });
            None
        }
    }

    fn take_records() -> Vec<SlotRecord> {
        std::mem::take(&mut *RECORDS.lock().unwrap())
    }

    fn recording_registry() -> WidgetRegistry {
        let mut reg = WidgetRegistry::default();
        reg.register(
            "record",
            WidgetFactory::new(|_: RecordingConfig| RecordingWidget),
        );
        reg
    }

    fn dashboard_context<'a>(plugins: &'a PluginManager) -> DashboardContext<'a> {
        static EMPTY_USAGE: Lazy<HashMap<String, u32>> = Lazy::new(HashMap::new);
        DashboardContext {
            actions: &[],
            usage: &EMPTY_USAGE,
            plugins,
            default_location: None,
        }
    }

    fn dashboard_with_config(path: &Path, registry: WidgetRegistry) -> Dashboard {
        let mut dashboard = Dashboard::new(path, registry, None);
        dashboard.attach_watcher();
        dashboard
    }

    #[test]
    fn widget_paint_is_clipped_to_slot() {
        let cfg = DashboardConfig {
            version: 1,
            grid: GridConfig { rows: 1, cols: 1 },
            slots: vec![SlotConfig::with_widget("record", 0, 0)],
        };
        let tmp = tempfile::NamedTempFile::new().unwrap();
        cfg.save(tmp.path()).unwrap();

        let registry = recording_registry();
        let mut dashboard = dashboard_with_config(tmp.path(), registry);
        let plugins = PluginManager::new();
        let ctx = dashboard_context(&plugins);

        egui::__run_test_ui(|ui| {
            let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(240.0, 180.0));
            ui.allocate_ui_at_rect(rect, |ui| {
                dashboard.ui(ui, &ctx, WidgetActivation::Click);
            });
        });

        let records = take_records();
        assert_eq!(records.len(), 1);
        let record = records[0];
        let expected_clip = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(240.0, 180.0));
        assert_eq!(record.clip, expected_clip);
        assert!(rect_contains(record.clip, record.max_rect));
    }

    #[test]
    fn slot_rects_do_not_overlap() {
        let cfg = DashboardConfig {
            version: 1,
            grid: GridConfig { rows: 1, cols: 2 },
            slots: vec![
                SlotConfig::with_widget("record", 0, 0),
                SlotConfig::with_widget("record", 0, 1),
            ],
        };
        let tmp = tempfile::NamedTempFile::new().unwrap();
        cfg.save(tmp.path()).unwrap();

        let registry = recording_registry();
        let mut dashboard = dashboard_with_config(tmp.path(), registry);
        let plugins = PluginManager::new();
        let ctx = dashboard_context(&plugins);

        egui::__run_test_ui(|ui| {
            let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(300.0, 200.0));
            ui.allocate_ui_at_rect(rect, |ui| {
                dashboard.ui(ui, &ctx, WidgetActivation::Click);
            });
        });

        let records = take_records();
        assert_eq!(records.len(), 2);
        let a = records[0].clip;
        let b = records[1].clip;
        assert!((a.max.x - b.min.x).abs() < f32::EPSILON || !a.intersects(b));
        assert_eq!(a.min.y, b.min.y);
        assert_eq!(a.height(), b.height());
    }

    #[test]
    fn auto_overflow_scrolls_when_needed() {
        let cfg = DashboardConfig {
            version: 1,
            grid: GridConfig { rows: 1, cols: 1 },
            slots: vec![SlotConfig {
                overflow: OverflowMode::Auto,
                ..SlotConfig::with_widget("record", 0, 0)
            }],
        };
        let tmp = tempfile::NamedTempFile::new().unwrap();
        cfg.save(tmp.path()).unwrap();

        let registry = recording_registry();
        let mut dashboard = dashboard_with_config(tmp.path(), registry);
        let plugins = PluginManager::new();
        let ctx = dashboard_context(&plugins);

        egui::__run_test_ui(|ui| {
            let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(200.0, 80.0));
            ui.allocate_ui_at_rect(rect, |ui| {
                dashboard.ui(ui, &ctx, WidgetActivation::Click);
            });
        });

        let records = take_records();
        assert_eq!(records.len(), 1);
        let record = records[0];
        let expected_clip = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(200.0, 80.0));
        assert_eq!(record.clip, expected_clip);
        assert!(rect_contains(record.clip, record.max_rect));
    }

    fn rect_contains(outer: egui::Rect, inner: egui::Rect) -> bool {
        outer.min.x <= inner.min.x
            && outer.min.y <= inner.min.y
            && outer.max.x >= inner.max.x
            && outer.max.y >= inner.max.y
    }
}
