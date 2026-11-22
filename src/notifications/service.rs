//! Profile-owned worker for bounded, typed Inbox changes.
use super::{
    ActionKind, Category, FactId, Inbox, InboxStore, Ingest, InternalFact, Record, RelatedItem,
    Settings, Source, Status, WindowsCapture, WindowsCaptureView, WindowsSourceAction,
    create_todo_from_action,
};
use crate::platform::app_data::AppDataRoot;
use crate::plugins::calendar::CalendarInboxReminders;
use std::{
    collections::VecDeque,
    sync::{
        Arc, RwLock, TryLockError,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const QUEUE_LIMIT: usize = 256;
const RECEIPT_LIMIT: usize = 64;

#[derive(Clone, Debug)]
pub(crate) struct InternalEvent {
    pub id: FactId,
    pub occurred_at: i64,
    pub category: Category,
    pub status: Status,
    pub fact: InternalFact,
    pub related: Option<RelatedItem>,
}
#[derive(Clone, Debug)]
pub(crate) enum InboxCommand {
    SetRead {
        id: FactId,
        read: bool,
    },
    SetPinned {
        id: FactId,
        pinned: bool,
    },
    Dismiss(FactId),
    ClearSource(Source),
    ClearWindowsApp(String),
    ClearAll,
    Settings(Settings),
    /// The exact currently displayed record, checked again on the worker.
    CreateTodo {
        expected: Record,
    },
    Snooze {
        expected: Record,
        minutes: u8,
    },
    WindowsAction {
        expected: Record,
        action: WindowsSourceAction,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SubmitError {
    Full,
    Closed,
    Invalid,
    Exhausted,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PersistenceStatus {
    Loading,
    Durable,
    MemoryOnly,
    WriteFailed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InboxCommandOutcome {
    Applied,
    NotFound,
    Invalid,
    Stale,
    TodoCreated,
    TodoFailed,
    Snoozed,
    SnoozeFailed,
    SourceOpened,
    SourceDismissRequested,
    SourceUnavailable,
    StoreFailed,
}
#[derive(Clone, Debug)]
pub(crate) struct InboxCommandReceipt {
    pub request: u64,
    pub outcome: InboxCommandOutcome,
}
#[derive(Clone)]
pub(crate) struct InboxServiceSnapshot {
    pub inbox: Arc<Inbox>,
    pub persistence: PersistenceStatus,
    pub dropped_events: u64,
    pub calendar_warning: bool,
    pub windows_capture: WindowsCaptureView,
    pub receipts: Arc<[InboxCommandReceipt]>,
}
impl Default for InboxServiceSnapshot {
    fn default() -> Self {
        Self {
            inbox: Arc::new(Inbox::default()),
            persistence: PersistenceStatus::Loading,
            dropped_events: 0,
            calendar_warning: false,
            windows_capture: WindowsCaptureView::default(),
            receipts: Arc::from([]),
        }
    }
}
enum Message {
    Event(InternalEvent),
    Command(u64, InboxCommand),
    Wake,
}
struct Shared {
    sender: SyncSender<Message>,
    snapshot: RwLock<Arc<InboxServiceSnapshot>>,
    stopped: AtomicBool,
    admission: RwLock<()>,
    calendar_enabled: AtomicBool,
    calendar_dirty: AtomicBool,
    dropped: AtomicU64,
    next_request: AtomicU64,
}

impl Shared {
    fn enqueue(&self, message: Message) -> Result<(), SubmitError> {
        let _admission = match self.admission.try_read() {
            Ok(guard) => guard,
            Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
            Err(TryLockError::WouldBlock) => return Err(SubmitError::Closed),
        };
        if self.stopped.load(Ordering::Acquire) {
            return Err(SubmitError::Closed);
        }
        self.sender.try_send(message).map_err(|error| match error {
            TrySendError::Full(_) => SubmitError::Full,
            TrySendError::Disconnected(_) => SubmitError::Closed,
        })
    }
}
#[derive(Clone)]
pub(crate) struct InboxPublisher {
    shared: Arc<Shared>,
}
impl InboxPublisher {
    /// Never blocks a producer and never accepts arbitrary internal text.
    pub(crate) fn try_publish(&self, event: InternalEvent) -> Result<(), SubmitError> {
        if !bounded_id(&event.id)
            || matches!(event.id, FactId::Windows { .. })
            || !bounded_related(event.related.as_ref())
            || !super::internal_event_valid(&event.id, event.fact, event.related.as_ref())
        {
            return Err(SubmitError::Invalid);
        }
        let result = self.shared.enqueue(Message::Event(event));
        if result == Err(SubmitError::Full) {
            self.shared.dropped.fetch_add(1, Ordering::Relaxed);
        }
        result
    }
}
pub(crate) struct InboxService {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}
impl InboxService {
    pub(crate) fn start(
        profile: AppDataRoot,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> std::io::Result<Self> {
        let (sender, receiver) = mpsc::sync_channel(QUEUE_LIMIT);
        let shared = Arc::new(Shared {
            sender,
            snapshot: RwLock::new(Arc::new(InboxServiceSnapshot::default())),
            stopped: AtomicBool::new(false),
            admission: RwLock::new(()),
            calendar_enabled: AtomicBool::new(false),
            calendar_dirty: AtomicBool::new(false),
            dropped: AtomicU64::new(0),
            next_request: AtomicU64::new(0),
        });
        let worker_shared = shared.clone();
        let worker = thread::Builder::new()
            .name("notification-inbox".into())
            .spawn(move || run(profile, wake, worker_shared, receiver))?;
        Ok(Self {
            shared,
            worker: Some(worker),
        })
    }
    pub(crate) fn publisher(&self) -> InboxPublisher {
        InboxPublisher {
            shared: self.shared.clone(),
        }
    }
    pub(crate) fn snapshot(&self) -> Arc<InboxServiceSnapshot> {
        self.shared
            .snapshot
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
    pub(crate) fn submit(&self, command: InboxCommand) -> Result<u64, SubmitError> {
        if self.shared.stopped.load(Ordering::Acquire) {
            return Err(SubmitError::Closed);
        }
        if !bounded_command(&command) {
            return Err(SubmitError::Invalid);
        }
        let request = self
            .shared
            .next_request
            .try_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(1))
            .map_err(|_| SubmitError::Exhausted)?
            + 1;
        self.shared.enqueue(Message::Command(request, command))?;
        Ok(request)
    }

    pub(crate) fn enable_calendar(&self) {
        self.shared.calendar_enabled.store(true, Ordering::Release);
        self.calendar_changed();
    }

    pub(crate) fn calendar_changed(&self) {
        if self.shared.calendar_enabled.load(Ordering::Acquire) {
            self.shared.calendar_dirty.store(true, Ordering::Release);
            let _ = self.shared.enqueue(Message::Wake);
        }
    }

    pub(crate) fn shutdown_checked(&mut self) -> Result<(), String> {
        {
            let _admission = self
                .shared
                .admission
                .write()
                .unwrap_or_else(|error| error.into_inner());
            self.shared.stopped.store(true, Ordering::Release);
            let _ = self.shared.sender.try_send(Message::Wake);
        }
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| "Notification Inbox worker panicked during shutdown".to_owned())?;
        }
        Ok(())
    }

    pub(crate) fn shutdown(&mut self) {
        if let Err(error) = self.shutdown_checked() {
            tracing::error!(%error, "Notification Inbox worker did not shut down cleanly");
        }
    }
}
impl Drop for InboxService {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn bounded_id(id: &FactId) -> bool {
    match id {
        FactId::Calendar { event, .. } => event.len() <= 256,
        FactId::HotFolder { rule, .. } => rule.len() <= 32,
        FactId::Windows { app, .. } => app.len() <= 512,
        _ => true,
    }
}
fn bounded_related(related: Option<&RelatedItem>) -> bool {
    match related {
        Some(RelatedItem::Calendar { event, .. }) => event.len() <= 256,
        Some(RelatedItem::HotFolder { rule }) => rule.len() <= 32,
        _ => true,
    }
}
fn bounded_command(command: &InboxCommand) -> bool {
    match command {
        InboxCommand::SetRead { id, .. }
        | InboxCommand::SetPinned { id, .. }
        | InboxCommand::Dismiss(id) => bounded_id(id),
        InboxCommand::ClearWindowsApp(app) => !app.is_empty() && app.len() <= 512,
        InboxCommand::Settings(settings) => settings.valid(),
        InboxCommand::Snooze { expected, minutes } => {
            bounded_id(&expected.id) && matches!(minutes, 5 | 10 | 15)
        }
        InboxCommand::CreateTodo { expected } | InboxCommand::WindowsAction { expected, .. } => {
            bounded_id(&expected.id)
                && bounded_related(expected.related.as_ref())
                && match &expected.content {
                    super::Content::External { title, body, .. } => {
                        [title.as_deref(), body.as_deref()]
                            .into_iter()
                            .flatten()
                            .all(|text| text.len() <= 2048)
                    }
                    _ => true,
                }
        }
        _ => true,
    }
}
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_secs()).ok())
        .unwrap_or(0)
}
fn next_expiry(inbox: &Inbox, current: i64) -> Option<Duration> {
    let lifetime = i64::from(inbox.settings().retention_days) * 86_400;
    inbox
        .records()
        .iter()
        .filter(|row| !row.pinned)
        .map(|row| row.occurred_at.saturating_add(lifetime).saturating_add(1))
        .min()
        .map(|deadline| Duration::from_secs(deadline.saturating_sub(current).max(0) as u64))
}
fn publish(
    shared: &Shared,
    wake: &Arc<dyn Fn() + Send + Sync>,
    store: &InboxStore,
    persistence: PersistenceStatus,
    receipts: &VecDeque<InboxCommandReceipt>,
    calendar_warning: bool,
    windows_capture: WindowsCaptureView,
) {
    let mut view = store.inbox().clone();
    view.prune(now());
    let snapshot = InboxServiceSnapshot {
        inbox: Arc::new(view),
        persistence,
        dropped_events: shared.dropped.load(Ordering::Relaxed),
        calendar_warning,
        windows_capture,
        receipts: receipts.iter().cloned().collect::<Vec<_>>().into(),
    };
    *shared.snapshot.write().unwrap_or_else(|e| e.into_inner()) = Arc::new(snapshot);
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| wake()));
}
fn run(
    profile: AppDataRoot,
    wake: Arc<dyn Fn() + Send + Sync>,
    shared: Arc<Shared>,
    receiver: Receiver<Message>,
) {
    let (mut store, mut persistence) = match InboxStore::open(&profile) {
        Ok(store) => (store, PersistenceStatus::Durable),
        Err(_) => (InboxStore::memory_only(), PersistenceStatus::MemoryOnly),
    };
    let mut receipts = VecDeque::new();
    let mut expiry_retry: Option<Instant> = None;
    let mut calendar = None;
    let mut calendar_deadline: Option<chrono::NaiveDateTime> = None;
    let mut calendar_warning = false;
    let mut windows = WindowsCapture::default();
    let callback_owner = Arc::downgrade(&shared);
    let source_wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        if let Some(owner) = callback_owner.upgrade() {
            let _ = owner.enqueue(Message::Wake);
        }
    });

    prune_due(&mut store, &mut persistence, &mut expiry_retry);
    publish(
        &shared,
        &wake,
        &store,
        persistence,
        &receipts,
        calendar_warning,
        windows.view(),
    );
    loop {
        let stopped = shared.stopped.load(Ordering::Acquire);
        let now_local = chrono::Local::now().naive_local();
        if !stopped
            && shared.calendar_enabled.load(Ordering::Acquire)
            && (shared.calendar_dirty.swap(false, Ordering::AcqRel)
                || calendar_deadline.is_some_and(|deadline| deadline <= now_local))
        {
            if calendar.is_none() {
                calendar = CalendarInboxReminders::open(&profile).ok();
            }
            let before = store.write_generation();
            let mut adoption_failed = false;
            let result = calendar.as_mut().map(|owner| {
                owner.tick(now_local, |event| match adopt_internal(&mut store, event) {
                    Ok(Ingest::Invalid) | Err(_) => {
                        adoption_failed = true;
                        false
                    }
                    Ok(_) => true,
                })
            });
            if store.write_generation() != before {
                persistence = PersistenceStatus::Durable;
            }
            match result {
                Some(Ok(next)) => {
                    calendar_deadline = if adoption_failed {
                        now_local.checked_add_signed(chrono::TimeDelta::minutes(1))
                    } else {
                        next
                    };
                    calendar_warning = adoption_failed;
                }
                Some(Err(_)) => {
                    calendar_deadline = now_local.checked_add_signed(chrono::TimeDelta::minutes(1));
                    calendar_warning = true;
                }
                None => {
                    calendar_deadline = None;
                    calendar_warning = true;
                }
            }
            if adoption_failed {
                persistence = PersistenceStatus::WriteFailed;
            }
            publish(
                &shared,
                &wake,
                &store,
                persistence,
                &receipts,
                calendar_warning,
                windows.view(),
            );
        }
        let previous_capture = windows.view();
        if stopped {
            if previous_capture != WindowsCaptureView::default() {
                windows.stop();
            }
        } else {
            let prior_write = store.write_generation();
            let update = windows.tick(&mut store, &source_wake);
            if update.is_err() {
                persistence = PersistenceStatus::WriteFailed;
            } else if store.write_generation() != prior_write {
                persistence = PersistenceStatus::Durable;
            }
            if update.unwrap_or(true) || windows.view() != previous_capture {
                publish(
                    &shared,
                    &wake,
                    &store,
                    persistence,
                    &receipts,
                    calendar_warning,
                    windows.view(),
                );
            }
        }
        let message = if stopped {
            match receiver.try_recv() {
                Ok(message) => Some(message),
                Err(_) => break,
            }
        } else {
            let expiry_wait = next_expiry(store.inbox(), now()).map(|due| {
                if due.is_zero() {
                    expiry_retry
                        .map_or(due, |retry| retry.saturating_duration_since(Instant::now()))
                } else {
                    due
                }
            });
            let calendar_wait = calendar_deadline.map(|deadline| {
                deadline
                    .signed_duration_since(chrono::Local::now().naive_local())
                    .to_std()
                    .unwrap_or_default()
            });
            let wait = [expiry_wait, calendar_wait, windows.wait()]
                .into_iter()
                .flatten()
                .min();
            match wait {
                Some(wait) => match receiver.recv_timeout(wait) {
                    Ok(message) => Some(message),
                    Err(mpsc::RecvTimeoutError::Timeout) => None,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                },
                None => match receiver.recv() {
                    Ok(message) => Some(message),
                    Err(_) => break,
                },
            }
        };

        // Apply retention before even a busy command stream can act on a row.
        let expired = prune_due(&mut store, &mut persistence, &mut expiry_retry);
        let worked = matches!(
            message.as_ref(),
            Some(Message::Event(_) | Message::Command(..))
        );
        let prior_write = store.write_generation();
        match message {
            None | Some(Message::Wake) => {}
            Some(Message::Event(event)) => {
                if adopt_internal(&mut store, event).is_err() {
                    persistence = PersistenceStatus::WriteFailed;
                    shared.dropped.fetch_add(1, Ordering::Relaxed);
                }
            }
            Some(Message::Command(request, command)) => {
                let outcome = match command {
                    InboxCommand::WindowsAction { expected, action } => {
                        apply_windows_action(&store, &mut windows, &expected, action)
                    }
                    _ => apply_command(&mut store, &profile, calendar.as_mut(), command),
                };
                if outcome == InboxCommandOutcome::StoreFailed {
                    persistence = PersistenceStatus::WriteFailed;
                }
                if outcome == InboxCommandOutcome::Snoozed {
                    shared.calendar_dirty.store(true, Ordering::Release);
                }
                if receipts.len() == RECEIPT_LIMIT {
                    receipts.pop_front();
                }
                receipts.push_back(InboxCommandReceipt { request, outcome });
            }
        }
        if store.write_generation() != prior_write {
            persistence = PersistenceStatus::Durable;
        }
        if expired || worked {
            publish(
                &shared,
                &wake,
                &store,
                persistence,
                &receipts,
                calendar_warning,
                windows.view(),
            );
        }
    }
}

fn adopt_internal(store: &mut InboxStore, event: InternalEvent) -> anyhow::Result<Ingest> {
    store.transact(|inbox| {
        inbox.ingest_internal(
            event.id,
            event.occurred_at,
            event.category,
            event.status,
            event.fact,
            event.related,
            now(),
        )
    })
}

fn prune_due(
    store: &mut InboxStore,
    status: &mut PersistenceStatus,
    retry_at: &mut Option<Instant>,
) -> bool {
    let current = now();
    if !next_expiry(store.inbox(), current).is_some_and(|due| due.is_zero())
        || retry_at.is_some_and(|due| due > Instant::now())
    {
        return false;
    }
    let prior_write = store.write_generation();
    if store.transact(|inbox| inbox.prune(current)).is_err() {
        *status = PersistenceStatus::WriteFailed;
        *retry_at = Instant::now().checked_add(Duration::from_secs(60));
    } else {
        *retry_at = None;
        if store.write_generation() != prior_write {
            *status = PersistenceStatus::Durable;
        }
    }
    true
}

fn apply_command(
    store: &mut InboxStore,
    profile: &AppDataRoot,
    calendar: Option<&mut CalendarInboxReminders>,
    command: InboxCommand,
) -> InboxCommandOutcome {
    use InboxCommandOutcome as O;
    let minutes = match &command {
        InboxCommand::Snooze { minutes, .. } => Some(*minutes),
        _ => None,
    };
    match command {
        InboxCommand::CreateTodo { expected } | InboxCommand::Snooze { expected, .. } => {
            if expired(store.inbox(), &expected)
                || store.inbox().records().iter().find(|r| r.id == expected.id) != Some(&expected)
            {
                return O::Stale;
            }
            let kind = if minutes.is_some() {
                ActionKind::Snooze
            } else {
                ActionKind::CreateTodo
            };
            let ticket = match store.transact(|inbox| inbox.begin_action(&expected.id, kind)) {
                Ok(Some(ticket)) => ticket,
                Ok(None) => return O::Invalid,
                Err(_) => return O::StoreFailed,
            };
            if !store.inbox().action_is_current(&ticket) {
                let _ = store.transact(|inbox| inbox.finish_action(&ticket, false));
                return O::Stale;
            }
            let outcome = if let Some(minutes) = minutes {
                let duration = Duration::from_secs(u64::from(minutes) * 60);
                let result = match &ticket.id {
                    FactId::Timer { .. } => {
                        crate::plugins::timer::snooze_inbox_timer(&ticket, duration).map(|_| ())
                    }
                    FactId::Calendar { .. } => calendar
                        .ok_or_else(|| anyhow::anyhow!("Calendar reminders are unavailable."))
                        .and_then(|owner| {
                            owner.snooze(&ticket, duration, chrono::Local::now().naive_local())
                        }),
                    _ => Err(anyhow::anyhow!("Only a current reminder can be deferred.")),
                };
                if result.is_ok() {
                    O::Snoozed
                } else {
                    O::SnoozeFailed
                }
            } else if create_todo_from_action(profile, store.inbox(), &ticket).is_ok() {
                O::TodoCreated
            } else {
                O::TodoFailed
            };
            let success = matches!(outcome, O::TodoCreated | O::Snoozed);
            if store
                .transact(|inbox| inbox.finish_action(&ticket, success))
                .is_err()
            {
                return O::StoreFailed;
            }
            outcome
        }
        other => match store.transact(|inbox| {
            inbox.prune(now());
            match other {
                InboxCommand::SetRead { id, read } => {
                    if inbox.set_read(&id, read) {
                        O::Applied
                    } else {
                        O::NotFound
                    }
                }
                InboxCommand::SetPinned { id, pinned } => {
                    if inbox.set_pinned(&id, pinned) {
                        O::Applied
                    } else {
                        O::NotFound
                    }
                }
                InboxCommand::Dismiss(id) => {
                    if inbox.dismiss(&id) {
                        O::Applied
                    } else {
                        O::NotFound
                    }
                }
                InboxCommand::ClearSource(source) => {
                    inbox.clear_source(source);
                    O::Applied
                }
                InboxCommand::ClearWindowsApp(app) => {
                    inbox.clear_windows_app(&app);
                    O::Applied
                }
                InboxCommand::ClearAll => {
                    inbox.clear_all();
                    O::Applied
                }
                InboxCommand::Settings(settings) => {
                    if inbox.set_settings(settings, now()).is_ok() {
                        O::Applied
                    } else {
                        O::Invalid
                    }
                }
                InboxCommand::CreateTodo { .. }
                | InboxCommand::Snooze { .. }
                | InboxCommand::WindowsAction { .. } => unreachable!(),
            }
        }) {
            Ok(result) => result,
            Err(_) => O::StoreFailed,
        },
    }
}

fn apply_windows_action(
    store: &InboxStore,
    windows: &mut WindowsCapture,
    expected: &Record,
    action: WindowsSourceAction,
) -> InboxCommandOutcome {
    use InboxCommandOutcome as O;
    let inbox = store.inbox();
    let settings = inbox.settings();
    let FactId::Windows { app, .. } = &expected.id else {
        return O::Invalid;
    };
    if !settings.external_enabled
        || settings.excluded.contains(&Source::Windows)
        || settings.excluded_apps.contains(app)
        || expired(inbox, expected)
        || inbox
            .records()
            .iter()
            .find(|record| record.id == expected.id)
            != Some(expected)
    {
        return O::Stale;
    }
    if !windows.action(expected, action, settings.retain_external_text) {
        return O::SourceUnavailable;
    }
    match action {
        WindowsSourceAction::Open => O::SourceOpened,
        WindowsSourceAction::Dismiss => O::SourceDismissRequested,
    }
}

fn expired(inbox: &Inbox, record: &Record) -> bool {
    !record.pinned
        && record.occurred_at
            < now().saturating_sub(i64::from(inbox.settings().retention_days) * 86_400)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn wait_for(
        service: &InboxService,
        predicate: impl Fn(&InboxServiceSnapshot) -> bool,
    ) -> Arc<InboxServiceSnapshot> {
        let limit = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            let snapshot = service.snapshot();
            if predicate(&snapshot) {
                return snapshot;
            }
            assert!(std::time::Instant::now() < limit);
            std::thread::yield_now();
        }
    }
    fn event() -> InternalEvent {
        InternalEvent {
            id: FactId::Launcher {
                session: [1; 16],
                sequence: 3,
            },
            occurred_at: now(),
            category: Category::Information,
            status: Status::Current,
            fact: InternalFact::LauncherInfo,
            related: None,
        }
    }
    #[test]
    fn notification_inbox_service_shutdown_drains_requests_it_accepted() {
        let directory = tempfile::tempdir().unwrap();
        let profile = AppDataRoot::from_path(directory.path());
        let handshake = Arc::new(std::sync::Barrier::new(2));
        let worker_handshake = handshake.clone();
        let first = AtomicBool::new(true);
        let mut service = InboxService::start(
            profile.clone(),
            Arc::new(move || {
                if first.swap(false, Ordering::AcqRel) {
                    worker_handshake.wait();
                    worker_handshake.wait();
                }
            }),
        )
        .unwrap();
        handshake.wait();
        let publisher = service.publisher();
        let accepted = event();
        let id = accepted.id.clone();
        publisher.try_publish(accepted).unwrap();
        service
            .submit(InboxCommand::SetRead { id, read: true })
            .unwrap();

        std::thread::scope(|scope| {
            scope.spawn(|| service.shutdown());
            let deadline = Instant::now() + Duration::from_secs(3);
            while !publisher.shared.stopped.load(Ordering::Acquire) {
                assert!(Instant::now() < deadline);
                std::thread::yield_now();
            }
            assert_eq!(publisher.try_publish(event()), Err(SubmitError::Closed));
            handshake.wait();
        });
        let stored = InboxStore::open(&profile).unwrap();
        assert_eq!(stored.inbox().records().len(), 1);
        assert!(stored.inbox().records()[0].read);
    }

    #[test]
    fn notification_inbox_service_does_not_let_a_command_pin_an_expired_row() {
        let directory = tempfile::tempdir().unwrap();
        let profile = AppDataRoot::from_path(directory.path());
        let event = event();
        let old = now() - 8 * 86_400;
        InboxStore::open(&profile)
            .unwrap()
            .transact(|inbox| {
                inbox.ingest_internal(
                    event.id.clone(),
                    old,
                    event.category,
                    event.status,
                    event.fact,
                    None,
                    old,
                )
            })
            .unwrap();
        let mut service = InboxService::start(profile.clone(), Arc::new(|| {})).unwrap();
        let request = service
            .submit(InboxCommand::SetPinned {
                id: event.id,
                pinned: true,
            })
            .unwrap();
        let snapshot = wait_for(&service, |value| {
            value
                .receipts
                .iter()
                .any(|receipt| receipt.request == request)
        });
        assert!(snapshot.inbox.records().is_empty());
        assert!(
            snapshot
                .receipts
                .iter()
                .any(|receipt| receipt.request == request
                    && receipt.outcome == InboxCommandOutcome::NotFound)
        );
        service.shutdown();
        assert!(
            InboxStore::open(&profile)
                .unwrap()
                .inbox()
                .records()
                .is_empty()
        );
    }

    #[test]
    fn notification_inbox_service_commits_actual_calendar_facts_before_delivery() {
        let directory = tempfile::tempdir().unwrap();
        let profile = AppDataRoot::from_path(directory.path());
        std::fs::create_dir(directory.path().join("calendar")).unwrap();
        let started = chrono::Local::now().naive_local() - chrono::TimeDelta::minutes(1);
        let path = directory
            .path()
            .join(crate::plugins::calendar::CALENDAR_EVENTS_FILE);
        let events = serde_json::json!([{"id":"evt-active-fixture","title":"PRIVATE_EVENT_SENTINEL",
            "start":started.format("%Y-%m-%dT%H:%M:%S").to_string(),"reminders":[{"minutes_before":0}]}]);
        std::fs::write(path, serde_json::to_vec(&events).unwrap()).unwrap();
        let mut service = InboxService::start(profile.clone(), Arc::new(|| {})).unwrap();
        service.enable_calendar();
        let snapshot = wait_for(&service, |snapshot| {
            snapshot
                .inbox
                .records()
                .iter()
                .any(|row| row.id.source() == Source::Calendar)
        });
        assert!(!snapshot.calendar_warning);
        service.shutdown();
        let history = InboxStore::open(&profile).unwrap();
        assert_eq!(history.inbox().records().len(), 1);
        let raw_history =
            std::fs::read_to_string(directory.path().join(super::super::HISTORY_FILE)).unwrap();
        assert!(!raw_history.contains("PRIVATE_EVENT_SENTINEL"));
    }

    #[test]
    fn notification_inbox_service_ingests_without_view_and_persists() {
        let temp = tempfile::tempdir().unwrap();
        let profile = AppDataRoot::from_path(temp.path());
        let mut service = InboxService::start(profile.clone(), Arc::new(|| {})).unwrap();
        let publisher = service.publisher();
        publisher.try_publish(event()).unwrap();
        publisher.try_publish(event()).unwrap();
        let snap = wait_for(&service, |s| s.inbox.records().len() == 1);
        assert_eq!(snap.persistence, PersistenceStatus::Durable);
        service.shutdown();
        assert_eq!(publisher.try_publish(event()), Err(SubmitError::Closed));
        assert_eq!(
            InboxStore::open(&profile).unwrap().inbox().records().len(),
            1
        );
    }
    #[test]
    fn notification_inbox_service_rejects_stale_todo_and_creates_current_once() {
        let temp = tempfile::tempdir().unwrap();
        let profile = AppDataRoot::from_path(temp.path());
        let mut service = InboxService::start(profile.clone(), Arc::new(|| {})).unwrap();
        service.publisher().try_publish(event()).unwrap();
        let snapshot = wait_for(&service, |snapshot| !snapshot.inbox.records().is_empty());
        let expected = snapshot.inbox.records()[0].clone();
        let mark = service
            .submit(InboxCommand::SetRead {
                id: expected.id.clone(),
                read: true,
            })
            .unwrap();
        wait_for(&service, |snapshot| {
            snapshot.receipts.iter().any(|r| r.request == mark)
        });
        let stale = service
            .submit(InboxCommand::CreateTodo { expected })
            .unwrap();
        let snapshot = wait_for(&service, |snapshot| {
            snapshot.receipts.iter().any(|r| r.request == stale)
        });
        assert_eq!(
            snapshot
                .receipts
                .iter()
                .find(|r| r.request == stale)
                .unwrap()
                .outcome,
            InboxCommandOutcome::Stale
        );
        let current = snapshot.inbox.records()[0].clone();
        let request = service
            .submit(InboxCommand::CreateTodo { expected: current })
            .unwrap();
        let snapshot = wait_for(&service, |snapshot| {
            snapshot.receipts.iter().any(|r| r.request == request)
        });
        assert_eq!(
            snapshot
                .receipts
                .iter()
                .find(|r| r.request == request)
                .unwrap()
                .outcome,
            InboxCommandOutcome::TodoCreated
        );
        let todo_path = temp.path().join(crate::plugins::todo::TODO_FILE);
        assert_eq!(
            crate::plugins::todo::load_todos(todo_path.to_str().unwrap())
                .unwrap()
                .len(),
            1
        );
        service.shutdown();
    }
    #[test]
    fn notification_inbox_service_rejects_mismatched_fact_before_queueing() {
        let temp = tempfile::tempdir().unwrap();
        let mut service =
            InboxService::start(AppDataRoot::from_path(temp.path()), Arc::new(|| {})).unwrap();
        let mut wrong = event();
        wrong.fact = InternalFact::TimerFinished;
        assert_eq!(
            service.publisher().try_publish(wrong),
            Err(SubmitError::Invalid)
        );
        service.shutdown();
    }
    #[test]
    fn notification_inbox_service_corrupt_history_stays_untouched() {
        let temp = tempfile::tempdir().unwrap();
        let profile = AppDataRoot::from_path(temp.path());
        let path = temp.path().join(super::super::HISTORY_FILE);
        std::fs::write(&path, b"bad bytes").unwrap();
        let mut service = InboxService::start(profile, Arc::new(|| {})).unwrap();
        service.publisher().try_publish(event()).unwrap();
        let snap = wait_for(&service, |s| s.inbox.records().len() == 1);
        assert_eq!(snap.persistence, PersistenceStatus::MemoryOnly);
        service.shutdown();
        assert_eq!(std::fs::read(path).unwrap(), b"bad bytes");
    }
}
