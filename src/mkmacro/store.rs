use super::{model::*, validation::*};
use crate::common::{
    atomic_file::save_atomic,
    json_watch::{JsonWatcher, watch_json},
};
use anyhow::{Context, Result};
use image::{DynamicImage, ImageDecoder, ImageFormat, RgbaImage, codecs::png::PngDecoder};
use std::io::Cursor;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, RwLock},
};
pub const MKMACROS_FILE: &str = "mkmacros.json";
pub const ASSET_DIRECTORY: &str = "mkmacro_assets";
pub(crate) fn managed_image_path(asset_root: &Path, image: &MkImageRef) -> Result<PathBuf> {
    if asset_root.file_name() != Some(std::ffi::OsStr::new(ASSET_DIRECTORY))
        || !image.is_valid_filename()
    {
        anyhow::bail!("invalid managed asset root or image reference")
    }
    Ok(asset_root.join(image.filename()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageImportChoice {
    ReplaceExisting,
    SaveAs(MkImageRef),
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageImportResult {
    Imported(MkImageRef),
    Collision { image: MkImageRef },
    Cancelled,
}
#[derive(Debug)]
pub enum LoadDisposition {
    Missing,
    Empty,
    Loaded,
    NeedsUserRecovery { error: String },
}
struct Inner {
    path: PathBuf,
    /// Orders the entire disk-read/write and publication transaction. In particular,
    /// a watcher may not publish bytes it read before a completed local save.
    transaction: Mutex<()>,
    /// Serializes image writes with publication of the corresponding PNG.
    asset_authoring: Mutex<()>,
    snapshot: RwLock<Arc<MkMacroDocument>>,
    diagnostics: RwLock<Arc<[MkDiagnostic]>>,
    last_external_error: RwLock<Option<String>>,
}
pub struct MkMacroStore {
    inner: Arc<Inner>,
    _watcher: Option<JsonWatcher>,
}
impl MkMacroStore {
    pub fn asset_root(&self) -> PathBuf {
        self.inner
            .path
            .parent()
            .unwrap_or(Path::new("."))
            .join(ASSET_DIRECTORY)
    }
    /// Enumerates direct regular PNG files in the shared canonical root in
    /// deterministic filename order. Symlinks and nested files are ignored.
    pub fn image_refs(&self) -> Result<Vec<MkImageRef>> {
        let directory = self.asset_root();
        let mut refs = Vec::new();
        match fs::read_dir(&directory) {
            Ok(entries) => {
                for entry in entries {
                    let entry = entry?;
                    let path = entry.path();
                    if !entry.file_type()?.is_file()
                        || path
                            .extension()
                            .and_then(|x| x.to_str())
                            .is_none_or(|x| !x.eq_ignore_ascii_case("png"))
                    {
                        continue;
                    }
                    let Some(name) = path.file_name().and_then(|x| x.to_str()) else {
                        continue;
                    };
                    if let Ok(image) = MkImageRef::new(name.to_owned()) {
                        refs.push(image);
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e).context("enumerate mkmacro_assets"),
        }
        refs.sort_by(|a, b| a.filename().cmp(b.filename()));
        Ok(refs)
    }
    pub fn open(directory: impl AsRef<Path>) -> Result<(Self, LoadDisposition)> {
        let path = directory.as_ref().join(MKMACROS_FILE);
        let inner = Arc::new(Inner {
            path: path.clone(),
            transaction: Mutex::new(()),
            asset_authoring: Mutex::new(()),
            snapshot: RwLock::new(Arc::new(MkMacroDocument::default())),
            diagnostics: RwLock::new(Arc::from([])),
            last_external_error: RwLock::new(None),
        });
        let disposition = match read_document(&path) {
            Ok(None) => {
                if path.exists() {
                    LoadDisposition::Empty
                } else {
                    LoadDisposition::Missing
                }
            }
            Ok(Some((doc, changed))) => {
                if changed {
                    persist(&path, &doc)?
                }
                publish(&inner, doc);
                LoadDisposition::Loaded
            }
            Err(e) => LoadDisposition::NeedsUserRecovery {
                error: e.to_string(),
            },
        };
        let weak = Arc::downgrade(&inner);
        let watcher = watch_json(&path, move || {
            if let Some(i) = weak.upgrade() {
                reload_from_disk(&i);
            }
        })
        .ok();
        Ok((
            Self {
                inner,
                _watcher: watcher,
            },
            disposition,
        ))
    }
    pub fn snapshot(&self) -> Arc<MkMacroDocument> {
        self.inner.snapshot.read().unwrap().clone()
    }
    pub fn diagnostics(&self) -> Arc<[MkDiagnostic]> {
        self.inner.diagnostics.read().unwrap().clone()
    }
    pub fn can_run(&self) -> bool {
        can_run(&self.diagnostics())
    }
    pub fn last_external_error(&self) -> Option<String> {
        self.inner.last_external_error.read().unwrap().clone()
    }
    pub fn save(&self, mut doc: MkMacroDocument) -> Result<Arc<MkMacroDocument>> {
        self.save_transaction(&mut doc, || {})
    }
    fn save_transaction(
        &self,
        doc: &mut MkMacroDocument,
        before_publication: impl FnOnce(),
    ) -> Result<Arc<MkMacroDocument>> {
        let _transaction = self.inner.transaction.lock().unwrap();
        let before = self.snapshot().macros.len();
        doc.schema_version = SCHEMA_VERSION;
        repair_ids(doc);
        persist(&self.inner.path, doc)?;
        before_publication();
        publish(&self.inner, doc.clone());
        let saved = self.snapshot();
        tracing::info!(
            path = %resolved_path(&self.inner.path).display(),
            macro_count = saved.macros.len(),
            snapshot_macro_count_before = before,
            snapshot_macro_count_after = saved.macros.len(),
            "mkmacro save"
        );
        Ok(saved)
    }
    pub fn image_path(&self, image: &MkImageRef) -> Result<PathBuf> {
        let path = managed_image_path(&self.asset_root(), image)?;
        ensure_safe_direct_child(&self.asset_root(), &path)?;
        Ok(path)
    }

    pub fn validate_image_ref(&self, image: &MkImageRef) -> Result<RgbaImage> {
        let path = self.image_path(image)?;
        let bytes = fs::read(&path).with_context(|| format!("read image {}", image.filename()))?;
        decode_png(&bytes).with_context(|| format!("decode image {}", image.filename()))
    }

    pub fn import_png(&self, source: &Path) -> Result<ImageImportResult> {
        let bytes = fs::read(source).with_context(|| format!("read image {}", source.display()))?;
        let _ = decode_png(&bytes)
            .with_context(|| format!("{} is not a valid PNG image", source.display()))?;
        if let Some(image) = direct_root_reference(&self.asset_root(), source)? {
            return Ok(ImageImportResult::Imported(image));
        }
        let name = source
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| anyhow::anyhow!("source image has no usable filename"))?;
        let image = MkImageRef::new(name.to_owned()).map_err(anyhow::Error::msg)?;
        // A first import is create-only: using SaveAs here makes an existing
        // destination report a collision without permitting an overwrite.
        self.write_image_bytes(&bytes, image.clone(), ImageImportChoice::SaveAs(image))
    }

    pub fn import_png_with_choice(
        &self,
        source: &Path,
        choice: ImageImportChoice,
    ) -> Result<ImageImportResult> {
        let bytes = fs::read(source).with_context(|| format!("read image {}", source.display()))?;
        let _ = decode_png(&bytes)
            .with_context(|| format!("{} is not a valid PNG image", source.display()))?;
        if let Some(image) = direct_root_reference(&self.asset_root(), source)? {
            return Ok(ImageImportResult::Imported(image));
        }
        let default_name = source
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| anyhow::anyhow!("source image has no usable filename"))?;
        let requested = match &choice {
            ImageImportChoice::SaveAs(image) => image.clone(),
            _ => MkImageRef::new(default_name.to_owned()).map_err(anyhow::Error::msg)?,
        };
        self.write_image_bytes(&bytes, requested, choice)
    }

    pub fn write_captured_png(
        &self,
        image: &RgbaImage,
        requested: MkImageRef,
        choice: ImageImportChoice,
    ) -> Result<ImageImportResult> {
        validate_capture(image)?;
        let mut encoded = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(image.clone()).write_to(&mut encoded, ImageFormat::Png)?;
        self.write_image_bytes(encoded.get_ref(), requested, choice)
    }

    fn write_image_bytes(
        &self,
        bytes: &[u8],
        requested: MkImageRef,
        choice: ImageImportChoice,
    ) -> Result<ImageImportResult> {
        let _authoring = self.inner.asset_authoring.lock().unwrap();
        if matches!(choice, ImageImportChoice::Cancel) {
            return Ok(ImageImportResult::Cancelled);
        }
        let requested = match &choice {
            ImageImportChoice::SaveAs(image) => image.clone(),
            _ => requested,
        };
        let destination = self.image_path(&requested)?;
        if destination.exists() {
            match choice {
                ImageImportChoice::ReplaceExisting => {}
                ImageImportChoice::SaveAs(_) | ImageImportChoice::Cancel => {
                    return Ok(ImageImportResult::Collision { image: requested });
                }
            }
        }
        fs::create_dir_all(self.asset_root())?;
        save_atomic(&destination, bytes)
            .with_context(|| format!("write image {}", requested.filename()))?;
        Ok(ImageImportResult::Imported(requested))
    }
}

fn validate_capture(image: &RgbaImage) -> Result<()> {
    if image.width() == 0 || image.height() == 0 {
        anyhow::bail!("reference image is empty")
    }
    Ok(())
}

fn decode_png(bytes: &[u8]) -> Result<RgbaImage> {
    let decoder = PngDecoder::new(Cursor::new(bytes))?;
    let (width, height) = decoder.dimensions();
    super::asset_authoring::validate_image_dimensions(width, height)?;
    Ok(DynamicImage::from_decoder(decoder)?.to_rgba8())
}

fn ensure_safe_direct_child(root: &Path, path: &Path) -> Result<()> {
    let root_canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    if path.parent() != Some(root) {
        anyhow::bail!("image reference must be a direct child of mkmacro_assets")
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            anyhow::bail!("image reference must not follow a symlink")
        }
        Ok(_) => {
            let canonical = path.canonicalize()?;
            if canonical.parent() != Some(root_canonical.as_path()) {
                anyhow::bail!("image reference escapes mkmacro_assets")
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    Ok(())
}

fn direct_root_reference(root: &Path, source: &Path) -> Result<Option<MkImageRef>> {
    let source_metadata = match fs::symlink_metadata(source) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if source_metadata.file_type().is_symlink() {
        anyhow::bail!("image source must not be a symlink")
    }
    if !source_metadata.file_type().is_file() {
        return Ok(None);
    }
    let source_canonical = source.canonicalize()?;
    let root_canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    if source_canonical.parent() != Some(root_canonical.as_path()) {
        return Ok(None);
    }
    let Some(name) = source_canonical.file_name().and_then(|n| n.to_str()) else {
        return Ok(None);
    };
    let Ok(image) = MkImageRef::new(name.to_owned()) else {
        return Ok(None);
    };
    let candidate = root.join(image.filename());
    ensure_safe_direct_child(root, &candidate)?;
    Ok(Some(image))
}
fn reload_from_disk(i: &Inner) {
    let _transaction = i.transaction.lock().unwrap();
    match read_document(&i.path) {
        Ok(Some((d, changed))) => {
            if !changed || persist(&i.path, &d).is_ok() {
                publish(i, d);
                *i.last_external_error.write().unwrap() = None
            }
        }
        Ok(None) => {}
        Err(e) => *i.last_external_error.write().unwrap() = Some(e.to_string()),
    }
}

fn resolved_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    })
}
fn publish(i: &Inner, d: MkMacroDocument) {
    let ds = validate_document(
        &d,
        i.path.parent().map(|p| p.join(ASSET_DIRECTORY)).as_deref(),
    );
    *i.diagnostics.write().unwrap() = ds.into();
    *i.snapshot.write().unwrap() = Arc::new(d)
}
fn persist(path: &Path, d: &MkMacroDocument) -> Result<()> {
    save_atomic(path, &serde_json::to_vec_pretty(d)?)
        .with_context(|| format!("save {}", path.display()))
}
fn read_document(path: &Path) -> Result<Option<(MkMacroDocument, bool)>> {
    let text = match fs::read_to_string(path) {
        Ok(x) => x,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    if text.trim().is_empty() {
        return Ok(None);
    };
    let mut value: serde_json::Value = serde_json::from_str(&text)
        .context("mkmacros.json is malformed; keep it for recovery or correct the JSON")?;
    let input_version = value
        .get("schema_version")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    if input_version > SCHEMA_VERSION {
        anyhow::bail!(
            "mkmacros.json schema version {input_version} is newer than supported version {SCHEMA_VERSION}; update Multi Launcher before opening it"
        )
    };
    if input_version == 1 {
        migrate_v1_to_v2(&mut value)?;
    }
    if input_version <= 2 {
        migrate_v2_to_v3(&mut value)?;
    }
    if input_version <= 3 {
        migrate_v3_to_v4(&mut value);
    }
    if input_version <= 4 {
        migrate_v4_to_v5(&mut value)?;
    }
    if input_version <= 7 {
        migrate_v7_to_v8(&mut value)?;
    }
    if value.get("schema_version").and_then(|v| v.as_u64()) == Some(8) {
        migrate_v8_to_v9(&mut value)?;
    }
    if value.get("schema_version").and_then(|v| v.as_u64()) == Some(9) {
        migrate_v9_to_v10(&mut value)?;
    }
    if value.get("schema_version").and_then(|v| v.as_u64()) == Some(10) {
        migrate_v10_to_v11(&mut value, path)?;
    }
    let mut doc: MkMacroDocument =
        serde_json::from_value(value).context("mkmacros.json does not match the macro schema")?;
    let mut changed = input_version != SCHEMA_VERSION;
    doc.schema_version = SCHEMA_VERSION;
    changed |= repair_ids(&mut doc);
    Ok(Some((doc, changed)))
}

/// Filesystem-aware schema-10 migration. The JSON value is rewritten only after
/// every required legacy source has been decoded and copied successfully. Flat
/// files created by this pass are tracked so a failed transaction can remove
/// only files this invocation created; nested legacy files are never touched.
fn migrate_v10_to_v11(value: &mut serde_json::Value, document_path: &Path) -> Result<()> {
    let root = document_path
        .parent()
        .unwrap_or(Path::new("."))
        .join(ASSET_DIRECTORY);
    let macros = value
        .get_mut("macros")
        .and_then(|v| v.as_array_mut())
        .ok_or_else(|| anyhow::anyhow!("schema 10 macro document must contain a macros array"))?;

    let mut planned: HashMap<String, Vec<u8>> = HashMap::new();
    match fs::read_dir(&root) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry?;
                if !entry.file_type()?.is_file() {
                    continue;
                }
                let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                    continue;
                };
                if name.to_ascii_lowercase().ends_with(".png") {
                    planned.insert(name, fs::read(entry.path())?);
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error)
                .with_context(|| format!("enumerate legacy asset root {}", root.display()));
        }
    }
    let mut created = Vec::<PathBuf>::new();
    let mut source_destinations: HashMap<PathBuf, MkImageRef> = HashMap::new();
    let result = (|| -> Result<()> {
        for macro_value in macros.iter_mut() {
            let (macro_id, metadata) = {
                let macro_object = macro_value
                    .as_object()
                    .ok_or_else(|| anyhow::anyhow!("schema 10 macro must be an object"))?;
                let macro_id = macro_object.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
                let mut metadata = HashMap::<u64, (String, PathBuf)>::new();
                if let Some(entries) = macro_object.get("image_assets").and_then(|v| v.as_array()) {
                    for entry in entries {
                        let Some(object) = entry.as_object() else {
                            continue;
                        };
                        let Some(id) = object.get("id").and_then(|v| v.as_u64()) else {
                            continue;
                        };
                        let name = object
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_owned();
                        metadata.insert(
                            id,
                            (
                                name,
                                root.join(macro_id.to_string()).join(format!("{id}.png")),
                            ),
                        );
                    }
                }
                (macro_id, metadata)
            };
            let mut ids = Vec::new();
            collect_legacy_asset_ids(macro_value, &mut ids);
            ids.extend(metadata.keys().copied());
            ids.sort_unstable();
            ids.dedup();
            let mut mapping = HashMap::<u64, MkImageRef>::new();
            for id in ids {
                let (friendly, source) = metadata.get(&id).cloned().unwrap_or_else(|| {
                    (
                        format!("image_{id}"),
                        root.join(macro_id.to_string()).join(format!("{id}.png")),
                    )
                });
                let source_exists = match fs::symlink_metadata(&source) {
                    Ok(metadata) => {
                        if metadata.file_type().is_symlink() {
                            anyhow::bail!(
                                "legacy image source must not be a symlink: {}",
                                source.display()
                            );
                        }
                        if !metadata.is_file() {
                            anyhow::bail!(
                                "legacy image source is not a regular file: {}",
                                source.display()
                            );
                        }
                        true
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
                    Err(error) => {
                        return Err(error).with_context(|| {
                            format!("inspect legacy image source {}", source.display())
                        });
                    }
                };
                if !source_exists {
                    mapping.insert(id, missing_image_ref(macro_id, id, &mut planned)?);
                    continue;
                }
                let bytes = fs::read(&source)
                    .with_context(|| format!("read legacy image source {}", source.display()))?;
                decode_png(&bytes)
                    .with_context(|| format!("decode legacy image source {}", source.display()))?;
                if let Some(existing) = source_destinations.get(&source) {
                    mapping.insert(id, existing.clone());
                    continue;
                }
                let candidate = sanitize_legacy_filename(&friendly, id);
                let destination = choose_migration_destination(&candidate, &bytes, &planned)?;
                let image = MkImageRef::new(destination.clone()).map_err(anyhow::Error::msg)?;
                if !planned.contains_key(&destination) {
                    fs::create_dir_all(&root)?;
                    let path = root.join(&destination);
                    save_atomic(&path, &bytes)
                        .with_context(|| format!("copy legacy image {}", source.display()))?;
                    created.push(path);
                    planned.insert(destination, bytes);
                }
                source_destinations.insert(source, image.clone());
                mapping.insert(id, image);
            }
            rewrite_legacy_asset_ids(macro_value, &mapping)?;
            macro_value.as_object_mut().unwrap().remove("image_assets");
        }
        Ok(())
    })();
    if let Err(error) = result {
        for path in created {
            let _ = fs::remove_file(path);
        }
        return Err(error);
    }
    value["schema_version"] = serde_json::json!(11);
    Ok(())
}

fn collect_legacy_asset_ids(value: &serde_json::Value, ids: &mut Vec<u64>) {
    match value {
        serde_json::Value::Object(object) => {
            if let Some(id) = object.get("asset_id").and_then(|v| v.as_u64()) {
                ids.push(id);
            }
            for child in object.values() {
                collect_legacy_asset_ids(child, ids);
            }
        }
        serde_json::Value::Array(values) => {
            for child in values {
                collect_legacy_asset_ids(child, ids);
            }
        }
        _ => {}
    }
}

fn rewrite_legacy_asset_ids(
    value: &mut serde_json::Value,
    mapping: &HashMap<u64, MkImageRef>,
) -> Result<()> {
    match value {
        serde_json::Value::Object(object) => {
            if let Some(asset_id) = object.remove("asset_id") {
                let image = match asset_id {
                    serde_json::Value::Null => serde_json::Value::Null,
                    serde_json::Value::Number(number) => {
                        let id = number.as_u64().ok_or_else(|| {
                            anyhow::anyhow!("legacy asset_id must be an unsigned integer or null")
                        })?;
                        serde_json::to_value(mapping.get(&id).ok_or_else(|| {
                            anyhow::anyhow!("legacy asset ID {id} was not collected")
                        })?)?
                    }
                    _ => anyhow::bail!("legacy asset_id must be an unsigned integer or null"),
                };
                object.insert("image".into(), image);
            }
            for child in object.values_mut() {
                rewrite_legacy_asset_ids(child, mapping)?;
            }
        }
        serde_json::Value::Array(values) => {
            for child in values {
                rewrite_legacy_asset_ids(child, mapping)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn sanitize_legacy_filename(name: &str, id: u64) -> String {
    let mut base = name.trim().to_owned();
    if base.is_empty() {
        base = format!("image_{id}");
    }
    base = base
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    base = base.trim_end_matches(['.', ' ']).to_owned();
    if base.is_empty() {
        base = format!("image_{id}");
    }
    let stem = if base.to_ascii_lowercase().ends_with(".png") {
        &base[..base.len() - 4]
    } else {
        &base
    };
    let mut stem = stem.to_owned();
    let reserved = stem.split('.').next().unwrap_or(&stem).to_ascii_uppercase();
    if matches!(
        reserved.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    ) {
        stem.insert(0, '_');
    }
    format!("{stem}.png")
}

fn choose_migration_destination(
    candidate: &str,
    bytes: &[u8],
    planned: &HashMap<String, Vec<u8>>,
) -> Result<String> {
    if let Some((existing_name, existing)) = planned
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case(candidate))
        .min_by(|(left, _), (right, _)| left.cmp(right))
    {
        if existing == bytes {
            return Ok(existing_name.clone());
        }
    } else {
        return Ok(candidate.to_owned());
    }
    let stem = candidate.strip_suffix(".png").unwrap_or(candidate);
    for suffix in 2.. {
        let candidate = format!("{stem}_{suffix}.png");
        match planned
            .iter()
            .filter(|(name, _)| name.eq_ignore_ascii_case(&candidate))
            .min_by(|(left, _), (right, _)| left.cmp(right))
        {
            None => return Ok(candidate),
            Some((existing_name, existing)) if existing == bytes => {
                return Ok(existing_name.clone());
            }
            Some(_) => {}
        }
    }
    unreachable!()
}

fn missing_image_ref(
    macro_id: u64,
    asset_id: u64,
    planned: &mut HashMap<String, Vec<u8>>,
) -> Result<MkImageRef> {
    let base = format!("missing_image_{macro_id}_{asset_id}");
    let mut candidate = format!("{base}.png");
    let mut suffix = 2;
    while planned
        .keys()
        .any(|name| name.eq_ignore_ascii_case(&candidate))
    {
        candidate = format!("{base}_{suffix}.png");
        suffix += 1;
    }
    Ok(MkImageRef::new(candidate).map_err(anyhow::Error::msg)?)
}
/// Converts schema-7 resolved Launcher actions into the temporary compatibility
/// state used by schema 8. No Serde aliases accept the old shape outside here.
fn migrate_v7_to_v8(value: &mut serde_json::Value) -> Result<()> {
    if let Some(macros) = value.get_mut("macros").and_then(|v| v.as_array_mut()) {
        for (macro_index, mac) in macros.iter_mut().enumerate() {
            let Some(steps) = mac.get_mut("steps").and_then(|v| v.as_array_mut()) else {
                continue;
            };
            for (step_index, step) in steps.iter_mut().enumerate() {
                let Some(action) = step.get_mut("action").and_then(|v| v.as_object_mut()) else {
                    continue;
                };
                if action.get("type").and_then(|v| v.as_str()) != Some("launcher_command") {
                    continue;
                }
                let data = action
                    .get_mut("data")
                    .and_then(|v| v.as_object_mut())
                    .ok_or_else(|| anyhow::anyhow!("macro {macro_index} step {step_index}: legacy launcher_command data must be an object"))?;
                let command = data
                    .remove("command")
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .ok_or_else(|| anyhow::anyhow!("macro {macro_index} step {step_index}: legacy launcher_command command must be a string"))?;
                let args = match data.remove("args") {
                    None | Some(serde_json::Value::Null) => None,
                    Some(serde_json::Value::String(args)) => Some(args),
                    Some(_) => anyhow::bail!(
                        "macro {macro_index} step {step_index}: legacy launcher_command args must be a string or null"
                    ),
                };
                let (query, preserve) = classify_legacy_launcher_command(&command, args.as_deref());
                let mut replacement = serde_json::Map::new();
                replacement.insert("query".into(), serde_json::Value::String(query));
                if preserve {
                    replacement.insert(
                        "legacy_resolved_action".into(),
                        serde_json::to_value(crate::actions::Action {
                            label: command.clone(),
                            desc: String::new(),
                            action: command,
                            args,
                        })?,
                    );
                }
                *data = replacement;
            }
        }
    }
    value["schema_version"] = serde_json::json!(8);
    Ok(())
}

/// Converts schema-8 delay objects and adds the schema-9 document/macro fields.
fn migrate_v8_to_v9(value: &mut serde_json::Value) -> Result<()> {
    let object = value
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("schema 8 macro document must be an object"))?;
    object
        .entry("folders")
        .or_insert_with(|| serde_json::json!([]));

    let Some(macros) = object.get_mut("macros").and_then(|v| v.as_array_mut()) else {
        object.insert("schema_version".into(), serde_json::json!(9));
        return Ok(());
    };
    for (macro_index, mac) in macros.iter_mut().enumerate() {
        let macro_object = mac.as_object_mut().ok_or_else(|| {
            anyhow::anyhow!("macro {macro_index}: legacy macro must be an object")
        })?;
        macro_object
            .entry("folder_id")
            .or_insert(serde_json::Value::Null);
        macro_object
            .entry("hotkey_scope")
            .or_insert_with(|| serde_json::json!({"type": "any_window"}));

        let Some(steps) = macro_object.get_mut("steps").and_then(|v| v.as_array_mut()) else {
            continue;
        };
        for (step_index, step) in steps.iter_mut().enumerate() {
            let Some(action) = step.get_mut("action").and_then(|v| v.as_object_mut()) else {
                continue;
            };
            if action.get("type").and_then(|v| v.as_str()) != Some("delay") {
                continue;
            }
            let data = action
                .get("data")
                .and_then(|v| v.as_object())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "macro {macro_index} step {step_index}: legacy delay data must be an object"
                    )
                })?;
            let milliseconds = data
                .get("milliseconds")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| anyhow::anyhow!(
                    "macro {macro_index} step {step_index}: legacy delay milliseconds must be an unsigned integer"
                ))?;
            action.insert(
                "data".into(),
                serde_json::json!({
                    "mode": "fixed",
                    "fixed_ms": milliseconds,
                    "minimum_ms": 0,
                    "maximum_ms": milliseconds,
                }),
            );
        }
    }
    object.insert("schema_version".into(), serde_json::json!(9));
    Ok(())
}

/// Adds persisted debugging breakpoint state to every schema-9 macro step.
fn migrate_v9_to_v10(value: &mut serde_json::Value) -> Result<()> {
    let object = value
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("schema 9 macro document must be an object"))?;

    let Some(macros) = object.get_mut("macros").and_then(|v| v.as_array_mut()) else {
        object.insert("schema_version".into(), serde_json::json!(10));
        return Ok(());
    };
    for (macro_index, mac) in macros.iter_mut().enumerate() {
        let macro_object = mac.as_object_mut().ok_or_else(|| {
            anyhow::anyhow!("macro {macro_index}: schema 9 macro must be an object")
        })?;
        let Some(steps) = macro_object.get_mut("steps").and_then(|v| v.as_array_mut()) else {
            continue;
        };
        for (step_index, step) in steps.iter_mut().enumerate() {
            let step_object = step.as_object_mut().ok_or_else(|| {
                anyhow::anyhow!(
                    "macro {macro_index} step {step_index}: schema 9 macro step must be an object"
                )
            })?;
            step_object
                .entry("breakpoint")
                .or_insert(serde_json::Value::Bool(false));
        }
    }
    object.insert("schema_version".into(), serde_json::json!(10));
    Ok(())
}
/// Joins legacy free-form fields without trimming meaningful text from either field.
fn join_legacy_text(command: &str, args: Option<&str>) -> String {
    match args.filter(|args| !args.trim().is_empty()) {
        Some(args) => format!("{} {}", command.trim_end(), args.trim_start()),
        None => command.to_owned(),
    }
}

pub(crate) fn query_action_text(query: &str, args: Option<&str>) -> String {
    // This is the only argument contract implemented by Launcher `query:` actions.
    // Arbitrary action args are deliberately not guessed into the query.
    let query_arg = args
        .and_then(|args| serde_json::from_str::<serde_json::Value>(args).ok())
        .and_then(|value| value.get("query")?.as_str().map(str::to_owned));
    join_legacy_text(query, query_arg.as_deref())
}

fn is_absolute_legacy_target(command: &str) -> bool {
    command.starts_with('/')
        || command.starts_with("\\\\")
        || command.as_bytes().get(1) == Some(&b':')
            && command
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphabetic)
}

/// Namespaces dispatched by Launcher rather than searched as user text. Keeping this
/// list explicit prevents an ordinary query such as `meeting: tomorrow` becoming an
/// opaque compatibility action merely because it contains a colon.
fn is_canonical_action(command: &str) -> bool {
    const NAMESPACES: &[&str] = &[
        "bookmark",
        "brightness",
        "calendar",
        "clipboard",
        "clipboard_modify",
        "cmd",
        "dashboard",
        "exec",
        "fav",
        "folder",
        "help",
        "history",
        "keys",
        "launcher",
        "layout",
        "link",
        "macro",
        "media",
        "mg",
        "mkmacro",
        "mm",
        "net",
        "noop",
        "note",
        "plugin",
        "power",
        "process",
        "recycle",
        "settings",
        "shell",
        "snippet",
        "stopwatch",
        "sysinfo",
        "system",
        "tab",
        "tempfile",
        "theme",
        "timer",
        "todo",
        "volume",
        "window",
    ];
    let Some((namespace, suffix)) = command.split_once(':') else {
        return false;
    };
    !suffix.is_empty() && NAMESPACES.contains(&namespace)
}

fn classify_legacy_launcher_command(command: &str, args: Option<&str>) -> (String, bool) {
    if let Some(query) = command.strip_prefix("query:") {
        return (query_action_text(query, args), false);
    }
    if let Some(query) = command.strip_prefix("queryexec:") {
        // No raw-query syntax promises queryexec's immediate first-result activation.
        return (query_action_text(query, args), true);
    }
    let unsafe_target = command.starts_with("http://")
        || command.starts_with("https://")
        || is_absolute_legacy_target(command)
        || is_canonical_action(command);
    (join_legacy_text(command, args), unsafe_target)
}
/// Splits the schema-4 ambiguous `image_result` condition into an explicit live
/// search. Previous-result conditions did not exist in schema 4.
fn migrate_v4_to_v5(value: &mut serde_json::Value) -> Result<()> {
    fn condition(value: &mut serde_json::Value) -> Result<()> {
        let object = value
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("condition is not an object"))?;
        match object.get("type").and_then(serde_json::Value::as_str) {
            Some("image_result") => {
                let asset_id = object
                    .remove("asset_id")
                    .ok_or_else(|| anyhow::anyhow!("legacy image_result has no asset_id"))?;
                if !asset_id.as_u64().is_some_and(|id| id > 0) {
                    anyhow::bail!("legacy image_result has an invalid asset_id")
                }
                if !object
                    .get("found")
                    .is_some_and(serde_json::Value::is_boolean)
                {
                    anyhow::bail!("legacy image_result has an invalid found value")
                }
                object.insert("type".into(), serde_json::json!("image_search"));
                object.insert(
                    "search".into(),
                    serde_json::json!({
                        "asset_id": asset_id,
                        "region": {"type": "desktop"},
                        "tolerance": 0,
                        "alpha": "compare",
                        "return_point": "center"
                    }),
                );
            }
            Some("all" | "any") => {
                if let Some(children) = object.get_mut("conditions").and_then(|v| v.as_array_mut())
                {
                    for child in children {
                        condition(child)?;
                    }
                }
            }
            Some("not") => {
                if let Some(child) = object.get_mut("condition") {
                    condition(child)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    if let Some(macros) = value.get_mut("macros").and_then(|v| v.as_array_mut()) {
        for mac in macros {
            if let Some(steps) = mac.get_mut("steps").and_then(|v| v.as_array_mut()) {
                for step in steps {
                    let Some(action) = step.get_mut("action").and_then(|v| v.as_object_mut())
                    else {
                        continue;
                    };
                    let ty = action.get("type").and_then(|v| v.as_str());
                    if ty == Some("if") {
                        if let Some(data) = action.get_mut("data") {
                            condition(data)?;
                        }
                    } else if matches!(ty, Some("while_start" | "wait_until")) {
                        if let Some(c) = action.get_mut("data").and_then(|v| v.get_mut("condition"))
                        {
                            condition(c)?;
                        }
                    }
                }
            }
        }
    }
    value["schema_version"] = serde_json::json!(5);
    Ok(())
}
/// Adds the document-wide recorder control without replacing a value supplied by
/// a forward-compatible older writer.
fn migrate_v3_to_v4(value: &mut serde_json::Value) {
    if let Some(object) = value.as_object_mut() {
        object
            .entry("settings")
            .or_insert_with(|| serde_json::to_value(MkMacroSettings::default()).unwrap());
        object.insert("schema_version".into(), serde_json::json!(SCHEMA_VERSION));
    }
}
/// Adds schema-3 image matching defaults. Legacy `confidence` is removed rather
/// than translated because its floating-point semantics never matched byte
/// tolerance; this makes migration deterministic.
fn migrate_v2_to_v3(value: &mut serde_json::Value) -> Result<()> {
    let Some(macros) = value.get_mut("macros").and_then(|v| v.as_array_mut()) else {
        value["schema_version"] = serde_json::json!(SCHEMA_VERSION);
        return Ok(());
    };
    for mac in macros {
        if let Some(steps) = mac.get_mut("steps").and_then(|v| v.as_array_mut()) {
            for step in steps {
                if let Some(action) = step.get_mut("action").and_then(|v| v.as_object_mut()) {
                    if matches!(
                        action.get("type").and_then(|v| v.as_str()),
                        Some("image_find" | "image_click")
                    ) {
                        if let Some(data) = action.get_mut("data").and_then(|v| v.as_object_mut()) {
                            data.entry("region")
                                .or_insert_with(|| serde_json::json!({"type":"desktop"}));
                            data.entry("tolerance")
                                .or_insert_with(|| serde_json::json!(0));
                            data.entry("alpha")
                                .or_insert_with(|| serde_json::json!("compare"));
                            data.entry("return_point")
                                .or_insert_with(|| serde_json::json!("center"));
                            data.remove("confidence");
                        }
                    }
                }
            }
        }
    }
    value["schema_version"] = serde_json::json!(SCHEMA_VERSION);
    Ok(())
}
fn migrate_v1_to_v2(value: &mut serde_json::Value) -> Result<()> {
    let macros = value
        .get_mut("macros")
        .and_then(serde_json::Value::as_array_mut)
        .ok_or_else(|| anyhow::anyhow!("version 1 macro document has no macros array"))?;
    for mac in macros {
        let Some(steps) = mac
            .get_mut("steps")
            .and_then(serde_json::Value::as_array_mut)
        else {
            continue;
        };
        for step in steps {
            let Some(action) = step
                .get_mut("action")
                .and_then(serde_json::Value::as_object_mut)
            else {
                continue;
            };
            if action.get("type").and_then(serde_json::Value::as_str) != Some("mouse_move") {
                continue;
            }
            let already = action
                .get("data")
                .and_then(serde_json::Value::as_object)
                .is_some_and(|data| data.contains_key("target"));
            if !already {
                let old = action
                    .remove("data")
                    .ok_or_else(|| anyhow::anyhow!("version 1 mouse_move has no data"))?;
                action.insert(
                    "data".into(),
                    serde_json::json!({"target": old, "duration_ms": 0}),
                );
            }
        }
    }
    value["schema_version"] = serde_json::json!(SCHEMA_VERSION);
    Ok(())
}
pub fn repair_ids(d: &mut MkMacroDocument) -> bool {
    let mut max_id = 0u64;
    for m in &d.macros {
        max_id = max_id.max(m.id);
        for s in &m.steps {
            max_id = max_id.max(s.id);
        }
    }
    let mut next = max_id.checked_add(1).unwrap_or(1);
    let mut changed = false;
    let mut used = HashSet::new();
    for m in &mut d.macros {
        if m.id == 0 || !used.insert(m.id) {
            if let Some(replacement) = next_unused_id(&used, &mut next) {
                m.id = replacement;
                used.insert(replacement);
                changed = true
            }
        }
        let mut steps = HashSet::new();
        for s in &mut m.steps {
            if s.id == 0 || !steps.insert(s.id) {
                if let Some(replacement) = next_unused_id(&steps, &mut next) {
                    s.id = replacement;
                    steps.insert(replacement);
                    changed = true
                }
            }
        }
    }

    // Folder IDs are a separate namespace. In particular, a folder may have the
    // same numeric ID as a macro or step without being repaired.
    let mut max_folder_id = 0u64;
    for folder in &d.folders {
        if folder.id > 0 {
            max_folder_id = max_folder_id.max(folder.id);
        }
    }
    let mut next_folder_id = max_folder_id.checked_add(1).unwrap_or(1);
    let mut used_folder_ids = HashSet::new();
    for folder in &mut d.folders {
        if folder.id == 0 || !used_folder_ids.insert(folder.id) {
            if let Some(replacement) = next_unused_id(&used_folder_ids, &mut next_folder_id) {
                folder.id = replacement;
                used_folder_ids.insert(replacement);
                changed = true;
            }
        }
    }

    let valid_folder_ids: HashSet<u64> = d
        .folders
        .iter()
        .filter_map(|folder| (folder.id > 0).then_some(folder.id))
        .collect();
    for m in &mut d.macros {
        if m.folder_id
            .is_some_and(|folder_id| !valid_folder_ids.contains(&folder_id))
        {
            m.folder_id = None;
            changed = true;
        }
    }

    changed
}

/// Returns the next unused positive ID and advances the cursor. Wrapping to 1
/// after `u64::MAX` makes a saturated high-water mark safe when lower IDs are
/// available, while the cycle check prevents an exhausted namespace from
/// spinning forever.
fn next_unused_id(used: &HashSet<u64>, next: &mut u64) -> Option<u64> {
    let start = (*next).max(1);
    let mut candidate = start;
    loop {
        if !used.contains(&candidate) {
            *next = candidate.checked_add(1).unwrap_or(1);
            return Some(candidate);
        }
        candidate = candidate.checked_add(1).unwrap_or(1);
        if candidate == start {
            return None;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::{AlphaPolicy, MkPoint, ReturnPoint, SearchRegion};
    use std::{sync::mpsc, thread, time::Duration};

    fn png_bytes(color: [u8; 4]) -> Vec<u8> {
        let mut output = std::io::Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(1, 1, image::Rgba(color)))
            .write_to(&mut output, ImageFormat::Png)
            .unwrap();
        output.into_inner()
    }

    fn schema_v7_document(actions: Vec<serde_json::Value>) -> serde_json::Value {
        serde_json::json!({
            "schema_version": 7,
            "settings": serde_json::to_value(MkMacroSettings::default()).unwrap(),
            "macros": [{
                "id": 1,
                "name": "legacy",
                "description": "",
                "enabled": true,
                "hotkey": null,
                "playback": {},
                "steps": actions.into_iter().enumerate().map(|(index, action)| serde_json::json!({
                    "id": index + 1,
                    "enabled": true,
                    "repeat": 1,
                    "delay_after_ms": 0,
                    "on_error": "stop",
                    "action": action
                })).collect::<Vec<_>>(),
                "image_assets": []
            }]
        })
    }

    fn legacy_launcher(command: &str, args: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "type": "launcher_command",
            "data": {"command": command, "args": args}
        })
    }
    fn document() -> MkMacroDocument {
        MkMacroDocument {
            settings: Default::default(),
            schema_version: SCHEMA_VERSION,
            folders: vec![],
            macros: vec![MkMacro {
                id: 7,
                name: "x".into(),
                description: String::new(),
                enabled: true,
                hotkey: None,
                hotkey_scope: Default::default(),
                folder_id: None,
                playback: Default::default(),
                steps: vec![MkStep {
                    id: 9,
                    enabled: true,
                    breakpoint: false,
                    repeat: 1,
                    delay_after_ms: 0,
                    on_error: Default::default(),
                    action: MkAction::Delay(MkDelayPayload {
                        fixed_ms: 1,
                        ..Default::default()
                    }),
                }],
            }],
        }
    }
    #[test]
    fn v7_launcher_commands_are_classified_conservatively() {
        let cases = [
            ("note list", None, "note list", false),
            (
                "search words",
                Some("more words"),
                "search words more words",
                false,
            ),
            ("calculator", Some("  "), "calculator", false),
            ("query:note list", None, "note list", false),
            ("query:f list", None, "f list", false),
            (
                "query:note list",
                Some(r##"{"query":"#work"}"##),
                "note list #work",
                false,
            ),
            ("queryexec:note today", None, "note today", true),
            ("https://example.com", None, "https://example.com", true),
            (
                "/usr/bin/tool",
                Some("--flag"),
                "/usr/bin/tool --flag",
                true,
            ),
            (r"C:\Tools\app.exe", None, r"C:\Tools\app.exe", true),
            (
                r"\\server\share\app.exe",
                None,
                r"\\server\share\app.exe",
                true,
            ),
            (
                "settings:dialog",
                Some("section"),
                "settings:dialog section",
                true,
            ),
            ("volume:toggle_mute", None, "volume:toggle_mute", true),
            ("my application name", None, "my application name", false),
            ("meeting: tomorrow", None, "meeting: tomorrow", false),
        ];
        for (command, args, expected_query, expected_legacy) in cases {
            let mut value = serde_json::json!({"schema_version":7,"macros":[{
                "name":"legacy","steps":[{"action":{"type":"launcher_command","data":{
                    "command":command,"args":args}}}]}]});
            migrate_v7_to_v8(&mut value).unwrap();
            let data = &value["macros"][0]["steps"][0]["action"]["data"];
            assert_eq!(data["query"], expected_query, "{command}");
            assert_eq!(
                data.get("legacy_resolved_action").is_some(),
                expected_legacy,
                "{command}"
            );
            assert!(data.get("command").is_none());
            assert!(data.get("args").is_none());
            if expected_legacy {
                assert_eq!(data["legacy_resolved_action"]["action"], command);
                assert_eq!(
                    data["legacy_resolved_action"]["args"],
                    serde_json::json!(args)
                );
            }
            let doc: MkMacroDocument = serde_json::from_value(value.clone()).unwrap();
            let stable = serde_json::to_value(&doc).unwrap();
            let decoded: MkMacroDocument = serde_json::from_value(stable.clone()).unwrap();
            assert_eq!(serde_json::to_value(decoded).unwrap(), stable);
        }
    }

    #[test]
    fn production_load_migrates_manually_entered_launcher_query() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(MKMACROS_FILE);
        let legacy =
            schema_v7_document(vec![legacy_launcher("note list", serde_json::Value::Null)]);
        fs::write(&path, serde_json::to_vec_pretty(&legacy).unwrap()).unwrap();

        let (store, disposition) = MkMacroStore::open(dir.path()).unwrap();
        assert!(matches!(disposition, LoadDisposition::Loaded));
        let document = store.snapshot();
        assert_eq!(document.schema_version, SCHEMA_VERSION);
        let MkAction::LauncherCommand(payload) = &document.macros[0].steps[0].action else {
            panic!("legacy Launcher step was not migrated")
        };
        assert_eq!(payload.query, "note list");
        assert_eq!(payload.legacy_resolved_action, None);

        let persisted: serde_json::Value =
            serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        let data = &persisted["macros"][0]["steps"][0]["action"]["data"];
        assert_eq!(persisted["schema_version"], SCHEMA_VERSION);
        assert_eq!(data["query"], "note list");
        assert!(data.get("legacy_resolved_action").is_none());
        assert!(data.get("command").is_none());
        assert!(data.get("args").is_none());
    }

    #[test]
    fn query_picker_migration_strips_prefix_and_honors_json_query_argument() {
        let mut value = schema_v7_document(vec![
            legacy_launcher("query:bm list", serde_json::Value::Null),
            legacy_launcher(
                "query:bm list",
                serde_json::json!(r#"{"query":"unique bookmark"}"#),
            ),
        ]);
        migrate_v7_to_v8(&mut value).unwrap();

        for (index, expected) in [(0, "bm list"), (1, "bm list unique bookmark")] {
            let data = &value["macros"][0]["steps"][index]["action"]["data"];
            assert_eq!(data["query"], expected);
            assert!(data.get("legacy_resolved_action").is_none());
            assert!(data.get("command").is_none());
            assert!(data.get("args").is_none());
        }
        assert_eq!(
            query_action_text("bm list", Some(r#"{"query":"unique bookmark"}"#)),
            "bm list unique bookmark"
        );
    }

    #[test]
    fn canonical_action_migration_preserves_complete_resolved_action() {
        let mut value = schema_v7_document(vec![legacy_launcher(
            "settings:open_section",
            serde_json::json!("advanced-launcher-options"),
        )]);
        migrate_v7_to_v8(&mut value).unwrap();
        let data = &value["macros"][0]["steps"][0]["action"]["data"];
        assert_eq!(
            data["query"],
            "settings:open_section advanced-launcher-options"
        );
        assert_eq!(
            data["legacy_resolved_action"],
            serde_json::json!({
                "label": "settings:open_section",
                "desc": "",
                "action": "settings:open_section",
                "args": "advanced-launcher-options"
            })
        );

        let document: MkMacroDocument = serde_json::from_value(value).unwrap();
        let MkAction::LauncherCommand(payload) = &document.macros[0].steps[0].action else {
            panic!()
        };
        assert_eq!(
            payload.legacy_resolved_action,
            Some(crate::actions::Action {
                label: "settings:open_section".into(),
                desc: String::new(),
                action: "settings:open_section".into(),
                args: Some("advanced-launcher-options".into()),
            })
        );
        assert_eq!(
            serde_json::from_value::<MkMacroDocument>(serde_json::to_value(&document).unwrap())
                .unwrap(),
            document
        );
    }

    #[test]
    fn production_store_v7_to_v8_round_trip_is_structurally_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(MKMACROS_FILE);
        let legacy = schema_v7_document(vec![
            legacy_launcher("note list", serde_json::Value::Null),
            legacy_launcher("settings:open_section", serde_json::json!("launcher")),
        ]);
        fs::write(&path, serde_json::to_vec_pretty(&legacy).unwrap()).unwrap();

        let (store, disposition) = MkMacroStore::open(dir.path()).unwrap();
        assert!(matches!(disposition, LoadDisposition::Loaded));
        let first = store.snapshot();
        assert_eq!(first.schema_version, SCHEMA_VERSION);
        store.save((*first).clone()).unwrap();
        drop(store);

        let first_saved: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let (reopened, disposition) = MkMacroStore::open(dir.path()).unwrap();
        assert!(matches!(disposition, LoadDisposition::Loaded));
        assert_eq!(reopened.snapshot().as_ref(), first.as_ref());
        reopened.save((*reopened.snapshot()).clone()).unwrap();
        let second_saved: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(second_saved, first_saved);
        assert_eq!(second_saved["schema_version"], SCHEMA_VERSION);
        let text = serde_json::to_string(&second_saved).unwrap();
        assert!(!text.contains("\"command\""));
        assert_eq!(text.matches("legacy_resolved_action").count(), 1);
    }

    #[test]
    fn malformed_v7_launcher_command_reports_location() {
        let mut value = serde_json::json!({"schema_version":7,"macros":[{"steps":[
            {"action":{"type":"delay","data":{"milliseconds":1}}},
            {"action":{"type":"launcher_command","data":{"command":42}}}
        ]}]});
        let error = migrate_v7_to_v8(&mut value).unwrap_err().to_string();
        assert!(error.contains("macro 0 step 1"), "{error}");
        assert!(error.contains("command must be a string"), "{error}");
        // Version is only advanced after every step succeeds.
        assert_eq!(value["schema_version"], 7);
    }

    #[test]
    fn v7_migration_visits_all_macros_and_preserves_other_actions() {
        let untouched = serde_json::json!({"type":"text","data":{"text":"a:  b","mode":"type"}});
        let mut value = serde_json::json!({"schema_version":7,"macros":[
            {"steps":[
                {"action":{"type":"launcher_command","data":{"command":"one","args":null}}},
                {"action":untouched.clone()},
                {"action":{"type":"launcher_command","data":{"command":"query:two","args":null}}}
            ]},
            {"steps":[{"action":{"type":"launcher_command","data":{
                "command":"https://example.com","args":"--old"}}}]}
        ]});
        migrate_v7_to_v8(&mut value).unwrap();
        assert_eq!(
            value["macros"][0]["steps"][0]["action"]["data"]["query"],
            "one"
        );
        assert_eq!(value["macros"][0]["steps"][1]["action"], untouched);
        assert_eq!(
            value["macros"][0]["steps"][2]["action"]["data"]["query"],
            "two"
        );
        assert_eq!(
            value["macros"][1]["steps"][0]["action"]["data"]["legacy_resolved_action"]["args"],
            "--old"
        );
    }

    #[test]
    fn old_launcher_shape_is_not_a_serde_alias_for_schema_8() {
        let action = serde_json::json!({
            "type": "launcher_command",
            "data": {"command": "tool.exe", "args": "--flag"}
        });
        assert!(serde_json::from_value::<MkAction>(action).is_err());
    }
    #[test]
    fn v4_image_conditions_migrate_recursively_with_live_search_defaults() {
        let legacy = serde_json::json!({
            "schema_version": 4,
            "macros": [{"id":1,"name":"m","steps":[{"id":2,"action":{
                "type":"wait_until","data":{"wait":{"timeout_ms":1,"poll_interval_ms":1},
                "condition":{"type":"all","conditions":[
                    {"type":"image_result","asset_id":7,"found":true},
                    {"type":"not","condition":{"type":"any","conditions":[
                        {"type":"image_result","asset_id":8,"found":false}
                    ]}}
                ]}}
            }}]}]
        });
        let mut migrated = legacy;
        migrate_v4_to_v5(&mut migrated).unwrap();
        assert_eq!(migrated["schema_version"], 5);
        let json = &migrated["macros"][0]["steps"][0]["action"]["data"]["condition"];
        let first = &json["conditions"][0];
        assert_eq!(first["type"], "image_search");
        assert_eq!(first["found"], true);
        assert_eq!(first["search"]["asset_id"], 7);
        assert_eq!(
            first["search"]["region"],
            serde_json::json!({"type":"desktop"})
        );
        assert_eq!(first["search"]["tolerance"], 0);
        assert_eq!(first["search"]["alpha"], "compare");
        assert_eq!(first["search"]["return_point"], "center");
        let nested = &json["conditions"][1]["condition"]["conditions"][0];
        assert_eq!(nested["type"], "image_search");
        assert_eq!(nested["found"], false);
    }

    #[test]
    fn complete_v4_condition_document_migrates_once_and_round_trips_stably() {
        let unrelated = serde_json::json!({
            "type":"variable", "name":"sentinel", "op":"eq",
            "value":{"type":"string","value":"unchanged"}
        });
        let legacy = serde_json::json!({
            "schema_version":4,
            "settings":{"record_toggle_hotkey":{"key":{"function":9},"modifiers":[]}},
            "macros":[{"id":1,"name":"complete","description":"","enabled":true,
                "hotkey":null,"playback":{"speed_percent":100,"random_delay_ms":0,
                    "random_offset_px":0},"image_assets":[],"steps":[
                {"id":10,"enabled":true,"repeat":1,"delay_after_ms":0,"on_error":"stop",
                 "action":{"type":"if","data":{"type":"image_result","asset_id":7,"found":true}}},
                {"id":11,"enabled":true,"repeat":1,"delay_after_ms":0,"on_error":"stop",
                 "action":{"type":"while_start","data":{"condition":{"type":"not","condition":
                    {"type":"image_result","asset_id":8,"found":false}},"max_iterations":9}}},
                {"id":12,"enabled":true,"repeat":1,"delay_after_ms":0,"on_error":"stop",
                 "action":{"type":"wait_until","data":{"wait":{"timeout_ms":10,"poll_interval_ms":1},
                    "condition":{"type":"all","conditions":[
                        {"type":"any","conditions":[unrelated.clone(),{"type":"not","condition":
                            {"type":"image_result","asset_id":9,"found":true}}]},
                        {"type":"image_result","asset_id":10,"found":false}]}}}}
            ]}]
        });
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(MKMACROS_FILE);
        fs::write(&path, serde_json::to_vec_pretty(&legacy).unwrap()).unwrap();

        let (doc, changed) = read_document(&path).unwrap().unwrap();
        assert!(changed);
        assert_eq!(doc.schema_version, SCHEMA_VERSION);
        let encoded = serde_json::to_value(&doc).unwrap();
        assert_eq!(
            encoded["macros"][0]["steps"][2]["action"]["data"]["condition"]["conditions"][0]["conditions"]
                [0],
            unrelated
        );
        for (step, found) in [(0, true), (1, false), (2, true)] {
            let action = &doc.macros[0].steps[step].action;
            let condition: &MkCondition = match action {
                MkAction::If(c) => c,
                MkAction::WhileStart { condition, .. } => condition,
                MkAction::WaitUntil { condition, .. } => condition,
                _ => unreachable!(),
            };
            let image = if step == 1 {
                let MkCondition::Not { condition } = condition else {
                    panic!()
                };
                condition.as_ref()
            } else if step == 2 {
                let MkCondition::All { conditions } = condition else {
                    panic!()
                };
                let MkCondition::Any { conditions } = &conditions[0] else {
                    panic!()
                };
                let MkCondition::Not { condition } = &conditions[1] else {
                    panic!()
                };
                condition.as_ref()
            } else {
                condition
            };
            let MkCondition::ImageSearch {
                search,
                found: actual,
            } = image
            else {
                panic!()
            };
            assert_eq!(actual, &found);
            assert_eq!(search.region, SearchRegion::Desktop);
            assert_eq!(search.tolerance, 0);
            assert_eq!(search.alpha, AlphaPolicy::Compare);
            assert_eq!(search.return_point, ReturnPoint::Center);
        }

        persist(&path, &doc).unwrap();
        let (reloaded, changed_again) = read_document(&path).unwrap().unwrap();
        assert!(!changed_again);
        assert_eq!(reloaded, doc);
    }

    #[test]
    fn malformed_v4_image_conditions_are_recoverable_load_errors() {
        for malformed in [
            serde_json::json!({"type":"image_result","found":true}),
            serde_json::json!({"type":"image_result","asset_id":0,"found":true}),
            serde_json::json!({"type":"image_result","asset_id":7,"found":"yes"}),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let value = serde_json::json!({"schema_version":4,"macros":[{"id":1,"name":"m",
                "steps":[{"id":2,"action":{"type":"if","data":malformed}}]}]});
            fs::write(
                dir.path().join(MKMACROS_FILE),
                serde_json::to_vec(&value).unwrap(),
            )
            .unwrap();
            let (_, disposition) = MkMacroStore::open(dir.path()).unwrap();
            assert!(matches!(
                disposition,
                LoadDisposition::NeedsUserRecovery { .. }
            ));
        }
    }

    #[test]
    fn v5_condition_types_round_trip_with_distinct_names() {
        let conditions = vec![
            MkCondition::ImageSearch {
                search: MkImageSearchCondition {
                    image: MkImageRef::from_filename("3.png"),
                    region: SearchRegion::Desktop,
                    tolerance: 4,
                    alpha: AlphaPolicy::Ignore,
                    return_point: ReturnPoint::TopLeft,
                },
                found: false,
            },
            MkCondition::PreviousImageResult {
                image: None,
                found: true,
            },
            MkCondition::PreviousImageResult {
                image: Some(MkImageRef::from_filename("3.png")),
                found: false,
            },
        ];
        let json = serde_json::to_string(&conditions).unwrap();
        assert!(json.contains("image_search"));
        assert!(json.contains("previous_image_result"));
        assert!(!json.contains("\"type\":\"image_result\""));
        assert_eq!(
            serde_json::from_str::<Vec<MkCondition>>(&json).unwrap(),
            conditions
        );
    }
    #[test]
    fn missing_empty_and_malformed_are_recoverable() {
        let d = tempfile::tempdir().unwrap();
        let (s, x) = MkMacroStore::open(d.path()).unwrap();
        assert!(matches!(x, LoadDisposition::Missing));
        drop(s);
        fs::write(d.path().join(MKMACROS_FILE), "").unwrap();
        let (_, x) = MkMacroStore::open(d.path()).unwrap();
        assert!(matches!(x, LoadDisposition::Empty));
        fs::write(d.path().join(MKMACROS_FILE), "{").unwrap();
        let (_, x) = MkMacroStore::open(d.path()).unwrap();
        assert!(matches!(x, LoadDisposition::NeedsUserRecovery { .. }))
    }
    #[test]
    fn round_trip_reorder_preserves_ids() {
        let d = tempfile::tempdir().unwrap();
        let (s, _) = MkMacroStore::open(d.path()).unwrap();
        s.save(document()).unwrap();
        drop(s);
        let (s, _) = MkMacroStore::open(d.path()).unwrap();
        assert_eq!(s.snapshot().macros[0].id, 7);
        assert_eq!(s.snapshot().macros[0].steps[0].id, 9)
    }
    #[test]
    fn saving_empty_document_preserves_canonical_file_and_snapshot() {
        let d = tempfile::tempdir().unwrap();
        let (s, _) = MkMacroStore::open(d.path()).unwrap();
        s.save(document()).unwrap();
        let saved = s.save(MkMacroDocument::default()).unwrap();
        assert_eq!(saved.schema_version, SCHEMA_VERSION);
        assert!(saved.macros.is_empty());
        assert!(s.snapshot().macros.is_empty());

        let path = d.path().join(MKMACROS_FILE);
        let bytes = fs::read(&path).unwrap();
        assert!(!bytes.is_empty());
        let disk: MkMacroDocument = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(disk.schema_version, SCHEMA_VERSION);
        assert!(disk.macros.is_empty());
    }

    #[test]
    fn watcher_reload_cannot_publish_across_newer_save_transaction() {
        let d = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(d.path()).unwrap();
        store.save(document()).unwrap();
        let store = Arc::new(store);
        let (started_tx, started_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let reload_inner = Arc::clone(&store.inner);

        store
            .save_transaction(&mut MkMacroDocument::default(), || {
                thread::spawn(move || {
                    started_tx.send(()).unwrap();
                    reload_from_disk(&reload_inner);
                    done_tx.send(()).unwrap();
                });
                started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
                assert!(done_rx.recv_timeout(Duration::from_millis(50)).is_err());
            })
            .unwrap();

        done_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(store.snapshot().macros.is_empty());
        let disk: MkMacroDocument =
            serde_json::from_slice(&fs::read(d.path().join(MKMACROS_FILE)).unwrap()).unwrap();
        assert!(disk.macros.is_empty());
    }
    #[test]
    fn old_migrates_and_schema_newer_than_ten_is_rejected() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join(MKMACROS_FILE);
        let mut v = serde_json::to_value(document()).unwrap();
        v.as_object_mut().unwrap().remove("schema_version");
        fs::write(&p, serde_json::to_vec(&v).unwrap()).unwrap();
        let (s, _) = MkMacroStore::open(d.path()).unwrap();
        assert_eq!(s.snapshot().schema_version, SCHEMA_VERSION);
        drop(s);
        fs::write(
            &p,
            format!(r#"{{"schema_version":{},"macros":[]}}"#, SCHEMA_VERSION + 1),
        )
        .unwrap();
        let (_, disposition) = MkMacroStore::open(d.path()).unwrap();
        let LoadDisposition::NeedsUserRecovery { error } = disposition else {
            panic!("schema 12 should require user recovery")
        };
        assert!(error.contains("schema version 12"), "{error}");
        assert!(error.contains("supported version 11"), "{error}");
    }
    #[test]
    fn version_one_mouse_move_migrates_once_to_payload() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join(MKMACROS_FILE);
        fs::write(
            &p,
            r#"{
          "schema_version": 1,
          "macros": [{"id": 1, "name": "legacy", "steps": [{
            "id": 2, "action": {"type": "mouse_move", "data": {
              "kind": "screen", "point": {"x": 12, "y": -4}
            }}
          }]}]
        }"#,
        )
        .unwrap();
        let (doc, changed) = read_document(&p).unwrap().unwrap();
        assert!(changed);
        assert_eq!(doc.schema_version, SCHEMA_VERSION);
        let MkAction::MouseMove(payload) = &doc.macros[0].steps[0].action else {
            panic!()
        };
        assert_eq!(payload.duration_ms, 0);
        assert_eq!(
            payload.target,
            MkCoordinateTarget::Screen {
                point: MkPoint { x: 12, y: -4 }
            }
        );
        persist(&p, &doc).unwrap();
        let text = fs::read_to_string(&p).unwrap();
        assert!(text.contains("\"target\""));
        let (again, changed_again) = read_document(&p).unwrap().unwrap();
        assert!(!changed_again);
        assert_eq!(again, doc);
    }
    #[test]
    fn repairs_zero_and_duplicates() {
        let mut d = document();
        d.macros.push(d.macros[0].clone());
        let duplicate_step = d.macros[0].steps[0].clone();
        d.macros[0].steps.push(duplicate_step);
        d.macros[0].id = 0;
        d.macros[0].steps[0].id = 0;
        assert!(repair_ids(&mut d));
        assert!(d.macros.iter().all(|m| m.id > 0));
        assert_ne!(d.macros[0].id, d.macros[1].id);
        assert_ne!(d.macros[0].steps[0].id, d.macros[0].steps[1].id)
    }
    #[test]
    fn repairs_folder_ids_and_memberships_without_losing_content() {
        let mut d = document();
        let mut valid = d.macros[0].clone();
        valid.id = 42;
        valid.name = "valid".into();
        valid.steps[0].id = 42;
        valid.folder_id = Some(42);
        let mut dangling = d.macros[0].clone();
        dangling.id = 100;
        dangling.name = "dangling".into();
        dangling.steps[0].id = 101;
        dangling.folder_id = Some(999);
        let mut unfiled = d.macros[0].clone();
        unfiled.id = 200;
        unfiled.name = "unfiled".into();
        unfiled.steps[0].id = 201;
        unfiled.folder_id = None;
        d.macros = vec![valid, dangling, unfiled];
        let original_macros = d.macros.clone();
        d.folders = vec![
            MkMacroFolder {
                id: 0,
                name: "zero one".into(),
            },
            MkMacroFolder {
                id: 0,
                name: "zero two".into(),
            },
            MkMacroFolder {
                id: 42,
                name: "valid folder".into(),
            },
            MkMacroFolder {
                id: 42,
                name: "duplicate folder".into(),
            },
            MkMacroFolder {
                id: 7,
                name: "another valid folder".into(),
            },
        ];

        assert!(repair_ids(&mut d));
        let folder_ids: Vec<u64> = d.folders.iter().map(|folder| folder.id).collect();
        assert_eq!(folder_ids, vec![43, 44, 42, 45, 7]);
        assert!(folder_ids.iter().all(|id| *id > 0));
        assert_eq!(
            folder_ids.iter().collect::<HashSet<_>>().len(),
            folder_ids.len()
        );
        assert_eq!(d.macros[0].folder_id, Some(42));
        assert_eq!(d.macros[1].folder_id, None);
        assert_eq!(d.macros[2].folder_id, None);
        assert_eq!(d.macros.len(), original_macros.len());
        for (actual, original) in d.macros.iter().zip(original_macros.iter()) {
            let mut expected = original.clone();
            if expected.folder_id == Some(999) {
                expected.folder_id = None;
            }
            assert_eq!(actual, &expected);
        }

        let repaired = d.clone();
        assert!(!repair_ids(&mut d));
        assert_eq!(d, repaired);
    }

    #[test]
    fn folder_id_repair_wraps_safely_after_u64_max() {
        let mut d = document();
        d.macros[0].folder_id = Some(u64::MAX);
        d.folders = vec![
            MkMacroFolder {
                id: u64::MAX,
                name: "max".into(),
            },
            MkMacroFolder {
                id: u64::MAX,
                name: "duplicate max".into(),
            },
            MkMacroFolder {
                id: 0,
                name: "zero".into(),
            },
        ];

        assert!(repair_ids(&mut d));
        assert_eq!(
            d.folders.iter().map(|folder| folder.id).collect::<Vec<_>>(),
            vec![u64::MAX, 1, 2]
        );
        assert_eq!(d.macros[0].folder_id, Some(u64::MAX));
        let repaired = d.clone();
        assert!(!repair_ids(&mut d));
        assert_eq!(d, repaired);
    }
    #[test]
    fn image_references_are_direct_children_of_the_canonical_root() {
        let d = tempfile::tempdir().unwrap();
        let (s, _) = MkMacroStore::open(d.path()).unwrap();
        for name in [
            "",
            ".",
            "..",
            "../x.png",
            "nested/x.png",
            "/tmp/x.png",
            r"C:\x.png",
            r"\\server\share\x.png",
            "x.jpg",
        ] {
            assert!(
                s.image_path(&MkImageRef::from_filename(name)).is_err(),
                "accepted {name:?}"
            );
        }
        assert!(
            s.image_path(&MkImageRef::from_filename("login.png"))
                .is_ok()
        );
    }
    #[test]
    fn image_enumeration_is_flat_sorted_and_ignores_nested_or_non_png_files() {
        let d = tempfile::tempdir().unwrap();
        let (s, _) = MkMacroStore::open(d.path()).unwrap();
        let dir = d.path().join(ASSET_DIRECTORY);
        fs::create_dir_all(&dir).unwrap();
        for name in ["z.png", "a.PNG", "notes.txt", "nested.png"] {
            if name == "nested.png" {
                fs::create_dir_all(dir.join(name)).unwrap();
            } else {
                fs::write(dir.join(name), png_bytes([1, 2, 3, 255])).unwrap();
            }
        }
        fs::create_dir_all(dir.join("nested")).unwrap();
        fs::write(
            dir.join("nested").join("inner.png"),
            png_bytes([1, 2, 3, 255]),
        )
        .unwrap();
        assert_eq!(
            s.image_refs()
                .unwrap()
                .iter()
                .map(|image| image.filename())
                .collect::<Vec<_>>(),
            vec!["a.PNG", "z.png"]
        );
    }

    #[test]
    fn replacing_a_reference_never_deletes_the_shared_library_file() {
        let d = tempfile::tempdir().unwrap();
        let (s, _) = MkMacroStore::open(d.path()).unwrap();
        let old = MkImageRef::from_filename("old.png");
        let new = MkImageRef::from_filename("new.png");
        s.write_captured_png(
            &RgbaImage::from_pixel(1, 1, image::Rgba([1, 2, 3, 255])),
            old.clone(),
            ImageImportChoice::SaveAs(old.clone()),
        )
        .unwrap();
        s.write_captured_png(
            &RgbaImage::from_pixel(1, 1, image::Rgba([5, 6, 7, 255])),
            new.clone(),
            ImageImportChoice::SaveAs(new.clone()),
        )
        .unwrap();
        assert!(s.image_path(&old).unwrap().is_file());
        assert!(s.image_path(&new).unwrap().is_file());
    }

    #[test]
    fn schema_ten_migration_flattens_all_reference_forms_and_keeps_legacy_sources() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join(ASSET_DIRECTORY);
        let first_source = root.join("2").join("7.png");
        let second_source = root.join("2").join("8.png");
        let third_source = root.join("2").join("9.png");
        fs::create_dir_all(first_source.parent().unwrap()).unwrap();
        fs::write(&first_source, png_bytes([1, 2, 3, 255])).unwrap();
        fs::write(&second_source, png_bytes([9, 8, 7, 255])).unwrap();
        fs::write(&third_source, png_bytes([4, 5, 6, 255])).unwrap();
        let legacy = serde_json::json!({
            "schema_version": 10,
            "macros": [{
                "id": 2,
                "name": "legacy",
                "image_assets": [
                    {"id": 7, "name": "Login Button", "relative_path": "old/7.png"},
                    {"id": 8, "name": "Login Button", "relative_path": "old/8.png"},
                    {"id": 9, "name": "Login Button", "relative_path": "old/9.png"}
                ],
                "steps": [
                    {"id": 1, "action": {"type": "image_find", "data": {
                        "asset_id": 7, "wait": {"timeout_ms": 1, "poll_interval_ms": 1}
                    }}},
                    {"id": 2, "action": {"type": "if", "data": {
                        "type": "all", "conditions": [
                            {"type": "image_search", "search": {
                                "asset_id": 8, "region": {"type": "desktop"}
                            }, "found": true},
                            {"type": "not", "condition": {
                                "type": "previous_image_result", "asset_id": null, "found": false
                            }}
                        ]
                    }}},
                    {"id": 3, "action": {"type": "mouse_move", "data": {
                        "target": {"kind": "image", "asset_id": 7, "offset": {"x": 3, "y": -2}},
                        "duration_ms": 0
                    }}}
                ]
            }]
        });
        fs::write(
            d.path().join(MKMACROS_FILE),
            serde_json::to_vec_pretty(&legacy).unwrap(),
        )
        .unwrap();

        let (store, disposition) = MkMacroStore::open(d.path()).unwrap();
        assert!(matches!(disposition, LoadDisposition::Loaded));
        assert_eq!(store.snapshot().schema_version, SCHEMA_VERSION);
        let first = MkImageRef::from_filename("Login Button.png");
        let second = MkImageRef::from_filename("Login Button_2.png");
        let third = MkImageRef::from_filename("Login Button_3.png");
        assert_eq!(
            store.image_refs().unwrap(),
            vec![first.clone(), second.clone(), third.clone()]
        );
        assert_eq!(fs::read(&first_source).unwrap(), png_bytes([1, 2, 3, 255]));
        assert_eq!(fs::read(&second_source).unwrap(), png_bytes([9, 8, 7, 255]));
        assert_eq!(fs::read(&third_source).unwrap(), png_bytes([4, 5, 6, 255]));
        let json = serde_json::to_value(store.snapshot().as_ref()).unwrap();
        let text = serde_json::to_string(&json).unwrap();
        assert!(!text.contains("asset_id"));
        assert!(!text.contains("image_assets"));
        assert_eq!(
            json["macros"][0]["steps"][0]["action"]["data"]["image"],
            first.filename()
        );
        assert_eq!(
            json["macros"][0]["steps"][1]["action"]["data"]["conditions"][0]["search"]["image"],
            second.filename()
        );
        assert!(
            json["macros"][0]["steps"][1]["action"]["data"]["conditions"][1]["condition"]["image"]
                .is_null()
        );
        assert_eq!(
            json["macros"][0]["steps"][2]["action"]["data"]["target"]["image"],
            first.filename()
        );
        let persisted: serde_json::Value =
            serde_json::from_slice(&fs::read(d.path().join(MKMACROS_FILE)).unwrap()).unwrap();
        assert_eq!(persisted["schema_version"], 11);
        assert_eq!(persisted, json);
    }

    #[test]
    fn schema_ten_missing_reference_is_explicit_without_blocking_other_assets() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join(ASSET_DIRECTORY).join("6");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("6.png"), png_bytes([4, 5, 6, 255])).unwrap();
        let legacy = serde_json::json!({
            "schema_version": 10,
            "macros": [{"id": 5, "name": "missing", "image_assets": [], "steps": [
                {"id": 1, "action": {"type": "image_find", "data": {
                    "asset_id": 9, "wait": {"timeout_ms": 1, "poll_interval_ms": 1}
                }}}
            ]}, {
                "id": 6, "name": "present", "image_assets": [
                    {"id": 6, "name": "Present", "relative_path": "6.png"}
                ], "steps": []
            }]
        });
        fs::write(
            d.path().join(MKMACROS_FILE),
            serde_json::to_vec(&legacy).unwrap(),
        )
        .unwrap();
        let (store, disposition) = MkMacroStore::open(d.path()).unwrap();
        assert!(matches!(disposition, LoadDisposition::Loaded));
        let json = serde_json::to_value(store.snapshot().as_ref()).unwrap();
        assert_eq!(
            json["macros"][0]["steps"][0]["action"]["data"]["image"],
            "missing_image_5_9.png"
        );
        assert_eq!(json["macros"][1]["steps"], serde_json::json!([]));
        assert!(
            store
                .image_path(&MkImageRef::from_filename("missing_image_5_9.png"))
                .unwrap()
                .exists()
                == false
        );
        assert!(
            store
                .image_path(&MkImageRef::from_filename("Present.png"))
                .unwrap()
                .is_file()
        );
    }

    #[test]
    fn schema_ten_copy_failure_leaves_json_and_flat_library_untouched() {
        let d = tempfile::tempdir().unwrap();
        let source = d.path().join(ASSET_DIRECTORY).join("7").join("3.png");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, b"corrupt png").unwrap();
        let legacy = serde_json::json!({
            "schema_version": 10,
            "macros": [{"id": 7, "name": "bad", "image_assets": [
                {"id": 3, "name": "Bad", "relative_path": "3.png"}
            ], "steps": []}]
        });
        let original = serde_json::to_vec(&legacy).unwrap();
        let path = d.path().join(MKMACROS_FILE);
        fs::write(&path, &original).unwrap();
        let (_, disposition) = MkMacroStore::open(d.path()).unwrap();
        assert!(matches!(
            disposition,
            LoadDisposition::NeedsUserRecovery { .. }
        ));
        assert_eq!(fs::read(&path).unwrap(), original);
        assert!(!d.path().join(ASSET_DIRECTORY).join("Bad.png").exists());
    }

    #[test]
    fn schema_ten_retry_reuses_identical_flat_destination_without_suffix_drift() {
        let d = tempfile::tempdir().unwrap();
        let source = d.path().join(ASSET_DIRECTORY).join("8").join("4.png");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        let bytes = png_bytes([7, 7, 7, 255]);
        fs::write(&source, &bytes).unwrap();
        let legacy = serde_json::json!({
            "schema_version": 10,
            "macros": [{"id": 8, "name": "retry", "image_assets": [
                {"id": 4, "name": "Retry", "relative_path": "4.png"}
            ], "steps": [{"id": 1, "action": {"type": "image_find", "data": {
                "asset_id": 4, "wait": {"timeout_ms": 1, "poll_interval_ms": 1}
            }}}]}]
        });
        let mut first = legacy.clone();
        migrate_v10_to_v11(&mut first, &d.path().join(MKMACROS_FILE)).unwrap();
        let mut second = legacy;
        migrate_v10_to_v11(&mut second, &d.path().join(MKMACROS_FILE)).unwrap();
        assert_eq!(first, second);
        assert!(d.path().join(ASSET_DIRECTORY).join("Retry.png").is_file());
        assert!(!d.path().join(ASSET_DIRECTORY).join("Retry_2.png").exists());
    }

    #[test]
    fn version_two_images_migrate_once_and_drop_confidence() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join(MKMACROS_FILE);
        fs::write(&p, r#"{"schema_version":2,"macros":[{"id":1,"name":"m","steps":[{"id":2,"action":{"type":"image_find","data":{"asset_id":9,"wait":{"timeout_ms":20,"poll_interval_ms":5},"confidence":0.8}}}]}]}"#).unwrap();
        let (doc, changed) = read_document(&p).unwrap().unwrap();
        assert!(changed);
        let MkAction::ImageFind(image) = &doc.macros[0].steps[0].action else {
            panic!()
        };
        assert_eq!(image.image.filename(), "missing_image_1_9.png");
        assert_eq!(image.wait.timeout_ms, 20);
        assert_eq!(image.region, SearchRegion::Desktop);
        assert_eq!(image.tolerance, 0);
        persist(&p, &doc).unwrap();
        assert!(!fs::read_to_string(&p).unwrap().contains("confidence"));
        assert!(!read_document(&p).unwrap().unwrap().1);
    }

    #[test]
    fn schema_eight_migration_covers_fields_boundaries_virtual_desktops_and_errors() {
        let mut value = serde_json::json!({"schema_version":8,"macros":[{"name":"legacy","hotkey_scope":{"type":"active_window","data":{"title":"Editor"}},"folder_id":7,"steps":[
            {"action":{"type":"delay","data":{"milliseconds":0}}}, {"action":{"type":"delay","data":{"milliseconds":u64::MAX}}},
            {"action":{"type":"virtual_desktop","data":"create"}}, {"action":{"type":"virtual_desktop","data":"switch_left"}},
            {"action":{"type":"virtual_desktop","data":"switch_right"}}, {"action":{"type":"virtual_desktop","data":"close_current"}}
        ]}]});
        migrate_v8_to_v9(&mut value).unwrap();
        assert_eq!(value["folders"], serde_json::json!([]));
        assert_eq!(value["macros"][0]["folder_id"], 7);
        assert_eq!(value["macros"][0]["hotkey_scope"]["type"], "active_window");
        assert_eq!(
            value["macros"][0]["steps"][0]["action"]["data"]["fixed_ms"],
            0
        );
        assert_eq!(
            value["macros"][0]["steps"][1]["action"]["data"]["maximum_ms"],
            18446744073709551615u64
        );
        for (index, action) in ["create", "switch_left", "switch_right", "close_current"]
            .iter()
            .enumerate()
        {
            assert_eq!(
                value["macros"][0]["steps"][index + 2]["action"],
                serde_json::json!({"type":"virtual_desktop","data":action})
            );
        }
        for data in [
            serde_json::json!(null),
            serde_json::json!({"milliseconds":-1}),
            serde_json::json!({"milliseconds":"1"}),
        ] {
            let mut malformed = serde_json::json!({"schema_version":8,"macros":[{"steps":[{"action":{"type":"delay","data":data}}]}]});
            let error = migrate_v8_to_v9(&mut malformed).unwrap_err().to_string();
            assert!(error.contains("macro 0 step 0"), "{error}");
            assert_eq!(malformed["schema_version"], 8);
        }
    }

    #[test]
    fn schema_seven_load_runs_all_migrations_and_round_trips_as_ten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(MKMACROS_FILE);
        let value = schema_v7_document(vec![
            legacy_launcher("note list", serde_json::Value::Null),
            serde_json::json!({"type":"delay","data":{"milliseconds":123}}),
        ]);
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        let (doc, changed) = read_document(&path).unwrap().unwrap();
        assert!(changed);
        assert_eq!(doc.schema_version, SCHEMA_VERSION);
        assert_eq!(doc.macros[0].hotkey_scope, MkHotkeyScope::AnyWindow);
        assert_eq!(
            doc.macros[0].steps[1].action,
            MkAction::Delay(MkDelayPayload {
                fixed_ms: 123,
                maximum_ms: 123,
                ..Default::default()
            })
        );
        persist(&path, &doc).unwrap();
        let (again, changed_again) = read_document(&path).unwrap().unwrap();
        assert!(!changed_again);
        assert_eq!(again, doc);
    }
    #[test]
    fn schema_eight_delay_migrates_to_schema_nine_payload() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(MKMACROS_FILE);
        fs::write(
            &path,
            r#"{"schema_version":8,"macros":[{"id":1,"name":"delay","steps":[{"id":2,"action":{"type":"delay","data":{"milliseconds":42}}}]}]}"#,
        )
        .unwrap();

        let (document, changed) = read_document(&path).unwrap().unwrap();
        assert!(changed);
        assert_eq!(document.schema_version, SCHEMA_VERSION);
        assert_eq!(
            document.macros[0].steps[0].action,
            MkAction::Delay(crate::mkmacro::MkDelayPayload {
                fixed_ms: 42,
                maximum_ms: 42,
                ..Default::default()
            })
        );
        persist(&path, &document).unwrap();
        assert!(!read_document(&path).unwrap().unwrap().1);
    }

    #[test]
    fn schema_nine_load_adds_breakpoints_without_changing_steps_or_actions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(MKMACROS_FILE);
        let original = serde_json::json!({
            "schema_version": 9,
            "folders": [],
            "macros": [{
                "id": 1,
                "name": "debug migration",
                "description": "preserve me",
                "enabled": true,
                "hotkey": null,
                "hotkey_scope": {"type": "any_window"},
                "folder_id": null,
                "playback": {},
                "steps": [
                    {
                        "id": 11,
                        "enabled": false,
                        "repeat": 2,
                        "delay_after_ms": 3,
                        "on_error": "continue",
                        "action": {"type": "delay", "data": {
                            "mode": "fixed", "fixed_ms": 125,
                            "minimum_ms": 5, "maximum_ms": 250
                        }}
                    },
                    {
                        "id": 12,
                        "enabled": true,
                        "repeat": 4,
                        "delay_after_ms": 6,
                        "on_error": "stop",
                        "action": {"type": "mouse_move", "data": {
                            "target": {"kind": "screen", "point": {"x": -7, "y": 19}},
                            "duration_ms": 80
                        }}
                    },
                    {
                        "id": 13,
                        "enabled": true,
                        "repeat": 1,
                        "delay_after_ms": 9,
                        "on_error": "continue",
                        "action": {"type": "image_find", "data": {
                            "asset_id": 21,
                            "wait": {"timeout_ms": 500, "poll_interval_ms": 25},
                            "region": {"type": "desktop"},
                            "tolerance": 8,
                            "alpha": "ignore",
                            "return_point": "top_left",
                            "not_found_policy": "fail",
                            "outputs": {"found": "found", "point": "point", "x": "x", "y": "y"}
                        }}
                    }
                ],
                "image_assets": []
            }],
            "settings": serde_json::to_value(MkMacroSettings::default()).unwrap()
        });
        let original_steps = original["macros"][0]["steps"].as_array().unwrap().clone();
        fs::write(&path, serde_json::to_vec(&original).unwrap()).unwrap();

        let (store, disposition) = MkMacroStore::open(dir.path()).unwrap();
        assert!(matches!(disposition, LoadDisposition::Loaded));
        let loaded = store.snapshot();
        assert_eq!(loaded.schema_version, 11);
        assert!(loaded.macros[0].steps.iter().all(|step| !step.breakpoint));

        let rewritten: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(rewritten["schema_version"], 11);
        for (index, original_step) in original_steps.iter().enumerate() {
            let loaded_step = &loaded.macros[0].steps[index];
            if index == 2 {
                let MkAction::ImageFind(image) = &loaded_step.action else {
                    panic!()
                };
                assert_eq!(image.image.filename(), "missing_image_1_21.png");
            } else {
                assert_eq!(
                    serde_json::to_value(&loaded_step.action).unwrap(),
                    original_step["action"]
                );
            }
            let rewritten_step = &rewritten["macros"][0]["steps"][index];
            for property in [
                "id",
                "enabled",
                "repeat",
                "delay_after_ms",
                "on_error",
                "action",
            ] {
                if !(index == 2 && property == "action") {
                    assert_eq!(
                        rewritten_step[property], original_step[property],
                        "step {index} property {property}"
                    );
                }
            }
            assert_eq!(rewritten_step["breakpoint"], serde_json::Value::Bool(false));
        }
    }

    #[test]
    fn schema_nine_migration_preserves_existing_breakpoint_and_is_stable() {
        let mut value = serde_json::json!({
            "schema_version": 9,
            "macros": [{"steps": [
                {"breakpoint": true, "action": {"type": "delay", "data": {
                    "mode": "fixed", "fixed_ms": 1, "minimum_ms": 0, "maximum_ms": 1
                }}},
                {"action": {"type": "mouse_move", "data": {
                    "target": {"kind": "screen", "point": {"x": 1, "y": 2}},
                    "duration_ms": 3
                }}}
            ]}]
        });

        migrate_v9_to_v10(&mut value).unwrap();
        assert_eq!(value["macros"][0]["steps"][0]["breakpoint"], true);
        assert_eq!(value["macros"][0]["steps"][1]["breakpoint"], false);
        let once = value.clone();
        migrate_v9_to_v10(&mut value).unwrap();
        assert_eq!(value, once);
    }

    #[test]
    fn schema_nine_empty_macros_and_missing_collections_migrate_successfully() {
        for mut value in [
            serde_json::json!({"schema_version": 9, "macros": []}),
            serde_json::json!({"schema_version": 9}),
            serde_json::json!({"schema_version": 9, "macros": [{"name": "no steps"}]}),
        ] {
            migrate_v9_to_v10(&mut value).unwrap();
            assert_eq!(value["schema_version"], 10);
        }
    }

    #[test]
    fn schema_nine_migration_reports_context_for_malformed_entries() {
        let mut malformed_macro = serde_json::json!({"schema_version": 9, "macros": [{}, "bad"]});
        let error = migrate_v9_to_v10(&mut malformed_macro)
            .unwrap_err()
            .to_string();
        assert!(error.contains("macro 1"), "{error}");
        assert_eq!(malformed_macro["schema_version"], 9);

        let mut malformed_step = serde_json::json!({
            "schema_version": 9,
            "macros": [{"steps": [{"action": {}}, null]}]
        });
        let error = migrate_v9_to_v10(&mut malformed_step)
            .unwrap_err()
            .to_string();
        assert!(error.contains("macro 0 step 1"), "{error}");
        assert_eq!(malformed_step["schema_version"], 9);

        let mut malformed_root = serde_json::json!([]);
        let error = migrate_v9_to_v10(&mut malformed_root)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("schema 9 macro document must be an object"),
            "{error}"
        );
    }
}
