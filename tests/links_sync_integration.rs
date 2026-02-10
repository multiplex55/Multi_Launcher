use multi_launcher::common::entity_ref::{EntityKind, EntityRef};
use multi_launcher::linking::{
    build_index_from_notes_and_todos, BacklinkFilters, EntityKey, LinkTarget,
};
use multi_launcher::note_todo_sync::{sync_note_todos, RevisionState, SyncConfig, SyncMode};
use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::link::LinkPlugin;
use multi_launcher::plugins::note::{save_notes, Note, NotePlugin};
use multi_launcher::plugins::todo::{save_todos, TodoEntry, TodoPlugin, TODO_FILE};
use once_cell::sync::Lazy;
use std::path::PathBuf;
use std::sync::Mutex;
use tempfile::tempdir;

static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

fn setup() -> tempfile::TempDir {
    let dir = tempdir().expect("tempdir");
    let notes_dir = dir.path().join("notes");
    std::fs::create_dir_all(&notes_dir).expect("create notes dir");
    std::env::set_var("ML_NOTES_DIR", &notes_dir);
    std::env::set_var("HOME", dir.path());
    std::env::set_current_dir(dir.path()).expect("set cwd");
    save_notes(&[]).expect("clear notes");
    save_todos(TODO_FILE, &[]).expect("clear todos");
    dir
}

#[test]
fn checkbox_sync_creates_todo_and_note_backlinks_surface_in_note_links() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let _tmp = setup();

    let note_body = "# Launch\n## Checklist\nSee @note:launch#checklist\n- [ ] ship parser p2 #ops @due 2026-03-01";
    let sync = sync_note_todos(
        note_body,
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

    assert_eq!(sync.todos.len(), 1);
    assert!(sync.note_content.contains("<!-- ml:todo:note-sync-3 -->"));

    let notes = vec![Note {
        title: "Launch".into(),
        path: PathBuf::from("launch.md"),
        content: sync.note_content,
        tags: vec![],
        links: vec![],
        slug: "launch".into(),
        alias: None,
        entity_refs: vec![EntityRef::new(EntityKind::Todo, "note-sync-3", None)],
    }];
    save_notes(&notes).expect("save notes");

    let todos = vec![TodoEntry {
        id: "note-sync-3".into(),
        text: "ship parser".into(),
        done: false,
        priority: 2,
        tags: vec!["ops".into()],
        entity_refs: vec![EntityRef::new(EntityKind::Note, "launch", None)],
    }];
    save_todos(TODO_FILE, &todos).expect("save todos");

    let index = build_index_from_notes_and_todos(&notes, &todos);
    let backlinks = index.get_backlinks(
        &EntityKey::new(LinkTarget::Note, "launch"),
        BacklinkFilters {
            linked_todos: true,
            related_notes: false,
            mentions: false,
        },
    );
    assert_eq!(
        backlinks,
        vec![EntityKey::new(LinkTarget::Todo, "note-sync-3")]
    );

    let note_actions = NotePlugin::default().search("note links launch");
    assert!(note_actions
        .iter()
        .any(|a| a.label.contains("status=mentioned_by") && a.label.contains("type=todo")));
}

#[test]
fn linking_commands_reflect_at_entity_links_and_canonical_link_resolution() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let _tmp = setup();

    let notes = vec![
        Note {
            title: "Plan".into(),
            path: PathBuf::from("plan.md"),
            content: "# Phase 1\nRef @todo:t-1 and [[Runbook]].".into(),
            tags: vec![],
            links: vec!["runbook".into()],
            slug: "plan".into(),
            alias: None,
            entity_refs: vec![EntityRef::new(EntityKind::Todo, "t-1", None)],
        },
        Note {
            title: "Runbook".into(),
            path: PathBuf::from("runbook.md"),
            content: "# Intro\n# Phase 1".into(),
            tags: vec![],
            links: vec![],
            slug: "runbook".into(),
            alias: None,
            entity_refs: vec![],
        },
    ];
    save_notes(&notes).expect("save notes");

    let todos = vec![TodoEntry {
        id: "t-1".into(),
        text: "Implement parser @note:plan".into(),
        done: false,
        priority: 3,
        tags: vec!["core".into()],
        entity_refs: vec![EntityRef::new(EntityKind::Note, "plan", None)],
    }];
    save_todos(TODO_FILE, &todos).expect("save todos");

    let note_links = NotePlugin::default().search("note links plan");
    assert!(
        note_links
            .iter()
            .any(|a| a.label.contains("type=todo") && a.action == "query:todo links id:t-1"),
        "expected note links to include todo backlink action for t-1, got: {:?}",
        note_links
            .iter()
            .map(|a| (&a.label, &a.action))
            .collect::<Vec<_>>()
    );

    let todo_links = TodoPlugin::default().search("todo links id:t-1");
    assert!(todo_links
        .iter()
        .any(|a| a.label.contains("type=note") && a.label.contains("id=plan")));

    let canonical = LinkPlugin {}.search("link link://note/runbook#phase-1");
    assert_eq!(canonical.len(), 1);
    assert_eq!(canonical[0].action, "link:open:link://note/runbook#phase-1");
}

#[test]
fn deleting_and_recreating_target_toggles_broken_link_without_crash() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let _tmp = setup();

    let note = Note {
        title: "Target".into(),
        path: PathBuf::from("target.md"),
        content: "# Anchor".into(),
        tags: vec![],
        links: vec![],
        slug: "target".into(),
        alias: None,
        entity_refs: vec![],
    };
    save_notes(&[note.clone()]).expect("save initial note");

    let ok = LinkPlugin {}.search("link link://note/target#anchor");
    assert!(ok[0].action.starts_with("link:open:"));

    save_notes(&[]).expect("delete target");
    let broken = LinkPlugin {}.search("link link://note/target#anchor");
    assert!(broken[0].label.contains("Invalid or broken link id"));

    save_notes(&[note]).expect("recreate target");
    let restored = LinkPlugin {}.search("link link://note/target#anchor");
    assert!(restored[0].action.starts_with("link:open:"));
}
