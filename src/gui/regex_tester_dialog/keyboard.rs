use eframe::egui;

/// Embedded windows own only their focused layer. A native utility may also
/// navigate while its independent viewport has focus and no widget owns it.
pub(super) fn navigation(
    ctx: &egui::Context,
    layer: egui::LayerId,
    independent: bool,
) -> Option<bool> {
    if ctx.memory(|memory| memory.any_popup_open()) || ctx.is_context_menu_open() {
        return None;
    }
    let widget_owned = ctx
        .memory(|memory| memory.focused())
        .and_then(|id| ctx.read_response(id))
        .is_some_and(|response| response.layer_id == layer);
    let viewport_owned = independent && ctx.input(|input| input.viewport().focused == Some(true));
    if !widget_owned && !viewport_owned {
        return None;
    }
    ctx.input_mut(|input| {
        // consume_key permits extra modifiers. Match the event exactly instead,
        // so Ctrl/Alt/Command+F3 remain available to their actual owner.
        for shift in [true, false] {
            let index = input.events.iter().position(|event| {
                matches!(event,
                    egui::Event::Key { key: egui::Key::F3, pressed: true, modifiers, .. }
                    if modifiers.shift == shift && !modifiers.ctrl && !modifiers.alt
                        && !modifiers.command && !modifiers.mac_cmd
                )
            });
            if let Some(index) = index {
                input.events.remove(index);
                return Some(!shift);
            }
        }
        None
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focused_independent_viewport_owns_navigation_and_shift_precedes_plain_f3() {
        let ctx = egui::Context::default();
        let mut input = egui::RawInput::default();
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .focused = Some(true);
        for modifiers in [egui::Modifiers::NONE, egui::Modifiers::SHIFT] {
            input.events.push(egui::Event::Key {
                key: egui::Key::F3,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            });
        }
        let _ = ctx.run(input, |ctx| {
            let layer = egui::LayerId::background();
            assert_eq!(navigation(ctx, layer, false), None);
            assert_eq!(navigation(ctx, layer, true), Some(false));
            assert_eq!(navigation(ctx, layer, true), Some(true));
            assert_eq!(navigation(ctx, layer, true), None);
        });
    }
}
