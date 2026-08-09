//! UI-independent exports for text comparisons. Binary bytes are never stored
//! in or emitted by this module.
use similar::{ChangeTag, TextDiff};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextExportFormat {
    UnifiedDiff,
    PlainTextSummary,
    HtmlSideBySide,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportContent {
    Text(String),
    Binary { size: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextExportSnapshot {
    pub left_name: String,
    pub right_name: String,
    pub left: ExportContent,
    pub right: ExportContent,
}

impl TextExportSnapshot {
    pub fn text(
        left_name: impl Into<String>,
        right_name: impl Into<String>,
        left: &str,
        right: &str,
    ) -> Self {
        Self {
            left_name: left_name.into(),
            right_name: right_name.into(),
            left: ExportContent::Text(left.into()),
            right: ExportContent::Text(right.into()),
        }
    }
    pub fn render(&self, format: TextExportFormat) -> String {
        match format {
            TextExportFormat::UnifiedDiff => self.unified(),
            TextExportFormat::PlainTextSummary => self.summary(),
            TextExportFormat::HtmlSideBySide => self.html(),
        }
    }
    fn texts(&self) -> Option<(&str, &str)> {
        match (&self.left, &self.right) {
            (ExportContent::Text(l), ExportContent::Text(r)) => Some((l, r)),
            _ => None,
        }
    }
    fn unified(&self) -> String {
        let Some((left, right)) = self.texts() else {
            return self.summary();
        };
        if left == right {
            return String::new();
        }
        TextDiff::from_lines(left, right)
            .unified_diff()
            .header(
                &safe_header(&self.left_name),
                &safe_header(&self.right_name),
            )
            .to_string()
            .replace("\r\n", "\n")
    }
    fn summary(&self) -> String {
        let mut out = format!(
            "Left: {} ({})\nRight: {} ({})\n",
            text_escape(&self.left_name),
            description(&self.left),
            text_escape(&self.right_name),
            description(&self.right)
        );
        if let Some((l, r)) = self.texts() {
            let mut add = 0;
            let mut del = 0;
            for c in TextDiff::from_lines(l, r).iter_all_changes() {
                match c.tag() {
                    ChangeTag::Insert => add += 1,
                    ChangeTag::Delete => del += 1,
                    ChangeTag::Equal => {}
                }
            }
            out.push_str(&format!(
                "Added lines: {add}\nDeleted lines: {del}\nDifferent: {}\n",
                l != r
            ));
        } else {
            out.push_str("Binary content omitted.\n");
        }
        out
    }
    fn html(&self) -> String {
        let mut out = format!(
            "<!doctype html>\n<meta charset=\"utf-8\">\n<title>Text comparison</title>\n<h1>{} ↔ {}</h1>\n<table><thead><tr><th>Left</th><th>Right</th></tr></thead><tbody>\n",
            html(&self.left_name),
            html(&self.right_name)
        );
        if let Some((l, r)) = self.texts() {
            let diff = TextDiff::from_lines(l, r);
            for op in diff.ops() {
                for change in diff.iter_changes(op) {
                    let value = html(change.value().trim_end_matches(['\r', '\n']));
                    match change.tag() {
                        ChangeTag::Equal => {
                            out.push_str(&format!("<tr><td>{0}</td><td>{0}</td></tr>\n", value))
                        }
                        ChangeTag::Delete => out.push_str(&format!(
                            "<tr class=\"deleted\"><td>{}</td><td></td></tr>\n",
                            value
                        )),
                        ChangeTag::Insert => out.push_str(&format!(
                            "<tr class=\"inserted\"><td></td><td>{}</td></tr>\n",
                            value
                        )),
                    }
                }
            }
        } else {
            out.push_str(&format!(
                "<tr><td>{}</td><td>{}</td></tr>\n",
                html(&description(&self.left)),
                html(&description(&self.right))
            ));
        }
        out.push_str("</tbody></table>\n");
        out
    }
}
fn description(c: &ExportContent) -> String {
    match c {
        ExportContent::Text(s) => format!("text, {} bytes", s.len()),
        ExportContent::Binary { size } => format!("binary, {size} bytes; content omitted"),
    }
}
fn safe_header(v: &str) -> String {
    v.replace(['\r', '\n', '\t'], " ")
}
fn text_escape(v: &str) -> String {
    v.replace('\\', "\\\\")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
}
fn html(v: &str) -> String {
    let mut out = String::new();
    for c in v.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unified_add_delete_modify_empty_and_equal() {
        let changed = TextExportSnapshot::text("左/a", "右/b", "old\ndelete\n", "new\n追加\n")
            .render(TextExportFormat::UnifiedDiff);
        assert!(changed.contains("-old"));
        assert!(changed.contains("-delete"));
        assert!(changed.contains("+new"));
        assert!(changed.contains("+追加"));
        assert!(
            !TextExportSnapshot::text("a", "b", "same\n", "same\n")
                .render(TextExportFormat::UnifiedDiff)
                .contains("@@")
        );
        assert!(
            TextExportSnapshot::text("a", "b", "", "one\n")
                .render(TextExportFormat::UnifiedDiff)
                .contains("+one")
        );
        assert!(
            TextExportSnapshot::text("a", "b", "one\n", "")
                .render(TextExportFormat::UnifiedDiff)
                .contains("-one")
        );
    }
    #[test]
    fn html_escapes_paths_and_source() {
        let value = TextExportSnapshot::text("<&", "\"right", "雪 <&>", "雪")
            .render(TextExportFormat::HtmlSideBySide);
        assert!(value.contains("&lt;&amp;"));
        assert!(value.contains("&quot;right"));
        assert!(!value.contains("雪 <&>"));
    }
    #[test]
    fn binary_is_summary_only() {
        let snapshot = TextExportSnapshot {
            left_name: "a".into(),
            right_name: "b".into(),
            left: ExportContent::Binary { size: 42 },
            right: ExportContent::Binary { size: 7 },
        };
        assert!(
            snapshot
                .render(TextExportFormat::UnifiedDiff)
                .contains("Binary content omitted")
        );
    }
}
