use anyhow::{anyhow, Result};
use chrono::DateTime;
use shlex;
use std::collections::HashSet;
use std::fs;

use crate::plugins::rss::{poller::Poller, storage};
use roxmltree::Document;
use slug::slugify;
use url::Url;

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
        "ls" => ls(rest),
        "items" => items(rest),
        "open" => open(rest),
        "group" => group(rest),
        "mark" => mark(rest),
        "import" => import(rest),
        "export" => export(rest),
        // `dialog` opens the UI; nothing to do in CLI.
        "dialog" => Ok(()),
        _ => Ok(()),
    }
}

fn refresh(args: &str) -> Result<()> {
    let parts = shlex::split(args).unwrap_or_default();
    if parts.is_empty() {
        return Ok(());
    }
    let target = &parts[0];
    let force = parts.iter().any(|p| p == "--force");

    let mut feeds = storage::FeedsFile::load();
    let mut state = storage::StateFile::load();
    let poller = Poller::new()?;
    let now = chrono::Utc::now().timestamp() as u64;
    let mut changed = false;
    let targets = resolve_targets_mut(&mut feeds, target);
    for feed in targets {
        if !force {
            if let Some(next) = feed.next_poll {
                if next > now {
                    continue;
                }
            }
        }
        let _ = poller.poll_feed(feed, &mut state, true, force);
        changed = true;
    }
    if changed {
        feeds.save()?;
    }
    Ok(())
}

fn open(args: &str) -> Result<()> {
    let parts = shlex::split(args).unwrap_or_default();
    if parts.is_empty() {
        return Ok(());
    }
    let target = &parts[0];
    let mut unread_only = false;
    let mut limit: Option<usize> = None;
    let mut since: Option<u64> = None;
    let mut newest_first = true;
    let mut i = 1;
    while i < parts.len() {
        match parts[i].as_str() {
            "--unread" => unread_only = true,
            "--n" if i + 1 < parts.len() => {
                limit = parts[i + 1].parse().ok();
                i += 1;
            }
            "--since" if i + 1 < parts.len() => {
                if let Ok(dt) = DateTime::parse_from_rfc3339(&parts[i + 1]) {
                    since = Some(dt.timestamp() as u64);
                }
                i += 1;
            }
            "--order" if i + 1 < parts.len() => {
                newest_first = parts[i + 1].to_lowercase() != "oldest";
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }
    let mut state = storage::StateFile::load();
    let feeds = storage::FeedsFile::load();
    let targets = resolve_targets(&feeds, target);
    let mut items = collect_items(&targets, &state, unread_only, since);
    items.sort_by_key(|i| i.timestamp);
    if newest_first {
        items.reverse();
    }
    if let Some(n) = limit {
        items.truncate(n);
    }
    for item in &items {
        if let Some(link) = &item.link {
            let _ = open::that(link);
        }
        let entry = state.feeds.entry(item.feed_id.clone()).or_default();
        entry.read.insert(item.guid.clone());
        recompute_unread(&item.feed_id, entry);
    }
    state.save()
}

fn ls(args: &str) -> Result<()> {
    let parts = shlex::split(args).unwrap_or_default();
    let target = parts.get(0).map(|s| s.as_str()).unwrap_or("groups");
    let mut unread_only = false;
    if parts.iter().any(|p| p == "--unread") {
        unread_only = true;
    }
    let feeds = storage::FeedsFile::load();
    let state = storage::StateFile::load();
    if target == "groups" {
        for g in &feeds.groups {
            let unread: u32 = feeds
                .feeds
                .iter()
                .filter(|f| f.group.as_deref() == Some(g))
                .map(|f| state.feeds.get(&f.id).map(|s| s.unread).unwrap_or(0))
                .sum();
            if unread_only && unread == 0 {
                continue;
            }
            println!("{g}\t{unread}");
        }
    } else {
        for f in resolve_targets(&feeds, target) {
            let unread = state.feeds.get(&f.id).map(|s| s.unread).unwrap_or(0);
            if unread_only && unread == 0 {
                continue;
            }
            let title = f.title.clone().unwrap_or_else(|| f.id.clone());
            println!("{}\t{}\t{}", f.id, title, unread);
        }
    }
    Ok(())
}

fn items(args: &str) -> Result<()> {
    let parts = shlex::split(args).unwrap_or_default();
    if parts.is_empty() {
        return Ok(());
    }
    let target = &parts[0];
    let mut unread_only = false;
    let mut limit: Option<usize> = None;
    let mut since: Option<u64> = None;
    let mut newest_first = true;
    let mut i = 1;
    while i < parts.len() {
        match parts[i].as_str() {
            "--unread" => unread_only = true,
            "--n" if i + 1 < parts.len() => {
                limit = parts[i + 1].parse().ok();
                i += 1;
            }
            "--since" if i + 1 < parts.len() => {
                if let Ok(dt) = DateTime::parse_from_rfc3339(&parts[i + 1]) {
                    since = Some(dt.timestamp() as u64);
                }
                i += 1;
            }
            "--order" if i + 1 < parts.len() => {
                newest_first = parts[i + 1].to_lowercase() != "oldest";
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }
    let state = storage::StateFile::load();
    let feeds = storage::FeedsFile::load();
    let targets = resolve_targets(&feeds, target);
    let mut items = collect_items(&targets, &state, unread_only, since);
    items.sort_by_key(|i| i.timestamp);
    if newest_first {
        items.reverse();
    }
    if let Some(n) = limit {
        items.truncate(n);
    }
    for item in &items {
        let title = &item.title;
        let link = item.link.as_deref().unwrap_or("");
        println!("{}\t{}\t{}", item.feed_id, title, link);
    }
    Ok(())
}

fn group(args: &str) -> Result<()> {
    let parts = shlex::split(args).unwrap_or_default();
    if parts.is_empty() {
        return Ok(());
    }
    let sub = parts[0].as_str();
    match sub {
        "add" if parts.len() >= 2 => group_add(&parts[1]),
        "rm" if parts.len() >= 2 => group_rm(&parts[1]),
        "mv" if parts.len() >= 3 => group_mv(&parts[1], &parts[2]),
        _ => Ok(()),
    }
}

fn group_add(name: &str) -> Result<()> {
    if name.is_empty() {
        return Ok(());
    }
    let mut feeds = storage::FeedsFile::load();
    if !feeds.groups.iter().any(|g| g == name) {
        feeds.groups.push(name.to_string());
        feeds.save()?;
    }
    Ok(())
}

fn group_rm(name: &str) -> Result<()> {
    let mut feeds = storage::FeedsFile::load();
    feeds.groups.retain(|g| g != name);
    for f in feeds.feeds.iter_mut() {
        if f.group.as_deref() == Some(name) {
            f.group = None;
        }
    }
    feeds.save()?;
    Ok(())
}

fn group_mv(old: &str, new: &str) -> Result<()> {
    let mut feeds = storage::FeedsFile::load();
    if let Some(g) = feeds.groups.iter_mut().find(|g| g.as_str() == old) {
        *g = new.to_string();
    }
    for f in feeds.feeds.iter_mut() {
        if f.group.as_deref() == Some(old) {
            f.group = Some(new.to_string());
        }
    }
    feeds.save()?;
    Ok(())
}

fn import(args: &str) -> Result<()> {
    let parts = shlex::split(args).unwrap_or_default();
    if parts.is_empty() {
        return Ok(());
    }
    let path = &parts[0];
    let data = fs::read_to_string(path)?;
    let doc = Document::parse(&data)?;

    let mut outlines = Vec::new();
    if let Some(body) = doc.descendants().find(|n| n.has_tag_name("body")) {
        for node in body
            .children()
            .filter(|n| n.is_element() && n.has_tag_name("outline"))
        {
            collect_outline(node, None, &mut outlines);
        }
    }

    let mut feeds = storage::FeedsFile::load();
    let mut existing_urls: HashSet<String> =
        feeds.feeds.iter().map(|f| f.url.clone()).collect();
    let mut existing_ids: HashSet<String> =
        feeds.feeds.iter().map(|f| f.id.clone()).collect();
    let mut added = 0;
    let mut duplicates = 0;
    let mut invalid = 0;

    for o in outlines {
        if existing_urls.contains(&o.url) {
            duplicates += 1;
            continue;
        }
        if Url::parse(&o.url).is_err() {
            invalid += 1;
            continue;
        }
        if let Some(g) = &o.group {
            if !feeds.groups.iter().any(|x| x == g) {
                feeds.groups.push(g.clone());
            }
        }
        let mut id_base = slugify(&o.title);
        if id_base.is_empty() {
            id_base = slugify(&o.url);
        }
        if id_base.is_empty() {
            id_base = format!("feed{}", feeds.feeds.len());
        }
        let mut id = id_base.clone();
        let mut idx = 1;
        while existing_ids.contains(&id) {
            id = format!("{id_base}-{idx}");
            idx += 1;
        }
        existing_ids.insert(id.clone());
        existing_urls.insert(o.url.clone());
        feeds.feeds.push(storage::FeedConfig {
            id,
            url: o.url,
            title: if o.title.is_empty() { None } else { Some(o.title) },
            group: o.group,
            last_poll: None,
            next_poll: None,
            cadence: None,
        });
        added += 1;
    }

    if added > 0 {
        feeds.save()?;
    }
    println!("imported {added}, duplicates {duplicates}, invalid {invalid}");
    Ok(())
}

fn export(args: &str) -> Result<()> {
    let parts = shlex::split(args).unwrap_or_default();
    if parts.is_empty() {
        return Ok(());
    }
    let path = &parts[0];
    let feeds = storage::FeedsFile::load();
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<opml version=\"1.0\">\n<head><title>feeds</title></head>\n<body>\n");
    for g in &feeds.groups {
        out.push_str(&format!("  <outline text=\"{}\">\n", xml_escape(g)));
        for f in feeds.feeds.iter().filter(|f| f.group.as_deref() == Some(g)) {
            let title = f
                .title
                .clone()
                .unwrap_or_else(|| f.url.clone());
            out.push_str(&format!(
                "    <outline type=\"rss\" text=\"{}\" xmlUrl=\"{}\"",
                xml_escape(&title),
                xml_escape(&f.url)
            ));
            if let Some(t) = &f.title {
                out.push_str(&format!(" title=\"{}\"", xml_escape(t)));
            }
            out.push_str(" />\n");
        }
        out.push_str("  </outline>\n");
    }
    for f in feeds.feeds.iter().filter(|f| f.group.is_none()) {
        let title = f
            .title
            .clone()
            .unwrap_or_else(|| f.url.clone());
        out.push_str(&format!(
            "  <outline type=\"rss\" text=\"{}\" xmlUrl=\"{}\"",
            xml_escape(&title),
            xml_escape(&f.url)
        ));
        if let Some(t) = &f.title {
            out.push_str(&format!(" title=\"{}\"", xml_escape(t)));
        }
        out.push_str(" />\n");
    }
    out.push_str("</body>\n</opml>\n");
    fs::write(path, out)?;
    println!("exported {} feeds", feeds.feeds.len());
    Ok(())
}

struct OutlineFeed {
    title: String,
    url: String,
    group: Option<String>,
}

fn collect_outline(node: roxmltree::Node, group: Option<&str>, out: &mut Vec<OutlineFeed>) {
    if node.attribute("xmlUrl").is_some() || node.attribute("xmlURL").is_some() {
        let url = node
            .attribute("xmlUrl")
            .or_else(|| node.attribute("xmlURL"))
            .unwrap();
        let title = node
            .attribute("title")
            .or_else(|| node.attribute("text"))
            .unwrap_or(url);
        out.push(OutlineFeed {
            title: title.to_string(),
            url: url.to_string(),
            group: group.map(|s| s.to_string()),
        });
    } else {
        let next_group = node
            .attribute("text")
            .or_else(|| node.attribute("title"))
            .or(group);
        for child in node
            .children()
            .filter(|n| n.is_element() && n.has_tag_name("outline"))
        {
            collect_outline(child, next_group, out);
        }
    }
}

fn xml_escape(s: &str) -> String {
    let mut escaped = String::new();
    for c in s.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            _ => escaped.push(c),
        }
    }
    escaped
}

#[derive(Clone)]
struct ItemInfo {
    feed_id: String,
    guid: String,
    title: String,
    link: Option<String>,
    timestamp: u64,
}

fn collect_items(
    feeds: &[&storage::FeedConfig],
    state: &storage::StateFile,
    unread_only: bool,
    since: Option<u64>,
) -> Vec<ItemInfo> {
    let mut items = Vec::new();
    for feed in feeds {
        let entry = state.feeds.get(&feed.id).cloned().unwrap_or_default();
        let cursor = entry.catchup.unwrap_or(0);
        let cache = storage::FeedCache::load(&feed.id);
        for item in cache.items {
            let ts = item.timestamp.unwrap_or(0);
            if let Some(min) = since {
                if ts < min {
                    continue;
                }
            }
            let unread = ts > cursor && !entry.read.contains(&item.guid);
            if unread_only && !unread {
                continue;
            }
            items.push(ItemInfo {
                feed_id: feed.id.clone(),
                guid: item.guid,
                title: item.title,
                link: item.link,
                timestamp: ts,
            });
        }
    }
    items
}

fn resolve_targets<'a>(
    feeds: &'a storage::FeedsFile,
    target: &str,
) -> Vec<&'a storage::FeedConfig> {
    if target == "all" {
        return feeds.feeds.iter().collect();
    }
    if let Some(feed) = feeds
        .feeds
        .iter()
        .find(|f| f.id == target || f.title.as_deref() == Some(target))
    {
        return vec![feed];
    }
    feeds
        .feeds
        .iter()
        .filter(|f| f.group.as_deref() == Some(target))
        .collect()
}

fn resolve_targets_mut<'a>(
    feeds: &'a mut storage::FeedsFile,
    target: &str,
) -> Vec<&'a mut storage::FeedConfig> {
    if target == "all" {
        return feeds.feeds.iter_mut().collect();
    }
    if let Some(idx) = feeds
        .feeds
        .iter()
        .position(|f| f.id == target || f.title.as_deref() == Some(target))
    {
        return vec![&mut feeds.feeds[idx]];
    }
    feeds
        .feeds
        .iter_mut()
        .filter(|f| f.group.as_deref() == Some(target))
        .collect()
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
