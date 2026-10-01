//! Curated style data and narrow menu appearance patches. No runtime actions.

use super::model::*;
use super::skin::{StyleField, StyleSource, compile_menu_tree};

/// Catalog identity is separate from authored SkinId. Document copies remain
/// custom entries; editing/importing a reserved-looking ID never grants origin.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BuiltinSkin {
    ModernClean,
    Compact,
    Comfortable,
    HighContrast,
    Classic,
}

pub const BUILTIN_SKINS: [BuiltinSkin; 5] = [
    BuiltinSkin::ModernClean,
    BuiltinSkin::Compact,
    BuiltinSkin::Comfortable,
    BuiltinSkin::HighContrast,
    BuiltinSkin::Classic,
];

impl BuiltinSkin {
    pub fn key(self) -> &'static str {
        match self {
            Self::ModernClean => "modern-clean",
            Self::Compact => "compact",
            Self::Comfortable => "comfortable",
            Self::HighContrast => "high-contrast",
            Self::Classic => "classic",
        }
    }
    pub fn definition(self) -> SkinDefinition {
        let (name, item, size, spacing, rgb) = match self {
            Self::ModernClean => ("Modern Clean", 64.0, 14.0, 1.12, [215, 232, 255]),
            Self::Compact => ("Compact", 48.0, 11.0, 0.94, [200, 224, 250]),
            Self::Comfortable => ("Comfortable", 72.0, 15.5, 1.25, [225, 240, 255]),
            Self::HighContrast => ("High Contrast", 72.0, 17.0, 1.27, [255, 255, 255]),
            Self::Classic => ("Classic", 56.0, 12.0, 1.12, [255, 235, 190]),
        };
        SkinDefinition {
            id: SkinId::new(format!("builtin-{}", self.key())),
            name: name.into(),
            style: SelectedSkinStyleLayer {
                values: StyleOverrides {
                    geometry: GeometryStyleOverrides {
                        item_size: Override::Value(item),
                        menu_scale: Override::Value(1.0),
                        radius_scale: Override::Value(spacing),
                        outer_rim_width: Override::Value(if self == Self::Classic {
                            6.0
                        } else {
                            0.0
                        }),
                        ..Default::default()
                    },
                    text: TextStyleOverrides {
                        visible: Override::Value(true),
                        font_size: Override::Value(size),
                        color: Override::Value(ColorRgba {
                            red: rgb[0],
                            green: rgb[1],
                            blue: rgb[2],
                            alpha: 255,
                        }),
                        shadow_enabled: Override::Value(self == Self::Classic),
                        ..Default::default()
                    },
                    effects: EffectStyleOverrides {
                        glow_enabled: Override::Value(self == Self::Classic),
                        emphasize_selection: Override::Value(self == Self::HighContrast),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            },
        }
    }
}

/// Build from the latest draft, preserving every override, binding, other menu
/// and asset. Callers validate and check their captured session/generation before
/// publishing this candidate as one history transaction.
pub fn preset_candidate(
    document: &RadialDocument,
    destination: &MenuId,
    skin: &SkinDefinition,
) -> Result<RadialDocument, String> {
    if !document.menus.iter().any(|menu| &menu.id == destination) {
        return Err("appearance destination menu no longer exists".into());
    }
    let mut candidate = document.clone();
    let id = if let Some(existing) = candidate
        .skins
        .iter()
        .find(|existing| existing.style == skin.style && existing.name == skin.name)
    {
        existing.id.clone()
    } else {
        if candidate.skins.len() >= limits::MAX_SKINS {
            return Err("skin limit reached".into());
        }
        let mut copy = skin.clone();
        for ordinal in 1..=limits::MAX_SKINS + 1 {
            let id = if ordinal == 1 {
                skin.id.clone()
            } else {
                SkinId::new(format!("{}-{ordinal}", skin.id))
            };
            if !candidate.skins.iter().any(|existing| existing.id == id) {
                copy.id = id;
                break;
            }
        }
        let id = copy.id.clone();
        candidate.skins.push(copy);
        id
    };
    candidate
        .menus
        .iter_mut()
        .find(|menu| &menu.id == destination)
        .ok_or("appearance destination missing")?
        .skin_id = id;
    Ok(candidate)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimpleControl {
    Accent,
    Opacity,
    Scale,
    Spacing,
    LabelSize,
    Labels,
    Bold,
    Shadow,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SimpleEdit {
    Accent([u8; 3]),
    Opacity(f32),
    Scale(f32),
    Spacing(f32),
    LabelSize(f32),
    Labels(bool),
    Bold(bool),
    Shadow(bool),
    Reset {
        control: SimpleControl,
        descendants: bool,
    },
}

/// Exact opacity mapping: label alpha, icons, and the ten decorative image /
/// glow channels. Fallback circle/wedge fills and tooltip alpha are untouched.
pub const OPACITY_FIELDS: &[StyleField] = &[
    StyleField::TextColor,
    StyleField::IconOpacity,
    StyleField::ItemGlowOpacity,
    StyleField::ItemBackgroundOpacity,
    StyleField::ItemForegroundOpacity,
    StyleField::ItemShadowOpacity,
    StyleField::MenuBackgroundOpacity,
    StyleField::MenuForegroundOpacity,
    StyleField::MenuOuterRimOpacity,
    StyleField::CenterBackgroundOpacity,
    StyleField::CenterImageOpacity,
    StyleField::SubmenuIndicatorOpacity,
];

impl SimpleControl {
    pub fn fields(self) -> &'static [StyleField] {
        match self {
            Self::Accent => &[StyleField::TextColor],
            Self::Opacity => OPACITY_FIELDS,
            Self::Scale => &[StyleField::MenuScale],
            Self::Spacing => &[StyleField::RadiusScale],
            Self::LabelSize => &[StyleField::FontSize],
            Self::Labels => &[StyleField::TextVisible],
            Self::Bold => &[StyleField::Bold],
            Self::Shadow => &[StyleField::TextShadowEnabled],
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SimpleAppearance {
    pub values: StyleOverrides,
    /// None means the normalized channels differ (Mixed).
    pub opacity: Option<f32>,
    pub provenance: std::collections::BTreeMap<StyleField, StyleSource>,
    pub masks: std::collections::BTreeMap<StyleField, Vec<StyleSource>>,
}

pub fn simple_state(
    document: &RadialDocument,
    destination: &MenuId,
) -> Result<SimpleAppearance, String> {
    let menu = document
        .menus
        .iter()
        .find(|menu| &menu.id == destination)
        .ok_or("menu missing")?;
    let tree = compile_menu_tree(document, menu).map_err(|error| format!("{error:?}"))?;
    let values = tree.menu.values.clone();
    let color = value(&values.text.color)?;
    let opacity = color.alpha as f32 / 255.0;
    let image = &values.images;
    let channels = [
        &image.icon_opacity,
        &image.item_glow_opacity,
        &image.item_background_opacity,
        &image.item_foreground_opacity,
        &image.item_shadow_opacity,
        &image.menu_background_opacity,
        &image.menu_foreground_opacity,
        &image.menu_outer_rim_opacity,
        &image.center_background_opacity,
        &image.center_image_opacity,
        &image.submenu_indicator_opacity,
    ];
    let uniform = channels
        .iter()
        .all(|channel| value(channel).is_ok_and(|v| (v - opacity).abs() <= 1.0 / 255.0));
    let mut masks = std::collections::BTreeMap::<StyleField, Vec<StyleSource>>::new();
    for field in super::skin::ALL_STYLE_FIELDS {
        for source in tree
            .rings
            .values()
            .filter_map(|ring| ring.source(*field))
            .chain(tree.cells.values().filter_map(|cell| cell.source(*field)))
        {
            if matches!(source, StyleSource::Ring { .. } | StyleSource::Cell { .. }) {
                let entries = masks.entry(*field).or_default();
                if !entries.contains(source) {
                    entries.push(source.clone());
                }
            }
        }
    }
    Ok(SimpleAppearance {
        values,
        opacity: uniform.then_some(opacity),
        provenance: super::skin::ALL_STYLE_FIELDS
            .iter()
            .filter_map(|field| {
                tree.menu
                    .source(*field)
                    .cloned()
                    .map(|source| (*field, source))
            })
            .collect(),
        masks,
    })
}

pub fn value<T: Clone>(slot: &Override<T>) -> Result<T, String> {
    if let Override::Value(value) = slot {
        Ok(value.clone())
    } else {
        Err("unresolved effective style".into())
    }
}

pub fn simple_candidate(
    document: &RadialDocument,
    destination: &MenuId,
    edit: &SimpleEdit,
) -> Result<RadialDocument, String> {
    let effective = simple_state(document, destination)?.values;
    let mut candidate = document.clone();
    let menu = candidate
        .menus
        .iter_mut()
        .find(|menu| &menu.id == destination)
        .ok_or("menu missing")?;
    let patch = &mut menu.style.values;
    match edit {
        SimpleEdit::Accent(rgb) => {
            let mut color = value(&effective.text.color)?;
            [color.red, color.green, color.blue] = *rgb;
            patch.text.color = Override::Value(color);
        }
        SimpleEdit::Opacity(opacity) => {
            if !opacity.is_finite() || !(0.0..=1.0).contains(opacity) {
                return Err("opacity must be between 0 and 1".into());
            }
            let mut color = value(&effective.text.color)?;
            color.alpha = (opacity * 255.0).round() as u8;
            patch.text.color = Override::Value(color);
            macro_rules! set { ($($field:ident),+) => { $(patch.images.$field = Override::Value(*opacity);)+ }; }
            set!(
                icon_opacity,
                item_glow_opacity,
                item_background_opacity,
                item_foreground_opacity,
                item_shadow_opacity,
                menu_background_opacity,
                menu_foreground_opacity,
                menu_outer_rim_opacity,
                center_background_opacity,
                center_image_opacity,
                submenu_indicator_opacity
            );
        }
        SimpleEdit::Scale(v) => patch.geometry.menu_scale = Override::Value(*v),
        SimpleEdit::Spacing(v) => patch.geometry.radius_scale = Override::Value(*v),
        SimpleEdit::LabelSize(v) => patch.text.font_size = Override::Value(*v),
        SimpleEdit::Labels(v) => patch.text.visible = Override::Value(*v),
        SimpleEdit::Bold(v) => patch.text.bold = Override::Value(*v),
        SimpleEdit::Shadow(v) => patch.text.shadow_enabled = Override::Value(*v),
        SimpleEdit::Reset {
            control,
            descendants,
        } => {
            // Serialize only the chosen legal slots; all Clear/Value/Inherit
            // states outside them remain byte-for-byte equivalent in the model.
            reset_fields(patch, control.fields())?;
            if *descendants {
                for ring in &mut menu.rings {
                    reset_fields(&mut ring.style, control.fields())?;
                    for cell in &mut ring.cells {
                        reset_fields(&mut cell.style, control.fields())?;
                    }
                }
            }
        }
    }
    super::validation::validate(&candidate).map_err(|errors| format!("{errors:?}"))?;
    Ok(candidate)
}

fn reset_fields<T: serde::Serialize + serde::de::DeserializeOwned>(
    layer: &mut T,
    fields: &[StyleField],
) -> Result<(), String> {
    let mut json = serde_json::to_value(&*layer).map_err(|error| error.to_string())?;
    for (section, slots) in json.as_object_mut().ok_or("invalid style")? {
        if let Some(slots) = slots.as_object_mut() {
            for (name, slot) in slots {
                if field_path(section, name).is_some_and(|field| fields.contains(&field)) {
                    *slot = serde_json::to_value(Override::<bool>::Inherit)
                        .map_err(|error| error.to_string())?;
                }
            }
        }
    }
    *layer = serde_json::from_value(json).map_err(|error| error.to_string())?;
    Ok(())
}

fn field_path(section: &str, field: &str) -> Option<StyleField> {
    match (section, field) {
        ("geometry", "menu_scale") => Some(StyleField::MenuScale),
        ("geometry", "radius_scale") => Some(StyleField::RadiusScale),
        ("text", "font_size") => Some(StyleField::FontSize),
        ("text", "visible") => Some(StyleField::TextVisible),
        ("text", "bold") => Some(StyleField::Bold),
        ("text", "shadow_enabled") => Some(StyleField::TextShadowEnabled),
        ("text", "color") => Some(StyleField::TextColor),
        ("images", "icon_opacity") => Some(StyleField::IconOpacity),
        ("images", "item_glow_opacity") => Some(StyleField::ItemGlowOpacity),
        ("images", "item_background_opacity") => Some(StyleField::ItemBackgroundOpacity),
        ("images", "item_foreground_opacity") => Some(StyleField::ItemForegroundOpacity),
        ("images", "item_shadow_opacity") => Some(StyleField::ItemShadowOpacity),
        ("images", "menu_background_opacity") => Some(StyleField::MenuBackgroundOpacity),
        ("images", "menu_foreground_opacity") => Some(StyleField::MenuForegroundOpacity),
        ("images", "menu_outer_rim_opacity") => Some(StyleField::MenuOuterRimOpacity),
        ("images", "center_background_opacity") => Some(StyleField::CenterBackgroundOpacity),
        ("images", "center_image_opacity") => Some(StyleField::CenterImageOpacity),
        ("images", "submenu_indicator_opacity") => Some(StyleField::SubmenuIndicatorOpacity),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn grouped_opacity_gesture_is_one_history_unit_and_preserves_selection_assets_and_unmapped_slots()
     {
        use super::super::authoring::*;
        use std::sync::Arc;
        let mut doc = RadialDocument::starter();
        let destination = doc.default_menu_id.clone();
        doc.menus[0].style.values.effects.tooltip_mode = Override::Clear;
        doc.menus[0].rings[0].cells[0].style.text.bold = Override::Clear;
        let mut session =
            RadialAuthoringSession::new(AuthoringSnapshot::new(Arc::new(doc), "coalescing"));
        let before = Arc::clone(&session.draft);
        let assets = session.pending_assets.clone();
        let selection = session.selection.clone();
        let key = EditKey {
            entity: format!("appearance:{destination}"),
            field: "opacity".into(),
        };
        for (opacity, phase) in [
            (0.8, EditPhase::Begin),
            (0.6, EditPhase::Update),
            (0.4, EditPhase::End),
        ] {
            let next =
                simple_candidate(&session.draft, &destination, &SimpleEdit::Opacity(opacity))
                    .unwrap();
            assert_eq!(next.menus[0].rings, before.menus[0].rings);
            assert_eq!(next.menus[1..], before.menus[1..]);
            assert_eq!(
                next.menus[0].style.values.effects,
                before.menus[0].style.values.effects
            );
            session
                .replace_document_edit(next, key.clone(), phase)
                .unwrap();
        }
        assert_eq!(session.acceptance_history_depths(), (1, 0));
        assert_eq!(session.pending_assets, assets);
        assert_eq!(session.selection, selection);
        let after = Arc::clone(&session.draft);
        assert!(session.undo());
        assert_eq!(session.draft, before);
        assert!(session.redo());
        assert_eq!(session.draft, after);
    }
    #[test]
    fn every_simple_mapping_and_reset_touches_only_its_named_slots() {
        let document = RadialDocument::starter();
        let destination = document.default_menu_id.clone();
        for (edit, control) in [
            (SimpleEdit::Accent([3, 5, 7]), SimpleControl::Accent),
            (SimpleEdit::Opacity(0.5), SimpleControl::Opacity),
            (SimpleEdit::Scale(1.25), SimpleControl::Scale),
            (SimpleEdit::Spacing(1.4), SimpleControl::Spacing),
            (SimpleEdit::LabelSize(19.0), SimpleControl::LabelSize),
            (SimpleEdit::Labels(false), SimpleControl::Labels),
            (SimpleEdit::Bold(true), SimpleControl::Bold),
            (SimpleEdit::Shadow(true), SimpleControl::Shadow),
        ] {
            let candidate = simple_candidate(&document, &destination, &edit).unwrap();
            let reset = simple_candidate(
                &candidate,
                &destination,
                &SimpleEdit::Reset {
                    control,
                    descendants: false,
                },
            )
            .unwrap();
            assert_eq!(
                reset, document,
                "reset should only remove the deliberately added mapped menu slots: {control:?}"
            );
        }
        let mut mixed = document.clone();
        mixed.menus[0].style.values.images.icon_opacity = Override::Value(0.2);
        let state = simple_state(&mixed, &destination).unwrap();
        assert!(state.opacity.is_none());
        assert!(matches!(
            state.provenance[&StyleField::IconOpacity],
            StyleSource::Menu(_)
        ));
        assert!(simple_candidate(&document, &destination, &SimpleEdit::Scale(f32::NAN)).is_err());
    }
    #[test]
    fn curated_skins_round_trip_through_the_existing_collision_safe_package_boundary() {
        use super::super::package::*;
        use std::collections::BTreeMap;
        for builtin in BUILTIN_SKINS {
            let mut document = RadialDocument::starter();
            let skin = builtin.definition();
            document.skins = vec![skin.clone()];
            for menu in &mut document.menus {
                menu.skin_id = skin.id.clone();
            }
            let export =
                plan_skin_export(&document, &skin.id, &BTreeMap::new(), Vec::new()).unwrap();
            let imported = plan_skin_import(
                decode_mlradial(&encode_mlradial(&export).unwrap()).unwrap(),
                &document,
            )
            .unwrap();
            assert_eq!(imported.skin.style, skin.style);
            assert_ne!(imported.skin.id, skin.id);
            assert!(imported.assets.is_empty());
        }
    }
    #[test]
    fn presets_validate_are_distinct_and_collision_safe_without_touching_existing_work() {
        let mut document = RadialDocument::starter();
        let destination = document.default_menu_id.clone();
        let mut collision = BuiltinSkin::Compact.definition();
        collision.name = "My unrelated skin".into();
        document.skins.push(collision.clone());
        document.menus[0].style.values.text.underline = Override::Clear;
        let prior = document.clone();
        for preset in BUILTIN_SKINS {
            let candidate =
                preset_candidate(&document, &destination, &preset.definition()).unwrap();
            super::super::validation::validate(&candidate).unwrap();
            assert_eq!(
                candidate.skins.iter().find(|s| s.id == collision.id),
                Some(&collision)
            );
            assert_eq!(candidate.menus[0].style, document.menus[0].style);
            assert_eq!(candidate.menus[1..], document.menus[1..]);
            assert_eq!(candidate.menus[0].rings, document.menus[0].rings);
        }
        assert_eq!(document, prior);
        let styles: Vec<_> = BUILTIN_SKINS.iter().map(|p| p.definition().style).collect();
        for (i, a) in styles.iter().enumerate() {
            for b in &styles[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }
    #[test]
    fn opacity_and_accent_preserve_rgb_alpha_media_and_advanced_masks() {
        let mut document = RadialDocument::starter();
        let destination = document.default_menu_id.clone();
        document.menus[0].style.values.text.color = Override::Value(ColorRgba {
            red: 1,
            green: 2,
            blue: 3,
            alpha: 111,
        });
        document.menus[0].style.values.images.center_image = Override::Clear;
        document.menus[0].rings[0].style.text.color = Override::Clear;
        let accent =
            simple_candidate(&document, &destination, &SimpleEdit::Accent([9, 8, 7])).unwrap();
        assert_eq!(
            value(
                &simple_state(&accent, &destination)
                    .unwrap()
                    .values
                    .text
                    .color
            )
            .unwrap()
            .alpha,
            111
        );
        let opacity = simple_candidate(&accent, &destination, &SimpleEdit::Opacity(0.4)).unwrap();
        let state = simple_state(&opacity, &destination).unwrap();
        assert!((state.opacity.unwrap() - 0.4).abs() < 0.001);
        assert_eq!(
            value(&state.values.text.color).unwrap(),
            ColorRgba {
                red: 9,
                green: 8,
                blue: 7,
                alpha: 102
            }
        );
        assert_eq!(opacity.menus[0].rings, document.menus[0].rings);
        assert_eq!(
            opacity.menus[0].style.values.images.center_image,
            Override::Clear
        );
        assert!(state.masks.contains_key(&StyleField::TextColor));
        let reset = simple_candidate(
            &opacity,
            &destination,
            &SimpleEdit::Reset {
                control: SimpleControl::Opacity,
                descendants: false,
            },
        )
        .unwrap();
        assert_eq!(reset.menus[0].rings[0].style.text.color, Override::Clear);
        let descendants = simple_candidate(
            &opacity,
            &destination,
            &SimpleEdit::Reset {
                control: SimpleControl::Opacity,
                descendants: true,
            },
        )
        .unwrap();
        assert_eq!(
            descendants.menus[0].rings[0].style.text.color,
            Override::Inherit
        );
        assert_eq!(
            descendants.menus[0].style.values.images.center_image,
            Override::Clear
        );
        assert!(simple_candidate(&document, &destination, &SimpleEdit::Opacity(f32::NAN)).is_err());
    }
    #[test]
    fn old_document_without_emphasis_keeps_legacy_false_and_custom_style() {
        let mut document = RadialDocument::starter();
        document.skins[0].name = "Carbon custom".into();
        document.skins[0].style.values = StyleOverrides::default();
        let mut json = serde_json::to_value(&document).unwrap();
        json["skins"][0]["style"]["values"]["effects"]
            .as_object_mut()
            .unwrap()
            .remove("emphasize_selection");
        let decoded: RadialDocument = serde_json::from_value(json).unwrap();
        assert_eq!(decoded, document);
        let tree = compile_menu_tree(&decoded, &decoded.menus[0]).unwrap();
        assert_eq!(
            tree.menu.values.effects.emphasize_selection,
            Override::Value(false)
        );
    }
}
