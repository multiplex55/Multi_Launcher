use super::{model::*, validation::*};
use crate::common::{
    atomic_file::save_atomic,
    json_watch::{JsonWatcher, watch_json},
};
use anyhow::{Context, Result};
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
    let value: serde_json::Value = serde_json::from_str(&text)
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
    let mut doc: MkMacroDocument = match version {
        0 | 1 => serde_json::from_value(value)
            .context("mkmacros.json does not match the macro schema")?,
        _ => unreachable!(),
    };
    let mut changed = version != SCHEMA_VERSION;
    doc.schema_version = SCHEMA_VERSION;
    changed |= repair_ids(&mut doc);
    Ok(Some((doc, changed)))
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
    fn repairs_zero_and_duplicates() {
        let mut d = document();
        d.macros.push(d.macros[0].clone());
        d.macros[0].steps.push(d.macros[0].steps[0].clone());
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
