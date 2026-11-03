//! Narrow native-key filtering shared by utility viewports.

use eframe::egui;

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
