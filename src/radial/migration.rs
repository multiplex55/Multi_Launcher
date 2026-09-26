use super::model::{CURRENT_SCHEMA_VERSION, RadialDocument};
use super::validation::{ValidationErrors, validate};

const V2_SCHEMA_VERSION: u32 = 2;

#[derive(Debug)]
pub enum DocumentDecodeError {
    Malformed(serde_json::Error),
    UnsupportedNewerVersion { found: u64, supported: u32 },
    Validation(ValidationErrors),
}

#[derive(Clone, Debug, PartialEq)]
pub struct DecodedDocument {
    pub document: RadialDocument,
    pub migrated_from: Option<u32>,
}

/// Decode, migrate, and validate without touching the source bytes. Both the
/// runtime store and persistence health probe use this exact compatibility boundary.
pub fn decode_document(bytes: &[u8]) -> Result<DecodedDocument, DocumentDecodeError> {
    let mut value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(DocumentDecodeError::Malformed)?;
    let version: u64 = serde_json::from_value(
        value
            .get("schema_version")
            .cloned()
            .unwrap_or(serde_json::Value::Null),
    )
    .map_err(DocumentDecodeError::Malformed)?;
    if version > CURRENT_SCHEMA_VERSION as u64 {
        return Err(DocumentDecodeError::UnsupportedNewerVersion {
            found: version,
            supported: CURRENT_SCHEMA_VERSION,
        });
    }
    let migrated_from = match version {
        1 => {
            migrate_v1(&mut value);
            migrate_v2_to_v3(&mut value);
            Some(1)
        }
        2 => {
            migrate_v2_to_v3(&mut value);
            Some(2)
        }
        _ => None,
    };
    let document = serde_json::from_value(value).map_err(DocumentDecodeError::Malformed)?;
    validate(&document).map_err(DocumentDecodeError::Validation)?;
    Ok(DecodedDocument {
        document,
        migrated_from,
    })
}

fn migrate_v1(value: &mut serde_json::Value) {
    value["schema_version"] = serde_json::Value::from(V2_SCHEMA_VERSION);
    let Some(document) = value.as_object_mut() else {
        return;
    };
    if let Some(menus) = document
        .get_mut("menus")
        .and_then(serde_json::Value::as_array_mut)
    {
        for menu in menus {
            let Some(rings) = menu
                .get_mut("rings")
                .and_then(serde_json::Value::as_array_mut)
            else {
                continue;
            };
            for ring in rings {
                let Some(cells) = ring
                    .get_mut("cells")
                    .and_then(serde_json::Value::as_array_mut)
                else {
                    continue;
                };
                for cell in cells {
                    if let Some(icon) = cell.get_mut("icon") {
                        migrate_media_override(icon);
                    }
                }
            }
        }
    }
    let Some(skins) = document
        .get_mut("skins")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return;
    };
    for skin in skins {
        let Some(object) = skin.as_object_mut() else {
            continue;
        };
        let scale = object.remove("scale");
        let mut glow = object.remove("enable_glow");
        if let Some(glow) = glow.as_mut() {
            normalize_override_tag(glow);
        }
        let mut center = object.remove("center_image");
        if let Some(center) = center.as_mut() {
            migrate_media_override(center);
        }
        let mut values = serde_json::Map::new();
        if let Some(scale) = scale {
            values.insert(
                "geometry".into(),
                serde_json::json!({ "menu_scale": { "value": scale } }),
            );
        }
        if let Some(glow) = glow {
            values.insert(
                "effects".into(),
                serde_json::json!({ "glow_enabled": glow }),
            );
        }
        if let Some(center) = center {
            values.insert(
                "images".into(),
                serde_json::json!({ "center_image": center }),
            );
        }
        object.insert("style".into(), serde_json::json!({ "values": values }));
    }
}

fn migrate_v2_to_v3(value: &mut serde_json::Value) {
    value["schema_version"] = serde_json::Value::from(CURRENT_SCHEMA_VERSION);
}

fn migrate_media_override(value: &mut serde_json::Value) {
    let Some(raw) = value
        .get("Value")
        .or_else(|| value.get("value"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
    else {
        normalize_override_tag(value);
        return;
    };
    let media = if let Some((path, index)) = parse_icon_resource(&raw) {
        serde_json::json!({ "kind": "icon_resource", "path": path, "index": index })
    } else if raw
        .chars()
        .any(|character| matches!(character, '\\' | '/' | ':'))
    {
        serde_json::json!({ "kind": "external_file", "path": raw })
    } else {
        serde_json::json!({ "kind": "search_path", "file_name": raw })
    };
    *value = serde_json::json!({ "value": media });
}

fn normalize_override_tag(value: &mut serde_json::Value) {
    if let Some(tag) = value.as_str() {
        if tag.eq_ignore_ascii_case("inherit") {
            *value = serde_json::Value::String("inherit".into());
        } else if tag.eq_ignore_ascii_case("clear") {
            *value = serde_json::Value::String("clear".into());
        }
        return;
    }
    let Some(object) = value.as_object_mut() else {
        return;
    };
    if let Some(payload) = object.remove("Value") {
        object.insert("value".into(), payload);
    }
}

fn parse_icon_resource(raw: &str) -> Option<(&str, u32)> {
    let (path, suffix) = raw.rsplit_once("|icon")?;
    let index = suffix.parse().ok()?;
    Some((path, index))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::radial::model::{MediaReference, Override};

    #[test]
    fn v1_media_and_skin_fields_migrate_without_losing_explicit_values() {
        let mut value = serde_json::to_value(RadialDocument::starter()).unwrap();
        value["schema_version"] = 1.into();
        let document = value.as_object_mut().unwrap();
        document.remove("user_style_defaults");
        document.remove("media_search_roots");
        document.remove("assets");
        for menu in document["menus"].as_array_mut().unwrap() {
            menu.as_object_mut().unwrap().remove("style");
            for ring in menu["rings"].as_array_mut().unwrap() {
                ring.as_object_mut().unwrap().remove("style");
                for cell in ring["cells"].as_array_mut().unwrap() {
                    let cell = cell.as_object_mut().unwrap();
                    cell.remove("tooltip");
                    cell.remove("style");
                    cell.remove("shortcuts");
                    cell.remove("hotstrings");
                }
            }
        }
        let skin = &mut value["skins"][0];
        skin.as_object_mut().unwrap().remove("style");
        skin["scale"] = serde_json::json!(0.25);
        skin["enable_glow"] = serde_json::json!({ "Value": false });
        skin["center_image"] = serde_json::json!({ "Value": "CenterImage.png" });
        value["menus"][0]["rings"][0]["cells"][0]["icon"] =
            serde_json::json!({ "Value": "C:\\icons.dll|icon19" });

        let decoded = decode_document(&serde_json::to_vec(&value).unwrap()).unwrap();
        assert_eq!(decoded.migrated_from, Some(1));
        assert_eq!(decoded.document.schema_version, CURRENT_SCHEMA_VERSION);
        let style = &decoded.document.skins[0].style.values;
        assert_eq!(style.geometry.menu_scale, Override::Value(0.25));
        assert_eq!(style.effects.glow_enabled, Override::Value(false));
        assert!(matches!(
            style.images.center_image,
            Override::Value(MediaReference::SearchPath { ref file_name })
                if file_name == "CenterImage.png"
        ));
        assert!(matches!(
            decoded.document.menus[0].rings[0].cells[0].icon,
            Override::Value(MediaReference::IconResource { index: 19, .. })
        ));
        assert!(decoded.document.assets.is_empty());
        assert!(
            decoded
                .document
                .media_search_roots
                .image_directories
                .is_empty()
        );
    }

    #[test]
    fn v2_documents_advance_to_v3_without_changing_authored_values() {
        let mut document = RadialDocument::starter();
        document.schema_version = V2_SCHEMA_VERSION;
        document.menus[0].name = "  Keep authored text  ".into();
        let input = serde_json::to_vec(&document).unwrap();

        let decoded = decode_document(&input).unwrap();

        assert_eq!(decoded.migrated_from, Some(2));
        assert_eq!(decoded.document.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(decoded.document.menus[0].name, "  Keep authored text  ");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&input).unwrap()["schema_version"],
            V2_SCHEMA_VERSION
        );
    }

    #[test]
    fn current_schema_rejects_out_of_scope_style_fields() {
        let mut value = serde_json::to_value(RadialDocument::starter()).unwrap();
        value["menus"][0]["rings"][0]["cells"][0]["style"]["images"]["menu_background"] =
            serde_json::json!("Clear");
        assert!(matches!(
            decode_document(&serde_json::to_vec(&value).unwrap()),
            Err(DocumentDecodeError::Malformed(_))
        ));
        let mut value = serde_json::to_value(RadialDocument::starter()).unwrap();
        value["skins"][0]["hIcon"] = 42.into();
        assert!(matches!(
            decode_document(&serde_json::to_vec(&value).unwrap()),
            Err(DocumentDecodeError::Malformed(_))
        ));
    }

    #[test]
    fn newer_and_malformed_documents_are_distinct() {
        assert!(matches!(
            decode_document(br#"{"schema_version":999}"#),
            Err(DocumentDecodeError::UnsupportedNewerVersion { found: 999, .. })
        ));
        assert!(matches!(
            decode_document(b"{"),
            Err(DocumentDecodeError::Malformed(_))
        ));
    }
}
