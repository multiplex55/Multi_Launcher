use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use eframe::egui;
use multi_launcher::{
    actions::Action,
    completion,
    gui::LauncherApp,
    plugin::{Plugin, PluginManager},
    plugins::browser_tabs::BrowserTabsPlugin,
    settings::Settings,
};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

const REPRESENTATIVE_ACTIONS: usize = 500;
const STRESS_ACTIONS: usize = 10_000;
const COMMANDS: usize = 250;

fn actions(count: usize) -> Vec<Action> {
    (0..count)
        .map(|index| Action {
            label: format!("Item {index:05}"),
            desc: format!("Representative application number {index}"),
            action: format!("app:{index}"),
            args: None,
        })
        .collect()
}

fn app(actions: Arc<Vec<Action>>, plugins: PluginManager) -> LauncherApp {
    let ctx = egui::Context::default();
    LauncherApp::new(
        &ctx,
        Arc::clone(&actions),
        actions.len(),
        plugins,
        "actions.json".into(),
        "settings.json".into(),
        Settings::default(),
        None,
        None,
        None,
        None,
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
    )
}

struct CommandBenchPlugin {
    commands: Vec<Action>,
}

impl Plugin for CommandBenchPlugin {
    fn search(&self, _query: &str) -> Vec<Action> {
        Vec::new()
    }

    fn name(&self) -> &str {
        "criterion_commands"
    }

    fn description(&self) -> &str {
        "Criterion command-cache fixture"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        self.commands.clone()
    }
}

fn bench_static_search(c: &mut Criterion) {
    let representative = Arc::new(actions(REPRESENTATIVE_ACTIONS));
    let mut group = c.benchmark_group("search_static/representative_500");
    group.throughput(Throughput::Elements(REPRESENTATIVE_ACTIONS as u64));

    let mut exact = app(Arc::clone(&representative), PluginManager::new());
    let exact_queries = ["app Item 00499", "app Item 00399"];
    let mut iteration = 0;
    group.bench_function("high_specificity_real", |b| {
        b.iter(|| {
            exact.query = exact_queries[iteration % exact_queries.len()].to_owned();
            iteration += 1;
            exact.search();
            black_box(exact.results.len())
        })
    });

    let mut broad = app(Arc::clone(&representative), PluginManager::new());
    let broad_queries = ["app Item 0", "app Item 00"];
    let mut iteration = 0;
    group.bench_function("broad_real", |b| {
        b.iter(|| {
            broad.query = broad_queries[iteration % broad_queries.len()].to_owned();
            iteration += 1;
            broad.search();
            black_box(broad.results.len())
        })
    });

    let mut no_match = app(Arc::clone(&representative), PluginManager::new());
    let no_match_queries = ["app zzz-no-match", "app yyy-no-match"];
    let mut iteration = 0;
    group.bench_function("no_match_real", |b| {
        b.iter(|| {
            no_match.query = no_match_queries[iteration % no_match_queries.len()].to_owned();
            iteration += 1;
            no_match.search();
            black_box(no_match.results.len())
        })
    });
    group.finish();

    let stress = Arc::new(actions(STRESS_ACTIONS));
    let mut group = c.benchmark_group("search_static/stress_10k");
    group.throughput(Throughput::Elements(STRESS_ACTIONS as u64));

    let mut real = app(Arc::clone(&stress), PluginManager::new());
    let real_queries = [
        "app Item 09999",
        "app Item 08999",
        "app Item 07999",
        "app Item 06999",
    ];
    let mut iteration = 0;
    group.bench_function("real_query_cycle", |b| {
        b.iter(|| {
            real.query = real_queries[iteration % real_queries.len()].to_owned();
            iteration += 1;
            real.search();
            black_box(real.results.len())
        })
    });

    let mut cached = app(Arc::clone(&stress), PluginManager::new());
    cached.query = "app Item 09999".to_owned();
    cached.search();
    group.bench_function("cached_repeat", |b| {
        b.iter(|| {
            cached.search();
            black_box(cached.results.len())
        })
    });
    group.finish();
}

fn bench_command_cache(c: &mut Criterion) {
    let commands = actions(COMMANDS)
        .into_iter()
        .map(|mut action| {
            action.label = format!("Command {}", action.label);
            action.action = format!("command:{}", action.action);
            action
        })
        .collect();
    let mut plugins = PluginManager::new();
    plugins.register(Box::new(CommandBenchPlugin { commands }));
    let mut app = app(Arc::new(Vec::new()), plugins);
    let queries = ["Command Item 00249", "Command Item 00149"];
    let mut iteration = 0;

    let mut group = c.benchmark_group("search_command_cache");
    group.throughput(Throughput::Elements(COMMANDS as u64));
    group.bench_function(BenchmarkId::new("lookup_real", COMMANDS), |b| {
        b.iter(|| {
            app.query = queries[iteration % queries.len()].to_owned();
            iteration += 1;
            app.search();
            black_box(app.results.len())
        })
    });
    group.finish();
}

fn loaded_plugins() -> PluginManager {
    let mut plugins = PluginManager::new();
    let generation = plugins.search_generation();
    plugins.reload_from_dirs(
        &[],
        10,
        multi_launcher::settings::NetUnit::Auto,
        false,
        &HashMap::new(),
        Arc::new(Vec::new()),
    );
    let sysinfo = HashSet::from(["sysinfo".to_owned()]);
    plugins.search_filtered("info cpu", Some(&sysinfo), None);
    let started = Instant::now();
    while plugins.search_generation() == generation {
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "system-data cache did not warm"
        );
        std::thread::yield_now();
    }
    plugins
}

fn bench_dynamic_plugins(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_dynamic_cached");
    for (plugin_name, query) in [
        ("processes", "ps multi_launcher"),
        ("sysinfo", "info cpu"),
        ("network", "net"),
        ("volume", "vol name definitely_not_real.exe 20"),
        ("shell", "sh"),
        ("layout", "layout"),
        ("mouse_gestures", "mg"),
        ("missing", "check missing"),
    ] {
        let plugins = loaded_plugins();
        let enabled = HashSet::from([plugin_name.to_owned()]);
        group.bench_function(plugin_name, |b| {
            b.iter(|| black_box(plugins.search_filtered(black_box(query), Some(&enabled), None)))
        });
    }
    let mut browser_plugins = PluginManager::new();
    browser_plugins.register(Box::new(BrowserTabsPlugin::with_cached_tabs_for_benchmark(
        (0..1_000).map(|index| {
            (
                format!("Project documentation {index:04}"),
                format!("https://example.test/docs/{index}"),
                vec![index],
            )
        }),
    )));
    let browser_enabled = HashSet::from(["browser_tabs".to_owned()]);
    group.throughput(Throughput::Elements(1_000));
    group.bench_function("browser_tabs_cached_filter_1000", |b| {
        b.iter(|| {
            black_box(browser_plugins.search_filtered(
                black_box("tab documentation 0420"),
                Some(&browser_enabled),
                None,
            ))
        })
    });
    group.throughput(Throughput::Elements(1));
    group.bench_function("browser_tabs_clear_command", |b| {
        b.iter(|| {
            black_box(browser_plugins.search_filtered(
                black_box("tab clear"),
                Some(&browser_enabled),
                None,
            ))
        })
    });
    group.finish();
}
fn bench_completion(c: &mut Criterion) {
    let commands = actions(COMMANDS);
    let representative = actions(REPRESENTATIVE_ACTIONS);
    let stress = actions(STRESS_ACTIONS);

    let mut group = c.benchmark_group("completion");
    group.bench_function(
        BenchmarkId::new("build_index", REPRESENTATIVE_ACTIONS),
        |b| {
            b.iter(|| {
                black_box(completion::build_index(
                    black_box(&commands),
                    black_box(&representative),
                ))
            })
        },
    );
    group.bench_function(BenchmarkId::new("build_index", STRESS_ACTIONS), |b| {
        b.iter(|| {
            black_box(completion::build_index(
                black_box(&commands),
                black_box(&stress),
            ))
        })
    });

    let index = completion::build_index(&commands, &stress);
    group.bench_function(BenchmarkId::new("suggestion_lookup", STRESS_ACTIONS), |b| {
        b.iter(|| {
            black_box(completion::suggestions(
                black_box(&index),
                black_box("app item 099"),
                5,
            ))
        })
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_static_search,
    bench_command_cache,
    bench_dynamic_plugins,
    bench_completion
);
criterion_main!(benches);
