use crate::common::entity_ref::EntityKind;
use crate::linking::{LinkRef, LinkTarget};
use crate::plugins::note::Note;
use crate::plugins::todo::TodoEntry;
use std::collections::{BTreeSet, HashMap, HashSet};

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

pub(crate) fn dedupe_links(mut links: Vec<LinkRef>) -> Vec<LinkRef> {
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
