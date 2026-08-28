//! Thread-safe boundary for submitting macro text to the GUI query pipeline.
use super::{DiagnosticKind, ExecResult, ExecutionDiagnostic, RunControl};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::time::Duration;

pub struct PendingLauncherQuery {
    pub query: String,
    response: Option<mpsc::Sender<Result<(), String>>>,
}

impl PendingLauncherQuery {
    pub fn respond(mut self, response: Result<(), String>) -> bool {
        self.response
            .take()
            .is_some_and(|tx| tx.send(response).is_ok())
    }
}

#[derive(Default)]
pub struct LauncherQueryBroker {
    pending: Mutex<Option<PendingLauncherQuery>>,
    repaint: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}

impl LauncherQueryBroker {
    pub fn set_repaint(&self, callback: Arc<dyn Fn() + Send + Sync>) {
        *self.repaint.lock().unwrap() = Some(callback);
    }

    pub fn take_pending(&self) -> Option<PendingLauncherQuery> {
        self.pending.lock().unwrap().take()
    }

    pub fn submit(&self, query: &str, control: &RunControl) -> ExecResult {
        control.checkpoint()?;
        let (tx, rx) = mpsc::channel();
        {
            let mut pending = self.pending.lock().unwrap();
            if pending.is_some() {
                return Err(ExecutionDiagnostic::new(
                    DiagnosticKind::Backend,
                    "another launcher query is already pending",
                )
                .context("backend", "launcher")
                .context("query", query));
            }
            *pending = Some(PendingLauncherQuery {
                query: query.to_owned(),
                response: Some(tx),
            });
        }
        if let Some(repaint) = self.repaint.lock().unwrap().as_ref() {
            repaint();
        }
        loop {
            if control.is_stopped() {
                self.pending.lock().unwrap().take();
                return Err(ExecutionDiagnostic::new(
                    DiagnosticKind::Cancelled,
                    "launcher query cancelled because automation stopped",
                )
                .context("backend", "launcher")
                .context("query", query));
            }
            match rx.recv_timeout(Duration::from_millis(20)) {
                Ok(Ok(())) => return Ok(()),
                Ok(Err(message)) => {
                    return Err(ExecutionDiagnostic::new(DiagnosticKind::Backend, message)
                        .context("backend", "launcher")
                        .context("query", query));
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(ExecutionDiagnostic::new(
                        DiagnosticKind::Backend,
                        "launcher query response channel disconnected",
                    )
                    .context("backend", "launcher")
                    .context("query", query));
                }
            }
        }
    }
}

static BROKER: OnceLock<Arc<LauncherQueryBroker>> = OnceLock::new();

pub fn production_launcher_query_broker() -> Arc<LauncherQueryBroker> {
    BROKER
        .get_or_init(|| Arc::new(LauncherQueryBroker::default()))
        .clone()
}
