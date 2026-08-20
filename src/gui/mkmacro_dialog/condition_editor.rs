//! Recursive editor and pure mutation operations for macro conditions.

use super::action_editor::{matcher_ui, target_ui, value_ui};
use crate::mkmacro::variables::{MkPoint, MkValue};
use crate::mkmacro::{MkCompareOp, MkCondition, MkCoordinateTarget, MkWindowMatcher};
use eframe::egui;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConditionKind {
    Variable,
    WindowExists,
    WindowActive,
    ImageResult,
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
        ConditionKind::ImageResult => MkCondition::ImageResult {
            asset_id: 1,
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
        MkCondition::ImageResult { .. } => ConditionKind::ImageResult,
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

pub fn condition_ui(ui: &mut egui::Ui, condition: &mut MkCondition) -> Option<Vec<usize>> {
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
                    ConditionKind::ImageResult,
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
                if matcher_ui(ui, matcher) {
                    requested = Some(vec![]);
                }
            }
            MkCondition::ImageResult { asset_id, found } => {
                ui.horizontal(|ui| {
                    ui.label("Image asset ID");
                    ui.add(egui::DragValue::new(asset_id));
                });
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
                let _ = target_ui(ui, target);
                ui.horizontal(|ui| {
                    ui.label("Color");
                    ui.text_edit_singleline(color);
                    ui.label("Tolerance");
                    ui.add(egui::DragValue::new(tolerance));
                });
            }
            MkCondition::All { conditions } | MkCondition::Any { conditions } => {
                requested = group_ui(ui, conditions)
            }
            MkCondition::Not { condition } => {
                ui.indent("not-child", |ui| {
                    if let Some(mut p) = condition_ui(ui, condition) {
                        p.insert(0, 0);
                        requested = Some(p)
                    }
                });
            }
        }
    });
    requested
}
fn group_ui(ui: &mut egui::Ui, conditions: &mut Vec<MkCondition>) -> Option<Vec<usize>> {
    let mut remove = None;
    let mut requested = None;
    for (index, child) in conditions.iter_mut().enumerate() {
        ui.indent(index, |ui| {
            if let Some(mut p) = condition_ui(ui, child) {
                p.insert(0, index);
                requested = Some(p);
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
fn kind_label(k: ConditionKind) -> &'static str {
    match k {
        ConditionKind::Variable => "Variable comparison",
        ConditionKind::WindowExists => "Window exists",
        ConditionKind::WindowActive => "Window active",
        ConditionKind::ImageResult => "Image found / not found",
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
                        MkCondition::ImageResult {
                            asset_id: 42,
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
}
