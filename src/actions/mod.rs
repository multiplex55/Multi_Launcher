use crate::common::persistence::{LoadState, PersistenceError, load_json, save_json_atomic};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

static ACTIONS_VERSION: AtomicU64 = AtomicU64::new(0);
static ACTIONS_TRANSACTION: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Action {
    pub label: String,
    pub desc: String,
    pub action: String, // Path to folder or exe
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<String>,
}

pub fn load_actions_typed(
    path: impl AsRef<Path>,
) -> Result<LoadState<Vec<Action>>, PersistenceError> {
    load_json(path)
}

pub fn load_actions(path: &str) -> anyhow::Result<Vec<Action>> {
    load_effective(path).map_err(Into::into)
}

pub fn save_actions(path: &str, actions: &[Action]) -> anyhow::Result<()> {
    let replacement = actions.to_vec();
    update_actions(path, move |current| {
        *current = replacement;
        Ok(())
    })
    .map(|_| ())
}

/// Serialize an actions read-modify-write transaction and return only the
/// durably committed custom actions.
pub fn update_actions(
    path: &str,
    mutate: impl FnOnce(&mut Vec<Action>) -> anyhow::Result<()>,
) -> anyhow::Result<Vec<Action>> {
    let _transaction = transaction_guard();
    let mut actions = load_effective(path)?;
    mutate(&mut actions)?;
    save_json_atomic(path, &actions)?;
    bump_actions_version();
    Ok(actions)
}

fn load_effective(path: impl AsRef<Path>) -> Result<Vec<Action>, PersistenceError> {
    match load_actions_typed(path)? {
        LoadState::Missing | LoadState::Empty => Ok(Vec::new()),
        LoadState::Loaded(actions) => Ok(actions),
    }
}

fn transaction_guard() -> MutexGuard<'static, ()> {
    ACTIONS_TRANSACTION
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub fn actions_version() -> u64 {
    ACTIONS_VERSION.load(Ordering::SeqCst)
}

pub fn bump_actions_version() {
    ACTIONS_VERSION.fetch_add(1, Ordering::SeqCst);
}

#[derive(Debug)]
pub struct StartupActions {
    pub actions: Vec<Action>,
    pub diagnostic: Option<PersistenceError>,
}

/// Load startup actions without allowing invalid persisted data to become a
/// replacement candidate.
pub fn load_startup_actions(path: impl AsRef<Path>) -> StartupActions {
    match load_actions_typed(path) {
        Ok(LoadState::Missing | LoadState::Empty) => StartupActions {
            actions: Vec::new(),
            diagnostic: None,
        },
        Ok(LoadState::Loaded(actions)) => StartupActions {
            actions,
            diagnostic: None,
        },
        Err(error) => StartupActions {
            actions: Vec::new(),
            diagnostic: Some(error),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};

    static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    fn action(label: &str) -> Action {
        Action {
            label: label.into(),
            desc: format!("{label} description"),
            action: format!("{label}:action"),
            args: Some(format!("{label} args")),
        }
    }

    #[test]
    fn typed_load_distinguishes_missing_empty_loaded_malformed_and_unreadable() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("missing.json");
        assert!(matches!(
            load_actions_typed(&missing).unwrap(),
            LoadState::Missing
        ));

        let empty = directory.path().join("empty.json");
        std::fs::write(&empty, " \r\n\t").unwrap();
        assert!(matches!(
            load_actions_typed(&empty).unwrap(),
            LoadState::Empty
        ));

        let valid = directory.path().join("valid.json");
        std::fs::write(&valid, serde_json::to_vec(&vec![action("valid")]).unwrap()).unwrap();
        assert!(matches!(
            load_actions_typed(&valid).unwrap(),
            LoadState::Loaded(actions) if actions == vec![action("valid")]
        ));

        let malformed = directory.path().join("malformed.json");
        std::fs::write(&malformed, "{broken").unwrap();
        assert!(matches!(
            load_actions_typed(&malformed).unwrap_err(),
            PersistenceError::MalformedJson { .. }
        ));
        assert!(matches!(
            load_actions_typed(directory.path()).unwrap_err(),
            PersistenceError::Read { .. }
        ));
    }

    #[test]
    fn missing_first_mutation_creates_pretty_compatible_json() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("actions.json");
        let committed = vec![action("first")];
        save_actions(path.to_str().unwrap(), &committed).unwrap();

        assert_eq!(committed, vec![action("first")]);
        assert_eq!(load_actions(path.to_str().unwrap()).unwrap(), committed);
        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            serde_json::to_string_pretty(&committed).unwrap()
        );
    }

    #[test]
    fn malformed_mutation_is_rejected_without_changing_bytes_or_version() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("actions.json");
        let invalid = b"[{ not an action";
        std::fs::write(&path, invalid).unwrap();
        let version = actions_version();

        let result = save_actions(path.to_str().unwrap(), &[action("lost")]);

        assert!(result.is_err());
        assert_eq!(std::fs::read(path).unwrap(), invalid);
        assert_eq!(actions_version(), version);
    }

    #[test]
    fn failed_persistence_does_not_bump_version() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let blocker = directory.path().join("not-a-directory");
        std::fs::write(&blocker, "unchanged").unwrap();
        let path = blocker.join("actions.json");
        let version = actions_version();

        assert!(
            update_actions(path.to_str().unwrap(), |actions| {
                actions.push(action("lost"));
                Ok(())
            })
            .is_err()
        );
        assert_eq!(actions_version(), version);
        assert_eq!(std::fs::read_to_string(blocker).unwrap(), "unchanged");
    }

    #[test]
    fn concurrent_logical_mutations_both_survive() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = Arc::new(
            directory
                .path()
                .join("actions.json")
                .to_string_lossy()
                .into_owned(),
        );
        let barrier = Arc::new(Barrier::new(3));
        let first = {
            let path = Arc::clone(&path);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                update_actions(&path, |actions| {
                    actions.push(action("first"));
                    Ok(())
                })
                .unwrap();
            })
        };
        let second = {
            let path = Arc::clone(&path);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                update_actions(&path, |actions| {
                    actions.push(action("second"));
                    Ok(())
                })
                .unwrap();
            })
        };

        barrier.wait();
        first.join().unwrap();
        second.join().unwrap();
        let committed = load_actions(&path).unwrap();
        assert_eq!(committed.len(), 2);
        assert!(committed.contains(&action("first")));
        assert!(committed.contains(&action("second")));
    }

    #[test]
    fn startup_invalid_uses_temporary_empty_without_rewriting() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("actions.json");
        let invalid = b"invalid actions";
        std::fs::write(&path, invalid).unwrap();

        let startup = load_startup_actions(&path);

        assert!(startup.actions.is_empty());
        assert!(matches!(
            startup.diagnostic,
            Some(PersistenceError::MalformedJson { .. })
        ));
        assert_eq!(std::fs::read(path).unwrap(), invalid);
    }
}

pub mod bookmarks;
pub mod calc;
pub mod clipboard;
pub mod exec;
pub mod fav;
pub mod folders;
pub mod history;
pub mod keys;
pub mod layout;
pub mod media;
pub mod screenshot;
pub mod shell;
pub mod snippets;
pub mod stopwatch;
pub mod system;
pub mod tempfiles;
pub mod timer;
pub mod todo;
