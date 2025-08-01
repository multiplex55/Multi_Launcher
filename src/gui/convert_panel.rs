use crate::gui::LauncherApp;
use eframe::egui;

const OPTIONS: &[&str] = &[
    "meters",
    "kilometers",
    "miles",
    "feet",
    "inches",
    "centimeters",
    "millimeters",
    "grams",
    "kilograms",
    "pounds",
    "ounces",
];

#[derive(Default)]
pub struct ConvertPanel {
    pub open: bool,
    text: String,
    from_idx: usize,
    to_idx: usize,
    filter: String,
}

impl ConvertPanel {
    pub fn open(&mut self) {
        self.open = true;
    }

    pub fn ui(&mut self, ctx: &egui::Context, _app: &mut LauncherApp) {
        if !self.open { return; }
        let mut close = false;
        egui::Window::new("Convert")
            .open(&mut self.open)
            .show(ctx, |ui| {
                ui.text_edit_singleline(&mut self.text);
                let filtered: Vec<&str> = OPTIONS
                    .iter()
                    .copied()
                    .filter(|o| self.filter.is_empty() || o.contains(&self.filter))
                    .collect();
                ui.horizontal(|ui| {
                    ui.label("From");
                    let from_text = filtered.get(self.from_idx).copied().unwrap_or("");
                    egui::ComboBox::from_id_source("convert_from")
                        .selected_text(from_text)
                        .show_ui(ui, |ui| {
                            for (i, opt) in filtered.iter().enumerate() {
                                ui.selectable_value(&mut self.from_idx, i, *opt);
                            }
                        });
                    ui.label("To");
                    let to_text = filtered.get(self.to_idx).copied().unwrap_or("");
                    egui::ComboBox::from_id_source("convert_to")
                        .selected_text(to_text)
                        .show_ui(ui, |ui| {
                            for (i, opt) in filtered.iter().enumerate() {
                                ui.selectable_value(&mut self.to_idx, i, *opt);
                            }
                        });
                });
                ui.label("Filter");
                ui.text_edit_singleline(&mut self.filter);
                if ui.button("Close").clicked() {
                    close = true;
                }
            });
        if close {
            self.open = false;
        }
    }
}

