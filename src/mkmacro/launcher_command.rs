//! Thread-safe boundary for Launcher requests sent from macro workers to the GUI.

use super::executor::{DiagnosticKind, ExecResult, ExecutionDiagnostic, RunControl};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq)]
pub enum LauncherCommandKind {
    Query(String),
    /// Compatibility-only action produced by macro document migration.
    ResolvedLegacy(crate::actions::Action),
}

#[derive(Clone, Debug, PartialEq)]
pub struct LauncherCommandRequest {
    pub id: u64,
    pub kind: LauncherCommandKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LauncherCommandResponse {
    Activated,
    PresentedForSelection { result_count: usize },
    NoResults,
    Failed(String),
}

pub struct PendingLauncherCommand {
    pub request: LauncherCommandRequest,
    response: Option<mpsc::Sender<(u64, LauncherCommandResponse)>>,
}

impl PendingLauncherCommand {
    pub fn respond(mut self, response: LauncherCommandResponse) -> bool {
        self.response
            .take()
            .is_some_and(|tx| tx.send((self.request.id, response)).is_ok())
    }
}

pub struct LauncherCommandBroker {
    pending: Mutex<Option<PendingLauncherCommand>>,
    repaint: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
    next_id: AtomicU64,
}

impl Default for LauncherCommandBroker {
    fn default() -> Self {
        Self {
            pending: Mutex::new(None),
            repaint: Mutex::new(None),
            next_id: AtomicU64::new(1),
        }
    }
}

impl LauncherCommandBroker {
    pub fn set_repaint(&self, callback: Arc<dyn Fn() + Send + Sync>) {
        *self.repaint.lock().unwrap() = Some(callback);
    }

    pub fn take_pending(&self) -> Option<PendingLauncherCommand> {
        self.pending.lock().unwrap().take()
    }

    /// Removes and returns the pending request only when its ID matches.
    pub fn withdraw_pending(&self, request_id: u64) -> Option<PendingLauncherCommand> {
        let mut pending = self.pending.lock().unwrap();
        if pending
            .as_ref()
            .is_some_and(|pending| pending.request.id == request_id)
        {
            pending.take()
        } else {
            None
        }
    }

    pub fn submit_query(
        &self,
        query: impl Into<String>,
        control: &RunControl,
    ) -> ExecResult<LauncherCommandResponse> {
        self.submit_kind(LauncherCommandKind::Query(query.into()), control)
    }

    pub fn submit_resolved_legacy(
        &self,
        action: crate::actions::Action,
        control: &RunControl,
    ) -> ExecResult<LauncherCommandResponse> {
        self.submit_kind(LauncherCommandKind::ResolvedLegacy(action), control)
    }

    fn submit_kind(
        &self,
        kind: LauncherCommandKind,
        control: &RunControl,
    ) -> ExecResult<LauncherCommandResponse> {
        let diagnostic_value = match &kind {
            LauncherCommandKind::Query(query) => ("query", query.clone()),
            LauncherCommandKind::ResolvedLegacy(action) => ("action", action.action.clone()),
        };
        let (tx, rx) = mpsc::channel();
        let request_id;
        {
            let mut pending = self.pending.lock().unwrap();
            if pending.is_some() {
                return Err(ExecutionDiagnostic::new(
                    DiagnosticKind::Backend,
                    "another Launcher command request is already active",
                ));
            }
            request_id = self.next_id.fetch_add(1, Ordering::Relaxed);
            *pending = Some(PendingLauncherCommand {
                request: LauncherCommandRequest {
                    id: request_id,
                    kind,
                },
                response: Some(tx),
            });
        }

        let repaint = self.repaint.lock().unwrap().clone();
        if let Some(repaint) = repaint {
            repaint();
        }

        loop {
            if control.is_stopped() {
                // Withdrawal prevents execution only until the GUI takes ownership.
                // Once taken, dropping `rx` makes the eventual response harmless.
                self.withdraw_pending(request_id);
                return Err(ExecutionDiagnostic::new(
                    DiagnosticKind::Cancelled,
                    "Launcher command cancelled because automation stopped",
                )
                .context(diagnostic_value.0, diagnostic_value.1));
            }
            match rx.recv_timeout(Duration::from_millis(20)) {
                Ok((id, response)) if id == request_id => return Ok(response),
                Ok(_) | Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    self.withdraw_pending(request_id);
                    return Err(ExecutionDiagnostic::new(
                        DiagnosticKind::Backend,
                        "Launcher command response channel disconnected",
                    )
                    .context("backend", "launcher")
                    .context(diagnostic_value.0, diagnostic_value.1));
                }
            }
        }
    }
}

static BROKER: OnceLock<Arc<LauncherCommandBroker>> = OnceLock::new();

pub fn production_launcher_command_broker() -> Arc<LauncherCommandBroker> {
    BROKER
        .get_or_init(|| Arc::new(LauncherCommandBroker::default()))
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Barrier, mpsc};
    use std::thread;

    fn submit_async(
        broker: &Arc<LauncherCommandBroker>,
        query: &str,
    ) -> thread::JoinHandle<ExecResult<LauncherCommandResponse>> {
        let broker = Arc::clone(broker);
        let query = query.to_owned();
        thread::spawn(move || broker.submit_query(query, &RunControl::default()))
    }

    fn wait_for_pending(broker: &LauncherCommandBroker) -> PendingLauncherCommand {
        for _ in 0..100 {
            if let Some(pending) = broker.take_pending() {
                return pending;
            }
            thread::sleep(Duration::from_millis(2));
        }
        panic!("submission did not install a pending request");
    }

    fn submit_with_control(
        broker: &Arc<LauncherCommandBroker>,
        query: &str,
        control: Arc<RunControl>,
    ) -> (
        mpsc::Receiver<ExecResult<LauncherCommandResponse>>,
        thread::JoinHandle<()>,
    ) {
        let broker = Arc::clone(broker);
        let query = query.to_owned();
        let (result_tx, result_rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            result_tx
                .send(broker.submit_query(query, &control))
                .unwrap();
        });
        (result_rx, handle)
    }

    #[test]
    fn ids_increase_monotonically_and_query_is_exact() {
        let broker = Arc::new(LauncherCommandBroker::default());
        let first = submit_async(&broker, "  first QUERY  ");
        let pending = wait_for_pending(&broker);
        let first_id = pending.request.id;
        assert_eq!(
            match &pending.request.kind {
                LauncherCommandKind::Query(query) => query,
                _ => panic!("expected query"),
            },
            "  first QUERY  "
        );
        assert!(pending.respond(LauncherCommandResponse::Activated));
        assert_eq!(
            first.join().unwrap().unwrap(),
            LauncherCommandResponse::Activated
        );

        let second = submit_async(&broker, "second");
        let pending = wait_for_pending(&broker);
        assert!(pending.request.id > first_id);
        assert!(pending.respond(LauncherCommandResponse::NoResults));
        second.join().unwrap().unwrap();
    }

    #[test]
    fn both_request_kinds_round_trip_and_preserve_ids() {
        let broker = Arc::new(LauncherCommandBroker::default());
        let query_worker = submit_async(&broker, "query");
        let query = wait_for_pending(&broker);
        let query_id = query.request.id;
        assert_eq!(
            query.request.kind,
            LauncherCommandKind::Query("query".into())
        );
        assert!(query.respond(LauncherCommandResponse::NoResults));
        query_worker.join().unwrap().unwrap();

        let action = crate::actions::Action {
            label: "Notes".into(),
            desc: "Open dialog".into(),
            action: "note:dialog".into(),
            args: Some("payload".into()),
        };
        let submit_broker = Arc::clone(&broker);
        let expected = action.clone();
        let worker = thread::spawn(move || {
            submit_broker.submit_resolved_legacy(action, &RunControl::default())
        });
        let legacy = wait_for_pending(&broker);
        assert!(legacy.request.id > query_id);
        assert_eq!(
            legacy.request.kind,
            LauncherCommandKind::ResolvedLegacy(expected)
        );
        assert!(legacy.respond(LauncherCommandResponse::Activated));
        assert_eq!(
            worker.join().unwrap().unwrap(),
            LauncherCommandResponse::Activated
        );
    }

    #[test]
    fn query_and_legacy_share_pending_slot() {
        let broker = Arc::new(LauncherCommandBroker::default());
        let first = submit_async(&broker, "first");
        while broker.pending.lock().unwrap().is_none() {
            thread::yield_now();
        }
        let error = broker
            .submit_resolved_legacy(
                crate::actions::Action {
                    label: "legacy".into(),
                    desc: String::new(),
                    action: "help:show".into(),
                    args: None,
                },
                &RunControl::default(),
            )
            .unwrap_err();
        assert_eq!(
            error.message,
            "another Launcher command request is already active"
        );
        assert!(wait_for_pending(&broker).respond(LauncherCommandResponse::Activated));
        first.join().unwrap().unwrap();
    }

    #[test]
    fn taking_removes_the_pending_request() {
        let broker = Arc::new(LauncherCommandBroker::default());
        let submitter = submit_async(&broker, "query");
        let pending = wait_for_pending(&broker);
        assert!(broker.take_pending().is_none());
        assert!(pending.respond(LauncherCommandResponse::Activated));
        submitter.join().unwrap().unwrap();
    }

    #[test]
    fn every_response_variant_round_trips() {
        let responses = [
            LauncherCommandResponse::Activated,
            LauncherCommandResponse::PresentedForSelection { result_count: 7 },
            LauncherCommandResponse::NoResults,
            LauncherCommandResponse::Failed("bad query".to_owned()),
        ];
        let broker = Arc::new(LauncherCommandBroker::default());
        for expected in responses {
            let submitter = submit_async(&broker, "query");
            assert!(wait_for_pending(&broker).respond(expected.clone()));
            assert_eq!(submitter.join().unwrap().unwrap(), expected);
        }
    }

    #[test]
    fn occupied_slot_rejects_without_replacing_first_request() {
        let broker = Arc::new(LauncherCommandBroker::default());
        let first = submit_async(&broker, "first");
        while broker.pending.lock().unwrap().is_none() {
            thread::yield_now();
        }
        let error = broker
            .submit_query("second", &RunControl::default())
            .unwrap_err();
        assert_eq!(error.kind, DiagnosticKind::Backend);
        assert_eq!(
            error.message,
            "another Launcher command request is already active"
        );
        let pending = broker.take_pending().unwrap();
        assert_eq!(
            match &pending.request.kind {
                LauncherCommandKind::Query(query) => query,
                _ => panic!("expected query"),
            },
            "first"
        );
        assert!(pending.respond(LauncherCommandResponse::Activated));
        first.join().unwrap().unwrap();
    }

    #[test]
    fn repaint_runs_after_submission_and_can_reenter() {
        let broker = Arc::new(LauncherCommandBroker::default());
        let called = Arc::new(AtomicBool::new(false));
        let callback_broker = Arc::clone(&broker);
        let callback_called = Arc::clone(&called);
        broker.set_repaint(Arc::new(move || {
            let pending = callback_broker.take_pending().unwrap();
            callback_called.store(true, Ordering::SeqCst);
            pending.respond(LauncherCommandResponse::Activated);
        }));

        assert_eq!(
            broker
                .submit_query("query", &RunControl::default())
                .unwrap(),
            LauncherCommandResponse::Activated
        );
        assert!(called.load(Ordering::SeqCst));
    }

    #[test]
    fn dropped_gui_sender_is_a_backend_diagnostic() {
        let broker = Arc::new(LauncherCommandBroker::default());
        let submitter = submit_async(&broker, "query");
        drop(wait_for_pending(&broker));
        let error = submitter.join().unwrap().unwrap_err();
        assert_eq!(error.kind, DiagnosticKind::Backend);
        assert_eq!(
            error.message,
            "Launcher command response channel disconnected"
        );
    }

    #[test]
    fn wrong_response_id_is_ignored() {
        let broker = Arc::new(LauncherCommandBroker::default());
        let submitter = submit_async(&broker, "query");
        let mut pending = wait_for_pending(&broker);
        let tx = pending.response.take().unwrap();
        tx.send((pending.request.id + 1, LauncherCommandResponse::NoResults))
            .unwrap();
        tx.send((pending.request.id, LauncherCommandResponse::Activated))
            .unwrap();
        assert_eq!(
            submitter.join().unwrap().unwrap(),
            LauncherCommandResponse::Activated
        );
    }

    #[test]
    fn waiting_submitter_completes_after_gui_response() {
        let broker = Arc::new(LauncherCommandBroker::default());
        let submitter = submit_async(&broker, "query");
        let pending = wait_for_pending(&broker);
        assert!(pending.respond(LauncherCommandResponse::NoResults));
        assert_eq!(
            submitter.join().unwrap().unwrap(),
            LauncherCommandResponse::NoResults
        );
    }

    #[test]
    fn stop_cancels_promptly_and_withdraws_pending_request() {
        let broker = Arc::new(LauncherCommandBroker::default());
        let control = Arc::new(RunControl::default());
        let (result, submitter) = submit_with_control(&broker, "cancel me", control.clone());
        while broker.pending.lock().unwrap().is_none() {
            thread::yield_now();
        }
        control.stop();
        let error = result
            .recv_timeout(Duration::from_millis(250))
            .expect("stop must wake the polling submitter")
            .unwrap_err();
        assert_eq!(error.kind, DiagnosticKind::Cancelled);
        assert_eq!(
            error.context.get("query").map(String::as_str),
            Some("cancel me")
        );
        assert!(broker.take_pending().is_none());
        submitter.join().unwrap();
    }

    #[test]
    fn stop_withdraws_pending_legacy_request() {
        let broker = Arc::new(LauncherCommandBroker::default());
        let control = Arc::new(RunControl::default());
        let submit_broker = Arc::clone(&broker);
        let submit_control = Arc::clone(&control);
        let worker = thread::spawn(move || {
            submit_broker.submit_resolved_legacy(
                crate::actions::Action {
                    label: "Help".into(),
                    desc: String::new(),
                    action: "help:show".into(),
                    args: None,
                },
                &submit_control,
            )
        });
        while broker.pending.lock().unwrap().is_none() {
            thread::yield_now();
        }
        control.stop();
        assert_eq!(
            worker.join().unwrap().unwrap_err().kind,
            DiagnosticKind::Cancelled
        );
        assert!(broker.take_pending().is_none());
    }

    #[test]
    fn cancellation_of_taken_request_does_not_withdraw_later_request() {
        let broker = Arc::new(LauncherCommandBroker::default());
        let first_control = Arc::new(RunControl::default());
        let (first_result, first_submitter) =
            submit_with_control(&broker, "first", first_control.clone());
        let first = wait_for_pending(&broker);

        let second_control = Arc::new(RunControl::default());
        let (second_result, second_submitter) =
            submit_with_control(&broker, "second", second_control);
        while broker.pending.lock().unwrap().is_none() {
            thread::yield_now();
        }

        first_control.stop();
        assert_eq!(
            first_result
                .recv_timeout(Duration::from_millis(250))
                .unwrap()
                .unwrap_err()
                .kind,
            DiagnosticKind::Cancelled
        );
        let second = broker
            .take_pending()
            .expect("request N+1 must remain pending");
        assert_eq!(
            second.request.kind,
            LauncherCommandKind::Query("second".into())
        );
        assert!(!first.respond(LauncherCommandResponse::Activated));
        assert!(second.respond(LauncherCommandResponse::Activated));
        assert_eq!(
            second_result.recv().unwrap().unwrap(),
            LauncherCommandResponse::Activated
        );
        first_submitter.join().unwrap();
        second_submitter.join().unwrap();
    }

    #[test]
    fn pause_does_not_masquerade_as_cancellation() {
        let broker = Arc::new(LauncherCommandBroker::default());
        let control = Arc::new(RunControl::default());
        control.pause();
        let (result, submitter) = submit_with_control(&broker, "paused", control);
        assert!(wait_for_pending(&broker).respond(LauncherCommandResponse::Activated));
        assert_eq!(
            result.recv().unwrap().unwrap(),
            LauncherCommandResponse::Activated
        );
        submitter.join().unwrap();
    }

    #[test]
    fn response_racing_with_stop_has_one_terminal_result_and_no_stale_slot() {
        let broker = Arc::new(LauncherCommandBroker::default());
        let control = Arc::new(RunControl::default());
        let (result, submitter) = submit_with_control(&broker, "race", control.clone());
        let pending = wait_for_pending(&broker);
        let barrier = Arc::new(Barrier::new(3));
        let stop_barrier = barrier.clone();
        let stopper = thread::spawn(move || {
            stop_barrier.wait();
            control.stop();
        });
        let response_barrier = barrier.clone();
        let responder = thread::spawn(move || {
            response_barrier.wait();
            pending.respond(LauncherCommandResponse::Activated)
        });
        barrier.wait();

        let terminal = result.recv_timeout(Duration::from_millis(250)).unwrap();
        assert!(matches!(
            terminal,
            Ok(LauncherCommandResponse::Activated)
                | Err(ExecutionDiagnostic {
                    kind: DiagnosticKind::Cancelled,
                    ..
                })
        ));
        stopper.join().unwrap();
        let _ = responder.join().unwrap();
        submitter.join().unwrap();
        assert!(broker.take_pending().is_none());
    }
}
