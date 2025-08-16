use criterion::{criterion_group, criterion_main, Criterion};
use fst::{automaton::Subsequence, IntoStreamer, Map, MapBuilder, Streamer};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use multi_launcher::actions::Action;
use std::collections::HashSet;

fn build_index(actions: &[Action]) -> Map<Vec<u8>> {
    let mut entries: Vec<(String, u64)> = Vec::new();
    for (i, a) in actions.iter().enumerate() {
        entries.push((a.label.to_lowercase(), i as u64));
        if !a.desc.is_empty() {
            entries.push((a.desc.to_lowercase(), i as u64));
        }
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let mut builder = MapBuilder::memory();
    for (k, v) in entries {
        builder.insert(k, v).unwrap();
    }
    Map::new(builder.into_inner().unwrap()).unwrap()
}

fn bench_omni_search(c: &mut Criterion) {
    let actions: Vec<Action> = (0..10_000)
        .map(|i| Action {
            label: format!("Item {i}"),
            desc: format!("Description {i}"),
            action: i.to_string(),
            args: None,
        })
        .collect();
    let index = build_index(&actions);
    let matcher = SkimMatcherV2::default();

    c.bench_function("search_linear", |b| {
        b.iter(|| {
            let q = "Item 9999";
            actions
                .iter()
                .filter(|a| {
                    matcher.fuzzy_match(&a.label, q).is_some()
                        || matcher.fuzzy_match(&a.desc, q).is_some()
                })
                .count()
        })
    });

    c.bench_function("search_indexed", |b| {
        b.iter(|| {
            let q = "Item 9999".to_lowercase();
            let automaton = Subsequence::new(&q);
            let mut stream = index.search(automaton).into_stream();
            let mut seen = HashSet::new();
            while let Some((_, idx)) = stream.next() {
                seen.insert(idx);
            }
            seen.len()
        })
    });
}

criterion_group!(benches, bench_omni_search);
criterion_main!(benches);
