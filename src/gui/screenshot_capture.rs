use crate::commands::{
    CommandInvocation, ScreenshotDelay, ScreenshotDestination, ScreenshotMarkup, ScreenshotMode,
};
use crate::thread_reaper::{self, ReapPermit};
use crate::universal_actions::RootLauncherPolicy;
use eframe::egui;
use image::RgbaImage;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

type CaptureFn = dyn Fn(ScreenshotMode) -> Result<Option<RgbaImage>, String> + Send + Sync;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ScreenshotRequestId(u64);

impl ScreenshotRequestId {
    pub(crate) fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScreenshotCapturePhase {
    Waiting,
    AwaitingPresentation,
    Capturing,
    Draining,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ScreenshotCaptureSnapshot {
    pub request_id: ScreenshotRequestId,
    pub deadline: Instant,
    pub phase: ScreenshotCapturePhase,
    pub mode: ScreenshotMode,
    pub destination: ScreenshotDestination,
    pub markup: ScreenshotMarkup,
}

struct ScreenshotRequest {
    id: ScreenshotRequestId,
    deadline: Instant,
    mode: ScreenshotMode,
    destination: ScreenshotDestination,
    markup: ScreenshotMarkup,
    invocation: CommandInvocation,
    history_query: String,
    root_policy: RootLauncherPolicy,
}

type WorkerResult = Result<Option<RgbaImage>, String>;

struct CaptureWorker {
    cancelled: Arc<AtomicBool>,
    result_rx: Receiver<WorkerResult>,
    result: Option<WorkerResult>,
    join: JoinHandle<()>,
    reap_permit: ReapPermit,
}

pub(crate) struct ScreenshotCaptureCompletion {
    pub request_id: ScreenshotRequestId,
    pub mode: ScreenshotMode,
    pub destination: ScreenshotDestination,
    pub markup: ScreenshotMarkup,
    pub image: Option<RgbaImage>,
    pub error: Option<String>,
    pub invocation: CommandInvocation,
    pub history_query: String,
    pub root_policy: RootLauncherPolicy,
}

/// One delayed request and, while it drains, its one owned capture worker.
/// Expiry only enters `AwaitingPresentation`; a current generation must also
/// receive an explicit visibility grant before a foreground target is read.
pub(crate) struct ScreenshotCaptureOwner {
    generation: u64,
    request: Option<ScreenshotRequest>,
    phase: Option<ScreenshotCapturePhase>,
    worker: Option<CaptureWorker>,
    shutdown: bool,
    capture: Arc<CaptureFn>,
    #[cfg(test)]
    after_result_poll: Option<Box<dyn FnOnce(&JoinHandle<()>)>>,
}

impl Default for ScreenshotCaptureOwner {
    fn default() -> Self {
        Self::new()
    }
}

impl ScreenshotCaptureOwner {
    pub(crate) fn new() -> Self {
        Self::with_capture(Arc::new(|mode| {
            let mode = match mode {
                ScreenshotMode::Window => crate::actions::screenshot::Mode::Window,
                ScreenshotMode::Region => crate::actions::screenshot::Mode::Region,
                ScreenshotMode::Desktop => crate::actions::screenshot::Mode::Desktop,
            };
            match crate::actions::screenshot::capture_raw(mode) {
                Ok(image) => Ok(Some(image)),
                Err(error) if crate::actions::screenshot::is_cancel_or_timeout(&error) => Ok(None),
                Err(error) => Err(error.to_string()),
            }
        }))
    }

    fn with_capture(capture: Arc<CaptureFn>) -> Self {
        Self {
            generation: 0,
            request: None,
            phase: None,
            worker: None,
            shutdown: false,
            capture,
            #[cfg(test)]
            after_result_poll: None,
        }
    }

    pub(crate) fn schedule(
        &mut self,
        delay: ScreenshotDelay,
        mode: ScreenshotMode,
        destination: ScreenshotDestination,
        markup: ScreenshotMarkup,
        invocation: &CommandInvocation,
        history_query: &str,
        root_policy: RootLauncherPolicy,
        now: Instant,
        ctx: &egui::Context,
    ) -> Result<ScreenshotRequestId, String> {
        if self.shutdown {
            return Err("Screenshot capture is shutting down".into());
        }
        if self.request.is_some() || self.worker.is_some() {
            return Err("A screenshot capture is already pending".into());
        }
        self.generation = self.generation.wrapping_add(1).max(1);
        let deadline = now + delay.duration();
        let id = ScreenshotRequestId(self.generation);
        self.request = Some(ScreenshotRequest {
            id,
            deadline,
            mode,
            destination,
            markup,
            invocation: invocation.clone(),
            history_query: history_query.to_owned(),
            root_policy,
        });
        self.phase = Some(ScreenshotCapturePhase::Waiting);
        ctx.request_repaint_after(delay.duration());
        Ok(id)
    }

    /// Advances the deadline and polls the owned worker without blocking.
    pub(crate) fn tick(
        &mut self,
        now: Instant,
        ctx: &egui::Context,
    ) -> Option<ScreenshotCaptureCompletion> {
        if let Some(request) = self.request.as_ref()
            && self.phase == Some(ScreenshotCapturePhase::Waiting)
        {
            if now >= request.deadline {
                self.phase = Some(ScreenshotCapturePhase::AwaitingPresentation);
                ctx.request_repaint_of(egui::ViewportId::ROOT);
            } else {
                ctx.request_repaint_after(request.deadline.saturating_duration_since(now));
            }
        }

        let worker = self.worker.as_mut()?;
        if worker.result.is_none() {
            match worker.result_rx.try_recv() {
                Ok(result) => worker.result = Some(result),
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    worker.result =
                        Some(Err("Screenshot capture worker stopped unexpectedly".into()));
                }
            }
        }
        #[cfg(test)]
        if worker.result.is_none()
            && let Some(hook) = self.after_result_poll.take()
        {
            hook(&worker.join);
        }
        if !worker.join.is_finished() {
            if worker.result.is_some() {
                ctx.request_repaint_after(Duration::from_millis(8));
            }
            return None;
        }

        let CaptureWorker {
            cancelled,
            result_rx,
            result: observed_result,
            join,
            reap_permit,
        } = self.worker.take().expect("worker checked above");
        let join_result = join.join();
        // A send can race the first nonblocking poll: the worker may finish
        // after returning Empty but before is_finished becomes true.
        let result = observed_result.or_else(|| result_rx.try_recv().ok());
        drop(reap_permit);
        let request = self.request.take()?;
        let result = match join_result {
            Err(_) => Err("Screenshot capture worker panicked".into()),
            Ok(()) => {
                result.unwrap_or_else(|| Err("Screenshot capture worker returned no result".into()))
            }
        };
        let was_current = self.phase == Some(ScreenshotCapturePhase::Capturing)
            && self.generation == request.id.0
            && !cancelled.load(Ordering::Acquire)
            && !self.shutdown;
        self.phase = None;
        if !was_current {
            return None;
        }
        Some(ScreenshotCaptureCompletion {
            request_id: request.id,
            mode: request.mode,
            destination: request.destination,
            markup: request.markup,
            image: result.as_ref().ok().and_then(|image| image.clone()),
            error: result.err(),
            invocation: request.invocation,
            history_query: request.history_query,
            root_policy: request.root_policy,
        })
    }

    pub(crate) fn grant_presentation(
        &mut self,
        request_id: ScreenshotRequestId,
        now: Instant,
        ctx: &egui::Context,
    ) -> Result<(), String> {
        if self.shutdown {
            return Err("Screenshot capture is shutting down".into());
        }
        let Some(request) = self.request.as_ref() else {
            return Err("Screenshot capture request is no longer current".into());
        };
        if request.id != request_id
            || self.phase != Some(ScreenshotCapturePhase::AwaitingPresentation)
            || now < request.deadline
        {
            return Err("Screenshot capture presentation grant is stale".into());
        }
        if self.worker.is_some() {
            return Err("Screenshot capture worker is still draining".into());
        }

        let mode = request.mode;
        let capture = Arc::clone(&self.capture);
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        let reap_permit = thread_reaper::reserve().map_err(str::to_owned)?;
        let completion_notifier = reap_permit.completion_notifier();
        let wake = ctx.clone();
        let (result_tx, result_rx) = mpsc::sync_channel(1);
        let join = thread::Builder::new()
            .name("screenshot-delayed-capture".into())
            .spawn(move || {
                // Keep this last-owned guard until the result and wake signal
                // have both been published, so a reaper can join this worker.
                let completion_notifier = completion_notifier;
                let result = if worker_cancelled.load(Ordering::Acquire) {
                    Ok(None)
                } else {
                    let result = capture(mode);
                    if worker_cancelled.load(Ordering::Acquire) {
                        Ok(None)
                    } else {
                        result
                    }
                };
                let _ = result_tx.try_send(result);
                wake.request_repaint_of(egui::ViewportId::ROOT);
                drop(completion_notifier);
            })
            .map_err(|error| {
                self.request = None;
                self.phase = None;
                error.to_string()
            })?;
        self.worker = Some(CaptureWorker {
            cancelled,
            result_rx,
            result: None,
            join,
            reap_permit,
        });
        self.phase = Some(ScreenshotCapturePhase::Capturing);
        Ok(())
    }

    /// Invalidates a request immediately. An active platform capture cannot
    /// always be interrupted, so its owned slot remains in Draining until the
    /// worker exits and any late image is discarded.
    pub(crate) fn cancel(&mut self) -> bool {
        let had_request = self.request.is_some();
        if !had_request {
            return false;
        }
        self.generation = self.generation.wrapping_add(1).max(1);
        if let Some(worker) = self.worker.as_ref() {
            worker.cancelled.store(true, Ordering::Release);
            self.phase = Some(ScreenshotCapturePhase::Draining);
        } else {
            self.request = None;
            self.phase = None;
        }
        true
    }

    pub(crate) fn shutdown(&mut self) {
        self.shutdown = true;
        let _ = self.cancel();
        self.request = None;
        self.phase = None;
        if let Some(worker) = self.worker.take() {
            worker.cancelled.store(true, Ordering::Release);
            let CaptureWorker {
                result_rx,
                join,
                reap_permit,
                ..
            } = worker;
            drop(result_rx);
            if let Err(error) = reap_permit.reap(join) {
                tracing::error!(%error, "failed to transfer screenshot capture worker to join reaper");
            }
        }
    }

    pub(crate) fn snapshot(&self) -> Option<ScreenshotCaptureSnapshot> {
        let request = self.request.as_ref()?;
        Some(ScreenshotCaptureSnapshot {
            request_id: request.id,
            deadline: request.deadline,
            phase: self.phase?,
            mode: request.mode,
            destination: request.destination,
            markup: request.markup,
        })
    }

    pub(crate) fn is_busy(&self) -> bool {
        self.request.is_some() || self.worker.is_some()
    }

    #[cfg(test)]
    pub(crate) fn set_capture_for_test(
        &mut self,
        capture: impl Fn(ScreenshotMode) -> Result<Option<RgbaImage>, String> + Send + Sync + 'static,
    ) {
        self.capture = Arc::new(capture);
    }

    #[cfg(test)]
    pub(crate) fn set_after_result_poll_for_test(
        &mut self,
        hook: impl FnOnce(&JoinHandle<()>) + 'static,
    ) {
        self.after_result_poll = Some(Box::new(hook));
    }

    #[cfg(test)]
    pub(crate) fn reaper_probe_for_test(&self) -> Option<thread_reaper::ReapCompletionProbe> {
        self.worker
            .as_ref()
            .map(|worker| worker.reap_permit.reaped_probe())
    }
}

impl Drop for ScreenshotCaptureOwner {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::Action;
    use crate::commands::{ActivationSource, Command, ScreenshotCommand};
    use std::sync::mpsc;
    use std::time::Duration;

    fn invocation() -> CommandInvocation {
        CommandInvocation {
            command: Command::Screenshot(ScreenshotCommand::Schedule {
                delay: ScreenshotDelay::ThreeSeconds,
                mode: ScreenshotMode::Window,
                destination: ScreenshotDestination::Editor,
                markup: ScreenshotMarkup::Rectangle,
            }),
            original_action: Action {
                label: "Delayed window capture".into(),
                desc: "preserved invocation".into(),
                action: "screenshot:delay:3:window".into(),
                args: Some("original args".into()),
            },
            query_override: Some("original query".into()),
            source: ActivationSource::Enter,
        }
    }

    fn options() -> (ScreenshotMode, ScreenshotDestination, ScreenshotMarkup) {
        (
            ScreenshotMode::Window,
            ScreenshotDestination::Editor,
            ScreenshotMarkup::Rectangle,
        )
    }

    #[test]
    fn screenshot_delay_waits_for_expiry_and_an_explicit_current_grant() {
        let ctx = egui::Context::default();
        let started = Instant::now();
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let observed_calls = Arc::clone(&calls);
        let mut owner = ScreenshotCaptureOwner::with_capture(Arc::new(move |_| {
            observed_calls.fetch_add(1, Ordering::AcqRel);
            Ok(Some(RgbaImage::from_pixel(
                1,
                1,
                image::Rgba([1, 2, 3, 255]),
            )))
        }));
        let (mode, destination, markup) = options();
        let id = owner
            .schedule(
                ScreenshotDelay::ThreeSeconds,
                mode,
                destination,
                markup,
                &invocation(),
                "captured history query",
                RootLauncherPolicy::PreserveOrdinaryState,
                started,
                &ctx,
            )
            .unwrap();

        assert_eq!(calls.load(Ordering::Acquire), 0);
        assert_eq!(
            owner.snapshot().unwrap().phase,
            ScreenshotCapturePhase::Waiting
        );
        assert!(owner.tick(started + Duration::from_secs(2), &ctx).is_none());
        assert_eq!(calls.load(Ordering::Acquire), 0);
        assert_eq!(
            owner.snapshot().unwrap().phase,
            ScreenshotCapturePhase::Waiting
        );

        assert!(owner.tick(started + Duration::from_secs(3), &ctx).is_none());
        assert_eq!(
            owner.snapshot().unwrap().phase,
            ScreenshotCapturePhase::AwaitingPresentation
        );
        assert_eq!(calls.load(Ordering::Acquire), 0);
        assert!(
            owner
                .grant_presentation(id, started + Duration::from_secs(3), &ctx)
                .is_ok()
        );
        assert_eq!(
            owner.snapshot().unwrap().phase,
            ScreenshotCapturePhase::Capturing
        );
        assert_eq!(
            calls.load(Ordering::Acquire),
            0,
            "capture runs on the worker"
        );
    }

    #[test]
    fn screenshot_delay_requires_deadline_and_rejects_stale_reentry() {
        let ctx = egui::Context::default();
        let started = Instant::now();
        let mut owner = ScreenshotCaptureOwner::with_capture(Arc::new(|_| Ok(None)));
        let (mode, destination, markup) = options();
        let id = owner
            .schedule(
                ScreenshotDelay::FiveSeconds,
                mode,
                destination,
                markup,
                &invocation(),
                "",
                RootLauncherPolicy::Legacy,
                started,
                &ctx,
            )
            .unwrap();
        assert!(
            owner
                .schedule(
                    ScreenshotDelay::ThreeSeconds,
                    mode,
                    destination,
                    markup,
                    &invocation(),
                    "",
                    RootLauncherPolicy::Legacy,
                    started,
                    &ctx,
                )
                .is_err()
        );
        assert!(
            owner
                .grant_presentation(id, started + Duration::from_secs(5), &ctx)
                .is_err()
        );
        owner.tick(started + Duration::from_secs(5), &ctx);
        let current = owner.snapshot().unwrap().request_id;
        assert_eq!(current, id);
        assert!(
            owner
                .grant_presentation(
                    ScreenshotRequestId(id.get().wrapping_add(1)),
                    started + Duration::from_secs(5),
                    &ctx
                )
                .is_err()
        );
        assert!(
            owner
                .grant_presentation(id, started + Duration::from_secs(5), &ctx)
                .is_ok()
        );
    }

    #[test]
    fn screenshot_delay_cancel_before_expiry_prevents_later_grants() {
        let ctx = egui::Context::default();
        let started = Instant::now();
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let observed_calls = Arc::clone(&calls);
        let mut owner = ScreenshotCaptureOwner::with_capture(Arc::new(move |_| {
            observed_calls.fetch_add(1, Ordering::AcqRel);
            Ok(None)
        }));
        let (mode, destination, markup) = options();
        let id = owner
            .schedule(
                ScreenshotDelay::TenSeconds,
                mode,
                destination,
                markup,
                &invocation(),
                "",
                RootLauncherPolicy::Legacy,
                started,
                &ctx,
            )
            .unwrap();
        assert!(owner.cancel());
        assert!(!owner.is_busy());
        assert!(
            owner
                .tick(started + Duration::from_secs(10), &ctx)
                .is_none()
        );
        assert!(
            owner
                .grant_presentation(id, started + Duration::from_secs(10), &ctx)
                .is_err()
        );
        assert_eq!(calls.load(Ordering::Acquire), 0);

        let deadline_owner_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let observed_calls = Arc::clone(&deadline_owner_calls);
        let mut at_deadline = ScreenshotCaptureOwner::with_capture(Arc::new(move |_| {
            observed_calls.fetch_add(1, Ordering::AcqRel);
            Ok(None)
        }));
        let at_deadline_id = at_deadline
            .schedule(
                ScreenshotDelay::ThreeSeconds,
                mode,
                destination,
                markup,
                &invocation(),
                "",
                RootLauncherPolicy::Legacy,
                started,
                &ctx,
            )
            .unwrap();
        at_deadline.tick(started + Duration::from_secs(3), &ctx);
        assert_eq!(
            at_deadline.snapshot().unwrap().phase,
            ScreenshotCapturePhase::AwaitingPresentation
        );
        assert!(at_deadline.cancel());
        assert!(
            at_deadline
                .grant_presentation(at_deadline_id, started + Duration::from_secs(3), &ctx)
                .is_err()
        );
        assert_eq!(deadline_owner_calls.load(Ordering::Acquire), 0);
    }

    #[test]
    fn screenshot_delay_shutdown_invalidates_pending_requests() {
        let ctx = egui::Context::default();
        let started = Instant::now();
        let mut owner = ScreenshotCaptureOwner::with_capture(Arc::new(|_| Ok(None)));
        let (mode, destination, markup) = options();
        let id = owner
            .schedule(
                ScreenshotDelay::TenSeconds,
                mode,
                destination,
                markup,
                &invocation(),
                "",
                RootLauncherPolicy::Legacy,
                started,
                &ctx,
            )
            .unwrap();
        owner.shutdown();
        assert!(!owner.is_busy());
        assert!(
            owner
                .grant_presentation(id, started + Duration::from_secs(10), &ctx)
                .is_err()
        );
        assert!(
            owner
                .schedule(
                    ScreenshotDelay::ThreeSeconds,
                    mode,
                    destination,
                    markup,
                    &invocation(),
                    "",
                    RootLauncherPolicy::Legacy,
                    started,
                    &ctx,
                )
                .is_err()
        );
    }

    #[test]
    fn screenshot_delay_active_cancel_drains_and_discards_late_image() {
        let ctx = egui::Context::default();
        let started = Instant::now();
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let release_rx = std::sync::Mutex::new(release_rx);
        let mut owner = ScreenshotCaptureOwner::with_capture(Arc::new(move |_| {
            started_tx.send(()).unwrap();
            release_rx.lock().unwrap().recv().unwrap();
            Ok(Some(RgbaImage::from_pixel(
                1,
                1,
                image::Rgba([4, 5, 6, 255]),
            )))
        }));
        let (mode, destination, markup) = options();
        let id = owner
            .schedule(
                ScreenshotDelay::ThreeSeconds,
                mode,
                destination,
                markup,
                &invocation(),
                "",
                RootLauncherPolicy::Legacy,
                started,
                &ctx,
            )
            .unwrap();
        owner.tick(started + Duration::from_secs(3), &ctx);
        owner
            .grant_presentation(id, started + Duration::from_secs(3), &ctx)
            .unwrap();
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(owner.cancel());
        assert_eq!(
            owner.snapshot().unwrap().phase,
            ScreenshotCapturePhase::Draining
        );
        assert!(owner.is_busy());
        release_tx.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while owner.is_busy() && Instant::now() < deadline {
            assert!(owner.tick(Instant::now(), &ctx).is_none());
            std::thread::yield_now();
        }
        assert!(!owner.is_busy(), "cancelled worker must drain");
        assert!(owner.tick(Instant::now(), &ctx).is_none());
    }

    #[test]
    fn screenshot_delay_completion_retains_context_and_publishes_once() {
        let ctx = egui::Context::default();
        let started = Instant::now();
        let target_revision = Arc::new(std::sync::atomic::AtomicUsize::new(1));
        let observed_revision = Arc::clone(&target_revision);
        let captured_revision = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let captured_revision_worker = Arc::clone(&captured_revision);
        let mut owner = ScreenshotCaptureOwner::with_capture(Arc::new(move |_| {
            captured_revision_worker
                .store(observed_revision.load(Ordering::Acquire), Ordering::Release);
            Ok(Some(RgbaImage::from_pixel(
                1,
                1,
                image::Rgba([7, 8, 9, 255]),
            )))
        }));
        let original = invocation();
        let id = owner
            .schedule(
                ScreenshotDelay::ThreeSeconds,
                ScreenshotMode::Window,
                ScreenshotDestination::Editor,
                ScreenshotMarkup::Rectangle,
                &original,
                "the scheduled history context",
                RootLauncherPolicy::PreserveOrdinaryState,
                started,
                &ctx,
            )
            .unwrap();
        owner.tick(started + Duration::from_secs(3), &ctx);
        target_revision.store(2, Ordering::Release);
        owner
            .grant_presentation(id, started + Duration::from_secs(3), &ctx)
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        let completion = loop {
            if let Some(completion) = owner.tick(Instant::now(), &ctx) {
                break completion;
            }
            assert!(Instant::now() < deadline, "capture worker did not complete");
            std::thread::yield_now();
        };
        assert_eq!(completion.request_id, id);
        assert_eq!(completion.invocation, original);
        assert_eq!(completion.history_query, "the scheduled history context");
        assert_eq!(
            completion.root_policy,
            RootLauncherPolicy::PreserveOrdinaryState
        );
        assert!(completion.image.is_some());
        assert_eq!(captured_revision.load(Ordering::Acquire), 2);

        let mut deliveries = 0;
        if let Some(image) = completion.image.as_ref() {
            assert_eq!(image.get_pixel(0, 0).0, [7, 8, 9, 255]);
            deliveries += 1;
        }
        assert_eq!(deliveries, 1);
        assert!(owner.tick(Instant::now(), &ctx).is_none());
        assert_eq!(deliveries, 1);
    }

    #[test]
    fn screenshot_delay_reads_result_sent_before_worker_exit() {
        let ctx = egui::Context::default();
        let started = Instant::now();
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let release_rx = std::sync::Mutex::new(release_rx);
        let mut owner = ScreenshotCaptureOwner::with_capture(Arc::new(move |_| {
            started_tx.send(()).unwrap();
            release_rx.lock().unwrap().recv().unwrap();
            Ok(Some(RgbaImage::from_pixel(
                1,
                1,
                image::Rgba([10, 11, 12, 255]),
            )))
        }));
        let (mode, destination, markup) = options();
        let id = owner
            .schedule(
                ScreenshotDelay::ThreeSeconds,
                mode,
                destination,
                markup,
                &invocation(),
                "race query",
                RootLauncherPolicy::Legacy,
                started,
                &ctx,
            )
            .unwrap();
        owner.tick(started + Duration::from_secs(3), &ctx);
        owner
            .grant_presentation(id, started + Duration::from_secs(3), &ctx)
            .unwrap();
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        owner.set_after_result_poll_for_test(move |join| {
            release_tx.send(()).unwrap();
            let deadline = Instant::now() + Duration::from_secs(2);
            while !join.is_finished() {
                assert!(
                    Instant::now() < deadline,
                    "capture worker did not finish after send"
                );
                thread::yield_now();
            }
        });

        // The first poll observes Empty. The hook releases the worker to send
        // and fully exit before the owner checks is_finished().
        let completion = owner
            .tick(Instant::now(), &ctx)
            .expect("result sent in the poll/finish gap must be drained after join");
        assert_eq!(completion.request_id, id);
        assert_eq!(
            completion.image.unwrap().get_pixel(0, 0).0,
            [10, 11, 12, 255]
        );
        assert_eq!(completion.error, None);
        assert!(!owner.is_busy());
    }

    #[test]
    fn screenshot_delay_shutdown_hands_blocked_worker_to_reaper_without_waiting() {
        let ctx = egui::Context::default();
        let started = Instant::now();
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let release_rx = std::sync::Mutex::new(release_rx);
        let worker_finished = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let finished_from_worker = Arc::clone(&worker_finished);
        let mut owner = ScreenshotCaptureOwner::with_capture(Arc::new(move |_| {
            started_tx.send(()).unwrap();
            release_rx.lock().unwrap().recv().unwrap();
            finished_from_worker.store(true, Ordering::Release);
            Ok(Some(RgbaImage::from_pixel(
                1,
                1,
                image::Rgba([13, 14, 15, 255]),
            )))
        }));
        let (mode, destination, markup) = options();
        let id = owner
            .schedule(
                ScreenshotDelay::ThreeSeconds,
                mode,
                destination,
                markup,
                &invocation(),
                "drop query",
                RootLauncherPolicy::Legacy,
                started,
                &ctx,
            )
            .unwrap();
        owner.tick(started + Duration::from_secs(3), &ctx);
        owner
            .grant_presentation(id, started + Duration::from_secs(3), &ctx)
            .unwrap();
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let reaped = owner.reaper_probe_for_test().unwrap();

        // Shutdown must invalidate and transfer the active handle to the
        // bounded reaper; it cannot synchronously wait on an OS operation.
        owner.shutdown();
        assert!(!owner.is_busy());
        assert!(!reaped.is_reaped());
        drop(owner);
        assert!(!worker_finished.load(Ordering::Acquire));
        release_tx.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while !reaped.is_reaped() && Instant::now() < deadline {
            thread::yield_now();
        }
        assert!(worker_finished.load(Ordering::Acquire));
        assert!(
            reaped.is_reaped(),
            "the bounded reaper joins the transferred worker"
        );
    }
}
