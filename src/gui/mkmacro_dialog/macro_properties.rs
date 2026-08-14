use super::MkMacroDialog;
use crate::mkmacro::{MkHotkey, MkKey};
use eframe::egui;

pub fn key_name(key: &MkKey) -> String {
    match key {
        MkKey::Character(v) => v.to_uppercase(),
        MkKey::Function(n) => format!("F{n}"),
        MkKey::PageUp => "Page Up".into(),
        MkKey::PageDown => "Page Down".into(),
        MkKey::LeftControl | MkKey::RightControl | MkKey::Control => "Ctrl".into(),
        MkKey::LeftAlt | MkKey::RightAlt | MkKey::Alt => "Alt".into(),
        MkKey::LeftShift | MkKey::RightShift | MkKey::Shift => "Shift".into(),
        MkKey::LeftMeta | MkKey::RightMeta | MkKey::Meta => "Meta".into(),
        MkKey::Enter => "Enter".into(),
        MkKey::Tab => "Tab".into(),
        MkKey::Escape => "Escape".into(),
        MkKey::Space => "Space".into(),
        MkKey::Backspace => "Backspace".into(),
        MkKey::Delete => "Delete".into(),
        MkKey::Up => "Up".into(),
        MkKey::Down => "Down".into(),
        MkKey::Left => "Left".into(),
        MkKey::Right => "Right".into(),
        MkKey::Home => "Home".into(),
        MkKey::End => "End".into(),
    }
}
pub fn hotkey_name(h: &MkHotkey) -> String {
    h.modifiers
        .iter()
        .chain(std::iter::once(&h.key))
        .map(key_name)
        .collect::<Vec<_>>()
        .join(" + ")
}
fn egui_key(k: egui::Key) -> Option<MkKey> {
    use egui::Key::*;
    Some(match k {
        Enter => MkKey::Enter,
        Tab => MkKey::Tab,
        Space => MkKey::Space,
        ArrowUp => MkKey::Up,
        ArrowDown => MkKey::Down,
        ArrowLeft => MkKey::Left,
        ArrowRight => MkKey::Right,
        Home => MkKey::Home,
        End => MkKey::End,
        PageUp => MkKey::PageUp,
        PageDown => MkKey::PageDown,
        F1 => MkKey::Function(1),
        F2 => MkKey::Function(2),
        F3 => MkKey::Function(3),
        F4 => MkKey::Function(4),
        F5 => MkKey::Function(5),
        F6 => MkKey::Function(6),
        F7 => MkKey::Function(7),
        F8 => MkKey::Function(8),
        F9 => MkKey::Function(9),
        F10 => MkKey::Function(10),
        F11 => MkKey::Function(11),
        F12 => MkKey::Function(12),
        _ => return None,
    })
}
pub(super) fn show(ui: &mut egui::Ui, d: &mut MkMacroDialog) {
    let capturing = d.hotkey_capture;
    let mut changed = false;
    let mut capture = None;
    let mut clear = false;
    let Some(m) = d.selected_macro_mut() else {
        ui.label("Select a macro");
        return;
    };
    ui.heading("Macro Properties");
    changed |= ui.text_edit_singleline(&mut m.name).changed();
    changed |= ui
        .add(
            egui::TextEdit::multiline(&mut m.description)
                .desired_rows(2)
                .hint_text("Description"),
        )
        .changed();
    changed |= ui.checkbox(&mut m.enabled, "Enabled").changed();
    ui.horizontal(|ui| {
        ui.label("Hotkey:");
        let label = m
            .hotkey
            .as_ref()
            .map(hotkey_name)
            .unwrap_or_else(|| "None".into());
        if ui
            .button(if capturing { "Press a key…" } else { &label })
            .clicked()
        {
            capture = Some(true);
        }
        if m.hotkey.is_some() && ui.small_button("Clear").clicked() {
            clear = true;
        }
    });
    egui::CollapsingHeader::new("Playback Settings").show(ui, |ui| {
        changed |= ui
            .add(egui::Slider::new(&mut m.playback.speed_percent, 1..=1000).text("Speed %"))
            .changed();
        changed |= ui
            .add(
                egui::DragValue::new(&mut m.playback.random_delay_ms)
                    .clamp_range(0..=60_000)
                    .suffix(" ms random delay"),
            )
            .changed();
        changed |= ui
            .add(
                egui::DragValue::new(&mut m.playback.random_offset_px)
                    .clamp_range(0..=10_000)
                    .suffix(" px random offset"),
            )
            .changed();
    });
    if clear {
        m.hotkey = None;
        changed = true;
    }
    let _ = m;
    if capture == Some(true) {
        d.hotkey_capture = true;
    }
    if capturing {
        let events = ui.input(|i| i.events.clone());
        for event in events {
            if let egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } = event
            {
                if key == egui::Key::Escape {
                    d.hotkey_capture = false;
                    break;
                }
                if matches!(key, egui::Key::Backspace | egui::Key::Delete) {
                    if let Some(m) = d.selected_macro_mut() {
                        m.hotkey = None;
                    }
                    changed = true;
                    d.hotkey_capture = false;
                    break;
                }
                if let Some(key) = egui_key(key) {
                    let mut mods = vec![];
                    if modifiers.ctrl {
                        mods.push(MkKey::Control)
                    }
                    if modifiers.shift {
                        mods.push(MkKey::Shift)
                    }
                    if modifiers.alt {
                        mods.push(MkKey::Alt)
                    }
                    if modifiers.mac_cmd {
                        mods.push(MkKey::Meta)
                    }
                    if let Some(m) = d.selected_macro_mut() {
                        m.hotkey = Some(MkHotkey {
                            key,
                            modifiers: mods,
                        });
                    }
                    changed = true;
                    d.hotkey_capture = false;
                    break;
                }
            }
        }
    }
    if let Some(w) = crate::mkmacro::hotkeys::validate_hotkeys(&d.draft, &[])
        .into_iter()
        .find(|x| Some(x.macro_id) == d.selected_macro_id)
    {
        ui.colored_label(egui::Color32::YELLOW, w.message);
    }
    if changed {
        d.mark_dirty();
    }
}
