use eframe::egui;
use multi_launcher::actions::Action;
use multi_launcher::gui::{
    ActivationSource, LauncherApp, set_activation_hook, set_execute_action_hook,
};
use multi_launcher::mkmacro::{
    LauncherCommandBroker, LauncherCommandKind, LauncherCommandRequest, LauncherCommandResponse,
    RunControl,
};
use multi_launcher::plugin::PluginManager;
use multi_launcher::plugins::bookmarks::{BOOKMARKS_FILE, BookmarkEntry, save_bookmarks};
use multi_launcher::plugins::folders::{FOLDERS_FILE, FolderEntry, save_folders};
use multi_launcher::plugins::note::append_note;
use multi_launcher::settings::Settings;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, atomic::AtomicBool};

struct IsolatedEnvironment {
    old_cwd: PathBuf,
    old_home: Option<std::ffi::OsString>,
    old_notes: Option<std::ffi::OsString>,
    _temp: tempfile::TempDir,
}

impl IsolatedEnvironment {
    fn new() -> Self {
        let old_cwd = std::env::current_dir().unwrap();
        let old_home = std::env::var_os("HOME");
        let old_notes = std::env::var_os("ML_NOTES_DIR");
        let temp = tempfile::tempdir().unwrap();
        let notes = temp.path().join("notes");
        std::fs::create_dir_all(&notes).unwrap();
        std::env::set_current_dir(temp.path()).unwrap();
        // SAFETY: this integration binary contains one test, so no peer test
        // thread can observe these process-wide overrides.
        unsafe {
            std::env::set_var("HOME", temp.path());
            std::env::set_var("ML_NOTES_DIR", notes);
        }
        Self {
            old_cwd,
            old_home,
            old_notes,
            _temp: temp,
        }
    }
}

impl Drop for IsolatedEnvironment {
    fn drop(&mut self) {
        set_activation_hook(None);
        set_execute_action_hook(None);
        std::env::set_current_dir(&self.old_cwd).unwrap();
        // SAFETY: see `new`; restore every override before dropping the fixture.
        unsafe {
            match &self.old_home {
                Some(value) => std::env::set_var("HOME", value),
                None => std::env::remove_var("HOME"),
            }
            match &self.old_notes {
                Some(value) => std::env::set_var("ML_NOTES_DIR", value),
                None => std::env::remove_var("ML_NOTES_DIR"),
            }
        }
    }
}

fn request(id: u64, query: &str) -> LauncherCommandRequest {
    LauncherCommandRequest {
        id,
        kind: LauncherCommandKind::Query(query.to_owned()),
    }
}

fn dispatch(app: &mut LauncherApp, id: u64, query: &str) -> LauncherCommandResponse {
    app.handle_macro_launcher_command(&request(id, query))
}

fn result_debug(query: &str, plugin: &str, results: &[Action]) -> String {
    format!(
        "query={query:?}, plugin={plugin}, result_count={}, results={:?}",
        results.len(),
        results
            .iter()
            .map(|a| (&a.label, &a.action, &a.args))
            .collect::<Vec<_>>()
    )
}

#[test]
fn real_registration_search_broker_and_gui_activation_cover_builtin_routes() {
    let _environment = IsolatedEnvironment::new();

    for title in ["alpha", "beta", "gamma"] {
        append_note(title, &format!("content for {title}")).unwrap();
    }
    save_bookmarks(
        BOOKMARKS_FILE,
        &[
            BookmarkEntry {
                url: "https://unique.invalid/path".into(),
                alias: Some("unique-bookmark".into()),
            },
            BookmarkEntry {
                url: "https://second.invalid/".into(),
                alias: Some("second-bookmark".into()),
            },
        ],
    )
    .unwrap();
    let folder_path = std::env::current_dir()
        .unwrap()
        .join("deterministic-folder-path");
    std::fs::create_dir(&folder_path).unwrap();
    save_folders(
        FOLDERS_FILE,
        &[FolderEntry {
            label: "unique-folder".into(),
            path: folder_path.display().to_string(),
            alias: None,
        }],
    )
    .unwrap();

    let application = Action {
        label: "Deterministic Integration Application".into(),
        desc: "Application".into(),
        action: "deterministic-integration-application".into(),
        args: Some("--integration-argument".into()),
    };
    let actions = Arc::new(vec![application.clone()]);
    let mut manager = PluginManager::new();
    manager.reload_from_dirs(
        &[],
        10,
        multi_launcher::settings::NetUnit::Auto,
        false,
        &HashMap::new(),
        Arc::clone(&actions),
    );
    for plugin in ["notes", "bookmarks", "folders"] {
        assert!(
            manager.plugin_names().iter().any(|name| name == plugin),
            "startup registration omitted plugin={plugin}"
        );
    }

    let ctx = egui::Context::default();
    let mut app = LauncherApp::new(
        &ctx,
        actions,
        0,
        manager,
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
    );
    let activations = Arc::new(Mutex::new(Vec::<(Action, ActivationSource)>::new()));
    let executions = Arc::new(Mutex::new(Vec::<Action>::new()));
    set_activation_hook(Some(Box::new({
        let activations = Arc::clone(&activations);
        move |action, source| activations.lock().unwrap().push((action.clone(), source))
    })));
    set_execute_action_hook(Some(Box::new({
        let executions = Arc::clone(&executions);
        move |action| {
            executions.lock().unwrap().push(action.clone());
            Ok(())
        }
    })));

    let response = dispatch(&mut app, 1, "note list");
    let detail = result_debug("note list", "notes", &app.results);
    assert_eq!(
        response,
        LauncherCommandResponse::PresentedForSelection { result_count: 3 },
        "{detail}"
    );
    for slug in ["alpha", "beta", "gamma"] {
        assert!(
            app.results
                .iter()
                .any(|a| a.action == format!("note:open:{slug}")),
            "missing {slug}; {detail}"
        );
    }
    assert!(
        activations.lock().unwrap().is_empty(),
        "list arbitrarily activated; {detail}"
    );
    assert!(
        executions.lock().unwrap().is_empty(),
        "raw query was executed; {detail}"
    );

    assert_eq!(
        dispatch(&mut app, 2, "note open alpha"),
        LauncherCommandResponse::Activated
    );
    assert_eq!(
        app.open_note_panel_count(),
        1,
        "normal Notes UI did not open"
    );
    assert_eq!(
        activations
            .lock()
            .unwrap()
            .last()
            .map(|(a, s)| (a.action.as_str(), *s)),
        Some(("note:open:alpha", ActivationSource::Macro))
    );

    let response = dispatch(&mut app, 3, "bm list");
    let detail = result_debug("bm list", "bookmarks", &app.results);
    assert_eq!(
        response,
        LauncherCommandResponse::PresentedForSelection { result_count: 2 },
        "{detail}"
    );
    assert_eq!(
        dispatch(&mut app, 4, "bm unique-bookmark"),
        LauncherCommandResponse::Activated
    );
    assert_eq!(
        executions.lock().unwrap().last().map(|a| a.action.as_str()),
        Some("https://unique.invalid/path")
    );

    let response = dispatch(&mut app, 5, "f list");
    let detail = result_debug("f list", "folders", &app.results);
    assert_eq!(response, LauncherCommandResponse::Activated, "{detail}");
    assert_eq!(
        dispatch(&mut app, 6, "f unique-folder"),
        LauncherCommandResponse::Activated
    );
    assert_eq!(
        executions.lock().unwrap().last().map(|a| a.action.as_str()),
        Some(folder_path.to_str().unwrap())
    );

    assert_eq!(
        dispatch(&mut app, 7, "app Deterministic Integration Application"),
        LauncherCommandResponse::Activated
    );
    let (activated_app, source) = activations.lock().unwrap().last().cloned().unwrap();
    assert_eq!(
        (activated_app.action, activated_app.args, source),
        (
            application.action.clone(),
            application.args.clone(),
            ActivationSource::Macro
        )
    );
    assert_eq!(
        executions.lock().unwrap().last().cloned(),
        Some(application)
    );
    assert!(executions.lock().unwrap().iter().all(|a| {
        ![
            "note list",
            "note open alpha",
            "bm list",
            "bm unique-bookmark",
            "f list",
            "f unique-folder",
        ]
        .contains(&a.action.as_str())
    }));

    // Exercise the real worker/GUI boundary: precisely one untouched raw query
    // is submitted, then the GUI owns search and returns the complete list.
    let broker = Arc::new(LauncherCommandBroker::default());
    let worker = {
        let broker = Arc::clone(&broker);
        std::thread::spawn(move || broker.submit_query("note list", &RunControl::default()))
    };
    let pending = loop {
        if let Some(pending) = broker.take_pending() {
            break pending;
        }
        std::thread::yield_now();
    };
    assert_eq!(
        pending.request.kind,
        LauncherCommandKind::Query("note list".into())
    );
    let broker_response = app.handle_macro_launcher_command(&pending.request);
    assert!(pending.respond(broker_response));
    assert_eq!(
        worker.join().unwrap().unwrap(),
        LauncherCommandResponse::PresentedForSelection { result_count: 3 }
    );
    assert!(
        broker.take_pending().is_none(),
        "macro submitted more than one request"
    );
}
