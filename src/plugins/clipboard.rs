use crate::actions::Action;
use crate::plugin::Plugin;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub struct ClipboardPlugin {
    history: Arc<Mutex<VecDeque<String>>>,
    max_entries: usize,
}

impl ClipboardPlugin {
    pub fn new(max_entries: usize) -> Self {
        let history = Arc::new(Mutex::new(VecDeque::new()));
        let history_clone = history.clone();
        thread::spawn(move || {
            let mut clipboard = match arboard::Clipboard::new() {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!("failed to init clipboard: {e}");
                    return;
                }
            };
            let mut last = String::new();
            loop {
                match clipboard.get_text() {
                    Ok(txt) => {
                        if txt != last {
                            let mut h = history_clone.lock().unwrap();
                            if let Some(pos) = h.iter().position(|v| v == &txt) {
                                h.remove(pos);
                            }
                            h.push_front(txt.clone());
                            while h.len() > max_entries {
                                h.pop_back();
                            }
                            last = txt;
                        }
                    }
                    Err(e) => {
                        tracing::debug!("clipboard read error: {e}");
                    }
                }
                thread::sleep(Duration::from_millis(500));
            }
        });
        Self { history, max_entries }
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
        let filter = query.strip_prefix("cb").unwrap_or("").trim();
        let history = self.history.lock().unwrap();
        history
            .iter()
            .filter(|entry| entry.contains(filter))
            .map(|entry| Action {
                label: entry.clone(),
                desc: "Clipboard".into(),
                action: format!("clipboard:{}", entry),
            })
            .collect()
    }

    fn name(&self) -> &str {
        "clipboard"
    }
}
