use crate::actions::Action;
use crate::plugin::Plugin;
use chrono::Local;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use once_cell::sync::Lazy;
use regex::Regex;
use slug::slugify;
use std::collections::HashSet;
use std::path::PathBuf;
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
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("notes")
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
        let trimmed = query.trim();
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "note") {
            let rest = rest.trim();
            if rest.is_empty() {
                return vec![Action {
                    label: "note: edit notes".into(),
                    desc: "Note".into(),
                    action: "note:dialog".into(),
                    args: None,
                }];
            }

            let mut parts = rest.splitn(2, ' ');
            let cmd = parts.next().unwrap_or("").to_ascii_lowercase();
            let args = parts.next().unwrap_or("").trim();

            let guard = match self.data.lock() {
                Ok(g) => g,
                Err(_) => return Vec::new(),
            };

            match cmd.as_str() {
                "new" => {
                    if !args.is_empty() {
                        let slug = slugify(args);
                        return vec![Action {
                            label: format!("New note {args}"),
                            desc: "Note".into(),
                            action: format!("note:new:{slug}"),
                            args: None,
                        }];
                    }
                }
                "open" => {
                    let filter = args;
                    return guard
                        .notes
                        .iter()
                        .filter(|n| self.matcher.fuzzy_match(&n.title, filter).is_some())
                        .map(|n| {
                            let slug = slugify(&n.title);
                            Action {
                                label: n.title.clone(),
                                desc: "Note".into(),
                                action: format!("note:open:{slug}"),
                                args: None,
                            }
                        })
                        .collect();
                }
                "list" => {
                    let filter = args;
                    let tag_filter = filter.starts_with('#');
                    return guard
                        .notes
                        .iter()
                        .filter(|n| {
                            if filter.is_empty() {
                                true
                            } else if tag_filter {
                                let tag = filter.trim_start_matches('#');
                                n.tags.iter().any(|t| t.eq_ignore_ascii_case(tag))
                            } else {
                                self.matcher.fuzzy_match(&n.title, filter).is_some()
                            }
                        })
                        .map(|n| {
                            let slug = slugify(&n.title);
                            Action {
                                label: n.title.clone(),
                                desc: "Note".into(),
                                action: format!("note:open:{slug}"),
                                args: None,
                            }
                        })
                        .collect();
                }
                "search" => {
                    let filter = args;
                    return guard
                        .notes
                        .iter()
                        .filter(|n| self.matcher.fuzzy_match(&n.content, filter).is_some())
                        .map(|n| {
                            let slug = slugify(&n.title);
                            Action {
                                label: n.title.clone(),
                                desc: "Note".into(),
                                action: format!("note:open:{slug}"),
                                args: None,
                            }
                        })
                        .collect();
                }
                "tags" => {
                    let filter = args;
                    let mut tags: HashSet<String> = HashSet::new();
                    for n in &guard.notes {
                        for t in &n.tags {
                            tags.insert(t.clone());
                        }
                    }
                    let mut tags: Vec<String> = tags.into_iter().collect();
                    if !filter.is_empty() {
                        tags.retain(|t| self.matcher.fuzzy_match(t, filter).is_some());
                    }
                    tags.sort();
                    return tags
                        .into_iter()
                        .map(|t| Action {
                            label: format!("#{t}"),
                            desc: "Note".into(),
                            action: format!("query:note list #{t}"),
                            args: None,
                        })
                        .collect();
                }
                "today" => {
                    let slug = Local::now().format("%Y-%m-%d").to_string();
                    return vec![Action {
                        label: format!("Open {slug}"),
                        desc: "Note".into(),
                        action: format!("note:open:{slug}"),
                        args: None,
                    }];
                }
                "link" => {
                    let filter = args;
                    return guard
                        .notes
                        .iter()
                        .flat_map(|n| n.links.iter().cloned())
                        .filter(|l| {
                            filter.is_empty() || self.matcher.fuzzy_match(l, filter).is_some()
                        })
                        .map(|l| Action {
                            label: l.clone(),
                            desc: "Note".into(),
                            action: format!("note:link:{l}"),
                            args: None,
                        })
                        .collect();
                }
                "delete" => {
                    let filter = args;
                    return guard
                        .notes
                        .iter()
                        .filter(|n| self.matcher.fuzzy_match(&n.title, filter).is_some())
                        .map(|n| {
                            let slug = slugify(&n.title);
                            Action {
                                label: format!("Delete {}", n.title),
                                desc: "Note".into(),
                                action: format!("note:delete:{slug}"),
                                args: None,
                            }
                        })
                        .collect();
                }
                _ => {}
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
                label: "note new".into(),
                desc: "Note".into(),
                action: "query:note new ".into(),
                args: None,
            },
            Action {
                label: "note open".into(),
                desc: "Note".into(),
                action: "query:note open ".into(),
                args: None,
            },
            Action {
                label: "note list".into(),
                desc: "Note".into(),
                action: "query:note list".into(),
                args: None,
            },
            Action {
                label: "note search".into(),
                desc: "Note".into(),
                action: "query:note search ".into(),
                args: None,
            },
            Action {
                label: "note tags".into(),
                desc: "Note".into(),
                action: "query:note tags".into(),
                args: None,
            },
            Action {
                label: "note today".into(),
                desc: "Note".into(),
                action: "query:note today".into(),
                args: None,
            },
            Action {
                label: "note link".into(),
                desc: "Note".into(),
                action: "query:note link ".into(),
                args: None,
            },
            Action {
                label: "note delete".into(),
                desc: "Note".into(),
                action: "query:note delete ".into(),
                args: None,
            },
        ]
    }
}
