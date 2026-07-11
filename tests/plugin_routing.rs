use multi_launcher::actions::Action;
use multi_launcher::plugin::{Plugin, PluginManager};
use multi_launcher::plugins::todo::TodoPlugin;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

struct CountingPlugin {
    name: &'static str,
    prefixes: &'static [&'static str],
    always_search: bool,
    calls: Arc<AtomicUsize>,
}

impl CountingPlugin {
    fn new(
        name: &'static str,
        prefixes: &'static [&'static str],
        always_search: bool,
        calls: Arc<AtomicUsize>,
    ) -> Self {
        Self {
            name,
            prefixes,
            always_search,
            calls,
        }
    }
}

impl Plugin for CountingPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        vec![Action {
            label: format!("{}:{query}", self.name),
            desc: "test".into(),
            action: self.name.into(),
            args: None,
        }]
    }

    fn name(&self) -> &str {
        self.name
    }

    fn description(&self) -> &str {
        "test"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn query_prefixes(&self) -> &[&str] {
        self.prefixes
    }

    fn always_search(&self) -> bool {
        self.always_search
    }
}

#[test]
fn routing_selects_expected_plugins() {
    let todo_calls = Arc::new(AtomicUsize::new(0));
    let timer_calls = Arc::new(AtomicUsize::new(0));
    let global_calls = Arc::new(AtomicUsize::new(0));

    let mut pm = PluginManager::new();
    pm.register(Box::new(CountingPlugin::new(
        "todo_plugin",
        &["todo"],
        false,
        todo_calls.clone(),
    )));
    pm.register(Box::new(CountingPlugin::new(
        "timer_plugin",
        &["timer"],
        false,
        timer_calls.clone(),
    )));
    pm.register(Box::new(CountingPlugin::new(
        "global_plugin",
        &[],
        false,
        global_calls.clone(),
    )));

    let out = pm.search_filtered("todo list", None, None);
    assert_eq!(todo_calls.load(Ordering::SeqCst), 1);
    assert_eq!(timer_calls.load(Ordering::SeqCst), 0);
    assert_eq!(global_calls.load(Ordering::SeqCst), 1);
    assert!(out.iter().any(|a| a.action == "todo_plugin"));
    assert!(out.iter().any(|a| a.action == "global_plugin"));
    assert!(!out.iter().any(|a| a.action == "timer_plugin"));
}

#[test]
fn global_plugins_and_opt_out_plugins_still_run() {
    let global_calls = Arc::new(AtomicUsize::new(0));
    let opt_out_calls = Arc::new(AtomicUsize::new(0));
    let prefixed_calls = Arc::new(AtomicUsize::new(0));

    let mut pm = PluginManager::new();
    pm.register(Box::new(CountingPlugin::new(
        "global",
        &[],
        false,
        global_calls.clone(),
    )));
    pm.register(Box::new(CountingPlugin::new(
        "always",
        &["timer"],
        true,
        opt_out_calls.clone(),
    )));
    pm.register(Box::new(CountingPlugin::new(
        "prefixed",
        &["todo"],
        false,
        prefixed_calls.clone(),
    )));

    pm.search_filtered("plain query", None, None);
    assert_eq!(global_calls.load(Ordering::SeqCst), 1);
    assert_eq!(opt_out_calls.load(Ordering::SeqCst), 1);
    assert_eq!(prefixed_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn search_capability_gate_skips_plugin_when_disabled() {
    use std::collections::HashMap;

    let calls = Arc::new(AtomicUsize::new(0));

    let mut pm = PluginManager::new();
    pm.register(Box::new(CountingPlugin::new(
        "searchable",
        &[],
        false,
        calls.clone(),
    )));

    let mut enabled_caps = HashMap::new();
    enabled_caps.insert("searchable".to_string(), vec!["commands".to_string()]);

    let out = pm.search_filtered("query", None, Some(&enabled_caps));
    assert!(out.is_empty());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn existing_prefix_commands_remain_equivalent() {
    let plugin = TodoPlugin::default();
    let direct = plugin.search("todo list");

    let mut pm = PluginManager::new();
    pm.register(Box::new(TodoPlugin::default()));
    let routed = pm.search_filtered("todo list", None, None);

    let routed_view: Vec<_> = routed
        .iter()
        .map(|a| {
            (
                a.label.as_str(),
                a.desc.as_str(),
                a.action.as_str(),
                a.args.as_ref(),
            )
        })
        .collect();
    let direct_view: Vec<_> = direct
        .iter()
        .map(|a| {
            (
                a.label.as_str(),
                a.desc.as_str(),
                a.action.as_str(),
                a.args.as_ref(),
            )
        })
        .collect();

    assert_eq!(routed_view, direct_view);
}

#[test]
fn builtin_search_filtered_routes_file_omni_and_folder_prefixes() {
    use multi_launcher::plugins::file_search::FileSearchPlugin;
    use multi_launcher::plugins::folders::FoldersPlugin;
    use multi_launcher::plugins::omni_search::OmniSearchPlugin;

    let actions = Arc::new(vec![Action {
        label: "plan app".into(),
        desc: "launcher".into(),
        action: "app:plan".into(),
        args: None,
    }]);
    let mut pm = PluginManager::new();
    pm.register(Box::new(FileSearchPlugin::default()));
    pm.register(Box::new(OmniSearchPlugin::new(Arc::clone(&actions))));
    pm.register(Box::new(FoldersPlugin::default()));

    let fs = pm.search_filtered("fs", None, None);
    assert!(fs.iter().any(|a| a.action == "file_search:open"));
    assert!(!fs.iter().any(|a| a.action == "app:plan"));

    let omni = pm.search_filtered("o plan", None, None);
    assert!(omni.iter().any(|a| a.action == "app:plan"));
    assert!(!omni.iter().any(|a| a.action.starts_with("file_search:")));

    let folders = pm.search_filtered("f list", None, None);
    assert!(folders.iter().any(|a| a.action.starts_with("folder:")));
    assert!(!folders.iter().any(|a| a.action.starts_with("file_search:")));

    let unrelated = pm.search_filtered("plain query", None, None);
    assert!(!unrelated
        .iter()
        .any(|a| a.action.starts_with("file_search:")));
}

#[test]
fn file_search_plugin_prefix_is_only_fs() {
    use multi_launcher::plugin::Plugin;
    use multi_launcher::plugins::file_search::FileSearchPlugin;

    let plugin = FileSearchPlugin::default();
    assert_eq!(plugin.query_prefixes(), &["fs"]);
}

#[test]
fn constructing_manager_with_file_search_settings_preserves_omni_and_indexer_behavior() {
    use multi_launcher::file_search::settings::FileSearchSettings;
    use multi_launcher::indexer::index_paths;
    use multi_launcher::settings::{NetUnit, Settings};
    use serde_json::json;
    use std::collections::HashMap;
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    let file_path = dir.path().join("indexed.txt");
    std::fs::write(&file_path, "indexed").unwrap();
    let roots = vec![dir.path().display().to_string()];
    let indexed_before = index_paths(&roots).unwrap();

    let actions = Arc::new(vec![Action {
        label: "plan app".into(),
        desc: "launcher".into(),
        action: "app:plan".into(),
        args: None,
    }]);
    let mut plugin_settings = HashMap::new();
    plugin_settings.insert(
        "omni_search".to_string(),
        json!({"include_apps": false, "include_notes": false, "include_todos": false, "include_calendar": false, "include_folders": false, "include_bookmarks": false}),
    );
    plugin_settings.insert(
        "file_search".to_string(),
        serde_json::to_value(FileSearchSettings {
            global_content_search_roots: vec![dir.path().to_path_buf()],
            max_search_results: 7,
            everything_enabled: false,
            ..FileSearchSettings::default()
        })
        .unwrap(),
    );

    let mut pm = PluginManager::new();
    pm.reload_from_dirs(
        &[],
        Settings::default().clipboard_limit,
        NetUnit::Bytes,
        false,
        &plugin_settings,
        Arc::clone(&actions),
    );

    let omni = pm.search_filtered("o plan", None, None);
    assert!(!omni.iter().any(|a| a.action == "app:plan"));

    let indexed_after = index_paths(&roots).unwrap();
    let action_view = |actions: Vec<Action>| {
        actions
            .into_iter()
            .map(|a| (a.label, a.desc, a.action, a.args))
            .collect::<Vec<_>>()
    };
    assert_eq!(action_view(indexed_before), action_view(indexed_after));
}
