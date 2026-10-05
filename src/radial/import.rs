//! Side-effect-free legacy import inspection.
//!
//! This module deliberately stops at an immutable preview. It accepts caller-owned
//! bytes and paths, parses only data, and never creates directories, copies media,
//! evaluates AutoHotkey, or mutates a [`RadialDocument`](super::model::RadialDocument).

use super::compatibility::{
    CompatibilityClassification, CompatibilitySource, FIELD_REGISTRY, compatibility_for,
};
use super::model::{
    AfterActionPolicy, AssetId, AssetRecord, CellContent, CellDefinition, CellId, CellStyleLayer,
    ClickGesture, ColorRgba, Control, ControlClickBinding, DynamicSource, HotstringId,
    ItemHotstring, ItemShortcut, MediaKind, MediaReference, MenuId, Override, RadialDocument,
    RenderingQuality, RingId, ShortcutId, SkinId, TooltipMode, TriggerScope,
};
use super::package::{
    DOCUMENT_FILE, IdRemap, ImportPlan, PACKAGE_VERSION, PackageFileRecord, PackageManifest,
    sha256_hex,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

pub const FAMILIAR_SKIN_FILES: [&str; 6] = [
    "ItemBack.png",
    "ItemGlow.png",
    "MenuBack.png",
    "MenuOuterRim.png",
    "CenterImage.png",
    "SubmenuIndicator.png",
];

/// Read-only members inspected from the tracked RM4 reference archive. Sizes
/// make accidental substitution of a different fixture visible without making
/// runtime import depend on the archive or redistributing any member.
pub const SUPPLIED_RM4_DEFINITIONS: [(&str, u64); 5] = [
    ("Skins/Anectra's target/Skin definition.txt", 869),
    ("Skins/Cell/Skin definition.txt", 834),
    ("Skins/Merlock's device/Skin definition.txt", 826),
    ("Skins/Metal plate/Skin definition.txt", 913),
    ("Skins/Orb/Skin definition.txt", 1_304),
];

pub const SUPPLIED_RM4_REPRESENTATIVE_MEDIA: [(&str, u64, (u32, u32)); 5] = [
    ("Skins/Anectra's target/ItemBack.png", 5_454, (76, 76)),
    ("Skins/Cell/MenuOuterRim.png", 12_266, (600, 600)),
    ("Skins/Merlock's device/MenuBack.png", 116_365, (305, 306)),
    ("Skins/Metal plate/ItemGlow.png", 14_636, (122, 122)),
    ("Skins/Orb/MenuFore.png", 11_965, (543, 543)),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportEvidence {
    SuppliedReference,
    SyntheticFixture,
    UserSelected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiscoveredRole {
    RadifyPreferences,
    RadifySettings,
    Rm4SkinDefinition,
    FamiliarSkinMedia,
    Sound,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportInput<'a> {
    pub relative_path: &'a str,
    pub bytes: &'a [u8],
    pub evidence: ImportEvidence,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveredImportFile {
    pub relative_path: String,
    pub role: DiscoveredRole,
    pub evidence: ImportEvidence,
    pub portable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportDestination {
    pub menu_id: MenuId,
    pub skin_id: SkinId,
    pub display_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportProvenance {
    pub source: CompatibilitySource,
    pub dialect: ImportDialect,
    pub file: String,
    pub line: Option<usize>,
    pub field: String,
    pub evidence: ImportEvidence,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportDialect {
    RadifySettings,
    Rm4Definition,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SizeExpression {
    FactorOffset(f64),
    Additive(f64),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ImportedValue {
    Clear,
    Bool(bool),
    Number(f64),
    Text(String),
    MediaFilename(String),
    SizeExpression(SizeExpression),
    Control(Control),
    Structured(Value),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImportMapping {
    pub destination_field: String,
    pub value: ImportedValue,
    pub classification: CompatibilityClassification,
    pub provenance: ImportProvenance,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportWarningKind {
    MalformedData,
    UnknownField,
    UnsupportedExpression,
    IgnoredExecutableBehavior,
    Collision,
    MissingRequiredAsset,
    NonPortablePath,
    IncompatibleField,
    NotApplicable,
    MissingSourceEvidence,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportWarning {
    pub kind: ImportWarningKind,
    pub file: String,
    pub line: Option<usize>,
    pub field: Option<String>,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ImportApplicationPolicy {
    Applied,
    Diagnosed(&'static str),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ApplyOutcome {
    Changed,
    AlreadyEqual,
    Invalid(String),
    MissingAsset(String),
    ExplicitlyDiagnosed(String),
}

fn application_policy(source: CompatibilitySource, field: &str) -> Option<ImportApplicationPolicy> {
    let group = compatibility_for(source, field)?;
    if !matches!(
        group.classification,
        CompatibilityClassification::Native | CompatibilityClassification::Translated
    ) {
        return None;
    }
    Some(match (source, field) {
        (CompatibilitySource::Radify, "Submenu" | "SubmenuOptions") => {
            ImportApplicationPolicy::Diagnosed(
                "requires a complete legacy menu graph; a skin/settings preview cannot safely manufacture a dangling submenu reference",
            )
        }
        _ => ImportApplicationPolicy::Applied,
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct RadifySettingsSnapshot {
    pub defaults: BTreeMap<String, Value>,
    pub skins: BTreeMap<String, BTreeMap<String, Value>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImportPreview {
    pub source: CompatibilitySource,
    pub discovered_files: Vec<DiscoveredImportFile>,
    pub destination: ImportDestination,
    /// Exact legacy skin scope selected by the caller. It deliberately does
    /// not derive from the new destination/display name.
    pub source_skin: Option<String>,
    pub radify_settings: Option<RadifySettingsSnapshot>,
    pub mappings: Vec<ImportMapping>,
    pub warnings: Vec<ImportWarning>,
    /// Every approved compatibility field is accounted for even when absent
    /// from the particular input. This makes importer/registry drift testable.
    pub compatibility_coverage: BTreeMap<String, CompatibilityClassification>,
    /// Fully typed, immutable candidate consumed by the transactional store API.
    pub definition: RadialDocument,
    /// Content-addressed package entries; preview itself performs no writes.
    pub asset_bytes: BTreeMap<String, Vec<u8>>,
}

impl ImportPreview {
    pub fn apply_plan(&self) -> ImportPlan {
        let document_bytes = serde_json::to_vec_pretty(&self.definition).unwrap_or_default();
        let mut files = vec![PackageFileRecord {
            path: DOCUMENT_FILE.into(),
            sha256: sha256_hex(&document_bytes),
            byte_len: document_bytes.len() as u64,
        }];
        files.extend(
            self.asset_bytes
                .iter()
                .map(|(path, bytes)| PackageFileRecord {
                    path: path.clone(),
                    sha256: sha256_hex(bytes),
                    byte_len: bytes.len() as u64,
                }),
        );
        ImportPlan {
            manifest: PackageManifest {
                package_version: PACKAGE_VERSION,
                document_path: DOCUMENT_FILE.into(),
                root_menu_ids: vec![self.definition.default_menu_id.clone()],
                payload: crate::radial::package::PackagePayloadKind::MenuGraph,
                files,
                notices: Vec::new(),
            },
            document: self.definition.clone(),
            assets: self.asset_bytes.clone(),
            remap: IdRemap::default(),
        }
    }
}

pub fn preview_radify(
    inputs: &[ImportInput<'_>],
    requested_name: &str,
    existing_menu_ids: &BTreeSet<String>,
    existing_skin_ids: &BTreeSet<String>,
) -> ImportPreview {
    preview_radify_skin(
        inputs,
        None,
        requested_name,
        existing_menu_ids,
        existing_skin_ids,
    )
}

pub fn preview_radify_skin(
    inputs: &[ImportInput<'_>],
    source_skin: Option<&str>,
    requested_name: &str,
    existing_menu_ids: &BTreeSet<String>,
    existing_skin_ids: &BTreeSet<String>,
) -> ImportPreview {
    let destination = planned_destination(requested_name, existing_menu_ids, existing_skin_ids);
    let (discovered_files, mut warnings) = discover(inputs);
    let mut settings = None;
    let mut mappings = Vec::new();

    for input in inputs {
        let name = file_name(input.relative_path);
        if name.eq_ignore_ascii_case("Preferences.json")
            || name.eq_ignore_ascii_case("Settings.json")
        {
            match parse_radify_settings(input.bytes) {
                Ok(snapshot) => {
                    append_radify_mappings(&snapshot, input, &mut mappings, &mut warnings);
                    if name.eq_ignore_ascii_case("Preferences.json") {
                        settings = Some(snapshot);
                    }
                }
                Err(message) => warnings.push(warning(
                    ImportWarningKind::MalformedData,
                    input.relative_path,
                    None,
                    None,
                    message,
                )),
            }
        }
    }

    if inputs
        .iter()
        .any(|input| input.evidence == ImportEvidence::SuppliedReference)
    {
        for (filename, description) in [
            ("Preferences.json", "generated Radify preferences"),
            ("Settings.json", "generated Radify settings"),
        ] {
            if !inputs
                .iter()
                .any(|input| file_name(input.relative_path).eq_ignore_ascii_case(filename))
            {
                warnings.push(warning(
                    ImportWarningKind::MissingSourceEvidence,
                    filename,
                    None,
                    None,
                    format!("the supplied reference archive contains no {description}; parser fixtures for this shape must be labelled synthetic"),
                ));
            }
        }
        if !inputs.iter().any(|input| {
            file_name(input.relative_path)
                .to_ascii_lowercase()
                .ends_with(".wav")
        }) {
            warnings.push(warning(
                ImportWarningKind::MissingSourceEvidence,
                "sounds",
                None,
                None,
                "the supplied Radify reference archive contains no sound fixtures; synthetic WAV evidence must be labelled synthetic",
            ));
        }
    }

    if let Some(snapshot) = &settings {
        for skin_name in snapshot.skins.keys() {
            let has_item_back = inputs.iter().any(|input| {
                parent_key(input.relative_path)
                    .rsplit('/')
                    .next()
                    .is_some_and(|folder| folder.eq_ignore_ascii_case(skin_name))
                    && file_name(input.relative_path).eq_ignore_ascii_case("ItemBack.png")
            });
            if !has_item_back {
                warnings.push(warning(
                    ImportWarningKind::MissingRequiredAsset,
                    skin_name,
                    None,
                    Some("ItemBack"),
                    "named imported Radify skin is missing required ItemBack.png; native vector skins do not have this requirement",
                ));
            }
        }
    }

    add_case_collisions(inputs, &mut warnings);
    add_id_collision_warnings(
        requested_name,
        existing_menu_ids,
        existing_skin_ids,
        &mut warnings,
    );
    let (definition, asset_bytes, apply_warnings) =
        build_import_definition(&destination, source_skin, inputs, &mappings);
    warnings.extend(apply_warnings);
    ImportPreview {
        source: CompatibilitySource::Radify,
        discovered_files,
        destination,
        source_skin: source_skin.map(str::to_owned),
        radify_settings: settings,
        mappings,
        warnings,
        compatibility_coverage: registry_coverage(CompatibilitySource::Radify),
        definition,
        asset_bytes,
    }
}

pub fn preview_rm4(
    inputs: &[ImportInput<'_>],
    requested_name: &str,
    existing_menu_ids: &BTreeSet<String>,
    existing_skin_ids: &BTreeSet<String>,
) -> ImportPreview {
    let destination = planned_destination(requested_name, existing_menu_ids, existing_skin_ids);
    let (discovered_files, mut warnings) = discover(inputs);
    let mut mappings = Vec::new();
    let mut named_skins = BTreeSet::new();

    for input in inputs {
        if file_name(input.relative_path).eq_ignore_ascii_case("Skin definition.txt") {
            let parsed = parse_rm4_definition(input);
            for mapping in parsed.mappings {
                if mapping.provenance.field == "SkinName" {
                    if let ImportedValue::Text(name) = &mapping.value {
                        if !name.trim().is_empty() {
                            named_skins.insert(parent_key(input.relative_path));
                        }
                    }
                }
                mappings.push(mapping);
            }
            warnings.extend(parsed.warnings);
        }
    }
    add_case_collisions(inputs, &mut warnings);
    add_id_collision_warnings(
        requested_name,
        existing_menu_ids,
        existing_skin_ids,
        &mut warnings,
    );
    for skin_folder in named_skins {
        let has_item_back = inputs.iter().any(|input| {
            parent_key(input.relative_path).eq_ignore_ascii_case(&skin_folder)
                && file_name(input.relative_path).eq_ignore_ascii_case("ItemBack.png")
        });
        if !has_item_back {
            warnings.push(warning(
                ImportWarningKind::MissingRequiredAsset,
                &skin_folder,
                None,
                Some("ItemBack"),
                "named imported RM4 skin is missing required ItemBack.png; native vector skins do not have this requirement",
            ));
        }
    }

    let source_skin = mappings.iter().find_map(|mapping| {
        (mapping.provenance.field == "SkinName").then(|| parent_key(&mapping.provenance.file))
    });
    let (definition, asset_bytes, apply_warnings) =
        build_import_definition(&destination, source_skin.as_deref(), inputs, &mappings);
    warnings.extend(apply_warnings);
    ImportPreview {
        source: CompatibilitySource::RadialMenuV4,
        discovered_files,
        destination,
        source_skin,
        radify_settings: None,
        mappings,
        warnings,
        compatibility_coverage: registry_coverage(CompatibilitySource::RadialMenuV4),
        definition,
        asset_bytes,
    }
}

fn build_import_definition(
    destination: &ImportDestination,
    source_skin: Option<&str>,
    inputs: &[ImportInput<'_>],
    mappings: &[ImportMapping],
) -> (
    RadialDocument,
    BTreeMap<String, Vec<u8>>,
    Vec<ImportWarning>,
) {
    let mut document = legacy_import_template();
    document.default_menu_id = destination.menu_id.clone();
    document.menus[0].id = destination.menu_id.clone();
    document.menus[0].name = destination.display_name.clone();
    document.menus[0].skin_id = destination.skin_id.clone();
    for (ring_index, ring) in document.menus[0].rings.iter_mut().enumerate() {
        ring.id = RingId::new(format!(
            "{}-ring-{ring_index}",
            destination.menu_id.as_str()
        ));
        for (cell_index, cell) in ring.cells.iter_mut().enumerate() {
            cell.id = CellId::new(format!(
                "{}-cell-{ring_index}-{cell_index}",
                destination.menu_id.as_str()
            ));
        }
    }
    document.skins[0].id = destination.skin_id.clone();
    document.skins[0].name = destination.display_name.clone();
    document.assets.clear();
    let mut asset_bytes = BTreeMap::new();
    let mut imported_media = BTreeMap::new();
    let mut basename_media = BTreeMap::<String, Option<MediaReference>>::new();
    let mut content_assets = BTreeMap::<(String, MediaKind), (AssetId, String)>::new();
    let referenced_media = mappings
        .iter()
        .filter_map(|mapping| imported_text(&mapping.value))
        .map(|name| file_name(&name).to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    for input in inputs {
        let filename = file_name(input.relative_path);
        let lower = filename.to_ascii_lowercase();
        let kind = if lower.ends_with(".wav") {
            Some(MediaKind::Sound)
        } else if FAMILIAR_SKIN_FILES
            .iter()
            .any(|known| known.eq_ignore_ascii_case(filename))
            || (referenced_media.contains(&lower)
                && matches!(
                    Path::new(filename)
                        .extension()
                        .and_then(|value| value.to_str())
                        .unwrap_or_default()
                        .to_ascii_lowercase()
                        .as_str(),
                    "png" | "jpg" | "jpeg" | "bmp" | "ico" | "gif" | "tif" | "tiff"
                ))
        {
            Some(MediaKind::Image)
        } else {
            None
        };
        let Some(kind) = kind else { continue };
        let sha = sha256_hex(input.bytes);
        let extension = Path::new(filename)
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("bin")
            .to_ascii_lowercase();
        let (id, relative_path, is_new) =
            if let Some((id, path)) = content_assets.get(&(sha.clone(), kind)) {
                (id.clone(), path.clone(), false)
            } else {
                let id = imported_asset_id(kind, &sha);
                let path = format!("{sha}.{extension}");
                content_assets.insert((sha.clone(), kind), (id.clone(), path.clone()));
                (id, path, true)
            };
        let reference = Override::Value(MediaReference::Managed {
            asset_id: id.clone(),
        });
        if let Override::Value(reference) = &reference {
            basename_media
                .entry(lower.clone())
                .and_modify(|candidate| {
                    if candidate.as_ref() != Some(reference) {
                        *candidate = None;
                    }
                })
                .or_insert_with(|| Some(reference.clone()));
            let candidates = imported_media
                .entry(normalize_archive_path(input.relative_path))
                .or_insert_with(Vec::new);
            if !candidates.contains(reference) {
                candidates.push(reference.clone());
            }
        }
        if is_new {
            document.assets.push(AssetRecord {
                id,
                kind,
                relative_path: relative_path.clone(),
                content_sha256: sha,
                byte_len: input.bytes.len() as u64,
            });
            asset_bytes.insert(format!("assets/{relative_path}"), input.bytes.to_vec());
        }
        if !parent_key(input.relative_path).is_empty() {
            continue;
        }
        if kind == MediaKind::Sound {
            document.skins[0].style.values.sounds.on_show = reference;
        } else {
            let images = &mut document.skins[0].style.values.images;
            match lower.as_str() {
                "itemback.png" => images.item_background = reference,
                "itemglow.png" => images.item_glow = reference,
                "menuback.png" => images.menu_background = reference,
                "menuouterrim.png" => images.menu_outer_rim = reference,
                "centerimage.png" => images.center_image = reference,
                "submenuindicator.png" => images.submenu_indicator = reference,
                _ => {}
            }
        }
    }
    for (basename, reference) in basename_media {
        if let Some(reference) = reference {
            imported_media.insert(basename, vec![reference]);
        }
    }
    let warnings = apply_import_mappings(&mut document, source_skin, mappings, &imported_media);
    (document, asset_bytes, warnings)
}

/// Legacy skin imports need a small, self-contained menu canvas. They must not
/// inherit the evolving user-facing starter graph: doing so would couple
/// compatibility validation and package contents to unrelated starter menus.
fn legacy_import_template() -> RadialDocument {
    let mut document = RadialDocument::starter();
    document.menus.truncate(1);
    document.metadata.remove("starter_content");
    document.menus[0].rings[0].cells = [
        ("favorites", "Favorites", DynamicSource::Favorites),
        ("recent", "Recent", DynamicSource::RecentItems),
        (
            "results",
            "Launcher results",
            DynamicSource::LauncherQuery {
                query: String::new(),
                max_items: 12,
            },
        ),
    ]
    .into_iter()
    .map(|(id, label, source)| CellDefinition {
        id: CellId::new(id),
        label: label.into(),
        content: CellContent::Dynamic { source },
        alternate_clicks: Vec::new(),
        alternate_controls: Vec::new(),
        after_action: AfterActionPolicy::Inherit,
        secondary_after_action: AfterActionPolicy::Inherit,
        icon: Override::Inherit,
        tooltip: Override::Inherit,
        style: CellStyleLayer::default(),
        shortcuts: Vec::new(),
        hotstrings: Vec::new(),
    })
    .collect();
    document
}

fn imported_asset_id(kind: MediaKind, full_sha256: &str) -> AssetId {
    let kind_name = match kind {
        MediaKind::Image => "image",
        MediaKind::Sound => "sound",
    };
    AssetId::new(format!("import-{kind_name}-{full_sha256}"))
}

fn apply_import_mappings(
    document: &mut RadialDocument,
    source_skin: Option<&str>,
    mappings: &[ImportMapping],
    imported_media: &BTreeMap<String, Vec<MediaReference>>,
) -> Vec<ImportWarning> {
    let mut warnings = Vec::new();
    let initial_images = document.skins[0].style.values.images.clone();
    let initial_sounds = document.skins[0].style.values.sounds.clone();
    let asset_for = |mapping: &ImportMapping, name: &str| {
        let wanted = file_name(name);
        let source_relative = parent_key(&mapping.provenance.file);
        if mapping.provenance.dialect == ImportDialect::Rm4Definition && !source_relative.is_empty()
        {
            let sibling = normalize_archive_path(&format!("{source_relative}/{wanted}"));
            if let Some(references) = imported_media.get(&sibling)
                && let [reference] = references.as_slice()
            {
                return Some(reference.clone());
            }
        }
        if let Some(selected) = source_skin {
            let selected = selected.to_ascii_lowercase();
            let wanted = wanted.to_ascii_lowercase();
            let mut matching = imported_media.iter().filter(|(path, _)| {
                path.contains('/')
                    && path.rsplit('/').next() == Some(wanted.as_str())
                    && path.rsplit('/').nth(1) == Some(selected.as_str())
            });
            if let Some((_, references)) = matching.next()
                && matching.next().is_none()
                && let [reference] = references.as_slice()
            {
                return Some(reference.clone());
            }
            // An explicit source skin is authoritative. Falling through to a
            // globally unique basename could retarget media from another skin.
            return None;
        }
        if let Some(references) = imported_media.get(&wanted.to_ascii_lowercase())
            && let [reference] = references.as_slice()
        {
            return Some(reference.clone());
        }
        match wanted.to_ascii_lowercase().as_str() {
            "itemback.png" => media_value(&initial_images.item_background),
            "itemglow.png" => media_value(&initial_images.item_glow),
            "menuback.png" => media_value(&initial_images.menu_background),
            "menuouterrim.png" => media_value(&initial_images.menu_outer_rim),
            "centerimage.png" => media_value(&initial_images.center_image),
            "submenuindicator.png" => media_value(&initial_images.submenu_indicator),
            name if name.ends_with(".wav") => media_value(&initial_sounds.on_show),
            _ => None,
        }
    };
    let selected_scope = source_skin.map(|name| format!("skin:{name}.").to_ascii_lowercase());
    let mut ordered = mappings.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|mapping| {
        let field = mapping.provenance.field.as_str();
        usize::from(field.starts_with("Hotkey") || field.starts_with("Hotstring"))
    });
    for mapping in ordered {
        let (scope, field) = mapping.destination_field.rsplit_once('.').map_or(
            ("", mapping.destination_field.as_str()),
            |(scope, field)| (scope, field),
        );
        if !scope.is_empty()
            && !scope.eq_ignore_ascii_case("default")
            && selected_scope
                .as_ref()
                .is_none_or(|selected| &format!("{scope}.").to_ascii_lowercase() != selected)
        {
            continue;
        }
        if matches!(
            &mapping.value,
            ImportedValue::Structured(Value::Array(_) | Value::Object(_))
        ) {
            warnings.push(warning(
                ImportWarningKind::UnsupportedExpression,
                &mapping.provenance.file,
                mapping.provenance.line,
                Some(field),
                format!("{field} requires a scalar literal; arrays/objects are not accepted"),
            ));
            continue;
        }
        let before_document = document.clone();
        let style = &mut document.skins[0].style.values;
        let menu = &mut document.menus[0];
        let number = imported_number(&mapping.value);
        let boolean = imported_bool(&mapping.value);
        let text = imported_text(&mapping.value);
        let clear = matches!(&mapping.value, ImportedValue::Clear)
            || matches!(&mapping.value, ImportedValue::Structured(Value::Null));
        macro_rules! number_field {
            ($target:expr) => {
                if clear {
                    $target = Override::Clear;
                } else if let Some(value) = number {
                    $target = Override::Value(value as f32);
                }
            };
        }
        macro_rules! bool_field {
            ($target:expr) => {
                if clear {
                    $target = Override::Clear;
                } else if let Some(value) = boolean {
                    $target = Override::Value(value);
                }
            };
        }
        match field {
            "SkinName" | "Skin" => {
                if let Some(value) = text {
                    document.metadata.insert("legacy.source_skin".into(), value);
                }
            }
            "ItemSize" => number_field!(style.geometry.item_size),
            "RadiusScale" | "RadiusSizeFactor" => number_field!(style.geometry.radius_scale),
            "CenterSize" => number_field!(style.geometry.center_size),
            "CenterImageScale" => number_field!(style.geometry.center_image_scale),
            "ItemImageScale" => number_field!(style.geometry.item_image_scale),
            "ItemImageYRatio" => number_field!(style.geometry.item_image_y_ratio),
            "SubmenuIndicatorSize" => number_field!(style.geometry.submenu_indicator_size),
            "SubmenuIndicatorYRatio" => number_field!(style.geometry.submenu_indicator_y_ratio),
            "OuterRingMargin" => number_field!(style.geometry.outer_ring_margin),
            "OuterRimWidth" | "MenuBackOuterRimWidth" => {
                number_field!(style.geometry.outer_rim_width)
            }
            "ItemBackgroundImageOnCenter" => bool_field!(style.geometry.item_background_on_center),
            "ItemBackgroundImageOnItems" => bool_field!(style.geometry.item_background_on_items),
            "EnableItemText" => bool_field!(style.text.visible),
            "EnableGlow" => bool_field!(style.effects.glow_enabled),
            "AlwaysOnTop" => bool_field!(style.window.always_on_top),
            "ActivateOnShow" => bool_field!(style.window.activate_on_show),
            "FillCenterHitZone" => bool_field!(style.window.fill_center_hit_zone),
            "FillItemsHitZone" => bool_field!(style.window.fill_item_hit_zones),
            "TextFont" => style.text.font_family = override_text(text, clear),
            "TextSize" => number_field!(style.text.font_size),
            "TextFontOptions" => {
                if let Some(value) = text.as_deref() {
                    let normalized = value.to_ascii_lowercase();
                    let tokens = normalized
                        .split(|c: char| c.is_whitespace() || c == ',' || c == ';')
                        .filter(|token| !token.is_empty())
                        .collect::<Vec<_>>();
                    if tokens.iter().all(|token| {
                        matches!(
                            *token,
                            "bold" | "italic" | "underline" | "strike" | "strikeout"
                        )
                    }) {
                        style.text.bold = Override::Value(tokens.contains(&"bold"));
                        style.text.italic = Override::Value(tokens.contains(&"italic"));
                        style.text.underline = Override::Value(tokens.contains(&"underline"));
                        style.text.strikeout = Override::Value(
                            tokens.contains(&"strike") || tokens.contains(&"strikeout"),
                        );
                    }
                }
            }
            "TextColor" => {
                if let Some(value) = text.and_then(|v| parse_color(&v)) {
                    style.text.color = Override::Value(value);
                }
            }
            "TextShadowColor" => {
                if let Some(value) = text.and_then(|v| parse_color(&v)) {
                    style.text.shadow_color = Override::Value(value);
                }
            }
            "MenuShadowInnerColor" => {
                if let Some(value) = text.and_then(|v| parse_color(&v)) {
                    style.effects.menu_shadow_inner_color = Override::Value(value);
                }
            }
            "MenuShadowOuterColor" => {
                if let Some(value) = text.and_then(|v| parse_color(&v)) {
                    style.effects.menu_shadow_outer_color = Override::Value(value);
                }
            }
            "TextShadow" => bool_field!(style.text.shadow_enabled),
            "TextShadowOffset" => {
                if let Some(value) = number {
                    style.text.shadow_offset = Override::Value(super::model::Offset2D {
                        x: value as f32,
                        y: value as f32,
                    });
                }
            }
            "MenuShadowWidth" => number_field!(style.effects.menu_shadow_width),
            "TextBoxScale" => number_field!(style.text.text_box_scale),
            "TextYRatio" => number_field!(style.text.vertical_ratio),
            "AutoTooltip" => {
                if let Some(value) = boolean {
                    style.effects.tooltip_mode = Override::Value(if value {
                        TooltipMode::Automatic
                    } else {
                        TooltipMode::Disabled
                    });
                }
            }
            "EnableTooltip" => {
                if let Some(value) = boolean {
                    style.effects.tooltip_mode = Override::Value(if value {
                        TooltipMode::Explicit
                    } else {
                        TooltipMode::Disabled
                    });
                }
            }
            "TextRendering" | "SmoothingMode" | "InterpolationMode" => {
                if let Some(value) = number {
                    let quality = quality(value);
                    match field {
                        "TextRendering" => style.quality.text = Override::Value(quality),
                        "SmoothingMode" => style.quality.shape = Override::Value(quality),
                        _ => style.quality.interpolation = Override::Value(quality),
                    }
                }
            }
            "IconTrans" => {
                if let Some(value) = number {
                    style.images.icon_opacity = Override::Value(normalized_opacity(value));
                }
            }
            "ItemBackTrans" => {
                if let Some(value) = number {
                    style.images.item_background_opacity =
                        Override::Value(normalized_opacity(value));
                }
            }
            "ItemForeTrans" => {
                if let Some(value) = number {
                    style.images.item_foreground_opacity =
                        Override::Value(normalized_opacity(value));
                }
            }
            "ItemShadowTrans" => {
                if let Some(value) = number {
                    style.images.item_shadow_opacity = Override::Value(normalized_opacity(value));
                }
            }
            "MenuBackTrans" => {
                if let Some(value) = number {
                    style.images.menu_background_opacity =
                        Override::Value(normalized_opacity(value));
                }
            }
            "MenuBackOuterRimTrans" => {
                if let Some(value) = number {
                    style.images.menu_outer_rim_opacity =
                        Override::Value(normalized_opacity(value));
                }
            }
            "MenuForeTrans" => {
                if let Some(value) = number {
                    style.images.menu_foreground_opacity =
                        Override::Value(normalized_opacity(value));
                }
            }
            "TextTrans" => {
                if let Some(value) = number {
                    let mut color = resolved_color(&style.text.color);
                    color.alpha = (normalized_opacity(value) * 255.0).round() as u8;
                    style.text.color = Override::Value(color);
                }
            }
            "TextShadowTrans" => {
                if let Some(value) = number {
                    let mut color = resolved_color(&style.text.shadow_color);
                    color.alpha = (normalized_opacity(value) * 255.0).round() as u8;
                    style.text.shadow_color = Override::Value(color);
                }
            }
            "IconShrink" => {
                if let Some(value) = number {
                    style.geometry.item_image_scale = Override::Value(shrink_scale(value));
                }
            }
            "ItemBackShrink" => {
                if let Some(value) = number {
                    style.geometry.item_background_scale = Override::Value(shrink_scale(value));
                }
            }
            "ItemForeShrink" => {
                if let Some(value) = number {
                    style.geometry.item_foreground_scale = Override::Value(shrink_scale(value));
                }
            }
            "ItemShadowShrink" => {
                if let Some(value) = number {
                    style.geometry.item_shadow_scale = Override::Value(shrink_scale(value));
                }
            }
            "TextBoxShrink" => {
                if let Some(value) = number {
                    style.text.text_box_scale = Override::Value(shrink_scale(value));
                }
            }
            "MenuBackCenterShrink" => {
                if let Some(value) = number {
                    style.geometry.center_background_scale = Override::Value(shrink_scale(value));
                }
            }
            "MenuBackSize" => {
                apply_size_expression(&mapping.value, &mut style.geometry.menu_background_scale)
            }
            "MenuForeSize" => {
                apply_size_expression(&mapping.value, &mut style.geometry.menu_foreground_scale)
            }
            "ItemGlowImage"
            | "ItemGlow"
            | "ItemBackgroundImage"
            | "MenuBackgroundImage"
            | "MenuOuterRimImage"
            | "CenterBackgroundImage"
            | "CenterImage"
            | "SubmenuIndicatorImage"
            | "ItemBack"
            | "ItemFore"
            | "ItemShadow"
            | "MenuBack"
            | "MenuBackOuterRim"
            | "MenuFore"
            | "MenuBackCenter" => {
                let target = match field {
                    "ItemGlowImage" | "ItemGlow" => &mut style.images.item_glow,
                    "ItemBackgroundImage" | "ItemBack" => &mut style.images.item_background,
                    "MenuBackgroundImage" | "MenuBack" => &mut style.images.menu_background,
                    "MenuOuterRimImage" | "MenuBackOuterRim" => &mut style.images.menu_outer_rim,
                    "CenterBackgroundImage" | "MenuBackCenter" => {
                        &mut style.images.center_background
                    }
                    "CenterImage" => &mut style.images.center_image,
                    "SubmenuIndicatorImage" => &mut style.images.submenu_indicator,
                    "ItemFore" => &mut style.images.item_foreground,
                    "ItemShadow" => &mut style.images.item_shadow,
                    "MenuFore" => &mut style.images.menu_foreground,
                    _ => unreachable!(),
                };
                if clear {
                    *target = Override::Clear;
                } else if let Some(reference) =
                    text.as_deref().and_then(|name| asset_for(mapping, name))
                {
                    *target = Override::Value(reference);
                }
            }
            "SoundOnShow" | "SoundOnClose" | "SoundOnSelect" | "SoundOnSubShow"
            | "SoundOnSubClose" => {
                let target = match field {
                    "SoundOnShow" => &mut style.sounds.on_show,
                    "SoundOnClose" => &mut style.sounds.on_close,
                    "SoundOnSelect" => &mut style.sounds.on_select,
                    "SoundOnSubShow" => &mut style.sounds.on_submenu_show,
                    _ => &mut style.sounds.on_submenu_close,
                };
                if clear {
                    *target = Override::Clear;
                } else if let Some(reference) =
                    text.as_deref().and_then(|name| asset_for(mapping, name))
                {
                    *target = Override::Value(reference);
                }
            }
            "CenterClick" => {
                if let ImportedValue::Control(control) = &mapping.value {
                    menu.center_control = Some(*control);
                }
            }
            "CenterRightClick" => {
                if let ImportedValue::Control(control) = &mapping.value {
                    menu.center_secondary_control = Some(*control);
                }
            }
            "MenuClick" => {
                if let ImportedValue::Control(control) = &mapping.value {
                    menu.background_control = Some(*control);
                }
            }
            "MenuRightClick" => {
                if let ImportedValue::Control(control) = &mapping.value {
                    menu.background_secondary_control = Some(*control);
                }
            }
            "Click" => {
                if let ImportedValue::Control(control) = &mapping.value {
                    menu.rings[0].cells[0].content =
                        super::model::CellContent::Control { control: *control };
                }
            }
            "RightClick" | "CtrlClick" | "ShiftClick" | "AltClick" => {
                if let ImportedValue::Control(control) = &mapping.value {
                    let gesture = gesture_for_legacy_field(field);
                    let controls = &mut menu.rings[0].cells[0].alternate_controls;
                    controls.retain(|binding| binding.gesture != gesture);
                    controls.push(ControlClickBinding {
                        gesture,
                        control: *control,
                    });
                }
            }
            "HotkeyClick" | "HotkeyRightClick" | "HotkeyCtrlClick" | "HotkeyShiftClick"
            | "HotkeyAltClick" => {
                if let Some(chord) = text.filter(|value| !value.trim().is_empty()) {
                    let id = ShortcutId::new(format!("import-{}", field.to_ascii_lowercase()));
                    let gesture = gesture_for_legacy_field(field);
                    menu.rings[0].cells[0]
                        .shortcuts
                        .retain(|binding| binding.id != id && binding.gesture != gesture);
                    menu.rings[0].cells[0].shortcuts.push(ItemShortcut {
                        id,
                        chord,
                        gesture,
                        scope: TriggerScope::MenuLocal,
                    });
                }
            }
            "HotstringClick"
            | "HotstringRightClick"
            | "HotstringCtrlClick"
            | "HotstringShiftClick"
            | "HotstringAltClick" => {
                if let Some(value) = text.filter(|value| !value.is_empty()) {
                    let id = HotstringId::new(format!("import-{}", field.to_ascii_lowercase()));
                    let gesture = gesture_for_legacy_field(field);
                    menu.rings[0].cells[0]
                        .hotstrings
                        .retain(|binding| binding.id != id && binding.gesture != gesture);
                    menu.rings[0].cells[0].hotstrings.push(ItemHotstring {
                        id,
                        text: value,
                        gesture,
                        case_sensitive: false,
                        scope: TriggerScope::MenuLocal,
                    });
                }
            }
            "Text" => {
                if let Some(value) = text {
                    menu.rings[0].cells[0].label = value;
                }
            }
            "Tooltip" => menu.rings[0].cells[0].tooltip = override_text(text, clear),
            "Image" => {
                if clear {
                    menu.rings[0].cells[0].icon = Override::Clear;
                } else if let Some(reference) =
                    text.as_deref().and_then(|name| asset_for(mapping, name))
                {
                    menu.rings[0].cells[0].icon = Override::Value(reference);
                }
            }
            "MirrorClickToRightClick" => {
                if let Some(value) = boolean {
                    menu.mirror_primary_to_secondary = value;
                }
            }
            "CloseOnItemClick" => {
                if let Some(value) = boolean {
                    menu.after_action = if value {
                        super::model::AfterActionPolicy::CloseTree
                    } else {
                        super::model::AfterActionPolicy::KeepOpen
                    };
                }
            }
            "CloseOnItemRightClick" => {
                if let Some(value) = boolean {
                    menu.rings[0].cells[0].secondary_after_action = if value {
                        super::model::AfterActionPolicy::CloseTree
                    } else {
                        super::model::AfterActionPolicy::KeepOpen
                    };
                }
            }
            "AutoSubmenuMarking" => {
                if boolean == Some(false) {
                    style.images.submenu_indicator = Override::Clear;
                }
            }
            "AutoSubmenuMark" => {
                style.text.submenu_indicator_text = override_text(text, clear);
            }
            // `Submenu` and `SubmenuOptions` require a complete legacy menu
            // graph. Preview reports them explicitly instead of manufacturing
            // a dangling MenuId or silently dropping them.
            "Submenu" | "SubmenuOptions" => {}
            _ => warnings.push(warning(
                ImportWarningKind::IncompatibleField,
                &mapping.provenance.file,
                mapping.provenance.line,
                Some(field),
                "accepted compatibility field has no safe typed application handler",
            )),
        }
        let outcome = if *document != before_document {
            match super::validation::validate(document) {
                Ok(()) => ApplyOutcome::Changed,
                Err(error) => {
                    let context = error
                        .0
                        .iter()
                        .take(3)
                        .map(|issue| format!("{}: {}", issue.path, issue.message))
                        .collect::<Vec<_>>()
                        .join("; ");
                    *document = before_document;
                    ApplyOutcome::Invalid(format!(
                        "{field} value {:?} would make the radial document invalid: {context}",
                        mapping.value
                    ))
                }
            }
        } else {
            classify_unchanged_mapping(field, mapping, &asset_for)
        };
        match outcome {
            ApplyOutcome::Invalid(reason) => warnings.push(warning(
                ImportWarningKind::UnsupportedExpression,
                &mapping.provenance.file,
                mapping.provenance.line,
                Some(field),
                reason,
            )),
            ApplyOutcome::MissingAsset(reason) => warnings.push(warning(
                ImportWarningKind::MissingRequiredAsset,
                &mapping.provenance.file,
                mapping.provenance.line,
                Some(field),
                reason,
            )),
            ApplyOutcome::ExplicitlyDiagnosed(reason) => warnings.push(warning(
                ImportWarningKind::IncompatibleField,
                &mapping.provenance.file,
                mapping.provenance.line,
                Some(field),
                reason,
            )),
            ApplyOutcome::Changed | ApplyOutcome::AlreadyEqual => {}
        }
    }
    warnings
}

fn classify_unchanged_mapping(
    field: &str,
    mapping: &ImportMapping,
    asset_for: &impl Fn(&ImportMapping, &str) -> Option<MediaReference>,
) -> ApplyOutcome {
    if let Some(ImportApplicationPolicy::Diagnosed(reason)) =
        application_policy(mapping.provenance.source, field)
    {
        return ApplyOutcome::ExplicitlyDiagnosed(reason.into());
    }
    let value = &mapping.value;
    let text = imported_text(value);
    let media = is_media_field(field)
        || matches!(
            field,
            "ItemGlowImage"
                | "ItemBackgroundImage"
                | "MenuBackgroundImage"
                | "MenuOuterRimImage"
                | "CenterBackgroundImage"
                | "CenterImage"
                | "SubmenuIndicatorImage"
                | "Image"
                | "SoundOnShow"
                | "SoundOnClose"
                | "SoundOnSelect"
                | "SoundOnSubShow"
                | "SoundOnSubClose"
        );
    if media {
        if matches!(
            value,
            ImportedValue::Clear | ImportedValue::Structured(Value::Null)
        ) {
            return ApplyOutcome::AlreadyEqual;
        }
        return match text {
            Some(name) if asset_for(mapping, &name).is_some() => ApplyOutcome::AlreadyEqual,
            Some(name) => ApplyOutcome::MissingAsset(format!(
                "{field} references missing or ambiguous media {name:?} for the selected source skin"
            )),
            None => ApplyOutcome::Invalid(format!(
                "{field} requires a media filename string or explicit clear"
            )),
        };
    }
    let valid = if is_action_field(field) {
        matches!(value, ImportedValue::Control(_))
    } else if field.starts_with("Hotkey") || field.starts_with("Hotstring") {
        text.as_ref().is_some_and(|value| !value.is_empty())
    } else if matches!(
        field,
        "ItemBackgroundImageOnCenter"
            | "ItemBackgroundImageOnItems"
            | "EnableItemText"
            | "TextShadow"
            | "MirrorClickToRightClick"
            | "CloseOnItemClick"
            | "CloseOnItemRightClick"
            | "EnableGlow"
            | "AutoTooltip"
            | "EnableTooltip"
            | "AlwaysOnTop"
            | "ActivateOnShow"
            | "FillCenterHitZone"
            | "FillItemsHitZone"
            | "AutoSubmenuMarking"
    ) {
        imported_bool(value).is_some()
    } else if matches!(
        field,
        "TextColor" | "TextShadowColor" | "MenuShadowInnerColor" | "MenuShadowOuterColor"
    ) {
        text.as_deref().and_then(parse_color).is_some()
    } else if field == "TextFontOptions" {
        text.as_ref().is_some_and(|value| {
            value
                .to_ascii_lowercase()
                .split(|c: char| c.is_whitespace() || c == ',' || c == ';')
                .filter(|token| !token.is_empty())
                .all(|token| {
                    matches!(
                        token,
                        "bold" | "italic" | "underline" | "strike" | "strikeout"
                    )
                })
        })
    } else if matches!(
        field,
        "Skin" | "SkinName" | "TextFont" | "AutoSubmenuMark" | "Tooltip" | "Text"
    ) {
        text.is_some()
            || matches!(
                value,
                ImportedValue::Clear | ImportedValue::Structured(Value::Null)
            )
    } else if matches!(field, "MenuBackSize" | "MenuForeSize") {
        matches!(
            value,
            ImportedValue::SizeExpression(_) | ImportedValue::Number(_) | ImportedValue::Clear
        )
    } else {
        imported_number(value).is_some()
            || matches!(
                value,
                ImportedValue::Clear | ImportedValue::Structured(Value::Null)
            )
    };
    if valid {
        ApplyOutcome::AlreadyEqual
    } else {
        ApplyOutcome::Invalid(format!("{field} does not accept typed value {value:?}"))
    }
}

fn gesture_for_legacy_field(field: &str) -> ClickGesture {
    if field.contains("Right") {
        ClickGesture::Secondary
    } else if field.contains("Ctrl") {
        ClickGesture::CtrlPrimary
    } else if field.contains("Shift") {
        ClickGesture::ShiftPrimary
    } else if field.contains("Alt") {
        ClickGesture::AltPrimary
    } else {
        ClickGesture::Primary
    }
}

fn media_value(value: &Override<MediaReference>) -> Option<MediaReference> {
    match value {
        Override::Value(value) => Some(value.clone()),
        _ => None,
    }
}
fn imported_number(value: &ImportedValue) -> Option<f64> {
    match value {
        ImportedValue::Number(v) => Some(*v),
        ImportedValue::Structured(v) => v.as_f64(),
        _ => None,
    }
}
fn imported_bool(value: &ImportedValue) -> Option<bool> {
    match value {
        ImportedValue::Bool(v) => Some(*v),
        ImportedValue::Structured(v) => v.as_bool(),
        _ => None,
    }
}
fn imported_text(value: &ImportedValue) -> Option<String> {
    match value {
        ImportedValue::Text(v) | ImportedValue::MediaFilename(v) => Some(v.clone()),
        ImportedValue::Structured(Value::String(v)) => Some(v.clone()),
        _ => None,
    }
}
fn override_text(value: Option<String>, clear: bool) -> Override<String> {
    if clear {
        Override::Clear
    } else {
        value.map_or(Override::Inherit, Override::Value)
    }
}
fn normalized_opacity(value: f64) -> f32 {
    if value > 1.0 {
        (value / 255.0).clamp(0.0, 1.0) as f32
    } else {
        value.clamp(0.0, 1.0) as f32
    }
}
fn shrink_scale(value: f64) -> f32 {
    (1.0 - if value > 1.0 { value / 100.0 } else { value }).clamp(0.0, 16.0) as f32
}
fn quality(value: f64) -> RenderingQuality {
    if value <= 1.0 {
        RenderingQuality::Fast
    } else if value >= 4.0 {
        RenderingQuality::HighQuality
    } else {
        RenderingQuality::Balanced
    }
}
fn resolved_color(value: &Override<ColorRgba>) -> ColorRgba {
    match value {
        Override::Value(value) => *value,
        _ => ColorRgba {
            red: 255,
            green: 255,
            blue: 255,
            alpha: 255,
        },
    }
}
fn parse_color(value: &str) -> Option<ColorRgba> {
    let value = value
        .trim()
        .trim_start_matches('#')
        .trim_start_matches("0x");
    let raw = u32::from_str_radix(value, 16).ok()?;
    Some(ColorRgba {
        red: ((raw >> 16) & 0xff) as u8,
        green: ((raw >> 8) & 0xff) as u8,
        blue: (raw & 0xff) as u8,
        alpha: 255,
    })
}
fn apply_size_expression(value: &ImportedValue, target: &mut Override<f32>) {
    match value {
        ImportedValue::SizeExpression(SizeExpression::FactorOffset(v)) => {
            *target = Override::Value((1.0 + *v) as f32)
        }
        ImportedValue::SizeExpression(SizeExpression::Additive(v)) => {
            *target = Override::Value((1.0 + *v / 100.0).max(0.0) as f32)
        }
        ImportedValue::Number(v) => *target = Override::Value(*v as f32),
        ImportedValue::Clear => *target = Override::Clear,
        _ => {}
    }
}

pub fn parse_radify_settings(bytes: &[u8]) -> Result<RadifySettingsSnapshot, String> {
    let value: Value = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    let root = value
        .as_object()
        .ok_or_else(|| "Radify settings root must be a JSON object".to_string())?;
    let mut defaults = BTreeMap::new();
    let mut skins = BTreeMap::new();
    for (key, value) in root {
        if key.eq_ignore_ascii_case("skins") {
            let object = value
                .as_object()
                .ok_or_else(|| "Radify Skins must be an object".to_string())?;
            for (skin, fields) in object {
                let fields = fields
                    .as_object()
                    .ok_or_else(|| format!("Radify skin {skin:?} must be an object"))?;
                skins.insert(
                    skin.clone(),
                    fields.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
                );
            }
        } else if key.eq_ignore_ascii_case("default") || key.eq_ignore_ascii_case("defaults") {
            let object = value
                .as_object()
                .ok_or_else(|| "Radify defaults must be an object".to_string())?;
            defaults.extend(object.iter().map(|(k, v)| (k.clone(), v.clone())));
        } else {
            defaults.insert(key.clone(), value.clone());
        }
    }
    Ok(RadifySettingsSnapshot { defaults, skins })
}

#[derive(Default)]
struct ParsedRm4 {
    mappings: Vec<ImportMapping>,
    warnings: Vec<ImportWarning>,
}

fn parse_rm4_definition(input: &ImportInput<'_>) -> ParsedRm4 {
    let mut result = ParsedRm4::default();
    let text = match std::str::from_utf8(input.bytes) {
        Ok(text) => text,
        Err(error) => {
            result.warnings.push(warning(
                ImportWarningKind::MalformedData,
                input.relative_path,
                None,
                None,
                format!("skin definition is not UTF-8 data: {error}"),
            ));
            return result;
        }
    };
    for (index, raw_line) in text.lines().enumerate() {
        let line_number = index + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        let Some((raw_field, raw_value)) = line.split_once('=') else {
            result.warnings.push(warning(
                ImportWarningKind::IgnoredExecutableBehavior,
                input.relative_path,
                Some(line_number),
                None,
                "non-assignment content was ignored; scripts and callbacks are never evaluated",
            ));
            continue;
        };
        let field = raw_field.trim();
        let Some(canonical) = canonical_field(CompatibilitySource::RadialMenuV4, field) else {
            result.warnings.push(warning(
                ImportWarningKind::UnknownField,
                input.relative_path,
                Some(line_number),
                Some(field),
                format!("unknown RM4 field {field:?}"),
            ));
            continue;
        };
        let group = compatibility_for(CompatibilitySource::RadialMenuV4, canonical).unwrap();
        if matches!(
            group.classification,
            CompatibilityClassification::Incompatible | CompatibilityClassification::NotApplicable
        ) {
            result.warnings.push(warning(
                if group.classification == CompatibilityClassification::Incompatible {
                    ImportWarningKind::IncompatibleField
                } else {
                    ImportWarningKind::NotApplicable
                },
                input.relative_path,
                Some(line_number),
                Some(canonical),
                group.import,
            ));
            continue;
        }
        match parse_rm4_value(canonical, raw_value.trim()) {
            Ok(value) => result.mappings.push(ImportMapping {
                destination_field: canonical.to_string(),
                value,
                classification: group.classification,
                provenance: ImportProvenance {
                    source: CompatibilitySource::RadialMenuV4,
                    dialect: ImportDialect::Rm4Definition,
                    file: input.relative_path.to_string(),
                    line: Some(line_number),
                    field: canonical.to_string(),
                    evidence: input.evidence,
                },
            }),
            Err(message) => result.warnings.push(warning(
                if is_action_field(canonical) {
                    ImportWarningKind::IgnoredExecutableBehavior
                } else {
                    ImportWarningKind::UnsupportedExpression
                },
                input.relative_path,
                Some(line_number),
                Some(canonical),
                message,
            )),
        }
    }
    result
}

fn parse_rm4_value(field: &str, value: &str) -> Result<ImportedValue, String> {
    if value.is_empty() {
        return Ok(ImportedValue::Clear);
    }
    if matches!(field, "MenuBackSize" | "MenuForeSize") {
        if let Some(number) = value.strip_prefix("fac+") {
            return finite(number)
                .map(|n| ImportedValue::SizeExpression(SizeExpression::FactorOffset(n)));
        }
        if let Some(number) = value.strip_prefix("fac-") {
            return finite(number)
                .map(|n| ImportedValue::SizeExpression(SizeExpression::FactorOffset(-n)));
        }
        if let Some(number) = value.strip_prefix("add+") {
            return finite(number)
                .map(|n| ImportedValue::SizeExpression(SizeExpression::Additive(n)));
        }
        if let Some(number) = value.strip_prefix("add-") {
            return finite(number)
                .map(|n| ImportedValue::SizeExpression(SizeExpression::Additive(-n)));
        }
        return Err(format!("{field} accepts only fac±N or add±N"));
    }
    if field == "IconTrans" && (value.contains('|') || value.contains(',')) {
        return Err("IconTrans color matrices/ARGB transforms are not imported; only scalar opacity is supported".to_string());
    }
    if is_media_field(field) {
        return literal_text(value).map(ImportedValue::MediaFilename);
    }
    if is_action_field(field) {
        return parse_control(value)
            .map(ImportedValue::Control)
            .ok_or_else(|| "only Close, CloseMenu, and Drag action literals are imported; callbacks/scripts are never evaluated".to_string());
    }
    if is_rm4_text_field(field) {
        return literal_text(value).map(ImportedValue::Text);
    }
    if value.eq_ignore_ascii_case("true") {
        return Ok(ImportedValue::Bool(true));
    }
    if value.eq_ignore_ascii_case("false") {
        return Ok(ImportedValue::Bool(false));
    }
    if let Ok(number) = finite(value) {
        return Ok(ImportedValue::Number(number));
    }
    Err(format!(
        "{field} requires a finite literal number, not expression {value:?}"
    ))
}

fn append_radify_mappings(
    snapshot: &RadifySettingsSnapshot,
    input: &ImportInput<'_>,
    mappings: &mut Vec<ImportMapping>,
    warnings: &mut Vec<ImportWarning>,
) {
    let scopes = std::iter::once(("default".to_string(), &snapshot.defaults)).chain(
        snapshot
            .skins
            .iter()
            .map(|(name, fields)| (format!("skin:{name}"), fields)),
    );
    for (scope, fields) in scopes {
        for (field, value) in fields {
            let Some(canonical) = canonical_field(CompatibilitySource::Radify, field) else {
                warnings.push(warning(
                    ImportWarningKind::UnknownField,
                    input.relative_path,
                    None,
                    Some(field),
                    format!("unknown Radify field {field:?} in {scope}"),
                ));
                continue;
            };
            let group = compatibility_for(CompatibilitySource::Radify, canonical).unwrap();
            if matches!(
                group.classification,
                CompatibilityClassification::Incompatible
                    | CompatibilityClassification::NotApplicable
            ) {
                warnings.push(warning(
                    if group.classification == CompatibilityClassification::Incompatible {
                        ImportWarningKind::IncompatibleField
                    } else {
                        ImportWarningKind::NotApplicable
                    },
                    input.relative_path,
                    None,
                    Some(canonical),
                    group.import,
                ));
                continue;
            }
            let imported = if is_action_field(canonical) {
                match control_from_json(value) {
                    Some(control) => control,
                    None => {
                        warnings.push(warning(
                            ImportWarningKind::IgnoredExecutableBehavior,
                            input.relative_path,
                            None,
                            Some(canonical),
                            "only Close, CloseMenu, and Drag action literals are imported; callbacks/scripts are never evaluated",
                        ));
                        continue;
                    }
                }
            } else {
                ImportedValue::Structured(value.clone())
            };
            mappings.push(ImportMapping {
                destination_field: format!("{scope}.{canonical}"),
                value: imported,
                classification: group.classification,
                provenance: ImportProvenance {
                    source: CompatibilitySource::Radify,
                    dialect: ImportDialect::RadifySettings,
                    file: input.relative_path.to_string(),
                    line: None,
                    field: canonical.to_string(),
                    evidence: input.evidence,
                },
            });
        }
    }
}

fn control_from_json(value: &Value) -> Option<ImportedValue> {
    value
        .as_str()
        .and_then(parse_control)
        .map(ImportedValue::Control)
}

fn parse_control(value: &str) -> Option<Control> {
    match value.trim().to_ascii_lowercase().as_str() {
        "close" => Some(Control::Close),
        "closemenu" | "close_menu" => Some(Control::Back),
        "drag" => Some(Control::Drag),
        _ => None,
    }
}

fn discover(inputs: &[ImportInput<'_>]) -> (Vec<DiscoveredImportFile>, Vec<ImportWarning>) {
    let mut discovered = Vec::new();
    let mut warnings = Vec::new();
    for input in inputs {
        let filename = file_name(input.relative_path);
        let role = if filename.eq_ignore_ascii_case("Preferences.json") {
            DiscoveredRole::RadifyPreferences
        } else if filename.eq_ignore_ascii_case("Settings.json") {
            DiscoveredRole::RadifySettings
        } else if filename.eq_ignore_ascii_case("Skin definition.txt") {
            DiscoveredRole::Rm4SkinDefinition
        } else if FAMILIAR_SKIN_FILES
            .iter()
            .any(|known| filename.eq_ignore_ascii_case(known))
        {
            DiscoveredRole::FamiliarSkinMedia
        } else if filename.to_ascii_lowercase().ends_with(".wav") {
            DiscoveredRole::Sound
        } else {
            DiscoveredRole::Other
        };
        let portable = is_portable_relative(input.relative_path);
        if !portable {
            warnings.push(warning(
                ImportWarningKind::NonPortablePath,
                input.relative_path,
                None,
                None,
                "absolute, parent-traversing, or prefixed paths remain external/nonportable and are never copied during preview",
            ));
        }
        discovered.push(DiscoveredImportFile {
            relative_path: input.relative_path.to_string(),
            role,
            evidence: input.evidence,
            portable,
        });
    }
    (discovered, warnings)
}

fn add_case_collisions(inputs: &[ImportInput<'_>], warnings: &mut Vec<ImportWarning>) {
    let mut paths = BTreeMap::<String, &str>::new();
    for input in inputs {
        let folded = input.relative_path.replace('\\', "/").to_ascii_lowercase();
        if let Some(first) = paths.insert(folded, input.relative_path) {
            warnings.push(warning(
                ImportWarningKind::Collision,
                input.relative_path,
                None,
                None,
                format!("case-insensitive path collision with {first:?}"),
            ));
        }
    }
}

fn add_id_collision_warnings(
    requested_name: &str,
    menu_ids: &BTreeSet<String>,
    skin_ids: &BTreeSet<String>,
    warnings: &mut Vec<ImportWarning>,
) {
    let base = slug(requested_name);
    for (kind, candidate, occupied) in [
        ("menu", format!("import-menu-{base}"), menu_ids),
        ("skin", format!("import-skin-{base}"), skin_ids),
    ] {
        if occupied.contains(&candidate) {
            warnings.push(warning(
                ImportWarningKind::Collision,
                requested_name,
                None,
                None,
                format!("planned {kind} id {candidate:?} already exists; preview selected a deterministic suffixed id"),
            ));
        }
    }
}

fn planned_destination(
    requested_name: &str,
    menu_ids: &BTreeSet<String>,
    skin_ids: &BTreeSet<String>,
) -> ImportDestination {
    let base = slug(requested_name);
    ImportDestination {
        menu_id: MenuId::new(unique_id(&format!("import-menu-{base}"), menu_ids)),
        skin_id: SkinId::new(unique_id(&format!("import-skin-{base}"), skin_ids)),
        display_name: requested_name.trim().to_string(),
    }
}

fn unique_id(base: &str, occupied: &BTreeSet<String>) -> String {
    if !occupied.contains(base) {
        return base.to_string();
    }
    (2..)
        .map(|suffix| format!("{base}-{suffix}"))
        .find(|candidate| !occupied.contains(candidate))
        .expect("unbounded numeric suffix has an available stable id")
}

fn slug(value: &str) -> String {
    let mut slug = String::new();
    for character in value.trim().chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    let slug = slug.trim_end_matches('-');
    if slug.is_empty() {
        "legacy".to_string()
    } else {
        slug.to_string()
    }
}

fn registry_coverage(source: CompatibilitySource) -> BTreeMap<String, CompatibilityClassification> {
    FIELD_REGISTRY
        .iter()
        .filter(|group| group.source == source)
        .flat_map(|group| {
            group
                .fields
                .iter()
                .map(move |field| ((*field).to_string(), group.classification))
        })
        .collect()
}

fn canonical_field(source: CompatibilitySource, input: &str) -> Option<&'static str> {
    FIELD_REGISTRY
        .iter()
        .filter(|group| group.source == source)
        .flat_map(|group| group.fields.iter().copied())
        .find(|field| field.eq_ignore_ascii_case(input))
}

fn is_media_field(field: &str) -> bool {
    matches!(
        field,
        "ItemGlow"
            | "ItemBack"
            | "ItemFore"
            | "ItemShadow"
            | "MenuBack"
            | "MenuBackOuterRim"
            | "MenuFore"
            | "MenuBackCenter"
    )
}

fn is_action_field(field: &str) -> bool {
    matches!(
        field,
        "MenuClick"
            | "MenuRightClick"
            | "CenterClick"
            | "CenterRightClick"
            | "Click"
            | "RightClick"
            | "CtrlClick"
            | "ShiftClick"
            | "AltClick"
    )
}

fn is_rm4_text_field(field: &str) -> bool {
    matches!(
        field,
        "SkinName"
            | "AutoSubmenuMark"
            | "TextFont"
            | "TextColor"
            | "TextTrans"
            | "TextShadowColor"
            | "TextShadowTrans"
            | "MenuShadowInnerColor"
            | "MenuShadowOuterColor"
    )
}

fn literal_text(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.contains('(') || value.contains(')') || value.contains("%") || value.contains(":=") {
        return Err("executable or computed expressions are never evaluated".to_string());
    }
    let unquoted = if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        &value[1..value.len() - 1]
    } else {
        value
    };
    Ok(unquoted.to_string())
}

fn finite(value: &str) -> Result<f64, String> {
    let parsed = value
        .trim()
        .parse::<f64>()
        .map_err(|_| format!("unsupported expression {value:?}"))?;
    if parsed.is_finite() {
        Ok(parsed)
    } else {
        Err("numeric value must be finite".to_string())
    }
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

fn parent_key(path: &str) -> String {
    path.replace('\\', "/")
        .rsplit_once('/')
        .map_or_else(String::new, |(parent, _)| parent.to_string())
}

fn normalize_archive_path(path: &str) -> String {
    path.replace('\\', "/")
        .split('/')
        .filter(|component| !component.is_empty() && *component != ".")
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>()
        .join("/")
}

fn is_portable_relative(path: &str) -> bool {
    let path = Path::new(path);
    !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

fn warning(
    kind: ImportWarningKind,
    file: impl Into<String>,
    line: Option<usize>,
    field: Option<&str>,
    message: impl Into<String>,
) -> ImportWarning {
    ImportWarning {
        kind,
        file: file.into(),
        line,
        field: field.map(str::to_string),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    fn input<'a>(path: &'a str, bytes: &'a [u8], evidence: ImportEvidence) -> ImportInput<'a> {
        ImportInput {
            relative_path: path,
            bytes,
            evidence,
        }
    }

    #[test]
    fn synthetic_radify_json_preserves_default_skin_false_zero_and_clear() {
        // No generated Preferences.json is present in the supplied archive, so
        // this shape evidence is intentionally and visibly synthetic.
        let bytes = br#"{
            "Defaults": {"EnableGlow": false, "OuterRimWidth": 0, "TextFont": ""},
            "Skins": {"Dark": {"EnableItemText": false, "ItemSize": 0, "CenterImage": null}}
        }"#;
        let preview = preview_radify(
            &[input(
                "Preferences.JSON",
                bytes,
                ImportEvidence::SyntheticFixture,
            )],
            "Imported",
            &BTreeSet::new(),
            &BTreeSet::new(),
        );
        let settings = preview.radify_settings.unwrap();
        assert_eq!(settings.defaults["EnableGlow"], Value::Bool(false));
        assert_eq!(settings.defaults["OuterRimWidth"], Value::from(0));
        assert_eq!(settings.defaults["TextFont"], Value::String(String::new()));
        assert_eq!(settings.skins["Dark"]["CenterImage"], Value::Null);
        assert!(
            preview
                .mappings
                .iter()
                .all(|mapping| mapping.provenance.evidence == ImportEvidence::SyntheticFixture)
        );
    }

    #[test]
    fn familiar_names_are_case_insensitive_and_native_vector_skin_needs_no_item_back() {
        let inputs = [
            input("ITEMBACK.PNG", b"png", ImportEvidence::UserSelected),
            input("ITEMGLOW.PNG", b"png", ImportEvidence::UserSelected),
            input("MENUBACK.PNG", b"png", ImportEvidence::UserSelected),
            input("MENUOUTERRIM.PNG", b"png", ImportEvidence::UserSelected),
            input("CENTERIMAGE.PNG", b"png", ImportEvidence::UserSelected),
            input("SUBMENUINDICATOR.PNG", b"png", ImportEvidence::UserSelected),
        ];
        let preview = preview_radify(&inputs, "Vector", &BTreeSet::new(), &BTreeSet::new());
        assert!(
            preview
                .discovered_files
                .iter()
                .all(|file| file.role == DiscoveredRole::FamiliarSkinMedia)
        );
        assert!(
            !preview
                .warnings
                .iter()
                .any(|warning| warning.kind == ImportWarningKind::MissingRequiredAsset)
        );
    }

    #[test]
    fn named_rm4_skin_requires_item_back_and_reports_exact_line_expression() {
        let definition = b"SkinName = Orb\nMenuBackSize = fac+0.34\nIconTrans = ARGB|0.94|0|0.05|0.66\nMystery = x\nCallback()\n";
        let preview = preview_rm4(
            &[input(
                "Skins/Orb/Skin definition.txt",
                definition,
                ImportEvidence::SuppliedReference,
            )],
            "Orb",
            &BTreeSet::new(),
            &BTreeSet::new(),
        );
        assert!(preview.mappings.iter().any(|mapping| mapping.value
            == ImportedValue::SizeExpression(SizeExpression::FactorOffset(0.34))));
        assert!(
            preview
                .warnings
                .iter()
                .any(|warning| warning.kind == ImportWarningKind::MissingRequiredAsset)
        );
        assert!(preview.warnings.iter().any(
            |warning| warning.line == Some(3) && warning.field.as_deref() == Some("IconTrans")
        ));
        assert!(
            preview
                .warnings
                .iter()
                .any(|warning| warning.line == Some(4)
                    && warning.field.as_deref() == Some("Mystery"))
        );
        assert!(
            preview
                .warnings
                .iter()
                .any(|warning| warning.line == Some(5)
                    && warning.kind == ImportWarningKind::IgnoredExecutableBehavior)
        );
    }

    #[test]
    fn rm4_literals_controls_and_required_media_are_typed_without_evaluation() {
        let definition = b"SkinName = Cell\nItemSize = 0\nTextColor = 000011\nTextShadow = false\nIconTrans = 128\nItemBack = ItemBack.png\nMenuClick = Close\nCenterClick = CloseMenu\nCenterRightClick = Drag\nMenuForeSize = add-2\n";
        let inputs = [
            input(
                "Skins/Cell/Skin definition.txt",
                definition,
                ImportEvidence::SyntheticFixture,
            ),
            input(
                "Skins/Cell/itemback.PNG",
                b"png",
                ImportEvidence::SyntheticFixture,
            ),
        ];
        let preview = preview_rm4(&inputs, "Cell", &BTreeSet::new(), &BTreeSet::new());
        assert!(
            !preview
                .warnings
                .iter()
                .any(|warning| warning.kind == ImportWarningKind::MissingRequiredAsset)
        );
        let values = preview
            .mappings
            .iter()
            .map(|mapping| &mapping.value)
            .collect::<Vec<_>>();
        assert!(values.contains(&&ImportedValue::Number(0.0)));
        assert!(values.contains(&&ImportedValue::Text("000011".to_string())));
        assert!(values.contains(&&ImportedValue::Bool(false)));
        assert!(values.contains(&&ImportedValue::Control(Control::Close)));
        assert!(values.contains(&&ImportedValue::Control(Control::Back)));
        assert!(values.contains(&&ImportedValue::Control(Control::Drag)));
        assert!(
            values.contains(&&ImportedValue::SizeExpression(SizeExpression::Additive(
                -2.0
            )))
        );
        let skin = &preview.definition.skins[0].style.values;
        assert_eq!(skin.geometry.item_size, Override::Value(0.0));
        assert_eq!(
            skin.text.color,
            Override::Value(ColorRgba {
                red: 0,
                green: 0,
                blue: 17,
                alpha: 255
            })
        );
        assert_eq!(skin.text.shadow_enabled, Override::Value(false));
        assert_eq!(skin.images.icon_opacity, Override::Value(128.0 / 255.0));
        assert_eq!(skin.geometry.menu_foreground_scale, Override::Value(0.98));
        assert_eq!(
            preview.definition.menus[0].center_control,
            Some(Control::Back)
        );
        assert_eq!(
            preview.definition.menus[0].center_secondary_control,
            Some(Control::Drag)
        );
        assert_eq!(
            preview.definition.menus[0].background_control,
            Some(Control::Close)
        );
        assert!(matches!(
            skin.images.item_background,
            Override::Value(MediaReference::Managed { .. })
        ));
    }

    #[test]
    fn radify_default_then_selected_skin_maps_false_zero_media_and_controls_into_plan() {
        let json = br#"{
          "Defaults": {"EnableGlow": false, "OuterRimWidth": 0, "CenterClick": "Close"},
          "Skins": {"Dark": {"EnableItemText": false, "ItemSize": 42,
                    "ItemBackgroundImageOnItems": false, "ItemBackgroundImage": "ItemBack.png"}}
        }"#;
        let png = base64::engine::general_purpose::STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=")
            .unwrap();
        let preview = preview_radify_skin(
            &[
                input("Preferences.json", json, ImportEvidence::SyntheticFixture),
                input("Dark/ItemBack.png", &png, ImportEvidence::SyntheticFixture),
            ],
            Some("Dark"),
            "Dark",
            &BTreeSet::new(),
            &BTreeSet::new(),
        );
        let style = &preview.definition.skins[0].style.values;
        assert_eq!(style.effects.glow_enabled, Override::Value(false));
        assert_eq!(style.geometry.outer_rim_width, Override::Value(0.0));
        assert_eq!(style.geometry.item_size, Override::Value(42.0));
        assert_eq!(
            style.geometry.item_background_on_items,
            Override::Value(false)
        );
        assert!(matches!(
            style.images.item_background,
            Override::Value(MediaReference::Managed { .. })
        ));
        assert_eq!(
            preview.definition.menus[0].center_control,
            Some(Control::Close)
        );
        let plan = preview.apply_plan();
        let bytes = serde_json::to_vec(&plan.document).unwrap();
        let decoded: RadialDocument = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            decoded.skins[0].style.values.geometry.item_size,
            Override::Value(42.0)
        );
        let managed = preview
            .definition
            .assets
            .iter()
            .map(|asset| (asset.id.clone(), png.clone()))
            .collect();
        let export = super::super::package::plan_export(
            &preview.definition,
            std::slice::from_ref(&preview.definition.default_menu_id),
            &managed,
            Vec::new(),
        )
        .unwrap();
        let imported =
            super::super::package::plan_import(export.files, &RadialDocument::starter()).unwrap();
        assert_eq!(
            imported.document.skins[0].style.values.geometry.item_size,
            Override::Value(42.0)
        );
        assert_eq!(
            imported.document.skins[0]
                .style
                .values
                .geometry
                .item_background_on_items,
            Override::Value(false)
        );
    }

    #[test]
    fn callbacks_and_numeric_expressions_are_ignored_with_field_diagnostics() {
        let radify = br#"{"Defaults":{"MenuClick":"RunCallback()","CenterClick":"Close"}}"#;
        let preview = preview_radify(
            &[input(
                "Preferences.json",
                radify,
                ImportEvidence::SyntheticFixture,
            )],
            "callbacks",
            &BTreeSet::new(),
            &BTreeSet::new(),
        );
        assert!(preview.mappings.iter().any(|mapping| {
            mapping.provenance.field == "CenterClick"
                && mapping.value == ImportedValue::Control(Control::Close)
        }));
        assert!(preview.warnings.iter().any(|warning| {
            warning.kind == ImportWarningKind::IgnoredExecutableBehavior
                && warning.field.as_deref() == Some("MenuClick")
        }));

        let rm4 = preview_rm4(
            &[input(
                "Skins/X/Skin definition.txt",
                b"SkinName = X\nItemSize = base+1\nMenuClick = callback()\n",
                ImportEvidence::SyntheticFixture,
            )],
            "X",
            &BTreeSet::new(),
            &BTreeSet::new(),
        );
        assert!(rm4.warnings.iter().any(|warning| {
            warning.line == Some(2)
                && warning.field.as_deref() == Some("ItemSize")
                && warning.kind == ImportWarningKind::UnsupportedExpression
        }));
        assert!(rm4.warnings.iter().any(|warning| {
            warning.line == Some(3)
                && warning.field.as_deref() == Some("MenuClick")
                && warning.kind == ImportWarningKind::IgnoredExecutableBehavior
        }));
    }

    #[test]
    fn supplied_reference_inventory_is_explicit_and_not_runtime_input() {
        assert_eq!(SUPPLIED_RM4_DEFINITIONS.len(), 5);
        assert!(
            SUPPLIED_RM4_DEFINITIONS
                .iter()
                .all(|(path, bytes)| path.ends_with("Skin definition.txt") && *bytes > 800)
        );
        assert_eq!(SUPPLIED_RM4_REPRESENTATIVE_MEDIA.len(), 5);
        assert!(
            SUPPLIED_RM4_REPRESENTATIVE_MEDIA
                .iter()
                .all(|(path, bytes, (width, height))| path.ends_with(".png")
                    && *bytes > 0
                    && *width > 0
                    && *height > 0)
        );
    }

    #[test]
    fn preview_reports_collisions_portability_and_stable_new_ids_without_writes() {
        let inputs = [
            input("Skins/A/ItemBack.png", b"a", ImportEvidence::UserSelected),
            input("skins/a/itemback.PNG", b"b", ImportEvidence::UserSelected),
            input(
                "C:\\external\\MenuBack.png",
                b"c",
                ImportEvidence::UserSelected,
            ),
        ];
        let occupied = BTreeSet::from([
            "import-menu-legacy-2".to_string(),
            "import-menu-legacy".to_string(),
        ]);
        let preview = preview_radify(
            &inputs,
            "Legacy",
            &occupied,
            &BTreeSet::from(["import-skin-legacy".to_string()]),
        );
        assert_eq!(preview.destination.menu_id.as_str(), "import-menu-legacy-3");
        assert_eq!(preview.destination.skin_id.as_str(), "import-skin-legacy-2");
        assert!(
            preview
                .warnings
                .iter()
                .any(|warning| warning.kind == ImportWarningKind::Collision)
        );
        assert!(
            preview
                .warnings
                .iter()
                .any(|warning| warning.kind == ImportWarningKind::NonPortablePath)
        );
    }

    #[test]
    fn every_registry_field_has_a_preview_policy() {
        for source in [
            CompatibilitySource::Radify,
            CompatibilitySource::RadialMenuV4,
        ] {
            let coverage = registry_coverage(source);
            let expected = FIELD_REGISTRY
                .iter()
                .filter(|group| group.source == source)
                .map(|group| group.fields.len())
                .sum::<usize>();
            assert_eq!(coverage.len(), expected);
            for group in FIELD_REGISTRY.iter().filter(|group| group.source == source) {
                for field in group.fields {
                    assert_eq!(coverage.get(*field), Some(&group.classification));
                    assert!(!group.import.is_empty());
                    if matches!(
                        group.classification,
                        CompatibilityClassification::Native
                            | CompatibilityClassification::Translated
                    ) {
                        assert!(
                            application_policy(source, field).is_some(),
                            "accepted field {source:?}.{field} lacks an apply policy"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn source_skin_identity_is_independent_of_destination_name_and_media_basename() {
        let settings = br#"{
          "Defaults": {"AutoTooltip": true, "ItemBackgroundImage": "ItemBack.png"},
          "Skins": {
            "Blue": {"AutoTooltip": false, "ItemBackgroundImage": "ItemBack.png"},
            "Red": {"ItemBackgroundImage": "ItemBack.png"}
          }
        }"#;
        let preview = preview_radify_skin(
            &[
                input(
                    "Preferences.json",
                    settings,
                    ImportEvidence::SyntheticFixture,
                ),
                input(
                    "arbitrary/Skins/Blue/ItemBack.png",
                    b"blue",
                    ImportEvidence::SyntheticFixture,
                ),
                input("Red/ItemBack.png", b"red", ImportEvidence::SyntheticFixture),
            ],
            Some("Blue"),
            "Renamed destination",
            &BTreeSet::new(),
            &BTreeSet::new(),
        );
        assert_eq!(preview.source_skin.as_deref(), Some("Blue"));
        assert_eq!(preview.definition.skins[0].name, "Renamed destination");
        assert_eq!(
            preview.definition.skins[0]
                .style
                .values
                .effects
                .tooltip_mode,
            Override::Value(TooltipMode::Disabled)
        );
        let Override::Value(MediaReference::Managed { asset_id }) = &preview.definition.skins[0]
            .style
            .values
            .images
            .item_background
        else {
            panic!("selected skin media was not applied")
        };
        let record = preview
            .definition
            .assets
            .iter()
            .find(|asset| &asset.id == asset_id)
            .unwrap();
        assert_eq!(record.content_sha256, sha256_hex(b"blue"));
    }

    #[test]
    fn duplicate_imported_media_bytes_share_one_content_addressed_asset() {
        let settings = br#"{"Defaults":{"ItemBackgroundImage":"A.png","CenterImage":"B.png"}}"#;
        let png = b"identical-png";
        let preview = preview_radify(
            &[
                input(
                    "Preferences.json",
                    settings,
                    ImportEvidence::SyntheticFixture,
                ),
                input("A.png", png, ImportEvidence::SyntheticFixture),
                input("B.png", png, ImportEvidence::SyntheticFixture),
            ],
            "Deduplicated",
            &BTreeSet::new(),
            &BTreeSet::new(),
        );
        assert_eq!(preview.definition.assets.len(), 1);
        assert_eq!(preview.asset_bytes.len(), 1);
        let images = &preview.definition.skins[0].style.values.images;
        assert_eq!(images.item_background, images.center_image);
    }

    #[test]
    fn ambiguous_basename_never_depends_on_reverse_input_order() {
        let settings = br#"{"Defaults":{"MenuBackgroundImage":"MenuBack.png"}}"#;
        let first = [
            input(
                "Preferences.json",
                settings,
                ImportEvidence::SyntheticFixture,
            ),
            input(
                "Skins/A/MenuBack.png",
                b"a",
                ImportEvidence::SyntheticFixture,
            ),
            input(
                "Skins/B/MenuBack.png",
                b"b",
                ImportEvidence::SyntheticFixture,
            ),
        ];
        let second = [
            input(
                "Preferences.json",
                settings,
                ImportEvidence::SyntheticFixture,
            ),
            input(
                "Skins/B/MenuBack.png",
                b"b",
                ImportEvidence::SyntheticFixture,
            ),
            input(
                "Skins/A/MenuBack.png",
                b"a",
                ImportEvidence::SyntheticFixture,
            ),
        ];
        for inputs in [&first[..], &second[..]] {
            let preview = preview_radify(inputs, "Ambiguous", &BTreeSet::new(), &BTreeSet::new());
            assert_eq!(
                preview.definition.skins[0]
                    .style
                    .values
                    .images
                    .menu_background,
                Override::Inherit
            );
            assert!(preview.warnings.iter().any(|warning| {
                warning.field.as_deref() == Some("MenuBackgroundImage")
                    && warning.kind == ImportWarningKind::MissingRequiredAsset
            }));
        }
    }

    #[test]
    fn source_skin_matches_immediate_media_parent_not_an_arbitrary_component() {
        let settings = br#"{"Defaults":{"MenuBackgroundImage":"MenuBack.png"}}"#;
        let preview = preview_radify_skin(
            &[
                input(
                    "Preferences.json",
                    settings,
                    ImportEvidence::SyntheticFixture,
                ),
                input(
                    "Blue/Skins/Red/MenuBack.png",
                    b"red",
                    ImportEvidence::SyntheticFixture,
                ),
            ],
            Some("Blue"),
            "Renamed",
            &BTreeSet::new(),
            &BTreeSet::new(),
        );
        assert_eq!(
            preview.definition.skins[0]
                .style
                .values
                .images
                .menu_background,
            Override::Inherit
        );
        assert!(preview.warnings.iter().any(|warning| {
            warning.field.as_deref() == Some("MenuBackgroundImage")
                && warning.kind == ImportWarningKind::MissingRequiredAsset
        }));
    }

    #[test]
    fn dialect_controls_root_sibling_vs_selected_skin_media_precedence() {
        let settings = br#"{"Defaults":{"MenuBackgroundImage":"MenuBack.png"}}"#;
        let radify = preview_radify_skin(
            &[
                input(
                    "Bundle/Preferences.json",
                    settings,
                    ImportEvidence::SyntheticFixture,
                ),
                input(
                    "Bundle/MenuBack.png",
                    b"root",
                    ImportEvidence::SyntheticFixture,
                ),
                input(
                    "Bundle/Skins/Blue/MenuBack.png",
                    b"blue",
                    ImportEvidence::SyntheticFixture,
                ),
            ],
            Some("Blue"),
            "Blue",
            &BTreeSet::new(),
            &BTreeSet::new(),
        );
        let Override::Value(MediaReference::Managed { asset_id }) = &radify.definition.skins[0]
            .style
            .values
            .images
            .menu_background
        else {
            panic!("Radify selected-skin image missing")
        };
        assert_eq!(
            radify
                .definition
                .assets
                .iter()
                .find(|asset| &asset.id == asset_id)
                .unwrap()
                .content_sha256,
            sha256_hex(b"blue")
        );

        let rm4 = preview_rm4(
            &[
                input(
                    "Bundle/Skins/Red/Skin definition.txt",
                    b"SkinName=Red\nMenuBack=MenuBack.png\n",
                    ImportEvidence::SyntheticFixture,
                ),
                input(
                    "Bundle/Skins/Red/MenuBack.png",
                    b"sibling",
                    ImportEvidence::SyntheticFixture,
                ),
                input(
                    "Bundle/MenuBack.png",
                    b"root",
                    ImportEvidence::SyntheticFixture,
                ),
            ],
            "Red",
            &BTreeSet::new(),
            &BTreeSet::new(),
        );
        let Override::Value(MediaReference::Managed { asset_id }) =
            &rm4.definition.skins[0].style.values.images.menu_background
        else {
            panic!("RM4 sibling image missing")
        };
        assert_eq!(
            rm4.definition
                .assets
                .iter()
                .find(|asset| &asset.id == asset_id)
                .unwrap()
                .content_sha256,
            sha256_hex(b"sibling")
        );
    }

    #[test]
    fn case_folded_duplicate_paths_are_ambiguous_unless_content_deduplicates() {
        let settings = br#"{"Defaults":{"MenuBackgroundImage":"MenuBack.png"}}"#;
        let ambiguous = preview_radify_skin(
            &[
                input(
                    "Preferences.json",
                    settings,
                    ImportEvidence::SyntheticFixture,
                ),
                input(
                    "Skins/Blue/MenuBack.png",
                    b"one",
                    ImportEvidence::SyntheticFixture,
                ),
                input(
                    "skins/blue/menuback.PNG",
                    b"two",
                    ImportEvidence::SyntheticFixture,
                ),
            ],
            Some("Blue"),
            "Blue",
            &BTreeSet::new(),
            &BTreeSet::new(),
        );
        assert_eq!(
            ambiguous.definition.skins[0]
                .style
                .values
                .images
                .menu_background,
            Override::Inherit
        );
        let deduped = preview_radify_skin(
            &[
                input(
                    "Preferences.json",
                    settings,
                    ImportEvidence::SyntheticFixture,
                ),
                input(
                    "Skins/Blue/MenuBack.png",
                    b"same",
                    ImportEvidence::SyntheticFixture,
                ),
                input(
                    "skins/blue/menuback.PNG",
                    b"same",
                    ImportEvidence::SyntheticFixture,
                ),
            ],
            Some("Blue"),
            "Blue",
            &BTreeSet::new(),
            &BTreeSet::new(),
        );
        assert!(matches!(
            deduped.definition.skins[0]
                .style
                .values
                .images
                .menu_background,
            Override::Value(MediaReference::Managed { .. })
        ));
        assert_eq!(deduped.definition.assets.len(), 1);
    }

    #[test]
    fn full_digest_asset_ids_cannot_alias_on_a_shared_prefix() {
        let prefix = "0123456789abcdef";
        let left = format!("{prefix}{}", "0".repeat(48));
        let right = format!("{prefix}{}", "f".repeat(48));
        let left_id = imported_asset_id(MediaKind::Image, &left);
        let right_id = imported_asset_id(MediaKind::Image, &right);
        assert_ne!(left_id, right_id);
        assert!(left_id.as_str().ends_with(&left));
        assert!(right_id.as_str().ends_with(&right));
        assert_ne!(
            imported_asset_id(MediaKind::Image, &left),
            imported_asset_id(MediaKind::Sound, &left)
        );
    }

    #[test]
    fn every_accepted_field_rejects_an_invalid_json_value_with_a_field_diagnostic() {
        for group in FIELD_REGISTRY.iter().filter(|group| {
            matches!(
                group.classification,
                CompatibilityClassification::Native | CompatibilityClassification::Translated
            )
        }) {
            for field in group.fields {
                if matches!(
                    application_policy(group.source, field),
                    Some(ImportApplicationPolicy::Diagnosed(_))
                ) {
                    continue;
                }
                let mut document = legacy_import_template();
                let mapping = ImportMapping {
                    destination_field: field.to_string(),
                    value: ImportedValue::Structured(Value::Array(vec![Value::Bool(true)])),
                    classification: group.classification,
                    provenance: ImportProvenance {
                        source: group.source,
                        dialect: match group.source {
                            CompatibilitySource::Radify => ImportDialect::RadifySettings,
                            CompatibilitySource::RadialMenuV4 => ImportDialect::Rm4Definition,
                        },
                        file: "invalid.fixture".into(),
                        line: Some(1),
                        field: field.to_string(),
                        evidence: ImportEvidence::SyntheticFixture,
                    },
                };
                let warnings =
                    apply_import_mappings(&mut document, None, &[mapping], &BTreeMap::new());
                assert!(
                    warnings
                        .iter()
                        .any(|warning| warning.field.as_deref() == Some(field)),
                    "invalid accepted value was silent for {:?}.{field}",
                    group.source
                );
            }
        }
    }

    #[test]
    fn every_accepted_field_has_a_semantic_valid_value_outcome() {
        let media: BTreeMap<String, Vec<MediaReference>> = BTreeMap::from([
            (
                "asset.png".into(),
                vec![MediaReference::Managed {
                    asset_id: AssetId::new(format!("import-image-{}", "a".repeat(64))),
                }],
            ),
            (
                "asset.wav".into(),
                vec![MediaReference::Managed {
                    asset_id: AssetId::new(format!("import-sound-{}", "b".repeat(64))),
                }],
            ),
        ]);
        for group in FIELD_REGISTRY.iter().filter(|group| {
            matches!(
                group.classification,
                CompatibilityClassification::Native | CompatibilityClassification::Translated
            )
        }) {
            for field in group.fields {
                if matches!(
                    application_policy(group.source, field),
                    Some(ImportApplicationPolicy::Diagnosed(_))
                ) {
                    continue;
                }
                let value = representative_value(field);
                let mapping = ImportMapping {
                    destination_field: field.to_string(),
                    value,
                    classification: group.classification,
                    provenance: ImportProvenance {
                        source: group.source,
                        dialect: match group.source {
                            CompatibilitySource::Radify => ImportDialect::RadifySettings,
                            CompatibilitySource::RadialMenuV4 => ImportDialect::Rm4Definition,
                        },
                        file: "valid.fixture".into(),
                        line: Some(1),
                        field: field.to_string(),
                        evidence: ImportEvidence::SyntheticFixture,
                    },
                };
                let mut document = legacy_import_template();
                if (field.starts_with("Hotkey") || field.starts_with("Hotstring"))
                    && gesture_for_legacy_field(field) != ClickGesture::Primary
                {
                    document.menus[0].rings[0].cells[0].alternate_controls.push(
                        ControlClickBinding {
                            gesture: gesture_for_legacy_field(field),
                            control: Control::Close,
                        },
                    );
                }
                for (name, values) in &media {
                    let MediaReference::Managed { asset_id } = &values[0] else {
                        continue;
                    };
                    let sound = name.ends_with(".wav");
                    document.assets.push(AssetRecord {
                        id: asset_id.clone(),
                        kind: if sound {
                            MediaKind::Sound
                        } else {
                            MediaKind::Image
                        },
                        relative_path: if sound { "valid.wav" } else { "valid.png" }.into(),
                        content_sha256: if sound { "b" } else { "a" }.repeat(64),
                        byte_len: 1,
                    });
                }
                let before = serde_json::to_value(&document).unwrap();
                let warnings =
                    apply_import_mappings(&mut document, None, &[mapping.clone()], &media);
                let changed = serde_json::to_value(&document).unwrap() != before;
                let diagnosed = warnings
                    .iter()
                    .any(|warning| warning.field.as_deref() == Some(field));
                let already_equal = if !changed && !diagnosed {
                    let lookup = |_mapping: &ImportMapping, name: &str| {
                        media
                            .get(&name.to_ascii_lowercase())
                            .and_then(|values| values.as_slice().first())
                            .cloned()
                    };
                    classify_unchanged_mapping(field, &mapping, &lookup)
                        == ApplyOutcome::AlreadyEqual
                } else {
                    false
                };
                assert!(
                    changed || diagnosed || already_equal,
                    "accepted field {:?}.{field} has no typed outcome",
                    group.source
                );
            }
        }
    }

    #[test]
    fn apply_outcomes_distinguish_changed_equal_invalid_missing_and_diagnosed() {
        let provenance = |field: &str| ImportProvenance {
            source: CompatibilitySource::Radify,
            dialect: ImportDialect::RadifySettings,
            file: "outcomes.fixture".into(),
            line: Some(1),
            field: field.into(),
            evidence: ImportEvidence::SyntheticFixture,
        };
        let no_assets = |_mapping: &ImportMapping, _name: &str| None;
        let equal = ImportMapping {
            destination_field: "EnableGlow".into(),
            value: ImportedValue::Bool(false),
            classification: CompatibilityClassification::Native,
            provenance: provenance("EnableGlow"),
        };
        assert_eq!(
            classify_unchanged_mapping("EnableGlow", &equal, &no_assets),
            ApplyOutcome::AlreadyEqual
        );
        let mut equal_document = legacy_import_template();
        let mirror_equal = ImportMapping {
            destination_field: "MirrorClickToRightClick".into(),
            value: ImportedValue::Bool(false),
            classification: CompatibilityClassification::Native,
            provenance: provenance("MirrorClickToRightClick"),
        };
        let equal_before = equal_document.clone();
        assert!(
            apply_import_mappings(&mut equal_document, None, &[mirror_equal], &BTreeMap::new(),)
                .is_empty()
        );
        assert_eq!(equal_document, equal_before);
        let invalid = ImportMapping {
            destination_field: "EnableGlow".into(),
            value: ImportedValue::Text("not-a-bool".into()),
            classification: CompatibilityClassification::Native,
            provenance: provenance("EnableGlow"),
        };
        assert!(matches!(
            classify_unchanged_mapping("EnableGlow", &invalid, &no_assets),
            ApplyOutcome::Invalid(_)
        ));
        let missing = ImportMapping {
            destination_field: "MenuBackgroundImage".into(),
            value: ImportedValue::MediaFilename("missing.png".into()),
            classification: CompatibilityClassification::Native,
            provenance: provenance("MenuBackgroundImage"),
        };
        assert!(matches!(
            classify_unchanged_mapping("MenuBackgroundImage", &missing, &no_assets),
            ApplyOutcome::MissingAsset(_)
        ));
        let diagnosed = ImportMapping {
            destination_field: "Submenu".into(),
            value: ImportedValue::Text("child".into()),
            classification: CompatibilityClassification::Native,
            provenance: provenance("Submenu"),
        };
        assert!(matches!(
            classify_unchanged_mapping("Submenu", &diagnosed, &no_assets),
            ApplyOutcome::ExplicitlyDiagnosed(_)
        ));

        let mut document = legacy_import_template();
        let changed = ImportMapping {
            destination_field: "ItemSize".into(),
            value: ImportedValue::Number(99.0),
            classification: CompatibilityClassification::Native,
            provenance: provenance("ItemSize"),
        };
        assert!(
            apply_import_mappings(&mut document, None, &[changed], &BTreeMap::new()).is_empty()
        );
        assert_eq!(
            document.skins[0].style.values.geometry.item_size,
            Override::Value(99.0)
        );
    }

    #[test]
    fn invalid_hotkey_and_numeric_range_are_transactional_and_unchanged() {
        for (field, value) in [
            ("HotkeyClick", ImportedValue::Text("Ctrl+".into())),
            ("ItemSize", ImportedValue::Number(-500.0)),
        ] {
            let mut document = legacy_import_template();
            let before = document.clone();
            let mapping = ImportMapping {
                destination_field: field.into(),
                value,
                classification: CompatibilityClassification::Native,
                provenance: ImportProvenance {
                    source: CompatibilitySource::Radify,
                    dialect: ImportDialect::RadifySettings,
                    file: "invalid.fixture".into(),
                    line: Some(1),
                    field: field.into(),
                    evidence: ImportEvidence::SyntheticFixture,
                },
            };
            let warnings = apply_import_mappings(&mut document, None, &[mapping], &BTreeMap::new());
            assert_eq!(document, before);
            assert!(warnings.iter().any(|warning| {
                warning.field.as_deref() == Some(field) && warning.message.contains("invalid")
            }));
        }
    }

    fn representative_value(field: &str) -> ImportedValue {
        if is_action_field(field) {
            return ImportedValue::Control(Control::Close);
        }
        if field.starts_with("Hotkey") || field.starts_with("Hotstring") {
            return ImportedValue::Text(
                if field.starts_with("Hotkey") {
                    "Ctrl+A"
                } else {
                    "typed"
                }
                .into(),
            );
        }
        if matches!(
            field,
            "ItemBackgroundImageOnCenter"
                | "ItemBackgroundImageOnItems"
                | "EnableItemText"
                | "TextShadow"
                | "MirrorClickToRightClick"
                | "CloseOnItemClick"
                | "CloseOnItemRightClick"
                | "EnableGlow"
                | "AutoTooltip"
                | "EnableTooltip"
                | "AlwaysOnTop"
                | "ActivateOnShow"
                | "FillCenterHitZone"
                | "FillItemsHitZone"
                | "AutoSubmenuMarking"
        ) {
            return ImportedValue::Bool(false);
        }
        if is_media_field(field)
            || field.ends_with("Image")
            || field.ends_with("ImageOnCenter")
            || field.ends_with("ImageOnItems")
        {
            return ImportedValue::MediaFilename("asset.png".into());
        }
        if field.starts_with("SoundOn") {
            return ImportedValue::MediaFilename("asset.wav".into());
        }
        if matches!(field, "MenuBackSize" | "MenuForeSize") {
            return ImportedValue::SizeExpression(SizeExpression::Additive(17.0));
        }
        if matches!(
            field,
            "TextColor" | "TextShadowColor" | "MenuShadowInnerColor" | "MenuShadowOuterColor"
        ) {
            return ImportedValue::Text("123456".into());
        }
        if matches!(field, "Skin" | "SkinName" | "TextFont" | "AutoSubmenuMark") {
            return ImportedValue::Text("typed-value".into());
        }
        if field == "TextFontOptions" {
            return ImportedValue::Text("bold italic".into());
        }
        if matches!(field, "Image" | "Tooltip" | "Text") {
            return ImportedValue::Text("typed-value".into());
        }
        ImportedValue::Number(42.0)
    }

    #[test]
    fn click_and_item_input_variants_apply_to_typed_cell_contracts() {
        let settings = br#"{"Defaults":{
          "RightClick":"Close","CtrlClick":"CloseMenu","ShiftClick":"Drag","AltClick":"Close",
          "HotkeyClick":"A","HotkeyRightClick":"B","HotkeyCtrlClick":"C","HotkeyShiftClick":"D","HotkeyAltClick":"E",
          "HotstringClick":"aa","HotstringRightClick":"bb","HotstringCtrlClick":"cc","HotstringShiftClick":"dd","HotstringAltClick":"ee",
          "CloseOnItemRightClick":false
        }}"#;
        let preview = preview_radify(
            &[input(
                "Preferences.json",
                settings,
                ImportEvidence::SyntheticFixture,
            )],
            "Inputs",
            &BTreeSet::new(),
            &BTreeSet::new(),
        );
        let cell = &preview.definition.menus[0].rings[0].cells[0];
        assert_eq!(cell.alternate_controls.len(), 4);
        assert_eq!(cell.shortcuts.len(), 5);
        assert_eq!(cell.hotstrings.len(), 5);
        assert_eq!(
            cell.secondary_after_action,
            super::super::model::AfterActionPolicy::KeepOpen
        );
        assert!(
            cell.shortcuts
                .iter()
                .all(|binding| binding.scope == TriggerScope::MenuLocal)
        );
        assert!(
            cell.hotstrings
                .iter()
                .all(|binding| binding.scope == TriggerScope::MenuLocal)
        );
    }

    #[test]
    fn selected_skin_upserts_default_shortcut_and_hotstring_slots() {
        let settings = br#"{
          "Defaults":{"HotkeyClick":"Ctrl+A","HotstringClick":"aa"},
          "Skins":{"Blue":{"HotkeyClick":"Ctrl+B","HotstringClick":"bb"}}
        }"#;
        let preview = preview_radify_skin(
            &[input(
                "Preferences.json",
                settings,
                ImportEvidence::SyntheticFixture,
            )],
            Some("Blue"),
            "Renamed",
            &BTreeSet::new(),
            &BTreeSet::new(),
        );
        let cell = &preview.definition.menus[0].rings[0].cells[0];
        assert_eq!(cell.shortcuts.len(), 1);
        assert_eq!(cell.shortcuts[0].chord, "Ctrl+B");
        assert_eq!(cell.hotstrings.len(), 1);
        assert_eq!(cell.hotstrings[0].text, "bb");
        assert!(!preview.warnings.iter().any(|warning| {
            matches!(
                warning.field.as_deref(),
                Some("HotkeyClick" | "HotstringClick")
            )
        }));
    }

    #[test]
    fn submenu_fields_each_have_exactly_one_explicit_diagnostic() {
        let settings = br#"{"Defaults":{"Submenu":"Child","SubmenuOptions":"same_center"}}"#;
        let preview = preview_radify(
            &[input(
                "Preferences.json",
                settings,
                ImportEvidence::SyntheticFixture,
            )],
            "Submenu diagnostics",
            &BTreeSet::new(),
            &BTreeSet::new(),
        );
        for field in ["Submenu", "SubmenuOptions"] {
            let diagnostics = preview
                .warnings
                .iter()
                .filter(|warning| warning.field.as_deref() == Some(field))
                .collect::<Vec<_>>();
            assert_eq!(diagnostics.len(), 1, "duplicate diagnostic for {field}");
            assert_eq!(diagnostics[0].kind, ImportWarningKind::IncompatibleField);
            assert!(diagnostics[0].message.contains("legacy menu graph"));
        }
    }

    #[test]
    fn legacy_preview_produces_a_typed_portable_apply_plan_without_writes() {
        let png = base64::engine::general_purpose::STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=")
            .unwrap();
        let preview = preview_rm4(
            &[
                input(
                    "Skins/Legacy/Skin definition.txt",
                    b"SkinName = Legacy\nItemBack = ItemBack.png\n",
                    ImportEvidence::SyntheticFixture,
                ),
                input(
                    "Skins/Legacy/ItemBack.png",
                    &png,
                    ImportEvidence::SyntheticFixture,
                ),
            ],
            "Legacy",
            &BTreeSet::new(),
            &BTreeSet::new(),
        );
        assert_eq!(
            preview.definition.default_menu_id,
            preview.destination.menu_id
        );
        assert_eq!(preview.definition.assets.len(), 1);
        let plan = preview.apply_plan();
        assert_eq!(plan.document, preview.definition);
        assert_eq!(plan.assets, preview.asset_bytes);
        assert!(plan.assets.keys().all(|path| path.starts_with("assets/")));
    }
}
