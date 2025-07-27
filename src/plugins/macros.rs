use crate::actions::Action;
use crate::common::json_watch::{watch_json, JsonWatcher};
use crate::launcher::launch_action;
use crate::plugin::Plugin;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

pub const MACROS_FILE: &str = "macros.json";

#[derive(Serialize, Deserialize, Clone)]
pub struct MacroEntry {
    pub label: String,
    pub desc: String,
    #[serde(default)]
    pub steps: Vec<String>,
}

pub fn load_macros(path: &str) -> anyhow::Result<Vec<MacroEntry>> {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }
    let list: Vec<MacroEntry> = serde_json::from_str(&content)?;
    Ok(list)
}

pub fn save_macros(path: &str, macros: &[MacroEntry]) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(macros)?;
    std::fs::write(path, json)?;
    Ok(())
}

pub fn run_macro(name: &str) -> anyhow::Result<()> {
    let list = load_macros(MACROS_FILE).unwrap_or_default();
    if let Some(entry) = list.iter().find(|m| m.label.eq_ignore_ascii_case(name)) {
        for step in &entry.steps {
            let act = Action {
                label: step.clone(),
                desc: String::new(),
                action: step.clone(),
                args: None,
            };
            if let Err(e) = launch_action(&act) {
                tracing::error!(?e, "failed to run macro step");
            }
        }
    }
    Ok(())
}

pub struct MacrosPlugin {
    matcher: SkimMatcherV2,
    data: Arc<Mutex<Vec<MacroEntry>>>,
    #[allow(dead_code)]
    watcher: Option<JsonWatcher>,
}

impl MacrosPlugin {
    pub fn new() -> Self {
        let data = Arc::new(Mutex::new(load_macros(MACROS_FILE).unwrap_or_default()));
        let data_clone = data.clone();
        let path = MACROS_FILE.to_string();
        let watch_path = path.clone();
        let watcher = watch_json(&watch_path, {
            let watch_path = watch_path.clone();
            move || {
                if let Ok(list) = load_macros(&watch_path) {
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
            .filter(|m| {
                filter.is_empty()
                    || self.matcher.fuzzy_match(&m.label, filter).is_some()
                    || self.matcher.fuzzy_match(&m.desc, filter).is_some()
            })
            .map(|m| Action {
                label: m.label.clone(),
                desc: "Macro".into(),
                action: format!("macro:{}", m.label),
                args: None,
            })
            .collect()
    }
}

impl Default for MacrosPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for MacrosPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        if trimmed.eq_ignore_ascii_case("macro") {
            return vec![Action {
                label: "macro: edit macros".into(),
                desc: "Macro".into(),
                action: "macro:dialog".into(),
                args: None,
            }];
        }

        const LIST_PREFIX: &str = "macro list";
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, LIST_PREFIX) {
            return self.list(rest.trim());
        }

        const PREFIX: &str = "macro ";
        let rest = match crate::common::strip_prefix_ci(trimmed, PREFIX) {
            Some(r) => r,
            None => return Vec::new(),
        };
        self.list(rest.trim())
    }

    fn name(&self) -> &str {
        "macros"
    }

    fn description(&self) -> &str {
        "Run command macros (prefix: `macro`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "macro".into(),
                desc: "Macro".into(),
                action: "query:macro ".into(),
                args: None,
            },
            Action {
                label: "macro list".into(),
                desc: "Macro".into(),
                action: "query:macro list".into(),
                args: None,
            },
        ]
    }
}
