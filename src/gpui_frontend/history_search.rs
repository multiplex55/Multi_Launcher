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

pub(super) fn unique_saved(entry: &HistoryEntry, saved: &[Action]) -> Option<Action> {
    let mut matches = saved.iter().filter(|action| {
        (entry.action.label == action.label
            || entry.action.label.strip_prefix("app ") == Some(action.label.as_str()))
            && entry.action.desc == action.desc
            && entry.action.action == action.action
            && entry.action.args == action.args
            && super::external_launch::saved_external(action).is_some()
            && local_executable(&action.action)
    });
    let first = matches.next()?;
    matches.next().is_none().then(|| first.clone())
}

fn local_executable(path: &str) -> bool {
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
    claim: impl FnOnce() -> bool,
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
            && unique_saved(&receipt.entry, &rows).as_ref() == Some(&receipt.inner),
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
    claim: impl Fn() -> bool,
    launch: impl FnOnce(&dyn Fn() -> bool) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let failure = std::cell::RefCell::new(None);
    let result =
        launch(
            &|| match checked_entry(receipt, actions_path, history_path, &current, &claim) {
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
    ) -> Option<Receipt> {
        let index = self.rows.iter().find(|(action, _)| action == outer)?.1;
        let entry = self.snapshot.get(index)?;
        let inner = unique_saved(entry, &canonical)?;
        Some(Receipt {
            snapshot: self.snapshot.clone(),
            index,
            entry: entry.clone(),
            inner,
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
            Some(app.clone())
        );
        let mut prefixed = app.clone();
        prefixed.label = format!("app {}", prefixed.label);
        assert_eq!(
            unique_saved(&entry(prefixed), &[app.clone()]),
            Some(app.clone())
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
        let receipt = projection.receipt(&outer, canonical.clone(), 7).unwrap();
        assert!(Arc::ptr_eq(&receipt.snapshot, &snapshot));
        assert_eq!(receipt.index, 0);
        assert_eq!(receipt.inner, app);
        assert_eq!(receipt.canonical_revision, 7);
        assert!(provider.projection("hi other").is_none());
        let mut wrong = outer;
        wrong.action = "history:1".into();
        assert!(projection.receipt(&wrong, canonical, 7).is_none());
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
                || true,
                |entry| {
                    if change_actions {
                        std::fs::write(&actions_path, b"[]").unwrap();
                    } else {
                        std::fs::write(&history_path, b"[]").unwrap();
                    }
                    if entry() {
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
                .receipt(&projection.rows[0].0, vec![app.clone()].into(), 1)
                .is_none()
        );
        let replay = projection
            .receipt(&projection.rows[1].0, vec![app].into(), 1)
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
