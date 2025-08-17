use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

/// Return the configuration directory for the RSS plugin.
///
/// The directory and the `cache` sub-directory are created on first use so
/// subsequent operations can assume they exist.
pub fn ensure_config_dir() -> PathBuf {
    static DIR: Lazy<PathBuf> = Lazy::new(|| {
        let base = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."))
            .join("rss");
        let _ = fs::create_dir_all(&base);
        let _ = fs::create_dir_all(base.join("cache"));
        base
    });
    DIR.clone()
}

fn feeds_path() -> PathBuf {
    ensure_config_dir().join("feeds.json")
}

fn state_path() -> PathBuf {
    ensure_config_dir().join("state.json")
}

/// Path to the cache file for a specific feed identified by `feed_id`.
pub fn cache_path(feed_id: &str) -> PathBuf {
    ensure_config_dir()
        .join("cache")
        .join(format!("{feed_id}.json"))
}

fn atomic_write(path: &Path, data: &[u8]) -> io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir)?;
    let mut tmp = NamedTempFile::new_in(dir)?;
    tmp.write_all(data)?;
    tmp.flush()?;
    tmp.as_file().sync_all()?;
    tmp.persist(path)?;
    Ok(())
}

/// Trait implemented by versioned files stored by the RSS plugin.
pub trait HasVersion {
    const VERSION: u32;
    fn version(&self) -> u32;
}

fn load_json<T>(path: &Path) -> Option<T>
where
    T: for<'de> Deserialize<'de> + Default + HasVersion,
{
    match fs::read_to_string(path) {
        Ok(content) => match serde_json::from_str::<T>(&content) {
            Ok(data) if data.version() == T::VERSION => Some(data),
            _ => {
                let _ = fs::rename(path, path.with_extension("corrupt"));
                None
            }
        },
        Err(err) => {
            if err.kind() != io::ErrorKind::NotFound {
                let _ = fs::rename(path, path.with_extension("corrupt"));
            }
            None
        }
    }
}

fn save_json<T>(path: &Path, value: &T) -> Result<()>
where
    T: Serialize,
{
    let json = serde_json::to_vec_pretty(value)?;
    atomic_write(path, &json).context("atomic write")?;
    Ok(())
}

// ----------------------------------------------------------------------------
// feeds.json
// ----------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FeedConfig {
    pub id: String,
    pub url: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FeedsFile {
    pub version: u32,
    #[serde(default)]
    pub feeds: Vec<FeedConfig>,
}

impl FeedsFile {
    pub const VERSION: u32 = 1;

    pub fn load() -> Self {
        load_json(&feeds_path()).unwrap_or_default()
    }

    pub fn save(&self) -> Result<()> {
        save_json(&feeds_path(), self)
    }
}

impl Default for FeedsFile {
    fn default() -> Self {
        Self {
            version: Self::VERSION,
            feeds: Vec::new(),
        }
    }
}

impl HasVersion for FeedsFile {
    const VERSION: u32 = FeedsFile::VERSION;
    fn version(&self) -> u32 {
        self.version
    }
}

// ----------------------------------------------------------------------------
// state.json
// ----------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct FeedState {
    #[serde(default)]
    pub last_guid: Option<String>,
    #[serde(default)]
    pub last_fetch: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StateFile {
    pub version: u32,
    #[serde(default)]
    pub feeds: HashMap<String, FeedState>,
}

impl StateFile {
    pub const VERSION: u32 = 1;

    pub fn load() -> Self {
        load_json(&state_path()).unwrap_or_default()
    }

    pub fn save(&self) -> Result<()> {
        save_json(&state_path(), self)
    }
}

impl Default for StateFile {
    fn default() -> Self {
        Self {
            version: Self::VERSION,
            feeds: HashMap::new(),
        }
    }
}

impl HasVersion for StateFile {
    const VERSION: u32 = StateFile::VERSION;
    fn version(&self) -> u32 {
        self.version
    }
}

// ----------------------------------------------------------------------------
// per-feed caches
// ----------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct CachedItem {
    pub guid: String,
    pub title: String,
    #[serde(default)]
    pub link: Option<String>,
    #[serde(default)]
    pub timestamp: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FeedCache {
    pub version: u32,
    #[serde(default)]
    pub items: Vec<CachedItem>,
}

impl FeedCache {
    pub const VERSION: u32 = 1;

    pub fn load(feed_id: &str) -> Self {
        load_json(&cache_path(feed_id)).unwrap_or_default()
    }

    pub fn save(&self, feed_id: &str) -> Result<()> {
        save_json(&cache_path(feed_id), self)
    }
}

impl Default for FeedCache {
    fn default() -> Self {
        Self {
            version: Self::VERSION,
            items: Vec::new(),
        }
    }
}

impl HasVersion for FeedCache {
    const VERSION: u32 = FeedCache::VERSION;
    fn version(&self) -> u32 {
        self.version
    }
}
