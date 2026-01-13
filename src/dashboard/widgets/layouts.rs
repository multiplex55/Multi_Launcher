use super::{edit_typed_settings, TimedCache, Widget, WidgetAction, WidgetSettingsContext, WidgetSettingsUiResult};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use crate::plugins::layouts_storage::{self, Layout, LayoutMatch, LAYOUTS_FILE};
use crate::windows_layout::{collect_layout_windows, LayoutWindowOptions};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::time::Duration;

fn default_refresh_interval() -> f32 {
    30.0
}

fn default_show_health_indicator() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutsConfig {
    #[serde(default = "default_refresh_interval")]
    pub refresh_interval_secs: f32,
    #[serde(default = "default_show_health_indicator")]
    pub show_health_indicator: bool,
}

impl Default for LayoutsConfig {
    fn default() -> Self {
        Self {
            refresh_interval_secs: default_refresh_interval(),
            show_health_indicator: default_show_health_indicator(),
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

pub struct LayoutsWidget {
    cfg: LayoutsConfig,
    cache: TimedCache<LayoutsData>,
    error: Option<String>,
    refresh_pending: bool,
    rename_target: Option<String>,
    rename_value: String,
}

impl LayoutsWidget {
    pub fn new(cfg: LayoutsConfig) -> Self {
        let interval = Duration::from_secs_f32(cfg.refresh_interval_secs.max(1.0));
        Self {
            cfg,
            cache: TimedCache::new(LayoutsData { layouts: Vec::new() }, interval),
            error: None,
            refresh_pending: false,
            rename_target: None,
            rename_value: String::new(),
        }
    }

    pub fn settings_ui(
        ui: &mut egui::Ui,
        value: &mut serde_json::Value,
        ctx: &WidgetSettingsContext<'_>,
    ) -> WidgetSettingsUiResult {
        edit_typed_settings(ui, value, ctx, |ui, cfg: &mut LayoutsConfig, _ctx| {
            let mut changed = false;
            ui.horizontal(|ui| {
                ui.label("Refresh every");
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut cfg.refresh_interval_secs)
                            .clamp_range(1.0..=300.0)
                            .speed(0.5),
                    )
                    .changed();
                ui.label("seconds");
            });
            changed |= ui
                .checkbox(&mut cfg.show_health_indicator, "Show health indicator")
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

    fn maybe_refresh(&mut self, visible: bool) {
        if !visible {
            return;
        }
        self.update_interval();
        if self.refresh_pending {
            self.refresh_pending = false;
            self.refresh();
        } else if self.cache.should_refresh() {
            self.refresh();
        }
    }

    fn load_layouts(cfg: &LayoutsConfig) -> (LayoutsData, Option<String>) {
        let store = match layouts_storage::load_layouts(LAYOUTS_FILE) {
            Ok(store) => store,
            Err(err) => {
                return (
                    LayoutsData { layouts: Vec::new() },
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
        let candidates: Vec<LayoutMatch> = windows.into_iter().map(|window| window.matcher).collect();
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
        self.maybe_refresh(visible);

        if let Some(err) = &self.error {
            ui.colored_label(egui::Color32::YELLOW, err);
        }

        if self.cache.data.layouts.is_empty() {
            ui.label("No layouts saved.");
            return None;
        }

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

    fn header_ui(&mut self, ui: &mut egui::Ui, _ctx: &DashboardContext<'_>) -> Option<WidgetAction> {
        let tooltip = format!(
            "Cached for {:.0}s. Refresh to rescan layouts.",
            self.cfg.refresh_interval_secs
        );
        if ui.small_button("Refresh").on_hover_text(tooltip).clicked() {
            self.refresh();
        }
        None
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
        .filter(|candidate| !layout.ignore.iter().any(|rule| is_rule_match(rule, candidate)))
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
