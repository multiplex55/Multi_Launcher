use crate::gui::LauncherApp;
use eframe::egui;

#[derive(Default)]
pub struct HelpWindow {
    pub open: bool,
    pub show_examples: bool,
    pub overlay_open: bool,
    pub filter: String,
}

impl HelpWindow {
    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if self.overlay_open {
            let mut open = self.overlay_open;
            egui::Window::new("Quick Help")
                .open(&mut open)
                .resizable(true)
                .default_size((300.0, 200.0))
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label(egui::RichText::new("Hotkeys").strong());
                    if let Some(hk) = &app.hotkey_str {
                        ui.label(format!("Toggle launcher: {hk}"));
                    }
                    if let Some(hk) = &app.quit_hotkey_str {
                        ui.label(format!("Quit launcher: {hk}"));
                    }
                    if let Some(hk) = &app.help_hotkey_str {
                        ui.label(format!("Help overlay: {hk}"));
                    }
                    ui.separator();
                ui.label(egui::RichText::new("Commands").strong());
                ui.text_edit_singleline(&mut self.filter);
                let mut infos = app.plugins.plugin_infos();
                infos.sort_by(|a, b| a.0.cmp(&b.0));
                let area_height = ui.available_height();
                egui::ScrollArea::vertical()
                    .max_height(area_height)
                    .show(ui, |ui| {
                        let filt = self.filter.to_lowercase();
                        for (name, desc, _) in &infos {
                            if !filt.is_empty()
                                && !name.to_lowercase().contains(&filt)
                                && !desc.to_lowercase().contains(&filt)
                            {
                                continue;
                            }
                            ui.label(format!("{name}: {desc}"));
                        }
                    });
                });
            self.overlay_open = open;
        }
        if !self.open {
            return;
        }
        let mut open = self.open;
        egui::Window::new("Command Help")
            .open(&mut open)
            .resizable(true)
            .default_size((400.0, 300.0))
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| ui.heading("Available commands"));
                ui.separator();
                if ui.checkbox(&mut self.show_examples, "Show examples").changed() {
                    if let Ok(mut s) = crate::settings::Settings::load(&app.settings_path) {
                        s.show_examples = self.show_examples;
                        let _ = s.save(&app.settings_path);
                    }
                }
                ui.horizontal(|ui| {
                    ui.label("Filter:");
                    ui.text_edit_singleline(&mut self.filter);
                });
                ui.separator();
                let mut infos = app.plugins.plugin_infos();
                infos.sort_by(|a, b| a.0.cmp(&b.0));
                let area_height = ui.available_height();
                egui::ScrollArea::vertical()
                    .max_height(area_height)
                    .show(ui, |ui| {
                        let filt = self.filter.to_lowercase();
                        for (name, desc, _) in infos {
                            if !filt.is_empty()
                                && !name.to_lowercase().contains(&filt)
                                && !desc.to_lowercase().contains(&filt)
                            {
                                continue;
                            }
                            ui.label(egui::RichText::new(&name).strong());
                            ui.label(desc);
                            if self.show_examples {
                                if let Some(examples) = example_queries(&name) {
                                    for ex in examples {
                                        ui.monospace(format!("  {ex}"));
                                    }
                                }
                            }
                            ui.separator();
                        }
                    });
            });
        self.open = open;
    }
}

fn example_queries(name: &str) -> Option<&'static [&'static str]> {
    match name {
        "web_search" => Some(&["g rust"]),
        "runescape_search" => Some(&["rs dragon scimitar", "osrs agility guide"]),
        "youtube" => Some(&["yt rust"]),
        "reddit" => Some(&["red cats"]),
        "calculator" => Some(&["= 1+2"]),
        "unit_convert" => Some(&["conv 10 km to mi"]),
        "clipboard" => Some(&["cb"]),
        "bookmarks" => Some(&["bm add https://example.com", "bm rm", "bm list"]),
        "folders" => Some(&["f add C:/path", "f rm docs"]),
        "shell" => Some(&["sh", "sh echo hello"]),
        "system" => Some(&["sys shutdown"]),
        "sysinfo" => Some(&["info", "info cpu", "info cpu list 5"]),
        "network" => Some(&["net"]),
        "weather" => Some(&["weather Berlin"]),
        "history" => Some(&["hi"]),
        "timer" => Some(&[
            "timer add 10s break",
            "timer list",
            "timer pause 1",
            "timer resume 1",
            "alarm 07:30",
        ]),
        "notes" => Some(&["note", "note add buy milk", "note list", "note rm milk"]),
        "volume" => Some(&["vol 50"]),
        "brightness" => Some(&["bright 50"]),
        "asciiart" => Some(&["ascii hello"]),
        "processes" => Some(&["ps", "ps firefox"]),
        "dropcalc" => Some(&["drop 1/128 25"]),
        "recycle" => Some(&["rec"]),
        "tempfile" => Some(&["tmp new"]),
        "snippets" => Some(&["cs hello"]),
        "todo" => Some(&["todo add buy milk", "todo list"]),
        "wikipedia" => Some(&["wiki rust"]),
        "help" => Some(&["help"]),
        _ => None,
    }
}
