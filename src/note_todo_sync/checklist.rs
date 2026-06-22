use crate::note_todo_sync::metadata::parse_metadata;
use crate::notes_markdown::task_list::TASK_LIST_LINE_RE;
use chrono::NaiveDate;
use regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChecklistItem {
    pub line_index: usize,
    pub checked: bool,
    pub text: String,
    pub tags: Vec<String>,
    pub priority: Option<u8>,
    pub due: Option<NaiveDate>,
    pub todo_id: Option<String>,
}

pub fn checklist_re() -> Regex {
    Regex::new(TASK_LIST_LINE_RE).expect("valid checklist regex")
}

pub fn parse_checklist_items(note_content: &str) -> Vec<ChecklistItem> {
    let re = checklist_re();
    note_content
        .lines()
        .enumerate()
        .filter_map(|(line_index, line)| {
            let cap = re.captures(line)?;
            let checked = cap.get(2).map(|m| m.as_str().eq_ignore_ascii_case("x"))?;
            let body = cap.get(3).map(|m| m.as_str()).unwrap_or_default();
            let (text, mut tags, priority, due) = parse_metadata(body);
            tags.sort();
            let todo_id = cap.get(5).map(|m| m.as_str().to_string());
            Some(ChecklistItem {
                line_index,
                checked,
                text,
                tags,
                priority,
                due,
                todo_id,
            })
        })
        .collect()
}

pub fn checkbox_sync_enabled(note_content: &str) -> bool {
    note_content
        .lines()
        .any(|l| l.trim() == "<!-- ml:checkbox_sync:on -->")
}

pub fn set_checkbox_sync_enabled(note_content: &str, enabled: bool) -> String {
    let marker = "<!-- ml:checkbox_sync:on -->";
    let mut lines = note_content
        .lines()
        .map(|l| l.to_string())
        .collect::<Vec<_>>();
    let marker_pos = lines.iter().position(|l| l.trim() == marker);
    match (enabled, marker_pos) {
        (true, None) => lines.insert(0, marker.to_string()),
        (false, Some(idx)) => {
            lines.remove(idx);
        }
        _ => {}
    }
    lines.join("\n")
}

pub fn upsert_mapping_token(line: &str, todo_id: &str) -> String {
    let re = checklist_re();
    if let Some(cap) = re.captures(line) {
        let prefix = cap.get(1).map(|m| m.as_str()).unwrap_or_default();
        let body = cap
            .get(3)
            .map(|m| m.as_str())
            .unwrap_or_default()
            .trim_end();
        return format!("{prefix}{body} <!-- ml:todo:{todo_id} -->");
    }
    line.to_string()
}

pub fn render_checklist_line(
    template: &str,
    checked: bool,
    text: &str,
    tags: &[String],
    priority: Option<u8>,
    due: Option<NaiveDate>,
    todo_id: &str,
) -> String {
    let re = checklist_re();
    let prefix = re
        .captures(template)
        .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
        .unwrap_or_else(|| "- [ ] ".to_string());
    let marker = if checked { "x" } else { " " };
    let mut payload = text.trim().to_string();
    if let Some(p) = priority {
        payload.push_str(&format!(" p{p}"));
    }
    if let Some(d) = due {
        payload.push_str(&format!(" @due {}", d.format("%Y-%m-%d")));
    }
    for t in tags {
        payload.push_str(&format!(" #{t}"));
    }
    let prefix = format!("{}[{}] ", prefix.split('[').next().unwrap_or("- "), marker);
    format!("{}{} <!-- ml:todo:{} -->", prefix, payload.trim(), todo_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn parser_extracts_checked_unchecked_and_metadata() {
        let note = "- [ ] Ship parser #work #rust p2 @due 2026-03-01\n- [x] Done item p6 #ignored";
        let items = parse_checklist_items(note);
        assert_eq!(items.len(), 2);
        assert!(!items[0].checked);
        assert_eq!(items[0].text, "Ship parser");
        assert_eq!(items[0].priority, Some(2));
        assert_eq!(items[0].due, NaiveDate::from_ymd_opt(2026, 3, 1));
        assert_eq!(items[0].tags, vec!["rust", "work"]);
        assert!(items[1].checked);
        assert_eq!(items[1].priority, None);
    }

    #[test]
    fn renderer_preserves_checkbox_markers_and_metadata() {
        let line = render_checklist_line(
            "  * [ ] old",
            true,
            "new text",
            &["ops".to_string()],
            Some(3),
            NaiveDate::from_ymd_opt(2026, 1, 1),
            "t-1",
        );
        assert_eq!(
            line,
            "  * [x] new text p3 @due 2026-01-01 #ops <!-- ml:todo:t-1 -->"
        );
    }
}
