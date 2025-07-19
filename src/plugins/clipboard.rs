use crate::actions::Action;
use crate::plugin::Plugin;
use arboard::Clipboard;
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

pub const CLIPBOARD_FILE: &str = "clipboard_history.json";

/// Load clipboard history from `path`.
///
/// Returns an empty queue when the file is missing or empty.
pub fn load_history(path: &str) -> anyhow::Result<VecDeque<String>> {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    if content.trim().is_empty() {
        return Ok(VecDeque::new());
    }
    let list: Vec<String> = serde_json::from_str(&content)?;
    Ok(list.into())
}

/// Save the clipboard `history` to `path`.
pub fn save_history(path: &str, history: &VecDeque<String>) -> anyhow::Result<()> {
    let list: Vec<String> = history.iter().cloned().collect();
    let json = serde_json::to_string_pretty(&list)?;
    std::fs::write(path, json)?;
    Ok(())
}

/// Remove the history entry at `index` from the file at `path`.
pub fn remove_entry(path: &str, index: usize) -> anyhow::Result<()> {
    let mut history = load_history(path).unwrap_or_default();
    if index < history.len() {
        history.remove(index);
        save_history(path, &history)?;
    }
    Ok(())
}

/// Clear the clipboard history file at `path`.
pub fn clear_history_file(path: &str) -> anyhow::Result<()> {
    save_history(path, &VecDeque::new())
}

pub struct ClipboardPlugin {
    max_entries: usize,
    path: String,
    history: Arc<Mutex<VecDeque<String>>>,
    clipboard: Mutex<Option<Clipboard>>,
    #[allow(dead_code)]
    watcher: Option<RecommendedWatcher>,
}

impl ClipboardPlugin {
    /// Create a new plugin keeping up to `max_entries` in history.
    pub fn new(max_entries: usize) -> Self {
        let path = CLIPBOARD_FILE.to_string();
        let history = Arc::new(Mutex::new(load_history(&path).unwrap_or_default()));
        let history_clone = history.clone();
        let mut watcher = RecommendedWatcher::new(
            {
                let path = path.clone();
                move |res: notify::Result<notify::Event>| {
                    if let Ok(event) = res {
                        if matches!(
                            event.kind,
                            EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                        ) {
                            if let Ok(list) = load_history(&path) {
                                if let Ok(mut lock) = history_clone.lock() {
                                    *lock = list;
                                }
                            }
                        }
                    }
                }
            },
            Config::default(),
        )
        .ok();
        if let Some(w) = watcher.as_mut() {
            let p = std::path::Path::new(&path);
            if w.watch(p, RecursiveMode::NonRecursive).is_err() {
                let parent = p.parent().unwrap_or_else(|| std::path::Path::new("."));
                let _ = w.watch(parent, RecursiveMode::NonRecursive);
            }
        }
        let clipboard = Mutex::new(Clipboard::new().ok());
        Self {
            max_entries,
            path,
            history,
            clipboard,
            watcher,
        }
    }

    fn update_history(&self) -> VecDeque<String> {
        let mut history = self.history.lock().unwrap();
        let mut cb_lock = self.clipboard.lock().unwrap();

        if cb_lock.is_none() {
            match Clipboard::new() {
                Ok(cb) => {
                    *cb_lock = Some(cb);
                }
                Err(e) => {
                    tracing::error!("clipboard init error: {:?}", e);
                    return history.clone();
                }
            }
        }

        if let Some(ref mut clipboard) = *cb_lock {
            match clipboard.get_text() {
                Ok(txt) => {
                    if history.front().map(|v| v != &txt).unwrap_or(true) {
                        if let Some(pos) = history.iter().position(|v| v == &txt) {
                            history.remove(pos);
                        }
                        history.push_front(txt);
                        while history.len() > self.max_entries {
                            history.pop_back();
                        }
                        let _ = save_history(&self.path, &history);
                    }
                }
                Err(e) => {
                    tracing::error!("clipboard read error: {:?}", e);
                    *cb_lock = None;
                }
            }
        }

        history.clone()
    }
}

impl Default for ClipboardPlugin {
    fn default() -> Self {
        Self::new(20)
    }
}

impl Plugin for ClipboardPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        const PREFIX: &str = "cb";
        if crate::common::strip_prefix_ci(query, PREFIX).is_none() {
            return Vec::new();
        }

        let trimmed = query.trim();
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "cb") {
            if rest.is_empty() {
                return vec![Action {
                    label: "cb: edit clipboard".into(),
                    desc: "Clipboard".into(),
                    action: "clipboard:dialog".into(),
                    args: None,
                }];
            }
        }

        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "cb clear") {
            if rest.is_empty() {
                return vec![Action {
                    label: "Clear clipboard history".into(),
                    desc: "Clipboard".into(),
                    action: "clipboard:clear".into(),
                    args: None,
                }];
            }
        }

        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "cb list") {
            if rest.is_empty() {
                let history = self.update_history();
                return history
                    .iter()
                    .enumerate()
                    .map(|(idx, entry)| Action {
                        label: entry.clone(),
                        desc: "Clipboard".into(),
                        action: format!("clipboard:copy:{idx}"),
                        args: None,
                    })
                    .collect();
            }
        }

        let filter = trimmed[PREFIX.len()..].trim().to_lowercase();
        let history = self.update_history();
        history
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.to_lowercase().contains(&filter))
            .map(|(idx, entry)| Action {
                label: entry.clone(),
                desc: "Clipboard".into(),
                action: format!("clipboard:copy:{idx}"),
                args: None,
            })
            .collect()
    }

    fn name(&self) -> &str {
        "clipboard"
    }

    fn description(&self) -> &str {
        "Provides clipboard history search (prefix: `cb`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "cb".into(),
                desc: "Clipboard".into(),
                action: "query:cb".into(),
                args: None,
            },
            Action {
                label: "cb list".into(),
                desc: "Clipboard".into(),
                action: "query:cb list".into(),
                args: None,
            },
            Action {
                label: "cb clear".into(),
                desc: "Clipboard".into(),
                action: "query:cb clear".into(),
                args: None,
            },
        ]
    }
}
