use crate::actions::Action;
use crate::common::persistence::{LoadState, PersistenceError, load_json, save_json_atomic};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Mutex, MutexGuard, RwLock};

#[derive(Serialize, Deserialize, Clone)]
pub struct HistoryEntry {
    pub query: String,
    #[serde(skip)]
    pub query_lc: String,
    pub action: Action,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub timestamp: i64,
}

const HISTORY_FILE: &str = "history.json";
pub const HISTORY_PINS_FILE: &str = "history_pins.json";
static PINS_TRANSACTION: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HistoryPin {
    pub action_id: String,
    pub label: String,
    pub desc: String,
    pub args: Option<String>,
    pub query: String,
    #[serde(default)]
    pub timestamp: i64,
}

impl HistoryPin {
    pub fn from_history(entry: &HistoryEntry) -> Self {
        Self {
            action_id: entry.action.action.clone(),
            label: entry.action.label.clone(),
            desc: entry.action.desc.clone(),
            args: entry.action.args.clone(),
            query: entry.query.clone(),
            timestamp: entry.timestamp,
        }
    }

    pub fn matches_action(&self, action: &Action) -> bool {
        self.matches_id(&action.action, action.args.as_deref())
    }

    pub fn matches_id(&self, action_id: &str, args: Option<&str>) -> bool {
        self.action_id == action_id && self.args.as_deref() == args
    }

    pub fn update_from_action(&mut self, action: &Action) -> bool {
        let mut changed = false;
        if self.label != action.label {
            self.label = action.label.clone();
            changed = true;
        }
        if self.desc != action.desc {
            self.desc = action.desc.clone();
            changed = true;
        }
        if self.args != action.args {
            self.args = action.args.clone();
            changed = true;
        }
        changed
    }
}

impl PartialEq for HistoryPin {
    fn eq(&self, other: &Self) -> bool {
        self.action_id == other.action_id && self.args == other.args
    }
}

impl Eq for HistoryPin {}

static HISTORY: Lazy<RwLock<VecDeque<HistoryEntry>>> = Lazy::new(|| {
    let hist = load_history_internal().unwrap_or_else(|e| {
        tracing::error!("failed to load history: {e}");
        VecDeque::new()
    });
    RwLock::new(hist)
});

pub fn poison_history_lock() {
    let _ = std::panic::catch_unwind(|| {
        if let Ok(_guard) = HISTORY.write() {
            panic!("poison");
        }
    });
}

fn load_history_internal() -> anyhow::Result<VecDeque<HistoryEntry>> {
    let content = std::fs::read_to_string(HISTORY_FILE).unwrap_or_default();
    if content.is_empty() {
        return Ok(VecDeque::new());
    }
    let mut list: Vec<HistoryEntry> = serde_json::from_str(&content)?;
    for e in &mut list {
        e.query_lc = e.query.to_lowercase();
    }
    Ok(list.into())
}

/// Save the current HISTORY list to `history.json`.
pub fn save_history() -> anyhow::Result<()> {
    let Some(h) = HISTORY.read().ok() else {
        return Ok(());
    };
    let list: Vec<HistoryEntry> = h.iter().cloned().collect();
    let json = serde_json::to_string_pretty(&list)?;
    std::fs::write(HISTORY_FILE, json)?;
    Ok(())
}

/// Append an entry to the history and persist the list. The `limit` parameter
/// specifies the maximum number of entries kept.
pub fn append_history(mut entry: HistoryEntry, limit: usize) -> anyhow::Result<()> {
    entry.query_lc = entry.query.to_lowercase();
    if entry.timestamp == 0 {
        entry.timestamp = chrono::Utc::now().timestamp();
    }
    {
        let Some(mut h) = HISTORY.write().ok() else {
            return Ok(());
        };
        h.push_front(entry);
        while h.len() > limit {
            h.pop_back();
        }
    }
    save_history()
}

/// Run a closure while holding a lock on the history list.
///
/// The closure receives a reference to the current list which should only be
/// used within the scope of the closure. This avoids cloning the entire
/// history for read-only operations.
pub fn with_history<R>(f: impl FnOnce(&VecDeque<HistoryEntry>) -> R) -> Option<R> {
    let h = HISTORY.read().ok()?;
    Some(f(&h))
}

/// Return a clone of the current history list.
pub fn get_history() -> VecDeque<HistoryEntry> {
    with_history(|h| h.iter().cloned().collect()).unwrap_or_default()
}

/// Clear all history entries and persist the empty list to `history.json`.
pub fn clear_history() -> anyhow::Result<()> {
    {
        let Some(mut h) = HISTORY.write().ok() else {
            return Ok(());
        };
        h.clear();
    }
    save_history()
}

pub fn load_pins(path: &str) -> anyhow::Result<Vec<HistoryPin>> {
    match load_pins_typed(path)? {
        LoadState::Missing | LoadState::Empty => Ok(Vec::new()),
        LoadState::Loaded(pins) => Ok(pins),
    }
}

pub fn load_pins_typed(
    path: impl AsRef<Path>,
) -> Result<LoadState<Vec<HistoryPin>>, PersistenceError> {
    load_json(path)
}

pub fn save_pins(path: &str, pins: &[HistoryPin]) -> anyhow::Result<()> {
    replace_pins(path, pins.to_vec()).map(|_| ())
}

pub fn replace_pins(path: &str, replacement: Vec<HistoryPin>) -> anyhow::Result<Vec<HistoryPin>> {
    update_pins(path, move |pins| {
        *pins = replacement;
        Ok(true)
    })
}

pub fn update_pins(
    path: &str,
    mutate: impl FnOnce(&mut Vec<HistoryPin>) -> anyhow::Result<bool>,
) -> anyhow::Result<Vec<HistoryPin>> {
    update_pins_with_save(path, mutate, |path, pins| {
        save_json_atomic(path, pins).map_err(Into::into)
    })
}

fn update_pins_with_save(
    path: &str,
    mutate: impl FnOnce(&mut Vec<HistoryPin>) -> anyhow::Result<bool>,
    save: impl FnOnce(&str, &[HistoryPin]) -> anyhow::Result<()>,
) -> anyhow::Result<Vec<HistoryPin>> {
    let _transaction = pins_transaction_guard();
    let mut pins = load_pins(path)?;
    if mutate(&mut pins)? {
        save(path, &pins)?;
    }
    Ok(pins)
}

fn pins_transaction_guard() -> MutexGuard<'static, ()> {
    PINS_TRANSACTION
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub fn toggle_pin(path: &str, pin: &HistoryPin) -> anyhow::Result<bool> {
    let pin = pin.clone();
    let mut now_pinned = false;
    update_pins(path, |pins| {
        if let Some(index) = pins.iter().position(|existing| existing == &pin) {
            pins.remove(index);
        } else {
            pins.push(pin);
            now_pinned = true;
        }
        Ok(true)
    })?;
    Ok(now_pinned)
}

pub fn upsert_pin(path: &str, pin: &HistoryPin) -> anyhow::Result<bool> {
    let pin = pin.clone();
    let mut added = false;
    update_pins(path, |pins| {
        if let Some(existing) = pins
            .iter_mut()
            .find(|existing| existing.matches_id(&pin.action_id, pin.args.as_deref()))
        {
            *existing = pin;
        } else {
            pins.push(pin);
            added = true;
        }
        Ok(true)
    })?;
    Ok(added)
}

pub fn remove_pin(path: &str, action_id: &str, args: Option<&str>) -> anyhow::Result<bool> {
    let action_id = action_id.to_owned();
    let args = args.map(str::to_owned);
    let mut removed = false;
    update_pins(path, |pins| {
        let Some(index) = pins
            .iter()
            .position(|pin| pin.matches_id(&action_id, args.as_deref()))
        else {
            return Ok(false);
        };
        pins.remove(index);
        removed = true;
        Ok(true)
    })?;
    Ok(removed)
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PinRecomputeReport {
    pub updated: usize,
    pub missing: usize,
}

pub fn recompute_pins<F>(path: &str, mut resolve: F) -> anyhow::Result<PinRecomputeReport>
where
    F: FnMut(&HistoryPin) -> Option<Action>,
{
    let mut report = PinRecomputeReport::default();
    update_pins(path, |pins| {
        let mut changed = false;
        for pin in pins {
            if let Some(action) = resolve(pin) {
                if pin.update_from_action(&action) {
                    report.updated += 1;
                    changed = true;
                }
            } else {
                report.missing += 1;
            }
        }
        Ok(changed)
    })?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::Action;
    use std::sync::{Arc, Barrier};
    use tempfile::tempdir;

    static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    fn pin(action_id: &str) -> HistoryPin {
        HistoryPin {
            action_id: action_id.into(),
            label: action_id.into(),
            desc: "Test".into(),
            args: None,
            query: action_id.into(),
            timestamp: 1,
        }
    }

    #[test]
    fn pin_roundtrip_and_toggle() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("pins.json");
        let pin = HistoryPin {
            action_id: "action:one".into(),
            label: "One".into(),
            desc: "Test".into(),
            args: Some("--flag".into()),
            query: "one".into(),
            timestamp: 123,
        };

        save_pins(path.to_str().unwrap(), std::slice::from_ref(&pin)).expect("save pins");
        let loaded = load_pins(path.to_str().unwrap()).expect("load pins");
        assert_eq!(loaded, vec![pin.clone()]);

        let now_pinned = toggle_pin(path.to_str().unwrap(), &pin).expect("toggle off");
        assert!(!now_pinned);
        let cleared = load_pins(path.to_str().unwrap()).expect("load after clear");
        assert!(cleared.is_empty());

        let now_pinned = toggle_pin(path.to_str().unwrap(), &pin).expect("toggle on");
        assert!(now_pinned);
        let reloaded = load_pins(path.to_str().unwrap()).expect("load after add");
        assert_eq!(reloaded, vec![pin]);
    }

    #[test]
    fn pin_identity_uses_action_id_and_args() {
        let pin = HistoryPin {
            action_id: "action:one".into(),
            label: "One".into(),
            desc: "Test".into(),
            args: Some("--flag".into()),
            query: "one".into(),
            timestamp: 1,
        };
        let same_action = HistoryPin {
            action_id: "action:one".into(),
            label: "One Updated".into(),
            desc: "Other".into(),
            args: Some("--flag".into()),
            query: "two".into(),
            timestamp: 2,
        };
        let different_args = HistoryPin {
            action_id: "action:one".into(),
            label: "One".into(),
            desc: "Test".into(),
            args: Some("--other".into()),
            query: "one".into(),
            timestamp: 1,
        };
        assert_eq!(pin, same_action);
        assert_ne!(pin, different_args);
    }

    #[test]
    fn upsert_and_recompute_pins() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("pins.json");
        let pin = HistoryPin {
            action_id: "action:one".into(),
            label: "One".into(),
            desc: "Old".into(),
            args: None,
            query: "one".into(),
            timestamp: 10,
        };
        let added = upsert_pin(path.to_str().unwrap(), &pin).expect("upsert add");
        assert!(added);

        let updated_pin = HistoryPin {
            action_id: "action:one".into(),
            label: "One Updated".into(),
            desc: "New".into(),
            args: None,
            query: "two".into(),
            timestamp: 11,
        };
        let added = upsert_pin(path.to_str().unwrap(), &updated_pin).expect("upsert update");
        assert!(!added);

        let report = recompute_pins(path.to_str().unwrap(), |pin| {
            if pin.action_id == "action:one" {
                Some(Action {
                    label: "One Fresh".into(),
                    desc: "Fresh".into(),
                    action: pin.action_id.clone(),
                    args: None,
                })
            } else {
                None
            }
        })
        .expect("recompute");
        assert_eq!(report.updated, 1);
        assert_eq!(report.missing, 0);

        let pins = load_pins(path.to_str().unwrap()).expect("reload pins");
        assert_eq!(pins.len(), 1);
        assert_eq!(pins[0].label, "One Fresh");
        assert_eq!(pins[0].desc, "Fresh");

        let removed = remove_pin(path.to_str().unwrap(), "action:one", None).expect("remove pin");
        assert!(removed);
        let pins = load_pins(path.to_str().unwrap()).expect("reload pins");
        assert!(pins.is_empty());
    }

    #[test]
    fn pin_typed_states_defaults_and_pretty_schema_are_compatible() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let dir = tempdir().unwrap();
        let missing = dir.path().join("missing.json");
        assert_eq!(load_pins_typed(&missing).unwrap(), LoadState::Missing);
        let empty = dir.path().join("empty.json");
        std::fs::write(&empty, " \r\n").unwrap();
        assert_eq!(load_pins_typed(&empty).unwrap(), LoadState::Empty);
        assert!(toggle_pin(empty.to_str().unwrap(), &pin("initialized")).unwrap());
        assert_eq!(load_pins(empty.to_str().unwrap()).unwrap().len(), 1);
        let legacy = dir.path().join("legacy.json");
        std::fs::write(
            &legacy,
            r#"[{"action_id":"old","label":"Old","desc":"","args":null,"query":"old"}]"#,
        )
        .unwrap();
        let loaded = load_pins(legacy.to_str().unwrap()).unwrap();
        assert_eq!(loaded[0].timestamp, 0);
        let saved = dir.path().join("nested").join("pins.json");
        save_pins(saved.to_str().unwrap(), &loaded).unwrap();
        assert_eq!(
            std::fs::read_to_string(saved).unwrap(),
            serde_json::to_string_pretty(&loaded).unwrap()
        );
        let malformed = dir.path().join("malformed.json");
        std::fs::write(&malformed, "{").unwrap();
        assert!(matches!(
            load_pins_typed(&malformed).unwrap_err(),
            PersistenceError::MalformedJson { .. }
        ));
        assert!(matches!(
            load_pins_typed(dir.path()).unwrap_err(),
            PersistenceError::Read { .. }
        ));
    }

    #[test]
    fn malformed_pins_reject_every_mutation_family_unchanged() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let dir = tempdir().unwrap();
        let path = dir.path().join("pins.json");
        let invalid = b"not pins";
        std::fs::write(&path, invalid).unwrap();
        let path = path.to_str().unwrap();
        assert!(toggle_pin(path, &pin("toggle")).is_err());
        assert!(upsert_pin(path, &pin("upsert")).is_err());
        assert!(remove_pin(path, "remove", None).is_err());
        assert!(recompute_pins(path, |_| None).is_err());
        assert!(save_pins(path, &[pin("replace")]).is_err());
        assert_eq!(std::fs::read(path).unwrap(), invalid);
        let unreadable = dir.path().to_str().unwrap();
        assert!(toggle_pin(unreadable, &pin("toggle")).is_err());
        assert!(upsert_pin(unreadable, &pin("upsert")).is_err());
        assert!(remove_pin(unreadable, "remove", None).is_err());
        assert!(recompute_pins(unreadable, |_| None).is_err());
        assert!(save_pins(unreadable, &[pin("replace")]).is_err());
    }

    #[test]
    fn concurrent_pin_upserts_both_survive() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let dir = tempdir().unwrap();
        let path = Arc::new(dir.path().join("pins.json").to_string_lossy().into_owned());
        let barrier = Arc::new(Barrier::new(3));
        let handles = ["first", "second"].map(|action_id| {
            let path = Arc::clone(&path);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                upsert_pin(&path, &pin(action_id)).unwrap();
            })
        });
        barrier.wait();
        for handle in handles {
            handle.join().unwrap();
        }
        let pins = load_pins(&path).unwrap();
        assert!(pins.contains(&pin("first")));
        assert!(pins.contains(&pin("second")));
    }

    #[test]
    fn failed_pin_save_retains_destination() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let dir = tempdir().unwrap();
        let path = dir.path().join("pins.json");
        let original = vec![pin("saved")];
        std::fs::write(&path, serde_json::to_vec_pretty(&original).unwrap()).unwrap();
        let result = update_pins_with_save(
            path.to_str().unwrap(),
            |pins| {
                pins.push(pin("lost"));
                Ok(true)
            },
            |_path, _pins| anyhow::bail!("deterministic save failure"),
        );
        assert!(result.is_err());
        assert_eq!(load_pins(path.to_str().unwrap()).unwrap(), original);
    }
}
