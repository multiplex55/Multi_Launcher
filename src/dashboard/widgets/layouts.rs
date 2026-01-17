use super::{
    default_refresh_throttle_secs, edit_typed_settings, refresh_schedule, refresh_settings_ui,
    run_refresh_schedule, RefreshMode, TimedCache, Widget, WidgetAction, WidgetSettingsContext,
    WidgetSettingsUiResult,
};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use crate::plugins::layouts_storage::{self, Layout, LayoutMatch, LayoutStore, LAYOUTS_FILE};
use crate::windows_layout::{collect_layout_windows, LayoutWindowOptions};
use chrono::Utc;
use eframe::egui;
use rfd::FileDialog;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

fn default_refresh_interval() -> f32 {
    30.0
}

fn default_show_health_indicator() -> bool {
    true
}

fn default_autosave_on_change() -> bool {
    false
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutsConfig {
    #[serde(default = "default_refresh_interval")]
    pub refresh_interval_secs: f32,
    #[serde(default)]
    pub refresh_mode: RefreshMode,
    #[serde(default = "default_refresh_throttle_secs")]
    pub refresh_throttle_secs: f32,
    #[serde(default = "default_show_health_indicator")]
    pub show_health_indicator: bool,
    #[serde(default = "default_autosave_on_change")]
    pub autosave_on_change: bool,
}

impl Default for LayoutsConfig {
    fn default() -> Self {
        Self {
            refresh_interval_secs: default_refresh_interval(),
            refresh_mode: RefreshMode::Auto,
            refresh_throttle_secs: default_refresh_throttle_secs(),
            show_health_indicator: default_show_health_indicator(),
            autosave_on_change: default_autosave_on_change(),
        }
    }
}

#[derive(Debug, Clone)]
struct LayoutHealth {
    matched: usize,
    expected: usize,
}

#[derive(Debug, Clone)]
struct LayoutSummary {
    name: String,
    created_at: Option<String>,
    notes: Option<String>,
    health: Option<LayoutHealth>,
}

#[derive(Debug, Clone)]
struct LayoutsData {
    layouts: Vec<LayoutSummary>,
}

#[derive(Debug, Clone)]
struct StatusMessage {
    text: String,
    color: egui::Color32,
}

#[derive(Debug, Clone)]
struct PendingImport {
    layout: Layout,
    source: PathBuf,
}

pub struct LayoutsWidget {
    cfg: LayoutsConfig,
    cache: TimedCache<LayoutsData>,
    error: Option<String>,
    refresh_pending: bool,
    rename_target: Option<String>,
    rename_value: String,
    active_layout_name: Option<String>,
    active_layout: Option<Layout>,
    last_saved_layout: Option<Layout>,
    dirty: bool,
    status: Option<StatusMessage>,
    pending_import: Option<PendingImport>,
}

impl LayoutsWidget {
    pub fn new(cfg: LayoutsConfig) -> Self {
        let interval = Duration::from_secs_f32(cfg.refresh_interval_secs.max(1.0));
        Self {
            cfg,
            cache: TimedCache::new(
                LayoutsData {
                    layouts: Vec::new(),
                },
                interval,
            ),
            error: None,
            refresh_pending: false,
            rename_target: None,
            rename_value: String::new(),
            active_layout_name: None,
            active_layout: None,
            last_saved_layout: None,
            dirty: false,
            status: None,
            pending_import: None,
        }
    }

    pub fn settings_ui(
        ui: &mut egui::Ui,
        value: &mut serde_json::Value,
        ctx: &WidgetSettingsContext<'_>,
    ) -> WidgetSettingsUiResult {
        edit_typed_settings(ui, value, ctx, |ui, cfg: &mut LayoutsConfig, _ctx| {
            let mut changed = false;
            changed |= refresh_settings_ui(
                ui,
                &mut cfg.refresh_interval_secs,
                &mut cfg.refresh_mode,
                &mut cfg.refresh_throttle_secs,
                None,
                "Layouts are cached between refreshes.",
            );
            changed |= ui
                .checkbox(&mut cfg.show_health_indicator, "Show health indicator")
                .changed();
            changed |= ui
                .checkbox(&mut cfg.autosave_on_change, "Autosave on change")
                .changed();
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
        let (data, error) = Self::load_layouts(&self.cfg);
        self.error = error;
        self.cache.refresh(|cache| *cache = data);
    }

    fn set_status(&mut self, text: impl Into<String>, color: egui::Color32) {
        self.status = Some(StatusMessage {
            text: text.into(),
            color,
        });
    }

    fn update_dirty(&mut self) {
        self.dirty = self.active_layout != self.last_saved_layout;
    }

    fn load_active_layout(&mut self, name: &str) -> Result<(), String> {
        let store = layouts_storage::load_layouts(LAYOUTS_FILE)
            .map_err(|err| format!("Failed to load layouts: {err}"))?;
        let layout = layouts_storage::get_layout(&store, name)
            .ok_or_else(|| "Layout not found.".to_string())?
            .clone();
        self.active_layout_name = Some(name.to_string());
        self.active_layout = Some(layout.clone());
        self.last_saved_layout = Some(layout);
        self.dirty = false;
        Ok(())
    }

    fn save_layout(&mut self, layout: Layout) -> Result<(), String> {
        let mut store = layouts_storage::load_layouts(LAYOUTS_FILE)
            .map_err(|err| format!("Failed to load layouts: {err}"))?;
        layouts_storage::upsert_layout(&mut store, layout.clone());
        layouts_storage::save_layouts(LAYOUTS_FILE, &store)
            .map_err(|err| format!("Failed to save layouts: {err}"))?;
        self.active_layout_name = Some(layout.name.clone());
        self.active_layout = Some(layout.clone());
        self.last_saved_layout = Some(layout);
        self.dirty = false;
        self.refresh_pending = true;
        self.cache.invalidate();
        Ok(())
    }

    fn apply_layout_change(&mut self, layout: Layout) -> Result<(), String> {
        self.active_layout_name = Some(layout.name.clone());
        self.active_layout = Some(layout.clone());
        if self.cfg.autosave_on_change {
            self.save_layout(layout)?;
        } else {
            self.update_dirty();
        }
        Ok(())
    }

    fn unique_layout_name(base: &str, store: &LayoutStore, suffix: &str) -> String {
        if !store.layouts.iter().any(|layout| layout.name == base) {
            return base.to_string();
        }
        let mut idx = 1;
        loop {
            let candidate = if idx == 1 {
                format!("{base}{suffix}")
            } else {
                format!("{base}{suffix} {idx}")
            };
            if !store.layouts.iter().any(|layout| layout.name == candidate) {
                return candidate;
            }
            idx += 1;
        }
    }

    fn duplicate_active_layout(&mut self) -> Result<String, String> {
        let Some(layout) = self.active_layout.clone() else {
            return Err("No active layout selected.".to_string());
        };
        let mut store = layouts_storage::load_layouts(LAYOUTS_FILE)
            .map_err(|err| format!("Failed to load layouts: {err}"))?;
        let new_name = Self::unique_layout_name(&layout.name, &store, " (copy)");
        let mut cloned = layout.clone();
        cloned.name = new_name.clone();
        cloned.created_at = Some(Utc::now().to_rfc3339());
        layouts_storage::upsert_layout(&mut store, cloned.clone());
        layouts_storage::save_layouts(LAYOUTS_FILE, &store)
            .map_err(|err| format!("Failed to save layouts: {err}"))?;
        self.active_layout_name = Some(new_name.clone());
        self.active_layout = Some(cloned.clone());
        self.last_saved_layout = Some(cloned);
        self.dirty = false;
        self.refresh_pending = true;
        self.cache.invalidate();
        Ok(new_name)
    }

    fn revert_active_layout(&mut self) -> Result<(), String> {
        let Some(name) = self.active_layout_name.clone() else {
            return Err("No active layout selected.".to_string());
        };
        self.load_active_layout(&name)?;
        Ok(())
    }

    fn export_active_layout(&mut self) -> Result<(), String> {
        let Some(layout) = self.active_layout.clone() else {
            return Err("No active layout selected.".to_string());
        };
        let Some(path) = FileDialog::new()
            .set_file_name(&format!("{}.json", layout.name))
            .save_file()
        else {
            return Ok(());
        };
        let json = serde_json::to_string_pretty(&layout)
            .map_err(|err| format!("Serialize failed: {err}"))?;
        std::fs::write(&path, json).map_err(|err| format!("Failed to write layout: {err}"))?;
        Ok(())
    }

    fn parse_layout_json(contents: &str) -> Result<Layout, String> {
        if let Ok(layout) = serde_json::from_str::<Layout>(contents) {
            return Ok(layout);
        }
        let store = serde_json::from_str::<LayoutStore>(contents)
            .map_err(|err| format!("Invalid layout JSON: {err}"))?;
        if store.layouts.len() == 1 {
            return Ok(store.layouts[0].clone());
        }
        Err("JSON must contain a single layout.".to_string())
    }

    fn begin_import(&mut self, path: PathBuf) -> Result<(), String> {
        let contents =
            std::fs::read_to_string(&path).map_err(|err| format!("Failed to read file: {err}"))?;
        let layout = Self::parse_layout_json(&contents)?;
        self.pending_import = Some(PendingImport {
            layout,
            source: path,
        });
        Ok(())
    }

    fn maybe_refresh(&mut self, ctx: &DashboardContext<'_>, visible: bool) {
        if !visible {
            return;
        }
        self.update_interval();
        let schedule = refresh_schedule(
            self.refresh_interval(),
            self.cfg.refresh_mode,
            false,
            self.cfg.refresh_throttle_secs,
        );
        run_refresh_schedule(
            ctx,
            schedule,
            &mut self.refresh_pending,
            &mut self.cache.last_refresh,
            || self.refresh(),
        );
    }

    fn load_layouts(cfg: &LayoutsConfig) -> (LayoutsData, Option<String>) {
        let store = match layouts_storage::load_layouts(LAYOUTS_FILE) {
            Ok(store) => store,
            Err(err) => {
                return (
                    LayoutsData {
                        layouts: Vec::new(),
                    },
                    Some(format!("Failed to load layouts: {err}")),
                );
            }
        };
        let health = if cfg.show_health_indicator {
            Self::collect_health(&store.layouts)
        } else {
            vec![None; store.layouts.len()]
        };
        let layouts = store
            .layouts
            .iter()
            .zip(health)
            .map(|(layout, health)| LayoutSummary {
                name: layout.name.clone(),
                created_at: layout.created_at.clone(),
                notes: if layout.notes.trim().is_empty() {
                    None
                } else {
                    Some(layout.notes.trim().to_string())
                },
                health,
            })
            .collect();
        (LayoutsData { layouts }, None)
    }

    fn collect_health(layouts: &[Layout]) -> Vec<Option<LayoutHealth>> {
        if layouts.is_empty() {
            return Vec::new();
        }
        let windows = collect_layout_windows(LayoutWindowOptions::default()).unwrap_or_default();
        let candidates: Vec<LayoutMatch> =
            windows.into_iter().map(|window| window.matcher).collect();
        layouts
            .iter()
            .map(|layout| Self::layout_health(layout, &candidates))
            .collect()
    }

    fn layout_health(layout: &Layout, candidates: &[LayoutMatch]) -> Option<LayoutHealth> {
        layout_health(layout, candidates)
    }

    fn metadata_text(layout: &LayoutSummary) -> Option<String> {
        let mut parts = Vec::new();
        if let Some(created) = &layout.created_at {
            parts.push(format!("Created {created}"));
        }
        if let Some(notes) = &layout.notes {
            parts.push(notes.clone());
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join(" · "))
        }
    }

    fn action(label: String, action: String) -> WidgetAction {
        WidgetAction {
            action: Action {
                label,
                desc: "Layout".into(),
                action,
                args: None,
            },
            query_override: None,
        }
    }

    fn rename_layout(&mut self, from: &str, to: &str) -> Result<(), String> {
        let new_name = to.trim();
        if new_name.is_empty() {
            return Err("Layout name cannot be empty.".to_string());
        }
        if new_name == from {
            return Ok(());
        }
        let mut store = layouts_storage::load_layouts(LAYOUTS_FILE)
            .map_err(|err| format!("Failed to load layouts: {err}"))?;
        if store.layouts.iter().any(|layout| layout.name == new_name) {
            return Err("A layout with that name already exists.".to_string());
        }
        let Some(layout) = store.layouts.iter_mut().find(|layout| layout.name == from) else {
            return Err("Layout not found.".to_string());
        };
        layout.name = new_name.to_string();
        layouts_storage::save_layouts(LAYOUTS_FILE, &store)
            .map_err(|err| format!("Failed to save layouts: {err}"))?;
        if self.active_layout_name.as_deref() == Some(from) {
            if let Some(active) = self.active_layout.as_mut() {
                active.name = new_name.to_string();
            }
            if let Some(saved) = self.last_saved_layout.as_mut() {
                saved.name = new_name.to_string();
            }
            self.active_layout_name = Some(new_name.to_string());
        }
        Ok(())
    }

    fn health_label(health: &LayoutHealth) -> (String, egui::Color32) {
        let text = format!("{}/{}", health.matched, health.expected);
        let color = if health.expected == 0 || health.matched == health.expected {
            egui::Color32::GREEN
        } else if health.matched == 0 {
            egui::Color32::RED
        } else {
            egui::Color32::YELLOW
        };
        (text, color)
    }
}

impl Default for LayoutsWidget {
    fn default() -> Self {
        Self::new(LayoutsConfig::default())
    }
}

impl Widget for LayoutsWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        _ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        let visible = ui.is_rect_visible(ui.available_rect_before_wrap());
        self.maybe_refresh(ctx, visible);

        if let Some(err) = &self.error {
            ui.colored_label(egui::Color32::YELLOW, err);
        }
        if let Some(status) = &self.status {
            ui.colored_label(status.color, &status.text);
        }

        if self.cache.data.layouts.is_empty() {
            ui.label("No layouts saved.");
            return None;
        }

        let layout_names: Vec<String> = self
            .cache
            .data
            .layouts
            .iter()
            .map(|layout| layout.name.clone())
            .collect();
        if self.active_layout_name.is_none() {
            if let Some(first) = layout_names.first() {
                if let Err(err) = self.load_active_layout(first) {
                    self.error = Some(err);
                }
            }
        } else if let Some(active) = &self.active_layout_name {
            if !layout_names.iter().any(|name| name == active) {
                self.active_layout_name = None;
                self.active_layout = None;
                self.last_saved_layout = None;
                self.dirty = false;
            }
        }

        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label("Active layout");
                let mut selected = self.active_layout_name.clone().unwrap_or_default();
                egui::ComboBox::from_id_source("active_layout_combo")
                    .selected_text(selected.clone())
                    .show_ui(ui, |ui| {
                        for name in &layout_names {
                            if ui.selectable_label(name == &selected, name).clicked() {
                                selected = name.clone();
                            }
                        }
                    });
                if !selected.is_empty()
                    && self.active_layout_name.as_deref() != Some(selected.as_str())
                {
                    if let Err(err) = self.load_active_layout(&selected) {
                        self.error = Some(err);
                    }
                }
                if self.dirty {
                    ui.colored_label(egui::Color32::YELLOW, "Unsaved changes");
                }
            });
            ui.horizontal(|ui| {
                let has_active = self.active_layout.is_some();
                ui.add_enabled_ui(has_active, |ui| {
                    if ui.button("Update layout").clicked() {
                        if let Some(layout) = self.active_layout.clone() {
                            if let Err(err) = self.save_layout(layout) {
                                self.set_status(err, egui::Color32::YELLOW);
                            } else {
                                self.set_status("Layout updated.", egui::Color32::GREEN);
                            }
                        }
                    }
                    if ui.button("Duplicate layout").clicked() {
                        match self.duplicate_active_layout() {
                            Ok(name) => {
                                self.set_status(
                                    format!("Duplicated layout as '{name}'."),
                                    egui::Color32::GREEN,
                                );
                            }
                            Err(err) => self.set_status(err, egui::Color32::YELLOW),
                        }
                    }
                    if ui.button("Revert layout").clicked() {
                        match self.revert_active_layout() {
                            Ok(()) => {
                                self.set_status("Reverted layout.", egui::Color32::GREEN);
                            }
                            Err(err) => self.set_status(err, egui::Color32::YELLOW),
                        }
                    }
                    if ui.button("Export layout").clicked() {
                        match self.export_active_layout() {
                            Ok(()) => {
                                self.set_status("Exported layout.", egui::Color32::GREEN);
                            }
                            Err(err) => self.set_status(err, egui::Color32::YELLOW),
                        }
                    }
                    if ui.button("Import layout").clicked() {
                        if let Some(path) = FileDialog::new().pick_file() {
                            match self.begin_import(path) {
                                Ok(()) => {
                                    self.set_status(
                                        "Layout loaded. Choose how to import.",
                                        egui::Color32::YELLOW,
                                    );
                                }
                                Err(err) => self.set_status(err, egui::Color32::YELLOW),
                            }
                        }
                    }
                });
            });
            if let Some(pending) = self.pending_import.clone() {
                ui.separator();
                ui.label(format!(
                    "Import layout '{}' from {}",
                    pending.layout.name,
                    pending.source.display()
                ));
                ui.horizontal(|ui| {
                    if ui.button("Replace current").clicked() {
                        let Some(active_name) = self.active_layout_name.clone() else {
                            self.set_status(
                                "No active layout selected for replace.",
                                egui::Color32::YELLOW,
                            );
                            self.pending_import = None;
                            return;
                        };
                        let mut imported = pending.layout.clone();
                        imported.name = active_name;
                        match self.apply_layout_change(imported) {
                            Ok(()) => {
                                self.set_status("Layout replaced.", egui::Color32::GREEN);
                                if !self.cfg.autosave_on_change {
                                    self.dirty = true;
                                }
                            }
                            Err(err) => self.set_status(err, egui::Color32::YELLOW),
                        }
                        self.pending_import = None;
                    }
                    if ui.button("Import as new").clicked() {
                        let mut store = match layouts_storage::load_layouts(LAYOUTS_FILE) {
                            Ok(store) => store,
                            Err(err) => {
                                self.set_status(
                                    format!("Failed to load layouts: {err}"),
                                    egui::Color32::YELLOW,
                                );
                                self.pending_import = None;
                                return;
                            }
                        };
                        let mut imported = pending.layout.clone();
                        let new_name =
                            Self::unique_layout_name(&imported.name, &store, " (imported)");
                        imported.name = new_name.clone();
                        layouts_storage::upsert_layout(&mut store, imported.clone());
                        if let Err(err) = layouts_storage::save_layouts(LAYOUTS_FILE, &store) {
                            self.set_status(
                                format!("Failed to save layouts: {err}"),
                                egui::Color32::YELLOW,
                            );
                        } else {
                            self.active_layout_name = Some(imported.name.clone());
                            self.active_layout = Some(imported.clone());
                            self.last_saved_layout = Some(imported);
                            self.dirty = false;
                            self.refresh_pending = true;
                            self.cache.invalidate();
                            self.set_status("Layout imported.", egui::Color32::GREEN);
                        }
                        self.pending_import = None;
                    }
                    if ui.button("Cancel").clicked() {
                        self.pending_import = None;
                    }
                });
            }
        });
        ui.add_space(6.0);

        let mut selected = None;
        let layouts = self.cache.data.layouts.clone();
        for layout in layouts {
            let is_renaming = self.rename_target.as_deref() == Some(layout.name.as_str());
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(&layout.name);
                        if let Some(meta) = Self::metadata_text(&layout) {
                            ui.label(egui::RichText::new(meta).small());
                        }
                    });

                    if let Some(health) = &layout.health {
                        let (text, color) = Self::health_label(health);
                        ui.colored_label(color, text)
                            .on_hover_text("Matched windows / expected windows");
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.menu_button("⋯", |ui| {
                            if ui.button("Dry run").clicked() {
                                selected = Some(Self::action(
                                    format!("Dry run layout {}", layout.name),
                                    format!("layout:load:{}|dry_run", layout.name),
                                ));
                                ui.close_menu();
                            }
                            if ui.button("Edit JSON").clicked() {
                                selected = Some(Self::action(
                                    "Open layouts.json".to_string(),
                                    LAYOUTS_FILE.to_string(),
                                ));
                                ui.close_menu();
                            }
                            if ui.button("Rename").clicked() {
                                self.rename_target = Some(layout.name.clone());
                                self.rename_value = layout.name.clone();
                                ui.close_menu();
                            }
                            if ui.button("Delete").clicked() {
                                selected = Some(Self::action(
                                    format!("Remove layout {}", layout.name),
                                    format!("layout:rm:{}", layout.name),
                                ));
                                ui.close_menu();
                            }
                        });

                        if ui.button("Restore").clicked() {
                            selected = Some(Self::action(
                                format!("Load layout {}", layout.name),
                                format!("layout:load:{}", layout.name),
                            ));
                        }
                    });
                });

                if is_renaming {
                    ui.horizontal(|ui| {
                        ui.label("Rename to");
                        ui.text_edit_singleline(&mut self.rename_value);
                        if ui.button("Save").clicked() {
                            let target = layout.name.clone();
                            let rename_value = self.rename_value.clone();
                            match self.rename_layout(&target, &rename_value) {
                                Ok(()) => {
                                    self.error = None;
                                    self.refresh_pending = true;
                                    self.cache.invalidate();
                                }
                                Err(err) => {
                                    self.error = Some(err);
                                }
                            }
                            self.rename_target = None;
                        }
                        if ui.button("Cancel").clicked() {
                            self.rename_target = None;
                        }
                    });
                }
            });
            ui.add_space(4.0);
        }

        selected
    }

    fn on_config_updated(&mut self, settings: &serde_json::Value) {
        if let Ok(cfg) = serde_json::from_value::<LayoutsConfig>(settings.clone()) {
            self.cfg = cfg;
            self.update_interval();
            self.cache.invalidate();
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
            false,
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
                "Cached for {:.0}s. Refresh to rescan layouts.",
                self.cfg.refresh_interval_secs
            ),
        };
        let mut action = None;
        ui.horizontal(|ui| {
            if ui.small_button("Refresh").on_hover_text(tooltip).clicked() {
                self.refresh_pending = true;
            }
            if ui.small_button("Create new").clicked() {
                action = Some(Self::action(
                    "layout save <name>".into(),
                    "query:layout save ".into(),
                ));
            }
        });
        action
    }
}

#[cfg(windows)]
fn matches_title_regex(pattern: &str, title: &str) -> bool {
    regex::RegexBuilder::new(pattern)
        .case_insensitive(true)
        .build()
        .map(|re| re.is_match(title))
        .unwrap_or(false)
}

#[cfg(windows)]
fn is_rule_match(rule: &LayoutMatch, candidate: &LayoutMatch) -> bool {
    if rule.app_id.is_none()
        && rule.process.is_none()
        && rule.class.is_none()
        && rule.title.is_none()
    {
        return false;
    }
    let app_ok = match (&rule.app_id, &candidate.app_id) {
        (Some(rule), Some(candidate)) => rule.eq_ignore_ascii_case(candidate),
        (Some(_), None) => false,
        (None, _) => true,
    };
    let process_ok = match (&rule.process, &candidate.process) {
        (Some(rule), Some(candidate)) => rule.eq_ignore_ascii_case(candidate),
        (Some(_), None) => false,
        (None, _) => true,
    };
    let class_ok = match (&rule.class, &candidate.class) {
        (Some(rule), Some(candidate)) => rule.eq_ignore_ascii_case(candidate),
        (Some(_), None) => false,
        (None, _) => true,
    };
    let title_ok = match (&rule.title, &candidate.title) {
        (Some(rule), Some(candidate)) => matches_title_regex(rule, candidate),
        (Some(_), None) => false,
        (None, _) => true,
    };
    app_ok && process_ok && class_ok && title_ok
}

#[cfg(windows)]
fn match_score(saved: &LayoutMatch, candidate: &LayoutMatch) -> Option<u8> {
    if saved.app_id.is_none()
        && saved.process.is_none()
        && saved.class.is_none()
        && saved.title.is_none()
    {
        return None;
    }
    if let (Some(saved), Some(candidate)) = (&saved.app_id, &candidate.app_id) {
        if saved.eq_ignore_ascii_case(candidate) {
            return Some(4);
        }
    }
    if let (Some(saved), Some(candidate)) = (&saved.process, &candidate.process) {
        if saved.eq_ignore_ascii_case(candidate) {
            return Some(3);
        }
    }
    if let (Some(saved), Some(candidate)) = (&saved.class, &candidate.class) {
        if saved.eq_ignore_ascii_case(candidate) {
            return Some(2);
        }
    }
    if let (Some(saved), Some(candidate)) = (&saved.title, &candidate.title) {
        if matches_title_regex(saved, candidate) {
            return Some(1);
        }
    }
    None
}

#[cfg(windows)]
fn layout_health(layout: &Layout, candidates: &[LayoutMatch]) -> Option<LayoutHealth> {
    let available: Vec<LayoutMatch> = candidates
        .iter()
        .filter(|candidate| {
            !layout
                .ignore
                .iter()
                .any(|rule| is_rule_match(rule, candidate))
        })
        .cloned()
        .collect();
    let mut used = vec![false; available.len()];
    let mut matched = 0;

    for saved in &layout.windows {
        let mut best_idx = None;
        let mut best_score = 0u8;
        for (idx, candidate) in available.iter().enumerate() {
            if used[idx] {
                continue;
            }
            if let Some(score) = match_score(&saved.matcher, candidate) {
                if score > best_score {
                    best_score = score;
                    best_idx = Some(idx);
                }
            }
        }
        if let Some(idx) = best_idx {
            used[idx] = true;
            matched += 1;
        }
    }

    Some(LayoutHealth {
        matched,
        expected: layout.windows.len(),
    })
}

#[cfg(not(windows))]
fn layout_health(_layout: &Layout, _candidates: &[LayoutMatch]) -> Option<LayoutHealth> {
    None
}
