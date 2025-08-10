use criterion::{criterion_group, criterion_main, Criterion};
use multi_launcher::plugins::macros::search_first_action;

/// Benchmark the cached `PluginManager` lookup used by `search_first_action`.
fn bench_macros_search(c: &mut Criterion) {
    // Warm-up to ensure the `PluginManager` is initialised.
    let _ = search_first_action("help");
    c.bench_function("search_first_action_cached", |b| {
        b.iter(|| {
            let _ = search_first_action("help");
        })
    });
}

criterion_group!(benches, bench_macros_search);
criterion_main!(benches);
