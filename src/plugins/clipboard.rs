use crate::actions::Action;
use crate::common::json_watch::{watch_json, JsonWatcher};
use crate::plugin::Plugin;
use arboard::Clipboard;
use eframe::egui;
use serde_json;
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
    watcher: Option<JsonWatcher>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct ClipboardPluginSettings {
    pub max_entries: usize,
}

impl Default for ClipboardPluginSettings {
    fn default() -> Self {
        Self { max_entries: 20 }
    }
}

impl ClipboardPlugin {
    /// Create a new plugin keeping up to `max_entries` in history.
    pub fn new(max_entries: usize) -> Self {
        let path = CLIPBOARD_FILE.to_string();
        let history = Arc::new(Mutex::new(load_history(&path).unwrap_or_default()));
        let history_clone = history.clone();
        let watch_path = path.clone();
        let watcher = watch_json(&watch_path, {
            let watch_path = watch_path.clone();
            move || {
                if let Ok(list) = load_history(&watch_path) {
                    if let Ok(mut lock) = history_clone.lock() {
                        *lock = list;
                    }
                }
            }
        })
        .ok();
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
        let mut history = match self.history.lock().ok() {
            Some(h) => h,
            None => return VecDeque::new(),
        };
        let mut cb_lock = match self.clipboard.lock().ok() {
            Some(c) => c,
            None => return history.clone(),
        };

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

    fn default_settings(&self) -> Option<serde_json::Value> {
        serde_json::to_value(ClipboardPluginSettings {
            max_entries: self.max_entries,
        })
        .ok()
    }

    fn apply_settings(&mut self, value: &serde_json::Value) {
        if let Ok(s) = serde_json::from_value::<ClipboardPluginSettings>(value.clone()) {
            self.max_entries = s.max_entries;
        }
    }

    fn settings_ui(&mut self, ui: &mut egui::Ui, value: &mut serde_json::Value) {
        let mut cfg: ClipboardPluginSettings =
            serde_json::from_value(value.clone()).unwrap_or_default();
        ui.horizontal(|ui| {
            ui.label("Clipboard limit");
            ui.add(egui::DragValue::new(&mut cfg.max_entries).clamp_range(1..=200));
        });
        match serde_json::to_value(&cfg) {
            Ok(v) => *value = v,
            Err(e) => tracing::error!("failed to serialize clipboard settings: {e}"),
        }
        self.max_entries = cfg.max_entries;
    }
}
