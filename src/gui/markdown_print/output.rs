//! Explicit PDF output over an accepted preview scene.

use super::{DocumentStamp, SceneReceipt};
use crate::gui::utility_host::HostIdentity;
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
    review: PdfDestinationReview,
}

enum Reply {
    Reviewed(Result<PdfDestinationReview, String>),
    Published(Result<PdfPublishOutcome, WriteError>),
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

    fn admit(&self) -> bool {
        self.decide(Self::ADMITTED)
    }
}

struct Job {
    owner: Owner,
    path: PathBuf,
    admission: Arc<Admission>,
    reply: Receiver<Reply>,
}

#[derive(Default)]
pub(in crate::gui) struct PdfOutput {
    job: Option<Job>,
    confirmation: Option<Confirmation>,
    confirmation_needs_focus: bool,
    feedback: Option<(bool, String)>,
}

impl PdfOutput {
    pub(in crate::gui) fn busy(&self) -> bool {
        self.job.is_some()
    }

    pub(super) fn can_cancel_job(&self) -> bool {
        self.job
            .as_ref()
            .is_some_and(|job| job.admission.0.load(Ordering::Acquire) == Admission::PENDING)
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
        self.confirmation
            .as_ref()
            .map(|pending| pending.review.path())
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
        if let Some(job) = &self.job {
            job.admission.revoke();
        }
        self.confirmation = None;
        self.confirmation_needs_focus = false;
        self.feedback = None;
    }

    pub(in crate::gui) fn cancel(&mut self) {
        self.retire();
        let text = match &self.job {
            Some(job) if job.admission.cancelled() => "Cancelling PDF save…",
            Some(_) => "Finishing PDF save…",
            None => "PDF save cancelled.",
        };
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
        if let Some(job) = &self.job
            && !job.owner.is_current(host, scene)
        {
            job.admission.revoke();
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
            path,
            ctx,
            viewport,
            |owner, path, admission| {
                let result = review_pdf_destination_with_cancel(path, owner.active_path(), || {
                    admission.cancelled()
                })
                .map_err(|error| error.to_string());
                Reply::Reviewed(result)
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
        if let Some(confirmation) = self.confirmation.take()
            && confirmation.owner.is_current(host, Some(scene))
        {
            self.write(confirmation, ctx, viewport);
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

        let job = self.job.as_ref()?;
        let reply = match job.reply.try_recv() {
            Ok(reply) => reply,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => {
                self.job = None;
                return Some((
                    ToastKind::Error,
                    "The PDF output worker ended before reporting a result.".into(),
                ));
            }
        };
        let job = self.job.take()?;
        if !job.owner.is_current(host, scene) {
            return match reply {
                Reply::Published(Ok(outcome)) => Some(saved_notice(&job.path, outcome)),
                Reply::Published(Err(WriteError::Publication(
                    error @ (PdfPublishError::Recovery { .. } | PdfPublishError::Io(_)),
                ))) => Some((ToastKind::Error, error.to_string())),
                _ => None,
            };
        }
        let cancelled = job.admission.cancelled();
        match reply {
            Reply::Reviewed(Ok(review)) if !cancelled => {
                let replace = review.kind() == PdfTargetKind::Replace;
                let confirmation = Confirmation {
                    owner: job.owner,
                    review,
                };
                if replace {
                    self.feedback = None;
                    self.confirmation = Some(confirmation);
                    self.confirmation_needs_focus = true;
                } else {
                    self.write(confirmation, ctx, viewport);
                }
            }
            Reply::Published(Ok(outcome)) => {
                let (kind, detail) = saved_notice(&job.path, outcome);
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
        }
        ctx.request_repaint_of(viewport);
        None
    }

    fn write(
        &mut self,
        confirmation: Confirmation,
        ctx: &egui::Context,
        viewport: egui::ViewportId,
    ) {
        let path = confirmation.review.path();
        let permit = confirmation.review.authorize();
        self.start(
            confirmation.owner,
            path,
            ctx,
            viewport,
            move |owner, _, admission| {
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
        path: PathBuf,
        ctx: &egui::Context,
        viewport: egui::ViewportId,
        run: impl FnOnce(&Owner, &Path, &Admission) -> Reply + Send + 'static,
    ) {
        if self.busy() {
            return;
        }
        let worker_owner = Owner {
            host: owner.host,
            scene: owner.scene.clone(),
        };
        let worker_path = path.clone();
        let admission = Arc::new(Admission::default());
        let worker_admission = Arc::clone(&admission);
        let ctx = ctx.clone();
        let (send, reply) = mpsc::sync_channel(1);
        match std::thread::Builder::new()
            .name("markdown-pdf-output".into())
            .spawn(move || {
                if let Ok(result) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run(&worker_owner, &worker_path, &worker_admission)
                })) {
                    let _ = send.send(result);
                }
                drop(send);
                ctx.request_repaint_of(viewport);
                ctx.request_repaint_of(egui::ViewportId::ROOT);
            }) {
            Ok(_) => {
                self.feedback = Some((false, "Preparing PDF output…".into()));
                self.job = Some(Job {
                    owner,
                    path,
                    admission,
                    reply,
                });
            }
            Err(error) => self.report_error(format!("Could not start PDF output. {error}")),
        }
    }
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

impl Drop for PdfOutput {
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
        output: &mut PdfOutput,
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
        let mut output = PdfOutput::default();
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

    #[test]
    fn markdown_print_pdf_stale_job_does_not_approve_replacement() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("draft.pdf");
        std::fs::write(&path, b"reviewed original").unwrap();
        let (host, scene) = scene();
        let ctx = egui::Context::default();
        let mut output = PdfOutput::default();
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
