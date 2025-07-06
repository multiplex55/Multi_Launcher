use crate::actions::Action;
use crate::plugin::Plugin;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

pub struct BookmarksPlugin {
    bookmarks: Vec<String>,
    matcher: SkimMatcherV2,
}

impl BookmarksPlugin {
    pub fn new(bookmarks: Vec<String>) -> Self {
        Self { bookmarks, matcher: SkimMatcherV2::default() }
    }
}

pub fn load_bookmarks(path: &str) -> anyhow::Result<Vec<String>> {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    if content.is_empty() {
        return Ok(Vec::new());
    }
    let list: Vec<String> = serde_json::from_str(&content)?;
    Ok(list)
}

pub fn save_bookmarks(path: &str, bookmarks: &[String]) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(bookmarks)?;
    std::fs::write(path, json)?;
    Ok(())
}

impl Default for BookmarksPlugin {
    fn default() -> Self {
        let bookmarks = load_bookmarks("bookmarks.json").unwrap_or_default();
        Self::new(bookmarks)
    }
}

impl Plugin for BookmarksPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if !query.starts_with("bm") {
            return Vec::new();
        }
        let filter = query.strip_prefix("bm").unwrap_or("").trim();
        self.bookmarks
            .iter()
            .filter(|url| {
                self.matcher
                    .fuzzy_match(url, filter)
                    .is_some()
            })
            .map(|url| Action {
                label: url.clone(),
                desc: "Bookmark".into(),
                action: url.clone(),
            })
            .collect()
    }

    fn name(&self) -> &str {
        "bookmarks"
    }

    fn description(&self) -> &str {
        "Return bookmarked URLs"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }
}

