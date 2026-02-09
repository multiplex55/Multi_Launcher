use crate::common::entity_ref::EntityKind;
use crate::plugins::note::Note;
use crate::plugins::todo::TodoEntry;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};
use tracing::warn;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkTarget {
    Note,
    Todo,
    Bookmark,
    Layout,
    File,
}

impl LinkTarget {
    fn as_str(self) -> &'static str {
        match self {
            LinkTarget::Note => "note",
            LinkTarget::Todo => "todo",
            LinkTarget::Bookmark => "bookmark",
            LinkTarget::Layout => "layout",
            LinkTarget::File => "file",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw.to_ascii_lowercase().as_str() {
            "note" => Some(LinkTarget::Note),
            "todo" => Some(LinkTarget::Todo),
            "bookmark" => Some(LinkTarget::Bookmark),
            "layout" => Some(LinkTarget::Layout),
            "file" => Some(LinkTarget::File),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LinkRef {
    pub target_type: LinkTarget,
    pub target_id: String,
    #[serde(default)]
    pub anchor: Option<String>,
    #[serde(default)]
    pub display_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkTrigger {
    pub at_char_index: usize,
    pub query: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkSearchResult {
    pub link: LinkRef,
    pub title: String,
    pub subtitle: String,
    pub type_badge: String,
    pub recent_hits: u32,
}

pub trait LinkSearchProvider {
    fn search_documents(&self) -> Vec<LinkSearchResult>;
    fn resolve(&self, link: &LinkRef) -> bool;
}

#[derive(Default)]
pub struct LinkSearchCatalog {
    providers: Vec<Box<dyn LinkSearchProvider>>,
}

impl LinkSearchCatalog {
    pub fn register(&mut self, provider: Box<dyn LinkSearchProvider>) {
        self.providers.push(provider);
    }

    pub fn fuzzy_search(&self, query: &str) -> Vec<LinkSearchResult> {
        let matcher = SkimMatcherV2::default();
        let query = query.trim().to_ascii_lowercase();
        let mut scored: Vec<(u8, i64, i64, LinkSearchResult)> = self
            .providers
            .iter()
            .flat_map(|provider| provider.search_documents())
            .filter_map(|doc| rank_search_result(&matcher, &query, doc))
            .collect();
        scored.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| b.1.cmp(&a.1))
                .then_with(|| b.2.cmp(&a.2))
                .then_with(|| {
                    a.3.title
                        .to_ascii_lowercase()
                        .cmp(&b.3.title.to_ascii_lowercase())
                })
                .then_with(|| a.3.link.target_id.cmp(&b.3.link.target_id))
        });
        scored.into_iter().map(|(_, _, _, item)| item).collect()
    }

    pub fn resolve_for_insert(&self, link: &LinkRef) -> Result<String, ResolveLinkError> {
        if self.providers.iter().any(|provider| provider.resolve(link)) {
            Ok(format_link_id(link))
        } else {
            Err(ResolveLinkError::MissingTarget)
        }
    }
}

fn rank_search_result(
    matcher: &SkimMatcherV2,
    query: &str,
    doc: LinkSearchResult,
) -> Option<(u8, i64, i64, LinkSearchResult)> {
    if query.is_empty() {
        return Some((3, 0, doc.recent_hits as i64, doc));
    }
    let title = doc.title.to_ascii_lowercase();
    if title == query {
        return Some((0, i64::MAX, doc.recent_hits as i64, doc));
    }
    if title.starts_with(query) {
        return Some((1, i64::MAX / 2, doc.recent_hits as i64, doc));
    }
    if let Some(score) = matcher.fuzzy_match(&title, query) {
        return Some((2, score, doc.recent_hits as i64, doc));
    }
    if doc.recent_hits > 0 {
        return Some((3, 0, doc.recent_hits as i64, doc));
    }
    None
}

pub fn detect_link_trigger(text: &str, cursor_char_index: usize) -> Option<LinkTrigger> {
    let cursor_byte_index = char_to_byte_index(text, cursor_char_index);
    let prefix = &text[..cursor_byte_index];
    let at_byte_index = prefix.rfind('@')?;

    if is_code_context_at(prefix, at_byte_index) {
        return None;
    }
    if at_byte_index > 0 && prefix.as_bytes()[at_byte_index - 1] == b'\\' {
        return None;
    }
    let before = prefix[..at_byte_index].chars().next_back();
    if before.is_some_and(|ch| ch.is_alphanumeric()) {
        return None;
    }
    let query = &prefix[at_byte_index + 1..];
    if query
        .chars()
        .any(|ch| ch.is_whitespace() || matches!(ch, '`' | ']'))
    {
        return None;
    }

    Some(LinkTrigger {
        at_char_index: prefix[..at_byte_index].chars().count(),
        query: query.to_string(),
    })
}

pub fn format_inserted_link(link: &LinkRef) -> String {
    format_link_id(link)
}

fn is_code_context_at(text: &str, at_byte_index: usize) -> bool {
    let bytes = text.as_bytes();
    let mut idx = 0;
    let mut in_fenced = false;
    let mut in_inline = false;

    while idx < at_byte_index {
        if idx + 2 < at_byte_index && &bytes[idx..idx + 3] == b"```" {
            in_fenced = !in_fenced;
            idx += 3;
            continue;
        }
        if !in_fenced && bytes[idx] == b'`' {
            in_inline = !in_inline;
        }
        idx += 1;
    }

    in_fenced || in_inline
}

fn char_to_byte_index(s: &str, char_index: usize) -> usize {
    s.char_indices()
        .nth(char_index)
        .map(|(idx, _)| idx)
        .unwrap_or_else(|| s.len())
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntityKey {
    pub entity_type: LinkTarget,
    pub entity_id: String,
}

impl EntityKey {
    pub fn new(entity_type: LinkTarget, entity_id: impl Into<String>) -> Self {
        Self {
            entity_type,
            entity_id: entity_id.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkParseError {
    InvalidScheme,
    InvalidTargetType,
    MissingTargetId,
}

/// Canonical format: `link://<targetType>/<targetId>[#<anchor>][?text=<urlencoded>]`.
pub fn format_link_id(link: &LinkRef) -> String {
    let mut out = format!(
        "link://{}/{}",
        link.target_type.as_str(),
        urlencoding::encode(&link.target_id)
    );
    if let Some(anchor) = &link.anchor {
        if !anchor.is_empty() {
            out.push('#');
            out.push_str(&urlencoding::encode(anchor));
        }
    }
    if let Some(text) = &link.display_text {
        if !text.is_empty() {
            out.push_str("?text=");
            out.push_str(&urlencoding::encode(text));
        }
    }
    out
}

pub fn parse_link_id(link_id: &str) -> Result<LinkRef, LinkParseError> {
    let rest = link_id
        .strip_prefix("link://")
        .ok_or(LinkParseError::InvalidScheme)?;
    let (path_part, query_part) = rest.split_once('?').unwrap_or((rest, ""));
    let (path_core, anchor) = path_part
        .split_once('#')
        .map(|(a, b)| (a, Some(b)))
        .unwrap_or((path_part, None));
    let (target_type_raw, target_id_raw) = path_core
        .split_once('/')
        .ok_or(LinkParseError::MissingTargetId)?;
    let target_type =
        LinkTarget::parse(target_type_raw).ok_or(LinkParseError::InvalidTargetType)?;
    let target_id = urlencoding::decode(target_id_raw)
        .map_err(|_| LinkParseError::MissingTargetId)?
        .to_string();
    if target_id.trim().is_empty() {
        return Err(LinkParseError::MissingTargetId);
    }
    let anchor = anchor
        .filter(|s| !s.is_empty())
        .map(|s| urlencoding::decode(s).unwrap_or_default().to_string())
        .filter(|s| !s.trim().is_empty());
    let mut display_text = None;
    if !query_part.is_empty() {
        for pair in query_part.split('&') {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            if k == "text" {
                let decoded = urlencoding::decode(v).unwrap_or_default().to_string();
                if !decoded.trim().is_empty() {
                    display_text = Some(decoded);
                }
            }
        }
    }
    Ok(LinkRef {
        target_type,
        target_id,
        anchor,
        display_text,
    })
}

#[derive(Debug, Clone, Default)]
pub struct LinkIndex {
    outgoing: HashMap<EntityKey, Vec<LinkRef>>,
    backlinks: HashMap<EntityKey, BTreeSet<EntityKey>>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct BacklinkFilters {
    pub linked_todos: bool,
    pub related_notes: bool,
    pub mentions: bool,
}

impl LinkIndex {
    pub fn set_outgoing_links(&mut self, source: EntityKey, links: Vec<LinkRef>) {
        if let Some(prev) = self.outgoing.insert(source.clone(), links.clone()) {
            self.remove_reverse_entries(&source, &prev);
        }
        self.add_reverse_entries(&source, &links);
    }

    pub fn remove_entity(&mut self, source: &EntityKey) {
        if let Some(prev) = self.outgoing.remove(source) {
            self.remove_reverse_entries(source, &prev);
        }
    }

    pub fn get_forward_links(&self, source: &EntityKey) -> Vec<LinkRef> {
        self.outgoing.get(source).cloned().unwrap_or_default()
    }

    pub fn get_backlinks(&self, target: &EntityKey, filters: BacklinkFilters) -> Vec<EntityKey> {
        let mut entries: Vec<_> = self
            .backlinks
            .get(target)
            .into_iter()
            .flat_map(|set| set.iter())
            .filter(|source| match source.entity_type {
                LinkTarget::Todo => !filters.any_set() || filters.linked_todos,
                LinkTarget::Note => !filters.any_set() || filters.related_notes,
                LinkTarget::Bookmark | LinkTarget::Layout | LinkTarget::File => {
                    !filters.any_set() || filters.mentions
                }
            })
            .cloned()
            .collect();
        entries.sort_by(|a, b| a.entity_id.cmp(&b.entity_id));
        entries
    }

    pub fn search_link_targets(&self, query: &str, scopes: &[LinkTarget]) -> Vec<EntityKey> {
        let query = query.trim().to_ascii_lowercase();
        let scope_set: HashSet<LinkTarget> = scopes.iter().copied().collect();
        let include_all = scope_set.is_empty();
        let mut seen = BTreeSet::new();
        let mut results = Vec::new();
        for links in self.outgoing.values() {
            for link in links {
                if !include_all && !scope_set.contains(&link.target_type) {
                    continue;
                }
                if query.is_empty() || link.target_id.to_ascii_lowercase().contains(&query) {
                    let key = format!("{}:{}", link.target_type.as_str(), link.target_id);
                    if seen.insert(key) {
                        results.push(EntityKey::new(link.target_type, link.target_id.clone()));
                    }
                }
            }
        }
        results
    }

    fn add_reverse_entries(&mut self, source: &EntityKey, links: &[LinkRef]) {
        for link in links {
            let target = EntityKey::new(link.target_type, link.target_id.clone());
            self.backlinks
                .entry(target)
                .or_default()
                .insert(source.clone());
        }
    }

    fn remove_reverse_entries(&mut self, source: &EntityKey, links: &[LinkRef]) {
        for link in links {
            let target = EntityKey::new(link.target_type, link.target_id.clone());
            if let Some(sources) = self.backlinks.get_mut(&target) {
                sources.remove(source);
                if sources.is_empty() {
                    self.backlinks.remove(&target);
                }
            }
        }
    }
}

impl BacklinkFilters {
    fn any_set(self) -> bool {
        self.linked_todos || self.related_notes || self.mentions
    }
}

pub trait LinkTelemetry {
    fn on_resolve_failure(&self, link_id: &str, reason: &ResolveLinkError);
    fn on_broken_anchor(&self, link: &LinkRef, anchor: &str);
}

pub struct TracingLinkTelemetry;

impl LinkTelemetry for TracingLinkTelemetry {
    fn on_resolve_failure(&self, link_id: &str, reason: &ResolveLinkError) {
        warn!(link_id, ?reason, "link resolution failed");
    }

    fn on_broken_anchor(&self, link: &LinkRef, anchor: &str) {
        warn!(
            target_type = link.target_type.as_str(),
            target_id = link.target_id,
            anchor,
            "link anchor is invalid"
        );
    }
}

#[derive(Debug, Clone, Default)]
pub struct LinkResolverCatalog {
    existing: HashSet<EntityKey>,
    anchors: HashMap<EntityKey, HashSet<String>>,
}

impl LinkResolverCatalog {
    pub fn add_target(&mut self, target: EntityKey) {
        self.existing.insert(target);
    }

    pub fn add_anchor(&mut self, target: EntityKey, anchor: impl Into<String>) {
        self.existing.insert(target.clone());
        self.anchors
            .entry(target)
            .or_default()
            .insert(anchor.into().to_ascii_lowercase());
    }

    fn has_target(&self, target: &EntityKey) -> bool {
        self.existing.contains(target)
    }

    fn has_anchor(&self, target: &EntityKey, anchor: &str) -> bool {
        self.anchors
            .get(target)
            .map(|anchors| anchors.contains(&anchor.to_ascii_lowercase()))
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveLinkError {
    InvalidLinkId(LinkParseError),
    MissingTarget,
    InvalidAnchor,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedLink {
    pub canonical: LinkRef,
    pub location: String,
}

pub fn resolve_link(
    link_id: &str,
    catalog: &LinkResolverCatalog,
    telemetry: &dyn LinkTelemetry,
) -> Result<ResolvedLink, ResolveLinkError> {
    let parsed = parse_link_id(link_id).map_err(ResolveLinkError::InvalidLinkId)?;
    let key = EntityKey::new(parsed.target_type, parsed.target_id.clone());
    if !catalog.has_target(&key) {
        let err = ResolveLinkError::MissingTarget;
        telemetry.on_resolve_failure(link_id, &err);
        return Err(err);
    }
    if let Some(anchor) = &parsed.anchor {
        if !catalog.has_anchor(&key, anchor) {
            telemetry.on_broken_anchor(&parsed, anchor);
            let err = ResolveLinkError::InvalidAnchor;
            telemetry.on_resolve_failure(link_id, &err);
            return Err(err);
        }
    }
    Ok(ResolvedLink {
        canonical: parsed.clone(),
        location: format_link_id(&parsed),
    })
}

pub fn build_index_from_notes_and_todos(notes: &[Note], todos: &[TodoEntry]) -> LinkIndex {
    let mut index = LinkIndex::default();
    for note in notes {
        let source = EntityKey::new(LinkTarget::Note, note.slug.clone());
        index.set_outgoing_links(source, links_from_note(note));
    }
    for todo in todos {
        let source = EntityKey::new(LinkTarget::Todo, todo.id.clone());
        index.set_outgoing_links(source, links_from_todo(todo));
    }
    index
}

pub fn links_from_note(note: &Note) -> Vec<LinkRef> {
    let mut links = Vec::new();
    for slug in &note.links {
        links.push(LinkRef {
            target_type: LinkTarget::Note,
            target_id: slug.clone(),
            anchor: None,
            display_text: None,
        });
    }
    for r in &note.entity_refs {
        if let Some(target_type) = map_entity_kind(r.kind.clone()) {
            links.push(LinkRef {
                target_type,
                target_id: r.id.clone(),
                anchor: None,
                display_text: r.title.clone(),
            });
        }
    }
    dedupe_links(links)
}

pub fn links_from_todo(todo: &TodoEntry) -> Vec<LinkRef> {
    let mut links = Vec::new();
    for r in &todo.entity_refs {
        if let Some(target_type) = map_entity_kind(r.kind.clone()) {
            links.push(LinkRef {
                target_type,
                target_id: r.id.clone(),
                anchor: None,
                display_text: r.title.clone(),
            });
        }
    }
    dedupe_links(links)
}

fn map_entity_kind(kind: EntityKind) -> Option<LinkTarget> {
    match kind {
        EntityKind::Note => Some(LinkTarget::Note),
        EntityKind::Todo => Some(LinkTarget::Todo),
        EntityKind::Event => None,
    }
}

fn dedupe_links(mut links: Vec<LinkRef>) -> Vec<LinkRef> {
    links.sort_by(|a, b| {
        (
            &a.target_type.as_str(),
            &a.target_id,
            &a.anchor,
            &a.display_text,
        )
            .cmp(&(
                &b.target_type.as_str(),
                &b.target_id,
                &b.anchor,
                &b.display_text,
            ))
    });
    links.dedup();
    links
}

pub fn migrate_legacy_links(metadata: &serde_json::Value) -> Vec<LinkRef> {
    let mut links = Vec::new();
    if let Some(items) = metadata.get("links").and_then(|v| v.as_array()) {
        for item in items {
            let ty = item
                .get("type")
                .and_then(|v| v.as_str())
                .and_then(LinkTarget::parse);
            let id = item.get("id").and_then(|v| v.as_str());
            if let (Some(target_type), Some(target_id)) = (ty, id) {
                links.push(LinkRef {
                    target_type,
                    target_id: target_id.to_string(),
                    anchor: item
                        .get("anchor")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    display_text: item
                        .get("text")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                });
            }
        }
    }
    if let Some(raw_refs) = metadata
        .get("metadata")
        .and_then(|v| v.get("refs"))
        .and_then(|v| v.as_array())
    {
        for entry in raw_refs.iter().filter_map(|v| v.as_str()) {
            if let Some((kind, id)) = entry.trim_start_matches('@').split_once(':') {
                if let Some(target_type) = LinkTarget::parse(kind) {
                    if !id.trim().is_empty() {
                        links.push(LinkRef {
                            target_type,
                            target_id: id.trim().to_string(),
                            anchor: None,
                            display_text: None,
                        });
                    }
                }
            }
        }
    }
    dedupe_links(links)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::entity_ref::EntityRef;
    use std::path::PathBuf;

    struct ProviderFixture {
        docs: Vec<LinkSearchResult>,
    }

    impl LinkSearchProvider for ProviderFixture {
        fn search_documents(&self) -> Vec<LinkSearchResult> {
            self.docs.clone()
        }

        fn resolve(&self, link: &LinkRef) -> bool {
            self.docs.iter().any(|doc| {
                doc.link.target_type == link.target_type && doc.link.target_id == link.target_id
            })
        }
    }

    fn fixture_doc(kind: LinkTarget, id: &str, title: &str, recent_hits: u32) -> LinkSearchResult {
        LinkSearchResult {
            link: LinkRef {
                target_type: kind,
                target_id: id.to_string(),
                anchor: None,
                display_text: None,
            },
            title: title.to_string(),
            subtitle: format!("/{id}"),
            type_badge: format!("{kind:?}"),
            recent_hits,
        }
    }

    struct TestTelemetry {
        failures: std::sync::Mutex<Vec<ResolveLinkError>>,
    }

    impl TestTelemetry {
        fn new() -> Self {
            Self {
                failures: std::sync::Mutex::new(Vec::new()),
            }
        }
    }

    impl LinkTelemetry for TestTelemetry {
        fn on_resolve_failure(&self, _link_id: &str, reason: &ResolveLinkError) {
            self.failures.lock().unwrap().push(reason.clone());
        }

        fn on_broken_anchor(&self, _link: &LinkRef, _anchor: &str) {}
    }

    #[test]
    fn link_id_round_trip() {
        let link = LinkRef {
            target_type: LinkTarget::Note,
            target_id: "release plan".into(),
            anchor: Some("section 2".into()),
            display_text: Some("Release Plan".into()),
        };
        let id = format_link_id(&link);
        let parsed = parse_link_id(&id).unwrap();
        assert_eq!(parsed, link);
    }

    #[test]
    fn reverse_index_tracks_add_update_delete() {
        let source = EntityKey::new(LinkTarget::Todo, "todo-1");
        let target_a = EntityKey::new(LinkTarget::Note, "alpha");
        let target_b = EntityKey::new(LinkTarget::Note, "beta");
        let mut index = LinkIndex::default();

        index.set_outgoing_links(
            source.clone(),
            vec![LinkRef {
                target_type: LinkTarget::Note,
                target_id: "alpha".into(),
                anchor: None,
                display_text: None,
            }],
        );
        assert_eq!(
            index.get_backlinks(&target_a, BacklinkFilters::default()),
            vec![source.clone()]
        );

        index.set_outgoing_links(
            source.clone(),
            vec![LinkRef {
                target_type: LinkTarget::Note,
                target_id: "beta".into(),
                anchor: None,
                display_text: None,
            }],
        );
        assert!(index
            .get_backlinks(&target_a, BacklinkFilters::default())
            .is_empty());
        assert_eq!(
            index.get_backlinks(&target_b, BacklinkFilters::default()),
            vec![source.clone()]
        );

        index.remove_entity(&source);
        assert!(index
            .get_backlinks(&target_b, BacklinkFilters::default())
            .is_empty());
    }

    #[test]
    fn resolver_handles_valid_missing_and_invalid_anchor() {
        let telemetry = TestTelemetry::new();
        let mut catalog = LinkResolverCatalog::default();
        catalog.add_target(EntityKey::new(LinkTarget::Note, "alpha"));
        catalog.add_anchor(EntityKey::new(LinkTarget::Note, "alpha"), "intro");

        let ok = resolve_link("link://note/alpha#intro", &catalog, &telemetry);
        assert!(ok.is_ok());

        let missing = resolve_link("link://note/missing", &catalog, &telemetry);
        assert_eq!(missing, Err(ResolveLinkError::MissingTarget));

        let bad_anchor = resolve_link("link://note/alpha#missing", &catalog, &telemetry);
        assert_eq!(bad_anchor, Err(ResolveLinkError::InvalidAnchor));
    }

    #[test]
    fn migrates_legacy_metadata_shapes() {
        let blob = serde_json::json!({
            "links": [
                {"type": "note", "id": "alpha"},
                {"type": "todo", "id": "todo-1", "text": "Todo One"}
            ],
            "metadata": {
                "refs": ["@note:beta", "@layout:daily"]
            }
        });
        let links = migrate_legacy_links(&blob);
        assert_eq!(
            links,
            vec![
                LinkRef {
                    target_type: LinkTarget::Layout,
                    target_id: "daily".into(),
                    anchor: None,
                    display_text: None,
                },
                LinkRef {
                    target_type: LinkTarget::Note,
                    target_id: "alpha".into(),
                    anchor: None,
                    display_text: None,
                },
                LinkRef {
                    target_type: LinkTarget::Note,
                    target_id: "beta".into(),
                    anchor: None,
                    display_text: None,
                },
                LinkRef {
                    target_type: LinkTarget::Todo,
                    target_id: "todo-1".into(),
                    anchor: None,
                    display_text: Some("Todo One".into()),
                }
            ]
        );
    }

    #[test]
    fn todo_note_links_are_discoverable_from_shared_index() {
        let notes = vec![Note {
            title: "Alpha".into(),
            path: PathBuf::from("alpha.md"),
            content: String::new(),
            tags: vec![],
            links: vec!["beta".into()],
            slug: "alpha".into(),
            alias: None,
            entity_refs: vec![EntityRef::new(EntityKind::Todo, "todo-1", None)],
        }];
        let todos = vec![TodoEntry {
            id: "todo-1".into(),
            text: "Do thing".into(),
            done: false,
            priority: 1,
            tags: vec![],
            entity_refs: vec![EntityRef::new(EntityKind::Note, "alpha", None)],
        }];

        let index = build_index_from_notes_and_todos(&notes, &todos);

        let todo_forward = index.get_forward_links(&EntityKey::new(LinkTarget::Todo, "todo-1"));
        assert!(todo_forward
            .iter()
            .any(|l| l.target_type == LinkTarget::Note && l.target_id == "alpha"));

        let backlinks = index.get_backlinks(
            &EntityKey::new(LinkTarget::Note, "alpha"),
            BacklinkFilters {
                linked_todos: true,
                ..BacklinkFilters::default()
            },
        );
        assert_eq!(backlinks, vec![EntityKey::new(LinkTarget::Todo, "todo-1")]);
    }

    #[test]
    fn trigger_detection_rejects_escaped_or_code_context() {
        let valid = detect_link_trigger("hello @pla", "hello @pla".chars().count());
        assert_eq!(
            valid,
            Some(LinkTrigger {
                at_char_index: 6,
                query: "pla".to_string()
            })
        );
        assert!(detect_link_trigger("hello \\@pla", "hello \\@pla".chars().count()).is_none());
        assert!(detect_link_trigger("`hello @pla`", "`hello @pla`".chars().count()).is_none());
        assert!(detect_link_trigger("```\n@pla\n```", "```\n@pla".chars().count()).is_none());
    }

    #[test]
    fn ranking_prefers_exact_then_prefix_then_fuzzy_then_recent() {
        let mut catalog = LinkSearchCatalog::default();
        catalog.register(Box::new(ProviderFixture {
            docs: vec![
                fixture_doc(LinkTarget::Note, "n1", "plan", 0),
                fixture_doc(LinkTarget::Todo, "t1", "planet", 0),
                fixture_doc(LinkTarget::Bookmark, "b1", "p-l-a-n board", 0),
                fixture_doc(LinkTarget::Layout, "l1", "workspace", 5),
            ],
        }));

        let ranked = catalog.fuzzy_search("plan");
        let ids: Vec<String> = ranked.into_iter().map(|r| r.link.target_id).collect();
        assert_eq!(ids, vec!["n1", "t1", "b1", "l1"]);
    }

    #[test]
    fn insertion_formatter_preserves_anchor_and_text() {
        let base = LinkRef {
            target_type: LinkTarget::Note,
            target_id: "alpha".to_string(),
            anchor: None,
            display_text: None,
        };
        assert_eq!(format_inserted_link(&base), "link://note/alpha");

        let with_anchor = LinkRef {
            target_type: LinkTarget::Note,
            target_id: "alpha".to_string(),
            anchor: Some("section-1".to_string()),
            display_text: Some("Section 1".to_string()),
        };
        assert_eq!(
            format_inserted_link(&with_anchor),
            "link://note/alpha#section-1?text=Section%201"
        );
    }

    #[test]
    fn provider_contract_search_and_resolve() {
        let mut catalog = LinkSearchCatalog::default();
        catalog.register(Box::new(ProviderFixture {
            docs: vec![fixture_doc(LinkTarget::File, "Cargo.toml", "Cargo.toml", 1)],
        }));

        let hits = catalog.fuzzy_search("cargo");
        assert_eq!(hits.len(), 1);
        let ok = catalog.resolve_for_insert(&hits[0].link);
        assert!(ok.is_ok());

        let missing = catalog.resolve_for_insert(&LinkRef {
            target_type: LinkTarget::File,
            target_id: "missing.txt".to_string(),
            anchor: None,
            display_text: None,
        });
        assert_eq!(missing, Err(ResolveLinkError::MissingTarget));
    }
}
