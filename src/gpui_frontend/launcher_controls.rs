//! Conditional launcher utility feedback, kept out of the ordinary Search view.
use super::*;
use multi_launcher::gui::theme::{SemanticPalette, ThemeMetrics};

const IDLE_COORDINATE_STATUS: &str = "Coordinate HUD probe is idle.";

#[derive(IntoElement)]
pub(super) struct LauncherUtilityControls {
    metrics: ThemeMetrics,
    palette: SemanticPalette,
    coordinate_status: Option<String>,
    restart_status: Option<String>,
    toast_log_error: Option<String>,
    secondary_error: Option<String>,
    hash_open_error: Option<String>,
    hash_presentation_error: Option<String>,
    hash_native_geometry_error: Option<String>,
    hash_size_write_error: Option<String>,
}

impl LauncherUtilityControls {
    pub(super) fn visible(state: &RootWindow) -> bool {
        state.coordinate_hud_probe_status != IDLE_COORDINATE_STATUS
            || state.restart_status.is_some()
            || state.toast_log_external_open_error.is_some()
            || state.secondary_error.is_some()
            || state.hash_open_error.is_some()
            || state.hash_presentation_error.is_some()
            || state.hash_native_geometry_error.is_some()
            || state.hash_size_write_error.is_some()
    }

    pub(super) fn new(
        state: &RootWindow,
        metrics: ThemeMetrics,
        palette: SemanticPalette,
    ) -> Self {
        Self {
            metrics,
            palette,
            coordinate_status: (state.coordinate_hud_probe_status != IDLE_COORDINATE_STATUS)
                .then(|| state.coordinate_hud_probe_status.clone()),
            restart_status: state.restart_status.clone(),
            toast_log_error: state.toast_log_external_open_error.clone(),
            secondary_error: state.secondary_error.clone(),
            hash_open_error: state.hash_open_error.clone(),
            hash_presentation_error: state.hash_presentation_error.clone(),
            hash_native_geometry_error: state.hash_native_geometry_error.clone(),
            hash_size_write_error: state.hash_size_write_error.clone(),
        }
    }
}

impl RenderOnce for LauncherUtilityControls {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let metrics = self.metrics;
        let palette = self.palette;
        let status_label = |id, text: String| {
            Label::new(text)
                .id(id)
                .w_full()
                .min_w(px(0.0))
                .overflow_hidden()
                .text_size(px(metrics.small_text_size))
                .line_height(px(metrics.line_height))
        };
        div()
            .flex()
            .flex_col()
            .gap_y(px(metrics.item_spacing_y))
            .w_full()
            .children(self.coordinate_status.map(|status| status_label("coordinate-hud-status", status)))
            .children(self.restart_status.map(|status| {
                status_label("restart-status", status).text_color(appearance::to_hsla(palette.warning))
            }))
            .children(self.toast_log_error.map(|error| {
                status_label("toast-log-open-error", error).text_color(appearance::to_hsla(palette.error))
            }))
            .children(
                [
                    ("retained-window-error", self.secondary_error),
                    ("hash-open-error", self.hash_open_error),
                    ("hash-presentation-error", self.hash_presentation_error),
                ]
                .into_iter()
                .filter_map(|(id, error)| {
                    Some(status_label(id, error?).text_color(appearance::to_hsla(palette.error)))
                }),
            )
            .children(self.hash_native_geometry_error.map(|error| {
                status_label("hash-native-geometry-error", error).text_color(appearance::to_hsla(palette.warning))
            }))
            .children(self.hash_size_write_error.map(|error| {
                status_label("hash-size-write-error", error).text_color(appearance::to_hsla(palette.error))
            }))
    }
}
