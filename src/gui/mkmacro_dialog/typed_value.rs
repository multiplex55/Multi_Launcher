//! Shared value controls. Constructing an input and rendering it are separate:
//! rendering an incompatible persisted value must never silently repair it.
use crate::mkmacro::{MkPoint, MkValue, MkValueType};
use eframe::egui;

pub fn initial_value(kind: MkValueType) -> MkValue {
    match kind {
        MkValueType::String => MkValue::String(String::new()),
        MkValueType::Number => MkValue::Number(0.0),
        MkValueType::Boolean => MkValue::Boolean(false),
        MkValueType::Point => MkValue::Point(MkPoint::default()),
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ValueEditResponse {
    pub changed: bool,
    pub compatible: bool,
}
pub fn fixed_type_ui(
    ui: &mut egui::Ui,
    kind: MkValueType,
    value: &mut MkValue,
) -> ValueEditResponse {
    if !kind.accepts(value) {
        ui.colored_label(
            egui::Color32::RED,
            format!("Value requires {}", kind.label()),
        );
        return ValueEditResponse {
            changed: false,
            compatible: false,
        };
    }
    ValueEditResponse {
        changed: value_controls(ui, value),
        compatible: true,
    }
}

pub fn value_controls(ui: &mut egui::Ui, value: &mut MkValue) -> bool {
    match value {
        MkValue::String(text) => {
            ui.horizontal(|ui| {
                ui.label("Value");
                ui.text_edit_singleline(text).changed()
            })
            .inner
        }
        MkValue::Number(number) => {
            ui.horizontal(|ui| {
                ui.label("Value");
                ui.add(egui::DragValue::new(number)).changed()
            })
            .inner
        }
        MkValue::Boolean(boolean) => ui.checkbox(boolean, "Value").changed(),
        MkValue::Point(point) => {
            ui.horizontal(|ui| {
                ui.label("X");
                let x = ui.add(egui::DragValue::new(&mut point.x)).changed();
                ui.label("Y");
                let y = ui.add(egui::DragValue::new(&mut point.y)).changed();
                x || y
            })
            .inner
        }
        MkValue::Null => {
            ui.small("Null has no value.");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_type_render_does_not_coerce_incompatible_values() {
        let context = egui::Context::default();
        let mut value = MkValue::Null;
        let _ = context.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let edit = fixed_type_ui(ui, MkValueType::Number, &mut value);
                assert!(!edit.compatible && !edit.changed);
            });
        });
        assert_eq!(value, MkValue::Null);
        for kind in [
            MkValueType::String,
            MkValueType::Number,
            MkValueType::Boolean,
            MkValueType::Point,
        ] {
            assert!(kind.accepts(&initial_value(kind)));
        }
    }
}
