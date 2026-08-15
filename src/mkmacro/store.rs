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
    sync::{Arc, RwLock},
};
pub const MKMACROS_FILE: &str = "mkmacros.json";
pub const ASSET_DIRECTORY: &str = "mkmacro_assets";
#[derive(Debug)]
pub enum LoadDisposition {
    Missing,
    Empty,
    Loaded,
    NeedsUserRecovery { error: String },
}
struct Inner {
    path: PathBuf,
    snapshot: RwLock<Arc<MkMacroDocument>>,
    diagnostics: RwLock<Arc<[MkDiagnostic]>>,
    last_external_error: RwLock<Option<String>>,
}
pub struct MkMacroStore {
    inner: Arc<Inner>,
    _watcher: Option<JsonWatcher>,
}
impl MkMacroStore {
    pub fn open(directory: impl AsRef<Path>) -> Result<(Self, LoadDisposition)> {
        let path = directory.as_ref().join(MKMACROS_FILE);
        let inner = Arc::new(Inner {
            path: path.clone(),
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
                match read_document(&i.path) {
                    Ok(Some((d, changed))) => {
                        if !changed || persist(&i.path, &d).is_ok() {
                            publish(&i, d);
                            *i.last_external_error.write().unwrap() = None
                        }
                    }
                    Ok(None) => {}
                    Err(e) => *i.last_external_error.write().unwrap() = Some(e.to_string()),
                }
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
        doc.schema_version = SCHEMA_VERSION;
        repair_ids(&mut doc);
        persist(&self.inner.path, &doc)?;
        publish(&self.inner, doc);
        Ok(self.snapshot())
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
    let mut doc: MkMacroDocument = match version {
        0 | 1 | 2 => serde_json::from_value(value)
            .context("mkmacros.json does not match the macro schema")?,
        _ => unreachable!(),
    };
    let mut changed = version != SCHEMA_VERSION;
    doc.schema_version = SCHEMA_VERSION;
    changed |= repair_ids(&mut doc);
    Ok(Some((doc, changed)))
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
    use crate::mkmacro::MkPoint;
    fn document() -> MkMacroDocument {
        MkMacroDocument {
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
            }],
        }
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
        assert_eq!(doc.schema_version, 2);
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
}
