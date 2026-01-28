use crate::common::config_files::{resolve_config_path, ConfigFileSpec};
use crate::settings;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const LAYOUTS_FILE: &str = "layouts.json";
pub const DEFAULT_LAYOUTS_TEMPLATE: &str = r#"{
  "version": 1,
  "layouts": []
}
"#;
pub const LAYOUTS_CONFIG: ConfigFileSpec<'static> =
    ConfigFileSpec::new("layouts", LAYOUTS_FILE, DEFAULT_LAYOUTS_TEMPLATE);

static LAYOUTS_VERSION: AtomicU64 = AtomicU64::new(0);

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LayoutCoordMode {
    MonitorWorkareaRelative,
}

impl Default for LayoutCoordMode {
    fn default() -> Self {
        Self::MonitorWorkareaRelative
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LayoutWindowState {
    Normal,
    Maximized,
    Minimized,
}

impl Default for LayoutWindowState {
    fn default() -> Self {
        Self::Normal
    }
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
    let content = std::fs::read_to_string(path).unwrap_or_default();
    if content.trim().is_empty() {
        return Ok(LayoutStore::default());
    }
    let mut store: LayoutStore = serde_json::from_str(&content)?;
    if store.version == 0 {
        store.version = 1;
    }
    Ok(store)
}

pub fn save_layouts(path: impl AsRef<Path>, store: &LayoutStore) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(store)?;
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, json)?;
    bump_layouts_version();
    Ok(())
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
