use crate::actions::Action;
use crate::common::json_watch::{JsonWatcher, watch_json};
use crate::common::persistence::{LoadState, PersistenceError, load_json, save_json_atomic};
use crate::launcher::launch_action;
use crate::plugin::Plugin;
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex, MutexGuard,
    atomic::{AtomicU64, Ordering},
};

pub const FAV_FILE: &str = "fav.json";

static FAV_VERSION: AtomicU64 = AtomicU64::new(0);
static FAV_TRANSACTION: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));
static VERSIONED_FAVS: Lazy<Mutex<Option<(PathBuf, Vec<FavEntry>)>>> =
    Lazy::new(|| Mutex::new(None));

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct FavEntry {
    pub label: String,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<String>,
}

pub fn load_favs(path: &str) -> anyhow::Result<Vec<FavEntry>> {
    match load_favs_typed(path)? {
        LoadState::Missing | LoadState::Empty => Ok(Vec::new()),
        LoadState::Loaded(favs) => Ok(favs),
    }
}

pub fn load_favs_typed(
    path: impl AsRef<Path>,
) -> Result<LoadState<Vec<FavEntry>>, PersistenceError> {
    load_json(path)
}

pub(crate) fn load_favs_for_reload(
    path: impl AsRef<Path>,
) -> Result<LoadState<Vec<FavEntry>>, PersistenceError> {
    let _transaction = fav_transaction_guard();
    load_favs_typed(path)
}

pub fn save_favs(path: &str, favs: &[FavEntry]) -> anyhow::Result<()> {
    replace_favs(path, favs.to_vec()).map(|_| ())
}

pub fn replace_favs(path: &str, replacement: Vec<FavEntry>) -> anyhow::Result<Vec<FavEntry>> {
    update_favs(path, move |favs| {
        *favs = replacement;
        Ok(true)
    })
}

pub fn update_favs(
    path: &str,
    mutate: impl FnOnce(&mut Vec<FavEntry>) -> anyhow::Result<bool>,
) -> anyhow::Result<Vec<FavEntry>> {
    update_favs_with_save(path, mutate, |path, favs| {
        save_json_atomic(path, favs).map_err(Into::into)
    })
}

fn update_favs_with_save(
    path: &str,
    mutate: impl FnOnce(&mut Vec<FavEntry>) -> anyhow::Result<bool>,
    save: impl FnOnce(&str, &[FavEntry]) -> anyhow::Result<()>,
) -> anyhow::Result<Vec<FavEntry>> {
    let _transaction = fav_transaction_guard();
    let mut favs = load_favs(path)?;
    if mutate(&mut favs)? {
        save(path, &favs)?;
        record_versioned_favs(path, &favs);
    }
    Ok(favs)
}

fn fav_transaction_guard() -> MutexGuard<'static, ()> {
    FAV_TRANSACTION
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub fn set_fav(path: &str, label: &str, action: &str, args: Option<&str>) -> anyhow::Result<()> {
    let label = label.to_owned();
    let action = action.to_owned();
    let args = args.map(str::to_owned);
    update_favs(path, move |list| {
        if let Some(item) = list
            .iter_mut()
            .find(|entry| entry.label.eq_ignore_ascii_case(&label))
        {
            if item.action == action && item.args == args {
                return Ok(false);
            }
            item.action = action;
            item.args = args;
        } else {
            list.push(FavEntry {
                label,
                action,
                args,
            });
        }
        Ok(true)
    })?;
    Ok(())
}

pub fn remove_fav(path: &str, label: &str) -> anyhow::Result<()> {
    let label = label.to_owned();
    update_favs(path, move |list| {
        let Some(pos) = list
            .iter()
            .position(|entry| entry.label.eq_ignore_ascii_case(&label))
        else {
            return Ok(false);
        };
        list.remove(pos);
        Ok(true)
    })?;
    Ok(())
}

pub fn fav_version() -> u64 {
    FAV_VERSION.load(Ordering::SeqCst)
}

fn bump_fav_version() {
    FAV_VERSION.fetch_add(1, Ordering::SeqCst);
}

fn record_versioned_favs(path: &str, favs: &[FavEntry]) {
    *VERSIONED_FAVS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) =
        Some((PathBuf::from(path), favs.to_vec()));
    bump_fav_version();
}

fn bump_for_external_favs(path: &str, favs: &[FavEntry]) {
    let mut versioned = VERSIONED_FAVS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if versioned
        .as_ref()
        .is_some_and(|(saved_path, saved)| saved_path == Path::new(path) && saved == favs)
    {
        return;
    }
    *versioned = Some((PathBuf::from(path), favs.to_vec()));
    bump_fav_version();
}

/// Resolve a command and optional arguments against a plugin's search results.
///
/// The `command` and `args` are concatenated and passed to `plugin.search`.
/// If the plugin returns a result, its `action` and `args` are used; otherwise
/// the original `command` and `args` are returned unchanged.
pub fn resolve_with_plugin(
    plugin: &dyn Plugin,
    command: &str,
    args: Option<&str>,
) -> (String, Option<String>) {
    let query = join_command_args(command, args);
    if let Some(res) = plugin.search(&query).into_iter().next() {
        (res.action, res.args)
    } else {
        (command.to_string(), args.map(|s| s.to_string()))
    }
}

pub fn join_command_args(command: &str, args: Option<&str>) -> String {
    let command = command.trim_end();
    let Some(args) = args else {
        return command.to_string();
    };

    let args = args.trim_start();
    if args.is_empty() {
        command.to_string()
    } else {
        format!("{command} {args}")
    }
}

pub fn run_fav(label: &str) -> anyhow::Result<()> {
    let list = load_favs(FAV_FILE)?;
    if let Some(entry) = list.iter().find(|e| e.label.eq_ignore_ascii_case(label)) {
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

pub struct FavPlugin {
    matcher: SkimMatcherV2,
    data: Arc<Mutex<Vec<FavEntry>>>,
    #[allow(dead_code)]
    watcher: Option<JsonWatcher>,
}

impl FavPlugin {
    pub fn new() -> Self {
        let startup = load_favs(FAV_FILE).unwrap_or_else(|error| {
            tracing::error!(%error, "favorite startup retained invalid persisted file");
            Vec::new()
        });
        let data = Arc::new(Mutex::new(startup.clone()));
        *VERSIONED_FAVS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            Some((PathBuf::from(FAV_FILE), startup));
        let data_clone = data.clone();
        let path = FAV_FILE.to_string();
        let watch_path = path.clone();
        let watcher = watch_json(&watch_path, {
            let watch_path = watch_path.clone();
            move || {
                if let Err(error) = reload_fav_snapshot(&watch_path, &data_clone) {
                    tracing::error!(%error, "invalid favorite reload retained last-good state");
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
            .filter(|f| self.matcher.fuzzy_match(&f.label, filter).is_some())
            .map(|f| Action {
                label: f.label.clone(),
                desc: "Fav".into(),
                action: f.action.clone(),
                args: f.args.clone(),
            })
            .collect()
    }
}

fn reload_fav_snapshot(path: &str, data: &Arc<Mutex<Vec<FavEntry>>>) -> anyhow::Result<()> {
    let _transaction = fav_transaction_guard();
    let favs = match load_favs_typed(path)? {
        LoadState::Missing => {
            anyhow::bail!("favorites file was removed; retaining last-good state")
        }
        LoadState::Empty => Vec::new(),
        LoadState::Loaded(favs) => favs,
    };
    let mut current = data
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if *current == favs {
        return Ok(());
    }
    *current = favs.clone();
    drop(current);
    bump_for_external_favs(path, &favs);
    Ok(())
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
                label: "Favorites".into(),
                desc: "Fav".into(),
                action: "fav:dialog:".into(),
                args: None,
            }];
        }

        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "fav add") {
            let label = rest.trim();
            return vec![Action {
                label: if label.is_empty() {
                    "fav: add".into()
                } else {
                    format!("Add fav {label}")
                },
                desc: "Fav".into(),
                action: format!("fav:dialog:{label}"),
                args: None,
            }];
        }

        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "fav rm") {
            let filter = rest.trim();
            let guard = match self.data.lock() {
                Ok(g) => g,
                Err(_) => return Vec::new(),
            };
            return guard
                .iter()
                .filter(|f| self.matcher.fuzzy_match(&f.label, filter).is_some())
                .map(|f| Action {
                    label: format!("Remove fav {}", f.label),
                    desc: "Fav".into(),
                    action: format!("fav:remove:{}", f.label),
                    args: None,
                })
                .collect();
        }

        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "fav list") {
            return self.list(rest.trim());
        }

        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "fav ") {
            return self.list(rest.trim());
        }

        Vec::new()
    }

    fn name(&self) -> &str {
        "favorites"
    }

    fn description(&self) -> &str {
        "Run saved favorite commands (prefix: `fav`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "fav".into(),
                desc: "Fav".into(),
                action: "query:fav ".into(),
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

#[cfg(test)]
mod tests {
    use super::{join_command_args, resolve_with_plugin};
    use crate::{actions::Action, plugin::Plugin};

    struct TestPlugin;

    impl Plugin for TestPlugin {
        fn search(&self, query: &str) -> Vec<Action> {
            vec![Action {
                label: query.to_string(),
                desc: String::new(),
                action: query.to_string(),
                args: None,
            }]
        }

        fn name(&self) -> &str {
            "test"
        }

        fn description(&self) -> &str {
            "test plugin"
        }

        fn capabilities(&self) -> &[&str] {
            &["search"]
        }

        fn commands(&self) -> Vec<Action> {
            Vec::new()
        }
    }

    #[test]
    fn join_command_and_args_for_tokenized_query() {
        assert_eq!(join_command_args("todo", Some("list")), "todo list");
    }

    #[test]
    fn join_keeps_command_when_args_empty() {
        assert_eq!(join_command_args("todo", None), "todo");
        assert_eq!(join_command_args("todo", Some("   ")), "todo");
    }

    #[test]
    fn join_normalizes_whitespace_between_command_and_args() {
        assert_eq!(
            join_command_args("todo   ", Some("   list now")),
            "todo list now"
        );
    }

    #[test]
    fn resolve_with_plugin_uses_safe_joined_query() {
        let plugin = TestPlugin;
        let (action, args) = resolve_with_plugin(&plugin, "todo  ", Some("  list"));
        assert_eq!(action, "todo list");
        assert!(args.is_none());
    }
}

#[cfg(test)]
mod persistence_tests {
    use super::*;
    use std::sync::Barrier;

    static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    fn fav(label: &str, action: &str, args: Option<&str>) -> FavEntry {
        FavEntry {
            label: label.into(),
            action: action.into(),
            args: args.map(str::to_owned),
        }
    }

    #[test]
    fn typed_load_and_pretty_schema_cover_all_file_states() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("missing.json");
        assert_eq!(load_favs_typed(&missing).unwrap(), LoadState::Missing);
        let empty = directory.path().join("empty.json");
        std::fs::write(&empty, " \r\n\t").unwrap();
        assert_eq!(load_favs_typed(&empty).unwrap(), LoadState::Empty);
        let expected = vec![fav("build", "cargo", Some("test --all"))];
        let valid = directory.path().join("valid.json");
        std::fs::write(&valid, serde_json::to_vec(&expected).unwrap()).unwrap();
        assert_eq!(
            load_favs_typed(&valid).unwrap(),
            LoadState::Loaded(expected.clone())
        );
        let saved = directory.path().join("saved.json");
        save_favs(saved.to_str().unwrap(), &expected).unwrap();
        assert_eq!(
            std::fs::read_to_string(saved).unwrap(),
            serde_json::to_string_pretty(&expected).unwrap()
        );
        let malformed = directory.path().join("malformed.json");
        std::fs::write(&malformed, "{broken").unwrap();
        assert!(matches!(
            load_favs_typed(&malformed).unwrap_err(),
            PersistenceError::MalformedJson { .. }
        ));
        assert!(matches!(
            load_favs_typed(directory.path()).unwrap_err(),
            PersistenceError::Read { .. }
        ));
    }

    #[test]
    fn malformed_file_rejects_add_edit_remove_and_replacement_unchanged() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fav.json");
        let invalid = b"not favorites JSON";
        std::fs::write(&path, invalid).unwrap();
        let path = path.to_str().unwrap();
        for result in [
            set_fav(path, "new", "noop:new", None),
            set_fav(path, "existing", "noop:edited", Some("args")),
            remove_fav(path, "existing"),
            save_favs(path, &[fav("replacement", "noop:lost", None)]),
        ] {
            assert!(result.is_err());
            assert_eq!(std::fs::read(path).unwrap(), invalid);
        }
    }

    #[test]
    fn concurrent_mutations_both_survive() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = Arc::new(
            directory
                .path()
                .join("fav.json")
                .to_string_lossy()
                .into_owned(),
        );
        let barrier = Arc::new(Barrier::new(3));
        let first = {
            let path = Arc::clone(&path);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                set_fav(&path, "first", "noop:first", None).unwrap();
            })
        };
        let second = {
            let path = Arc::clone(&path);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                set_fav(&path, "second", "noop:second", Some("args")).unwrap();
            })
        };
        barrier.wait();
        first.join().unwrap();
        second.join().unwrap();
        let committed = load_favs(&path).unwrap();
        assert!(committed.contains(&fav("first", "noop:first", None)));
        assert!(committed.contains(&fav("second", "noop:second", Some("args"))));
    }

    #[test]
    fn failed_save_retains_destination_and_version() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fav.json");
        let original = vec![fav("saved", "noop:saved", None)];
        std::fs::write(&path, serde_json::to_vec_pretty(&original).unwrap()).unwrap();
        let version = fav_version();
        let result = update_favs_with_save(
            path.to_str().unwrap(),
            |favs| {
                favs.push(fav("lost", "noop:lost", None));
                Ok(true)
            },
            |_path, _favs| anyhow::bail!("deterministic replacement failure"),
        );
        assert!(result.is_err());
        assert_eq!(load_favs(path.to_str().unwrap()).unwrap(), original);
        assert_eq!(fav_version(), version);
    }

    #[test]
    fn watcher_retains_invalid_recovers_and_does_not_double_bump_local_save() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fav.json");
        let path_text = path.to_str().unwrap();
        let initial = vec![fav("initial", "noop:initial", None)];
        std::fs::write(&path, serde_json::to_vec_pretty(&initial).unwrap()).unwrap();
        let data = Arc::new(Mutex::new(initial.clone()));
        let local = vec![fav("local", "noop:local", Some("args"))];
        save_favs(path_text, &local).unwrap();
        let after_local = fav_version();
        reload_fav_snapshot(path_text, &data).unwrap();
        assert_eq!(*data.lock().unwrap(), local);
        assert_eq!(fav_version(), after_local);

        std::fs::write(&path, "invalid").unwrap();
        assert!(reload_fav_snapshot(path_text, &data).is_err());
        assert_eq!(*data.lock().unwrap(), local);
        assert_eq!(fav_version(), after_local);

        std::fs::remove_file(&path).unwrap();
        assert!(reload_fav_snapshot(path_text, &data).is_err());
        assert_eq!(*data.lock().unwrap(), local);
        assert_eq!(fav_version(), after_local);

        let external = vec![fav("external", "noop:external", None)];
        std::fs::write(&path, serde_json::to_vec_pretty(&external).unwrap()).unwrap();
        reload_fav_snapshot(path_text, &data).unwrap();
        assert_eq!(*data.lock().unwrap(), external);
        assert_eq!(fav_version(), after_local + 1);
    }
}
