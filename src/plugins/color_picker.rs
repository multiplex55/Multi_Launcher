use crate::actions::Action;
use crate::plugin::Plugin;
use eframe::egui::{self, Color32};
use serde::{Deserialize, Serialize};

pub struct ColorPickerPlugin {
    color: Color32,
}

impl Default for ColorPickerPlugin {
    fn default() -> Self {
        Self {
            color: Color32::from_rgb(0xff, 0x00, 0x00),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct ColorPickerSettings {
    color: [u8; 4],
}

impl Plugin for ColorPickerPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        const PREFIX: &str = "color";
        let trimmed = query.trim();
        let Some(rest) = crate::common::strip_prefix_ci(trimmed, PREFIX) else {
            return Vec::new();
        };
        let arg = rest.trim();

        let mut color = self.color;
        if !arg.is_empty() {
            if let Some(c) = parse_hex(arg) {
                color = c;
            } else {
                return Vec::new();
            }
        }

        let hex = format!("#{:02x}{:02x}{:02x}", color.r(), color.g(), color.b());
        let rgb = format!("rgb({}, {}, {})", color.r(), color.g(), color.b());
        let (h, s, l) = rgb_to_hsl(color.r(), color.g(), color.b());
        let hsl = format!("hsl({h:.0}, {s:.0}%, {l:.0}%)");

        vec![
            Action {
                label: hex.clone(),
                desc: "Color hex".into(),
                action: format!("clipboard:{hex}"),
                args: None,
            },
            Action {
                label: rgb.clone(),
                desc: "Color rgb".into(),
                action: format!("clipboard:{rgb}"),
                args: None,
            },
            Action {
                label: hsl.clone(),
                desc: "Color hsl".into(),
                action: format!("clipboard:{hsl}"),
                args: None,
            },
        ]
    }

    fn name(&self) -> &str {
        "color_picker"
    }

    fn description(&self) -> &str {
        "Color picker and converter (prefix: `color`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "color".into(),
                desc: "Color picker".into(),
                action: "query:color ".into(),
                args: None,
            },
            Action {
                label: "color #ff0000".into(),
                desc: "Color picker".into(),
                action: "query:color #ff0000".into(),
                args: None,
            },
        ]
    }

    fn default_settings(&self) -> Option<serde_json::Value> {
        serde_json::to_value(ColorPickerSettings {
            color: [
                self.color.r(),
                self.color.g(),
                self.color.b(),
                self.color.a(),
            ],
        })
        .ok()
    }

    fn apply_settings(&mut self, value: &serde_json::Value) {
        if let Ok(cfg) = serde_json::from_value::<ColorPickerSettings>(value.clone()) {
            self.color = Color32::from_rgba_unmultiplied(
                cfg.color[0],
                cfg.color[1],
                cfg.color[2],
                cfg.color[3],
            );
        }
    }

    fn settings_ui(&mut self, ui: &mut egui::Ui, value: &mut serde_json::Value) {
        let mut cfg: ColorPickerSettings =
            serde_json::from_value(value.clone()).unwrap_or(ColorPickerSettings {
                color: [
                    self.color.r(),
                    self.color.g(),
                    self.color.b(),
                    self.color.a(),
                ],
            });
        let mut col =
            Color32::from_rgba_unmultiplied(cfg.color[0], cfg.color[1], cfg.color[2], cfg.color[3]);
        if ui.color_edit_button_srgba(&mut col).changed() {
            cfg.color = [col.r(), col.g(), col.b(), col.a()];
            self.color = col;
        }
        if let Ok(v) = serde_json::to_value(&cfg) {
            *value = v;
        }
    }
}

fn parse_hex(input: &str) -> Option<Color32> {
    let s = input.trim().trim_start_matches('#');
    let bytes = match s.len() {
        6 => (
            u8::from_str_radix(&s[0..2], 16).ok()?,
            u8::from_str_radix(&s[2..4], 16).ok()?,
            u8::from_str_radix(&s[4..6], 16).ok()?,
        ),
        3 => (
            u8::from_str_radix(&s[0..1], 16).ok()? * 17,
            u8::from_str_radix(&s[1..2], 16).ok()? * 17,
            u8::from_str_radix(&s[2..3], 16).ok()? * 17,
        ),
        _ => return None,
    };
    Some(Color32::from_rgb(bytes.0, bytes.1, bytes.2))
}

fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let r = r as f32 / 255.0;
    let g = g as f32 / 255.0;
    let b = b as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if (max - min).abs() < f32::EPSILON {
        (0.0, 0.0, l * 100.0)
    } else {
        let d = max - min;
        let s = if l > 0.5 {
            d / (2.0 - max - min)
        } else {
            d / (max + min)
        };
        let h = if (max - r).abs() < f32::EPSILON {
            (g - b) / d + if g < b { 6.0 } else { 0.0 }
        } else if (max - g).abs() < f32::EPSILON {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        };
        (h / 6.0 * 360.0, s * 100.0, l * 100.0)
    }
}
