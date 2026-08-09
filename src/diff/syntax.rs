//! Syntax selection and revision-aware line highlighting.
//!
//! Paint precedence in the text view is: semantic diff background, current
//! hunk overlay, syntax foreground, intraline emphasis, then selection/caret.
//! Backgrounds are owned by the diff renderer so a syntax theme can never
//! erase insert/delete/modify meaning.
use std::{collections::HashMap, path::Path};
use syntect::{easy::HighlightLines, highlighting::ThemeSet, parsing::SyntaxSet};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HighlightKey {
    pub revision: u64,
    pub language: String,
    pub theme: String,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightFragment {
    pub text: String,
    pub rgb: [u8; 3],
}

#[derive(Default)]
pub struct SyntaxCache {
    lines: HashMap<HighlightKey, Vec<HighlightFragment>>,
}

pub fn language_for_path(path: Option<&Path>) -> String {
    let Some(ext) = path.and_then(Path::extension).and_then(|x| x.to_str()) else {
        return "Plain Text".into();
    };
    let ps = SyntaxSet::load_defaults_newlines();
    ps.find_syntax_by_extension(ext)
        .map_or("Plain Text", |s| s.name.as_str())
        .to_owned()
}

pub fn code_like(path: Option<&Path>) -> bool {
    matches!(
        path.and_then(Path::extension)
            .and_then(|x| x.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str(),
        "rs" | "c"
            | "h"
            | "cpp"
            | "cs"
            | "go"
            | "java"
            | "js"
            | "ts"
            | "py"
            | "rb"
            | "sh"
            | "toml"
            | "yaml"
            | "yml"
            | "json"
            | "xml"
            | "html"
            | "css"
    )
}

impl SyntaxCache {
    pub fn line(&mut self, key: HighlightKey, source: &str) -> &[HighlightFragment] {
        self.lines.entry(key.clone()).or_insert_with(|| {
            let ps = SyntaxSet::load_defaults_newlines();
            let ts = ThemeSet::load_defaults();
            let syntax = ps
                .find_syntax_by_name(&key.language)
                .unwrap_or_else(|| ps.find_syntax_plain_text());
            let theme = ts
                .themes
                .get(&key.theme)
                .or_else(|| ts.themes.values().next())
                .unwrap();
            let mut h = HighlightLines::new(syntax, theme);
            h.highlight_line(source, &ps)
                .unwrap_or_default()
                .into_iter()
                .map(|(s, t)| HighlightFragment {
                    text: t.into(),
                    rgb: [s.foreground.r, s.foreground.g, s.foreground.b],
                })
                .collect()
        })
    }
    pub fn retain_revision(&mut self, revision: u64) {
        self.lines.retain(|k, _| k.revision == revision);
    }
}
