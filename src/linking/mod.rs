use crate::plugins::note::Note;
use crate::plugins::todo::TodoEntry;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

pub mod index;
pub mod migrate;
pub mod parse;
pub mod resolve;

pub use index::{build_index_from_notes_and_todos, BacklinkFilters, EntityKey, LinkIndex};
pub use migrate::migrate_legacy_links;
pub use parse::{
    detect_link_trigger, format_inserted_link, format_link_id, parse_link_id, LinkParseError,
    LinkRef, LinkTarget, LinkTrigger,
};
pub use resolve::{
    resolve_link, LinkResolverCatalog, LinkTelemetry, ResolveLinkError, ResolvedLink,
    TracingLinkTelemetry,
};

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::entity_ref::{EntityKind, EntityRef};
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

        assert!(resolve_link("link://note/alpha#intro", &catalog, &telemetry).is_ok());
        assert_eq!(
            resolve_link("link://note/missing", &catalog, &telemetry),
            Err(ResolveLinkError::MissingTarget)
        );
        assert_eq!(
            resolve_link("link://note/alpha#missing", &catalog, &telemetry),
            Err(ResolveLinkError::InvalidAnchor)
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
    fn provider_contract_search_and_resolve() {
        let mut catalog = LinkSearchCatalog::default();
        catalog.register(Box::new(ProviderFixture {
            docs: vec![fixture_doc(LinkTarget::File, "Cargo.toml", "Cargo.toml", 1)],
        }));

        let hits = catalog.fuzzy_search("cargo");
        assert_eq!(hits.len(), 1);
        assert!(catalog.resolve_for_insert(&hits[0].link).is_ok());
        assert_eq!(
            catalog.resolve_for_insert(&LinkRef {
                target_type: LinkTarget::File,
                target_id: "missing.txt".to_string(),
                anchor: None,
                display_text: None,
            }),
            Err(ResolveLinkError::MissingTarget)
        );
    }
}
