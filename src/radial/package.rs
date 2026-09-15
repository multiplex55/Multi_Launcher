//! Portable radial packages and hostile-input validation.
//!
//! The package layer is pure: it plans imports/exports as immutable byte maps.
//! Only [`RadialStore`](super::store::RadialStore) may apply a plan to disk.

use super::model::{
    AssetId, AssetRecord, CellContent, ContextRuleId, HotstringId, MediaReference, MenuId,
    RadialDocument, ShortcutId, SkinDefinition, SkinId, TriggerId, limits,
};
use super::validation::{ValidationErrors, validate};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io::{Cursor, Read};
use std::path::Path;

pub const PACKAGE_VERSION: u32 = 1;
pub const MANIFEST_FILE: &str = "manifest.json";
pub const DOCUMENT_FILE: &str = "radial.json";
pub const SKIN_FILE: &str = "skin.json";
pub const MAX_PACKAGE_FILES: usize = 4_096;
pub const MAX_PACKAGE_PATH_BYTES: usize = 512;
pub const MAX_PACKAGE_DEPTH: usize = 24;
pub const MAX_PACKAGE_ENTRY_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_PACKAGE_EXPANDED_BYTES: u64 = limits::MAX_IMPORT_BYTES;
pub const MAX_PACKAGE_COMPRESSED_BYTES: u64 = limits::MAX_IMPORT_BYTES;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageManifest {
    pub package_version: u32,
    pub document_path: String,
    pub root_menu_ids: Vec<MenuId>,
    #[serde(default)]
    pub payload: PackagePayloadKind,
    pub files: Vec<PackageFileRecord>,
    pub notices: Vec<PackageNotice>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackagePayloadKind {
    #[default]
    MenuGraph,
    SkinBundle,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkinBundle {
    pub skin: SkinDefinition,
    pub assets: Vec<AssetRecord>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SkinImportPlan {
    pub manifest: PackageManifest,
    pub skin: SkinDefinition,
    pub asset_records: Vec<AssetRecord>,
    pub assets: BTreeMap<String, Vec<u8>>,
    pub remap: IdRemap,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageFileRecord {
    pub path: String,
    pub sha256: String,
    pub byte_len: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageNotice {
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageFile {
    pub path: String,
    pub bytes: Vec<u8>,
    pub kind: PackageEntryKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackageEntryKind {
    File,
    Symlink,
    ReparsePoint,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportPlan {
    pub manifest: PackageManifest,
    /// Plain-folder representation; the same entries are serialized to ZIP.
    pub files: BTreeMap<String, Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImportPlan {
    pub manifest: PackageManifest,
    pub document: RadialDocument,
    pub assets: BTreeMap<String, Vec<u8>>,
    pub remap: IdRemap,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IdRemap {
    pub menus: BTreeMap<String, String>,
    pub skins: BTreeMap<String, String>,
    pub rings: BTreeMap<String, String>,
    pub cells: BTreeMap<String, String>,
    pub assets: BTreeMap<String, String>,
    pub shortcuts: BTreeMap<String, String>,
    pub hotstrings: BTreeMap<String, String>,
    pub contexts: BTreeMap<String, String>,
    pub triggers: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PackageError {
    InvalidPath { path: String, reason: &'static str },
    CaseCollision { first: String, second: String },
    BudgetExceeded(&'static str),
    Malformed(String),
    UnsupportedVersion(u32),
    UnsupportedCompression(u16),
    EncryptedEntry(String),
    LinkEntry(String),
    ChecksumMismatch(String),
    MissingFile(String),
    MissingRoot(MenuId),
    MissingAsset(AssetId),
    NonPortableReference(String),
    InvalidMedia { asset: AssetId, reason: String },
    Validation(ValidationErrors),
    Serialize(String),
}

impl std::fmt::Display for PackageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for PackageError {}

pub fn plan_export(
    document: &RadialDocument,
    roots: &[MenuId],
    managed_asset_bytes: &BTreeMap<AssetId, Vec<u8>>,
    notices: Vec<PackageNotice>,
) -> Result<ExportPlan, PackageError> {
    validate(document).map_err(PackageError::Validation)?;
    let menu_ids = submenu_closure(document, roots)?;
    let skin_ids = document
        .menus
        .iter()
        .filter(|menu| menu_ids.contains(menu.id.as_str()))
        .map(|menu| menu.skin_id.as_str().to_string())
        .collect::<BTreeSet<_>>();
    let mut subset = document.clone();
    subset.default_menu_id = roots
        .first()
        .cloned()
        .ok_or_else(|| PackageError::Malformed("at least one root menu is required".into()))?;
    subset
        .menus
        .retain(|menu| menu_ids.contains(menu.id.as_str()));
    subset
        .skins
        .retain(|skin| skin_ids.contains(skin.id.as_str()));
    subset
        .context_rules
        .retain(|rule| menu_ids.contains(rule.menu_id.as_str()));
    subset
        .custom_triggers
        .retain(|trigger| menu_ids.contains(trigger.menu_id.as_str()));
    subset.media_search_roots = Default::default();
    subset.metadata.clear();

    ensure_portable_media(&subset)?;

    let referenced_assets = managed_asset_ids(&subset)?;
    subset
        .assets
        .retain(|asset| referenced_assets.contains(asset.id.as_str()));
    let available = subset
        .assets
        .iter()
        .map(|asset| asset.id.as_str())
        .collect::<BTreeSet<_>>();
    for id in &referenced_assets {
        if !available.contains(id.as_str()) {
            return Err(PackageError::MissingAsset(AssetId::new(id)));
        }
    }
    validate(&subset).map_err(PackageError::Validation)?;

    let mut files = BTreeMap::new();
    let document_bytes = serde_json::to_vec_pretty(&subset)
        .map_err(|error| PackageError::Serialize(error.to_string()))?;
    files.insert(DOCUMENT_FILE.to_string(), document_bytes);
    for asset in &subset.assets {
        let bytes = managed_asset_bytes
            .get(&asset.id)
            .ok_or_else(|| PackageError::MissingAsset(asset.id.clone()))?;
        if sha256_hex(bytes) != asset.content_sha256 || bytes.len() as u64 != asset.byte_len {
            return Err(PackageError::ChecksumMismatch(asset.relative_path.clone()));
        }
        let extension = Path::new(&asset.relative_path)
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| format!(".{value}"))
            .unwrap_or_default();
        files.insert(
            format!("assets/{}{}", asset.content_sha256, extension),
            bytes.clone(),
        );
    }
    validate_file_map(&files)?;
    let records = files
        .iter()
        .map(|(path, bytes)| PackageFileRecord {
            path: path.clone(),
            sha256: sha256_hex(bytes),
            byte_len: bytes.len() as u64,
        })
        .collect();
    let manifest = PackageManifest {
        package_version: PACKAGE_VERSION,
        document_path: DOCUMENT_FILE.into(),
        root_menu_ids: roots.to_vec(),
        payload: PackagePayloadKind::MenuGraph,
        files: records,
        notices,
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| PackageError::Serialize(error.to_string()))?;
    files.insert(MANIFEST_FILE.into(), manifest_bytes);
    validate_file_map(&files)?;
    Ok(ExportPlan { manifest, files })
}

pub fn plan_skin_export(
    document: &RadialDocument,
    skin_id: &SkinId,
    managed_asset_bytes: &BTreeMap<AssetId, Vec<u8>>,
    notices: Vec<PackageNotice>,
) -> Result<ExportPlan, PackageError> {
    validate(document).map_err(PackageError::Validation)?;
    let skin = document
        .skins
        .iter()
        .find(|skin| &skin.id == skin_id)
        .cloned()
        .ok_or_else(|| PackageError::Malformed(format!("skin {skin_id} is missing")))?;
    let skin_value =
        serde_json::to_value(&skin).map_err(|error| PackageError::Serialize(error.to_string()))?;
    reject_nonportable_media(&skin_value)?;
    let mut referenced = BTreeSet::new();
    collect_managed_ids(&skin_value, &mut referenced);
    let records = document
        .assets
        .iter()
        .filter(|asset| referenced.contains(asset.id.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    for id in &referenced {
        if !records.iter().any(|record| record.id.as_str() == id) {
            return Err(PackageError::MissingAsset(AssetId::new(id)));
        }
    }
    let bundle = SkinBundle {
        skin,
        assets: records.clone(),
    };
    let mut files = BTreeMap::from([(
        SKIN_FILE.to_owned(),
        serde_json::to_vec_pretty(&bundle)
            .map_err(|error| PackageError::Serialize(error.to_string()))?,
    )]);
    for asset in &records {
        let bytes = managed_asset_bytes
            .get(&asset.id)
            .ok_or_else(|| PackageError::MissingAsset(asset.id.clone()))?;
        if sha256_hex(bytes) != asset.content_sha256 || bytes.len() as u64 != asset.byte_len {
            return Err(PackageError::ChecksumMismatch(asset.relative_path.clone()));
        }
        let extension = Path::new(&asset.relative_path)
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| format!(".{value}"))
            .unwrap_or_default();
        files.insert(
            format!("assets/{}{}", asset.content_sha256, extension),
            bytes.clone(),
        );
    }
    validate_file_map(&files)?;
    let manifest = PackageManifest {
        package_version: PACKAGE_VERSION,
        document_path: SKIN_FILE.into(),
        root_menu_ids: Vec::new(),
        payload: PackagePayloadKind::SkinBundle,
        files: files
            .iter()
            .map(|(path, bytes)| PackageFileRecord {
                path: path.clone(),
                sha256: sha256_hex(bytes),
                byte_len: bytes.len() as u64,
            })
            .collect(),
        notices,
    };
    files.insert(
        MANIFEST_FILE.into(),
        serde_json::to_vec_pretty(&manifest)
            .map_err(|error| PackageError::Serialize(error.to_string()))?,
    );
    validate_file_map(&files)?;
    Ok(ExportPlan { manifest, files })
}

pub fn plan_skin_import(
    mut files: BTreeMap<String, Vec<u8>>,
    target: &RadialDocument,
) -> Result<SkinImportPlan, PackageError> {
    validate_file_map(&files)?;
    let manifest_bytes = files
        .remove(MANIFEST_FILE)
        .ok_or_else(|| PackageError::MissingFile(MANIFEST_FILE.into()))?;
    let manifest: PackageManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| PackageError::Malformed(error.to_string()))?;
    if manifest.package_version != PACKAGE_VERSION {
        return Err(PackageError::UnsupportedVersion(manifest.package_version));
    }
    if manifest.payload != PackagePayloadKind::SkinBundle || !manifest.root_menu_ids.is_empty() {
        return Err(PackageError::Malformed(
            "package is not a skin bundle".into(),
        ));
    }
    validate_package_path(&manifest.document_path)?;
    let expected = manifest
        .files
        .iter()
        .map(|record| (record.path.as_str(), record))
        .collect::<BTreeMap<_, _>>();
    if expected.len() != manifest.files.len() || expected.len() != files.len() {
        return Err(PackageError::Malformed(
            "manifest file list must exactly match package entries".into(),
        ));
    }
    for (path, bytes) in &files {
        let record = expected
            .get(path.as_str())
            .ok_or_else(|| PackageError::MissingFile(path.clone()))?;
        if record.byte_len != bytes.len() as u64 || record.sha256 != sha256_hex(bytes) {
            return Err(PackageError::ChecksumMismatch(path.clone()));
        }
    }
    let bundle_bytes = files
        .get(&manifest.document_path)
        .ok_or_else(|| PackageError::MissingFile(manifest.document_path.clone()))?;
    let mut bundle: SkinBundle = serde_json::from_slice(bundle_bytes)
        .map_err(|error| PackageError::Malformed(error.to_string()))?;
    let value = serde_json::to_value(&bundle.skin)
        .map_err(|error| PackageError::Serialize(error.to_string()))?;
    reject_nonportable_media(&value)?;
    let mut referenced = BTreeSet::new();
    collect_managed_ids(&value, &mut referenced);
    if referenced.len() != bundle.assets.len()
        || bundle
            .assets
            .iter()
            .any(|asset| !referenced.contains(asset.id.as_str()))
    {
        return Err(PackageError::Malformed(
            "skin asset records must exactly match its managed dependency closure".into(),
        ));
    }
    validate_exact_asset_files(&files, &bundle.assets)?;
    let mut remap = IdRemap::default();
    let mut skin_ids = target
        .skins
        .iter()
        .map(|skin| skin.id.as_str().to_owned())
        .collect::<BTreeSet<_>>();
    insert_remap(&mut remap.skins, bundle.skin.id.as_str(), &mut skin_ids);
    let mut asset_ids = target
        .assets
        .iter()
        .map(|asset| asset.id.as_str().to_owned())
        .collect::<BTreeSet<_>>();
    let mut reused_assets = BTreeSet::new();
    for asset in &bundle.assets {
        let packaged = files
            .iter()
            .find(|(path, bytes)| {
                path.starts_with("assets/")
                    && sha256_hex(bytes) == asset.content_sha256
                    && bytes.len() as u64 == asset.byte_len
            })
            .map(|(_, bytes)| bytes)
            .ok_or_else(|| PackageError::MissingAsset(asset.id.clone()))?;
        super::assets::validate_packaged_media(packaged, asset.kind).map_err(|reason| {
            PackageError::InvalidMedia {
                asset: asset.id.clone(),
                reason: reason.to_string(),
            }
        })?;
        if let Some(existing) = target.assets.iter().find(|existing| {
            existing.kind == asset.kind
                && existing
                    .content_sha256
                    .eq_ignore_ascii_case(&asset.content_sha256)
                && existing.byte_len == asset.byte_len
        }) {
            remap.assets.insert(
                asset.id.as_str().to_owned(),
                existing.id.as_str().to_owned(),
            );
            reused_assets.insert(asset.id.clone());
        } else if asset_ids.contains(asset.id.as_str()) {
            let kind = match asset.kind {
                super::model::MediaKind::Image => "image",
                super::model::MediaKind::Sound => "sound",
            };
            let generic = unique_id(
                &format!("asset-{kind}-{}", asset.content_sha256.to_ascii_lowercase()),
                &asset_ids,
            );
            asset_ids.insert(generic.clone());
            remap.assets.insert(asset.id.as_str().to_owned(), generic);
        } else {
            insert_remap(&mut remap.assets, asset.id.as_str(), &mut asset_ids);
        }
    }
    bundle.skin.id = SkinId::new(mapped(&remap.skins, bundle.skin.id.as_str()));
    bundle
        .assets
        .retain(|asset| !reused_assets.contains(&asset.id));
    for asset in &mut bundle.assets {
        let original = asset.id.clone();
        asset.id = AssetId::new(mapped(&remap.assets, asset.id.as_str()));
        if asset.id != original {
            let extension = Path::new(&asset.relative_path)
                .extension()
                .and_then(|value| value.to_str())
                .map(|value| format!(".{value}"))
                .unwrap_or_default();
            asset.relative_path = format!("{}{}", asset.id, extension);
        }
    }
    let mut skin_value = serde_json::to_value(&bundle.skin)
        .map_err(|error| PackageError::Serialize(error.to_string()))?;
    rewrite_managed_ids(&mut skin_value, &remap.assets);
    bundle.skin = serde_json::from_value(skin_value)
        .map_err(|error| PackageError::Malformed(error.to_string()))?;
    let retained_assets = bundle
        .assets
        .iter()
        .map(|asset| (asset.content_sha256.clone(), asset.byte_len))
        .collect::<BTreeSet<_>>();
    let mut candidate = target.clone();
    candidate.skins.push(bundle.skin.clone());
    candidate.assets.extend(bundle.assets.clone());
    validate(&candidate).map_err(PackageError::Validation)?;
    Ok(SkinImportPlan {
        manifest,
        skin: bundle.skin,
        asset_records: bundle.assets,
        assets: files
            .into_iter()
            .filter(|(path, bytes)| {
                path.starts_with("assets/")
                    && retained_assets.contains(&(sha256_hex(bytes), bytes.len() as u64))
            })
            .collect(),
        remap,
    })
}

pub fn plan_import(
    mut files: BTreeMap<String, Vec<u8>>,
    target: &RadialDocument,
) -> Result<ImportPlan, PackageError> {
    validate_file_map(&files)?;
    let manifest_bytes = files
        .remove(MANIFEST_FILE)
        .ok_or_else(|| PackageError::MissingFile(MANIFEST_FILE.into()))?;
    let manifest: PackageManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| PackageError::Malformed(error.to_string()))?;
    if manifest.package_version != PACKAGE_VERSION {
        return Err(PackageError::UnsupportedVersion(manifest.package_version));
    }
    if manifest.payload != PackagePayloadKind::MenuGraph {
        return Err(PackageError::Malformed(
            "package is not a menu graph".into(),
        ));
    }
    validate_package_path(&manifest.document_path)?;
    let expected = manifest
        .files
        .iter()
        .map(|record| (record.path.as_str(), record))
        .collect::<BTreeMap<_, _>>();
    if expected.len() != manifest.files.len() || expected.len() != files.len() {
        return Err(PackageError::Malformed(
            "manifest file list must exactly match package entries".into(),
        ));
    }
    for (path, bytes) in &files {
        let record = expected
            .get(path.as_str())
            .ok_or_else(|| PackageError::MissingFile(path.clone()))?;
        if record.byte_len != bytes.len() as u64 || record.sha256 != sha256_hex(bytes) {
            return Err(PackageError::ChecksumMismatch(path.clone()));
        }
    }
    let document_bytes = files
        .get(&manifest.document_path)
        .ok_or_else(|| PackageError::MissingFile(manifest.document_path.clone()))?;
    let mut document: RadialDocument = serde_json::from_slice(document_bytes)
        .map_err(|error| PackageError::Malformed(error.to_string()))?;
    validate(&document).map_err(PackageError::Validation)?;
    ensure_portable_media(&document)?;
    let referenced = managed_asset_ids(&document)?;
    if referenced.len() != document.assets.len()
        || document
            .assets
            .iter()
            .any(|asset| !referenced.contains(asset.id.as_str()))
    {
        return Err(PackageError::Malformed(
            "menu asset records must exactly match its managed dependency closure".into(),
        ));
    }
    validate_exact_asset_files(&files, &document.assets)?;
    for root in &manifest.root_menu_ids {
        if !document.menus.iter().any(|menu| menu.id == *root) {
            return Err(PackageError::MissingRoot(root.clone()));
        }
    }
    for asset in &document.assets {
        let bytes = files
            .iter()
            .find(|(path, bytes)| {
                path.starts_with("assets/")
                    && sha256_hex(bytes) == asset.content_sha256
                    && bytes.len() as u64 == asset.byte_len
            })
            .map(|(_, bytes)| bytes)
            .ok_or_else(|| PackageError::MissingAsset(asset.id.clone()))?;
        super::assets::validate_packaged_media(bytes, asset.kind).map_err(|reason| {
            PackageError::InvalidMedia {
                asset: asset.id.clone(),
                reason: reason.to_string(),
            }
        })?;
    }
    let remap = remap_document_ids(&mut document, target)?;
    let assets = files
        .into_iter()
        .filter(|(path, _)| path.starts_with("assets/"))
        .collect();
    Ok(ImportPlan {
        manifest,
        document,
        assets,
        remap,
    })
}

fn packaged_asset_path(asset: &AssetRecord) -> String {
    let extension = Path::new(&asset.relative_path)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| format!(".{value}"))
        .unwrap_or_default();
    format!("assets/{}{}", asset.content_sha256, extension)
}

fn validate_exact_asset_files(
    files: &BTreeMap<String, Vec<u8>>,
    records: &[AssetRecord],
) -> Result<(), PackageError> {
    let expected = records
        .iter()
        .map(packaged_asset_path)
        .collect::<BTreeSet<_>>();
    let actual = files
        .keys()
        .filter(|path| path.starts_with("assets/"))
        .cloned()
        .collect::<BTreeSet<_>>();
    if actual != expected {
        return Err(PackageError::Malformed(
            "asset files must exactly match the declared dependency closure".into(),
        ));
    }
    for asset in records {
        let path = packaged_asset_path(asset);
        let bytes = files
            .get(&path)
            .ok_or_else(|| PackageError::MissingAsset(asset.id.clone()))?;
        if sha256_hex(bytes) != asset.content_sha256 || bytes.len() as u64 != asset.byte_len {
            return Err(PackageError::ChecksumMismatch(path));
        }
    }
    Ok(())
}

/// Plain-folder adapters provide already-read entries plus link metadata. The
/// same hostile-input policy is used as for ZIP without ever following links.
pub fn plain_folder_file_map(
    entries: Vec<PackageFile>,
) -> Result<BTreeMap<String, Vec<u8>>, PackageError> {
    let mut files = BTreeMap::new();
    for entry in entries {
        if entry.kind != PackageEntryKind::File {
            return Err(PackageError::LinkEntry(entry.path));
        }
        validate_package_path(&entry.path)?;
        if files.insert(entry.path.clone(), entry.bytes).is_some() {
            return Err(PackageError::CaseCollision {
                first: entry.path.clone(),
                second: entry.path,
            });
        }
    }
    validate_file_map(&files)?;
    Ok(files)
}

pub fn encode_mlradial(plan: &ExportPlan) -> Result<Vec<u8>, PackageError> {
    validate_file_map(&plan.files)?;
    let mut output = Vec::new();
    let mut central = Vec::new();
    for (path, bytes) in &plan.files {
        let offset =
            u32::try_from(output.len()).map_err(|_| PackageError::BudgetExceeded("ZIP offset"))?;
        let size =
            u32::try_from(bytes.len()).map_err(|_| PackageError::BudgetExceeded("ZIP entry"))?;
        let name = path.as_bytes();
        let crc = crc32(bytes);
        put_u32(&mut output, 0x0403_4b50);
        put_u16(&mut output, 20);
        put_u16(&mut output, 1 << 11);
        put_u16(&mut output, 0);
        put_u16(&mut output, 0);
        put_u16(&mut output, 0);
        put_u32(&mut output, crc);
        put_u32(&mut output, size);
        put_u32(&mut output, size);
        put_u16(&mut output, name.len() as u16);
        put_u16(&mut output, 0);
        output.extend_from_slice(name);
        output.extend_from_slice(bytes);
        central.push((path, crc, size, offset));
    }
    let central_offset = output.len() as u32;
    for (path, crc, size, offset) in &central {
        let name = path.as_bytes();
        put_u32(&mut output, 0x0201_4b50);
        put_u16(&mut output, 20);
        put_u16(&mut output, 20);
        put_u16(&mut output, 1 << 11);
        put_u16(&mut output, 0);
        put_u16(&mut output, 0);
        put_u16(&mut output, 0);
        put_u32(&mut output, *crc);
        put_u32(&mut output, *size);
        put_u32(&mut output, *size);
        put_u16(&mut output, name.len() as u16);
        put_u16(&mut output, 0);
        put_u16(&mut output, 0);
        put_u16(&mut output, 0);
        put_u16(&mut output, 0);
        put_u32(&mut output, 0);
        put_u32(&mut output, *offset);
        output.extend_from_slice(name);
    }
    let central_size = output.len() as u32 - central_offset;
    put_u32(&mut output, 0x0605_4b50);
    put_u16(&mut output, 0);
    put_u16(&mut output, 0);
    put_u16(&mut output, central.len() as u16);
    put_u16(&mut output, central.len() as u16);
    put_u32(&mut output, central_size);
    put_u32(&mut output, central_offset);
    put_u16(&mut output, 0);
    Ok(output)
}

pub fn decode_mlradial(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, PackageError> {
    if bytes.len() as u64 > MAX_PACKAGE_COMPRESSED_BYTES {
        return Err(PackageError::BudgetExceeded("compressed bytes"));
    }
    let eocd =
        find_eocd(bytes).ok_or_else(|| PackageError::Malformed("missing ZIP end record".into()))?;
    let disk = get_u16(bytes, eocd + 4)?;
    let central_disk = get_u16(bytes, eocd + 6)?;
    let entries_on_disk = get_u16(bytes, eocd + 8)? as usize;
    let entries = get_u16(bytes, eocd + 10)? as usize;
    let central_size = get_u32(bytes, eocd + 12)? as usize;
    let central_offset = get_u32(bytes, eocd + 16)? as usize;
    let archive_comment = get_u16(bytes, eocd + 20)? as usize;
    if disk != 0
        || central_disk != 0
        || entries_on_disk != entries
        || eocd + 22 + archive_comment != bytes.len()
    {
        return Err(PackageError::Malformed(
            "multi-disk, appended, or ambiguous ZIP is unsupported".into(),
        ));
    }
    if entries > MAX_PACKAGE_FILES
        || central_offset
            .checked_add(central_size)
            .filter(|end| *end == eocd)
            .is_none()
    {
        return Err(PackageError::BudgetExceeded("ZIP central directory"));
    }
    let mut cursor = central_offset;
    let mut output = BTreeMap::new();
    let mut folded = BTreeMap::<String, String>::new();
    let mut local_ranges = Vec::<(usize, usize, String)>::new();
    let mut expanded = 0u64;
    for _ in 0..entries {
        if get_u32(bytes, cursor)? != 0x0201_4b50 {
            return Err(PackageError::Malformed("invalid ZIP central record".into()));
        }
        let flags = get_u16(bytes, cursor + 8)?;
        let method = get_u16(bytes, cursor + 10)?;
        let crc = get_u32(bytes, cursor + 16)?;
        let compressed = get_u32(bytes, cursor + 20)? as u64;
        let uncompressed = get_u32(bytes, cursor + 24)? as u64;
        let name_len = get_u16(bytes, cursor + 28)? as usize;
        let extra_len = get_u16(bytes, cursor + 30)? as usize;
        let comment_len = get_u16(bytes, cursor + 32)? as usize;
        let external = get_u32(bytes, cursor + 38)?;
        let local_offset = get_u32(bytes, cursor + 42)? as usize;
        let name_start = cursor + 46;
        let name_bytes = slice(bytes, name_start, name_len)?;
        if flags & 1 != 0 {
            return Err(PackageError::EncryptedEntry(
                String::from_utf8_lossy(name_bytes).into(),
            ));
        }
        if flags != 1 << 11 {
            return Err(PackageError::Malformed(
                "ZIP paths must be UTF-8 and data-descriptor/optional flags are unsupported".into(),
            ));
        }
        if method != 0 {
            return Err(PackageError::UnsupportedCompression(method));
        }
        if compressed != uncompressed {
            return Err(PackageError::Malformed("stored ZIP size mismatch".into()));
        }
        let name = std::str::from_utf8(name_bytes)
            .map_err(|_| PackageError::Malformed("non-Unicode ZIP path".into()))?
            .to_string();
        validate_package_path(&name)?;
        if (external >> 16) & 0o170000 == 0o120000 || external & 0x400 != 0 {
            return Err(PackageError::LinkEntry(name));
        }
        if let Some(first) = folded.insert(name.to_ascii_lowercase(), name.clone()) {
            return Err(PackageError::CaseCollision {
                first,
                second: name,
            });
        }
        expanded = expanded
            .checked_add(uncompressed)
            .ok_or(PackageError::BudgetExceeded("expanded bytes"))?;
        if uncompressed > MAX_PACKAGE_ENTRY_BYTES || expanded > MAX_PACKAGE_EXPANDED_BYTES {
            return Err(PackageError::BudgetExceeded("expanded bytes"));
        }
        if get_u32(bytes, local_offset)? != 0x0403_4b50 {
            return Err(PackageError::Malformed("invalid ZIP local record".into()));
        }
        let local_flags = get_u16(bytes, local_offset + 6)?;
        let local_method = get_u16(bytes, local_offset + 8)?;
        let local_crc = get_u32(bytes, local_offset + 14)?;
        let local_compressed = get_u32(bytes, local_offset + 18)? as u64;
        let local_uncompressed = get_u32(bytes, local_offset + 22)? as u64;
        let local_name_len = get_u16(bytes, local_offset + 26)? as usize;
        let local_extra_len = get_u16(bytes, local_offset + 28)? as usize;
        let local_name = slice(bytes, local_offset + 30, local_name_len)?;
        if extra_len != 0
            || comment_len != 0
            || local_extra_len != 0
            || local_flags != flags
            || local_method != method
            || local_crc != crc
            || local_compressed != compressed
            || local_uncompressed != uncompressed
            || local_name != name_bytes
        {
            return Err(PackageError::Malformed("ZIP local/central mismatch".into()));
        }
        let data_start = local_offset
            .checked_add(30)
            .and_then(|value| value.checked_add(local_name_len))
            .and_then(|value| value.checked_add(local_extra_len))
            .ok_or_else(|| PackageError::Malformed("ZIP local offset overflow".into()))?;
        let data_end = data_start
            .checked_add(compressed as usize)
            .filter(|end| *end <= central_offset)
            .ok_or_else(|| {
                PackageError::Malformed("ZIP local data crosses central directory".into())
            })?;
        if local_offset >= central_offset
            || local_ranges
                .iter()
                .any(|(start, end, _)| local_offset < *end && *start < data_end)
        {
            return Err(PackageError::Malformed(
                "ZIP local records overlap or alias".into(),
            ));
        }
        local_ranges.push((local_offset, data_end, name.clone()));
        let mut reader = Cursor::new(slice(bytes, data_start, compressed as usize)?);
        let mut data = Vec::with_capacity(uncompressed as usize);
        let mut limited = reader.by_ref().take(MAX_PACKAGE_ENTRY_BYTES + 1);
        limited
            .read_to_end(&mut data)
            .map_err(|error| PackageError::Malformed(error.to_string()))?;
        if data.len() as u64 != uncompressed || crc32(&data) != crc {
            return Err(PackageError::ChecksumMismatch(name));
        }
        output.insert(name, data);
        cursor = name_start + name_len + extra_len + comment_len;
    }
    if cursor != central_offset + central_size {
        return Err(PackageError::Malformed(
            "ZIP central directory size mismatch".into(),
        ));
    }
    local_ranges.sort_by_key(|(start, _, _)| *start);
    if local_ranges
        .first()
        .is_some_and(|(start, _, _)| *start != 0)
        || local_ranges.windows(2).any(|pair| pair[0].1 != pair[1].0)
        || local_ranges
            .last()
            .is_some_and(|(_, end, _)| *end != central_offset)
    {
        return Err(PackageError::Malformed(
            "ZIP contains unreferenced or ambiguous local bytes".into(),
        ));
    }
    validate_file_map(&output)?;
    Ok(output)
}

pub fn validate_package_path(path: &str) -> Result<(), PackageError> {
    if path.is_empty()
        || path.len() > MAX_PACKAGE_PATH_BYTES
        || path.contains('\\')
        || path.chars().any(|character| {
            character < ' ' || matches!(character, '<' | '>' | '"' | '|' | '?' | '*')
        })
    {
        return Err(PackageError::InvalidPath {
            path: path.into(),
            reason: "empty, long, or backslash path",
        });
    }
    if path.starts_with('/')
        || path.starts_with("//")
        || path.contains(':')
        || Path::new(path).is_absolute()
    {
        return Err(PackageError::InvalidPath {
            path: path.into(),
            reason: "absolute, drive, device, UNC, or ADS path",
        });
    }
    let components = path.split('/').collect::<Vec<_>>();
    if components.len() > MAX_PACKAGE_DEPTH {
        return Err(PackageError::InvalidPath {
            path: path.into(),
            reason: "path depth",
        });
    }
    for component in components {
        if component.is_empty()
            || component == "."
            || component == ".."
            || component.ends_with(['.', ' '])
        {
            return Err(PackageError::InvalidPath {
                path: path.into(),
                reason: "empty, traversal, or trailing dot/space component",
            });
        }
        let stem = component
            .split('.')
            .next()
            .unwrap_or(component)
            .to_ascii_uppercase();
        if is_reserved_dos_name(&stem) {
            return Err(PackageError::InvalidPath {
                path: path.into(),
                reason: "reserved DOS device name",
            });
        }
    }
    Ok(())
}

fn validate_file_map(files: &BTreeMap<String, Vec<u8>>) -> Result<(), PackageError> {
    if files.len() > MAX_PACKAGE_FILES {
        return Err(PackageError::BudgetExceeded("file count"));
    }
    let mut folded = BTreeMap::<String, String>::new();
    let mut total = 0u64;
    for (path, bytes) in files {
        validate_package_path(path)?;
        if bytes.len() as u64 > MAX_PACKAGE_ENTRY_BYTES {
            return Err(PackageError::BudgetExceeded("entry bytes"));
        }
        total = total
            .checked_add(bytes.len() as u64)
            .ok_or(PackageError::BudgetExceeded("expanded bytes"))?;
        if total > MAX_PACKAGE_EXPANDED_BYTES {
            return Err(PackageError::BudgetExceeded("expanded bytes"));
        }
        if let Some(first) = folded.insert(path.to_ascii_lowercase(), path.clone()) {
            return Err(PackageError::CaseCollision {
                first,
                second: path.clone(),
            });
        }
    }
    Ok(())
}

fn submenu_closure(
    document: &RadialDocument,
    roots: &[MenuId],
) -> Result<BTreeSet<String>, PackageError> {
    let mut found = BTreeSet::new();
    let mut queue = roots.iter().cloned().collect::<VecDeque<_>>();
    while let Some(id) = queue.pop_front() {
        if !found.insert(id.as_str().to_string()) {
            continue;
        }
        let menu = document
            .menus
            .iter()
            .find(|menu| menu.id == id)
            .ok_or_else(|| PackageError::MissingRoot(id.clone()))?;
        for cell in menu.rings.iter().flat_map(|ring| &ring.cells) {
            if let CellContent::Submenu { menu_id } = &cell.content {
                queue.push_back(menu_id.clone());
            }
        }
    }
    Ok(found)
}

fn managed_asset_ids(document: &RadialDocument) -> Result<BTreeSet<String>, PackageError> {
    let value = serde_json::to_value(document)
        .map_err(|error| PackageError::Serialize(error.to_string()))?;
    let mut ids = BTreeSet::new();
    collect_managed_ids(&value, &mut ids);
    Ok(ids)
}

fn ensure_portable_media(document: &RadialDocument) -> Result<(), PackageError> {
    let value = serde_json::to_value(document)
        .map_err(|error| PackageError::Serialize(error.to_string()))?;
    reject_nonportable_media(&value)
}

fn reject_nonportable_media(value: &serde_json::Value) -> Result<(), PackageError> {
    match value {
        serde_json::Value::Object(object) => {
            if let Some(kind @ ("external_file" | "search_path" | "icon_resource")) =
                object.get("kind").and_then(|value| value.as_str())
            {
                return Err(PackageError::NonPortableReference(kind.to_string()));
            }
            for value in object.values() {
                reject_nonportable_media(value)?;
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                reject_nonportable_media(value)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn collect_managed_ids(value: &serde_json::Value, ids: &mut BTreeSet<String>) {
    match value {
        serde_json::Value::Object(object) => {
            if object.get("kind").and_then(|value| value.as_str()) == Some("managed") {
                if let Some(id) = object.get("asset_id").and_then(|value| value.as_str()) {
                    ids.insert(id.to_string());
                }
            }
            for value in object.values() {
                collect_managed_ids(value, ids);
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                collect_managed_ids(value, ids);
            }
        }
        _ => {}
    }
}

fn remap_document_ids(
    document: &mut RadialDocument,
    target: &RadialDocument,
) -> Result<IdRemap, PackageError> {
    let mut remap = IdRemap::default();
    let mut menu_ids = target
        .menus
        .iter()
        .map(|value| value.id.as_str().to_string())
        .collect::<BTreeSet<_>>();
    let mut skin_ids = target
        .skins
        .iter()
        .map(|value| value.id.as_str().to_string())
        .collect::<BTreeSet<_>>();
    let mut asset_ids = target
        .assets
        .iter()
        .map(|value| value.id.as_str().to_string())
        .collect::<BTreeSet<_>>();
    let mut ring_ids = target
        .menus
        .iter()
        .flat_map(|menu| &menu.rings)
        .map(|value| value.id.as_str().to_string())
        .collect::<BTreeSet<_>>();
    let mut cell_ids = target
        .menus
        .iter()
        .flat_map(|menu| &menu.rings)
        .flat_map(|ring| &ring.cells)
        .map(|value| value.id.as_str().to_string())
        .collect::<BTreeSet<_>>();
    for menu in &document.menus {
        insert_remap(&mut remap.menus, menu.id.as_str(), &mut menu_ids);
    }
    for skin in &document.skins {
        insert_remap(&mut remap.skins, skin.id.as_str(), &mut skin_ids);
    }
    for asset in &document.assets {
        insert_remap(&mut remap.assets, asset.id.as_str(), &mut asset_ids);
    }
    for ring in document.menus.iter().flat_map(|menu| &menu.rings) {
        insert_remap(&mut remap.rings, ring.id.as_str(), &mut ring_ids);
    }
    for cell in document
        .menus
        .iter()
        .flat_map(|menu| &menu.rings)
        .flat_map(|ring| &ring.cells)
    {
        insert_remap(&mut remap.cells, cell.id.as_str(), &mut cell_ids);
    }
    let mut shortcut_ids = target
        .menus
        .iter()
        .flat_map(|menu| &menu.rings)
        .flat_map(|ring| &ring.cells)
        .flat_map(|cell| &cell.shortcuts)
        .map(|value| value.id.as_str().to_string())
        .collect();
    let mut hotstring_ids = target
        .menus
        .iter()
        .flat_map(|menu| &menu.rings)
        .flat_map(|ring| &ring.cells)
        .flat_map(|cell| &cell.hotstrings)
        .map(|value| value.id.as_str().to_string())
        .collect();
    for cell in document
        .menus
        .iter()
        .flat_map(|menu| &menu.rings)
        .flat_map(|ring| &ring.cells)
    {
        for value in &cell.shortcuts {
            insert_remap(&mut remap.shortcuts, value.id.as_str(), &mut shortcut_ids);
        }
        for value in &cell.hotstrings {
            insert_remap(&mut remap.hotstrings, value.id.as_str(), &mut hotstring_ids);
        }
    }
    let mut context_ids = target
        .context_rules
        .iter()
        .map(|value| value.id.as_str().to_string())
        .collect();
    for value in &document.context_rules {
        insert_remap(&mut remap.contexts, value.id.as_str(), &mut context_ids);
    }
    let mut trigger_ids = target
        .custom_triggers
        .iter()
        .map(|value| value.id.as_str().to_string())
        .collect();
    for value in &document.custom_triggers {
        insert_remap(&mut remap.triggers, value.id.as_str(), &mut trigger_ids);
    }

    document.default_menu_id = MenuId::new(mapped(&remap.menus, document.default_menu_id.as_str()));
    for menu in &mut document.menus {
        menu.id = MenuId::new(mapped(&remap.menus, menu.id.as_str()));
        menu.skin_id = SkinId::new(mapped(&remap.skins, menu.skin_id.as_str()));
        for ring in &mut menu.rings {
            ring.id = super::model::RingId::new(mapped(&remap.rings, ring.id.as_str()));
        }
        for cell in menu.rings.iter_mut().flat_map(|ring| &mut ring.cells) {
            cell.id = super::model::CellId::new(mapped(&remap.cells, cell.id.as_str()));
            if let CellContent::Submenu { menu_id } = &mut cell.content {
                *menu_id = MenuId::new(mapped(&remap.menus, menu_id.as_str()));
            }
            for shortcut in &mut cell.shortcuts {
                shortcut.id = ShortcutId::new(mapped(&remap.shortcuts, shortcut.id.as_str()));
            }
            for hotstring in &mut cell.hotstrings {
                hotstring.id = HotstringId::new(mapped(&remap.hotstrings, hotstring.id.as_str()));
            }
        }
    }
    for skin in &mut document.skins {
        skin.id = SkinId::new(mapped(&remap.skins, skin.id.as_str()));
    }
    for asset in &mut document.assets {
        asset.id = AssetId::new(mapped(&remap.assets, asset.id.as_str()));
    }
    for rule in &mut document.context_rules {
        rule.id = ContextRuleId::new(mapped(&remap.contexts, rule.id.as_str()));
        rule.menu_id = MenuId::new(mapped(&remap.menus, rule.menu_id.as_str()));
    }
    for trigger in &mut document.custom_triggers {
        trigger.id = TriggerId::new(mapped(&remap.triggers, trigger.id.as_str()));
        trigger.menu_id = MenuId::new(mapped(&remap.menus, trigger.menu_id.as_str()));
    }
    let mut value = serde_json::to_value(&*document)
        .map_err(|error| PackageError::Serialize(error.to_string()))?;
    rewrite_managed_ids(&mut value, &remap.assets);
    *document = serde_json::from_value(value)
        .map_err(|error| PackageError::Malformed(error.to_string()))?;
    validate(document).map_err(PackageError::Validation)?;
    Ok(remap)
}

fn insert_remap(
    map: &mut BTreeMap<String, String>,
    original: &str,
    occupied: &mut BTreeSet<String>,
) {
    let candidate = unique_id(original, occupied);
    occupied.insert(candidate.clone());
    map.insert(original.to_string(), candidate);
}

fn unique_id(original: &str, occupied: &BTreeSet<String>) -> String {
    if !occupied.contains(original) {
        return original.to_string();
    }
    (2..)
        .map(|suffix| format!("{original}-{suffix}"))
        .find(|value| !occupied.contains(value))
        .expect("numeric suffix remains available")
}

fn mapped(map: &BTreeMap<String, String>, original: &str) -> String {
    map.get(original)
        .cloned()
        .unwrap_or_else(|| original.to_string())
}

fn rewrite_managed_ids(value: &mut serde_json::Value, remap: &BTreeMap<String, String>) {
    match value {
        serde_json::Value::Object(object) => {
            if object.get("kind").and_then(|value| value.as_str()) == Some("managed") {
                if let Some(serde_json::Value::String(id)) = object.get_mut("asset_id") {
                    if let Some(mapped) = remap.get(id.as_str()) {
                        *id = mapped.clone();
                    }
                }
            }
            for value in object.values_mut() {
                rewrite_managed_ids(value, remap);
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                rewrite_managed_ids(value, remap);
            }
        }
        _ => {}
    }
}

fn is_reserved_dos_name(stem: &str) -> bool {
    matches!(stem, "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
}

fn find_eocd(bytes: &[u8]) -> Option<usize> {
    let start = bytes.len().saturating_sub(65_557);
    (start..bytes.len().saturating_sub(21))
        .rev()
        .find(|offset| {
            bytes
                .get(*offset..*offset + 4)
                .is_some_and(|value| value == b"\x50\x4b\x05\x06")
        })
}

fn slice(bytes: &[u8], start: usize, len: usize) -> Result<&[u8], PackageError> {
    start
        .checked_add(len)
        .and_then(|end| bytes.get(start..end))
        .ok_or_else(|| PackageError::Malformed("truncated ZIP".into()))
}
fn get_u16(bytes: &[u8], offset: usize) -> Result<u16, PackageError> {
    Ok(u16::from_le_bytes(
        slice(bytes, offset, 2)?.try_into().unwrap(),
    ))
}
fn get_u32(bytes: &[u8], offset: usize) -> Result<u32, PackageError> {
    Ok(u32::from_le_bytes(
        slice(bytes, offset, 4)?.try_into().unwrap(),
    ))
}
fn put_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in bytes {
        crc ^= *byte as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (0u32.wrapping_sub(crc & 1)));
        }
    }
    !crc
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;

    fn isolated_package_fixture() -> RadialDocument {
        let mut document = RadialDocument::starter();
        document.menus.truncate(1);
        document.menus[0].rings[0].cells.truncate(1);
        document.menus[0].rings[0].cells[0].content = CellContent::Spacer;
        document
    }

    fn append_manifested_file(plan: &mut ExportPlan, path: &str, bytes: Vec<u8>) {
        plan.manifest.files.push(PackageFileRecord {
            path: path.into(),
            sha256: sha256_hex(&bytes),
            byte_len: bytes.len() as u64,
        });
        plan.files.insert(path.into(), bytes);
        plan.files.insert(
            MANIFEST_FILE.into(),
            serde_json::to_vec_pretty(&plan.manifest).unwrap(),
        );
    }

    #[test]
    fn sha256_and_stored_zip_round_trip_are_canonical() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let plan = ExportPlan {
            manifest: PackageManifest {
                package_version: 1,
                document_path: DOCUMENT_FILE.into(),
                root_menu_ids: vec![],
                payload: PackagePayloadKind::MenuGraph,
                files: vec![],
                notices: vec![],
            },
            files: BTreeMap::from([
                ("manifest.json".into(), b"{}".to_vec()),
                ("radial.json".into(), b"[]".to_vec()),
            ]),
        };
        let first = encode_mlradial(&plan).unwrap();
        let second = encode_mlradial(&plan).unwrap();
        assert_eq!(first, second);
        assert_eq!(decode_mlradial(&first).unwrap(), plan.files);
    }

    #[test]
    fn hostile_paths_are_rejected() {
        for path in [
            "/abs",
            "//server/share",
            "C:/drive",
            "../up",
            "a/../b",
            "file:ads",
            "NUL.txt",
            "COM1",
            "trail. ",
            "a\\b",
        ] {
            assert!(validate_package_path(path).is_err(), "{path}");
        }
        assert!(validate_package_path("assets/abc.png").is_ok());
    }

    #[test]
    fn skin_bundle_round_trip_is_canonical_dependency_complete_and_collision_safe() {
        let mut source = isolated_package_fixture();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=")
            .unwrap();
        let digest = sha256_hex(&bytes);
        let asset_id = AssetId::new(format!("import-image-{digest}"));
        source.assets.push(AssetRecord {
            id: asset_id.clone(),
            kind: super::super::model::MediaKind::Image,
            relative_path: "skin-image.png".into(),
            content_sha256: digest,
            byte_len: bytes.len() as u64,
        });
        source.skins[0].style.values.images.center_image =
            super::super::model::Override::Value(MediaReference::Managed {
                asset_id: asset_id.clone(),
            });
        let plan = plan_skin_export(
            &source,
            &source.skins[0].id,
            &BTreeMap::from([(asset_id, bytes.clone())]),
            Vec::new(),
        )
        .unwrap();
        let first = encode_mlradial(&plan).unwrap();
        assert_eq!(first, encode_mlradial(&plan).unwrap());
        let imported = plan_skin_import(decode_mlradial(&first).unwrap(), &source).unwrap();
        assert_ne!(imported.skin.id, source.skins[0].id);
        assert!(imported.asset_records.is_empty());
        assert!(
            imported.assets.is_empty(),
            "content already owned by the target must not be retained in the import plan"
        );
        assert!(matches!(
            imported.skin.style.values.images.center_image,
            super::super::model::Override::Value(MediaReference::Managed { ref asset_id })
                if asset_id == &source.assets[0].id
        ));
        assert_eq!(
            imported.remap.assets.get(source.assets[0].id.as_str()),
            Some(&source.assets[0].id.as_str().to_owned())
        );

        let mut collision_source = isolated_package_fixture();
        collision_source.assets.push(AssetRecord {
            id: AssetId::new("skin-image"),
            kind: super::super::model::MediaKind::Image,
            relative_path: "skin-image.png".into(),
            content_sha256: sha256_hex(&bytes),
            byte_len: bytes.len() as u64,
        });
        collision_source.skins[0].style.values.images.center_image =
            super::super::model::Override::Value(MediaReference::Managed {
                asset_id: AssetId::new("skin-image"),
            });
        let collision_package = plan_skin_export(
            &collision_source,
            &collision_source.skins[0].id,
            &BTreeMap::from([(AssetId::new("skin-image"), bytes.clone())]),
            Vec::new(),
        )
        .unwrap();
        let mut mismatched_target = RadialDocument::starter();
        mismatched_target.assets.push(AssetRecord {
            id: AssetId::new("skin-image"),
            kind: super::super::model::MediaKind::Image,
            relative_path: "different.png".into(),
            content_sha256: "a".repeat(64),
            byte_len: 1,
        });
        let remapped = plan_skin_import(collision_package.files, &mismatched_target).unwrap();
        assert_eq!(remapped.asset_records.len(), 1);
        assert!(
            remapped.asset_records[0]
                .id
                .as_str()
                .starts_with("asset-image-")
        );
        assert!(!remapped.asset_records[0].id.as_str().starts_with("import-"));
        assert!(matches!(
            remapped.skin.style.values.images.center_image,
            super::super::model::Override::Value(MediaReference::Managed { ref asset_id })
                if asset_id == &remapped.asset_records[0].id
        ));

        let legacy = plan_export(
            &source,
            &[source.default_menu_id.clone()],
            &BTreeMap::from([(source.assets[0].id.clone(), bytes)]),
            Vec::new(),
        )
        .unwrap();
        let legacy_import = plan_import(legacy.files, &RadialDocument::starter());
        assert!(legacy_import.is_ok(), "{legacy_import:?}");
    }

    #[test]
    fn skin_bundle_rejects_nonportable_media_and_tampered_dependencies() {
        let mut source = isolated_package_fixture();
        source.skins[0].style.values.images.center_image =
            super::super::model::Override::Value(MediaReference::IconResource {
                path: "shell32.dll".into(),
                index: 4,
            });
        assert!(matches!(
            plan_skin_export(&source, &source.skins[0].id, &BTreeMap::new(), Vec::new()),
            Err(PackageError::NonPortableReference(_))
        ));
    }

    #[test]
    fn menu_and_skin_import_reject_valid_but_unreferenced_asset_entries() {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=")
            .unwrap();
        let extra_path = format!("assets/{}.png", sha256_hex(&bytes));
        let source = isolated_package_fixture();
        let mut menu = plan_export(
            &source,
            &[source.default_menu_id.clone()],
            &BTreeMap::new(),
            Vec::new(),
        )
        .unwrap();
        append_manifested_file(&mut menu, &extra_path, bytes.clone());
        assert!(matches!(
            plan_import(menu.files, &RadialDocument::starter()),
            Err(PackageError::Malformed(message))
                if message.contains("exactly match the declared dependency closure")
        ));

        let mut skin =
            plan_skin_export(&source, &source.skins[0].id, &BTreeMap::new(), Vec::new()).unwrap();
        append_manifested_file(&mut skin, &extra_path, bytes);
        assert!(matches!(
            plan_skin_import(skin.files, &RadialDocument::starter()),
            Err(PackageError::Malformed(message))
                if message.contains("exactly match the declared dependency closure")
        ));
    }

    #[test]
    fn zip_flags_encryption_compression_links_and_checksum_are_rejected() {
        let plan = ExportPlan {
            manifest: PackageManifest {
                package_version: 1,
                document_path: DOCUMENT_FILE.into(),
                root_menu_ids: vec![],
                payload: PackagePayloadKind::MenuGraph,
                files: vec![],
                notices: vec![],
            },
            files: BTreeMap::from([("manifest.json".into(), b"{}".to_vec())]),
        };
        let zip = encode_mlradial(&plan).unwrap();
        let mut encrypted = zip.clone();
        encrypted[6] |= 1;
        let central = encrypted
            .windows(4)
            .position(|v| v == [0x50, 0x4b, 0x01, 0x02])
            .unwrap();
        encrypted[central + 8] |= 1;
        assert!(matches!(
            decode_mlradial(&encrypted),
            Err(PackageError::EncryptedEntry(_))
        ));
        let mut compressed = zip.clone();
        compressed[8] = 8;
        compressed[central + 10] = 8;
        assert!(matches!(
            decode_mlradial(&compressed),
            Err(PackageError::UnsupportedCompression(8))
        ));
        let mut corrupt = zip;
        let data = 30 + "manifest.json".len();
        corrupt[data] ^= 1;
        assert!(matches!(
            decode_mlradial(&corrupt),
            Err(PackageError::ChecksumMismatch(_))
        ));

        let mut linked = encode_mlradial(&plan).unwrap();
        let central = linked
            .windows(4)
            .position(|value| value == [0x50, 0x4b, 0x01, 0x02])
            .unwrap();
        linked[central + 38..central + 42].copy_from_slice(&(0o120000u32 << 16).to_le_bytes());
        assert!(matches!(
            decode_mlradial(&linked),
            Err(PackageError::LinkEntry(_))
        ));

        let mut oversized = encode_mlradial(&plan).unwrap();
        let central = oversized
            .windows(4)
            .position(|value| value == [0x50, 0x4b, 0x01, 0x02])
            .unwrap();
        let claimed = (MAX_PACKAGE_ENTRY_BYTES as u32).saturating_add(1);
        oversized[central + 20..central + 24].copy_from_slice(&claimed.to_le_bytes());
        oversized[central + 24..central + 28].copy_from_slice(&claimed.to_le_bytes());
        assert!(matches!(
            decode_mlradial(&oversized),
            Err(PackageError::BudgetExceeded("expanded bytes"))
        ));
    }

    #[test]
    fn zip_local_headers_must_exactly_match_central_records_and_cover_payload_once() {
        let plan = ExportPlan {
            manifest: PackageManifest {
                package_version: 1,
                document_path: DOCUMENT_FILE.into(),
                root_menu_ids: vec![],
                payload: PackagePayloadKind::MenuGraph,
                files: vec![],
                notices: vec![],
            },
            files: BTreeMap::from([("manifest.json".into(), b"{}".to_vec())]),
        };
        let zip = encode_mlradial(&plan).unwrap();
        for offset in [14_usize, 18, 22] {
            let mut mismatched = zip.clone();
            mismatched[offset] ^= 1;
            assert!(matches!(
                decode_mlradial(&mismatched),
                Err(PackageError::Malformed(_))
            ));
        }
        let mut gap = zip.clone();
        let central = gap
            .windows(4)
            .position(|value| value == [0x50, 0x4b, 0x01, 0x02])
            .unwrap();
        gap.insert(central, 0);
        let eocd = gap
            .windows(4)
            .rposition(|value| value == [0x50, 0x4b, 0x05, 0x06])
            .unwrap();
        gap[eocd + 16..eocd + 20].copy_from_slice(&((central + 1) as u32).to_le_bytes());
        assert!(matches!(
            decode_mlradial(&gap),
            Err(PackageError::Malformed(_))
        ));
    }

    #[test]
    fn plain_folder_rejects_links_reparse_points_and_case_collisions() {
        for kind in [PackageEntryKind::Symlink, PackageEntryKind::ReparsePoint] {
            assert!(matches!(
                plain_folder_file_map(vec![PackageFile {
                    path: "radial.json".into(),
                    bytes: vec![],
                    kind,
                }]),
                Err(PackageError::LinkEntry(_))
            ));
        }
        assert!(matches!(
            plain_folder_file_map(vec![
                PackageFile {
                    path: "Asset.png".into(),
                    bytes: vec![],
                    kind: PackageEntryKind::File,
                },
                PackageFile {
                    path: "asset.PNG".into(),
                    bytes: vec![],
                    kind: PackageEntryKind::File,
                },
            ]),
            Err(PackageError::CaseCollision { .. })
        ));
    }

    #[test]
    fn import_remaps_every_colliding_stable_identity_and_rewrites_references() {
        let mut source = RadialDocument::starter();
        source.menus[0].rings[0].cells[0].content = super::super::model::CellContent::Action {
            binding: super::super::model::ActionBinding::Contextual {
                selector: super::super::model::TargetSelector::LastExternal,
                action_id: crate::universal_actions::ActionId::new("window.activate"),
            },
        };
        source.menus[0].rings[0].cells[0].after_action =
            super::super::model::AfterActionPolicy::CloseTree;
        source.menus[0].rings[0].cells[0]
            .shortcuts
            .push(super::super::model::ItemShortcut {
                id: ShortcutId::new("input-shortcut"),
                chord: "Ctrl+Alt+8".into(),
                gesture: super::super::model::ClickGesture::Primary,
                scope: super::super::model::TriggerScope::MenuLocal,
            });
        source.menus[0].rings[0].cells[0]
            .hotstrings
            .push(super::super::model::ItemHotstring {
                id: HotstringId::new("input-hotstring"),
                text: "::radial".into(),
                gesture: super::super::model::ClickGesture::Primary,
                case_sensitive: false,
                scope: super::super::model::TriggerScope::MenuLocal,
            });
        source.context_rules.push(super::super::model::ContextRule {
            id: ContextRuleId::new("context"),
            enabled: true,
            priority: 1,
            process_name: Some("explorer.exe".into()),
            window_title_contains: None,
            monitor_id: None,
            menu_id: source.default_menu_id.clone(),
        });
        source
            .custom_triggers
            .push(super::super::model::TriggerDefinition {
                id: TriggerId::new("trigger"),
                chord: "Ctrl+Alt+9".into(),
                menu_id: source.default_menu_id.clone(),
                scope: super::super::model::TriggerScope::Global,
            });
        let export = plan_export(
            &source,
            &[source.default_menu_id.clone()],
            &BTreeMap::new(),
            vec![],
        )
        .unwrap();
        let imported = plan_import(export.files, &source).unwrap();
        assert_ne!(imported.document.default_menu_id, source.default_menu_id);
        assert!(
            imported
                .document
                .menus
                .iter()
                .all(|menu| !source.menus.iter().any(|old| old.id == menu.id))
        );
        assert!(
            imported
                .remap
                .rings
                .values()
                .any(|value| value.ends_with("-2"))
        );
        assert!(
            imported
                .remap
                .cells
                .values()
                .any(|value| value.ends_with("-2"))
        );
        assert_eq!(
            imported.remap.shortcuts["input-shortcut"],
            "input-shortcut-2"
        );
        assert_eq!(
            imported.remap.hotstrings["input-hotstring"],
            "input-hotstring-2"
        );
        assert_eq!(imported.remap.contexts["context"], "context-2");
        assert_eq!(imported.remap.triggers["trigger"], "trigger-2");
        validate(&imported.document).unwrap();
    }

    #[test]
    fn canonical_export_import_export_and_full_submenu_dependency_closure() {
        let mut source = isolated_package_fixture();
        let mut child = source.menus[0].clone();
        child.id = MenuId::new("child");
        child.name = "Child".into();
        child.rings[0].id = super::super::model::RingId::new("child-ring");
        for (index, cell) in child.rings[0].cells.iter_mut().enumerate() {
            cell.id = super::super::model::CellId::new(format!("child-cell-{index}"));
        }
        source.menus[0].rings[0].cells[0].content = CellContent::Submenu {
            menu_id: child.id.clone(),
        };
        source.menus.push(child);
        let first = plan_export(
            &source,
            &[source.default_menu_id.clone()],
            &BTreeMap::new(),
            vec![PackageNotice {
                code: "license".into(),
                message: "user-owned media only".into(),
            }],
        )
        .unwrap();
        let packed = encode_mlradial(&first).unwrap();
        let files = decode_mlradial(&packed).unwrap();

        let mut target = isolated_package_fixture();
        target.default_menu_id = MenuId::new("target");
        target.menus[0].id = target.default_menu_id.clone();
        target.menus[0].skin_id = SkinId::new("target-skin");
        target.skins[0].id = SkinId::new("target-skin");
        target.menus[0].rings[0].id = super::super::model::RingId::new("target-ring");
        for (index, cell) in target.menus[0].rings[0].cells.iter_mut().enumerate() {
            cell.id = super::super::model::CellId::new(format!("target-cell-{index}"));
        }
        let imported = plan_import(files, &target).unwrap();
        assert_eq!(imported.document.menus.len(), 2);
        let second = plan_export(
            &imported.document,
            &[imported.document.default_menu_id.clone()],
            &BTreeMap::new(),
            first.manifest.notices.clone(),
        )
        .unwrap();
        assert_eq!(first.files, second.files);
    }

    #[test]
    fn submenu_closure_and_broken_roots_are_explicit() {
        let mut document = isolated_package_fixture();
        let child = document.menus[0].clone();
        let child_id = MenuId::new("child");
        let mut child = child;
        child.id = child_id.clone();
        child.name = "Child".into();
        document.menus[0].rings[0].cells[0].content = CellContent::Submenu {
            menu_id: child_id.clone(),
        };
        document.menus.push(child);
        let closure = submenu_closure(&document, &[document.default_menu_id.clone()]).unwrap();
        assert_eq!(
            closure,
            BTreeSet::from([
                "child".to_string(),
                document.default_menu_id.as_str().to_string()
            ])
        );
        assert!(matches!(
            submenu_closure(&document, &[MenuId::new("missing")]),
            Err(PackageError::MissingRoot(_))
        ));
    }

    #[test]
    fn malformed_references_nonportable_media_and_manifest_checksum_fail_closed() {
        let mut broken = RadialDocument::starter();
        broken.menus[0].rings[0].cells[0].content = CellContent::Submenu {
            menu_id: MenuId::new("missing"),
        };
        assert!(matches!(
            plan_export(
                &broken,
                &[broken.default_menu_id.clone()],
                &BTreeMap::new(),
                vec![]
            ),
            Err(PackageError::Validation(_))
        ));

        let mut external = RadialDocument::starter();
        external.skins[0].style.values.images.center_image =
            super::super::model::Override::Value(MediaReference::ExternalFile {
                path: "C:/secret.png".into(),
            });
        assert!(matches!(
            plan_export(
                &external,
                &[external.default_menu_id.clone()],
                &BTreeMap::new(),
                vec![]
            ),
            Err(PackageError::NonPortableReference(_))
        ));

        let source = RadialDocument::starter();
        let mut export = plan_export(
            &source,
            &[source.default_menu_id.clone()],
            &BTreeMap::new(),
            vec![],
        )
        .unwrap();
        export.files.get_mut(DOCUMENT_FILE).unwrap().push(b' ');
        assert!(matches!(
            plan_import(export.files, &source),
            Err(PackageError::ChecksumMismatch(_))
        ));
    }
}
