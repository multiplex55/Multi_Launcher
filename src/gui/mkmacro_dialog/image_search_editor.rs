//! Reusable image-search action editor and its UI-only authoring state.
use crate::mkmacro::{
    AlphaPolicy, MkImagePayload, MkWindowMatcher, MonitorDescriptor, ReturnPoint, ScreenRect,
    SearchRegion,
};
use eframe::egui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchRegionKind {
    Desktop,
    Monitor,
    Rectangle,
    Window,
    ClientArea,
}

impl SearchRegionKind {
    pub fn from_region(region: &SearchRegion) -> Self {
        match region {
            SearchRegion::Desktop => Self::Desktop,
            SearchRegion::Monitor { .. } => Self::Monitor,
            SearchRegion::Rectangle { .. } => Self::Rectangle,
            SearchRegion::Window { .. } => Self::Window,
            SearchRegion::ClientArea { .. } => Self::ClientArea,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageEditorRequest {
    ImportPng,
    CaptureRectangle,
    PickRectangle,
    PreviewRectangle,
    HighlightMonitor,
    IdentifyMonitors,
    PickWindow { client_area: bool },
    HighlightWindow { client_area: bool },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageSearchEditorState {
    pub kind: SearchRegionKind,
    pub monitor_index: usize,
    pub rectangle: ScreenRect,
    pub window_matcher: MkWindowMatcher,
    pub client_matcher: MkWindowMatcher,
    pub monitors: Result<Vec<MonitorDescriptor>, String>,
    pub pending_request: Option<ImageEditorRequest>,
    pub preview_error: Option<String>,
}

impl ImageSearchEditorState {
    pub fn from_payload(payload: &MkImagePayload) -> Self {
        let mut state = Self {
            kind: SearchRegionKind::from_region(&payload.region),
            monitor_index: 0,
            rectangle: ScreenRect::new(0, 0, 640, 480),
            window_matcher: MkWindowMatcher::default(),
            client_matcher: MkWindowMatcher::default(),
            monitors: crate::mkmacro::monitor_descriptors().map_err(|e| e.to_string()),
            pending_request: None,
            preview_error: None,
        };
        match &payload.region {
            SearchRegion::Monitor { index } => state.monitor_index = *index,
            SearchRegion::Rectangle { rect } => state.rectangle = *rect,
            SearchRegion::Window { matcher } => state.window_matcher = matcher.clone(),
            SearchRegion::ClientArea { matcher } => state.client_matcher = matcher.clone(),
            SearchRegion::Desktop => {}
        }
        state
    }

    pub fn select(&mut self, kind: SearchRegionKind) {
        self.kind = kind;
    }
    /// Refreshes display metadata without changing the persisted playback
    /// index.  In particular, disconnecting a display must not silently point
    /// an existing action at a different monitor.
    pub fn refresh_monitors(&mut self) {
        self.monitors = crate::mkmacro::monitor_descriptors().map_err(|e| e.to_string());
    }

    pub fn validation_error(&self) -> Option<String> {
        match self.kind {
            SearchRegionKind::Rectangle if self.rectangle.is_empty() => {
                Some("Rectangle width and height must be positive".into())
            }
            SearchRegionKind::Monitor => match &self.monitors {
                Ok(monitors) if !monitors.iter().any(|m| m.index == self.monitor_index) => Some(
                    format!("Monitor {} is currently unavailable", self.monitor_index),
                ),
                Err(error) => Some(format!("Monitor information unavailable: {error}")),
                _ => None,
            },
            _ => None,
        }
    }
    pub fn selected_region(&self) -> SearchRegion {
        match self.kind {
            SearchRegionKind::Desktop => SearchRegion::Desktop,
            SearchRegionKind::Monitor => SearchRegion::Monitor {
                index: self.monitor_index,
            },
            SearchRegionKind::Rectangle => SearchRegion::Rectangle {
                rect: self.rectangle,
            },
            SearchRegionKind::Window => SearchRegion::Window {
                matcher: self.window_matcher.clone(),
            },
            SearchRegionKind::ClientArea => SearchRegion::ClientArea {
                matcher: self.client_matcher.clone(),
            },
        }
    }
}

fn request(
    ui: &mut egui::Ui,
    label: &str,
    value: ImageEditorRequest,
    out: &mut Option<ImageEditorRequest>,
) {
    if ui.button(label).clicked() {
        *out = Some(value);
    }
}

fn disabled_request(
    ui: &mut egui::Ui,
    label: &str,
    enabled: bool,
    value: ImageEditorRequest,
    out: &mut Option<ImageEditorRequest>,
) {
    if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
        *out = Some(value);
    }
}

/// Renders both `ImageFind` and `ImageClick`. The returned value describes work
/// for the owner to perform after egui releases its widget borrows.
pub(super) fn show(
    ui: &mut egui::Ui,
    payload: &mut MkImagePayload,
    state: &mut ImageSearchEditorState,
    store: &crate::mkmacro::MkMacroStore,
    macro_id: u64,
    authoring_busy: bool,
) -> Option<ImageEditorRequest> {
    let mut out = None;
    ui.heading("Reference Image");
    ui.horizontal_wrapped(|ui| {
        disabled_request(
            ui,
            "Select PNG…",
            !authoring_busy,
            ImageEditorRequest::ImportPng,
            &mut out,
        );
        disabled_request(
            ui,
            "Capture…",
            !authoring_busy,
            ImageEditorRequest::CaptureRectangle,
            &mut out,
        );
    });
    if authoring_busy {
        ui.label("Importing reference image...");
    }
    if payload.asset_id == 0 {
        ui.label("No reference image selected.");
    } else {
        ui.label("Saved reference image");
        ui.small(format!(
            "mkmacro_assets/{macro_id}/{}.png",
            payload.asset_id
        ));
        super::image_preview::show(ui, store, macro_id, payload.asset_id);
    }

    ui.separator();
    ui.heading("Search Settings");
    ui.horizontal(|ui| {
        ui.label("Tolerance");
        ui.add(egui::Slider::new(&mut payload.tolerance, 0..=255));
    });
    egui::ComboBox::from_label("Alpha handling")
        .selected_text(format!("{:?}", payload.alpha))
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut payload.alpha, AlphaPolicy::Compare, "Compare alpha");
            ui.selectable_value(&mut payload.alpha, AlphaPolicy::Ignore, "Ignore alpha");
        });
    ui.horizontal_wrapped(|ui| {
        ui.label("Return point");
        ui.selectable_value(&mut payload.return_point, ReturnPoint::Center, "Center");
        ui.selectable_value(&mut payload.return_point, ReturnPoint::TopLeft, "Top-left");
    });
    ui.horizontal_wrapped(|ui| {
        ui.label("Timeout");
        ui.add(
            egui::DragValue::new(&mut payload.wait.timeout_ms)
                .clamp_range(1..=86_400_000)
                .suffix(" ms"),
        );
        ui.label("Polling interval");
        ui.add(
            egui::DragValue::new(&mut payload.wait.poll_interval_ms)
                .clamp_range(1..=86_400_000)
                .suffix(" ms"),
        );
    });

    ui.separator();
    ui.heading("Search Area");
    egui::ComboBox::from_label("Area")
        .selected_text(match state.kind {
            SearchRegionKind::Desktop => "Entire Desktop",
            SearchRegionKind::Monitor => "Selected Monitor",
            SearchRegionKind::Rectangle => "Rectangle",
            SearchRegionKind::Window => "Window",
            SearchRegionKind::ClientArea => "Window Client Area",
        })
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut state.kind, SearchRegionKind::Desktop, "Entire Desktop");
            ui.selectable_value(
                &mut state.kind,
                SearchRegionKind::Monitor,
                "Selected Monitor",
            );
            ui.selectable_value(&mut state.kind, SearchRegionKind::Rectangle, "Rectangle");
            ui.selectable_value(&mut state.kind, SearchRegionKind::Window, "Window");
            ui.selectable_value(
                &mut state.kind,
                SearchRegionKind::ClientArea,
                "Window Client Area",
            );
        });
    match state.kind {
        SearchRegionKind::Desktop => {
            ui.label("Searches the complete virtual desktop across all connected monitors.");
        }
        SearchRegionKind::Monitor => {
            if ui.button("Refresh").clicked() {
                state.refresh_monitors();
            }
            match &state.monitors {
                Ok(monitors) => {
                    let selected = monitors
                        .iter()
                        .find(|m| m.index == state.monitor_index)
                        .map(|m| m.label())
                        .unwrap_or_else(|| {
                            format!("Monitor {} — unavailable", state.monitor_index)
                        });
                    egui::ComboBox::from_label("Monitor")
                        .selected_text(selected)
                        .show_ui(ui, |ui| {
                            for m in monitors {
                                ui.selectable_value(&mut state.monitor_index, m.index, m.label());
                            }
                        });
                    if !monitors.iter().any(|m| m.index == state.monitor_index) {
                        ui.colored_label(egui::Color32::YELLOW, format!("Stored monitor {} is currently unavailable; it has been preserved.", state.monitor_index));
                    }
                }
                Err(error) => {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        format!("Monitor information unavailable: {error}"),
                    );
                }
            }
            ui.horizontal_wrapped(|ui| {
                request(
                    ui,
                    "Highlight Selected",
                    ImageEditorRequest::HighlightMonitor,
                    &mut out,
                );
                request(
                    ui,
                    "Identify All Monitors",
                    ImageEditorRequest::IdentifyMonitors,
                    &mut out,
                );
            });
        }
        SearchRegionKind::Rectangle => {
            ui.horizontal_wrapped(|ui| {
                ui.label("X");
                ui.add(egui::DragValue::new(&mut state.rectangle.x));
                ui.label("Y");
                ui.add(egui::DragValue::new(&mut state.rectangle.y));
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("Width");
                ui.add(egui::DragValue::new(&mut state.rectangle.width));
                ui.label("Height");
                ui.add(egui::DragValue::new(&mut state.rectangle.height));
            });
            if state.rectangle.is_empty() {
                ui.colored_label(egui::Color32::RED, "Width and height must be positive.");
            }
            ui.horizontal_wrapped(|ui| {
                request(
                    ui,
                    "Pick Region",
                    ImageEditorRequest::PickRectangle,
                    &mut out,
                );
                request(
                    ui,
                    "Preview Region",
                    ImageEditorRequest::PreviewRectangle,
                    &mut out,
                );
            });
        }
        SearchRegionKind::Window | SearchRegionKind::ClientArea => {
            let client = state.kind == SearchRegionKind::ClientArea;
            if client {
                ui.label("Searches only the window client area; title bars, borders, and other non-client chrome are excluded.");
            }
            let matcher = if client {
                &mut state.client_matcher
            } else {
                &mut state.window_matcher
            };
            if super::action_editor::matcher_ui(ui, matcher) {
                out = Some(ImageEditorRequest::PickWindow {
                    client_area: client,
                });
            }
            ui.horizontal_wrapped(|ui| {
                request(
                    ui,
                    "Pick Window",
                    ImageEditorRequest::PickWindow {
                        client_area: client,
                    },
                    &mut out,
                );
                request(
                    ui,
                    if client {
                        "Highlight Client Area"
                    } else {
                        "Highlight Window"
                    },
                    ImageEditorRequest::HighlightWindow {
                        client_area: client,
                    },
                    &mut out,
                );
            });
        }
    }
    state.pending_request = out;
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::MkWaitOptions;
    fn payload(region: SearchRegion) -> MkImagePayload {
        MkImagePayload {
            asset_id: 3,
            wait: MkWaitOptions {
                timeout_ms: 10,
                poll_interval_ms: 1,
            },
            region,
            tolerance: 2,
            alpha: AlphaPolicy::Compare,
            return_point: ReturnPoint::Center,
        }
    }
    #[test]
    fn every_kind_converts() {
        for region in [
            SearchRegion::Desktop,
            SearchRegion::Monitor { index: 7 },
            SearchRegion::Rectangle {
                rect: ScreenRect::new(-4, 5, 6, 7),
            },
            SearchRegion::Window {
                matcher: MkWindowMatcher::default(),
            },
            SearchRegion::ClientArea {
                matcher: MkWindowMatcher::default(),
            },
        ] {
            let s = ImageSearchEditorState::from_payload(&payload(region.clone()));
            assert_eq!(
                SearchRegionKind::from_region(&s.selected_region()),
                SearchRegionKind::from_region(&region)
            );
        }
    }
    #[test]
    fn drafts_survive_switches_and_matchers_are_independent() {
        let mut s = ImageSearchEditorState::from_payload(&payload(SearchRegion::Desktop));
        s.rectangle = ScreenRect::new(-9, 8, 7, 6);
        s.window_matcher.title = Some("whole".into());
        s.client_matcher.title = Some("client".into());
        for k in [
            SearchRegionKind::Rectangle,
            SearchRegionKind::Desktop,
            SearchRegionKind::Window,
            SearchRegionKind::ClientArea,
            SearchRegionKind::Rectangle,
        ] {
            s.select(k);
        }
        assert_eq!(s.rectangle, ScreenRect::new(-9, 8, 7, 6));
        assert_ne!(s.window_matcher, s.client_matcher);
    }
    #[test]
    fn transient_state_is_not_serialized_with_payload() {
        let p = payload(SearchRegion::Desktop);
        let mut s = ImageSearchEditorState::from_payload(&p);
        s.pending_request = Some(ImageEditorRequest::CaptureRectangle);
        let json = serde_json::to_string(&p).unwrap();
        assert!(
            !json.contains("pending_request")
                && !json.contains("monitors")
                && !json.contains("preview_error")
        );
    }

    #[test]
    fn validation_preserves_invalid_rectangle_and_missing_monitor_index() {
        let mut s = ImageSearchEditorState::from_payload(&payload(SearchRegion::Rectangle {
            rect: ScreenRect::new(-10, -20, 0, 30),
        }));
        assert!(s.validation_error().unwrap().contains("positive"));
        assert_eq!(s.rectangle, ScreenRect::new(-10, -20, 0, 30));

        s.kind = SearchRegionKind::Monitor;
        s.monitor_index = 19;
        s.monitors = Ok(vec![MonitorDescriptor {
            index: 2,
            bounds: ScreenRect::new(-1920, 0, 1920, 1080),
            primary: false,
        }]);
        assert!(s.validation_error().unwrap().contains("19"));
        assert_eq!(s.monitor_index, 19);
    }

    #[test]
    fn sparse_monitor_indices_survive_reorder_removal_and_reload() {
        let descriptor = |index, x, primary| MonitorDescriptor {
            index,
            bounds: ScreenRect::new(x, -40, 800, 600),
            primary,
        };
        let mut s =
            ImageSearchEditorState::from_payload(&payload(SearchRegion::Monitor { index: 42 }));
        s.monitors = Ok(vec![descriptor(7, -800, false), descriptor(42, 0, true)]);
        assert_eq!(s.monitor_index, 42);
        assert_eq!(
            s.monitors.as_ref().unwrap()[1].label(),
            "Monitor 42 — 800×600 @ (0, -40) — Primary"
        );

        s.monitors = Ok(vec![descriptor(42, 0, true), descriptor(7, -800, false)]);
        assert_eq!(s.selected_region(), SearchRegion::Monitor { index: 42 });
        assert!(s.validation_error().is_none());

        s.monitors = Ok(vec![descriptor(7, -800, false)]);
        assert_eq!(s.monitor_index, 42);
        assert_eq!(
            s.validation_error().as_deref(),
            Some("Monitor 42 is currently unavailable")
        );
        s.monitors = Err("enumeration failed".into());
        assert_eq!(s.monitor_index, 42); // A reload failure must never fall back to monitor zero.
    }
}
