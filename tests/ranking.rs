use eframe::egui;
use multi_launcher::actions::Action;
use multi_launcher::gui::{LauncherApp, APP_PREFIX};
use multi_launcher::plugin::PluginManager;
use multi_launcher::settings::Settings;
use std::sync::{atomic::AtomicBool, Arc};

fn new_app_with_settings(
    ctx: &egui::Context,
    actions: Vec<Action>,
    settings: Settings,
) -> LauncherApp {
    let custom_len = actions.len();
    LauncherApp::new(
        ctx,
        Arc::new(actions),
        custom_len,
        PluginManager::new(),
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

#[test]
fn usage_ranking() {
    let ctx = egui::Context::default();
    let actions = vec![
        Action {
            label: "foo".into(),
            desc: "".into(),
            action: "a".into(),
            args: None,
        },
        Action {
            label: "foo".into(),
            desc: "".into(),
            action: "b".into(),
            args: None,
        },
    ];
    let settings = Settings::default();
    let mut app = new_app_with_settings(&ctx, actions, settings);
    app.usage.insert("b".into(), 5);
    app.query = format!("{} foo", APP_PREFIX);
    app.search();
    assert_eq!(app.results[0].action, "b");
}

#[test]
fn fuzzy_vs_usage_weight() {
    let ctx = egui::Context::default();
    let actions = vec![
        Action {
            label: "abc".into(),
            desc: "".into(),
            action: "a".into(),
            args: None,
        },
        Action {
            label: "defabc".into(),
            desc: "".into(),
            action: "b".into(),
            args: None,
        },
    ];
    let mut settings = Settings::default();
    settings.fuzzy_weight = 5.0;
    settings.usage_weight = 1.0;
    let mut app = new_app_with_settings(&ctx, actions, settings);
    app.usage.insert("b".into(), 20);
    app.query = format!("{} abc", APP_PREFIX);
    app.search();
    assert_eq!(app.results[0].action, "a");
}

#[test]
fn exact_mode_excludes_fuzzy_only_candidate_even_if_fuzzy_would_prefer_it() {
    let ctx = egui::Context::default();
    let actions = vec![
        Action {
            label: "e1v2e".into(),
            desc: "".into(),
            action: "fuzzy_only".into(),
            args: None,
        },
        Action {
            label: "testingeve123".into(),
            desc: "".into(),
            action: "substring".into(),
            args: None,
        },
    ];

    let mut fuzzy_settings = Settings::default();
    fuzzy_settings.fuzzy_weight = 10.0;
    fuzzy_settings.match_exact = false;
    let mut fuzzy_app = new_app_with_settings(&ctx, actions.clone(), fuzzy_settings);
    fuzzy_app.query = format!("{} eve", APP_PREFIX);
    fuzzy_app.search();
    assert!(fuzzy_app.results.iter().any(|a| a.action == "fuzzy_only"));

    let mut exact_settings = Settings::default();
    exact_settings.fuzzy_weight = 10.0;
    exact_settings.match_exact = true;
    let mut exact_app = new_app_with_settings(&ctx, actions, exact_settings);
    exact_app.query = format!("{} eve", APP_PREFIX);
    exact_app.search();
    assert!(exact_app.results.iter().any(|a| a.action == "substring"));
    assert!(!exact_app.results.iter().any(|a| a.action == "fuzzy_only"));
}

#[test]
fn exact_mode_partial_label_match_is_case_insensitive() {
    let ctx = egui::Context::default();
    let actions = vec![Action {
        label: "testingeve123".into(),
        desc: "".into(),
        action: "a".into(),
        args: None,
    }];
    let mut settings = Settings::default();
    settings.match_exact = true;
    let mut app = new_app_with_settings(&ctx, actions, settings);

    app.query = format!("{} Eve", APP_PREFIX);
    app.search();

    assert!(app.results.iter().any(|a| a.action == "a"));
}

#[test]
fn exact_mode_ignores_fuzzy_weight_for_fuzzy_only_match() {
    let ctx = egui::Context::default();
    let actions = vec![Action {
        label: "e1v2e".into(),
        desc: "".into(),
        action: "fuzzy_only".into(),
        args: None,
    }];
    let mut settings = Settings::default();
    settings.match_exact = true;
    settings.fuzzy_weight = 1000.0;
    let mut app = new_app_with_settings(&ctx, actions, settings);

    app.query = format!("{} eve", APP_PREFIX);
    app.search();

    assert!(!app.results.iter().any(|a| a.action == "fuzzy_only"));
}
