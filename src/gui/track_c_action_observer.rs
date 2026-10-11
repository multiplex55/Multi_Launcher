//! Bounded, test-only phase receipts for Track C action publication.
//!
//! This observer is installed only by an explicitly scoped test fixture. Calls
//! made while it is absent do not read the clock or allocate receipt storage.
//! The phases intentionally overlap at owner boundaries; their durations must
//! not be subtracted to infer costs that were not measured independently.

use std::{cell::RefCell, rc::Rc, time::Instant};

use crate::actions::Action;

const PHASE_COUNT: usize = 15;
const COUNTER_COUNT: usize = 17;

#[derive(Clone, Copy, Debug)]
#[repr(usize)]
pub(super) enum Phase {
    ActionsEvent = 0,
    TypedLoad = 1,
    TailCopy = 2,
    CatalogAssemblyArc = 3,
    SearchEntries = 4,
    FilterMetadata = 5,
    IdMap = 6,
    CompletionInvalidation = 7,
    SameQueryRefresh = 8,
    SearchTotal = 9,
    IndexReadyOwner = 10,
    IndexRequest = 11,
    ScannerWait = 12,
    IndexCustomPrefixCopy = 13,
    IndexTailClone = 14,
}

#[derive(Clone, Copy, Debug)]
#[repr(usize)]
pub(super) enum Counter {
    TypedLoadOutcome = 0,
    TypedActionsLoaded = 1,
    TailActionsCloned = 2,
    IndexCustomActionsCloned = 3,
    IndexTailActionsCloned = 4,
    CatalogAssemblyActions = 5,
    SearchEntriesProcessed = 6,
    FilterEntriesProcessed = 7,
    IdActionsProcessed = 8,
    CacheEntries = 9,
    FilterEntries = 10,
    IdEntries = 11,
    IndexAccepted = 12,
    IndexRejected = 13,
    IndexTailChanged = 14,
    IndexTailEqual = 15,
    SameQueryRefreshCalls = 16,
}

#[derive(Clone, Debug, Default)]
pub(super) struct Snapshot {
    pub phases_ns: [u64; PHASE_COUNT],
    pub phase_calls: [u64; PHASE_COUNT],
    pub counters: [u64; COUNTER_COUNT],
}

impl Snapshot {
    pub(super) fn phase_ns(&self, phase: Phase) -> u64 {
        self.phases_ns[phase as usize]
    }

    pub(super) fn phase_calls(&self, phase: Phase) -> u64 {
        self.phase_calls[phase as usize]
    }

    pub(super) fn counter(&self, counter: Counter) -> u64 {
        self.counters[counter as usize]
    }
}

thread_local! {
    static ACTIVE: RefCell<Option<Rc<RefCell<Snapshot>>>> = const { RefCell::new(None) };
}

pub(super) struct Guard {
    receipt: Rc<RefCell<Snapshot>>,
    previous: Option<Rc<RefCell<Snapshot>>>,
}

impl Guard {
    pub(super) fn install() -> Self {
        let receipt = Rc::new(RefCell::new(Snapshot::default()));
        let previous = ACTIVE.with(|active| active.replace(Some(Rc::clone(&receipt))));
        Self { receipt, previous }
    }

    pub(super) fn snapshot(&self) -> Snapshot {
        self.receipt.borrow().clone()
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        ACTIVE.with(|active| {
            let current = active.replace(self.previous.take());
            debug_assert!(
                current
                    .as_ref()
                    .is_some_and(|current| Rc::ptr_eq(current, &self.receipt)),
                "Track C action observers must be restored in stack order"
            );
        });
    }
}

pub(super) struct Scope {
    receipt: Option<Rc<RefCell<Snapshot>>>,
    phase: Phase,
    started: Option<Instant>,
}

pub(super) fn scope(phase: Phase) -> Scope {
    let receipt = ACTIVE.with(|active| active.borrow().clone());
    let started = receipt.as_ref().map(|_| Instant::now());
    Scope {
        receipt,
        phase,
        started,
    }
}

pub(super) fn is_active() -> bool {
    ACTIVE.with(|active| active.borrow().is_some())
}

/// One observer is moved into the standard `slice::Iter::map` closure used by
/// indexed catalog assembly. The map preserves the slice iterator's size
/// hint/`Extend` behavior; the timer is closed when that iterator is dropped.
pub(super) struct IndexedCloneObserver {
    receipt: Option<Rc<RefCell<Snapshot>>>,
    started: Option<Instant>,
    cloned: usize,
}

pub(super) fn indexed_clone_observer() -> IndexedCloneObserver {
    let receipt = ACTIVE.with(|active| active.borrow().as_ref().cloned());
    IndexedCloneObserver {
        receipt,
        started: None,
        cloned: 0,
    }
}

impl IndexedCloneObserver {
    pub(super) fn clone_action(&mut self, action: &Action) -> Action {
        if self.receipt.is_some() {
            if self.started.is_none() {
                self.started = Some(Instant::now());
            }
            self.cloned = self.cloned.saturating_add(1);
        }
        action.clone()
    }
}

impl Drop for IndexedCloneObserver {
    fn drop(&mut self) {
        let Some(receipt) = &self.receipt else {
            return;
        };
        let Some(started) = self.started else {
            return;
        };
        let nanos = started.elapsed().as_nanos().min(u64::MAX as u128) as u64;
        let mut receipt = receipt.borrow_mut();
        let phase = Phase::IndexTailClone as usize;
        receipt.phases_ns[phase] = receipt.phases_ns[phase].saturating_add(nanos);
        receipt.phase_calls[phase] = receipt.phase_calls[phase].saturating_add(1);
        let counter = Counter::IndexTailActionsCloned as usize;
        receipt.counters[counter] = receipt.counters[counter].saturating_add(self.cloned as u64);
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        let (Some(receipt), Some(started)) = (&self.receipt, self.started) else {
            return;
        };
        let nanos = started.elapsed().as_nanos().min(u64::MAX as u128) as u64;
        let mut receipt = receipt.borrow_mut();
        let phase = self.phase as usize;
        receipt.phases_ns[phase] = receipt.phases_ns[phase].saturating_add(nanos);
        receipt.phase_calls[phase] = receipt.phase_calls[phase].saturating_add(1);
    }
}

pub(super) fn add(counter: Counter, amount: usize) {
    ACTIVE.with(|active| {
        if let Some(receipt) = active.borrow().as_ref() {
            let mut receipt = receipt.borrow_mut();
            let slot = &mut receipt.counters[counter as usize];
            *slot = slot.saturating_add(amount as u64);
        }
    });
}

pub(super) fn set(counter: Counter, value: usize) {
    ACTIVE.with(|active| {
        if let Some(receipt) = active.borrow().as_ref() {
            receipt.borrow_mut().counters[counter as usize] = value as u64;
        }
    });
}

pub(super) fn set_value(counter: Counter, value: u64) {
    ACTIVE.with(|active| {
        if let Some(receipt) = active.borrow().as_ref() {
            receipt.borrow_mut().counters[counter as usize] = value;
        }
    });
}
