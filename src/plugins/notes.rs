use crate::actions::Action;
use crate::plugin::Plugin;
use chrono::{Local, TimeZone};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

pub const QUICK_NOTES_FILE: &str = "quick_notes.json";

#[derive(Serialize, Deserialize, Clone)]
pub struct NoteEntry {
    pub ts: u64,
    pub text: String,
}

/// Load notes from `path`.
pub fn load_notes(path: &str) -> anyhow::Result<Vec<NoteEntry>> {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }
    let list: Vec<NoteEntry> = serde_json::from_str(&content)?;
    Ok(list)
}

/// Save `notes` to `path` in JSON format.
pub fn save_notes(path: &str, notes: &[NoteEntry]) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(notes)?;
    std::fs::write(path, json)?;
    Ok(())
}

/// Append a note with the provided `text` and current timestamp.
pub fn append_note(path: &str, text: &str) -> anyhow::Result<()> {
    let mut list = load_notes(path).unwrap_or_default();
    list.push(NoteEntry {
        ts: Local::now().timestamp() as u64,
        text: text.to_string(),
    });
    save_notes(path, &list)
}

/// Remove the note at `index` from `path`.
pub fn remove_note(path: &str, index: usize) -> anyhow::Result<()> {
    let mut list = load_notes(path).unwrap_or_default();
    if index < list.len() {
        list.remove(index);
        save_notes(path, &list)?;
    }
    Ok(())
}

fn format_ts(ts: u64) -> String {
    match Local.timestamp_opt(ts as i64, 0).single() {
        Some(dt) => dt.format("%Y-%m-%d %H:%M").to_string(),
        None => {
            tracing::warn!("invalid timestamp {ts}");
            match Local.timestamp_opt(0, 0).single() {
                Some(fallback) => fallback.format("%Y-%m-%d %H:%M").to_string(),
                None => "1970-01-01 00:00".to_string(),
            }
        }
    }
}

pub struct NotesPlugin {
    matcher: SkimMatcherV2,
    data: Arc<Mutex<Vec<NoteEntry>>>,
    #[allow(dead_code)]
    watcher: Option<RecommendedWatcher>,
}

impl NotesPlugin {
    /// Create a new notes plugin with a fuzzy matcher.
    pub fn new() -> Self {
        let data = Arc::new(Mutex::new(load_notes(QUICK_NOTES_FILE).unwrap_or_default()));
        let data_clone = data.clone();
        let path = QUICK_NOTES_FILE.to_string();
        let mut watcher = RecommendedWatcher::new(
            {
                let path = path.clone();
                move |res: notify::Result<notify::Event>| {
                    if let Ok(event) = res {
                        if matches!(
                            event.kind,
                            EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                        ) {
                            if let Ok(list) = load_notes(&path) {
                                if let Ok(mut lock) = data_clone.lock() {
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
        Self {
            matcher: SkimMatcherV2::default(),
            data,
            watcher,
        }
    }
}

impl Default for NotesPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for NotesPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        const ADD_PREFIX: &str = "note add ";
        if let Some(rest) = crate::common::strip_prefix_ci(query, ADD_PREFIX) {
            let text = rest.trim();
            if !text.is_empty() {
                return vec![Action {
                    label: format!("Add note {text}"),
                    desc: "Note".into(),
                    action: format!("note:add:{text}"),
                    args: None,
                }];
            }
        }

        const RM_PREFIX: &str = "note rm ";
        if let Some(rest) = crate::common::strip_prefix_ci(query, RM_PREFIX) {
            let filter = rest.trim();
            let notes = self
                .data
                .lock()
                .ok()
                .map(|g| g.clone())
                .unwrap_or_default();
            return notes
                .into_iter()
                .enumerate()
                .filter(|(_, n)| self.matcher.fuzzy_match(&n.text, filter).is_some())
                .map(|(idx, n)| Action {
                    label: format!("Remove note {} - {}", format_ts(n.ts), n.text),
                    desc: "Note".into(),
                    action: format!("note:remove:{idx}"),
                    args: None,
                })
                .collect();
        }

        const LIST_PREFIX: &str = "note list";
        if let Some(rest) = crate::common::strip_prefix_ci(query, LIST_PREFIX) {
            let filter = rest.trim();
            let notes = self
                .data
                .lock()
                .ok()
                .map(|g| g.clone())
                .unwrap_or_default();
            return notes
                .into_iter()
                .enumerate()
                .filter(|(_, n)| self.matcher.fuzzy_match(&n.text, filter).is_some())
                .map(|(idx, n)| Action {
                    label: format!("{} - {}", format_ts(n.ts), n.text),
                    desc: "Note".into(),
                    action: format!("note:copy:{idx}"),
                    args: None,
                })
                .collect();
        }

        if let Some(rest) = crate::common::strip_prefix_ci(query.trim(), "note") {
            if rest.is_empty() {
            return vec![Action {
                label: "note: edit notes".into(),
                desc: "Note".into(),
                action: "note:dialog".into(),
                args: None,
            }];
        }
        }

        Vec::new()
    }

    fn name(&self) -> &str {
        "notes"
    }

    fn description(&self) -> &str {
        "Quick notes (prefix: `note`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action { label: "note".into(), desc: "Note".into(), action: "query:note".into(), args: None },
            Action { label: "note add".into(), desc: "Note".into(), action: "query:note add ".into(), args: None },
            Action { label: "note list".into(), desc: "Note".into(), action: "query:note list".into(), args: None },
            Action { label: "note rm".into(), desc: "Note".into(), action: "query:note rm ".into(), args: None },
        ]
    }
}
