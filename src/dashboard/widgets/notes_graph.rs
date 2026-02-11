use super::{
    edit_typed_settings, Widget, WidgetAction, WidgetSettingsContext, WidgetSettingsUiResult,
};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use crate::graph::note_graph::{
    build_draw_primitives, LayoutConfig, NoteGraphEngine, NoteGraphFilter, RenderSurface,
};
use crate::plugins::note::Note;
use eframe::egui::{self, Color32, Pos2, Stroke};
use serde::{Deserialize, Serialize};

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
    engine: NoteGraphEngine,
    last_rendered_version: Option<u64>,
    cached_slugs: Vec<String>,
}

impl NotesGraphWidget {
    pub fn new(cfg: NotesGraphConfig) -> Self {
        Self {
            cfg,
            engine: NoteGraphEngine::default(),
            last_rendered_version: None,
            cached_slugs: Vec::new(),
        }
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

    fn sync_from_snapshot(&mut self, notes: &[Note], notes_version: u64) -> bool {
        let rebuilt = self.engine.rebuild_if_needed(
            notes,
            notes_version,
            &NoteGraphFilter {
                include_backlinks: true,
                ..NoteGraphFilter::default()
            },
        );

        if rebuilt {
            self.cached_slugs = self
                .engine
                .model
                .nodes
                .iter()
                .map(|n| n.id.clone())
                .take(self.cfg.max_nodes.clamp(4, 64))
                .collect();
            self.last_rendered_version = Some(notes_version);
        }

        rebuilt
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
        let snapshot = ctx.data_cache.snapshot();
        let notes: Vec<Note> = snapshot.notes.iter().cloned().collect();
        self.sync_from_snapshot(&notes, ctx.notes_version);

        let can_animate = !ctx.reduce_dashboard_work_when_unfocused || ctx.dashboard_focused;
        if can_animate {
            self.engine.layout.step(
                &self.engine.model,
                LayoutConfig {
                    iterations_per_frame: 1,
                    ..LayoutConfig::default()
                },
            );
        }

        if self.engine.model.nodes.is_empty() {
            ui.label("No note links yet.");
            return None;
        }

        let target_height = if ui.available_height() < 150.0 {
            120.0
        } else {
            180.0
        };
        let desired = egui::vec2(ui.available_width().max(180.0), target_height);
        let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);

        let draw = build_draw_primitives(
            &self.engine.model,
            &self.engine.layout,
            RenderSurface {
                center: [rect.center().x, rect.center().y],
                pan: [0.0, 0.0],
                zoom: 1.0,
            },
            self.cfg.max_nodes.clamp(4, 64),
        );

        for edge in &draw.edges {
            let a = Pos2::new(edge.from[0], edge.from[1]);
            let b = Pos2::new(edge.to[0], edge.to[1]);
            painter.line_segment([a, b], Stroke::new(1.0, Color32::LIGHT_BLUE));
        }

        for node in &draw.nodes {
            painter.circle_filled(
                Pos2::new(node.screen[0], node.screen[1]),
                6.0,
                Color32::from_rgb(90, 170, 120),
            );
        }

        let mut clicked = None;
        ui.separator();
        for slug in self
            .cached_slugs
            .iter()
            .take(self.cfg.max_nodes.clamp(4, 64))
        {
            if ui.link(slug).clicked() {
                clicked = Some(WidgetAction {
                    action: Action {
                        label: format!("Open {slug}"),
                        desc: "Note".into(),
                        action: format!("note:open:{slug}"),
                        args: None,
                    },
                    query_override: Some(format!("note open {slug}")),
                });
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn note(slug: &str, links: &[&str]) -> Note {
        Note {
            title: slug.to_string(),
            path: PathBuf::from(format!("{slug}.md")),
            content: String::new(),
            tags: Vec::new(),
            links: links.iter().map(|v| v.to_string()).collect(),
            slug: slug.to_string(),
            alias: None,
            entity_refs: Vec::new(),
        }
    }

    #[test]
    fn cached_graph_refreshes_only_when_notes_version_changes() {
        let mut widget = NotesGraphWidget::default();
        let notes = vec![note("a", &["b"]), note("b", &[])];
        assert!(widget.sync_from_snapshot(&notes, 1));
        assert_eq!(widget.last_rendered_version, Some(1));

        let notes_changed = vec![note("a", &["b", "c"]), note("b", &[]), note("c", &[])];
        assert!(!widget.sync_from_snapshot(&notes_changed, 1));
        assert_eq!(widget.engine.model.nodes.len(), 2);

        assert!(widget.sync_from_snapshot(&notes_changed, 2));
        assert_eq!(widget.engine.model.nodes.len(), 3);
    }
}
