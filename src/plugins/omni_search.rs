use crate::actions::Action;
use crate::plugin::Plugin;
use crate::plugins::bookmarks::BookmarksPlugin;
use crate::plugins::folders::FoldersPlugin;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use std::sync::Arc;

pub struct OmniSearchPlugin {
    folders: FoldersPlugin,
    bookmarks: BookmarksPlugin,
    actions: Arc<Vec<Action>>,
    matcher: SkimMatcherV2,
}

impl OmniSearchPlugin {
    pub fn new(actions: Arc<Vec<Action>>) -> Self {
        Self {
            folders: FoldersPlugin::default(),
            bookmarks: BookmarksPlugin::default(),
            actions,
            matcher: SkimMatcherV2::default(),
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
            for a in self.actions.iter() {
                if self.matcher.fuzzy_match(&a.label, q).is_some()
                    || self.matcher.fuzzy_match(&a.desc, q).is_some()
                {
                    out.push(a.clone());
                }
            }
        }
        out
    }
}
