use crate::actions::Action;
use crate::color::RgbColor;
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
        let mut parts = query.split_whitespace();
        if !parts
            .next()
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("color"))
        {
            return Vec::new();
        }
        let arg = parts.next();
        if parts.next().is_some() {
            return Vec::new();
        }
        if arg.is_some_and(|arg| arg.eq_ignore_ascii_case("pick")) {
            return vec![pick_action()];
        }
        let color = match arg {
            Some(arg) => match RgbColor::parse_hex(arg) {
                Some(color) => color,
                None => return Vec::new(),
            },
            None => RgbColor::new(self.color.r(), self.color.g(), self.color.b()),
        };
        color_actions(color)
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

    fn query_prefixes(&self) -> &[&str] {
        &["color"]
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
            Action {
                label: "color pick".into(),
                desc: "Pick a screen color".into(),
                action: "query:color pick".into(),
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

fn pick_action() -> Action {
    Action {
        label: "Pick Screen Color".into(),
        desc: "Choose a color from the screen".into(),
        action: "color:pick".into(),
        args: None,
    }
}

/// The existing result path for both manually parsed colors and picked pixels.
pub fn color_actions(color: RgbColor) -> Vec<Action> {
    [
        (color.hex(), "Color hex"),
        (color.rgb(), "Color rgb"),
        (color.hsl(), "Color hsl"),
    ]
    .into_iter()
    .map(|(output, description)| Action {
        label: output.clone(),
        desc: description.into(),
        action: format!("clipboard:{output}"),
        args: None,
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_and_typed_color_queries_preserve_existing_three_results() {
        let plugin = ColorPickerPlugin::default();
        for query in ["color", "color #ff0000", " COLOR #F00 "] {
            let actions = plugin.search(query);
            assert_eq!(actions.len(), 3);
            for (action, output, description) in [
                (&actions[0], "#ff0000", "Color hex"),
                (&actions[1], "rgb(255, 0, 0)", "Color rgb"),
                (&actions[2], "hsl(0, 100%, 50%)", "Color hsl"),
            ] {
                assert_eq!(action.label, output);
                assert_eq!(action.desc, description);
                assert_eq!(action.action, format!("clipboard:{output}"));
                assert!(action.args.is_none());
            }
        }
    }

    #[test]
    fn settings_keep_existing_rgba_shape_and_color32_channel_behavior() {
        let mut plugin = ColorPickerPlugin::default();
        assert_eq!(
            plugin.default_settings().unwrap(),
            serde_json::json!({"color": [255, 0, 0, 255]})
        );
        let settings = serde_json::json!({"color": [40, 80, 120, 128]});
        plugin.apply_settings(&settings);
        let expected = Color32::from_rgba_unmultiplied(40, 80, 120, 128);
        assert_eq!(
            plugin.default_settings().unwrap(),
            serde_json::json!({"color": [expected.r(), expected.g(), expected.b(), expected.a()]})
        );
        assert_eq!(
            plugin.search("color")[0].label,
            RgbColor::new(expected.r(), expected.g(), expected.b()).hex()
        );
        plugin.apply_settings(&serde_json::json!({"invalid": true}));
        assert_eq!(
            plugin.search("color")[0].label,
            RgbColor::new(expected.r(), expected.g(), expected.b()).hex()
        );
    }

    #[test]
    fn screen_pixel_and_typed_hex_use_identical_results() {
        let pixel = RgbColor::new(12, 34, 56);
        let picked = color_actions(pixel);
        let typed = ColorPickerPlugin::default().search(&format!("color {}", pixel.hex()));
        for (picked, typed) in picked.iter().zip(&typed) {
            assert_eq!(picked.label, typed.label);
            assert_eq!(picked.action, typed.action);
        }
    }

    #[test]
    fn pick_is_distinct_and_discoverable_without_prefix_collisions() {
        let plugin = ColorPickerPlugin::default();
        assert!(
            plugin
                .commands()
                .iter()
                .any(|action| action.label == "color pick" && action.action == "query:color pick")
        );
        for query in ["color pick", " COLOR PICK "] {
            let actions = plugin.search(query);
            assert_eq!(actions.len(), 1);
            assert_eq!(actions[0].action, "color:pick");
        }
        for query in [
            "colorpick",
            "colorabc",
            "color pick extra",
            "colors pick",
            "color éa",
            "color aéabc",
        ] {
            assert!(plugin.search(query).is_empty(), "query {query:?}");
        }
    }
}
