use eframe::egui;

#[derive(Default)]
pub struct ConvertPanel {
    pub open: bool,
    pub value: String,
    pub from: String,
    pub to: String,
    pub filter: String,
}

impl ConvertPanel {
    pub fn open(&mut self) {
        self.open = true;
    }

    pub fn ui(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }

        egui::Window::new("Converter")
            .open(&mut self.open)
            .resizable(true)
            .default_size((300.0, 120.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Value");
                    ui.text_edit_singleline(&mut self.value);
                });
                ui.horizontal(|ui| {
                    ui.label("Filter");
                    ui.text_edit_singleline(&mut self.filter);
                });

                let units = [
                    "m", "km", "mi", "ft", "in", "cm", "mm", "kg", "g", "lb", "oz", "c", "f", "k",
                    "l", "ml", "gal",
                ];

                let filter = self.filter.to_lowercase();
                let opts: Vec<&str> = units
                    .iter()
                    .copied()
                    .filter(|u| filter.is_empty() || u.contains(&filter))
                    .collect();

                egui::ComboBox::from_label("From")
                    .selected_text(if self.from.is_empty() {
                        "Select".into()
                    } else {
                        self.from.clone()
                    })
                    .show_ui(ui, |ui| {
                        for unit in &opts {
                            ui.selectable_value(&mut self.from, unit.to_string(), *unit);
                        }
                    });

                egui::ComboBox::from_label("To")
                    .selected_text(if self.to.is_empty() {
                        "Select".into()
                    } else {
                        self.to.clone()
                    })
                    .show_ui(ui, |ui| {
                        for unit in &opts {
                            ui.selectable_value(&mut self.to, unit.to_string(), *unit);
                        }
                    });
            });
    }
}
