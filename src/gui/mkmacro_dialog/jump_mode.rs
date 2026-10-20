use crate::gui::jump_mode::{TargetKind, ui::JumpModeScope};
use eframe::egui;

pub(super) fn register_target(
    ui: &egui::Ui,
    scope: Option<&JumpModeScope>,
    response: &egui::Response,
    local_identity: impl AsRef<str>,
    label: impl Into<String>,
    kind: TargetKind,
    visual_order: &mut u64,
) -> Option<crate::gui::jump_mode::ui::FocusDirective> {
    crate::gui::jump_mode::ui::register_and_apply_target(
        scope,
        ui,
        response,
        local_identity,
        label,
        kind,
        visual_order,
    )
}
