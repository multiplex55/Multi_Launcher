use crate::actions::Action;
use crate::plugin::Plugin;
use crate::plugins::bookmarks::BookmarksPlugin;
use crate::plugins::folders::FoldersPlugin;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

pub struct OmniSearchPlugin {
    folders: FoldersPlugin,
    bookmarks: BookmarksPlugin,
    actions: Vec<Action>,
    matcher: SkimMatcherV2,
}

impl OmniSearchPlugin {
    pub fn new(actions: Vec<Action>) -> Self {
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
        const PREFIX: &str = "o";
        let rest = match crate::common::strip_prefix_ci(query, PREFIX) {
            Some(r) => r.trim_start(),
            None => return Vec::new(),
        };
        let mut out = Vec::new();
        out.extend(self.folders.search(&format!("f {rest}")));
        out.extend(self.bookmarks.search(&format!("bm {rest}")));
        let q = rest.trim();
        if q.is_empty() {
            out.extend(self.actions.iter().cloned());
        } else {
            for a in &self.actions {
                if self.matcher.fuzzy_match(&a.label, q).is_some()
                    || self.matcher.fuzzy_match(&a.desc, q).is_some()
                {
                    out.push(a.clone());
                }
            }
        }
        out
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
        vec![Action {
            label: "o".into(),
            desc: "Omni".into(),
            action: "query:o ".into(),
            args: None,
        }]
    }
}
