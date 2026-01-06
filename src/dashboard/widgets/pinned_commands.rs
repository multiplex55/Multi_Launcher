use super::{
    edit_typed_settings, Widget, WidgetAction, WidgetSettingsContext, WidgetSettingsUiResult,
};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use crate::plugins::fav::{load_favs, FavEntry, FAV_FILE};
use eframe::egui;
use serde::{Deserialize, Serialize};

fn default_layout() -> PinnedLayout {
    PinnedLayout::List
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PinnedLayout {
    Grid,
    List,
}

impl Default for PinnedLayout {
    fn default() -> Self {
        PinnedLayout::List
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PinnedCommandsConfig {
    #[serde(default)]
    pub action_ids: Vec<String>,
    #[serde(default = "default_layout")]
    pub layout: PinnedLayout,
}

pub struct PinnedCommandsWidget {
    cfg: PinnedCommandsConfig,
    cached_favorites: Vec<FavEntry>,
    cached_resolved: Vec<Action>,
    last_actions_version: u64,
    last_fav_version: u64,
}

impl PinnedCommandsWidget {
    pub fn new(cfg: PinnedCommandsConfig) -> Self {
        Self {
            cfg,
            cached_favorites: Vec::new(),
            cached_resolved: Vec::new(),
            last_actions_version: u64::MAX,
            last_fav_version: u64::MAX,
        }
    }

    pub fn settings_ui(
        ui: &mut egui::Ui,
        value: &mut serde_json::Value,
        ctx: &WidgetSettingsContext<'_>,
    ) -> WidgetSettingsUiResult {
        edit_typed_settings(ui, value, ctx, |ui, cfg: &mut PinnedCommandsConfig, ctx| {
            let mut changed = false;
            let options = available_choices(ctx);
            if cfg.action_ids.is_empty() && !options.is_empty() {
                cfg.action_ids.push(options[0].0.clone());
                changed = true;
            }

            let mut remove_idx = None;
            for (idx, action_id) in cfg.action_ids.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("Action {}", idx + 1));
                    egui::ComboBox::from_id_source(format!("pin-choice-{idx}"))
                        .selected_text(label_for(action_id, &options))
                        .show_ui(ui, |ui| {
                            for (id, label) in &options {
                                changed |=
                                    ui.selectable_value(action_id, id.clone(), label).changed();
                            }
                        });
                    if ui.small_button("✕").clicked() {
                        remove_idx = Some(idx);
                    }
                });
            }
            if let Some(idx) = remove_idx {
                cfg.action_ids.remove(idx);
                changed = true;
            }
            if ui.button("Add action").clicked() {
                cfg.action_ids.push(
                    options
                        .first()
                        .map(|o| o.0.clone())
                        .unwrap_or_else(|| "".into()),
                );
                changed = true;
            }

            egui::ComboBox::from_label("Layout")
                .selected_text(match cfg.layout {
                    PinnedLayout::Grid => "Grid",
                    PinnedLayout::List => "List",
                })
                .show_ui(ui, |ui| {
                    changed |= ui
                        .selectable_value(&mut cfg.layout, PinnedLayout::Grid, "Grid")
                        .changed();
                    changed |= ui
                        .selectable_value(&mut cfg.layout, PinnedLayout::List, "List")
                        .changed();
                });
            changed
        })
    }

    fn resolve_action<'a>(
        &self,
        actions: &'a [Action],
        favorites: &'a [FavEntry],
        id: &str,
    ) -> Option<Action> {
        if let Some(label) = id.strip_prefix("fav:") {
            if let Some(f) = favorites
                .iter()
                .find(|f| f.label.eq_ignore_ascii_case(label))
            {
                return Some(Action {
                    label: f.label.clone(),
                    desc: "Fav".into(),
                    action: f.action.clone(),
                    args: f.args.clone(),
                });
            }
        }

        actions
            .iter()
            .find(|a| a.action == id)
            .cloned()
            .or_else(|| {
                if let Some(fav) = favorites.iter().find(|f| f.action == id) {
                    Some(Action {
                        label: fav.label.clone(),
                        desc: "Fav".into(),
                        action: fav.action.clone(),
                        args: fav.args.clone(),
                    })
                } else {
                    None
                }
            })
    }

    fn refresh_cache(&mut self, ctx: &DashboardContext<'_>) {
        if self.last_actions_version == ctx.actions_version
            && self.last_fav_version == ctx.fav_version
        {
            return;
        }

        self.cached_favorites = load_favs(FAV_FILE).unwrap_or_default();
        self.cached_resolved.clear();
        for id in &self.cfg.action_ids {
            if let Some(action) = self.resolve_action(ctx.actions, &self.cached_favorites, id) {
                self.cached_resolved.push(action);
            }
        }
        self.last_actions_version = ctx.actions_version;
        self.last_fav_version = ctx.fav_version;
    }
}

impl Default for PinnedCommandsWidget {
    fn default() -> Self {
        Self {
            cfg: PinnedCommandsConfig::default(),
            cached_favorites: Vec::new(),
            cached_resolved: Vec::new(),
            last_actions_version: u64::MAX,
            last_fav_version: u64::MAX,
        }
    }
}

impl Widget for PinnedCommandsWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        self.refresh_cache(ctx);
        if self.cached_resolved.is_empty() {
            ui.label("Pick actions or favorites in the widget settings.");
            return None;
        }

        match self.cfg.layout {
            PinnedLayout::List => {
                for action in &self.cached_resolved {
                    if ui.button(&action.label).clicked() {
                        return Some(WidgetAction {
                            query_override: Some(action.label.clone()),
                            action: action.clone(),
                        });
                    }
                }
            }
            PinnedLayout::Grid => {
                let mut clicked = None;
                ui.horizontal_wrapped(|ui| {
                    for action in &self.cached_resolved {
                        if ui.button(&action.label).clicked() {
                            clicked = Some(action.clone());
                        }
                    }
                });
                if let Some(action) = clicked {
                    return Some(WidgetAction {
                        query_override: Some(action.label.clone()),
                        action,
                    });
                }
            }
        }

        None
    }
}

fn available_choices(ctx: &WidgetSettingsContext<'_>) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Some(actions) = ctx.actions {
        for a in actions {
            out.push((a.action.clone(), format!("{} ({})", a.label, a.action)));
        }
    }
    if let Ok(favs) = load_favs(FAV_FILE) {
        for f in favs {
            out.push((format!("fav:{}", f.label), format!("Favorite: {}", f.label)));
        }
    }
    out.sort_by(|a, b| a.1.cmp(&b.1));
    out.dedup_by(|a, b| a.0 == b.0);
    out
}

fn label_for(id: &str, options: &[(String, String)]) -> String {
    options
        .iter()
        .find(|(opt_id, _)| opt_id == id)
        .map(|(_, label)| label.clone())
        .unwrap_or_else(|| id.to_string())
}
