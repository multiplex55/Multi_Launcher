use crate::note_todo_sync::checklist::{
    parse_checklist_items, render_checklist_line, upsert_mapping_token,
};
use crate::note_todo_sync::metadata::normalize_text;
use chrono::NaiveDate;
use std::collections::HashMap;

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
                if let Some(id) = mapped_id
                    && let Some(todo) = todo_map.get(&id)
                {
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
                .unwrap()
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
        assert!(
            result
                .note_content
                .contains("- [x] new text p3 @due 2026-01-01 #ops <!-- ml:todo:t-1 -->")
        );
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
        assert_eq!(
            result.todos.iter().find(|t| t.id == "t-2").unwrap().text,
            "renamed task"
        );
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
        let lines: Vec<&str> = result.note_content.lines().collect();
        assert_eq!(lines[0], "# Heading");
        assert_eq!(lines[1], "plain paragraph");
    }
}
