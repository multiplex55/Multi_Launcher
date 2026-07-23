use crate::actions::Action;
use crate::clipboard_modify::clipboard::{
    ClipboardBackend, ClipboardError, ClipboardService, UndoRecord,
};
use crate::clipboard_modify::executor::{
    Cancellation, ExecuteError, execute_pipeline, execute_stages,
};
use crate::clipboard_modify::model::{ClipboardModifierCatalog, StageSpec};
use crate::clipboard_modify::parser::ClipboardModifyIntent;
use crate::gui::ActivationSource;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};

pub const DISPLAY_HEAD_BYTES: usize = 500 * 1024;
pub const DISPLAY_TAIL_BYTES: usize = 50 * 1024;
pub const TRUNCATION_MARKER: &str =
    "\n\n… [clipboard modify preview truncated; middle omitted] …\n\n";
const SYNC_PREVIEW_LIMIT: usize = 32 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OperationId(pub u64);

#[derive(Debug, Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);
impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}
impl Cancellation for CancellationToken {
    fn is_cancelled(&self) -> bool {
        self.is_cancelled()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputMetadata {
    pub chars: usize,
    pub lines: usize,
    pub bytes: usize,
}
impl OutputMetadata {
    pub fn from_text(s: &str) -> Self {
        Self {
            chars: s.chars().count(),
            lines: logical_line_count(s),
            bytes: s.len(),
        }
    }
}
fn logical_line_count(s: &str) -> usize {
    if s.is_empty() {
        0
    } else {
        s.lines().count() + usize::from(s.ends_with('\n'))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayProjection {
    pub text: String,
    pub truncated: bool,
    pub metadata: OutputMetadata,
}
pub fn display_projection(full: &str) -> DisplayProjection {
    let metadata = OutputMetadata::from_text(full);
    if full.len() <= DISPLAY_HEAD_BYTES + DISPLAY_TAIL_BYTES {
        return DisplayProjection {
            text: full.to_string(),
            truncated: false,
            metadata,
        };
    }
    let h = floor_boundary(full, DISPLAY_HEAD_BYTES);
    let t = ceil_boundary(full, full.len() - DISPLAY_TAIL_BYTES);
    DisplayProjection {
        text: format!("{}{}{}", &full[..h], TRUNCATION_MARKER, &full[t..]),
        truncated: true,
        metadata,
    }
}
fn floor_boundary(s: &str, mut i: usize) -> usize {
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}
fn ceil_boundary(s: &str, mut i: usize) -> usize {
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewState {
    IdleMissing,
    PendingDebounce {
        id: OperationId,
    },
    Running {
        id: OperationId,
    },
    Completed {
        id: OperationId,
        display: DisplayProjection,
    },
    Cancelled {
        id: OperationId,
    },
    Failed {
        id: OperationId,
        error: String,
    },
}

#[derive(Debug)]
enum PreviewEvent {
    Completed { id: OperationId, full: String },
    Cancelled { id: OperationId },
    Failed { id: OperationId, error: String },
}

pub struct PreviewCoordinator {
    next_id: u64,
    active: Option<(OperationId, CancellationToken)>,
    pending: Option<(
        OperationId,
        Instant,
        String,
        ClipboardModifyIntent,
        Arc<ClipboardModifierCatalog>,
    )>,
    tx: mpsc::Sender<PreviewEvent>,
    rx: mpsc::Receiver<PreviewEvent>,
    state: PreviewState,
    full_output: Option<String>,
    debounce: Duration,
}
impl Default for PreviewCoordinator {
    fn default() -> Self {
        Self::new(Duration::from_millis(120))
    }
}
impl PreviewCoordinator {
    pub fn new(debounce: Duration) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            next_id: 1,
            active: None,
            pending: None,
            tx,
            rx,
            state: PreviewState::IdleMissing,
            full_output: None,
            debounce,
        }
    }
    pub fn request(
        &mut self,
        source: String,
        intent: ClipboardModifyIntent,
        catalog: Arc<ClipboardModifierCatalog>,
    ) -> OperationId {
        self.cancel_active();
        self.full_output = None;
        let id = OperationId(self.next_id);
        self.next_id += 1;
        self.pending = Some((id, Instant::now() + self.debounce, source, intent, catalog));
        self.state = PreviewState::PendingDebounce { id };
        id
    }
    pub fn cancel_active(&mut self) {
        if let Some((_, t)) = &self.active {
            t.cancel();
        }
    }
    pub fn cancel_and_invalidate(&mut self) {
        self.cancel_active();
        self.active = None;
        self.pending = None;
        self.full_output = None;
        self.state = PreviewState::IdleMissing;
    }
    pub fn is_active(&self) -> bool {
        self.active.is_some() || self.pending.is_some()
    }
    pub fn tick(&mut self) -> bool {
        let before = self.is_active();
        self.drain();
        if self.pending.as_ref().is_some_and(|p| Instant::now() >= p.1) {
            let (id, _, source, intent, catalog) = self.pending.take().unwrap();
            self.start(id, source, intent, catalog);
        }
        self.drain();
        before && !self.is_active()
    }
    pub fn force_start_pending(&mut self) {
        if let Some((id, _, s, i, c)) = self.pending.take() {
            self.start(id, s, i, c);
        }
        self.drain();
    }
    fn start(
        &mut self,
        id: OperationId,
        source: String,
        intent: ClipboardModifyIntent,
        catalog: Arc<ClipboardModifierCatalog>,
    ) {
        let token = CancellationToken::new();
        self.active = Some((id, token.clone()));
        self.state = PreviewState::Running { id };
        let tx = self.tx.clone();
        let run_sync = source.len() <= SYNC_PREVIEW_LIMIT;
        let work = move || run_intent(&source, &intent, catalog.as_ref(), &token);
        if run_sync {
            match work() {
                Ok(full) => {
                    let _ = tx.send(PreviewEvent::Completed { id, full });
                }
                Err(ExecuteError::Cancelled) => {
                    let _ = tx.send(PreviewEvent::Cancelled { id });
                }
                Err(e) => {
                    let _ = tx.send(PreviewEvent::Failed {
                        id,
                        error: e.to_string(),
                    });
                }
            }
        } else {
            thread::spawn(move || match work() {
                Ok(full) => {
                    let _ = tx.send(PreviewEvent::Completed { id, full });
                }
                Err(ExecuteError::Cancelled) => {
                    let _ = tx.send(PreviewEvent::Cancelled { id });
                }
                Err(e) => {
                    let _ = tx.send(PreviewEvent::Failed {
                        id,
                        error: e.to_string(),
                    });
                }
            });
        }
    }
    fn drain(&mut self) {
        while let Ok(ev) = self.rx.try_recv() {
            let id = match &ev {
                PreviewEvent::Completed { id, .. }
                | PreviewEvent::Cancelled { id }
                | PreviewEvent::Failed { id, .. } => *id,
            };
            if Some(id) != self.active.as_ref().map(|a| a.0) {
                continue;
            }
            match ev {
                PreviewEvent::Completed { id, full } => {
                    let display = display_projection(&full);
                    self.full_output = Some(full);
                    self.state = PreviewState::Completed { id, display };
                    self.active = None;
                }
                PreviewEvent::Cancelled { id } => {
                    self.full_output = None;
                    self.state = PreviewState::Cancelled { id };
                    self.active = None;
                }
                PreviewEvent::Failed { id, error } => {
                    self.full_output = None;
                    self.state = PreviewState::Failed { id, error };
                    self.active = None;
                }
            }
        }
    }
    pub fn state(&self) -> &PreviewState {
        &self.state
    }
    pub fn full_output(&self) -> Option<&str> {
        self.full_output.as_deref()
    }
    pub fn apply_text(&self) -> Option<&str> {
        self.full_output()
    }
    pub fn copy_result_text(&self) -> Option<&str> {
        self.full_output()
    }
}

fn run_intent<C: Cancellation + ?Sized>(
    source: &str,
    intent: &ClipboardModifyIntent,
    catalog: &ClipboardModifierCatalog,
    c: &C,
) -> Result<String, ExecuteError> {
    match intent {
        ClipboardModifyIntent::Stages(stages) => execute_stages(source, stages, catalog, c),
        ClipboardModifyIntent::ApplyTemplate { name } => execute_stages(
            source,
            &[crate::clipboard_modify::model::StageSpec {
                operation: crate::clipboard_modify::model::OperationId::Template,
                arguments: crate::clipboard_modify::model::StageArguments {
                    name: Some(name.clone()),
                    ..Default::default()
                },
            }],
            catalog,
            c,
        ),
        ClipboardModifyIntent::ApplySavedPipeline { name } => {
            execute_pipeline(source, name, catalog, c)
        }
        ClipboardModifyIntent::Undo => Ok(source.to_string()),
    }
}

#[derive(Debug, Clone)]
pub struct ImmediateRequestMetadata {
    pub action: Action,
    pub query: String,
    pub source: ActivationSource,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuredClipboardModifyError {
    pub message: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImmediateCompletionEvent {
    pub request_id: OperationId,
    pub display_label: String,
    pub character_count: usize,
    pub line_count: usize,
    pub undo_available: bool,
    pub result: Result<(), StructuredClipboardModifyError>,
}

pub trait ClipboardCommit: Send + Sync + 'static {
    fn read_text(&self) -> Result<String, ClipboardError>;
    fn commit_output(&self, output: String, label: &str) -> Result<UndoRecord, ClipboardError>;
}
impl<B: ClipboardBackend + 'static> ClipboardCommit for ClipboardService<B> {
    fn read_text(&self) -> Result<String, ClipboardError> {
        self.read_text_for_modify()
    }
    fn commit_output(&self, output: String, label: &str) -> Result<UndoRecord, ClipboardError> {
        self.commit_output(output, label)
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ImmediateDiagnostics {
    pub started: u64,
    pub completed: u64,
    pub failed: u64,
    pub hooks_after_success: u64,
}

pub type SuccessHook =
    Arc<dyn Fn(&ImmediateRequestMetadata, &ImmediateCompletionEvent) + Send + Sync>;

pub struct ImmediateExecutionCoordinator<S: ClipboardCommit> {
    next_id: u64,
    service: Arc<S>,
    tx: mpsc::Sender<ImmediateCompletionEvent>,
    rx: mpsc::Receiver<ImmediateCompletionEvent>,
    pending: std::collections::BTreeMap<u64, ImmediateRequestMetadata>,
    success_hook: Option<SuccessHook>,
    repaint: Option<Arc<dyn Fn() + Send + Sync>>,
    diagnostics: ImmediateDiagnostics,
}
impl<S: ClipboardCommit> ImmediateExecutionCoordinator<S> {
    pub fn new(service: Arc<S>) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            next_id: 1,
            service,
            tx,
            rx,
            pending: Default::default(),
            success_hook: None,
            repaint: None,
            diagnostics: Default::default(),
        }
    }
    pub fn set_success_hook(&mut self, hook: SuccessHook) {
        self.success_hook = Some(hook);
    }
    pub fn set_repaint_callback(&mut self, repaint: Arc<dyn Fn() + Send + Sync>) {
        self.repaint = Some(repaint);
    }
    pub fn start(
        &mut self,
        intent: ClipboardModifyIntent,
        catalog: Arc<ClipboardModifierCatalog>,
        meta: ImmediateRequestMetadata,
    ) -> Result<OperationId, StructuredClipboardModifyError> {
        if self.has_pending() {
            return Err(StructuredClipboardModifyError {
                message: "Clipboard Modify operation already running".to_string(),
            });
        }
        let id = OperationId(self.next_id);
        self.next_id += 1;
        self.pending.insert(id.0, meta.clone());
        self.diagnostics.started += 1;
        let tx = self.tx.clone();
        let service = Arc::clone(&self.service);
        let repaint = self.repaint.clone();
        thread::spawn(move || {
            let label = meta.action.label.clone();
            let result = catch_unwind(AssertUnwindSafe(|| {
                let source = service.read_text()?;
                let cancel = CancellationToken::new();
                let out = run_intent(&source, &intent, catalog.as_ref(), &cancel)
                    .map_err(ClipboardError::from)?;
                let md = OutputMetadata::from_text(&out);
                service.commit_output(out, &label)?;
                Ok::<_, ClipboardError>(md)
            }));
            let ev = match result {
                Err(_) => ImmediateCompletionEvent {
                    request_id: id,
                    display_label: label,
                    character_count: 0,
                    line_count: 0,
                    undo_available: false,
                    result: Err(StructuredClipboardModifyError {
                        message: "Clipboard Modify operation failed unexpectedly".to_string(),
                    }),
                },
                Ok(result) => match result {
                    Ok(md) => ImmediateCompletionEvent {
                        request_id: id,
                        display_label: label,
                        character_count: md.chars,
                        line_count: md.lines,
                        undo_available: true,
                        result: Ok(()),
                    },
                    Err(e) => ImmediateCompletionEvent {
                        request_id: id,
                        display_label: label,
                        character_count: 0,
                        line_count: 0,
                        undo_available: false,
                        result: Err(StructuredClipboardModifyError {
                            message: e.to_string(),
                        }),
                    },
                },
            };
            let _ = tx.send(ev);
            if let Some(r) = repaint {
                r();
            }
        });
        Ok(id)
    }
    pub fn drain_completions(&mut self) -> Vec<ImmediateCompletionEvent> {
        let mut out = Vec::new();
        while let Ok(ev) = self.rx.try_recv() {
            if let Some(meta) = self.pending.remove(&ev.request_id.0) {
                if ev.result.is_ok() {
                    self.diagnostics.completed += 1;
                    if let Some(h) = &self.success_hook {
                        h(&meta, &ev);
                        self.diagnostics.hooks_after_success += 1;
                    }
                } else {
                    self.diagnostics.failed += 1;
                }
                out.push(ev);
            }
        }
        out
    }
    pub fn pending_metadata(&self, id: OperationId) -> Option<&ImmediateRequestMetadata> {
        self.pending.get(&id.0)
    }
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }
    pub fn cancel_pending(&mut self) {
        self.pending.clear();
    }
    pub fn diagnostics(&self) -> &ImmediateDiagnostics {
        &self.diagnostics
    }
    #[cfg(test)]
    pub(crate) fn inject_completion_for_test(
        &mut self,
        meta: ImmediateRequestMetadata,
        ev: ImmediateCompletionEvent,
    ) {
        self.pending.insert(ev.request_id.0, meta);
        self.tx.send(ev).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clipboard_modify::clipboard::{
        ClipboardService,
        fake::{FakeClipboardBackend, Op},
    };
    use crate::clipboard_modify::default_catalog;
    use crate::clipboard_modify::model::{OperationId as OpId, StageArguments};
    use std::sync::atomic::AtomicUsize;

    fn stage(op: OpId) -> StageSpec {
        StageSpec {
            operation: op,
            arguments: StageArguments::default(),
        }
    }
    fn action() -> Action {
        Action {
            label: "Run".into(),
            desc: "d".into(),
            action: "clipboard_modify:execute".into(),
            args: None,
        }
    }

    #[test]
    fn truncation_preserves_valid_utf8() {
        let s = format!(
            "{}{}{}",
            "a".repeat(DISPLAY_HEAD_BYTES),
            "💖".repeat(10),
            "z".repeat(DISPLAY_TAIL_BYTES + 1)
        );
        let p = display_projection(&s);
        assert!(p.truncated);
        assert!(std::str::from_utf8(p.text.as_bytes()).is_ok());
        assert_eq!(p.metadata.bytes, s.len());
    }
    #[test]
    fn full_output_not_duplicated_and_apply_copy_use_full() {
        let cat = Arc::new(default_catalog());
        let mut pc = PreviewCoordinator::new(Duration::ZERO);
        let id = pc.request(
            "x".into(),
            ClipboardModifyIntent::Stages(vec![stage(OpId::DoubleQuote)]),
            cat,
        );
        pc.force_start_pending();
        pc.tick();
        assert!(matches!(pc.state(), PreviewState::Completed{id: got,..} if *got==id));
        assert_eq!(pc.full_output(), Some("\"x\""));
        assert_eq!(pc.apply_text(), pc.full_output());
        assert_eq!(pc.copy_result_text(), pc.full_output());
    }
    #[test]
    fn stale_preview_results_are_ignored() {
        let cat = Arc::new(default_catalog());
        let mut pc = PreviewCoordinator::new(Duration::from_secs(60));
        let old = pc.request(
            "old".into(),
            ClipboardModifyIntent::Stages(vec![stage(OpId::Uppercase)]),
            cat.clone(),
        );
        let new = pc.request(
            "new".into(),
            ClipboardModifyIntent::Stages(vec![stage(OpId::DoubleQuote)]),
            cat,
        );
        assert_ne!(old, new);
        pc.force_start_pending();
        pc.tick();
        assert_eq!(pc.full_output(), Some("\"new\""));
    }
    #[test]
    fn cancellation_prevents_old_results_visible() {
        let cat = Arc::new(default_catalog());
        let mut pc = PreviewCoordinator::new(Duration::ZERO);
        let old = pc.request(
            "old".into(),
            ClipboardModifyIntent::Stages(vec![stage(OpId::Uppercase)]),
            cat.clone(),
        );
        pc.force_start_pending();
        pc.cancel_active();
        let _new = pc.request(
            "new".into(),
            ClipboardModifyIntent::Stages(vec![stage(OpId::DoubleQuote)]),
            cat,
        );
        pc.force_start_pending();
        pc.tick();
        assert_ne!(pc.full_output(), Some("OLD"));
        assert!(matches!(pc.state(), PreviewState::Completed{id,..} if *id != old));
    }
    #[test]
    fn immediate_completion_event_contains_metadata() {
        let svc = Arc::new(ClipboardService::new(FakeClipboardBackend::with_text(
            "a\nb",
        )));
        let mut ic = ImmediateExecutionCoordinator::new(svc);
        let id = ic
            .start(
                ClipboardModifyIntent::Stages(vec![stage(OpId::Uppercase)]),
                Arc::new(default_catalog()),
                ImmediateRequestMetadata {
                    action: action(),
                    query: "cm upper".into(),
                    source: ActivationSource::Enter,
                },
            )
            .unwrap();
        std::thread::sleep(Duration::from_millis(50));
        let ev = ic.drain_completions().pop().unwrap();
        assert_eq!(ev.request_id, id);
        assert_eq!(ev.character_count, 3);
        assert_eq!(ev.line_count, 2);
        assert!(ev.undo_available);
        assert!(ev.result.is_ok());
    }
    #[test]
    fn immediate_cancel_pending_rejects_stale_completion() {
        let svc = Arc::new(ClipboardService::new(FakeClipboardBackend::with_text("x")));
        let mut ic = ImmediateExecutionCoordinator::new(svc);
        let id = ic
            .start(
                ClipboardModifyIntent::Stages(vec![stage(OpId::Uppercase)]),
                Arc::new(default_catalog()),
                ImmediateRequestMetadata {
                    action: action(),
                    query: "q".into(),
                    source: ActivationSource::Enter,
                },
            )
            .unwrap();
        assert!(ic.pending_metadata(id).is_some());
        ic.cancel_pending();
        std::thread::sleep(Duration::from_millis(50));
        assert!(ic.drain_completions().is_empty());
    }

    #[derive(Clone, Copy)]
    enum PanicAt {
        Read,
        Commit,
    }
    struct PanicService {
        at: PanicAt,
    }
    impl ClipboardCommit for PanicService {
        fn read_text(&self) -> Result<String, ClipboardError> {
            if matches!(self.at, PanicAt::Read) {
                panic!("secret source");
            }
            Ok("x".into())
        }
        fn commit_output(
            &self,
            _output: String,
            _label: &str,
        ) -> Result<UndoRecord, ClipboardError> {
            if matches!(self.at, PanicAt::Commit) {
                panic!("secret output");
            }
            Ok(UndoRecord {
                original_text: "x".into(),
                modified_text: "X".into(),
                operation_label: "Run".into(),
            })
        }
    }

    fn meta(query: &str) -> ImmediateRequestMetadata {
        ImmediateRequestMetadata {
            action: action(),
            query: query.into(),
            source: ActivationSource::Enter,
        }
    }

    fn wait_one<S: ClipboardCommit>(
        ic: &mut ImmediateExecutionCoordinator<S>,
    ) -> ImmediateCompletionEvent {
        for _ in 0..50 {
            if let Some(ev) = ic.drain_completions().pop() {
                return ev;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("completion not received")
    }

    #[test]
    fn immediate_rejects_start_while_pending_without_mutation() {
        let svc = Arc::new(ClipboardService::new(FakeClipboardBackend::with_text("x")));
        let mut ic = ImmediateExecutionCoordinator::new(svc);
        let id = ic
            .start(
                ClipboardModifyIntent::Stages(vec![stage(OpId::Uppercase)]),
                Arc::new(default_catalog()),
                meta("first"),
            )
            .unwrap();
        let diagnostics = ic.diagnostics().clone();
        let err = ic
            .start(
                ClipboardModifyIntent::Stages(vec![stage(OpId::DoubleQuote)]),
                Arc::new(default_catalog()),
                meta("second"),
            )
            .unwrap_err();
        assert_eq!(err.message, "Clipboard Modify operation already running");
        assert_eq!(ic.pending_metadata(id).unwrap().query, "first");
        assert_eq!(ic.diagnostics(), &diagnostics);
        let _ = wait_one(&mut ic);
    }

    #[test]
    fn immediate_panic_during_read_becomes_failure_and_clears_pending() {
        let mut ic =
            ImmediateExecutionCoordinator::new(Arc::new(PanicService { at: PanicAt::Read }));
        let id = ic
            .start(
                ClipboardModifyIntent::Stages(vec![stage(OpId::Uppercase)]),
                Arc::new(default_catalog()),
                meta("q"),
            )
            .unwrap();
        let ev = wait_one(&mut ic);
        assert_eq!(ev.request_id, id);
        assert_eq!(
            ev.result.unwrap_err().message,
            "Clipboard Modify operation failed unexpectedly"
        );
        assert!(!ic.has_pending());
    }

    #[test]
    fn immediate_panic_during_transformation_becomes_failure_and_clears_pending() {
        let svc = Arc::new(ClipboardService::new(FakeClipboardBackend::with_text("x")));
        let mut ic = ImmediateExecutionCoordinator::new(svc);
        let id = ic
            .start(
                ClipboardModifyIntent::ApplyTemplate {
                    name: "missing".into(),
                },
                Arc::new(ClipboardModifierCatalog::new(Vec::new(), Vec::new()).unwrap()),
                meta("q"),
            )
            .unwrap();
        let ev = wait_one(&mut ic);
        assert_eq!(ev.request_id, id);
        assert!(ev.result.is_err());
        assert!(!ic.has_pending());
    }

    #[test]
    fn immediate_panic_during_commit_becomes_failure_and_clears_pending() {
        let mut ic = ImmediateExecutionCoordinator::new(Arc::new(PanicService {
            at: PanicAt::Commit,
        }));
        let id = ic
            .start(
                ClipboardModifyIntent::Stages(vec![stage(OpId::Uppercase)]),
                Arc::new(default_catalog()),
                meta("q"),
            )
            .unwrap();
        let ev = wait_one(&mut ic);
        assert_eq!(ev.request_id, id);
        assert_eq!(
            ev.result.unwrap_err().message,
            "Clipboard Modify operation failed unexpectedly"
        );
        assert!(!ic.has_pending());
    }

    #[test]
    fn immediate_repaint_after_success_expected_failure_and_panic() {
        let repaint_count = Arc::new(AtomicUsize::new(0));
        let mut success = ImmediateExecutionCoordinator::new(Arc::new(ClipboardService::new(
            FakeClipboardBackend::with_text("x"),
        )));
        let c = repaint_count.clone();
        success.set_repaint_callback(Arc::new(move || {
            c.fetch_add(1, Ordering::SeqCst);
        }));
        success
            .start(
                ClipboardModifyIntent::Stages(vec![stage(OpId::Uppercase)]),
                Arc::new(default_catalog()),
                meta("q"),
            )
            .unwrap();
        let _ = wait_one(&mut success);

        let mut fail = ImmediateExecutionCoordinator::new(Arc::new(ClipboardService::new(
            FakeClipboardBackend::with_text("x"),
        )));
        fail.service
            .backend()
            .push(Op::Write(Err(ClipboardError::Permanent("no".into()))));
        let c = repaint_count.clone();
        fail.set_repaint_callback(Arc::new(move || {
            c.fetch_add(1, Ordering::SeqCst);
        }));
        fail.start(
            ClipboardModifyIntent::Stages(vec![stage(OpId::Uppercase)]),
            Arc::new(default_catalog()),
            meta("q"),
        )
        .unwrap();
        let _ = wait_one(&mut fail);

        let mut panicc =
            ImmediateExecutionCoordinator::new(Arc::new(PanicService { at: PanicAt::Read }));
        let c = repaint_count.clone();
        panicc.set_repaint_callback(Arc::new(move || {
            c.fetch_add(1, Ordering::SeqCst);
        }));
        panicc
            .start(
                ClipboardModifyIntent::Stages(vec![stage(OpId::Uppercase)]),
                Arc::new(default_catalog()),
                meta("q"),
            )
            .unwrap();
        let _ = wait_one(&mut panicc);
        assert_eq!(repaint_count.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn hooks_only_after_successful_commits() {
        let svc = Arc::new(ClipboardService::new(FakeClipboardBackend::with_text("x")));
        svc.backend()
            .push(Op::Write(Err(ClipboardError::Permanent("no".into()))));
        let mut ic = ImmediateExecutionCoordinator::new(svc);
        let calls = Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        ic.set_success_hook(Arc::new(move |_, _| {
            c.fetch_add(1, Ordering::SeqCst);
        }));
        ic.start(
            ClipboardModifyIntent::Stages(vec![stage(OpId::Uppercase)]),
            Arc::new(default_catalog()),
            ImmediateRequestMetadata {
                action: action(),
                query: "q".into(),
                source: ActivationSource::Click,
            },
        )
        .unwrap();
        std::thread::sleep(Duration::from_millis(50));
        let ev = ic.drain_completions().pop().unwrap();
        assert!(ev.result.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}
