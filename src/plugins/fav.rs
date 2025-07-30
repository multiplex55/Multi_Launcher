use crate::actions::Action;
use crate::common::json_watch::{watch_json, JsonWatcher};
use crate::launcher::launch_action;
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

pub fn append_fav(path: &str, entry: FavEntry) -> anyhow::Result<()> {
    let mut list = load_favs(path).unwrap_or_default();
    if !list.iter().any(|e| e.label == entry.label) {
        list.push(entry);
        save_favs(path, &list)?;
    }
    Ok(())
}

pub fn remove_fav(path: &str, label: &str) -> anyhow::Result<()> {
    let mut list = load_favs(path).unwrap_or_default();
    if let Some(pos) = list.iter().position(|e| e.label == label) {
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
        let watch_path = path.clone();
        let watcher = watch_json(&watch_path, {
            let watch_path = watch_path.clone();
            move || {
                if let Ok(list) = load_favs(&watch_path) {
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
            .filter(|f| {
                filter.is_empty()
                    || self.matcher.fuzzy_match(&f.label, filter).is_some()
                    || self.matcher.fuzzy_match(&f.action, filter).is_some()
            })
            .map(|f| Action {
                label: f.label.clone(),
                desc: "Fav".into(),
                action: f.action.clone(),
                args: f.args.clone(),
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
                desc: "Fav".into(),
                action: "fav:dialog".into(),
                args: None,
            }];
        }
        const ADD_PREFIX: &str = "fav add ";
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, ADD_PREFIX) {
            let label = rest.trim();
            if !label.is_empty() {
                return vec![Action {
                    label: format!("Add fav {label}"),
                    desc: "Fav".into(),
                    action: format!("fav:dialog:add:{label}"),
                    args: None,
                }];
            }
        }
        const RM_PREFIX: &str = "fav rm";
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, RM_PREFIX) {
            let filter = rest.trim();
            let guard = match self.data.lock() {
                Ok(g) => g,
                Err(_) => return Vec::new(),
            };
            return guard
                .iter()
                .filter(|f| {
                    filter.is_empty()
                        || self.matcher.fuzzy_match(&f.label, filter).is_some()
                        || self.matcher.fuzzy_match(&f.action, filter).is_some()
                })
                .map(|f| Action {
                    label: format!("Remove fav {}", f.label),
                    desc: "Fav".into(),
                    action: format!("fav:remove:{}", f.label),
                    args: None,
                })
                .collect();
        }
        const LIST_PREFIX: &str = "fav list";
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, LIST_PREFIX) {
            return self.list(rest.trim());
        }
        Vec::new()
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
                desc: "Fav".into(),
                action: "query:fav".into(),
                args: None,
            },
            Action {
                label: "fav add".into(),
                desc: "Fav".into(),
                action: "query:fav add ".into(),
                args: None,
            },
            Action {
                label: "fav rm".into(),
                desc: "Fav".into(),
                action: "query:fav rm ".into(),
                args: None,
            },
            Action {
                label: "fav list".into(),
                desc: "Fav".into(),
                action: "query:fav list".into(),
                args: None,
            },
        ]
    }
}

pub fn run_fav(label: &str) -> anyhow::Result<()> {
    let list = load_favs(FAV_FILE).unwrap_or_default();
    if let Some(entry) = list.iter().find(|f| f.label.eq_ignore_ascii_case(label)) {
        let act = Action {
            label: entry.label.clone(),
            desc: String::new(),
            action: entry.action.clone(),
            args: entry.args.clone(),
        };
        launch_action(&act)?;
    }
    Ok(())
}
