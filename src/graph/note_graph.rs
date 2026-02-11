use crate::plugins::note::Note;
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum GraphNodeType {
    Note,
    Tag,
    Broken,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub tags: Vec<String>,
    pub degree: usize,
    pub node_type: Option<GraphNodeType>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    pub directed: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NoteGraphModel {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub adjacency: HashMap<String, Vec<String>>, // outgoing adjacency
    pub node_lookup: HashMap<String, usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct NoteGraphFilter {
    pub include_tags: BTreeSet<String>,
    pub exclude_tags: BTreeSet<String>,
    pub orphan_notes_only: bool,
    pub root_slug: Option<String>,
    pub depth: Option<usize>,
    pub max_nodes: Option<usize>,
    pub include_backlinks: bool,
}

impl NoteGraphFilter {
    pub fn normalized(mut self) -> Self {
        self.include_tags = self
            .include_tags
            .iter()
            .map(|tag| normalize_tag(tag))
            .collect();
        self.exclude_tags = self
            .exclude_tags
            .iter()
            .map(|tag| normalize_tag(tag))
            .collect();
        self.depth = self.depth.map(|d| d.clamp(1, 3));
        self
    }

    pub fn filter_hash(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NodePhysics {
    pub position: [f32; 2],
    pub velocity: [f32; 2],
    pub pinned: bool,
}

impl Default for NodePhysics {
    fn default() -> Self {
        Self {
            position: [0.0, 0.0],
            velocity: [0.0, 0.0],
            pinned: false,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LayoutState {
    pub nodes: HashMap<String, NodePhysics>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutConfig {
    pub iterations_per_frame: usize,
    pub repulsion_strength: f32,
    pub link_distance: f32,
    pub damping: f32,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            iterations_per_frame: 2,
            repulsion_strength: 3000.0,
            link_distance: 60.0,
            damping: 0.85,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct NoteGraphEngine {
    pub model: NoteGraphModel,
    pub layout: LayoutState,
    last_note_version: Option<u64>,
    last_filter_hash: Option<u64>,
}

impl NoteGraphEngine {
    pub fn rebuild_if_needed(
        &mut self,
        notes: &[Note],
        note_version: u64,
        filter: &NoteGraphFilter,
    ) -> bool {
        let normalized = filter.clone().normalized();
        let filter_hash = normalized.filter_hash();
        let should_rebuild = self.last_note_version != Some(note_version)
            || self.last_filter_hash != Some(filter_hash);

        if !should_rebuild {
            return false;
        }

        self.model = build_note_graph(notes, &normalized);
        self.layout.sync_model(&self.model);
        self.last_note_version = Some(note_version);
        self.last_filter_hash = Some(filter_hash);
        true
    }
}

pub fn build_note_graph(notes: &[Note], filter: &NoteGraphFilter) -> NoteGraphModel {
    let filter = filter.clone().normalized();
    let mut candidates: HashMap<String, GraphNode> = HashMap::new();

    for note in notes {
        if !passes_tag_filter(note, &filter) {
            continue;
        }
        candidates.insert(
            note.slug.clone(),
            GraphNode {
                id: note.slug.clone(),
                label: note.title.clone(),
                tags: note.tags.iter().map(|t| normalize_tag(t)).collect(),
                degree: 0,
                node_type: Some(GraphNodeType::Note),
            },
        );
    }

    let mut edge_set: BTreeSet<(String, String, bool)> = BTreeSet::new();
    for note in notes {
        if !candidates.contains_key(&note.slug) {
            continue;
        }
        for link in &note.links {
            if !candidates.contains_key(link) {
                continue;
            }
            let _ = edge_set.insert((note.slug.clone(), link.clone(), true));
        }
    }

    if filter.include_backlinks {
        let existing: Vec<_> = edge_set.iter().cloned().collect();
        for (from, to, _) in existing {
            let _ = edge_set.insert((to, from, true));
        }
    }

    let mut edges: Vec<GraphEdge> = edge_set
        .iter()
        .map(|(from, to, directed)| GraphEdge {
            from: from.clone(),
            to: to.clone(),
            directed: *directed,
        })
        .collect();

    let mut kept_nodes: HashSet<String> = candidates.keys().cloned().collect();

    if filter.orphan_notes_only {
        let degrees = compute_degrees(&edges);
        kept_nodes.retain(|id| degrees.get(id).copied().unwrap_or(0) == 0);
        edges.clear();
    }

    if let Some(root) = filter.root_slug.as_ref() {
        let depth = filter.depth.unwrap_or(1).clamp(1, 3);
        let scoped = extract_local_scope(root, depth, &edges, &kept_nodes);
        kept_nodes = scoped;
    }

    if let Some(max_nodes) = filter.max_nodes {
        if max_nodes > 0 && kept_nodes.len() > max_nodes {
            let mut ranked: Vec<_> = kept_nodes.iter().cloned().collect();
            let degree_map = compute_degrees_for_nodes(&edges, &kept_nodes);
            ranked.sort_by(|a, b| {
                degree_map
                    .get(b)
                    .copied()
                    .unwrap_or(0)
                    .cmp(&degree_map.get(a).copied().unwrap_or(0))
                    .then_with(|| a.cmp(b))
            });
            kept_nodes = ranked.into_iter().take(max_nodes).collect();
        }
    }

    edges.retain(|e| kept_nodes.contains(&e.from) && kept_nodes.contains(&e.to));

    let degrees = compute_degrees(&edges);

    let mut nodes: Vec<GraphNode> = kept_nodes
        .iter()
        .filter_map(|id| candidates.get(id).cloned())
        .collect();
    nodes.sort_by(|a, b| a.id.cmp(&b.id));
    for node in &mut nodes {
        node.degree = degrees.get(&node.id).copied().unwrap_or(0);
    }

    let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();
    for node in &nodes {
        adjacency.entry(node.id.clone()).or_default();
    }
    for edge in &edges {
        adjacency
            .entry(edge.from.clone())
            .or_default()
            .push(edge.to.clone());
    }
    for neighbors in adjacency.values_mut() {
        neighbors.sort();
        neighbors.dedup();
    }

    let node_lookup = nodes
        .iter()
        .enumerate()
        .map(|(idx, node)| (node.id.clone(), idx))
        .collect();

    NoteGraphModel {
        nodes,
        edges,
        adjacency,
        node_lookup,
    }
}

fn passes_tag_filter(note: &Note, filter: &NoteGraphFilter) -> bool {
    let tags: BTreeSet<String> = note.tags.iter().map(|t| normalize_tag(t)).collect();

    if !filter.include_tags.is_empty() && !filter.include_tags.is_subset(&tags) {
        return false;
    }

    if tags.iter().any(|tag| filter.exclude_tags.contains(tag)) {
        return false;
    }

    true
}

fn compute_degrees(edges: &[GraphEdge]) -> HashMap<String, usize> {
    let node_ids: HashSet<String> = edges
        .iter()
        .flat_map(|e| [e.from.clone(), e.to.clone()])
        .collect();
    compute_degrees_for_nodes(edges, &node_ids)
}

fn compute_degrees_for_nodes(
    edges: &[GraphEdge],
    node_ids: &HashSet<String>,
) -> HashMap<String, usize> {
    let mut neighbors: HashMap<String, HashSet<String>> = node_ids
        .iter()
        .cloned()
        .map(|id| (id, HashSet::new()))
        .collect();

    for edge in edges {
        if node_ids.contains(&edge.from) && node_ids.contains(&edge.to) {
            neighbors
                .entry(edge.from.clone())
                .or_default()
                .insert(edge.to.clone());
            neighbors
                .entry(edge.to.clone())
                .or_default()
                .insert(edge.from.clone());
        }
    }

    neighbors
        .into_iter()
        .map(|(id, n)| (id, n.len()))
        .collect::<HashMap<_, _>>()
}

fn extract_local_scope(
    root: &str,
    depth: usize,
    edges: &[GraphEdge],
    allowed_nodes: &HashSet<String>,
) -> HashSet<String> {
    if !allowed_nodes.contains(root) {
        return HashSet::new();
    }

    let mut undirected: HashMap<String, Vec<String>> = HashMap::new();
    for edge in edges {
        if allowed_nodes.contains(&edge.from) && allowed_nodes.contains(&edge.to) {
            undirected
                .entry(edge.from.clone())
                .or_default()
                .push(edge.to.clone());
            undirected
                .entry(edge.to.clone())
                .or_default()
                .push(edge.from.clone());
        }
    }

    let mut visited: HashSet<String> = HashSet::new();
    let mut queue = VecDeque::new();
    let _ = visited.insert(root.to_string());
    queue.push_back((root.to_string(), 0usize));

    while let Some((node, d)) = queue.pop_front() {
        if d >= depth {
            continue;
        }

        for neighbor in undirected.get(&node).into_iter().flat_map(|x| x.iter()) {
            if visited.insert(neighbor.clone()) {
                queue.push_back((neighbor.clone(), d + 1));
            }
        }
    }

    visited
}

impl LayoutState {
    pub fn sync_model(&mut self, model: &NoteGraphModel) {
        let existing: HashSet<String> = self.nodes.keys().cloned().collect();
        let next: HashSet<String> = model.nodes.iter().map(|n| n.id.clone()).collect();

        for removed in existing.difference(&next) {
            self.nodes.remove(removed);
        }

        for node in &model.nodes {
            self.nodes
                .entry(node.id.clone())
                .or_insert_with(|| NodePhysics {
                    position: seeded_position(&node.id),
                    ..Default::default()
                });
        }
    }

    pub fn step(&mut self, model: &NoteGraphModel, cfg: LayoutConfig) {
        if model.nodes.is_empty() {
            return;
        }
        self.sync_model(model);

        let iterations = cfg.iterations_per_frame.max(1);
        for _ in 0..iterations {
            let node_ids: Vec<String> = model.nodes.iter().map(|n| n.id.clone()).collect();
            let mut forces: HashMap<String, [f32; 2]> = node_ids
                .iter()
                .map(|id| (id.clone(), [0.0_f32, 0.0_f32]))
                .collect();

            for i in 0..node_ids.len() {
                for j in (i + 1)..node_ids.len() {
                    let a = &node_ids[i];
                    let b = &node_ids[j];
                    let (pa, pb) = match (self.nodes.get(a), self.nodes.get(b)) {
                        (Some(na), Some(nb)) => (na.position, nb.position),
                        _ => continue,
                    };
                    let dx = pa[0] - pb[0];
                    let dy = pa[1] - pb[1];
                    let dist_sq = (dx * dx + dy * dy).max(0.01);
                    let dist = dist_sq.sqrt();
                    let force_mag = cfg.repulsion_strength / dist_sq;
                    let fx = force_mag * dx / dist;
                    let fy = force_mag * dy / dist;

                    if let Some(fa) = forces.get_mut(a) {
                        fa[0] += fx;
                        fa[1] += fy;
                    }
                    if let Some(fb) = forces.get_mut(b) {
                        fb[0] -= fx;
                        fb[1] -= fy;
                    }
                }
            }

            for edge in &model.edges {
                let (pa, pb) = match (self.nodes.get(&edge.from), self.nodes.get(&edge.to)) {
                    (Some(na), Some(nb)) => (na.position, nb.position),
                    _ => continue,
                };
                let dx = pb[0] - pa[0];
                let dy = pb[1] - pa[1];
                let dist = (dx * dx + dy * dy).sqrt().max(0.01);
                let delta = dist - cfg.link_distance;
                let spring = 0.03 * delta;
                let fx = spring * dx / dist;
                let fy = spring * dy / dist;

                if let Some(fa) = forces.get_mut(&edge.from) {
                    fa[0] += fx;
                    fa[1] += fy;
                }
                if let Some(fb) = forces.get_mut(&edge.to) {
                    fb[0] -= fx;
                    fb[1] -= fy;
                }
            }

            for node in &model.nodes {
                let force = forces.get(&node.id).copied().unwrap_or([0.0, 0.0]);
                if let Some(physics) = self.nodes.get_mut(&node.id) {
                    if physics.pinned {
                        physics.velocity = [0.0, 0.0];
                        continue;
                    }
                    physics.velocity[0] = (physics.velocity[0] + force[0] * 0.01) * cfg.damping;
                    physics.velocity[1] = (physics.velocity[1] + force[1] * 0.01) * cfg.damping;
                    physics.position[0] += physics.velocity[0];
                    physics.position[1] += physics.velocity[1];
                }
            }
        }
    }
}

fn seeded_position(seed: &str) -> [f32; 2] {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    seed.hash(&mut hasher);
    let hash = hasher.finish();
    let angle = ((hash & 0xffff) as f32 / 65535.0) * std::f32::consts::TAU;
    let radius = 40.0 + (((hash >> 16) & 0xffff) as f32 / 65535.0) * 30.0;
    [angle.cos() * radius, angle.sin() * radius]
}

fn normalize_tag(tag: &str) -> String {
    tag.trim()
        .trim_start_matches('#')
        .trim_start_matches('@')
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn note(slug: &str, title: &str, tags: &[&str], links: &[&str]) -> Note {
        Note {
            title: title.to_string(),
            path: PathBuf::from(format!("{slug}.md")),
            content: String::new(),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            links: links.iter().map(|l| l.to_string()).collect(),
            slug: slug.to_string(),
            alias: None,
            entity_refs: Vec::new(),
        }
    }

    #[test]
    fn builder_creates_expected_nodes_and_edges() {
        let notes = vec![
            note("alpha", "Alpha", &["work"], &["beta", "gamma"]),
            note("beta", "Beta", &["work"], &["gamma"]),
            note("gamma", "Gamma", &["personal"], &[]),
        ];

        let graph = build_note_graph(&notes, &NoteGraphFilter::default());

        assert_eq!(graph.nodes.len(), 3);
        assert_eq!(graph.edges.len(), 3);
        assert!(graph
            .edges
            .iter()
            .any(|e| e.from == "alpha" && e.to == "beta"));
        assert!(graph
            .edges
            .iter()
            .any(|e| e.from == "alpha" && e.to == "gamma"));
        assert!(graph
            .edges
            .iter()
            .any(|e| e.from == "beta" && e.to == "gamma"));
    }

    #[test]
    fn tag_include_exclude_normalizes_hash_and_at_forms() {
        let notes = vec![
            note("alpha", "Alpha", &["Work"], &[]),
            note("beta", "Beta", &["personal"], &[]),
        ];

        let filter = NoteGraphFilter {
            include_tags: BTreeSet::from(["#work".to_string()]),
            exclude_tags: BTreeSet::from(["@personal".to_string()]),
            ..Default::default()
        };

        let graph = build_note_graph(&notes, &filter);
        let slugs: BTreeSet<_> = graph.nodes.iter().map(|n| n.id.as_str()).collect();

        assert_eq!(slugs, BTreeSet::from(["alpha"]));
    }

    #[test]
    fn local_scope_depth_traversal_limits_nodes() {
        let notes = vec![
            note("a", "A", &[], &["b"]),
            note("b", "B", &[], &["c"]),
            note("c", "C", &[], &["d"]),
            note("d", "D", &[], &[]),
        ];

        let depth1 = build_note_graph(
            &notes,
            &NoteGraphFilter {
                root_slug: Some("a".to_string()),
                depth: Some(1),
                ..Default::default()
            },
        );
        let depth1_nodes: BTreeSet<_> = depth1.nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(depth1_nodes, BTreeSet::from(["a", "b"]));

        let depth2 = build_note_graph(
            &notes,
            &NoteGraphFilter {
                root_slug: Some("a".to_string()),
                depth: Some(2),
                ..Default::default()
            },
        );
        let depth2_nodes: BTreeSet<_> = depth2.nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(depth2_nodes, BTreeSet::from(["a", "b", "c"]));
    }

    #[test]
    fn layout_positions_persist_for_unchanged_nodes_after_rebuild() {
        let notes = vec![note("a", "A", &[], &["b"]), note("b", "B", &[], &[])];

        let mut engine = NoteGraphEngine::default();
        let changed = engine.rebuild_if_needed(&notes, 1, &NoteGraphFilter::default());
        assert!(changed);
        engine.layout.step(&engine.model, LayoutConfig::default());

        let a_before = engine
            .layout
            .nodes
            .get("a")
            .map(|p| p.position)
            .expect("node a should exist after first build");

        let notes_rebuilt = vec![
            note("a", "A", &[], &["b"]),
            note("b", "B", &[], &["c"]),
            note("c", "C", &[], &[]),
        ];
        let changed_again =
            engine.rebuild_if_needed(&notes_rebuilt, 2, &NoteGraphFilter::default());
        assert!(changed_again);

        let a_after = engine
            .layout
            .nodes
            .get("a")
            .map(|p| p.position)
            .expect("node a should still exist after rebuild");

        assert_eq!(a_before, a_after);
    }

    #[test]
    fn max_nodes_cap_is_deterministic_degree_then_slug() {
        let notes = vec![
            note("a", "A", &[], &["b", "c", "d"]),
            note("b", "B", &[], &["a"]),
            note("c", "C", &[], &["a"]),
            note("d", "D", &[], &["a"]),
            note("e", "E", &[], &["f"]),
            note("f", "F", &[], &["e"]),
        ];

        let graph = build_note_graph(
            &notes,
            &NoteGraphFilter {
                max_nodes: Some(3),
                ..Default::default()
            },
        );

        let nodes: Vec<_> = graph.nodes.iter().map(|n| n.id.clone()).collect();
        assert_eq!(nodes, vec!["a", "b", "c"]);
    }
}
