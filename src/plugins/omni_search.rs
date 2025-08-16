use crate::actions::Action;
use crate::plugin::Plugin;
use crate::plugins::bookmarks::BookmarksPlugin;
use crate::plugins::folders::FoldersPlugin;
use fst::{automaton::Subsequence, IntoStreamer, Map, MapBuilder, Streamer};
use std::collections::HashSet;
use std::sync::Arc;

/// Combined search across folders, bookmarks, and launcher actions.
///
/// The action list is provided as an [`Arc<Vec<Action>>`] so the plugin can
/// participate in searches without holding its own copy. Cloning the `Arc`
/// replicates only the pointer, keeping the underlying `Vec` shared and
/// thread-safe.
pub struct OmniSearchPlugin {
    folders: FoldersPlugin,
    bookmarks: BookmarksPlugin,
    /// Shared list of launcher actions searched alongside folders and
    /// bookmarks. Cloning the `Arc` only clones the pointer so the underlying
    /// `Vec` remains shared.
    actions: Arc<Vec<Action>>,
    index: Map<Vec<u8>>,
}

impl OmniSearchPlugin {
    /// Create a new `OmniSearchPlugin`.
    ///
    /// `actions` is an [`Arc`] over the application's action list. Cloning the
    /// `Arc` does not clone the `Vec` itself, so the plugin can read the shared
    /// action data without duplicating it.
    pub fn new(actions: Arc<Vec<Action>>) -> Self {
        let mut entries: Vec<(String, u64)> = Vec::new();
        for (i, a) in actions.iter().enumerate() {
            entries.push((a.label.to_lowercase(), i as u64));
            if !a.desc.is_empty() {
                entries.push((a.desc.to_lowercase(), i as u64));
            }
        }
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        let mut builder = MapBuilder::memory();
        for (k, v) in entries {
            builder.insert(k, v).unwrap();
        }
        let index = Map::new(builder.into_inner().unwrap()).unwrap();

        Self {
            folders: FoldersPlugin::default(),
            bookmarks: BookmarksPlugin::default(),
            actions,
            index,
        }
    }
}

impl Plugin for OmniSearchPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        const LIST_PREFIX: &str = "o list";
        if let Some(rest) = crate::common::strip_prefix_ci(query, LIST_PREFIX) {
            return self.search_all(rest.trim_start());
        }

        const PREFIX: &str = "o";
        let rest = match crate::common::strip_prefix_ci(query, PREFIX) {
            Some(r) => r.trim_start(),
            None => return Vec::new(),
        };
        self.search_all(rest)
    }

    fn name(&self) -> &str {
        "omni_search"
    }

    fn description(&self) -> &str {
        "Combined search for folders, bookmarks and apps (prefix: `o`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "o".into(),
                desc: "Omni".into(),
                action: "query:o ".into(),
                args: None,
            },
            Action {
                label: "o list".into(),
                desc: "Omni".into(),
                action: "query:o list".into(),
                args: None,
            },
        ]
    }
}

impl OmniSearchPlugin {
    fn search_all(&self, rest: &str) -> Vec<Action> {
        let mut out = Vec::new();
        out.extend(self.folders.search(&format!("f {rest}")));
        if rest.trim().is_empty() {
            out.extend(self.bookmarks.search("bm list"));
        } else {
            out.extend(self.bookmarks.search(&format!("bm {rest}")));
        }
        let q = rest.trim();
        if q.is_empty() {
            out.extend(self.actions.iter().cloned());
        } else {
            let q_lc = q.to_lowercase();
            let automaton = Subsequence::new(&q_lc);
            let mut stream = self.index.search(automaton).into_stream();
            let mut seen = HashSet::new();
            while let Some((_, idx)) = stream.next() {
                if seen.insert(idx) {
                    if let Some(a) = self.actions.get(idx as usize) {
                        out.push(a.clone());
                    }
                }
            }
        }
        out
    }
}
