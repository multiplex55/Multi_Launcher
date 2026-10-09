//! Deterministic workload fixtures and opt-in, headless benchmark protocol.
//!
//! Ignored workload tests run only with `MULTI_LAUNCHER_PERF=1` set before the
//! test process starts. Set `ML_TRACK_A_BENCH_MODE=small` for minimum-size
//! smoke runs; the default `full` mode uses every documented size.

use crate::actions::Action;
use crate::history::{HistoryEntry, HistoryPin};
use crate::performance::{Metric, MetricSnapshot, reset_metrics, snapshot_metrics};
use crate::plugins::note::Note;
use once_cell::sync::Lazy;
use std::collections::{HashMap, VecDeque};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Instant;
use tempfile::TempDir;

pub const UI_WARMUPS: usize = 5;
pub const INDEX_WARMUPS: usize = 1;
pub const SAMPLE_COUNT: usize = 20;
const NORMAL_NOTE_BYTES: usize = 4 * 1024;
const LONG_NOTE_BYTES: usize = 32 * 1024;
const NOTE_LONG_PERIOD: usize = 29;
const ACTION_LONG_PERIOD: usize = 19;
const FIXTURE_SEED: u64 = 0xA11C_E55E_2026_1009;
static WORKLOAD_ENV_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FixtureSummary {
    pub count: usize,
    pub estimated_bytes: u64,
    pub signature: u64,
    pub output_signature: Option<u64>,
    pub output_order_signature: Option<u64>,
}

impl FixtureSummary {
    pub fn with_output_signatures(mut self, membership: Option<u64>, order: Option<u64>) -> Self {
        self.output_signature = membership;
        self.output_order_signature = order;
        self
    }
}

#[derive(Debug)]
pub struct Fixture<T> {
    pub values: Vec<T>,
    pub summary: FixtureSummary,
}

pub struct HistoryFixture {
    pub entries: VecDeque<HistoryEntry>,
    pub pins: Vec<HistoryPin>,
    pub actions_by_id: HashMap<String, Action>,
    pub catalog_action: Action,
    pub summary: FixtureSummary,
    pub direct_resolved: usize,
    pub catalog_resolved: usize,
    pub missing: usize,
    pub rare_entry_index: usize,
}

/// Holds an isolated current directory and note-specific path overrides.
/// Declare this before any application, worker, or lazy global that can touch
/// paths; later locals then drop before this guard restores paths and cleans up.
pub struct IsolatedWorkspace {
    temp: Option<TempDir>,
    previous_cwd: PathBuf,
    previous_notes_dir: Option<OsString>,
    previous_template_dir: Option<OsString>,
    _lock: MutexGuard<'static, ()>,
}

impl IsolatedWorkspace {
    pub fn new() -> Self {
        let lock = WORKLOAD_ENV_LOCK
            .lock()
            .expect("workload environment lock poisoned");
        let temp = tempfile::tempdir().expect("create isolated workload workspace");
        let root = temp.path().to_path_buf();
        let notes_dir = root.join("notes");
        let templates_dir = root.join("templates");
        fs::create_dir_all(&notes_dir).expect("create isolated notes directory");
        fs::create_dir_all(&templates_dir).expect("create isolated templates directory");
        let previous_cwd = std::env::current_dir().expect("read current directory");
        std::env::set_current_dir(&root).expect("enter isolated workload workspace");
        let previous_notes_dir = std::env::var_os("ML_NOTES_DIR");
        let previous_template_dir = std::env::var_os("ML_NOTE_TEMPLATES_DIR");
        // Environment mutation is process-local in these process-isolated tests.
        unsafe {
            std::env::set_var("ML_NOTES_DIR", &notes_dir);
            std::env::set_var("ML_NOTE_TEMPLATES_DIR", &templates_dir);
        }
        Self {
            temp: Some(temp),
            previous_cwd,
            previous_notes_dir,
            previous_template_dir,
            _lock: lock,
        }
    }

    pub fn root(&self) -> &Path {
        self.temp.as_ref().expect("workspace is alive").path()
    }
}

impl Default for IsolatedWorkspace {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for IsolatedWorkspace {
    fn drop(&mut self) {
        restore_env("ML_NOTES_DIR", self.previous_notes_dir.take());
        restore_env("ML_NOTE_TEMPLATES_DIR", self.previous_template_dir.take());
        let _ = std::env::set_current_dir(&self.previous_cwd);
        drop(self.temp.take());
        // `_lock` is dropped after this method, once paths and temp files are restored.
    }
}

fn restore_env(name: &str, previous: Option<OsString>) {
    // Environment mutation is process-local in these process-isolated tests.
    unsafe {
        if let Some(previous) = previous {
            std::env::set_var(name, previous);
        } else {
            std::env::remove_var(name);
        }
    }
}

#[derive(Default)]
pub struct StableSignature(u64);

impl StableSignature {
    pub fn new(seed: u64, kind: &str, count: usize) -> Self {
        let mut signature = Self(0xcbf2_9ce4_8422_2325);
        signature.bytes(kind.as_bytes());
        signature.number(seed);
        signature.number(count as u64);
        signature
    }

    pub fn bytes(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x100_0000_01b3);
        }
    }

    pub fn number(&mut self, value: u64) {
        self.bytes(&value.to_le_bytes());
    }

    pub fn finish(self) -> u64 {
        self.0
    }
}

fn add_action_bytes(action: &Action) -> usize {
    std::mem::size_of::<Action>()
        .saturating_add(action.label.len())
        .saturating_add(action.desc.len())
        .saturating_add(action.action.len())
        .saturating_add(action.args.as_ref().map_or(0, String::len))
}

fn add_note_bytes(note: &Note) -> usize {
    use crate::common::entity_ref::EntityRef;

    let string_slots = note
        .tags
        .len()
        .saturating_add(note.links.len())
        .saturating_add(note.aliases.len())
        .saturating_mul(std::mem::size_of::<String>());
    let tag_link_alias_bytes = note
        .tags
        .iter()
        .chain(&note.links)
        .chain(&note.aliases)
        .map(String::len)
        .fold(0_usize, usize::saturating_add);
    let entity_bytes = note
        .entity_refs
        .iter()
        .map(|entity| {
            std::mem::size_of::<EntityRef>()
                .saturating_add(entity.id.len())
                .saturating_add(entity.title.as_ref().map_or(0, String::len))
        })
        .fold(0_usize, usize::saturating_add);

    std::mem::size_of::<Note>()
        .saturating_add(note.title.len())
        .saturating_add(note.path.as_os_str().len())
        .saturating_add(note.content.len())
        .saturating_add(note.slug.len())
        .saturating_add(note.alias.as_ref().map_or(0, String::len))
        .saturating_add(string_slots)
        .saturating_add(tag_link_alias_bytes)
        .saturating_add(entity_bytes)
}

/// Build realistic but entirely synthetic note documents with a fixed seed.
pub fn note_fixture(seed: u64, count: usize) -> Fixture<Note> {
    let mut values = Vec::with_capacity(count);
    let mut estimated_bytes = 0_usize;
    let mut signature = StableSignature::new(seed, "notes", count);

    for index in 0..count {
        let id = index as u64;
        let topic = id.wrapping_mul(17).wrapping_add(seed) % 31;
        let target = if count > 1 && index % 17 == 1 {
            Some(0)
        } else if count > 1 && index % 97 == 0 {
            Some((index + 1) % count)
        } else {
            None
        };
        let wrap_title = index % 37 == 36;
        let title = if wrap_title {
            format!(
                "Synthetic note {index:05} · 東京 🙂 — wrapped title {}",
                "layout-segment-λ-".repeat(10)
            )
        } else {
            format!("Synthetic note {index:05} · 東京 🙂")
        };
        let slug = format!("synthetic-note-{index:05}");
        let primary_alias = if wrap_title {
            format!(
                "Synthetic alias {index:05} with a wrapped visible label {}",
                "alias-segment-東京-".repeat(8)
            )
        } else {
            format!("Synthetic alias {index:05}")
        };
        let secondary_alias = format!("Secondary alias {index:05} 東京");
        let mut content = format!(
            "# {title}\nAlias: {primary_alias}\nAliases: {secondary_alias}, Shared alias {}\n\n@topic-{topic} #tag-{topic}\n\n- [ ] synthetic task {index:05}\n- [x] completed task {topic:02}\n\n",
            topic % 7,
        );
        content.push_str(&format!("Marker: track-a-note-{index:05}\n"));
        if let Some(target) = target {
            content.push_str(&format!("[[synthetic-note-{target:05}]]\n"));
        }
        content.push_str(&format!("@todo:todo-{topic:02}\n"));
        if let Some(target) = target {
            content.push_str(&format!("@note:synthetic-note-{target:05}\n"));
        }
        content.push('\n');
        let long_body = index % NOTE_LONG_PERIOD == NOTE_LONG_PERIOD - 1;
        let target_bytes = if long_body {
            LONG_NOTE_BYTES
        } else {
            NORMAL_NOTE_BYTES
        };
        let filler = format!(
            "Synthetic body seed {seed:016x}, section {topic:02}; reusable explanatory prose and wrapped preview text.\n"
        );
        while content.len() < target_bytes {
            content.push_str(&filler);
        }
        content.truncate(target_bytes);

        let tags = vec![format!("tag-{topic}"), format!("topic-{}", topic % 7)];
        let aliases = vec![primary_alias.clone(), secondary_alias.clone()];
        let mut note = Note {
            title,
            path: PathBuf::from(format!("synthetic-notes/{slug}.md")),
            content,
            tags,
            links: target
                .map(|target| vec![format!("synthetic-note-{target:05}")])
                .unwrap_or_default(),
            slug: slug.clone(),
            alias: Some(primary_alias),
            aliases,
            entity_refs: vec![crate::common::entity_ref::EntityRef::new(
                crate::common::entity_ref::EntityKind::Todo,
                format!("todo-{topic:02}"),
                Some(format!("Synthetic todo {topic:02}")),
            )],
        };
        if let Some(target) = target {
            note.entity_refs
                .push(crate::common::entity_ref::EntityRef::new(
                    crate::common::entity_ref::EntityKind::Note,
                    format!("synthetic-note-{target:05}"),
                    None,
                ));
        }
        estimated_bytes = estimated_bytes.saturating_add(add_note_bytes(&note));
        signature.bytes(slug.as_bytes());
        signature.bytes(note.title.as_bytes());
        signature.bytes(note.content.as_bytes());
        for value in note.tags.iter().chain(&note.aliases).chain(&note.links) {
            signature.bytes(value.as_bytes());
        }
        signature.number(target.map_or(u64::MAX, |target| target as u64));
        signature.number(note.entity_refs.len() as u64);
        values.push(note);
    }

    Fixture {
        summary: FixtureSummary {
            count,
            estimated_bytes: estimated_bytes as u64,
            signature: signature.finish(),
            output_signature: None,
            output_order_signature: None,
        },
        values,
    }
}

/// Build launcher results with occasional long labels/descriptions for wrapping.
pub fn action_fixture(seed: u64, count: usize) -> Fixture<Action> {
    let mut values = Vec::with_capacity(count);
    let mut estimated_bytes = 0_usize;
    let mut signature = StableSignature::new(seed, "actions", count);

    for index in 0..count {
        let id = format!("synthetic:action:{index:05}");
        let long = index % ACTION_LONG_PERIOD == ACTION_LONG_PERIOD - 1;
        let label = if long {
            format!(
                "Synthetic action {index:05} — long wrapped label {}",
                "label-segment-東京-".repeat(12)
            )
        } else {
            format!("Synthetic action {index:05} — 東京")
        };
        let desc = if long {
            format!(
                "Synthetic description for layout wrapping: {}",
                "description-segment-λ-".repeat(24)
            )
        } else {
            format!("Synthetic action description group {:02}", index % 23)
        };
        let args = (index % 8 == 3).then(|| format!("{{\"fixture\":{index}}}"));
        let action = Action {
            label,
            desc,
            action: id.clone(),
            args,
        };
        estimated_bytes = estimated_bytes.saturating_add(add_action_bytes(&action));
        signature.bytes(id.as_bytes());
        signature.bytes(action.label.as_bytes());
        signature.bytes(action.desc.as_bytes());
        signature.bytes(action.args.as_deref().unwrap_or_default().as_bytes());
        signature.number(long as u64);
        values.push(action);
    }

    Fixture {
        summary: FixtureSummary {
            count,
            estimated_bytes: estimated_bytes as u64,
            signature: signature.finish(),
            output_signature: None,
            output_order_signature: None,
        },
        values,
    }
}

/// Build fixed-time history rows spanning renamed, catalog-resolved and missing actions.
pub fn history_fixture(seed: u64, count: usize) -> HistoryFixture {
    let mut entries = VecDeque::with_capacity(count);
    let mut actions_by_id = HashMap::with_capacity(count / 2 + 1);
    let catalog_action = Action {
        label: "Synthetic catalog action (current label)".into(),
        desc: "Synthetic plugin command".into(),
        action: "synthetic:plugin-catalog".into(),
        args: None,
    };
    let rare_entry_index = if count > 8 {
        count * 3 / 4
    } else {
        count.saturating_sub(1)
    };
    let mut direct_resolved = 0;
    let mut catalog_resolved = 0;
    let mut missing = 0;
    let mut estimated_bytes = 0_usize;
    let mut signature = StableSignature::new(seed, "history", count);

    for index in 0..count {
        let identity = match index % 4 {
            0 => {
                let action_id = format!("synthetic:current:{index:05}");
                let current = Action {
                    label: format!("Renamed current action {index:05}"),
                    desc: "Synthetic current command".into(),
                    action: action_id.clone(),
                    args: (index % 8 == 0).then(|| format!("{{\"arg\":{index}}}")),
                };
                actions_by_id.insert(action_id.clone(), current);
                direct_resolved += 1;
                action_id
            }
            1 => {
                catalog_resolved += 1;
                catalog_action.action.clone()
            }
            _ => {
                missing += 1;
                format!("synthetic:deleted:{index:05}")
            }
        };
        let args = match index % 8 {
            0 => Some(format!("{{\"arg\":{index}}}")),
            4 => Some(format!("{{\"missing\":{index}}}")),
            _ => None,
        };
        let rare = index == rare_entry_index;
        let query = if rare {
            "rare-query-synthetic".to_owned()
        } else {
            format!("synthetic query group {:02}", (index * 13) % 29)
        };
        let saved_label = format!("Saved synthetic action {index:05}");
        let entry = HistoryEntry {
            query: query.clone(),
            query_lc: query.to_lowercase(),
            action: Action {
                label: saved_label,
                desc: "Saved synthetic presentation".into(),
                action: identity.clone(),
                args,
            },
            source: Some("track-a-fixture".into()),
            timestamp: 1_700_000_000 + (count - index) as i64,
        };
        estimated_bytes = estimated_bytes
            .saturating_add(std::mem::size_of::<HistoryEntry>())
            .saturating_add(entry.query.len())
            .saturating_add(entry.query_lc.len())
            .saturating_add(entry.action.label.len())
            .saturating_add(entry.action.desc.len())
            .saturating_add(entry.action.action.len())
            .saturating_add(entry.action.args.as_ref().map_or(0, String::len));
        signature.bytes(identity.as_bytes());
        signature.bytes(query.as_bytes());
        signature.bytes(entry.action.label.as_bytes());
        signature.bytes(entry.action.desc.as_bytes());
        signature.bytes(entry.action.args.as_deref().unwrap_or_default().as_bytes());
        signature.number(entry.timestamp as u64);
        entries.push_back(entry);
    }

    let pins = entries
        .iter()
        .enumerate()
        .filter(|(index, _)| index % 13 == 0)
        .map(|(_, entry)| HistoryPin::from_history(entry))
        .collect::<Vec<_>>();
    estimated_bytes = estimated_bytes.saturating_add(
        pins.iter()
            .map(|pin| {
                std::mem::size_of::<HistoryPin>()
                    .saturating_add(pin.action_id.len())
                    .saturating_add(pin.label.len())
                    .saturating_add(pin.desc.len())
                    .saturating_add(pin.args.as_ref().map_or(0, String::len))
                    .saturating_add(pin.query.len())
            })
            .fold(0_usize, usize::saturating_add),
    );
    estimated_bytes = estimated_bytes.saturating_add(
        actions_by_id
            .iter()
            .map(|(id, action)| id.len().saturating_add(add_action_bytes(action)))
            .fold(0_usize, usize::saturating_add),
    );
    estimated_bytes = estimated_bytes.saturating_add(add_action_bytes(&catalog_action));

    HistoryFixture {
        entries,
        pins,
        actions_by_id,
        catalog_action,
        summary: FixtureSummary {
            count,
            estimated_bytes: estimated_bytes as u64,
            signature: signature.finish(),
            output_signature: None,
            output_order_signature: None,
        },
        direct_resolved,
        catalog_resolved,
        missing,
        rare_entry_index,
    }
}

/// Create a deterministic nested tree of small synthetic files and return a
/// summary that contains no absolute path or file contents.
pub fn create_index_tree(root: &Path, seed: u64, count: usize) -> FixtureSummary {
    fs::create_dir_all(root).expect("create synthetic index root");
    const FILES_PER_LEAF: usize = 64;
    let mut estimated_bytes = 0_usize;
    let mut signature = StableSignature::new(seed, "index-tree", count);
    let mut current_leaf = usize::MAX;

    for index in 0..count {
        let leaf = index / FILES_PER_LEAF;
        if leaf != current_leaf {
            let directory = root
                .join(format!("shard-{:03}", leaf / 16))
                .join(format!("group-{leaf:04}"));
            fs::create_dir_all(directory).expect("create nested synthetic index directory");
            current_leaf = leaf;
        }
        let relative = PathBuf::from(format!(
            "shard-{:03}/group-{leaf:04}/file-{index:05}.txt",
            leaf / 16
        ));
        let content =
            format!("Synthetic index fixture {index:05}; seed {seed:016x}; nested file payload.\n");
        fs::write(root.join(&relative), content.as_bytes()).expect("write synthetic index file");
        estimated_bytes = estimated_bytes
            .saturating_add(relative.as_os_str().len())
            .saturating_add(content.len());
        signature.bytes(relative.to_string_lossy().as_bytes());
        signature.number(content.len() as u64);
    }

    FixtureSummary {
        count,
        estimated_bytes: estimated_bytes as u64,
        signature: signature.finish(),
        output_signature: None,
        output_order_signature: None,
    }
}

/// Stable identity for a completed index traversal, independent of filesystem order.
pub fn index_actions_signature(root: &Path, actions: &[Action]) -> u64 {
    let canonical_root = fs::canonicalize(root).expect("canonicalize synthetic index root");
    let mut relative_paths = actions
        .iter()
        .filter_map(|action| {
            Path::new(&action.action)
                .strip_prefix(&canonical_root)
                .ok()
                .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        })
        .collect::<Vec<_>>();
    relative_paths.sort_unstable();
    let mut signature = StableSignature::new(FIXTURE_SEED, "indexed-actions", actions.len());
    for relative in relative_paths {
        signature.bytes(relative.as_bytes());
    }
    signature.finish()
}

pub fn index_actions_order_signature(root: &Path, actions: &[Action]) -> u64 {
    let canonical_root = fs::canonicalize(root).expect("canonicalize synthetic index root");
    let mut signature = StableSignature::new(FIXTURE_SEED, "indexed-action-order", actions.len());
    for action in actions {
        if let Ok(relative) = Path::new(&action.action).strip_prefix(&canonical_root) {
            signature.bytes(relative.to_string_lossy().replace('\\', "/").as_bytes());
        }
    }
    signature.finish()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimingSummary {
    pub warmups: usize,
    pub samples: usize,
    pub p50_nanos: u64,
    pub p95_nanos: u64,
    pub max_nanos: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RootListGeometrySummary {
    pub cold_rebuild_count: u64,
    pub cold_rows_measured: u64,
    /// Duration of the last cold geometry rebuild; this is one observation,
    /// not a percentile distribution.
    pub last_cold_rebuild_nanos: u64,
    pub warm_rebuild_count: u64,
    pub warm_rows_measured: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RootGridGeometrySummary {
    pub cold_rebuild_count: u64,
    pub cold_cells_measured: u64,
    /// Duration of the last cold grid geometry rebuild; this is one
    /// observation, not a percentile distribution.
    pub last_cold_rebuild_nanos: u64,
    pub warm_rebuild_count: u64,
    pub warm_cells_measured: u64,
}

fn nearest_rank(sorted: &[u64; SAMPLE_COUNT], percentile: usize) -> u64 {
    let rank = (sorted.len() * percentile).div_ceil(100).max(1);
    sorted[rank - 1]
}

/// Run fixed warmups, reset global metrics at the quiescent boundary, then time
/// exactly 20 target operations. Setup executes outside each measured interval.
pub fn measure<FSetup, FOperation, T>(
    warmups: usize,
    mut setup: FSetup,
    mut operation: FOperation,
) -> (TimingSummary, T)
where
    FSetup: FnMut(bool, usize),
    FOperation: FnMut() -> T,
{
    let mut unit = ();
    measure_with_state(
        &mut unit,
        warmups,
        |_, warmup, iteration| setup(warmup, iteration),
        |_| operation(),
    )
}

/// Measure an operation against mutable owner state while running fixture
/// re-arming through the same borrow before, but outside, every timed interval.
pub fn measure_with_state<State, FSetup, FOperation, T>(
    state: &mut State,
    warmups: usize,
    mut setup: FSetup,
    mut operation: FOperation,
) -> (TimingSummary, T)
where
    FSetup: FnMut(&mut State, bool, usize),
    FOperation: FnMut(&mut State) -> T,
{
    assert!(
        crate::performance::enabled(),
        "set MULTI_LAUNCHER_PERF=1 before starting this test process"
    );
    for iteration in 0..warmups {
        let before_setup = snapshot_metrics();
        setup(state, true, iteration);
        assert_eq!(
            snapshot_metrics(),
            before_setup,
            "benchmark warmup setup must not invoke instrumented work"
        );
        std::hint::black_box(operation(state));
    }
    reset_metrics();

    let mut samples = [0_u64; SAMPLE_COUNT];
    let mut last_output = None;
    for (iteration, elapsed) in samples.iter_mut().enumerate() {
        let before_setup = snapshot_metrics();
        setup(state, false, iteration);
        assert_eq!(
            snapshot_metrics(),
            before_setup,
            "benchmark sample setup must not invoke instrumented work"
        );
        let started = Instant::now();
        let output = operation(state);
        *elapsed = started.elapsed().as_nanos().min(u64::MAX as u128) as u64;
        last_output = Some(std::hint::black_box(output));
    }
    samples.sort_unstable();
    (
        TimingSummary {
            warmups,
            samples: SAMPLE_COUNT,
            p50_nanos: nearest_rank(&samples, 50),
            p95_nanos: nearest_rank(&samples, 95),
            max_nanos: samples[SAMPLE_COUNT - 1],
        },
        last_output.expect("the fixed benchmark sample count is nonzero"),
    )
}

pub fn selected_sizes(sizes: &[usize]) -> Vec<usize> {
    if std::env::var("ML_TRACK_A_BENCH_MODE").as_deref() == Ok("small") {
        sizes.first().copied().into_iter().collect()
    } else {
        sizes.to_vec()
    }
}

pub fn metrics_for(metrics: &[Metric]) -> Vec<MetricSnapshot> {
    snapshot_metrics()
        .into_iter()
        .filter(|snapshot| metrics.contains(&snapshot.metric))
        .collect()
}

/// Emit exactly one bounded machine-readable summary for one scenario.
pub fn emit_summary(
    scenario: &str,
    scope: &str,
    fixture: FixtureSummary,
    timing: TimingSummary,
    metrics: &[MetricSnapshot],
) {
    emit_summary_with_root_list_geometry(scenario, scope, fixture, timing, metrics, None);
}

pub fn emit_summary_with_root_list_geometry(
    scenario: &str,
    scope: &str,
    fixture: FixtureSummary,
    timing: TimingSummary,
    metrics: &[MetricSnapshot],
    root_list_geometry: Option<RootListGeometrySummary>,
) {
    emit_summary_with_root_geometries(
        scenario,
        scope,
        fixture,
        timing,
        metrics,
        root_list_geometry,
        None,
    );
}

pub fn emit_summary_with_root_geometries(
    scenario: &str,
    scope: &str,
    fixture: FixtureSummary,
    timing: TimingSummary,
    metrics: &[MetricSnapshot],
    root_list_geometry: Option<RootListGeometrySummary>,
    root_grid_geometry: Option<RootGridGeometrySummary>,
) {
    let metrics = metrics
        .iter()
        .map(|snapshot| {
            serde_json::json!({
                "metric": snapshot.metric.name(),
                "work_unit": snapshot.metric.work_unit_name(),
                "calls": snapshot.calls,
                "work_units": snapshot.work_units,
                "elapsed_nanos_total": snapshot.elapsed_nanos_total,
                "elapsed_nanos_max": snapshot.elapsed_nanos_max,
                "lock_wait_nanos_total": snapshot.lock_wait_nanos_total,
                "lock_wait_nanos_max": snapshot.lock_wait_nanos_max,
                "completed": snapshot.completed,
                "errors": snapshot.errors,
                "abandoned": snapshot.abandoned,
            })
        })
        .collect::<Vec<_>>();
    let profile = if cfg!(debug_assertions) {
        "debug-test"
    } else {
        "non-debug-test"
    };
    println!(
        "TRACK_A_WORKLOAD {}",
        serde_json::json!({
            "scenario": scenario,
            "scope": scope,
            "profile": profile,
            "os": std::env::consts::OS,
            "warmups": timing.warmups,
            "samples": timing.samples,
            "p50_nanos": timing.p50_nanos,
            "p95_nanos": timing.p95_nanos,
            "max_nanos": timing.max_nanos,
            "fixture_count": fixture.count,
            "estimated_fixture_bytes": fixture.estimated_bytes,
            "fixture_signature": format!("{:016x}", fixture.signature),
            "output_signature": fixture.output_signature.map(|value| format!("{value:016x}")),
            "output_order_signature": fixture.output_order_signature.map(|value| format!("{value:016x}")),
            "root_list_geometry": root_list_geometry.map(|geometry| serde_json::json!({
                "cold_rebuild_count": geometry.cold_rebuild_count,
                "cold_rows_measured": geometry.cold_rows_measured,
                "last_cold_rebuild_nanos": geometry.last_cold_rebuild_nanos,
                "warm_rebuild_count": geometry.warm_rebuild_count,
                "warm_rows_measured": geometry.warm_rows_measured,
            })),
            "root_grid_geometry": root_grid_geometry.map(|geometry| serde_json::json!({
                "cold_rebuild_count": geometry.cold_rebuild_count,
                "cold_cells_measured": geometry.cold_cells_measured,
                "last_cold_rebuild_nanos": geometry.last_cold_rebuild_nanos,
                "warm_rebuild_count": geometry.warm_rebuild_count,
                "warm_cells_measured": geometry.warm_cells_measured,
            })),
            "metrics": metrics,
        })
    );
}

#[cfg(test)]
mod tests {
    use super::{action_fixture, history_fixture, note_fixture};

    #[test]
    fn track_a_fixture_builders_are_deterministic_and_cover_edge_shapes() {
        for count in [100, 1_000, 5_000] {
            let first = note_fixture(17, count);
            let second = note_fixture(17, count);
            assert_eq!(first.summary, second.summary);
            assert_eq!(first.values.len(), count);
            assert!(
                first
                    .values
                    .iter()
                    .all(|note| note.content.len() >= 4 * 1024)
            );
            assert!(first.values.iter().any(|note| note.aliases.len() >= 2));
            assert!(first.values.iter().any(|note| !note.entity_refs.is_empty()));
            assert!(
                first
                    .values
                    .iter()
                    .any(|note| note.content.contains("- [ ]"))
            );
            assert!(
                first
                    .values
                    .iter()
                    .any(|note| note.content.contains("東京"))
            );
        }

        for count in [100, 1_000, 10_000] {
            let first = action_fixture(23, count);
            let second = action_fixture(23, count);
            assert_eq!(first.summary, second.summary);
            assert_eq!(first.values.len(), count);
            assert!(first.values.iter().any(|action| action.label.len() > 100));
            assert!(first.values.iter().any(|action| action.desc.len() > 300));

            let first_history = history_fixture(31, count);
            let second_history = history_fixture(31, count);
            assert_eq!(first_history.summary, second_history.summary);
            assert_eq!(first_history.entries.len(), count);
            assert!(first_history.direct_resolved > 0);
            assert!(first_history.catalog_resolved > 0);
            assert!(first_history.missing > 0);
            assert!(first_history.rare_entry_index > 8);
            assert!(!first_history.pins.is_empty());
        }
    }
}
