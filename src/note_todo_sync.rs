use chrono::NaiveDate;
use regex::Regex;
use std::collections::{HashMap, HashSet};

pub const PRIORITY_MIN: u8 = 1;
pub const PRIORITY_MAX: u8 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncMode {
    NoteSourceOfTruth,
    TodoSourceOfTruth,
    OneWayImportFromNote,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncConfig {
    pub enabled: bool,
    pub mode: SyncMode,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: SyncMode::OneWayImportFromNote,
        }
    }
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TodoItem {
    pub id: String,
    pub text: String,
    pub done: bool,
    pub tags: Vec<String>,
    pub priority: Option<u8>,
    pub due: Option<NaiveDate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevisionState {
    pub note_rev: u64,
    pub todo_rev: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewAction {
    CreateTodo { line_index: usize, text: String },
    UpdateTodo { todo_id: String, line_index: usize },
    UpdateNote { todo_id: String, line_index: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncConflict {
    pub todo_id: Option<String>,
    pub line_index: usize,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncResult {
    pub note_content: String,
    pub todos: Vec<TodoItem>,
    pub preview: Vec<PreviewAction>,
    pub conflicts: Vec<SyncConflict>,
    pub revision: RevisionState,
}

fn checklist_re() -> Regex {
    Regex::new(r"^(\s*[-*]\s+\[( |x|X)\]\s*)(.*?)(\s*<!--\s*ml:todo:([A-Za-z0-9:_-]+)\s*-->\s*)?$")
        .expect("valid checklist regex")
}

fn tag_re() -> Regex {
    Regex::new(r"(?P<tag>#[A-Za-z][A-Za-z0-9_-]*)").expect("valid tag regex")
}

fn priority_re() -> Regex {
    Regex::new(r"\bp(?P<n>[0-9]+)\b").expect("valid priority regex")
}

fn due_re() -> Regex {
    Regex::new(r"@due\s+(?P<date>\d{4}-\d{2}-\d{2})").expect("valid due regex")
}

fn normalize_text(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn parse_metadata(body: &str) -> (String, Vec<String>, Option<u8>, Option<NaiveDate>) {
    let tags = tag_re()
        .captures_iter(body)
        .filter_map(|c| c.name("tag"))
        .map(|m| m.as_str().trim_start_matches('#').to_lowercase())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    let priority = priority_re()
        .captures_iter(body)
        .filter_map(|c| c.name("n"))
        .filter_map(|m| m.as_str().parse::<u8>().ok())
        .find(|p| (*p >= PRIORITY_MIN) && (*p <= PRIORITY_MAX));

    let due = due_re()
        .captures_iter(body)
        .filter_map(|c| c.name("date"))
        .find_map(|m| NaiveDate::parse_from_str(m.as_str(), "%Y-%m-%d").ok());

    let stripped = due_re().replace_all(body, "");
    let stripped = priority_re().replace_all(&stripped, "");
    let stripped = tag_re().replace_all(&stripped, "");
    (stripped.trim().to_string(), tags, priority, due)
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

fn upsert_mapping_token(line: &str, todo_id: &str) -> String {
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

fn render_checklist_line(
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

pub fn sync_note_todos(
    note_content: &str,
    todos: &[TodoItem],
    config: SyncConfig,
    last_revision: Option<RevisionState>,
    current_revision: RevisionState,
) -> SyncResult {
    if !config.enabled {
        return SyncResult {
            note_content: note_content.to_string(),
            todos: todos.to_vec(),
            preview: Vec::new(),
            conflicts: Vec::new(),
            revision: current_revision,
        };
    }

    let note_items = parse_checklist_items(note_content);
    let mut todo_map = todos
        .iter()
        .cloned()
        .map(|t| (t.id.clone(), t))
        .collect::<HashMap<_, _>>();
    let mut preview = Vec::new();
    let mut conflicts = Vec::new();

    let note_changed = last_revision
        .as_ref()
        .map(|r| r.note_rev != current_revision.note_rev)
        .unwrap_or(true);
    let todo_changed = last_revision
        .as_ref()
        .map(|r| r.todo_rev != current_revision.todo_rev)
        .unwrap_or(true);

    let conflict_mode = note_changed && todo_changed;

    let mut lines = note_content
        .lines()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    let mut normalized_to_todo = HashMap::<String, String>::new();
    for t in todos {
        normalized_to_todo.insert(normalize_text(&t.text), t.id.clone());
    }

    for item in &note_items {
        let mapped_id = item
            .todo_id
            .clone()
            .or_else(|| normalized_to_todo.get(&normalize_text(&item.text)).cloned());

        if conflict_mode {
            conflicts.push(SyncConflict {
                todo_id: mapped_id.clone(),
                line_index: item.line_index,
                reason: "note and todo changed concurrently".into(),
            });
        }

        match config.mode {
            SyncMode::NoteSourceOfTruth | SyncMode::OneWayImportFromNote => match mapped_id {
                Some(id) => {
                    preview.push(PreviewAction::UpdateTodo {
                        todo_id: id.clone(),
                        line_index: item.line_index,
                    });
                    todo_map.insert(
                        id.clone(),
                        TodoItem {
                            id,
                            text: item.text.clone(),
                            done: item.checked,
                            tags: item.tags.clone(),
                            priority: item.priority,
                            due: item.due,
                        },
                    );
                }
                None => {
                    let id = format!("note-sync-{}", item.line_index);
                    preview.push(PreviewAction::CreateTodo {
                        line_index: item.line_index,
                        text: item.text.clone(),
                    });
                    todo_map.insert(
                        id.clone(),
                        TodoItem {
                            id: id.clone(),
                            text: item.text.clone(),
                            done: item.checked,
                            tags: item.tags.clone(),
                            priority: item.priority,
                            due: item.due,
                        },
                    );
                    if let Some(line) = lines.get_mut(item.line_index) {
                        *line = upsert_mapping_token(line, &id);
                    }
                }
            },
            SyncMode::TodoSourceOfTruth => {
                if let Some(id) = mapped_id {
                    if let Some(todo) = todo_map.get(&id) {
                        preview.push(PreviewAction::UpdateNote {
                            todo_id: id.clone(),
                            line_index: item.line_index,
                        });
                        if let Some(line) = lines.get_mut(item.line_index) {
                            *line = render_checklist_line(
                                line,
                                todo.done,
                                &todo.text,
                                &todo.tags,
                                todo.priority,
                                todo.due,
                                &id,
                            );
                        }
                    }
                }
            }
        }
    }

    let mut updated_todos = todo_map.into_values().collect::<Vec<_>>();
    updated_todos.sort_by(|a, b| a.id.cmp(&b.id));

    SyncResult {
        note_content: lines.join("\n"),
        todos: updated_todos,
        preview,
        conflicts,
        revision: current_revision,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::entity_ref::{EntityKind, EntityRef};
    use crate::linking::{
        build_index_from_notes_and_todos, BacklinkFilters, EntityKey, LinkTarget,
    };
    use crate::plugins::note::Note;
    use crate::plugins::todo::TodoEntry;
    use std::path::PathBuf;

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
        assert_eq!(
            items[1].priority, None,
            "priority outside allowed range is ignored"
        );
    }

    #[test]
    fn note_source_of_truth_updates_todos() {
        let note = "- [x] Finish release p1 #work <!-- ml:todo:t-1 -->";
        let todos = vec![TodoItem {
            id: "t-1".into(),
            text: "old".into(),
            done: false,
            tags: vec![],
            priority: None,
            due: None,
        }];
        let result = sync_note_todos(
            note,
            &todos,
            SyncConfig {
                enabled: true,
                mode: SyncMode::NoteSourceOfTruth,
            },
            None,
            RevisionState {
                note_rev: 2,
                todo_rev: 1,
            },
        );
        assert_eq!(
            result
                .todos
                .into_iter()
                .find(|t| t.id == "t-1")
                .expect("todo")
                .text,
            "Finish release"
        );
    }

    #[test]
    fn todo_source_of_truth_updates_note_directionally() {
        let note = "- [ ] old text #work <!-- ml:todo:t-1 -->";
        let todos = vec![TodoItem {
            id: "t-1".into(),
            text: "new text".into(),
            done: true,
            tags: vec!["ops".into()],
            priority: Some(3),
            due: NaiveDate::from_ymd_opt(2026, 1, 1),
        }];
        let result = sync_note_todos(
            note,
            &todos,
            SyncConfig {
                enabled: true,
                mode: SyncMode::TodoSourceOfTruth,
            },
            None,
            RevisionState {
                note_rev: 4,
                todo_rev: 5,
            },
        );
        assert!(result
            .note_content
            .contains("- [x] new text p3 @due 2026-01-01 #ops <!-- ml:todo:t-1 -->"));
    }

    #[test]
    fn mapping_is_robust_under_reorder_and_rename() {
        let note = "- [ ] renamed task <!-- ml:todo:t-2 -->\n- [ ] another <!-- ml:todo:t-1 -->";
        let todos = vec![
            TodoItem {
                id: "t-1".into(),
                text: "first".into(),
                done: false,
                tags: vec![],
                priority: None,
                due: None,
            },
            TodoItem {
                id: "t-2".into(),
                text: "second".into(),
                done: false,
                tags: vec![],
                priority: None,
                due: None,
            },
        ];
        let result = sync_note_todos(
            note,
            &todos,
            SyncConfig {
                enabled: true,
                mode: SyncMode::NoteSourceOfTruth,
            },
            None,
            RevisionState {
                note_rev: 1,
                todo_rev: 1,
            },
        );
        let t2 = result.todos.iter().find(|t| t.id == "t-2").expect("t2");
        assert_eq!(t2.text, "renamed task");
    }

    #[test]
    fn conflicts_are_recorded_on_concurrent_edits() {
        let note = "- [ ] task <!-- ml:todo:t-1 -->";
        let todos = vec![TodoItem {
            id: "t-1".into(),
            text: "task".into(),
            done: false,
            tags: vec![],
            priority: None,
            due: None,
        }];
        let result = sync_note_todos(
            note,
            &todos,
            SyncConfig {
                enabled: true,
                mode: SyncMode::NoteSourceOfTruth,
            },
            Some(RevisionState {
                note_rev: 1,
                todo_rev: 1,
            }),
            RevisionState {
                note_rev: 2,
                todo_rev: 2,
            },
        );
        assert_eq!(result.conflicts.len(), 1);
    }

    #[test]
    fn non_checkbox_markdown_is_untouched() {
        let note = "# Heading\nplain paragraph\n- [ ] sync me";
        let todos = vec![];
        let result = sync_note_todos(
            note,
            &todos,
            SyncConfig {
                enabled: true,
                mode: SyncMode::OneWayImportFromNote,
            },
            None,
            RevisionState {
                note_rev: 1,
                todo_rev: 0,
            },
        );
        let lines: Vec<&str> = result.note_content.lines().collect();
        assert_eq!(lines[0], "# Heading");
        assert_eq!(lines[1], "plain paragraph");
    }

    #[test]
    fn fixture_parser_keeps_heading_mentions_links_and_checkbox_metadata() {
        let note = r#"# Plan
## Phase 1
See @note:architecture and [spec](https://example.com/spec).
- [ ] Ship parser p2 #core @due 2026-03-01 <!-- ml:todo:t-10 -->
- [x] Publish notes #docs
"#;
        let items = parse_checklist_items(note);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].todo_id.as_deref(), Some("t-10"));
        assert_eq!(items[0].priority, Some(2));
        assert_eq!(items[0].tags, vec!["core"]);
        assert_eq!(items[1].text, "Publish notes");
    }

    #[test]
    fn integration_checkbox_sync_creates_todo_and_backlink_index_updates() {
        let note = "# Launch\n- [ ] Ship launch checklist #ops @due 2026-04-02";
        let result = sync_note_todos(
            note,
            &[],
            SyncConfig {
                enabled: true,
                mode: SyncMode::OneWayImportFromNote,
            },
            None,
            RevisionState {
                note_rev: 1,
                todo_rev: 0,
            },
        );

        assert_eq!(result.todos.len(), 1);
        assert!(result.note_content.contains("<!-- ml:todo:note-sync-1 -->"));

        let notes = vec![Note {
            title: "Launch".into(),
            path: PathBuf::from("launch.md"),
            content: result.note_content.clone(),
            tags: vec![],
            links: vec![],
            slug: "launch".into(),
            alias: None,
            entity_refs: vec![EntityRef::new(EntityKind::Todo, "note-sync-1", None)],
        }];
        let todos = vec![TodoEntry {
            id: "note-sync-1".into(),
            text: "Ship launch checklist".into(),
            done: false,
            priority: 2,
            tags: vec!["ops".into()],
            entity_refs: vec![EntityRef::new(EntityKind::Note, "launch", None)],
        }];
        let index = build_index_from_notes_and_todos(&notes, &todos);

        let note_backlinks = index.get_backlinks(
            &EntityKey::new(LinkTarget::Note, "launch"),
            BacklinkFilters {
                linked_todos: true,
                related_notes: false,
                mentions: false,
            },
        );
        assert_eq!(note_backlinks.len(), 1);
        assert_eq!(
            note_backlinks[0],
            EntityKey::new(LinkTarget::Todo, "note-sync-1")
        );
    }

    #[test]
    fn sync_mode_transition_preserves_data_without_loss() {
        let initial_note = "- [ ] stabilize API p2 #platform <!-- ml:todo:t-1 -->";
        let initial_todos = vec![TodoItem {
            id: "t-1".into(),
            text: "stabilize API".into(),
            done: false,
            tags: vec!["platform".into()],
            priority: Some(2),
            due: None,
        }];

        let note_authoritative = sync_note_todos(
            initial_note,
            &initial_todos,
            SyncConfig {
                enabled: true,
                mode: SyncMode::NoteSourceOfTruth,
            },
            Some(RevisionState {
                note_rev: 1,
                todo_rev: 1,
            }),
            RevisionState {
                note_rev: 2,
                todo_rev: 1,
            },
        );

        let switched = sync_note_todos(
            &note_authoritative.note_content,
            &note_authoritative.todos,
            SyncConfig {
                enabled: true,
                mode: SyncMode::TodoSourceOfTruth,
            },
            Some(RevisionState {
                note_rev: 2,
                todo_rev: 1,
            }),
            RevisionState {
                note_rev: 2,
                todo_rev: 2,
            },
        );

        assert_eq!(switched.todos.len(), 1);
        assert!(switched.note_content.contains("stabilize API"));
        assert!(switched.note_content.contains("<!-- ml:todo:t-1 -->"));
    }
}
