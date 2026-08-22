//! Thread-safe boundary between macro workers and the GUI prompt viewport.
use super::executor::{DiagnosticKind, ExecResult, ExecutionDiagnostic, RunControl};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PromptRequest {
    pub id: u64,
    pub title: String,
    pub prompt: String,
    pub default_value: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PromptResponse {
    Submitted(String),
    Cancelled,
}
pub trait PromptBackend: Send + Sync {
    fn prompt(&self, request: PromptRequest, control: &RunControl) -> ExecResult<PromptResponse>;
}

pub struct PendingPrompt {
    pub request: PromptRequest,
    response: Option<mpsc::Sender<(u64, PromptResponse)>>,
}
impl PendingPrompt {
    pub fn respond(mut self, response: PromptResponse) -> bool {
        self.response
            .take()
            .is_some_and(|tx| tx.send((self.request.id, response)).is_ok())
    }
}
pub struct PromptBroker {
    pending: Mutex<Option<PendingPrompt>>,
    repaint: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}
impl Default for PromptBroker {
    fn default() -> Self {
        Self {
            pending: Mutex::new(None),
            repaint: Mutex::new(None),
        }
    }
}
impl PromptBroker {
    pub fn set_repaint(&self, callback: Arc<dyn Fn() + Send + Sync>) {
        *self.repaint.lock().unwrap() = Some(callback);
    }
    pub fn take_pending(&self) -> Option<PendingPrompt> {
        self.pending.lock().unwrap().take()
    }
    fn withdraw(&self, id: u64) {
        let mut p = self.pending.lock().unwrap();
        if p.as_ref().is_some_and(|p| p.request.id == id) {
            p.take();
        }
    }
}
impl PromptBackend for PromptBroker {
    fn prompt(&self, request: PromptRequest, control: &RunControl) -> ExecResult<PromptResponse> {
        let (tx, rx) = mpsc::channel();
        {
            let mut slot = self.pending.lock().unwrap();
            if slot.is_some() {
                return Err(ExecutionDiagnostic::new(
                    DiagnosticKind::Backend,
                    "another prompt is already active",
                )
                .context("backend", "prompt"));
            }
            *slot = Some(PendingPrompt {
                request: request.clone(),
                response: Some(tx),
            });
        }
        if let Some(cb) = self.repaint.lock().unwrap().as_ref() {
            cb();
        }
        loop {
            if control.is_stopped() {
                self.withdraw(request.id);
                return Err(ExecutionDiagnostic::new(
                    DiagnosticKind::Cancelled,
                    "input prompt cancelled because automation stopped",
                ));
            }
            match rx.recv_timeout(Duration::from_millis(20)) {
                Ok((id, response)) if id == request.id => return Ok(response),
                Ok(_) | Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    self.withdraw(request.id);
                    return Err(ExecutionDiagnostic::new(
                        DiagnosticKind::Backend,
                        "prompt response channel disconnected",
                    )
                    .context("backend", "prompt"));
                }
            }
        }
    }
}
static BROKER: OnceLock<Arc<PromptBroker>> = OnceLock::new();
pub fn production_prompt_broker() -> Arc<PromptBroker> {
    BROKER
        .get_or_init(|| Arc::new(PromptBroker::default()))
        .clone()
}
static IDS: AtomicU64 = AtomicU64::new(1);
pub fn next_request_id() -> u64 {
    IDS.fetch_add(1, Ordering::Relaxed)
}

#[cfg(all(test, windows))]
mod manual_tests {
    /// Manual smoke procedure: hide/minimize the main launcher, run a macro
    /// containing Prompt for Input, verify the child viewport is focused and
    /// always-on-top, then exercise OK, Cancel, Escape, Enter, and native close.
    #[test]
    #[ignore = "manual Windows native-viewport smoke test"]
    fn prompt_remains_usable_while_launcher_is_hidden() {}
}
