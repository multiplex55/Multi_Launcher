use crate::watchlist::{WatchFilterConfig, WatchItemConfig, WatchItemKind, WatchRawValue};
use anyhow::{bail, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use regex::Regex;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct WatchItemValue {
    pub raw_value: Option<WatchRawValue>,
    pub value_text: String,
}

pub fn evaluate_item(item: &WatchItemConfig) -> Result<WatchItemValue> {
    let path = item
        .path
        .as_ref()
        .map(|p| PathBuf::from(p))
        .ok_or_else(|| anyhow::anyhow!("missing path"))?;
    match item.kind {
        WatchItemKind::DirCount => eval_dir_count(&path, &item.filter),
        WatchItemKind::DirSize => eval_dir_size(&path, &item.filter),
        WatchItemKind::LatestFile => eval_latest_file(&path, &item.filter),
        WatchItemKind::FileExists => eval_file_exists(&path),
        WatchItemKind::FileAge => eval_file_age(&path),
        WatchItemKind::FileRegexCount => eval_file_regex_count(&path, item.regex.as_deref()),
    }
}

fn eval_dir_count(path: &Path, filter: &WatchFilterConfig) -> Result<WatchItemValue> {
    ensure_dir(path)?;
    let globset = build_globset(filter)?;
    let mut count = 0u64;
    for entry in walk_dir_entries(path, filter) {
        let entry = entry?;
        if entry.file_type().is_file() && filter_match(entry.path(), filter, globset.as_ref()) {
            count += 1;
        }
    }
    Ok(WatchItemValue {
        raw_value: Some(WatchRawValue::Count(count)),
        value_text: count.to_string(),
    })
}

fn eval_dir_size(path: &Path, filter: &WatchFilterConfig) -> Result<WatchItemValue> {
    ensure_dir(path)?;
    let globset = build_globset(filter)?;
    let mut total = 0u64;
    for entry in walk_dir_entries(path, filter) {
        let entry = entry?;
        if entry.file_type().is_file() && filter_match(entry.path(), filter, globset.as_ref()) {
            total = total.saturating_add(entry.metadata()?.len());
        }
    }
    Ok(WatchItemValue {
        raw_value: Some(WatchRawValue::Bytes(total)),
        value_text: format_bytes(total),
    })
}

fn eval_latest_file(path: &Path, filter: &WatchFilterConfig) -> Result<WatchItemValue> {
    ensure_dir(path)?;
    let globset = build_globset(filter)?;
    let mut newest: Option<(SystemTime, PathBuf)> = None;
    for entry in walk_dir_entries(path, filter) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        if !filter_match(entry.path(), filter, globset.as_ref()) {
            continue;
        }
        let modified = entry.metadata()?.modified()?;
        let replace = newest
            .as_ref()
            .map(|(ts, _)| modified > *ts)
            .unwrap_or(true);
        if replace {
            newest = Some((modified, entry.path().to_path_buf()));
        }
    }

    if let Some((modified, path)) = newest {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("<unknown>")
            .to_string();
        let age = SystemTime::now()
            .duration_since(modified)
            .unwrap_or_else(|_| Duration::ZERO);
        Ok(WatchItemValue {
            raw_value: Some(WatchRawValue::Timestamp(modified)),
            value_text: format!("{} ({})", name, format_age(age)),
        })
    } else {
        Ok(WatchItemValue {
            raw_value: None,
            value_text: "No files".to_string(),
        })
    }
}

fn eval_file_exists(path: &Path) -> Result<WatchItemValue> {
    let exists = path.exists();
    Ok(WatchItemValue {
        raw_value: Some(WatchRawValue::Bool(exists)),
        value_text: if exists { "Yes" } else { "No" }.to_string(),
    })
}

fn eval_file_age(path: &Path) -> Result<WatchItemValue> {
    let meta = path.metadata()?;
    let modified = meta.modified()?;
    let age = SystemTime::now()
        .duration_since(modified)
        .unwrap_or_else(|_| Duration::ZERO);
    Ok(WatchItemValue {
        raw_value: Some(WatchRawValue::Duration(age)),
        value_text: format_age(age),
    })
}

fn eval_file_regex_count(path: &Path, pattern: Option<&str>) -> Result<WatchItemValue> {
    let pattern = pattern.unwrap_or("");
    if pattern.trim().is_empty() {
        bail!("missing regex pattern");
    }
    let re = Regex::new(pattern)?;
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut count = 0u64;
    for line in reader.lines() {
        let line = line?;
        count = count.saturating_add(re.find_iter(&line).count() as u64);
    }
    Ok(WatchItemValue {
        raw_value: Some(WatchRawValue::Count(count)),
        value_text: count.to_string(),
    })
}

fn ensure_dir(path: &Path) -> Result<()> {
    if !path.is_dir() {
        bail!("{} is not a directory", path.display());
    }
    Ok(())
}

fn walk_dir_entries(path: &Path, filter: &WatchFilterConfig) -> walkdir::IntoIter {
    let mut walker = WalkDir::new(path).min_depth(1);
    if !filter.recursive {
        walker = walker.max_depth(1);
    }
    walker.into_iter()
}

fn build_globset(filter: &WatchFilterConfig) -> Result<Option<GlobSet>> {
    let Some(glob) = filter.glob.as_deref() else {
        return Ok(None);
    };
    if glob.trim().is_empty() {
        return Ok(None);
    }
    let mut builder = GlobSetBuilder::new();
    builder.add(Glob::new(glob)?);
    Ok(Some(builder.build()?))
}

fn filter_match(path: &Path, filter: &WatchFilterConfig, globset: Option<&GlobSet>) -> bool {
    if let Some(globset) = globset {
        if !globset.is_match(path) {
            return false;
        }
    }
    if filter.extensions.is_empty() {
        return true;
    }
    let ext = path
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    filter
        .extensions
        .iter()
        .any(|e| e.eq_ignore_ascii_case(&ext))
}

fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.2} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{} B", bytes)
    }
}

fn format_age(duration: Duration) -> String {
    if duration.as_secs() < 60 {
        format!("{}s ago", duration.as_secs())
    } else if duration.as_secs() < 3600 {
        format!("{}m ago", duration.as_secs() / 60)
    } else if duration.as_secs() < 86_400 {
        format!("{}h ago", duration.as_secs() / 3600)
    } else {
        format!("{}d ago", duration.as_secs() / 86_400)
    }
}

#[cfg(test)]
mod tests {
    use super::format_bytes;

    #[test]
    fn format_bytes_values() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
    }
}
