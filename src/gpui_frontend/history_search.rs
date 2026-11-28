//! Snapshot-bound History rows for checked saved-application replay.
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use multi_launcher::{actions::Action, history::HistoryEntry, plugin::Plugin};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Receipt {
    pub snapshot: Arc<VecDeque<HistoryEntry>>,
    pub index: usize,
    pub entry: HistoryEntry,
    pub inner: Action,
    pub kind: SavedKind,
    pub canonical: Arc<[Action]>,
    pub canonical_revision: u64,
}

#[derive(Default)]
pub(super) struct Provider {
    projection: Mutex<Option<Arc<Projection>>>,
}

pub(super) struct Projection {
    query: String,
    snapshot: Arc<VecDeque<HistoryEntry>>,
    rows: Vec<(Action, usize)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SavedKind { Executable, Document, Folder, Url, Shortcut }

fn saved_kind(action: &Action) -> Option<SavedKind> {
    if super::external_launch::saved_external(action).is_some() && ordinary_local_path(&action.action) {
        return Some(SavedKind::Executable);
    }
    if super::external_launch::saved_document(action).is_some() && ordinary_local_path(&action.action) {
        return Some(SavedKind::Document);
    }
    if super::saved_folder::candidate(action).is_some() { return Some(SavedKind::Folder); }
    if super::external_launch::saved_url(action).is_some() { return Some(SavedKind::Url); }
    #[cfg(windows)]
    if super::external_launch::saved_shortcut(action).is_some() { return Some(SavedKind::Shortcut); }
    None
}

fn unique_saved<'a>(entry: &HistoryEntry, saved: &'a [Action]) -> Option<(&'a Action, SavedKind)> {
    let mut matches = saved.iter().filter_map(|action| {
        if !(entry.action.label == action.label
            || entry.action.label.strip_prefix("app ") == Some(action.label.as_str()))
            || entry.action.desc != action.desc || entry.action.action != action.action || entry.action.args != action.args { return None; }
        Some((action, saved_kind(action)?))
    });
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

fn ordinary_local_path(path: &str) -> bool {
    #[cfg(windows)]
    {
        multi_launcher::actions::exec::ordinary_local_path_is_admissible(std::path::Path::new(path))
    }
    #[cfg(not(windows))]
    {
        std::path::Path::new(path).is_absolute()
    }
}

/// Recheck both durable stores sequentially at native entry. This does not
/// provide an atomic transaction across the two independent files.
pub(super) fn checked_entry(
    receipt: &Receipt,
    actions_path: &std::path::Path,
    history_path: &std::path::Path,
    current: impl Fn() -> bool,
    preclaim: impl Fn(Option<(&std::path::Path, &std::path::Path)>) -> anyhow::Result<()>,
    claim: impl FnOnce() -> bool,
    shortcut_paths: Option<(&std::path::Path, &std::path::Path)>,
) -> anyhow::Result<()> {
    anyhow::ensure!(current(), "History result changed; search History again");
    let rows = match multi_launcher::actions::load_actions_typed(actions_path).map_err(|error| {
        anyhow::anyhow!("Cannot read saved applications; refresh App Editor and try again: {error}")
    })? {
        multi_launcher::common::persistence::LoadState::Loaded(rows) => rows,
        _ => anyhow::bail!("Saved applications changed; refresh App Editor and try again"),
    };
    anyhow::ensure!(
        rows.as_slice() == receipt.canonical.as_ref()
            && unique_saved(&receipt.entry, &rows) == Some((&receipt.inner, receipt.kind)),
        "Saved applications changed; refresh App Editor and try again"
    );
    anyhow::ensure!(
        multi_launcher::history::checked_history_snapshot_matches(
            history_path,
            &receipt.snapshot,
            receipt.index,
            &receipt.entry,
            &current,
        )
        .map_err(|error| anyhow::anyhow!(
            "Cannot read History; manually relaunch to reload it: {error}"
        ))?,
        "History changed on disk; manually relaunch to reload it"
    );
    preclaim(shortcut_paths)?;
    anyhow::ensure!(
        current() && claim() && current(),
        "History result changed; search History again"
    );
    Ok(())
}

pub(super) fn checked_launch_with(
    receipt: &Receipt,
    actions_path: &std::path::Path,
    history_path: &std::path::Path,
    current: impl Fn() -> bool,
    preclaim: impl Fn(Option<(&std::path::Path, &std::path::Path)>) -> anyhow::Result<()>,
    claim: impl Fn() -> bool,
    launch: impl FnOnce(&dyn Fn(Option<(&std::path::Path, &std::path::Path)>) -> bool) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let failure = std::cell::RefCell::new(None);
    let result =
        launch(
            &|paths| match checked_entry(receipt, actions_path, history_path, &current, &preclaim, &claim, paths) {
                Ok(()) => true,
                Err(error) => {
                    *failure.borrow_mut() = Some(error);
                    false
                }
            },
        );
    match failure.into_inner() {
        Some(error) => Err(error),
        None => result,
    }
}

impl Projection {
    pub(super) fn receipt(
        &self,
        outer: &Action,
        canonical: Arc<[Action]>,
        revision: u64,
        verified_folders: &[Action],
    ) -> Option<Receipt> {
        let index = self.rows.iter().find(|(action, _)| action == outer)?.1;
        let entry = self.snapshot.get(index)?;
        let (inner, kind) = unique_saved(entry, &canonical)?;
        if kind == SavedKind::Folder && !verified_folders.contains(inner) { return None; }
        let inner = inner.clone();
        Some(Receipt {
            snapshot: self.snapshot.clone(),
            index,
            entry: entry.clone(),
            inner,
            kind,
            canonical,
            canonical_revision: revision,
        })
    }
}

impl Provider {
    pub(super) fn projection(&self, raw_query: &str) -> Option<Arc<Projection>> {
        let effective = multi_launcher::common::query::split_action_filters(raw_query).0;
        self.projection
            .lock()
            .ok()?
            .as_ref()
            .filter(|projection| projection.query == effective)
            .cloned()
    }
}

impl Plugin for Provider {
    fn name(&self) -> &str {
        "history"
    }
    fn description(&self) -> &str {
        "Search previously executed queries (prefix: `hi`)"
    }
    fn capabilities(&self) -> &[&str] {
        &["search"]
    }
    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "hi".into(),
            desc: "History".into(),
            action: "query:hi".into(),
            args: None,
        }]
    }
    fn search(&self, query: &str) -> Vec<Action> {
        if let Some(action) = multi_launcher::plugins::history::clear_action(query) { return vec![action]; }
        if multi_launcher::common::strip_prefix_ci(query, "hi").is_none() {
            return Vec::new();
        }
        let Some(snapshot) = multi_launcher::history::with_history(|rows| Arc::new(rows.clone()))
        else {
            return Vec::new();
        };
        let rows: Vec<_> = multi_launcher::plugins::history::project_history_rows(query, &snapshot)
            .into_iter()
            .map(|row| (row.action, row.index))
            .collect();
        if let Ok(mut projection) = self.projection.lock() {
            *projection = Some(Arc::new(Projection {
                query: query.to_owned(),
                snapshot,
                rows: rows.clone(),
            }));
        }
        rows.into_iter().map(|(action, _)| action).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saved() -> Action {
        Action {
            label: "Editor".into(),
            desc: "A saved app".into(),
            action: if cfg!(windows) {
                r"C:\Apps\editor.exe"
            } else {
                "/apps/editor.exe"
            }
            .into(),
            args: Some("--new".into()),
        }
    }

    fn entry(action: Action) -> HistoryEntry {
        HistoryEntry {
            query: "editor".into(),
            query_lc: "editor".into(),
            action,
            source: None,
            timestamp: 0,
        }
    }

    #[test]
    fn exact_saved_history_target_rejects_ambiguity_and_changed_arguments() {
        let app = saved();
        assert_eq!(
            unique_saved(&entry(app.clone()), &[app.clone()]),
            Some((&app, SavedKind::Executable))
        );
        let mut prefixed = app.clone();
        prefixed.label = format!("app {}", prefixed.label);
        assert_eq!(
            unique_saved(&entry(prefixed), &[app.clone()]),
            Some((&app, SavedKind::Executable))
        );
        assert!(unique_saved(&entry(app.clone()), &[app.clone(), app.clone()]).is_none());
        let mut changed = app.clone();
        changed.args = None;
        assert!(unique_saved(&entry(changed), &[app.clone()]).is_none());
        let mut nested = app.clone();
        nested.action = "history:0".into();
        assert!(unique_saved(&entry(nested.clone()), &[nested]).is_none());
        let mut relative = app;
        relative.action = "editor.exe".into();
        assert!(unique_saved(&entry(relative.clone()), &[relative]).is_none());
    }

    #[cfg(windows)]
    #[test]
    fn projected_history_admits_verified_folder_and_local_document_only() {
        let document = Action { label: "Report".into(), desc: "Saved".into(), action: r"C:\Reports\report.pdf".into(), args: None };
        let folder = Action { label: "Folder".into(), desc: "Saved".into(), action: r"C:\Reports".into(), args: None };
        let history = Arc::new(VecDeque::from([entry(document.clone()), entry(folder.clone())]));
        let rows = multi_launcher::plugins::history::project_history_rows("hi", &history).into_iter().map(|row| (row.action, row.index)).collect();
        let projection = Projection { query: "hi".into(), snapshot: history, rows };
        let canonical: Arc<[Action]> = vec![document.clone(), folder.clone()].into();
        let doc = projection.rows.iter().find(|(_, index)| *index == 0).unwrap();
        let dir = projection.rows.iter().find(|(_, index)| *index == 1).unwrap();
        assert_eq!(projection.receipt(&doc.0, canonical.clone(), 1, &[]).unwrap().kind, SavedKind::Document);
        assert!(projection.receipt(&dir.0, canonical.clone(), 1, &[]).is_none());
        assert_eq!(projection.receipt(&dir.0, canonical.clone(), 1, &[folder.clone()]).unwrap().kind, SavedKind::Folder);
        let mut wrong = folder.clone(); wrong.args = Some("--changed".into());
        assert!(projection.receipt(&dir.0, canonical.clone(), 1, &[wrong]).is_none());
        let mut duplicate = canonical.to_vec(); duplicate.push(folder.clone());
        assert!(projection.receipt(&dir.0, duplicate.into(), 1, &[folder]).is_none());
        let mut remote = document.clone(); remote.action = r"\\server\share\report.pdf".into();
        assert!(unique_saved(&entry(remote.clone()), &[remote]).is_none());
    }

    #[test]
    fn saved_history_url_requires_strict_exact_target_and_args() {
        let url = Action { label: "Web".into(), desc: "Saved".into(), action: "https://example.com/path".into(), args: None };
        let snapshot = Arc::new(VecDeque::from([entry(url.clone())]));
        let rows = multi_launcher::plugins::history::project_history_rows("hi", &snapshot).into_iter().map(|row| (row.action, row.index)).collect();
        let projection = Projection { query: "hi".into(), snapshot, rows };
        assert_eq!(projection.receipt(&projection.rows[0].0, vec![url.clone()].into(), 1, &[]).unwrap().kind, SavedKind::Url);
        let mut invalid = url.clone(); invalid.action = "https://example.com/bad path".into();
        assert!(unique_saved(&entry(invalid.clone()), &[invalid]).is_none());
        let mut changed = url.clone(); changed.args = Some("--changed".into());
        assert!(unique_saved(&entry(changed), &[url]).is_none());
    }

    #[cfg(windows)]
    #[test]
    fn saved_history_shortcut_requires_exact_local_stock_target() {
        let shortcut = Action { label: "Shortcut".into(), desc: "Saved".into(), action: r"C:\Apps\app.lnk".into(), args: None };
        let snapshot = Arc::new(VecDeque::from([entry(shortcut.clone())]));
        let rows = multi_launcher::plugins::history::project_history_rows("hi", &snapshot).into_iter().map(|row| (row.action, row.index)).collect();
        let projection = Projection { query: "hi".into(), snapshot, rows };
        assert_eq!(projection.receipt(&projection.rows[0].0, vec![shortcut.clone()].into(), 1, &[]).unwrap().kind, SavedKind::Shortcut);
        let mut wrong = shortcut.clone(); wrong.args = Some("--changed".into());
        assert!(unique_saved(&entry(wrong), &[shortcut.clone()]).is_none());
        let mut remote = shortcut; remote.action = r"\\server\share\app.lnk".into();
        assert!(unique_saved(&entry(remote.clone()), &[remote]).is_none());
    }

    #[test]
    fn receipt_binds_original_query_snapshot_and_canonical_revision() {
        let app = saved();
        let snapshot = Arc::new(VecDeque::from([entry(app.clone())]));
        let provider = Provider::default();
        let outer = multi_launcher::plugins::history::project_history_rows("hi editor", &snapshot)
            [0]
        .action
        .clone();
        *provider.projection.lock().unwrap() = Some(Arc::new(Projection {
            query: "hi editor".into(),
            snapshot: snapshot.clone(),
            rows: vec![(outer.clone(), 0)],
        }));
        let projection = provider.projection("hi editor").unwrap();
        let canonical: Arc<[Action]> = vec![app.clone()].into();
        let receipt = projection.receipt(&outer, canonical.clone(), 7, &[]).unwrap();
        assert!(Arc::ptr_eq(&receipt.snapshot, &snapshot));
        assert_eq!(receipt.index, 0);
        assert_eq!(receipt.inner, app);
        assert_eq!(receipt.canonical_revision, 7);
        assert!(provider.projection("hi other").is_none());
        let mut wrong = outer;
        wrong.action = "history:1".into();
        assert!(projection.receipt(&wrong, canonical, 7, &[]).is_none());
    }

    #[test]
    fn checked_entry_rejects_both_stores_changed_after_preparation() {
        let directory = tempfile::tempdir().unwrap();
        let actions_path = directory.path().join("actions.json");
        let history_path = directory.path().join("history.json");
        let app = saved();
        let row = entry(app.clone());
        let receipt = Receipt {
            snapshot: Arc::new(VecDeque::from([row.clone()])),
            index: 0,
            entry: row.clone(),
            inner: app.clone(),
            kind: SavedKind::Executable,
            canonical: vec![app.clone()].into(),
            canonical_revision: 1,
        };
        let actions = serde_json::to_vec(&[app]).unwrap();
        let history = serde_json::to_vec(&[row]).unwrap();
        for change_actions in [true, false] {
            std::fs::write(&actions_path, &actions).unwrap();
            std::fs::write(&history_path, &history).unwrap();
            let spawned = std::cell::Cell::new(false);
            let result = checked_launch_with(
                &receipt,
                &actions_path,
                &history_path,
                || true,
                |_| Ok(()),
                || true,
                |entry| {
                    if change_actions {
                        std::fs::write(&actions_path, b"[]").unwrap();
                    } else {
                        std::fs::write(&history_path, b"[]").unwrap();
                    }
                    if entry(None) {
                        spawned.set(true);
                        Ok(())
                    } else {
                        Err(anyhow::anyhow!("native refused"))
                    }
                },
            );
            assert!(result.is_err());
            assert!(!spawned.get());
        }
    }

    #[test]
    fn association_final_eligibility_failure_prevents_claim_and_effect() {
        let directory = tempfile::tempdir().unwrap();
        let actions_path = directory.path().join("actions.json");
        let history_path = directory.path().join("history.json");
        let url = Action { label: "Site".into(), desc: "Saved".into(), action: "https://example.com/".into(), args: None };
        let row = entry(url.clone());
        let receipt = Receipt { snapshot: Arc::new(VecDeque::from([row.clone()])), index: 0, entry: row.clone(), inner: url.clone(), kind: SavedKind::Url, canonical: vec![url.clone()].into(), canonical_revision: 1 };
        std::fs::write(&actions_path, serde_json::to_vec(&[url]).unwrap()).unwrap();
        std::fs::write(&history_path, serde_json::to_vec(&[row]).unwrap()).unwrap();
        let claimed = std::cell::Cell::new(false);
        let launched = std::cell::Cell::new(false);
        let result = checked_launch_with(&receipt, &actions_path, &history_path, || true,
            |_| Err(anyhow::anyhow!("target no longer eligible")),
            || { claimed.set(true); true },
            |entry| { if entry(None) { launched.set(true); Ok(()) } else { Err(anyhow::anyhow!("refused")) } });
        assert!(result.unwrap_err().to_string().contains("target no longer eligible"));
        assert!(!claimed.get());
        assert!(!launched.get());
    }

    #[test]
    fn successful_outer_append_produces_fresh_replay_ordinal() {
        let app = saved();
        let old = entry(app.clone());
        let mut outer = old.clone();
        outer.action = Action {
            label: "editor".into(),
            desc: "History".into(),
            action: "history:0".into(),
            args: None,
        };
        let snapshot = Arc::new(VecDeque::from([outer, old]));
        let rows = multi_launcher::plugins::history::project_history_rows("hi editor", &snapshot)
            .into_iter()
            .map(|row| (row.action, row.index))
            .collect();
        let projection = Projection {
            query: "hi editor".into(),
            snapshot,
            rows,
        };
        assert!(
            projection
                .receipt(&projection.rows[0].0, vec![app.clone()].into(), 1, &[])
                .is_none()
        );
        let replay = projection
            .receipt(&projection.rows[1].0, vec![app].into(), 1, &[])
            .unwrap();
        assert_eq!(replay.index, 1);
    }

    #[test]
    fn history_recomposition_uses_routed_raw_action_filters() {
        struct Fixture;
        impl Plugin for Fixture {
            fn name(&self) -> &str {
                "history"
            }
            fn description(&self) -> &str {
                "History fixture"
            }
            fn capabilities(&self) -> &[&str] {
                &["search"]
            }
            fn search(&self, query: &str) -> Vec<Action> {
                assert_eq!(query, "hi");
                vec![Action {
                    label: "one".into(),
                    desc: "History".into(),
                    action: "history:0".into(),
                    args: None,
                }]
            }
        }
        let providers = [&Fixture as &dyn Plugin];
        assert_eq!(
            super::super::search_providers::search_routed(providers, "hi", None, None).len(),
            1
        );
        assert!(
            super::super::search_providers::search_routed(
                providers,
                "hi !id:history:0",
                None,
                None
            )
            .is_empty()
        );
    }
}
