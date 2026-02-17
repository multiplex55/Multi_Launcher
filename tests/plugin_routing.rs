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
