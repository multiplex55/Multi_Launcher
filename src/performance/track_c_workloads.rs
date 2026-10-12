//! Test-only fixture identity, isolation, and fixed-sample protocol for Track C.
//!
//! This module intentionally leaves the historical Track A `workloads` API and
//! its environment switches unchanged. Track C samples use their own seed
//! domain and emit sanitized numeric evidence only.

use super::workloads::{self, Fixture, IsolatedWorkspace};
use crate::actions::Action;
use crate::plugins::note::Note;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Instant;

pub use super::workloads::StableSignature;

pub const WARMUPS: usize = 5;
pub const MEASURED_SAMPLES: usize = 20;

const TRACK_C_SEED_DOMAIN: u64 = 0x435F_574F_524B_4C44;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BenchmarkMode {
    Small,
    Full,
}

impl BenchmarkMode {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "small" => Ok(Self::Small),
            "full" => Ok(Self::Full),
            _ => Err(format!(
                "ML_TRACK_C_BENCH_MODE must be `small` or `full`, got `{value}`"
            )),
        }
    }

    pub fn from_process_env() -> Result<Self, String> {
        std::env::var("ML_TRACK_C_BENCH_MODE").map_or(Ok(Self::Full), |value| Self::parse(&value))
    }

    pub fn note_sizes(self) -> &'static [usize] {
        match self {
            Self::Small => &[100],
            Self::Full => &[100, 1_000, 5_000],
        }
    }

    pub fn action_sizes(self) -> &'static [usize] {
        match self {
            Self::Small => &[100],
            Self::Full => &[100, 1_000, 10_000],
        }
    }

    pub fn combined_action_sizes(self) -> &'static [usize] {
        match self {
            Self::Small => &[200],
            Self::Full => &[20_000],
        }
    }

    pub fn index_sizes(self) -> &'static [usize] {
        match self {
            Self::Small => &[16],
            Self::Full => &[16, 1_000, 10_000],
        }
    }
}

/// Owned process-local paths and CWD for a Track C test. Construct this before
/// any app, plugin, coordinator, or worker. Its environment is restored before
/// the wrapped workspace removes its temporary directory.
pub struct TrackCWorkspace {
    workspace: IsolatedWorkspace,
    previous_skip_clipboard_sync: Option<OsString>,
}

impl TrackCWorkspace {
    pub fn new() -> Self {
        let workspace = IsolatedWorkspace::new();
        let previous_skip_clipboard_sync = std::env::var_os("ML_SKIP_CLIPBOARD_SYNC");
        // The supported isolation switch is also set in the process command;
        // setting it here protects owner construction if a test is run alone.
        unsafe { std::env::set_var("ML_SKIP_CLIPBOARD_SYNC", "1") };
        Self {
            workspace,
            previous_skip_clipboard_sync,
        }
    }

    pub fn root(&self) -> &Path {
        self.workspace.root()
    }

    pub fn notes_dir(&self) -> PathBuf {
        self.root().join("notes")
    }

    pub fn templates_dir(&self) -> PathBuf {
        self.root().join("templates")
    }

    pub fn actions_path(&self) -> PathBuf {
        self.root().join("actions.json")
    }

    pub fn settings_path(&self) -> PathBuf {
        self.root().join("settings.json")
    }
}

impl Default for TrackCWorkspace {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TrackCWorkspace {
    fn drop(&mut self) {
        // `workspace` is dropped after this method, restoring CWD and cleaning
        // the owned TempDir only after the skip switch has been restored.
        unsafe {
            if let Some(value) = self.previous_skip_clipboard_sync.take() {
                std::env::set_var("ML_SKIP_CLIPBOARD_SYNC", value);
            } else {
                std::env::remove_var("ML_SKIP_CLIPBOARD_SYNC");
            }
        }
    }
}

fn domain_seed(kind: &'static str) -> u64 {
    let mut signature = StableSignature::new(TRACK_C_SEED_DOMAIN, "track-c-fixture-seed", 1);
    signature.bytes(kind.as_bytes());
    signature.finish()
}

pub fn note_fixture(count: usize) -> Fixture<Note> {
    let mut fixture = workloads::note_fixture(domain_seed("note"), count);
    fixture.summary.signature = note_fixture_identity(&fixture.values);
    fixture
}

pub fn action_fixture(count: usize) -> Fixture<Action> {
    let mut fixture = workloads::action_fixture(domain_seed("action"), count);
    fixture.summary.signature = action_fixture_identity(&fixture.values);
    fixture
}

pub fn index_tree(root: &Path, count: usize) -> workloads::FixtureSummary {
    let seed = domain_seed("index-tree");
    let mut summary = workloads::create_index_tree(root, seed, count);
    // Recompute under the Track C name domain so Track A and Track C fixture
    // signatures cannot be confused in reports.
    let mut signature = StableSignature::new(seed, "track-c-index-tree", count);
    for index in 0..count {
        let leaf = index / 64;
        let relative = format!("shard-{:03}/group-{leaf:04}/file-{index:05}.txt", leaf / 16);
        signature_string(&mut signature, &relative);
        signature_string(
            &mut signature,
            &format!(
                "Synthetic index fixture {index:05}; seed {seed:016x}; nested file payload.\n"
            ),
        );
    }
    summary.signature = signature.finish();
    summary
}

pub fn note_fixture_identity(notes: &[Note]) -> u64 {
    let mut signature =
        StableSignature::new(TRACK_C_SEED_DOMAIN, "track-c-note-state", notes.len());
    for note in notes {
        signature_string(&mut signature, &note.title);
        signature_string(&mut signature, &note.path.to_string_lossy());
        signature_string(&mut signature, &note.content);
        signature.number(note.tags.len() as u64);
        for tag in &note.tags {
            signature_string(&mut signature, tag);
        }
        signature.number(note.links.len() as u64);
        for link in &note.links {
            signature_string(&mut signature, link);
        }
        signature_string(&mut signature, &note.slug);
        signature_option(&mut signature, note.alias.as_deref());
        signature.number(note.aliases.len() as u64);
        for alias in &note.aliases {
            signature_string(&mut signature, alias);
        }
        signature.number(note.entity_refs.len() as u64);
        for reference in &note.entity_refs {
            signature_string(&mut signature, &format!("{:?}", reference.kind));
            signature_string(&mut signature, &reference.id);
            signature_option(&mut signature, reference.title.as_deref());
        }
    }
    signature.finish()
}

pub fn action_fixture_identity(actions: &[Action]) -> u64 {
    let mut signature =
        StableSignature::new(TRACK_C_SEED_DOMAIN, "track-c-action-state", actions.len());
    for action in actions {
        signature_string(&mut signature, &action.label);
        signature_string(&mut signature, &action.desc);
        signature_string(&mut signature, &action.action);
        signature_option(&mut signature, action.args.as_deref());
    }
    signature.finish()
}

fn signature_option(signature: &mut StableSignature, value: Option<&str>) {
    match value {
        Some(value) => {
            signature.number(1);
            signature_string(signature, value);
        }
        None => signature.number(0),
    }
}

fn signature_string(signature: &mut StableSignature, value: &str) {
    signature.number(value.len() as u64);
    signature.bytes(value.as_bytes());
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnerObservation {
    /// Identity of complete output and state, including per-sample revisions.
    pub output_identity: u64,
    /// Stable structural output identity, separately retained from revisions.
    pub structural_signature: u64,
    /// Raw revisions/counters expected for this individual sample.
    pub revision_receipts: Vec<u64>,
    /// Optional receipt of only the visible viewport; never substitutes for
    /// `output_identity`.
    pub viewport_receipt: Option<u64>,
    pub work_units: u64,
    /// Named by each owner-specific section in the workload report; values are
    /// preserved per sample instead of being folded into an unexplained sum.
    pub work_counters: Vec<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SampleRecord {
    pub elapsed_nanos: u64,
    pub output_identity: u64,
    pub structural_signature: u64,
    pub revision_receipts: Vec<u64>,
    pub viewport_receipt: Option<u64>,
    pub work_units: u64,
    pub work_counters: Vec<u64>,
}

/// Runs the fixed protocol. `make_state` performs all fixture/setup/cold
/// invalidation before timing; `validate` receives mutable state after the
/// timer stops and must compare the complete owner result with its independent
/// oracle. It may perform required untimed finalization after capturing that
/// pre-finalization observation and any associated receipts.
pub fn measure_owner<S, O>(
    mut make_state: impl FnMut() -> S,
    mut run_owner: impl FnMut(&mut S) -> O,
    mut validate: impl FnMut(&mut S, &O) -> OwnerObservation,
) -> Vec<SampleRecord> {
    for _ in 0..WARMUPS {
        let mut state = make_state();
        let output = run_owner(&mut state);
        let _ = validate(&mut state, &output);
    }

    let mut samples = Vec::with_capacity(MEASURED_SAMPLES);
    for _ in 0..MEASURED_SAMPLES {
        let mut state = make_state();
        let started = Instant::now();
        let output = run_owner(&mut state);
        let elapsed_nanos = started.elapsed().as_nanos().min(u64::MAX as u128) as u64;
        let observation = validate(&mut state, &output);
        samples.push(SampleRecord {
            elapsed_nanos,
            output_identity: observation.output_identity,
            structural_signature: observation.structural_signature,
            revision_receipts: observation.revision_receipts,
            viewport_receipt: observation.viewport_receipt,
            work_units: observation.work_units,
            work_counters: observation.work_counters,
        });
    }
    samples
}

pub fn nearest_rank(samples: &[u64], percentile: usize) -> u64 {
    assert!(!samples.is_empty(), "quantiles require at least one sample");
    assert!(
        (1..=100).contains(&percentile),
        "percentile must be 1..=100"
    );
    let mut ordered = samples.to_vec();
    ordered.sort_unstable();
    let rank = percentile.saturating_mul(ordered.len()).saturating_add(99) / 100;
    ordered[rank.saturating_sub(1)]
}

#[derive(Clone, Copy, Debug)]
pub struct ReportMetadata {
    pub owner: &'static str,
    pub fixture_name: &'static str,
    pub fixture_signature: u64,
    pub item_count: usize,
    pub viewport: &'static str,
    pub scale_milli: u32,
    pub font_state: &'static str,
    pub settings: &'static str,
    pub cold_type: &'static str,
    pub mode: BenchmarkMode,
}

/// Emits only static labels and deterministic/numeric fields. Source provenance
/// is injected by the parent process; uncommitted harness code is identified
/// truthfully when no source SHA was supplied.
pub fn emit_samples(metadata: ReportMetadata, samples: &[SampleRecord]) {
    assert_eq!(samples.len(), MEASURED_SAMPLES);
    for (field, value) in [
        ("owner", metadata.owner),
        ("fixture", metadata.fixture_name),
        ("viewport", metadata.viewport),
        ("fonts", metadata.font_state),
        ("settings", metadata.settings),
        ("cold_type", metadata.cold_type),
    ] {
        assert!(
            !value.bytes().any(|byte| byte.is_ascii_whitespace()),
            "Track C {field} label must be one token"
        );
    }
    assert_eq!(
        BenchmarkMode::from_process_env().expect("invalid Track C benchmark mode"),
        metadata.mode,
        "report mode must be the process's effective mode"
    );
    let elapsed = samples
        .iter()
        .map(|sample| sample.elapsed_nanos)
        .collect::<Vec<_>>();
    let output_ids = samples
        .iter()
        .map(|sample| format!("{:016x}", sample.output_identity))
        .collect::<Vec<_>>();
    let structure_ids = samples
        .iter()
        .map(|sample| format!("{:016x}", sample.structural_signature))
        .collect::<Vec<_>>();
    let revision_receipts = samples
        .iter()
        .map(|sample| {
            sample
                .revision_receipts
                .iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join(":")
        })
        .collect::<Vec<_>>();
    let viewport_receipts = samples
        .iter()
        .map(|sample| {
            sample
                .viewport_receipt
                .map_or_else(|| "none".into(), |v| format!("{v:016x}"))
        })
        .collect::<Vec<_>>();
    let work_units = samples
        .iter()
        .map(|sample| sample.work_units.to_string())
        .collect::<Vec<_>>();
    let work_counters = samples
        .iter()
        .map(|sample| {
            sample
                .work_counters
                .iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join(":")
        })
        .collect::<Vec<_>>();
    let elapsed_text = elapsed.iter().map(u64::to_string).collect::<Vec<_>>();
    let source =
        std::env::var("ML_TRACK_C_SOURCE_SHA").unwrap_or_else(|_| "UNCOMMITTED_SOURCE".into());
    let profile =
        std::env::var("ML_TRACK_C_PROFILE").unwrap_or_else(|_| "unspecified-profile".into());
    let telemetry_enabled = crate::performance::enabled();
    eprintln!(
        "TRACK_C_RESULT schema=1 owner={} source={} profile={} mode={:?} telemetry_enabled={} fixture={} fixture_signature={:016x} count={} viewport={} scale_milli={} fonts={} settings={} cold_type={} warmups={} samples={} elapsed_ns=[{}] p50_ns={} p95_ns={} max_ns={} complete_output_state_ids=[{}] structural_output_ids=[{}] revision_receipts=[{}] viewport_receipts=[{}] work_units=[{}] work_counters=[{}] exclusions=0",
        metadata.owner,
        source,
        profile,
        metadata.mode,
        telemetry_enabled,
        metadata.fixture_name,
        metadata.fixture_signature,
        metadata.item_count,
        metadata.viewport,
        metadata.scale_milli,
        metadata.font_state,
        metadata.settings,
        metadata.cold_type,
        WARMUPS,
        samples.len(),
        elapsed_text.join(","),
        nearest_rank(&elapsed, 50),
        nearest_rank(&elapsed, 95),
        elapsed.iter().copied().max().unwrap_or_default(),
        output_ids.join(","),
        structure_ids.join(","),
        revision_receipts.join(","),
        viewport_receipts.join(","),
        work_units.join(","),
        work_counters.join("|"),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_c_fixture_mode_and_domains_are_stable_and_separate() {
        assert_eq!(BenchmarkMode::parse("small"), Ok(BenchmarkMode::Small));
        assert_eq!(BenchmarkMode::parse("full"), Ok(BenchmarkMode::Full));
        assert!(BenchmarkMode::parse("debug").is_err());
        assert_eq!(BenchmarkMode::Small.note_sizes(), &[100]);
        assert_eq!(BenchmarkMode::Full.note_sizes(), &[100, 1_000, 5_000]);

        let first = note_fixture(8);
        let second = note_fixture(8);
        let old = workloads::note_fixture(domain_seed("track-a-comparison"), 8);
        assert_eq!(first.summary.signature, second.summary.signature);
        assert_ne!(first.summary.signature, old.summary.signature);
        assert_eq!(first.values, second.values);
    }

    #[test]
    fn track_c_signatures_frame_fields_and_preserve_optional_arguments() {
        let left = [Action {
            label: "ab".into(),
            desc: "c".into(),
            action: "x".into(),
            args: None,
        }];
        let right = [Action {
            label: "a".into(),
            desc: "bc".into(),
            action: "x".into(),
            args: None,
        }];
        let empty_args = [Action {
            label: "ab".into(),
            desc: "c".into(),
            action: "x".into(),
            args: Some(String::new()),
        }];
        assert_ne!(
            action_fixture_identity(&left),
            action_fixture_identity(&right),
            "variable string fields are framed"
        );
        assert_ne!(
            action_fixture_identity(&left),
            action_fixture_identity(&empty_args),
            "None and Some(empty) args have distinct structural identities"
        );
    }

    #[test]
    fn track_c_nearest_rank_uses_ceiling_rank() {
        let values = [
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
        ];
        assert_eq!(nearest_rank(&values, 50), 10);
        assert_eq!(nearest_rank(&values, 95), 19);
        assert_eq!(nearest_rank(&values, 100), 20);
    }

    #[test]
    fn track_c_measurement_validates_after_timer_and_preserves_every_sample() {
        use std::cell::Cell;

        let setup_count = Cell::new(0);
        let run_count = Cell::new(0);
        let validation_count = Cell::new(0);
        let samples = measure_owner(
            || {
                setup_count.set(setup_count.get() + 1);
                4_u64
            },
            |state| {
                run_count.set(run_count.get() + 1);
                *state += 1;
                *state
            },
            |state, output| {
                validation_count.set(validation_count.get() + 1);
                assert_eq!(*state, *output);
                assert_eq!(*output, 5);
                OwnerObservation {
                    output_identity: *output,
                    structural_signature: *output,
                    revision_receipts: vec![],
                    viewport_receipt: Some(0x1234),
                    work_units: 2,
                    work_counters: vec![1, 2],
                }
            },
        );

        assert_eq!(setup_count.get(), WARMUPS + MEASURED_SAMPLES);
        assert_eq!(run_count.get(), WARMUPS + MEASURED_SAMPLES);
        assert_eq!(validation_count.get(), WARMUPS + MEASURED_SAMPLES);
        assert_eq!(samples.len(), MEASURED_SAMPLES);
        assert!(samples.iter().all(|sample| {
            sample.output_identity == 5
                && sample.structural_signature == 5
                && sample.revision_receipts.is_empty()
                && sample.viewport_receipt == Some(0x1234)
                && sample.work_units == 2
                && sample.work_counters == [1, 2]
        }));
    }

    #[test]
    fn track_c_measurement_retains_legitimate_per_sample_revision_receipts() {
        use std::cell::Cell;

        let revision = Cell::new(40_u64);
        let samples = measure_owner(
            || (),
            |_| {
                let next = revision.get() + 1;
                revision.set(next);
                next
            },
            |_, value| OwnerObservation {
                output_identity: *value,
                structural_signature: 0xfeed,
                revision_receipts: vec![*value],
                viewport_receipt: None,
                work_units: 0,
                work_counters: vec![*value],
            },
        );
        assert_eq!(samples.len(), MEASURED_SAMPLES);
        assert_eq!(samples[0].structural_signature, 0xfeed);
        assert_eq!(samples[1].structural_signature, 0xfeed);
        assert_ne!(samples[0].output_identity, samples[1].output_identity);
        assert_eq!(samples[0].revision_receipts, vec![46]);
        assert_eq!(samples[1].revision_receipts, vec![47]);
    }
}
