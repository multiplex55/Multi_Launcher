use crate::actions::Action;
use crate::plugin::Plugin;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

const BOOKMARKS_FILE: &str = "bookmarks.json";

pub struct BookmarksPlugin {
    matcher: SkimMatcherV2,
}

impl BookmarksPlugin {
    pub fn new() -> Self {
        Self { matcher: SkimMatcherV2::default() }
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

pub fn append_bookmark(path: &str, url: &str) -> anyhow::Result<()> {
    let mut list = load_bookmarks(path).unwrap_or_default();
    if !list.contains(&url.to_string()) {
        list.push(url.to_string());
        save_bookmarks(path, &list)?;
    }
    Ok(())
}

impl Default for BookmarksPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for BookmarksPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if let Some(url) = query.strip_prefix("bm add ") {
            let url = url.trim();
            if !url.is_empty() {
                return vec![Action {
                    label: format!("Add bookmark {url}"),
                    desc: "Bookmark".into(),
                    action: format!("bookmark:add:{url}"),
                }];
            }
        }

        if !query.starts_with("bm") {
            return Vec::new();
        }
        let filter = query.strip_prefix("bm").unwrap_or("").trim();
        let bookmarks = load_bookmarks(BOOKMARKS_FILE).unwrap_or_default();
        bookmarks
            .into_iter()
            .filter(|url| self.matcher.fuzzy_match(url, filter).is_some())
            .map(|url| Action {
                label: url.clone(),
                desc: "Bookmark".into(),
                action: url,
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

