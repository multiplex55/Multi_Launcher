mod evaluator;

use crate::common::json_watch::{watch_json, JsonWatcher};
use crate::settings::Settings;
use anyhow::{bail, Result};
use chrono::{DateTime, Local};
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, RwLock,
};
use std::time::{Duration, Instant, SystemTime};

pub const WATCHLIST_FILE: &str = "watchlist.json";

static WATCHLIST_VERSION: AtomicU64 = AtomicU64::new(0);

pub static WATCHLIST_DATA: Lazy<Arc<RwLock<WatchlistConfig>>> =
    Lazy::new(|| Arc::new(RwLock::new(WatchlistConfig::default())));
pub static WATCHLIST_PATH: Lazy<RwLock<PathBuf>> =
    Lazy::new(|| RwLock::new(PathBuf::from(WATCHLIST_FILE)));

pub fn watchlist_refresh_ms() -> u64 {
    WATCHLIST_DATA
        .read()
        .map(|cfg| cfg.refresh_ms)
        .unwrap_or_else(|_| default_refresh_ms())
}

pub fn watchlist_path() -> PathBuf {
    WATCHLIST_PATH
        .read()
        .map(|path| path.clone())
        .unwrap_or_else(|_| PathBuf::from(WATCHLIST_FILE))
}

pub fn watchlist_path_string() -> String {
    watchlist_path().to_string_lossy().to_string()
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchStatus {
    Ok,
    Warn,
    Critical,
}

#[derive(Debug, Clone)]
pub enum WatchRawValue {
    Count(u64),
    Bytes(u64),
    Bool(bool),
    Duration(Duration),
    Timestamp(SystemTime),
}

#[derive(Debug, Clone)]
pub struct WatchItemSnapshot {
    pub id: String,
    pub label: String,
    pub value_text: String,
    pub delta_text: Option<String>,
    pub status: WatchStatus,
    pub last_updated: DateTime<Local>,
    pub raw_value: Option<WatchRawValue>,
    pub previous_raw_value: Option<WatchRawValue>,
}

#[derive(Debug)]
pub struct WatchlistState {
    snapshot: Arc<Vec<WatchItemSnapshot>>,
    dirty: Arc<AtomicBool>,
    last_refresh: Instant,
    watchers: Vec<RecommendedWatcher>,
    previous_values: HashMap<String, WatchRawValue>,
    config_version: u64,
}

impl WatchlistState {
    pub fn new() -> Self {
        let dirty = Arc::new(AtomicBool::new(true));
        let items = WATCHLIST_DATA
            .read()
            .map(|cfg| cfg.items.clone())
            .unwrap_or_default();
        let watchers = build_watchers(&items, Arc::clone(&dirty));
        Self {
            snapshot: Arc::new(Vec::new()),
            dirty,
            last_refresh: Instant::now() - Duration::from_secs(3600),
            watchers,
            previous_values: HashMap::new(),
            config_version: watchlist_version(),
        }
    }

    pub fn snapshot(&self) -> Arc<Vec<WatchItemSnapshot>> {
        Arc::clone(&self.snapshot)
    }

    pub fn maybe_refresh(&mut self, refresh_ms: u64) {
        let current_version = watchlist_version();
        if current_version != self.config_version {
            self.config_version = current_version;
            self.watchers = build_watchers(&self.load_items(), Arc::clone(&self.dirty));
            self.dirty.store(true, Ordering::SeqCst);
        }

        let should_refresh = self.dirty.load(Ordering::SeqCst)
            || self.last_refresh.elapsed() >= Duration::from_millis(refresh_ms);
        if should_refresh {
            self.refresh_now();
        }
    }

    fn refresh_now(&mut self) {
        let now = Local::now();
        let items = self.load_items();
        let mut snapshot = Vec::with_capacity(items.len());
        for item in items {
            let label = item.label.clone().unwrap_or_else(|| item.id.clone());
            let prev_raw = self.previous_values.get(&item.id).cloned();
            let result = evaluator::evaluate_item(&item);
            let (value_text, raw_value, status) = match result {
                Ok(value) => {
                    let status = evaluate_status(&item, value.raw_value.as_ref());
                    (value.value_text, value.raw_value, status)
                }
                Err(err) => (
                    format!("Error: {err}"),
                    None,
                    WatchStatus::Critical,
                ),
            };
            let delta_text = match (raw_value.as_ref(), prev_raw.as_ref()) {
                (Some(current), Some(previous)) => compute_delta_text(current, previous),
                _ => None,
            };
            if let Some(raw) = raw_value.clone() {
                self.previous_values.insert(item.id.clone(), raw);
            }
            snapshot.push(WatchItemSnapshot {
                id: item.id,
                label,
                value_text,
                delta_text,
                status,
                last_updated: now,
                raw_value,
                previous_raw_value: prev_raw,
            });
        }
        self.snapshot = Arc::new(snapshot);
        self.last_refresh = Instant::now();
        self.dirty.store(false, Ordering::SeqCst);
    }

    fn load_items(&self) -> Vec<WatchItemConfig> {
        WATCHLIST_DATA
            .read()
            .map(|cfg| cfg.items.clone())
            .unwrap_or_default()
    }
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

pub fn normalize_watchlist_config(cfg: &mut WatchlistConfig) {
    for item in &mut cfg.items {
        item.id = item.id.trim().to_string();
        if let Some(path) = item.path.as_ref() {
            let normalized = normalize_watchlist_path(path);
            if normalized.is_empty() {
                item.path = None;
            } else {
                item.path = Some(normalized);
            }
        }
        normalize_extensions(&mut item.filter.extensions);
    }
}

pub fn init_watchlist(path: &str, force: bool) -> Result<()> {
    let path_buf = Path::new(path);
    if path_buf.exists() && !force {
        let content = std::fs::read_to_string(path)?;
        if content.trim().is_empty() {
            bail!("watchlist file is empty; run watch init --force to overwrite");
        }
        if let Err(err) = load_watchlist(path) {
            bail!("watchlist file is invalid: {err}; run watch init --force to overwrite");
        }
        bail!("watchlist file already exists");
    }

    if let Some(parent) = path_buf.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let mut cfg = WatchlistConfig::default();
    write_watchlist_config(path, &mut cfg)?;
    let _ = refresh_watchlist_cache(path);
    Ok(())
}

pub fn load_watchlist(path: &str) -> Result<WatchlistConfig> {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    if content.trim().is_empty() {
        return Ok(WatchlistConfig::default());
    }
    let mut cfg: WatchlistConfig = serde_json::from_str(&content)?;
    normalize_watchlist_config(&mut cfg);
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

fn write_watchlist_config(path: &str, cfg: &mut WatchlistConfig) -> Result<()> {
    normalize_watchlist_config(cfg);
    validate_watchlist(cfg)?;
    let json = serde_json::to_string_pretty(cfg)?;
    std::fs::write(path, json)?;
    Ok(())
}

pub fn start_watchlist_watcher(settings: &Settings, settings_path: &str) -> Option<JsonWatcher> {
    let path = resolve_watchlist_path(settings, settings_path);
    if let Ok(mut lock) = WATCHLIST_PATH.write() {
        *lock = path.clone();
    }
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
        if id.chars().any(char::is_whitespace) {
            bail!("watchlist item id '{id}' cannot contain whitespace");
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

fn normalize_extensions(extensions: &mut Vec<String>) {
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    for ext in extensions.iter() {
        let ext = ext.trim().trim_start_matches('.').to_ascii_lowercase();
        if ext.is_empty() {
            continue;
        }
        if seen.insert(ext.clone()) {
            normalized.push(ext);
        }
    }
    *extensions = normalized;
}

fn normalize_watchlist_path(path: &str) -> String {
    let trimmed = path.trim();
    let stripped = strip_wrapping_quotes(trimmed).trim();
    expand_env_vars(stripped)
}

fn strip_wrapping_quotes(value: &str) -> &str {
    let bytes = value.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return &value[1..value.len() - 1];
        }
    }
    value
}

fn expand_env_vars(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars();
    while let Some(ch) = chars.next() {
        if ch == '%' {
            let mut name = String::new();
            let mut found = false;
            while let Some(next) = chars.next() {
                if next == '%' {
                    found = true;
                    break;
                }
                name.push(next);
            }
            if found {
                if name.is_empty() {
                    out.push('%');
                    out.push('%');
                } else if let Ok(value) = std::env::var(&name) {
                    out.push_str(&value);
                } else {
                    out.push('%');
                    out.push_str(&name);
                    out.push('%');
                }
            } else {
                out.push('%');
                out.push_str(&name);
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn build_watchers(
    items: &[WatchItemConfig],
    dirty: Arc<AtomicBool>,
) -> Vec<RecommendedWatcher> {
    let mut watchers = Vec::new();
    for item in items {
        let Some(path) = item.path.as_deref() else {
            continue;
        };
        let path = PathBuf::from(path);
        if let Some(watcher) = watch_path(&path, Arc::clone(&dirty)) {
            watchers.push(watcher);
        }
    }
    watchers
}

fn watch_path(path: &Path, dirty: Arc<AtomicBool>) -> Option<RecommendedWatcher> {
    let mut watcher = RecommendedWatcher::new(
        move |res: notify::Result<notify::Event>| match res {
            Ok(ev) => {
                if matches!(
                    ev.kind,
                    EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                ) {
                    dirty.store(true, Ordering::SeqCst);
                }
            }
            Err(err) => tracing::error!("watchlist watch error: {err:?}"),
        },
        Config::default(),
    )
    .ok()?;

    if watcher
        .watch(path, RecursiveMode::NonRecursive)
        .is_err()
    {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        if watcher
            .watch(parent, RecursiveMode::NonRecursive)
            .is_err()
        {
            return None;
        }
    }
    Some(watcher)
}

fn evaluate_status(item: &WatchItemConfig, raw: Option<&WatchRawValue>) -> WatchStatus {
    let Some(thresholds) = item.thresholds.as_ref() else {
        return WatchStatus::Ok;
    };
    let Some(raw) = raw else {
        return WatchStatus::Ok;
    };
    match item.kind {
        WatchItemKind::DirCount | WatchItemKind::DirSize | WatchItemKind::FileRegexCount => {
            let value = match raw {
                WatchRawValue::Count(v) | WatchRawValue::Bytes(v) => *v as f64,
                _ => return WatchStatus::Ok,
            };
            numeric_thresholds(value, thresholds)
        }
        WatchItemKind::FileExists => match raw {
            WatchRawValue::Bool(value) => bool_thresholds(*value, thresholds),
            _ => WatchStatus::Ok,
        },
        WatchItemKind::FileAge => match raw {
            WatchRawValue::Duration(duration) => age_thresholds(*duration, thresholds),
            _ => WatchStatus::Ok,
        },
        WatchItemKind::LatestFile => match raw {
            WatchRawValue::Timestamp(ts) => {
                let age = SystemTime::now()
                    .duration_since(*ts)
                    .unwrap_or_else(|_| Duration::ZERO);
                age_thresholds(age, thresholds)
            }
            _ => WatchStatus::Ok,
        },
    }
}

fn numeric_thresholds(value: f64, thresholds: &serde_json::Value) -> WatchStatus {
    let critical_gt = threshold_number(thresholds, "critical_if_gt");
    let critical_lt = threshold_number(thresholds, "critical_if_lt");
    let warn_gt = threshold_number(thresholds, "warn_if_gt");
    let warn_lt = threshold_number(thresholds, "warn_if_lt");

    if critical_gt.map(|t| value > t).unwrap_or(false)
        || critical_lt.map(|t| value < t).unwrap_or(false)
    {
        return WatchStatus::Critical;
    }
    if warn_gt.map(|t| value > t).unwrap_or(false)
        || warn_lt.map(|t| value < t).unwrap_or(false)
    {
        return WatchStatus::Warn;
    }
    WatchStatus::Ok
}

fn age_thresholds(duration: Duration, thresholds: &serde_json::Value) -> WatchStatus {
    let minutes = duration.as_secs_f64() / 60.0;
    let critical_lt = threshold_number(thresholds, "critical_if_age_lt_minutes");
    let critical_gt = threshold_number(thresholds, "critical_if_age_gt_minutes");
    let warn_lt = threshold_number(thresholds, "warn_if_age_lt_minutes");
    let warn_gt = threshold_number(thresholds, "warn_if_age_gt_minutes");

    if critical_lt.map(|t| minutes < t).unwrap_or(false)
        || critical_gt.map(|t| minutes > t).unwrap_or(false)
    {
        return WatchStatus::Critical;
    }
    if warn_lt.map(|t| minutes < t).unwrap_or(false)
        || warn_gt.map(|t| minutes > t).unwrap_or(false)
    {
        return WatchStatus::Warn;
    }
    WatchStatus::Ok
}

fn bool_thresholds(value: bool, thresholds: &serde_json::Value) -> WatchStatus {
    let critical_true = threshold_bool(thresholds, "critical_if_true");
    let critical_false = threshold_bool(thresholds, "critical_if_false");
    let warn_true = threshold_bool(thresholds, "warn_if_true");
    let warn_false = threshold_bool(thresholds, "warn_if_false");

    if critical_true == Some(true) && value {
        return WatchStatus::Critical;
    }
    if critical_false == Some(true) && !value {
        return WatchStatus::Critical;
    }
    if warn_true == Some(true) && value {
        return WatchStatus::Warn;
    }
    if warn_false == Some(true) && !value {
        return WatchStatus::Warn;
    }
    WatchStatus::Ok
}

fn threshold_number(thresholds: &serde_json::Value, key: &str) -> Option<f64> {
    thresholds.get(key)?.as_f64()
}

fn threshold_bool(thresholds: &serde_json::Value, key: &str) -> Option<bool> {
    thresholds.get(key)?.as_bool()
}

fn compute_delta_text(current: &WatchRawValue, previous: &WatchRawValue) -> Option<String> {
    match (current, previous) {
        (WatchRawValue::Count(cur), WatchRawValue::Count(prev)) => {
            format_delta_i64(*cur as i64 - *prev as i64, |v| v.to_string())
        }
        (WatchRawValue::Bytes(cur), WatchRawValue::Bytes(prev)) => {
            format_delta_i64(*cur as i64 - *prev as i64, format_delta_bytes)
        }
        (WatchRawValue::Bool(cur), WatchRawValue::Bool(prev)) => {
            if cur != prev {
                Some(format!("{}→{}", prev, cur))
            } else {
                None
            }
        }
        (WatchRawValue::Duration(cur), WatchRawValue::Duration(prev)) => {
            let delta = cur.as_secs_f64() - prev.as_secs_f64();
            format_delta_i64(delta.round() as i64, format_delta_duration)
        }
        (WatchRawValue::Timestamp(cur), WatchRawValue::Timestamp(prev)) => {
            let delta = cur
                .duration_since(*prev)
                .map(|d| d.as_secs_f64())
                .unwrap_or_else(|e| -(e.duration().as_secs_f64()));
            format_delta_i64(delta.round() as i64, format_delta_duration)
        }
        _ => None,
    }
}

fn format_delta_i64<F>(delta: i64, formatter: F) -> Option<String>
where
    F: Fn(u64) -> String,
{
    if delta == 0 {
        return None;
    }
    let sign = if delta > 0 { "+" } else { "-" };
    Some(format!("{sign}{}", formatter(delta.unsigned_abs())))
}

fn format_delta_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.2} GB", bytes as f64 / 1024.0 / 1024.0 / 1024.0)
    } else if bytes >= 1024 * 1024 {
        format!("{:.2} MB", bytes as f64 / 1024.0 / 1024.0)
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

fn format_delta_duration(secs: u64) -> String {
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86_400 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}d", secs / 86_400)
    }
}
