use super::{
    edit_typed_settings, find_plugin, refresh_interval_setting, TimedCache, Widget, WidgetAction,
    WidgetSettingsContext, WidgetSettingsUiResult,
};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use eframe::egui;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

fn default_limit_per_source() -> usize {
    4
}

fn default_refresh_interval() -> f32 {
    10.0
}

fn default_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GroupKind {
    QuickActions,
    Continuity,
    TaskTime,
    SystemGlance,
    Workspace,
    Utilities,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Plugin,
    Usage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionSourceConfig {
    pub label: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub plugin: Option<String>,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default = "SourceKind::Plugin")]
    pub kind: SourceKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionGroupConfig {
    pub kind: GroupKind,
    #[serde(default = "default_limit_per_source")]
    pub limit_per_source: usize,
    #[serde(default)]
    pub filter: Option<String>,
    #[serde(default = "default_refresh_interval")]
    pub refresh_interval_secs: f32,
    #[serde(default)]
    pub sources: Vec<ActionSourceConfig>,
}

impl ActionGroupConfig {
    pub fn defaults_for(kind: GroupKind) -> Self {
        Self {
            kind,
            limit_per_source: default_limit_per_source(),
            filter: None,
            refresh_interval_secs: default_refresh_interval(),
            sources: default_sources(kind),
        }
    }

    fn ensure_kind(&mut self, kind: GroupKind) {
        self.kind = kind;
        if self.sources.is_empty() {
            self.sources = default_sources(kind);
        }
    }
}

#[derive(Clone, Default)]
struct SourceResult {
    label: String,
    actions: Vec<Action>,
}

pub struct ActionGroupWidget {
    cfg: ActionGroupConfig,
    cache: TimedCache<Vec<SourceResult>>,
    errors: Vec<String>,
}

impl ActionGroupWidget {
    pub fn descriptor(kind: GroupKind) -> super::WidgetDescriptor {
        use std::sync::Arc;
        let default_cfg = ActionGroupConfig::defaults_for(kind);
        let default_value =
            serde_json::to_value(&default_cfg).unwrap_or_else(|_| serde_json::json!({}));
        super::WidgetDescriptor {
            ctor: Arc::new(move |v: &Value| {
                let mut cfg: ActionGroupConfig = serde_json::from_value(v.clone())
                    .unwrap_or_else(|_| ActionGroupConfig::defaults_for(kind));
                cfg.ensure_kind(kind);
                Box::new(Self::new(cfg))
            }),
            default_settings: Arc::new(move || default_value.clone()),
            settings_ui: Some(Self::settings_ui),
        }
    }

    pub fn new(cfg: ActionGroupConfig) -> Self {
        let mut cfg = cfg;
        cfg.ensure_kind(cfg.kind);
        let interval = Duration::from_secs_f32(cfg.refresh_interval_secs.max(1.0));
        Self {
            cfg,
            cache: TimedCache::new(Vec::new(), interval),
            errors: Vec::new(),
        }
    }

    pub fn settings_ui(
        ui: &mut egui::Ui,
        value: &mut serde_json::Value,
        ctx: &WidgetSettingsContext<'_>,
    ) -> WidgetSettingsUiResult {
        edit_typed_settings(ui, value, ctx, |ui, cfg: &mut ActionGroupConfig, _ctx| {
            cfg.ensure_kind(cfg.kind);
            let mut changed = false;
            ui.horizontal(|ui| {
                ui.label("Max actions per source");
                changed |= ui
                    .add(egui::DragValue::new(&mut cfg.limit_per_source).clamp_range(1..=20))
                    .changed();
            });
            ui.horizontal(|ui| {
                ui.label("Filter contains");
                let mut filter = cfg.filter.clone().unwrap_or_default();
                if ui.text_edit_singleline(&mut filter).changed() {
                    cfg.filter = if filter.trim().is_empty() {
                        None
                    } else {
                        Some(filter)
                    };
                    changed = true;
                }
            });
            changed |= refresh_interval_setting(
                ui,
                &mut cfg.refresh_interval_secs,
                "Cached actions refresh on this cadence; use Refresh to reload immediately.",
            );

            ui.separator();
            ui.heading("Sources");
            let mut to_remove = None;
            for (idx, source) in cfg.sources.iter_mut().enumerate() {
                ui.push_id(idx, |ui| {
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut source.enabled, "");
                        ui.label("Label");
                        changed |= ui.text_edit_singleline(&mut source.label).changed();
                        egui::ComboBox::from_label("Type")
                            .selected_text(match source.kind {
                                SourceKind::Plugin => "Plugin",
                                SourceKind::Usage => "Usage",
                            })
                            .show_ui(ui, |ui| {
                                changed |= ui
                                    .selectable_value(
                                        &mut source.kind,
                                        SourceKind::Plugin,
                                        "Plugin",
                                    )
                                    .changed();
                                changed |= ui
                                    .selectable_value(&mut source.kind, SourceKind::Usage, "Usage")
                                    .changed();
                            });
                        if ui.button("Remove").clicked() {
                            to_remove = Some(idx);
                        }
                    });

                    if source.kind == SourceKind::Plugin {
                        ui.horizontal(|ui| {
                            ui.label("Plugin");
                            let mut plugin = source.plugin.clone().unwrap_or_default();
                            if ui.text_edit_singleline(&mut plugin).changed() {
                                source.plugin = if plugin.trim().is_empty() {
                                    None
                                } else {
                                    Some(plugin)
                                };
                                changed = true;
                            }
                            ui.label("Query");
                            let mut query = source.query.clone().unwrap_or_default();
                            if ui.text_edit_singleline(&mut query).changed() {
                                source.query = if query.trim().is_empty() {
                                    None
                                } else {
                                    Some(query)
                                };
                                changed = true;
                            }
                        });
                    }

                    ui.horizontal(|ui| {
                        ui.label("Limit");
                        let mut limit = source.limit.unwrap_or(cfg.limit_per_source);
                        if ui
                            .add(egui::DragValue::new(&mut limit).clamp_range(1..=50))
                            .changed()
                        {
                            source.limit = Some(limit);
                            changed = true;
                        }
                    });

                    ui.separator();
                });
            }
            if let Some(idx) = to_remove {
                cfg.sources.remove(idx);
                changed = true;
            }
            if ui.button("Add source").clicked() {
                cfg.sources.push(ActionSourceConfig {
                    label: "Custom".into(),
                    enabled: true,
                    plugin: Some("plugin_name".into()),
                    query: Some("query".into()),
                    limit: None,
                    kind: SourceKind::Plugin,
                });
                changed = true;
            }

            changed
        })
    }

    fn refresh_interval(&self) -> Duration {
        Duration::from_secs_f32(self.cfg.refresh_interval_secs.max(1.0))
    }

    fn update_interval(&mut self) {
        self.cache.set_interval(self.refresh_interval());
    }

    fn refresh(&mut self, ctx: &DashboardContext<'_>) {
        self.update_interval();
        self.errors.clear();
        let filter = self.cfg.filter.clone().map(|s| s.to_lowercase());
        let mut results = Vec::new();
        for source in self.cfg.sources.iter().filter(|s| s.enabled) {
            match self.load_source(ctx, source, &filter) {
                Ok(Some(res)) => results.push(res),
                Ok(None) => {}
                Err(err) => self.errors.push(err),
            }
        }
        self.cache.refresh(|data| *data = results);
    }

    fn maybe_refresh(&mut self, ctx: &DashboardContext<'_>) {
        self.update_interval();
        if self.cache.should_refresh() {
            self.refresh(ctx);
        }
    }

    fn load_source(
        &self,
        ctx: &DashboardContext<'_>,
        source: &ActionSourceConfig,
        filter: &Option<String>,
    ) -> Result<Option<SourceResult>, String> {
        let limit = source.limit.unwrap_or(self.cfg.limit_per_source).max(1);
        let mut actions = match source.kind {
            SourceKind::Plugin => {
                let plugin = source
                    .plugin
                    .as_deref()
                    .ok_or_else(|| format!("Missing plugin name for {}", source.label))?;
                let query = source
                    .query
                    .as_deref()
                    .ok_or_else(|| format!("Missing query for {}", source.label))?;
                let Some(plugin_ref) = find_plugin(ctx, plugin) else {
                    return Err(format!("Plugin '{plugin}' not available."));
                };
                plugin_ref.search(query)
            }
            SourceKind::Usage => self.usage_actions(ctx),
        };

        if let Some(filter) = filter {
            let filter = filter.as_str();
            actions.retain(|a| {
                a.label.to_lowercase().contains(filter) || a.desc.to_lowercase().contains(filter)
            });
        }
        if actions.is_empty() {
            return Ok(None);
        }
        actions.truncate(limit);
        Ok(Some(SourceResult {
            label: source.label.clone(),
            actions,
        }))
    }

    fn usage_actions(&self, ctx: &DashboardContext<'_>) -> Vec<Action> {
        let mut usage: Vec<(&String, &u32)> = ctx.usage.iter().collect();
        usage.sort_by(|a, b| b.1.cmp(a.1));
        usage
            .into_iter()
            .filter_map(|(action_id, _)| self.resolve_action(ctx.actions, action_id))
            .take(self.cfg.limit_per_source)
            .collect()
    }

    fn resolve_action(&self, actions: &[Action], key: &str) -> Option<Action> {
        actions
            .iter()
            .find(|a| a.action == key)
            .cloned()
            .or_else(|| {
                Some(Action {
                    label: key.to_string(),
                    desc: "Command".into(),
                    action: key.to_string(),
                    args: None,
                })
            })
    }

    fn render_sources(&self, ui: &mut egui::Ui) -> Option<WidgetAction> {
        let mut clicked = None;
        for source in &self.cache.data {
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(&source.label).small().strong());
                for action in &source.actions {
                    if ui
                        .button(&action.label)
                        .on_hover_text(&action.desc)
                        .clicked()
                    {
                        clicked = Some(action.clone());
                    }
                }
            });
            ui.separator();
        }
        clicked.map(|action| WidgetAction {
            query_override: Some(action.label.clone()),
            action,
        })
    }
}

impl Widget for ActionGroupWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        self.maybe_refresh(ctx);

        for err in &self.errors {
            ui.colored_label(egui::Color32::YELLOW, err);
        }

        if self.cache.data.is_empty() {
            ui.label("No actions available. Check plugin configuration or refresh.");
            return None;
        }

        self.render_sources(ui)
    }

    fn on_config_updated(&mut self, settings: &serde_json::Value) {
        if let Ok(mut cfg) = serde_json::from_value::<ActionGroupConfig>(settings.clone()) {
            cfg.ensure_kind(cfg.kind);
            self.cfg = cfg;
            self.update_interval();
            self.cache.invalidate();
        }
    }

    fn header_ui(&mut self, ui: &mut egui::Ui, ctx: &DashboardContext<'_>) -> Option<WidgetAction> {
        let tooltip = format!(
            "Cached for {:.0}s. Refresh to reload grouped actions immediately.",
            self.cfg.refresh_interval_secs
        );
        if ui.small_button("Refresh").on_hover_text(tooltip).clicked() {
            self.refresh(ctx);
        }
        None
    }
}

fn plugin_source(
    label: &str,
    plugin: &str,
    query: &str,
    limit: Option<usize>,
) -> ActionSourceConfig {
    ActionSourceConfig {
        label: label.to_string(),
        enabled: true,
        plugin: Some(plugin.to_string()),
        query: Some(query.to_string()),
        limit,
        kind: SourceKind::Plugin,
    }
}

fn usage_source(label: &str, limit: Option<usize>) -> ActionSourceConfig {
    ActionSourceConfig {
        label: label.to_string(),
        enabled: true,
        plugin: None,
        query: None,
        limit,
        kind: SourceKind::Usage,
    }
}

fn default_sources(kind: GroupKind) -> Vec<ActionSourceConfig> {
    match kind {
        GroupKind::QuickActions => vec![
            plugin_source("Favorites", "favorites", "fav list", None),
            plugin_source("Bookmarks", "bookmarks", "bm list", None),
            plugin_source("Folders", "folders", "f", None),
            plugin_source("Snippets", "snippets", "cs list", None),
        ],
        GroupKind::Continuity => vec![
            plugin_source("History", "history", "hi", None),
            usage_source("Usage", None),
            plugin_source("Calc history", "calculator", "= history", None),
        ],
        GroupKind::TaskTime => vec![
            plugin_source("Todos", "todo", "todo list", None),
            plugin_source("Timers", "timer", "timer list", None),
            plugin_source("Stopwatch", "stopwatch", "sw list", None),
        ],
        GroupKind::SystemGlance => vec![
            plugin_source("CPU / RAM / Disk", "sysinfo", "info", Some(3)),
            plugin_source("Network", "network", "net", None),
            plugin_source("IP", "ip", "ip", Some(3)),
            plugin_source("Processes", "processes", "ps", None),
            plugin_source("Recycle bin", "recycle", "rec", Some(1)),
        ],
        GroupKind::Workspace => vec![
            plugin_source("Windows", "windows", "win", None),
            plugin_source("Browser tabs", "browser_tabs", "tab", None),
        ],
        GroupKind::Utilities => vec![
            plugin_source("Clipboard", "clipboard", "cb list", None),
            plugin_source("Units m→ft", "unit_convert", "conv 1 m to ft", Some(1)),
            plugin_source("Units kg→lb", "unit_convert", "conv 1 kg to lb", Some(1)),
            plugin_source("Timestamp", "timestamp", "ts 1700000000", Some(1)),
            plugin_source("Media", "media", "media", None),
            plugin_source("Volume", "volume", "vol", Some(1)),
            plugin_source("Brightness", "brightness", "bright", Some(1)),
        ],
    }
}
