use crate::common::config_files::{ConfigFileSpec, resolve_config_path};
use crate::common::persistence::{LoadState, PersistenceError, load_json, save_json_atomic};
use crate::settings;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

pub const LAYOUTS_FILE: &str = "layouts.json";
pub const DEFAULT_LAYOUTS_TEMPLATE: &str = r#"{
  "version": 1,
  "layouts": []
}
"#;
pub const LAYOUTS_CONFIG: ConfigFileSpec<'static> =
    ConfigFileSpec::new("layouts", LAYOUTS_FILE, DEFAULT_LAYOUTS_TEMPLATE);

static LAYOUTS_VERSION: AtomicU64 = AtomicU64::new(0);
static LAYOUTS_TRANSACTION: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum LayoutCoordMode {
    #[default]
    MonitorWorkareaRelative,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum LayoutWindowState {
    #[default]
    Normal,
    Maximized,
    Minimized,
}

impl std::fmt::Display for LayoutWindowState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LayoutWindowState::Normal => write!(f, "normal"),
            LayoutWindowState::Maximized => write!(f, "maximized"),
            LayoutWindowState::Minimized => write!(f, "minimized"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LayoutOptions {
    #[serde(default)]
    pub coord_mode: LayoutCoordMode,
    #[serde(default)]
    pub launch_missing: bool,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        Self {
            coord_mode: LayoutCoordMode::MonitorWorkareaRelative,
            launch_missing: false,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LayoutStore {
    pub version: u32,
    #[serde(default)]
    pub layouts: Vec<Layout>,
}

impl Default for LayoutStore {
    fn default() -> Self {
        Self {
            version: 1,
            layouts: Vec::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Layout {
    pub name: String,
    #[serde(default)]
    pub windows: Vec<LayoutWindow>,
    #[serde(default)]
    pub launches: Vec<LayoutLaunch>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub options: LayoutOptions,
    #[serde(default)]
    pub ignore: Vec<LayoutMatch>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LayoutWindow {
    pub matcher: LayoutMatch,
    pub placement: LayoutPlacement,
    #[serde(default)]
    pub desktop: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub launch: Option<LayoutWindowLaunch>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct LayoutMatch {
    #[serde(default)]
    pub app_id: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub class: Option<String>,
    #[serde(default)]
    pub process: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LayoutPlacement {
    /// Rect defined as normalized fractions (0-1) of the monitor work area.
    pub rect: [f32; 4],
    #[serde(default)]
    pub monitor: Option<String>,
    #[serde(default)]
    pub state: LayoutWindowState,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct LayoutWindowLaunch {
    pub kind: String,
    #[serde(default, alias = "cmd")]
    pub command: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LayoutLaunch {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: Option<String>,
}

pub fn layouts_version() -> u64 {
    LAYOUTS_VERSION.load(Ordering::SeqCst)
}

pub fn bump_layouts_version() {
    LAYOUTS_VERSION.fetch_add(1, Ordering::SeqCst);
}

pub fn layouts_config_path() -> PathBuf {
    resolve_config_path(&settings::settings_path(), &LAYOUTS_CONFIG)
}

pub fn load_layouts(path: impl AsRef<Path>) -> anyhow::Result<LayoutStore> {
    let _transaction = layouts_transaction_guard();
    load_layouts_unlocked(path.as_ref()).map_err(Into::into)
}

pub fn load_layouts_typed(
    path: impl AsRef<Path>,
) -> Result<LoadState<LayoutStore>, PersistenceError> {
    load_json(path)
}

fn load_layouts_unlocked(path: &Path) -> Result<LayoutStore, PersistenceError> {
    load_layouts_plan(path).map(|(store, _)| store)
}

fn load_layouts_plan(path: &Path) -> Result<(LayoutStore, bool), PersistenceError> {
    let mut store = match load_layouts_typed(path)? {
        LoadState::Missing | LoadState::Empty => LayoutStore::default(),
        LoadState::Loaded(store) => store,
    };
    let migrated = store.version == 0;
    if migrated {
        store.version = 1;
    }
    Ok((store, migrated))
}

pub fn save_layouts(path: impl AsRef<Path>, store: &LayoutStore) -> anyhow::Result<()> {
    replace_layouts(path, store.clone()).map(|_| ())
}

pub fn replace_layouts(
    path: impl AsRef<Path>,
    replacement: LayoutStore,
) -> anyhow::Result<LayoutStore> {
    update_layouts(path, move |store| {
        *store = replacement;
        Ok(true)
    })
}

pub fn update_layouts(
    path: impl AsRef<Path>,
    mutate: impl FnOnce(&mut LayoutStore) -> anyhow::Result<bool>,
) -> anyhow::Result<LayoutStore> {
    update_layouts_with_save(path.as_ref(), mutate, |path, store| {
        save_json_atomic(path, store).map_err(Into::into)
    })
}

fn update_layouts_with_save(
    path: &Path,
    mutate: impl FnOnce(&mut LayoutStore) -> anyhow::Result<bool>,
    save: impl FnOnce(&Path, &LayoutStore) -> anyhow::Result<()>,
) -> anyhow::Result<LayoutStore> {
    let _transaction = layouts_transaction_guard();
    let (mut store, migrated) = load_layouts_plan(path)?;
    if mutate(&mut store)? || migrated {
        if store.version == 0 {
            store.version = 1;
        }
        save(path, &store)?;
        bump_layouts_version();
    }
    Ok(store)
}

fn layouts_transaction_guard() -> MutexGuard<'static, ()> {
    LAYOUTS_TRANSACTION
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub fn get_layout<'a>(store: &'a LayoutStore, name: &str) -> Option<&'a Layout> {
    store.layouts.iter().find(|layout| layout.name == name)
}

pub fn upsert_layout(store: &mut LayoutStore, layout: Layout) {
    if let Some(existing) = store
        .layouts
        .iter_mut()
        .find(|existing| existing.name == layout.name)
    {
        *existing = layout;
    } else {
        store.layouts.push(layout);
    }
}

pub fn remove_layout(store: &mut LayoutStore, name: &str) -> bool {
    let before = store.layouts.len();
    store.layouts.retain(|layout| layout.name != name);
    before != store.layouts.len()
}

pub fn list_layouts(store: &LayoutStore) -> Vec<String> {
    store
        .layouts
        .iter()
        .map(|layout| layout.name.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};

    fn layout(name: &str) -> Layout {
        Layout {
            name: name.into(),
            windows: Vec::new(),
            launches: Vec::new(),
            created_at: None,
            notes: String::new(),
            options: LayoutOptions::default(),
            ignore: Vec::new(),
        }
    }

    #[test]
    fn typed_load_distinguishes_all_file_states() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("missing.json");
        assert!(matches!(
            load_layouts_typed(&missing).unwrap(),
            LoadState::Missing
        ));
        assert_eq!(load_layouts(&missing).unwrap(), LayoutStore::default());

        let empty = directory.path().join("empty.json");
        std::fs::write(&empty, " \r\n").unwrap();
        assert!(matches!(
            load_layouts_typed(&empty).unwrap(),
            LoadState::Empty
        ));
        assert_eq!(load_layouts(&empty).unwrap(), LayoutStore::default());

        let valid = directory.path().join("valid.json");
        save_json_atomic(&valid, &LayoutStore::default()).unwrap();
        assert!(matches!(
            load_layouts_typed(&valid).unwrap(),
            LoadState::Loaded(_)
        ));

        let malformed = directory.path().join("malformed.json");
        std::fs::write(&malformed, b"{broken").unwrap();
        assert!(matches!(
            load_layouts_typed(&malformed).unwrap_err(),
            PersistenceError::MalformedJson { .. }
        ));
        assert!(matches!(
            load_layouts_typed(directory.path()).unwrap_err(),
            PersistenceError::Read { .. }
        ));
        assert!(
            update_layouts(directory.path(), |store| {
                upsert_layout(store, layout("rejected"));
                Ok(true)
            })
            .is_err()
        );
        assert!(directory.path().is_dir());
    }

    #[test]
    fn malformed_mutation_and_failed_save_retain_bytes_and_version() {
        let directory = tempfile::tempdir().unwrap();
        let malformed = directory.path().join("malformed.json");
        let original = b"{broken";
        std::fs::write(&malformed, original).unwrap();
        assert!(
            update_layouts(&malformed, |store| {
                upsert_layout(store, layout("new"));
                Ok(true)
            })
            .is_err()
        );
        assert_eq!(std::fs::read(&malformed).unwrap(), original);

        let valid = directory.path().join("valid.json");
        save_json_atomic(&valid, &LayoutStore::default()).unwrap();
        let before = std::fs::read(&valid).unwrap();
        let version = layouts_version();
        let result = update_layouts_with_save(
            &valid,
            |store| {
                upsert_layout(store, layout("new"));
                Ok(true)
            },
            |_path, _store| anyhow::bail!("injected save failure"),
        );
        assert!(result.is_err());
        assert_eq!(std::fs::read(&valid).unwrap(), before);
        assert_eq!(layouts_version(), version);
    }

    #[test]
    fn concurrent_updates_initialize_parent_and_preserve_both_layouts() {
        let directory = tempfile::tempdir().unwrap();
        let path = Arc::new(directory.path().join("nested").join("layouts.json"));
        let barrier = Arc::new(Barrier::new(3));
        let mut threads = Vec::new();
        for name in ["left", "right"] {
            let path = Arc::clone(&path);
            let barrier = Arc::clone(&barrier);
            threads.push(std::thread::spawn(move || {
                barrier.wait();
                update_layouts(path.as_path(), |store| {
                    upsert_layout(store, layout(name));
                    Ok(true)
                })
            }));
        }
        barrier.wait();
        for thread in threads {
            thread.join().unwrap().unwrap();
        }
        let store = load_layouts(path.as_path()).unwrap();
        assert!(get_layout(&store, "left").is_some());
        assert!(get_layout(&store, "right").is_some());
        let json = std::fs::read_to_string(path.as_path()).unwrap();
        assert!(json.contains("\n  \"version\""));
    }

    #[test]
    fn version_zero_is_normalized_and_persisted_on_update() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("layouts.json");
        std::fs::write(&path, br#"{"version":0,"layouts":[]}"#).unwrap();
        let store = update_layouts(&path, |_| Ok(false)).unwrap();
        assert_eq!(store.version, 1);
        let persisted: LayoutStore = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(persisted.version, 1);
    }

    #[test]
    fn config_spec_remains_relative_to_the_settings_file() {
        let directory = tempfile::tempdir().unwrap();
        let settings_path = directory.path().join("settings.json");
        assert_eq!(
            resolve_config_path(&settings_path, &LAYOUTS_CONFIG),
            directory.path().join(LAYOUTS_FILE)
        );
    }
}
