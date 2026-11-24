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
static LIVE_FAVS: Lazy<super::live_snapshot::LiveSnapshotRegistry<FavEntry>> =
    Lazy::new(super::live_snapshot::LiveSnapshotRegistry::new);

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct FavEntry {
    pub label: String,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PrefilledFavoriteOrigin {
    Existing {
        index: usize,
        original: FavEntry,
    },
    Append {
        lookup_label: String,
        lookup_action: String,
        lookup_args: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrefilledFavoriteDraft {
    pub label: String,
    pub action: String,
    pub args: Option<String>,
    pub(crate) origin: PrefilledFavoriteOrigin,
}

#[derive(Debug)]
pub enum PrefilledFavoriteSaveError {
    InvalidDraft,
    StaleTarget,
    Store(anyhow::Error),
}

impl std::fmt::Display for PrefilledFavoriteSaveError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidDraft => formatter.write_str("Label and command are required"),
            Self::StaleTarget => formatter.write_str(
                "Favorites changed while this prefilled draft was open. Reopen it before saving.",
            ),
            Self::Store(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for PrefilledFavoriteSaveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Store(error) => Some(error.as_ref()),
            Self::InvalidDraft | Self::StaleTarget => None,
        }
    }
}

pub fn prefilled_favorite_draft(entries: &[FavEntry], action: &Action) -> PrefilledFavoriteDraft {
    let exact = entries.iter().position(|entry| {
        entry.action == action.action && entry.args.as_deref() == action.args.as_deref()
    });
    let matching_label = entries
        .iter()
        .position(|entry| entry.label.eq_ignore_ascii_case(&action.label));
    let origin = exact
        .or(matching_label)
        .map(|index| PrefilledFavoriteOrigin::Existing {
            index,
            original: entries[index].clone(),
        })
        .unwrap_or_else(|| PrefilledFavoriteOrigin::Append {
            lookup_label: action.label.clone(),
            lookup_action: action.action.clone(),
            lookup_args: action.args.clone(),
        });

    PrefilledFavoriteDraft {
        label: action.label.clone(),
        action: action.action.clone(),
        args: action.args.clone(),
        origin,
    }
}

pub fn save_prefilled_favorite(
    path: &str,
    draft: PrefilledFavoriteDraft,
) -> Result<Vec<FavEntry>, PrefilledFavoriteSaveError> {
    if draft.label.trim().is_empty() || draft.action.trim().is_empty() {
        return Err(PrefilledFavoriteSaveError::InvalidDraft);
    }

    let replacement = FavEntry {
        label: draft.label,
        action: draft.action,
        args: draft.args.filter(|args| !args.trim().is_empty()),
    };
    let mut stale_target = false;
    let updated = update_favs(path, |entries| {
        match draft.origin {
            PrefilledFavoriteOrigin::Existing { index, original } => {
                if entries.get(index) != Some(&original) {
                    stale_target = true;
                    anyhow::bail!("the prefilled favorite target changed");
                }
                entries[index] = replacement;
            }
            PrefilledFavoriteOrigin::Append {
                lookup_label,
                lookup_action,
                lookup_args,
            } => {
                if entries.iter().any(|entry| {
                    (entry.action == lookup_action && entry.args == lookup_args)
                        || entry.label.eq_ignore_ascii_case(&lookup_label)
                }) {
                    stale_target = true;
                    anyhow::bail!("the prefilled favorite append target now exists");
                }
                entries.push(replacement);
            }
        }
        Ok(true)
    });

    match updated {
        Ok(entries) => Ok(entries),
        Err(_) if stale_target => Err(PrefilledFavoriteSaveError::StaleTarget),
        Err(error) => Err(PrefilledFavoriteSaveError::Store(error)),
    }
}

/// A captured row identity, checked against the canonical store on mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FavoriteRowTarget {
    index: usize,
    original: FavEntry,
    matching_positions: Vec<usize>,
}

pub fn favorite_row_target(entries: &[FavEntry], index: usize) -> Option<FavoriteRowTarget> {
    let original = entries.get(index)?.clone();
    let matching_positions = entries
        .iter()
        .enumerate()
        .filter_map(|(i, entry)| (entry == &original).then_some(i))
        .collect();
    Some(FavoriteRowTarget {
        index,
        original,
        matching_positions,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManualFavoriteOrigin {
    Append,
    NamedAppend(String),
    Existing(FavoriteRowTarget),
}

pub fn named_favorite_origin(entries: &[FavEntry], label: &str) -> ManualFavoriteOrigin {
    entries
        .iter()
        .position(|entry| entry.label == label)
        .and_then(|index| favorite_row_target(entries, index))
        .map(ManualFavoriteOrigin::Existing)
        .unwrap_or_else(|| ManualFavoriteOrigin::NamedAppend(label.to_owned()))
}

fn checked_row(entries: &[FavEntry], target: &FavoriteRowTarget) -> bool {
    if entries.get(target.index) != Some(&target.original) {
        return false;
    }
    entries
        .iter()
        .enumerate()
        .filter_map(|(i, entry)| (entry == &target.original).then_some(i))
        .eq(target.matching_positions.iter().copied())
}

enum ManualFavoriteMutation {
    Save(ManualFavoriteOrigin, FavEntry),
    Remove(FavoriteRowTarget),
}

fn manual_favorite_update(
    path: &str,
    mutation: ManualFavoriteMutation,
) -> Result<Vec<FavEntry>, PrefilledFavoriteSaveError> {
    if let ManualFavoriteMutation::Save(_, entry) = &mutation {
        if entry.label.trim().is_empty() || entry.action.trim().is_empty() {
            return Err(PrefilledFavoriteSaveError::InvalidDraft);
        }
    }
    let mut stale = false;
    let result = update_favs(path, |entries| {
        match mutation {
            ManualFavoriteMutation::Save(ManualFavoriteOrigin::Append, entry) => {
                entries.push(entry)
            }
            ManualFavoriteMutation::Save(ManualFavoriteOrigin::NamedAppend(label), entry) => {
                if entries.iter().any(|row| row.label == label) {
                    stale = true;
                    anyhow::bail!("named favorite now exists");
                }
                entries.push(entry);
            }
            ManualFavoriteMutation::Save(ManualFavoriteOrigin::Existing(target), entry) => {
                if !checked_row(entries, &target) {
                    stale = true;
                    anyhow::bail!("favorite row changed");
                }
                entries[target.index] = entry;
            }
            ManualFavoriteMutation::Remove(target) => {
                if !checked_row(entries, &target) {
                    stale = true;
                    anyhow::bail!("favorite row changed");
                }
                entries.remove(target.index);
            }
        }
        Ok(true)
    });
    match result {
        Ok(entries) => Ok(entries),
        Err(_) if stale => Err(PrefilledFavoriteSaveError::StaleTarget),
        Err(error) => Err(PrefilledFavoriteSaveError::Store(error)),
    }
}

pub fn save_manual_favorite(
    path: &str,
    origin: ManualFavoriteOrigin,
    entry: FavEntry,
) -> Result<Vec<FavEntry>, PrefilledFavoriteSaveError> {
    manual_favorite_update(path, ManualFavoriteMutation::Save(origin, entry))
}

pub fn remove_favorite_row(
    path: &str,
    target: FavoriteRowTarget,
) -> Result<Vec<FavEntry>, PrefilledFavoriteSaveError> {
    manual_favorite_update(path, ManualFavoriteMutation::Remove(target))
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
    let path = path.as_ref();
    let _permit = crate::common::repair_watch::require_persisted_read(path)?;
    load_favs_typed_unfenced(path)
}

fn load_favs_typed_unfenced(
    path: impl AsRef<Path>,
) -> Result<LoadState<Vec<FavEntry>>, PersistenceError> {
    load_json(path)
}

pub(crate) fn load_favs_for_reload(
    path: impl AsRef<Path>,
) -> Result<LoadState<Vec<FavEntry>>, PersistenceError> {
    let path = path.as_ref();
    let _permit = crate::common::repair_watch::require_persisted_read(path)?;
    let _transaction = fav_transaction_guard();
    load_favs_typed_unfenced(path)
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
    let _permit = crate::common::repair_watch::require_path_read(Path::new(path))?;
    let _transaction = fav_transaction_guard();
    let mut favs = match load_favs_typed_unfenced(path)? {
        LoadState::Missing | LoadState::Empty => Vec::new(),
        LoadState::Loaded(favs) => favs,
    };
    if mutate(&mut favs)? {
        save(path, &favs)?;
        LIVE_FAVS.publish(path, &favs);
        record_versioned_favs(path, &favs);
    }
    Ok(favs)
}

pub(crate) fn fav_transaction_guard() -> MutexGuard<'static, ()> {
    FAV_TRANSACTION
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Publish a favorite snapshot after an owner-coordinated repair commits.
pub(crate) fn publish_repaired_favs(path: &str, favs: &[FavEntry]) {
    LIVE_FAVS.publish(path, favs);
    record_versioned_favs(path, favs);
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
        Self::new_for_path(FAV_FILE)
    }

    fn new_for_path(path: &str) -> Self {
        let data = {
            let _transaction = fav_transaction_guard();
            let startup = match load_favs(path) {
                Ok(favs) => Some(favs),
                Err(error) => {
                    tracing::error!(%error, "favorite startup retained invalid persisted file");
                    None
                }
            };
            let data = LIVE_FAVS.get_or_create(path, startup.clone());
            if let Some(startup) = startup {
                *VERSIONED_FAVS
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) =
                    Some((PathBuf::from(path), startup));
            }
            data
        };
        let data_clone = data.clone();
        let path = path.to_string();
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
    let favs = match load_favs_typed_unfenced(path)? {
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

fn favorite_dialog_action(label: &str) -> Action {
    Action {
        label: if label.is_empty() {
            "fav: add".into()
        } else {
            format!("Add fav {label}")
        },
        desc: "Fav".into(),
        action: format!("fav:dialog:{label}"),
        args: None,
    }
}

fn favorite_browse_action() -> Action {
    Action {
        label: "Favorites".into(),
        ..favorite_dialog_action("")
    }
}

/// Stateless Search adapter for the Favorites browse routes.
pub struct FavBrowsePlugin;

impl Plugin for FavBrowsePlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let query = query.trim();
        if query.eq_ignore_ascii_case("fav") {
            vec![favorite_browse_action()]
        } else if query.eq_ignore_ascii_case("fav add") {
            vec![favorite_dialog_action("")]
        } else {
            Vec::new()
        }
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
        favorite_browse_commands()
    }
}

fn favorite_browse_commands() -> Vec<Action> {
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
    ]
}

impl Plugin for FavPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        if trimmed.eq_ignore_ascii_case("fav") {
            return vec![favorite_browse_action()];
        }

        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "fav add") {
            let label = rest.trim();
            return vec![favorite_dialog_action(label)];
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
        let mut commands = favorite_browse_commands();
        commands.extend([
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
        ]);
        commands
    }
}

#[cfg(test)]
mod tests {
    use super::{join_command_args, resolve_with_plugin};
    use crate::{actions::Action, plugin::Plugin};

    #[test]
    fn favorite_browse_provider_exact_queries_share_egui_results() {
        use super::{FavBrowsePlugin, favorite_browse_action, favorite_dialog_action};
        let provider = FavBrowsePlugin;
        for query in ["fav", " FAV "] {
            assert_eq!(provider.search(query), vec![favorite_browse_action()]);
        }
        for query in ["fav add", " FAV ADD  "] {
            assert_eq!(provider.search(query), vec![favorite_dialog_action("")]);
        }
        for query in ["fav add name", "fav addx", "fav list", "favorite"] {
            assert!(provider.search(query).is_empty());
        }
    }

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

    fn action(label: &str, command: &str, args: Option<&str>) -> Action {
        Action {
            label: label.into(),
            desc: String::new(),
            action: command.into(),
            args: args.map(str::to_owned),
        }
    }

    #[test]
    fn prefilled_draft_matches_exact_optional_args_before_first_label() {
        let entries = vec![
            fav("PROJECT", "old", None),
            fav("other", "cmd", None),
            fav("exact", "cmd", Some("arg")),
            fav("project", "later", None),
        ];
        let draft = prefilled_favorite_draft(&entries, &action("project", "cmd", Some("arg")));
        assert!(matches!(
            draft.origin,
            PrefilledFavoriteOrigin::Existing { index: 2, .. }
        ));
        let draft =
            prefilled_favorite_draft(&entries, &action("project", "cmd", Some("different")));
        assert!(matches!(
            draft.origin,
            PrefilledFavoriteOrigin::Existing { index: 0, .. }
        ));
        let draft = prefilled_favorite_draft(&entries, &action("new", "cmd", None));
        assert!(matches!(
            draft.origin,
            PrefilledFavoriteOrigin::Existing { index: 1, .. }
        ));
        let draft = prefilled_favorite_draft(&entries, &action("new", "cmd", Some("")));
        assert!(matches!(
            draft.origin,
            PrefilledFavoriteOrigin::Append { .. }
        ));
    }

    #[test]
    fn prefilled_save_preserves_latest_entries_whitespace_and_publication() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fav.json");
        let path = path.to_str().unwrap();
        let original = fav("original", "cmd", None);
        save_favs(path, &[original.clone()]).unwrap();
        let live = LIVE_FAVS.get_or_create(path, Some(vec![original.clone()]));
        let mut draft = prefilled_favorite_draft(&[original.clone()], &action("new", "cmd", None));
        let unrelated = fav("unrelated", "other", None);
        save_favs(path, &[original, unrelated.clone()]).unwrap();
        draft.label = "  label  ".into();
        draft.action = "  command  ".into();
        draft.args = Some("  args  ".into());
        let expected = vec![fav("  label  ", "  command  ", Some("  args  ")), unrelated];
        assert_eq!(save_prefilled_favorite(path, draft).unwrap(), expected);
        assert_eq!(load_favs(path).unwrap(), expected);
        assert_eq!(*live.lock().unwrap(), expected);

        let mut append = prefilled_favorite_draft(&expected, &action("append", "new", None));
        append.args = Some(" \t ".into());
        let committed = save_prefilled_favorite(path, append).unwrap();
        assert_eq!(committed.last().unwrap().args, None);
    }

    #[test]
    fn prefilled_save_rejects_stale_and_malformed_without_publication() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fav.json");
        let path = path.to_str().unwrap();
        let original = fav("target", "cmd", None);
        let other = fav("other", "other", None);
        let draft = prefilled_favorite_draft(
            &[original.clone(), other.clone()],
            &action("target", "cmd", None),
        );
        for latest in [
            vec![fav("changed", "cmd", None), other.clone()],
            vec![other.clone(), original.clone()],
        ] {
            save_favs(path, &latest).unwrap();
            let bytes = std::fs::read(path).unwrap();
            let snapshot = LIVE_FAVS.snapshot(path);
            assert!(matches!(
                save_prefilled_favorite(path, draft.clone()),
                Err(PrefilledFavoriteSaveError::StaleTarget)
            ));
            assert_eq!(std::fs::read(path).unwrap(), bytes);
            assert_eq!(LIVE_FAVS.snapshot(path), snapshot);
        }
        let append = prefilled_favorite_draft(&[], &action("new", "new-cmd", Some("arg")));
        for conflict in [
            fav("NEW", "other", None),
            fav("other", "new-cmd", Some("arg")),
        ] {
            save_favs(path, &[conflict]).unwrap();
            let bytes = std::fs::read(path).unwrap();
            let snapshot = LIVE_FAVS.snapshot(path);
            assert!(matches!(
                save_prefilled_favorite(path, append.clone()),
                Err(PrefilledFavoriteSaveError::StaleTarget)
            ));
            assert_eq!(std::fs::read(path).unwrap(), bytes);
            assert_eq!(LIVE_FAVS.snapshot(path), snapshot);
        }
        std::fs::write(path, b"{broken").unwrap();
        let snapshot = LIVE_FAVS.snapshot(path);
        assert!(matches!(
            save_prefilled_favorite(path, append.clone()),
            Err(PrefilledFavoriteSaveError::Store(_))
        ));
        assert_eq!(std::fs::read(path).unwrap(), b"{broken");
        assert_eq!(LIVE_FAVS.snapshot(path), snapshot);
        std::fs::remove_file(path).unwrap();
        assert_eq!(
            save_prefilled_favorite(path, append).unwrap(),
            vec![fav("new", "new-cmd", Some("arg"))]
        );
    }

    #[test]
    fn manual_rows_preserve_concurrent_entries_and_reject_stale_targets() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fav.json");
        let path = path.to_str().unwrap();
        let first = fav("same", "cmd", Some("one"));
        let second = fav("same", "cmd", Some("two"));
        let entries = vec![first.clone(), second.clone()];
        let target = favorite_row_target(&entries, 1).unwrap();
        let unrelated = fav("other", "other", None);
        save_favs(path, &[first.clone(), second.clone(), unrelated.clone()]).unwrap();
        let edited = fav("same", "edited", Some("two"));
        assert_eq!(
            save_manual_favorite(
                path,
                ManualFavoriteOrigin::Existing(target.clone()),
                edited.clone()
            )
            .unwrap(),
            vec![first.clone(), edited.clone(), unrelated.clone()]
        );
        assert!(matches!(
            remove_favorite_row(path, target.clone()),
            Err(PrefilledFavoriteSaveError::StaleTarget)
        ));
        assert!(matches!(
            save_manual_favorite(path, ManualFavoriteOrigin::Existing(target), second.clone()),
            Err(PrefilledFavoriteSaveError::StaleTarget)
        ));
        let rows = load_favs(path).unwrap();
        let target = favorite_row_target(&rows, 1).unwrap();
        assert_eq!(
            remove_favorite_row(path, target).unwrap(),
            vec![first.clone(), unrelated]
        );
        let rows = save_manual_favorite(path, ManualFavoriteOrigin::Append, first.clone()).unwrap();
        assert_eq!(rows.iter().filter(|row| row.label == "same").count(), 2);
        let duplicate_target = favorite_row_target(&[first.clone(), rows[1].clone()], 0).unwrap();
        save_favs(
            path,
            &[
                first.clone(),
                rows[1].clone(),
                first.clone(),
                rows[1].clone(),
            ],
        )
        .unwrap();
        assert!(matches!(
            remove_favorite_row(path, duplicate_target),
            Err(PrefilledFavoriteSaveError::StaleTarget)
        ));
        let duplicate_target = favorite_row_target(&rows, 2).unwrap();
        save_favs(path, &[rows[1].clone(), first.clone()]).unwrap();
        assert!(matches!(
            remove_favorite_row(path, duplicate_target),
            Err(PrefilledFavoriteSaveError::StaleTarget)
        ));
    }

    #[test]
    fn named_manual_drafts_use_exact_case_and_reject_new_matching_target() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fav.json");
        let path = path.to_str().unwrap();
        let rows = vec![
            fav("Name", "first", None),
            fav("name", "second", None),
            fav("Name", "third", None),
        ];
        assert_eq!(
            named_favorite_origin(&rows, "Name"),
            ManualFavoriteOrigin::Existing(favorite_row_target(&rows, 0).unwrap())
        );
        assert_eq!(
            named_favorite_origin(&rows, "name"),
            ManualFavoriteOrigin::Existing(favorite_row_target(&rows, 1).unwrap())
        );
        let origin = named_favorite_origin(&rows, "NAME");
        save_favs(path, &rows).unwrap();
        let new = fav("NAME", "new", Some("  "));
        let saved = save_manual_favorite(path, origin.clone(), new.clone()).unwrap();
        assert_eq!(saved.last().unwrap().args.as_deref(), Some("  "));
        assert!(matches!(
            save_manual_favorite(path, origin, new),
            Err(PrefilledFavoriteSaveError::StaleTarget)
        ));
        std::fs::write(path, b"{broken").unwrap();
        assert!(matches!(
            save_manual_favorite(path, ManualFavoriteOrigin::Append, rows[0].clone()),
            Err(PrefilledFavoriteSaveError::Store(_))
        ));
        assert_eq!(std::fs::read(path).unwrap(), b"{broken");
    }

    #[test]
    fn checked_manual_favorites_cannot_write_during_recovery_conflict() {
        use crate::common::repair_watch::{block_paths, resolve_recovered_paths};

        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fav.json");
        let path_text = path.to_str().unwrap();
        let rows = vec![fav("original", "query:owned", None)];
        save_favs(path_text, &rows).unwrap();
        let original_bytes = std::fs::read(&path).unwrap();
        let live = LIVE_FAVS.get_or_create(path_text, Some(rows.clone()));
        let target = favorite_row_target(&rows, 0).unwrap();

        block_paths([path.clone()]);
        let append = save_manual_favorite(
            path_text,
            ManualFavoriteOrigin::Append,
            fav("new", "query:new", None),
        );
        let remove = remove_favorite_row(path_text, target);
        resolve_recovered_paths([path.clone()]);

        for result in [append, remove] {
            assert!(matches!(
                result,
                Err(PrefilledFavoriteSaveError::Store(error))
                    if error.to_string().contains("saved-target recovery needs attention")
            ));
        }
        assert_eq!(std::fs::read(path).unwrap(), original_bytes);
        assert_eq!(*live.lock().unwrap(), rows);
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

    #[test]
    fn committed_mutation_is_visible_to_all_instances_without_watcher_delivery() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fav.json");
        let path = path.to_str().unwrap();
        save_favs(path, &[]).unwrap();
        let first = FavPlugin::new_for_path(path);
        let second = FavPlugin::new_for_path(path);

        set_fav(path, "Immediate", "noop:published", Some("argument")).unwrap();

        for plugin in [&first, &second] {
            assert!(plugin.search("fav Immediate").iter().any(|action| {
                action.label == "Immediate"
                    && action.action == "noop:published"
                    && action.args.as_deref() == Some("argument")
            }));
        }
    }
}
