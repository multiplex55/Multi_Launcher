//! Search fields shared by image actions and live image conditions.
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
pub struct ImageSearchControlState {
    pub kind: SearchRegionKind,
    pub monitor_index: usize,
    pub rectangle: ScreenRect,
    pub window_matcher: MkWindowMatcher,
    pub client_matcher: MkWindowMatcher,
    pub monitors: Result<Vec<MonitorDescriptor>, String>,
}

impl ImageSearchControlState {
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
    asset_id: &mut u64,
    region: &mut SearchRegion,
    tolerance: &mut u8,
    alpha: &mut AlphaPolicy,
    return_point: &mut ReturnPoint,
    assets: &[crate::mkmacro::MkImageAsset],
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
    egui::ComboBox::from_label("Reference image")
        .selected_text(super::action_editor::image_asset_label(*asset_id, assets))
        .show_ui(ui, |ui| {
            for asset in assets {
                ui.selectable_value(
                    asset_id,
                    asset.id,
                    super::action_editor::image_asset_label(asset.id, assets),
                );
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
    let mut state = ImageSearchControlState::from_region(region);
    egui::ComboBox::from_label("Area")
        .selected_text(format!("{:?}", state.kind))
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
            ui.add(egui::DragValue::new(&mut state.monitor_index).prefix("Monitor "));
            if ui.button("Highlight Monitor").clicked() {
                request = Some(SharedImageOperation::HighlightMonitor)
            }
        }
        SearchRegionKind::Rectangle => {
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut state.rectangle.x).prefix("X "));
                ui.add(egui::DragValue::new(&mut state.rectangle.y).prefix("Y "));
                ui.add(egui::DragValue::new(&mut state.rectangle.width).prefix("W "));
                ui.add(egui::DragValue::new(&mut state.rectangle.height).prefix("H "));
            });
            ui.horizontal(|ui| {
                if ui.button("Pick Region").clicked() {
                    request = Some(SharedImageOperation::PickRectangle)
                }
                if ui.button("Preview Region").clicked() {
                    request = Some(SharedImageOperation::PreviewRectangle)
                }
            });
        }
        SearchRegionKind::Window | SearchRegionKind::ClientArea => {
            let matcher = if state.kind == SearchRegionKind::Window {
                &mut state.window_matcher
            } else {
                &mut state.client_matcher
            };
            if super::action_editor::matcher_ui(ui, matcher) || ui.button("Pick Window").clicked() {
                request = Some(SharedImageOperation::PickWindow)
            }
            if ui
                .add_enabled(cfg!(windows), egui::Button::new("Highlight Window"))
                .on_disabled_hover_text("Window highlighting is available on Windows only.")
                .clicked()
            {
                request = Some(SharedImageOperation::HighlightWindow)
            }
        }
        SearchRegionKind::Desktop => {}
    }
    *region = state.selected_region();
    request
}

pub fn payload_to_condition(p: &MkImagePayload) -> MkImageSearchCondition {
    MkImageSearchCondition {
        asset_id: p.asset_id,
        region: p.region.clone(),
        tolerance: p.tolerance,
        alpha: p.alpha.clone(),
        return_point: p.return_point.clone(),
    }
}
pub fn apply_condition_to_payload(c: &MkImageSearchCondition, p: &mut MkImagePayload) {
    p.asset_id = c.asset_id;
    p.region = c.region.clone();
    p.tolerance = c.tolerance;
    p.alpha = c.alpha.clone();
    p.return_point = c.return_point.clone();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::{MkImageNotFoundPolicy, MkImageOutputs, MkWaitOptions};
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
                asset_id: 9,
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
            q.asset_id = 0;
            apply_condition_to_payload(&c, &mut q);
            assert_eq!(payload_to_condition(&q), c);
        }
    }
}
