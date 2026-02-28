use crate::actions::Action;
use crate::plugin::Plugin;
use crate::plugins::bookmarks::BookmarksPlugin;
use crate::plugins::folders::FoldersPlugin;
use crate::plugins::note::NotePlugin;
use crate::plugins::todo::TodoPlugin;
use fst::{automaton::Subsequence, IntoStreamer, Map, MapBuilder, Streamer};
use std::collections::HashSet;
use std::sync::Arc;

/// Combined search across folders, bookmarks, notes, todos, and launcher
/// actions.
///
/// The action list is provided as an [`Arc<Vec<Action>>`] so the plugin can
/// participate in searches without holding its own copy. Cloning the `Arc`
/// replicates only the pointer, keeping the underlying `Vec` shared and
/// thread-safe.
pub struct OmniSearchPlugin {
    folders: FoldersPlugin,
    bookmarks: BookmarksPlugin,
    note: NotePlugin,
    todo: TodoPlugin,
    /// Shared list of launcher actions searched alongside folders and
    /// bookmarks. Cloning the `Arc` only clones the pointer so the underlying
    /// `Vec` remains shared.
    actions: Arc<Vec<Action>>,
    index: Option<Map<Vec<u8>>>,
}

impl OmniSearchPlugin {
    /// Create a new `OmniSearchPlugin`.
    ///
    /// `actions` is an [`Arc`] over the application's action list. Cloning the
    /// `Arc` does not clone the `Vec` itself, so the plugin can read the shared
    /// action data without duplicating it.
    pub fn new(actions: Arc<Vec<Action>>) -> Self {
        let mut entries: Vec<(String, u64)> = Vec::new();
        let mut seen = HashSet::new();
        for (i, a) in actions.iter().enumerate() {
            let label_key = a.label.to_lowercase();
            if seen.insert(label_key.clone()) {
                entries.push((label_key.clone(), i as u64));
            } else {
                tracing::warn!(key = %label_key, "duplicate search key; skipping");
            }
            if !a.desc.is_empty() {
                let desc_key = a.desc.to_lowercase();
                if desc_key != label_key {
                    if seen.insert(desc_key.clone()) {
                        entries.push((desc_key.clone(), i as u64));
                    } else {
                        tracing::warn!(key = %desc_key, "duplicate search key; skipping");
                    }
                }
            }
        }
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        let mut builder = MapBuilder::memory();
        for (k, v) in entries {
            if let Err(err) = builder.insert(k.as_str(), v) {
                tracing::warn!(%k, value = v, ?err, "failed to insert key into search index");
            }
        }
        let index = match builder
            .into_inner()
            .map_err(anyhow::Error::from)
            .and_then(|bytes| Map::new(bytes).map_err(anyhow::Error::from))
        {
            Ok(index) => Some(index),
            Err(err) => {
                tracing::error!(
                    ?err,
                    "failed to build omni search index; falling back to linear scan"
                );
                None
            }
        };

        Self {
            folders: FoldersPlugin::default(),
            bookmarks: BookmarksPlugin::default(),
            note: NotePlugin::default(),
            todo: TodoPlugin::default(),
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
        "Combined search for folders, bookmarks, apps, notes and todos (prefix: `o`)"
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

    fn query_prefixes(&self) -> &[&str] {
        &["o"]
    }
}

impl OmniSearchPlugin {
    fn search_all(&self, rest: &str) -> Vec<Action> {
        let mut out = Vec::new();
        out.extend(self.collect_folder_results(rest));
        out.extend(self.collect_bookmark_results(rest));
        out.extend(self.collect_app_results(rest));
        out.extend(self.collect_note_results(rest));
        out.extend(self.collect_todo_results(rest));

        self.dedup_actions(out)
    }

    fn collect_folder_results(&self, rest: &str) -> Vec<Action> {
        self.folders.search(&format!("f {rest}"))
    }

    fn collect_bookmark_results(&self, rest: &str) -> Vec<Action> {
        if rest.trim().is_empty() {
            self.bookmarks.search("bm list")
        } else {
            self.bookmarks.search(&format!("bm {rest}"))
        }
    }

    fn collect_note_results(&self, rest: &str) -> Vec<Action> {
        if rest.trim().is_empty() {
            self.note.search("note list")
        } else {
            self.note.search(&format!("note {rest}"))
        }
    }

    fn collect_todo_results(&self, rest: &str) -> Vec<Action> {
        if rest.trim().is_empty() {
            self.todo.search("todo list")
        } else {
            self.todo.search(&format!("todo {rest}"))
        }
    }

    fn collect_app_results(&self, rest: &str) -> Vec<Action> {
        let mut out = Vec::new();
        let q = rest.trim();
        if q.is_empty() {
            out.extend(self.actions.iter().cloned());
        } else {
            let q_lc = q.to_lowercase();
            if let Some(index) = &self.index {
                let automaton = Subsequence::new(&q_lc);
                let mut stream = index.search(automaton).into_stream();
                let mut seen = HashSet::new();
                while let Some((_, idx)) = stream.next() {
                    if seen.insert(idx) {
                        if let Some(a) = self.actions.get(idx as usize) {
                            out.push(a.clone());
                        }
                    }
                }
            } else {
                for action in self.actions.iter() {
                    let label = action.label.to_lowercase();
                    let desc = action.desc.to_lowercase();
                    if label.contains(&q_lc) || desc.contains(&q_lc) {
                        out.push(action.clone());
                    }
                }
            }
        }

        out
    }

    fn dedup_actions(&self, actions: Vec<Action>) -> Vec<Action> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        for action in actions {
            let normalized_label = action
                .label
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase();
            let dedup_key = format!("{}\x1f{}", action.action, normalized_label);
            if seen.insert(dedup_key) {
                out.push(action);
            }
        }
        out
    }
}
