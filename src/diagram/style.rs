//! Bounded, source-authoritative Mermaid colors.
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MermaidColor(pub [u8; 3]);

impl MermaidColor {
    pub fn parse(value: &str) -> Result<Self, &'static str> {
        let digits = value.strip_prefix('#').ok_or("color must begin with #")?;
        let expanded = match digits.len() {
            3 if digits.is_ascii() => digits.chars().flat_map(|c| [c, c]).collect::<String>(),
            6 if digits.is_ascii() => digits.to_owned(),
            _ => return Err("color must use #RGB or #RRGGBB"),
        };
        let value = u32::from_str_radix(&expanded, 16).map_err(|_| "invalid hexadecimal color")?;
        Ok(Self([(value >> 16) as u8, (value >> 8) as u8, value as u8]))
    }
    pub fn hex(self) -> String { format!("#{:02x}{:02x}{:02x}", self.0[0], self.0[1], self.0[2]) }
}
impl Serialize for MermaidColor {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> { serializer.serialize_str(&self.hex()) }
}
impl<'de> Deserialize<'de> for MermaidColor {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(de::Error::custom)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MermaidTheme { Default, Dark, Forest, Neutral }

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct MermaidThemeVariables {
    #[serde(skip_serializing_if = "Option::is_none")] pub primary_color: Option<MermaidColor>,
    #[serde(skip_serializing_if = "Option::is_none")] pub primary_border_color: Option<MermaidColor>,
    #[serde(skip_serializing_if = "Option::is_none")] pub primary_text_color: Option<MermaidColor>,
    #[serde(skip_serializing_if = "Option::is_none")] pub line_color: Option<MermaidColor>,
    #[serde(skip_serializing_if = "Option::is_none")] pub background: Option<MermaidColor>,
    #[serde(skip_serializing_if = "Option::is_none")] pub secondary_color: Option<MermaidColor>,
    #[serde(skip_serializing_if = "Option::is_none")] pub tertiary_color: Option<MermaidColor>,
    #[serde(skip_serializing_if = "Option::is_none")] pub mindmap_root_fill: Option<MermaidColor>,
    #[serde(skip_serializing_if = "Option::is_none")] pub mindmap_root_text: Option<MermaidColor>,
    #[serde(skip_serializing_if = "Option::is_none")] pub mindmap_edge_color: Option<MermaidColor>,
    #[serde(default, deserialize_with = "bounded_palette", skip_serializing_if = "Vec::is_empty")] pub mindmap_section_colors: Vec<MermaidColor>,
    #[serde(default, deserialize_with = "bounded_palette", skip_serializing_if = "Vec::is_empty")] pub mindmap_section_label_colors: Vec<MermaidColor>,
    #[serde(default, deserialize_with = "bounded_palette", skip_serializing_if = "Vec::is_empty")] pub mindmap_section_line_colors: Vec<MermaidColor>,
    #[serde(default, deserialize_with = "bounded_palette", skip_serializing_if = "Vec::is_empty")] pub timeline_event_colors: Vec<MermaidColor>,
    #[serde(default, deserialize_with = "bounded_palette", skip_serializing_if = "Vec::is_empty")] pub pie_colors: Vec<MermaidColor>,
    #[serde(default, deserialize_with = "bounded_palette", skip_serializing_if = "Vec::is_empty")] pub git_colors: Vec<MermaidColor>,
    #[serde(default, deserialize_with = "bounded_palette", skip_serializing_if = "Vec::is_empty")] pub git_inv_colors: Vec<MermaidColor>,
    #[serde(default, deserialize_with = "bounded_palette", skip_serializing_if = "Vec::is_empty")] pub git_branch_label_colors: Vec<MermaidColor>,
}
fn bounded_palette<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<MermaidColor>, D::Error> {
    struct PaletteVisitor;
    impl<'de> de::Visitor<'de> for PaletteVisitor {
        type Value = Vec<MermaidColor>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str("a palette of at most eight colors") }
        fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut colors = Vec::new();
            while let Some(color) = seq.next_element()? {
                if colors.len() == 8 { return Err(de::Error::custom("palette exceeds eight colors")); }
                colors.push(color);
            }
            if colors.is_empty() { return Err(de::Error::custom("palette must not be empty")); }
            Ok(colors)
        }
    }
    deserializer.deserialize_seq(PaletteVisitor)
}
impl MermaidThemeVariables { pub fn is_empty(&self) -> bool { self == &Self::default() } }
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct MermaidStyle {
    #[serde(skip_serializing_if = "Option::is_none")] pub theme: Option<MermaidTheme>,
    #[serde(skip_serializing_if = "MermaidThemeVariables::is_empty")] pub theme_variables: MermaidThemeVariables,
}
impl MermaidStyle {
    pub fn is_empty(&self) -> bool { self == &Self::default() }
    pub fn is_valid(&self) -> bool {
        let vars = &self.theme_variables;
        [&vars.mindmap_section_colors, &vars.mindmap_section_label_colors, &vars.mindmap_section_line_colors,
         &vars.timeline_event_colors, &vars.pie_colors, &vars.git_colors, &vars.git_inv_colors, &vars.git_branch_label_colors]
            .iter().all(|palette| palette.len() <= 8)
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MermaidNodeStyle {
    #[serde(skip_serializing_if = "Option::is_none")] pub fill: Option<MermaidColor>,
    #[serde(skip_serializing_if = "Option::is_none")] pub stroke: Option<MermaidColor>,
    #[serde(skip_serializing_if = "Option::is_none")] pub color: Option<MermaidColor>,
}
impl MermaidNodeStyle { pub fn is_empty(&self) -> bool { self == &Self::default() } }

/// Returns the recognized leading init line and its byte range. Other directive
/// syntax remains visible to the existing strict body admission.
pub fn extract_style(source: &str) -> Result<(MermaidStyle, Option<std::ops::Range<usize>>), (usize, String)> {
    let mut offset = 0;
    for line in source.split_inclusive('\n') {
        let content = line.trim();
        if content.is_empty() { offset += line.len(); continue; }
        if !content.starts_with("%%{") { return Ok((MermaidStyle::default(), None)); }
        let body = content.strip_prefix("%%{init:").and_then(|s| s.strip_suffix("}%%"))
            .ok_or_else(|| (offset, "unsupported or malformed Mermaid init directive".to_owned()))?;
        let style: MermaidStyle = serde_json::from_str(body.trim()).map_err(|error| (offset, format!("invalid color init directive: {error}")))?;
        if style.is_empty() { return Err((offset, "empty color init directive".to_owned())); }
        return Ok((style, Some(offset..offset + line.len())));
    }
    Ok((MermaidStyle::default(), None))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectiveMermaidColors {
    pub explicit: bool,
    pub fill: MermaidColor,
    pub stroke: MermaidColor,
    pub text: MermaidColor,
    pub line: MermaidColor,
    pub background: MermaidColor,
    pub group_fill: MermaidColor,
    pub group_stroke: MermaidColor,
}
impl MermaidStyle {
    pub fn effective(&self) -> EffectiveMermaidColors {
        let mut colors = EffectiveMermaidColors {
            explicit: !self.is_empty(),
            fill: MermaidColor([255, 255, 255]), stroke: MermaidColor([51, 65, 85]),
            text: MermaidColor([15, 23, 42]), line: MermaidColor([71, 85, 105]),
            background: MermaidColor([249, 250, 251]),
            group_fill: MermaidColor([239, 246, 255]), group_stroke: MermaidColor([100, 116, 139]),
        };
        if let Some(theme) = self.theme {
            let name = match theme { MermaidTheme::Default => "default", MermaidTheme::Dark => "dark", MermaidTheme::Forest => "forest", MermaidTheme::Neutral => "neutral" };
            if let Some(preset) = mermaid_rs_renderer::Theme::from_name(name) {
                colors.fill = MermaidColor::parse(&preset.primary_color).unwrap_or(colors.fill);
                colors.stroke = MermaidColor::parse(&preset.primary_border_color).unwrap_or(colors.stroke);
                colors.text = MermaidColor::parse(&preset.primary_text_color).unwrap_or(colors.text);
                colors.background = MermaidColor::parse(&preset.background).unwrap_or(colors.background);
                colors.line = match preset.line_color.as_str() {
                    "lightgrey" => MermaidColor([211, 211, 211]),
                    "green" => MermaidColor([0, 128, 0]),
                    value => MermaidColor::parse(value).unwrap_or(colors.line),
                };
                colors.group_fill = MermaidColor::parse(&preset.cluster_background).unwrap_or(colors.group_fill);
                colors.group_stroke = if preset.cluster_border == "rgba(255, 255, 255, 0.25)" {
                    let [r, g, b] = colors.group_fill.0;
                    MermaidColor([((r as u16 * 3 + 255) / 4) as u8, ((g as u16 * 3 + 255) / 4) as u8, ((b as u16 * 3 + 255) / 4) as u8])
                } else { MermaidColor::parse(&preset.cluster_border).unwrap_or(colors.group_stroke) };
            }
        }
        let vars = &self.theme_variables;
        colors.fill = vars.primary_color.unwrap_or(colors.fill);
        colors.stroke = vars.primary_border_color.unwrap_or(colors.stroke);
        colors.text = vars.primary_text_color.unwrap_or(colors.text);
        colors.line = vars.line_color.unwrap_or(colors.line);
        colors.background = vars.background.unwrap_or(colors.background);
        colors.group_fill = vars.secondary_color.unwrap_or(colors.group_fill);
        colors
    }
}
impl MermaidNodeStyle {
    pub fn effective(&self, mut base: EffectiveMermaidColors) -> EffectiveMermaidColors {
        base.explicit |= !self.is_empty();
        base.fill = self.fill.unwrap_or(base.fill);
        base.stroke = self.stroke.unwrap_or(base.stroke);
        base.text = self.color.unwrap_or(base.text);
        base
    }
}
