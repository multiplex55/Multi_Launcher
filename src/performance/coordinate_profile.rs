//! Separately activated, fixed-capacity native coordinate profiling.
//!
//! Ordinary subsystem metrics never touch these buffers. A profile session
//! allocates its sample slots once, and timed native phases record into atomic
//! slots without allocating or logging on the worker thread. The owner must
//! keep its [`ProfileWorker`] alive until the profiled worker has stopped, then
//! call [`ProfileSession::finish`] before exporting a summary.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use serde::Serialize;

pub const SAMPLE_CAPACITY_PER_PHASE: usize = 8192;

static SESSION_ACTIVE: AtomicBool = AtomicBool::new(false);
static COLLECTOR: OnceLock<Collector> = OnceLock::new();
static SESSION_GATE: Mutex<()> = Mutex::new(());

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum NativePhase {
    PassiveSample,
    SampleMetadata,
    CursorPosition,
    VirtualDesktopMetrics,
    MonitorLookup,
    MonitorInfo,
    MonitorDpi,
    ForegroundClientGeometry,
    HudRender,
    HudDraw,
    HudPresentation,
    HudUpload,
    HudShow,
    HudReposition,
}

impl NativePhase {
    const COUNT: usize = 14;

    const ALL: [Self; Self::COUNT] = [
        Self::PassiveSample,
        Self::SampleMetadata,
        Self::CursorPosition,
        Self::VirtualDesktopMetrics,
        Self::MonitorLookup,
        Self::MonitorInfo,
        Self::MonitorDpi,
        Self::ForegroundClientGeometry,
        Self::HudRender,
        Self::HudDraw,
        Self::HudPresentation,
        Self::HudUpload,
        Self::HudShow,
        Self::HudReposition,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::PassiveSample => "sample.passive_total",
            Self::SampleMetadata => "sample.metadata",
            Self::CursorPosition => "sample.cursor_position",
            Self::VirtualDesktopMetrics => "sample.virtual_desktop_metrics",
            Self::MonitorLookup => "sample.monitor_lookup",
            Self::MonitorInfo => "sample.monitor_info",
            Self::MonitorDpi => "sample.monitor_dpi",
            Self::ForegroundClientGeometry => "sample.foreground_client_geometry",
            Self::HudRender => "hud.render",
            Self::HudDraw => "hud.draw",
            Self::HudPresentation => "hud.present",
            Self::HudUpload => "hud.upload",
            Self::HudShow => "hud.show",
            Self::HudReposition => "hud.reposition",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum GdiObject {
    Brush,
    Font,
}

impl GdiObject {
    const COUNT: usize = 2;

    const ALL: [Self; Self::COUNT] = [Self::Brush, Self::Font];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Brush => "brush",
            Self::Font => "font",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HudPath {
    Redraw,
    Reposition,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct PhaseSummary {
    pub phase: &'static str,
    pub calls: u64,
    pub recorded: u64,
    pub dropped: u64,
    pub p50_nanos: Option<u64>,
    pub p95_nanos: Option<u64>,
    pub max_nanos: Option<u64>,
    pub errors: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct GdiObjectSummary {
    pub object: &'static str,
    pub create_attempts: u64,
    pub create_successes: u64,
    pub delete_attempts: u64,
    pub delete_successes: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct NativeProfileSummary {
    pub capacity_per_phase: usize,
    pub phases: Vec<PhaseSummary>,
    pub gdi_objects: Vec<GdiObjectSummary>,
    pub hud_redraws: u64,
    pub hud_repositions: u64,
}

struct PhaseBuffer {
    samples: Box<[AtomicU64]>,
    calls: AtomicU64,
    dropped: AtomicU64,
    errors: AtomicU64,
}

impl PhaseBuffer {
    fn new(capacity: usize) -> Self {
        let samples = (0..capacity)
            .map(|_| AtomicU64::new(0))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Self {
            samples,
            calls: AtomicU64::new(0),
            dropped: AtomicU64::new(0),
            errors: AtomicU64::new(0),
        }
    }

    fn record(&self, elapsed_nanos: u64, failed: bool) {
        let index = self.calls.fetch_add(1, Ordering::Relaxed) as usize;
        if failed {
            self.errors.fetch_add(1, Ordering::Relaxed);
        }
        if let Some(sample) = self.samples.get(index) {
            sample.store(elapsed_nanos, Ordering::Relaxed);
        } else {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn reset(&self) {
        self.calls.store(0, Ordering::Relaxed);
        self.dropped.store(0, Ordering::Relaxed);
        self.errors.store(0, Ordering::Relaxed);
        for sample in &self.samples {
            sample.store(0, Ordering::Relaxed);
        }
    }

    fn summary(&self, phase: NativePhase) -> PhaseSummary {
        let calls = self.calls.load(Ordering::Acquire);
        let recorded = calls.min(self.samples.len() as u64);
        let mut samples = self.samples[..recorded as usize]
            .iter()
            .map(|sample| sample.load(Ordering::Relaxed))
            .collect::<Vec<_>>();
        samples.sort_unstable();
        PhaseSummary {
            phase: phase.name(),
            calls,
            recorded,
            dropped: self.dropped.load(Ordering::Relaxed),
            p50_nanos: nearest_rank(&samples, 50, 100),
            p95_nanos: nearest_rank(&samples, 95, 100),
            max_nanos: samples.last().copied(),
            errors: self.errors.load(Ordering::Relaxed),
        }
    }
}

struct GdiCounters {
    create_attempts: AtomicU64,
    create_successes: AtomicU64,
    delete_attempts: AtomicU64,
    delete_successes: AtomicU64,
}

impl GdiCounters {
    const fn new() -> Self {
        Self {
            create_attempts: AtomicU64::new(0),
            create_successes: AtomicU64::new(0),
            delete_attempts: AtomicU64::new(0),
            delete_successes: AtomicU64::new(0),
        }
    }

    fn record(&self, create: bool, succeeded: bool) {
        let (attempts, successes) = if create {
            (&self.create_attempts, &self.create_successes)
        } else {
            (&self.delete_attempts, &self.delete_successes)
        };
        attempts.fetch_add(1, Ordering::Relaxed);
        if succeeded {
            successes.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn reset(&self) {
        self.create_attempts.store(0, Ordering::Relaxed);
        self.create_successes.store(0, Ordering::Relaxed);
        self.delete_attempts.store(0, Ordering::Relaxed);
        self.delete_successes.store(0, Ordering::Relaxed);
    }

    fn summary(&self, object: GdiObject) -> GdiObjectSummary {
        GdiObjectSummary {
            object: object.name(),
            create_attempts: self.create_attempts.load(Ordering::Relaxed),
            create_successes: self.create_successes.load(Ordering::Relaxed),
            delete_attempts: self.delete_attempts.load(Ordering::Relaxed),
            delete_successes: self.delete_successes.load(Ordering::Relaxed),
        }
    }
}

struct Collector {
    capacity: usize,
    phases: [PhaseBuffer; NativePhase::COUNT],
    gdi: [GdiCounters; GdiObject::COUNT],
    hud_redraws: AtomicU64,
    hud_repositions: AtomicU64,
    accepting: AtomicBool,
    writers: AtomicUsize,
    workers: AtomicUsize,
}

impl Collector {
    fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "coordinate profile capacity must be positive");
        Self {
            capacity,
            phases: std::array::from_fn(|_| PhaseBuffer::new(capacity)),
            gdi: std::array::from_fn(|_| GdiCounters::new()),
            hud_redraws: AtomicU64::new(0),
            hud_repositions: AtomicU64::new(0),
            accepting: AtomicBool::new(false),
            writers: AtomicUsize::new(0),
            workers: AtomicUsize::new(0),
        }
    }

    fn reset_quiescent(&self) {
        assert_eq!(self.workers.load(Ordering::Acquire), 0);
        assert_eq!(self.writers.load(Ordering::Acquire), 0);
        for phase in &self.phases {
            phase.reset();
        }
        for counters in &self.gdi {
            counters.reset();
        }
        self.hud_redraws.store(0, Ordering::Relaxed);
        self.hud_repositions.store(0, Ordering::Relaxed);
    }

    fn snapshot(&self) -> NativeProfileSummary {
        NativeProfileSummary {
            capacity_per_phase: self.capacity,
            phases: NativePhase::ALL
                .iter()
                .map(|phase| self.phases[*phase as usize].summary(*phase))
                .collect(),
            gdi_objects: GdiObject::ALL
                .iter()
                .map(|object| self.gdi[*object as usize].summary(*object))
                .collect(),
            hud_redraws: self.hud_redraws.load(Ordering::Relaxed),
            hud_repositions: self.hud_repositions.load(Ordering::Relaxed),
        }
    }

    fn record_hud_path(&self, path: HudPath) {
        let counter = match path {
            HudPath::Redraw => &self.hud_redraws,
            HudPath::Reposition => &self.hud_repositions,
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }
}

/// Exclusive owner for one explicitly requested native profile run.
pub struct ProfileSession {
    collector: &'static Collector,
    finished: bool,
}

impl ProfileSession {
    /// Start a fresh profile session. The first session allocates the fixed
    /// per-phase buffers; subsequent sessions reset them only while inactive.
    pub fn start() -> Result<Self, String> {
        let _gate = SESSION_GATE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if SESSION_ACTIVE.load(Ordering::Acquire) {
            return Err("a coordinate native profile session is already active".into());
        }
        let collector = COLLECTOR.get_or_init(|| Collector::new(SAMPLE_CAPACITY_PER_PHASE));
        if collector.capacity != SAMPLE_CAPACITY_PER_PHASE {
            return Err("coordinate native profile capacity changed after initialization".into());
        }
        collector.reset_quiescent();
        collector.accepting.store(true, Ordering::Release);
        SESSION_ACTIVE.store(true, Ordering::Release);
        Ok(Self {
            collector,
            finished: false,
        })
    }

    /// Attach the single profiled native worker. Keep this guard alive until
    /// that worker has been joined or shut down.
    pub fn attach_worker(&self) -> Result<ProfileWorker<'_>, String> {
        if self.finished || !self.collector.accepting.load(Ordering::Acquire) {
            return Err("coordinate native profile session is not active".into());
        }
        self.collector
            .workers
            .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "coordinate native profile already has a worker".to_owned())?;
        Ok(ProfileWorker { session: self })
    }

    /// Stop accepting records and return the bounded summary. A worker guard
    /// prevents finalization until its owner has stopped the worker.
    pub fn finish(&mut self) -> Result<NativeProfileSummary, String> {
        let _gate = SESSION_GATE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if self.finished {
            return Err("coordinate native profile session is already finished".into());
        }
        if self.collector.workers.load(Ordering::Acquire) != 0 {
            return Err("stop the profiled coordinate worker before summarizing".into());
        }
        self.collector.accepting.store(false, Ordering::Release);
        SESSION_ACTIVE.store(false, Ordering::Release);
        while self.collector.writers.load(Ordering::Acquire) != 0 {
            std::thread::yield_now();
        }
        let summary = self.collector.snapshot();
        self.finished = true;
        Ok(summary)
    }
}

impl Drop for ProfileSession {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        let _gate = SESSION_GATE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.collector.accepting.store(false, Ordering::Release);
        SESSION_ACTIVE.store(false, Ordering::Release);
        while self.collector.writers.load(Ordering::Acquire) != 0 {
            std::thread::yield_now();
        }
    }
}

/// Lifetime token for the one worker whose native work belongs to a session.
pub struct ProfileWorker<'a> {
    session: &'a ProfileSession,
}

impl Drop for ProfileWorker<'_> {
    fn drop(&mut self) {
        self.session.collector.workers.store(0, Ordering::Release);
    }
}

/// Scoped duration recorder. The inactive path performs no clock read and no
/// collector write.
pub struct PhaseTimer {
    collector: Option<&'static Collector>,
    phase: NativePhase,
    started: Option<Instant>,
    failed: bool,
}

impl PhaseTimer {
    #[must_use]
    pub fn start(phase: NativePhase) -> Self {
        Self::start_with_collector(phase, active_collector())
    }

    fn start_with_collector(phase: NativePhase, collector: Option<&'static Collector>) -> Self {
        let Some(collector) = collector else {
            return Self::inactive(phase);
        };
        collector.writers.fetch_add(1, Ordering::AcqRel);
        if !SESSION_ACTIVE.load(Ordering::Acquire) || !collector.accepting.load(Ordering::Acquire) {
            collector.writers.fetch_sub(1, Ordering::AcqRel);
            return Self::inactive(phase);
        }
        Self {
            collector: Some(collector),
            phase,
            started: Some(Instant::now()),
            failed: false,
        }
    }

    fn inactive(phase: NativePhase) -> Self {
        Self {
            collector: None,
            phase,
            started: None,
            failed: false,
        }
    }

    pub fn mark_error(&mut self) {
        self.failed = true;
    }

    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.started.is_some()
    }
}

impl Drop for PhaseTimer {
    fn drop(&mut self) {
        let (Some(collector), Some(started)) = (self.collector, self.started) else {
            return;
        };
        let nanos = started.elapsed().as_nanos().min(u64::MAX as u128) as u64;
        collector.phases[self.phase as usize].record(nanos, self.failed);
        collector.writers.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Record a GDI object create/delete attempt and its native API success value.
pub fn record_gdi_operation(object: GdiObject, create: bool, succeeded: bool) {
    let Some(collector) = begin_write() else {
        return;
    };
    collector.gdi[object as usize].record(create, succeeded);
    collector.writers.fetch_sub(1, Ordering::AcqRel);
}

/// Count the HUD's current redraw or reposition branch.
pub fn record_hud_path(path: HudPath) {
    let Some(collector) = begin_write() else {
        return;
    };
    collector.record_hud_path(path);
    collector.writers.fetch_sub(1, Ordering::AcqRel);
}

/// Measure a fallible operation without allocating. When profiling is off,
/// the timer remains inert and the operation runs unchanged.
pub fn measure<T, E>(phase: NativePhase, operation: impl FnOnce() -> Result<T, E>) -> Result<T, E> {
    let mut timer = PhaseTimer::start(phase);
    let result = operation();
    if result.is_err() {
        timer.mark_error();
    }
    result
}

/// Measure an optional native query; `None` is recorded as a failed query.
pub fn measure_optional<T>(phase: NativePhase, operation: impl FnOnce() -> Option<T>) -> Option<T> {
    let mut timer = PhaseTimer::start(phase);
    let result = operation();
    if result.is_none() {
        timer.mark_error();
    }
    result
}

fn begin_write() -> Option<&'static Collector> {
    let collector = active_collector()?;
    collector.writers.fetch_add(1, Ordering::AcqRel);
    if !SESSION_ACTIVE.load(Ordering::Acquire) || !collector.accepting.load(Ordering::Acquire) {
        collector.writers.fetch_sub(1, Ordering::AcqRel);
        return None;
    }
    Some(collector)
}

fn active_collector() -> Option<&'static Collector> {
    if !SESSION_ACTIVE.load(Ordering::Acquire) {
        return None;
    }
    COLLECTOR.get()
}

fn nearest_rank(samples: &[u64], numerator: usize, denominator: usize) -> Option<u64> {
    if samples.is_empty() {
        return None;
    }
    let rank = samples
        .len()
        .saturating_mul(numerator)
        .div_ceil(denominator);
    samples.get(rank.saturating_sub(1)).copied()
}

#[cfg(test)]
mod tests {
    use super::{
        Collector, GdiObject, HudPath, NativePhase, PhaseTimer, ProfileSession, nearest_rank,
    };

    #[test]
    fn coordinate_profile_disabled_timer_has_no_clock_or_collector_write() {
        let timer = PhaseTimer::start_with_collector(NativePhase::PassiveSample, None);
        assert!(!timer.is_active());
        drop(timer);
    }

    #[test]
    fn coordinate_profile_buffers_are_phase_separated_and_bounded() {
        let collector = Collector::new(2);
        collector
            .accepting
            .store(true, std::sync::atomic::Ordering::Release);
        collector.phases[NativePhase::CursorPosition as usize].record(11, false);
        collector.phases[NativePhase::CursorPosition as usize].record(22, true);
        collector.phases[NativePhase::CursorPosition as usize].record(33, false);
        collector.phases[NativePhase::MonitorInfo as usize].record(44, false);

        let snapshot = collector.snapshot();
        let cursor = snapshot
            .phases
            .iter()
            .find(|phase| phase.phase == NativePhase::CursorPosition.name())
            .unwrap();
        assert_eq!(
            (cursor.calls, cursor.recorded, cursor.dropped, cursor.errors),
            (3, 2, 1, 1)
        );
        assert_eq!(cursor.p50_nanos, Some(11));
        assert_eq!(cursor.p95_nanos, Some(22));
        assert_eq!(cursor.max_nanos, Some(22));
        let monitor = snapshot
            .phases
            .iter()
            .find(|phase| phase.phase == NativePhase::MonitorInfo.name())
            .unwrap();
        assert_eq!((monitor.calls, monitor.p50_nanos), (1, Some(44)));
        assert_eq!(snapshot.capacity_per_phase, 2);
    }

    #[test]
    fn coordinate_profile_gdi_and_hud_path_counters_are_separate() {
        let collector = Collector::new(2);
        collector.gdi[GdiObject::Brush as usize].record(true, true);
        collector.gdi[GdiObject::Brush as usize].record(false, false);
        collector.gdi[GdiObject::Font as usize].record(true, false);
        collector.record_hud_path(HudPath::Redraw);
        collector.record_hud_path(HudPath::Redraw);
        collector.record_hud_path(HudPath::Redraw);
        collector.record_hud_path(HudPath::Reposition);
        collector.record_hud_path(HudPath::Reposition);
        collector.record_hud_path(HudPath::Reposition);
        collector.record_hud_path(HudPath::Reposition);
        collector.record_hud_path(HudPath::Reposition);

        let snapshot = collector.snapshot();
        let brush = snapshot
            .gdi_objects
            .iter()
            .find(|object| object.object == "brush")
            .unwrap();
        let font = snapshot
            .gdi_objects
            .iter()
            .find(|object| object.object == "font")
            .unwrap();
        assert_eq!((brush.create_attempts, brush.create_successes), (1, 1));
        assert_eq!((brush.delete_attempts, brush.delete_successes), (1, 0));
        assert_eq!((font.create_attempts, font.create_successes), (1, 0));
        assert_eq!((snapshot.hud_redraws, snapshot.hud_repositions), (3, 5));
        assert_eq!(HudPath::Redraw, HudPath::Redraw);
    }

    #[test]
    fn coordinate_profile_session_is_exclusive_and_requires_worker_stop() {
        let mut session = ProfileSession::start().unwrap();
        assert!(ProfileSession::start().is_err());
        {
            let _worker = session.attach_worker().unwrap();
            assert!(session.attach_worker().is_err());
            let mut timer = PhaseTimer::start(NativePhase::CursorPosition);
            assert!(timer.is_active());
            timer.mark_error();
        }
        let summary = session.finish().unwrap();
        assert_eq!(summary.capacity_per_phase, super::SAMPLE_CAPACITY_PER_PHASE);
        let cursor = summary
            .phases
            .iter()
            .find(|phase| phase.phase == NativePhase::CursorPosition.name())
            .unwrap();
        assert_eq!((cursor.calls, cursor.errors), (1, 1));
        assert!(session.finish().is_err());
        let _next = ProfileSession::start().unwrap();
    }

    #[test]
    fn coordinate_profile_nearest_rank_handles_empty_and_even_samples() {
        assert_eq!(nearest_rank(&[], 50, 100), None);
        assert_eq!(nearest_rank(&[10, 20, 30, 40], 50, 100), Some(20));
        assert_eq!(nearest_rank(&[10, 20, 30, 40], 95, 100), Some(40));
    }
}
