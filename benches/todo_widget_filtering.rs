use criterion::{black_box, criterion_group, criterion_main, Criterion};
use multi_launcher::plugins::todo::TodoEntry;

fn build_entries(count: usize) -> Vec<TodoEntry> {
    (0..count)
        .map(|i| TodoEntry {
            id: String::new(),
            text: format!("Todo item {i:05} with mixed CASE"),
            done: i % 3 == 0,
            priority: (i % 10) as u8,
            tags: vec![
                format!("team{}", i % 8),
                format!("Feature{}", i % 16),
                "Urgent".into(),
            ],
            entity_refs: Vec::new(),
        })
        .collect()
}

fn old_tags_match(filter_tags: &[String], entry: &TodoEntry) -> bool {
    if filter_tags.is_empty() {
        return true;
    }
    filter_tags.iter().any(|tag| {
        let filter = tag.to_lowercase();
        entry
            .tags
            .iter()
            .any(|t| t.to_lowercase().contains(&filter))
    })
}

fn new_tags_match(normalized_filter_tags: &[String], entry: &TodoEntry) -> bool {
    if normalized_filter_tags.is_empty() {
        return true;
    }
    normalized_filter_tags.iter().any(|filter| {
        entry
            .tags
            .iter()
            .any(|tag| tag.eq_ignore_ascii_case(filter) || tag.to_lowercase().contains(filter))
    })
}

fn old_sort_entries(entries: &mut Vec<(usize, TodoEntry)>) {
    entries.sort_by(|a, b| {
        a.1.text
            .to_lowercase()
            .cmp(&b.1.text.to_lowercase())
            .then_with(|| a.0.cmp(&b.0))
    });
}

fn new_sort_entries(entries: &mut Vec<(usize, TodoEntry)>) {
    let mut keyed: Vec<(String, usize, TodoEntry)> = entries
        .drain(..)
        .map(|(idx, entry)| (entry.text.to_lowercase(), idx, entry))
        .collect();
    keyed.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    entries.extend(keyed.into_iter().map(|(_, idx, entry)| (idx, entry)));
}

fn bench_todo_filter_and_sort(c: &mut Criterion) {
    let todos = build_entries(25_000);
    let filter_tags = vec![
        "urgent".to_string(),
        "feature1".to_string(),
        "team7".to_string(),
    ];
    let normalized_filter_tags: Vec<String> =
        filter_tags.iter().map(|t| t.to_lowercase()).collect();

    c.bench_function("todo_filter_old", |b| {
        b.iter(|| {
            let count = todos
                .iter()
                .filter(|entry| old_tags_match(black_box(&filter_tags), entry))
                .count();
            black_box(count)
        })
    });

    c.bench_function("todo_filter_new", |b| {
        b.iter(|| {
            let count = todos
                .iter()
                .filter(|entry| new_tags_match(black_box(&normalized_filter_tags), entry))
                .count();
            black_box(count)
        })
    });

    c.bench_function("todo_sort_old", |b| {
        b.iter(|| {
            let mut entries: Vec<(usize, TodoEntry)> = todos.iter().cloned().enumerate().collect();
            old_sort_entries(&mut entries);
            black_box(entries.len())
        })
    });

    c.bench_function("todo_sort_new", |b| {
        b.iter(|| {
            let mut entries: Vec<(usize, TodoEntry)> = todos.iter().cloned().enumerate().collect();
            new_sort_entries(&mut entries);
            black_box(entries.len())
        })
    });
}

criterion_group!(benches, bench_todo_filter_and_sort);
criterion_main!(benches);
