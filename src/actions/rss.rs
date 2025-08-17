use anyhow::{anyhow, Result};
use chrono::DateTime;
use shlex;

use crate::plugins::rss::storage;

/// Execute an RSS command routed from the launcher.
///
/// Commands are space separated: `<verb> [args...]`.
/// The `target` may be a feed id, name, group or `all` depending on the
/// verb.
pub fn run(command: &str) -> Result<()> {
    let mut parts = command.splitn(2, ' ');
    let verb = parts.next().unwrap_or("");
    let rest = parts.next().unwrap_or("");
    match verb {
        "refresh" => refresh(rest),
        "open" => open(rest),
        "mark" => mark(rest),
        // `dialog` opens the UI; nothing to do in CLI.
        "dialog" => Ok(()),
        _ => Ok(()),
    }
}

fn refresh(_target: &str) -> Result<()> {
    // Refresh feed(s); accepts id, name, group or `all`.
    Ok(())
}

fn open(target: &str) -> Result<()> {
    let feed_id = target.trim();
    if feed_id.is_empty() {
        return Ok(());
    }
    let mut state = storage::StateFile::load();
    let feeds = storage::FeedsFile::load();
    let feed = feeds
        .feeds
        .iter()
        .find(|f| f.id == feed_id || f.title.as_deref() == Some(feed_id))
        .ok_or_else(|| anyhow!("feed not found"))?;

    let cache = storage::FeedCache::load(&feed.id);
    let entry = state.feeds.entry(feed.id.clone()).or_default();
    let cursor = entry.catchup.unwrap_or(0);
    for item in &cache.items {
        let ts = item.timestamp.unwrap_or(0);
        if ts <= cursor || entry.read.contains(&item.guid) {
            continue;
        }
        if let Some(link) = &item.link {
            let _ = open::that(link);
        }
        entry.read.insert(item.guid.clone());
    }
    recompute_unread(&feed.id, entry);
    state.save()
}

fn mark(command: &str) -> Result<()> {
    let mut parts = command.splitn(2, ' ');
    let subverb = parts.next().unwrap_or("");
    let rest = parts.next().unwrap_or("");
    match subverb {
        "read" => mark_read(rest),
        "unread" => mark_unread(rest),
        _ => Ok(()),
    }
}

fn mark_read(args: &str) -> Result<()> {
    let parts = shlex::split(args).unwrap_or_default();
    if parts.is_empty() {
        return Ok(());
    }
    let target = &parts[0];
    // parse --through
    let mut through: Option<String> = None;
    let mut i = 1;
    while i < parts.len() {
        if parts[i] == "--through" && i + 1 < parts.len() {
            through = Some(parts[i + 1].clone());
            break;
        }
        i += 1;
    }
    let through = through.ok_or_else(|| anyhow!("--through required"))?;
    let mut state = storage::StateFile::load();
    let feeds = storage::FeedsFile::load();
    let targets: Vec<String> = if target == "all" {
        feeds.feeds.iter().map(|f| f.id.clone()).collect()
    } else {
        feeds
            .feeds
            .iter()
            .filter(|f| f.id == *target || f.title.as_deref() == Some(target))
            .map(|f| f.id.clone())
            .collect()
    };
    for fid in targets {
        let cache = storage::FeedCache::load(&fid);
        let entry = state.feeds.entry(fid.clone()).or_default();
        let mut new_ts = if through == "newest" {
            cache
                .items
                .iter()
                .filter_map(|i| i.timestamp)
                .max()
                .unwrap_or(0)
        } else {
            DateTime::parse_from_rfc3339(&through)?.timestamp() as u64
        };
        if let Some(cur) = entry.catchup {
            if new_ts < cur {
                new_ts = cur;
            }
        }
        entry.catchup = Some(new_ts);
        // Build map of guid -> timestamp for pruning
        let ts_map: std::collections::HashMap<_, _> = cache
            .items
            .iter()
            .filter_map(|i| i.timestamp.map(|ts| (i.guid.clone(), ts)))
            .collect();
        let cutoff = new_ts;
        entry
            .read
            .retain(|id| ts_map.get(id).map(|t| *t > cutoff).unwrap_or(true));
        recompute_unread(&fid, entry);
    }
    state.save()
}

fn mark_unread(args: &str) -> Result<()> {
    let ids = shlex::split(args).unwrap_or_default();
    if ids.is_empty() {
        return Ok(());
    }
    let mut state = storage::StateFile::load();
    for spec in ids {
        if let Some((fid, guid)) = spec.split_once('/') {
            if let Some(entry) = state.feeds.get_mut(fid) {
                entry.read.remove(guid);
                recompute_unread(fid, entry);
            }
        }
    }
    state.save()
}

fn recompute_unread(feed_id: &str, entry: &mut storage::FeedState) {
    let cache = storage::FeedCache::load(feed_id);
    let cursor = entry.catchup.unwrap_or(0);
    let count = cache
        .items
        .iter()
        .filter(|i| {
            let ts = i.timestamp.unwrap_or(0);
            ts > cursor && !entry.read.contains(&i.guid)
        })
        .count();
    entry.unread = count as u32;
}
