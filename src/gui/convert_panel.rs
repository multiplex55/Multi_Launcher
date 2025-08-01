use crate::gui::LauncherApp;
use eframe::egui;

struct Category {
    name: &'static str,
    units: &'static [&'static str],
}

const CATEGORIES: &[Category] = &[
    Category {
        name: "Distance",
        units: &["m", "km", "cm", "mm", "mi", "ft", "in", "yd", "nm"],
    },
    Category {
        name: "Mass",
        units: &["kg", "g", "lb", "oz"],
    },
    Category {
        name: "Temperature",
        units: &["c", "f", "k"],
    },
    Category {
        name: "Volume",
        units: &["l", "ml", "gal"],
    },
    Category {
        name: "Base",
        units: &["dec", "hex", "bin", "oct"],
    },
];

/// Simple conversion panel with an input box and two combo boxes.
pub struct ConvertPanel {
    pub open: bool,
    input: String,
    filter: String,
    category: String,
    from: String,
    to: String,
    focus_input: bool,
}

impl Default for ConvertPanel {
    fn default() -> Self {
        Self {
            open: false,
            input: String::new(),
            filter: String::new(),
            category: CATEGORIES[0].name.to_string(),
            from: String::new(),
            to: String::new(),
            focus_input: false,
        }
    }
}

impl ConvertPanel {
    /// Open the panel.
    pub fn open(&mut self) {
        self.open = true;
        self.focus_input = true;
    }

    /// Draw the panel UI when open.
    pub fn ui(&mut self, ctx: &egui::Context, _app: &mut LauncherApp) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        let units = CATEGORIES
            .iter()
            .find(|c| c.name == self.category)
            .map(|c| c.units)
            .unwrap_or_default();
        let filtered: Vec<&str> = units
            .iter()
            .copied()
            .filter(|u| self.filter.is_empty() || u.contains(&self.filter))
            .collect();
        if (self.from.is_empty() || !units.contains(&self.from.as_str())) && !filtered.is_empty() {
            self.from = filtered[0].to_string();
        }
        if (self.to.is_empty() || !units.contains(&self.to.as_str())) && !filtered.is_empty() {
            self.to = filtered[0].to_string();
        }
        egui::Window::new("Convert")
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("Value");
                let val_edit = ui.text_edit_singleline(&mut self.input);
                if self.focus_input {
                    val_edit.request_focus();
                    self.focus_input = false;
                }
                ui.label("Type");
                let mut cat_changed = false;
                egui::ComboBox::from_id_source("convert_category")
                    .selected_text(&self.category)
                    .show_ui(ui, |ui| {
                        for cat in CATEGORIES {
                            if ui
                                .selectable_value(
                                    &mut self.category,
                                    cat.name.to_string(),
                                    cat.name,
                                )
                                .changed()
                            {
                                cat_changed = true;
                            }
                        }
                    });
                if cat_changed {
                    self.from.clear();
                    self.to.clear();
                }
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
