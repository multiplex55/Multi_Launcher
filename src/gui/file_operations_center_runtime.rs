//! Relays accepted File Operations Center intents to the existing review and job owners.
use super::add_action_dialog::NativeDialogParent;
use super::file_operations_center_dialog::{
    Command, DraftTicket, FileOperationsCenterDialog, PickerKind, RecoveryPreview, RecoveryVerb,
    ReviewTicket, Runtime,
};
use crate::file_operations::ScanCancellation;
use crate::file_transfer::{
    ApprovedTransferPlan, PreparedReceiptRetry, PreparedReceiptUndo, ReceiptDetail, ReceiptSummary,
    TransferHistoryError, TransferHistoryQuery, TransferHistoryValue, TransferJobs, TransferReview,
    inspect_source_tree, review_transfer,
};
use eframe::egui;
use std::num::NonZeroIsize;
use std::sync::{
    Arc,
    mpsc::{self, Receiver, TryRecvError},
};
use std::time::Duration;

enum WorkResult {
    Review(Result<TransferReview, String>),
    Approval(Result<ApprovedTransferPlan, String>),
    Notice(String),
}

struct PendingWork {
    draft: DraftTicket,
    review: Option<ReviewTicket>,
    cancel: ScanCancellation,
    result: Receiver<WorkResult>,
}

enum PreparedRecovery {
    Retry(PreparedReceiptRetry),
    Undo(PreparedReceiptUndo),
}

impl PreparedRecovery {
    fn preview(&self, token: u64) -> RecoveryPreview {
        let (verb, source, destination) = match self {
            Self::Retry(value) => (RecoveryVerb::Retry, value.source(), value.destination()),
            Self::Undo(value) => (RecoveryVerb::Undo, value.source(), value.destination()),
        };
        RecoveryPreview {
            token,
            verb,
            source: source.display().to_string(),
            destination: destination.display().to_string(),
        }
    }

    fn approve(self, cancel: &ScanCancellation) -> Result<ApprovedTransferPlan, String> {
        match self {
            Self::Retry(value) => value.approve(cancel),
            Self::Undo(value) => value.approve(cancel),
        }
        .map_err(|error| error.to_string())
    }
}

#[derive(Default)]
pub(super) struct FileOperationsCenterRuntime {
    work: Option<PendingWork>,
    history_request: Option<(u64, u64)>,
    history_session: Option<u64>,
    history: Arc<[ReceiptSummary]>,
    detail: Option<Arc<ReceiptDetail>>,
    recovery: Option<(u64, PreparedRecovery)>,
}

impl FileOperationsCenterRuntime {
    pub(super) fn snapshot(&self, jobs: &TransferJobs) -> Runtime {
        Runtime {
            queue: jobs.snapshot(),
            history: self.history.clone(),
            detail: self.detail.clone(),
            recovery: self
                .recovery
                .as_ref()
                .map(|(token, value)| value.preview(*token)),
            busy: self.work.is_some(),
            history_waiting: self.history_request.is_some(),
        }
    }

    pub(super) fn needs_poll(&self) -> bool {
        self.work.is_some() || self.history_request.is_some()
    }

    pub(super) fn close_view(&mut self) {
        self.recovery = None;
        if let Some(pending) = &self.work {
            pending.cancel.cancel();
        }
    }

    fn run(
        &mut self,
        ctx: &egui::Context,
        dialog: &mut FileOperationsCenterDialog,
        review: Option<ReviewTicket>,
        job: impl FnOnce(&ScanCancellation) -> WorkResult + Send + 'static,
    ) {
        let draft = dialog.ticket();
        if self.work.is_some() {
            dialog.set_notice(draft, "Wait for the current request to finish.".into());
            return;
        }
        let cancel = ScanCancellation::new();
        let task_cancel = cancel.clone();
        let (send, result) = mpsc::sync_channel(1);
        let repaint = ctx.clone();
        match std::thread::Builder::new()
            .name("file-operations-review".into())
            .spawn(move || {
                let _ = send.send(job(&task_cancel));
                repaint.request_repaint();
            }) {
            Ok(_) => {
                self.work = Some(PendingWork {
                    draft,
                    review,
                    cancel,
                    result,
                })
            }
            Err(error) => {
                dialog.set_notice(draft, format!("Could not start the request. {error}"));
            }
        }
    }

    fn history(
        &mut self,
        dialog: &mut FileOperationsCenterDialog,
        jobs: &mut TransferJobs,
        query: TransferHistoryQuery,
    ) {
        self.recovery = None;
        match jobs.request_history(query) {
            Ok(request) => self.history_request = Some((request, dialog.ticket().session)),
            Err(error) => {
                dialog.set_notice(dialog.ticket(), history_error(error).into());
            }
        }
    }

    pub(super) fn poll(
        &mut self,
        ctx: &egui::Context,
        dialog: &mut FileOperationsCenterDialog,
        jobs: &mut TransferJobs,
    ) {
        if let Some(pending) = &self.work {
            if !dialog.open || pending.draft != dialog.ticket() {
                pending.cancel.cancel();
            }
            let result = match pending.result.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Disconnected) => Some(WorkResult::Notice(
                    "The file operation request stopped.".into(),
                )),
                Err(TryRecvError::Empty) => None,
            };
            if let Some(result) = result {
                let accepted = dialog.open
                    && pending.draft == dialog.ticket()
                    && pending
                        .review
                        .is_none_or(|ticket| dialog.review_ticket() == Some(ticket));
                self.work = None;
                if accepted {
                    match result {
                        WorkResult::Review(review) => {
                            dialog.set_review(dialog.ticket(), review);
                        }
                        WorkResult::Notice(message) | WorkResult::Approval(Err(message)) => {
                            dialog.set_notice(dialog.ticket(), message);
                        }
                        WorkResult::Approval(Ok(plan)) => {
                            let client = jobs.queue_client();
                            self.run(ctx, dialog, None, move |_| {
                                WorkResult::Notice(match client.enqueue(plan) {
                                    Ok(_) => "Queued. Select Start to run the job.".into(),
                                    Err(error) => error.to_string(),
                                })
                            });
                        }
                    }
                }
            }
        }

        if let Some(response) = jobs.take_history_response() {
            if let Some((request, session)) = self.history_request.take()
                && dialog.open
                && request == response.request
                && session == dialog.ticket().session
            {
                match response.result {
                    Ok(TransferHistoryValue::Summaries(rows)) => self.history = rows.into(),
                    Ok(TransferHistoryValue::Detail(detail)) => {
                        self.detail = Some(Arc::new(detail))
                    }
                    Ok(TransferHistoryValue::Retry(value)) => {
                        self.recovery = Some((request, PreparedRecovery::Retry(value)));
                    }
                    Ok(TransferHistoryValue::Undo(value)) => {
                        self.recovery = Some((request, PreparedRecovery::Undo(value)));
                    }
                    Ok(
                        TransferHistoryValue::Interrupted(_) | TransferHistoryValue::Discarded(_),
                    ) => {}
                    Err(error) => {
                        dialog.set_notice(dialog.ticket(), history_error(error).into());
                    }
                }
            }
        }

        if dialog.open && self.history_request.is_none() {
            let session = dialog.ticket().session;
            if self.history_session != Some(session) {
                self.history_session = Some(session);
                self.history(dialog, jobs, TransferHistoryQuery::Interrupted);
            }
        }
    }

    pub(super) fn dispatch(
        &mut self,
        ctx: &egui::Context,
        dialog: &mut FileOperationsCenterDialog,
        jobs: &mut TransferJobs,
        command: Command,
    ) {
        let current = dialog.ticket();
        match command {
            Command::Closed => self.close_view(),
            Command::Pick { ticket, kind, hwnd } if ticket == current => {
                let Some(hwnd) = NonZeroIsize::new(hwnd as isize) else {
                    dialog.set_notice(ticket, "The picker window is unavailable.".into());
                    return;
                };
                let picker = rfd::FileDialog::new().set_parent(&NativeDialogParent(hwnd));
                let selected = match kind {
                    PickerKind::Files => picker.pick_files(),
                    PickerKind::Folder | PickerKind::Destination => {
                        picker.pick_folder().map(|path| vec![path])
                    }
                };
                if let Some(paths) = selected {
                    dialog.set_picker_result(ticket, kind, paths);
                }
            }
            Command::Review {
                ticket,
                sources,
                draft,
            } if ticket == current => {
                self.run(ctx, dialog, None, move |cancel| {
                    let result = inspect_source_tree(&sources, cancel)
                        .map_err(|error| error.to_string())
                        .and_then(|tree| {
                            review_transfer(Arc::new(tree), draft, cancel)
                                .map_err(|error| error.to_string())
                        });
                    WorkResult::Review(result)
                });
            }
            Command::Choice {
                ticket,
                node,
                choice,
            } if Some(ticket) == dialog.review_ticket() => {
                let Some(review) = dialog.review().cloned() else {
                    return;
                };
                self.run(ctx, dialog, Some(ticket), move |cancel| {
                    review
                        .choose(ticket.generation, node, choice, cancel)
                        .map_or_else(
                            |error| WorkResult::Notice(error.to_string()),
                            |value| WorkResult::Review(Ok(value)),
                        )
                });
            }
            Command::Group {
                ticket,
                nodes,
                choice,
            } if Some(ticket) == dialog.review_ticket() => {
                let Some(review) = dialog.review().cloned() else {
                    return;
                };
                self.run(ctx, dialog, Some(ticket), move |cancel| {
                    review
                        .choose_group(ticket.generation, &nodes, choice, cancel)
                        .map_or_else(
                            |error| WorkResult::Notice(error.to_string()),
                            |value| WorkResult::Review(Ok(value)),
                        )
                });
            }
            Command::Queue { ticket } if Some(ticket) == dialog.review_ticket() => {
                let Some(review) = dialog.review().cloned() else {
                    return;
                };
                self.run(ctx, dialog, Some(ticket), move |cancel| {
                    WorkResult::Approval(
                        review
                            .approve(ticket.generation, cancel)
                            .map_err(|error| error.to_string()),
                    )
                });
            }
            Command::Start => {
                if let Err(error) = jobs.start() {
                    dialog.set_notice(current, error.to_string());
                }
            }
            Command::Reorder { job, before } => {
                let client = jobs.queue_client();
                self.run(ctx, dialog, None, move |_| {
                    WorkResult::Notice(match client.try_reorder(job, before) {
                        Ok(true) => "Queue order saved.".into(),
                        Ok(false) => "The selected queue position is no longer available.".into(),
                        Err(error) => error.to_string(),
                    })
                });
            }
            Command::Pause(job) => {
                jobs.pause(job);
            }
            Command::Resume(job) => {
                jobs.resume(job);
            }
            Command::Cancel(job) => {
                if !jobs.cancel_active(job) {
                    let client = jobs.queue_client();
                    self.run(ctx, dialog, None, move |_| {
                        WorkResult::Notice(match client.try_cancel(job) {
                            Ok(true) => "Cancellation requested or queued job cancelled.".into(),
                            Ok(false) => "The selected job is no longer available.".into(),
                            Err(error) => error.to_string(),
                        })
                    });
                }
            }
            Command::History(query) => {
                if matches!(query, TransferHistoryQuery::Detail(_)) {
                    self.detail = None;
                }
                self.history(dialog, jobs, query);
            }
            Command::ConfirmRecovery { token, verb } => {
                if self.work.is_none()
                    && self.recovery.as_ref().is_some_and(|(id, recovery)| {
                        *id == token && recovery.preview(*id).verb == verb
                    })
                    && let Some((_, recovery)) = self.recovery.take()
                {
                    self.run(ctx, dialog, None, move |cancel| {
                        WorkResult::Approval(recovery.approve(cancel))
                    });
                }
            }
            Command::CancelRecovery { token } => {
                if self.recovery.as_ref().is_some_and(|(id, _)| *id == token) {
                    self.recovery = None;
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, time::Instant};

    fn wait_for(
        ctx: &egui::Context,
        controller: &mut FileOperationsCenterRuntime,
        dialog: &mut FileOperationsCenterDialog,
        jobs: &mut TransferJobs,
        mut observed: impl FnMut(&FileOperationsCenterDialog, &TransferJobs) -> bool,
    ) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            controller.poll(ctx, dialog, jobs);
            if observed(dialog, jobs) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "File Operations action timed out"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn file_operations_center_async_review_queue_and_start_are_separate() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("first.txt");
        let second = temp.path().join("second.txt");
        let destination = temp.path().join("destination");
        fs::write(&first, b"first").unwrap();
        fs::write(&second, b"second").unwrap();
        fs::create_dir(&destination).unwrap();

        let ctx = egui::Context::default();
        let mut jobs = TransferJobs::new(crate::activity::ActivityCenter::new());
        let mut controller = FileOperationsCenterRuntime::default();
        let mut dialog = FileOperationsCenterDialog::default();
        dialog.open_with_sources(vec![first.clone()]);
        assert!(dialog.set_picker_result(
            dialog.ticket(),
            PickerKind::Destination,
            vec![destination.clone()]
        ));
        let review_first = Command::Review {
            ticket: dialog.ticket(),
            sources: vec![first],
            draft: dialog.draft().unwrap(),
        };
        controller.dispatch(&ctx, &mut dialog, &mut jobs, review_first);
        dialog.open_with_sources(vec![second.clone()]);
        let stale_deadline = Instant::now() + Duration::from_secs(5);
        while controller.work.is_some() {
            controller.poll(&ctx, &mut dialog, &mut jobs);
            assert!(
                Instant::now() < stale_deadline,
                "Stale review did not retire"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(dialog.review().is_none());

        let review_second = Command::Review {
            ticket: dialog.ticket(),
            sources: vec![second],
            draft: dialog.draft().unwrap(),
        };
        controller.dispatch(&ctx, &mut dialog, &mut jobs, review_second);
        wait_for(
            &ctx,
            &mut controller,
            &mut dialog,
            &mut jobs,
            |dialog, _| dialog.review_ticket().is_some(),
        );
        let ticket = dialog.review_ticket().unwrap();
        controller.dispatch(&ctx, &mut dialog, &mut jobs, Command::Queue { ticket });
        wait_for(&ctx, &mut controller, &mut dialog, &mut jobs, |_, jobs| {
            jobs.snapshot().queued.len() == 1
        });
        assert!(!destination.join("first.txt").exists());
        assert!(!destination.join("second.txt").exists());

        controller.dispatch(&ctx, &mut dialog, &mut jobs, Command::Start);
        wait_for(&ctx, &mut controller, &mut dialog, &mut jobs, |_, jobs| {
            !jobs.snapshot().history.is_empty()
        });
        assert_eq!(fs::read(destination.join("second.txt")).unwrap(), b"second");
        assert!(!destination.join("first.txt").exists());
    }
}

fn history_error(error: TransferHistoryError) -> &'static str {
    match error {
        TransferHistoryError::Busy => "A history request is already pending.",
        TransferHistoryError::Unavailable => "The receipt or queue information is unavailable.",
        TransferHistoryError::Refused => "The recorded operation is unavailable for that action.",
    }
}
