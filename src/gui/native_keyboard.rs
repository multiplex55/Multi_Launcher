//! Narrow native-key filtering shared by utility viewports.

use eframe::egui;

/// Dismiss the deepest visible submenu for this exact memory-backed popup.
pub(super) fn dismiss_owned_submenu_on_escape(ui: &egui::Ui) -> Option<egui::Id> {
    let ctx = ui.ctx();
    let menu = ui.layer_id().id;
    if !ui.is_enabled()
        || !ui.is_visible()
        || ui.is_sizing_pass()
        || ctx.will_discard()
        || !ctx.input(|input| input.focused && input.key_pressed(egui::Key::Escape))
        || !egui::Popup::is_id_open(ctx, menu)
    {
        return None;
    }
    let mut parent = menu;
    let mut deepest = None;
    for _ in 0..8 {
        let next = egui::menu::MenuState::from_id(ctx, parent, |state| state.open_item);
        let Some(child) = next else { break };
        deepest = Some((parent, child));
        parent = child;
    }
    let (parent, child) = deepest?;
    let layer = egui::LayerId::new(egui::Order::Foreground, child);
    let bounds = ctx.memory(|memory| memory.area_rect(child))?;
    if !bounds.is_positive()
        || ctx.layer_id_at(bounds.center()) != Some(layer)
        || !ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        return None;
    }
    egui::menu::MenuState::from_id(ctx, parent, |state| state.open_item = None);
    Some(child)
}

/// Draw the actual submenu trigger. A just-dismissed trigger regains focus.
pub(super) fn show_submenu(
    ui: &mut egui::Ui,
    label: &'static str,
    dismissed: Option<egui::Id>,
    contents: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    let button = egui::menu::SubMenuButton::new(label);
    let Some(dismissed) = dismissed else {
        return button.ui(ui, contents).0;
    };
    let response = ui.add(button.button);
    if egui::menu::SubMenu::id_from_widget_id(response.id) == dismissed {
        response.request_focus();
    } else {
        button.sub_menu.show(ui, &response, contents);
    }
    response
}

/// Windows reserves Alt+Space for the native window system menu. Winit also
/// forwards the key's generated text event to egui, so remove only that
/// exact Alt-only keydown and its immediately associated space text.
#[cfg(windows)]
pub(super) fn reserve_windows_system_menu_alt_space(events: &mut Vec<egui::Event>) {
    let mut remove_immediate_space_text = false;
    events.retain(|event| {
        let associated_text_space =
            remove_immediate_space_text && matches!(event, egui::Event::Text(text) if text == " ");
        remove_immediate_space_text = false;

        if matches!(
            event,
            egui::Event::Key {
                key: egui::Key::Space,
                pressed: true,
                modifiers,
                ..
            } if modifiers.alt
                && !modifiers.shift
                && !modifiers.ctrl
                && !modifiers.command
                && !modifiers.mac_cmd
        ) {
            remove_immediate_space_text = true;
            false
        } else {
            !associated_text_space
        }
    });
}
