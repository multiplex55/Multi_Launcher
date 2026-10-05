use criterion::{
    BatchSize, BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main,
};
use multi_launcher::radial::bindings::project_menu_frame;
use multi_launcher::radial::compositor::{CompositorCache, MAX_COMPOSITOR_CACHE_BYTES};
use multi_launcher::radial::dynamic::{
    FrozenAvailability, FrozenBinding, FrozenDynamicFrame, FrozenEntryId, FrozenEntryKind,
    FrozenRadialEntry, SourceFingerprint,
};
use multi_launcher::radial::geometry::{
    LogicalPoint, PhysicalPoint, PhysicalRect, ScaleFactor, layout_menu,
};
use multi_launcher::radial::handoff::InteractionRequirement;
use multi_launcher::radial::model::{CellContent, CellId, DynamicSource, RadialDocument, RingId};
use multi_launcher::radial::render::{build_scene, build_scene_selected, input_owner};
use std::collections::BTreeMap;

const CELL_COUNTS: [usize; 4] = [8, 32, 128, 512];

struct Fixture {
    menu: multi_launcher::radial::model::MenuDefinition,
    projection_menu: multi_launcher::radial::model::MenuDefinition,
    layout: multi_launcher::radial::geometry::LayoutSnapshot,
    scene: multi_launcher::radial::render::VectorScene,
    selected: CellId,
    dynamic: BTreeMap<CellId, FrozenDynamicFrame>,
}

fn fixture(count: usize) -> Fixture {
    let document = RadialDocument::starter();
    let mut menu = document.menus[0].clone();
    let template = menu.rings[0].cells[0].clone();
    let ring_template = menu.rings[0].clone();
    menu.rings = (0..count.div_ceil(128))
        .map(|ring_index| {
            let mut ring = ring_template.clone();
            ring.id = RingId::new(format!("bench-ring-{ring_index}"));
            ring.radius = ring_template.radius + ring_index as f32 * 100.0;
            let begin = ring_index * 128;
            let end = (begin + 128).min(count);
            ring.cells = (begin..end)
                .map(|index| {
                    let mut cell = template.clone();
                    cell.id = CellId::new(format!("bench-{index}"));
                    cell.label = format!("Cell {index}");
                    cell
                })
                .collect();
            ring
        })
        .collect();
    let layout = layout_menu(
        &menu,
        PhysicalPoint { x: 0.0, y: 0.0 },
        PhysicalRect {
            min: PhysicalPoint {
                x: -10_000.0,
                y: -10_000.0,
            },
            max: PhysicalPoint {
                x: 10_000.0,
                y: 10_000.0,
            },
        },
        ScaleFactor::new(1.0).unwrap(),
        0.25,
    )
    .unwrap();
    let scene = build_scene(&layout, 1);
    let selected = menu.rings[0].cells[0].id.clone();

    let source_id = CellId::new("bench-source");
    let mut source = template;
    source.id = source_id.clone();
    source.label = "Dynamic benchmark".into();
    source.content = CellContent::Dynamic {
        source: DynamicSource::Favorites,
    };
    let mut projection_menu = document.menus[0].clone();
    projection_menu.rings[0].cells = vec![source];
    let entries = (0..count)
        .map(|index| FrozenRadialEntry {
            id: FrozenEntryId(format!("entry-{index}")),
            label: format!("Entry {index}"),
            kind: FrozenEntryKind::Action,
            binding: Some(FrozenBinding::Informational),
            availability: FrozenAvailability::Available,
            history_query: String::new(),
            requirement: InteractionRequirement::None,
        })
        .collect();
    let dynamic = BTreeMap::from([(
        source_id,
        FrozenDynamicFrame {
            fingerprint: SourceFingerprint {
                generation: 1,
                source: "bench".into(),
                query: None,
            },
            entries,
        },
    )]);
    Fixture {
        menu,
        projection_menu,
        layout,
        scene,
        selected,
        dynamic,
    }
}

fn radial_runtime(c: &mut Criterion) {
    for count in CELL_COUNTS {
        let fixture = fixture(count);
        let mut group = c.benchmark_group(format!("radial_runtime/{count}"));
        group.throughput(Throughput::Elements(count as u64));

        group.bench_function(BenchmarkId::new("layout", count), |b| {
            b.iter(|| {
                layout_menu(
                    black_box(&fixture.menu),
                    PhysicalPoint { x: 0.0, y: 0.0 },
                    PhysicalRect {
                        min: PhysicalPoint {
                            x: -10_000.0,
                            y: -10_000.0,
                        },
                        max: PhysicalPoint {
                            x: 10_000.0,
                            y: 10_000.0,
                        },
                    },
                    ScaleFactor::new(1.0).unwrap(),
                    0.25,
                )
                .unwrap()
            })
        });
        group.bench_function(BenchmarkId::new("hit", count), |b| {
            b.iter(|| {
                input_owner(
                    black_box(&fixture.layout),
                    black_box(LogicalPoint { x: 0.0, y: 0.0 }),
                    false,
                )
            })
        });
        group.bench_function(BenchmarkId::new("scene", count), |b| {
            b.iter(|| build_scene(black_box(&fixture.layout), black_box(2)))
        });
        group.bench_function(BenchmarkId::new("selection", count), |b| {
            b.iter(|| {
                build_scene_selected(
                    black_box(&fixture.layout),
                    black_box(3),
                    Some(black_box(&fixture.selected)),
                )
            })
        });
        group.bench_function(BenchmarkId::new("static_composition", count), |b| {
            b.iter_batched(
                || {
                    CompositorCache::with_budget_and_observability(
                        MAX_COMPOSITOR_CACHE_BYTES,
                        false,
                    )
                },
                |mut cache| {
                    cache
                        .compose(black_box(&fixture.scene), ScaleFactor::new(1.0).unwrap(), 0)
                        .unwrap()
                },
                BatchSize::SmallInput,
            )
        });
        let mut warmed =
            CompositorCache::with_budget_and_observability(MAX_COMPOSITOR_CACHE_BYTES, false);
        warmed
            .compose(&fixture.scene, ScaleFactor::new(1.0).unwrap(), 0)
            .unwrap();
        group.bench_function(BenchmarkId::new("warmed_composition", count), |b| {
            b.iter(|| {
                warmed
                    .compose(black_box(&fixture.scene), ScaleFactor::new(1.0).unwrap(), 0)
                    .unwrap()
            })
        });
        let last_page = count.saturating_sub(1) / 8;
        let pages = if last_page == 0 {
            vec![0]
        } else {
            vec![0, last_page]
        };
        for page in pages {
            group.bench_function(BenchmarkId::new(format!("page_{page}"), count), |b| {
                b.iter(|| {
                    project_menu_frame(
                        black_box(&fixture.projection_menu),
                        BTreeMap::new(),
                        black_box(&fixture.dynamic),
                        black_box(page),
                        0,
                    )
                })
            });
        }
        group.finish();
    }
}

criterion_group!(benches, radial_runtime);
criterion_main!(benches);
