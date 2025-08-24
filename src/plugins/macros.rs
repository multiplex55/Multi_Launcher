use crate::actions::{load_actions, Action};
use crate::launcher::launch_action;
use crate::plugin::{Plugin, PluginManager};
use crate::settings::Settings;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use once_cell::sync::{Lazy, OnceCell};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

pub const MACROS_FILE: &str = "macros.json";
pub static STEP_MESSAGES: Lazy<Mutex<Vec<String>>> = Lazy::new(|| Mutex::new(Vec::new()));
pub static ERROR_MESSAGES: Lazy<Mutex<Vec<String>>> = Lazy::new(|| Mutex::new(Vec::new()));

/// Lazily constructed [`PluginManager`] reused across macro executions.
///
/// Access is serialized through a [`Mutex`] because the manager is mutable and
/// not `Sync`. The `OnceCell` ensures the manager is only initialised once,
/// avoiding the cost of rebuilding the plugin list on every macro step.
///
/// The associated [`SETTINGS_HASH`] tracks configuration changes; when relevant
/// settings differ from the cached hash the manager is refreshed. This design is
/// safe to call from multiple threads but callers will block while the manager
/// is reloaded.
static PLUGIN_MANAGER: OnceCell<Mutex<PluginManager>> = OnceCell::new();

/// Hash of the settings used to populate [`PLUGIN_MANAGER`].
static SETTINGS_HASH: OnceCell<Mutex<u64>> = OnceCell::new();

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

/// Search for the first matching action across all plugins.
///
/// The global [`PLUGIN_MANAGER`] is reused between calls. When the relevant
/// [`Settings`] change the manager is refreshed; otherwise the cached instance
/// is used to avoid plugin reinitialisation costs.
///
/// See `benches/macros_search.rs` for a simple Criterion benchmark measuring
/// the steady-state performance of this function.
pub fn search_first_action(query: &str) -> Option<Action> {
    let settings = Settings::load("settings.json").unwrap_or_default();
    let actions = load_actions("actions.json").unwrap_or_default();
    let dirs = settings.plugin_dirs.clone().unwrap_or_default();
    let actions_arc = Arc::new(actions);

    // Compute a hash of the settings fields that influence plugin loading.
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    dirs.hash(&mut hasher);
    settings.clipboard_limit.hash(&mut hasher);
    (settings.net_unit as u8).hash(&mut hasher);
    serde_json::to_string(&settings.enabled_plugins)
        .unwrap_or_default()
        .hash(&mut hasher);
    serde_json::to_string(&settings.enabled_capabilities)
        .unwrap_or_default()
        .hash(&mut hasher);
    serde_json::to_string(&settings.plugin_settings)
        .unwrap_or_default()
        .hash(&mut hasher);
    let settings_hash = hasher.finish();

    // Initialise global manager and update if settings have changed.
    let pm_cell = PLUGIN_MANAGER.get_or_init(|| Mutex::new(PluginManager::new()));
    let mut pm = pm_cell.lock().ok()?;
    let hash_cell = SETTINGS_HASH.get_or_init(|| Mutex::new(0));
    if let Ok(mut cached_hash) = hash_cell.lock() {
        if *cached_hash != settings_hash {
            pm.reload_from_dirs(
                &dirs,
                settings.clipboard_limit,
                settings.net_unit,
                false,
                &settings.plugin_settings,
                actions_arc,
            );
            *cached_hash = settings_hash;
        }
    }

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
    /// Last known modification time of the macros file.
    last_modified: Mutex<SystemTime>,
}

impl MacrosPlugin {
    pub fn new() -> Self {
        let data = Arc::new(Mutex::new(load_macros(MACROS_FILE).unwrap_or_default()));
        let modified = std::fs::metadata(MACROS_FILE)
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        Self {
            matcher: SkimMatcherV2::default(),
            data,
            last_modified: Mutex::new(modified),
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
        // Reload macros if the source file has changed since the last check.
        if let Ok(meta) = std::fs::metadata(MACROS_FILE) {
            if let Ok(modified) = meta.modified() {
                let mut last = self.last_modified.lock().unwrap();
                if *last != modified {
                    if let Ok(list) = load_macros(MACROS_FILE) {
                        if let Ok(mut data) = self.data.lock() {
                            *data = list;
                        }
                    }
                    *last = modified;
                }
            }
        }
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
