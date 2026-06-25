use crate::actions::Action;
use crate::common::entity_ref::{EntityKind, EntityRef};
use crate::common::query::parse_query_filters;
use crate::common::slug::{register_slug, reset_slug_lookup, slugify, unique_slug};
use crate::linking::{
    EntityKey, LinkRef, LinkTarget, build_index_from_notes_and_todos, format_link_id,
};
use crate::plugin::Plugin;
use crate::plugins::todo::TODO_DATA;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Local;
use eframe::egui;
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
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
    #[serde(default = "default_note_backlinks_enabled")]
    pub backlinks_enabled: bool,
    #[serde(default = "default_note_aliases_enabled")]
    pub aliases_enabled: bool,
    #[serde(default = "default_note_templates_enabled")]
    pub templates_enabled: bool,
}

fn default_note_backlinks_enabled() -> bool {
    true
}

fn default_note_aliases_enabled() -> bool {
    true
}

fn default_note_templates_enabled() -> bool {
    true
}

pub fn note_plugin_settings_with_backlinks(
    value: Option<&serde_json::Value>,
    backlinks_enabled: bool,
    aliases_enabled: bool,
    templates_enabled: bool,
) -> serde_json::Value {
    let mut cfg = value
        .cloned()
        .and_then(|v| serde_json::from_value::<NotePluginSettings>(v).ok())
        .unwrap_or_default();
    cfg.backlinks_enabled = backlinks_enabled;
    cfg.aliases_enabled = aliases_enabled;
    cfg.templates_enabled = templates_enabled;
    serde_json::to_value(cfg).unwrap_or_else(|_| {
        serde_json::json!({
            "external_open": NoteExternalOpen::Wezterm,
            "backlinks_enabled": backlinks_enabled,
            "aliases_enabled": aliases_enabled,
            "templates_enabled": templates_enabled,
        })
    })
}

impl Default for NotePluginSettings {
    fn default() -> Self {
        Self {
            external_open: NoteExternalOpen::Wezterm,
            backlinks_enabled: true,
            aliases_enabled: true,
            templates_enabled: true,
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
    pub aliases: Vec<String>,
    pub entity_refs: Vec<EntityRef>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteLinkMenuTarget {
    pub slug: String,
    pub title: String,
    pub alias: Option<String>,
    pub aliases: Vec<String>,
}

impl NoteLinkMenuTarget {
    pub fn display_title(&self) -> &str {
        self.alias.as_deref().unwrap_or(&self.title)
    }

    pub fn search_text(&self) -> String {
        let mut text = self.title.clone();
        if let Some(alias) = &self.alias {
            text.push('\n');
            text.push_str(alias);
        }
        for alias in &self.aliases {
            text.push('\n');
            text.push_str(alias);
        }
        text.push('\n');
        text.push_str(&self.slug);
        text
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NoteTarget {
    Resolved(String),
    Broken,
    Ambiguous(Vec<String>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WikiReference {
    pub raw: String,
    pub target: String,
    pub resolved: NoteTarget,
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
    /// Map of lowercased note alias -> candidate note slugs for quick lookup.
    pub aliases: HashMap<String, Vec<String>>,
    /// Set of canonical note slugs for exact-existence checks.
    pub slug_set: HashSet<String>,
    /// Map of lowercased slug -> canonical slug.
    pub slug_map: HashMap<String, String>,
    /// Map of lowercased title -> candidate slugs.
    pub title_map: HashMap<String, Vec<String>>,
}

impl NoteCache {
    fn from_notes(notes: Vec<Note>) -> Self {
        let mut notes = notes;
        let mut tag_set: HashSet<String> = HashSet::new();
        let mut link_map: HashMap<String, Vec<String>> = HashMap::new();
        let mut alias_map: HashMap<String, Vec<String>> = HashMap::new();
        let mut slug_set: HashSet<String> = HashSet::new();
        let mut slug_map: HashMap<String, String> = HashMap::new();
        let mut title_map: HashMap<String, Vec<String>> = HashMap::new();

        for n in &mut notes {
            if n.tags.is_empty() {
                n.tags = extract_tags(&n.content);
            } else {
                n.tags = n.tags.iter().map(|t| t.to_lowercase()).collect();
            }
            normalize_note_aliases(n);
            for a in &n.aliases {
                let entry = alias_map.entry(a.to_lowercase()).or_default();
                if !entry.contains(&n.slug) {
                    entry.push(n.slug.clone());
                }
            }
            slug_set.insert(n.slug.clone());
            slug_map.insert(n.slug.to_lowercase(), n.slug.clone());
            title_map
                .entry(n.title.to_lowercase())
                .or_default()
                .push(n.slug.clone());
            for t in &n.tags {
                tag_set.insert(t.clone());
            }
        }

        let resolver = NoteCache {
            notes: notes.clone(),
            tags: Vec::new(),
            links: HashMap::new(),
            index: Vec::new(),
            aliases: alias_map.clone(),
            slug_set: slug_set.clone(),
            slug_map: slug_map.clone(),
            title_map: title_map.clone(),
        };

        for n in &mut notes {
            let mut resolved: Vec<String> = resolve_wiki_references(&resolver, &n.content)
                .into_iter()
                .filter_map(|r| match r.resolved {
                    NoteTarget::Resolved(slug) if slug != n.slug => Some(slug),
                    _ => None,
                })
                .collect();
            resolved.sort();
            resolved.dedup();
            n.links = resolved;

            for target_slug in &n.links {
                let entry = link_map.entry(target_slug.clone()).or_default();
                if !entry.contains(&n.slug) {
                    entry.push(n.slug.clone());
                }
            }
        }

        let mut tags: Vec<String> = tag_set.into_iter().collect();
        tags.sort();

        let index = notes
            .iter()
            .map(|n| {
                let mut txt = n.title.to_lowercase();
                txt.push('\n');
                txt.push_str(&n.slug.to_lowercase());
                if let Some(a) = &n.alias {
                    txt.push('\n');
                    txt.push_str(&a.to_lowercase());
                }
                for a in &n.aliases {
                    txt.push('\n');
                    txt.push_str(&a.to_lowercase());
                }
                txt.push('\n');
                txt.push_str(&n.content.to_lowercase());
                txt
            })
            .collect();

        Self {
            notes,
            tags,
            links: link_map,
            index,
            aliases: alias_map,
            slug_set,
            slug_map,
            title_map,
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
static LAST_NOTE_REINDEX_MS: AtomicU64 = AtomicU64::new(0);
const NOTE_REINDEX_DEBOUNCE_MS: u64 = 250;

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

fn content_without_fenced_code(content: &str) -> String {
    let mut out = String::new();
    let mut in_code = false;
    for line in content.lines() {
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
            out.push('\n');
            continue;
        }
        if !in_code {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

fn extract_links(content: &str) -> Vec<String> {
    let searchable = content_without_fenced_code(content);
    let mut links: Vec<String> = WIKI_RE
        .captures_iter(&searchable)
        .map(|c| slugify(&c[1]))
        .collect();
    links.sort();
    links.dedup();
    links
}

fn extract_entity_refs(content: &str) -> Vec<EntityRef> {
    let searchable = content_without_fenced_code(content);
    let mut refs = Vec::new();
    for token in searchable.split_whitespace() {
        let token = token.trim_matches(|c: char| ",.;()[]{}<>`".contains(c));
        if let Some(slug) = token.strip_prefix("link://note/") {
            let id = slug
                .split(['#', '?'])
                .next()
                .unwrap_or(slug)
                .trim_matches('/');
            if !id.is_empty() {
                refs.push(EntityRef::new(EntityKind::Note, id.to_string(), None));
            }
            continue;
        }
        let token = token.strip_prefix('@').unwrap_or(token);
        if let Some((kind, id)) = token.split_once(':') {
            let kind = match kind.to_ascii_lowercase().as_str() {
                "todo" => EntityKind::Todo,
                "event" => EntityKind::Event,
                "note" => EntityKind::Note,
                _ => continue,
            };
            if !id.trim().is_empty() {
                refs.push(EntityRef::new(kind, id.trim().to_string(), None));
            }
        }
    }
    refs.sort_by(|a, b| a.id.cmp(&b.id));
    refs.dedup_by(|a, b| a.kind == b.kind && a.id == b.id);
    refs
}

fn parse_wiki_references(content: &str) -> Vec<String> {
    WIKI_RE
        .captures_iter(content)
        .filter_map(|cap| cap.get(1).map(|m| m.as_str().trim().to_string()))
        .filter(|s| !s.is_empty())
        .collect()
}

fn target_from_reference(raw: &str) -> &str {
    raw.split('|').next().unwrap_or(raw).trim()
}

fn path_matches_note(path_query: &str, note: &Note) -> bool {
    let q = path_query.trim().trim_start_matches("./").to_lowercase();
    if q.is_empty() {
        return false;
    }
    let file_name = note
        .path
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();
    let full = note.path.to_string_lossy().to_lowercase();
    file_name == q || full.ends_with(&q)
}

fn resolve_target(cache: &NoteCache, query: &str) -> NoteTarget {
    let query = query.trim();
    if query.is_empty() {
        return NoteTarget::Broken;
    }
    let query_lower = query.to_lowercase();
    if let Some(slugs) = cache.aliases.get(&query_lower) {
        return match slugs.as_slice() {
            [slug] => NoteTarget::Resolved(slug.clone()),
            [] => NoteTarget::Broken,
            _ => NoteTarget::Ambiguous(slugs.clone()),
        };
    }
    if let Some(slug) = query_lower.strip_prefix("slug:") {
        let slug = slug.trim();
        return if cache.slug_set.contains(slug) {
            NoteTarget::Resolved(slug.to_string())
        } else {
            NoteTarget::Broken
        };
    }
    if let Some(path) = query_lower.strip_prefix("path:") {
        let mut matches: Vec<String> = cache
            .notes
            .iter()
            .filter(|n| path_matches_note(path, n))
            .map(|n| n.slug.clone())
            .collect();
        matches.sort();
        matches.dedup();
        return match matches.len() {
            0 => NoteTarget::Broken,
            1 => NoteTarget::Resolved(matches.remove(0)),
            _ => NoteTarget::Ambiguous(matches),
        };
    }
    if let Some(slug) = cache.slug_map.get(&query_lower) {
        return NoteTarget::Resolved(slug.clone());
    }

    if let Some(title_matches) = cache.title_map.get(&query_lower) {
        if title_matches.len() == 1 {
            return NoteTarget::Resolved(title_matches[0].clone());
        }
        if !title_matches.is_empty() {
            return NoteTarget::Ambiguous(title_matches.clone());
        }
    }

    let slug = slugify(query);
    if cache.slug_set.contains(&slug) {
        NoteTarget::Resolved(slug)
    } else {
        NoteTarget::Broken
    }
}

fn resolve_wiki_references(cache: &NoteCache, content: &str) -> Vec<WikiReference> {
    let mut refs = Vec::new();
    for raw in parse_wiki_references(content) {
        let target = target_from_reference(&raw).to_string();
        refs.push(WikiReference {
            raw,
            resolved: resolve_target(cache, &target),
            target,
        });
    }
    refs
}

fn format_link_row(
    notes: &[Note],
    todos: &[crate::plugins::todo::TodoEntry],
    link: &LinkRef,
    status: &str,
) -> Action {
    let title = match link.target_type {
        LinkTarget::Note => notes
            .iter()
            .find(|n| n.slug == link.target_id)
            .map(|n| n.alias.as_ref().unwrap_or(&n.title).clone())
            .unwrap_or_else(|| link.target_id.clone()),
        LinkTarget::Todo => todos
            .iter()
            .find(|t| t.id == link.target_id)
            .map(|t| t.text.clone())
            .unwrap_or_else(|| link.target_id.clone()),
        _ => link
            .display_text
            .clone()
            .unwrap_or_else(|| link.target_id.clone()),
    };
    let anchor = link.anchor.clone().unwrap_or_else(|| "-".into());
    let action = match link.target_type {
        LinkTarget::Note => format!("note:open:{}", link.target_id),
        LinkTarget::Todo => format!("query:todo links id:{}", link.target_id),
        _ => format!("link:open:{}", format_link_id(link)),
    };
    Action {
        label: format!(
            "type={} | title={} | target={} | anchor={} | status={}",
            match link.target_type {
                LinkTarget::Note => "note",
                LinkTarget::Todo => "todo",
                LinkTarget::Bookmark => "bookmark",
                LinkTarget::Layout => "layout",
                LinkTarget::File => "file",
            },
            title,
            format_link_id(link),
            anchor,
            status
        ),
        desc: "Links".into(),
        action,
        args: None,
    }
}

pub fn resolve_note_query(query: &str) -> NoteTarget {
    CACHE
        .lock()
        .map(|cache| resolve_target(&cache, query))
        .unwrap_or(NoteTarget::Broken)
}

pub fn note_backlinks(slug: &str) -> Vec<Note> {
    CACHE
        .lock()
        .ok()
        .map(|cache| {
            cache
                .links
                .get(slug)
                .into_iter()
                .flat_map(|v| v.iter())
                .filter_map(|s| cache.notes.iter().find(|n| n.slug == *s).cloned())
                .collect()
        })
        .unwrap_or_default()
}

pub fn note_refs_for(slug: &str) -> Vec<WikiReference> {
    CACHE
        .lock()
        .ok()
        .and_then(|cache| {
            cache
                .notes
                .iter()
                .find(|n| n.slug == slug)
                .map(|n| resolve_wiki_references(&cache, &n.content))
        })
        .unwrap_or_default()
}

pub fn note_relationship_edges() -> Vec<(String, String)> {
    CACHE
        .lock()
        .ok()
        .map(|cache| {
            let mut edges = Vec::new();
            for n in &cache.notes {
                for to in &n.links {
                    edges.push((n.slug.clone(), to.clone()));
                }
            }
            edges
        })
        .unwrap_or_default()
}

pub fn extract_alias(content: &str) -> Option<String> {
    extract_aliases(content).into_iter().next()
}

pub fn extract_aliases(content: &str) -> Vec<String> {
    let mut aliases = Vec::new();
    for line in content.lines().skip(1).take_while(|l| !l.trim().is_empty()) {
        let trimmed = line.trim_start();
        if let Some(alias) = trimmed.strip_prefix("Alias:") {
            aliases.push(alias.trim().to_string());
        } else if let Some(alias_list) = trimmed.strip_prefix("Aliases:") {
            aliases.extend(
                alias_list
                    .split(',')
                    .map(str::trim)
                    .filter(|alias| !alias.is_empty())
                    .map(str::to_string),
            );
        }
    }
    dedup_aliases(aliases)
}

fn dedup_aliases(aliases: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut deduped = Vec::new();
    for alias in aliases {
        let alias = alias.trim();
        if alias.is_empty() {
            continue;
        }
        if seen.insert(alias.to_lowercase()) {
            deduped.push(alias.to_string());
        }
    }
    deduped
}

fn normalize_note_aliases(note: &mut Note) {
    let mut aliases = Vec::new();
    if let Some(alias) = &note.alias {
        aliases.push(alias.clone());
    }
    aliases.extend(note.aliases.clone());
    if !note.content.is_empty() {
        aliases.extend(extract_aliases(&note.content));
    }
    note.aliases = dedup_aliases(aliases);
    note.alias = note.aliases.first().cloned();
}

pub fn template_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("ML_NOTE_TEMPLATES_DIR") {
        return PathBuf::from(dir);
    }
    dirs_next::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".multi_launcher")
        .join("templates")
}

pub fn validate_template_name(name: &str) -> anyhow::Result<&str> {
    let name = name.trim();
    if name.is_empty()
        || name.contains('/')
        || name.contains('\\')
        || name.contains("..")
        || std::path::Path::new(name).is_absolute()
    {
        anyhow::bail!("invalid note template name: {name}");
    }
    Ok(name)
}

pub fn template_path(name: &str) -> anyhow::Result<PathBuf> {
    let name = validate_template_name(name)?;
    Ok(template_dir().join(format!("{name}.md")))
}

fn load_templates() -> anyhow::Result<HashMap<String, String>> {
    let dir = template_dir();
    let mut map = HashMap::new();
    if !dir.exists() {
        std::fs::create_dir_all(&dir)?;
        return Ok(map);
    }
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
            if validate_template_name(name).is_err() {
                continue;
            }
            if let Ok(content) = std::fs::read_to_string(&path) {
                map.insert(name.to_string(), content);
            }
        }
    }
    Ok(map)
}

pub fn list_templates() -> anyhow::Result<Vec<String>> {
    let mut names: Vec<String> = load_templates()?.into_keys().collect();
    names.sort();
    Ok(names)
}

pub fn reload_templates() -> anyhow::Result<()> {
    let templates = load_templates()?;
    if let Ok(mut guard) = TEMPLATE_CACHE.lock() {
        *guard = templates;
    }
    Ok(())
}

fn refresh_template_cache() -> anyhow::Result<()> {
    reload_templates()
}

pub fn get_template(name: &str) -> Option<String> {
    let name = validate_template_name(name).ok()?;
    TEMPLATE_CACHE
        .lock()
        .ok()
        .and_then(|m| m.get(name).cloned())
}

pub fn expand_template_variables<Tz: chrono::TimeZone>(
    template: &str,
    title: &str,
    slug: &str,
    now: chrono::DateTime<Tz>,
) -> String
where
    Tz::Offset: std::fmt::Display,
{
    template
        .replace("{{title}}", title)
        .replace("{{slug}}", slug)
        .replace("{{date}}", slug)
        .replace("{{datetime}}", &now.format("%Y-%m-%d %H:%M:%S").to_string())
        .replace("{{year}}", &now.format("%Y").to_string())
        .replace("{{month}}", &now.format("%m").to_string())
        .replace("{{day}}", &now.format("%d").to_string())
        .replace("{{time}}", &now.format("%H:%M:%S").to_string())
}

pub fn save_template(name: &str, content: &str) -> anyhow::Result<()> {
    let path = template_path(name)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)?;
    reload_templates()
}

pub fn delete_template(name: &str) -> anyhow::Result<()> {
    let path = template_path(name)?;
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    reload_templates()
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
        let aliases = extract_aliases(&content);
        let alias = aliases.first().cloned();
        let title = content
            .lines()
            .next()
            .and_then(|l| l.strip_prefix("# "))
            .map(|s| s.to_string())
            .unwrap_or_else(|| slug.replace('-', " "));
        let tags = extract_tags(&content);
        let links = extract_links(&content);
        let entity_refs = extract_entity_refs(&content);
        notes.push(Note {
            title,
            path,
            content,
            tags,
            links,
            slug,
            alias,
            aliases,
            entity_refs,
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

/// Return a snapshot of notes from the in-memory cache without hitting disk.
pub fn note_cache_snapshot() -> Vec<Note> {
    CACHE.lock().map(|c| c.notes.clone()).unwrap_or_default()
}

/// Return the cached lowercased alias -> note slug map without hitting disk.
pub fn note_alias_map_snapshot() -> HashMap<String, Vec<String>> {
    CACHE.lock().map(|c| c.aliases.clone()).unwrap_or_default()
}

/// Return lightweight note link menu targets from the in-memory cache without hitting disk.
pub fn note_link_menu_targets_snapshot() -> Vec<NoteLinkMenuTarget> {
    CACHE
        .lock()
        .map(|c| {
            c.notes
                .iter()
                .map(|note| NoteLinkMenuTarget {
                    slug: note.slug.clone(),
                    title: note.title.clone(),
                    alias: note.alias.clone(),
                    aliases: note.aliases.clone(),
                })
                .collect()
        })
        .unwrap_or_default()
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
    let existing_slugs: HashSet<String> = CACHE
        .lock()
        .map(|cache| {
            cache
                .notes
                .iter()
                .filter(|cached| {
                    (note.path.as_os_str().is_empty() || cached.path != note.path)
                        && (note.slug.is_empty() || cached.slug != note.slug)
                })
                .map(|cached| cached.slug.clone())
                .collect()
        })
        .unwrap_or_default();

    let slug = if note.slug.is_empty() {
        reset_slug_lookup();
        for existing in &existing_slugs {
            register_slug(existing);
        }
        unique_slug(&note.title)
    } else {
        note.slug.clone()
    };
    let path = dir.join(format!("{slug}.md"));
    if existing_slugs.contains(&slug) && note.path != path && !overwrite {
        return Ok(false);
    }
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
    note.aliases = extract_aliases(&content);
    if let Some(alias) = &note.alias {
        if !note.aliases.iter().any(|a| a.eq_ignore_ascii_case(alias)) {
            note.aliases.insert(0, alias.clone());
        }
    }
    note.aliases = dedup_aliases(note.aliases.clone());
    note.alias = note.aliases.first().cloned();
    note.tags = extract_tags(&content);
    note.entity_refs = extract_entity_refs(&content);
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
        aliases: Vec::new(),
        entity_refs: extract_entity_refs(content),
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
    backlinks_enabled: bool,
    aliases_enabled: bool,
    templates_enabled: bool,
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
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_millis() as u64)
                            .unwrap_or(0);
                        let last = LAST_NOTE_REINDEX_MS.load(Ordering::SeqCst);
                        if now.saturating_sub(last) >= NOTE_REINDEX_DEBOUNCE_MS {
                            LAST_NOTE_REINDEX_MS.store(now, Ordering::SeqCst);
                            let _ = refresh_cache();
                        }
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
            backlinks_enabled: true,
            aliases_enabled: true,
            templates_enabled: true,
            watcher,
        }
    }
}

impl Default for NotePlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy)]
enum NoteRelationshipCommand {
    Backlinks,
    Links,
    Mentions,
}

impl NoteRelationshipCommand {
    fn query_name(self) -> &'static str {
        match self {
            Self::Backlinks => "backlinks",
            Self::Links => "links",
            Self::Mentions => "mentions",
        }
    }

    fn label_badge(self) -> &'static str {
        match self {
            Self::Backlinks => "[backlink]",
            Self::Links => "[outgoing link]",
            Self::Mentions => "[mention]",
        }
    }

    fn empty_label(self, note_title: &str) -> String {
        match self {
            Self::Backlinks => format!("No backlinks for {note_title}"),
            Self::Links => format!("No outgoing links for {note_title}"),
            Self::Mentions => format!("No mentions for {note_title}"),
        }
    }
}

fn note_fuzzy_match_ci(matcher: &SkimMatcherV2, haystack: &str, needle: &str) -> bool {
    matcher.fuzzy_match(haystack, needle).is_some()
        || matcher
            .fuzzy_match(&haystack.to_lowercase(), &needle.to_lowercase())
            .is_some()
}

fn note_matches_title_or_alias(matcher: &SkimMatcherV2, note: &Note, filter: &str) -> bool {
    note_fuzzy_match_ci(matcher, &note.title, filter)
        || note_fuzzy_match_ci(matcher, &note.slug, filter)
        || note
            .alias
            .as_ref()
            .is_some_and(|a| note_fuzzy_match_ci(matcher, a, filter))
        || note
            .aliases
            .iter()
            .any(|a| note_fuzzy_match_ci(matcher, a, filter))
}

fn note_query_action(command: NoteRelationshipCommand, query: &str) -> Action {
    Action {
        label: format!("{} for {}", command.query_name(), query),
        desc: "Note".into(),
        action: format!("query:note {}", command.query_name()),
        args: Some(serde_json::json!({ "query": query }).to_string()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteNewPayload {
    pub slug: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
}

pub const NOTE_NEW_JSON_PREFIX: &str = "note:new-json:";

pub fn encode_note_new_payload(payload: &NoteNewPayload) -> anyhow::Result<String> {
    let json = serde_json::to_vec(payload)?;
    Ok(format!(
        "{NOTE_NEW_JSON_PREFIX}{}",
        URL_SAFE_NO_PAD.encode(json)
    ))
}

pub fn decode_note_new_payload(encoded: &str) -> anyhow::Result<NoteNewPayload> {
    let bytes = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|err| anyhow::anyhow!("invalid note new payload base64: {err}"))?;
    serde_json::from_slice(&bytes)
        .map_err(|err| anyhow::anyhow!("invalid note new payload json: {err}"))
}

fn encoded_note_new_action(slug: &str, template: Option<&str>) -> Action {
    Action {
        label: String::new(),
        desc: "Note".into(),
        action: format!("note:new:{}", urlencoding::encode(slug)),
        args: template.map(|name| serde_json::json!({ "template": name }).to_string()),
    }
}

fn templates_disabled_action() -> Action {
    Action {
        label: "Note templates are disabled in settings".into(),
        desc: "Note".into(),
        action: "note:templates_disabled".into(),
        args: None,
    }
}

fn relationship_entity_action(
    command: NoteRelationshipCommand,
    notes: &[Note],
    todos: &[crate::plugins::todo::TodoEntry],
    entity: EntityKey,
) -> Action {
    let link = LinkRef {
        target_type: entity.entity_type,
        target_id: entity.entity_id,
        anchor: None,
        display_text: None,
    };
    let mut action = format_link_row(notes, todos, &link, command.query_name());
    action.label = format!("{} {}", command.label_badge(), action.label);
    action
}

fn relationship_actions(
    command: NoteRelationshipCommand,
    guard: &NoteCache,
    args: &str,
    backlinks_enabled: bool,
) -> Vec<Action> {
    if matches!(
        command,
        NoteRelationshipCommand::Backlinks | NoteRelationshipCommand::Mentions
    ) && !backlinks_enabled
    {
        return vec![Action {
            label: "Note backlinks are disabled in settings".into(),
            desc: "Note".into(),
            action: "query:note".into(),
            args: None,
        }];
    }

    if args.is_empty() {
        let mut actions = vec![Action {
            label: format!("Usage: note {} <query>", command.query_name()),
            desc: "Usage".into(),
            action: format!("query:note {} ", command.query_name()),
            args: None,
        }];
        actions.extend(
            guard
                .notes
                .iter()
                .map(|n| note_query_action(command, &format!("slug:{}", n.slug))),
        );
        return actions;
    }

    let note = match resolve_target(guard, args) {
        NoteTarget::Resolved(slug) => guard.notes.iter().find(|n| n.slug == slug),
        NoteTarget::Ambiguous(slugs) => {
            let mut actions = vec![Action {
                label: format!("Ambiguous note query \"{args}\" ({} matches)", slugs.len()),
                desc: "Note".into(),
                action: format!("query:note {} ", command.query_name()),
                args: None,
            }];
            actions.extend(
                slugs
                    .into_iter()
                    .take(8)
                    .map(|slug| note_query_action(command, &format!("slug:{slug}"))),
            );
            return actions;
        }
        NoteTarget::Broken => None,
    };

    let Some(note) = note else {
        return vec![Action {
            label: format!("No note found for \"{args}\""),
            desc: "Note".into(),
            action: format!("query:note {} ", command.query_name()),
            args: None,
        }];
    };

    let todos = TODO_DATA.read().map(|g| g.clone()).unwrap_or_default();
    let index = build_index_from_notes_and_todos(&guard.notes, &todos);
    let source = EntityKey::new(LinkTarget::Note, note.slug.clone());
    let mut actions = Vec::new();

    if matches!(command, NoteRelationshipCommand::Links) {
        actions.extend(index.get_forward_links(&source).into_iter().map(|link| {
            relationship_entity_action(
                NoteRelationshipCommand::Links,
                &guard.notes,
                &todos,
                EntityKey::new(link.target_type, link.target_id),
            )
        }));
        if backlinks_enabled {
            actions.extend(
                index
                    .get_backlinks(
                        &source,
                        crate::linking::BacklinkFilters {
                            linked_todos: true,
                            related_notes: true,
                            mentions: true,
                        },
                    )
                    .into_iter()
                    .map(|entity| {
                        let row_command = if entity.entity_type == LinkTarget::Note {
                            NoteRelationshipCommand::Backlinks
                        } else {
                            NoteRelationshipCommand::Mentions
                        };
                        relationship_entity_action(row_command, &guard.notes, &todos, entity)
                    }),
            );
        }
    }

    if matches!(
        command,
        NoteRelationshipCommand::Backlinks | NoteRelationshipCommand::Mentions
    ) {
        let filters = match command {
            NoteRelationshipCommand::Backlinks => crate::linking::BacklinkFilters {
                linked_todos: false,
                related_notes: true,
                mentions: false,
            },
            NoteRelationshipCommand::Mentions => crate::linking::BacklinkFilters {
                linked_todos: true,
                related_notes: true,
                mentions: true,
            },
            NoteRelationshipCommand::Links => unreachable!(),
        };
        actions.extend(
            index
                .get_backlinks(&source, filters)
                .into_iter()
                .map(|entity| relationship_entity_action(command, &guard.notes, &todos, entity)),
        );
    }

    if actions.is_empty() {
        actions.push(Action {
            label: command.empty_label(&note.title),
            desc: "Note".into(),
            action: format!("note:open:{}", note.slug),
            args: None,
        });
    }

    actions
}

fn note_command_action(label: &str, action: &str) -> Action {
    Action {
        label: label.into(),
        desc: "Note".into(),
        action: action.into(),
        args: None,
    }
}

fn note_command_suggestions(
    backlinks_enabled: bool,
    aliases_enabled: bool,
    templates_enabled: bool,
) -> Vec<Action> {
    let mut commands = vec![
        note_command_action("note", "query:note"),
        note_command_action("note new", "query:note new "),
        note_command_action("note add", "query:note add "),
        note_command_action("note create", "query:note create "),
        note_command_action("note open", "query:note open "),
        note_command_action("note list", "query:note list"),
        note_command_action("note search", "query:note search "),
        note_command_action("note tag", "query:note tag"),
        note_command_action("note tags", "query:note tags"),
        note_command_action("note graph", "query:note graph"),
        note_command_action("note today", "query:note today"),
        note_command_action("note link", "query:note link "),
        note_command_action("note links", "query:note links "),
        note_command_action("note rm", "query:note rm "),
        note_command_action("note reload", "note:reload"),
        note_command_action("notes unused", "note:unused_assets"),
    ];
    if backlinks_enabled {
        commands.extend([
            note_command_action("note backlinks", "query:note backlinks "),
            note_command_action("note mentions", "query:note mentions "),
        ]);
    }
    if aliases_enabled {
        commands.extend([
            note_command_action("note alias", "query:note alias "),
            note_command_action("note aliases", "query:note aliases"),
        ]);
    }
    if templates_enabled {
        commands.extend([
            note_command_action("note templates", "query:note templates"),
            note_command_action("note template list", "query:note template list"),
            note_command_action("note template new", "query:note template new "),
            note_command_action("note template edit", "query:note template edit "),
            note_command_action("note template open", "query:note template open "),
            note_command_action("note template rm", "query:note template rm "),
        ]);
    }
    commands
}

fn note_root_suggestions(backlinks_enabled: bool, templates_enabled: bool) -> Vec<Action> {
    let mut commands = vec![
        note_command_action("note search", "query:note search "),
        note_command_action("note list", "query:note list"),
        note_command_action("note tag", "query:note tag"),
        note_command_action("note graph", "query:note graph"),
    ];
    if backlinks_enabled {
        commands.extend([
            note_command_action("note backlinks", "query:note backlinks "),
            note_command_action("note links", "query:note links "),
            note_command_action("note mentions", "query:note mentions "),
        ]);
    } else {
        commands.push(note_command_action("note links", "query:note links "));
    }
    if templates_enabled {
        commands.push(note_command_action(
            "note templates",
            "query:note templates",
        ));
    }
    commands.extend([
        note_command_action("note new", "query:note new "),
        note_command_action("note add", "query:note add "),
        note_command_action("note open", "query:note open "),
        note_command_action("note today", "query:note today"),
        note_command_action("note link", "query:note link "),
        note_command_action("note rm", "query:note rm "),
        note_command_action("note reload", "note:reload"),
        note_command_action("notes unused", "note:unused_assets"),
    ]);
    commands
}

fn note_suggestions_for_query_prefix(
    rest: &str,
    backlinks_enabled: bool,
    aliases_enabled: bool,
    templates_enabled: bool,
) -> Vec<Action> {
    let prefix = format!("note {}", rest.trim()).to_lowercase();
    note_command_suggestions(backlinks_enabled, aliases_enabled, templates_enabled)
        .into_iter()
        .filter(|a| a.label.to_lowercase().starts_with(&prefix))
        .collect()
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
                actions.extend(note_root_suggestions(
                    self.backlinks_enabled,
                    self.templates_enabled,
                ));
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
                            let name = rest["--template".len()..].trim();
                            if !name.is_empty() {
                                template = Some(name.to_string());
                            }
                        }
                        if !title.is_empty() {
                            let slug = slugify(title);
                            if template.is_some() && !self.templates_enabled {
                                return vec![templates_disabled_action()];
                            }
                            let mut action = encoded_note_new_action(&slug, template.as_deref());
                            action.label = format!("New note {title}");
                            return vec![Action {
                                label: format!("New note {title}"),
                                ..action
                            }];
                        }
                    }
                }
                "open" => {
                    let filter = args;
                    return guard
                        .notes
                        .iter()
                        .filter(|n| note_matches_title_or_alias(&self.matcher, n, filter))
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
                                    .all(|tag| n.tags.iter().any(|t| t.contains(tag)))
                            };
                            let exclude_ok = !filters
                                .exclude_tags
                                .iter()
                                .any(|tag| n.tags.iter().any(|t| t.contains(tag)));
                            let text_ok = if text_filter.is_empty() {
                                true
                            } else {
                                let matches =
                                    note_matches_title_or_alias(&self.matcher, n, &text_filter);
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
                    return guard
                        .index
                        .iter()
                        .zip(guard.notes.iter())
                        .filter(|(text, _)| filter.is_empty() || text.contains(&filter))
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
                    if let Some(stripped) = filter
                        .strip_prefix('#')
                        .or_else(|| filter.strip_prefix('@'))
                    {
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
                "graph" => {
                    let mut filters = parse_query_filters(args, &["@", "#", "tag:"]);
                    filters.include_tags = filters
                        .include_tags
                        .into_iter()
                        .map(|tag| tag.to_lowercase())
                        .collect();
                    let root = filters.remaining_tokens.first().cloned();
                    let args = serde_json::json!({
                        "include_tags": filters.include_tags,
                        "root": root,
                    });
                    return vec![Action {
                        label: "Open note graph".into(),
                        desc: "Note".into(),
                        action: "note:graph_dialog".into(),
                        args: Some(args.to_string()),
                    }];
                }
                "today" => {
                    let slug = Local::now().format("%Y-%m-%d").to_string();
                    let tmpl = self
                        .templates_enabled
                        .then(|| ())
                        .and_then(|_| self.templates.lock().ok())
                        .and_then(|t| {
                            if t.contains_key("today") {
                                Some("today")
                            } else if t.contains_key("default") {
                                Some("default")
                            } else {
                                None
                            }
                        });
                    let action = encoded_note_new_action(&slug, tmpl);
                    let title = slug.replace('-', " ");
                    let mut actions = vec![Action {
                        label: format!("Create {title}"),
                        ..action
                    }];
                    if !args.is_empty() {
                        actions.push(Action {
                            label: format!(
                                "Ignored trailing arguments for note today: {}",
                                args.trim()
                            ),
                            desc: "Note".into(),
                            action: "query:note today".into(),
                            args: None,
                        });
                    }
                    return actions;
                }
                "backlinks" => {
                    return relationship_actions(
                        NoteRelationshipCommand::Backlinks,
                        &guard,
                        args,
                        self.backlinks_enabled,
                    );
                }
                "mentions" => {
                    return relationship_actions(
                        NoteRelationshipCommand::Mentions,
                        &guard,
                        args,
                        self.backlinks_enabled,
                    );
                }
                "links" | "link" => {
                    return relationship_actions(
                        NoteRelationshipCommand::Links,
                        &guard,
                        args,
                        self.backlinks_enabled,
                    );
                }
                "rm" => {
                    let filter = args;
                    return guard
                        .notes
                        .iter()
                        .filter(|n| note_matches_title_or_alias(&self.matcher, n, filter))
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
                    if !self.templates_enabled {
                        return Vec::new();
                    }
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
                            .map(|name| {
                                let mut action = encoded_note_new_action("", Some(name));
                                action.label = name.clone();
                                action.action = "query:note new ".into();
                                action
                            })
                            .collect();
                    }
                }
                "alias" | "aliases" => {
                    if !self.aliases_enabled {
                        return Vec::new();
                    }
                    if args.is_empty() {
                        return note_suggestions_for_query_prefix(
                            rest,
                            self.backlinks_enabled,
                            self.aliases_enabled,
                            self.templates_enabled,
                        );
                    }
                    let filter = args;
                    let mut actions = Vec::new();
                    for note in &guard.notes {
                        for alias in &note.aliases {
                            if filter.is_empty()
                                || note_fuzzy_match_ci(&self.matcher, alias, filter)
                            {
                                actions.push(Action {
                                    label: format!("{alias} → {}", note.title),
                                    desc: "Note".into(),
                                    action: format!("note:open:{}", note.slug),
                                    args: None,
                                });
                            }
                        }
                    }
                    return actions;
                }
                "template" => {
                    if !self.templates_enabled {
                        return Vec::new();
                    }
                    let mut template_parts = args.splitn(2, ' ');
                    let subcmd = template_parts.next().unwrap_or("");
                    let name_filter = template_parts.next().unwrap_or("").trim();
                    if !subcmd.is_empty()
                        && !["list", "new", "edit", "open", "rm"].contains(&subcmd)
                    {
                        let suggestions = note_suggestions_for_query_prefix(
                            rest,
                            self.backlinks_enabled,
                            self.aliases_enabled,
                            self.templates_enabled,
                        );
                        if !suggestions.is_empty() {
                            return suggestions;
                        }
                    }
                    if matches!(subcmd, "" | "list") {
                        return self.search(&format!("note templates {name_filter}"));
                    }
                    if matches!(subcmd, "new" | "edit" | "open" | "rm") {
                        let label = if name_filter.is_empty() {
                            format!("note template {subcmd} <name>")
                        } else {
                            format!("note template {subcmd} {name_filter}")
                        };
                        return vec![Action {
                            label,
                            desc: "Note".into(),
                            action: format!("note:template:{subcmd}"),
                            args: (!name_filter.is_empty())
                                .then(|| serde_json::json!({ "name": name_filter }).to_string()),
                        }];
                    }
                }
                _ => {}
            }

            let suggestions = note_suggestions_for_query_prefix(
                rest,
                self.backlinks_enabled,
                self.aliases_enabled,
                self.templates_enabled,
            );
            if !suggestions.is_empty() {
                return suggestions;
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
        note_command_suggestions(
            self.backlinks_enabled,
            self.aliases_enabled,
            self.templates_enabled,
        )
    }

    fn default_settings(&self) -> Option<serde_json::Value> {
        serde_json::to_value(NotePluginSettings::default()).ok()
    }

    fn apply_settings(&mut self, value: &serde_json::Value) {
        if let Ok(cfg) = serde_json::from_value::<NotePluginSettings>(value.clone()) {
            self.external_open = cfg.external_open;
            self.backlinks_enabled = cfg.backlinks_enabled;
            self.aliases_enabled = cfg.aliases_enabled;
            self.templates_enabled = cfg.templates_enabled;
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
        ui.checkbox(&mut cfg.backlinks_enabled, "Enable backlinks");
        ui.checkbox(&mut cfg.aliases_enabled, "Enable aliases");
        ui.checkbox(&mut cfg.templates_enabled, "Enable templates");
        match serde_json::to_value(&cfg) {
            Ok(v) => *value = v,
            Err(e) => tracing::error!("failed to serialize note settings: {e}"),
        }
        self.external_open = cfg.external_open;
        self.backlinks_enabled = cfg.backlinks_enabled;
        self.aliases_enabled = cfg.aliases_enabled;
        self.templates_enabled = cfg.templates_enabled;
    }

    fn query_prefixes(&self) -> &[&str] {
        &["note", "notes"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
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

    fn test_note(title: &str, slug: &str, content: &str) -> Note {
        let aliases = extract_aliases(content);
        Note {
            title: title.into(),
            path: PathBuf::new(),
            content: content.into(),
            tags: Vec::new(),
            links: Vec::new(),
            slug: slug.into(),
            alias: aliases.first().cloned(),
            aliases,
            entity_refs: Vec::new(),
        }
    }

    fn test_plugin_with_notes(notes: Vec<Note>) -> (NotePlugin, Arc<Mutex<NoteCache>>) {
        let data = Arc::new(Mutex::new(NoteCache::from_notes(notes)));
        (
            NotePlugin {
                matcher: SkimMatcherV2::default(),
                data: data.clone(),
                templates: TEMPLATE_CACHE.clone(),
                external_open: NoteExternalOpen::Wezterm,
                backlinks_enabled: true,
                aliases_enabled: true,
                templates_enabled: true,
                watcher: None,
            },
            data,
        )
    }

    static TEMPLATE_ENV_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    fn with_template_dir<T>(test: impl FnOnce(&std::path::Path) -> T) -> T {
        let _lock = TEMPLATE_ENV_LOCK
            .lock()
            .expect("template env lock poisoned");
        let dir = tempfile::tempdir().expect("tempdir");
        let template_dir = dir.path().join("templates");
        let prev = std::env::var("ML_NOTE_TEMPLATES_DIR").ok();
        unsafe { std::env::set_var("ML_NOTE_TEMPLATES_DIR", &template_dir) };
        let original_templates = TEMPLATE_CACHE
            .lock()
            .expect("template cache lock poisoned")
            .clone();

        let result = test(&template_dir);

        if let Some(prev) = prev {
            unsafe { std::env::set_var("ML_NOTE_TEMPLATES_DIR", prev) };
        } else {
            unsafe { std::env::remove_var("ML_NOTE_TEMPLATES_DIR") };
        }
        *TEMPLATE_CACHE.lock().expect("template cache lock poisoned") = original_templates;
        result
    }

    #[test]
    fn note_templates_ignore_non_markdown_files() {
        with_template_dir(|dir| {
            std::fs::create_dir_all(dir).unwrap();
            std::fs::write(dir.join("daily.md"), "# Daily").unwrap();
            std::fs::write(dir.join("scratch.txt"), "ignore me").unwrap();

            reload_templates().unwrap();

            assert_eq!(list_templates().unwrap(), vec!["daily"]);
            assert_eq!(get_template("daily"), Some("# Daily".into()));
            assert_eq!(get_template("scratch"), None);
        });
    }

    #[test]
    fn note_templates_missing_directory_returns_empty_and_creates_directory() {
        with_template_dir(|dir| {
            assert!(!dir.exists());

            assert_eq!(list_templates().unwrap(), Vec::<String>::new());

            assert!(dir.exists());
            assert!(dir.is_dir());
        });
    }

    #[test]
    fn note_template_helpers_reject_path_traversal_names() {
        with_template_dir(|_| {
            for name in [
                "../secret",
                "folder/name",
                r"folder\name",
                "..",
                "/tmp/secret",
            ] {
                assert!(template_path(name).is_err(), "{name} should be rejected");
                assert!(
                    save_template(name, "content").is_err(),
                    "{name} should be rejected"
                );
                assert!(delete_template(name).is_err(), "{name} should be rejected");
                assert_eq!(get_template(name), None, "{name} should not resolve");
            }
        });
    }

    #[test]
    fn note_template_helpers_trim_names_for_crud_operations() {
        with_template_dir(|dir| {
            save_template("  meeting  ", "# Meeting").unwrap();

            assert_eq!(list_templates().unwrap(), vec!["meeting"]);
            assert_eq!(get_template(" meeting "), Some("# Meeting".into()));
            assert!(dir.join("meeting.md").exists());

            delete_template(" meeting ").unwrap();
            assert_eq!(list_templates().unwrap(), Vec::<String>::new());
            assert!(!dir.join("meeting.md").exists());
        });
    }

    #[test]
    fn note_template_name_validation_matches_command_helper() {
        assert_eq!(validate_template_name(" daily ").unwrap(), "daily");
        assert!(validate_template_name("").is_err());
        assert!(validate_template_name("nested/template").is_err());
        assert!(validate_template_name(r"nested\template").is_err());
        assert!(validate_template_name("../template").is_err());
    }

    #[test]
    fn note_templates_command_still_lists_cached_templates() {
        let templates = Arc::new(Mutex::new(HashMap::from([
            ("daily".to_string(), "# Daily".to_string()),
            ("meeting".to_string(), "# Meeting".to_string()),
        ])));
        let plugin = NotePlugin {
            matcher: SkimMatcherV2::default(),
            data: Arc::new(Mutex::new(NoteCache::default())),
            templates,
            external_open: NoteExternalOpen::Wezterm,
            backlinks_enabled: true,
            aliases_enabled: true,
            templates_enabled: true,
            watcher: None,
        };

        let mut actions = plugin.search("note templates");
        actions.sort_by(|a, b| a.label.cmp(&b.label));

        let labels: Vec<&str> = actions.iter().map(|a| a.label.as_str()).collect();
        let action_values: Vec<&str> = actions.iter().map(|a| a.action.as_str()).collect();
        assert_eq!(labels, vec!["daily", "meeting"]);
        assert_eq!(action_values, vec!["query:note new ", "query:note new "]);
        let args: Vec<String> = actions.iter().filter_map(|a| a.args.clone()).collect();
        assert_eq!(
            args,
            vec![
                serde_json::json!({ "template": "daily" }).to_string(),
                serde_json::json!({ "template": "meeting" }).to_string(),
            ]
        );
    }

    #[test]
    fn note_common_query_prefixes_return_command_suggestions() {
        let plugin = NotePlugin {
            matcher: SkimMatcherV2::default(),
            data: Arc::new(Mutex::new(NoteCache::default())),
            templates: Arc::new(Mutex::new(HashMap::new())),
            external_open: NoteExternalOpen::Wezterm,
            backlinks_enabled: true,
            aliases_enabled: true,
            templates_enabled: true,
            watcher: None,
        };

        for (query, expected_label) in [
            ("note op", "note open"),
            ("note li", "note list"),
            ("note se", "note search"),
            ("note ta", "note tags"),
            ("note gr", "note graph"),
            ("note lin", "note links"),
            ("note back", "note backlinks"),
            ("note templ", "note templates"),
            ("note template l", "note template list"),
            ("note template n", "note template new"),
            ("note tod", "note today"),
            ("note alia", "note alias"),
            ("note aliases", "note aliases"),
        ] {
            let labels: Vec<String> = plugin.search(query).into_iter().map(|a| a.label).collect();
            assert!(
                labels.iter().any(|label| label == expected_label),
                "{query:?} should suggest {expected_label:?}; got {labels:?}"
            );
        }
    }

    #[test]
    fn note_command_suggestions_respect_feature_gates() {
        let plugin = NotePlugin {
            matcher: SkimMatcherV2::default(),
            data: Arc::new(Mutex::new(NoteCache::default())),
            templates: Arc::new(Mutex::new(HashMap::from([(
                "daily".to_string(),
                "# Daily".to_string(),
            )]))),
            external_open: NoteExternalOpen::Wezterm,
            backlinks_enabled: false,
            aliases_enabled: false,
            templates_enabled: false,
            watcher: None,
        };

        let command_labels: Vec<String> = plugin.commands().into_iter().map(|a| a.label).collect();
        assert!(
            !command_labels
                .iter()
                .any(|label| label.contains("backlink"))
        );
        assert!(!command_labels.iter().any(|label| label.contains("alias")));
        assert!(
            !command_labels
                .iter()
                .any(|label| label.contains("template"))
        );

        assert!(plugin.search("note back").is_empty());
        assert!(plugin.search("note alia").is_empty());
        assert!(plugin.search("note templ").is_empty());
        assert!(
            plugin.search("note backlinks target")[0]
                .label
                .contains("backlinks are disabled")
        );
        assert!(plugin.search("note aliases").is_empty());
        assert!(plugin.search("note templates").is_empty());
    }

    #[test]
    fn note_template_suggestions_hide_when_templates_disabled() {
        let plugin = NotePlugin {
            matcher: SkimMatcherV2::default(),
            data: Arc::new(Mutex::new(NoteCache::default())),
            templates: Arc::new(Mutex::new(HashMap::from([(
                "daily".to_string(),
                "# Daily".to_string(),
            )]))),
            external_open: NoteExternalOpen::Wezterm,
            backlinks_enabled: true,
            aliases_enabled: true,
            templates_enabled: false,
            watcher: None,
        };

        assert!(
            !plugin
                .search("note")
                .iter()
                .any(|a| a.label.contains("template"))
        );
        assert!(
            !plugin
                .commands()
                .iter()
                .any(|a| a.label.contains("template"))
        );
        assert!(plugin.search("note templates").is_empty());
    }

    #[test]
    fn note_new_template_returns_noop_when_templates_disabled() {
        let plugin = NotePlugin {
            matcher: SkimMatcherV2::default(),
            data: Arc::new(Mutex::new(NoteCache::default())),
            templates: Arc::new(Mutex::new(HashMap::new())),
            external_open: NoteExternalOpen::Wezterm,
            backlinks_enabled: true,
            aliases_enabled: true,
            templates_enabled: false,
            watcher: None,
        };

        let actions = plugin.search("note new Quarterly Plan --template fancy:name with spaces");

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].label, "Note templates are disabled in settings");
        assert_eq!(actions[0].action, "note:templates_disabled");
    }

    #[test]
    fn note_new_template_uses_json_payload_for_arbitrary_template_names() {
        let plugin = NotePlugin {
            matcher: SkimMatcherV2::default(),
            data: Arc::new(Mutex::new(NoteCache::default())),
            templates: Arc::new(Mutex::new(HashMap::new())),
            external_open: NoteExternalOpen::Wezterm,
            backlinks_enabled: true,
            aliases_enabled: true,
            templates_enabled: true,
            watcher: None,
        };

        let actions = plugin.search("note new Quarterly Plan --template fancy:name with spaces");

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].action, "note:new:quarterly-plan");
        let expected_args = serde_json::json!({ "template": "fancy:name with spaces" }).to_string();
        assert_eq!(actions[0].args.as_deref(), Some(expected_args.as_str()));
    }

    #[test]
    fn note_new_payload_round_trips_arbitrary_template_text() {
        let template = "colon: slash/ backslash\\ unicode☃\nnewline \"quotes\"";
        let payload = NoteNewPayload {
            slug: "unicode-note".into(),
            template: Some(template.into()),
        };

        let action = encode_note_new_payload(&payload).expect("encode note payload");

        assert!(action.starts_with(NOTE_NEW_JSON_PREFIX));
        let encoded = action.strip_prefix(NOTE_NEW_JSON_PREFIX).unwrap();
        let decoded = decode_note_new_payload(encoded).expect("decode note payload");
        assert_eq!(decoded, payload);
        assert!(!encoded.contains(':'));
        assert!(!encoded.contains('/'));
        assert!(!encoded.contains('\\'));
        assert!(!encoded.contains(char::is_whitespace));
        assert!(!encoded.contains('\n'));
        assert!(!encoded.contains('"'));
    }

    #[test]
    fn expand_template_variables_replaces_all_supported_variables() {
        let now = chrono::FixedOffset::east_opt(0)
            .unwrap()
            .with_ymd_and_hms(2026, 6, 24, 7, 8, 9)
            .unwrap();
        let expanded = expand_template_variables(
            "{{title}}|{{slug}}|{{date}}|{{datetime}}|{{year}}|{{month}}|{{day}}|{{time}}",
            "Daily Notes",
            "daily-notes",
            now,
        );

        assert_eq!(
            expanded,
            "Daily Notes|daily-notes|daily-notes|2026-06-24 07:08:09|2026|06|24|07:08:09"
        );
    }

    #[test]
    fn expand_template_variables_preserves_title_and_date_compatibility() {
        let now = chrono::FixedOffset::east_opt(0)
            .unwrap()
            .with_ymd_and_hms(1999, 1, 2, 3, 4, 5)
            .unwrap();

        assert_eq!(
            expand_template_variables("# {{title}}\nDate: {{date}}", "My Note", "my-note", now),
            "# My Note\nDate: my-note"
        );
    }

    #[test]
    fn extract_alias_parses_single_alias() {
        let content = "# Alpha\nAlias: Display Name\n\nBody";
        assert_eq!(extract_alias(content), Some("Display Name".into()));
        assert_eq!(extract_aliases(content), vec!["Display Name"]);
    }

    #[test]
    fn extract_aliases_parses_comma_separated_aliases() {
        let content = "# Alpha\nAliases: foo, bar, baz\n\nBody";
        assert_eq!(extract_aliases(content), vec!["foo", "bar", "baz"]);
    }

    #[test]
    fn extract_aliases_includes_old_single_alias_and_dedups_case_insensitively() {
        let content = "# Alpha\nAlias: Foo\nAliases: foo, Bar, FOO\n\nBody";
        assert_eq!(extract_aliases(content), vec!["Foo", "Bar"]);
    }

    #[test]
    fn note_alias_resolution_is_case_insensitive_and_supports_all_aliases() {
        let cache = NoteCache::from_notes(vec![test_note(
            "Alpha",
            "alpha",
            "# Alpha\nAlias: Primary\nAliases: Second, Third\n\nBody",
        )]);

        assert_eq!(
            resolve_target(&cache, "primary"),
            NoteTarget::Resolved("alpha".into())
        );
        assert_eq!(
            resolve_target(&cache, "SECOND"),
            NoteTarget::Resolved("alpha".into())
        );
        assert_eq!(
            resolve_target(&cache, "third"),
            NoteTarget::Resolved("alpha".into())
        );
    }

    #[test]
    fn duplicate_alias_resolution_is_ambiguous() {
        let (plugin, data) = test_plugin_with_notes(vec![
            test_note("Alpha", "alpha", "# Alpha\nAlias: Shared\n\nBody"),
            test_note("Beta", "beta", "# Beta\nAliases: shared, Other\n\nBody"),
        ]);

        let cache = data.lock().expect("note cache lock poisoned");
        let mut resolved_slugs = match resolve_target(&cache, "SHARED") {
            NoteTarget::Ambiguous(slugs) => slugs,
            other => panic!("expected ambiguous alias resolution, got {other:?}"),
        };
        resolved_slugs.sort();
        assert_eq!(
            resolved_slugs,
            vec!["alpha".to_string(), "beta".to_string()]
        );
        drop(cache);

        let open_actions = plugin.search("note open Shared");
        let mut open_slugs: Vec<&str> = open_actions
            .iter()
            .map(|action| action.action.as_str())
            .collect();
        open_slugs.sort_unstable();
        assert_eq!(open_slugs, vec!["note:open:alpha", "note:open:beta"]);
    }

    #[test]
    fn note_search_and_open_include_all_aliases() {
        let (plugin, _) = test_plugin_with_notes(vec![
            test_note(
                "Alpha",
                "alpha",
                "# Alpha\nAlias: Primary\nAliases: Second, Third\n\nBody",
            ),
            test_note("Beta", "beta", "# Beta\n\nBody"),
        ]);

        let search_labels: Vec<String> = plugin
            .search("note search third")
            .into_iter()
            .map(|a| a.label)
            .collect();
        assert_eq!(search_labels, vec!["Primary"]);

        let open_labels: Vec<String> = plugin
            .search("note open Second")
            .into_iter()
            .map(|a| a.label)
            .collect();
        assert_eq!(open_labels, vec!["Primary"]);
    }

    #[test]
    fn note_cache_snapshot_is_read_only_copy() {
        let original = set_notes(vec![Note {
            title: "Alpha".into(),
            path: PathBuf::new(),
            content: "# Alpha".into(),
            tags: Vec::new(),
            links: Vec::new(),
            slug: "alpha".into(),
            alias: None,
            aliases: Vec::new(),
            entity_refs: Vec::new(),
        }]);

        let mut snapshot = note_cache_snapshot();
        snapshot[0].title = "Mutated".into();
        let fresh = note_cache_snapshot();
        assert_eq!(fresh[0].title, "Alpha");

        restore_cache(original);
    }

    #[test]
    fn note_cache_snapshot_reflects_refresh_cache() {
        use std::fs;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let prev = std::env::var("ML_NOTES_DIR").ok();
        unsafe { std::env::set_var("ML_NOTES_DIR", dir.path()) };

        fs::write(
            dir.path().join("one.md"),
            "# One

Body",
        )
        .unwrap();
        refresh_cache().unwrap();
        let first = note_cache_snapshot();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].slug, "one");

        fs::write(
            dir.path().join("two.md"),
            "# Two

Body",
        )
        .unwrap();
        refresh_cache().unwrap();
        let second = note_cache_snapshot();
        assert_eq!(second.len(), 2);
        assert!(second.iter().any(|n| n.slug == "two"));

        if let Some(p) = prev {
            unsafe { std::env::set_var("ML_NOTES_DIR", p) };
        } else {
            unsafe { std::env::remove_var("ML_NOTES_DIR") };
        }
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
                aliases: Vec::new(),
                entity_refs: Vec::new(),
            },
            Note {
                title: "Beta".into(),
                path: PathBuf::new(),
                content: "Planning @testing coverage.".into(),
                tags: Vec::new(),
                links: Vec::new(),
                slug: "beta".into(),
                alias: None,
                aliases: Vec::new(),
                entity_refs: Vec::new(),
            },
            Note {
                title: "Gamma".into(),
                path: PathBuf::new(),
                content: "Wrap up #ui and #chore items.".into(),
                tags: Vec::new(),
                links: Vec::new(),
                slug: "gamma".into(),
                alias: None,
                aliases: Vec::new(),
                entity_refs: Vec::new(),
            },
        ]);

        let plugin = NotePlugin {
            matcher: SkimMatcherV2::default(),
            data: CACHE.clone(),
            templates: TEMPLATE_CACHE.clone(),
            external_open: NoteExternalOpen::Wezterm,
            backlinks_enabled: true,
            aliases_enabled: true,
            templates_enabled: true,
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
        let labels_both_hash: Vec<&str> = list_both_hash.iter().map(|a| a.label.as_str()).collect();
        assert_eq!(labels_both_hash, vec!["Alpha"]);

        restore_cache(original);
    }

    #[test]
    fn note_list_supports_partial_tag_filters() {
        let original = set_notes(vec![
            Note {
                title: "Alpha".into(),
                path: PathBuf::new(),
                content: "Review #testing and #ui-kit changes.".into(),
                tags: Vec::new(),
                links: Vec::new(),
                slug: "alpha".into(),
                alias: None,
                aliases: Vec::new(),
                entity_refs: Vec::new(),
            },
            Note {
                title: "Beta".into(),
                path: PathBuf::new(),
                content: "Follow up on #testing checklist.".into(),
                tags: Vec::new(),
                links: Vec::new(),
                slug: "beta".into(),
                alias: None,
                aliases: Vec::new(),
                entity_refs: Vec::new(),
            },
            Note {
                title: "Gamma".into(),
                path: PathBuf::new(),
                content: "Finalize #ui rollout.".into(),
                tags: Vec::new(),
                links: Vec::new(),
                slug: "gamma".into(),
                alias: None,
                aliases: Vec::new(),
                entity_refs: Vec::new(),
            },
        ]);

        let plugin = NotePlugin {
            matcher: SkimMatcherV2::default(),
            data: CACHE.clone(),
            templates: TEMPLATE_CACHE.clone(),
            external_open: NoteExternalOpen::Wezterm,
            backlinks_enabled: true,
            aliases_enabled: true,
            templates_enabled: true,
            watcher: None,
        };

        let list_test = plugin.search("note list #test");
        let labels_test: Vec<&str> = list_test.iter().map(|a| a.label.as_str()).collect();
        assert_eq!(labels_test, vec!["Alpha", "Beta"]);

        let list_ui = plugin.search("note list @ui");
        let labels_ui: Vec<&str> = list_ui.iter().map(|a| a.label.as_str()).collect();
        assert_eq!(labels_ui, vec!["Alpha", "Gamma"]);

        let list_not_ui = plugin.search("note list !#ui");
        let labels_not_ui: Vec<&str> = list_not_ui.iter().map(|a| a.label.as_str()).collect();
        assert_eq!(labels_not_ui, vec!["Beta"]);

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
                aliases: Vec::new(),
                entity_refs: Vec::new(),
            },
            Note {
                title: "Beta".into(),
                path: PathBuf::new(),
                content: "Planning @testing coverage.".into(),
                tags: Vec::new(),
                links: Vec::new(),
                slug: "beta".into(),
                alias: None,
                aliases: Vec::new(),
                entity_refs: Vec::new(),
            },
            Note {
                title: "Gamma".into(),
                path: PathBuf::new(),
                content: "Wrap up #ui and #chore items.".into(),
                tags: Vec::new(),
                links: Vec::new(),
                slug: "gamma".into(),
                alias: None,
                aliases: Vec::new(),
                entity_refs: Vec::new(),
            },
        ]);

        let plugin = NotePlugin {
            matcher: SkimMatcherV2::default(),
            data: CACHE.clone(),
            templates: TEMPLATE_CACHE.clone(),
            external_open: NoteExternalOpen::Wezterm,
            backlinks_enabled: true,
            aliases_enabled: true,
            templates_enabled: true,
            watcher: None,
        };

        let tags = plugin.search("note tag");
        let labels: Vec<&str> = tags.iter().map(|a| a.label.as_str()).collect();
        assert_eq!(labels, vec!["#testing (2)", "#ui (2)", "#chore (1)"]);
        let actions: Vec<&str> = tags.iter().map(|a| a.action.as_str()).collect();
        assert_eq!(
            actions,
            vec![
                "query:note list #testing",
                "query:note list #ui",
                "query:note list #chore"
            ]
        );

        let tags_ui = plugin.search("note tag @ui");
        let labels_ui: Vec<&str> = tags_ui.iter().map(|a| a.label.as_str()).collect();
        assert_eq!(labels_ui, vec!["#ui (2)"]);

        let tags_ui_hash = plugin.search("note tag #ui");
        let labels_ui_hash: Vec<&str> = tags_ui_hash.iter().map(|a| a.label.as_str()).collect();
        assert_eq!(labels_ui_hash, vec!["#ui (2)"]);

        let tags_ui_tag = plugin.search("note tag tag:ui");
        let labels_ui_tag: Vec<&str> = tags_ui_tag.iter().map(|a| a.label.as_str()).collect();
        assert_eq!(labels_ui_tag, vec!["#ui (2)"]);

        // Verify that the drill action uses `note list`.
        assert_eq!(tags_ui[0].action, "query:note list #ui");

        restore_cache(original);
    }

    #[test]
    fn note_tags_alias_is_not_exposed_in_commands() {
        let original = set_notes(vec![
            Note {
                title: "Alpha".into(),
                path: PathBuf::new(),
                content: "Working on @testing and #ui updates.".into(),
                tags: Vec::new(),
                links: Vec::new(),
                slug: "alpha".into(),
                alias: None,
                aliases: Vec::new(),
                entity_refs: Vec::new(),
            },
            Note {
                title: "Beta".into(),
                path: PathBuf::new(),
                content: "Planning @testing coverage.".into(),
                tags: Vec::new(),
                links: Vec::new(),
                slug: "beta".into(),
                alias: None,
                aliases: Vec::new(),
                entity_refs: Vec::new(),
            },
        ]);

        let plugin = NotePlugin {
            matcher: SkimMatcherV2::default(),
            data: CACHE.clone(),
            templates: TEMPLATE_CACHE.clone(),
            external_open: NoteExternalOpen::Wezterm,
            backlinks_enabled: true,
            aliases_enabled: true,
            templates_enabled: true,
            watcher: None,
        };

        let tags = plugin.search("note tag");
        let tags_alias = plugin.search("note tags");
        let labels: Vec<&str> = tags.iter().map(|a| a.label.as_str()).collect();
        let labels_alias: Vec<&str> = tags_alias.iter().map(|a| a.label.as_str()).collect();
        assert_eq!(labels_alias, labels);

        let commands = plugin.commands();
        assert!(commands.iter().any(|a| a.label == "note tags"));

        restore_cache(original);
    }

    #[test]
    fn note_search_matches_content_substring() {
        let original = set_notes(vec![
            Note {
                title: "Alpha".into(),
                path: PathBuf::new(),
                content: "The cat naps on the keyboard.".into(),
                tags: Vec::new(),
                links: Vec::new(),
                slug: "alpha".into(),
                alias: None,
                aliases: Vec::new(),
                entity_refs: Vec::new(),
            },
            Note {
                title: "Beta".into(),
                path: PathBuf::new(),
                content: "No pets here.".into(),
                tags: Vec::new(),
                links: Vec::new(),
                slug: "beta".into(),
                alias: None,
                aliases: Vec::new(),
                entity_refs: Vec::new(),
            },
        ]);

        let plugin = NotePlugin {
            matcher: SkimMatcherV2::default(),
            data: CACHE.clone(),
            templates: TEMPLATE_CACHE.clone(),
            external_open: NoteExternalOpen::Wezterm,
            backlinks_enabled: true,
            aliases_enabled: true,
            templates_enabled: true,
            watcher: None,
        };

        let matches = plugin.search("note search cat");
        let labels: Vec<&str> = matches.iter().map(|a| a.label.as_str()).collect();
        assert_eq!(labels, vec!["Alpha"]);

        restore_cache(original);
    }

    #[test]
    fn note_links_lists_forward_and_back_links_for_note() {
        let alpha_content = "See [[Beta Note]] and [[Gamma Note]].";
        let delta_content = "Reference [[Beta Note]].";
        let original = set_notes(vec![
            Note {
                title: "Alpha".into(),
                path: PathBuf::new(),
                content: alpha_content.into(),
                tags: Vec::new(),
                links: extract_links(alpha_content),
                slug: "alpha".into(),
                alias: None,
                aliases: Vec::new(),
                entity_refs: Vec::new(),
            },
            Note {
                title: "Beta Note".into(),
                path: PathBuf::new(),
                content: "Backlink target.".into(),
                tags: Vec::new(),
                links: Vec::new(),
                slug: "beta-note".into(),
                alias: Some("Second".into()),
                aliases: vec!["Second".into()],
                entity_refs: Vec::new(),
            },
            Note {
                title: "Delta".into(),
                path: PathBuf::new(),
                content: delta_content.into(),
                tags: Vec::new(),
                links: extract_links(delta_content),
                slug: "delta".into(),
                alias: None,
                aliases: Vec::new(),
                entity_refs: Vec::new(),
            },
            Note {
                title: "Gamma Note".into(),
                path: PathBuf::new(),
                content: "Gamma note content.".into(),
                tags: Vec::new(),
                links: Vec::new(),
                slug: "gamma-note".into(),
                alias: None,
                aliases: Vec::new(),
                entity_refs: Vec::new(),
            },
        ]);

        let plugin = NotePlugin {
            matcher: SkimMatcherV2::default(),
            data: CACHE.clone(),
            templates: TEMPLATE_CACHE.clone(),
            external_open: NoteExternalOpen::Wezterm,
            backlinks_enabled: true,
            aliases_enabled: true,
            templates_enabled: true,
            watcher: None,
        };

        let links = plugin.search("note links Second");
        assert!(
            links
                .iter()
                .any(|a| a.label.contains("[backlink]") && a.label.contains("type=note"))
        );
        assert!(links.iter().any(|a| a.action.starts_with("note:open:")));

        restore_cache(original);
    }

    #[test]
    fn note_links_ambiguous_query_returns_candidates() {
        let original = set_notes(vec![
            Note {
                title: "Roadmap".into(),
                path: PathBuf::new(),
                content: String::new(),
                tags: Vec::new(),
                links: Vec::new(),
                slug: "roadmap-a".into(),
                alias: None,
                aliases: Vec::new(),
                entity_refs: Vec::new(),
            },
            Note {
                title: "Roadmap".into(),
                path: PathBuf::new(),
                content: String::new(),
                tags: Vec::new(),
                links: Vec::new(),
                slug: "roadmap-b".into(),
                alias: None,
                aliases: Vec::new(),
                entity_refs: Vec::new(),
            },
        ]);
        let plugin = NotePlugin {
            matcher: SkimMatcherV2::default(),
            data: CACHE.clone(),
            templates: TEMPLATE_CACHE.clone(),
            external_open: NoteExternalOpen::Wezterm,
            backlinks_enabled: true,
            aliases_enabled: true,
            templates_enabled: true,
            watcher: None,
        };
        let links = plugin.search("note links Roadmap");
        assert!(
            links
                .iter()
                .any(|a| a.label.starts_with("Ambiguous note query"))
        );
        assert!(links.iter().any(|a| {
            a.action == "query:note links"
                && a.args
                    .as_deref()
                    .is_some_and(|args| args.contains("slug:roadmap-a"))
        }));
        assert!(links.iter().any(|a| {
            a.action == "query:note links"
                && a.args
                    .as_deref()
                    .is_some_and(|args| args.contains("slug:roadmap-b"))
        }));
        restore_cache(original);
    }

    #[test]
    fn note_relationship_commands_resolve_alias_title_and_slug() {
        let alpha_content = "See [[Beta Note]].";
        let delta_content = "Reference [[Beta Note]].";
        let original = set_notes(vec![
            Note {
                title: "Alpha".into(),
                path: PathBuf::new(),
                content: alpha_content.into(),
                tags: Vec::new(),
                links: extract_links(alpha_content),
                slug: "alpha".into(),
                alias: None,
                aliases: Vec::new(),
                entity_refs: Vec::new(),
            },
            Note {
                title: "Beta Note".into(),
                path: PathBuf::new(),
                content: "Backlink target.".into(),
                tags: Vec::new(),
                links: Vec::new(),
                slug: "beta-note".into(),
                alias: Some("Second".into()),
                aliases: vec!["Second".into()],
                entity_refs: Vec::new(),
            },
            Note {
                title: "Delta".into(),
                path: PathBuf::new(),
                content: delta_content.into(),
                tags: Vec::new(),
                links: extract_links(delta_content),
                slug: "delta".into(),
                alias: None,
                aliases: Vec::new(),
                entity_refs: Vec::new(),
            },
        ]);

        let plugin = NotePlugin {
            matcher: SkimMatcherV2::default(),
            data: CACHE.clone(),
            templates: TEMPLATE_CACHE.clone(),
            external_open: NoteExternalOpen::Wezterm,
            backlinks_enabled: true,
            aliases_enabled: true,
            templates_enabled: true,
            watcher: None,
        };

        let links = plugin.search("note links alpha");
        assert_eq!(links.len(), 1);
        assert!(links[0].label.contains("[outgoing link]"));
        assert_eq!(links[0].action, "note:open:beta-note");

        let backlinks = plugin.search("note backlinks Second");
        let backlink_actions: Vec<&str> = backlinks.iter().map(|a| a.action.as_str()).collect();
        assert_eq!(backlink_actions, vec!["note:open:alpha", "note:open:delta"]);
        assert!(backlinks.iter().all(|a| a.label.contains("[backlink]")));

        let mentions = plugin.search("note mentions Beta Note");
        assert!(mentions.iter().all(|a| a.label.contains("[mention]")));
        assert!(mentions.iter().any(|a| a.action == "note:open:alpha"));

        restore_cache(original);
    }

    #[test]
    fn note_backlinks_command_respects_disabled_setting() {
        let original = set_notes(vec![Note {
            title: "Target".into(),
            path: PathBuf::new(),
            content: String::new(),
            tags: Vec::new(),
            links: Vec::new(),
            slug: "target".into(),
            alias: None,
            aliases: Vec::new(),
            entity_refs: Vec::new(),
        }]);
        let plugin = NotePlugin {
            matcher: SkimMatcherV2::default(),
            data: CACHE.clone(),
            templates: TEMPLATE_CACHE.clone(),
            external_open: NoteExternalOpen::Wezterm,
            backlinks_enabled: false,
            aliases_enabled: true,
            templates_enabled: true,
            watcher: None,
        };

        let backlinks = plugin.search("note backlinks target");
        assert_eq!(backlinks.len(), 1);
        assert_eq!(
            backlinks[0].label,
            "Note backlinks are disabled in settings"
        );

        restore_cache(original);
    }

    #[test]
    fn resolve_target_handles_duplicate_titles_with_slug_or_path() {
        let original = set_notes(vec![
            Note {
                title: "Roadmap".into(),
                path: PathBuf::from("/tmp/alpha-roadmap.md"),
                content: String::new(),
                tags: Vec::new(),
                links: Vec::new(),
                slug: "roadmap-alpha".into(),
                alias: None,
                aliases: Vec::new(),
                entity_refs: Vec::new(),
            },
            Note {
                title: "Roadmap".into(),
                path: PathBuf::from("/tmp/team/beta-roadmap.md"),
                content: String::new(),
                tags: Vec::new(),
                links: Vec::new(),
                slug: "roadmap-beta".into(),
                alias: None,
                aliases: Vec::new(),
                entity_refs: Vec::new(),
            },
            Note {
                title: "Beta Roadmap".into(),
                path: PathBuf::from("/tmp/alt/beta-roadmap.md"),
                content: String::new(),
                tags: Vec::new(),
                links: Vec::new(),
                slug: "beta-roadmap-copy".into(),
                alias: None,
                aliases: Vec::new(),
                entity_refs: Vec::new(),
            },
        ]);

        assert!(matches!(
            resolve_note_query("Roadmap"),
            NoteTarget::Ambiguous(_)
        ));
        assert_eq!(
            resolve_note_query("roadmap-beta"),
            NoteTarget::Resolved("roadmap-beta".into())
        );
        assert_eq!(
            resolve_note_query("slug:roadmap-beta"),
            NoteTarget::Resolved("roadmap-beta".into())
        );
        assert_eq!(
            resolve_note_query("path:team/beta-roadmap.md"),
            NoteTarget::Resolved("roadmap-beta".into())
        );
        assert!(matches!(
            resolve_note_query("path:beta-roadmap.md"),
            NoteTarget::Ambiguous(slugs)
                if slugs == vec!["beta-roadmap-copy".to_string(), "roadmap-beta".to_string()]
        ));

        restore_cache(original);
    }

    #[test]
    fn backlinks_index_uses_resolved_title_links() {
        let original = set_notes(vec![
            Note {
                title: "Main".into(),
                path: PathBuf::new(),
                content: "Link to [[Target]].".into(),
                tags: Vec::new(),
                links: Vec::new(),
                slug: "main".into(),
                alias: None,
                aliases: Vec::new(),
                entity_refs: Vec::new(),
            },
            Note {
                title: "Target".into(),
                path: PathBuf::new(),
                content: "target".into(),
                tags: Vec::new(),
                links: Vec::new(),
                slug: "target".into(),
                alias: None,
                aliases: Vec::new(),
                entity_refs: Vec::new(),
            },
        ]);

        let backlinks = note_backlinks("target");
        assert_eq!(backlinks.len(), 1);
        assert_eq!(backlinks[0].slug, "main");

        restore_cache(original);
    }

    #[test]
    fn save_existing_note_succeeds_without_overwrite() {
        use std::fs;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let prev = std::env::var("ML_NOTES_DIR").ok();
        unsafe { std::env::set_var("ML_NOTES_DIR", dir.path()) };

        let path = dir.path().join("alpha.md");
        fs::write(&path, "# Alpha\n\nold").unwrap();
        refresh_cache().unwrap();

        let mut note = Note {
            title: "Alpha".into(),
            path: path.clone(),
            content: "# Alpha\n\nupdated".into(),
            tags: Vec::new(),
            links: Vec::new(),
            slug: "alpha".into(),
            alias: None,
            aliases: Vec::new(),
            entity_refs: Vec::new(),
        };

        let saved = save_note(&mut note, false).unwrap();
        assert!(saved);
        assert_eq!(fs::read_to_string(path).unwrap(), "# Alpha\n\nupdated");

        if let Some(p) = prev {
            unsafe { std::env::set_var("ML_NOTES_DIR", p) };
        } else {
            unsafe { std::env::remove_var("ML_NOTES_DIR") };
        }
    }

    #[test]
    fn save_as_new_generates_unique_slug_from_cache() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let prev = std::env::var("ML_NOTES_DIR").ok();
        unsafe { std::env::set_var("ML_NOTES_DIR", dir.path()) };

        std::fs::write(dir.path().join("alpha.md"), "# Alpha\n\nbody").unwrap();
        refresh_cache().unwrap();

        let mut note = Note {
            title: "Alpha".into(),
            path: PathBuf::new(),
            content: "Body".into(),
            tags: Vec::new(),
            links: Vec::new(),
            slug: String::new(),
            alias: None,
            aliases: Vec::new(),
            entity_refs: Vec::new(),
        };

        let saved = save_note(&mut note, false).unwrap();
        assert!(saved);
        assert_eq!(note.slug, "alpha-1");
        assert!(dir.path().join("alpha-1.md").exists());

        if let Some(p) = prev {
            unsafe { std::env::set_var("ML_NOTES_DIR", p) };
        } else {
            unsafe { std::env::remove_var("ML_NOTES_DIR") };
        }
    }

    #[test]
    fn save_note_renames_slug_and_path() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let prev = std::env::var("ML_NOTES_DIR").ok();
        unsafe { std::env::set_var("ML_NOTES_DIR", dir.path()) };

        let old_path = dir.path().join("alpha.md");
        std::fs::write(&old_path, "# Alpha\n\nbody").unwrap();
        refresh_cache().unwrap();

        let mut note = Note {
            title: "Alpha renamed".into(),
            path: old_path.clone(),
            content: "Body".into(),
            tags: Vec::new(),
            links: Vec::new(),
            slug: "alpha-renamed".into(),
            alias: None,
            aliases: Vec::new(),
            entity_refs: Vec::new(),
        };

        let saved = save_note(&mut note, false).unwrap();
        assert!(saved);
        assert!(!old_path.exists());
        assert_eq!(note.path, dir.path().join("alpha-renamed.md"));
        assert!(note.path.exists());

        if let Some(p) = prev {
            unsafe { std::env::set_var("ML_NOTES_DIR", p) };
        } else {
            unsafe { std::env::remove_var("ML_NOTES_DIR") };
        }
    }
}
