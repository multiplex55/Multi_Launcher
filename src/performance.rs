use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const PERF_ENV: &str = "MULTI_LAUNCHER_PERF";

static ENABLED: OnceLock<bool> = OnceLock::new();
static PROCESS_START: OnceLock<Instant> = OnceLock::new();
static FRAME_STATE: OnceLock<Mutex<FrameState>> = OnceLock::new();
static METRICS: MetricCollector = MetricCollector::new();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum Metric {
    NoteRefreshCheck,
    NoteSnapshot,
    NoteAliasHash,
    NoteHeavyRecompute,
    HistoryPrepare,
    HistoryResolve,
    HistoryCatalogBuild,
    LauncherRowsBuilt,
    QuickNotesRowsBuilt,
    ActionsReload,
    IndexScan,
    CoordinateSample,
    EffectsRefreshSource,
    EffectsPresentSource,
    HudGdiCreate,
}

impl Metric {
    pub const COUNT: usize = 15;

    const ALL: [Self; Self::COUNT] = [
        Self::NoteRefreshCheck,
        Self::NoteSnapshot,
        Self::NoteAliasHash,
        Self::NoteHeavyRecompute,
        Self::HistoryPrepare,
        Self::HistoryResolve,
        Self::HistoryCatalogBuild,
        Self::LauncherRowsBuilt,
        Self::QuickNotesRowsBuilt,
        Self::ActionsReload,
        Self::IndexScan,
        Self::CoordinateSample,
        Self::EffectsRefreshSource,
        Self::EffectsPresentSource,
        Self::HudGdiCreate,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::NoteRefreshCheck => "note.refresh_check",
            Self::NoteSnapshot => "note.snapshot",
            Self::NoteAliasHash => "note.alias_hash",
            Self::NoteHeavyRecompute => "note.heavy_recompute",
            Self::HistoryPrepare => "history.prepare",
            Self::HistoryResolve => "history.resolve",
            Self::HistoryCatalogBuild => "history.catalog_build",
            Self::LauncherRowsBuilt => "launcher.rows_built",
            Self::QuickNotesRowsBuilt => "quick_notes.rows_built",
            Self::ActionsReload => "actions.reload",
            Self::IndexScan => "index.scan",
            Self::CoordinateSample => "coordinate.sample",
            Self::EffectsRefreshSource => "effects.refresh_source",
            Self::EffectsPresentSource => "effects.present_source",
            Self::HudGdiCreate => "hud.gdi_create",
        }
    }

    #[must_use]
    pub const fn work_unit_name(self) -> &'static str {
        match self {
            Self::NoteRefreshCheck => "heavy_refreshes_requested",
            Self::NoteSnapshot => "estimated_clone_bytes",
            Self::NoteAliasHash => "alias_pairs_hashed",
            Self::NoteHeavyRecompute => "heavy_recomputes",
            Self::HistoryPrepare => "history_entries_cloned",
            Self::HistoryResolve => "candidates_resolved",
            Self::HistoryCatalogBuild => "command_actions_enumerated",
            Self::LauncherRowsBuilt | Self::QuickNotesRowsBuilt => "widgets_built",
            Self::ActionsReload => "actions_processed",
            Self::IndexScan => "actions_constructed",
            Self::CoordinateSample => "samples",
            Self::EffectsRefreshSource | Self::EffectsPresentSource => "source_dispatches",
            Self::HudGdiCreate => "creation_attempts",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Non-duration outcome counts associated with a metric owner.
pub enum MetricOutcome {
    Completed,
    Error,
    Abandoned,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MetricSnapshot {
    pub metric: Metric,
    /// Number of owner-boundary samples, including failed attempts.
    pub calls: u64,
    /// Sum of the metric-specific quantity named by `Metric::work_unit_name`.
    pub work_units: u64,
    /// Sum and maximum elapsed time at the instrumented owner boundary.
    pub elapsed_nanos_total: u64,
    pub elapsed_nanos_max: u64,
    /// Note-cache lock acquisition time; zero for metrics without a lock scope.
    pub lock_wait_nanos_total: u64,
    pub lock_wait_nanos_max: u64,
    /// Iterator completion/error/abandonment counts; errors also mark failed note snapshots.
    pub completed: u64,
    pub errors: u64,
    pub abandoned: u64,
}

#[derive(Debug)]
struct MetricCounters {
    calls: AtomicU64,
    work_units: AtomicU64,
    elapsed_nanos_total: AtomicU64,
    elapsed_nanos_max: AtomicU64,
    lock_wait_nanos_total: AtomicU64,
    lock_wait_nanos_max: AtomicU64,
    completed: AtomicU64,
    errors: AtomicU64,
    abandoned: AtomicU64,
}

impl MetricCounters {
    const fn new() -> Self {
        Self {
            calls: AtomicU64::new(0),
            work_units: AtomicU64::new(0),
            elapsed_nanos_total: AtomicU64::new(0),
            elapsed_nanos_max: AtomicU64::new(0),
            lock_wait_nanos_total: AtomicU64::new(0),
            lock_wait_nanos_max: AtomicU64::new(0),
            completed: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            abandoned: AtomicU64::new(0),
        }
    }

    fn record(&self, work_units: u64, elapsed: Duration, lock_wait: Option<Duration>) {
        let elapsed_nanos = duration_nanos(elapsed);
        self.calls.fetch_add(1, AtomicOrdering::Relaxed);
        self.work_units
            .fetch_add(work_units, AtomicOrdering::Relaxed);
        self.elapsed_nanos_total
            .fetch_add(elapsed_nanos, AtomicOrdering::Relaxed);
        self.elapsed_nanos_max
            .fetch_max(elapsed_nanos, AtomicOrdering::Relaxed);
        if let Some(lock_wait) = lock_wait {
            let wait_nanos = duration_nanos(lock_wait);
            self.lock_wait_nanos_total
                .fetch_add(wait_nanos, AtomicOrdering::Relaxed);
            self.lock_wait_nanos_max
                .fetch_max(wait_nanos, AtomicOrdering::Relaxed);
        }
    }

    fn record_outcome(&self, outcome: MetricOutcome) {
        let counter = match outcome {
            MetricOutcome::Completed => &self.completed,
            MetricOutcome::Error => &self.errors,
            MetricOutcome::Abandoned => &self.abandoned,
        };
        counter.fetch_add(1, AtomicOrdering::Relaxed);
    }

    fn snapshot(&self, metric: Metric) -> MetricSnapshot {
        MetricSnapshot {
            metric,
            calls: self.calls.load(AtomicOrdering::Relaxed),
            work_units: self.work_units.load(AtomicOrdering::Relaxed),
            elapsed_nanos_total: self.elapsed_nanos_total.load(AtomicOrdering::Relaxed),
            elapsed_nanos_max: self.elapsed_nanos_max.load(AtomicOrdering::Relaxed),
            lock_wait_nanos_total: self.lock_wait_nanos_total.load(AtomicOrdering::Relaxed),
            lock_wait_nanos_max: self.lock_wait_nanos_max.load(AtomicOrdering::Relaxed),
            completed: self.completed.load(AtomicOrdering::Relaxed),
            errors: self.errors.load(AtomicOrdering::Relaxed),
            abandoned: self.abandoned.load(AtomicOrdering::Relaxed),
        }
    }

    fn reset(&self) {
        self.calls.store(0, AtomicOrdering::Relaxed);
        self.work_units.store(0, AtomicOrdering::Relaxed);
        self.elapsed_nanos_total.store(0, AtomicOrdering::Relaxed);
        self.elapsed_nanos_max.store(0, AtomicOrdering::Relaxed);
        self.lock_wait_nanos_total.store(0, AtomicOrdering::Relaxed);
        self.lock_wait_nanos_max.store(0, AtomicOrdering::Relaxed);
        self.completed.store(0, AtomicOrdering::Relaxed);
        self.errors.store(0, AtomicOrdering::Relaxed);
        self.abandoned.store(0, AtomicOrdering::Relaxed);
    }
}

#[derive(Debug)]
struct MetricCollector {
    counters: [MetricCounters; Metric::COUNT],
}

impl MetricCollector {
    const fn new() -> Self {
        Self {
            counters: [
                MetricCounters::new(),
                MetricCounters::new(),
                MetricCounters::new(),
                MetricCounters::new(),
                MetricCounters::new(),
                MetricCounters::new(),
                MetricCounters::new(),
                MetricCounters::new(),
                MetricCounters::new(),
                MetricCounters::new(),
                MetricCounters::new(),
                MetricCounters::new(),
                MetricCounters::new(),
                MetricCounters::new(),
                MetricCounters::new(),
            ],
        }
    }

    fn record_if_enabled(
        &self,
        enabled: bool,
        metric: Metric,
        work_units: u64,
        elapsed: Duration,
        lock_wait: Option<Duration>,
    ) {
        if enabled {
            self.counters[metric as usize].record(work_units, elapsed, lock_wait);
        }
    }

    fn record_outcome_if_enabled(&self, enabled: bool, metric: Metric, outcome: MetricOutcome) {
        if enabled {
            self.counters[metric as usize].record_outcome(outcome);
        }
    }

    fn snapshot(&self) -> [MetricSnapshot; Metric::COUNT] {
        std::array::from_fn(|index| self.counters[index].snapshot(Metric::ALL[index]))
    }

    fn reset(&self) {
        for counter in &self.counters {
            counter.reset();
        }
    }
}

/// A fixed-memory duration sample. Work units are metric-specific (for
/// example, cloned bytes, resolved history entries, or built widgets).
pub struct MetricTimer {
    metric: Metric,
    work_units: u64,
    enabled: bool,
    started: Option<Instant>,
}

impl MetricTimer {
    #[must_use]
    pub fn start(metric: Metric) -> Self {
        Self::start_if(metric, enabled())
    }

    fn start_if(metric: Metric, enabled: bool) -> Self {
        Self {
            metric,
            work_units: 1,
            enabled,
            started: enabled.then(Instant::now),
        }
    }

    pub fn set_work_units(&mut self, work_units: u64) {
        if self.enabled {
            self.work_units = work_units;
        }
    }

    pub fn add_work_units(&mut self, work_units: u64) {
        if self.enabled {
            self.work_units = self.work_units.saturating_add(work_units);
        }
    }

    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        self.enabled
    }
}

impl Drop for MetricTimer {
    fn drop(&mut self) {
        if let Some(started) = self.started {
            METRICS.record_if_enabled(
                self.enabled,
                self.metric,
                self.work_units,
                started.elapsed(),
                None,
            );
        }
    }
}

/// A snapshot is bounded and lock-free. Values may straddle concurrent samples.
#[must_use]
pub fn snapshot_metrics() -> [MetricSnapshot; Metric::COUNT] {
    METRICS.snapshot()
}

/// Reset counters between controlled runs while metric writers are quiescent.
pub fn reset_metrics() {
    METRICS.reset();
}

pub fn record_metric_sample(
    metric: Metric,
    work_units: u64,
    elapsed: Duration,
    lock_wait: Option<Duration>,
) {
    METRICS.record_if_enabled(enabled(), metric, work_units, elapsed, lock_wait);
}

pub fn record_metric_outcome(metric: Metric, outcome: MetricOutcome) {
    METRICS.record_outcome_if_enabled(enabled(), metric, outcome);
}

fn duration_nanos(duration: Duration) -> u64 {
    duration.as_nanos().min(u64::MAX as u128) as u64
}

#[derive(Debug)]
struct FrameState {
    first_frame_seen: bool,
    window_started: Instant,
    frames: u64,
    dashboard_repaint_requests: u64,
}

pub fn init_process_timer() {
    if enabled() {
        let _ = PROCESS_START.set(Instant::now());
    }
}

pub fn enabled() -> bool {
    *ENABLED.get_or_init(|| enabled_from_value(std::env::var(PERF_ENV).ok().as_deref()))
}

fn enabled_from_value(value: Option<&str>) -> bool {
    value.is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

#[must_use]
pub struct Timer(Option<Instant>);

impl Timer {
    pub fn start() -> Self {
        Self(enabled().then(Instant::now))
    }

    pub fn start_if(enabled: bool) -> Self {
        Self(enabled.then(Instant::now))
    }

    pub fn finish(self, phase: &'static str) {
        if let Some(started) = self.0 {
            log_duration(phase, started.elapsed());
        }
    }

    pub fn finish_plugin(self, plugin: &str) {
        if let Some(started) = self.0 {
            log_plugin_duration(plugin, started.elapsed());
        }
    }
}

pub fn started() -> Option<Instant> {
    enabled().then(Instant::now)
}

pub fn started_if(enabled: bool) -> Option<Instant> {
    enabled.then(Instant::now)
}

pub fn log_elapsed(phase: &'static str, started: Option<Instant>) {
    if let Some(started) = started {
        log_duration(phase, started.elapsed());
    }
}

pub fn log_duration(phase: &'static str, duration: Duration) {
    if enabled() {
        tracing::info!(
            target: "multi_launcher::performance",
            phase,
            duration_ms = duration.as_secs_f64() * 1_000.0,
            "perf"
        );
    }
}

pub fn log_plugin_duration(plugin: &str, duration: Duration) {
    if enabled() {
        tracing::info!(
            target: "multi_launcher::performance",
            phase = "search.plugin",
            plugin,
            duration_ms = duration.as_secs_f64() * 1_000.0,
            "perf"
        );
    }
}

pub fn log_process_elapsed(phase: &'static str) {
    if enabled()
        && let Some(started) = PROCESS_START.get()
    {
        log_duration(phase, started.elapsed());
    }
}

pub fn record_dashboard_repaint_request() {
    if !enabled() {
        return;
    }
    if let Ok(mut state) = frame_state().lock() {
        state.dashboard_repaint_requests += 1;
    }
}

pub fn record_frame(visible: bool, focused: bool, dashboard: bool) {
    if !enabled() {
        return;
    }
    let Ok(mut state) = frame_state().lock() else {
        return;
    };
    state.frames += 1;
    if !state.first_frame_seen {
        state.first_frame_seen = true;
        drop(state);
        log_process_elapsed("startup.first_update");
        log_process_elapsed("startup.first_usable_frame");
        return;
    }
    let elapsed = state.window_started.elapsed();
    if elapsed >= Duration::from_secs(1) {
        tracing::info!(
            target: "multi_launcher::performance",
            phase = "runtime.frames",
            window_ms = elapsed.as_secs_f64() * 1_000.0,
            frames = state.frames,
            dashboard_repaint_requests = state.dashboard_repaint_requests,
            visible,
            focused,
            dashboard,
            "perf"
        );
        state.window_started = Instant::now();
        state.frames = 0;
        state.dashboard_repaint_requests = 0;
    }
}

fn frame_state() -> &'static Mutex<FrameState> {
    FRAME_STATE.get_or_init(|| {
        Mutex::new(FrameState {
            first_frame_seen: false,
            window_started: Instant::now(),
            frames: 0,
            dashboard_repaint_requests: 0,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::enabled_from_value;
    use std::time::Duration;

    #[test]
    fn diagnostics_switch_accepts_only_explicit_truthy_values() {
        for value in ["1", "true", "TRUE", " yes ", "on"] {
            assert!(enabled_from_value(Some(value)), "{value}");
        }
        for value in [None, Some(""), Some("0"), Some("false"), Some("anything")] {
            assert!(!enabled_from_value(value));
        }
    }

    #[test]
    fn subsystem_metrics_are_opt_in_bounded_and_resettable() {
        let collector = super::MetricCollector::new();
        let disabled_timer = super::MetricTimer::start_if(super::Metric::NoteSnapshot, false);
        assert!(!disabled_timer.is_enabled());
        assert!(disabled_timer.started.is_none());
        drop(disabled_timer);

        collector.record_if_enabled(
            false,
            super::Metric::NoteSnapshot,
            99,
            Duration::from_nanos(123),
            Some(Duration::from_nanos(7)),
        );
        collector.record_outcome_if_enabled(
            false,
            super::Metric::IndexScan,
            super::MetricOutcome::Abandoned,
        );
        let disabled = collector.snapshot();
        assert_eq!(disabled[super::Metric::NoteSnapshot as usize].calls, 0);
        assert_eq!(disabled[super::Metric::IndexScan as usize].abandoned, 0);

        collector.record_if_enabled(
            true,
            super::Metric::NoteSnapshot,
            99,
            Duration::from_nanos(123),
            Some(Duration::from_nanos(7)),
        );
        collector.record_if_enabled(
            true,
            super::Metric::NoteSnapshot,
            21,
            Duration::from_nanos(45),
            Some(Duration::from_nanos(3)),
        );
        collector.record_outcome_if_enabled(
            true,
            super::Metric::IndexScan,
            super::MetricOutcome::Completed,
        );
        collector.record_outcome_if_enabled(
            true,
            super::Metric::IndexScan,
            super::MetricOutcome::Error,
        );
        collector.record_outcome_if_enabled(
            true,
            super::Metric::IndexScan,
            super::MetricOutcome::Abandoned,
        );

        let enabled = collector.snapshot();
        let snapshots = enabled[super::Metric::NoteSnapshot as usize];
        assert_eq!(snapshots.calls, 2);
        assert_eq!(snapshots.work_units, 120);
        assert_eq!(snapshots.elapsed_nanos_total, 168);
        assert_eq!(snapshots.elapsed_nanos_max, 123);
        assert_eq!(snapshots.lock_wait_nanos_total, 10);
        assert_eq!(snapshots.lock_wait_nanos_max, 7);
        let scans = enabled[super::Metric::IndexScan as usize];
        assert_eq!(scans.completed, 1);
        assert_eq!(scans.errors, 1);
        assert_eq!(scans.abandoned, 1);

        collector.reset();
        assert_eq!(
            collector.snapshot()[super::Metric::NoteSnapshot as usize].calls,
            0
        );
    }

    #[test]
    fn subsystem_metric_atomics_aggregate_concurrent_samples() {
        let collector = std::sync::Arc::new(super::MetricCollector::new());
        let workers = 4_u64;
        let samples_per_worker = 500_u64;
        let threads = (0..workers)
            .map(|_| {
                let collector = std::sync::Arc::clone(&collector);
                std::thread::spawn(move || {
                    for _ in 0..samples_per_worker {
                        collector.record_if_enabled(
                            true,
                            super::Metric::HistoryResolve,
                            1,
                            Duration::from_nanos(2),
                            None,
                        );
                    }
                })
            })
            .collect::<Vec<_>>();
        for thread in threads {
            thread.join().unwrap();
        }

        let resolved = collector.snapshot()[super::Metric::HistoryResolve as usize];
        assert_eq!(resolved.calls, workers * samples_per_worker);
        assert_eq!(resolved.work_units, workers * samples_per_worker);
        assert_eq!(
            resolved.elapsed_nanos_total,
            workers * samples_per_worker * 2
        );
    }
}
