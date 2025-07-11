use crate::actions::Action;
use crate::plugin::Plugin;
use chrono::{Local, TimeZone};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use serde::{Deserialize, Serialize};

pub const QUICK_NOTES_FILE: &str = "quick_notes.json";

#[derive(Serialize, Deserialize, Clone)]
pub struct NoteEntry {
    pub ts: u64,
    pub text: String,
}

pub fn load_notes(path: &str) -> anyhow::Result<Vec<NoteEntry>> {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }
    let list: Vec<NoteEntry> = serde_json::from_str(&content)?;
    Ok(list)
}

pub fn save_notes(path: &str, notes: &[NoteEntry]) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(notes)?;
    std::fs::write(path, json)?;
    Ok(())
}

pub fn append_note(path: &str, text: &str) -> anyhow::Result<()> {
    let mut list = load_notes(path).unwrap_or_default();
    list.push(NoteEntry {
        ts: Local::now().timestamp() as u64,
        text: text.to_string(),
    });
    save_notes(path, &list)
}

pub fn remove_note(path: &str, index: usize) -> anyhow::Result<()> {
    let mut list = load_notes(path).unwrap_or_default();
    if index < list.len() {
        list.remove(index);
        save_notes(path, &list)?;
    }
    Ok(())
}

fn format_ts(ts: u64) -> String {
    Local
        .timestamp_opt(ts as i64, 0)
        .single()
        .unwrap_or_else(|| Local.timestamp_opt(0, 0).single().unwrap())
        .format("%Y-%m-%d %H:%M")
        .to_string()
}

pub struct NotesPlugin {
    matcher: SkimMatcherV2,
}

impl NotesPlugin {
    pub fn new() -> Self {
        Self {
            matcher: SkimMatcherV2::default(),
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
        if let Some(text) = query.strip_prefix("note add ") {
            let text = text.trim();
            if !text.is_empty() {
                return vec![Action {
                    label: format!("Add note {text}"),
                    desc: "Note".into(),
                    action: format!("note:add:{text}"),
                    args: None,
                }];
            }
        }

        if let Some(pattern) = query.strip_prefix("note rm ") {
            let filter = pattern.trim();
            let notes = load_notes(QUICK_NOTES_FILE).unwrap_or_default();
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

        if let Some(rest) = query.strip_prefix("note list") {
            let filter = rest.trim();
            let notes = load_notes(QUICK_NOTES_FILE).unwrap_or_default();
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

        if query.trim() == "note" {
            return vec![Action {
                label: "note: edit notes".into(),
                desc: "Note".into(),
                action: "note:dialog".into(),
                args: None,
            }];
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
}
