use super::{Widget, WidgetAction};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use std::collections::BTreeMap;

#[derive(Default)]
pub struct ContextLinksWidget;

impl ContextLinksWidget {
    pub fn new(_: ()) -> Self {
        Self
    }
}

impl Widget for ContextLinksWidget {
    fn render(
        &mut self,
        ui: &mut eframe::egui::Ui,
        ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        let snapshot = ctx.data_cache.snapshot();
        let mut bundles: BTreeMap<String, (usize, usize)> = BTreeMap::new();
        for note in snapshot.notes.iter() {
            for tag in &note.tags {
                let e = bundles.entry(tag.to_lowercase()).or_default();
                e.0 += 1;
            }
        }
        for todo in snapshot.todos.iter() {
            for tag in &todo.tags {
                let e = bundles.entry(tag.to_lowercase()).or_default();
                e.1 += 1;
            }
        }
        if bundles.is_empty() {
            ui.label("No context links yet.");
            return None;
        }
        let mut out = None;
        for (tag, (notes, todos)) in bundles.into_iter().take(10) {
            ui.horizontal(|ui| {
                ui.label(format!("#{tag}"));
                if ui.link(format!("notes: {notes}")).clicked() {
                    out = Some(WidgetAction {
                        action: Action {
                            label: format!("notes #{tag}"),
                            desc: "Note".into(),
                            action: format!("query:note list #{tag}"),
                            args: None,
                        },
                        query_override: Some(format!("note list #{tag}")),
                    });
                }
                if ui.link(format!("todos: {todos}")).clicked() {
                    out = Some(WidgetAction {
                        action: Action {
                            label: format!("todos #{tag}"),
                            desc: "Todo".into(),
                            action: format!("query:todo list #{tag}"),
                            args: None,
                        },
                        query_override: Some(format!("todo list #{tag}")),
                    });
                }
            });
        }
        out
    }
}
