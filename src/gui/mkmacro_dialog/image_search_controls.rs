//! Search fields shared by image actions and live image conditions.
use super::window_matcher_editor::matcher_ui;
use crate::mkmacro::{
    AlphaPolicy, MkImagePayload, MkImageSearchCondition, MkWindowMatcher, MonitorDescriptor,
    ReturnPoint, ScreenRect, SearchRegion,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchRegionEditorState {
    pub kind: SearchRegionKind,
    pub monitor_index: usize,
    pub rectangle: ScreenRect,
    pub window_matcher: MkWindowMatcher,
    pub client_matcher: MkWindowMatcher,
    pub monitors: Result<Vec<MonitorDescriptor>, String>,
}

impl SearchRegionEditorState {
    pub fn from_region(region: &SearchRegion) -> Self {
        let mut s = Self {
            kind: SearchRegionKind::from_region(region),
            monitor_index: 0,
            rectangle: ScreenRect::new(0, 0, 640, 480),
            window_matcher: Default::default(),
            client_matcher: Default::default(),
            monitors: crate::mkmacro::monitor_descriptors().map_err(|e| e.to_string()),
        };
        match region {
            SearchRegion::Monitor { index } => s.monitor_index = *index,
            SearchRegion::Rectangle { rect } => s.rectangle = *rect,
            SearchRegion::Window { matcher } => s.window_matcher = matcher.clone(),
            SearchRegion::ClientArea { matcher } => s.client_matcher = matcher.clone(),
            _ => {}
        }
        s
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
    pub fn refresh_monitors(&mut self) {
        self.monitors = crate::mkmacro::monitor_descriptors().map_err(|e| e.to_string());
    }
    pub fn select(&mut self, kind: SearchRegionKind) {
        self.kind = kind;
    }
    pub fn validation_error(&self) -> Option<String> {
        match self.kind {
            SearchRegionKind::Rectangle if self.rectangle.is_empty() => {
                Some("Rectangle width and height must be positive".into())
            }
            SearchRegionKind::Monitor => match &self.monitors {
                Ok(ms) if !ms.iter().any(|m| m.index == self.monitor_index) => Some(format!(
                    "Monitor {} is currently unavailable",
                    self.monitor_index
                )),
                Err(e) => Some(format!("Monitor information unavailable: {e}")),
                _ => None,
            },
            _ => None,
        }
    }
}

pub type ImageSearchControlState = SearchRegionEditorState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchRegionRequest {
    SelectRectangle,
    PreviewRegion,
    PickWindow,
    RefreshMonitors,
    IdentifyMonitors,
}

/// Edits only a screen region. Native workflows are deliberately returned to the owner.
pub fn show_search_region_fields(
    ui: &mut egui::Ui,
    state: &mut SearchRegionEditorState,
) -> Option<SearchRegionRequest> {
    let mut out = None;
    egui::ComboBox::from_label("Area")
        .selected_text(match state.kind {
            SearchRegionKind::Desktop => "Desktop",
            SearchRegionKind::Monitor => "Monitor",
            SearchRegionKind::Rectangle => "Rectangle",
            SearchRegionKind::Window => "Window",
            SearchRegionKind::ClientArea => "Client Area",
        })
        .show_ui(ui, |ui| {
            for (k, l) in [
                (SearchRegionKind::Desktop, "Desktop"),
                (SearchRegionKind::Monitor, "Monitor"),
                (SearchRegionKind::Rectangle, "Rectangle"),
                (SearchRegionKind::Window, "Window"),
                (SearchRegionKind::ClientArea, "Client Area"),
            ] {
                ui.selectable_value(&mut state.kind, k, l);
            }
        });
    match state.kind {
        SearchRegionKind::Monitor => {
            if ui.button("Refresh Monitors").clicked() {
                out = Some(SearchRegionRequest::RefreshMonitors);
            }
            match &state.monitors {
                Ok(ms) => {
                    let selected = ms
                        .iter()
                        .find(|m| m.index == state.monitor_index)
                        .map(MonitorDescriptor::label)
                        .unwrap_or_else(|| {
                            format!("Monitor {} — unavailable", state.monitor_index)
                        });
                    egui::ComboBox::from_label("Monitor")
                        .selected_text(selected)
                        .show_ui(ui, |ui| {
                            for m in ms {
                                ui.selectable_value(&mut state.monitor_index, m.index, m.label());
                            }
                        });
                }
                Err(e) => {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        format!("Monitor information unavailable: {e}"),
                    );
                }
            }
        }
        SearchRegionKind::Rectangle => {
            ui.horizontal(|ui| {
                ui.label("X");
                ui.add(egui::DragValue::new(&mut state.rectangle.x));
                ui.label("Y");
                ui.add(egui::DragValue::new(&mut state.rectangle.y));
                ui.label("W");
                ui.add(egui::DragValue::new(&mut state.rectangle.width));
                ui.label("H");
                ui.add(egui::DragValue::new(&mut state.rectangle.height));
                if ui.button("Select Region").clicked() {
                    out = Some(SearchRegionRequest::SelectRectangle);
                }
                if ui.button("Preview Region").clicked() {
                    out = Some(SearchRegionRequest::PreviewRegion);
                }
            });
        }
        SearchRegionKind::Window | SearchRegionKind::ClientArea => {
            let m = if state.kind == SearchRegionKind::Window {
                &mut state.window_matcher
            } else {
                &mut state.client_matcher
            };
            if matcher_ui(ui, m).pick_window {
                out = Some(SearchRegionRequest::PickWindow);
            }
        }
        SearchRegionKind::Desktop => {}
    }
    if !matches!(
        out,
        Some(SearchRegionRequest::PickWindow | SearchRegionRequest::SelectRectangle)
    ) && state.kind != SearchRegionKind::Rectangle
        && ui.button("Preview Region").clicked()
    {
        out = Some(SearchRegionRequest::PreviewRegion);
    }
    if let Some(e) = state.validation_error() {
        ui.colored_label(egui::Color32::YELLOW, e);
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SharedImageOperation {
    ImportPng,
    CaptureRectangle,
    PickRectangle,
    PreviewRectangle,
    HighlightMonitor,
    PickWindow,
    HighlightWindow,
}

/// Render the persisted fields that deliberately have identical semantics in actions and conditions.
pub fn show_shared_fields(
    ui: &mut egui::Ui,
    _image: &mut crate::mkmacro::MkImageRef,
    region: &mut SearchRegion,
    tolerance: &mut u8,
    alpha: &mut AlphaPolicy,
    return_point: &mut ReturnPoint,
    _assets: &[crate::mkmacro::MkImageRef],
) -> Option<SharedImageOperation> {
    let mut request = None;
    ui.heading("Reference Image");
    ui.horizontal_wrapped(|ui| {
        if ui.button("Select PNG…").clicked() {
            request = Some(SharedImageOperation::ImportPng)
        }
        if ui.button("Capture…").clicked() {
            request = Some(SharedImageOperation::CaptureRectangle)
        }
    });
    ui.add(egui::Slider::new(tolerance, 0..=255).text("Tolerance"));
    egui::ComboBox::from_label("Alpha handling")
        .selected_text(format!("{alpha:?}"))
        .show_ui(ui, |ui| {
            ui.selectable_value(alpha, AlphaPolicy::Compare, "Compare alpha");
            ui.selectable_value(alpha, AlphaPolicy::Ignore, "Ignore alpha");
        });
    ui.horizontal(|ui| {
        ui.label("Return point");
        ui.selectable_value(return_point, ReturnPoint::Center, "Center");
        ui.selectable_value(return_point, ReturnPoint::TopLeft, "Top-left");
    });
    let mut state = SearchRegionEditorState::from_region(region);
    if let Some(r) = show_search_region_fields(ui, &mut state) {
        request = Some(match r {
            SearchRegionRequest::SelectRectangle => SharedImageOperation::PickRectangle,
            SearchRegionRequest::PreviewRegion => match state.kind {
                SearchRegionKind::Monitor => SharedImageOperation::HighlightMonitor,
                SearchRegionKind::Window | SearchRegionKind::ClientArea => {
                    SharedImageOperation::HighlightWindow
                }
                _ => SharedImageOperation::PreviewRectangle,
            },
            SearchRegionRequest::PickWindow => SharedImageOperation::PickWindow,
            SearchRegionRequest::RefreshMonitors | SearchRegionRequest::IdentifyMonitors => {
                SharedImageOperation::HighlightMonitor
            }
        });
    }
    *region = state.selected_region();
    request
}

pub fn payload_to_condition(p: &MkImagePayload) -> MkImageSearchCondition {
    MkImageSearchCondition {
        image: p.image.clone(),
        region: p.region.clone(),
        tolerance: p.tolerance,
        alpha: p.alpha.clone(),
        return_point: p.return_point.clone(),
    }
}
pub fn apply_condition_to_payload(c: &MkImageSearchCondition, p: &mut MkImagePayload) {
    p.image = c.image.clone();
    p.region = c.region.clone();
    p.tolerance = c.tolerance;
    p.alpha = c.alpha.clone();
    p.return_point = c.return_point.clone();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::{MkImageNotFoundPolicy, MkImageOutputs, MkImageRef, MkWaitOptions};
    #[test]
    fn action_condition_shared_settings_have_parity_for_every_region() {
        for region in [
            SearchRegion::Desktop,
            SearchRegion::Monitor { index: 2 },
            SearchRegion::Rectangle {
                rect: ScreenRect::new(1, 2, 3, 4),
            },
            SearchRegion::Window {
                matcher: Default::default(),
            },
            SearchRegion::ClientArea {
                matcher: Default::default(),
            },
        ] {
            let p = MkImagePayload {
                image: MkImageRef::from_filename("9.png"),
                region,
                tolerance: 7,
                alpha: AlphaPolicy::Ignore,
                return_point: ReturnPoint::TopLeft,
                wait: MkWaitOptions {
                    timeout_ms: 1,
                    poll_interval_ms: 1,
                },
                not_found_policy: MkImageNotFoundPolicy::Fail,
                outputs: MkImageOutputs::default(),
            };
            let c = payload_to_condition(&p);
            let mut q = p.clone();
            q.image = MkImageRef::default();
            apply_condition_to_payload(&c, &mut q);
            assert_eq!(payload_to_condition(&q), c);
        }
    }
}
