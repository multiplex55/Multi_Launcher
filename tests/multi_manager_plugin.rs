use multi_launcher::actions::Action;
use multi_launcher::multi_manager::commands::search_mm_commands;
use multi_launcher::plugin::{Plugin, PluginManager};
use multi_launcher::plugins::multi_manager::MultiManagerPlugin;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

struct CountingPlugin {
    name: &'static str,
    prefixes: &'static [&'static str],
    calls: Arc<AtomicUsize>,
}

impl Plugin for CountingPlugin {
    fn search(&self, _query: &str) -> Vec<Action> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        vec![Action {
            label: self.name.into(),
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
}

#[test]
fn mm_query_routes_only_to_multi_manager_plus_global_plugins() {
    let other_calls = Arc::new(AtomicUsize::new(0));
    let global_calls = Arc::new(AtomicUsize::new(0));
    let mut pm = PluginManager::new();
    pm.register(Box::new(MultiManagerPlugin));
    pm.register(Box::new(CountingPlugin {
        name: "other",
        prefixes: &["todo"],
        calls: other_calls.clone(),
    }));
    pm.register(Box::new(CountingPlugin {
        name: "global",
        prefixes: &[],
        calls: global_calls.clone(),
    }));

    let out = pm.search_filtered("mm", None, None);

    assert_eq!(other_calls.load(Ordering::SeqCst), 0);
    assert_eq!(global_calls.load(Ordering::SeqCst), 1);
    assert!(out.iter().any(|a| a.action == "mm:open"));
    assert!(out.iter().any(|a| a.action == "global"));
    assert!(!out.iter().any(|a| a.action == "other"));
}

#[test]
fn non_mm_queries_do_not_call_multi_manager_plugin() {
    let mut pm = PluginManager::new();
    pm.register(Box::new(MultiManagerPlugin));

    let out = pm.search_filtered("todo list", None, None);

    assert!(out.iter().all(|a| !a.action.starts_with("mm:")));
}

#[test]
fn mm_returns_open() {
    let actions = search_mm_commands("mm");
    assert!(actions.iter().any(|a| a.action == "mm:open"));
}

#[test]
fn mm_settings_returns_settings() {
    let actions = search_mm_commands("mm settings");
    assert!(actions.iter().any(|a| a.action == "mm:settings"));
}

#[test]
fn mm_recapture_all_returns_recapture_all() {
    let actions = search_mm_commands("mm recapture all");
    assert!(actions.iter().any(|a| a.action == "mm:recapture-all"));
}

#[test]
fn non_mm_command_parser_input_returns_no_result() {
    assert!(search_mm_commands("todo list").is_empty());
    assert!(search_mm_commands("mms").is_empty());
}
