//! Fixed-cardinality, opt-in measurements for the Track C responsiveness work.
//!
//! This collector is deliberately separate from the historical `Metric`
//! contract in `performance.rs`; adding Track C phases must not renumber or
//! resize that collector.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum Phase {
    NoteRelationshipRefresh,
    NoteMentionsScan,
    ActionPublishPrepare,
    ActionPublishCommit,
    SearchScoreAndCloneHits,
    SearchMoveResults,
    RootGeometryCold,
    NotesGeometryCold,
    EventEnqueueAge,
    EventDrain,
    StartupCatalogReady,
    HotkeyFirstUsableFrame,
}

impl Phase {
    pub const COUNT: usize = 12;
    const ALL: [Self; Self::COUNT] = [
        Self::NoteRelationshipRefresh,
        Self::NoteMentionsScan,
        Self::ActionPublishPrepare,
        Self::ActionPublishCommit,
        Self::SearchScoreAndCloneHits,
        Self::SearchMoveResults,
        Self::RootGeometryCold,
        Self::NotesGeometryCold,
        Self::EventEnqueueAge,
        Self::EventDrain,
        Self::StartupCatalogReady,
        Self::HotkeyFirstUsableFrame,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::NoteRelationshipRefresh => "note.relationship_refresh",
            Self::NoteMentionsScan => "note.mentions_scan",
            Self::ActionPublishPrepare => "action.publish_prepare",
            Self::ActionPublishCommit => "action.publish_commit",
            Self::SearchScoreAndCloneHits => "search.score_and_clone_hits",
            Self::SearchMoveResults => "search.move_results",
            Self::RootGeometryCold => "root.geometry_cold",
            Self::NotesGeometryCold => "notes.geometry_cold",
            Self::EventEnqueueAge => "event.enqueue_age",
            Self::EventDrain => "event.drain",
            Self::StartupCatalogReady => "startup.catalog_ready",
            Self::HotkeyFirstUsableFrame => "hotkey.to_first_usable_frame",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Completed,
    Error,
    Abandoned,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum EventClass {
    FileReload,
    IndexCompletion,
    Action,
    Radial,
    Recovery,
    Clipboard,
    Dashboard,
    Other,
}

impl EventClass {
    pub const COUNT: usize = 8;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum EventOrigin {
    Gui,
    FileWatcher,
    IndexCoordinator,
    RadialProvider,
    CommandHost,
    Dashboard,
    Registry,
    Startup,
    Hotkey,
}

impl EventOrigin {
    pub const COUNT: usize = 9;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EventSnapshot {
    pub class: EventClass,
    pub origin: EventOrigin,
    pub enqueued: u64,
    pub dequeued: u64,
    pub send_failed: u64,
    pub abandoned: u64,
    pub age_nanos_total: u64,
    pub age_nanos_max: u64,
    /// Time spent handling dequeued events, attributed by static class/origin.
    pub handler_calls: u64,
    pub handler_nanos_total: u64,
    pub handler_nanos_max: u64,
}

#[derive(Debug, Default)]
struct EventCounters {
    enqueued: AtomicU64,
    dequeued: AtomicU64,
    send_failed: AtomicU64,
    abandoned: AtomicU64,
    age_nanos_total: AtomicU64,
    age_nanos_max: AtomicU64,
    handler_calls: AtomicU64,
    handler_nanos_total: AtomicU64,
    handler_nanos_max: AtomicU64,
}

impl EventCounters {
    fn record(&self, elapsed: Option<Duration>, outcome: EventOutcome) {
        let counter = match outcome {
            EventOutcome::Enqueued => &self.enqueued,
            EventOutcome::Dequeued => &self.dequeued,
            EventOutcome::SendFailed => &self.send_failed,
            EventOutcome::Abandoned => &self.abandoned,
        };
        Counters::add_saturating(counter, 1);
        if let Some(elapsed) = elapsed {
            let nanos = elapsed.as_nanos().min(u64::MAX as u128) as u64;
            Counters::add_saturating(&self.age_nanos_total, nanos);
            self.age_nanos_max.fetch_max(nanos, Ordering::Relaxed);
        }
    }

    fn snapshot(&self, class: EventClass, origin: EventOrigin) -> EventSnapshot {
        EventSnapshot {
            class,
            origin,
            enqueued: self.enqueued.load(Ordering::Relaxed),
            dequeued: self.dequeued.load(Ordering::Relaxed),
            send_failed: self.send_failed.load(Ordering::Relaxed),
            abandoned: self.abandoned.load(Ordering::Relaxed),
            age_nanos_total: self.age_nanos_total.load(Ordering::Relaxed),
            age_nanos_max: self.age_nanos_max.load(Ordering::Relaxed),
            handler_calls: self.handler_calls.load(Ordering::Relaxed),
            handler_nanos_total: self.handler_nanos_total.load(Ordering::Relaxed),
            handler_nanos_max: self.handler_nanos_max.load(Ordering::Relaxed),
        }
    }

    fn record_handler(&self, elapsed: Duration) {
        let nanos = elapsed.as_nanos().min(u64::MAX as u128) as u64;
        Counters::add_saturating(&self.handler_calls, 1);
        Counters::add_saturating(&self.handler_nanos_total, nanos);
        self.handler_nanos_max.fetch_max(nanos, Ordering::Relaxed);
    }

    fn reset(&self) {
        self.enqueued.store(0, Ordering::Relaxed);
        self.dequeued.store(0, Ordering::Relaxed);
        self.send_failed.store(0, Ordering::Relaxed);
        self.abandoned.store(0, Ordering::Relaxed);
        self.age_nanos_total.store(0, Ordering::Relaxed);
        self.age_nanos_max.store(0, Ordering::Relaxed);
        self.handler_calls.store(0, Ordering::Relaxed);
        self.handler_nanos_total.store(0, Ordering::Relaxed);
        self.handler_nanos_max.store(0, Ordering::Relaxed);
    }
}

#[derive(Clone, Copy)]
enum EventOutcome {
    Enqueued,
    Dequeued,
    SendFailed,
    Abandoned,
}

struct EventCollector([EventCounters; EventClass::COUNT * EventOrigin::COUNT]);

impl EventCollector {
    fn new() -> Self {
        Self(std::array::from_fn(|_| EventCounters::default()))
    }

    fn index(class: EventClass, origin: EventOrigin) -> usize {
        class as usize * EventOrigin::COUNT + origin as usize
    }

    fn record(
        &self,
        class: EventClass,
        origin: EventOrigin,
        elapsed: Option<Duration>,
        outcome: EventOutcome,
    ) {
        self.0[Self::index(class, origin)].record(elapsed, outcome);
    }

    fn record_handler(&self, class: EventClass, origin: EventOrigin, elapsed: Duration) {
        self.0[Self::index(class, origin)].record_handler(elapsed);
    }

    fn snapshot(&self) -> [EventSnapshot; EventClass::COUNT * EventOrigin::COUNT] {
        std::array::from_fn(|index| {
            let class = EventClass::ALL[index / EventOrigin::COUNT];
            let origin = EventOrigin::ALL[index % EventOrigin::COUNT];
            self.0[index].snapshot(class, origin)
        })
    }

    fn reset(&self) {
        for counters in &self.0 {
            counters.reset();
        }
    }
}

impl EventClass {
    const ALL: [Self; Self::COUNT] = [
        Self::FileReload,
        Self::IndexCompletion,
        Self::Action,
        Self::Radial,
        Self::Recovery,
        Self::Clipboard,
        Self::Dashboard,
        Self::Other,
    ];
}

impl EventOrigin {
    const ALL: [Self; Self::COUNT] = [
        Self::Gui,
        Self::FileWatcher,
        Self::IndexCoordinator,
        Self::RadialProvider,
        Self::CommandHost,
        Self::Dashboard,
        Self::Registry,
        Self::Startup,
        Self::Hotkey,
    ];
}

static EVENTS: std::sync::OnceLock<EventCollector> = std::sync::OnceLock::new();

fn event_collector() -> &'static EventCollector {
    EVENTS.get_or_init(EventCollector::new)
}

pub(crate) fn record_event_enqueue_if(enabled: bool, class: EventClass, origin: EventOrigin) {
    if enabled {
        event_collector().record(class, origin, None, EventOutcome::Enqueued);
    }
}

pub(crate) fn record_event_send_failure_if(enabled: bool, class: EventClass, origin: EventOrigin) {
    if enabled {
        event_collector().record(class, origin, None, EventOutcome::SendFailed);
    }
}

pub(crate) fn record_event_dequeue_if(
    enabled: bool,
    class: EventClass,
    origin: EventOrigin,
    age: Duration,
    abandoned: bool,
) {
    if enabled {
        event_collector().record(
            class,
            origin,
            Some(age),
            if abandoned {
                EventOutcome::Abandoned
            } else {
                EventOutcome::Dequeued
            },
        );
    }
}

pub(crate) fn record_event_handler_if(
    enabled: bool,
    class: EventClass,
    origin: EventOrigin,
    elapsed: Duration,
) {
    if enabled {
        event_collector().record_handler(class, origin, elapsed);
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub calls: u64,
    pub work_units: u64,
    /// Search candidates for which exact/fuzzy matching ran. Zero for other
    /// phases.
    pub candidates_scored: u64,
    /// Local Action values cloned into hits; final result materialization moves
    /// are reported separately by `work_units` on `SearchMoveResults`.
    pub action_clones: u64,
    pub elapsed_nanos_total: u64,
    pub elapsed_nanos_max: u64,
    pub completed: u64,
    pub errors: u64,
    pub abandoned: u64,
}

#[derive(Debug, Default)]
struct Counters {
    calls: AtomicU64,
    work_units: AtomicU64,
    candidates_scored: AtomicU64,
    action_clones: AtomicU64,
    elapsed_nanos_total: AtomicU64,
    elapsed_nanos_max: AtomicU64,
    completed: AtomicU64,
    errors: AtomicU64,
    abandoned: AtomicU64,
}

impl Counters {
    fn add_saturating(counter: &AtomicU64, amount: u64) {
        let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
            Some(old.saturating_add(amount))
        });
    }

    fn record(
        &self,
        work_units: u64,
        candidates_scored: u64,
        action_clones: u64,
        elapsed: Duration,
        outcome: Outcome,
    ) {
        let nanos = elapsed.as_nanos().min(u64::MAX as u128) as u64;
        Self::add_saturating(&self.calls, 1);
        Self::add_saturating(&self.work_units, work_units);
        Self::add_saturating(&self.candidates_scored, candidates_scored);
        Self::add_saturating(&self.action_clones, action_clones);
        Self::add_saturating(&self.elapsed_nanos_total, nanos);
        self.elapsed_nanos_max.fetch_max(nanos, Ordering::Relaxed);
        let counter = match outcome {
            Outcome::Completed => &self.completed,
            Outcome::Error => &self.errors,
            Outcome::Abandoned => &self.abandoned,
        };
        Self::add_saturating(counter, 1);
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            calls: self.calls.load(Ordering::Relaxed),
            work_units: self.work_units.load(Ordering::Relaxed),
            candidates_scored: self.candidates_scored.load(Ordering::Relaxed),
            action_clones: self.action_clones.load(Ordering::Relaxed),
            elapsed_nanos_total: self.elapsed_nanos_total.load(Ordering::Relaxed),
            elapsed_nanos_max: self.elapsed_nanos_max.load(Ordering::Relaxed),
            completed: self.completed.load(Ordering::Relaxed),
            errors: self.errors.load(Ordering::Relaxed),
            abandoned: self.abandoned.load(Ordering::Relaxed),
        }
    }

    fn reset(&self) {
        self.calls.store(0, Ordering::Relaxed);
        self.work_units.store(0, Ordering::Relaxed);
        self.candidates_scored.store(0, Ordering::Relaxed);
        self.action_clones.store(0, Ordering::Relaxed);
        self.elapsed_nanos_total.store(0, Ordering::Relaxed);
        self.elapsed_nanos_max.store(0, Ordering::Relaxed);
        self.completed.store(0, Ordering::Relaxed);
        self.errors.store(0, Ordering::Relaxed);
        self.abandoned.store(0, Ordering::Relaxed);
    }
}

#[derive(Debug)]
struct Collector([Counters; Phase::COUNT]);

impl Collector {
    fn new() -> Self {
        Self(std::array::from_fn(|_| Counters::default()))
    }

    fn record_if(
        &self,
        enabled: bool,
        phase: Phase,
        work_units: usize,
        elapsed: Duration,
        outcome: Outcome,
    ) {
        self.record_with_search_counts_if(enabled, phase, work_units, 0, 0, elapsed, outcome);
    }

    fn record_with_search_counts_if(
        &self,
        enabled: bool,
        phase: Phase,
        work_units: usize,
        candidates_scored: usize,
        action_clones: usize,
        elapsed: Duration,
        outcome: Outcome,
    ) {
        if enabled {
            self.0[phase as usize].record(
                work_units.min(u64::MAX as usize) as u64,
                candidates_scored.min(u64::MAX as usize) as u64,
                action_clones.min(u64::MAX as usize) as u64,
                elapsed,
                outcome,
            );
        }
    }

    fn snapshot(&self) -> [(Phase, Snapshot); Phase::COUNT] {
        std::array::from_fn(|index| (Phase::ALL[index], self.0[index].snapshot()))
    }

    fn reset(&self) {
        for counters in &self.0 {
            counters.reset();
        }
    }
}

static COLLECTOR: std::sync::OnceLock<Collector> = std::sync::OnceLock::new();

fn collector() -> &'static Collector {
    COLLECTOR.get_or_init(Collector::new)
}

pub struct Timer {
    phase: Phase,
    work_units: u64,
    candidates_scored: u64,
    action_clones: u64,
    started: Option<Instant>,
}

impl Timer {
    pub fn start(phase: Phase) -> Self {
        Self::start_if(phase, super::enabled())
    }

    pub fn start_if(phase: Phase, enabled: bool) -> Self {
        Self {
            phase,
            work_units: 0,
            candidates_scored: 0,
            action_clones: 0,
            started: enabled.then(Instant::now),
        }
    }

    pub fn set_work_units(&mut self, work_units: usize) {
        if self.started.is_some() {
            self.work_units = work_units.min(u64::MAX as usize) as u64;
        }
    }

    pub fn add_work_units(&mut self, work_units: usize) {
        if self.started.is_some() {
            self.work_units = self
                .work_units
                .saturating_add(work_units.min(u64::MAX as usize) as u64);
        }
    }

    /// Set search-only details separately from the examined-catalog work
    /// units.
    pub fn set_search_counts(&mut self, candidates_scored: usize, action_clones: usize) {
        if self.started.is_some() {
            self.candidates_scored = candidates_scored.min(u64::MAX as usize) as u64;
            self.action_clones = action_clones.min(u64::MAX as usize) as u64;
        }
    }

    pub fn finish(mut self, outcome: Outcome) {
        if let Some(started) = self.started.take() {
            collector().record_with_search_counts_if(
                true,
                self.phase,
                usize::try_from(self.work_units).unwrap_or(usize::MAX),
                usize::try_from(self.candidates_scored).unwrap_or(usize::MAX),
                usize::try_from(self.action_clones).unwrap_or(usize::MAX),
                started.elapsed(),
                outcome,
            );
        }
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        if let Some(started) = self.started.take() {
            collector().record_with_search_counts_if(
                true,
                self.phase,
                usize::try_from(self.work_units).unwrap_or(usize::MAX),
                usize::try_from(self.candidates_scored).unwrap_or(usize::MAX),
                usize::try_from(self.action_clones).unwrap_or(usize::MAX),
                started.elapsed(),
                Outcome::Abandoned,
            );
        }
    }
}

pub(crate) fn record_sample_if(
    enabled: bool,
    phase: Phase,
    work_units: usize,
    elapsed: Duration,
    outcome: Outcome,
) {
    if !enabled {
        return;
    }
    collector().record_if(true, phase, work_units, elapsed, outcome);
}

#[cfg(test)]
pub(crate) fn reset_for_tests() {
    collector().reset();
}

#[cfg(test)]
pub(crate) fn snapshot_for_tests() -> [(Phase, Snapshot); Phase::COUNT] {
    collector().snapshot()
}

/// Snapshot the fixed set of Track C counters. Concurrent samples may straddle
/// this read; every collection and update has fixed memory cost.
pub fn snapshot() -> [(Phase, Snapshot); Phase::COUNT] {
    collector().snapshot()
}

/// Reset Track C counters between controlled runs while writers are quiescent.
pub fn reset() {
    collector().reset();
    event_collector().reset();
}

/// Snapshot the bounded event class/origin matrix. Entries are static and do
/// not retain event payloads or producer-specific strings.
pub fn snapshot_events() -> [EventSnapshot; EventClass::COUNT * EventOrigin::COUNT] {
    event_collector().snapshot()
}

#[cfg(test)]
mod tests {
    use super::{Collector, EventClass, EventCollector, EventOrigin, Outcome, Phase};
    use std::time::Duration;

    #[test]
    fn track_c_metric_disabled_samples_do_not_start_or_record() {
        let timer = super::Timer::start_if(Phase::EventDrain, false);
        assert!(timer.started.is_none());
        timer.finish(Outcome::Completed);
        let collector = Collector::new();
        collector.record_if(
            false,
            Phase::EventDrain,
            4,
            Duration::from_nanos(9),
            Outcome::Error,
        );
        assert_eq!(
            collector.snapshot()[Phase::EventDrain as usize].1,
            Default::default()
        );
    }

    #[test]
    fn track_c_metric_disabled_entrypoint_does_not_record() {
        let phase = Phase::StartupCatalogReady;
        let before = super::snapshot()[phase as usize].1;
        super::record_sample_if(false, phase, 1, Duration::from_nanos(9), Outcome::Completed);
        assert_eq!(super::snapshot()[phase as usize].1, before);
    }

    #[test]
    fn track_c_metric_counts_saturate_reset_and_preserve_outcomes() {
        let collector = Collector::new();
        let counters = &collector.0[Phase::SearchScoreAndCloneHits as usize];
        counters
            .calls
            .store(u64::MAX - 1, std::sync::atomic::Ordering::Relaxed);
        counters
            .work_units
            .store(u64::MAX - 2, std::sync::atomic::Ordering::Relaxed);
        counters
            .candidates_scored
            .store(u64::MAX - 1, std::sync::atomic::Ordering::Relaxed);
        counters
            .action_clones
            .store(u64::MAX - 1, std::sync::atomic::Ordering::Relaxed);
        collector.0[Phase::SearchScoreAndCloneHits as usize].record(
            u64::MAX - 2,
            3,
            4,
            Duration::from_nanos(7),
            Outcome::Error,
        );
        collector.0[Phase::SearchScoreAndCloneHits as usize].record(
            9,
            5,
            6,
            Duration::from_nanos(11),
            Outcome::Abandoned,
        );
        let snapshot = collector.snapshot()[Phase::SearchScoreAndCloneHits as usize].1;
        assert_eq!(snapshot.calls, u64::MAX);
        assert_eq!(snapshot.work_units, u64::MAX);
        assert_eq!(snapshot.candidates_scored, u64::MAX);
        assert_eq!(snapshot.action_clones, u64::MAX);
        assert_eq!(snapshot.elapsed_nanos_total, 18);
        assert_eq!(snapshot.elapsed_nanos_max, 11);
        assert_eq!((snapshot.errors, snapshot.abandoned), (1, 1));
        collector.reset();
        assert_eq!(
            collector.snapshot()[Phase::SearchScoreAndCloneHits as usize].1,
            Default::default()
        );
    }

    #[test]
    fn track_c_event_matrix_attributes_static_class_and_origin() {
        let events = EventCollector::new();
        events.record(
            EventClass::IndexCompletion,
            EventOrigin::IndexCoordinator,
            None,
            super::EventOutcome::Enqueued,
        );
        events.record_handler(
            EventClass::IndexCompletion,
            EventOrigin::IndexCoordinator,
            Duration::from_nanos(29),
        );
        events.record(
            EventClass::IndexCompletion,
            EventOrigin::IndexCoordinator,
            Some(Duration::from_nanos(17)),
            super::EventOutcome::Dequeued,
        );
        let index = EventClass::IndexCompletion as usize * EventOrigin::COUNT
            + EventOrigin::IndexCoordinator as usize;
        let snapshot = events.snapshot()[index];
        assert_eq!((snapshot.enqueued, snapshot.dequeued), (1, 1));
        assert_eq!(snapshot.age_nanos_total, 17);
        assert_eq!(snapshot.age_nanos_max, 17);
        assert_eq!(snapshot.handler_calls, 1);
        assert_eq!(snapshot.handler_nanos_total, 29);
        assert_eq!(snapshot.handler_nanos_max, 29);
        assert_eq!(snapshot.send_failed, 0);
        assert_eq!(snapshot.abandoned, 0);
    }
}
