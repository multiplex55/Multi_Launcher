use super::*;
use multi_launcher::gui::theme::{SemanticPalette, ThemeMetrics};

#[derive(IntoElement)]
pub(super) struct LauncherUtilityControls {
    root: WeakEntity<RootWindow>,
    metrics: ThemeMetrics,
    palette: SemanticPalette,
    docked_hash: bool,
    coordinate_status: String,
    restart_status: Option<String>,
    toast_log_error: Option<String>,
    secondary_error: Option<String>,
    hash_open_error: Option<String>,
    hash_presentation_error: Option<String>,
    hash_native_geometry_error: Option<String>,
    hash_size_write_error: Option<String>,
}

impl LauncherUtilityControls {
    pub(super) fn new(
        state: &RootWindow,
        root: WeakEntity<RootWindow>,
        metrics: ThemeMetrics,
        palette: SemanticPalette,
        docked_hash: bool,
    ) -> Self {
        Self {
            root,
            metrics,
            palette,
            docked_hash,
            coordinate_status: state.coordinate_hud_probe_status.clone(),
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
        let secondary = self.root.clone();
        let coordinate = self.root.clone();
        let hash = self.root.clone();
        let restart = self.root.clone();
        let quit = self.root;
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
            .child(status_label(
                "retained-window-instructions",
                "Use the button to open or dismiss the retained window".to_owned(),
            ))
            .child(
                Button::new("retained-window")
                    .w_full()
                    .min_w(px(0.0))
                    .label("Open or dismiss retained window")
                    .tab_stop(true)
                    .on_click(move |_, _, cx| {
                        let _ = secondary.update(cx, |root, cx| root.open_or_toggle_secondary(cx));
                    }),
            )
            .child(
                Button::new("coordinate-hud-probe")
                    .w_full()
                    .min_w(px(0.0))
                    .label("Toggle passive Coordinate HUD (F6)")
                    .tab_stop(true)
                    .on_click(move |_, _, cx| {
                        let _ = coordinate.update(cx, |root, cx| root.toggle_coordinate_hud_probe(cx));
                    }),
            )
            .child(
                Button::new("open-hash-generator")
                    .w_full()
                    .min_w(px(0.0))
                    .label("Open Hash Generator (Ctrl+G)")
                    .tab_stop(true)
                    .on_click(move |_, window, cx| {
                        let _ = hash.update(cx, |root, cx| root.run_hash_generator_open(window, cx));
                    }),
            )
            .children(self.docked_hash.then(|| {
                status_label(
                    "docked-hash-resize-instructions",
                    "Ctrl+Shift+R: focus docked resize grip · arrows preview · Enter saves · Escape discards"
                        .to_owned(),
                )
            }))
            .child(
                Button::new("restart-egui")
                    .w_full()
                    .min_w(px(0.0))
                    .label("Restart in egui (Ctrl+Shift+E)")
                    .tab_stop(true)
                    .on_click(move |_, window, cx| {
                        let _ = restart.update(cx, |root, cx| root.open_restart_confirmation(window, cx));
                    }),
            )
            .child(status_label("coordinate-hud-status", self.coordinate_status))
            .children(self.restart_status.map(|status| {
                status_label("restart-status", status)
                    .text_color(appearance::to_hsla(palette.warning))
            }))
            .children(self.toast_log_error.map(|error| {
                status_label("toast-log-open-error", error)
                    .text_color(appearance::to_hsla(palette.error))
            }))
            .child(
                Button::new("quit-gpui")
                    .w_full()
                    .min_w(px(0.0))
                    .label("Quit Multi Launcher")
                    .tab_stop(true)
                    .on_click(move |_, _, cx| {
                        let _ = quit.update(cx, |root, cx| root.request_quit(cx));
                    }),
            )
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
                status_label("hash-native-geometry-error", error)
                    .text_color(appearance::to_hsla(palette.warning))
            }))
            .children(self.hash_size_write_error.map(|error| {
                status_label("hash-size-write-error", error)
                    .text_color(appearance::to_hsla(palette.error))
            }))
    }
}
