use crate::actions::Action;
use crate::plugin::Plugin;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

pub struct ClipboardPlugin {
    history: Arc<Mutex<VecDeque<String>>>,
    max_entries: usize,
}

impl ClipboardPlugin {
    pub fn new(max_entries: usize) -> Self {
        let history = Arc::new(Mutex::new(VecDeque::new()));
        Self { history, max_entries }
    }

    fn update_history(&self) {
        let mut clipboard = match arboard::Clipboard::new() {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("failed to init clipboard: {e}");
                return;
            }
        };
        match clipboard.get_text() {
            Ok(txt) => {
                let mut h = self.history.lock().unwrap();
                if h.front().map(|v| v != &txt).unwrap_or(true) {
                    if let Some(pos) = h.iter().position(|v| v == &txt) {
                        h.remove(pos);
                    }
                    h.push_front(txt.clone());
                    while h.len() > self.max_entries {
                        h.pop_back();
                    }
                }
            }
            Err(e) => {
                tracing::debug!("clipboard read error: {e}");
            }
        }
    }
}

impl Default for ClipboardPlugin {
    fn default() -> Self {
        Self::new(20)
    }
}

impl Plugin for ClipboardPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if !query.starts_with("cb") {
            return Vec::new();
        }
        self.update_history();
        let filter = query.strip_prefix("cb").unwrap_or("").trim();
        let history = self.history.lock().unwrap();
        history
            .iter()
            .filter(|entry| entry.contains(filter))
            .map(|entry| Action {
                label: entry.clone(),
                desc: "Clipboard".into(),
                action: format!("clipboard:{}", entry),
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
}
