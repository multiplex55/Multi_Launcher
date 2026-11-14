//! One coalescing worker for a Calculation Pad session.

use super::document::CalculationDocument;
use super::inspector::DocumentIncarnation;
use super::persistence::{DocumentId, LineId};
use super::session::{SessionEvaluation, project_evaluation};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct EvaluationStamp {
    pub document: DocumentId,
    pub incarnation: DocumentIncarnation,
    pub revision: u64,
}

pub(super) struct Request {
    pub stamp: EvaluationStamp,
    pub source: String,
    pub line_ids: Vec<LineId>,
}

pub(super) struct Completion {
    pub stamp: EvaluationStamp,
    pub result: Result<SessionEvaluation, String>,
}

#[derive(Default)]
struct Queue {
    pending: Option<Request>,
    active: Option<Arc<AtomicBool>>,
    completion: Option<Completion>,
    stopped: bool,
}

struct Shared {
    queue: Mutex<Queue>,
    ready: Condvar,
    wake: Arc<dyn Fn() + Send + Sync>,
    evaluate:
        Arc<dyn Fn(Request, Arc<AtomicBool>) -> Result<SessionEvaluation, String> + Send + Sync>,
}

pub(super) struct EvaluationWorker {
    shared: Arc<Shared>,
}

impl EvaluationWorker {
    pub fn start(wake: Arc<dyn Fn() + Send + Sync>) -> Result<Self, String> {
        Self::start_with(wake, Arc::new(evaluate))
    }

    fn start_with(
        wake: Arc<dyn Fn() + Send + Sync>,
        evaluate: Arc<
            dyn Fn(Request, Arc<AtomicBool>) -> Result<SessionEvaluation, String> + Send + Sync,
        >,
    ) -> Result<Self, String> {
        let shared = Arc::new(Shared {
            queue: Mutex::new(Queue::default()),
            ready: Condvar::new(),
            wake,
            evaluate,
        });
        let worker = Arc::clone(&shared);
        std::thread::Builder::new()
            .name("calculation-pad-evaluator".into())
            .spawn(move || run(worker))
            .map_err(|error| format!("Could not start calculation worker: {error}"))?;
        Ok(Self { shared })
    }

    pub fn submit(&self, request: Request) {
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

    pub fn take(&self) -> Option<Completion> {
        self.shared
            .queue
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .completion
            .take()
    }
}

impl Drop for EvaluationWorker {
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

fn run(shared: Arc<Shared>) {
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
        let stamp = request.stamp;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            (shared.evaluate)(request, Arc::clone(&cancellation))
        }))
        .unwrap_or_else(|_| {
            Err("Calculation worker failed while evaluating this revision.".to_owned())
        });
        let should_wake = {
            let mut queue = shared
                .queue
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            queue.active = None;
            if !queue.stopped && !cancellation.load(Ordering::Relaxed) && queue.pending.is_none() {
                queue.completion = Some(Completion { stamp, result });
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

fn evaluate(request: Request, cancellation: Arc<AtomicBool>) -> Result<SessionEvaluation, String> {
    let document = CalculationDocument::new(request.source);
    let evaluation = document.evaluate_with_cancellation(cancellation);
    project_evaluation(evaluation, &request.line_ids, request.stamp.revision)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

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
            },
            source: format!("{revision}"),
            line_ids: vec![LineId(1)],
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
            },
            source: "1".into(),
            line_ids: vec![LineId(1)],
        };
        worker.submit(request(1));
        wake_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        assert!(worker.take().unwrap().result.is_err());
        worker.submit(request(2));
        wake_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        assert!(worker.take().unwrap().result.is_ok());
    }
}
