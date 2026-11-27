//! Profile-bound, read-only watched Todo snapshots and exact checked toggles.
use super::*;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Debug)]
pub struct CheckedTodoSnapshot {
    pub entries: Arc<[TodoEntry]>,
    pub generation: u64,
    pub status: Option<String>,
    pub actionable: bool,
    source: Option<Arc<Vec<u8>>>,
    writable: Arc<[bool]>,
}

struct UniqueRawObject(BTreeMap<String, Box<serde_json::value::RawValue>>);
impl<'de> serde::Deserialize<'de> for UniqueRawObject {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = UniqueRawObject;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a Todo object without duplicate keys")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut map: M,
            ) -> Result<Self::Value, M::Error> {
                let mut entries = BTreeMap::new();
                while let Some((key, value)) =
                    map.next_entry::<String, Box<serde_json::value::RawValue>>()?
                {
                    if entries.insert(key, value).is_some() {
                        return Err(serde::de::Error::custom("duplicate Todo key"));
                    }
                }
                Ok(UniqueRawObject(entries))
            }
        }
        deserializer.deserialize_map(Visitor)
    }
}

fn row_writable(raw: &serde_json::value::RawValue) -> bool {
    let text = raw.get().trim_start();
    if text.starts_with('{') {
        serde_json::from_str::<UniqueRawObject>(text).is_ok()
    } else if text.starts_with('[') {
        serde_json::from_str::<Vec<Box<serde_json::value::RawValue>>>(text)
            .is_ok_and(|values| values.len() >= 3)
    } else {
        false
    }
}

fn writable_rows(bytes: &[u8]) -> anyhow::Result<Arc<[bool]>> {
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Ok(Arc::from([]));
    }
    let rows: Vec<Box<serde_json::value::RawValue>> = serde_json::from_slice(bytes)?;
    Ok(Arc::from(
        rows.iter().map(|row| row_writable(row)).collect::<Vec<_>>(),
    ))
}

#[derive(Clone, Debug)]
pub struct CheckedTodoReceipt {
    snapshot: Arc<CheckedTodoSnapshot>,
    index: usize,
    entry: TodoEntry,
    action: Action,
}

pub struct CheckedTodoOwner {
    path: PathBuf,
    active: AtomicBool,
    seen_loaded: AtomicBool,
    state: RwLock<Arc<CheckedTodoSnapshot>>,
    watcher: Mutex<Option<JsonWatcher>>,
    callback_gate: Mutex<()>,
    operation_gate: Mutex<()>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl CheckedTodoOwner {
    /// Start off the UI thread. This never invokes the legacy lazy migrator.
    pub fn start(
        root: &crate::platform::app_data::AppDataRoot,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> anyhow::Result<Arc<Self>> {
        let path = root.path().join(TODO_FILE);
        let owner = Arc::new(Self {
            path: path.clone(),
            active: AtomicBool::new(true),
            seen_loaded: AtomicBool::new(false),
            state: RwLock::new(Arc::new(CheckedTodoSnapshot {
                entries: Arc::from([]),
                generation: 0,
                status: None,
                actionable: false,
                source: None,
                writable: Arc::from([]),
            })),
            watcher: Mutex::new(None),
            callback_gate: Mutex::new(()),
            operation_gate: Mutex::new(()),
            wake,
        });
        let weak = Arc::downgrade(&owner);
        let watcher = watch_json(&path, move || {
            if let Some(owner) = weak.upgrade() {
                let Ok(_gate) = owner.callback_gate.lock() else {
                    return;
                };
                if owner.active.load(Ordering::Acquire) {
                    // `watch_json` already holds the repair read permit here.
                    owner.refresh_inner(None);
                }
            }
        })?;
        *owner
            .watcher
            .lock()
            .map_err(|_| anyhow::anyhow!("Todo watcher unavailable"))? = Some(watcher);
        owner.refresh();
        Ok(owner)
    }

    pub fn snapshot(&self) -> Arc<CheckedTodoSnapshot> {
        self.state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn read_source(&self) -> anyhow::Result<Option<Vec<u8>>> {
        match std::fs::read(&self.path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn refresh(&self) {
        let permit = crate::common::repair_watch::require_path_read(&self.path);
        let error = permit.as_ref().err().map(ToString::to_string);
        self.refresh_inner(error);
    }

    fn refresh_inner(&self, permit_error: Option<String>) {
        let _transaction = todo_transaction_guard();
        let result = (|| -> anyhow::Result<Option<(Vec<u8>, Vec<TodoEntry>, Arc<[bool]>)>> {
            if let Some(error) = &permit_error {
                anyhow::bail!(error.clone());
            }
            let Some(bytes) = self.read_source()? else {
                return Ok(None);
            };
            let entries = if bytes.iter().all(u8::is_ascii_whitespace) {
                Vec::new()
            } else {
                serde_json::from_slice(&bytes)?
            };
            let writable = writable_rows(&bytes)?;
            Ok(Some((bytes, entries, writable)))
        })();
        if !self.active.load(Ordering::Acquire) {
            return;
        }
        let mut state = self
            .state
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let prior = state.clone();
        let (entries, source, status, actionable, writable): (
            Arc<[TodoEntry]>,
            Option<Arc<Vec<u8>>>,
            Option<String>,
            bool,
            Arc<[bool]>,
        ) = match result {
            Ok(Some((bytes, entries, writable))) => {
                self.seen_loaded.store(true, Ordering::Release);
                (
                    Arc::from(entries),
                    Some(Arc::new(bytes)),
                    None,
                    true,
                    writable,
                )
            }
            Ok(None) if !self.seen_loaded.load(Ordering::Acquire) => {
                (Arc::from([]), None, None, true, Arc::from([]))
            }
            Ok(None) => (
                prior.entries.clone(),
                prior.source.clone(),
                Some("Todo file was removed; retaining the last valid list.".into()),
                false,
                prior.writable.clone(),
            ),
            Err(error) => (
                prior.entries.clone(),
                prior.source.clone(),
                Some(format!("Could not read Todo list: {error}")),
                false,
                prior.writable.clone(),
            ),
        };
        if prior.entries == entries
            && prior.source == source
            && prior.status == status
            && prior.actionable == actionable
        {
            return;
        }
        *state = Arc::new(CheckedTodoSnapshot {
            entries,
            generation: prior.generation.wrapping_add(1).max(1),
            status,
            actionable,
            source,
            writable,
        });
        drop(state);
        (self.wake)();
    }

    pub fn receipt(
        &self,
        snapshot: &Arc<CheckedTodoSnapshot>,
        action: &Action,
    ) -> Option<CheckedTodoReceipt> {
        if !self.active.load(Ordering::Acquire)
            || !snapshot.actionable
            || !Arc::ptr_eq(&self.snapshot(), snapshot)
        {
            return None;
        }
        let index = action
            .action
            .strip_prefix("todo:done:")?
            .parse::<usize>()
            .ok()?;
        let entry = snapshot.entries.get(index)?.clone();
        if !snapshot.writable.get(index).copied().unwrap_or(false) {
            return None;
        }
        if entry.id.trim().is_empty()
            || snapshot
                .entries
                .iter()
                .filter(|row| row.id == entry.id)
                .take(2)
                .count()
                != 1
        {
            return None;
        }
        let expected = Action {
            label: format!("{} {}", if entry.done { "[x]" } else { "[ ]" }, entry.text),
            desc: "Todo".into(),
            action: format!("todo:done:{index}"),
            args: None,
        };
        if *action != expected {
            return None;
        }
        Some(CheckedTodoReceipt {
            snapshot: snapshot.clone(),
            index,
            entry,
            action: action.clone(),
        })
    }

    /// Reread the exact source and recheck the selected preimage at the durable rename boundary.
    pub fn toggle(
        &self,
        receipt: &CheckedTodoReceipt,
        current: impl Fn() -> bool,
    ) -> anyhow::Result<Arc<CheckedTodoSnapshot>> {
        let _operation = self
            .operation_gate
            .lock()
            .map_err(|_| anyhow::anyhow!("Todo operation unavailable"))?;
        let _permit = crate::common::repair_watch::require_path_read(&self.path)?;
        let _transaction = todo_transaction_guard();
        let valid = || {
            self.active.load(Ordering::Acquire)
                && current()
                && Arc::ptr_eq(&self.snapshot(), &receipt.snapshot)
        };
        anyhow::ensure!(
            valid() && self.receipt(&receipt.snapshot, &receipt.action).is_some(),
            "Todo selection is no longer current"
        );
        let source = self
            .read_source()?
            .ok_or_else(|| anyhow::anyhow!("Todo file is missing"))?;
        anyhow::ensure!(
            receipt.snapshot.source.as_deref() == Some(&source),
            "Todo file changed; refresh the list"
        );
        let mut entries: Vec<TodoEntry> = serde_json::from_slice(&source)?;
        anyhow::ensure!(
            entries.get(receipt.index) == Some(&receipt.entry),
            "Todo entry changed; refresh the list"
        );
        let mut raw: Vec<Box<serde_json::value::RawValue>> = serde_json::from_slice(&source)?;
        entries[receipt.index].done = !entries[receipt.index].done;
        let done = entries[receipt.index].done.to_string();
        let text = raw[receipt.index].get().trim_start();
        let updated = if text.starts_with('{') {
            let mut row = serde_json::from_str::<UniqueRawObject>(text)?.0;
            row.insert(
                "done".into(),
                serde_json::value::RawValue::from_string(done)?,
            );
            serde_json::to_string(&row)?
        } else {
            let mut row: Vec<Box<serde_json::value::RawValue>> = serde_json::from_str(text)?;
            anyhow::ensure!(row.len() >= 3, "Todo row is missing its completion field");
            row[2] = serde_json::value::RawValue::from_string(done)?;
            serde_json::to_string(&row)?
        };
        raw[receipt.index] = serde_json::value::RawValue::from_string(updated)?;
        let bytes = serde_json::to_vec_pretty(&raw)?;
        crate::common::atomic_file::save_atomic_if(&self.path, &bytes, || {
            self.read_source().ok().flatten().as_ref() == Some(&source) && valid()
        })?;
        let mut state = self
            .state
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let next = Arc::new(CheckedTodoSnapshot {
            entries: Arc::from(entries),
            generation: state.generation.wrapping_add(1).max(1),
            status: None,
            actionable: true,
            source: Some(Arc::new(bytes)),
            writable: receipt.snapshot.writable.clone(),
        });
        *state = next.clone();
        drop(state);
        if self.active.load(Ordering::Acquire) {
            (self.wake)();
        }
        Ok(next)
    }

    /// Retire the watcher and wait for any callback already in progress.
    pub fn shutdown(&self) {
        self.active.store(false, Ordering::Release);
        let _operation = self
            .operation_gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let watcher = self
            .watcher
            .lock()
            .ok()
            .and_then(|mut watcher| watcher.take());
        drop(watcher);
        drop(
            self.callback_gate
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(path: PathBuf) -> CheckedTodoOwner {
        CheckedTodoOwner {
            path,
            active: AtomicBool::new(true),
            seen_loaded: AtomicBool::new(false),
            state: RwLock::new(Arc::new(CheckedTodoSnapshot {
                entries: Arc::from([]),
                generation: 0,
                status: None,
                actionable: false,
                source: None,
                writable: Arc::from([]),
            })),
            watcher: Mutex::new(None),
            callback_gate: Mutex::new(()),
            operation_gate: Mutex::new(()),
            wake: Arc::new(|| {}),
        }
    }

    #[test]
    fn exact_toggle_preserves_siblings_rejects_stale_and_supports_both_directions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(TODO_FILE);
        std::fs::write(&path, br#"[{"id":"a","text":"Caf\u00e9","done":false,"priority":1,"tags":["work"],"opaque":{"extra":7}},{"id":"b","text":"Other","done":false}]"#).unwrap();
        let owner = fixture(path.clone());
        owner.refresh();
        let first = owner.snapshot();
        let action = project_todo_list(&first.entries, "#work").remove(0);
        assert_eq!(action.action, "todo:done:0");
        let receipt = owner.receipt(&first, &action).unwrap();
        assert!(owner.toggle(&receipt, || false).is_err());
        assert!(!owner.snapshot().entries[0].done);
        let next = owner.toggle(&receipt, || true).unwrap();
        assert!(next.entries[0].done);
        assert!(owner.toggle(&receipt, || true).is_err());
        let second = owner
            .receipt(&next, &project_todo_list(&next.entries, "#work")[0])
            .unwrap();
        assert!(!owner.toggle(&second, || true).unwrap().entries[0].done);
        let bytes = std::fs::read_to_string(path).unwrap();
        assert!(bytes.contains("\"opaque\""));
        assert!(bytes.contains("\"Other\""));
    }

    #[test]
    fn invalid_missing_and_ambiguous_ids_retain_visible_nonactionable_state() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(TODO_FILE);
        let owner = fixture(path.clone());
        owner.refresh();
        std::fs::write(&path, br#"[{"id":"same","text":"One","done":false},{"id":"same","text":"Two","done":false},{"text":"Legacy","done":false}]"#).unwrap();
        owner.refresh();
        let snapshot = owner.snapshot();
        for action in project_todo_list(&snapshot.entries, "") {
            assert!(owner.receipt(&snapshot, &action).is_none());
        }
        std::fs::write(&path, b"{bad").unwrap();
        owner.refresh();
        let invalid = owner.snapshot();
        assert_eq!(invalid.entries.len(), 3);
        assert!(!invalid.actionable);
        assert!(invalid.status.is_some());
        assert!(
            owner
                .receipt(&invalid, &project_todo_list(&invalid.entries, "")[0])
                .is_none()
        );
        std::fs::remove_file(&path).unwrap();
        owner.refresh();
        let missing = owner.snapshot();
        assert_eq!(missing.entries.len(), 3);
        assert!(!missing.actionable);
    }

    #[test]
    fn reordered_source_and_revoked_final_authority_never_toggle() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(TODO_FILE);
        let first =
            br#"[{"id":"a","text":"One","done":false},{"id":"b","text":"Two","done":false}]"#;
        let reordered =
            br#"[{"id":"b","text":"Two","done":false},{"id":"a","text":"One","done":false}]"#;
        std::fs::write(&path, first).unwrap();
        let owner = fixture(path.clone());
        owner.refresh();
        let snapshot = owner.snapshot();
        let receipt = owner
            .receipt(&snapshot, &project_todo_list(&snapshot.entries, "")[0])
            .unwrap();
        std::fs::write(&path, reordered).unwrap();
        assert!(owner.toggle(&receipt, || true).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), reordered);
        owner.refresh();
        let current = owner.snapshot();
        let receipt = owner
            .receipt(&current, &project_todo_list(&current.entries, "")[0])
            .unwrap();
        let calls = std::cell::Cell::new(0);
        assert!(
            owner
                .toggle(&receipt, || {
                    let count = calls.get();
                    calls.set(count + 1);
                    count == 0
                })
                .is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), reordered);
    }

    #[test]
    fn positional_toggle_preserves_fields_and_duplicate_object_is_read_only() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(TODO_FILE);
        let input = br#"[["a","One",false,3,["work"],[]],{"id":"b","text":"Two","done":false,"opaque":1,"opaque":2}]"#;
        std::fs::write(&path, input).unwrap();
        let owner = fixture(path.clone());
        owner.refresh();
        let snapshot = owner.snapshot();
        let actions = project_todo_list(&snapshot.entries, "");
        let one = actions
            .iter()
            .find(|action| action.action == "todo:done:0")
            .unwrap();
        let two = actions
            .iter()
            .find(|action| action.action == "todo:done:1")
            .unwrap();
        assert!(owner.receipt(&snapshot, two).is_none());
        let receipt = owner.receipt(&snapshot, one).unwrap();
        owner.toggle(&receipt, || true).unwrap();
        let raw: Vec<Box<serde_json::value::RawValue>> =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(raw[0].get(), r#"["a","One",true,3,["work"],[]]"#);
        assert_eq!(
            raw[1].get(),
            r#"{"id":"b","text":"Two","done":false,"opaque":1,"opaque":2}"#
        );
    }
}
