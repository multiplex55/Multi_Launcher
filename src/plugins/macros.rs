use crate::actions::{load_actions, Action};
use crate::common::json_watch::{watch_json, JsonWatcher};
use crate::launcher::launch_action;
use crate::plugin::{Plugin, PluginManager};
use crate::settings::Settings;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

pub const MACROS_FILE: &str = "macros.json";
pub static STEP_MESSAGES: Lazy<Mutex<Vec<String>>> = Lazy::new(|| Mutex::new(Vec::new()));
pub static ERROR_MESSAGES: Lazy<Mutex<Vec<String>>> = Lazy::new(|| Mutex::new(Vec::new()));

#[derive(Serialize, Deserialize, Clone)]
pub struct MacroStep {
    /// Display label for this step.
    pub label: String,
    /// Command string to execute.
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<String>,
    /// Delay in milliseconds after this step when using manual delays.
    #[serde(default)]
    pub delay_ms: u64,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct MacroEntry {
    pub label: String,
    pub desc: String,
    /// When set, a fixed delay in milliseconds applied after every step.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_delay_ms: Option<u64>,
    #[serde(default)]
    pub steps: Vec<MacroStep>,
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

pub fn take_step_messages() -> Vec<String> {
    if let Some(mut list) = STEP_MESSAGES.lock().ok() {
        let out = list.clone();
        list.clear();
        out
    } else {
        Vec::new()
    }
}

pub fn take_error_messages() -> Vec<String> {
    if let Some(mut list) = ERROR_MESSAGES.lock().ok() {
        let out = list.clone();
        list.clear();
        out
    } else {
        Vec::new()
    }
}

fn search_first_action(query: &str) -> Option<Action> {
    let settings = Settings::load("settings.json").unwrap_or_default();
    let actions = load_actions("actions.json").unwrap_or_default();
    let mut pm = PluginManager::new();
    let dirs = settings.plugin_dirs.unwrap_or_default();
    let actions_arc = Arc::new(actions);
    pm.reload_from_dirs(
        &dirs,
        settings.clipboard_limit,
        settings.net_unit,
        false,
        &settings.plugin_settings,
        actions_arc,
    );
    pm.search_filtered(
        query,
        settings.enabled_plugins.as_ref(),
        settings.enabled_capabilities.as_ref(),
    )
    .into_iter()
    .next()
}

pub fn run_macro(name: &str) -> anyhow::Result<()> {
    let list = load_macros(MACROS_FILE).unwrap_or_default();
    if let Some(entry) = list.iter().find(|m| m.label.eq_ignore_ascii_case(name)) {
        for (i, step) in entry.steps.iter().enumerate() {
            let mut command = step.command.trim().to_string();
            let mut args = step.args.clone();
            if let Some(ref s) = args {
                if s.trim().is_empty() {
                    args = None;
                }
            }

            let mut query = if let Some(q) = command.strip_prefix("query:") {
                q.to_string()
            } else {
                command.clone()
            };
            if let Some(ref a) = args {
                if !query.ends_with(' ') {
                    query.push(' ');
                }
                query.push_str(a);
            }

            if let Some(res) = search_first_action(&query) {
                command = res.action;
                args = res.args;
            } else if command.starts_with("query:") {
                command = query;
                args = None;
            }
            tracing::info!(
                step = i + 1,
                label = %step.label,
                command = %command,
                args = ?args,
                "running macro step"
            );
            let act = Action {
                label: step.label.clone(),
                desc: String::new(),
                action: command,
                args,
            };
            if let Err(e) = launch_action(&act) {
                tracing::error!(?e, "failed to run macro step");
                if let Some(mut errs) = ERROR_MESSAGES.lock().ok() {
                    errs.push(format!("Step {} error: {e}", i + 1));
                }
            }
            if let Some(mut msgs) = STEP_MESSAGES.lock().ok() {
                msgs.push(format!("Step {}: {}", i + 1, step.label));
            }
            let delay = match entry.auto_delay_ms {
                Some(ms) => ms,
                None => step.delay_ms,
            };
            if delay > 0 && i + 1 < entry.steps.len() {
                std::thread::sleep(std::time::Duration::from_millis(delay));
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
