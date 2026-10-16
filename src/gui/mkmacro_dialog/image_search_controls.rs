//! Search fields shared by image actions and live image conditions.
use super::window_matcher_editor::matcher_ui;
use crate::gui::theme::widgets::{StatusTone, semantic_status};
use crate::mkmacro::{
    AlphaPolicy, MkImageOutputs, MkImagePayload, MkImageRef, MkImageSearchCondition,
    MkWindowMatcher, MonitorDescriptor, ReturnPoint, ScreenRect, SearchRegion,
};
use eframe::egui;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AutoPopulateOutputsResult {
    Unavailable,
    Populated(MkImageOutputs),
}

/// Derives a complete output set from Found without mutating the current values.
/// The caller can therefore replace all four fields atomically.
pub(crate) fn auto_populate_outputs(outputs: &MkImageOutputs) -> AutoPopulateOutputsResult {
    let Some(found) = outputs
        .found
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
    else {
        return AutoPopulateOutputsResult::Unavailable;
    };
    let base = found.strip_suffix("_found").unwrap_or(found);
    AutoPopulateOutputsResult::Populated(MkImageOutputs {
        found: Some(format!("{base}_found")),
        point: Some(format!("{base}_point")),
        x: Some(format!("{base}_x")),
        y: Some(format!("{base}_y")),
    })
}

fn configured_output_names(outputs: &MkImageOutputs) -> impl Iterator<Item = &str> {
    [
        outputs.found.as_deref(),
        outputs.point.as_deref(),
        outputs.x.as_deref(),
        outputs.y.as_deref(),
    ]
    .into_iter()
    .flatten()
    .filter(|name| !name.trim().is_empty())
}

pub(crate) fn image_outputs_valid(outputs: &MkImageOutputs) -> bool {
    let names: Vec<_> = configured_output_names(outputs).collect();
    names
        .iter()
        .all(|name| crate::mkmacro::variables::validate_variable_name(name).is_ok())
        && !names
            .iter()
            .enumerate()
            .any(|(index, name)| names[..index].contains(name))
}

/// Shared editor for the named result variables produced by image and pixel searches.
pub(crate) fn show_image_outputs_editor(ui: &mut egui::Ui, outputs: &mut MkImageOutputs) {
    ui.horizontal_wrapped(|ui| {
        ui.heading("Outputs");
        let available = outputs
            .found
            .as_deref()
            .is_some_and(|name| !name.trim().is_empty());
        if ui
            .add_enabled(available, egui::Button::new("Auto Populate"))
            .clicked()
            && let AutoPopulateOutputsResult::Populated(replacement) =
                auto_populate_outputs(outputs)
        {
            *outputs = replacement;
        }
    });
    ui.small("Optional named outputs; compatibility variables such as last_image_found remain available.");
    for (label, value) in [
        ("Found", &mut outputs.found),
        ("Point", &mut outputs.point),
        ("X", &mut outputs.x),
        ("Y", &mut outputs.y),
    ] {
        let label_response = ui.label(label);
        ui.add(
            egui::TextEdit::singleline(value.get_or_insert_with(String::new))
                .desired_width(ui.available_width().max(1.0)),
        )
        .labelled_by(label_response.id);
        if let Some(name) = value.as_deref().filter(|name| !name.trim().is_empty())
            && let Err(error) = crate::mkmacro::variables::validate_variable_name(name)
        {
            ui.colored_label(ui.visuals().error_fg_color, format!("{label}: {error}"));
        }
    }
    let names: Vec<_> = configured_output_names(outputs).collect();
    if names
        .iter()
        .enumerate()
        .any(|(index, name)| names[..index].contains(name))
    {
        ui.colored_label(ui.visuals().error_fg_color, "Output names must be unique");
    }
}

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
    let area_label = ui.label("Area");
    let area_width = ui.available_width().max(1.0);
    egui::ComboBox::from_id_salt("Area")
        .width(area_width)
        .wrap()
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
        })
        .response
        .labelled_by(area_label.id);
    match state.kind {
        SearchRegionKind::Monitor => {
            ui.horizontal_wrapped(|ui| {
                if ui.button("Refresh Monitors").clicked() {
                    out = Some(SearchRegionRequest::RefreshMonitors);
                }
            });
            match &state.monitors {
                Ok(ms) => {
                    let selected = ms
                        .iter()
                        .find(|m| m.index == state.monitor_index)
                        .map(MonitorDescriptor::label)
                        .unwrap_or_else(|| {
                            format!("Monitor {} — unavailable", state.monitor_index)
                        });
                    let monitor_label = ui.label("Monitor");
                    let monitor_width = ui.available_width().max(1.0);
                    egui::ComboBox::from_id_salt("Monitor")
                        .width(monitor_width)
                        .wrap()
                        .selected_text(selected)
                        .show_ui(ui, |ui| {
                            for m in ms {
                                ui.selectable_value(&mut state.monitor_index, m.index, m.label());
                            }
                        })
                        .response
                        .labelled_by(monitor_label.id);
                }
                Err(e) => {
                    semantic_status(
                        ui,
                        StatusTone::Warning,
                        &format!("Monitor information unavailable: {e}"),
                        None,
                    );
                }
            }
        }
        SearchRegionKind::Rectangle => {
            ui.horizontal_wrapped(|ui| {
                let label = ui.label("X");
                ui.add(egui::DragValue::new(&mut state.rectangle.x))
                    .labelled_by(label.id);
                let label = ui.label("Y");
                ui.add(egui::DragValue::new(&mut state.rectangle.y))
                    .labelled_by(label.id);
                let label = ui.label("W");
                ui.add(egui::DragValue::new(&mut state.rectangle.width))
                    .labelled_by(label.id);
                let label = ui.label("H");
                ui.add(egui::DragValue::new(&mut state.rectangle.height))
                    .labelled_by(label.id);
            });
            ui.horizontal_wrapped(|ui| {
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
    {
        ui.horizontal_wrapped(|ui| {
            if ui.button("Preview Region").clicked() {
                out = Some(SearchRegionRequest::PreviewRegion);
            }
        });
    }
    if let Some(e) = state.validation_error() {
        semantic_status(ui, StatusTone::Warning, &e, None);
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SharedImageOperation {
    ImportPng,
    CaptureRectangle,
    CropImage,
    TestImage,
    PickRectangle,
    PreviewRectangle,
    HighlightMonitor,
    PickWindow,
    HighlightWindow,
}

/// Returns whether a filename is an enumerated, managed asset in the current
/// store. Empty and stale references are deliberately not treated as usable.
pub(crate) fn managed_image_is_available(image: &MkImageRef, assets: &[MkImageRef]) -> bool {
    !image.is_empty() && assets.iter().any(|asset| asset == image)
}

/// All reference-authoring controls share this busy predicate, including the
/// controls rendered by the action and condition editors.
pub(crate) fn reference_authoring_buttons_enabled(authoring_busy: bool) -> bool {
    !authoring_busy
}

pub(crate) fn crop_button_enabled(valid_asset: bool, authoring_busy: bool) -> bool {
    reference_authoring_buttons_enabled(authoring_busy) && valid_asset
}

pub(crate) fn test_button_enabled(
    valid_asset: bool,
    authoring_busy: bool,
    test_busy: bool,
) -> bool {
    valid_asset && !authoring_busy && !test_busy
}

/// Render the persisted fields that deliberately have identical semantics in actions and conditions.
pub fn show_shared_fields(
    ui: &mut egui::Ui,
    image: &mut crate::mkmacro::MkImageRef,
    region: &mut SearchRegion,
    tolerance: &mut u8,
    alpha: &mut AlphaPolicy,
    return_point: &mut ReturnPoint,
    assets: &[crate::mkmacro::MkImageRef],
    authoring_busy: bool,
    test_busy: bool,
) -> Option<SharedImageOperation> {
    let mut request = None;
    let valid_asset = managed_image_is_available(image, assets);
    ui.heading("Reference Image");
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(
                reference_authoring_buttons_enabled(authoring_busy),
                egui::Button::new("Select PNG…"),
            )
            .clicked()
        {
            request = Some(SharedImageOperation::ImportPng)
        }
        if ui
            .add_enabled(
                reference_authoring_buttons_enabled(authoring_busy),
                egui::Button::new("Capture…"),
            )
            .clicked()
        {
            request = Some(SharedImageOperation::CaptureRectangle)
        }
        if ui
            .add_enabled(
                crop_button_enabled(valid_asset, authoring_busy),
                egui::Button::new("Crop…"),
            )
            .clicked()
        {
            request = Some(SharedImageOperation::CropImage)
        }
        if valid_asset
            && ui
                .add_enabled(
                    test_button_enabled(valid_asset, authoring_busy, test_busy),
                    egui::Button::new("Test"),
                )
                .clicked()
        {
            request = Some(SharedImageOperation::TestImage)
        }
    });
    ui.add(egui::Slider::new(tolerance, 0..=255).text("Tolerance"));
    alpha_policy_ui(ui, alpha);
    show_return_point_control(ui, return_point);
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

pub(super) fn alpha_policy_ui(ui: &mut egui::Ui, alpha: &mut AlphaPolicy) {
    let label = ui.label("Alpha handling");
    let available_width = ui.available_width().max(1.0);
    egui::ComboBox::from_id_salt("Alpha handling")
        .width(available_width)
        .wrap()
        .selected_text(format!("{alpha:?}"))
        .show_ui(ui, |ui| {
            ui.selectable_value(alpha, AlphaPolicy::Compare, "Compare alpha");
            ui.selectable_value(alpha, AlphaPolicy::Ignore, "Ignore alpha");
        })
        .response
        .labelled_by(label.id);
}

fn show_return_point_control(ui: &mut egui::Ui, return_point: &mut ReturnPoint) {
    ui.horizontal_wrapped(|ui| {
        ui.label("Return point");
        ui.selectable_value(return_point, ReturnPoint::Center, "Center");
        ui.selectable_value(return_point, ReturnPoint::TopLeft, "Top-left");
    });
}

pub fn payload_to_condition(p: &MkImagePayload) -> MkImageSearchCondition {
    MkImageSearchCondition {
        image: p.image.clone(),
        region: p.region.clone(),
        tolerance: p.tolerance,
        alpha: p.alpha,
        return_point: p.return_point,
    }
}
pub fn apply_condition_to_payload(c: &MkImageSearchCondition, p: &mut MkImagePayload) {
    p.image = c.image.clone();
    p.region = c.region.clone();
    p.tolerance = c.tolerance;
    p.alpha = c.alpha;
    p.return_point = c.return_point;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::{
        MkImageNotFoundPolicy, MkImageOutputs, MkImageRef, MkOcrReadPayload, MkWaitOptions,
    };

    #[test]
    fn mkmacro_image_controls_chrome_keeps_shared_fields_inside_narrow_view_without_mutation() {
        use egui::accesskit::{Node, NodeId, Role};

        fn control_has_label(nodes: &[(NodeId, Node)], node: &Node, expected: &str) -> bool {
            node.label() == Some(expected)
                || node.labelled_by().iter().any(|label_id| {
                    nodes.iter().any(|(id, label)| {
                        id == label_id
                            && label.role() == Role::Label
                            && label.value() == Some(expected)
                    })
                })
        }

        let mut outputs = MkImageOutputs {
            found: Some("found_result".into()),
            point: Some("point_result".into()),
            x: Some("x_result".into()),
            y: Some("y_result".into()),
        };
        let mut region = SearchRegionEditorState {
            kind: SearchRegionKind::Rectangle,
            monitor_index: 0,
            rectangle: ScreenRect::new(-140, 260, 640, 480),
            window_matcher: Default::default(),
            client_matcher: Default::default(),
            monitors: Ok(Vec::new()),
        };
        let mut return_point = ReturnPoint::TopLeft;
        let mut tolerance = 37;
        let mut alpha = AlphaPolicy::Ignore;
        let mut read = MkOcrReadPayload {
            output_variable: "recognized_result".into(),
            ..Default::default()
        };
        let original = (
            outputs.clone(),
            region.clone(),
            return_point,
            tolerance,
            alpha,
            read.clone(),
        );
        let mut request = None;

        let context = egui::Context::default();
        context.enable_accesskit();
        context.style_mut_of(context.theme(), |style| {
            style.override_font_id = Some(egui::FontId::new(22.0, egui::FontFamily::Proportional));
        });
        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(420.0, 760.0));
        let output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(screen_rect),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    show_image_outputs_editor(ui, &mut outputs);
                    request = show_search_region_fields(ui, &mut region);
                    alpha_policy_ui(ui, &mut alpha);
                    show_return_point_control(ui, &mut return_point);
                    super::super::image_search_editor::tolerance_slider_ui(ui, &mut tolerance);
                    super::super::ocr_controls::read_output_ui(ui, &mut read);
                });
            },
        );

        let labels = [
            "Found",
            "Point",
            "X",
            "Y",
            "W",
            "H",
            "Area",
            "Select Region",
            "Preview Region",
            "Alpha handling",
            "Return point",
            "Center",
            "Top-left",
            "Tolerance",
            "Output variable",
        ];
        let values = [
            "found_result",
            "point_result",
            "x_result",
            "y_result",
            "recognized_result",
        ];
        let mut all_labels_reachable = false;
        let mut all_values_reachable = false;
        let mut all_controls_reachable = false;
        let mut all_bounds_inside_view = false;
        if let Some(update) = output.platform_output.accesskit_update.as_ref() {
            let actual_bounds = labels
                .iter()
                .map(|expected| {
                    update.nodes.iter().find_map(|(_, node)| {
                        (node.label() == Some(expected) || node.value() == Some(expected))
                            .then(|| node.bounds())
                            .flatten()
                    })
                })
                .collect::<Vec<_>>();
            let value_bounds = values
                .iter()
                .map(|expected| {
                    update.nodes.iter().find_map(|(_, node)| {
                        (node.value() == Some(expected))
                            .then(|| node.bounds())
                            .flatten()
                    })
                })
                .collect::<Vec<_>>();
            let controls = [
                "Found",
                "Point",
                "X",
                "Y",
                "W",
                "H",
                "Area",
                "Select Region",
                "Preview Region",
                "Alpha handling",
                "Center",
                "Top-left",
                "Tolerance",
                "Output variable",
            ];
            let control_bounds = controls
                .iter()
                .map(|expected| {
                    update.nodes.iter().find_map(|(_, node)| {
                        (node.role() != Role::Label
                            && control_has_label(&update.nodes, node, expected))
                        .then(|| node.bounds())
                        .flatten()
                    })
                })
                .collect::<Vec<_>>();
            all_labels_reachable = actual_bounds.iter().all(Option::is_some);
            all_values_reachable = value_bounds.iter().all(Option::is_some);
            all_controls_reachable = control_bounds.iter().all(Option::is_some);
            all_bounds_inside_view =
                control_bounds
                    .iter()
                    .chain(&value_bounds)
                    .flatten()
                    .all(|bounds| {
                        bounds.x0 as f32 >= screen_rect.left() - 0.5
                            && bounds.x1 as f32 <= screen_rect.right() + 0.5
                            && bounds.y0 as f32 >= screen_rect.top() - 0.5
                            && bounds.y1 as f32 <= screen_rect.bottom() + 0.5
                    });
        }
        output.drop_without_applying_deltas();

        let monitor = MonitorDescriptor {
            index: 17,
            bounds: ScreenRect::new(-1920, 0, 3840, 2160),
            primary: true,
        };
        let expected_monitor_label = monitor.label();
        let mut monitor_region = SearchRegionEditorState {
            kind: SearchRegionKind::Monitor,
            monitor_index: monitor.index,
            rectangle: ScreenRect::new(0, 0, 1, 1),
            window_matcher: Default::default(),
            client_matcher: Default::default(),
            monitors: Ok(vec![monitor]),
        };
        let monitor_region_before = monitor_region.clone();
        let monitor_output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(screen_rect),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    request = show_search_region_fields(ui, &mut monitor_region);
                });
            },
        );
        let mut monitor_control_bounds = None;
        let mut monitor_text_preserved = false;
        if let Some(update) = monitor_output.platform_output.accesskit_update.as_ref() {
            for (_, node) in &update.nodes {
                if node.role() != Role::Label && control_has_label(&update.nodes, node, "Monitor") {
                    monitor_control_bounds = node.bounds();
                }
                if node.value() == Some(expected_monitor_label.as_str()) {
                    monitor_text_preserved = true;
                }
            }
        }
        let monitor_control_inside_view = monitor_control_bounds.is_some_and(|bounds| {
            bounds.x0 as f32 >= screen_rect.left() - 0.5
                && bounds.x1 as f32 <= screen_rect.right() + 0.5
                && bounds.y0 as f32 >= screen_rect.top() - 0.5
                && bounds.y1 as f32 <= screen_rect.bottom() + 0.5
        });
        monitor_output.drop_without_applying_deltas();

        assert!(
            all_labels_reachable,
            "actual native labels and actions are exposed"
        );
        assert!(
            all_values_reachable,
            "the image and OCR editor values are exposed"
        );
        assert!(
            all_controls_reachable,
            "native controls retain their accessible labels"
        );
        assert!(
            all_bounds_inside_view,
            "native shared-control bounds fit the constrained view"
        );
        assert!(
            monitor_text_preserved,
            "the selected native monitor label is preserved"
        );
        assert!(
            monitor_control_inside_view,
            "selected monitor control bounds {monitor_control_bounds:?} fit {screen_rect:?}"
        );
        assert_eq!(monitor_region, monitor_region_before);
        assert_eq!(request, None, "rendering does not emit a region operation");
        assert_eq!(
            (outputs, region, return_point, tolerance, alpha, read),
            original,
            "rendering does not modify authored image/OCR values"
        );
    }

    #[test]
    fn reference_authoring_availability_is_safe_for_actions_and_conditions() {
        let available = MkImageRef::from_filename("managed.png");
        let assets = vec![available.clone()];

        assert!(managed_image_is_available(&available, &assets));
        assert!(!managed_image_is_available(&MkImageRef::default(), &assets));
        assert!(!managed_image_is_available(
            &MkImageRef::from_filename("missing.png"),
            &assets
        ));

        // Both the image action editor and condition editor use these shared
        // predicates for their identical reference controls.
        assert!(reference_authoring_buttons_enabled(false));
        assert!(!reference_authoring_buttons_enabled(true));
        assert!(crop_button_enabled(true, false));
        assert!(!crop_button_enabled(false, false));
        assert!(!crop_button_enabled(true, true));
        assert!(test_button_enabled(true, false, false));
        assert!(!test_button_enabled(false, false, false));
        assert!(!test_button_enabled(true, true, false));
        assert!(!test_button_enabled(true, false, true));
    }

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

    fn outputs(found: Option<&str>) -> MkImageOutputs {
        MkImageOutputs {
            found: found.map(str::to_owned),
            point: Some("custom_point".into()),
            x: Some("custom_x".into()),
            y: Some("custom_y".into()),
        }
    }

    fn populated(found: Option<&str>) -> MkImageOutputs {
        match auto_populate_outputs(&outputs(found)) {
            AutoPopulateOutputsResult::Populated(outputs) => outputs,
            AutoPopulateOutputsResult::Unavailable => panic!("expected populated outputs"),
        }
    }

    #[test]
    fn auto_populate_derives_and_completely_replaces_outputs() {
        assert_eq!(
            populated(Some("button")),
            MkImageOutputs {
                found: Some("button_found".into()),
                point: Some("button_point".into()),
                x: Some("button_x".into()),
                y: Some("button_y".into()),
            }
        );
        assert_eq!(populated(Some("button_found")), populated(Some("button")));
        assert_eq!(
            populated(Some("  button_found  ")),
            populated(Some("button"))
        );
        assert_eq!(
            populated(Some("button_found_found")).found.as_deref(),
            Some("button_found_found")
        );
    }

    #[test]
    fn auto_populate_unavailable_is_a_no_op_for_absent_or_blank_found() {
        for found in [None, Some(""), Some("   ")] {
            let original = outputs(found);
            let unchanged = original.clone();
            assert_eq!(
                auto_populate_outputs(&original),
                AutoPopulateOutputsResult::Unavailable
            );
            assert_eq!(original, unchanged);
        }
    }
}
