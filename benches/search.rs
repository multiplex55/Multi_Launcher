use criterion::{criterion_group, criterion_main, Criterion};
use eframe::egui;
use multi_launcher::{gui::LauncherApp, plugin::PluginManager, actions::Action, settings::Settings};

fn bench_search(c: &mut Criterion) {
    let ctx = egui::Context::default();
    let actions: Vec<Action> = (0..10_000)
        .map(|i| Action {
            label: format!("Item {i}"),
            desc: String::new(),
            action: format!("{i}"),
            args: None,
        })
        .collect();
    let settings = Settings::default();
    let mut app = LauncherApp::new(
        &ctx,
        actions,
        10_000,
        PluginManager::new(),
        "actions.json".into(),
        "settings.json".into(),
        settings,
        None,
        None,
        None,
        None,
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );
    app.query = "app Item 9999".to_string();
    c.bench_function("search_10k", |b| b.iter(|| app.search()));
}

criterion_group!(benches, bench_search);
criterion_main!(benches);
