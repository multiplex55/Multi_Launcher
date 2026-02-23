use eframe::egui;
use multi_launcher::actions::Action;
use multi_launcher::gui::LauncherApp;
use multi_launcher::plugin::PluginManager;
use multi_launcher::plugins::bookmarks::{save_bookmarks, BookmarkEntry, BOOKMARKS_FILE};
use multi_launcher::plugins::note::{append_note, save_notes};
use multi_launcher::plugins::snippets::{save_snippets, SnippetEntry, SNIPPETS_FILE};
use multi_launcher::settings::Settings;
use once_cell::sync::Lazy;
use std::sync::{atomic::AtomicBool, Arc, Mutex};
use tempfile::tempdir;

static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

fn new_app(ctx: &egui::Context, settings: Settings) -> LauncherApp {
    let custom_len = 0;
    let mut plugins = PluginManager::new();
    let dirs: Vec<String> = Vec::new();
    let actions_arc: Arc<Vec<Action>> = Arc::new(Vec::new());
    plugins.reload_from_dirs(
        &dirs,
        Settings::default().clipboard_limit,
        Settings::default().net_unit,
        false,
        &std::collections::HashMap::new(),
        Arc::clone(&actions_arc),
    );
    LauncherApp::new(
        ctx,
        actions_arc,
        custom_len,
        plugins,
        "actions.json".into(),
        "settings.json".into(),
        settings,
        None,
        None,
        None,
        None,
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
    )
}

fn setup_notes_env(dir: &tempfile::TempDir) {
    let notes_dir = dir.path().join("notes");
    std::fs::create_dir_all(&notes_dir).unwrap();
    std::env::set_var("ML_NOTES_DIR", &notes_dir);
    std::env::set_var("HOME", dir.path());
    save_notes(&[]).unwrap();
}

#[test]
fn plugin_query_is_exact_when_fuzzy_disabled() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let entries = vec![BookmarkEntry {
        url: "https://example.com".into(),
        alias: Some("foobar".into()),
    }];
    save_bookmarks(BOOKMARKS_FILE, &entries).unwrap();

    let mut settings = Settings::default();
    settings.fuzzy_weight = 0.0;
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx, settings);

    app.query = "bm foobar".into();
    app.search();
    assert_eq!(app.results.len(), 1);

    app.query = "bm fbr".into();
    app.search();
    assert_eq!(app.results.len(), 0);
}

#[test]
fn plugin_command_unfiltered_when_no_query() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    let entries = vec![BookmarkEntry {
        url: "https://example.com".into(),
        alias: None,
    }];
    save_bookmarks(BOOKMARKS_FILE, &entries).unwrap();
    let mut settings = Settings::default();
    settings.fuzzy_weight = 0.0;
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx, settings);
    app.query = "bm list".into();
    app.search();
    assert_eq!(app.results.len(), 1);
}

#[test]
fn snippet_edit_command_unfiltered() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    let entries = vec![SnippetEntry {
        alias: "foo".into(),
        text: "bar".into(),
    }];
    save_snippets(SNIPPETS_FILE, &entries).unwrap();
    let mut settings = Settings::default();
    settings.fuzzy_weight = 0.0;
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx, settings);
    app.query = "cs edit".into();
    app.search();
    assert_eq!(app.results.len(), 1);
}

#[test]
fn note_today_returns_resolved_note_action_when_exact_match_enabled() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    setup_notes_env(&dir);

    let mut settings = Settings::default();
    settings.match_exact = true;
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx, settings);

    app.query = "note today".into();
    app.search();

    assert!(!app.results.is_empty());
    assert!(
        app.results
            .iter()
            .any(|result| result.action.starts_with("note:new:"))
    );
    assert!(
        app.results
            .iter()
            .all(|result| !result.action.starts_with("query:"))
    );
}

#[test]
fn note_search_matches_note_content_when_exact_match_enabled() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    setup_notes_env(&dir);
    append_note("alpha", "ordinary body").unwrap();
    append_note("beta", "contains unique needle in content").unwrap();

    let mut settings = Settings::default();
    settings.match_exact = true;
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx, settings);

    app.query = "note search needle".into();
    app.search();

    assert!(!app.results.is_empty());
    assert!(
        app.results
            .iter()
            .any(|result| result.action == "note:open:beta")
    );
    assert!(
        app.results
            .iter()
            .all(|result| !result.action.starts_with("query:"))
    );
}

#[test]
fn note_new_generates_slugged_action_when_exact_match_enabled() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    setup_notes_env(&dir);

    let mut settings = Settings::default();
    settings.match_exact = true;
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx, settings);

    app.query = "note new Hello World".into();
    app.search();

    assert!(!app.results.is_empty());
    assert!(
        app.results
            .iter()
            .any(|result| result.action == "note:new:hello-world")
    );
    assert!(
        app.results
            .iter()
            .all(|result| !result.action.starts_with("query:"))
    );
}
