use crate::actions::Action;
use crate::common::query::parse_query_filters;
use crate::common::slug::{register_slug, reset_slug_lookup, slugify, unique_slug};
use crate::plugin::Plugin;
use chrono::Local;
use eframe::egui;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoteExternalOpen {
    Neither,
    Powershell,
    Notepad,
    Wezterm,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotePluginSettings {
    pub external_open: NoteExternalOpen,
}

impl Default for NotePluginSettings {
    fn default() -> Self {
        Self {
            external_open: NoteExternalOpen::Wezterm,
        }
    }
}

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
        let mut notes = notes;
        let mut tag_set: HashSet<String> = HashSet::new();
        let mut link_map: HashMap<String, Vec<String>> = HashMap::new();
        let mut alias_map: HashMap<String, String> = HashMap::new();

        for n in &mut notes {
            if n.tags.is_empty() {
                n.tags = extract_tags(&n.content);
            } else {
                n.tags = n.tags.iter().map(|t| t.to_lowercase()).collect();
            }
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

static TAG_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?:[#@])([A-Za-z0-9_]+)").unwrap());
static WIKI_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\[\[([^\]]+)\]\]").unwrap());
// Matches markdown image syntax `![alt](path)` capturing the path portion.
static IMAGE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"!\[[^\]]*\]\(([^)]+)\)").unwrap());
static NOTE_VERSION: AtomicU64 = AtomicU64::new(0);

fn extract_tags(content: &str) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    let mut in_code = false;
    for line in content.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            continue;
        }
        for cap in TAG_RE.captures_iter(line) {
            if let Some(tag) = cap.get(1) {
                tags.push(tag.as_str().to_lowercase());
            }
        }
    }
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

pub fn notes_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("ML_NOTES_DIR") {
        return PathBuf::from(dir);
    }
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("notes")
}

/// Path to the assets directory inside the notes folder.
///
/// This directory stores images referenced from notes. The directory is
/// created if it does not already exist.
pub fn assets_dir() -> PathBuf {
    let dir = notes_dir().join("assets");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Return a sorted list of image file names located in [`assets_dir`].
///
/// Only common image extensions are considered.
pub fn image_files() -> Vec<String> {
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(assets_dir()) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                    let ext = ext.to_ascii_lowercase();
                    // Only allow formats supported by `egui`/`image` for rendering.
                    if matches!(
                        ext.as_str(),
                        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp"
                    ) {
                        if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                            files.push(name.to_string());
                        }
                    }
                }
            }
        }
    }
    files.sort();
    files
}

/// Return a list of asset filenames that are not referenced by any note.
///
/// This scans all notes for markdown image links using [`IMAGE_RE`] and
/// compares the referenced files to the contents of [`assets_dir`]. Only files
/// directly inside the assets directory are considered.
pub fn unused_assets() -> Vec<String> {
    let mut referenced = HashSet::new();
    if let Ok(notes) = load_notes() {
        for note in notes {
            for cap in IMAGE_RE.captures_iter(&note.content) {
                let target = cap.get(1).map(|m| m.as_str()).unwrap_or("");
                // Remove optional width specifier like `path|300`
                let (path, _) = target.split_once('|').unwrap_or((target, ""));
                if let Some(stripped) = path.strip_prefix("assets/") {
                    if let Some(name) = std::path::Path::new(stripped)
                        .file_name()
                        .and_then(|s| s.to_str())
                    {
                        referenced.insert(name.to_string());
                    }
                }
            }
        }
    }
    image_files()
        .into_iter()
        .filter(|f| !referenced.contains(f))
        .collect()
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

pub fn refresh_cache() -> anyhow::Result<()> {
    let notes = load_notes()?;
    let cache = NoteCache::from_notes(notes);
    if let Ok(mut guard) = CACHE.lock() {
        *guard = cache;
    }
    bump_note_version();
    Ok(())
}

fn bump_note_version() {
    NOTE_VERSION.fetch_add(1, Ordering::SeqCst);
}

pub fn note_version() -> u64 {
    NOTE_VERSION.load(Ordering::SeqCst)
}

/// Return a list of all unique tags from the cached notes.
pub fn available_tags() -> Vec<String> {
    CACHE.lock().map(|c| c.tags.clone()).unwrap_or_default()
}

/// Persist a single note to disk.
///
/// Returns `Ok(true)` when the note was written successfully. If `overwrite`
/// is `false` and a different note already exists at the target path, the
/// function returns `Ok(false)` without modifying the file system or the note
/// itself.
pub fn save_note(note: &mut Note, overwrite: bool) -> anyhow::Result<bool> {
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
    if path.exists() && note.path != path && !overwrite {
        return Ok(false);
    }
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
    std::fs::write(&path, content)?;
    if !note.path.as_os_str().is_empty() && note.path != path {
        let _ = std::fs::remove_file(&note.path);
    }
    note.path = path;
    note.slug = slug;
    refresh_cache()?;
    Ok(true)
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
    save_note(&mut note, true).map(|_| ())
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
    external_open: NoteExternalOpen,
    #[allow(dead_code)]
    watcher: Option<RecommendedWatcher>,
}

impl NotePlugin {
    pub fn new() -> Self {
        let _ = refresh_cache();
        let _ = refresh_template_cache();
        let data = CACHE.clone();
        let templates = TEMPLATE_CACHE.clone();
        let dir = notes_dir();
        let _ = std::fs::create_dir_all(&dir);
        let watcher = RecommendedWatcher::new(
            move |res: notify::Result<notify::Event>| {
                if let Ok(event) = res {
                    if matches!(
                        event.kind,
                        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                    ) {
                        let _ = refresh_cache();
                    }
                }
            },
            Config::default(),
        )
        .ok()
        .and_then(|mut w| {
            if w.watch(&dir, RecursiveMode::NonRecursive)
                .or_else(|_| {
                    dir.parent()
                        .map(|p| w.watch(p, RecursiveMode::NonRecursive))
                        .unwrap_or(Ok(()))
                })
                .is_ok()
            {
                Some(w)
            } else {
                None
            }
        });
        Self {
            matcher: SkimMatcherV2::default(),
            data,
            templates,
            external_open: NoteExternalOpen::Wezterm,
            watcher,
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
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "notes")
            .or_else(|| crate::common::strip_prefix_ci(trimmed, "note"))
        {
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
                        label: "note tag".into(),
                        desc: "Note".into(),
                        action: "query:note tag".into(),
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
                    Action {
                        label: "note reload".into(),
                        desc: "Note".into(),
                        action: "note:reload".into(),
                        args: None,
                    },
                    Action {
                        label: "notes unused".into(),
                        desc: "Note".into(),
                        action: "note:unused_assets".into(),
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
                "reload" => {
                    if args.is_empty() {
                        return vec![Action {
                            label: "Reload notes".into(),
                            desc: "Note".into(),
                            action: "note:reload".into(),
                            args: None,
                        }];
                    }
                }
                "new" | "add" | "create" => {
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
                                || n.alias
                                    .as_ref()
                                    .and_then(|a| self.matcher.fuzzy_match(a, filter))
                                    .is_some()
                        })
                        .map(|n| Action {
                            label: n.alias.as_ref().unwrap_or(&n.title).clone(),
                            desc: "Note".into(),
                            action: format!("note:open:{}", n.slug),
                            args: None,
                        })
                        .collect();
                }
                "list" => {
                    let mut filters = parse_query_filters(args, &["@", "#", "tag:"]);
                    filters.include_tags = filters
                        .include_tags
                        .into_iter()
                        .map(|tag| tag.to_lowercase())
                        .collect();
                    filters.exclude_tags = filters
                        .exclude_tags
                        .into_iter()
                        .map(|tag| tag.to_lowercase())
                        .collect();
                    let text_filter = filters.remaining_tokens.join(" ");
                    return guard
                        .notes
                        .iter()
                        .filter(|n| {
                            let tag_ok = if filters.include_tags.is_empty() {
                                true
                            } else {
                                filters
                                    .include_tags
                                    .iter()
                                    .all(|tag| n.tags.iter().any(|t| t == tag))
                            };
                            let exclude_ok = !filters
                                .exclude_tags
                                .iter()
                                .any(|tag| n.tags.iter().any(|t| t == tag));
                            let text_ok = if text_filter.is_empty() {
                                true
                            } else {
                                let matches =
                                    self.matcher.fuzzy_match(&n.title, &text_filter).is_some()
                                        || n.alias
                                            .as_ref()
                                            .and_then(|a| self.matcher.fuzzy_match(a, &text_filter))
                                            .is_some();
                                if filters.negate_text {
                                    !matches
                                } else {
                                    matches
                                }
                            };
                            tag_ok && exclude_ok && text_ok
                        })
                        .map(|n| Action {
                            label: n.alias.as_ref().unwrap_or(&n.title).clone(),
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
                            label: n.alias.as_ref().unwrap_or(&n.title).clone(),
                            desc: "Note".into(),
                            action: format!("note:open:{}", n.slug),
                            args: None,
                        })
                        .collect();
                }
                "tags" | "tag" => {
                    let mut filter = args.trim();
                    if let Some(stripped) = filter.strip_prefix("tag:") {
                        filter = stripped.trim();
                    }
                    if let Some(stripped) = filter.strip_prefix('#').or_else(|| filter.strip_prefix('@')) {
                        filter = stripped.trim();
                    }

                    let mut counts: HashMap<String, (String, usize)> = HashMap::new();
                    for note in &guard.notes {
                        for tag in &note.tags {
                            let key = tag.to_lowercase();
                            let entry = counts.entry(key).or_insert((tag.clone(), 0));
                            entry.1 += 1;
                        }
                    }

                    let mut tags: Vec<(String, usize)> = counts
                        .into_values()
                        .map(|(display, count)| (display, count))
                        .collect();

                    if !filter.is_empty() {
                        tags.retain(|(tag, _)| self.matcher.fuzzy_match(tag, filter).is_some());
                    }

                    tags.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

                    return tags
                        .into_iter()
                        .map(|(t, count)| Action {
                            label: format!("#{t} ({count})"),
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
                                        label: n.alias.as_ref().unwrap_or(&n.title).clone(),
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
                                || n.alias
                                    .as_ref()
                                    .and_then(|a| self.matcher.fuzzy_match(a, filter))
                                    .is_some()
                        })
                        .map(|n| Action {
                            label: format!("Remove {}", n.alias.as_ref().unwrap_or(&n.title)),
                            desc: "Note".into(),
                            action: format!("note:remove:{}", n.slug),
                            args: None,
                        })
                        .collect();
                }
                "unused" => {
                    if args.is_empty() {
                        return vec![Action {
                            label: "notes unused".into(),
                            desc: "Note".into(),
                            action: "note:unused_assets".into(),
                            args: None,
                        }];
                    }
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
                label: "note create".into(),
                desc: "Note".into(),
                action: "query:note create ".into(),
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
                label: "note tag".into(),
                desc: "Note".into(),
                action: "query:note tag".into(),
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
            Action {
                label: "note reload".into(),
                desc: "Note".into(),
                action: "note:reload".into(),
                args: None,
            },
            Action {
                label: "notes unused".into(),
                desc: "Note".into(),
                action: "note:unused_assets".into(),
                args: None,
            },
        ]
    }

    fn default_settings(&self) -> Option<serde_json::Value> {
        serde_json::to_value(NotePluginSettings::default()).ok()
    }

    fn apply_settings(&mut self, value: &serde_json::Value) {
        if let Ok(cfg) = serde_json::from_value::<NotePluginSettings>(value.clone()) {
            self.external_open = cfg.external_open;
        }
    }

    fn settings_ui(&mut self, ui: &mut egui::Ui, value: &mut serde_json::Value) {
        let mut cfg: NotePluginSettings = serde_json::from_value(value.clone()).unwrap_or_default();
        egui::ComboBox::from_label("Open externally")
            .selected_text(match cfg.external_open {
                NoteExternalOpen::Neither => "Neither",
                NoteExternalOpen::Powershell => "Powershell",
                NoteExternalOpen::Notepad => "Notepad",
                NoteExternalOpen::Wezterm => "WezTerm",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut cfg.external_open, NoteExternalOpen::Neither, "Neither");
                ui.selectable_value(
                    &mut cfg.external_open,
                    NoteExternalOpen::Powershell,
                    "Powershell",
                );
                ui.selectable_value(&mut cfg.external_open, NoteExternalOpen::Notepad, "Notepad");
                ui.selectable_value(&mut cfg.external_open, NoteExternalOpen::Wezterm, "WezTerm");
            });
        match serde_json::to_value(&cfg) {
            Ok(v) => *value = v,
            Err(e) => tracing::error!("failed to serialize note settings: {e}"),
        }
        self.external_open = cfg.external_open;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn set_notes(notes: Vec<Note>) -> NoteCache {
        let mut guard = CACHE.lock().expect("note cache lock poisoned");
        let original = std::mem::take(&mut *guard);
        *guard = NoteCache::from_notes(notes);
        original
    }

    fn restore_cache(original: NoteCache) {
        let mut guard = CACHE.lock().expect("note cache lock poisoned");
        *guard = original;
    }

    #[test]
    fn extract_tags_supports_hash_and_at_tags() {
        let content = "Notes about @UI and #Release.\n```\n#code-tag\n```\n";
        let tags = extract_tags(content);
        assert_eq!(tags, vec!["release", "ui"]);
    }

    #[test]
    fn note_list_supports_hash_and_at_tags() {
        let original = set_notes(vec![
            Note {
                title: "Alpha".into(),
                path: PathBuf::new(),
                content: "Working on @testing and #ui updates.".into(),
                tags: Vec::new(),
                links: Vec::new(),
                slug: "alpha".into(),
                alias: None,
            },
            Note {
                title: "Beta".into(),
                path: PathBuf::new(),
                content: "Planning @testing coverage.".into(),
                tags: Vec::new(),
                links: Vec::new(),
                slug: "beta".into(),
                alias: None,
            },
            Note {
                title: "Gamma".into(),
                path: PathBuf::new(),
                content: "Wrap up #ui and #chore items.".into(),
                tags: Vec::new(),
                links: Vec::new(),
                slug: "gamma".into(),
                alias: None,
            },
        ]);

        let plugin = NotePlugin {
            matcher: SkimMatcherV2::default(),
            data: CACHE.clone(),
            templates: TEMPLATE_CACHE.clone(),
            external_open: NoteExternalOpen::Wezterm,
            watcher: None,
        };

        let list_testing = plugin.search("note list @testing");
        let labels_testing: Vec<&str> = list_testing.iter().map(|a| a.label.as_str()).collect();
        assert_eq!(labels_testing, vec!["Alpha", "Beta"]);

        let list_testing_hash = plugin.search("note list #testing");
        let labels_testing_hash: Vec<&str> =
            list_testing_hash.iter().map(|a| a.label.as_str()).collect();
        assert_eq!(labels_testing_hash, vec!["Alpha", "Beta"]);

        let list_both = plugin.search("note list @testing @ui");
        let labels_both: Vec<&str> = list_both.iter().map(|a| a.label.as_str()).collect();
        assert_eq!(labels_both, vec!["Alpha"]);

        let list_both_hash = plugin.search("note list #testing #ui");
        let labels_both_hash: Vec<&str> =
            list_both_hash.iter().map(|a| a.label.as_str()).collect();
        assert_eq!(labels_both_hash, vec!["Alpha"]);

        restore_cache(original);
    }

    #[test]
    fn note_tag_lists_tags_and_drills_into_list() {
        let original = set_notes(vec![
            Note {
                title: "Alpha".into(),
                path: PathBuf::new(),
                content: "Working on @testing and #ui updates.".into(),
                tags: Vec::new(),
                links: Vec::new(),
                slug: "alpha".into(),
                alias: None,
            },
            Note {
                title: "Beta".into(),
                path: PathBuf::new(),
                content: "Planning @testing coverage.".into(),
                tags: Vec::new(),
                links: Vec::new(),
                slug: "beta".into(),
                alias: None,
            },
            Note {
                title: "Gamma".into(),
                path: PathBuf::new(),
                content: "Wrap up #ui and #chore items.".into(),
                tags: Vec::new(),
                links: Vec::new(),
                slug: "gamma".into(),
                alias: None,
            },
        ]);

        let plugin = NotePlugin {
            matcher: SkimMatcherV2::default(),
            data: CACHE.clone(),
            templates: TEMPLATE_CACHE.clone(),
            external_open: NoteExternalOpen::Wezterm,
            watcher: None,
        };

        let tags = plugin.search("note tag");
        let labels: Vec<&str> = tags.iter().map(|a| a.label.as_str()).collect();
        assert_eq!(labels, vec!["#testing (2)", "#ui (2)", "#chore (1)"]);

        let tags_ui = plugin.search("note tag @ui");
        let labels_ui: Vec<&str> = tags_ui.iter().map(|a| a.label.as_str()).collect();
        assert_eq!(labels_ui, vec!["#ui (2)"]);

        // Verify that the drill action uses `note list`.
        assert_eq!(tags_ui[0].action, "query:note list #ui");

        restore_cache(original);
    }
}
