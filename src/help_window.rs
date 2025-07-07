use crate::gui::LauncherApp;
use eframe::egui;

#[derive(Default)]
pub struct HelpWindow {
    pub open: bool,
    pub show_examples: bool,
    pub overlay_open: bool,
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
                    let mut infos = app.plugins.plugin_infos();
                    infos.sort_by(|a, b| a.0.cmp(&b.0));
                    let area_height = ui.available_height();
                    egui::ScrollArea::vertical()
                        .max_height(area_height)
                        .show(ui, |ui| {
                            for (name, desc, _) in &infos {
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
                ui.checkbox(&mut self.show_examples, "Show examples");
                ui.separator();
                let mut infos = app.plugins.plugin_infos();
                infos.sort_by(|a, b| a.0.cmp(&b.0));
                let area_height = ui.available_height();
                egui::ScrollArea::vertical()
                    .max_height(area_height)
                    .show(ui, |ui| {
                        for (name, desc, _) in infos {
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
        "clipboard" => Some(&["cb"]),
        "bookmarks" => Some(&["bm add https://example.com"]),
        "folders" => Some(&["f add C:/path", "f rm docs"]),
        "shell" => Some(&["sh echo hello"]),
        "system" => Some(&["sys shutdown"]),
        "history" => Some(&["hi"]),
        "help" => Some(&["help"]),
        _ => None,
    }
}
