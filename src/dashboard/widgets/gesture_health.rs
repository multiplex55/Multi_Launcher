use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use crate::dashboard::widgets::{Widget, WidgetAction};
use crate::mouse_gestures::stats::gesture_stats;
use eframe::egui;

#[derive(Default)]
pub struct GestureHealthWidget;

impl GestureHealthWidget {
    pub fn new(_cfg: ()) -> Self {
        Self
    }

    fn action(label: &str, action: &str) -> WidgetAction {
        WidgetAction {
            action: Action {
                label: label.into(),
                desc: "Mouse gestures".into(),
                action: action.into(),
                args: None,
                preview_text: None,
                risk_level: None,
                icon: None,
            },
            query_override: None,
        }
    }
}

impl Widget for GestureHealthWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        let snapshot = ctx.data_cache.snapshot();
        let stats = gesture_stats(&snapshot.gestures.db);

        ui.label("Gesture health");
        egui::Grid::new("gesture_health_stats")
            .striped(true)
            .show(ui, |ui| {
                ui.label("Zero bindings");
                ui.label(stats.zero_bindings.to_string());
                ui.end_row();
                ui.label("Duplicate tokens");
                ui.label(stats.duplicate_tokens.to_string());
                ui.end_row();
                ui.label("Disabled gestures");
                ui.label(stats.disabled_gestures.to_string());
                ui.end_row();
            });

        ui.separator();
        ui.label("Quick actions");
        let mut clicked = None;
        ui.horizontal_wrapped(|ui| {
            if ui.button("Edit gestures").clicked() {
                clicked = Some(Self::action("Edit gestures", "mg:dialog"));
            }
            if ui.button("Add gesture").clicked() {
                clicked = Some(Self::action("Add gesture", "mg:dialog:add"));
            }
            if ui.button("Add binding").clicked() {
                clicked = Some(Self::action("Add binding", "mg:dialog:binding"));
            }
            if ui.button("Settings").clicked() {
                clicked = Some(Self::action("Settings", "mg:dialog:settings"));
            }
        });

        clicked
    }
}
