use super::{
    edit_typed_settings, refresh_interval_setting, TimedCache, Widget, WidgetAction,
    WidgetSettingsContext, WidgetSettingsUiResult,
};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use crate::plugins::tempfile::list_files;
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

fn default_limit() -> usize {
    8
}

fn default_refresh_interval() -> f32 {
    10.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TempfilesConfig {
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default = "default_refresh_interval")]
    pub refresh_interval_secs: f32,
    #[serde(default)]
    pub manual_refresh_only: bool,
}

impl Default for TempfilesConfig {
    fn default() -> Self {
        Self {
            limit: default_limit(),
            refresh_interval_secs: default_refresh_interval(),
            manual_refresh_only: false,
        }
    }
}

#[derive(Clone, Debug)]
struct TempfileEntry {
    path: PathBuf,
    file_name: String,
    alias: Option<String>,
}

pub struct TempfilesWidget {
    cfg: TempfilesConfig,
    cache: TimedCache<Vec<TempfileEntry>>,
    error: Option<String>,
    refresh_pending: bool,
}

impl TempfilesWidget {
    pub fn new(cfg: TempfilesConfig) -> Self {
        let interval = Duration::from_secs_f32(cfg.refresh_interval_secs.max(1.0));
        Self {
            cfg,
            cache: TimedCache::new(Vec::new(), interval),
            error: None,
            refresh_pending: false,
        }
    }

    pub fn settings_ui(
        ui: &mut egui::Ui,
        value: &mut serde_json::Value,
        ctx: &WidgetSettingsContext<'_>,
    ) -> WidgetSettingsUiResult {
        edit_typed_settings(ui, value, ctx, |ui, cfg: &mut TempfilesConfig, _ctx| {
            let mut changed = false;
            ui.horizontal(|ui| {
                ui.label("Show up to");
                changed |= ui
                    .add(egui::DragValue::new(&mut cfg.limit).clamp_range(1..=50))
                    .changed();
                ui.label("files");
            });
            changed |= refresh_interval_setting(
                ui,
                &mut cfg.refresh_interval_secs,
                &mut cfg.manual_refresh_only,
                "Tempfile listing is cached. The widget will skip refreshing until this many seconds have passed. Use Refresh to update immediately.",
            );
            changed
        })
    }

    fn refresh_interval(&self) -> Duration {
        Duration::from_secs_f32(self.cfg.refresh_interval_secs.max(1.0))
    }

    fn update_interval(&mut self) {
        self.cache.set_interval(self.refresh_interval());
    }

    fn refresh(&mut self) {
        self.update_interval();
        let (entries, error) = Self::load_files(self.cfg.limit.max(1));
        self.error = error;
        self.cache.refresh(|data| *data = entries);
    }

    fn maybe_refresh(&mut self) {
        self.update_interval();
        if self.refresh_pending {
            self.refresh_pending = false;
            self.refresh();
        } else if !self.cfg.manual_refresh_only && self.cache.should_refresh() {
            self.refresh();
        }
    }

    fn load_files(limit: usize) -> (Vec<TempfileEntry>, Option<String>) {
        let mut files = match list_files() {
            Ok(list) => list,
            Err(e) => return (Vec::new(), Some(format!("Failed to list temp files: {e}"))),
        };
        files.sort_by(|a, b| {
            let a_name = a
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_lowercase();
            let b_name = b
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_lowercase();
            a_name.cmp(&b_name)
        });
        if files.len() > limit {
            files.truncate(limit);
        }
        let entries = files
            .into_iter()
            .map(|path| {
                let file_name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(str::to_string)
                    .unwrap_or_else(|| path.to_string_lossy().into_owned());
                let alias = Self::alias_for(&path);
                TempfileEntry {
                    path,
                    file_name,
                    alias,
                }
            })
            .collect();
        (entries, None)
    }

    fn alias_for(path: &Path) -> Option<String> {
        let stem = path.file_stem()?.to_str()?;
        let remainder = stem.strip_prefix("temp_")?;
        if remainder.is_empty() {
            return None;
        }
        if remainder.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        let alias = if let Some((base, suffix)) = remainder.rsplit_once('_') {
            if !base.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()) {
                base
            } else {
                remainder
            }
        } else {
            remainder
        };
        Some(alias.to_string())
    }

    fn add_action() -> Action {
        Action {
            label: "Create temp file".into(),
            desc: "Tempfile".into(),
            action: "tempfile:dialog".into(),
            args: None,
        }
    }

    fn clear_all_action() -> Action {
        Action {
            label: "Clear temp files".into(),
            desc: "Tempfile".into(),
            action: "tempfile:clear".into(),
            args: None,
        }
    }

    fn remove_action(path: &Path, name: &str) -> Action {
        Action {
            label: format!("Remove {name}"),
            desc: "Tempfile".into(),
            action: format!("tempfile:remove:{}", path.to_string_lossy()),
            args: None,
        }
    }

    fn open_action(path: &Path, name: &str) -> Action {
        Action {
            label: format!("Open {name}"),
            desc: "Tempfile".into(),
            action: format!("tempfile:open:{}", path.to_string_lossy()),
            args: None,
        }
    }
}

impl Default for TempfilesWidget {
    fn default() -> Self {
        Self::new(TempfilesConfig::default())
    }
}

impl Widget for TempfilesWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        _ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        self.maybe_refresh();

        if let Some(err) = &self.error {
            ui.colored_label(egui::Color32::YELLOW, err);
        }

        let mut clicked = None;
        ui.horizontal(|ui| {
            if ui.button("Add").clicked() {
                let action = Self::add_action();
                clicked = Some(WidgetAction {
                    query_override: Some(action.label.clone()),
                    action,
                });
            }
            if ui.button("Clear all").clicked() {
                let action = Self::clear_all_action();
                clicked = Some(WidgetAction {
                    query_override: Some(action.label.clone()),
                    action,
                });
            }
        });

        if clicked.is_some() {
            return clicked;
        }

        if self.cache.data.is_empty() {
            ui.label("No temp files found.");
            return None;
        }

        let row_height =
            ui.text_style_height(&egui::TextStyle::Body) + ui.spacing().item_spacing.y + 8.0;
        let scroll_id = ui.id().with("tempfiles_scroll");
        egui::ScrollArea::both()
            .id_source(scroll_id)
            .auto_shrink([false; 2])
            .show_rows(ui, row_height, self.cache.data.len(), |ui, range| {
                for entry in &self.cache.data[range] {
                    ui.horizontal(|ui| {
                        let display_name = entry.alias.as_deref().unwrap_or(&entry.file_name);
                        ui.add(egui::Label::new(display_name).wrap(false))
                            .on_hover_text(entry.path.to_string_lossy());
                        if let Some(alias) = &entry.alias {
                            if alias != &entry.file_name {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(format!("({})", entry.file_name))
                                            .small(),
                                    )
                                    .wrap(false),
                                );
                            }
                        }
                        if ui
                            .small_button("Open")
                            .on_hover_text("Open file (not folder)")
                            .clicked()
                        {
                            let action = Self::open_action(&entry.path, display_name);
                            clicked = Some(WidgetAction {
                                query_override: Some(action.label.clone()),
                                action,
                            });
                        }
                        if ui.small_button("Clear").clicked() {
                            let action = Self::remove_action(&entry.path, display_name);
                            clicked = Some(WidgetAction {
                                query_override: Some(action.label.clone()),
                                action,
                            });
                        }
                    });
                }
            });

        clicked
    }

    fn on_config_updated(&mut self, settings: &serde_json::Value) {
        if let Ok(cfg) = serde_json::from_value::<TempfilesConfig>(settings.clone()) {
            self.cfg = cfg;
            self.update_interval();
            self.cache.invalidate();
            self.refresh_pending = true;
        }
    }

    fn header_ui(&mut self, ui: &mut egui::Ui, _ctx: &DashboardContext<'_>) -> Option<WidgetAction> {
        let tooltip = if self.cfg.manual_refresh_only {
            "Manual refresh only.".to_string()
        } else {
            format!(
                "Cached for {:.0}s. Refresh to update the tempfile list immediately.",
                self.cfg.refresh_interval_secs
            )
        };
        if ui.small_button("Refresh").on_hover_text(tooltip).clicked() {
            self.refresh();
        }
        None
    }
}
