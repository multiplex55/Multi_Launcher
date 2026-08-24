use super::{model::*, validation::*};
use crate::common::{
    atomic_file::save_atomic,
    json_watch::{JsonWatcher, watch_json},
};
use anyhow::{Context, Result};
use image::{DynamicImage, ImageFormat, RgbaImage};
use std::io::Cursor;
use std::{
    collections::HashSet,
    fs,
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex, RwLock},
};
pub const MKMACROS_FILE: &str = "mkmacros.json";
pub const ASSET_DIRECTORY: &str = "mkmacro_assets";
pub(crate) fn managed_asset_path(
    asset_root: &Path,
    macro_id: u64,
    asset_id: u64,
) -> Result<PathBuf> {
    if macro_id == 0
        || asset_id == 0
        || asset_root.file_name() != Some(std::ffi::OsStr::new(ASSET_DIRECTORY))
    {
        anyhow::bail!("invalid managed asset root or identifier")
    }
    Ok(asset_root
        .join(macro_id.to_string())
        .join(format!("{asset_id}.png")))
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
    /// Serializes fresh asset allocation with publication of the corresponding PNG.
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
    /// Enumerates the canonical PNG assets owned by one macro.
    pub fn asset_ids(&self, macro_id: u64) -> Result<Vec<u64>> {
        if macro_id == 0 {
            anyhow::bail!("macro ID must be non-zero")
        }
        let directory = self
            .inner
            .path
            .parent()
            .unwrap_or(Path::new("."))
            .join(ASSET_DIRECTORY)
            .join(macro_id.to_string());
        let mut ids = Vec::new();
        match fs::read_dir(&directory) {
            Ok(entries) => {
                for entry in entries {
                    let entry = entry?;
                    let path = entry.path();
                    if entry.file_type()?.is_file()
                        && path.extension().and_then(|x| x.to_str()) == Some("png")
                    {
                        if let Some(id) = path
                            .file_stem()
                            .and_then(|x| x.to_str())
                            .and_then(|x| x.parse::<u64>().ok())
                        {
                            if id > 0
                                && path.file_stem().and_then(|x| x.to_str())
                                    == Some(&id.to_string())
                            {
                                ids.push(id);
                            }
                        }
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(e).with_context(|| format!("inspect assets for macro {macro_id}"));
            }
        }
        ids.sort_unstable();
        Ok(ids)
    }
    /// Returns the lowest unused canonical positive numeric PNG stem.
    pub fn next_asset_id(&self, macro_id: u64) -> Result<u64> {
        if macro_id == 0 {
            anyhow::bail!("macro ID must be non-zero")
        }
        let directory = self
            .inner
            .path
            .parent()
            .unwrap_or(Path::new("."))
            .join(ASSET_DIRECTORY)
            .join(macro_id.to_string());
        let mut occupied = HashSet::new();
        match fs::read_dir(&directory) {
            Ok(entries) => {
                for entry in entries {
                    let entry = entry?;
                    if !entry.file_type()?.is_file() {
                        continue;
                    }
                    let path = entry.path();
                    if path.extension().and_then(|x| x.to_str()) != Some("png") {
                        continue;
                    }
                    let Some(stem) = path.file_stem().and_then(|x| x.to_str()) else {
                        continue;
                    };
                    let Ok(id) = stem.parse::<u64>() else {
                        continue;
                    };
                    if id > 0 && stem == id.to_string() {
                        occupied.insert(id);
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(e).with_context(|| format!("inspect assets for macro {macro_id}"));
            }
        }
        let mut id = 1u64;
        while occupied.contains(&id) {
            id = id
                .checked_add(1)
                .ok_or_else(|| anyhow::anyhow!("asset ID space exhausted"))?;
        }
        Ok(id)
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
    pub fn asset_path(&self, macro_id: u64, asset_id: u64) -> Result<PathBuf> {
        if macro_id == 0 || asset_id == 0 {
            anyhow::bail!("macro and asset IDs must be non-zero")
        };
        Ok(self
            .inner
            .path
            .parent()
            .unwrap_or(Path::new("."))
            .join(ASSET_DIRECTORY)
            .join(macro_id.to_string())
            .join(format!("{asset_id}.png")))
    }
    /// Writes a validated PNG before the caller changes the document.  The returned
    /// value is the stable, portable reference that should be persisted in JSON.
    pub fn import_png_asset(&self, macro_id: u64, asset_id: u64, source: &Path) -> Result<PathBuf> {
        let bytes = fs::read(source).with_context(|| format!("read image {}", source.display()))?;
        let image = image::load_from_memory_with_format(&bytes, ImageFormat::Png)
            .with_context(|| format!("{} is not a valid PNG image", source.display()))?
            .to_rgba8();
        self.write_png_asset(macro_id, asset_id, &image)
    }
    /// Capture flow counterpart to [`Self::import_png_asset`].
    pub fn write_png_asset(
        &self,
        macro_id: u64,
        asset_id: u64,
        image: &RgbaImage,
    ) -> Result<PathBuf> {
        if image.width() == 0 || image.height() == 0 {
            anyhow::bail!("reference image is empty")
        }
        let destination = self.asset_path(macro_id, asset_id)?;
        let mut encoded = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(image.clone()).write_to(&mut encoded, ImageFormat::Png)?;
        save_atomic(&destination, encoded.get_ref())
            .with_context(|| format!("write asset {}", destination.display()))?;
        Ok(PathBuf::from(ASSET_DIRECTORY)
            .join(macro_id.to_string())
            .join(format!("{asset_id}.png")))
    }

    /// Allocates and completely stages a fresh asset as one concurrency-safe operation.
    pub(crate) fn stage_new_png_asset(
        &self,
        macro_id: u64,
        image: &RgbaImage,
    ) -> Result<(u64, PathBuf)> {
        let _authoring = self.inner.asset_authoring.lock().unwrap();
        let asset_id = self.next_asset_id(macro_id)?;
        let reference = self.write_png_asset(macro_id, asset_id, image)?;
        Ok((asset_id, reference))
    }
    /// JSON is committed only after an asset was staged. The old path is merely
    /// returned: deletion still requires a separate, explicit `cleanup_assets` call.
    pub fn commit_asset_update(
        &self,
        doc: MkMacroDocument,
        staged_reference: &Path,
        previous_reference: Option<&Path>,
    ) -> Result<(Arc<MkMacroDocument>, Option<PathBuf>)> {
        let macro_id = staged_reference
            .components()
            .nth(1)
            .and_then(|x| x.as_os_str().to_str())
            .and_then(|x| x.parse().ok())
            .ok_or_else(|| anyhow::anyhow!("invalid staged asset reference"))?;
        let staged = self.resolve_asset_reference(macro_id, staged_reference)?;
        if !staged.is_file() {
            anyhow::bail!("staged asset does not exist: {}", staged.display())
        }
        let saved = self.save(doc)?;
        let cleanup = previous_reference
            .filter(|old| *old != staged_reference)
            .map(|old| self.resolve_asset_reference(macro_id, old))
            .transpose()?;
        Ok((saved, cleanup))
    }
    pub fn resolve_asset_reference(&self, macro_id: u64, reference: &Path) -> Result<PathBuf> {
        if reference.is_absolute()
            || reference.components().any(|c| {
                matches!(
                    c,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            anyhow::bail!("asset reference must be a contained relative path")
        };
        let expected = PathBuf::from(ASSET_DIRECTORY).join(macro_id.to_string());
        if !reference.starts_with(&expected) {
            anyhow::bail!("asset reference crosses its macro directory")
        };
        Ok(self
            .inner
            .path
            .parent()
            .unwrap_or(Path::new("."))
            .join(reference))
    }
    pub fn cleanup_assets(&self, confirmed: bool, paths: &[PathBuf]) -> Result<()> {
        if !confirmed {
            anyhow::bail!("asset deletion requires explicit confirmation after saving")
        };
        let root = self
            .inner
            .path
            .parent()
            .unwrap_or(Path::new("."))
            .join(ASSET_DIRECTORY);
        for p in paths {
            if !p.starts_with(&root) {
                anyhow::bail!("refusing to delete asset outside asset root")
            };
            if p.is_file() {
                fs::remove_file(p)?
            }
        }
        Ok(())
    }
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
    let version = value
        .get("schema_version")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    if version > SCHEMA_VERSION {
        anyhow::bail!(
            "mkmacros.json schema version {version} is newer than supported version {SCHEMA_VERSION}; update Multi Launcher before opening it"
        )
    };
    if version == 1 {
        migrate_v1_to_v2(&mut value)?;
    }
    if version <= 2 {
        migrate_v2_to_v3(&mut value)?;
    }
    if version <= 3 {
        migrate_v3_to_v4(&mut value);
    }
    if version <= 4 {
        migrate_v4_to_v5(&mut value)?;
    }
    let mut doc: MkMacroDocument = match version {
        0 | 1 | 2 | 3 | 4 | 5 => serde_json::from_value(value)
            .context("mkmacros.json does not match the macro schema")?,
        _ => unreachable!(),
    };
    let mut changed = version != SCHEMA_VERSION;
    doc.schema_version = SCHEMA_VERSION;
    changed |= repair_ids(&mut doc);
    Ok(Some((doc, changed)))
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
                    if matches!(ty, Some("if" | "while_start")) {
                        if let Some(data) = action.get_mut("data") {
                            condition(data)?;
                        }
                    } else if ty == Some("wait_until") {
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
    let mut used = HashSet::new();
    let mut next = 1u64;
    for m in &d.macros {
        if m.id > 0 {
            next = next.max(m.id.saturating_add(1))
        }
    }
    for m in &d.macros {
        for s in &m.steps {
            if s.id > 0 {
                next = next.max(s.id.saturating_add(1))
            }
        }
    }
    let mut changed = false;
    for m in &mut d.macros {
        if m.id == 0 || !used.insert(m.id) {
            while used.contains(&next) || next == 0 {
                next = next.saturating_add(1)
            }
            m.id = next;
            used.insert(next);
            next = next.saturating_add(1);
            changed = true
        }
        let mut steps = HashSet::new();
        for s in &mut m.steps {
            if s.id == 0 || !steps.insert(s.id) {
                while steps.contains(&next) || next == 0 {
                    next = next.saturating_add(1)
                }
                s.id = next;
                steps.insert(next);
                next = next.saturating_add(1);
                changed = true
            }
        }
    }
    changed
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::{AlphaPolicy, MkPoint, ReturnPoint, SearchRegion};
    use std::{sync::mpsc, thread, time::Duration};
    fn document() -> MkMacroDocument {
        MkMacroDocument {
            settings: Default::default(),
            schema_version: SCHEMA_VERSION,
            macros: vec![MkMacro {
                id: 7,
                name: "x".into(),
                description: String::new(),
                enabled: true,
                hotkey: None,
                playback: Default::default(),
                steps: vec![MkStep {
                    id: 9,
                    enabled: true,
                    repeat: 1,
                    delay_after_ms: 0,
                    on_error: Default::default(),
                    action: MkAction::Delay { milliseconds: 1 },
                }],
                image_assets: vec![],
            }],
        }
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
        let doc: MkMacroDocument = serde_json::from_value(migrated).unwrap();
        let MkAction::WaitUntil { condition, .. } = &doc.macros[0].steps[0].action else {
            panic!()
        };
        let json = serde_json::to_value(condition).unwrap();
        let first = &json["conditions"][0];
        assert_eq!(first["type"], "image_search");
        assert_eq!(first["found"], true);
        assert_eq!(
            first["search"],
            serde_json::json!({
                "asset_id":7,"region":{"type":"desktop"},"tolerance":0,
                "alpha":"compare","return_point":"center"
            })
        );
        let nested = &json["conditions"][1]["condition"]["conditions"][0];
        assert_eq!(nested["type"], "image_search");
        assert_eq!(nested["found"], false);
    }

    #[test]
    fn v5_condition_types_round_trip_with_distinct_names() {
        let conditions = vec![
            MkCondition::ImageSearch {
                search: MkImageSearchCondition {
                    asset_id: 3,
                    region: SearchRegion::Desktop,
                    tolerance: 4,
                    alpha: AlphaPolicy::Ignore,
                    return_point: ReturnPoint::TopLeft,
                },
                found: false,
            },
            MkCondition::PreviousImageResult {
                asset_id: None,
                found: true,
            },
            MkCondition::PreviousImageResult {
                asset_id: Some(3),
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
    fn old_migrates_and_future_is_rejected() {
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
        let (_, x) = MkMacroStore::open(d.path()).unwrap();
        assert!(matches!(x, LoadDisposition::NeedsUserRecovery { .. }))
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
    fn assets_are_contained() {
        let d = tempfile::tempdir().unwrap();
        let (s, _) = MkMacroStore::open(d.path()).unwrap();
        assert!(s.resolve_asset_reference(4, Path::new("../x")).is_err());
        assert!(
            s.resolve_asset_reference(4, Path::new("mkmacro_assets/5/1.png"))
                .is_err()
        );
        assert!(
            s.resolve_asset_reference(4, Path::new("mkmacro_assets/4/1.png"))
                .is_ok()
        )
    }
    #[test]
    fn next_asset_id_uses_only_canonical_positive_png_files() {
        let d = tempfile::tempdir().unwrap();
        let (s, _) = MkMacroStore::open(d.path()).unwrap();
        assert_eq!(s.next_asset_id(7).unwrap(), 1);
        let dir = d.path().join(ASSET_DIRECTORY).join("7");
        fs::create_dir_all(&dir).unwrap();
        for name in [
            "1.png",
            "3.png",
            "0.png",
            "01.png",
            "2.jpg",
            "notes.txt",
            "18446744073709551615.png",
        ] {
            fs::write(dir.join(name), b"x").unwrap();
        }
        assert_eq!(s.next_asset_id(7).unwrap(), 2);
        assert_eq!(s.next_asset_id(8).unwrap(), 1);
        assert!(s.next_asset_id(0).is_err());
    }

    #[test]
    fn asset_replacement_requires_save_then_explicit_cleanup() {
        let d = tempfile::tempdir().unwrap();
        let (s, _) = MkMacroStore::open(d.path()).unwrap();
        let old_ref = s
            .write_png_asset(
                7,
                1,
                &RgbaImage::from_pixel(1, 1, image::Rgba([1, 2, 3, 4])),
            )
            .unwrap();
        let new_ref = s
            .write_png_asset(
                7,
                2,
                &RgbaImage::from_pixel(1, 1, image::Rgba([5, 6, 7, 8])),
            )
            .unwrap();
        let old_path = s.resolve_asset_reference(7, &old_ref).unwrap();
        let new_path = s.resolve_asset_reference(7, &new_ref).unwrap();
        assert!(old_path.is_file() && new_path.is_file());

        // Cancellation/failure is represented by not committing: staging alone is non-destructive.
        assert!(s.cleanup_assets(false, &[old_path.clone()]).is_err());
        assert!(old_path.is_file());
        let (_saved, cleanup) = s
            .commit_asset_update(document(), &new_ref, Some(&old_ref))
            .unwrap();
        assert!(old_path.is_file());
        s.cleanup_assets(true, &[cleanup.unwrap()]).unwrap();
        assert!(!old_path.exists());
        assert!(new_path.is_file());
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
        assert_eq!(image.asset_id, 9);
        assert_eq!(image.wait.timeout_ms, 20);
        assert_eq!(image.region, SearchRegion::Desktop);
        assert_eq!(image.tolerance, 0);
        persist(&p, &doc).unwrap();
        assert!(!fs::read_to_string(&p).unwrap().contains("confidence"));
        assert!(!read_document(&p).unwrap().unwrap().1);
    }
}
