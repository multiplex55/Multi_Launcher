use crate::actions::Action;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::RwLock;

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
    let content = std::fs::read_to_string(path).unwrap_or_default();
    if content.is_empty() {
        return Ok(Vec::new());
    }
    let list: Vec<HistoryPin> = serde_json::from_str(&content)?;
    Ok(list)
}

pub fn save_pins(path: &str, pins: &[HistoryPin]) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(pins)?;
    std::fs::write(path, json)?;
    Ok(())
}

pub fn toggle_pin(path: &str, pin: &HistoryPin) -> anyhow::Result<bool> {
    let mut pins = load_pins(path).unwrap_or_default();
    if let Some(idx) = pins.iter().position(|p| p == pin) {
        pins.remove(idx);
        save_pins(path, &pins)?;
        Ok(false)
    } else {
        pins.push(pin.clone());
        save_pins(path, &pins)?;
        Ok(true)
    }
}

pub fn upsert_pin(path: &str, pin: &HistoryPin) -> anyhow::Result<bool> {
    let mut pins = load_pins(path).unwrap_or_default();
    if let Some(existing) = pins
        .iter_mut()
        .find(|p| p.matches_id(&pin.action_id, pin.args.as_deref()))
    {
        existing.label = pin.label.clone();
        existing.desc = pin.desc.clone();
        existing.args = pin.args.clone();
        existing.query = pin.query.clone();
        existing.timestamp = pin.timestamp;
        save_pins(path, &pins)?;
        Ok(false)
    } else {
        pins.push(pin.clone());
        save_pins(path, &pins)?;
        Ok(true)
    }
}

pub fn remove_pin(path: &str, action_id: &str, args: Option<&str>) -> anyhow::Result<bool> {
    let mut pins = load_pins(path).unwrap_or_default();
    if let Some(idx) = pins.iter().position(|p| p.matches_id(action_id, args)) {
        pins.remove(idx);
        save_pins(path, &pins)?;
        Ok(true)
    } else {
        Ok(false)
    }
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
    let mut pins = load_pins(path).unwrap_or_default();
    let mut report = PinRecomputeReport::default();
    let mut changed = false;
    for pin in &mut pins {
        if let Some(action) = resolve(pin) {
            if pin.update_from_action(&action) {
                report.updated += 1;
                changed = true;
            }
        } else {
            report.missing += 1;
        }
    }
    if changed {
        save_pins(path, &pins)?;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::{
        load_pins, recompute_pins, remove_pin, save_pins, toggle_pin, upsert_pin, HistoryPin,
    };
    use crate::actions::Action;
    use tempfile::tempdir;

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

        save_pins(path.to_str().unwrap(), &[pin.clone()]).expect("save pins");
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
}
