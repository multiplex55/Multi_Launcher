use eframe::egui;
use multi_launcher::actions::Action;
use multi_launcher::gui::LauncherApp;
use multi_launcher::plugin::{Plugin, PluginManager};
use multi_launcher::settings::Settings;
use serde_json::json;
use std::sync::{atomic::AtomicBool, Arc};

fn new_app(ctx: &egui::Context, actions: Vec<Action>) -> LauncherApp {
    let custom_len = actions.len();
    let mut plugins = PluginManager::new();
    let dirs: Vec<String> = Vec::new();
    let actions_arc = Arc::new(actions);
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

fn new_app_with_settings(
    ctx: &egui::Context,
    actions: Vec<Action>,
    settings: Settings,
) -> LauncherApp {
    let custom_len = actions.len();
    let mut plugins = PluginManager::new();
    let dirs: Vec<String> = Vec::new();
    let plugin_settings = settings.plugin_settings.clone();
    let actions_arc = Arc::new(actions);
    plugins.reload_from_dirs(
        &dirs,
        settings.clipboard_limit,
        settings.net_unit,
        false,
        &plugin_settings,
        Arc::clone(&actions_arc),
    );
    let enabled_plugins = settings.enabled_plugins.clone();
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
        None,
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
    )
}

#[test]
fn empty_query_lists_commands() {
    let ctx = egui::Context::default();
    let actions = vec![Action {
        label: "chrome".into(),
        desc: "web".into(),
        action: "chrome".into(),
        args: None,
    }];
    let mut app = new_app(&ctx, actions);
    app.query.clear();
    app.search();
    assert!(app.results.iter().any(|a| a.label == "help"));
    assert!(app.results.iter().any(|a| a.label == "app chrome"));
}

#[test]
fn query_matches_commands_when_plugins_empty() {
    let ctx = egui::Context::default();
    let actions: Vec<Action> = Vec::new();
    let mut app = new_app(&ctx, actions);
    app.query = "hel".into();
    app.search();
    assert!(app.results.iter().any(|a| a.label == "help"));
}

#[test]
fn disabled_plugin_commands_hidden() {
    let ctx = egui::Context::default();
    let actions: Vec<Action> = Vec::new();
    let mut settings = Settings::default();
    settings.enabled_plugins = Some(std::collections::HashSet::from(["web_search".to_string()]));
    let mut app = new_app_with_settings(&ctx, actions, settings);
    app.query.clear();
    app.search();
    assert!(!app.results.iter().any(|a| a.label == "help"));
}

#[test]
fn omni_search_settings_from_plugin_manager_are_applied() {
    let ctx = egui::Context::default();
    let actions = vec![Action {
        label: "plan app".into(),
        desc: "launcher".into(),
        action: "app:plan".into(),
        args: None,
    }];
    let mut settings = Settings::default();
    settings.plugin_settings.insert(
        "omni_search".into(),
        json!({"include_calendar": false, "include_todos": false}),
    );

    let mut app = new_app_with_settings(&ctx, actions, settings);
    app.query = "o list".into();
    app.search();

    assert!(!app.results.iter().any(|a| a.action == "calendar:upcoming"));
    assert!(!app.results.iter().any(|a| a.action == "todo:done:0"));
    assert!(app.results.iter().any(|a| a.action == "app:plan"));
}

#[test]
fn command_collection_keeps_existing_folder_and_omni_commands_and_adds_file_search_commands() {
    use multi_launcher::plugins::file_search::FileSearchPlugin;
    use multi_launcher::plugins::folders::FoldersPlugin;
    use multi_launcher::plugins::omni_search::OmniSearchPlugin;

    fn command_view(actions: &[Action]) -> std::collections::HashSet<(String, String, String)> {
        actions
            .iter()
            .map(|a| (a.label.clone(), a.desc.clone(), a.action.clone()))
            .collect()
    }

    let actions = Arc::new(Vec::new());
    let mut previous = PluginManager::new();
    previous.register(Box::new(FoldersPlugin::default()));
    previous.register(Box::new(OmniSearchPlugin::new(Arc::clone(&actions))));
    let previous_commands = command_view(&previous.commands());

    let mut current = PluginManager::new();
    current.register(Box::new(FoldersPlugin::default()));
    current.register(Box::new(OmniSearchPlugin::new(Arc::clone(&actions))));
    current.register(Box::new(FileSearchPlugin::default()));
    let current_commands = command_view(&current.commands());

    for command in &previous_commands {
        assert!(
            current_commands.contains(command),
            "missing previous command: {command:?}"
        );
    }

    for expected in command_view(&FileSearchPlugin::default().commands()) {
        assert!(
            current_commands.contains(&expected),
            "missing file-search command: {expected:?}"
        );
    }
}

#[test]
fn default_command_collection_keeps_clipboard_modify_baseline_plugins_registered() {
    let actions = Arc::new(Vec::new());
    let mut plugins = PluginManager::new();
    plugins.reload_from_dirs(
        &[],
        Settings::default().clipboard_limit,
        Settings::default().net_unit,
        false,
        &std::collections::HashMap::new(),
        Arc::clone(&actions),
    );

    let plugin_names: std::collections::HashSet<_> = plugins.plugin_names().into_iter().collect();
    for name in [
        "clipboard",
        "snippets",
        "text_case",
        "file_search",
        "omni_search",
    ] {
        assert!(plugin_names.contains(name), "missing plugin {name}");
    }

    let commands: std::collections::HashSet<_> = plugins
        .commands()
        .into_iter()
        .map(|a| (a.label, a.desc, a.action))
        .collect();
    for expected in [
        ("cb", "Clipboard", "query:cb"),
        ("cs", "Snippet", "query:cs"),
        ("case <text>", "Text Case", "query:case "),
        ("fs", "Open local file search", "query:fs"),
        ("o", "Omni", "query:o "),
    ] {
        assert!(
            commands.contains(&(expected.0.into(), expected.1.into(), expected.2.into())),
            "missing command {expected:?}"
        );
    }
}

#[test]
fn cm_command_is_visible_without_hiding_existing_commands() {
    let actions = Arc::new(Vec::new());
    let mut plugins = PluginManager::new();
    plugins.reload_from_dirs(
        &[],
        Settings::default().clipboard_limit,
        Settings::default().net_unit,
        false,
        &std::collections::HashMap::new(),
        Arc::clone(&actions),
    );

    let commands: std::collections::HashSet<_> = plugins
        .commands()
        .into_iter()
        .map(|a| (a.label, a.desc, a.action))
        .collect();

    for expected in [
        ("cm", "Clipboard Modify", "query:cm"),
        ("cb", "Clipboard", "query:cb"),
        ("cs", "Snippet", "query:cs"),
        ("fs", "Open local file search", "query:fs"),
        ("o", "Omni", "query:o "),
    ] {
        assert!(
            commands.contains(&(expected.0.into(), expected.1.into(), expected.2.into())),
            "missing command {expected:?}"
        );
    }
}
