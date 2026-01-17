use super::{
    load_watchlist, normalize_extensions, refresh_watchlist_cache, write_watchlist_config,
    WatchFilterConfig, WatchItemConfig, WatchItemKind, WatchlistConfig,
};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WatchThresholdOverrides {
    pub warn_if_gt: Option<f64>,
    pub critical_if_gt: Option<f64>,
    pub warn_if_age_lt_minutes: Option<f64>,
    pub warn_if_age_gt_minutes: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchAddRequest {
    pub kind: WatchItemKind,
    pub path: String,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub recursive: bool,
    #[serde(default)]
    pub glob: Option<String>,
    #[serde(default)]
    pub regex: Option<String>,
    #[serde(default)]
    pub thresholds: WatchThresholdOverrides,
}

#[derive(Debug, Clone, Copy)]
pub enum MoveDirection {
    Up,
    Down,
}

pub fn parse_watch_add_input(input: &str) -> Result<WatchAddRequest, String> {
    let mut parts = input.split_whitespace();
    let kind_str = parts.next().ok_or("missing watch kind")?;
    let kind =
        parse_watch_kind(kind_str).ok_or_else(|| format!("unknown watch kind '{kind_str}'"))?;
    let path = parts.next().ok_or("missing watch path")?.trim().to_string();
    if path.is_empty() {
        return Err("missing watch path".into());
    }
    let mut req = WatchAddRequest {
        kind,
        path,
        id: None,
        label: None,
        extensions: Vec::new(),
        recursive: false,
        glob: None,
        regex: None,
        thresholds: WatchThresholdOverrides::default(),
    };
    while let Some(flag) = parts.next() {
        match flag {
            "--id" => {
                let value = parts.next().ok_or("missing value for --id")?;
                if !value.trim().is_empty() {
                    req.id = Some(value.trim().to_string());
                }
            }
            "--label" => {
                let value = parts.next().ok_or("missing value for --label")?;
                if !value.trim().is_empty() {
                    req.label = Some(value.trim().to_string());
                }
            }
            "--ext" => {
                let value = parts.next().ok_or("missing value for --ext")?;
                let raw = value.trim();
                if !raw.is_empty() {
                    req.extensions
                        .extend(raw.split(',').map(|ext| ext.trim().to_string()));
                }
            }
            "--recursive" => {
                req.recursive = true;
            }
            "--glob" => {
                let value = parts.next().ok_or("missing value for --glob")?;
                if !value.trim().is_empty() {
                    req.glob = Some(value.trim().to_string());
                }
            }
            "--regex" => {
                let value = parts.next().ok_or("missing value for --regex")?;
                if !value.trim().is_empty() {
                    req.regex = Some(value.trim().to_string());
                }
            }
            "--warn-gt" => {
                let value = parts.next().ok_or("missing value for --warn-gt")?;
                req.thresholds.warn_if_gt = parse_threshold(value, "--warn-gt")?;
            }
            "--critical-gt" => {
                let value = parts.next().ok_or("missing value for --critical-gt")?;
                req.thresholds.critical_if_gt = parse_threshold(value, "--critical-gt")?;
            }
            "--warn-age-lt-min" => {
                let value = parts.next().ok_or("missing value for --warn-age-lt-min")?;
                req.thresholds.warn_if_age_lt_minutes =
                    parse_threshold(value, "--warn-age-lt-min")?;
            }
            "--warn-age-gt-min" => {
                let value = parts.next().ok_or("missing value for --warn-age-gt-min")?;
                req.thresholds.warn_if_age_gt_minutes =
                    parse_threshold(value, "--warn-age-gt-min")?;
            }
            unknown => {
                return Err(format!("unknown flag '{unknown}'"));
            }
        }
    }
    Ok(req)
}

pub fn preview_watch_add_item(
    cfg: &WatchlistConfig,
    req: &WatchAddRequest,
) -> Result<WatchItemConfig> {
    build_watch_item(cfg, req)
}

pub fn apply_watch_add_payload(path: &str, payload: &str) -> Result<WatchItemConfig> {
    let req: WatchAddRequest = serde_json::from_str(payload)?;
    update_watchlist_config(path, |cfg| {
        let item = build_watch_item(cfg, &req)?;
        cfg.items.push(item.clone());
        Ok(item)
    })
}

pub fn remove_watch_item(path: &str, id: &str) -> Result<()> {
    update_watchlist_config(path, |cfg| {
        let Some(idx) = find_item_index(cfg, id) else {
            bail!("watchlist item '{id}' not found");
        };
        cfg.items.remove(idx);
        Ok(())
    })?;
    Ok(())
}

pub fn set_watch_item_enabled(path: &str, id: &str, enabled: bool) -> Result<()> {
    update_watchlist_config(path, |cfg| {
        let Some(item) = find_item_mut(cfg, id) else {
            bail!("watchlist item '{id}' not found");
        };
        item.enabled = enabled;
        Ok(())
    })?;
    Ok(())
}

pub fn set_watchlist_refresh_ms(path: &str, refresh_ms: u64) -> Result<()> {
    update_watchlist_config(path, |cfg| {
        cfg.refresh_ms = refresh_ms;
        Ok(())
    })?;
    Ok(())
}

pub fn move_watch_item(path: &str, id: &str, direction: MoveDirection) -> Result<()> {
    update_watchlist_config(path, |cfg| {
        let Some(idx) = find_item_index(cfg, id) else {
            bail!("watchlist item '{id}' not found");
        };
        match direction {
            MoveDirection::Up => {
                if idx == 0 {
                    bail!("watchlist item '{id}' is already at the top");
                }
                cfg.items.swap(idx, idx - 1);
            }
            MoveDirection::Down => {
                if idx + 1 >= cfg.items.len() {
                    bail!("watchlist item '{id}' is already at the bottom");
                }
                cfg.items.swap(idx, idx + 1);
            }
        }
        Ok(())
    })?;
    Ok(())
}

pub fn parse_move_direction(input: &str) -> Option<MoveDirection> {
    match input.to_ascii_lowercase().as_str() {
        "up" => Some(MoveDirection::Up),
        "down" => Some(MoveDirection::Down),
        _ => None,
    }
}

pub fn parse_watch_kind(input: &str) -> Option<WatchItemKind> {
    match input.to_ascii_lowercase().as_str() {
        "dir_count" => Some(WatchItemKind::DirCount),
        "dir_size" => Some(WatchItemKind::DirSize),
        "latest_file" => Some(WatchItemKind::LatestFile),
        "file_exists" => Some(WatchItemKind::FileExists),
        "file_age" => Some(WatchItemKind::FileAge),
        "file_regex_count" => Some(WatchItemKind::FileRegexCount),
        _ => None,
    }
}

fn parse_threshold(value: &str, flag: &str) -> Result<Option<f64>, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("missing value for {flag}"));
    }
    let parsed = trimmed
        .parse::<f64>()
        .map_err(|_| format!("invalid numeric value '{trimmed}' for {flag}"))?;
    Ok(Some(parsed))
}

fn kind_to_string(kind: &WatchItemKind) -> &'static str {
    match kind {
        WatchItemKind::DirCount => "dir_count",
        WatchItemKind::DirSize => "dir_size",
        WatchItemKind::LatestFile => "latest_file",
        WatchItemKind::FileExists => "file_exists",
        WatchItemKind::FileAge => "file_age",
        WatchItemKind::FileRegexCount => "file_regex_count",
    }
}

fn build_watch_item(cfg: &WatchlistConfig, req: &WatchAddRequest) -> Result<WatchItemConfig> {
    let mut extensions = req.extensions.clone();
    normalize_extensions(&mut extensions);
    let id = match req
        .id
        .as_ref()
        .map(|id| id.trim())
        .filter(|id| !id.is_empty())
    {
        Some(id) => id.to_string(),
        None => generate_watch_id(cfg, req),
    };
    let label = match req
        .label
        .as_ref()
        .map(|label| label.trim())
        .filter(|label| !label.is_empty())
    {
        Some(label) => Some(label.to_string()),
        None => {
            let label = default_label(&req.path);
            if label.is_empty() {
                None
            } else {
                Some(label)
            }
        }
    };
    if matches!(req.kind, WatchItemKind::FileRegexCount)
        && req.regex.as_deref().unwrap_or("").trim().is_empty()
    {
        bail!("file_regex_count requires --regex");
    }

    let thresholds = build_thresholds(&req.thresholds);
    let item = WatchItemConfig {
        id,
        label,
        kind: req.kind.clone(),
        enabled: true,
        path: Some(req.path.clone()),
        filter: WatchFilterConfig {
            extensions,
            recursive: req.recursive,
            glob: req.glob.clone(),
        },
        regex: req.regex.clone(),
        display: None,
        thresholds,
    };
    Ok(item)
}

fn build_thresholds(thresholds: &WatchThresholdOverrides) -> Option<serde_json::Value> {
    let mut map = serde_json::Map::new();
    if let Some(value) = thresholds.warn_if_gt {
        map.insert("warn_if_gt".into(), value.into());
    }
    if let Some(value) = thresholds.critical_if_gt {
        map.insert("critical_if_gt".into(), value.into());
    }
    if let Some(value) = thresholds.warn_if_age_lt_minutes {
        map.insert("warn_if_age_lt_minutes".into(), value.into());
    }
    if let Some(value) = thresholds.warn_if_age_gt_minutes {
        map.insert("warn_if_age_gt_minutes".into(), value.into());
    }
    if map.is_empty() {
        None
    } else {
        Some(serde_json::Value::Object(map))
    }
}

fn generate_watch_id(cfg: &WatchlistConfig, req: &WatchAddRequest) -> String {
    let kind = kind_to_string(&req.kind);
    let basename = basename(&req.path);
    let normalized = normalize_id(&basename);
    let mut candidate = format!("{kind}_{normalized}");
    let existing: std::collections::HashSet<String> = cfg
        .items
        .iter()
        .map(|item| item.id.to_ascii_lowercase())
        .collect();
    if !existing.contains(&candidate.to_ascii_lowercase()) {
        return candidate;
    }
    let mut suffix = 2;
    loop {
        let next = format!("{candidate}_{suffix}");
        if !existing.contains(&next.to_ascii_lowercase()) {
            return next;
        }
        suffix += 1;
    }
}

fn normalize_id(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return "item".into();
    }
    trimmed
        .chars()
        .map(|ch| if ch.is_whitespace() { '_' } else { ch })
        .collect::<String>()
        .to_ascii_lowercase()
}

fn default_label(path: &str) -> String {
    let base = basename(path);
    let label = base.trim();
    if label.is_empty() {
        return String::new();
    }
    label
        .chars()
        .map(|ch| if ch == '_' || ch == '-' { ' ' } else { ch })
        .collect::<String>()
        .trim()
        .to_string()
}

fn basename(path: &str) -> String {
    let path = Path::new(path);
    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
        if !stem.trim().is_empty() {
            return stem.to_string();
        }
    }
    path.file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(path.to_string_lossy().as_ref())
        .to_string()
}

fn find_item_index(cfg: &WatchlistConfig, id: &str) -> Option<usize> {
    cfg.items
        .iter()
        .position(|item| item.id.eq_ignore_ascii_case(id))
}

fn find_item_mut<'a>(cfg: &'a mut WatchlistConfig, id: &str) -> Option<&'a mut WatchItemConfig> {
    cfg.items
        .iter_mut()
        .find(|item| item.id.eq_ignore_ascii_case(id))
}

fn update_watchlist_config<T>(
    path: &str,
    mutator: impl FnOnce(&mut WatchlistConfig) -> Result<T>,
) -> Result<T> {
    let mut cfg = load_watchlist(path)?;
    let result = mutator(&mut cfg)?;
    write_watchlist_config(path, &mut cfg)?;
    let _ = refresh_watchlist_cache(path);
    Ok(result)
}
