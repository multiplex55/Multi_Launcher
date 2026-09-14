use super::model::*;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StyleSource {
    ApplicationFallback,
    UserDefaults,
    SelectedSkin(SkinId),
    Menu(MenuId),
    Ring {
        menu_id: MenuId,
        ring_id: RingId,
    },
    Cell {
        menu_id: MenuId,
        ring_id: RingId,
        cell_id: CellId,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StyleField {
    ItemGlow,
    MenuOuterRim,
    MenuBackground,
    ItemBackground,
    ItemForeground,
    ItemShadow,
    MenuForeground,
    CenterBackground,
    CenterImage,
    SubmenuIndicator,
    ItemGlowOpacity,
    MenuOuterRimOpacity,
    MenuBackgroundOpacity,
    ItemBackgroundOpacity,
    ItemForegroundOpacity,
    ItemShadowOpacity,
    MenuForegroundOpacity,
    CenterBackgroundOpacity,
    CenterImageOpacity,
    SubmenuIndicatorOpacity,
    IconOpacity,
    MenuScale,
    ItemSize,
    RadiusScale,
    CenterSize,
    CenterImageScale,
    ItemImageScale,
    ItemImageYRatio,
    ItemBackgroundScale,
    ItemForegroundScale,
    ItemShadowScale,
    MenuBackgroundScale,
    MenuForegroundScale,
    CenterBackgroundScale,
    SubmenuIndicatorSize,
    SubmenuIndicatorYRatio,
    OuterRingMargin,
    OuterRimWidth,
    ItemBackgroundOnCenter,
    ItemBackgroundOnItems,
    TextVisible,
    SubmenuIndicatorText,
    FontFamily,
    FontSize,
    TextColor,
    Bold,
    Italic,
    Underline,
    Strikeout,
    TextShadowEnabled,
    TextShadowColor,
    TextShadowOffset,
    TextBoxScale,
    TextVerticalRatio,
    GlowEnabled,
    TooltipMode,
    MenuShadowWidth,
    MenuShadowInnerColor,
    MenuShadowOuterColor,
    TextQuality,
    ShapeQuality,
    InterpolationQuality,
    SoundOnShow,
    SoundOnClose,
    SoundOnSelect,
    SoundOnSubmenuShow,
    SoundOnSubmenuClose,
    AlwaysOnTop,
    ActivateOnShow,
    FillCenterHitZone,
    FillItemHitZones,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClearSemantic {
    RemoveMedia,
    SystemFont,
    RemoveText,
    Disabled,
    ResetToApplicationFallback,
}

pub fn clear_semantic(field: StyleField) -> ClearSemantic {
    match field {
        StyleField::ItemGlow
        | StyleField::MenuOuterRim
        | StyleField::MenuBackground
        | StyleField::ItemBackground
        | StyleField::ItemForeground
        | StyleField::ItemShadow
        | StyleField::MenuForeground
        | StyleField::CenterBackground
        | StyleField::CenterImage
        | StyleField::SubmenuIndicator
        | StyleField::SoundOnShow
        | StyleField::SoundOnClose
        | StyleField::SoundOnSelect
        | StyleField::SoundOnSubmenuShow
        | StyleField::SoundOnSubmenuClose => ClearSemantic::RemoveMedia,
        StyleField::FontFamily => ClearSemantic::SystemFont,
        StyleField::SubmenuIndicatorText => ClearSemantic::RemoveText,
        StyleField::TooltipMode => ClearSemantic::Disabled,
        _ => ClearSemantic::ResetToApplicationFallback,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EffectiveMenuStyle {
    pub values: StyleOverrides,
    provenance: BTreeMap<StyleField, StyleSource>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EffectiveRingStyle {
    pub values: StyleOverrides,
    provenance: BTreeMap<StyleField, StyleSource>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EffectiveCellStyle {
    pub values: StyleOverrides,
    provenance: BTreeMap<StyleField, StyleSource>,
}

macro_rules! provenance_api {
    ($name:ident) => {
        impl $name {
            pub fn source(&self, field: StyleField) -> Option<&StyleSource> {
                self.provenance.get(&field)
            }
        }
    };
}

provenance_api!(EffectiveMenuStyle);
provenance_api!(EffectiveRingStyle);
provenance_api!(EffectiveCellStyle);

#[derive(Clone, Debug, PartialEq)]
pub struct EffectiveMenuTree {
    pub menu: EffectiveMenuStyle,
    pub rings: BTreeMap<RingId, EffectiveRingStyle>,
    pub cells: BTreeMap<CellId, EffectiveCellStyle>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StyleCompileError {
    MissingSkin(SkinId),
}

impl ApplicationStyleLayer {
    /// Concrete code-owned fallback. Later-layer `Clear` values use the typed
    /// semantics in `clear_semantic` without losing the clearing layer's provenance.
    pub fn fallback() -> Self {
        let absent = Override::Clear;
        Self {
            values: StyleOverrides {
                images: ImageStyleOverrides {
                    item_glow: absent.clone(),
                    menu_outer_rim: absent.clone(),
                    menu_background: absent.clone(),
                    item_background: absent.clone(),
                    item_foreground: absent.clone(),
                    item_shadow: absent.clone(),
                    menu_foreground: absent.clone(),
                    center_background: absent.clone(),
                    center_image: absent.clone(),
                    submenu_indicator: absent,
                    item_glow_opacity: Override::Value(1.0),
                    menu_outer_rim_opacity: Override::Value(1.0),
                    menu_background_opacity: Override::Value(1.0),
                    item_background_opacity: Override::Value(1.0),
                    item_foreground_opacity: Override::Value(1.0),
                    item_shadow_opacity: Override::Value(1.0),
                    menu_foreground_opacity: Override::Value(1.0),
                    center_background_opacity: Override::Value(1.0),
                    center_image_opacity: Override::Value(1.0),
                    submenu_indicator_opacity: Override::Value(1.0),
                    icon_opacity: Override::Value(1.0),
                },
                geometry: GeometryStyleOverrides {
                    menu_scale: Override::Value(1.0),
                    item_size: Override::Value(56.0),
                    radius_scale: Override::Value(1.0),
                    center_size: Override::Value(60.0),
                    center_image_scale: Override::Value(1.0),
                    item_image_scale: Override::Value(1.0),
                    item_image_y_ratio: Override::Value(0.5),
                    item_background_scale: Override::Value(1.0),
                    item_foreground_scale: Override::Value(1.0),
                    item_shadow_scale: Override::Value(1.0),
                    menu_background_scale: Override::Value(1.0),
                    menu_foreground_scale: Override::Value(1.0),
                    center_background_scale: Override::Value(1.0),
                    submenu_indicator_size: Override::Value(12.0),
                    submenu_indicator_y_ratio: Override::Value(0.75),
                    outer_ring_margin: Override::Value(0.0),
                    outer_rim_width: Override::Value(0.0),
                    item_background_on_center: Override::Value(true),
                    item_background_on_items: Override::Value(true),
                },
                text: TextStyleOverrides {
                    visible: Override::Value(true),
                    submenu_indicator_text: Override::Value("›".into()),
                    font_family: Override::Value(String::new()),
                    font_size: Override::Value(12.0),
                    color: Override::Value(ColorRgba {
                        red: 245,
                        green: 245,
                        blue: 245,
                        alpha: 255,
                    }),
                    bold: Override::Value(false),
                    italic: Override::Value(false),
                    underline: Override::Value(false),
                    strikeout: Override::Value(false),
                    shadow_enabled: Override::Value(false),
                    shadow_color: Override::Value(ColorRgba::default()),
                    shadow_offset: Override::Value(Offset2D { x: 1.0, y: 1.0 }),
                    text_box_scale: Override::Value(1.0),
                    vertical_ratio: Override::Value(0.5),
                },
                effects: EffectStyleOverrides {
                    glow_enabled: Override::Value(false),
                    tooltip_mode: Override::Value(TooltipMode::Explicit),
                    menu_shadow_width: Override::Value(0.0),
                    menu_shadow_inner_color: Override::Value(ColorRgba::default()),
                    menu_shadow_outer_color: Override::Value(ColorRgba::default()),
                },
                quality: QualityStyleOverrides {
                    text: Override::Value(RenderingQuality::Balanced),
                    shape: Override::Value(RenderingQuality::Balanced),
                    interpolation: Override::Value(RenderingQuality::Balanced),
                },
                sounds: SoundStyleOverrides {
                    on_show: Override::Clear,
                    on_close: Override::Clear,
                    on_select: Override::Clear,
                    on_submenu_show: Override::Clear,
                    on_submenu_close: Override::Clear,
                },
                window: WindowStyleOverrides {
                    always_on_top: Override::Value(true),
                    activate_on_show: Override::Value(false),
                    fill_center_hit_zone: Override::Value(true),
                    fill_item_hit_zones: Override::Value(true),
                },
            },
        }
    }
}

pub fn compile_menu_tree(
    document: &RadialDocument,
    menu: &MenuDefinition,
) -> Result<EffectiveMenuTree, StyleCompileError> {
    let skin = document
        .skins
        .iter()
        .find(|skin| skin.id == menu.skin_id)
        .ok_or_else(|| StyleCompileError::MissingSkin(menu.skin_id.clone()))?;
    let fallback = ApplicationStyleLayer::fallback().values;
    let mut values = StyleOverrides::default();
    let mut provenance = BTreeMap::new();
    apply_full(
        &mut values,
        &fallback,
        &fallback,
        StyleSource::ApplicationFallback,
        &mut provenance,
    );
    apply_full(
        &mut values,
        &document.user_style_defaults.values,
        &fallback,
        StyleSource::UserDefaults,
        &mut provenance,
    );
    apply_full(
        &mut values,
        &skin.style.values,
        &fallback,
        StyleSource::SelectedSkin(skin.id.clone()),
        &mut provenance,
    );
    apply_full(
        &mut values,
        &menu.style.values,
        &fallback,
        StyleSource::Menu(menu.id.clone()),
        &mut provenance,
    );
    let menu_style = EffectiveMenuStyle { values, provenance };
    let mut rings = BTreeMap::new();
    let mut cells = BTreeMap::new();
    for ring in &menu.rings {
        let mut ring_values = menu_style.values.clone();
        let mut ring_provenance = menu_style.provenance.clone();
        apply_item(
            &mut ring_values,
            &ring.style.images,
            &ring.style.geometry,
            &ring.style.text,
            &ring.style.quality,
            &ring.style.sounds,
            &fallback,
            StyleSource::Ring {
                menu_id: menu.id.clone(),
                ring_id: ring.id.clone(),
            },
            &mut ring_provenance,
        );
        let ring_style = EffectiveRingStyle {
            values: ring_values,
            provenance: ring_provenance,
        };
        for cell in &ring.cells {
            let mut cell_values = ring_style.values.clone();
            let mut cell_provenance = ring_style.provenance.clone();
            apply_item(
                &mut cell_values,
                &cell.style.images,
                &cell.style.geometry,
                &cell.style.text,
                &cell.style.quality,
                &cell.style.sounds,
                &fallback,
                StyleSource::Cell {
                    menu_id: menu.id.clone(),
                    ring_id: ring.id.clone(),
                    cell_id: cell.id.clone(),
                },
                &mut cell_provenance,
            );
            cells.insert(
                cell.id.clone(),
                EffectiveCellStyle {
                    values: cell_values,
                    provenance: cell_provenance,
                },
            );
        }
        rings.insert(ring.id.clone(), ring_style);
    }
    Ok(EffectiveMenuTree {
        menu: menu_style,
        rings,
        cells,
    })
}

fn apply_value<T: Clone>(
    target: &mut Override<T>,
    incoming: &Override<T>,
    clear_value: &Override<T>,
    field: StyleField,
    source: &StyleSource,
    provenance: &mut BTreeMap<StyleField, StyleSource>,
) {
    match incoming {
        Override::Inherit => return,
        Override::Value(value) => *target = Override::Value(value.clone()),
        Override::Clear => *target = clear_value.clone(),
    }
    provenance.insert(field, source.clone());
}

fn apply_media(
    target: &mut Override<MediaReference>,
    incoming: &Override<MediaReference>,
    field: StyleField,
    source: &StyleSource,
    provenance: &mut BTreeMap<StyleField, StyleSource>,
) {
    if !matches!(incoming, Override::Inherit) {
        *target = incoming.clone();
        provenance.insert(field, source.clone());
    }
}

macro_rules! value_fields {
    ($target:expr, $incoming:expr, $fallback:expr, $source:expr, $provenance:expr;
        $( $section:ident.$field:ident => $variant:ident ),+ $(,)?) => {
        $(apply_value(
            &mut $target.$section.$field,
            &$incoming.$section.$field,
            &$fallback.$section.$field,
            StyleField::$variant,
            &$source,
            $provenance,
        );)+
    };
}

macro_rules! media_fields {
    ($target:expr, $incoming:expr, $source:expr, $provenance:expr;
        $( $section:ident.$field:ident => $variant:ident ),+ $(,)?) => {
        $(apply_media(
            &mut $target.$section.$field,
            &$incoming.$section.$field,
            StyleField::$variant,
            &$source,
            $provenance,
        );)+
    };
}

fn apply_full(
    target: &mut StyleOverrides,
    incoming: &StyleOverrides,
    fallback: &StyleOverrides,
    source: StyleSource,
    provenance: &mut BTreeMap<StyleField, StyleSource>,
) {
    media_fields!(target, incoming, source, provenance;
        images.item_glow => ItemGlow,
        images.menu_outer_rim => MenuOuterRim,
        images.menu_background => MenuBackground,
        images.item_background => ItemBackground,
        images.item_foreground => ItemForeground,
        images.item_shadow => ItemShadow,
        images.menu_foreground => MenuForeground,
        images.center_background => CenterBackground,
        images.center_image => CenterImage,
        images.submenu_indicator => SubmenuIndicator,
        sounds.on_show => SoundOnShow,
        sounds.on_close => SoundOnClose,
        sounds.on_select => SoundOnSelect,
        sounds.on_submenu_show => SoundOnSubmenuShow,
        sounds.on_submenu_close => SoundOnSubmenuClose,
    );
    apply_value(
        &mut target.text.submenu_indicator_text,
        &incoming.text.submenu_indicator_text,
        &Override::Value(String::new()),
        StyleField::SubmenuIndicatorText,
        &source,
        provenance,
    );
    apply_value(
        &mut target.text.font_family,
        &incoming.text.font_family,
        &Override::Value(String::new()),
        StyleField::FontFamily,
        &source,
        provenance,
    );
    apply_value(
        &mut target.effects.tooltip_mode,
        &incoming.effects.tooltip_mode,
        &Override::Value(TooltipMode::Disabled),
        StyleField::TooltipMode,
        &source,
        provenance,
    );
    value_fields!(target, incoming, fallback, source, provenance;
        images.item_glow_opacity => ItemGlowOpacity,
        images.menu_outer_rim_opacity => MenuOuterRimOpacity,
        images.menu_background_opacity => MenuBackgroundOpacity,
        images.item_background_opacity => ItemBackgroundOpacity,
        images.item_foreground_opacity => ItemForegroundOpacity,
        images.item_shadow_opacity => ItemShadowOpacity,
        images.menu_foreground_opacity => MenuForegroundOpacity,
        images.center_background_opacity => CenterBackgroundOpacity,
        images.center_image_opacity => CenterImageOpacity,
        images.submenu_indicator_opacity => SubmenuIndicatorOpacity,
        images.icon_opacity => IconOpacity,
        geometry.menu_scale => MenuScale,
        geometry.item_size => ItemSize,
        geometry.radius_scale => RadiusScale,
        geometry.center_size => CenterSize,
        geometry.center_image_scale => CenterImageScale,
        geometry.item_image_scale => ItemImageScale,
        geometry.item_image_y_ratio => ItemImageYRatio,
        geometry.item_background_scale => ItemBackgroundScale,
        geometry.item_foreground_scale => ItemForegroundScale,
        geometry.item_shadow_scale => ItemShadowScale,
        geometry.menu_background_scale => MenuBackgroundScale,
        geometry.menu_foreground_scale => MenuForegroundScale,
        geometry.center_background_scale => CenterBackgroundScale,
        geometry.submenu_indicator_size => SubmenuIndicatorSize,
        geometry.submenu_indicator_y_ratio => SubmenuIndicatorYRatio,
        geometry.outer_ring_margin => OuterRingMargin,
        geometry.outer_rim_width => OuterRimWidth,
        geometry.item_background_on_center => ItemBackgroundOnCenter,
        geometry.item_background_on_items => ItemBackgroundOnItems,
        text.visible => TextVisible,
        text.font_size => FontSize,
        text.color => TextColor,
        text.bold => Bold,
        text.italic => Italic,
        text.underline => Underline,
        text.strikeout => Strikeout,
        text.shadow_enabled => TextShadowEnabled,
        text.shadow_color => TextShadowColor,
        text.shadow_offset => TextShadowOffset,
        text.text_box_scale => TextBoxScale,
        text.vertical_ratio => TextVerticalRatio,
        effects.glow_enabled => GlowEnabled,
        effects.menu_shadow_width => MenuShadowWidth,
        effects.menu_shadow_inner_color => MenuShadowInnerColor,
        effects.menu_shadow_outer_color => MenuShadowOuterColor,
        quality.text => TextQuality,
        quality.shape => ShapeQuality,
        quality.interpolation => InterpolationQuality,
        window.always_on_top => AlwaysOnTop,
        window.activate_on_show => ActivateOnShow,
        window.fill_center_hit_zone => FillCenterHitZone,
        window.fill_item_hit_zones => FillItemHitZones,
    );
}

#[allow(clippy::too_many_arguments)]
fn apply_item(
    target: &mut StyleOverrides,
    images: &ItemImageStyleOverrides,
    geometry: &ItemGeometryStyleOverrides,
    text: &TextStyleOverrides,
    quality: &QualityStyleOverrides,
    sounds: &ItemSoundStyleOverrides,
    fallback: &StyleOverrides,
    source: StyleSource,
    provenance: &mut BTreeMap<StyleField, StyleSource>,
) {
    apply_media(
        &mut target.images.item_background,
        &images.item_background,
        StyleField::ItemBackground,
        &source,
        provenance,
    );
    apply_media(
        &mut target.images.submenu_indicator,
        &images.submenu_indicator,
        StyleField::SubmenuIndicator,
        &source,
        provenance,
    );
    apply_value(
        &mut target.images.item_background_opacity,
        &images.item_background_opacity,
        &fallback.images.item_background_opacity,
        StyleField::ItemBackgroundOpacity,
        &source,
        provenance,
    );
    apply_value(
        &mut target.images.submenu_indicator_opacity,
        &images.submenu_indicator_opacity,
        &fallback.images.submenu_indicator_opacity,
        StyleField::SubmenuIndicatorOpacity,
        &source,
        provenance,
    );
    apply_value(
        &mut target.images.icon_opacity,
        &images.icon_opacity,
        &fallback.images.icon_opacity,
        StyleField::IconOpacity,
        &source,
        provenance,
    );
    macro_rules! item_value {
        ($target:expr, $incoming:expr, $fallback:expr, $field:ident) => {
            apply_value(
                $target,
                $incoming,
                $fallback,
                StyleField::$field,
                &source,
                provenance,
            )
        };
    }
    item_value!(
        &mut target.geometry.item_image_scale,
        &geometry.item_image_scale,
        &fallback.geometry.item_image_scale,
        ItemImageScale
    );
    item_value!(
        &mut target.geometry.item_image_y_ratio,
        &geometry.item_image_y_ratio,
        &fallback.geometry.item_image_y_ratio,
        ItemImageYRatio
    );
    item_value!(
        &mut target.geometry.submenu_indicator_size,
        &geometry.submenu_indicator_size,
        &fallback.geometry.submenu_indicator_size,
        SubmenuIndicatorSize
    );
    item_value!(
        &mut target.geometry.submenu_indicator_y_ratio,
        &geometry.submenu_indicator_y_ratio,
        &fallback.geometry.submenu_indicator_y_ratio,
        SubmenuIndicatorYRatio
    );
    macro_rules! text_value {
        ($field:ident, $variant:ident) => {
            item_value!(
                &mut target.text.$field,
                &text.$field,
                &fallback.text.$field,
                $variant
            );
        };
    }
    text_value!(visible, TextVisible);
    apply_value(
        &mut target.text.submenu_indicator_text,
        &text.submenu_indicator_text,
        &Override::Value(String::new()),
        StyleField::SubmenuIndicatorText,
        &source,
        provenance,
    );
    apply_value(
        &mut target.text.font_family,
        &text.font_family,
        &Override::Value(String::new()),
        StyleField::FontFamily,
        &source,
        provenance,
    );
    text_value!(font_size, FontSize);
    text_value!(color, TextColor);
    text_value!(bold, Bold);
    text_value!(italic, Italic);
    text_value!(underline, Underline);
    text_value!(strikeout, Strikeout);
    text_value!(shadow_enabled, TextShadowEnabled);
    text_value!(shadow_color, TextShadowColor);
    text_value!(shadow_offset, TextShadowOffset);
    text_value!(text_box_scale, TextBoxScale);
    text_value!(vertical_ratio, TextVerticalRatio);
    item_value!(
        &mut target.quality.text,
        &quality.text,
        &fallback.quality.text,
        TextQuality
    );
    item_value!(
        &mut target.quality.shape,
        &quality.shape,
        &fallback.quality.shape,
        ShapeQuality
    );
    item_value!(
        &mut target.quality.interpolation,
        &quality.interpolation,
        &fallback.quality.interpolation,
        InterpolationQuality
    );
    apply_media(
        &mut target.sounds.on_select,
        &sounds.on_select,
        StyleField::SoundOnSelect,
        &source,
        provenance,
    );
}

pub fn resolved_f32(value: &Override<f32>) -> f32 {
    match value {
        Override::Value(value) => *value,
        Override::Inherit | Override::Clear => unreachable!("compiled scalar style is concrete"),
    }
}

pub fn resolved_bool(value: &Override<bool>) -> bool {
    match value {
        Override::Value(value) => *value,
        Override::Inherit | Override::Clear => unreachable!("compiled boolean style is concrete"),
    }
}

pub fn resolved_clone<T: Clone>(value: &Override<T>) -> T {
    match value {
        Override::Value(value) => value.clone(),
        Override::Inherit | Override::Clear => unreachable!("compiled value style is concrete"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precedence_and_clear_are_field_specific_and_keep_provenance() {
        let mut document = RadialDocument::starter();
        document.user_style_defaults.values.geometry.item_size = Override::Value(40.0);
        document.skins[0].style.values.geometry.item_size = Override::Value(48.0);
        document.menus[0].style.values.geometry.item_size = Override::Value(52.0);
        document.menus[0].style.values.geometry.outer_ring_margin = Override::Value(0.0);
        document.menus[0].style.values.effects.glow_enabled = Override::Value(false);
        document.menus[0].rings[0].style.text.bold = Override::Value(true);
        document.menus[0].rings[0].cells[0].style.text.bold = Override::Clear;
        document.menus[0].rings[0].cells[0]
            .style
            .images
            .item_background = Override::Clear;
        document.menus[0].rings[0].cells[0].style.text.font_family = Override::Clear;
        let tree = compile_menu_tree(&document, &document.menus[0]).unwrap();
        assert_eq!(tree.menu.values.geometry.item_size, Override::Value(52.0));
        assert_eq!(
            tree.menu.values.geometry.outer_ring_margin,
            Override::Value(0.0)
        );
        assert_eq!(
            tree.menu.values.effects.glow_enabled,
            Override::Value(false)
        );
        assert_eq!(
            tree.menu.source(StyleField::ItemSize),
            Some(&StyleSource::Menu(document.menus[0].id.clone()))
        );
        let cell = tree
            .cells
            .get(&document.menus[0].rings[0].cells[0].id)
            .unwrap();
        assert_eq!(cell.values.text.bold, Override::Value(false));
        assert_eq!(cell.values.images.item_background, Override::Clear);
        assert_eq!(cell.values.text.font_family, Override::Value(String::new()));
        assert_eq!(
            clear_semantic(StyleField::ItemBackground),
            ClearSemantic::RemoveMedia
        );
        assert_eq!(
            clear_semantic(StyleField::FontFamily),
            ClearSemantic::SystemFont
        );
        assert_eq!(
            cell.source(StyleField::Bold),
            Some(&StyleSource::Cell {
                menu_id: document.menus[0].id.clone(),
                ring_id: document.menus[0].rings[0].id.clone(),
                cell_id: document.menus[0].rings[0].cells[0].id.clone(),
            })
        );
    }

    #[test]
    fn every_scope_participates_without_copying_behavior_to_children() {
        let mut document = RadialDocument::starter();
        let mut child = document.menus[0].clone();
        child.id = MenuId::new("child");
        child.name = "Child".into();
        child.interaction = InteractionMode::ReleaseToSelect;
        child.after_action = AfterActionPolicy::CloseTree;
        document.menus[0].interaction = InteractionMode::StickyClick;
        document.menus[0].after_action = AfterActionPolicy::KeepOpen;
        document.menus[0].style.values.text.bold = Override::Value(true);
        document.user_style_defaults.values.text.font_size = Override::Value(13.0);
        document.skins[0].style.values.text.font_size = Override::Value(14.0);
        child.style.values.text.font_size = Override::Value(15.0);
        child.rings[0].style.text.font_size = Override::Value(16.0);
        child.rings[0].cells[0].style.text.font_size = Override::Value(17.0);
        document.menus.push(child.clone());
        let compiled = compile_menu_tree(&document, &child).unwrap();
        assert_eq!(
            compiled
                .cells
                .get(&child.rings[0].cells[0].id)
                .unwrap()
                .values
                .text
                .font_size,
            Override::Value(17.0)
        );
        assert_eq!(child.interaction, InteractionMode::ReleaseToSelect);
        assert_eq!(child.after_action, AfterActionPolicy::CloseTree);
        assert_eq!(compiled.menu.values.text.bold, Override::Value(false));
    }

    #[test]
    fn provenance_is_table_driven_across_all_six_scopes() {
        for level in 0..=5 {
            let mut document = RadialDocument::starter();
            let menu_id = document.menus[0].id.clone();
            let ring_id = document.menus[0].rings[0].id.clone();
            let cell_id = document.menus[0].rings[0].cells[0].id.clone();
            let skin_id = document.skins[0].id.clone();
            match level {
                0 => {}
                1 => document.user_style_defaults.values.text.italic = Override::Value(true),
                2 => document.skins[0].style.values.text.italic = Override::Value(true),
                3 => document.menus[0].style.values.text.italic = Override::Value(true),
                4 => document.menus[0].rings[0].style.text.italic = Override::Value(true),
                5 => document.menus[0].rings[0].cells[0].style.text.italic = Override::Value(true),
                _ => unreachable!(),
            }
            let tree = compile_menu_tree(&document, &document.menus[0]).unwrap();
            let cell = tree.cells.get(&cell_id).unwrap();
            let expected = match level {
                0 => StyleSource::ApplicationFallback,
                1 => StyleSource::UserDefaults,
                2 => StyleSource::SelectedSkin(skin_id),
                3 => StyleSource::Menu(menu_id),
                4 => StyleSource::Ring { menu_id, ring_id },
                5 => StyleSource::Cell {
                    menu_id,
                    ring_id,
                    cell_id,
                },
                _ => unreachable!(),
            };
            assert_eq!(cell.source(StyleField::Italic), Some(&expected));
            assert_eq!(
                cell.values.text.italic,
                Override::Value(level != 0),
                "scope level {level}"
            );
        }
    }

    #[test]
    fn icon_opacity_is_independent_and_obeys_item_scope_precedence() {
        let mut document = RadialDocument::starter();
        let cell_id = document.menus[0].rings[0].cells[0].id.clone();
        document.skins[0]
            .style
            .values
            .images
            .item_foreground_opacity = Override::Value(0.9);
        document.skins[0].style.values.images.icon_opacity = Override::Value(0.8);
        document.menus[0].rings[0].style.images.icon_opacity = Override::Value(0.5);
        document.menus[0].rings[0].cells[0]
            .style
            .images
            .icon_opacity = Override::Value(0.25);
        let tree = compile_menu_tree(&document, &document.menus[0]).unwrap();
        let cell = tree.cells.get(&cell_id).unwrap();
        assert_eq!(cell.values.images.icon_opacity, Override::Value(0.25));
        assert_eq!(
            cell.values.images.item_foreground_opacity,
            Override::Value(0.9)
        );
        assert!(matches!(
            cell.source(StyleField::IconOpacity),
            Some(StyleSource::Cell { .. })
        ));
    }
}
