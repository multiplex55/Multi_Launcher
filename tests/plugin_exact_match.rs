#![allow(clippy::field_reassign_with_default)]

use chrono::{Duration, Local};
use eframe::egui;
use multi_launcher::actions::Action;
use multi_launcher::gui::LauncherApp;
use multi_launcher::plugin::PluginManager;
use multi_launcher::plugins::bookmarks::{BOOKMARKS_FILE, BookmarkEntry, save_bookmarks};
use multi_launcher::plugins::note::{append_note, save_notes};
use multi_launcher::plugins::snippets::{SNIPPETS_FILE, SnippetEntry, save_snippets};
use multi_launcher::settings::Settings;
use once_cell::sync::Lazy;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, atomic::AtomicBool};
use tempfile::tempdir;

static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

fn new_app(ctx: &egui::Context, settings: Settings) -> LauncherApp {
    new_app_with_filters(ctx, settings, None, None)
}

fn new_app_with_filters(
    ctx: &egui::Context,
    settings: Settings,
    enabled_plugins: Option<HashSet<String>>,
    enabled_capabilities: Option<HashMap<String, Vec<String>>>,
) -> LauncherApp {
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
        enabled_plugins,
        enabled_capabilities,
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
    )
}

#[test]
fn date_results_remain_visible_through_launcher_search_in_all_match_modes() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx, Settings::default());
    for (exact, fuzzy_weight) in [
        (false, Settings::default().fuzzy_weight),
        (true, 1.0),
        (false, 0.0),
    ] {
        app.match_exact = exact;
        app.fuzzy_weight = fuzzy_weight;
        for (query, expected) in [
            ("date 30 days from today", None),
            (
                "date 1 month after 2026-01-31",
                Some("clipboard:2026-02-28"),
            ),
            (
                "date days between 2026-10-05 and 2026-12-25",
                Some("clipboard:81 days"),
            ),
            ("date tomorrow", None),
            ("date 2026-02-30", Some("noop:")),
        ] {
            let before = Local::now().date_naive();
            app.query = query.into();
            app.search();
            let after = Local::now().date_naive();
            assert_eq!(
                app.results.len(),
                1,
                "{query}, exact={exact}, weight={fuzzy_weight}"
            );
            let result = &app.results[0];
            assert_eq!(result.desc, "Date Arithmetic");
            assert!(!result.label.is_empty());
            match expected {
                Some("noop:") => assert!(result.action.starts_with("noop:")),
                Some(action) => assert_eq!(result.action, action),
                None => {
                    let days = if query == "date tomorrow" { 1 } else { 30 };
                    let expected: Vec<_> = [before, after]
                        .into_iter()
                        .map(|date| {
                            format!(
                                "clipboard:{}",
                                (date + Duration::days(days)).format("%Y-%m-%d")
                            )
                        })
                        .collect();
                    assert!(expected.contains(&result.action), "{}", result.action);
                }
            }
        }
    }
    // Timestamp commands retain their ordinary routing; exact filtering
    // of timestamp labels is outside this date-only remediation.
    app.match_exact = false;
    app.fuzzy_weight = Settings::default().fuzzy_weight;
    for (query, description) in [("ts 0", "Timestamp"), ("tsm 3600000", "Midnight TS")] {
        app.query = query.into();
        app.search();
        assert_eq!(app.results.len(), 1, "{query}");
        assert_eq!(app.results[0].desc, description);
        assert!(app.results[0].action.starts_with("clipboard:"));
        if query.starts_with("tsm") {
            assert_eq!(app.results[0].action, "clipboard:01:00:00");
        }
    }
}

#[test]
fn date_manager_and_launcher_respect_plugin_and_search_capability_enablement() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    let ctx = egui::Context::default();
    let date_enabled = Some(HashSet::from(["date_arithmetic".to_owned()]));
    for (plugins, capabilities, expected) in [
        (None, None, 1),
        (date_enabled.clone(), None, 1),
        (Some(HashSet::from(["timestamp".to_owned()])), None, 0),
        (
            date_enabled.clone(),
            Some(HashMap::from([(
                "date_arithmetic".to_owned(),
                vec!["search".to_owned()],
            )])),
            1,
        ),
        (
            date_enabled,
            Some(HashMap::from([("date_arithmetic".to_owned(), Vec::new())])),
            0,
        ),
    ] {
        let mut app = new_app_with_filters(
            &ctx,
            Settings::default(),
            plugins.clone(),
            capabilities.clone(),
        );
        let query = "date 1 month after 2026-01-31";
        let actions = app
            .plugins
            .search_filtered(query, plugins.as_ref(), capabilities.as_ref());
        assert_eq!(actions.len(), expected);
        if expected == 1 {
            assert_eq!(actions[0].action, "clipboard:2026-02-28");
        }
        app.query = query.into();
        app.search();
        assert_eq!(app.results.len(), expected);
        if expected == 1 {
            assert_eq!(app.results[0].action, "clipboard:2026-02-28");
        }
    }
}

fn setup_notes_env(dir: &tempfile::TempDir) {
    let notes_dir = dir.path().join("notes");
    std::fs::create_dir_all(&notes_dir).unwrap();
    unsafe { std::env::set_var("ML_NOTES_DIR", &notes_dir) };
    unsafe { std::env::set_var("HOME", dir.path()) };
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
