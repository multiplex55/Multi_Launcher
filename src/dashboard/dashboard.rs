use crate::dashboard::config::{DashboardConfig, OverflowMode};
use crate::dashboard::data_cache::DashboardDataCache;
use crate::dashboard::layout::{normalize_slots, NormalizedSlot};
use crate::dashboard::widgets::{Widget, WidgetAction, WidgetRegistry};
use crate::{actions::Action, common::json_watch::JsonWatcher};
use eframe::egui;
use eframe::egui::scroll_area::ScrollBarVisibility;
#[cfg(test)]
use once_cell::sync::Lazy;
use serde_json;
use siphasher::sip::SipHasher24;
use std::collections::HashMap;
use std::hash::Hasher;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::Mutex;

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
    pub actions_by_id: &'a std::collections::HashMap<String, Action>,
    pub usage: &'a std::collections::HashMap<String, u32>,
    pub plugins: &'a crate::plugin::PluginManager,
    pub enabled_plugins: Option<&'a std::collections::HashSet<String>>,
    pub default_location: Option<&'a str>,
    pub data_cache: &'a DashboardDataCache,
    pub actions_version: u64,
    pub fav_version: u64,
    pub notes_version: u64,
    pub todo_version: u64,
    pub calendar_version: u64,
    pub clipboard_version: u64,
    pub snippets_version: u64,
    pub dashboard_visible: bool,
    pub dashboard_focused: bool,
    pub reduce_dashboard_work_when_unfocused: bool,
}

struct SlotRuntime {
    slot: NormalizedSlot,
    hash: u64,
    widget: Box<dyn Widget>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum SlotKey {
    Id {
        id: String,
        widget: String,
    },
    Position {
        widget: String,
        row: usize,
        col: usize,
    },
}

impl SlotKey {
    fn from_slot(slot: &NormalizedSlot) -> Self {
        if let Some(id) = &slot.id {
            SlotKey::Id {
                id: id.clone(),
                widget: slot.widget.clone(),
            }
        } else {
            SlotKey::Position {
                widget: slot.widget.clone(),
                row: slot.row,
                col: slot.col,
            }
        }
    }
}

fn slot_hash(slot: &NormalizedSlot) -> u64 {
    let mut hasher = SipHasher24::new_with_keys(0, 0);
    hasher.write(slot.widget.as_bytes());
    hasher.write_u64(slot.row as u64);
    hasher.write_u64(slot.col as u64);
    hasher.write_u64(slot.row_span as u64);
    hasher.write_u64(slot.col_span as u64);
    hasher.write(slot.overflow.as_str().as_bytes());
    if let Ok(bytes) = serde_json::to_vec(&slot.settings) {
        hasher.write(&bytes);
    }
    hasher.finish()
}

pub struct Dashboard {
    config_path: PathBuf,
    pub config: DashboardConfig,
    pub slots: Vec<NormalizedSlot>,
    runtime_slots: Vec<SlotRuntime>,
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
        let mut dashboard = Self {
            config_path: path,
            config,
            slots: Vec::new(),
            runtime_slots: Vec::new(),
            registry,
            watcher: None,
            warnings,
            event_cb,
        };
        dashboard.rebuild_runtime_slots(slots);
        dashboard
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

    fn rebuild_runtime_slots(&mut self, slots: Vec<NormalizedSlot>) {
        let mut reusable: HashMap<SlotKey, SlotRuntime> = self
            .runtime_slots
            .drain(..)
            .map(|rt| (SlotKey::from_slot(&rt.slot), rt))
            .collect();

        let mut runtime_slots = Vec::with_capacity(slots.len());
        for slot in &slots {
            let new_hash = slot_hash(slot);
            let key = SlotKey::from_slot(slot);
            if let Some(mut runtime) = reusable.remove(&key) {
                if runtime.hash != new_hash {
                    runtime.widget.on_config_updated(&slot.settings);
                }
                runtime.slot = slot.clone();
                runtime.hash = new_hash;
                runtime_slots.push(runtime);
            } else if let Some(widget) = self.registry.create(&slot.widget, &slot.settings) {
                runtime_slots.push(SlotRuntime {
                    slot: slot.clone(),
                    hash: new_hash,
                    widget,
                });
            }
        }

        self.slots = slots;
        self.runtime_slots = runtime_slots;
    }

    pub fn reload(&mut self) {
        let (cfg, slots, warnings) = Self::load_internal(&self.config_path, &self.registry);
        self.config = cfg;
        self.warnings = warnings;
        self.rebuild_runtime_slots(slots);
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

        for slot in &mut self.runtime_slots {
            let normalized = &slot.slot;
            let slot_rect = egui::Rect::from_min_size(
                rect.min
                    + egui::vec2(
                        col_width * normalized.col as f32,
                        row_height * normalized.row as f32,
                    ),
                egui::vec2(
                    col_width * normalized.col_span as f32,
                    row_height * normalized.row_span as f32,
                ),
            );
            let slot_clip = slot_rect.intersect(child.clip_rect());
            let response = child.allocate_ui_at_rect(slot_rect, |slot_ui| {
                slot_ui.set_clip_rect(slot_clip);
                slot_ui.set_min_size(slot_rect.size());
                Self::render_slot(slot, slot_rect, slot_clip, slot_ui, ctx, activation)
            });
            clicked = clicked.or(response.inner);
        }

        clicked
    }

    fn render_slot(
        slot: &mut SlotRuntime,
        slot_rect: egui::Rect,
        slot_clip: egui::Rect,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        let heading = slot
            .slot
            .id
            .clone()
            .unwrap_or_else(|| slot.slot.widget.clone());

        ui.set_clip_rect(slot_clip);
        ui.set_min_size(slot_rect.size());
        egui::Frame::group(ui.style())
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    let mut header_action = None;
                    let heading_rect = ui
                        .horizontal(|ui| {
                            let resp = ui.heading(&heading);
                            header_action = slot.widget.header_ui(ui, ctx);
                            resp.rect
                        })
                        .inner;
                    let header_height = heading_rect.height();
                    let body_height =
                        (slot_rect.height() - header_height - ui.spacing().item_spacing.y).max(0.0);
                    let overflow = match slot.slot.overflow {
                        OverflowMode::Clip => OverflowPolicy::Clip,
                        OverflowMode::Scroll => OverflowPolicy::Scroll {
                            visibility: ScrollBarVisibility::AlwaysVisible,
                        },
                        OverflowMode::Auto => OverflowPolicy::Scroll {
                            visibility: ScrollBarVisibility::VisibleWhenNeeded,
                        },
                    };

                    let action = match overflow {
                        OverflowPolicy::Clip => {
                            Self::render_clipped_widget(slot, ui, ctx, activation, body_height)
                        }
                        OverflowPolicy::Scroll { visibility } => Self::render_scrollable_widget(
                            slot,
                            ui,
                            ctx,
                            activation,
                            body_height,
                            slot_clip,
                            visibility,
                        ),
                    };

                    header_action.or(action)
                })
                .inner
            })
            .inner
    }

    fn render_clipped_widget(
        slot: &mut SlotRuntime,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        activation: WidgetActivation,
        body_height: f32,
    ) -> Option<WidgetAction> {
        ui.set_min_height(body_height);
        ui.set_max_height(body_height);
        Self::render_widget_content(slot, ui, ctx, activation)
    }

    fn render_scrollable_widget(
        slot: &mut SlotRuntime,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        activation: WidgetActivation,
        body_height: f32,
        slot_clip: egui::Rect,
        visibility: ScrollBarVisibility,
    ) -> Option<WidgetAction> {
        let scroll_id = egui::Id::new((
            "slot-scroll",
            slot.slot.id.as_deref().unwrap_or(&slot.slot.widget),
            slot.slot.row,
            slot.slot.col,
        ));
        let scroll_area = egui::ScrollArea::both()
            .id_source(scroll_id)
            .auto_shrink([false; 2])
            .max_height(body_height)
            .scroll_bar_visibility(visibility);
        #[cfg(test)]
        SCROLL_VISIBILITY_RECORDS.lock().unwrap().push(visibility);

        scroll_area
            .show_viewport(ui, |ui, viewport| {
                #[cfg(test)]
                SCROLL_VIEWPORTS.lock().unwrap().push(viewport);
                #[cfg(not(test))]
                let _ = viewport;
                ui.set_clip_rect(ui.clip_rect().intersect(slot_clip));
                ui.set_min_height(body_height);
                Self::render_widget_content(slot, ui, ctx, activation)
            })
            .inner
    }

    fn render_widget_content(
        slot: &mut SlotRuntime,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        slot.widget.render(ui, ctx, activation)
    }

    pub fn registry(&self) -> &WidgetRegistry {
        &self.registry
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OverflowPolicy {
    Clip,
    Scroll { visibility: ScrollBarVisibility },
}

#[cfg(test)]
static SCROLL_VISIBILITY_RECORDS: Lazy<Mutex<Vec<ScrollBarVisibility>>> =
    Lazy::new(|| Mutex::new(Vec::new()));
#[cfg(test)]
static SCROLL_VIEWPORTS: Lazy<Mutex<Vec<egui::Rect>>> = Lazy::new(|| Mutex::new(Vec::new()));

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboard::config::{GridConfig, SlotConfig};
    use crate::dashboard::widgets::{Widget, WidgetFactory};
    use crate::plugin::PluginManager;
    use once_cell::sync::Lazy;
    use serde::{Deserialize, Serialize};
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    #[derive(Default, Serialize, Deserialize)]
    struct RecordingConfig;

    #[derive(Default)]
    struct RecordingWidget;

    #[derive(Default, Serialize, Deserialize, Clone)]
    struct UpdatingConfig {
        label: String,
    }

    #[derive(Default)]
    struct UpdatingWidget {
        label: String,
    }

    #[derive(Default)]
    struct OverflowWidget;

    #[derive(Clone, Copy)]
    struct SlotRecord {
        clip: egui::Rect,
    }

    static RECORDS: Lazy<Mutex<Vec<SlotRecord>>> = Lazy::new(|| Mutex::new(Vec::new()));
    static RENDERS: Lazy<Mutex<Vec<String>>> = Lazy::new(|| Mutex::new(Vec::new()));
    static OVERFLOW_RECORDS: Lazy<Mutex<Vec<egui::Rect>>> = Lazy::new(|| Mutex::new(Vec::new()));
    static CREATED: AtomicUsize = AtomicUsize::new(0);
    static UPDATED: AtomicUsize = AtomicUsize::new(0);

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
            RECORDS.lock().unwrap().push(SlotRecord { clip });
            None
        }
    }

    impl Widget for UpdatingWidget {
        fn render(
            &mut self,
            _ui: &mut egui::Ui,
            _ctx: &DashboardContext<'_>,
            _activation: WidgetActivation,
        ) -> Option<WidgetAction> {
            RENDERS.lock().unwrap().push(self.label.clone());
            None
        }

        fn on_config_updated(&mut self, settings: &serde_json::Value) {
            if let Ok(cfg) = serde_json::from_value::<UpdatingConfig>(settings.clone()) {
                self.label = cfg.label;
                UPDATED.fetch_add(1, Ordering::SeqCst);
            }
        }
    }

    impl Widget for OverflowWidget {
        fn render(
            &mut self,
            ui: &mut egui::Ui,
            _ctx: &DashboardContext<'_>,
            _activation: WidgetActivation,
        ) -> Option<WidgetAction> {
            let (_, rect) = ui.allocate_space(egui::vec2(320.0, 180.0));
            OVERFLOW_RECORDS.lock().unwrap().push(rect);
            None
        }
    }

    fn take_records() -> Vec<SlotRecord> {
        std::mem::take(&mut *RECORDS.lock().unwrap())
    }

    fn take_renders() -> Vec<String> {
        std::mem::take(&mut *RENDERS.lock().unwrap())
    }

    fn take_scroll_visibilities() -> Vec<ScrollBarVisibility> {
        std::mem::take(&mut *SCROLL_VISIBILITY_RECORDS.lock().unwrap())
    }

    fn take_scroll_viewports() -> Vec<egui::Rect> {
        std::mem::take(&mut *SCROLL_VIEWPORTS.lock().unwrap())
    }

    fn take_overflow_records() -> Vec<egui::Rect> {
        std::mem::take(&mut *OVERFLOW_RECORDS.lock().unwrap())
    }

    fn recording_registry() -> WidgetRegistry {
        let mut reg = WidgetRegistry::default();
        reg.register(
            "record",
            WidgetFactory::new(|_: RecordingConfig| RecordingWidget),
        );
        reg
    }

    fn overflow_registry() -> WidgetRegistry {
        let mut reg = WidgetRegistry::default();
        reg.register(
            "overflow",
            WidgetFactory::new(|_: RecordingConfig| OverflowWidget),
        );
        reg
    }

    fn updating_registry() -> WidgetRegistry {
        let mut reg = WidgetRegistry::default();
        reg.register(
            "updating",
            WidgetFactory::new(|cfg: UpdatingConfig| {
                CREATED.fetch_add(1, Ordering::SeqCst);
                UpdatingWidget {
                    label: cfg.label.clone(),
                }
            }),
        );
        reg
    }

    fn dashboard_context<'a>(
        plugins: &'a PluginManager,
        data_cache: &'a DashboardDataCache,
    ) -> DashboardContext<'a> {
        static EMPTY_USAGE: Lazy<HashMap<String, u32>> = Lazy::new(HashMap::new);
        static EMPTY_ACTIONS_BY_ID: Lazy<HashMap<String, Action>> = Lazy::new(HashMap::new);
        DashboardContext {
            actions: &[],
            actions_by_id: &EMPTY_ACTIONS_BY_ID,
            usage: &EMPTY_USAGE,
            plugins,
            enabled_plugins: None,
            default_location: None,
            data_cache,
            actions_version: 0,
            fav_version: 0,
            notes_version: 0,
            todo_version: 0,
            calendar_version: 0,
            clipboard_version: 0,
            snippets_version: 0,
            dashboard_visible: true,
            dashboard_focused: true,
            reduce_dashboard_work_when_unfocused: false,
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
        let data_cache = DashboardDataCache::new();
        let ctx = dashboard_context(&plugins, &data_cache);

        egui::__run_test_ui(|ui| {
            let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(240.0, 180.0));
            ui.allocate_ui_at_rect(rect, |ui| {
                dashboard.ui(ui, &ctx, WidgetActivation::Click);
            });
        });

        let records = take_records();
        assert_eq!(records.len(), 1);
        let record = records[0];
        let slot_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(240.0, 180.0));
        assert!(record.clip.min.x >= slot_rect.min.x - f32::EPSILON);
        assert!(record.clip.min.y >= slot_rect.min.y - f32::EPSILON);
        assert!(record.clip.max.x <= slot_rect.max.x + f32::EPSILON);
        assert!(record.clip.max.y <= slot_rect.max.y + f32::EPSILON);
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
        let data_cache = DashboardDataCache::new();
        let ctx = dashboard_context(&plugins, &data_cache);

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
        take_scroll_visibilities();
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
        let data_cache = DashboardDataCache::new();
        let ctx = dashboard_context(&plugins, &data_cache);

        egui::__run_test_ui(|ui| {
            let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(200.0, 80.0));
            ui.allocate_ui_at_rect(rect, |ui| {
                dashboard.ui(ui, &ctx, WidgetActivation::Click);
            });
        });

        let records = take_records();
        assert_eq!(records.len(), 1);
        let record = records[0];
        let slot_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(200.0, 80.0));
        assert!(record.clip.min.x >= slot_rect.min.x - f32::EPSILON);
        assert!(record.clip.min.y >= slot_rect.min.y - f32::EPSILON);
        assert!(record.clip.max.x <= slot_rect.max.x + f32::EPSILON);
        assert!(record.clip.max.y <= slot_rect.max.y + f32::EPSILON);
        assert_eq!(
            take_scroll_visibilities(),
            vec![ScrollBarVisibility::VisibleWhenNeeded]
        );
    }

    #[test]
    fn clip_overflow_clips_without_scroll() {
        take_scroll_visibilities();
        let cfg = DashboardConfig {
            version: 1,
            grid: GridConfig { rows: 1, cols: 1 },
            slots: vec![SlotConfig {
                overflow: OverflowMode::Clip,
                ..SlotConfig::with_widget("record", 0, 0)
            }],
        };
        let tmp = tempfile::NamedTempFile::new().unwrap();
        cfg.save(tmp.path()).unwrap();

        let registry = recording_registry();
        let mut dashboard = dashboard_with_config(tmp.path(), registry);
        let plugins = PluginManager::new();
        let data_cache = DashboardDataCache::new();
        let ctx = dashboard_context(&plugins, &data_cache);

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
        assert!(take_scroll_visibilities().is_empty());
    }

    #[test]
    fn scroll_area_allows_horizontal_and_vertical_overflow() {
        take_scroll_visibilities();
        take_scroll_viewports();
        take_overflow_records();
        let cfg = DashboardConfig {
            version: 1,
            grid: GridConfig { rows: 1, cols: 1 },
            slots: vec![SlotConfig {
                overflow: OverflowMode::Scroll,
                ..SlotConfig::with_widget("overflow", 0, 0)
            }],
        };
        let tmp = tempfile::NamedTempFile::new().unwrap();
        cfg.save(tmp.path()).unwrap();

        let registry = overflow_registry();
        let mut dashboard = dashboard_with_config(tmp.path(), registry);
        let plugins = PluginManager::new();
        let data_cache = DashboardDataCache::new();
        let ctx = dashboard_context(&plugins, &data_cache);

        egui::__run_test_ui(|ui| {
            let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(120.0, 80.0));
            ui.allocate_ui_at_rect(rect, |ui| {
                dashboard.ui(ui, &ctx, WidgetActivation::Click);
            });
        });

        let viewports = take_scroll_viewports();
        let overflow_records = take_overflow_records();
        assert_eq!(viewports.len(), 1);
        assert_eq!(overflow_records.len(), 1);
        let viewport = viewports[0];
        let overflow = overflow_records[0];
        assert!(overflow.width() > viewport.width());
        assert!(overflow.height() > viewport.height());
        assert_eq!(
            take_scroll_visibilities(),
            vec![ScrollBarVisibility::AlwaysVisible]
        );
    }

    #[test]
    fn reuses_widget_instances_on_reload() {
        CREATED.store(0, Ordering::SeqCst);
        UPDATED.store(0, Ordering::SeqCst);
        take_renders();

        let cfg = DashboardConfig {
            version: 1,
            grid: GridConfig { rows: 1, cols: 1 },
            slots: vec![SlotConfig {
                settings: json!({ "label": "first" }),
                ..SlotConfig::with_widget("updating", 0, 0)
            }],
        };
        let tmp = tempfile::NamedTempFile::new().unwrap();
        cfg.save(tmp.path()).unwrap();

        let registry = updating_registry();
        let mut dashboard = dashboard_with_config(tmp.path(), registry);
        let plugins = PluginManager::new();
        let data_cache = DashboardDataCache::new();
        let ctx = dashboard_context(&plugins, &data_cache);

        egui::__run_test_ui(|ui| {
            let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(200.0, 80.0));
            ui.allocate_ui_at_rect(rect, |ui| {
                dashboard.ui(ui, &ctx, WidgetActivation::Click);
            });
        });

        assert_eq!(CREATED.load(Ordering::SeqCst), 1);
        assert_eq!(UPDATED.load(Ordering::SeqCst), 0);
        assert_eq!(take_renders(), vec!["first".to_string()]);

        let updated_cfg = DashboardConfig {
            version: 1,
            grid: GridConfig { rows: 1, cols: 1 },
            slots: vec![SlotConfig {
                settings: json!({ "label": "second" }),
                ..SlotConfig::with_widget("updating", 0, 0)
            }],
        };
        updated_cfg.save(tmp.path()).unwrap();

        dashboard.reload();

        egui::__run_test_ui(|ui| {
            let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(200.0, 80.0));
            ui.allocate_ui_at_rect(rect, |ui| {
                dashboard.ui(ui, &ctx, WidgetActivation::Click);
            });
        });

        assert_eq!(CREATED.load(Ordering::SeqCst), 1);
        assert_eq!(UPDATED.load(Ordering::SeqCst), 1);
        assert_eq!(take_renders(), vec!["second".to_string()]);
    }
}
