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
                if ui
                    .checkbox(&mut self.show_examples, "Show examples")
                    .changed()
                {
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
        "calculator" => Some(&["= 1+2", "= history"]),
        "unit_convert" => Some(&[
            "conv 10 km to mi",
            "conv 1 kwh to j",
            "conv 8 bit to byte",
            "conv 30 mpg to kpl",
            "conv 180 deg to rad",
        ]),
        "base_convert" => Some(&[
            "conv 1010 bin to hex",
            "conv ff hex to bin",
            "conv \"hello\" text to hex",
            "conv 48656c6c6f hex to text",
            "conv 42 dec to bin",
        ]),
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
            "timer add <duration> [name]",
            "timer add 10s tea",
            "timer add 5m",
            "timer add 1:30",
            "durations: s/m/h or hh:mm:ss/mm:ss",
            "alarm <HH:MM> [name]",
            "alarm 07:30 wake up",
            "timer list  # show active timers",
            "alarm list  # show active alarms",
            "timer pause <id>  # pause a timer",
            "timer resume <id>  # resume a timer",
            "timer cancel  # cancel timers/alarms",
            "timer rm  # remove timers",
        ]),
        "notes" => Some(&[
            "note",
            "note add buy milk",
            "note new buy milk",
            "note create buy milk",
            "note list",
            "note rm groceries",
        ]),
        "volume" => Some(&["vol 50"]),
        "brightness" => Some(&["bright 50"]),
        "asciiart" => Some(&["ascii hello"]),
        "screenshot" => Some(&["ss", "ss clip"]),
        "processes" => Some(&["ps", "ps firefox"]),
        "dropcalc" => Some(&["drop 1/128 25"]),
        "recycle" => Some(&["rec"]),
        "tempfile" => Some(&["tmp new", "tmp create"]),
        "timestamp" => Some(&[
            "ts 0",
            "ts 2024-05-01 12:00",
            "tsm 3600000",
            "tsm 01:00:00.500",
        ]),
        "snippets" => Some(&["cs hello"]),
        "favorites" => Some(&["fav add mycmd", "fav list"]),
        "browser_tabs" => Some(&["tab", "tab cache"]),
        "emoji" => Some(&["emoji smile", "emoji list heart"]),
        "ip" => Some(&["ip", "ip public"]),
        "lorem" => Some(&["lorem w 5", "lorem s 2"]),
        "macros" => Some(&["macro list", "macro mymacro"]),
        "omni_search" => Some(&["o list", "o list docs"]),
        "random" => Some(&["rand number 10", "rand dice", "rand pw 8"]),
        "settings" => Some(&["settings"]),
        "stopwatch" => Some(&["sw start", "sw list"]),
        "task_manager" => Some(&["tm"]),
        "text_case" => Some(&["case snake Hello World"]),
        "todo" => Some(&["todo add buy milk", "todo list"]),
        "wikipedia" => Some(&["wiki rust"]),
        "help" => Some(&["help"]),
        _ => None,
    }
}
