use super::{
    edit_typed_settings, Widget, WidgetAction, WidgetSettingsContext, WidgetSettingsUiResult,
};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use crate::plugins::note::note_relationship_edges;
use eframe::egui::{self, Color32, Pos2, Stroke};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

fn default_max_nodes() -> usize {
    16
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotesGraphConfig {
    #[serde(default = "default_max_nodes")]
    pub max_nodes: usize,
}

impl Default for NotesGraphConfig {
    fn default() -> Self {
        Self {
            max_nodes: default_max_nodes(),
        }
    }
}

pub struct NotesGraphWidget {
    cfg: NotesGraphConfig,
}

impl NotesGraphWidget {
    pub fn new(cfg: NotesGraphConfig) -> Self {
        Self { cfg }
    }

    pub fn settings_ui(
        ui: &mut egui::Ui,
        value: &mut serde_json::Value,
        ctx: &WidgetSettingsContext<'_>,
    ) -> WidgetSettingsUiResult {
        edit_typed_settings(ui, value, ctx, |ui, cfg: &mut NotesGraphConfig, _ctx| {
            ui.horizontal(|ui| {
                ui.label("Max nodes");
                ui.add(egui::DragValue::new(&mut cfg.max_nodes).clamp_range(4..=64))
                    .changed()
            })
            .inner
        })
    }
}

impl Default for NotesGraphWidget {
    fn default() -> Self {
        Self::new(NotesGraphConfig::default())
    }
}

impl Widget for NotesGraphWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        let edges = note_relationship_edges();
        let mut nodes: BTreeSet<String> = BTreeSet::new();
        for (a, b) in &edges {
            let _ = nodes.insert(a.clone());
            let _ = nodes.insert(b.clone());
        }
        if nodes.is_empty() {
            ui.label("No note links yet.");
            return None;
        }
        let node_slugs: Vec<String> = nodes.into_iter().take(self.cfg.max_nodes).collect();
        let n = node_slugs.len().max(1);

        let desired = egui::vec2(ui.available_width().max(180.0), 180.0);
        let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
        let painter = ui.painter_at(rect);
        let center = rect.center();
        let radius = rect.width().min(rect.height()) * 0.35;

        let mut pos = std::collections::HashMap::new();
        for (i, slug) in node_slugs.iter().enumerate() {
            let theta = (i as f32 / n as f32) * std::f32::consts::TAU;
            let p = Pos2::new(
                center.x + radius * theta.cos(),
                center.y + radius * theta.sin(),
            );
            pos.insert(slug.clone(), p);
        }

        for (from, to) in &edges {
            if let (Some(a), Some(b)) = (pos.get(from), pos.get(to)) {
                painter.line_segment([*a, *b], Stroke::new(1.0, Color32::LIGHT_BLUE));
            }
        }

        for slug in &node_slugs {
            if let Some(p) = pos.get(slug) {
                painter.circle_filled(*p, 6.0, Color32::from_rgb(90, 170, 120));
            }
        }

        let mut clicked = None;
        ui.separator();
        for slug in &node_slugs {
            if ui.link(slug).clicked() {
                clicked = Some(WidgetAction {
                    action: Action {
                        label: format!("Open {slug}"),
                        desc: "Note".into(),
                        action: format!("note:open:{slug}"),
                        args: None,
                        preview_text: None,
                        risk_level: None,
                        icon: None,
                    },
                    query_override: Some(format!("note open {slug}")),
                });
            }
        }

        if clicked.is_none() {
            let snapshot = ctx.data_cache.snapshot();
            let names: Vec<_> = snapshot
                .notes
                .iter()
                .take(3)
                .map(|n| n.title.clone())
                .collect();
            if !names.is_empty() {
                ui.label(format!("Examples: {}", names.join(", ")));
            }
        }

        clicked
    }

    fn on_config_updated(&mut self, settings: &serde_json::Value) {
        if let Ok(cfg) = serde_json::from_value::<NotesGraphConfig>(settings.clone()) {
            self.cfg = cfg;
        }
    }
}
