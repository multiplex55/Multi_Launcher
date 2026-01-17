use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use crate::dashboard::diagnostics::{DashboardDiagnosticsSnapshot, REFRESH_WARNING_THRESHOLD};
use crate::dashboard::widgets::{Widget, WidgetAction};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[derive(Default, Serialize, Deserialize)]
pub struct DiagnosticsSettings;

#[derive(Default)]
pub struct DiagnosticsWidget;

impl DiagnosticsWidget {
    pub fn new(_settings: DiagnosticsSettings) -> Self {
        Self
    }

    fn format_duration(duration: Duration) -> String {
        if duration.as_secs() >= 1 {
            format!("{:.2}s", duration.as_secs_f32())
        } else {
            format!("{:.1}ms", duration.as_secs_f32() * 1000.0)
        }
    }

    fn format_elapsed(now: Instant, then: Instant) -> String {
        Self::format_duration(now.duration_since(then))
    }

    fn render_frame_metrics(ui: &mut egui::Ui, diagnostics: &DashboardDiagnosticsSnapshot) {
        let frame_ms = diagnostics.frame_time.as_secs_f32() * 1000.0;
        ui.label(format!("FPS: {:.1}", diagnostics.fps));
        ui.label(format!("Frame time: {:.2} ms", frame_ms));
    }
}

impl Widget for DiagnosticsWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        let Some(diagnostics) = ctx.diagnostics.as_ref() else {
            ui.label("Diagnostics unavailable");
            return None;
        };

        ui.vertical(|ui| {
            Self::render_frame_metrics(ui, diagnostics);
            ui.separator();

            let snapshot = ctx.data_cache.snapshot();
            ui.label(format!(
                "Cache sizes: clipboard {}, todos {}, notes {}, snippets {}",
                snapshot.clipboard_history.len(),
                snapshot.todos.len(),
                snapshot.notes.len(),
                snapshot.snippets.len()
            ));
            ui.separator();

            ui.label("Widget refresh");
            egui::Grid::new("diagnostics-widget-refresh")
                .striped(true)
                .show(ui, |ui| {
                    ui.label("Widget");
                    ui.label("Last refresh");
                    ui.label("Duration");
                    ui.end_row();

                    let now = Instant::now();
                    for stat in &diagnostics.widget_refreshes {
                        ui.label(stat.label.as_str());
                        ui.label(Self::format_elapsed(now, stat.last_refresh_at));
                        ui.horizontal(|ui| {
                            ui.label(Self::format_duration(stat.last_duration));
                            if stat.last_duration >= REFRESH_WARNING_THRESHOLD {
                                ui.colored_label(egui::Color32::YELLOW, "⚠");
                            }
                        });
                        ui.end_row();
                    }
                });
        });

        None
    }
}
