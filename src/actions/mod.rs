pub mod search;

use crate::common::persistence::{LoadState, PersistenceError, load_json, save_json_atomic};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

static ACTIONS_VERSION: AtomicU64 = AtomicU64::new(0);
static ACTIONS_TRANSACTION: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Action {
    pub label: String,
    pub desc: String,
    pub action: String, // Path to folder or exe
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<String>,
}

/// A validated request to append one custom action.
#[derive(Debug, Clone)]
pub struct AddAction(Action);

impl AddAction {
    pub fn new(
        label: &str,
        desc: &str,
        path: &str,
        show_args: bool,
        args: &str,
    ) -> Result<Self, &'static str> {
        validated_action(label, desc, path, show_args, args).map(Self)
    }

    /// Apply this request to the current custom catalog without deduplication.
    pub fn append_to(self, actions: &mut Vec<Action>) -> anyhow::Result<()> {
        actions.push(self.0);
        Ok(())
    }

    /// Commit this request through the canonical actions transaction.
    pub fn commit(self, path: &str) -> anyhow::Result<Vec<Action>> {
        update_actions(path, |actions| self.append_to(actions))
    }
}

fn validated_action(
    label: &str,
    desc: &str,
    path: &str,
    show_args: bool,
    args: &str,
) -> Result<Action, &'static str> {
    if path.is_empty() || !Path::new(path).exists() {
        return Err("Path does not exist");
    }
    Ok(Action {
        label: label.to_owned(),
        desc: desc.to_owned(),
        action: path.to_owned(),
        args: (show_args && !args.trim().is_empty()).then(|| args.to_owned()),
    })
}

/// Identity of a presented custom row, including the positions of equal rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomActionRowTarget {
    index: usize,
    original: Action,
    matching_positions: Vec<usize>,
}

pub fn custom_action_row_target(actions: &[Action], index: usize) -> Option<CustomActionRowTarget> {
    let original = actions.get(index)?.clone();
    let matching_positions = actions
        .iter()
        .enumerate()
        .filter_map(|(i, action)| (action == &original).then_some(i))
        .collect();
    Some(CustomActionRowTarget {
        index,
        original,
        matching_positions,
    })
}

impl CustomActionRowTarget {
    fn is_current(&self, actions: &[Action]) -> bool {
        actions.get(self.index) == Some(&self.original)
            && actions
                .iter()
                .enumerate()
                .filter_map(|(i, action)| (action == &self.original).then_some(i))
                .eq(self.matching_positions.iter().copied())
    }
}

/// The selected row no longer has the identity captured by the frontend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaleCustomAction;

impl std::fmt::Display for StaleCustomAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("custom action selection is stale")
    }
}

impl std::error::Error for StaleCustomAction {}

#[derive(Debug)]
pub enum CheckedActionError {
    StaleTarget,
    Store(anyhow::Error),
}

impl std::fmt::Display for CheckedActionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleTarget => write!(f, "{StaleCustomAction}"),
            Self::Store(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for CheckedActionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::StaleTarget => None,
            Self::Store(error) => Some(error.as_ref()),
        }
    }
}

fn checked_commit(
    path: &str,
    apply: impl FnOnce(&mut Vec<Action>) -> Result<(), StaleCustomAction>,
) -> Result<Vec<Action>, CheckedActionError> {
    update_actions(path, |actions| apply(actions).map_err(anyhow::Error::new)).map_err(|error| {
        if error.is::<StaleCustomAction>() {
            CheckedActionError::StaleTarget
        } else {
            CheckedActionError::Store(error)
        }
    })
}

/// A validated replacement of one captured custom row.
#[derive(Debug, Clone)]
pub struct EditAction {
    target: CustomActionRowTarget,
    replacement: Action,
}

impl EditAction {
    pub fn new(
        target: CustomActionRowTarget,
        label: &str,
        desc: &str,
        path: &str,
        show_args: bool,
        args: &str,
    ) -> Result<Self, &'static str> {
        Ok(Self {
            target,
            replacement: validated_action(label, desc, path, show_args, args)?,
        })
    }

    pub fn apply_to(self, actions: &mut [Action]) -> Result<(), StaleCustomAction> {
        if !self.target.is_current(actions) {
            return Err(StaleCustomAction);
        }
        actions[self.target.index] = self.replacement;
        Ok(())
    }

    pub fn commit(self, path: &str) -> Result<Vec<Action>, CheckedActionError> {
        checked_commit(path, |actions| self.apply_to(actions))
    }
}

/// A checked immediate removal of one captured custom row.
#[derive(Debug, Clone)]
pub struct RemoveAction(CustomActionRowTarget);

impl RemoveAction {
    pub fn new(target: CustomActionRowTarget) -> Self {
        Self(target)
    }

    pub fn apply_to(self, actions: &mut Vec<Action>) -> Result<(), StaleCustomAction> {
        if !self.0.is_current(actions) {
            return Err(StaleCustomAction);
        }
        actions.remove(self.0.index);
        Ok(())
    }

    pub fn commit(self, path: &str) -> Result<Vec<Action>, CheckedActionError> {
        checked_commit(path, |actions| self.apply_to(actions))
    }
}

/// ID lookup for one immutable action catalog.
///
/// The lookup owns the catalog it indexes so a resolved position can never be
/// applied to a different publication. Duplicate IDs retain the existing
/// last-catalog-entry-wins behavior without keeping a second owned `Action`.
#[derive(Debug)]
pub struct ActionIdLookup {
    catalog: Arc<Vec<Action>>,
    positions: HashMap<String, usize>,
}

impl ActionIdLookup {
    /// Build a lookup bound to `catalog`, cloning ID keys but not action bodies.
    pub fn new(catalog: Arc<Vec<Action>>) -> Self {
        #[cfg(test)]
        {
            Self::build(catalog, || {})
        }
        #[cfg(not(test))]
        {
            Self::build(catalog)
        }
    }

    #[cfg(test)]
    pub(crate) fn new_with_test_receipt(
        catalog: Arc<Vec<Action>>,
    ) -> (Self, ActionIdLookupBuildReceipt) {
        let mut receipt = ActionIdLookupBuildReceipt::default();
        let lookup = Self::build(catalog, || {
            receipt.key_clone_attempts = receipt.key_clone_attempts.saturating_add(1);
        });
        (lookup, receipt)
    }

    fn build(catalog: Arc<Vec<Action>>, #[cfg(test)] mut record_key_clone: impl FnMut()) -> Self {
        let mut positions = HashMap::with_capacity(catalog.len());
        for (index, action) in catalog.iter().enumerate() {
            let id = action.action.clone();
            #[cfg(test)]
            record_key_clone();
            positions.insert(id, index);
        }
        Self { catalog, positions }
    }

    /// Resolve an ID to an action borrowed from this lookup's own catalog.
    pub fn get(&self, id: &str) -> Option<&Action> {
        let index = self.positions.get(id)?;
        self.catalog.get(*index)
    }

    /// Number of distinct action IDs in the catalog.
    pub fn len(&self) -> usize {
        self.positions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }

    /// Iterate over distinct IDs and their winning actions.
    ///
    /// Iteration order follows the hash table and is intentionally unspecified;
    /// action catalog order remains available from the owning catalog itself.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Action)> {
        self.positions
            .iter()
            .filter_map(|(id, index)| self.catalog.get(*index).map(|action| (id.as_str(), action)))
    }
}

#[cfg(test)]
#[derive(Debug, Default)]
pub(crate) struct ActionIdLookupBuildReceipt {
    /// Number of ID `String` clone attempts made by the constructor loop,
    /// including attempts whose duplicate key was immediately replaced.
    pub(crate) key_clone_attempts: usize,
}

impl Default for ActionIdLookup {
    fn default() -> Self {
        Self::new(Arc::new(Vec::new()))
    }
}

pub fn load_actions_typed(
    path: impl AsRef<Path>,
) -> Result<LoadState<Vec<Action>>, PersistenceError> {
    let path = path.as_ref();
    let _permit = crate::common::repair_watch::require_persisted_read(path)?;
    load_actions_typed_unfenced(path)
}

pub fn load_actions(path: &str) -> anyhow::Result<Vec<Action>> {
    let _permit = crate::common::repair_watch::require_path_read(Path::new(path))?;
    load_effective(path).map_err(Into::into)
}

pub(crate) fn load_actions_typed_unfenced(
    path: impl AsRef<Path>,
) -> Result<LoadState<Vec<Action>>, PersistenceError> {
    load_json(path)
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
    let _permit = crate::common::repair_watch::require_path_read(Path::new(path))?;
    let _transaction = transaction_guard();
    let mut actions = load_effective(path)?;
    mutate(&mut actions)?;
    save_json_atomic(path, &actions)?;
    bump_actions_version();
    Ok(actions)
}

fn load_effective(path: impl AsRef<Path>) -> Result<Vec<Action>, PersistenceError> {
    match load_actions_typed_unfenced(path)? {
        LoadState::Missing | LoadState::Empty => Ok(Vec::new()),
        LoadState::Loaded(actions) => Ok(actions),
    }
}

pub(crate) fn transaction_guard() -> MutexGuard<'static, ()> {
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
    fn add_action_validates_and_appends_exact_fields_including_duplicates() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("target.exe");
        std::fs::write(&target, "").unwrap();
        let target = target.to_str().unwrap();
        let storage = directory.path().join("actions.json");
        let storage = storage.to_str().unwrap();

        assert!(AddAction::new("", "", "", false, "").is_err());
        assert!(AddAction::new("", "", "missing target", false, "").is_err());
        let intent = AddAction::new(" label ", " description ", target, true, " --flag ").unwrap();
        let first = intent.clone().commit(storage).unwrap();
        let second = intent.commit(storage).unwrap();
        assert_eq!(second, vec![first[0].clone(), first[0].clone()]);
        assert_eq!(second[0].label, " label ");
        assert_eq!(second[0].desc, " description ");
        assert_eq!(second[0].action, target);
        assert_eq!(second[0].args.as_deref(), Some(" --flag "));
        assert_eq!(load_actions(storage).unwrap(), second);

        for (enabled, args) in [(false, " --flag "), (true, " \t ")] {
            let mut actions = Vec::new();
            AddAction::new("", "", target, enabled, args)
                .unwrap()
                .append_to(&mut actions)
                .unwrap();
            assert_eq!(actions[0].args, None);
        }
    }

    #[test]
    fn checked_custom_mutations_preserve_order_duplicates_and_exact_fields() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let target_path = directory.path().join("target.exe");
        std::fs::write(&target_path, "").unwrap();
        let target_path = target_path.to_str().unwrap();
        let storage = directory.path().join("actions.json");
        let storage = storage.to_str().unwrap();
        let duplicate = action("duplicate");
        let mut different_args = duplicate.clone();
        different_args.args = None;
        let seed = vec![duplicate.clone(), different_args.clone(), duplicate.clone()];
        save_actions(storage, &seed).unwrap();

        assert!(custom_action_row_target(&seed, seed.len()).is_none());
        let selected = custom_action_row_target(&seed, 2).unwrap();
        assert!(EditAction::new(selected.clone(), "", "", "", false, "").is_err());
        let edit = EditAction::new(
            selected,
            " label ",
            " description ",
            target_path,
            true,
            " --arg ",
        )
        .unwrap();
        let committed = edit.commit(storage).unwrap();
        let replacement = Action {
            label: " label ".into(),
            desc: " description ".into(),
            action: target_path.into(),
            args: Some(" --arg ".into()),
        };
        assert_eq!(
            committed,
            vec![
                duplicate.clone(),
                different_args.clone(),
                replacement.clone()
            ]
        );
        let target = custom_action_row_target(&committed, 1).unwrap();
        let removed = RemoveAction::new(target).commit(storage).unwrap();
        assert_eq!(removed, vec![duplicate, replacement]);
        assert_eq!(load_actions(storage).unwrap(), removed);
        assert_eq!(
            std::fs::read_to_string(storage).unwrap(),
            serde_json::to_string_pretty(&removed).unwrap()
        );
    }

    #[test]
    fn checked_custom_mutations_refuse_stale_targets_and_store_failures() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let target_path = directory.path().join("target.exe");
        std::fs::write(&target_path, "").unwrap();
        let target_path = target_path.to_str().unwrap();
        let storage = directory.path().join("actions.json");
        let storage = storage.to_str().unwrap();
        let duplicate = action("duplicate");
        let seed = vec![duplicate.clone(), action("other"), duplicate.clone()];
        let target = custom_action_row_target(&seed, 0).unwrap();
        let changed = vec![duplicate.clone(), action("other")];
        save_actions(storage, &changed).unwrap();
        let bytes = std::fs::read(storage).unwrap();
        let version = actions_version();
        let edit = EditAction::new(target.clone(), "new", "desc", target_path, false, "").unwrap();
        assert!(matches!(
            edit.commit(storage),
            Err(CheckedActionError::StaleTarget)
        ));
        assert!(matches!(
            RemoveAction::new(target.clone()).commit(storage),
            Err(CheckedActionError::StaleTarget)
        ));
        assert_eq!(std::fs::read(storage).unwrap(), bytes);
        assert_eq!(actions_version(), version);
        let mut in_memory = changed.clone();
        assert_eq!(
            RemoveAction::new(target).apply_to(&mut in_memory),
            Err(StaleCustomAction)
        );
        assert_eq!(in_memory, changed);

        let current = custom_action_row_target(&changed, 0).unwrap();
        std::fs::write(storage, b"invalid").unwrap();
        assert!(matches!(
            RemoveAction::new(current).commit(storage),
            Err(CheckedActionError::Store(_))
        ));
        assert_eq!(std::fs::read(storage).unwrap(), b"invalid");
        assert_eq!(actions_version(), version);
    }

    #[cfg(windows)]
    #[test]
    fn add_action_atomic_replace_failure_preserves_seed_and_retry() {
        use std::os::windows::fs::OpenOptionsExt;

        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("target.exe");
        std::fs::write(&target, b"owned target").unwrap();
        let path = directory.path().join("actions.json");
        let seed = vec![action("seed")];
        let bytes = serde_json::to_vec_pretty(&seed).unwrap();
        std::fs::write(&path, &bytes).unwrap();
        let target = target.to_str().unwrap();
        let path_str = path.to_str().unwrap();
        let request = AddAction::new(" label ", " description ", target, true, " --arg ").unwrap();
        let held = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&path)
            .unwrap();
        let version = actions_version();

        let error = request.clone().commit(path_str).unwrap_err();
        assert!(matches!(error.downcast_ref::<PersistenceError>(), Some(PersistenceError::AtomicWrite { .. })));
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(actions_version(), version);

        drop(held);
        let expected = vec![seed[0].clone(), Action {
            label: " label ".into(),
            desc: " description ".into(),
            action: target.into(),
            args: Some(" --arg ".into()),
        }];
        assert_eq!(request.commit(path_str).unwrap(), expected);
        assert_eq!(load_actions(path_str).unwrap(), expected);
        assert_eq!(std::fs::read(&path).unwrap(), serde_json::to_vec_pretty(&expected).unwrap());
    }

    #[test]
    fn action_id_lookup_is_catalog_bound_and_preserves_exact_args() {
        let mut duplicate_none = action("duplicate none");
        duplicate_none.action = "duplicate:id".into();
        duplicate_none.args = None;
        let mut duplicate_empty = action("duplicate empty");
        duplicate_empty.action = "duplicate:id".into();
        duplicate_empty.args = Some(String::new());
        let mut duplicate_value = action("duplicate value");
        duplicate_value.action = "duplicate:id".into();
        duplicate_value.args = Some("last args".into());
        let mut none_args = action("none args");
        none_args.action = "none:id".into();
        none_args.args = None;
        let mut empty_args = action("empty args");
        empty_args.action = "empty:id".into();
        empty_args.args = Some(String::new());
        let mut nonempty_args = action("nonempty args");
        nonempty_args.action = "value:id".into();
        nonempty_args.args = Some("value".into());

        let old_catalog = Arc::new(vec![
            duplicate_none,
            duplicate_empty,
            duplicate_value.clone(),
            none_args.clone(),
            empty_args.clone(),
            nonempty_args.clone(),
        ]);
        let (old_lookup, old_receipt) =
            ActionIdLookup::new_with_test_receipt(Arc::clone(&old_catalog));

        assert!(Arc::ptr_eq(&old_lookup.catalog, &old_catalog));
        assert_eq!(old_receipt.key_clone_attempts, old_catalog.len());
        assert_eq!(old_lookup.len(), 4);
        assert_eq!(old_lookup.get("duplicate:id"), Some(&duplicate_value));
        assert_eq!(old_lookup.get("none:id"), Some(&none_args));
        assert_eq!(old_lookup.get("empty:id"), Some(&empty_args));
        assert_eq!(old_lookup.get("value:id"), Some(&nonempty_args));
        assert_eq!(old_lookup.get("missing:id"), None);
        let resolved = old_lookup.iter().collect::<HashMap<_, _>>();
        assert_eq!(
            resolved.get("duplicate:id").copied(),
            Some(&duplicate_value)
        );
        assert_eq!(resolved.get("none:id").copied(), Some(&none_args));
        assert_eq!(resolved.get("empty:id").copied(), Some(&empty_args));
        assert_eq!(resolved.get("value:id").copied(), Some(&nonempty_args));
        drop(old_catalog);

        let mut replacement_duplicate = action("replacement duplicate");
        replacement_duplicate.action = "duplicate:id".into();
        replacement_duplicate.args = Some("new generation".into());
        let new_catalog = Arc::new(vec![replacement_duplicate.clone()]);
        let new_lookup = ActionIdLookup::new(Arc::clone(&new_catalog));

        assert!(Arc::ptr_eq(&new_lookup.catalog, &new_catalog));
        assert_eq!(new_lookup.get("duplicate:id"), Some(&replacement_duplicate));
        assert_eq!(old_lookup.get("duplicate:id"), Some(&duplicate_value));
        assert_eq!(old_lookup.get("none:id"), Some(&none_args));
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
