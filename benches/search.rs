use criterion::{criterion_group, criterion_main, Criterion};
use eframe::egui;
use multi_launcher::{
    actions::Action, gui::LauncherApp, plugin::PluginManager, settings::Settings,
};
use std::sync::Arc;

fn bench_search(c: &mut Criterion) {
    let ctx = egui::Context::default();
    let actions: Vec<Action> = (0..10_000)
        .map(|i| Action {
            label: format!("Item {i}"),
            desc: String::new(),
            action: format!("{i}"),
            args: None,
            preview_text: None,
            risk_level: None,
            icon: None,
        })
        .collect();
    let actions_arc = Arc::new(actions);
    let settings = Settings::default();
    let mut app = LauncherApp::new(
        &ctx,
        actions_arc,
        10_000,
        PluginManager::new(),
        "actions.json".into(),
        "settings.json".into(),
        settings,
        None,
        None,
        None,
        None,
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );
    app.query = "app Item 9999".to_string();
    c.bench_function("search_10k", |b| b.iter(|| app.search()));
}

criterion_group!(benches, bench_search);
criterion_main!(benches);
