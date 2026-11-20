//! Explicit PDF and printer output over an accepted preview scene.

#[cfg(windows)]
use super::printer_dialog::PrinterChoice;
use super::{DocumentStamp, SceneReceipt};
use crate::gui::utility_host::HostIdentity;
#[cfg(windows)]
use crate::notes_markdown::print_layout::printer::{
    PrintReview, PrintScale, PrintSubmission, PrinterError, prepare_print,
};
use crate::notes_markdown::print_layout::{
    PdfDestinationReview, PdfPublishError, PdfPublishOutcome, PdfTargetKind, pdf::encode_pdf,
    publish_pdf_with_admission, review_pdf_destination_with_cancel,
};
use eframe::egui;
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
        mpsc::{self, Receiver, TryRecvError},
    },
};
#[cfg(windows)]
use std::{sync::atomic::AtomicBool, sync::mpsc::Sender};

#[derive(Clone)]
struct Owner {
    host: HostIdentity,
    scene: Arc<SceneReceipt>,
}

impl Owner {
    fn is_current(&self, host: HostIdentity, scene: Option<&Arc<SceneReceipt>>) -> bool {
        self.host == host && scene.is_some_and(|scene| Arc::ptr_eq(scene, &self.scene))
    }

    fn active_path(&self) -> Option<&Path> {
        match &self.scene.stamp {
            DocumentStamp::Note { path, .. } => (!path.as_os_str().is_empty()).then_some(path),
            DocumentStamp::Studio { path, .. } => path.as_deref(),
        }
    }
}

struct Confirmation {
    owner: Owner,
    review: Review,
}

enum Review {
    Pdf(PdfDestinationReview),
    #[cfg(windows)]
    Printer(PrinterReview),
}

#[cfg(windows)]
pub(super) struct PrinterReview {
    pub device_label: Option<String>,
    pub mode: PrintScale,
    pub layout: PrintReview,
}

enum Reply {
    Reviewed(Result<PdfDestinationReview, String>),
    Published(Result<PdfPublishOutcome, WriteError>),
    #[cfg(windows)]
    PrinterReviewed(PrinterReview),
    #[cfg(windows)]
    Printed(Result<PrintSubmission, PrinterError>),
}

enum WriteError {
    Encoding(crate::notes_markdown::print_layout::pdf::PdfError),
    Publication(PdfPublishError),
}

#[derive(Default)]
struct Admission(AtomicU8);

impl Admission {
    const PENDING: u8 = 0;
    const REVOKED: u8 = 1;
    const ADMITTED: u8 = 2;

    fn decide(&self, state: u8) -> bool {
        self.0
            .compare_exchange(Self::PENDING, state, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    fn revoke(&self) {
        self.decide(Self::REVOKED);
    }

    fn cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire) == Self::REVOKED
    }

    #[cfg(windows)]
    fn admitted(&self) -> bool {
        self.0.load(Ordering::Acquire) == Self::ADMITTED
    }

    fn admit(&self) -> bool {
        self.decide(Self::ADMITTED)
    }
}

enum JobTarget {
    Pdf(PathBuf),
    #[cfg(windows)]
    Printer {
        cancelled: Arc<AtomicBool>,
        approval: Option<Sender<()>>,
    },
}

struct Job {
    owner: Owner,
    target: JobTarget,
    admission: Arc<Admission>,
    reply: Receiver<Reply>,
}

impl Job {
    fn revoke(&mut self) {
        self.admission.revoke();
        #[cfg(windows)]
        if let JobTarget::Printer {
            cancelled,
            approval,
        } = &mut self.target
        {
            cancelled.store(true, Ordering::Release);
            approval.take();
        }
    }

    fn can_cancel(&self) -> bool {
        match &self.target {
            JobTarget::Pdf(_) => self.admission.0.load(Ordering::Acquire) == Admission::PENDING,
            #[cfg(windows)]
            JobTarget::Printer { cancelled, .. } => !cancelled.load(Ordering::Acquire),
        }
    }

    fn cancelling_message(&self) -> &'static str {
        match &self.target {
            JobTarget::Pdf(_) if self.admission.cancelled() => "Cancelling PDF save…",
            JobTarget::Pdf(_) => "Finishing PDF save…",
            #[cfg(windows)]
            JobTarget::Printer { .. } => "Cancelling print…",
        }
    }

    fn pdf_path(&self) -> &Path {
        match &self.target {
            JobTarget::Pdf(path) => path,
            #[cfg(windows)]
            JobTarget::Printer { .. } => unreachable!("PDF reply for printer job"),
        }
    }
}

struct WorkerMail {
    send: mpsc::SyncSender<Reply>,
    ctx: egui::Context,
    viewport: egui::ViewportId,
}

#[cfg(windows)]
impl WorkerMail {
    fn send(&self, reply: Reply) -> bool {
        let sent = self.send.send(reply).is_ok();
        self.wake();
        sent
    }

    fn wake(&self) {
        self.ctx.request_repaint_of(self.viewport);
        self.ctx.request_repaint_of(egui::ViewportId::ROOT);
    }
}

#[derive(Default)]
pub(in crate::gui) struct MarkdownOutput {
    job: Option<Job>,
    confirmation: Option<Confirmation>,
    confirmation_needs_focus: bool,
    feedback: Option<(bool, String)>,
}

impl MarkdownOutput {
    pub(in crate::gui) fn busy(&self) -> bool {
        self.job.is_some()
    }

    pub(super) fn can_cancel_job(&self) -> bool {
        self.job.as_ref().is_some_and(Job::can_cancel)
    }

    pub(super) fn cancel_job_label(&self) -> Option<&'static str> {
        self.job.as_ref().map(|job| match &job.target {
            JobTarget::Pdf(_) => "Cancel PDF save",
            #[cfg(windows)]
            JobTarget::Printer { .. } => "Cancel print",
        })
    }

    pub(in crate::gui) fn confirming(&self) -> bool {
        self.confirmation.is_some()
    }

    pub(super) fn confirmation_needs_focus(&self) -> bool {
        self.confirmation_needs_focus
    }

    pub(super) fn finish_confirmation_focus(&mut self) {
        self.confirmation_needs_focus = false;
    }

    pub(super) fn target_path(&self) -> Option<PathBuf> {
        match &self.confirmation.as_ref()?.review {
            Review::Pdf(review) => Some(review.path()),
            #[cfg(windows)]
            Review::Printer(_) => None,
        }
    }

    #[cfg(windows)]
    pub(super) fn printer_review(&self) -> Option<&PrinterReview> {
        match &self.confirmation.as_ref()?.review {
            Review::Printer(review) => Some(review),
            Review::Pdf(_) => None,
        }
    }

    pub(super) fn feedback(&self) -> Option<(bool, &str)> {
        self.feedback
            .as_ref()
            .map(|(error, text)| (*error, text.as_str()))
    }

    pub(in crate::gui) fn report_error(&mut self, text: impl Into<String>) {
        self.feedback = Some((true, text.into()));
    }

    pub(super) fn retire(&mut self) {
        if let Some(job) = &mut self.job {
            job.revoke();
        }
        self.confirmation = None;
        self.confirmation_needs_focus = false;
        self.feedback = None;
    }

    pub(in crate::gui) fn cancel(&mut self) {
        self.retire();
        let text = self
            .job
            .as_ref()
            .map_or("PDF save cancelled.", Job::cancelling_message);
        self.feedback = Some((false, text.into()));
    }

    pub(in crate::gui) fn observe(
        &mut self,
        host: HostIdentity,
        scene: Option<&Arc<SceneReceipt>>,
    ) {
        if self
            .confirmation
            .as_ref()
            .is_some_and(|item| !item.owner.is_current(host, scene))
        {
            self.confirmation = None;
            self.confirmation_needs_focus = false;
            self.feedback = None;
        }
        if let Some(job) = &mut self.job
            && !job.owner.is_current(host, scene)
        {
            job.revoke();
            self.feedback = None;
        }
    }

    pub(in crate::gui) fn review(
        &mut self,
        path: PathBuf,
        host: HostIdentity,
        scene: Arc<SceneReceipt>,
        ctx: &egui::Context,
        viewport: egui::ViewportId,
    ) {
        if self.busy() || self.confirming() {
            return;
        }
        self.start(
            Owner { host, scene },
            JobTarget::Pdf(path.clone()),
            ctx,
            viewport,
            move |owner, admission, _| {
                let result = review_pdf_destination_with_cancel(&path, owner.active_path(), || {
                    admission.cancelled()
                })
                .map_err(|error| error.to_string());
                Reply::Reviewed(result)
            },
        );
    }

    #[cfg(windows)]
    pub(in crate::gui) fn review_printer(
        &mut self,
        choice: PrinterChoice,
        mode: PrintScale,
        host: HostIdentity,
        scene: Arc<SceneReceipt>,
        ctx: &egui::Context,
        viewport: egui::ViewportId,
    ) {
        if self.busy() || self.confirming() {
            return;
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancelled);
        let (approve, approval) = mpsc::channel();
        self.start(
            Owner { host, scene },
            JobTarget::Printer {
                cancelled,
                approval: Some(approve),
            },
            ctx,
            viewport,
            move |owner, admission, mail| {
                let result = (|| {
                    if worker_cancel.load(Ordering::Acquire) {
                        return Err(PrinterError::Cancelled);
                    }
                    let printer = prepare_print(
                        choice.dc,
                        Arc::clone(&owner.scene.scene),
                        choice.pages,
                        mode,
                    )?;
                    if !mail.send(Reply::PrinterReviewed(PrinterReview {
                        device_label: choice.device_label,
                        mode,
                        layout: printer.review().clone(),
                    })) || approval.recv().is_err()
                    {
                        return Err(PrinterError::Cancelled);
                    }
                    printer.submit(
                        "Multi Launcher Markdown",
                        Arc::clone(&worker_cancel),
                        || !worker_cancel.load(Ordering::Acquire),
                        || admission.admit(),
                    )
                })();
                Reply::Printed(result)
            },
        );
    }

    pub(in crate::gui) fn confirm(
        &mut self,
        host: HostIdentity,
        scene: &Arc<SceneReceipt>,
        ctx: &egui::Context,
        viewport: egui::ViewportId,
    ) {
        self.confirmation_needs_focus = false;
        let Some(confirmation) = self.confirmation.take() else {
            return;
        };
        if !confirmation.owner.is_current(host, Some(scene)) {
            self.retire();
            return;
        }
        match confirmation.review {
            Review::Pdf(review) => self.write(confirmation.owner, review, ctx, viewport),
            #[cfg(windows)]
            Review::Printer(_) => {
                if let Some(Job {
                    target: JobTarget::Printer { approval, .. },
                    ..
                }) = &mut self.job
                    && let Some(sender) = approval.take()
                {
                    if sender.send(()).is_ok() {
                        self.feedback = Some((false, "Submitting print job…".into()));
                    }
                }
            }
        }
    }

    pub(in crate::gui) fn poll(
        &mut self,
        host: HostIdentity,
        scene: Option<&Arc<SceneReceipt>>,
        ctx: &egui::Context,
        viewport: egui::ViewportId,
    ) -> Option<(egui_toast::ToastKind, String)> {
        use egui_toast::ToastKind;

        let job = self.job.as_mut()?;
        let reply = match job.reply.try_recv() {
            Ok(reply) => reply,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => {
                self.retire();
                self.job = None;
                return Some((
                    ToastKind::Error,
                    "The Markdown output worker ended before reporting a result.".into(),
                ));
            }
        };
        #[cfg(windows)]
        let reply = match reply {
            Reply::PrinterReviewed(review) => {
                if job.owner.is_current(host, scene) && job.can_cancel() {
                    self.confirmation = Some(Confirmation {
                        owner: job.owner.clone(),
                        review: Review::Printer(review),
                    });
                    self.confirmation_needs_focus = true;
                    self.feedback = None;
                } else {
                    job.revoke();
                }
                ctx.request_repaint_of(viewport);
                return None;
            }
            terminal => terminal,
        };
        let job = self.job.take()?;
        #[cfg(windows)]
        if matches!(job.target, JobTarget::Printer { .. }) {
            self.confirmation = None;
            self.confirmation_needs_focus = false;
        }
        if !job.owner.is_current(host, scene) {
            return match reply {
                Reply::Published(Ok(outcome)) => Some(saved_notice(job.pdf_path(), outcome)),
                Reply::Published(Err(WriteError::Publication(
                    error @ (PdfPublishError::Recovery { .. } | PdfPublishError::Io(_)),
                ))) => Some((ToastKind::Error, error.to_string())),
                #[cfg(windows)]
                Reply::Printed(Ok(submission)) => {
                    Some((ToastKind::Success, submitted_message(submission)))
                }
                #[cfg(windows)]
                Reply::Printed(Err(error @ PrinterError::AbortFailed { .. })) => {
                    Some((ToastKind::Error, error.to_string()))
                }
                #[cfg(windows)]
                Reply::Printed(Err(error))
                    if job.admission.admitted()
                        && !matches!(&error, PrinterError::Cancelled | PrinterError::Stale) =>
                {
                    Some((ToastKind::Error, error.to_string()))
                }
                _ => None,
            };
        }
        let cancelled = job.admission.cancelled();
        match reply {
            Reply::Reviewed(Ok(review)) if !cancelled => {
                let replace = review.kind() == PdfTargetKind::Replace;
                if replace {
                    self.feedback = None;
                    self.confirmation = Some(Confirmation {
                        owner: job.owner,
                        review: Review::Pdf(review),
                    });
                    self.confirmation_needs_focus = true;
                } else {
                    self.write(job.owner, review, ctx, viewport);
                }
            }
            Reply::Published(Ok(outcome)) => {
                let (kind, detail) = saved_notice(job.pdf_path(), outcome);
                self.feedback = Some((matches!(kind, ToastKind::Warning), detail));
            }
            Reply::Published(Err(WriteError::Publication(PdfPublishError::Cancelled)))
            | Reply::Published(Err(WriteError::Encoding(
                crate::notes_markdown::print_layout::pdf::PdfError::Cancelled,
            ))) => self.cancel(),
            Reply::Published(Err(WriteError::Publication(error))) => {
                self.report_error(error.to_string())
            }
            Reply::Published(Err(WriteError::Encoding(error))) => {
                self.report_error(error.to_string())
            }
            Reply::Reviewed(Err(error)) if !cancelled => self.report_error(error),
            Reply::Reviewed(_) => self.cancel(),
            #[cfg(windows)]
            Reply::Printed(Ok(submission)) => {
                self.feedback = Some((false, submitted_message(submission)))
            }
            #[cfg(windows)]
            Reply::Printed(Err(PrinterError::Cancelled)) => {
                self.feedback = Some((false, "Print cancelled.".into()));
            }
            #[cfg(windows)]
            Reply::Printed(Err(error)) => self.report_error(error.to_string()),
            #[cfg(windows)]
            Reply::PrinterReviewed(_) => {}
        }
        ctx.request_repaint_of(viewport);
        None
    }

    fn write(
        &mut self,
        owner: Owner,
        review: PdfDestinationReview,
        ctx: &egui::Context,
        viewport: egui::ViewportId,
    ) {
        let path = review.path();
        let permit = review.authorize();
        self.start(
            owner,
            JobTarget::Pdf(path),
            ctx,
            viewport,
            move |owner, admission, _| {
                let cancelled = || admission.cancelled();
                let result = encode_pdf(&owner.scene.scene, cancelled)
                    .map_err(WriteError::Encoding)
                    .and_then(|bytes| {
                        publish_pdf_with_admission(
                            permit,
                            &bytes,
                            cancelled,
                            || !cancelled(),
                            || admission.admit(),
                        )
                        .map_err(WriteError::Publication)
                    });
                Reply::Published(result)
            },
        );
    }

    fn start(
        &mut self,
        owner: Owner,
        target: JobTarget,
        ctx: &egui::Context,
        viewport: egui::ViewportId,
        run: impl FnOnce(&Owner, &Admission, &WorkerMail) -> Reply + Send + 'static,
    ) {
        if self.busy() {
            return;
        }
        let worker_owner = owner.clone();
        let admission = Arc::new(Admission::default());
        let worker_admission = Arc::clone(&admission);
        let (send, reply) = mpsc::sync_channel(1);
        let mail = WorkerMail {
            send,
            ctx: ctx.clone(),
            viewport,
        };
        let progress = match &target {
            JobTarget::Pdf(_) => "Preparing PDF output…",
            #[cfg(windows)]
            JobTarget::Printer { .. } => "Preparing printer review…",
        };
        match std::thread::Builder::new()
            .name("markdown-output".into())
            .spawn(move || {
                if let Ok(result) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run(&worker_owner, &worker_admission, &mail)
                })) {
                    let _ = mail.send.send(result);
                }
                let WorkerMail {
                    send,
                    ctx,
                    viewport,
                } = mail;
                drop(send);
                ctx.request_repaint_of(viewport);
                ctx.request_repaint_of(egui::ViewportId::ROOT);
            }) {
            Ok(_) => {
                self.feedback = Some((false, progress.into()));
                self.job = Some(Job {
                    owner,
                    target,
                    admission,
                    reply,
                });
            }
            Err(error) => self.report_error(format!("Could not start Markdown output. {error}")),
        }
    }
}

#[cfg(windows)]
fn submitted_message(submission: PrintSubmission) -> String {
    format!("Submitted print job {} to Windows.", submission.job_id)
}

fn saved_notice(path: &Path, outcome: PdfPublishOutcome) -> (egui_toast::ToastKind, String) {
    use egui_toast::ToastKind;
    let path = path.display();
    match outcome.backup_retained {
        Some(backup) => (
            ToastKind::Warning,
            format!(
                "Saved PDF to {path}. The previous file remains at {}.",
                backup.display()
            ),
        ),
        None => (ToastKind::Success, format!("Saved PDF to {path}.")),
    }
}

impl Drop for MarkdownOutput {
    fn drop(&mut self) {
        self.retire();
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::gui::markdown_print::AssetBase;
    use crate::gui::utility_host::UtilityPresentation;
    use crate::notes_markdown::print_layout::{PrintSettings, paginate};
    use std::time::{Duration, Instant};

    fn scene() -> (HostIdentity, Arc<SceneReceipt>) {
        let host = HostIdentity {
            host: UtilityPresentation::Docked,
            generation: 1,
        };
        let settings = PrintSettings::default();
        let source: Arc<str> = Arc::from("# Draft PDF fixture");
        let scene = Arc::new(paginate(&source, &settings, "", || false).unwrap());
        (
            host,
            Arc::new(SceneReceipt {
                stamp: DocumentStamp::Studio {
                    epoch: 1,
                    host,
                    path: None,
                    revision: Some(1),
                },
                settings,
                date: String::new(),
                source,
                base: AssetBase::Studio(None),
                scene,
            }),
        )
    }

    fn finish(
        output: &mut MarkdownOutput,
        host: HostIdentity,
        scene: &Arc<SceneReceipt>,
        ctx: &egui::Context,
    ) {
        let deadline = Instant::now() + Duration::from_secs(8);
        while output.busy() {
            assert!(
                output
                    .poll(host, Some(scene), ctx, egui::ViewportId::ROOT)
                    .is_none()
            );
            assert!(Instant::now() < deadline, "PDF output did not finish");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn markdown_print_pdf_writes_new_and_waits_for_exact_replacement() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("draft.pdf");
        let (host, scene) = scene();
        let ctx = egui::Context::default();
        let mut output = MarkdownOutput::default();
        output.review(
            path.clone(),
            host,
            scene.clone(),
            &ctx,
            egui::ViewportId::ROOT,
        );
        finish(&mut output, host, &scene, &ctx);
        assert!(std::fs::read(&path).unwrap().starts_with(b"%PDF-"));
        assert!(
            output
                .feedback()
                .is_some_and(|(error, text)| !error && text.starts_with("Saved PDF"))
        );

        std::fs::write(&path, b"reviewed original").unwrap();
        output.review(
            path.clone(),
            host,
            scene.clone(),
            &ctx,
            egui::ViewportId::ROOT,
        );
        finish(&mut output, host, &scene, &ctx);
        assert_eq!(output.target_path(), Some(path.clone()));
        assert_eq!(std::fs::read(&path).unwrap(), b"reviewed original");
        output.cancel();
        assert!(!output.confirming());
        assert_eq!(std::fs::read(&path).unwrap(), b"reviewed original");

        output.review(
            path.clone(),
            host,
            scene.clone(),
            &ctx,
            egui::ViewportId::ROOT,
        );
        finish(&mut output, host, &scene, &ctx);
        output.confirm(host, &scene, &ctx, egui::ViewportId::ROOT);
        finish(&mut output, host, &scene, &ctx);
        assert!(std::fs::read(path).unwrap().starts_with(b"%PDF-"));
    }

    #[test]
    fn markdown_print_pdf_admission_is_single_and_revocable_until_accepted() {
        let revoked = Admission::default();
        revoked.revoke();
        assert!(!revoked.admit());
        assert!(revoked.cancelled());

        let admitted = Admission::default();
        assert!(admitted.admit());
        admitted.revoke();
        assert!(!admitted.cancelled());
        assert!(!admitted.admit());
    }

    fn preview_test_printer(
        output: &mut MarkdownOutput,
        host: HostIdentity,
        scene: &Arc<SceneReceipt>,
        ctx: &egui::Context,
    ) -> Arc<std::sync::atomic::AtomicUsize> {
        use crate::notes_markdown::print_layout::printer::DeviceMetrics;
        let submitted = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let worker_submitted = Arc::clone(&submitted);
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancelled);
        let (approve, approval) = mpsc::channel();
        output.start(
            Owner {
                host,
                scene: Arc::clone(scene),
            },
            JobTarget::Printer {
                cancelled,
                approval: Some(approve),
            },
            ctx,
            egui::ViewportId::ROOT,
            move |_, admission, mail| {
                let summary = PrinterReview {
                    device_label: Some("Synthetic review device".into()),
                    mode: PrintScale::Fit,
                    layout: PrintReview {
                        device: DeviceMetrics {
                            dpi_x: 300,
                            dpi_y: 300,
                            physical_width_px: 2550,
                            physical_height_px: 3300,
                            printable_width_px: 2400,
                            printable_height_px: 3150,
                            offset_x_px: 75,
                            offset_y_px: 75,
                        },
                        pages: Vec::new(),
                    },
                };
                if mail.send(Reply::PrinterReviewed(summary))
                    && approval.recv().is_ok()
                    && !worker_cancel.load(Ordering::Acquire)
                    && admission.admit()
                {
                    worker_submitted.fetch_add(1, Ordering::AcqRel);
                    Reply::Printed(Ok(PrintSubmission { job_id: 8 }))
                } else {
                    Reply::Printed(Err(PrinterError::Cancelled))
                }
            },
        );
        let deadline = Instant::now() + Duration::from_secs(8);
        while output.printer_review().is_none() {
            assert!(
                output
                    .poll(host, Some(scene), ctx, egui::ViewportId::ROOT)
                    .is_none()
            );
            assert!(Instant::now() < deadline, "printer review did not arrive");
            std::thread::sleep(Duration::from_millis(5));
        }
        submitted
    }

    #[test]
    fn markdown_print_printer_reserves_output_until_exact_approval_or_retirement() {
        let (host, scene) = scene();
        let ctx = egui::Context::default();
        let viewport = egui::ViewportId::ROOT;
        let mut output = MarkdownOutput::default();
        let submitted = preview_test_printer(&mut output, host, &scene, &ctx);
        assert!(output.confirmation_needs_focus() && output.busy());
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("unexpected.pdf");
        output.review(path.clone(), host, Arc::clone(&scene), &ctx, viewport);
        assert!(output.printer_review().is_some());
        let stale_host = HostIdentity {
            generation: host.generation + 1,
            ..host
        };
        output.observe(stale_host, Some(&scene));
        assert!(!output.confirming());
        finish(&mut output, stale_host, &scene, &ctx);
        assert_eq!(submitted.load(Ordering::Acquire), 0);
        assert!(!path.exists());

        let submitted = preview_test_printer(&mut output, host, &scene, &ctx);
        assert_eq!(submitted.load(Ordering::Acquire), 0);
        output.confirm(host, &scene, &ctx, viewport);
        output.confirm(host, &scene, &ctx, viewport);
        finish(&mut output, host, &scene, &ctx);
        assert_eq!(submitted.load(Ordering::Acquire), 1);
        assert_eq!(
            output.feedback(),
            Some((false, "Submitted print job 8 to Windows."))
        );
    }

    #[test]
    fn markdown_print_pdf_stale_job_does_not_approve_replacement() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("draft.pdf");
        std::fs::write(&path, b"reviewed original").unwrap();
        let (host, scene) = scene();
        let ctx = egui::Context::default();
        let mut output = MarkdownOutput::default();
        output.review(
            path.clone(),
            host,
            scene.clone(),
            &ctx,
            egui::ViewportId::ROOT,
        );
        finish(&mut output, host, &scene, &ctx);
        assert!(output.confirming());
        output.observe(
            HostIdentity {
                generation: 2,
                ..host
            },
            Some(&scene),
        );
        assert!(!output.confirming());
        assert_eq!(std::fs::read(path).unwrap(), b"reviewed original");
        assert!(choose_pdf_destination(None).is_err());
    }
}

pub(in crate::gui) fn choose_pdf_destination(
    hwnd: Option<usize>,
) -> Result<Option<PathBuf>, &'static str> {
    #[cfg(windows)]
    {
        let hwnd = hwnd
            .and_then(|handle| std::num::NonZeroIsize::new(handle as isize))
            .ok_or(
                "The current preview window is not ready to own Save As. Focus it and try again.",
            )?;
        Ok(rfd::FileDialog::new()
            .set_parent(&crate::gui::add_action_dialog::NativeDialogParent(hwnd))
            .add_filter("PDF (*.pdf)", &["pdf"])
            .set_file_name("document.pdf")
            .save_file()
            .map(|path| crate::gui::markdown_export::ensure_extension(path, "pdf")))
    }
    #[cfg(not(windows))]
    {
        let _ = hwnd;
        Err("Guarded PDF Save As is unavailable on this platform.")
    }
}
