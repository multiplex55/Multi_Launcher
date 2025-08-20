use chrono::Local;
use eframe::egui;
use multi_launcher::gui::{extract_links, show_wiki_link, LauncherApp};
use multi_launcher::plugin::Plugin;
use multi_launcher::plugin::PluginManager;
use multi_launcher::plugins::note::{append_note, load_notes, save_notes, NotePlugin};
use multi_launcher::settings::Settings;
use once_cell::sync::Lazy;
use std::sync::Mutex;
use std::sync::{atomic::AtomicBool, Arc};
use tempfile::tempdir;

static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

fn setup() -> tempfile::TempDir {
    let dir = tempdir().unwrap();
    let notes_dir = dir.path().join("notes");
    std::fs::create_dir_all(&notes_dir).unwrap();
    std::env::set_var("ML_NOTES_DIR", &notes_dir);
    std::env::set_var("HOME", dir.path());
    save_notes(&[]).unwrap();
    dir
}

fn new_app(ctx: &egui::Context) -> LauncherApp {
    LauncherApp::new(
        ctx,
        Arc::new(Vec::new()),
        0,
        PluginManager::new(),
        "actions.json".into(),
        "settings.json".into(),
        Settings::default(),
        None,
        None,
        None,
        None,
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
    )
}

#[test]
fn note_root_query_returns_actions_in_order() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let _tmp = setup();
    let plugin = NotePlugin::default();
    let results = plugin.search("note");
    let actions: Vec<&str> = results.iter().map(|a| a.action.as_str()).collect();
    assert_eq!(
        actions,
        vec![
            "note:dialog",
            "query:note search ",
            "query:note list",
            "query:note tags",
            "query:note templates",
            "query:note new ",
            "query:note add ",
            "query:note open ",
            "query:note today",
            "query:note link ",
            "query:note rm ",
            "note:reload",
            "note:unused_assets",
        ]
    );
}

#[test]
fn note_new_generates_action() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let _tmp = setup();
    let plugin = NotePlugin::default();
    let results = plugin.search("note new Hello World");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "note:new:hello-world");
}

#[test]
fn note_create_generates_action() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let _tmp = setup();
    let plugin = NotePlugin::default();
    let results = plugin.search("note create Hello World");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "note:new:hello-world");
}

#[test]
fn note_reload_action_generated() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let _tmp = setup();
    let plugin = NotePlugin::default();
    let results = plugin.search("note reload");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "note:reload");
}

#[test]
fn note_open_returns_matching_note() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let _tmp = setup();
    append_note("alpha", "alpha content").unwrap();
    let plugin = NotePlugin::default();
    let results = plugin.search("note open alpha");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "note:open:alpha");
}

#[test]
fn note_list_handles_slug_collisions() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let _tmp = setup();
    append_note("alpha", "one").unwrap();
    append_note("alpha", "two").unwrap();
    let plugin = NotePlugin::default();
    let results = plugin.search("note list");
    assert_eq!(results.len(), 2);
    let actions: Vec<String> = results.into_iter().map(|a| a.action).collect();
    assert!(actions.contains(&"note:open:alpha".to_string()));
    assert!(actions.contains(&"note:open:alpha-1".to_string()));
}

#[test]
fn note_search_finds_content() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let _tmp = setup();
    append_note("alpha", "lorem ipsum").unwrap();
    append_note("beta", "unique needle").unwrap();
    let plugin = NotePlugin::default();
    let results = plugin.search("note search needle");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "note:open:beta");
}

#[test]
fn note_tags_parses_edge_cases() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let _tmp = setup();
    append_note("alpha", "#Foo #foo #bar-baz #baz_1 #dup #dup").unwrap();
    let plugin = NotePlugin::default();
    let results = plugin.search("note tags");
    assert_eq!(results.len(), 4);
    let labels: Vec<String> = results.iter().map(|a| a.label.clone()).collect();
    assert_eq!(labels.iter().filter(|l| l.as_str() == "#foo").count(), 1);
    assert!(labels.contains(&"#foo".to_string()));
    assert!(!labels.iter().any(|l| l.as_str() == "#Foo"));
    assert!(labels.contains(&"#bar".to_string()));
    assert!(labels.contains(&"#baz_1".to_string()));
    assert!(labels.contains(&"#dup".to_string()));
    let notes = load_notes().unwrap();
    let note = notes.iter().find(|n| n.title == "alpha").unwrap();
    assert_eq!(
        note.tags,
        vec![
            "bar".to_string(),
            "baz_1".to_string(),
            "dup".to_string(),
            "foo".to_string(),
        ]
    );
    let list_results = plugin.search("note list #bar");
    assert_eq!(list_results.len(), 1);
    assert_eq!(list_results[0].action, "note:open:alpha");
}

#[test]
fn note_link_dedupes_backlinks() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let _tmp = setup();
    append_note("alpha", "link to [[beta note]] and [[Beta Note]]").unwrap();
    append_note("beta note", "beta").unwrap();
    let plugin = NotePlugin::default();
    let results = plugin.search("note link beta note");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "note:open:alpha");
}

#[test]
fn note_today_opens_daily_note() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let _tmp = setup();
    let plugin = NotePlugin::default();
    let today = Local::now().format("%Y-%m-%d").to_string();
    let results = plugin.search("note today");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, format!("note:open:{}", today));
}

#[test]
fn note_open_uses_fuzzy_matching() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let _tmp = setup();
    append_note("fuzzy target", "content").unwrap();
    let plugin = NotePlugin::default();
    let results = plugin.search("note open fz targ");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "note:open:fuzzy-target");
}

#[test]
fn note_alias_supports_open_rm_and_labels() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let _tmp = setup();
    append_note("alpha", "# alpha\nAlias: special-name\n\ncontent").unwrap();
    let plugin = NotePlugin::default();

    let open_results = plugin.search("note open special-name");
    assert_eq!(open_results.len(), 1);
    assert_eq!(open_results[0].action, "note:open:alpha");
    assert_eq!(open_results[0].label, "special-name");

    let rm_results = plugin.search("note rm special-name");
    assert_eq!(rm_results.len(), 1);
    assert_eq!(rm_results[0].action, "note:remove:alpha");
    assert_eq!(rm_results[0].label, "Remove special-name");

    let list_results = plugin.search("note list");
    assert_eq!(list_results.len(), 1);
    assert_eq!(list_results[0].label, "special-name");
}

#[test]
fn launcher_app_delete_note_accepts_alias() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let _tmp = setup();
    append_note("alpha", "# alpha\nAlias: special-name\n\ncontent").unwrap();

    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    app.plugins.register(Box::new(NotePlugin::default()));

    app.query = "note list".into();
    app.search();
    assert_eq!(app.results.len(), 1);
    assert_eq!(app.results[0].label, "special-name");

    app.delete_note("special-name");
    assert!(load_notes().unwrap().is_empty());
    let plugin_after = NotePlugin::default();
    let after_results = plugin_after.search("note list");
    assert!(after_results.is_empty());
}

#[test]
fn extract_tags_skips_code_fences() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let _tmp = setup();
    append_note("alpha", "```\n#foo\n```\n#bar").unwrap();
    let notes = load_notes().unwrap();
    let note = notes.iter().find(|n| n.title == "alpha").unwrap();
    assert_eq!(note.tags, vec!["bar".to_string()]);
}

#[test]
fn note_list_filters_by_multiple_tags() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let _tmp = setup();
    append_note("one", "#foo #bar").unwrap();
    append_note("two", "#foo").unwrap();
    let plugin = NotePlugin::default();
    let results = plugin.search("note list #foo #bar");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "note:open:one");
}

#[test]
fn missing_link_colored_red() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let _tmp = setup();
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    let output = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            show_wiki_link(ui, &mut app, "missing");
        });
    });
    let shapes = output.shapes;
    assert!(shapes.iter().any(|s| match &s.shape {
        egui::epaint::Shape::Text(t) => {
            t.galley
                .job
                .sections
                .iter()
                .any(|sec| sec.format.color == egui::Color32::RED)
        }
        _ => false,
    }));
}

#[test]
fn link_validation_rejects_invalid_urls() {
    let content = "visit http://example.com and http://exa%mple.com also https://rust-lang.org and https://rust-lang.org and www.example.com and www.example.com and www.exa%mple.com";
    let links = extract_links(content);
    assert_eq!(
        links,
        vec![
            "https://rust-lang.org".to_string(),
            "www.example.com".to_string(),
        ]
    );
}
