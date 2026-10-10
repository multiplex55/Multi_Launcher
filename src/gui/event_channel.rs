use crate::gui::WatchEvent;
use crate::performance::track_c::{self, EventClass, EventOrigin, Outcome, Phase};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvError, RecvTimeoutError, SendError, TryRecvError};
use std::time::Instant;

struct EventEnvelope {
    event: Option<WatchEvent>,
    enqueued_at: Option<Instant>,
    metadata: Option<(EventClass, EventOrigin)>,
    depth: Option<Arc<AtomicUsize>>,
    pending: bool,
    #[cfg(test)]
    age_recorder: Option<Arc<std::sync::Mutex<EventAgeRecorderState>>>,
}

impl EventEnvelope {
    fn new(
        event: WatchEvent,
        origin: EventOrigin,
        observe: bool,
        depth: Option<Arc<AtomicUsize>>,
        #[cfg(test)] age_recorder: Option<Arc<std::sync::Mutex<EventAgeRecorderState>>>,
    ) -> Self {
        Self {
            metadata: observe.then(|| (class_of(&event), origin)),
            event: Some(event),
            enqueued_at: observe.then(Instant::now),
            depth,
            pending: false,
            #[cfg(test)]
            age_recorder,
        }
    }

    fn finish_pending(&mut self) {
        if self.pending {
            self.pending = false;
            if let Some(depth) = &self.depth {
                let _ = depth.fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                    Some(n.saturating_sub(1))
                });
            }
        }
    }

    fn into_event(mut self) -> WatchEvent {
        self.record_dequeue();
        self.event
            .take()
            .expect("event envelope retains its payload")
    }

    fn into_dispatch(mut self) -> EventDispatch {
        self.record_dequeue();
        let handler_timer = self.metadata.map(|(class, origin)| EventHandlerTimer {
            class,
            origin,
            started_at: Instant::now(),
        });
        EventDispatch {
            event: self
                .event
                .take()
                .expect("event envelope retains its payload"),
            _handler_timer: handler_timer,
        }
    }

    fn record_dequeue(&mut self) {
        self.finish_pending();
        if let (Some(enqueued_at), Some((class, origin))) = (self.enqueued_at, self.metadata) {
            let age = enqueued_at.elapsed();
            track_c::record_sample_if(true, Phase::EventEnqueueAge, 1, age, Outcome::Completed);
            track_c::record_event_dequeue_if(true, class, origin, age, false);
            #[cfg(test)]
            if let Some(recorder) = &self.age_recorder {
                EventAgeRecorder::record_delivered(recorder, class, origin, age);
            }
        }
    }

    fn record_abandoned(&self) {
        if let (Some(enqueued_at), Some((class, origin))) = (self.enqueued_at, self.metadata) {
            let age = enqueued_at.elapsed();
            track_c::record_sample_if(true, Phase::EventEnqueueAge, 1, age, Outcome::Abandoned);
            track_c::record_event_dequeue_if(true, class, origin, age, true);
            #[cfg(test)]
            if let Some(recorder) = &self.age_recorder {
                EventAgeRecorder::record_abandoned(recorder);
            }
        }
    }

    fn cancel_pending(&mut self) {
        self.finish_pending();
    }
}

struct EventHandlerTimer {
    class: EventClass,
    origin: EventOrigin,
    started_at: Instant,
}

impl Drop for EventHandlerTimer {
    fn drop(&mut self) {
        track_c::record_event_handler_if(true, self.class, self.origin, self.started_at.elapsed());
    }
}

pub(super) struct EventDispatch {
    pub(super) event: WatchEvent,
    // Kept alive across the complete reducer arm, including early `continue`s.
    _handler_timer: Option<EventHandlerTimer>,
}

impl Drop for EventEnvelope {
    fn drop(&mut self) {
        if self.pending {
            self.record_abandoned();
            self.finish_pending();
        }
    }
}

fn class_of(event: &WatchEvent) -> EventClass {
    match event {
        WatchEvent::Actions
        | WatchEvent::Folders
        | WatchEvent::Bookmarks
        | WatchEvent::Clipboard
        | WatchEvent::Snippets
        | WatchEvent::Notes
        | WatchEvent::Todos
        | WatchEvent::Favorites
        | WatchEvent::Gestures => EventClass::FileReload,
        WatchEvent::IndexReady => EventClass::IndexCompletion,
        WatchEvent::ExecuteAction(_) => EventClass::Action,
        WatchEvent::RadialDispatch(_)
        | WatchEvent::RadialPrepare(_)
        | WatchEvent::RadialResolveDeferred(_)
        | WatchEvent::RadialDeferredSearchReady { .. }
        | WatchEvent::RadialAuthoringSearchReady { .. }
        | WatchEvent::RadialAuthoringSearchFailed { .. }
        | WatchEvent::AuthoringProviderCapacityAvailable
        | WatchEvent::RadialInvalidate
        | WatchEvent::RadialConfigDiagnostic(_)
        | WatchEvent::RadialRuntimeDiagnostic(_)
        | WatchEvent::RadialDiagnostic(_)
        | WatchEvent::RadialSubmenuPlacementFailure(_)
        | WatchEvent::RadialPlacementActionResult { .. }
        | WatchEvent::RadialMigrationNotice(_)
        | WatchEvent::RadialMigrationState { .. } => EventClass::Radial,
        WatchEvent::ScreenDrawStart
        | WatchEvent::ScreenDrawRecover(_)
        | WatchEvent::ScreenDrawEmergency(_) => EventClass::Recovery,
        WatchEvent::ClipboardModify(_) | WatchEvent::Recycle(_) => EventClass::Clipboard,
        WatchEvent::Dashboard(_) => EventClass::Dashboard,
        WatchEvent::VirtualDesktop(_) => EventClass::Other,
    }
}

#[derive(Clone)]
pub(super) struct EventSender {
    tx: mpsc::Sender<EventEnvelope>,
    depth: Option<Arc<AtomicUsize>>,
    origin: EventOrigin,
    observe: bool,
    #[cfg(test)]
    age_recorder: Option<Arc<std::sync::Mutex<EventAgeRecorderState>>>,
}

impl EventSender {
    pub(super) fn with_origin(&self, origin: EventOrigin) -> Self {
        Self {
            tx: self.tx.clone(),
            depth: self.depth.clone(),
            origin,
            observe: self.observe,
            #[cfg(test)]
            age_recorder: self.age_recorder.clone(),
        }
    }

    pub(super) fn send(&self, event: WatchEvent) -> Result<(), SendError<WatchEvent>> {
        self.send_as(event, self.origin)
    }

    pub(super) fn send_as(
        &self,
        event: WatchEvent,
        origin: EventOrigin,
    ) -> Result<(), SendError<WatchEvent>> {
        let observing = self.observe;
        let mut envelope = EventEnvelope::new(
            event,
            origin,
            observing,
            self.depth.clone(),
            #[cfg(test)]
            self.age_recorder.clone(),
        );
        let metadata = envelope.metadata;
        if let Some(depth) = &self.depth {
            depth.fetch_add(1, Ordering::AcqRel);
            envelope.pending = true;
        }
        match self.tx.send(envelope) {
            Ok(()) => {
                #[cfg(test)]
                if let Some(recorder) = &self.age_recorder {
                    EventAgeRecorder::record_successful_send(recorder);
                }
                if let Some((class, origin)) = metadata {
                    track_c::record_event_enqueue_if(observing, class, origin);
                }
                Ok(())
            }
            Err(error) => {
                let mut envelope = error.0;
                envelope.cancel_pending();
                if let Some((class, origin)) = envelope.metadata {
                    track_c::record_event_send_failure_if(observing, class, origin);
                }
                Err(SendError(
                    envelope.event.take().expect("failed send retains event"),
                ))
            }
        }
    }

    pub(super) fn depth(&self) -> Option<usize> {
        self.depth
            .as_ref()
            .map(|depth| depth.load(Ordering::Acquire))
    }
}

pub struct EventReceiver {
    rx: Receiver<EventEnvelope>,
    depth: Option<Arc<AtomicUsize>>,
}

impl EventReceiver {
    fn dequeue(&self, envelope: EventEnvelope) -> WatchEvent {
        envelope.into_event()
    }

    pub(super) fn try_recv_for_dispatch(&self) -> Result<EventDispatch, TryRecvError> {
        self.rx.try_recv().map(EventEnvelope::into_dispatch)
    }

    pub fn try_recv(&self) -> Result<WatchEvent, TryRecvError> {
        self.rx.try_recv().map(|envelope| self.dequeue(envelope))
    }

    pub fn recv(&self) -> Result<WatchEvent, RecvError> {
        self.rx.recv().map(|envelope| self.dequeue(envelope))
    }

    pub fn recv_timeout(
        &self,
        timeout: std::time::Duration,
    ) -> Result<WatchEvent, RecvTimeoutError> {
        self.rx
            .recv_timeout(timeout)
            .map(|envelope| self.dequeue(envelope))
    }

    /// Current backlog in the observed app-owned event channel, excluding
    /// global registry bookkeeping and pending-before-owner events. Returns
    /// `None` when performance diagnostics were disabled when this channel was
    /// created.
    pub fn queued_depth(&self) -> Option<usize> {
        self.depth
            .as_ref()
            .map(|depth| depth.load(Ordering::Acquire))
    }
}

pub(super) fn channel() -> (EventSender, EventReceiver) {
    channel_with_observation(crate::performance::enabled())
}

pub(super) fn channel_with_observation(observe: bool) -> (EventSender, EventReceiver) {
    channel_inner(
        observe,
        #[cfg(test)]
        None,
    )
}

fn channel_inner(
    observe: bool,
    #[cfg(test)] age_recorder: Option<Arc<std::sync::Mutex<EventAgeRecorderState>>>,
) -> (EventSender, EventReceiver) {
    let (tx, rx) = mpsc::channel();
    let depth = observe.then(|| Arc::new(AtomicUsize::new(0)));
    (
        EventSender {
            tx,
            depth: depth.clone(),
            origin: EventOrigin::Gui,
            observe,
            #[cfg(test)]
            age_recorder: age_recorder.clone(),
        },
        EventReceiver { rx, depth },
    )
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct EventAgeSample {
    pub age_nanos: u64,
    pub class: EventClass,
    pub origin: EventOrigin,
}

#[cfg(test)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct EventAgeSnapshot {
    pub delivered: Vec<EventAgeSample>,
    pub overflow: usize,
    pub abandoned: usize,
}

#[cfg(test)]
impl EventAgeSnapshot {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            delivered: Vec::with_capacity(capacity),
            overflow: 0,
            abandoned: 0,
        }
    }
}

#[cfg(test)]
#[derive(Debug)]
struct EventAgeRecorderState {
    capacity: usize,
    successful_sends: usize,
    snapshot: EventAgeSnapshot,
}

#[cfg(test)]
#[derive(Clone, Debug)]
pub(super) struct EventAgeRecorder {
    state: Arc<std::sync::Mutex<EventAgeRecorderState>>,
}

#[cfg(test)]
impl EventAgeRecorder {
    pub(super) fn new(capacity: usize) -> Self {
        Self {
            state: Arc::new(std::sync::Mutex::new(EventAgeRecorderState {
                capacity,
                successful_sends: 0,
                snapshot: EventAgeSnapshot::with_capacity(capacity),
            })),
        }
    }

    fn record_successful_send(state: &Arc<std::sync::Mutex<EventAgeRecorderState>>) {
        if let Ok(mut state) = state.lock() {
            state.successful_sends = state.successful_sends.saturating_add(1);
        }
    }

    fn record_delivered(
        state: &Arc<std::sync::Mutex<EventAgeRecorderState>>,
        class: EventClass,
        origin: EventOrigin,
        age: std::time::Duration,
    ) {
        if let Ok(mut state) = state.lock() {
            if state.snapshot.delivered.len() == state.capacity {
                state.snapshot.overflow = state.snapshot.overflow.saturating_add(1);
            } else {
                state.snapshot.delivered.push(EventAgeSample {
                    age_nanos: age.as_nanos().min(u64::MAX as u128) as u64,
                    class,
                    origin,
                });
            }
        }
    }

    fn record_abandoned(state: &Arc<std::sync::Mutex<EventAgeRecorderState>>) {
        if let Ok(mut state) = state.lock() {
            state.snapshot.abandoned = state.snapshot.abandoned.saturating_add(1);
        }
    }

    pub(super) fn snapshot(&self) -> EventAgeSnapshot {
        self.state
            .lock()
            .map(|state| state.snapshot.clone())
            .unwrap_or_default()
    }

    pub(super) fn reset(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.successful_sends = 0;
            state.snapshot.delivered.clear();
            state.snapshot.overflow = 0;
            state.snapshot.abandoned = 0;
        }
    }

    pub(super) fn successful_sends(&self) -> usize {
        self.state
            .lock()
            .map(|state| state.successful_sends)
            .unwrap_or_default()
    }
}

#[cfg(test)]
pub(super) fn channel_with_test_age_recorder(
    capacity: usize,
) -> (EventSender, EventReceiver, EventAgeRecorder) {
    let recorder = EventAgeRecorder::new(capacity);
    let (sender, receiver) = channel_inner(true, Some(Arc::clone(&recorder.state)));
    (sender, receiver, recorder)
}

#[cfg(test)]
mod tests {
    use crate::gui::WatchEvent;
    use crate::performance::track_c::{EventClass, EventOrigin};

    #[test]
    fn track_c_event_direct_send_preserves_fifo_depth_and_plain_event_shape() {
        let (tx, rx) = super::channel_with_observation(true);
        let tx = tx.with_origin(EventOrigin::FileWatcher);
        tx.send(WatchEvent::Folders).unwrap();
        tx.send(WatchEvent::Bookmarks).unwrap();
        assert_eq!(tx.depth(), Some(2));
        assert!(matches!(rx.try_recv(), Ok(WatchEvent::Folders)));
        assert_eq!(rx.queued_depth(), Some(1));
        assert!(matches!(rx.try_recv(), Ok(WatchEvent::Bookmarks)));
        assert_eq!(rx.queued_depth(), Some(0));
    }

    #[test]
    fn track_c_event_send_failure_and_late_sender_keep_depth_zero() {
        let (tx, rx) = super::channel_with_observation(false);
        drop(rx);
        assert!(tx.send(WatchEvent::Actions).is_err());
        assert_eq!(tx.depth(), None);
        let (tx, rx) = super::channel_with_observation(true);
        let late = tx.clone();
        drop(rx);
        assert!(late.send(WatchEvent::Actions).is_err());
        assert_eq!(late.depth(), Some(0));
    }

    #[test]
    fn track_c_event_receiver_drop_accounts_queued_and_racing_sends() {
        let (tx, rx) = super::channel_with_observation(true);
        let start = std::sync::Arc::new(std::sync::Barrier::new(5));
        let workers = (0..4)
            .map(|_| {
                let sender = tx.clone();
                let start = std::sync::Arc::clone(&start);
                std::thread::spawn(move || {
                    start.wait();
                    for _ in 0..1_000 {
                        if sender.send(WatchEvent::IndexReady).is_err() {
                            break;
                        }
                    }
                })
            })
            .collect::<Vec<_>>();
        start.wait();
        drop(rx);
        for worker in workers {
            worker.join().unwrap();
        }
        assert_eq!(tx.depth(), Some(0));
    }

    #[test]
    fn track_c_event_metadata_is_static_and_payload_independent() {
        let envelope = super::EventEnvelope::new(
            WatchEvent::RadialRuntimeDiagnostic("payload is not retained in metadata".into()),
            EventOrigin::RadialProvider,
            true,
            None,
            None,
        );
        assert_eq!(
            envelope.metadata,
            Some((EventClass::Radial, EventOrigin::RadialProvider))
        );
        assert!(envelope.enqueued_at.is_some());
    }

    #[test]
    fn track_c_event_dispatch_times_the_full_handler_by_class_and_origin() {
        let pair = |snapshot: crate::performance::track_c::EventSnapshot| {
            snapshot.class == EventClass::FileReload && snapshot.origin == EventOrigin::Hotkey
        };
        let before = crate::performance::track_c::snapshot_events()
            .into_iter()
            .find(|snapshot| pair(*snapshot))
            .unwrap();
        let (tx, rx) = super::channel_with_observation(true);
        tx.with_origin(EventOrigin::Hotkey)
            .send(WatchEvent::Actions)
            .unwrap();
        let delivery = rx.try_recv_for_dispatch().unwrap();
        assert!(matches!(delivery.event, WatchEvent::Actions));
        drop(delivery);
        let after = crate::performance::track_c::snapshot_events()
            .into_iter()
            .find(|snapshot| pair(*snapshot))
            .unwrap();
        assert_eq!(after.handler_calls, before.handler_calls + 1);
    }

    #[test]
    fn track_c_event_dispatch_keeps_non_opt_in_receipt_unobserved() {
        let (tx, rx) = super::channel_with_observation(false);
        tx.with_origin(EventOrigin::Hotkey)
            .send(WatchEvent::Actions)
            .unwrap();
        let delivery = rx.try_recv_for_dispatch().unwrap();
        assert!(delivery._handler_timer.is_none());
        assert_eq!(rx.queued_depth(), None);
    }

    #[test]
    fn track_c_event_age_recorder_captures_only_successful_bounded_dequeues() {
        let (tx, rx, recorder) = super::channel_with_test_age_recorder(2);
        tx.with_origin(EventOrigin::FileWatcher)
            .send(WatchEvent::Actions)
            .unwrap();
        tx.with_origin(EventOrigin::IndexCoordinator)
            .send(WatchEvent::IndexReady)
            .unwrap();
        assert!(matches!(rx.try_recv(), Ok(WatchEvent::Actions)));
        assert!(matches!(rx.try_recv_for_dispatch(), Ok(_)));
        let snapshot = recorder.snapshot();
        assert_eq!(snapshot.delivered.len(), 2);
        assert_eq!(snapshot.overflow, 0);
        assert_eq!(snapshot.abandoned, 0);
        assert_eq!(snapshot.delivered[0].class, EventClass::FileReload);
        assert_eq!(snapshot.delivered[0].origin, EventOrigin::FileWatcher);
        assert_eq!(snapshot.delivered[1].class, EventClass::IndexCompletion);
        assert_eq!(snapshot.delivered[1].origin, EventOrigin::IndexCoordinator);

        recorder.reset();
        tx.with_origin(EventOrigin::Gui)
            .send(WatchEvent::Actions)
            .unwrap();
        tx.with_origin(EventOrigin::Gui)
            .send(WatchEvent::Folders)
            .unwrap();
        tx.with_origin(EventOrigin::Gui)
            .send(WatchEvent::Bookmarks)
            .unwrap();
        assert!(matches!(rx.try_recv(), Ok(_)));
        assert!(matches!(rx.try_recv(), Ok(_)));
        assert!(matches!(rx.try_recv(), Ok(_)));
        let snapshot = recorder.snapshot();
        assert_eq!(snapshot.delivered.len(), 2);
        assert_eq!(snapshot.overflow, 1, "overflow fails a benchmark receipt");
        assert_eq!(snapshot.abandoned, 0);
    }

    #[test]
    fn track_c_event_age_recorder_acknowledges_only_committed_sends() {
        let (tx, rx, recorder) = super::channel_with_test_age_recorder(2);
        assert_eq!(recorder.successful_sends(), 0);
        tx.send(WatchEvent::Actions).unwrap();
        assert_eq!(recorder.successful_sends(), 1);
        drop(rx);
        assert!(tx.send(WatchEvent::Folders).is_err());
        assert_eq!(recorder.successful_sends(), 1);
    }

    #[test]
    fn track_c_event_age_recorder_excludes_abandoned_envelopes() {
        let (tx, rx, recorder) = super::channel_with_test_age_recorder(2);
        tx.with_origin(EventOrigin::Dashboard)
            .send(WatchEvent::Actions)
            .unwrap();
        drop(rx);
        let snapshot = recorder.snapshot();
        assert!(snapshot.delivered.is_empty());
        assert_eq!(snapshot.overflow, 0);
        assert_eq!(snapshot.abandoned, 1);
    }
}
