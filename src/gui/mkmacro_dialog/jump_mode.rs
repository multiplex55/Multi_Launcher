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
    let order = *visual_order;
    *visual_order = visual_order.saturating_add(1);
    let scope = scope?;
    let crate::gui::jump_mode::ui::RegistrationResult::Registered { directive, .. } =
        scope.register(ui, response, local_identity, label, kind, order)
    else {
        return None;
    };

    if let Some(directive @ crate::gui::jump_mode::ui::FocusDirective::Focus(_)) = directive {
        directive.apply_to_response(response);
        Some(directive)
    } else {
        directive
    }
}
