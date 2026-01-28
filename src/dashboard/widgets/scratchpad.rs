use super::{
    default_refresh_throttle_secs, edit_typed_settings, refresh_schedule, refresh_settings_ui,
    run_refresh_schedule, RefreshMode, Widget, WidgetAction, WidgetSettingsContext,
    WidgetSettingsUiResult,
};
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use chrono::NaiveDateTime;
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn default_refresh_interval() -> f32 {
    30.0
}

fn default_debounce_secs() -> f32 {
    0.5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScratchpadConfig {
    #[serde(default = "default_refresh_interval")]
    pub refresh_interval_secs: f32,
    #[serde(default)]
    pub refresh_mode: RefreshMode,
    #[serde(default = "default_refresh_throttle_secs")]
    pub refresh_throttle_secs: f32,
    #[serde(default)]
    pub manual_refresh_only: bool,
    #[serde(default = "default_debounce_secs")]
    pub debounce_secs: f32,
    #[serde(default)]
    pub storage_path: Option<String>,
}

impl Default for ScratchpadConfig {
    fn default() -> Self {
        Self {
            refresh_interval_secs: default_refresh_interval(),
            refresh_mode: RefreshMode::Auto,
            refresh_throttle_secs: default_refresh_throttle_secs(),
            manual_refresh_only: false,
            debounce_secs: default_debounce_secs(),
            storage_path: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
struct ScratchpadStorage {
    content: String,
}

pub struct ScratchpadWidget {
    cfg: ScratchpadConfig,
    content: String,
    dirty: bool,
    last_edit: Option<Instant>,
    refresh_pending: bool,
    last_refresh: Instant,
    error: Option<String>,
}

impl ScratchpadWidget {
    pub fn new(cfg: ScratchpadConfig) -> Self {
        let interval = Duration::from_secs_f32(cfg.refresh_interval_secs.max(1.0));
        let path = storage_path_for(&cfg);
        let (content, error) = load_storage(&path);
        Self {
            cfg,
            content,
            dirty: false,
            last_edit: None,
            refresh_pending: false,
            last_refresh: Instant::now() - interval,
            error,
        }
    }

    pub fn settings_ui(
        ui: &mut egui::Ui,
        value: &mut serde_json::Value,
        ctx: &WidgetSettingsContext<'_>,
    ) -> WidgetSettingsUiResult {
        edit_typed_settings(ui, value, ctx, |ui, cfg: &mut ScratchpadConfig, _ctx| {
            let mut changed = false;
            changed |= refresh_settings_ui(
                ui,
                &mut cfg.refresh_interval_secs,
                &mut cfg.refresh_mode,
                &mut cfg.refresh_throttle_secs,
                Some(&mut cfg.manual_refresh_only),
                "Scratchpad reloads from disk. Use Refresh to reload immediately.",
            );
            ui.horizontal(|ui| {
                ui.label("Save debounce (secs)");
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut cfg.debounce_secs)
                            .clamp_range(0.1..=5.0)
                            .speed(0.1),
                    )
                    .changed();
            });
            ui.horizontal(|ui| {
                ui.label("Storage file");
                let mut path = cfg.storage_path.clone().unwrap_or_default();
                if ui.text_edit_singleline(&mut path).changed() {
                    cfg.storage_path = if path.trim().is_empty() {
                        None
                    } else {
                        Some(path)
                    };
                    changed = true;
                }
            });
            changed
        })
    }

    fn refresh_interval(&self) -> Duration {
        Duration::from_secs_f32(self.cfg.refresh_interval_secs.max(1.0))
    }

    fn text_edit_id(&self, ui: &egui::Ui) -> egui::Id {
        ui.id().with("scratchpad_text")
    }

    fn reload_from_storage(&mut self) {
        let path = storage_path_for(&self.cfg);
        let (content, error) = load_storage(&path);
        if error.is_none() {
            self.content = content;
            self.dirty = false;
            self.last_edit = None;
        }
        self.error = error;
    }

    fn schedule_save(&mut self) {
        self.dirty = true;
        self.last_edit = Some(Instant::now());
    }

    fn save_if_ready(&mut self) {
        if !self.dirty {
            return;
        }
        let Some(last_edit) = self.last_edit else {
            return;
        };
        let debounce = Duration::from_secs_f32(self.cfg.debounce_secs.max(0.1));
        if last_edit.elapsed() < debounce {
            return;
        }
        let path = storage_path_for(&self.cfg);
        match save_storage(&path, &self.content) {
            Ok(()) => {
                self.dirty = false;
                self.error = None;
            }
            Err(err) => {
                self.error = Some(err);
            }
        }
    }

    fn insert_timestamp(&mut self, ui: &egui::Ui) {
        let timestamp = format_timestamp(chrono::Local::now().naive_local());
        let id = self.text_edit_id(ui);
        let ctx = ui.ctx();
        let mut state = egui::widgets::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
        let cursor_index = state
            .cursor
            .char_range()
            .map(|range| range.primary.index)
            .unwrap_or_else(|| self.content.chars().count());
        let new_cursor = insert_at_char_index(&mut self.content, &timestamp, cursor_index);
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(
                egui::text::CCursor::new(new_cursor),
            )));
        state.store(ctx, id);
        self.schedule_save();
    }
}

impl Default for ScratchpadWidget {
    fn default() -> Self {
        Self::new(ScratchpadConfig::default())
    }
}

impl Widget for ScratchpadWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        let schedule = refresh_schedule(
            self.refresh_interval(),
            self.cfg.refresh_mode,
            self.cfg.manual_refresh_only,
            self.cfg.refresh_throttle_secs,
        );
        if run_refresh_schedule(
            ctx,
            schedule,
            &mut self.refresh_pending,
            &mut self.last_refresh,
        ) {
            if !self.dirty {
                self.reload_from_storage();
                self.last_refresh = Instant::now();
            } else {
                self.refresh_pending = true;
            }
        }

        ui.horizontal(|ui| {
            if ui.button("Copy").clicked() {
                ui.ctx().output_mut(|output| {
                    output.copied_text = self.content.clone();
                });
            }
            if ui.button("Append timestamp").clicked() {
                self.insert_timestamp(ui);
            }
        });

        if let Some(error) = &self.error {
            ui.colored_label(egui::Color32::YELLOW, error);
        }

        let text_id = self.text_edit_id(ui);
        let resp = ui.add(
            egui::TextEdit::multiline(&mut self.content)
                .id_source(text_id)
                .desired_rows(8)
                .desired_width(f32::INFINITY)
                .frame(true),
        );
        if resp.changed() {
            self.schedule_save();
        }

        self.save_if_ready();

        None
    }

    fn on_config_updated(&mut self, settings: &serde_json::Value) {
        if let Ok(cfg) = serde_json::from_value::<ScratchpadConfig>(settings.clone()) {
            self.cfg = cfg;
            self.refresh_pending = true;
        }
    }

    fn header_ui(
        &mut self,
        ui: &mut egui::Ui,
        _ctx: &DashboardContext<'_>,
    ) -> Option<WidgetAction> {
        let schedule = refresh_schedule(
            self.refresh_interval(),
            self.cfg.refresh_mode,
            self.cfg.manual_refresh_only,
            self.cfg.refresh_throttle_secs,
        );
        let tooltip = match schedule.mode {
            RefreshMode::Manual => "Manual refresh only.".to_string(),
            RefreshMode::Throttled => {
                format!(
                    "Minimum refresh interval {:.0}s.",
                    schedule.throttle.as_secs_f32()
                )
            }
            RefreshMode::Auto => format!(
                "Reloads from disk every {:.0}s.",
                self.cfg.refresh_interval_secs
            ),
        };
        if ui.small_button("Refresh").on_hover_text(tooltip).clicked() {
            self.refresh_pending = true;
        }
        None
    }
}

fn storage_path_for(cfg: &ScratchpadConfig) -> PathBuf {
    cfg.storage_path
        .as_ref()
        .filter(|p| !p.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("scratchpad.json"))
}

fn load_storage(path: &Path) -> (String, Option<String>) {
    if !path.exists() {
        return (String::new(), None);
    }
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(err) => return (String::new(), Some(err.to_string())),
    };
    if content.trim().is_empty() {
        return (String::new(), None);
    }
    match serde_json::from_str::<ScratchpadStorage>(&content) {
        Ok(storage) => (storage.content, None),
        Err(err) => (String::new(), Some(err.to_string())),
    }
}

fn save_storage(path: &Path, content: &str) -> Result<(), String> {
    let payload = ScratchpadStorage {
        content: content.to_string(),
    };
    let json = serde_json::to_string_pretty(&payload).map_err(|err| err.to_string())?;
    std::fs::write(path, json).map_err(|err| err.to_string())?;
    Ok(())
}

fn format_timestamp(value: NaiveDateTime) -> String {
    value.format("%Y-%m-%d %H:%M:%S").to_string()
}

fn insert_at_char_index(text: &mut String, insert: &str, char_index: usize) -> usize {
    let total_chars = text.chars().count();
    let clamped_index = char_index.min(total_chars);
    let byte_index = text
        .char_indices()
        .nth(clamped_index)
        .map(|(idx, _)| idx)
        .unwrap_or_else(|| text.len());
    text.insert_str(byte_index, insert);
    clamped_index + insert.chars().count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn format_timestamp_uses_expected_pattern() {
        let date = NaiveDate::from_ymd_opt(2024, 1, 2)
            .unwrap()
            .and_hms_opt(3, 4, 5)
            .unwrap();
        assert_eq!(format_timestamp(date), "2024-01-02 03:04:05");
    }

    #[test]
    fn insert_at_char_index_inserts_at_cursor() {
        let mut text = String::from("hi 🌟");
        let new_cursor = insert_at_char_index(&mut text, "there ", 3);
        assert_eq!(text, "hi there 🌟");
        assert_eq!(new_cursor, 3 + "there ".chars().count());
    }

    #[test]
    fn save_and_load_storage_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scratchpad.json");
        save_storage(&path, "hello world").unwrap();
        let (content, error) = load_storage(&path);
        assert!(error.is_none());
        assert_eq!(content, "hello world");
    }
}
