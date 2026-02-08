use crate::actions::Action;
use crate::gui::LauncherApp;
use eframe::egui;
use std::collections::HashSet;

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
                    ui.label(egui::RichText::new("Dashboard").strong());
                    ui.label(
                        "Open Settings → Dashboard → Customize Dashboard... to edit \
                         widget layout plus plugin-aware settings such as note/todo queries, \
                         context links, or weather location.",
                    );
                    ui.separator();
                    ui.label(egui::RichText::new("Linked context").strong());
                    ui.monospace("todo add Draft release @note:release-plan");
                    ui.monospace("note add Release Notes @todo:todo-123");
                    ui.monospace("cal add tomorrow 09:00 Kickoff @todo:todo-123");
                    ui.label("Use note/todo right-click menus to link existing items.");
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
                ui.label(egui::RichText::new("Query filters").strong());
                ui.label("Use filters to narrow results across plugins and widgets:");
                ui.monospace("  tag:<label>   !tag:<label>");
                ui.monospace("  kind:<kind>   id:<action_id>");
                ui.monospace("  Quotes for spaces: tag:\"high priority\"");
                ui.separator();
                ui.label(egui::RichText::new("Linked note/todo/calendar references").strong());
                ui.label("Use lightweight entity refs in command text:");
                ui.monospace("  todo add <text> [p=<priority>] [#tag] @note:<id> @event:<id>");
                ui.monospace("  note add <title/text> @todo:<id>");
                ui.monospace("  cal add <date> <time|all-day> <title> @todo:<id> @note:<id>");
                ui.label("UI linking:");
                ui.monospace("  Notes dialog → right-click note → Link to todo");
                ui.monospace("  Todos dialog → right-click todo → Link note");
                ui.separator();
                ui.label(egui::RichText::new("Launcher actions").strong());
                ui.label("Use these action IDs in custom actions, macros, or gestures:");
                ui.monospace("  launcher:toggle  launcher:show  launcher:hide");
                ui.monospace("  launcher:focus   launcher:restore");
                ui.separator();
                let mut command_map = std::collections::HashMap::new();
                for plugin in app.plugins.iter() {
                    command_map.insert(plugin.name().to_string(), plugin.commands());
                }
                let mut infos = app.plugins.plugin_infos();
                infos.sort_by(|a, b| a.0.cmp(&b.0));
                let area_height = ui.available_height();
                egui::ScrollArea::vertical()
                    .max_height(area_height)
                    .show(ui, |ui| {
                        let filt = self.filter.to_lowercase();
                        for (name, desc, capabilities) in infos {
                            let commands = command_map.get(&name).cloned().unwrap_or_default();
                            if !matches_filter(&filt, &name, &desc, &capabilities, &commands) {
                                continue;
                            }
                            let prefixes = command_prefixes(&commands);
                            ui.label(egui::RichText::new(&name).strong());
                            ui.label(desc);
                            if capabilities.is_empty() {
                                ui.label("Capabilities: none");
                            } else {
                                ui.label(format!("Capabilities: {}", capabilities.join(", ")));
                            }
                            if prefixes.is_empty() {
                                ui.label("Prefixes: none");
                            } else {
                                ui.label(format!("Prefixes: {}", prefixes.join(", ")));
                            }
                            if commands.is_empty() {
                                ui.label("Usage: none");
                            } else {
                                ui.label("Usage:");
                                for command in &commands {
                                    ui.monospace(format!("  {}", command.label));
                                }
                            }
                            if self.show_examples && !commands.is_empty() {
                                ui.label("Examples:");
                                for command in &commands {
                                    ui.monospace(format!("  {}", example_line(command)));
                                }
                            }
                            ui.separator();
                        }
                    });
            });
        self.open = open;
    }
}

fn matches_filter(
    filter: &str,
    name: &str,
    desc: &str,
    capabilities: &[String],
    commands: &[Action],
) -> bool {
    if filter.is_empty() {
        return true;
    }
    let filter = filter.to_lowercase();
    if name.to_lowercase().contains(&filter) || desc.to_lowercase().contains(&filter) {
        return true;
    }
    if capabilities
        .iter()
        .any(|cap| cap.to_lowercase().contains(&filter))
    {
        return true;
    }
    commands.iter().any(|command| {
        command.label.to_lowercase().contains(&filter)
            || command.desc.to_lowercase().contains(&filter)
            || command.action.to_lowercase().contains(&filter)
    })
}

fn command_prefixes(commands: &[Action]) -> Vec<String> {
    let mut prefixes = HashSet::new();
    for command in commands {
        if let Some(prefix) = command.label.split_whitespace().next() {
            prefixes.insert(prefix.to_string());
        }
    }
    let mut out: Vec<String> = prefixes.into_iter().collect();
    out.sort();
    out
}

fn example_line(command: &Action) -> String {
    if let Some(query) = command.action.strip_prefix("query:") {
        query.trim().to_string()
    } else {
        command.label.clone()
    }
}
