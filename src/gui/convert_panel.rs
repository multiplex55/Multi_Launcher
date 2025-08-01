use crate::gui::LauncherApp;
use eframe::egui;

const UNITS: &[&str] = &[
    "m", "km", "mi", "ft", "in", "cm", "mm", "kg", "g", "lb", "oz", "l", "ml", "gal", "c", "f", "k",
];

#[derive(Default)]
pub struct ConvertPanel {
    pub open: bool,
    value: String,
    from: String,
    to: String,
    from_filter: String,
    to_filter: String,
}

impl ConvertPanel {
    pub fn open(&mut self) {
        self.open = true;
    }

    pub fn ui(&mut self, ctx: &egui::Context, _app: &mut LauncherApp) {
        if !self.open {
            return;
        }
        let mut close = false;
        egui::Window::new("Converter")
            .resizable(false)
            .open(&mut self.open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Value");
                    ui.text_edit_singleline(&mut self.value);
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label("From");
                        ui.text_edit_singleline(&mut self.from_filter);
                        egui::ComboBox::from_id_source("convert_from")
                            .selected_text(self.from.clone())
                            .show_ui(ui, |ui| {
                                for opt in UNITS.iter().filter(|u| u.contains(&self.from_filter)) {
                                    ui.selectable_value(&mut self.from, (*opt).to_string(), *opt);
                                }
                            });
                    });
                    ui.vertical(|ui| {
                        ui.label("To");
                        ui.text_edit_singleline(&mut self.to_filter);
                        egui::ComboBox::from_id_source("convert_to")
                            .selected_text(self.to.clone())
                            .show_ui(ui, |ui| {
                                for opt in UNITS.iter().filter(|u| u.contains(&self.to_filter)) {
                                    ui.selectable_value(&mut self.to, (*opt).to_string(), *opt);
                                }
                            });
                    });
                });
                if ui.button("Close").clicked() {
                    close = true;
                }
            });
        if close {
            self.open = false;
        }
    }
}
