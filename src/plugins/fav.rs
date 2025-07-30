use crate::actions::Action;
use crate::common::json_watch::{watch_json, JsonWatcher};
use crate::plugin::Plugin;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

pub const FAV_FILE: &str = "fav.json";

#[derive(Serialize, Deserialize, Clone)]
pub struct FavEntry {
    pub label: String,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<String>,
}

pub fn load_favs(path: &str) -> anyhow::Result<Vec<FavEntry>> {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }
    let list: Vec<FavEntry> = serde_json::from_str(&content)?;
    Ok(list)
}

pub fn save_favs(path: &str, favs: &[FavEntry]) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(favs)?;
    std::fs::write(path, json)?;
    Ok(())
}

pub fn append_fav(path: &str, label: &str, action: &str, args: Option<&str>) -> anyhow::Result<()> {
    let mut list = load_favs(path).unwrap_or_default();
    if let Some(item) = list
        .iter_mut()
        .find(|e| e.label.eq_ignore_ascii_case(label))
    {
        item.action = action.to_string();
        item.args = args.map(|s| s.to_string());
    } else {
        list.push(FavEntry {
            label: label.to_string(),
            action: action.to_string(),
            args: args.map(|s| s.to_string()),
        });
    }
    save_favs(path, &list)
}

pub fn remove_fav(path: &str, label: &str) -> anyhow::Result<()> {
    let mut list = load_favs(path).unwrap_or_default();
    if let Some(pos) = list
        .iter()
        .position(|e| e.label.eq_ignore_ascii_case(label))
    {
        list.remove(pos);
        save_favs(path, &list)?;
    }
    Ok(())
}

pub struct FavPlugin {
    matcher: SkimMatcherV2,
    data: Arc<Mutex<Vec<FavEntry>>>,
    #[allow(dead_code)]
    watcher: Option<JsonWatcher>,
}

impl FavPlugin {
    pub fn new() -> Self {
        let data = Arc::new(Mutex::new(load_favs(FAV_FILE).unwrap_or_default()));
        let data_clone = data.clone();
        let path = FAV_FILE.to_string();
        let watcher = watch_json(&path, {
            let path = path.clone();
            move || {
                if let Ok(list) = load_favs(&path) {
                    if let Ok(mut lock) = data_clone.lock() {
                        *lock = list;
                    }
                }
            }
        })
        .ok();
        Self {
            matcher: SkimMatcherV2::default(),
            data,
            watcher,
        }
    }

    fn list(&self, filter: &str) -> Vec<Action> {
        let guard = match self.data.lock() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        guard
            .iter()
            .filter(|e| {
                filter.is_empty()
                    || self.matcher.fuzzy_match(&e.label, filter).is_some()
                    || self.matcher.fuzzy_match(&e.action, filter).is_some()
            })
            .map(|e| Action {
                label: e.label.clone(),
                desc: "Favorite".into(),
                action: e.action.clone(),
                args: e.args.clone(),
            })
            .collect()
    }
}

impl Default for FavPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for FavPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        if trimmed.eq_ignore_ascii_case("fav") {
            return vec![Action {
                label: "fav: edit favorites".into(),
                desc: "Favorite".into(),
                action: "fav:dialog".into(),
                args: None,
            }];
        }

        const ADD_PREFIX: &str = "fav add ";
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, ADD_PREFIX) {
            let label = rest.trim();
            if label.is_empty() {
                return vec![Action {
                    label: "fav: edit favorites".into(),
                    desc: "Favorite".into(),
                    action: "fav:dialog".into(),
                    args: None,
                }];
            }
            return vec![Action {
                label: format!("Add favorite {label}"),
                desc: "Favorite".into(),
                action: format!("fav:add:{label}"),
                args: None,
            }];
        }

        const RM_PREFIX: &str = "fav rm ";
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, RM_PREFIX) {
            let filter = rest.trim();
            let guard = match self.data.lock() {
                Ok(g) => g,
                Err(_) => return Vec::new(),
            };
            return guard
                .iter()
                .filter(|e| self.matcher.fuzzy_match(&e.label, filter).is_some())
                .map(|e| Action {
                    label: format!("Remove favorite {}", e.label),
                    desc: "Favorite".into(),
                    action: format!("fav:remove:{}", e.label),
                    args: None,
                })
                .collect();
        }

        const LIST_PREFIX: &str = "fav list";
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, LIST_PREFIX) {
            return self.list(rest.trim());
        }

        const PREFIX: &str = "fav";
        let rest = match crate::common::strip_prefix_ci(trimmed, PREFIX) {
            Some(r) => r,
            None => return Vec::new(),
        };
        self.list(rest.trim())
    }

    fn name(&self) -> &str {
        "fav"
    }

    fn description(&self) -> &str {
        "Favorite commands (prefix: `fav`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "fav".into(),
                desc: "Favorite".into(),
                action: "query:fav ".into(),
                args: None,
            },
            Action {
                label: "fav add".into(),
                desc: "Favorite".into(),
                action: "query:fav add ".into(),
                args: None,
            },
            Action {
                label: "fav rm".into(),
                desc: "Favorite".into(),
                action: "query:fav rm ".into(),
                args: None,
            },
            Action {
                label: "fav list".into(),
                desc: "Favorite".into(),
                action: "query:fav list".into(),
                args: None,
            },
        ]
    }
}
