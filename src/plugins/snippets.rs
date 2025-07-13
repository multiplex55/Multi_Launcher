use crate::actions::Action;
use crate::plugin::Plugin;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use serde::{Deserialize, Serialize};

pub const SNIPPETS_FILE: &str = "snippets.json";

#[derive(Serialize, Deserialize, Clone)]
pub struct SnippetEntry {
    pub alias: String,
    pub text: String,
}

/// Load all snippets from the JSON file at `path`.
pub fn load_snippets(path: &str) -> anyhow::Result<Vec<SnippetEntry>> {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }
    let list: Vec<SnippetEntry> = serde_json::from_str(&content)?;
    Ok(list)
}

/// Persist `snippets` to `path`.
pub fn save_snippets(path: &str, snippets: &[SnippetEntry]) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(snippets)?;
    std::fs::write(path, json)?;
    Ok(())
}


/// Remove the snippet identified by `alias`.
pub fn remove_snippet(path: &str, alias: &str) -> anyhow::Result<()> {
    let mut list = load_snippets(path).unwrap_or_default();
    if let Some(pos) = list.iter().position(|e| e.alias == alias) {
        list.remove(pos);
        save_snippets(path, &list)?;
    }
    Ok(())
}

pub struct SnippetsPlugin {
    matcher: SkimMatcherV2,
}

impl SnippetsPlugin {
    /// Create a new snippets plugin instance.
    pub fn new() -> Self {
        Self { matcher: SkimMatcherV2::default() }
    }
}

impl Default for SnippetsPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for SnippetsPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        if trimmed.eq_ignore_ascii_case("cs") {
            return vec![Action {
                label: "cs: edit snippets".into(),
                desc: "Snippet".into(),
                action: "snippet:dialog".into(),
                args: None,
            }];
        }
        if let Some(pattern) = trimmed.strip_prefix("cs rm ") {
            let filter = pattern.trim();
            let list = load_snippets(SNIPPETS_FILE).unwrap_or_default();
            return list
                .into_iter()
                .filter(|s| {
                    self.matcher.fuzzy_match(&s.alias, filter).is_some()
                        || self.matcher.fuzzy_match(&s.text, filter).is_some()
                })
                .map(|s| Action {
                    label: format!("Remove snippet {}", s.alias),
                    desc: "Snippet".into(),
                    action: format!("snippet:remove:{}", s.alias),
                    args: None,
                })
                .collect();
        }

        if let Some(rest) = trimmed.strip_prefix("cs list") {
            let filter = rest.trim();
            let list = load_snippets(SNIPPETS_FILE).unwrap_or_default();
            return list
                .into_iter()
                .filter(|s| {
                    self.matcher.fuzzy_match(&s.alias, filter).is_some()
                        || self.matcher.fuzzy_match(&s.text, filter).is_some()
                })
                .map(|s| Action {
                    label: s.alias,
                    desc: "Snippet".into(),
                    action: format!("clipboard:{}", s.text),
                    args: None,
                })
                .collect();
        }

        if !trimmed.starts_with("cs") {
            return Vec::new();
        }

        let filter = trimmed.strip_prefix("cs").unwrap_or("").trim();
        let list = load_snippets(SNIPPETS_FILE).unwrap_or_default();
        list.into_iter()
            .filter(|s| {
                self.matcher.fuzzy_match(&s.alias, filter).is_some()
                    || self.matcher.fuzzy_match(&s.text, filter).is_some()
            })
            .map(|s| Action {
                label: s.alias,
                desc: "Snippet".into(),
                action: format!("clipboard:{}", s.text),
                args: None,
            })
            .collect()
    }

    fn name(&self) -> &str {
        "snippets"
    }

    fn description(&self) -> &str {
        "Search saved text snippets (prefix: `cs`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }
}

