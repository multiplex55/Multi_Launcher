//! Static curated palette metadata and recipes.
//!
//! The dialog owns browsing and selection state. This module owns the catalog
//! and turns each compact recipe into the existing persisted `ColorScheme`.

use crate::settings::{ColorScheme, ThemeColor};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum PaletteFamily {
    Oled,
    Charcoal,
    Midnight,
    Tinted,
    Light,
}

impl PaletteFamily {
    pub(crate) const ALL: [Self; 5] = [
        Self::Oled,
        Self::Charcoal,
        Self::Midnight,
        Self::Tinted,
        Self::Light,
    ];

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Oled => "OLED / True Black",
            Self::Charcoal => "Charcoal / Graphite",
            Self::Midnight => "Midnight / Slate",
            Self::Tinted => "Tinted / Atmospheric Dark",
            Self::Light => "Light / Accessible",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PaletteAppearance {
    Dark,
    Light,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CuratedPreset {
    pub(crate) id: &'static str,
    pub(crate) label: &'static str,
    pub(crate) family: PaletteFamily,
    pub(crate) appearance: PaletteAppearance,
    recipe: PaletteRecipe,
}

impl CuratedPreset {
    pub(crate) fn scheme(self) -> ColorScheme {
        match self.recipe {
            PaletteRecipe::Legacy(legacy) => legacy.scheme(),
            PaletteRecipe::Generated(recipe) => recipe.scheme(self.appearance),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum PaletteRecipe {
    Legacy(LegacyPalette),
    Generated(GeneratedRecipe),
}

#[derive(Debug, Clone, Copy)]
enum LegacyPalette {
    GraphiteDark,
    GraphiteLight,
    SlateDark,
    SlateLight,
    OceanDark,
    OceanLight,
}

impl LegacyPalette {
    fn scheme(self) -> ColorScheme {
        let mut scheme = match self {
            Self::GraphiteDark | Self::SlateDark | Self::OceanDark => ColorScheme::dark(),
            Self::GraphiteLight | Self::SlateLight | Self::OceanLight => ColorScheme::light(),
        };
        match self {
            Self::GraphiteDark => {
                scheme.window_fill = rgb(0x1B1D21);
                scheme.panel_fill = rgb(0x23252A);
                scheme.text = rgb(0xE7E9EE);
                scheme.widget_active_fill = rgb(0x475265);
                scheme.widget_active_stroke = rgb(0x9FB1D2);
                scheme.selection_bg = rgba(70, 101, 153, 210);
            }
            Self::GraphiteLight => {
                scheme.window_fill = rgb(0xF3F4F6);
                scheme.panel_fill = rgb(0xFCFCFD);
                scheme.text = rgb(0x26292F);
                scheme.widget_active_fill = rgb(0xD3D9E4);
                scheme.widget_active_stroke = rgb(0x5B6982);
                scheme.selection_bg = rgba(169, 187, 219, 220);
            }
            Self::SlateDark => {
                scheme.window_fill = rgb(0x191F2A);
                scheme.panel_fill = rgb(0x202835);
                scheme.text = rgb(0xE1E9F4);
                scheme.hyperlink = rgb(0x82B4ED);
                scheme.widget_active_fill = rgb(0x3E5371);
                scheme.widget_active_stroke = rgb(0x8FB4E0);
                scheme.selection_bg = rgba(59, 105, 158, 210);
            }
            Self::SlateLight => {
                scheme.window_fill = rgb(0xEDF2F8);
                scheme.panel_fill = rgb(0xFAFCFF);
                scheme.text = rgb(0x222F40);
                scheme.hyperlink = rgb(0x2A69AD);
                scheme.widget_active_fill = rgb(0xC5D6EB);
                scheme.widget_active_stroke = rgb(0x4B6F99);
                scheme.selection_bg = rgba(146, 185, 224, 220);
            }
            Self::OceanDark => {
                scheme.window_fill = rgb(0x122026);
                scheme.panel_fill = rgb(0x182A31);
                scheme.text = rgb(0xDEF0EF);
                scheme.hyperlink = rgb(0x59CAC4);
                scheme.widget_active_fill = rgb(0x266869);
                scheme.widget_active_stroke = rgb(0x6FD3C7);
                scheme.selection_bg = rgba(37, 123, 122, 215);
            }
            Self::OceanLight => {
                scheme.window_fill = rgb(0xEBF7F6);
                scheme.panel_fill = rgb(0xFBFFFE);
                scheme.text = rgb(0x1B3737);
                scheme.hyperlink = rgb(0x147B78);
                scheme.widget_active_fill = rgb(0xB6E1DB);
                scheme.widget_active_stroke = rgb(0x2D847E);
                scheme.selection_bg = rgba(130, 207, 195, 215);
            }
        }
        scheme
    }
}

#[derive(Debug, Clone, Copy)]
struct GeneratedRecipe {
    window: u32,
    panel: u32,
    accent: u32,
    link: u32,
}

impl GeneratedRecipe {
    fn scheme(self, appearance: PaletteAppearance) -> ColorScheme {
        let window_fill = rgb(self.window);
        let panel_fill = rgb(self.panel);
        let accent = rgb(self.accent);
        let hyperlink = rgb(self.link);
        let text = match appearance {
            PaletteAppearance::Dark => rgb(0xF0F3F8),
            PaletteAppearance::Light => rgb(0x202633),
        };

        let (
            inactive_fill,
            inactive_stroke,
            hovered_fill,
            hovered_stroke,
            active_fill,
            active_stroke,
            selection_bg,
            selection_stroke,
            warn_accent,
            error_accent,
            success_accent,
        ) = match appearance {
            PaletteAppearance::Dark => (
                mix(panel_fill, window_fill, 0.20),
                mix(panel_fill, text, 0.15),
                mix(panel_fill, accent, 0.20),
                mix(panel_fill, hyperlink, 0.46),
                mix(panel_fill, accent, 0.48),
                mix(accent, text, 0.30),
                with_alpha(mix(panel_fill, accent, 0.52), 224),
                mix(accent, text, 0.48),
                rgb(0xFFC65A),
                rgb(0xFF687B),
                rgb(0x65DFA0),
            ),
            PaletteAppearance::Light => (
                mix(window_fill, panel_fill, 0.52),
                mix(panel_fill, text, 0.20),
                mix(panel_fill, accent, 0.10),
                mix(panel_fill, hyperlink, 0.46),
                mix(panel_fill, accent, 0.26),
                mix(accent, text, 0.32),
                with_alpha(mix(panel_fill, accent, 0.42), 224),
                mix(accent, text, 0.22),
                rgb(0x885000),
                rgb(0xA51F2B),
                rgb(0x176641),
            ),
        };

        ColorScheme {
            window_fill,
            panel_fill,
            text,
            hyperlink,
            widget_inactive_fill: inactive_fill,
            widget_inactive_stroke: inactive_stroke,
            widget_hovered_fill: hovered_fill,
            widget_hovered_stroke: hovered_stroke,
            widget_active_fill: active_fill,
            widget_active_stroke: active_stroke,
            selection_bg,
            selection_stroke,
            warn_accent,
            error_accent,
            success_accent,
        }
    }
}

const fn rgb(value: u32) -> ThemeColor {
    ThemeColor {
        r: (value >> 16) as u8,
        g: (value >> 8) as u8,
        b: value as u8,
        a: 255,
    }
}

const fn rgba(r: u8, g: u8, b: u8, a: u8) -> ThemeColor {
    ThemeColor { r, g, b, a }
}

fn with_alpha(mut color: ThemeColor, a: u8) -> ThemeColor {
    color.a = a;
    color
}

fn mix(from: ThemeColor, toward: ThemeColor, amount: f32) -> ThemeColor {
    fn channel(from: u8, toward: u8, amount: f32) -> u8 {
        (f32::from(from) + (f32::from(toward) - f32::from(from)) * amount)
            .round()
            .clamp(0.0, 255.0) as u8
    }

    ThemeColor {
        r: channel(from.r, toward.r, amount),
        g: channel(from.g, toward.g, amount),
        b: channel(from.b, toward.b, amount),
        a: 255,
    }
}

macro_rules! legacy {
    ($id:literal, $label:literal, $family:ident, $appearance:ident, $recipe:ident) => {
        CuratedPreset {
            id: $id,
            label: $label,
            family: PaletteFamily::$family,
            appearance: PaletteAppearance::$appearance,
            recipe: PaletteRecipe::Legacy(LegacyPalette::$recipe),
        }
    };
}

macro_rules! generated {
    ($id:literal, $label:literal, $family:ident, $appearance:ident,
        $window:literal, $panel:literal, $accent:literal, $link:literal) => {
        CuratedPreset {
            id: $id,
            label: $label,
            family: PaletteFamily::$family,
            appearance: PaletteAppearance::$appearance,
            recipe: PaletteRecipe::Generated(GeneratedRecipe {
                window: $window,
                panel: $panel,
                accent: $accent,
                link: $link,
            }),
        }
    };
}

pub(crate) const CURATED_PRESETS: &[CuratedPreset] = &[
    generated!(
        "obsidian-lime",
        "Obsidian Lime",
        Oled,
        Dark,
        0x000000,
        0x080C08,
        0xB7F34A,
        0xB7F34A
    ),
    generated!(
        "obsidian-ember",
        "Obsidian Ember",
        Oled,
        Dark,
        0x000000,
        0x100706,
        0xFF806D,
        0xFF998A
    ),
    generated!(
        "obsidian-amber",
        "Obsidian Amber",
        Oled,
        Dark,
        0x000000,
        0x100B03,
        0xFFC14D,
        0xFFD67C
    ),
    generated!(
        "obsidian-gold",
        "Obsidian Gold",
        Oled,
        Dark,
        0x000000,
        0x100D04,
        0xEFC64A,
        0xFFDF75
    ),
    generated!(
        "obsidian-cyan",
        "Obsidian Cyan",
        Oled,
        Dark,
        0x000000,
        0x061011,
        0x42E8F5,
        0x91F7FF
    ),
    generated!(
        "obsidian-teal",
        "Obsidian Teal",
        Oled,
        Dark,
        0x000000,
        0x06100D,
        0x41D8B4,
        0x91F1D8
    ),
    generated!(
        "obsidian-azure",
        "Obsidian Azure",
        Oled,
        Dark,
        0x000000,
        0x070C12,
        0x66B8FF,
        0xA5D5FF
    ),
    generated!(
        "obsidian-ruby",
        "Obsidian Ruby",
        Oled,
        Dark,
        0x000000,
        0x100609,
        0xFF6B82,
        0xFFA2AE
    ),
    generated!(
        "obsidian-rose",
        "Obsidian Rose",
        Oled,
        Dark,
        0x000000,
        0x10070C,
        0xFF85BB,
        0xFFC0DC
    ),
    generated!(
        "obsidian-violet",
        "Obsidian Violet",
        Oled,
        Dark,
        0x000000,
        0x0C0712,
        0xC58AFF,
        0xDAB8FF
    ),
    generated!(
        "obsidian-silver",
        "Obsidian Silver",
        Oled,
        Dark,
        0x000000,
        0x0A0D10,
        0xD2DBE8,
        0xEAF0F7
    ),
    generated!(
        "obsidian-ice",
        "Obsidian Ice",
        Oled,
        Dark,
        0x000000,
        0x061014,
        0x91E6FF,
        0xC0F2FF
    ),
    legacy!(
        "graphite-dark",
        "Graphite Dark",
        Charcoal,
        Dark,
        GraphiteDark
    ),
    generated!(
        "charcoal-mint",
        "Charcoal Mint",
        Charcoal,
        Dark,
        0x171A18,
        0x202723,
        0x54D99A,
        0x8DE9B9
    ),
    generated!(
        "carbon-orange",
        "Carbon Orange",
        Charcoal,
        Dark,
        0x1C1917,
        0x28211D,
        0xF27B38,
        0xFFAD72
    ),
    generated!(
        "forge-yellow",
        "Forge Yellow",
        Charcoal,
        Dark,
        0x1B1A16,
        0x27251B,
        0xE7B83D,
        0xFFD77A
    ),
    generated!(
        "night-steel",
        "Night Steel",
        Charcoal,
        Dark,
        0x171A1E,
        0x222830,
        0x85A9C5,
        0xB7D3E7
    ),
    generated!(
        "smokestack-gold",
        "Smokestack Gold",
        Charcoal,
        Dark,
        0x1A1917,
        0x26231E,
        0xD9A842,
        0xFFD98A
    ),
    generated!(
        "gunmetal-cyan",
        "Gunmetal Cyan",
        Charcoal,
        Dark,
        0x171B1D,
        0x20292B,
        0x46C6D6,
        0x83E4EF
    ),
    generated!(
        "ink-cobalt",
        "Ink Cobalt",
        Charcoal,
        Dark,
        0x151821,
        0x202735,
        0x557FE2,
        0x9AB4FF
    ),
    generated!(
        "ash-burgundy",
        "Ash Burgundy",
        Charcoal,
        Dark,
        0x1D171A,
        0x2A2025,
        0xC86078,
        0xF091A2
    ),
    generated!(
        "onyx-orchid",
        "Onyx Orchid",
        Charcoal,
        Dark,
        0x19171D,
        0x25212D,
        0xB07BE3,
        0xD4AEFF
    ),
    generated!(
        "coal-blush",
        "Coal Blush",
        Charcoal,
        Dark,
        0x1D181A,
        0x292226,
        0xD98193,
        0xF1B2BF
    ),
    generated!(
        "carbon-frost",
        "Carbon Frost",
        Charcoal,
        Dark,
        0x171B1F,
        0x222931,
        0x9CB8C9,
        0xD1E4F0
    ),
    legacy!("slate-dark", "Slate Dark", Midnight, Dark, SlateDark),
    generated!(
        "navy-neon",
        "Navy Neon",
        Midnight,
        Dark,
        0x0D1524,
        0x141F32,
        0x72A7FF,
        0xA9C8FF
    ),
    generated!(
        "starless-green",
        "Starless Green",
        Midnight,
        Dark,
        0x0C1714,
        0x12241E,
        0x50D99B,
        0x93F0C2
    ),
    generated!(
        "abyss-orange",
        "Abyss Orange",
        Midnight,
        Dark,
        0x101620,
        0x1A2530,
        0xFF8A52,
        0xFFB28A
    ),
    generated!(
        "midnight-lantern",
        "Midnight Lantern",
        Midnight,
        Dark,
        0x131722,
        0x202332,
        0xF1C46D,
        0xFFDF9B
    ),
    generated!(
        "midnight-turquoise",
        "Midnight Turquoise",
        Midnight,
        Dark,
        0x0D1C22,
        0x142A32,
        0x50D6D2,
        0x96F4EE
    ),
    legacy!("ocean-dark", "Ocean Dark", Midnight, Dark, OceanDark),
    generated!(
        "blue-hour",
        "Blue Hour",
        Midnight,
        Dark,
        0x131B2A,
        0x1D293A,
        0x91A8FF,
        0xBECAFF
    ),
    generated!(
        "dusk-raspberry",
        "Dusk Raspberry",
        Midnight,
        Dark,
        0x181521,
        0x272034,
        0xE982B1,
        0xFFB2D0
    ),
    generated!(
        "storm-plum",
        "Storm Plum",
        Midnight,
        Dark,
        0x181622,
        0x252236,
        0xBA9AF2,
        0xD9C4FF
    ),
    generated!(
        "twilight-ice",
        "Twilight Ice",
        Midnight,
        Dark,
        0x111A25,
        0x1A2B37,
        0x89D9EC,
        0xBDEFFF
    ),
    generated!(
        "steel-lantern",
        "Steel Lantern",
        Midnight,
        Dark,
        0x151B21,
        0x232B32,
        0xC0A66A,
        0xE8D595
    ),
    generated!(
        "forest-terminal",
        "Forest Terminal",
        Tinted,
        Dark,
        0x0A1712,
        0x10251D,
        0x7BE07D,
        0xB0F5AA
    ),
    generated!(
        "moss-ember",
        "Moss Ember",
        Tinted,
        Dark,
        0x171812,
        0x252317,
        0xE8A64D,
        0xFFD08A
    ),
    generated!(
        "espresso-gold",
        "Espresso Gold",
        Tinted,
        Dark,
        0x1C1410,
        0x2A1C15,
        0xD79B59,
        0xF0C08A
    ),
    generated!(
        "rust-and-ember",
        "Rust and Ember",
        Tinted,
        Dark,
        0x1B1210,
        0x2A1A17,
        0xFF704F,
        0xFFAA83
    ),
    generated!(
        "midnight-copper",
        "Midnight Copper",
        Tinted,
        Dark,
        0x171412,
        0x251B17,
        0xD98A5F,
        0xF5B48B
    ),
    generated!(
        "merlot-rose",
        "Merlot Rose",
        Tinted,
        Dark,
        0x1A1119,
        0x2A1A29,
        0xE875A9,
        0xFFB4D0
    ),
    generated!(
        "aubergine-frost",
        "Aubergine Frost",
        Tinted,
        Dark,
        0x17121E,
        0x251B33,
        0xAFA0F5,
        0xD6CDFF
    ),
    generated!(
        "arctic-graphite",
        "Arctic Graphite",
        Tinted,
        Dark,
        0x111820,
        0x1C2832,
        0x80D8E8,
        0xB3F1FA
    ),
    generated!(
        "vaporwave-dark",
        "Vaporwave Dark",
        Tinted,
        Dark,
        0x161226,
        0x241A38,
        0xE87BE8,
        0xFFB7FF
    ),
    generated!(
        "synthwave-noir",
        "Synthwave Noir",
        Tinted,
        Dark,
        0x160F20,
        0x251529,
        0xFF5CA8,
        0xFF9BC8
    ),
    generated!(
        "jungle-neon",
        "Jungle Neon",
        Tinted,
        Dark,
        0x0B1716,
        0x112624,
        0x4BE0BB,
        0x8BFFE6
    ),
    generated!(
        "infrared-night",
        "Infrared Night",
        Tinted,
        Dark,
        0x1D1014,
        0x2C171D,
        0xFF5D64,
        0xFF9B8C
    ),
    legacy!(
        "graphite-light",
        "Graphite Light",
        Light,
        Light,
        GraphiteLight
    ),
    legacy!("slate-light", "Slate Light", Light, Light, SlateLight),
    legacy!("ocean-light", "Ocean Light", Light, Light, OceanLight),
    generated!(
        "parchment-amber",
        "Parchment Amber",
        Light,
        Light,
        0xFBF5E9,
        0xFFF9EF,
        0xA45F00,
        0x8C4C00
    ),
    generated!(
        "frost-lime",
        "Frost Lime",
        Light,
        Light,
        0xF2F8EA,
        0xFCFFF8,
        0x4C742D,
        0x3F651F
    ),
    generated!(
        "cloud-violet",
        "Cloud Violet",
        Light,
        Light,
        0xF4F0FA,
        0xFEFCFF,
        0x7156A9,
        0x5D478E
    ),
];

pub(crate) fn curated_presets() -> &'static [CuratedPreset] {
    CURATED_PRESETS
}

pub(crate) fn find_curated_preset(id: &str) -> Option<&'static CuratedPreset> {
    CURATED_PRESETS.iter().find(|preset| preset.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    const EXPECTED_IDS: [&str; 54] = [
        "obsidian-lime",
        "obsidian-ember",
        "obsidian-amber",
        "obsidian-gold",
        "obsidian-cyan",
        "obsidian-teal",
        "obsidian-azure",
        "obsidian-ruby",
        "obsidian-rose",
        "obsidian-violet",
        "obsidian-silver",
        "obsidian-ice",
        "graphite-dark",
        "charcoal-mint",
        "carbon-orange",
        "forge-yellow",
        "night-steel",
        "smokestack-gold",
        "gunmetal-cyan",
        "ink-cobalt",
        "ash-burgundy",
        "onyx-orchid",
        "coal-blush",
        "carbon-frost",
        "slate-dark",
        "navy-neon",
        "starless-green",
        "abyss-orange",
        "midnight-lantern",
        "midnight-turquoise",
        "ocean-dark",
        "blue-hour",
        "dusk-raspberry",
        "storm-plum",
        "twilight-ice",
        "steel-lantern",
        "forest-terminal",
        "moss-ember",
        "espresso-gold",
        "rust-and-ember",
        "midnight-copper",
        "merlot-rose",
        "aubergine-frost",
        "arctic-graphite",
        "vaporwave-dark",
        "synthwave-noir",
        "jungle-neon",
        "infrared-night",
        "graphite-light",
        "slate-light",
        "ocean-light",
        "parchment-amber",
        "frost-lime",
        "cloud-violet",
    ];

    const EXPECTED_LABELS: [&str; 54] = [
        "Obsidian Lime",
        "Obsidian Ember",
        "Obsidian Amber",
        "Obsidian Gold",
        "Obsidian Cyan",
        "Obsidian Teal",
        "Obsidian Azure",
        "Obsidian Ruby",
        "Obsidian Rose",
        "Obsidian Violet",
        "Obsidian Silver",
        "Obsidian Ice",
        "Graphite Dark",
        "Charcoal Mint",
        "Carbon Orange",
        "Forge Yellow",
        "Night Steel",
        "Smokestack Gold",
        "Gunmetal Cyan",
        "Ink Cobalt",
        "Ash Burgundy",
        "Onyx Orchid",
        "Coal Blush",
        "Carbon Frost",
        "Slate Dark",
        "Navy Neon",
        "Starless Green",
        "Abyss Orange",
        "Midnight Lantern",
        "Midnight Turquoise",
        "Ocean Dark",
        "Blue Hour",
        "Dusk Raspberry",
        "Storm Plum",
        "Twilight Ice",
        "Steel Lantern",
        "Forest Terminal",
        "Moss Ember",
        "Espresso Gold",
        "Rust and Ember",
        "Midnight Copper",
        "Merlot Rose",
        "Aubergine Frost",
        "Arctic Graphite",
        "Vaporwave Dark",
        "Synthwave Noir",
        "Jungle Neon",
        "Infrared Night",
        "Graphite Light",
        "Slate Light",
        "Ocean Light",
        "Parchment Amber",
        "Frost Lime",
        "Cloud Violet",
    ];

    #[test]
    fn catalog_has_exact_membership_families_and_stable_unique_names() {
        assert_eq!(CURATED_PRESETS.len(), 54);
        let ids: Vec<_> = CURATED_PRESETS.iter().map(|preset| preset.id).collect();
        let labels: Vec<_> = CURATED_PRESETS.iter().map(|preset| preset.label).collect();
        assert_eq!(ids, EXPECTED_IDS);
        assert_eq!(labels, EXPECTED_LABELS);
        assert_eq!(ids.iter().copied().collect::<HashSet<_>>().len(), 54);
        assert_eq!(labels.iter().copied().collect::<HashSet<_>>().len(), 54);
        assert_eq!(
            CURATED_PRESETS
                .iter()
                .filter(|preset| preset.appearance == PaletteAppearance::Dark)
                .count(),
            48
        );
        assert_eq!(
            CURATED_PRESETS
                .iter()
                .filter(|preset| preset.appearance == PaletteAppearance::Light)
                .count(),
            6
        );
        assert_eq!(
            CURATED_PRESETS
                .iter()
                .filter(|preset| preset.family == PaletteFamily::Oled)
                .count(),
            12
        );
        for preset in CURATED_PRESETS {
            assert!(
                preset
                    .id
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            );
            assert!(!preset.label.trim().is_empty());
        }
    }

    #[test]
    fn generated_schemes_are_distinct_opaque_and_readable() {
        let mut encoded = HashSet::new();
        for preset in CURATED_PRESETS {
            let scheme = preset.scheme();
            let json = serde_json::to_string(&scheme).expect("palette scheme serializes");
            let round_trip: ColorScheme =
                serde_json::from_str(&json).expect("palette scheme deserializes");
            assert_eq!(round_trip, scheme);
            assert!(
                encoded.insert(json),
                "{} duplicates another palette",
                preset.id
            );

            if preset.family == PaletteFamily::Oled {
                assert_eq!(
                    (
                        scheme.window_fill.r,
                        scheme.window_fill.g,
                        scheme.window_fill.b
                    ),
                    (0, 0, 0),
                    "{} uses true black",
                    preset.id
                );
                assert_eq!(scheme.window_fill.a, 255);
            }

            for background in [scheme.window_fill, scheme.panel_fill] {
                assert!(
                    contrast_ratio(scheme.text, background) >= 4.5,
                    "text contrast in {}",
                    preset.id
                );
                assert!(
                    contrast_ratio(scheme.hyperlink, background) >= 4.5,
                    "link contrast in {}",
                    preset.id
                );
            }
            let selected = composite(scheme.selection_bg, scheme.panel_fill);
            assert!(
                contrast_ratio(scheme.text, selected) >= 3.0,
                "selection text remains readable in {}",
                preset.id
            );
        }
        assert_eq!(encoded.len(), 54);
    }

    #[test]
    fn six_legacy_schemes_match_their_pre_catalog_values() {
        for id in [
            "graphite-dark",
            "graphite-light",
            "slate-dark",
            "slate-light",
            "ocean-dark",
            "ocean-light",
        ] {
            let preset = find_curated_preset(id).expect("legacy id remains present");
            assert_eq!(preset.scheme(), legacy_baseline_scheme(id).unwrap(), "{id}");
        }
    }

    fn legacy_baseline_scheme(id: &str) -> Option<ColorScheme> {
        let mut scheme = match id {
            "graphite-dark" | "slate-dark" | "ocean-dark" => ColorScheme::dark(),
            "graphite-light" | "slate-light" | "ocean-light" => ColorScheme::light(),
            _ => return None,
        };
        match id {
            "graphite-dark" => {
                scheme.window_fill = rgb(0x1B1D21);
                scheme.panel_fill = rgb(0x23252A);
                scheme.text = rgb(0xE7E9EE);
                scheme.widget_active_fill = rgb(0x475265);
                scheme.widget_active_stroke = rgb(0x9FB1D2);
                scheme.selection_bg = rgba(70, 101, 153, 210);
            }
            "graphite-light" => {
                scheme.window_fill = rgb(0xF3F4F6);
                scheme.panel_fill = rgb(0xFCFCFD);
                scheme.text = rgb(0x26292F);
                scheme.widget_active_fill = rgb(0xD3D9E4);
                scheme.widget_active_stroke = rgb(0x5B6982);
                scheme.selection_bg = rgba(169, 187, 219, 220);
            }
            "slate-dark" => {
                scheme.window_fill = rgb(0x191F2A);
                scheme.panel_fill = rgb(0x202835);
                scheme.text = rgb(0xE1E9F4);
                scheme.hyperlink = rgb(0x82B4ED);
                scheme.widget_active_fill = rgb(0x3E5371);
                scheme.widget_active_stroke = rgb(0x8FB4E0);
                scheme.selection_bg = rgba(59, 105, 158, 210);
            }
            "slate-light" => {
                scheme.window_fill = rgb(0xEDF2F8);
                scheme.panel_fill = rgb(0xFAFCFF);
                scheme.text = rgb(0x222F40);
                scheme.hyperlink = rgb(0x2A69AD);
                scheme.widget_active_fill = rgb(0xC5D6EB);
                scheme.widget_active_stroke = rgb(0x4B6F99);
                scheme.selection_bg = rgba(146, 185, 224, 220);
            }
            "ocean-dark" => {
                scheme.window_fill = rgb(0x122026);
                scheme.panel_fill = rgb(0x182A31);
                scheme.text = rgb(0xDEF0EF);
                scheme.hyperlink = rgb(0x59CAC4);
                scheme.widget_active_fill = rgb(0x266869);
                scheme.widget_active_stroke = rgb(0x6FD3C7);
                scheme.selection_bg = rgba(37, 123, 122, 215);
            }
            "ocean-light" => {
                scheme.window_fill = rgb(0xEBF7F6);
                scheme.panel_fill = rgb(0xFBFFFE);
                scheme.text = rgb(0x1B3737);
                scheme.hyperlink = rgb(0x147B78);
                scheme.widget_active_fill = rgb(0xB6E1DB);
                scheme.widget_active_stroke = rgb(0x2D847E);
                scheme.selection_bg = rgba(130, 207, 195, 215);
            }
            _ => return None,
        }
        Some(scheme)
    }

    fn linear_channel(channel: u8) -> f64 {
        let channel = f64::from(channel) / 255.0;
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    }

    fn luminance(color: ThemeColor) -> f64 {
        0.2126 * linear_channel(color.r)
            + 0.7152 * linear_channel(color.g)
            + 0.0722 * linear_channel(color.b)
    }

    fn contrast_ratio(first: ThemeColor, second: ThemeColor) -> f64 {
        let first = luminance(first);
        let second = luminance(second);
        (first.max(second) + 0.05) / (first.min(second) + 0.05)
    }

    fn composite(foreground: ThemeColor, background: ThemeColor) -> ThemeColor {
        let alpha = f64::from(foreground.a) / 255.0;
        let channel = |front: u8, back: u8| {
            (f64::from(front) * alpha + f64::from(back) * (1.0 - alpha)).round() as u8
        };
        ThemeColor {
            r: channel(foreground.r, background.r),
            g: channel(foreground.g, background.g),
            b: channel(foreground.b, background.b),
            a: 255,
        }
    }
}
