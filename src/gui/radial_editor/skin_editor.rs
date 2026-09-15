//! Generic typed-style editor over the complete M4 override schema.

use crate::radial::authoring::RadialAuthoringSession;
use crate::radial::model::{
    ApplicationStyleLayer, CellId, MenuId, Override, RadialDocument, RingId, SkinId,
};
use crate::radial::skin::{StyleField, StyleSource, compile_menu_tree};
use crate::radial::store::{ReferenceImpact, references_to_skin};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum StyleScope {
    UserDefaults,
    Skin(SkinId),
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

#[derive(Clone, Debug, PartialEq)]
pub(super) struct StyleRow {
    pub section: String,
    pub field: String,
    pub current: serde_json::Value,
    pub inherited: serde_json::Value,
    pub source: Option<StyleSource>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StyleControlKind {
    Toggle,
    Scalar,
    Opacity,
    Color,
    Font,
    Text,
    Offset,
    TooltipMode,
    Quality,
    Resource,
}

/// The editor's explicit UI contract for every supported typed style field.
/// This is intentionally independent of serde's representation: JSON remains
/// an internal transport for the heterogeneous override slots, never a user
/// interface.
pub(super) fn control_kind(section: &str, field: &str) -> Option<StyleControlKind> {
    use StyleControlKind::*;
    match (section, field) {
        (
            "images",
            "item_glow" | "menu_outer_rim" | "menu_background" | "item_background"
            | "item_foreground" | "item_shadow" | "menu_foreground" | "center_background"
            | "center_image" | "submenu_indicator",
        )
        | (
            "sounds",
            "on_show" | "on_close" | "on_select" | "on_submenu_show" | "on_submenu_close",
        ) => Some(Resource),
        (
            "images",
            "item_glow_opacity"
            | "menu_outer_rim_opacity"
            | "menu_background_opacity"
            | "item_background_opacity"
            | "item_foreground_opacity"
            | "item_shadow_opacity"
            | "menu_foreground_opacity"
            | "center_background_opacity"
            | "center_image_opacity"
            | "submenu_indicator_opacity"
            | "icon_opacity",
        ) => Some(Opacity),
        ("geometry", "item_background_on_center" | "item_background_on_items")
        | ("text", "visible" | "bold" | "italic" | "underline" | "strikeout" | "shadow_enabled")
        | ("effects", "glow_enabled")
        | (
            "window",
            "always_on_top" | "activate_on_show" | "fill_center_hit_zone" | "fill_item_hit_zones",
        ) => Some(Toggle),
        ("text", "color" | "shadow_color")
        | ("effects", "menu_shadow_inner_color" | "menu_shadow_outer_color") => Some(Color),
        ("text", "font_family") => Some(Font),
        ("text", "submenu_indicator_text") => Some(Text),
        ("text", "shadow_offset") => Some(Offset),
        ("effects", "tooltip_mode") => Some(TooltipMode),
        ("quality", "text" | "shape" | "interpolation") => Some(Quality),
        (
            "geometry",
            "menu_scale"
            | "item_size"
            | "radius_scale"
            | "center_size"
            | "center_image_scale"
            | "item_image_scale"
            | "item_image_y_ratio"
            | "item_background_scale"
            | "item_foreground_scale"
            | "item_shadow_scale"
            | "menu_background_scale"
            | "menu_foreground_scale"
            | "center_background_scale"
            | "submenu_indicator_size"
            | "submenu_indicator_y_ratio"
            | "outer_ring_margin"
            | "outer_rim_width",
        )
        | ("text", "font_size" | "text_box_scale" | "vertical_ratio")
        | ("effects", "menu_shadow_width") => Some(Scalar),
        _ => None,
    }
}

pub(super) fn override_payload(value: &serde_json::Value) -> Option<serde_json::Value> {
    value.get("value").cloned()
}

pub(super) fn with_payload(payload: serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "value": payload })
}

pub(super) fn create_skin(
    session: &mut RadialAuthoringSession,
    name: &str,
) -> Result<SkinId, String> {
    let mut document = (*session.draft).clone();
    let mut ordinal = document.skins.len() + 1;
    let id = loop {
        let candidate = SkinId::new(format!("skin-{ordinal}"));
        if !document.skins.iter().any(|skin| skin.id == candidate) {
            break candidate;
        }
        ordinal += 1;
    };
    document.skins.push(crate::radial::model::SkinDefinition {
        id: id.clone(),
        name: name.to_owned(),
        style: Default::default(),
    });
    session
        .replace_document_atomic(document)
        .map_err(|error| format!("{error:?}"))?;
    session.select(Some(crate::radial::authoring::StableSelection::Skin(
        id.clone(),
    )));
    Ok(id)
}

pub(super) fn duplicate_skin(
    session: &mut RadialAuthoringSession,
    source: &SkinId,
) -> Result<SkinId, String> {
    let mut document = (*session.draft).clone();
    let mut copy = document
        .skins
        .iter()
        .find(|skin| &skin.id == source)
        .cloned()
        .ok_or("skin missing")?;
    let mut ordinal = document.skins.len() + 1;
    copy.id = loop {
        let id = SkinId::new(format!("skin-{ordinal}"));
        if !document.skins.iter().any(|skin| skin.id == id) {
            break id;
        }
        ordinal += 1;
    };
    copy.name = format!("{} copy", copy.name);
    let id = copy.id.clone();
    document.skins.push(copy);
    session
        .replace_document_atomic(document)
        .map_err(|error| format!("{error:?}"))?;
    session.select(Some(crate::radial::authoring::StableSelection::Skin(
        id.clone(),
    )));
    Ok(id)
}

pub(super) fn reset_scope(
    session: &mut RadialAuthoringSession,
    scope: &StyleScope,
) -> Result<(), String> {
    let mut document = (*session.draft).clone();
    let empty = match scope {
        StyleScope::Ring { .. } => {
            serde_json::to_value(crate::radial::model::RingStyleLayer::default())
        }
        StyleScope::Cell { .. } => {
            serde_json::to_value(crate::radial::model::CellStyleLayer::default())
        }
        _ => serde_json::to_value(crate::radial::model::StyleOverrides::default()),
    }
    .map_err(|error| error.to_string())?;
    set_scope_json(&mut document, scope, empty)?;
    session
        .replace_document_atomic(document)
        .map_err(|error| format!("{error:?}"))
}

pub(super) fn delete_skin(
    session: &mut RadialAuthoringSession,
    id: &SkinId,
) -> Result<(), ReferenceImpact> {
    let impact = references_to_skin(&session.draft, id);
    if !impact.paths.is_empty() {
        return Err(impact);
    }
    let mut document = (*session.draft).clone();
    document.skins.retain(|skin| &skin.id != id);
    session
        .replace_document_atomic(document)
        .map_err(|_| ReferenceImpact {
            paths: vec!["authoring session rejected deletion".into()],
        })
}

pub(super) fn rename_skin(
    session: &mut RadialAuthoringSession,
    id: &SkinId,
    name: String,
    phase: crate::radial::authoring::EditPhase,
) -> Result<(), String> {
    session
        .mutate(
            crate::radial::authoring::DocumentMutation::RenameSkin {
                id: id.clone(),
                name,
            },
            Some(crate::radial::authoring::EditKey {
                entity: format!("skin:{id}"),
                field: "name".into(),
            }),
            phase,
        )
        .map_err(|error| format!("{error:?}"))
}

pub(super) fn rows(document: &RadialDocument, scope: &StyleScope) -> Result<Vec<StyleRow>, String> {
    let current = scope_json(document, scope)?;
    let fallback = serde_json::to_value(&ApplicationStyleLayer::fallback().values)
        .map_err(|error| error.to_string())?;
    let provenance = provenance(document, scope);
    let object = current.as_object().ok_or("style layer is not an object")?;
    let mut rows = Vec::new();
    for (section, fields) in object {
        let Some(fields) = fields.as_object() else {
            continue;
        };
        for (field, current) in fields {
            let inherited = fallback
                .get(section)
                .and_then(|section| section.get(field))
                .cloned()
                .unwrap_or(serde_json::Value::String("inherit".into()));
            rows.push(StyleRow {
                section: section.clone(),
                field: field.clone(),
                current: current.clone(),
                inherited,
                source: style_field(section, field).and_then(|field| provenance(field)),
            });
        }
    }
    Ok(rows)
}

pub(super) fn set_override(
    session: &mut RadialAuthoringSession,
    scope: &StyleScope,
    section: &str,
    field: &str,
    value: serde_json::Value,
) -> Result<(), String> {
    let mut document = (*session.draft).clone();
    let mut layer = scope_json(&document, scope)?;
    let slot = layer
        .get_mut(section)
        .and_then(|section| section.get_mut(field))
        .ok_or_else(|| format!("{section}.{field} is not legal at this scope"))?;
    *slot = value;
    set_scope_json(&mut document, scope, layer)?;
    session
        .replace_document_atomic(document)
        .map_err(|error| format!("{error:?}"))
}

pub(super) fn set_override_edit(
    session: &mut RadialAuthoringSession,
    scope: &StyleScope,
    section: &str,
    field: &str,
    value: serde_json::Value,
    phase: crate::radial::authoring::EditPhase,
) -> Result<(), String> {
    let mut document = (*session.draft).clone();
    let mut layer = scope_json(&document, scope)?;
    let slot = layer
        .get_mut(section)
        .and_then(|section| section.get_mut(field))
        .ok_or_else(|| format!("{section}.{field} is not legal at this scope"))?;
    *slot = value;
    set_scope_json(&mut document, scope, layer)?;
    session
        .replace_document_edit(
            document,
            crate::radial::authoring::EditKey {
                entity: format!("style:{scope:?}"),
                field: format!("{section}.{field}"),
            },
            phase,
        )
        .map_err(|error| format!("{error:?}"))
}

pub(super) fn set_media_override(
    session: &mut RadialAuthoringSession,
    scope: &StyleScope,
    section: &str,
    field: &str,
    choice: &super::asset_picker::ResourceChoice,
) -> Result<(), String> {
    let mut document = (*session.draft).clone();
    let mut layer = scope_json(&document, scope)?;
    let slot = layer
        .get_mut(section)
        .and_then(|section| section.get_mut(field))
        .ok_or_else(|| format!("{section}.{field} is not legal at this scope"))?;
    *slot = serde_json::to_value(Override::Value(choice.media_reference()))
        .map_err(|error| error.to_string())?;
    set_scope_json(&mut document, scope, layer)?;
    let mut mutations = session.pending_assets.clone();
    if let super::asset_picker::ResourceChoice::Managed { record, .. } = choice
        && !document.assets.iter().any(|asset| asset.id == record.id)
    {
        document.assets.push(record.clone());
    }
    choice.stage(&mut mutations);
    session
        .replace_document_and_assets_atomic(document, mutations)
        .map_err(|error| format!("{error:?}"))
}

pub(super) fn inherit_value() -> serde_json::Value {
    serde_json::to_value(Override::<serde_json::Value>::Inherit).unwrap_or_default()
}

pub(super) fn clear_value() -> serde_json::Value {
    serde_json::to_value(Override::<serde_json::Value>::Clear).unwrap_or_default()
}

pub(super) fn is_media_field(section: &str, field: &str) -> bool {
    matches!(
        style_field(section, field),
        Some(
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
                | StyleField::SoundOnSubmenuClose
        )
    )
}

fn scope_json(document: &RadialDocument, scope: &StyleScope) -> Result<serde_json::Value, String> {
    match scope {
        StyleScope::UserDefaults => serde_json::to_value(&document.user_style_defaults.values),
        StyleScope::Skin(id) => serde_json::to_value(
            &document
                .skins
                .iter()
                .find(|skin| &skin.id == id)
                .ok_or("skin missing")?
                .style
                .values,
        ),
        StyleScope::Menu(id) => serde_json::to_value(
            &document
                .menus
                .iter()
                .find(|menu| &menu.id == id)
                .ok_or("menu missing")?
                .style
                .values,
        ),
        StyleScope::Ring { menu_id, ring_id } => serde_json::to_value(
            &document
                .menus
                .iter()
                .find(|menu| &menu.id == menu_id)
                .and_then(|menu| menu.rings.iter().find(|ring| &ring.id == ring_id))
                .ok_or("ring missing")?
                .style,
        ),
        StyleScope::Cell {
            menu_id,
            ring_id,
            cell_id,
        } => serde_json::to_value(
            &document
                .menus
                .iter()
                .find(|menu| &menu.id == menu_id)
                .and_then(|menu| menu.rings.iter().find(|ring| &ring.id == ring_id))
                .and_then(|ring| ring.cells.iter().find(|cell| &cell.id == cell_id))
                .ok_or("cell missing")?
                .style,
        ),
    }
    .map_err(|error| error.to_string())
}

fn set_scope_json(
    document: &mut RadialDocument,
    scope: &StyleScope,
    value: serde_json::Value,
) -> Result<(), String> {
    match scope {
        StyleScope::UserDefaults => {
            document.user_style_defaults.values =
                serde_json::from_value(value).map_err(|error| error.to_string())?;
        }
        StyleScope::Skin(id) => {
            document
                .skins
                .iter_mut()
                .find(|skin| &skin.id == id)
                .ok_or("skin missing")?
                .style
                .values = serde_json::from_value(value).map_err(|error| error.to_string())?;
        }
        StyleScope::Menu(id) => {
            document
                .menus
                .iter_mut()
                .find(|menu| &menu.id == id)
                .ok_or("menu missing")?
                .style
                .values = serde_json::from_value(value).map_err(|error| error.to_string())?;
        }
        StyleScope::Ring { menu_id, ring_id } => {
            document
                .menus
                .iter_mut()
                .find(|menu| &menu.id == menu_id)
                .and_then(|menu| menu.rings.iter_mut().find(|ring| &ring.id == ring_id))
                .ok_or("ring missing")?
                .style = serde_json::from_value(value).map_err(|error| error.to_string())?;
        }
        StyleScope::Cell {
            menu_id,
            ring_id,
            cell_id,
        } => {
            document
                .menus
                .iter_mut()
                .find(|menu| &menu.id == menu_id)
                .and_then(|menu| menu.rings.iter_mut().find(|ring| &ring.id == ring_id))
                .and_then(|ring| ring.cells.iter_mut().find(|cell| &cell.id == cell_id))
                .ok_or("cell missing")?
                .style = serde_json::from_value(value).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn provenance<'a>(
    document: &'a RadialDocument,
    scope: &'a StyleScope,
) -> Box<dyn Fn(StyleField) -> Option<StyleSource> + 'a> {
    let menu_id = match scope {
        StyleScope::Menu(id)
        | StyleScope::Ring { menu_id: id, .. }
        | StyleScope::Cell { menu_id: id, .. } => Some(id),
        StyleScope::Skin(skin_id) => document
            .menus
            .iter()
            .find(|menu| &menu.skin_id == skin_id)
            .map(|menu| &menu.id),
        StyleScope::UserDefaults => document.menus.first().map(|menu| &menu.id),
    };
    let tree = menu_id
        .and_then(|id| document.menus.iter().find(|menu| &menu.id == id))
        .and_then(|menu| compile_menu_tree(document, menu).ok());
    Box::new(move |field| match (scope, &tree) {
        (StyleScope::Menu(_) | StyleScope::Skin(_) | StyleScope::UserDefaults, Some(tree)) => {
            tree.menu.source(field).cloned()
        }
        (StyleScope::Ring { ring_id, .. }, Some(tree)) => {
            tree.rings.get(ring_id)?.source(field).cloned()
        }
        (StyleScope::Cell { cell_id, .. }, Some(tree)) => {
            tree.cells.get(cell_id)?.source(field).cloned()
        }
        _ => None,
    })
}

macro_rules! paths {
    ($( $section:ident . $field:ident => $variant:ident ),+ $(,)?) => {
        fn style_field(section: &str, field: &str) -> Option<StyleField> {
            match (section, field) {
                $((stringify!($section), stringify!($field)) => Some(StyleField::$variant),)+
                _ => None,
            }
        }
    };
}

paths!(
    images.item_glow => ItemGlow, images.menu_outer_rim => MenuOuterRim,
    images.menu_background => MenuBackground, images.item_background => ItemBackground,
    images.item_foreground => ItemForeground, images.item_shadow => ItemShadow,
    images.menu_foreground => MenuForeground, images.center_background => CenterBackground,
    images.center_image => CenterImage, images.submenu_indicator => SubmenuIndicator,
    images.item_glow_opacity => ItemGlowOpacity, images.menu_outer_rim_opacity => MenuOuterRimOpacity,
    images.menu_background_opacity => MenuBackgroundOpacity, images.item_background_opacity => ItemBackgroundOpacity,
    images.item_foreground_opacity => ItemForegroundOpacity, images.item_shadow_opacity => ItemShadowOpacity,
    images.menu_foreground_opacity => MenuForegroundOpacity, images.center_background_opacity => CenterBackgroundOpacity,
    images.center_image_opacity => CenterImageOpacity, images.submenu_indicator_opacity => SubmenuIndicatorOpacity,
    images.icon_opacity => IconOpacity,
    geometry.menu_scale => MenuScale, geometry.item_size => ItemSize, geometry.radius_scale => RadiusScale,
    geometry.center_size => CenterSize, geometry.center_image_scale => CenterImageScale,
    geometry.item_image_scale => ItemImageScale, geometry.item_image_y_ratio => ItemImageYRatio,
    geometry.item_background_scale => ItemBackgroundScale, geometry.item_foreground_scale => ItemForegroundScale,
    geometry.item_shadow_scale => ItemShadowScale, geometry.menu_background_scale => MenuBackgroundScale,
    geometry.menu_foreground_scale => MenuForegroundScale, geometry.center_background_scale => CenterBackgroundScale,
    geometry.submenu_indicator_size => SubmenuIndicatorSize, geometry.submenu_indicator_y_ratio => SubmenuIndicatorYRatio,
    geometry.outer_ring_margin => OuterRingMargin, geometry.outer_rim_width => OuterRimWidth,
    geometry.item_background_on_center => ItemBackgroundOnCenter, geometry.item_background_on_items => ItemBackgroundOnItems,
    text.visible => TextVisible, text.submenu_indicator_text => SubmenuIndicatorText,
    text.font_family => FontFamily, text.font_size => FontSize, text.color => TextColor,
    text.bold => Bold, text.italic => Italic, text.underline => Underline, text.strikeout => Strikeout,
    text.shadow_enabled => TextShadowEnabled, text.shadow_color => TextShadowColor,
    text.shadow_offset => TextShadowOffset, text.text_box_scale => TextBoxScale,
    text.vertical_ratio => TextVerticalRatio,
    effects.glow_enabled => GlowEnabled, effects.tooltip_mode => TooltipMode,
    effects.menu_shadow_width => MenuShadowWidth, effects.menu_shadow_inner_color => MenuShadowInnerColor,
    effects.menu_shadow_outer_color => MenuShadowOuterColor,
    quality.text => TextQuality, quality.shape => ShapeQuality, quality.interpolation => InterpolationQuality,
    sounds.on_show => SoundOnShow, sounds.on_close => SoundOnClose, sounds.on_select => SoundOnSelect,
    sounds.on_submenu_show => SoundOnSubmenuShow, sounds.on_submenu_close => SoundOnSubmenuClose,
    window.always_on_top => AlwaysOnTop, window.activate_on_show => ActivateOnShow,
    window.fill_center_hit_zone => FillCenterHitZone, window.fill_item_hit_zones => FillItemHitZones,
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::radial::authoring::{AuthoringSnapshot, DiskSha256};

    #[test]
    fn every_full_scope_field_roundtrips_inherit_set_clear_and_reports_provenance() {
        let document = RadialDocument::starter();
        let revision = document.revision;
        let skin = document.skins[0].id.clone();
        let mut session = RadialAuthoringSession::new(AuthoringSnapshot {
            document: std::sync::Arc::new(document),
            revision,
            disk_sha256: DiskSha256("test".into()),
        });
        let scope = StyleScope::Skin(skin);
        let initial = rows(&session.draft, &scope).unwrap();
        assert_eq!(initial.len(), 71);
        for row in initial {
            set_override(
                &mut session,
                &scope,
                &row.section,
                &row.field,
                row.inherited.clone(),
            )
            .unwrap();
            set_override(
                &mut session,
                &scope,
                &row.section,
                &row.field,
                clear_value(),
            )
            .unwrap();
            set_override(
                &mut session,
                &scope,
                &row.section,
                &row.field,
                inherit_value(),
            )
            .unwrap();
        }
        let menu_scope = StyleScope::Menu(session.draft.menus[0].id.clone());
        assert!(
            rows(&session.draft, &menu_scope)
                .unwrap()
                .iter()
                .all(|row| row.source.is_some())
        );
        let encoded = serde_json::to_vec(&*session.draft).unwrap();
        let decoded: RadialDocument = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded, *session.draft);
    }

    #[test]
    fn every_style_field_has_an_explicit_ergonomic_control() {
        let document = RadialDocument::starter();
        let scope = StyleScope::Skin(document.skins[0].id.clone());
        let fields: std::collections::BTreeMap<_, _> = rows(&document, &scope)
            .unwrap()
            .into_iter()
            .map(|row| {
                let kind = control_kind(&row.section, &row.field)
                    .unwrap_or_else(|| panic!("missing control for {}.{}", row.section, row.field));
                ((row.section, row.field), kind)
            })
            .collect();
        assert_eq!(fields.len(), 71);
        assert_eq!(
            fields.get(&("text".into(), "bold".into())),
            Some(&StyleControlKind::Toggle)
        );
        assert_eq!(
            fields.get(&("text".into(), "color".into())),
            Some(&StyleControlKind::Color)
        );
        assert_eq!(
            fields.get(&("text".into(), "font_family".into())),
            Some(&StyleControlKind::Font)
        );
        assert_eq!(
            fields.get(&("text".into(), "shadow_offset".into())),
            Some(&StyleControlKind::Offset)
        );
        assert_eq!(
            fields.get(&("quality".into(), "shape".into())),
            Some(&StyleControlKind::Quality)
        );
        assert_eq!(
            fields.get(&("sounds".into(), "on_show".into())),
            Some(&StyleControlKind::Resource)
        );
        assert_eq!(
            fields.get(&("images".into(), "icon_opacity".into())),
            Some(&StyleControlKind::Opacity)
        );
    }

    #[test]
    fn referenced_skin_delete_is_blocked_with_usage_paths() {
        let document = RadialDocument::starter();
        let revision = document.revision;
        let skin = document.skins[0].id.clone();
        let mut session = RadialAuthoringSession::new(AuthoringSnapshot {
            document: std::sync::Arc::new(document),
            revision,
            disk_sha256: DiskSha256("test".into()),
        });
        let impact = delete_skin(&mut session, &skin).unwrap_err();
        assert!(impact.paths.iter().any(|path| path.contains("skin_id")));
        assert!(
            session
                .draft
                .skins
                .iter()
                .any(|candidate| candidate.id == skin)
        );
    }

    #[test]
    fn icon_resource_index_roundtrips_through_typed_style_control() {
        let document = RadialDocument::starter();
        let skin = document.skins[0].id.clone();
        let mut session = RadialAuthoringSession::new(AuthoringSnapshot::new(
            std::sync::Arc::new(document),
            "test",
        ));
        let scope = StyleScope::Skin(skin);
        let media = crate::radial::model::MediaReference::IconResource {
            path: "shell32.dll".into(),
            index: 42,
        };
        set_override(
            &mut session,
            &scope,
            "images",
            "center_image",
            with_payload(serde_json::to_value(media.clone()).unwrap()),
        )
        .unwrap();
        let row = rows(&session.draft, &scope)
            .unwrap()
            .into_iter()
            .find(|row| row.section == "images" && row.field == "center_image")
            .unwrap();
        let decoded: crate::radial::model::Override<crate::radial::model::MediaReference> =
            serde_json::from_value(row.current).unwrap();
        assert_eq!(decoded, crate::radial::model::Override::Value(media));
    }
}
