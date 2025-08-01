use crate::gui::LauncherApp;
use eframe::egui;

/// Simple conversion panel with an input box and two combo boxes.
pub struct ConvertPanel {
    pub open: bool,
    input: String,
    filter: String,
    from: String,
    to: String,
    options: Vec<&'static str>,
}

impl Default for ConvertPanel {
    fn default() -> Self {
        Self {
            open: false,
            input: String::new(),
            filter: String::new(),
            from: String::new(),
            to: String::new(),
            options: vec![
                "m", "km", "cm", "mm", "mi", "ft", "in", "kg", "g", "lb", "oz",
                "c", "f", "k", "l", "ml", "gal",
            ],
        }
    }
}

impl ConvertPanel {
    /// Open the panel.
    pub fn open(&mut self) {
        self.open = true;
    }

    /// Draw the panel UI when open.
    pub fn ui(&mut self, ctx: &egui::Context, _app: &mut LauncherApp) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        let filtered: Vec<&str> = self
            .options
            .iter()
            .copied()
            .filter(|o| self.filter.is_empty() || o.contains(&self.filter))
            .collect();
        if self.from.is_empty() && !filtered.is_empty() {
            self.from = filtered[0].to_string();
        }
        if self.to.is_empty() && !filtered.is_empty() {
            self.to = filtered[0].to_string();
        }
        egui::Window::new("Convert")
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("Value");
                ui.text_edit_singleline(&mut self.input);
                ui.label("Filter");
                ui.text_edit_singleline(&mut self.filter);
                ui.horizontal(|ui| {
                    egui::ComboBox::from_label("From")
                        .selected_text(&self.from)
                        .show_ui(ui, |ui| {
                            for opt in &filtered {
                                ui.selectable_value(&mut self.from, (*opt).to_string(), *opt);
                            }
                        });
                    egui::ComboBox::from_label("To")
                        .selected_text(&self.to)
                        .show_ui(ui, |ui| {
                            for opt in &filtered {
                                ui.selectable_value(&mut self.to, (*opt).to_string(), *opt);
                            }
                        });
                });
            });
        self.open = open;
    }
}

