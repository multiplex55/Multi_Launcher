use crate::actions::Action;
use crate::plugin::Plugin;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use once_cell::sync::Lazy;
use regex::Regex;
use slug::slugify;
use std::collections::HashSet;
use std::path::{PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct Note {
    pub title: String,
    pub path: PathBuf,
    pub content: String,
    pub tags: Vec<String>,
    pub links: Vec<String>,
}

#[derive(Default)]
pub struct NoteCache {
    pub notes: Vec<Note>,
}

static TAG_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"#([A-Za-z0-9_]+)").unwrap());
static LINK_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"https?://\S+").unwrap());

fn extract_tags(content: &str) -> Vec<String> {
    TAG_RE
        .captures_iter(content)
        .map(|c| c[1].to_string())
        .collect()
}

fn extract_links(content: &str) -> Vec<String> {
    LINK_RE
        .find_iter(content)
        .map(|m| m.as_str().to_string())
        .collect()
}

fn notes_dir() -> PathBuf {
    let mut dir = dirs_next::home_dir().unwrap_or_else(|| PathBuf::from("."));
    dir.push(".multi_launcher");
    dir.push("notes");
    dir
}

pub fn load_notes() -> anyhow::Result<Vec<Note>> {
    let dir = notes_dir();
    std::fs::create_dir_all(&dir)?;
    let mut notes = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        let content = std::fs::read_to_string(&path)?;
        let title = content
            .lines()
            .next()
            .and_then(|l| l.strip_prefix("# "))
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default()
                    .replace('-', " ")
            });
        let tags = extract_tags(&content);
        let links = extract_links(&content);
        notes.push(Note {
            title,
            path,
            content,
            tags,
            links,
        });
    }
    Ok(notes)
}

pub fn save_note(note: &Note) -> anyhow::Result<()> {
    let dir = notes_dir();
    std::fs::create_dir_all(&dir)?;
    let slug = slugify(&note.title);
    let path = dir.join(format!("{slug}.md"));
    let content = if note.content.starts_with("# ") {
        note.content.clone()
    } else {
        format!("# {}\n\n{}", note.title, note.content)
    };
    std::fs::write(path, content)?;
    Ok(())
}

pub fn save_notes(notes: &[Note]) -> anyhow::Result<()> {
    let dir = notes_dir();
    std::fs::create_dir_all(&dir)?;
    let mut expected = HashSet::new();
    for note in notes {
        let slug = slugify(&note.title);
        let path = dir.join(format!("{slug}.md"));
        expected.insert(path.clone());
        let content = if note.content.starts_with("# ") {
            note.content.clone()
        } else {
            format!("# {}\n\n{}", note.title, note.content)
        };
        std::fs::write(path, content)?;
    }
    for entry in std::fs::read_dir(&dir)? {
        let path = entry?.path();
        if path.extension().and_then(|s| s.to_str()) == Some("md") && !expected.contains(&path) {
            let _ = std::fs::remove_file(path);
        }
    }
    Ok(())
}

pub fn append_note(title: &str, content: &str) -> anyhow::Result<()> {
    let note = Note {
        title: title.to_string(),
        path: PathBuf::new(),
        content: content.to_string(),
        tags: extract_tags(content),
        links: extract_links(content),
    };
    save_note(&note)
}

pub fn remove_note(index: usize) -> anyhow::Result<()> {
    let notes = load_notes()?;
    if let Some(note) = notes.get(index) {
        let _ = std::fs::remove_file(&note.path);
    }
    Ok(())
}

pub struct NotePlugin {
    matcher: SkimMatcherV2,
    data: Arc<Mutex<NoteCache>>,
}

impl NotePlugin {
    pub fn new() -> Self {
        let cache = NoteCache {
            notes: load_notes().unwrap_or_default(),
        };
        Self {
            matcher: SkimMatcherV2::default(),
            data: Arc::new(Mutex::new(cache)),
        }
    }
}

impl Default for NotePlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for NotePlugin {
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
            let guard = match self.data.lock() {
                Ok(g) => g,
                Err(_) => return Vec::new(),
            };
            return guard
                .notes
                .iter()
                .enumerate()
                .filter(|(_, n)| self.matcher.fuzzy_match(&n.title, filter).is_some())
                .map(|(idx, n)| Action {
                    label: format!("Remove note {}", n.title),
                    desc: "Note".into(),
                    action: format!("note:remove:{idx}"),
                    args: None,
                })
                .collect();
        }

        const LIST_PREFIX: &str = "note list";
        if let Some(rest) = crate::common::strip_prefix_ci(query, LIST_PREFIX) {
            let filter = rest.trim();
            let guard = match self.data.lock() {
                Ok(g) => g,
                Err(_) => return Vec::new(),
            };
            return guard
                .notes
                .iter()
                .enumerate()
                .filter(|(_, n)| self.matcher.fuzzy_match(&n.title, filter).is_some())
                .map(|(idx, n)| Action {
                    label: n.title.clone(),
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
        "Notes (prefix: `note`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "note".into(),
                desc: "Note".into(),
                action: "query:note".into(),
                args: None,
            },
            Action {
                label: "note add".into(),
                desc: "Note".into(),
                action: "query:note add ".into(),
                args: None,
            },
            Action {
                label: "note list".into(),
                desc: "Note".into(),
                action: "query:note list".into(),
                args: None,
            },
            Action {
                label: "note rm".into(),
                desc: "Note".into(),
                action: "query:note rm ".into(),
                args: None,
            },
        ]
    }
}

