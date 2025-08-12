use crate::actions::Action;
use crate::common::slug::{register_slug, reset_slug_lookup, slugify, unique_slug};
use crate::plugin::Plugin;
use chrono::Local;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct Note {
    pub title: String,
    pub path: PathBuf,
    pub content: String,
    pub tags: Vec<String>,
    pub links: Vec<String>,
    pub slug: String,
    pub alias: Option<String>,
}

#[derive(Default)]
pub struct NoteCache {
    /// All loaded notes.
    pub notes: Vec<Note>,
    /// Unique list of tags extracted from notes.
    pub tags: Vec<String>,
    /// Map of note slug -> notes that link to it (backlinks).
    pub links: HashMap<String, Vec<String>>,
    /// Lowercased contents for simple full-text search.
    pub index: Vec<String>,
    /// Map of note alias -> note slug for quick lookup.
    pub aliases: HashMap<String, String>,
}

impl NoteCache {
    fn from_notes(notes: Vec<Note>) -> Self {
        let mut tag_set: HashSet<String> = HashSet::new();
        let mut link_map: HashMap<String, Vec<String>> = HashMap::new();
        let mut alias_map: HashMap<String, String> = HashMap::new();

        for n in &notes {
            let slug = n.slug.clone();
            for t in &n.tags {
                tag_set.insert(t.clone());
            }
            for l in &n.links {
                let entry = link_map.entry(l.clone()).or_default();
                if !entry.contains(&slug) {
                    entry.push(slug.clone());
                }
            }
            if let Some(a) = &n.alias {
                alias_map.insert(a.to_lowercase(), slug.clone());
            }
        }

        let mut tags: Vec<String> = tag_set.into_iter().collect();
        tags.sort();

        let index = notes
            .iter()
            .map(|n| {
                let mut txt = n.content.to_lowercase();
                if let Some(a) = &n.alias {
                    txt.push('\n');
                    txt.push_str(&a.to_lowercase());
                }
                txt
            })
            .collect();

        Self {
            notes,
            tags,
            links: link_map,
            index,
            aliases: alias_map,
        }
    }
}

static CACHE: Lazy<Arc<Mutex<NoteCache>>> =
    Lazy::new(|| Arc::new(Mutex::new(NoteCache::default())));

static TEMPLATE_CACHE: Lazy<Arc<Mutex<HashMap<String, String>>>> =
    Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));

static TAG_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"#([A-Za-z0-9_]+)").unwrap());
static WIKI_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\[\[([^\]]+)\]\]").unwrap());

fn extract_tags(content: &str) -> Vec<String> {
    let mut tags: Vec<String> = TAG_RE
        .captures_iter(content)
        .map(|c| c[1].to_lowercase())
        .collect();
    tags.sort();
    tags.dedup();
    tags
}

fn extract_links(content: &str) -> Vec<String> {
    let mut links: Vec<String> = WIKI_RE
        .captures_iter(content)
        .map(|c| slugify(&c[1]))
        .collect();
    links.sort();
    links.dedup();
    links
}

pub fn extract_alias(content: &str) -> Option<String> {
    content
        .lines()
        .skip(1)
        .take_while(|l| !l.trim().is_empty())
        .find_map(|l| l.strip_prefix("Alias:").map(|a| a.trim().to_string()))
}

fn templates_dir() -> PathBuf {
    dirs_next::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".multi_launcher")
        .join("templates")
}

fn load_templates() -> anyhow::Result<HashMap<String, String>> {
    let dir = templates_dir();
    let mut map = HashMap::new();
    if dir.exists() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }
            if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    map.insert(name.to_string(), content);
                }
            }
        }
    }
    Ok(map)
}

fn refresh_template_cache() -> anyhow::Result<()> {
    let templates = load_templates()?;
    if let Ok(mut guard) = TEMPLATE_CACHE.lock() {
        *guard = templates;
    }
    Ok(())
}

pub fn get_template(name: &str) -> Option<String> {
    TEMPLATE_CACHE
        .lock()
        .ok()
        .and_then(|m| m.get(name).cloned())
}

fn notes_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("ML_NOTES_DIR") {
        return PathBuf::from(dir);
    }
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("notes")
}

pub fn load_notes() -> anyhow::Result<Vec<Note>> {
    let dir = notes_dir();
    std::fs::create_dir_all(&dir)?;
    reset_slug_lookup();
    let mut notes = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        let slug = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();
        register_slug(&slug);
        let content = std::fs::read_to_string(&path)?;
        let alias = extract_alias(&content);
        let title = content
            .lines()
            .next()
            .and_then(|l| l.strip_prefix("# "))
            .map(|s| s.to_string())
            .unwrap_or_else(|| slug.replace('-', " "));
        let tags = extract_tags(&content);
        let links = extract_links(&content);
        notes.push(Note {
            title,
            path,
            content,
            tags,
            links,
            slug,
            alias,
        });
    }
    Ok(notes)
}

fn refresh_cache() -> anyhow::Result<()> {
    let notes = load_notes()?;
    let cache = NoteCache::from_notes(notes);
    if let Ok(mut guard) = CACHE.lock() {
        *guard = cache;
    }
    Ok(())
}

pub fn save_note(note: &mut Note) -> anyhow::Result<()> {
    let dir = notes_dir();
    std::fs::create_dir_all(&dir)?;
    // Ensure slug lookup is aware of existing notes
    let _ = load_notes();
    let slug = if note.slug.is_empty() {
        unique_slug(&note.title)
    } else {
        note.slug.clone()
    };
    let path = dir.join(format!("{slug}.md"));
    let mut content = if note.content.starts_with("# ") {
        note.content.clone()
    } else {
        format!("# {}\n\n{}", note.title, note.content)
    };
    if let Some(a) = &note.alias {
        if !content.lines().any(|l| l.starts_with("Alias:")) {
            let mut lines = content.lines();
            let first = lines.next().unwrap_or("");
            let rest = lines.collect::<Vec<_>>().join("\n");
            content = format!("{first}\nAlias: {a}\n{rest}");
        }
    }
    note.alias = extract_alias(&content);
    note.tags = extract_tags(&content);
    std::fs::write(path, content)?;
    refresh_cache()?;
    Ok(())
}

pub fn save_notes(notes: &[Note]) -> anyhow::Result<()> {
    let dir = notes_dir();
    std::fs::create_dir_all(&dir)?;
    reset_slug_lookup();
    for n in notes {
        if !n.slug.is_empty() {
            register_slug(&n.slug);
        }
    }
    let mut expected = HashSet::new();
    for note in notes {
        let slug = if note.slug.is_empty() {
            unique_slug(&note.title)
        } else {
            note.slug.clone()
        };
        let path = dir.join(format!("{slug}.md"));
        expected.insert(path.clone());
        let mut content = if note.content.starts_with("# ") {
            note.content.clone()
        } else {
            format!("# {}\n\n{}", note.title, note.content)
        };
        if let Some(a) = &note.alias {
            if !content.lines().any(|l| l.starts_with("Alias:")) {
                let mut lines = content.lines();
                let first = lines.next().unwrap_or("");
                let rest = lines.collect::<Vec<_>>().join("\n");
                content = format!("{first}\nAlias: {a}\n{rest}");
            }
        }
        std::fs::write(path, content)?;
    }
    for entry in std::fs::read_dir(&dir)? {
        let path = entry?.path();
        if path.extension().and_then(|s| s.to_str()) == Some("md") && !expected.contains(&path) {
            let _ = std::fs::remove_file(path);
        }
    }
    refresh_cache()?;
    Ok(())
}

pub fn append_note(title: &str, content: &str) -> anyhow::Result<()> {
    let mut note = Note {
        title: title.to_string(),
        path: PathBuf::new(),
        content: content.to_string(),
        tags: extract_tags(content),
        links: extract_links(content),
        slug: String::new(),
        alias: None,
    };
    save_note(&mut note)
}

pub fn remove_note(index: usize) -> anyhow::Result<()> {
    let notes = load_notes()?;
    if let Some(note) = notes.get(index) {
        let _ = std::fs::remove_file(&note.path);
    }
    refresh_cache()?;
    Ok(())
}

pub struct NotePlugin {
    matcher: SkimMatcherV2,
    data: Arc<Mutex<NoteCache>>,
    templates: Arc<Mutex<HashMap<String, String>>>,
}

impl NotePlugin {
    pub fn new() -> Self {
        let _ = refresh_cache();
        let _ = refresh_template_cache();
        Self {
            matcher: SkimMatcherV2::default(),
            data: CACHE.clone(),
            templates: TEMPLATE_CACHE.clone(),
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
                let mut actions = vec![Action {
                    label: "note: edit notes".into(),
                    desc: "Note".into(),
                    action: "note:dialog".into(),
                    args: None,
                }];
                actions.extend([
                    Action {
                        label: "note search".into(),
                        desc: "Note".into(),
                        action: "query:note search ".into(),
                        args: None,
                    },
                    Action {
                        label: "note list".into(),
                        desc: "Note".into(),
                        action: "query:note list".into(),
                        args: None,
                    },
                    Action {
                        label: "note tags".into(),
                        desc: "Note".into(),
                        action: "query:note tags".into(),
                        args: None,
                    },
                    Action {
                        label: "note templates".into(),
                        desc: "Note".into(),
                        action: "query:note templates".into(),
                        args: None,
                    },
                    Action {
                        label: "note new".into(),
                        desc: "Note".into(),
                        action: "query:note new ".into(),
                        args: None,
                    },
                    Action {
                        label: "note add".into(),
                        desc: "Note".into(),
                        action: "query:note add ".into(),
                        args: None,
                    },
                    Action {
                        label: "note open".into(),
                        desc: "Note".into(),
                        action: "query:note open ".into(),
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
                        label: "note rm".into(),
                        desc: "Note".into(),
                        action: "query:note rm ".into(),
                        args: None,
                    },
                ]);
                return actions;
            }

            let mut parts = rest.splitn(2, ' ');
            let cmd = parts.next().unwrap_or("").to_ascii_lowercase();
            let args = parts.next().unwrap_or("").trim();

            let guard = match self.data.lock() {
                Ok(g) => g,
                Err(_) => return Vec::new(),
            };

            match cmd.as_str() {
                "new" | "add" => {
                    if !args.is_empty() {
                        let mut title = args;
                        let mut template = None;
                        if let Some(idx) = args.to_ascii_lowercase().find("--template") {
                            let (t, rest) = args.split_at(idx);
                            title = t.trim();
                            let mut iter = rest["--template".len()..].trim().split_whitespace();
                            if let Some(name) = iter.next() {
                                template = Some(name.to_string());
                            }
                        }
                        if !title.is_empty() {
                            let slug = slugify(title);
                            let action = if let Some(tpl) = template {
                                format!("note:new:{slug}:{tpl}")
                            } else {
                                format!("note:new:{slug}")
                            };
                            return vec![Action {
                                label: format!("New note {title}"),
                                desc: "Note".into(),
                                action,
                                args: None,
                            }];
                        }
                    }
                }
                "open" => {
                    let filter = args;
                    return guard
                        .notes
                        .iter()
                        .filter(|n| {
                            self.matcher.fuzzy_match(&n.title, filter).is_some()
                                || n
                                    .alias
                                    .as_ref()
                                    .and_then(|a| self.matcher.fuzzy_match(a, filter))
                                    .is_some()
                        })
                        .map(|n| Action {
                            label: n
                                .alias
                                .as_ref()
                                .unwrap_or(&n.title)
                                .clone(),
                            desc: "Note".into(),
                            action: format!("note:open:{}", n.slug),
                            args: None,
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
                                    || n
                                        .alias
                                        .as_ref()
                                        .and_then(|a| self.matcher.fuzzy_match(a, filter))
                                        .is_some()
                            }
                        })
                        .map(|n| Action {
                            label: n
                                .alias
                                .as_ref()
                                .unwrap_or(&n.title)
                                .clone(),
                            desc: "Note".into(),
                            action: format!("note:open:{}", n.slug),
                            args: None,
                        })
                        .collect();
                }
                "search" => {
                    let filter = args.to_lowercase();
                    let mut matches: Vec<(i64, &Note)> = guard
                        .index
                        .iter()
                        .zip(guard.notes.iter())
                        .filter_map(|(text, note)| {
                            self.matcher
                                .fuzzy_match(text, &filter)
                                .map(|score| (score, note))
                        })
                        .collect();
                    matches.sort_by(|a, b| b.0.cmp(&a.0));
                    return matches
                        .into_iter()
                        .map(|(_, n)| Action {
                            label: n
                                .alias
                                .as_ref()
                                .unwrap_or(&n.title)
                                .clone(),
                            desc: "Note".into(),
                            action: format!("note:open:{}", n.slug),
                            args: None,
                        })
                        .collect();
                }
                "tags" => {
                    let filter = args;
                    let tags = guard
                        .tags
                        .iter()
                        .filter(|t| {
                            filter.is_empty() || self.matcher.fuzzy_match(t, filter).is_some()
                        })
                        .cloned()
                        .collect::<Vec<_>>();
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
                    let tmpl = self.templates.lock().ok().and_then(|t| {
                        if t.contains_key("today") {
                            Some("today")
                        } else if t.contains_key("default") {
                            Some("default")
                        } else {
                            None
                        }
                    });
                    let action = if let Some(t) = tmpl {
                        format!("note:new:{slug}:{t}")
                    } else {
                        format!("note:open:{slug}")
                    };
                    return vec![Action {
                        label: format!("Open {slug}"),
                        desc: "Note".into(),
                        action,
                        args: None,
                    }];
                }
                "link" => {
                    let target = slugify(args);
                    if let Some(back) = guard.links.get(&target) {
                        return back
                            .iter()
                            .filter_map(|slug| {
                                guard
                                    .notes
                                    .iter()
                                    .find(|n| n.slug == *slug)
                            .map(|n| Action {
                                label: n
                                    .alias
                                    .as_ref()
                                    .unwrap_or(&n.title)
                                    .clone(),
                                desc: "Note".into(),
                                action: format!("note:open:{slug}"),
                                args: None,
                            })
                            })
                            .collect();
                    }
                    return Vec::new();
                }
                "rm" => {
                    let filter = args;
                    return guard
                        .notes
                        .iter()
                        .filter(|n| {
                            self.matcher.fuzzy_match(&n.title, filter).is_some()
                                || n
                                    .alias
                                    .as_ref()
                                    .and_then(|a| self.matcher.fuzzy_match(a, filter))
                                    .is_some()
                        })
                        .map(|n| Action {
                            label: format!(
                                "Remove {}",
                                n.alias.as_ref().unwrap_or(&n.title)
                            ),
                            desc: "Note".into(),
                            action: format!("note:remove:{}", n.slug),
                            args: None,
                        })
                        .collect();
                }
                "templates" => {
                    let filter = args;
                    if let Ok(tpl) = self.templates.lock() {
                        return tpl
                            .keys()
                            .filter(|name| {
                                if filter.is_empty() {
                                    true
                                } else {
                                    self.matcher.fuzzy_match(name, filter).is_some()
                                }
                            })
                            .map(|name| Action {
                                label: name.clone(),
                                desc: "Note".into(),
                                action: format!("query:note new --template {name} "),
                                args: None,
                            })
                            .collect();
                    }
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
                label: "note add".into(),
                desc: "Note".into(),
                action: "query:note add ".into(),
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
                label: "note templates".into(),
                desc: "Note".into(),
                action: "query:note templates".into(),
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
                label: "note rm".into(),
                desc: "Note".into(),
                action: "query:note rm ".into(),
                args: None,
            },
        ]
    }
}
