use anyhow::{Context, Result};
use chrono::Utc;
use rand::{thread_rng, Rng};
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED, LOCATION};
use reqwest::StatusCode;
use std::time::Duration;

use super::storage::{CachedItem, FeedCache, FeedConfig, FeedState, StateFile};

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct MediaMetadata {
    pub url: Option<String>,
    pub content_type: Option<String>,
    pub length: Option<u64>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Item {
    pub id: String,
    pub title: Option<String>,
    pub link: Option<String>,
    pub published: Option<u64>,
    pub author: Option<String>,
    pub summary: Option<String>,
    #[serde(default)]
    pub media: Vec<MediaMetadata>,
}

pub struct Poller {
    client: Client,
}

impl Poller {
    pub fn new() -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("multi-launcher rss poller")
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self { client })
    }

    /// Poll a single feed returning newly discovered items.
    ///
    /// Updates state and optional cache on success.
    pub fn poll_feed(
        &self,
        feed: &mut FeedConfig,
        state: &mut StateFile,
        cache_items: bool,
        force: bool,
    ) -> Result<Vec<Item>> {
        let now = Utc::now().timestamp() as u64;
        let entry = state.feeds.entry(feed.id.clone()).or_default();

        // Respect backoff if a previous error requested it unless forced.
        if !force {
            if let Some(until) = entry.backoff_until {
                if until > now {
                    return Ok(Vec::new());
                }
            }
        }

        // Handle redirects manually.
        let mut url = feed.url.clone();
        let resp = loop {
            let mut req = self.client.get(&url);
            if !force {
                if let Some(etag) = &entry.etag {
                    req = req.header(IF_NONE_MATCH, etag.as_str());
                }
                if let Some(lm) = &entry.last_modified {
                    req = req.header(IF_MODIFIED_SINCE, lm.as_str());
                }
            }
            let resp = match req.send() {
                Ok(r) => r,
                Err(err) => {
                    let delay = record_error(entry, now, err.to_string());
                    feed.last_poll = Some(now);
                    feed.cadence = Some(delay);
                    feed.next_poll = Some(now + delay);
                    state.save()?;
                    return Err(err.into());
                }
            };
            if resp.status().is_redirection() {
                if let Some(loc) = resp.headers().get(LOCATION).and_then(|h| h.to_str().ok()) {
                    if resp.status() == StatusCode::MOVED_PERMANENTLY
                        || resp.status() == StatusCode::PERMANENT_REDIRECT
                    {
                        if let Ok(new_url) = resp.url().join(loc) {
                            feed.url = new_url.to_string();
                            url = feed.url.clone();
                            continue;
                        }
                    }
                    if let Ok(new_url) = resp.url().join(loc) {
                        url = new_url.to_string();
                        continue;
                    }
                }
                let delay = record_error(entry, now, "redirect without location".into());
                feed.last_poll = Some(now);
                feed.cadence = Some(delay);
                feed.next_poll = Some(now + delay);
                state.save()?;
                anyhow::bail!("redirect without location");
            }
            break resp;
        };

        if !force && resp.status() == StatusCode::NOT_MODIFIED {
            // Nothing new; update fetch timestamp and reset errors.
            entry.last_fetch = Some(now);
            entry.error = None;
            entry.error_count = 0;
            entry.backoff_until = None;
            state.save()?;
            feed.last_poll = Some(now);
            let delay = thread_rng().gen_range(600..=1800);
            feed.cadence = Some(delay);
            feed.next_poll = Some(now + delay);
            return Ok(Vec::new());
        }

        if !resp.status().is_success() {
            let msg = format!("http status {}", resp.status());
            let delay = record_error(entry, now, msg.clone());
            feed.last_poll = Some(now);
            feed.cadence = Some(delay);
            feed.next_poll = Some(now + delay);
            state.save()?;
            anyhow::bail!(msg);
        }

        let headers = resp.headers().clone();
        let bytes = resp.bytes().context("read feed body")?;
        let feed_model = feed_rs::parser::parse(&bytes[..]).context("parse feed")?;

        // On first successful fetch, treat existing items as historical and
        // advance the catch-up cursor to the newest entry.
        if entry.last_guid.is_none() && entry.last_read_published.is_none() {
            if let Some(max_ts) = feed_model
                .entries
                .iter()
                .filter_map(|e| e.published.or(e.updated).map(|d| d.timestamp() as u64))
                .max()
            {
                entry.last_read_published = Some(max_ts);
            }
            entry.last_guid = feed_model.entries.first().map(|e| e.id.clone());
            if cache_items {
                let mut cache = FeedCache::load(&feed.id);
                for e in &feed_model.entries {
                    let item = convert_entry(e);
                    cache.items.push(CachedItem {
                        guid: item.id.clone(),
                        title: item.title.clone().unwrap_or_default(),
                        link: item.link.clone(),
                        timestamp: item.published,
                    });
                }
                cache.save(&feed.id)?;
            }
            update_success(entry, now, &headers);
            feed.last_poll = Some(now);
            let delay = thread_rng().gen_range(600..=1800);
            feed.cadence = Some(delay);
            feed.next_poll = Some(now + delay);
            state.save()?;
            return Ok(Vec::new());
        }

        // Collect new items until the last known GUID is encountered.
        let mut new_items = Vec::new();
        for entry_model in &feed_model.entries {
            let id = entry_model.id.clone();
            if entry.last_guid.as_deref() == Some(&id) {
                break;
            }
            new_items.push(convert_entry(entry_model));
        }
        if !new_items.is_empty() {
            // Preserve chronological order from oldest to newest.
            new_items.reverse();
            entry.last_guid = Some(feed_model.entries[0].id.clone());
            entry.unread = entry.unread.saturating_add(new_items.len() as u32);
            if cache_items {
                let mut cache = FeedCache::load(&feed.id);
                for it in &new_items {
                    cache.items.push(CachedItem {
                        guid: it.id.clone(),
                        title: it.title.clone().unwrap_or_default(),
                        link: it.link.clone(),
                        timestamp: it.published,
                    });
                }
                cache.save(&feed.id)?;
            }
        }

        update_success(entry, now, &headers);
        feed.last_poll = Some(now);
        let delay = thread_rng().gen_range(600..=1800);
        feed.cadence = Some(delay);
        feed.next_poll = Some(now + delay);
        state.save()?;
        Ok(new_items)
    }
}

fn record_error(state: &mut FeedState, now: u64, msg: String) -> u64 {
    state.error = Some(msg);
    state.error_count += 1;
    let delay = 2u64.pow(state.error_count.min(5)) * 60; // seconds
    state.backoff_until = Some(now + delay);
    delay
}

fn update_success(state: &mut FeedState, now: u64, headers: &HeaderMap) {
    state.last_fetch = Some(now);
    state.etag = headers
        .get(ETAG)
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());
    state.last_modified = headers
        .get(LAST_MODIFIED)
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());
    state.error = None;
    state.error_count = 0;
    state.backoff_until = None;
}

fn convert_entry(entry: &feed_rs::model::Entry) -> Item {
    let link = entry.links.first().map(|l| l.href.clone());
    let title = entry.title.as_ref().map(|t| t.content.clone());
    let published = entry
        .published
        .or(entry.updated)
        .map(|dt| dt.timestamp() as u64);
    let author = entry.authors.first().map(|p| p.name.clone());
    let summary = entry.summary.as_ref().map(|s| s.content.clone());

    let mut media = Vec::new();
    for m in &entry.media {
        for c in &m.content {
            media.push(MediaMetadata {
                url: c.url.as_ref().map(|u| u.to_string()),
                content_type: c.content_type.as_ref().map(|m| m.to_string()),
                length: c.size,
            });
        }
    }

    Item {
        id: entry.id.clone(),
        title,
        link,
        published,
        author,
        summary,
        media,
    }
}
