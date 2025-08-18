use crate::actions::Action;
use crate::plugin::Plugin;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

pub const RSS_FILE: &str = "rss.json";

#[derive(Serialize, Deserialize, Clone)]
pub struct RssEntry {
    pub url: String,
}

static RSS_DATA: Lazy<Arc<Mutex<Vec<RssEntry>>>> =
    Lazy::new(|| Arc::new(Mutex::new(load_rss(RSS_FILE).unwrap_or_default())));

fn load_rss(path: &str) -> anyhow::Result<Vec<RssEntry>> {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }
    let list: Vec<RssEntry> = serde_json::from_str(&content)?;
    Ok(list)
}

fn save_rss(path: &str, feeds: &[RssEntry]) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(feeds)?;
    std::fs::write(path, json)?;
    Ok(())
}

pub fn append_feed(path: &str, url: &str) -> anyhow::Result<()> {
    let mut list = load_rss(path).unwrap_or_default();
    if !list.iter().any(|f| f.url == url) {
        list.push(RssEntry {
            url: url.to_string(),
        });
        save_rss(path, &list)?;
        if let Ok(mut data) = RSS_DATA.lock() {
            *data = list;
        }
    }
    Ok(())
}

pub fn remove_feed(path: &str, url: &str) -> anyhow::Result<()> {
    let mut list = load_rss(path).unwrap_or_default();
    if let Some(pos) = list.iter().position(|f| f.url == url) {
        list.remove(pos);
        save_rss(path, &list)?;
        if let Ok(mut data) = RSS_DATA.lock() {
            *data = list;
        }
    }
    Ok(())
}

pub struct RssPlugin {
    matcher: SkimMatcherV2,
    data: Arc<Mutex<Vec<RssEntry>>>,
}

impl Default for RssPlugin {
    fn default() -> Self {
        Self {
            matcher: SkimMatcherV2::default(),
            data: RSS_DATA.clone(),
        }
    }
}

impl Plugin for RssPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "rss") {
            if rest.trim().is_empty() {
                return vec![
                    Action {
                        label: "rss add".into(),
                        desc: "RSS".into(),
                        action: "query:rss add ".into(),
                        args: None,
                    },
                    Action {
                        label: "rss list".into(),
                        desc: "RSS".into(),
                        action: "query:rss list".into(),
                        args: None,
                    },
                    Action {
                        label: "rss rm".into(),
                        desc: "RSS".into(),
                        action: "query:rss rm ".into(),
                        args: None,
                    },
                ];
            }
        }
        const ADD_PREFIX: &str = "rss add ";
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, ADD_PREFIX) {
            let url = rest.trim();
            if !url.is_empty() {
                return vec![Action {
                    label: format!("Add RSS feed {url}"),
                    desc: "RSS".into(),
                    action: format!("rss:add:{url}"),
                    args: None,
                }];
            }
        }
        const RM_PREFIX: &str = "rss rm";
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, RM_PREFIX) {
            let filter = rest.trim();
            let guard = match self.data.lock() {
                Ok(g) => g,
                Err(_) => return Vec::new(),
            };
            return guard
                .iter()
                .filter(|f| self.matcher.fuzzy_match(&f.url, filter).is_some())
                .map(|f| Action {
                    label: format!("Remove RSS feed {}", f.url),
                    desc: "RSS".into(),
                    action: format!("rss:remove:{}", f.url),
                    args: None,
                })
                .collect();
        }
        const LIST_PREFIX: &str = "rss list";
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, LIST_PREFIX) {
            let filter = rest.trim();
            let guard = match self.data.lock() {
                Ok(g) => g,
                Err(_) => return Vec::new(),
            };
            return guard
                .iter()
                .filter(|f| self.matcher.fuzzy_match(&f.url, filter).is_some())
                .map(|f| Action {
                    label: f.url.clone(),
                    desc: "RSS".into(),
                    action: f.url.clone(),
                    args: None,
                })
                .collect();
        }
        const PREFIX: &str = "rss";
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, PREFIX) {
            let filter = rest.trim();
            if filter.is_empty() {
                return Vec::new();
            }
            let guard = match self.data.lock() {
                Ok(g) => g,
                Err(_) => return Vec::new(),
            };
            return guard
                .iter()
                .filter(|f| self.matcher.fuzzy_match(&f.url, filter).is_some())
                .map(|f| Action {
                    label: f.url.clone(),
                    desc: "RSS".into(),
                    action: f.url.clone(),
                    args: None,
                })
                .collect();
        }
        Vec::new()
    }

    fn name(&self) -> &str {
        "rss"
    }

    fn description(&self) -> &str {
        "Manage RSS feed URLs (prefix: `rss`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "rss".into(),
                desc: "RSS".into(),
                action: "query:rss ".into(),
                args: None,
            },
            Action {
                label: "rss add".into(),
                desc: "RSS".into(),
                action: "query:rss add ".into(),
                args: None,
            },
            Action {
                label: "rss list".into(),
                desc: "RSS".into(),
                action: "query:rss list".into(),
                args: None,
            },
            Action {
                label: "rss rm".into(),
                desc: "RSS".into(),
                action: "query:rss rm ".into(),
                args: None,
            },
        ]
    }
}

