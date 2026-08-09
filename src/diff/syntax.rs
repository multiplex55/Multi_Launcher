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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderFragment {
    pub text: String,
    pub rgb: Option<[u8; 3]>,
    pub changed: bool,
}

/// Intersects cached syntax fragments with precomputed intraline byte ranges.
/// Split points are clamped to UTF-8 boundaries; no diff is performed here.
pub fn render_fragments(
    source: &str,
    syntax: &[HighlightFragment],
    changed: &[(usize, usize)],
) -> Vec<RenderFragment> {
    let mut colors = Vec::new();
    let mut offset = 0;
    for fragment in syntax {
        let end = (offset + fragment.text.len()).min(source.len());
        colors.push((offset, end, fragment.rgb));
        offset = end;
    }
    let mut points = vec![0, source.len()];
    for (a, b, _) in &colors {
        points.extend([*a, *b]);
    }
    for (a, b) in changed {
        points.extend([*a, *b]);
    }
    points.retain(|p| *p <= source.len() && source.is_char_boundary(*p));
    points.sort_unstable();
    points.dedup();
    points
        .windows(2)
        .filter_map(|p| {
            let (a, b) = (p[0], p[1]);
            (a < b).then(|| RenderFragment {
                text: source[a..b].into(),
                rgb: colors
                    .iter()
                    .find(|(s, e, _)| *s <= a && a < *e)
                    .map(|x| x.2),
                changed: changed.iter().any(|(s, e)| *s < b && a < *e),
            })
        })
        .collect()
}

#[cfg(test)]
mod render_tests {
    use super::*;
    #[test]
    fn unicode_range_mapping_preserves_boundaries() {
        let source = "aé👨‍👩‍👧z";
        let syntax = [HighlightFragment {
            text: source.into(),
            rgb: [1, 2, 3],
        }];
        let start = "aé".len();
        let end = source.len() - 1;
        let out = render_fragments(source, &syntax, &[(start, end)]);
        assert_eq!(
            out.iter().map(|f| f.text.as_str()).collect::<String>(),
            source
        );
        assert!(out.iter().any(|f| f.changed && f.text == "👨‍👩‍👧"));
    }
}
