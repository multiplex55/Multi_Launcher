use crate::common::json_watch::{watch_json, JsonWatcher};
use crate::settings::Settings;
use anyhow::{bail, Result};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, RwLock,
};

pub const WATCHLIST_FILE: &str = "watchlist.json";

static WATCHLIST_VERSION: AtomicU64 = AtomicU64::new(0);

pub static WATCHLIST_DATA: Lazy<Arc<RwLock<WatchlistConfig>>> =
    Lazy::new(|| Arc::new(RwLock::new(WatchlistConfig::default())));

fn default_watchlist_version() -> u32 {
    1
}

fn default_refresh_ms() -> u64 {
    2000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchlistConfig {
    #[serde(default = "default_watchlist_version")]
    pub version: u32,
    #[serde(default = "default_refresh_ms")]
    pub refresh_ms: u64,
    #[serde(default)]
    pub items: Vec<WatchItemConfig>,
}

impl Default for WatchlistConfig {
    fn default() -> Self {
        Self {
            version: default_watchlist_version(),
            refresh_ms: default_refresh_ms(),
            items: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchItemKind {
    DirCount,
    DirSize,
    LatestFile,
    FileExists,
    FileAge,
    FileRegexCount,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchFilterConfig {
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub recursive: bool,
    #[serde(default)]
    pub glob: Option<String>,
}

impl Default for WatchFilterConfig {
    fn default() -> Self {
        Self {
            extensions: Vec::new(),
            recursive: false,
            glob: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchItemConfig {
    pub id: String,
    #[serde(default)]
    pub label: Option<String>,
    pub kind: WatchItemKind,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub filter: WatchFilterConfig,
    #[serde(default)]
    pub regex: Option<String>,
    #[serde(default)]
    pub display: Option<serde_json::Value>,
    #[serde(default)]
    pub thresholds: Option<serde_json::Value>,
}

pub fn resolve_watchlist_path(settings: &Settings, settings_path: &str) -> PathBuf {
    if let Some(path) = &settings.watchlist_path {
        return PathBuf::from(path);
    }
    let base = Path::new(settings_path);
    if base.is_dir() {
        base.join(WATCHLIST_FILE)
    } else {
        base.parent()
            .unwrap_or_else(|| Path::new("."))
            .join(WATCHLIST_FILE)
    }
}

pub fn load_watchlist(path: &str) -> Result<WatchlistConfig> {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    if content.trim().is_empty() {
        return Ok(WatchlistConfig::default());
    }
    let cfg: WatchlistConfig = serde_json::from_str(&content)?;
    validate_watchlist(&cfg)?;
    Ok(cfg)
}

pub fn refresh_watchlist_cache(path: &str) -> Result<WatchlistConfig> {
    let cfg = load_watchlist(path)?;
    update_watchlist_cache(cfg.clone());
    Ok(cfg)
}

pub fn watchlist_version() -> u64 {
    WATCHLIST_VERSION.load(Ordering::SeqCst)
}

pub fn start_watchlist_watcher(settings: &Settings, settings_path: &str) -> Option<JsonWatcher> {
    let path = resolve_watchlist_path(settings, settings_path);
    let path_string = path.to_string_lossy().to_string();
    if let Err(err) = refresh_watchlist_cache(&path_string) {
        tracing::warn!("failed to load watchlist config: {err}");
    }
    let watch_path = path_string.clone();
    watch_json(&watch_path, {
        let watch_path = path_string.clone();
        move || {
            match load_watchlist(&watch_path) {
                Ok(cfg) => update_watchlist_cache(cfg),
                Err(err) => tracing::warn!("failed to reload watchlist config: {err}"),
            }
        }
    })
    .ok()
}

fn update_watchlist_cache(cfg: WatchlistConfig) {
    if let Ok(mut lock) = WATCHLIST_DATA.write() {
        *lock = cfg;
    }
    bump_watchlist_version();
}

fn bump_watchlist_version() {
    WATCHLIST_VERSION.fetch_add(1, Ordering::SeqCst);
}

fn validate_watchlist(cfg: &WatchlistConfig) -> Result<()> {
    let mut ids = HashSet::new();
    for item in &cfg.items {
        let id = item.id.trim();
        if id.is_empty() {
            bail!("watchlist item id cannot be empty");
        }
        let id_key = id.to_ascii_lowercase();
        if !ids.insert(id_key) {
            bail!("duplicate watchlist item id '{id}'");
        }
        let path = item.path.as_deref().unwrap_or("").trim();
        if path.is_empty() {
            bail!("watchlist item '{id}' requires a path");
        }
        if matches!(item.kind, WatchItemKind::FileRegexCount) {
            let regex = item.regex.as_deref().unwrap_or("").trim();
            if regex.is_empty() {
                bail!("watchlist item '{id}' requires a regex");
            }
        }
    }
    Ok(())
}
