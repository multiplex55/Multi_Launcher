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
    result: String,
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
            result: String::new(),
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
        self.result.clear();
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
                self.compute_result();
                ui.label("Result");
                ui.add_enabled(false, egui::TextEdit::singleline(&mut self.result));
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
        self.compute_result();
        self.open = open;
    }
}

fn distance_factor(unit: &str) -> Option<f64> {
    Some(match unit {
        "m" => 1.0,
        "km" => 1000.0,
        "cm" => 0.01,
        "mm" => 0.001,
        "mi" => 1609.344,
        "ft" => 0.3048,
        "in" => 0.0254,
        "yd" => 0.9144,
        "nm" => 1852.0,
        _ => return None,
    })
}

fn mass_factor(unit: &str) -> Option<f64> {
    Some(match unit {
        "kg" => 1000.0,
        "g" => 1.0,
        "lb" => 453.59237,
        "oz" => 28.349523125,
        _ => return None,
    })
}

fn volume_factor(unit: &str) -> Option<f64> {
    Some(match unit {
        "l" => 1.0,
        "ml" => 0.001,
        "gal" => 3.785411784,
        _ => return None,
    })
}

fn to_celsius(val: f64, unit: &str) -> Option<f64> {
    Some(match unit {
        "c" => val,
        "f" => (val - 32.0) * 5.0 / 9.0,
        "k" => val - 273.15,
        _ => return None,
    })
}

fn from_celsius(val: f64, unit: &str) -> Option<f64> {
    Some(match unit {
        "c" => val,
        "f" => val * 9.0 / 5.0 + 32.0,
        "k" => val + 273.15,
        _ => return None,
    })
}

fn base_radix(unit: &str) -> Option<u32> {
    match unit {
        "dec" => Some(10),
        "hex" => Some(16),
        "bin" => Some(2),
        "oct" => Some(8),
        _ => None,
    }
}

fn convert_base(input: &str, from: &str, to: &str) -> Option<String> {
    let from_radix = base_radix(from)?;
    let to_radix = base_radix(to)?;
    let trimmed = input.trim();
    let (neg, digits) = if let Some(rest) = trimmed.strip_prefix('-') {
        (true, rest)
    } else {
        (false, trimmed)
    };
    let value = i64::from_str_radix(digits, from_radix).ok()?;
    let value = if neg { -value } else { value };
    let res = match to_radix {
        10 => value.to_string(),
        16 => format!("{:x}", value),
        2 => format!("{:b}", value),
        8 => format!("{:o}", value),
        _ => return None,
    };
    Some(res)
}

impl ConvertPanel {
    fn compute_result(&mut self) {
        self.result.clear();
        if self.input.trim().is_empty() {
            return;
        }
        match self.category.as_str() {
            "Distance" => {
                if let Ok(v) = self.input.trim().parse::<f64>() {
                    if let (Some(ff), Some(tf)) = (distance_factor(&self.from), distance_factor(&self.to)) {
                        let res = v * ff / tf;
                        self.result = res.to_string();
                    }
                }
            }
            "Mass" => {
                if let Ok(v) = self.input.trim().parse::<f64>() {
                    if let (Some(ff), Some(tf)) = (mass_factor(&self.from), mass_factor(&self.to)) {
                        let res = v * ff / tf;
                        self.result = res.to_string();
                    }
                }
            }
            "Volume" => {
                if let Ok(v) = self.input.trim().parse::<f64>() {
                    if let (Some(ff), Some(tf)) = (volume_factor(&self.from), volume_factor(&self.to)) {
                        let res = v * ff / tf;
                        self.result = res.to_string();
                    }
                }
            }
            "Temperature" => {
                if let Ok(v) = self.input.trim().parse::<f64>() {
                    if let Some(c) = to_celsius(v, &self.from) {
                        if let Some(res) = from_celsius(c, &self.to) {
                            self.result = res.to_string();
                        }
                    }
                }
            }
            "Base" => {
                if let Some(res) = convert_base(&self.input, &self.from, &self.to) {
                    self.result = res;
                }
            }
            _ => {}
        }
    }
}
