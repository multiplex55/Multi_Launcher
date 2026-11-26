//! Serialized accepted Regex history writes, owned independently of a mount.
use super::*;
use multi_launcher::regex_tester::{AcceptedHistory, HistoryEntry, HistoryStore};
use std::collections::VecDeque;
use std::path::PathBuf;

pub(super) const WAIT_RESTART: &str =
    "Wait for Regex Tester history to finish saving before restarting.";
pub(super) const WAIT_QUIT: &str =
    "Wait for Regex Tester history to finish saving before quitting.";
pub(super) const WAIT_CLOSE: &str =
    "Wait for Regex Tester history to finish saving before closing.";
fn is_history_wait(message: &str) -> bool {
    matches!(message, WAIT_RESTART | WAIT_QUIT | WAIT_CLOSE)
}

fn history_error(
    store: &HistoryStore,
    error: multi_launcher::regex_tester::RegexStoreError,
) -> String {
    store
        .diagnostic()
        .map(|diagnostic| diagnostic.message.clone())
        .unwrap_or_else(|| error.to_string())
}

struct OrderedLane<T> {
    active: bool,
    pending: VecDeque<T>,
}
impl<T> Default for OrderedLane<T> {
    fn default() -> Self {
        Self {
            active: false,
            pending: VecDeque::new(),
        }
    }
}
impl<T> OrderedLane<T> {
    fn push(&mut self, value: T) {
        self.pending.push_back(value);
    }
    fn start(&mut self) -> Option<T> {
        if self.active {
            return None;
        }
        let item = self.pending.pop_front()?;
        self.active = true;
        Some(item)
    }
    fn finish(&mut self) {
        self.active = false;
    }
    fn busy(&self) -> bool {
        self.active || !self.pending.is_empty()
    }
}
struct Write {
    entry: HistoryEntry,
    permit: EffectPermit,
}
pub(super) struct HistoryLane {
    policy: AcceptedHistory,
    path: Option<PathBuf>,
    store: Arc<Mutex<Option<HistoryStore>>>,
    queue: OrderedLane<Write>,
    pub(super) diagnostic: Option<String>,
}
impl HistoryLane {
    pub(super) fn new(path: Option<PathBuf>) -> Self {
        Self {
            policy: AcceptedHistory::default(),
            path,
            store: Arc::new(Mutex::new(None)),
            queue: OrderedLane::default(),
            diagnostic: None,
        }
    }
    pub(super) fn busy(&self) -> bool {
        self.queue.busy()
    }
}
impl super::regex_tester::RegexWorkspace {
    pub(super) fn record_accepted_history(&mut self, cx: &mut Context<Self>) {
        if self.history.path.is_none() {
            return;
        }
        let Some(permit) = self.admission.admit() else {
            return;
        };
        let Some(entry) = self.history.policy.admit(&self.session) else {
            return;
        };
        self.history.queue.push(Write { entry, permit });
        self.start_history_write(cx);
    }
    fn start_history_write(&mut self, cx: &mut Context<Self>) {
        let Some(write) = self.history.queue.start() else {
            return;
        };
        let Some(path) = self.history.path.clone() else {
            self.history.queue.finish();
            return;
        };
        let store = self.history.store.clone();
        let task = cx.background_executor().spawn(async move {
            let _permit = write.permit;
            let mut store = store
                .lock()
                .map_err(|_| "Regex history store is unavailable.".to_owned())?;
            let store = store.get_or_insert_with(|| HistoryStore::open(path));
            store
                .record(&write.entry.pattern, write.entry.flags)
                .map_err(|error| history_error(store, error))
        });
        let owner = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let result = task.await;
            let _ = owner.update(cx, |view, cx| {
                view.history.queue.finish();
                view.history.diagnostic = result.err();
                view.start_history_write(cx);
                if !view.history.busy() {
                    let _ = view.root.update(cx, |root, cx| {
                        if root.restart_status.as_deref().is_some_and(is_history_wait) {
                            root.restart_status = None;
                            cx.notify();
                        }
                    });
                }
                cx.notify();
            });
        })
        .detach();
    }
}
#[cfg(test)]
mod tests {
    use super::{OrderedLane, WAIT_CLOSE, WAIT_QUIT, WAIT_RESTART, history_error, is_history_wait};
    use multi_launcher::regex_tester::{AcceptedHistory, HistoryStore, RegexSession};
    use std::time::{Duration, Instant};
    #[test]
    fn blocked_history_preserves_original_diagnostic_and_bytes() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("history.json");
        let corrupt = b"{invalid";
        std::fs::write(&path, corrupt).unwrap();
        let mut store = HistoryStore::open(&path);
        let original = store.diagnostic().unwrap().message.clone();
        let error = store.record("new", Default::default()).unwrap_err();
        assert_eq!(history_error(&store, error), original);
        assert_eq!(std::fs::read(path).unwrap(), corrupt);
    }
    #[test]
    fn only_history_waits_are_eligible_for_clear() {
        assert!(is_history_wait(WAIT_RESTART));
        assert!(is_history_wait(WAIT_QUIT));
        assert!(is_history_wait(WAIT_CLOSE));
        assert!(!is_history_wait("unrelated settings error"));
    }
    #[test]
    fn queued_accepted_pairs_write_in_order_without_private_buffers() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("history.json");
        let mut store = HistoryStore::open(&path);
        let mut policy = AcceptedHistory::default();
        let mut lane = OrderedLane::default();
        let mut session = RegexSession::default();
        let now = Instant::now();
        session.draft.test_text = "private sample".into();
        session.draft.replacement = "private replacement".into();
        for pattern in ["first", "second", "first"] {
            session.draft.pattern = pattern.into();
            session.mark_changed(now);
            session.tick(now + Duration::from_millis(200));
            lane.push(policy.admit(&session).unwrap());
        }
        while let Some(entry) = lane.start() {
            store.record(&entry.pattern, entry.flags).unwrap();
            lane.finish();
        }
        assert_eq!(
            store
                .entries()
                .iter()
                .map(|entry| entry.pattern.as_str())
                .collect::<Vec<_>>(),
            ["first", "second"]
        );
        let bytes = std::fs::read_to_string(path).unwrap();
        assert!(!bytes.contains("private"));
        assert!(!lane.busy());
    }
    #[test]
    fn accepted_history_lane_serializes_all_distinct_pairs_in_order() {
        let mut lane = OrderedLane::default();
        lane.push("first");
        lane.push("second");
        assert_eq!(lane.start(), Some("first"));
        lane.push("third");
        assert!(lane.start().is_none());
        assert!(lane.busy());
        lane.finish();
        assert_eq!(lane.start(), Some("second"));
        lane.finish();
        assert_eq!(lane.start(), Some("third"));
        lane.finish();
        assert!(!lane.busy());
    }
}
