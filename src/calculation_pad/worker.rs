//! One coalescing worker for a Calculation Pad session.

use super::document::CalculationDocument;
use super::inspector::DocumentIncarnation;
use super::persistence::{DocumentId, LineId};
use super::session::{SessionEvaluation, project_evaluation};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EvaluationBasis {
    pub document: DocumentId,
    pub incarnation: DocumentIncarnation,
    pub revision: u64,
    pub generation: u64,
}

pub(super) type EvaluationStamp = EvaluationBasis;

pub(super) struct Request {
    pub stamp: EvaluationStamp,
    pub source: String,
    pub line_ids: Vec<LineId>,
    pub profile: super::persistence::DocumentProfile,
}

pub(super) struct Completion {
    pub stamp: EvaluationStamp,
    pub result: Result<SessionEvaluation, String>,
}

struct Queue<R, C> {
    pending: Option<R>,
    active: Option<Arc<AtomicBool>>,
    completion: Option<C>,
    stopped: bool,
}
impl<R, C> Default for Queue<R, C> {
    fn default() -> Self { Self { pending: None, active: None, completion: None, stopped: false } }
}

struct Shared<R, C> {
    queue: Mutex<Queue<R, C>>,
    ready: Condvar,
    wake: Arc<dyn Fn() + Send + Sync>,
    evaluate: Arc<dyn Fn(R, Arc<AtomicBool>) -> C + Send + Sync>,
}

pub(super) struct CoalescingWorker<R, C> {
    shared: Arc<Shared<R, C>>,
}
impl<R: Send + 'static, C: Send + 'static> CoalescingWorker<R, C> {
    pub fn start(
        name: &str,
        wake: Arc<dyn Fn() + Send + Sync>,
        evaluate: Arc<dyn Fn(R, Arc<AtomicBool>) -> C + Send + Sync>,
    ) -> Result<Self, String> {
        let shared = Arc::new(Shared {
            queue: Mutex::new(Queue::default()),
            ready: Condvar::new(),
            wake,
            evaluate,
        });
        let worker = Arc::clone(&shared);
        std::thread::Builder::new()
            .name(name.into())
            .spawn(move || run(worker))
            .map_err(|error| format!("Could not start calculation worker: {error}"))?;
        Ok(Self { shared })
    }

    pub fn submit(&self, request: R) {
        let mut queue = self
            .shared
            .queue
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(active) = &queue.active {
            active.store(true, Ordering::Relaxed);
        }
        queue.completion = None;
        queue.pending = Some(request);
        self.shared.ready.notify_one();
    }

    pub fn retire(&self) {
        let mut queue = self
            .shared
            .queue
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(active) = &queue.active {
            active.store(true, Ordering::Relaxed);
        }
        queue.pending = None;
        queue.completion = None;
    }

    pub fn take(&self) -> Option<C> {
        self.shared
            .queue
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .completion
            .take()
    }
}

impl<R, C> Drop for CoalescingWorker<R, C> {
    fn drop(&mut self) {
        let mut queue = self
            .shared
            .queue
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(active) = &queue.active {
            active.store(true, Ordering::Relaxed);
        }
        queue.pending = None;
        queue.stopped = true;
        self.shared.ready.notify_one();
    }
}

fn run<R: Send + 'static, C: Send + 'static>(shared: Arc<Shared<R, C>>) {
    loop {
        let (request, cancellation) = {
            let mut queue = shared
                .queue
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            while !queue.stopped && queue.pending.is_none() {
                queue = shared
                    .ready
                    .wait(queue)
                    .unwrap_or_else(|error| error.into_inner());
            }
            if queue.stopped {
                return;
            }
            let Some(request) = queue.pending.take() else {
                continue;
            };
            let cancellation = Arc::new(AtomicBool::new(false));
            queue.active = Some(Arc::clone(&cancellation));
            (request, cancellation)
        };
        let result = (shared.evaluate)(request, Arc::clone(&cancellation));
        let should_wake = {
            let mut queue = shared
                .queue
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            queue.active = None;
            if !queue.stopped && !cancellation.load(Ordering::Relaxed) && queue.pending.is_none() {
                queue.completion = Some(result);
                true
            } else {
                false
            }
        };
        if should_wake {
            (shared.wake)();
        }
    }
}

pub(super) struct EvaluationWorker { inner: CoalescingWorker<Request, Completion> }
impl EvaluationWorker {
    pub fn start(wake: Arc<dyn Fn() + Send + Sync>) -> Result<Self, String> { Self::start_with(wake, Arc::new(evaluate)) }
    pub(super) fn start_with(wake: Arc<dyn Fn() + Send + Sync>, evaluate: Arc<dyn Fn(Request, Arc<AtomicBool>) -> Result<SessionEvaluation, String> + Send + Sync>) -> Result<Self, String> {
        let task = Arc::new(move |request: Request, cancellation: Arc<AtomicBool>| {
            let stamp = request.stamp;
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| evaluate(request, cancellation)))
                .unwrap_or_else(|_| Err("Calculation worker failed while evaluating this revision.".to_owned()));
            Completion { stamp, result }
        });
        Ok(Self { inner: CoalescingWorker::start("calculation-pad-evaluator", wake, task)? })
    }
    pub fn submit(&self, request: Request) { self.inner.submit(request); }
    pub fn retire(&self) { self.inner.retire(); }
    pub fn take(&self) -> Option<Completion> { self.inner.take() }
}

fn evaluate(request: Request, cancellation: Arc<AtomicBool>) -> Result<SessionEvaluation, String> {
    let document = CalculationDocument::with_profile(request.source, request.profile);
    let evaluation = document.evaluate_with_cancellation(cancellation);
    project_evaluation(evaluation, &request.line_ids, request.stamp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn extended_request_uses_profile_for_large_source() {
        let source = format!("{}2\nr257", "# note\n".repeat(256));
        let document = DocumentId::fresh();
        let request = Request {
            stamp: EvaluationStamp { document, incarnation: DocumentIncarnation(DocumentId::fresh()), revision: 0, generation: 0 },
            line_ids: (1..=258).map(LineId).collect(),
            source,
            profile: super::super::persistence::DocumentProfile::Extended,
        };
        let result = super::evaluate(request, Arc::new(AtomicBool::new(false))).unwrap();
        assert!(result.diagnostic.is_none());
        assert!(matches!(result.lines[257].outcome, super::super::document::LineOutcome::Value(_)));
    }

    #[test]
    fn worker_cancels_old_work_and_coalesces_latest_pending_revision() {
        let (started_tx, started_rx) = mpsc::channel();
        let (continue_tx, continue_rx) = mpsc::channel();
        let continue_rx = Mutex::new(continue_rx);
        let (wake_tx, wake_rx) = mpsc::channel();
        let wake = Arc::new(move || {
            let _ = wake_tx.send(());
        });
        let evaluate = Arc::new(move |request: Request, cancellation: Arc<AtomicBool>| {
            started_tx.send(request.stamp.revision).unwrap();
            continue_rx
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(10))
                .unwrap();
            super::evaluate(request, cancellation)
        });
        let worker = EvaluationWorker::start_with(wake, evaluate).unwrap();
        let id = DocumentId::fresh();
        let incarnation = DocumentIncarnation(DocumentId::fresh());
        let request = |revision| Request {
            stamp: EvaluationStamp {
                document: id,
                incarnation,
                revision,
                generation: revision,
            },
            source: format!("{revision}"),
            line_ids: vec![LineId(1)],
            profile: Default::default(),
        };
        worker.submit(request(1));
        assert_eq!(started_rx.recv_timeout(Duration::from_secs(10)).unwrap(), 1);
        worker.submit(request(2));
        worker.submit(request(3));
        continue_tx.send(()).unwrap();
        assert_eq!(started_rx.recv_timeout(Duration::from_secs(10)).unwrap(), 3);
        continue_tx.send(()).unwrap();
        wake_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        let completion = worker.take().unwrap();
        assert_eq!(completion.stamp.revision, 3);
        assert!(completion.result.is_ok());
        assert!(worker.take().is_none());
    }

    #[test]
    fn worker_reports_panics_and_accepts_later_revisions() {
        let (wake_tx, wake_rx) = mpsc::channel();
        let wake = Arc::new(move || {
            let _ = wake_tx.send(());
        });
        let evaluator = Arc::new(|request: Request, cancellation: Arc<AtomicBool>| {
            if request.stamp.revision == 1 {
                panic!("test worker failure");
            }
            super::evaluate(request, cancellation)
        });
        let worker = EvaluationWorker::start_with(wake, evaluator).unwrap();
        let document = DocumentId::fresh();
        let incarnation = DocumentIncarnation(DocumentId::fresh());
        let request = |revision| Request {
            stamp: EvaluationStamp {
                document,
                incarnation,
                revision,
                generation: revision,
            },
            source: "1".into(),
            line_ids: vec![LineId(1)],
            profile: Default::default(),
        };
        worker.submit(request(1));
        wake_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        assert!(worker.take().unwrap().result.is_err());
        worker.submit(request(2));
        wake_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        assert!(worker.take().unwrap().result.is_ok());
    }

    #[test]
    fn worker_retires_old_generation_at_same_revision() {
        let (started_tx, started_rx) = mpsc::channel();
        let (continue_tx, continue_rx) = mpsc::channel();
        let continue_rx = Mutex::new(continue_rx);
        let (wake_tx, wake_rx) = mpsc::channel();
        let wake = Arc::new(move || { let _ = wake_tx.send(()); });
        let evaluate = Arc::new(move |request: Request, cancellation: Arc<AtomicBool>| {
            started_tx.send(request.stamp.generation).unwrap();
            continue_rx.lock().unwrap().recv_timeout(Duration::from_secs(10)).unwrap();
            super::evaluate(request, cancellation)
        });
        let worker = EvaluationWorker::start_with(wake, evaluate).unwrap();
        let document = DocumentId::fresh();
        let incarnation = DocumentIncarnation(DocumentId::fresh());
        let request = |generation| Request {
            stamp: EvaluationStamp { document, incarnation, revision: 7, generation },
            source: "1".into(),
            line_ids: vec![LineId(1)],
            profile: Default::default(),
        };
        worker.submit(request(1));
        assert_eq!(started_rx.recv_timeout(Duration::from_secs(10)).unwrap(), 1);
        worker.submit(request(2));
        continue_tx.send(()).unwrap();
        assert_eq!(started_rx.recv_timeout(Duration::from_secs(10)).unwrap(), 2);
        continue_tx.send(()).unwrap();
        wake_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        let result = worker.take().unwrap();
        assert_eq!(result.stamp.generation, 2);
        assert_eq!(result.result.unwrap().basis, result.stamp);
        assert!(worker.take().is_none());
    }
}
