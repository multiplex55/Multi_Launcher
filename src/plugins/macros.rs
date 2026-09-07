use crate::actions::{Action, load_actions};
use crate::common::json_watch::{JsonWatcher, watch_json};
use crate::common::persistence::{LoadState, PersistenceError, load_json, save_json_atomic};
use crate::launcher::launch_action;
use crate::plugin::{Plugin, PluginManager};
use crate::settings::Settings;
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use once_cell::sync::{Lazy, OnceCell};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

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
static MACROS_TRANSACTION: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));
static LIVE_MACROS: Lazy<super::live_snapshot::LiveSnapshotRegistry<MacroEntry>> =
    Lazy::new(super::live_snapshot::LiveSnapshotRegistry::new);
static SEARCH_RUNTIME: Lazy<Mutex<LegacyMacroSearchRuntime>> =
    Lazy::new(|| Mutex::new(LegacyMacroSearchRuntime::default()));

#[derive(Clone)]
struct LegacyMacroSearchRuntime {
    settings: Settings,
    actions_path: String,
}

impl Default for LegacyMacroSearchRuntime {
    fn default() -> Self {
        Self {
            settings: Settings::default(),
            actions_path: "actions.json".into(),
        }
    }
}

/// Publish the already-committed runtime configuration used by legacy macro
/// step resolution. Macro execution must not re-read settings from disk.
pub fn configure_search_runtime(settings: &Settings, actions_path: &str) {
    *SEARCH_RUNTIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = LegacyMacroSearchRuntime {
        settings: settings.clone(),
        actions_path: actions_path.to_owned(),
    };
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
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
    match load_macros_typed(path)? {
        LoadState::Missing | LoadState::Empty => Ok(Vec::new()),
        LoadState::Loaded(macros) => Ok(macros),
    }
}

pub fn load_macros_typed(
    path: impl AsRef<Path>,
) -> Result<LoadState<Vec<MacroEntry>>, PersistenceError> {
    load_json(path)
}

pub fn save_macros(path: &str, macros: &[MacroEntry]) -> anyhow::Result<()> {
    replace_macros(path, macros.to_vec()).map(|_| ())
}

pub fn replace_macros(path: &str, replacement: Vec<MacroEntry>) -> anyhow::Result<Vec<MacroEntry>> {
    update_macros(path, move |macros| {
        *macros = replacement;
        Ok(true)
    })
}

pub fn update_macros(
    path: &str,
    mutate: impl FnOnce(&mut Vec<MacroEntry>) -> anyhow::Result<bool>,
) -> anyhow::Result<Vec<MacroEntry>> {
    update_macros_with_save(path, mutate, |path, macros| {
        save_json_atomic(path, macros).map_err(Into::into)
    })
}

fn update_macros_with_save(
    path: &str,
    mutate: impl FnOnce(&mut Vec<MacroEntry>) -> anyhow::Result<bool>,
    save: impl FnOnce(&str, &[MacroEntry]) -> anyhow::Result<()>,
) -> anyhow::Result<Vec<MacroEntry>> {
    let _transaction = macros_transaction_guard();
    let mut macros = load_macros(path)?;
    if mutate(&mut macros)? {
        save(path, &macros)?;
        LIVE_MACROS.publish(path, &macros);
    }
    Ok(macros)
}

fn macros_transaction_guard() -> MutexGuard<'static, ()> {
    MACROS_TRANSACTION
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub fn take_step_messages() -> Vec<String> {
    if let Ok(mut list) = STEP_MESSAGES.lock() {
        let out = list.clone();
        list.clear();
        out
    } else {
        Vec::new()
    }
}

pub fn take_error_messages() -> Vec<String> {
    if let Ok(mut list) = ERROR_MESSAGES.lock() {
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
    let runtime = SEARCH_RUNTIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    let settings = runtime.settings;
    let actions = match load_actions(&runtime.actions_path) {
        Ok(actions) => actions,
        Err(error) => {
            tracing::error!(%error, "macro action lookup retained invalid actions file");
            return None;
        }
    };
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
    if let Ok(mut cached_hash) = hash_cell.lock()
        && *cached_hash != settings_hash
    {
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

    pm.search_filtered(
        query,
        settings.enabled_plugins.as_ref(),
        settings.enabled_capabilities.as_ref(),
    )
    .into_iter()
    .next()
}

pub fn run_macro(name: &str) -> anyhow::Result<()> {
    let list = load_macros(MACROS_FILE)?;
    if let Some(entry) = list.iter().find(|m| m.label.eq_ignore_ascii_case(name)) {
        for (i, step) in entry.steps.iter().enumerate() {
            let mut command = step.command.trim().to_string();
            let mut args = step.args.clone();
            if let Some(ref s) = args
                && s.trim().is_empty()
            {
                args = None;
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
                if let Ok(mut errs) = ERROR_MESSAGES.lock() {
                    errs.push(format!("Step {} error: {e}", i + 1));
                }
            }
            if let Ok(mut msgs) = STEP_MESSAGES.lock() {
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
        Self::new_for_path(MACROS_FILE)
    }

    fn new_for_path(path: &str) -> Self {
        let data = {
            let _transaction = macros_transaction_guard();
            let startup = match load_macros(path) {
                Ok(macros) => Some(macros),
                Err(error) => {
                    tracing::error!(%error, "macro startup retained invalid persisted file");
                    None
                }
            };
            LIVE_MACROS.get_or_create(path, startup)
        };
        let data_clone = data.clone();
        let path = path.to_string();
        let watch_path = path.clone();
        let watcher = watch_json(&watch_path, {
            let watch_path = watch_path.clone();
            move || {
                if let Err(error) = reload_macro_snapshot(&watch_path, &data_clone) {
                    tracing::error!(%error, "invalid macro reload retained last-good state");
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

fn reload_macro_snapshot(path: &str, data: &Arc<Mutex<Vec<MacroEntry>>>) -> anyhow::Result<()> {
    let _transaction = macros_transaction_guard();
    let macros = match load_macros_typed(path)? {
        LoadState::Missing => anyhow::bail!("macros file was removed; retaining last-good state"),
        LoadState::Empty => Vec::new(),
        LoadState::Loaded(macros) => macros,
    };
    let mut current = data
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if *current != macros {
        *current = macros;
    }
    Ok(())
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

#[cfg(test)]
mod legacy_contract_tests {
    use super::*;
    #[test]
    fn legacy_macro_contract_remains_unchanged() {
        assert_eq!(MACROS_FILE, "macros.json");
        let step = MacroStep {
            label: "step".into(),
            command: "macro:child".into(),
            args: None,
            delay_ms: 0,
        };
        let entry = MacroEntry {
            label: "legacy".into(),
            desc: String::new(),
            auto_delay_ms: None,
            steps: vec![step],
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("macro:child"));
        // The plugin's registration name is plural, while its launcher command
        // and generated action routing intentionally use the singular prefix.
        let plugin = MacrosPlugin::new();
        assert_eq!(plugin.name(), "macros");
        assert_eq!(plugin.commands()[0].action, "query:macro ");
        assert_eq!(plugin.commands()[1].action, "query:macro list");
        assert_eq!(plugin.search("macro")[0].action, "macro:dialog");
    }
}

#[cfg(test)]
mod persistence_tests {
    use super::*;
    use std::sync::Barrier;

    static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    fn entry(label: &str) -> MacroEntry {
        MacroEntry {
            label: label.into(),
            desc: format!("{label} description"),
            auto_delay_ms: None,
            steps: vec![MacroStep {
                label: "step".into(),
                command: "history:clear".into(),
                args: None,
                delay_ms: 0,
            }],
        }
    }

    #[test]
    fn typed_states_defaults_and_legacy_filename_remain_compatible() {
        let _guard = TEST_MUTEX.lock().unwrap();
        assert_eq!(MACROS_FILE, "macros.json");
        assert_ne!(MACROS_FILE, crate::mkmacro::MKMACROS_FILE);
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join(MACROS_FILE);
        assert_eq!(load_macros_typed(&missing).unwrap(), LoadState::Missing);
        let empty = directory.path().join("empty.json");
        std::fs::write(&empty, " \r\n").unwrap();
        assert_eq!(load_macros_typed(&empty).unwrap(), LoadState::Empty);
        update_macros(empty.to_str().unwrap(), |macros| {
            macros.push(entry("initialized"));
            Ok(true)
        })
        .unwrap();
        assert_eq!(load_macros(empty.to_str().unwrap()).unwrap().len(), 1);
        let defaults = directory.path().join("defaults.json");
        std::fs::write(
            &defaults,
            r#"[{"label":"old","desc":"legacy","steps":[{"label":"step","command":"history:clear"}]}]"#,
        )
        .unwrap();
        let loaded = load_macros(defaults.to_str().unwrap()).unwrap();
        assert_eq!(loaded[0].auto_delay_ms, None);
        assert_eq!(loaded[0].steps[0].delay_ms, 0);
        let saved = directory.path().join("nested").join(MACROS_FILE);
        save_macros(saved.to_str().unwrap(), &loaded).unwrap();
        assert_eq!(
            std::fs::read_to_string(saved).unwrap(),
            serde_json::to_string_pretty(&loaded).unwrap()
        );
        let malformed = directory.path().join("malformed.json");
        std::fs::write(&malformed, "{").unwrap();
        assert!(matches!(
            load_macros_typed(&malformed).unwrap_err(),
            PersistenceError::MalformedJson { .. }
        ));
        assert!(matches!(
            load_macros_typed(directory.path()).unwrap_err(),
            PersistenceError::Read { .. }
        ));
    }

    #[test]
    fn malformed_store_rejects_update_and_full_replacement_unchanged() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(MACROS_FILE);
        let invalid = b"invalid legacy macros";
        std::fs::write(&path, invalid).unwrap();
        let path = path.to_str().unwrap();
        assert!(
            update_macros(path, |macros| {
                macros.push(entry("lost"));
                Ok(true)
            })
            .is_err()
        );
        assert_eq!(std::fs::read(path).unwrap(), invalid);
        assert!(save_macros(path, &[entry("replacement")]).is_err());
        assert_eq!(std::fs::read(path).unwrap(), invalid);
        assert!(update_macros(directory.path().to_str().unwrap(), |_| Ok(true)).is_err());
        assert!(save_macros(directory.path().to_str().unwrap(), &[entry("replacement")]).is_err());
    }

    #[test]
    fn concurrent_updates_both_survive() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = Arc::new(
            directory
                .path()
                .join(MACROS_FILE)
                .to_string_lossy()
                .into_owned(),
        );
        let barrier = Arc::new(Barrier::new(3));
        let handles = ["first", "second"].map(|label| {
            let path = Arc::clone(&path);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                update_macros(&path, |macros| {
                    macros.push(entry(label));
                    Ok(true)
                })
                .unwrap();
            })
        });
        barrier.wait();
        for handle in handles {
            handle.join().unwrap();
        }
        let macros = load_macros(&path).unwrap();
        assert!(macros.contains(&entry("first")));
        assert!(macros.contains(&entry("second")));
    }

    #[test]
    fn failed_save_and_invalid_watcher_retain_last_good_then_recover() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(MACROS_FILE);
        let initial = vec![entry("initial")];
        std::fs::write(&path, serde_json::to_vec_pretty(&initial).unwrap()).unwrap();
        let data = Arc::new(Mutex::new(initial.clone()));
        let result = update_macros_with_save(
            path.to_str().unwrap(),
            |macros| {
                macros.push(entry("lost"));
                Ok(true)
            },
            |_path, _macros| anyhow::bail!("deterministic save failure"),
        );
        assert!(result.is_err());
        assert_eq!(*data.lock().unwrap(), initial);
        assert_eq!(load_macros(path.to_str().unwrap()).unwrap(), initial);

        std::fs::write(&path, "invalid").unwrap();
        assert!(reload_macro_snapshot(path.to_str().unwrap(), &data).is_err());
        assert_eq!(*data.lock().unwrap(), initial);
        std::fs::remove_file(&path).unwrap();
        assert!(reload_macro_snapshot(path.to_str().unwrap(), &data).is_err());
        assert_eq!(*data.lock().unwrap(), initial);
        let recovered = vec![entry("recovered")];
        std::fs::write(&path, serde_json::to_vec_pretty(&recovered).unwrap()).unwrap();
        reload_macro_snapshot(path.to_str().unwrap(), &data).unwrap();
        assert_eq!(*data.lock().unwrap(), recovered);
    }

    #[test]
    fn committed_mutation_is_visible_to_all_instances_without_watcher_delivery() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(MACROS_FILE);
        let path = path.to_str().unwrap();
        save_macros(path, &[]).unwrap();
        let first = MacrosPlugin::new_for_path(path);
        let second = MacrosPlugin::new_for_path(path);

        save_macros(path, &[entry("immediate")]).unwrap();

        for plugin in [&first, &second] {
            assert!(plugin.search("macro list immediate").iter().any(|action| {
                action.label == "immediate" && action.action == "macro:immediate"
            }));
        }
    }

    #[test]
    fn search_uses_committed_runtime_settings_and_custom_actions_path_after_disk_corruption() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let settings_path = directory.path().join("custom-settings.json");
        let actions_path = directory.path().join("custom-actions.json");
        let action = Action {
            label: "Runtime action".into(),
            desc: "custom".into(),
            action: "runtime:action".into(),
            args: None,
        };
        crate::actions::save_actions(
            actions_path.to_str().unwrap(),
            std::slice::from_ref(&action),
        )
        .unwrap();
        let mut settings = Settings::default();
        settings.enabled_plugins = Some(["omni_search".to_string()].into_iter().collect());
        settings.save(settings_path.to_str().unwrap()).unwrap();
        configure_search_runtime(&settings, actions_path.to_str().unwrap());
        std::fs::write(&settings_path, "corrupt after startup").unwrap();

        let resolved =
            search_first_action("o Runtime action").expect("runtime action remains enabled");
        assert_eq!(resolved.action, action.action);
        assert!(
            search_first_action("help").is_none(),
            "plugins disabled in committed runtime settings must stay disabled"
        );
    }
}
