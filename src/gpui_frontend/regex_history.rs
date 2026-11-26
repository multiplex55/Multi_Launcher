//! Serialized accepted Regex history writes, owned independently of a mount.
use super::*;
use multi_launcher::regex_tester::{AcceptedHistory, EvaluationResult, HistoryEntry, HistoryStore, RegexSession};
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
enum Operation {
    Inspect,
    Reload,
    Record(HistoryEntry),
}
fn perform(store: &mut HistoryStore, operation: Operation) -> (Vec<HistoryEntry>, bool, Option<String>, bool) {
    let reload = matches!(&operation, Operation::Reload);
    let result = match operation {
        Operation::Inspect => Ok(()),
        Operation::Reload => store.reload(),
        Operation::Record(entry) => store.record(&entry.pattern, entry.flags),
    };
    let reloaded = reload && result.is_ok();
    let diagnostic = result.err().map(|error| history_error(store, error))
        .or_else(|| store.diagnostic().map(|diagnostic| diagnostic.message.clone()));
    (store.entries().to_vec(), store.is_writable(), diagnostic, reloaded)
}
struct Queued {
    operation: Operation,
    permit: EffectPermit,
}
#[derive(Clone, Default)]
struct Snapshot {
    initialized: bool,
    generation: u64,
    entries: Vec<HistoryEntry>,
    writable: bool,
}
impl Snapshot {
    fn publish(&mut self, entries: Vec<HistoryEntry>, writable: bool, reloaded: bool) {
        if !self.initialized || reloaded || self.entries != entries {
            self.generation = self.generation.wrapping_add(1);
        }
        self.initialized = true;
        self.entries = entries;
        self.writable = writable;
    }
}
fn row_current(snapshot: &Snapshot, generation: u64, index: usize, entry: &HistoryEntry) -> bool {
    snapshot.initialized && snapshot.generation == generation && snapshot.entries.get(index) == Some(entry)
}
fn retryable(session: &RegexSession) -> bool {
    !session.draft.pattern.is_empty()
        && session.evaluated_revision() == Some(session.revision())
        && matches!(session.result(), Some(EvaluationResult::Success { .. }))
}
pub(super) struct HistoryLane {
    policy: AcceptedHistory,
    path: Option<PathBuf>,
    store: Arc<Mutex<Option<HistoryStore>>>,
    queue: OrderedLane<Queued>,
    snapshot: Snapshot,
    inspect_queued: bool,
    expanded: bool,
    pub(super) diagnostic: Option<String>,
}
impl HistoryLane {
    pub(super) fn new(path: Option<PathBuf>) -> Self {
        Self {
            policy: AcceptedHistory::default(),
            path,
            store: Arc::new(Mutex::new(None)),
            queue: OrderedLane::default(),
            snapshot: Snapshot::default(),
            inspect_queued: false,
            expanded: false,
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
        self.history.queue.push(Queued { operation: Operation::Record(entry), permit });
        self.start_history_write(cx);
    }
    fn inspect_history(&mut self, reload: bool, cx: &mut Context<Self>) {
        if self.history.path.is_none() || (!reload && (self.history.snapshot.initialized || self.history.inspect_queued)) {
            return;
        }
        let Some(permit) = self.admission.admit() else { return; };
        self.history.inspect_queued = true;
        self.history.queue.push(Queued {
            operation: if reload { Operation::Reload } else { Operation::Inspect },
            permit,
        });
        self.start_history_write(cx);
        cx.notify();
    }
    fn retry_history(&mut self, cx: &mut Context<Self>) {
        if self.history.busy() || !self.history.snapshot.initialized || !self.history.snapshot.writable || !retryable(&self.session) {
            return;
        }
        let Some(permit) = self.admission.admit() else { return; };
        self.history.policy.retry();
        let Some(entry) = self.history.policy.admit(&self.session) else { return; };
        self.history.queue.push(Queued { operation: Operation::Record(entry), permit });
        self.start_history_write(cx);
        cx.notify();
    }
    fn start_history_write(&mut self, cx: &mut Context<Self>) {
        let Some(work) = self.history.queue.start() else {
            return;
        };
        let Some(path) = self.history.path.clone() else {
            self.history.queue.finish();
            return;
        };
        let inspecting = matches!(&work.operation, Operation::Inspect | Operation::Reload);
        let store = self.history.store.clone();
        let task = cx.background_executor().spawn(async move {
            let _permit = work.permit;
            let mut store = store
                .lock()
                .map_err(|_| "Regex history store is unavailable.".to_owned())?;
            let store = store.get_or_insert_with(|| HistoryStore::open(path));
            Ok::<_, String>(perform(store, work.operation))
        });
        let owner = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let result = task.await;
            let _ = owner.update(cx, |view, cx| {
                view.history.queue.finish();
                match result {
                    Ok((entries, writable, diagnostic, reloaded)) => {
                        view.history.snapshot.publish(entries, writable, reloaded);
                        view.history.diagnostic = diagnostic;
                    }
                    Err(error) => view.history.diagnostic = Some(error),
                }
                if inspecting { view.history.inspect_queued = false; }
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

pub(super) fn render(view: &mut super::regex_tester::RegexWorkspace, cx: &mut Context<super::regex_tester::RegexWorkspace>) -> AnyElement {
    let enabled = view.open && !view.rehosting && !view.admission.is_fenced();
    let expanded = view.history.expanded;
    let busy = view.history.busy();
    let receipt = view.mount.receipt.clone();
    let toggle = receipt.clone();
    let reload = receipt.clone();
    let retry = receipt.clone();
    let can_retry = enabled && !busy && view.history.snapshot.initialized
        && view.history.snapshot.writable && retryable(&view.session);
    let mut panel = div().id("regex-history").flex().flex_col().gap_2()
        .child(Button::new("regex-history-disclosure")
            .label(if expanded { "▾ History" } else { "▸ History" })
            .disabled(!enabled)
            .on_click(cx.listener(move |view, _, _, cx| {
                if toggle.is_current() && view.open && !view.rehosting && !view.admission.is_fenced() {
                    view.history.expanded = !view.history.expanded;
                    if view.history.expanded { view.inspect_history(false, cx); }
                    cx.notify();
                }
            })));
    if !expanded { return panel.into_any_element(); }
    panel = panel.child(div().flex().flex_wrap().gap_2()
        .child(Button::new("regex-history-reload").label("Reload history")
            .disabled(!enabled || busy || view.history.path.is_none())
            .on_click(cx.listener(move |view, _, _, cx| {
                if reload.is_current() && view.open && !view.rehosting && !view.admission.is_fenced() && !view.history.busy() {
                    view.inspect_history(true, cx);
                }
            })))
        .child(Button::new("regex-history-retry").label("Retry record")
            .disabled(!can_retry)
            .on_click(cx.listener(move |view, _, _, cx| {
                if retry.is_current() && view.open && !view.rehosting && !view.admission.is_fenced() {
                    view.retry_history(cx);
                }
            }))));
    if busy {
        panel = panel.child(Label::new("Loading or saving history…").id("regex-history-pending"));
    }
    if view.history.snapshot.initialized && !view.history.snapshot.writable {
        panel = panel.child(Label::new("History writes are blocked. Repair the file and reload to recover.").id("regex-history-blocked"));
    }
    if view.history.snapshot.initialized && view.history.snapshot.entries.is_empty() {
        panel = panel.child(Label::new("No recent expressions.").id("regex-history-empty"));
    }
    let generation = view.history.snapshot.generation;
    let mut rows = div().id("regex-history-rows").max_h(px(300.0)).overflow_y_scroll().flex().flex_col().gap_2();
    for (index, entry) in view.history.snapshot.entries.iter().enumerate() {
        let mut end = entry.pattern.len().min(160);
        while !entry.pattern.is_char_boundary(end) { end -= 1; }
        let label = format!("Recent #{} · flags {} · {}", index + 1, entry.flags.suffix(), &entry.pattern[..end]);
        let entry = entry.clone();
        let expected = entry.clone();
        let current = receipt.clone();
        rows = rows.child(Button::new(SharedString::from(format!("regex-history-load-{generation}-{index}")))
            .label(label).disabled(!enabled)
            .on_click(cx.listener(move |view, _, window, cx| {
                if !current.is_current() || !view.open || view.rehosting || view.admission.is_fenced()
                    || !row_current(&view.history.snapshot, generation, index, &expected) { return; }
                let pattern_changed = view.session.draft.pattern != entry.pattern;
                if pattern_changed { view.clipboard_epochs.pattern.fetch_add(1, Ordering::AcqRel); }
                view.clipboard_epochs.main_changed(true);
                view.session.draft.pattern = entry.pattern.clone();
                view.session.draft.flags = entry.flags;
                view.mount.pattern.focus_handle(cx).focus(window, cx);
                view.finish_prepared_change(cx);
                view.mount.pattern.update(cx, |input, cx| input.set_value(entry.pattern.clone(), window, cx));
            })));
    }
    panel.child(rows).into_any_element()
}
#[cfg(test)]
mod tests {
    use super::{Operation, OrderedLane, Snapshot, WAIT_CLOSE, WAIT_QUIT, WAIT_RESTART, history_error, is_history_wait, perform, retryable, row_current};
    use multi_launcher::regex_tester::{AcceptedHistory, HistoryEntry, HistoryStore, RegexSession};
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
    #[test]
    fn history_reload_preserves_snapshot_on_corruption_and_recovers() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("history.json");
        let mut store = HistoryStore::open(&path);
        let entry = HistoryEntry { pattern: "valid".into(), flags: Default::default() };
        let (before, writable, error, refreshed) = perform(&mut store, Operation::Record(entry.clone()));
        assert!(writable && error.is_none());
        assert!(!refreshed);
        let original = std::fs::read(&path).unwrap();
        std::fs::write(&path, b"{broken").unwrap();
        let (after, writable, error, refreshed) = perform(&mut store, Operation::Reload);
        assert_eq!(after, before);
        assert!(!writable && error.is_some());
        assert!(!refreshed);
        assert_eq!(std::fs::read(&path).unwrap(), b"{broken");
        std::fs::write(&path, original).unwrap();
        let (restored, writable, error, refreshed) = perform(&mut store, Operation::Reload);
        assert_eq!(restored, vec![entry]);
        assert!(writable && error.is_none());
        assert!(refreshed);
    }
    #[test]
    fn history_rows_require_exact_published_generation_and_value() {
        let entry = HistoryEntry { pattern: "first".into(), flags: Default::default() };
        let snapshot = Snapshot { initialized: true, generation: 3, entries: vec![entry.clone()], writable: true };
        assert!(row_current(&snapshot, 3, 0, &entry));
        assert!(!row_current(&snapshot, 2, 0, &entry));
        assert!(!row_current(&snapshot, 3, 1, &entry));
        let different = HistoryEntry { pattern: "other".into(), flags: Default::default() };
        assert!(!row_current(&snapshot, 3, 0, &different));
        let mut session = RegexSession::default();
        assert!(!retryable(&session));
        let now = Instant::now();
        session.draft.pattern = "first".into();
        session.mark_changed(now);
        assert!(!retryable(&session));
        session.tick(now + Duration::from_millis(200));
        assert!(retryable(&session));
    }
    #[test]
    fn unchanged_and_failed_publications_preserve_row_identity() {
        let entry = HistoryEntry { pattern: "first".into(), flags: Default::default() };
        let mut snapshot = Snapshot::default();
        snapshot.publish(vec![entry.clone()], true, false);
        let first = snapshot.generation;
        snapshot.publish(vec![entry.clone()], false, false);
        assert_eq!(snapshot.generation, first);
        assert!(row_current(&snapshot, first, 0, &entry));
        snapshot.publish(vec![entry.clone()], true, true);
        assert_ne!(snapshot.generation, first);
        assert!(!row_current(&snapshot, first, 0, &entry));
    }
}
