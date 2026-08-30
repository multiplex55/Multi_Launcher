//! Recursive editor and pure mutation operations for macro conditions.

use super::action_editor::{TargetEditorContext, target_ui, value_ui};
use super::image_asset_picker::ImageAssetUiContext;
use super::window_matcher_editor::matcher_ui;
use crate::mkmacro::variables::{MkPoint, MkValue};
use crate::mkmacro::{
    AlphaPolicy, MkCompareOp, MkCondition, MkCoordinateTarget, MkImageAsset,
    MkImageSearchCondition, MkWindowMatcher, ReturnPoint, SearchRegion,
};
use eframe::egui;

pub use super::image_authoring_destination::{
    ConditionBranch, ConditionImageOperation, ConditionPath,
};

impl ConditionImageOperation {
    pub(crate) fn rectangle_purpose(&self) -> Option<super::visual_overlay::RectanglePurpose> {
        use super::visual_overlay::RectanglePurpose;
        match self {
            Self::CaptureRectangle => Some(RectanglePurpose::ReferenceImageCapture),
            Self::PickRectangle => Some(RectanglePurpose::SearchRegion),
            _ => None,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConditionImageRequest {
    pub path: ConditionPath,
    pub operation: ConditionImageOperation,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConditionEditorRequest {
    WindowMatcher { path: ConditionPath },
    Image(ConditionImageRequest),
}

pub fn resolve_condition_mut<'a>(
    mut condition: &'a mut MkCondition,
    path: &ConditionPath,
) -> Option<&'a mut MkCondition> {
    for branch in path.branches() {
        condition = match (condition, branch) {
            (
                MkCondition::All { conditions },
                ConditionBranch::All(index) | ConditionBranch::Index(index),
            ) => conditions.get_mut(*index)?,
            (
                MkCondition::Any { conditions },
                ConditionBranch::Any(index) | ConditionBranch::Index(index),
            ) => conditions.get_mut(*index)?,
            (MkCondition::Not { condition }, ConditionBranch::Not | ConditionBranch::Index(0)) => {
                condition
            }
            _ => return None,
        };
    }
    Some(condition)
}
pub fn resolve_condition<'a>(
    mut condition: &'a MkCondition,
    path: &ConditionPath,
) -> Option<&'a MkCondition> {
    for branch in path.branches() {
        condition = match (condition, branch) {
            (
                MkCondition::All { conditions },
                ConditionBranch::All(index) | ConditionBranch::Index(index),
            ) => conditions.get(*index)?,
            (
                MkCondition::Any { conditions },
                ConditionBranch::Any(index) | ConditionBranch::Index(index),
            ) => conditions.get(*index)?,
            (MkCondition::Not { condition }, ConditionBranch::Not | ConditionBranch::Index(0)) => {
                condition
            }
            _ => return None,
        };
    }
    Some(condition)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConditionKind {
    Variable,
    WindowExists,
    WindowActive,
    ImageSearch,
    PreviousImageResult,
    PixelResult,
    All,
    Any,
    Not,
}

pub fn default_condition(kind: ConditionKind) -> MkCondition {
    match kind {
        ConditionKind::Variable => MkCondition::Variable {
            name: "value".into(),
            op: MkCompareOp::Eq,
            value: MkValue::String(String::new()),
        },
        ConditionKind::WindowExists => MkCondition::WindowExists {
            matcher: default_matcher(),
        },
        ConditionKind::WindowActive => MkCondition::WindowActive {
            matcher: default_matcher(),
        },
        ConditionKind::ImageSearch => MkCondition::ImageSearch {
            search: MkImageSearchCondition {
                asset_id: 1,
                region: SearchRegion::Desktop,
                tolerance: 0,
                alpha: AlphaPolicy::Compare,
                return_point: ReturnPoint::Center,
            },
            found: true,
        },
        ConditionKind::PreviousImageResult => MkCondition::PreviousImageResult {
            asset_id: None,
            found: true,
        },
        ConditionKind::PixelResult => MkCondition::PixelResult {
            target: MkCoordinateTarget::Screen {
                point: MkPoint { x: 0, y: 0 },
            },
            color: "#000000".into(),
            tolerance: 0,
        },
        ConditionKind::All => MkCondition::All { conditions: vec![] },
        ConditionKind::Any => MkCondition::Any { conditions: vec![] },
        ConditionKind::Not => MkCondition::Not {
            condition: Box::new(default_condition(ConditionKind::Variable)),
        },
    }
}
fn default_matcher() -> MkWindowMatcher {
    MkWindowMatcher {
        title: Some("Window".into()),
        title_regex: None,
        process: None,
        class: None,
    }
}
pub fn condition_kind(c: &MkCondition) -> ConditionKind {
    match c {
        MkCondition::Variable { .. } => ConditionKind::Variable,
        MkCondition::WindowExists { .. } => ConditionKind::WindowExists,
        MkCondition::WindowActive { .. } => ConditionKind::WindowActive,
        MkCondition::ImageSearch { .. } => ConditionKind::ImageSearch,
        MkCondition::PreviousImageResult { .. } => ConditionKind::PreviousImageResult,
        MkCondition::PixelResult { .. } => ConditionKind::PixelResult,
        MkCondition::All { .. } => ConditionKind::All,
        MkCondition::Any { .. } => ConditionKind::Any,
        MkCondition::Not { .. } => ConditionKind::Not,
    }
}
pub fn replace_condition(c: &mut MkCondition, kind: ConditionKind) {
    *c = default_condition(kind);
}
pub fn append_child(c: &mut MkCondition) -> bool {
    match c {
        MkCondition::All { conditions } | MkCondition::Any { conditions } => {
            conditions.push(default_condition(ConditionKind::Variable));
            true
        }
        _ => false,
    }
}
pub fn remove_child(c: &mut MkCondition, index: usize) -> bool {
    match c {
        MkCondition::All { conditions } | MkCondition::Any { conditions }
            if index < conditions.len() =>
        {
            conditions.remove(index);
            true
        }
        _ => false,
    }
}

/// Resolves the same recursive path shape returned by `condition_ui` when the
/// picker button for the first window condition is requested. Kept private to
/// the dialog module so routing tests do not expose editor internals publicly.
#[cfg(test)]
pub(super) fn first_window_picker_path(condition: &MkCondition) -> Option<Vec<usize>> {
    match condition {
        MkCondition::WindowExists { .. } | MkCondition::WindowActive { .. } => Some(vec![]),
        MkCondition::All { conditions } | MkCondition::Any { conditions } => {
            conditions.iter().enumerate().find_map(|(index, child)| {
                first_window_picker_path(child).map(|mut path| {
                    path.insert(0, index);
                    path
                })
            })
        }
        MkCondition::Not { condition } => first_window_picker_path(condition).map(|mut path| {
            path.insert(0, 0);
            path
        }),
        _ => None,
    }
}

/// Edits a condition tree using only assets from the active macro.  The same
/// catalog is threaded through every recursive child, so nested conditions do
/// not lose their authoring context.
pub(super) fn condition_ui_with_assets(
    ui: &mut egui::Ui,
    condition: &mut MkCondition,
    context: &TargetEditorContext<'_>,
) -> Option<ConditionEditorRequest> {
    let assets = context.assets;
    let mut requested = None;
    ui.group(|ui| {
        let kind = condition_kind(condition);
        let mut next = kind;
        egui::ComboBox::from_label("Condition type")
            .selected_text(kind_label(kind))
            .show_ui(ui, |ui| {
                for k in [
                    ConditionKind::Variable,
                    ConditionKind::WindowExists,
                    ConditionKind::WindowActive,
                    ConditionKind::ImageSearch,
                    ConditionKind::PreviousImageResult,
                    ConditionKind::PixelResult,
                    ConditionKind::All,
                    ConditionKind::Any,
                    ConditionKind::Not,
                ] {
                    ui.selectable_value(&mut next, k, kind_label(k));
                }
            });
        if next != kind {
            replace_condition(condition, next);
        }
        match condition {
            MkCondition::Variable { name, op, value } => {
                ui.horizontal(|ui| {
                    ui.label("Variable name");
                    ui.text_edit_singleline(name);
                });
                egui::ComboBox::from_label("Comparison")
                    .selected_text(op_label(op))
                    .show_ui(ui, |ui| {
                        for candidate in [
                            MkCompareOp::Eq,
                            MkCompareOp::NotEq,
                            MkCompareOp::Less,
                            MkCompareOp::LessOrEq,
                            MkCompareOp::Greater,
                            MkCompareOp::GreaterOrEq,
                            MkCompareOp::Contains,
                            MkCompareOp::StartsWith,
                            MkCompareOp::EndsWith,
                            MkCompareOp::Regex,
                        ] {
                            let label = op_label(&candidate);
                            ui.selectable_value(op, candidate, label);
                        }
                    });
                value_ui(ui, value);
            }
            MkCondition::WindowExists { matcher } | MkCondition::WindowActive { matcher } => {
                if matcher_ui(ui, matcher).pick_window {
                    requested = Some(ConditionEditorRequest::WindowMatcher {
                        path: ConditionPath::root(),
                    });
                }
            }
            MkCondition::ImageSearch { search, found } => {
                if let Some(operation) = super::image_search_controls::show_shared_fields(
                    ui,
                    &mut search.asset_id,
                    &mut search.region,
                    &mut search.tolerance,
                    &mut search.alpha,
                    &mut search.return_point,
                    assets,
                ) {
                    use super::image_search_controls::SharedImageOperation as S;
                    use ConditionImageOperation as O;
                    let operation = match operation {
                        S::ImportPng => O::ImportPng,
                        S::CaptureRectangle => O::CaptureRectangle,
                        S::PickRectangle => O::PickRectangle,
                        S::PreviewRectangle => O::PreviewRectangle,
                        S::HighlightMonitor => O::HighlightMonitor,
                        S::PickWindow => O::PickWindow,
                        S::HighlightWindow => O::HighlightWindow,
                    };
                    requested = Some(ConditionEditorRequest::Image(ConditionImageRequest {
                        path: ConditionPath::root(),
                        operation,
                    }));
                }
                egui::ComboBox::from_label("Expected")
                    .selected_text(if *found { "Found" } else { "Not Found" })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(found, true, "Found");
                        ui.selectable_value(found, false, "Not found");
                    });
            }
            MkCondition::PreviousImageResult { asset_id, found } => {
                let mut specific = asset_id.is_some();
                if ui
                    .checkbox(&mut specific, "Use a specific image's latest result")
                    .changed()
                {
                    *asset_id = specific.then_some(assets.first().map_or(1, |a| a.id));
                }
                if let Some(id) = asset_id {
                    egui::ComboBox::from_label("Reference image")
                        .selected_text(super::action_editor::image_asset_label(*id, assets))
                        .show_ui(ui, |ui| {
                            for asset in assets {
                                ui.selectable_value(
                                    id,
                                    asset.id,
                                    super::action_editor::image_asset_label(asset.id, assets),
                                );
                            }
                        });
                }
                egui::ComboBox::from_label("Result")
                    .selected_text(if *found { "Found" } else { "Not found" })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(found, true, "Found");
                        ui.selectable_value(found, false, "Not found");
                    });
            }
            MkCondition::PixelResult {
                target,
                color,
                tolerance,
            } => {
                let _ = target_ui(ui, target, context);
                ui.horizontal(|ui| {
                    ui.label("Color");
                    ui.text_edit_singleline(color);
                    ui.label("Tolerance");
                    ui.add(egui::DragValue::new(tolerance));
                });
            }
            MkCondition::All { conditions } => requested = group_ui(ui, conditions, context, true),
            MkCondition::Any { conditions } => requested = group_ui(ui, conditions, context, false),
            MkCondition::Not { condition } => {
                ui.indent("not-child", |ui| {
                    if let Some(mut request) = condition_ui_with_assets(ui, condition, context) {
                        prepend_request(&mut request, ConditionBranch::Not);
                        requested = Some(request)
                    }
                });
            }
        }
    });
    requested
}

/// Condition editor used by the action dialog. The active macro context is
/// passed unchanged through the whole condition tree.
pub fn condition_ui_with_context(
    ui: &mut egui::Ui,
    condition: &mut MkCondition,
    context: ImageAssetUiContext<'_>,
) -> Option<ConditionEditorRequest> {
    let target_context = TargetEditorContext {
        macro_id: context.macro_id,
        assets: context.assets,
        store: context.store,
    };
    condition_ui_context_at(
        ui,
        condition,
        context,
        &target_context,
        &ConditionPath::root(),
    )
}

fn condition_ui_context_at(
    ui: &mut egui::Ui,
    condition: &mut MkCondition,
    context: ImageAssetUiContext<'_>,
    target_context: &TargetEditorContext<'_>,
    path: &ConditionPath,
) -> Option<ConditionEditorRequest> {
    // The established editor handles all non-browser controls and request
    // routing. Temporarily rendering ImageSearch ourselves avoids maintaining
    // a second asset selector while leaving its capture controls untouched.
    if let MkCondition::ImageSearch { search, found } = condition {
        let mut requested = None;
        ui.group(|ui| {
            egui::ComboBox::from_label("Condition type")
                .selected_text(kind_label(ConditionKind::ImageSearch))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut ConditionKind::ImageSearch,
                        ConditionKind::ImageSearch,
                        kind_label(ConditionKind::ImageSearch),
                    );
                });
            super::image_asset_picker::show(ui, path.indexes(), context, &mut search.asset_id);
            if let Some(operation) = super::image_search_controls::show_shared_fields(
                ui,
                &mut search.asset_id,
                &mut search.region,
                &mut search.tolerance,
                &mut search.alpha,
                &mut search.return_point,
                context.assets,
            ) {
                use super::image_search_controls::SharedImageOperation as S;
                use ConditionImageOperation as O;
                let operation = match operation {
                    S::ImportPng => O::ImportPng,
                    S::CaptureRectangle => O::CaptureRectangle,
                    S::PickRectangle => O::PickRectangle,
                    S::PreviewRectangle => O::PreviewRectangle,
                    S::HighlightMonitor => O::HighlightMonitor,
                    S::PickWindow => O::PickWindow,
                    S::HighlightWindow => O::HighlightWindow,
                };
                requested = Some(ConditionEditorRequest::Image(ConditionImageRequest {
                    path: path.clone(),
                    operation,
                }));
            }
            egui::ComboBox::from_label("Expected")
                .selected_text(if *found { "Found" } else { "Not Found" })
                .show_ui(ui, |ui| {
                    ui.selectable_value(found, true, "Found");
                    ui.selectable_value(found, false, "Not found");
                });
        });
        return requested;
    }
    // Recursive containers need the context-aware path before their child is
    // rendered, both for request routing and stable per-node picker state.
    match condition {
        MkCondition::All { conditions } => {
            let mut request = None;
            for (index, child) in conditions.iter_mut().enumerate() {
                let branch = ConditionBranch::All(index);
                let mut child_path = path.clone();
                child_path.push(branch);
                ui.indent(index, |ui| {
                    if let Some(r) =
                        condition_ui_context_at(ui, child, context, target_context, &child_path)
                    {
                        request = Some(r);
                    }
                });
            }
            if ui.button("+ Add Condition").clicked() {
                conditions.push(default_condition(ConditionKind::Variable));
            }
            request
        }
        MkCondition::Any { conditions } => {
            let mut request = None;
            for (index, child) in conditions.iter_mut().enumerate() {
                let mut child_path = path.clone();
                child_path.push(ConditionBranch::Any(index));
                ui.indent(index, |ui| {
                    if let Some(r) =
                        condition_ui_context_at(ui, child, context, target_context, &child_path)
                    {
                        request = Some(r);
                    }
                });
            }
            if ui.button("+ Add Condition").clicked() {
                conditions.push(default_condition(ConditionKind::Variable));
            }
            request
        }
        MkCondition::Not { condition } => {
            let mut child_path = path.clone();
            child_path.push(ConditionBranch::Not);
            ui.indent("not-child", |ui| {
                condition_ui_context_at(ui, condition, context, target_context, &child_path)
            })
            .inner
        }
        _ => condition_ui_with_assets(ui, condition, target_context).map(|mut request| {
            match &mut request {
                ConditionEditorRequest::WindowMatcher { path: p }
                | ConditionEditorRequest::Image(ConditionImageRequest { path: p, .. }) => {
                    *p = path.clone()
                }
            }
            request
        }),
    }
}
fn group_ui(
    ui: &mut egui::Ui,
    conditions: &mut Vec<MkCondition>,
    context: &TargetEditorContext<'_>,
    is_all: bool,
) -> Option<ConditionEditorRequest> {
    let mut remove = None;
    let mut requested = None;
    for (index, child) in conditions.iter_mut().enumerate() {
        ui.indent(index, |ui| {
            if let Some(mut request) = condition_ui_with_assets(ui, child, context) {
                prepend_request(
                    &mut request,
                    if is_all {
                        ConditionBranch::All(index)
                    } else {
                        ConditionBranch::Any(index)
                    },
                );
                requested = Some(request);
            }
            if ui.button("Remove").clicked() {
                remove = Some(index);
            }
        });
    }
    if let Some(index) = remove {
        conditions.remove(index);
    }
    if ui.button("+ Add Condition").clicked() {
        conditions.push(default_condition(ConditionKind::Variable));
    }
    requested
}
fn prepend_request(request: &mut ConditionEditorRequest, branch: ConditionBranch) {
    match request {
        ConditionEditorRequest::WindowMatcher { path }
        | ConditionEditorRequest::Image(ConditionImageRequest { path, .. }) => path.prepend(branch),
    }
}
fn kind_label(k: ConditionKind) -> &'static str {
    match k {
        ConditionKind::Variable => "Variable comparison",
        ConditionKind::WindowExists => "Window exists",
        ConditionKind::WindowActive => "Window active",
        ConditionKind::ImageSearch => "Search image now",
        ConditionKind::PreviousImageResult => "Previous image result",
        ConditionKind::PixelResult => "Pixel matches",
        ConditionKind::All => "ALL",
        ConditionKind::Any => "ANY",
        ConditionKind::Not => "NOT",
    }
}
fn op_label(op: &MkCompareOp) -> &'static str {
    match op {
        MkCompareOp::Eq => "Equals",
        MkCompareOp::NotEq => "Not equal",
        MkCompareOp::Less => "Less than",
        MkCompareOp::LessOrEq => "Less than or equal",
        MkCompareOp::Greater => "Greater than",
        MkCompareOp::GreaterOrEq => "Greater than or equal",
        MkCompareOp::Contains => "Contains",
        MkCompareOp::StartsWith => "Starts with",
        MkCompareOp::EndsWith => "Ends with",
        MkCompareOp::Regex => "Regex",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mutations_are_recursive_and_groups_are_editable() {
        let mut c = default_condition(ConditionKind::All);
        assert!(append_child(&mut c));
        if let MkCondition::All { conditions } = &mut c {
            replace_condition(&mut conditions[0], ConditionKind::Not);
            if let MkCondition::Not { condition } = &mut conditions[0] {
                replace_condition(condition, ConditionKind::PixelResult);
            }
        }
        assert!(remove_child(&mut c, 0));
        assert!(matches!(c, MkCondition::All { conditions } if conditions.is_empty()));
    }

    #[test]
    fn nested_condition_data_survives_recursive_mutation_helpers() {
        let nested = MkCondition::All {
            conditions: vec![
                MkCondition::Any {
                    conditions: vec![
                        MkCondition::WindowExists {
                            matcher: default_matcher(),
                        },
                        MkCondition::PreviousImageResult {
                            asset_id: Some(42),
                            found: false,
                        },
                    ],
                },
                MkCondition::Not {
                    condition: Box::new(MkCondition::PixelResult {
                        target: MkCoordinateTarget::ActiveWindow {
                            point: MkPoint { x: -2, y: 9 },
                        },
                        color: "#12abEF".into(),
                        tolerance: 17,
                    }),
                },
                MkCondition::WindowActive {
                    matcher: MkWindowMatcher {
                        title: Some("Editor".into()),
                        title_regex: Some("^Edit".into()),
                        process: Some("app.exe".into()),
                        class: Some("Main".into()),
                    },
                },
                MkCondition::Variable {
                    name: "score".into(),
                    op: MkCompareOp::GreaterOrEq,
                    value: MkValue::Number(3.25),
                },
            ],
        };
        let mut edited = nested.clone();
        assert!(append_child(&mut edited));
        assert!(remove_child(&mut edited, 4));
        assert_eq!(edited, nested);
    }

    #[test]
    fn typed_paths_resolve_nested_groups_and_not_strictly() {
        let image = default_condition(ConditionKind::ImageSearch);
        let mut root = MkCondition::All {
            conditions: vec![
                image.clone(),
                MkCondition::Not {
                    condition: Box::new(image.clone()),
                },
                MkCondition::Any {
                    conditions: vec![MkCondition::All {
                        conditions: vec![image],
                    }],
                },
            ],
        };
        for path in [vec![0], vec![1, 0], vec![2, 0, 0]] {
            assert!(matches!(
                resolve_condition_mut(&mut root, &ConditionPath::from_indexes(path)),
                Some(MkCondition::ImageSearch { .. })
            ));
        }
        assert!(resolve_condition_mut(&mut root, &ConditionPath::from_indexes(vec![9])).is_none());
        assert!(
            resolve_condition_mut(&mut root, &ConditionPath::from_indexes(vec![1, 1])).is_none()
        );
        replace_condition(
            resolve_condition_mut(&mut root, &ConditionPath::from_indexes(vec![0])).unwrap(),
            ConditionKind::Variable,
        );
        assert!(!matches!(
            resolve_condition_mut(&mut root, &ConditionPath::from_indexes(vec![0])),
            Some(MkCondition::ImageSearch { .. })
        ));
    }

    #[test]
    fn request_prepending_preserves_operation_and_full_path() {
        for operation in [
            ConditionImageOperation::ImportPng,
            ConditionImageOperation::CaptureRectangle,
            ConditionImageOperation::PickRectangle,
            ConditionImageOperation::PreviewRectangle,
            ConditionImageOperation::HighlightMonitor,
            ConditionImageOperation::PickWindow,
            ConditionImageOperation::HighlightWindow,
        ] {
            let mut request = ConditionEditorRequest::Image(ConditionImageRequest {
                path: ConditionPath::root(),
                operation,
            });
            prepend_request(&mut request, ConditionBranch::Any(1));
            prepend_request(&mut request, ConditionBranch::All(0));
            let mut expected_path = ConditionPath::root();
            expected_path.prepend(ConditionBranch::Any(1));
            expected_path.prepend(ConditionBranch::All(0));
            assert_eq!(
                request,
                ConditionEditorRequest::Image(ConditionImageRequest {
                    path: expected_path,
                    operation
                })
            );
        }
    }
}
