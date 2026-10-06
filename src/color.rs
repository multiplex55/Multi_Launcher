/// An opaque RGB color shared by typed input and screen pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RgbColor {
    channels: [u8; 3],
}

impl RgbColor {
    pub const fn new(red: u8, green: u8, blue: u8) -> Self {
        Self {
            channels: [red, green, blue],
        }
    }

    pub const fn channels(self) -> [u8; 3] {
        self.channels
    }

    pub fn parse_hex(input: &str) -> Option<Self> {
        let text = input.trim().trim_start_matches('#');
        if !text.is_ascii() {
            return None;
        }
        let channels = match text.len() {
            6 => [
                u8::from_str_radix(&text[0..2], 16).ok()?,
                u8::from_str_radix(&text[2..4], 16).ok()?,
                u8::from_str_radix(&text[4..6], 16).ok()?,
            ],
            3 => [
                u8::from_str_radix(&text[0..1], 16).ok()? * 17,
                u8::from_str_radix(&text[1..2], 16).ok()? * 17,
                u8::from_str_radix(&text[2..3], 16).ok()? * 17,
            ],
            _ => return None,
        };
        Some(Self { channels })
    }

    pub fn hex(self) -> String {
        let [red, green, blue] = self.channels;
        format!("#{red:02x}{green:02x}{blue:02x}")
    }

    pub fn rgb(self) -> String {
        let [red, green, blue] = self.channels;
        format!("rgb({red}, {green}, {blue})")
    }

    pub fn hsl(self) -> String {
        let [red, green, blue] = self.channels;
        let (hue, saturation, lightness) = rgb_to_hsl(red, green, blue);
        format!("hsl({hue:.0}, {saturation:.0}%, {lightness:.0}%)")
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_accepts_existing_short_long_case_and_optional_hash_syntax() {
        for input in ["#F80", "f80", "#FF8800", "ff8800", "  #ff8800  "] {
            let color = RgbColor::parse_hex(input).unwrap();
            assert_eq!(color.channels(), [255, 136, 0]);
            assert_eq!(color.hex(), "#ff8800");
            assert_eq!(color.rgb(), "rgb(255, 136, 0)");
        }
    }

    #[test]
    fn representative_edges_preserve_existing_hsl_rounding() {
        for (channels, expected) in [
            ([0, 0, 0], "hsl(0, 0%, 0%)"),
            ([255, 255, 255], "hsl(0, 0%, 100%)"),
            ([128, 128, 128], "hsl(0, 0%, 50%)"),
            ([255, 0, 0], "hsl(0, 100%, 50%)"),
            ([0, 255, 0], "hsl(120, 100%, 50%)"),
            ([0, 0, 255], "hsl(240, 100%, 50%)"),
            ([255, 0, 255], "hsl(300, 100%, 50%)"),
            ([255, 255, 0], "hsl(60, 100%, 50%)"),
            ([0, 255, 255], "hsl(180, 100%, 50%)"),
        ] {
            let color = RgbColor::new(channels[0], channels[1], channels[2]);
            assert_eq!(color.hsl(), expected);
            assert_eq!(RgbColor::parse_hex(&color.hex()), Some(color));
        }
    }

    #[test]
    fn invalid_and_multibyte_hex_is_rejected_without_slicing_panics() {
        for input in [
            "",
            "12",
            "1234",
            "12345g",
            "éa",
            "aéabc",
            "💙ab",
            "#ffffff00",
        ] {
            assert!(RgbColor::parse_hex(input).is_none(), "input {input:?}");
        }
    }
}
