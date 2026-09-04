//! Reusable image-search action editor and its UI-only authoring state.
use crate::mkmacro::{
    AlphaPolicy, MkImageNotFoundPolicy, MkImagePayload, MkWindowMatcher, MonitorDescriptor,
    ReturnPoint, ScreenRect, SearchRegion,
};
use eframe::egui;

pub use super::image_search_controls::SearchRegionKind;
pub type ImageSearchEditorState = super::image_search_controls::SearchRegionEditorState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageEditorRequest {
    ImportPng,
    CaptureRectangle,
    Crop,
    SelectRegion,
    PreviewRegion,
    HighlightMonitor,
    IdentifyMonitors,
    PickWindow { client_area: bool },
    HighlightWindow { client_area: bool },
    AddSmoothMouseMove,
    AddActivateWindowBefore,
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
    _macro_id: u64,
    authoring_busy: bool,
    find_action: bool,
    valid_asset: bool,
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
        disabled_request(
            ui,
            "Crop…",
            !authoring_busy && valid_asset,
            ImageEditorRequest::Crop,
            &mut out,
        );
    });
    if authoring_busy {
        ui.label("Importing reference image...");
    }
    if payload.image.filename().is_empty() {
        ui.label("No reference image selected.");
    } else {
        ui.label("Saved reference image");
        ui.small(format!("mkmacro_assets/{}", payload.image.filename()));
        super::image_preview::show(ui, store, &payload.image);
    }

    ui.separator();
    ui.heading("Search Settings");
    if find_action {
        egui::ComboBox::from_label("When not found")
            .selected_text(match payload.not_found_policy {
                MkImageNotFoundPolicy::Continue => "Continue",
                MkImageNotFoundPolicy::Fail => "Fail Action",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut payload.not_found_policy,
                    MkImageNotFoundPolicy::Continue,
                    "Continue",
                );
                ui.selectable_value(
                    &mut payload.not_found_policy,
                    MkImageNotFoundPolicy::Fail,
                    "Fail Action",
                );
            });
        ui.separator();
        ui.heading("Outputs");
        ui.small("Optional named outputs; compatibility variables such as last_image_found remain available.");
        for (label, value) in [
            ("Found", &mut payload.outputs.found),
            ("Point", &mut payload.outputs.point),
            ("X", &mut payload.outputs.x),
            ("Y", &mut payload.outputs.y),
        ] {
            ui.horizontal(|ui| {
                ui.label(label);
                ui.text_edit_singleline(value.get_or_insert_with(String::new));
            });
            if let Some(name) = value.as_deref() {
                if let Err(error) = crate::mkmacro::variables::validate_variable_name(name) {
                    ui.colored_label(ui.visuals().error_fg_color, format!("{label}: {error}"));
                }
            }
        }
        let configured: Vec<_> = [
            &payload.outputs.found,
            &payload.outputs.point,
            &payload.outputs.x,
            &payload.outputs.y,
        ]
        .into_iter()
        .flatten()
        .filter(|n| !n.is_empty())
        .collect();
        if configured
            .iter()
            .enumerate()
            .any(|(i, n)| configured[..i].contains(n))
        {
            ui.colored_label(ui.visuals().error_fg_color, "Output names must be unique");
        }
    } else {
        // Click requires a point; do not expose an unsupported continuation contract.
        payload.not_found_policy = MkImageNotFoundPolicy::Fail;
    }
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
    if let Some(region_request) = super::image_search_controls::show_search_region_fields(ui, state)
    {
        use super::image_search_controls::SearchRegionRequest as R;
        out = Some(match region_request {
            R::SelectRectangle => ImageEditorRequest::SelectRegion,
            R::PreviewRegion => match state.kind {
                SearchRegionKind::Monitor => ImageEditorRequest::HighlightMonitor,
                SearchRegionKind::Window | SearchRegionKind::ClientArea => {
                    ImageEditorRequest::HighlightWindow {
                        client_area: state.kind == SearchRegionKind::ClientArea,
                    }
                }
                _ => ImageEditorRequest::PreviewRegion,
            },
            R::PickWindow => ImageEditorRequest::PickWindow {
                client_area: state.kind == SearchRegionKind::ClientArea,
            },
            R::RefreshMonitors => {
                state.refresh_monitors();
                ImageEditorRequest::IdentifyMonitors
            }
            R::IdentifyMonitors => ImageEditorRequest::IdentifyMonitors,
        });
    }
    ui.separator();
    ui.heading("Related actions");
    if find_action {
        let response = ui.add_enabled(
            valid_asset,
            egui::Button::new("Add Smooth Mouse Move to Result"),
        );
        if response
            .on_hover_text(if valid_asset {
                "On Apply, adds an independent 500 ms move immediately after this Find Image step."
            } else {
                "Select a valid reference image before adding a result move."
            })
            .clicked()
        {
            out = Some(ImageEditorRequest::AddSmoothMouseMove);
        }
        if !valid_asset {
            ui.small("Select a valid reference image to enable the smooth-move shortcut.");
        }
    }
    if matches!(
        state.kind,
        SearchRegionKind::Window | SearchRegionKind::ClientArea
    ) {
        if ui.button("Add Activate Window Before").on_hover_text("On Apply, adds one independent activation row immediately before this search. Repeated requests are allowed and predictable.").clicked() {
            out = Some(ImageEditorRequest::AddActivateWindowBefore);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::{MkImageOutputs, MkImageRef, MkWaitOptions};
    fn payload(region: SearchRegion) -> MkImagePayload {
        MkImagePayload {
            image: MkImageRef::from_filename("3.png"),
            wait: MkWaitOptions {
                timeout_ms: 10,
                poll_interval_ms: 1,
            },
            region,
            tolerance: 2,
            alpha: AlphaPolicy::Compare,
            return_point: ReturnPoint::Center,
            not_found_policy: MkImageNotFoundPolicy::Fail,
            outputs: MkImageOutputs::default(),
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
            let s = ImageSearchEditorState::from_region(&region);
            assert_eq!(
                SearchRegionKind::from_region(&s.selected_region()),
                SearchRegionKind::from_region(&region)
            );
        }
    }
    #[test]
    fn drafts_survive_switches_and_matchers_are_independent() {
        let mut s = ImageSearchEditorState::from_region(&SearchRegion::Desktop);
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
        let mut s = ImageSearchEditorState::from_region(&p.region);
        s.monitors = Err("transient discovery failure".into());
        let json = serde_json::to_string(&p).unwrap();
        assert!(
            !json.contains("pending_request")
                && !json.contains("monitors")
                && !json.contains("preview_error")
        );
    }

    #[test]
    fn validation_preserves_invalid_rectangle_and_missing_monitor_index() {
        let mut s = ImageSearchEditorState::from_region(&SearchRegion::Rectangle {
            rect: ScreenRect::new(-10, -20, 0, 30),
        });
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
        let mut s = ImageSearchEditorState::from_region(&SearchRegion::Monitor { index: 42 });
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
