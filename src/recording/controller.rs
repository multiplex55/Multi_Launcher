#[cfg(windows)]
use super::Mp4Recording;
use super::RecordingSessionError;
#[cfg(windows)]
use super::mp4_windows::Mp4RecordingError;
use super::output::RecordingOutputError;
use super::{
    GifRecording, RecorderState, RecordingBackend, RecordingFormat, RecordingRequest,
    RecordingSession, SystemRecorderClock,
};
use crate::activity::{
    ActivityCenter, ActivityDiagnostic, ActivityIncarnation, ActivityOperation, ActivityOutcome,
    ActivityOwner, ActivityProgress, ActivityProgressUnit, ActivityRetirement, ActivitySlot,
    ActivityTask,
};
use crate::thread_reaper::{self, CompletionNotifier, ReapPermit};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const RECORDING_COUNTDOWN: Duration = Duration::from_secs(3);
const WRITER_IDLE_WAIT: Duration = Duration::from_secs(1);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecordingControllerOutcome {
    Saved(PathBuf),
    Discarded,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecordingWriterPreparation {
    Pending,
    Ready,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordingControllerSnapshot {
    pub session: super::RecordingSessionView,
    pub active_generation: Option<u64>,
    pub preparation: Option<RecordingWriterPreparation>,
    pub writer_outcome: Option<RecordingControllerOutcome>,
    pub writer_retired: bool,
    pub result: Option<RecordingControllerOutcome>,
}

/// Launcher-owned orchestration for one bounded capture and its one encoder
/// thread. The capture source remains owned by `RecordingSession`; encoding,
/// finalization and Media Foundation stay on the writer thread.
pub struct RecordingController {
    session: Arc<RecordingSession>,
    wake: Arc<WriterWake>,
    notify_ui: Arc<dyn Fn() + Send + Sync + 'static>,
    writer: Option<WriterOwner>,
    result: Option<RecordingControllerOutcome>,
    shutdown_requested: bool,
}

struct WriterOwner {
    generation: u64,
    status: Arc<Mutex<WriterStatus>>,
    settled: Arc<AtomicBool>,
    reaped: Arc<AtomicBool>,
    reap_permit: Option<ReapPermit>,
    activity: ActivityTask,
    countdown_started: bool,
    last_progress_frames: u64,
    user_discarded: AtomicBool,
}

#[derive(Default)]
struct WriterStatus {
    preparation: Option<RecordingWriterPreparation>,
    outcome: Option<RecordingControllerOutcome>,
    retirement_unavailable: bool,
}

impl WriterOwner {
    fn retired(&self) -> bool {
        self.reaped.load(Ordering::Acquire)
            || (self
                .status
                .lock()
                .is_ok_and(|status| status.retirement_unavailable)
                && self.settled.load(Ordering::Acquire))
    }
}

struct WriterRetirementTicket {
    status: Arc<Mutex<WriterStatus>>,
    reaped: Arc<AtomicBool>,
    notify_ui: Arc<dyn Fn() + Send + Sync + 'static>,
    resolved: bool,
}

impl WriterRetirementTicket {
    fn after_join(mut self, panicked: bool) {
        if let Ok(mut status) = self.status.lock()
            && panicked
        {
            status.outcome.get_or_insert_with(|| {
                RecordingControllerOutcome::Failed(
                    "recording writer panicked while retiring".into(),
                )
            });
        }
        self.reaped.store(true, Ordering::Release);
        self.resolved = true;
        (self.notify_ui)();
    }
}

impl Drop for WriterRetirementTicket {
    fn drop(&mut self) {
        if self.resolved {
            return;
        }
        if let Ok(mut status) = self.status.lock() {
            status.retirement_unavailable = true;
        }
        (self.notify_ui)();
    }
}

#[derive(Default)]
struct WriterWake {
    epoch: Mutex<u64>,
    changed: Condvar,
}

impl WriterWake {
    fn signal(&self) {
        if let Ok(mut epoch) = self.epoch.lock() {
            *epoch = epoch.wrapping_add(1);
            self.changed.notify_one();
        }
    }

    fn current(&self) -> u64 {
        self.epoch.lock().map_or(0, |epoch| *epoch)
    }

    fn wait_for_change(&self, observed: u64) -> u64 {
        let Ok(epoch) = self.epoch.lock() else {
            return observed;
        };
        if *epoch != observed {
            return *epoch;
        }
        self.changed
            .wait_timeout(epoch, WRITER_IDLE_WAIT)
            .map_or(observed, |(epoch, _)| *epoch)
    }
}

impl RecordingController {
    pub fn new(
        backend: Arc<dyn RecordingBackend>,
        notify_ui: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        let wake = Arc::new(WriterWake::default());
        let signal = Arc::clone(&wake);
        let notify_ui: Arc<dyn Fn() + Send + Sync> = Arc::new(notify_ui);
        let session_notify = Arc::clone(&notify_ui);
        let session = Arc::new(RecordingSession::new(
            Arc::new(SystemRecorderClock::default()),
            backend,
            move || {
                signal.signal();
                session_notify();
            },
        ));
        Self {
            session,
            wake,
            notify_ui,
            writer: None,
            result: None,
            shutdown_requested: false,
        }
    }

    pub fn snapshot(&self) -> RecordingControllerSnapshot {
        let session = self.session.view();
        let Some(writer) = self.writer.as_ref() else {
            return RecordingControllerSnapshot {
                session,
                active_generation: None,
                preparation: None,
                writer_outcome: None,
                writer_retired: true,
                result: self.result.clone(),
            };
        };
        let (preparation, writer_outcome) = writer.status.lock().map_or((None, None), |status| {
            (status.preparation.clone(), status.outcome.clone())
        });
        let writer_retired = writer.retired();
        RecordingControllerSnapshot {
            session,
            active_generation: Some(writer.generation),
            preparation,
            writer_outcome,
            writer_retired,
            result: self.result.clone(),
        }
    }

    pub fn can_start(&self) -> bool {
        self.writer.is_none() && !self.session.view().worker_busy() && !self.shutdown_requested
    }

    fn cancel_for_unavailable_writer_retirement(
        &self,
        generation: u64,
        session: &super::RecordingSessionView,
    ) {
        if session.generation() != generation || session.finalization_claimed() {
            return;
        }
        match session.state() {
            RecorderState::Selecting | RecorderState::Countdown { .. } => {
                let _ = self.session.cancel();
            }
            RecorderState::Recording { .. } | RecorderState::Finalizing { .. } => {
                let _ = self
                    .session
                    .fail_sink(generation, "recording writer retirement is unavailable");
            }
            _ => {}
        }
        self.wake.signal();
    }

    pub fn conflicts_with_capture(&self) -> bool {
        let Some(writer) = self.writer.as_ref() else {
            return false;
        };
        let view = self.session.view();
        if view.worker_busy() {
            return true;
        }
        if view.generation() != writer.generation {
            return false;
        }
        match view.state() {
            RecorderState::Selecting
            | RecorderState::Countdown { .. }
            | RecorderState::Recording { .. } => true,
            RecorderState::Finalizing { .. } => false,
            RecorderState::Idle
            | RecorderState::Completed(_)
            | RecorderState::Failed(_)
            | RecorderState::Cancelled => false,
        }
    }

    pub fn has_active_job(&self) -> bool {
        self.writer.is_some()
    }

    pub fn prepare(
        &mut self,
        request: RecordingRequest,
        destination: PathBuf,
        activity_center: &ActivityCenter,
    ) -> Result<u64, String> {
        self.poll();
        if !self.can_start() {
            return Err("The previous recording is still being retired".into());
        }
        let permit = thread_reaper::reserve()
            .map_err(|error| format!("could not reserve the bounded recording writer: {error}"))?;
        let generation = self
            .session
            .select(request)
            .map_err(|error| error.to_string())?;
        let Some(activity) = activity_center.start(
            ActivityOwner::Recording,
            ActivitySlot::Primary,
            ActivityIncarnation::new(generation),
            ActivityOperation::RecordScreenClip,
        ) else {
            let _ = self.session.cancel();
            return Err("could not allocate a recording activity identity".into());
        };
        let status = Arc::new(Mutex::new(WriterStatus {
            preparation: Some(RecordingWriterPreparation::Pending),
            ..WriterStatus::default()
        }));
        let settled = Arc::new(AtomicBool::new(false));
        let reaped = Arc::new(AtomicBool::new(false));
        let completion = permit.completion_notifier();
        let join = match spawn_writer(
            Arc::clone(&self.session),
            generation,
            destination.clone(),
            Arc::clone(&status),
            Arc::clone(&settled),
            Arc::clone(&self.wake),
            Arc::clone(&self.notify_ui),
            completion,
        ) {
            Ok(join) => join,
            Err(error) => {
                let error = error.to_string();
                let _ = self.session.cancel();
                activity.finish(
                    ActivityOutcome::Failed,
                    Some(ActivityDiagnostic::WorkerUnavailable),
                    ActivityRetirement::Confirmed,
                );
                self.result = Some(RecordingControllerOutcome::Failed(error.clone()));
                return Err(error);
            }
        };
        self.writer = Some(WriterOwner {
            generation,
            status: Arc::clone(&status),
            settled,
            reaped: Arc::clone(&reaped),
            reap_permit: Some(permit),
            activity,
            countdown_started: false,
            last_progress_frames: 0,
            user_discarded: AtomicBool::new(false),
        });
        self.result = None;
        let transfer = {
            let writer = self.writer.as_mut().expect("writer was just installed");
            let permit = writer
                .reap_permit
                .take()
                .expect("a new writer owns one reaper permit");
            let ticket = WriterRetirementTicket {
                status: Arc::clone(&writer.status),
                reaped: Arc::clone(&reaped),
                notify_ui: Arc::clone(&self.notify_ui),
                resolved: false,
            };
            permit.reap_with_callback(join, move |panicked| ticket.after_join(panicked))
        };
        if let Err(error) = transfer {
            if let Some(writer) = self.writer.as_ref()
                && let Ok(mut status) = writer.status.lock()
            {
                status.retirement_unavailable = true;
            }
            let _ = self.session.cancel();
            self.wake.signal();
            self.notify();
            return Err(format!(
                "could not safely retire the recording writer: {error}"
            ));
        }
        Ok(generation)
    }

    /// Called by the single launcher runtime owner. It transfers completed
    /// capture handles, starts the countdown only after encoder setup succeeds,
    /// and retires the activity row after both worker owners are finished.
    pub fn poll(&mut self) {
        let mut session = self.session.view();
        if let Some(writer) = self.writer.as_ref()
            && writer
                .status
                .lock()
                .is_ok_and(|status| status.retirement_unavailable)
        {
            self.cancel_for_unavailable_writer_retirement(writer.generation, &session);
        }
        let capture_was_busy = session.worker_busy();
        if let Err(error) = self.session.poll()
            && let Some(writer) = self.writer.as_ref()
            && let Ok(mut status) = writer.status.lock()
        {
            let message = match self.session.view().state() {
                RecorderState::Failed(failure) => failure.to_string(),
                _ => error.to_string(),
            };
            status
                .outcome
                .get_or_insert_with(|| RecordingControllerOutcome::Failed(message));
        }
        session = self.session.view();
        if capture_was_busy && !session.worker_busy() {
            // This UI poll has completed capture-worker retirement. The writer
            // needs a second wake because its last worker callback happened
            // before the session owner consumed that receipt.
            self.wake.signal();
        }

        let Some((writer_generation, retirement_unavailable)) =
            self.writer.as_ref().map(|writer| {
                (
                    writer.generation,
                    writer
                        .status
                        .lock()
                        .is_ok_and(|status| status.retirement_unavailable),
                )
            })
        else {
            return;
        };
        if retirement_unavailable {
            self.cancel_for_unavailable_writer_retirement(writer_generation, &session);
            session = self.session.view();
        }
        let Some(writer) = self.writer.as_mut() else {
            return;
        };
        if !retirement_unavailable
            && session.generation() == writer.generation
            && matches!(session.state(), RecorderState::Selecting)
            && !writer.countdown_started
        {
            let ready = writer.status.lock().is_ok_and(|status| {
                matches!(status.preparation, Some(RecordingWriterPreparation::Ready))
            });
            if ready {
                writer.activity.mark_running();
                match self.session.begin_countdown(RECORDING_COUNTDOWN) {
                    Ok(()) => writer.countdown_started = true,
                    Err(error) => {
                        let message = format!("could not begin the recorder countdown: {error}");
                        let _ = self.session.cancel();
                        if let Ok(mut status) = writer.status.lock() {
                            status
                                .outcome
                                .get_or_insert(RecordingControllerOutcome::Failed(message));
                        }
                        self.wake.signal();
                    }
                }
            }
        }

        if session.generation() == writer.generation {
            let frames = session.frames_written();
            if frames != writer.last_progress_frames {
                writer.last_progress_frames = frames;
                writer.activity.set_progress(Some(ActivityProgress::new(
                    frames,
                    None,
                    ActivityProgressUnit::Items,
                )));
            }
        }

        if session.worker_busy() || !writer.retired() {
            return;
        }
        let Some(mut writer) = self.writer.take() else {
            return;
        };
        let status = writer.status.lock().ok();
        let mut outcome = status
            .as_ref()
            .and_then(|status| status.outcome.clone())
            .unwrap_or_else(|| match session.state() {
                RecorderState::Completed(_) => RecordingControllerOutcome::Failed(
                    "recording completed without a published output receipt".into(),
                ),
                RecorderState::Failed(failure) => {
                    RecordingControllerOutcome::Failed(failure.to_string())
                }
                RecorderState::Cancelled => RecordingControllerOutcome::Discarded,
                _ => RecordingControllerOutcome::Failed(
                    "recording writer retired before finalization".into(),
                ),
            });
        let retirement_unavailable = status
            .as_ref()
            .is_some_and(|status| status.retirement_unavailable);
        if retirement_unavailable
            && !writer.user_discarded.load(Ordering::Acquire)
            && matches!(outcome, RecordingControllerOutcome::Discarded)
        {
            outcome = RecordingControllerOutcome::Failed(
                "recording was cancelled because its writer could not be safely retired".into(),
            );
        }
        let retirement = if retirement_unavailable {
            ActivityRetirement::Unavailable
        } else {
            ActivityRetirement::Confirmed
        };
        let (activity_outcome, diagnostic) = match &outcome {
            RecordingControllerOutcome::Saved(_) => (ActivityOutcome::Completed, None),
            RecordingControllerOutcome::Discarded => (ActivityOutcome::Cancelled, None),
            RecordingControllerOutcome::Failed(_) => (
                ActivityOutcome::Failed,
                Some(ActivityDiagnostic::WorkerFailed),
            ),
        };
        writer.activity.clear_cancel_handler();
        writer
            .activity
            .finish(activity_outcome, diagnostic, retirement);
        self.result = Some(outcome);
        self.notify();
    }

    pub fn stop(&self) -> Result<bool, String> {
        if self.session.view().finalization_claimed() {
            return Ok(false);
        }
        let stopped = self.session.stop().map_err(|error| error.to_string())?;
        if stopped {
            self.wake.signal();
        }
        Ok(stopped)
    }

    pub fn stop_generation(&self, generation: u64) -> Result<bool, String> {
        if self
            .writer
            .as_ref()
            .is_none_or(|writer| writer.generation != generation)
        {
            return Err("the recording controls belong to an older generation".into());
        }
        self.stop()
    }

    pub fn discard(&self, generation: u64) -> Result<bool, String> {
        let Some(writer) = self
            .writer
            .as_ref()
            .filter(|writer| writer.generation == generation)
        else {
            return Err("the recording controls belong to an older generation".into());
        };
        writer.user_discarded.store(true, Ordering::Release);
        let cancelled = match self.session.cancel() {
            Ok(cancelled) => cancelled,
            Err(error) => {
                writer.user_discarded.store(false, Ordering::Release);
                return Err(error.to_string());
            }
        };
        if cancelled {
            self.wake.signal();
        } else {
            writer.user_discarded.store(false, Ordering::Release);
        }
        Ok(cancelled)
    }

    pub fn request_shutdown(&mut self) {
        if self.shutdown_requested {
            return;
        }
        self.shutdown_requested = true;
        if !self.session.view().finalization_claimed() {
            let _ = self.session.cancel();
        }
        self.wake.signal();
        self.notify();
    }

    pub fn shutdown_ready(&self) -> bool {
        self.writer.is_none() && !self.session.view().worker_busy()
    }

    fn notify(&self) {
        (self.notify_ui)();
    }
}

impl Drop for RecordingController {
    fn drop(&mut self) {
        self.request_shutdown();
    }
}

struct WriterSettled {
    settled: Arc<AtomicBool>,
    wake: Arc<WriterWake>,
    notify_ui: Arc<dyn Fn() + Send + Sync + 'static>,
}

impl WriterSettled {
    fn new(
        settled: Arc<AtomicBool>,
        wake: Arc<WriterWake>,
        notify_ui: Arc<dyn Fn() + Send + Sync + 'static>,
    ) -> Self {
        Self {
            settled,
            wake,
            notify_ui,
        }
    }
}

impl Drop for WriterSettled {
    fn drop(&mut self) {
        self.settled.store(true, Ordering::Release);
        self.wake.signal();
        (self.notify_ui)();
    }
}

enum RecordingOutput {
    Gif(GifRecording),
    #[cfg(windows)]
    Mp4(Mp4Recording),
}

struct WriterError {
    message: String,
    cleanup_failed: bool,
    cancellation_transition: bool,
    session_transition_race: bool,
}

fn is_cancellation_transition(error: &RecordingSessionError) -> bool {
    matches!(
        error,
        RecordingSessionError::Closed | RecordingSessionError::StaleGeneration
    )
}

fn is_session_transition_race(error: &RecordingSessionError) -> bool {
    matches!(
        error,
        RecordingSessionError::InvalidTransition | RecordingSessionError::NotFinalizing
    )
}

impl From<RecordingOutputError> for WriterError {
    fn from(error: RecordingOutputError) -> Self {
        let cleanup_failed = matches!(error, RecordingOutputError::WithCleanup { .. });
        let cancellation_transition = match &error {
            RecordingOutputError::Session(error) => is_cancellation_transition(error),
            _ => false,
        };
        let session_transition_race = match &error {
            RecordingOutputError::Session(error) => is_session_transition_race(error),
            _ => false,
        };
        Self {
            message: error.to_string(),
            cleanup_failed,
            cancellation_transition,
            session_transition_race,
        }
    }
}

#[cfg(windows)]
impl From<Mp4RecordingError> for WriterError {
    fn from(error: Mp4RecordingError) -> Self {
        let cleanup_failed = matches!(error, Mp4RecordingError::WithCleanup { .. });
        let cancellation_transition = match &error {
            Mp4RecordingError::Session(error) => is_cancellation_transition(error),
            Mp4RecordingError::Publication(error) => match error {
                RecordingOutputError::Session(error) => is_cancellation_transition(error),
                _ => false,
            },
            _ => false,
        };
        let session_transition_race = match &error {
            Mp4RecordingError::Session(error) => is_session_transition_race(error),
            Mp4RecordingError::Publication(RecordingOutputError::Session(error)) => {
                is_session_transition_race(error)
            }
            _ => false,
        };
        Self {
            message: error.to_string(),
            cleanup_failed,
            cancellation_transition,
            session_transition_race,
        }
    }
}

impl RecordingOutput {
    fn create(
        session: Arc<RecordingSession>,
        generation: u64,
        destination: &std::path::Path,
    ) -> Result<Self, WriterError> {
        let request = session
            .view()
            .request()
            .cloned()
            .ok_or_else(|| WriterError {
                message: "recording request was retired before writer setup".into(),
                cleanup_failed: false,
                cancellation_transition: true,
                session_transition_race: false,
            })?;
        match request.options().format() {
            RecordingFormat::Gif => GifRecording::create(session, generation, destination)
                .map(Self::Gif)
                .map_err(WriterError::from),
            RecordingFormat::Mp4 => {
                #[cfg(windows)]
                {
                    Mp4Recording::create(session, generation, destination)
                        .map(Self::Mp4)
                        .map_err(WriterError::from)
                }
                #[cfg(not(windows))]
                {
                    let _ = (session, generation, destination);
                    Err(WriterError {
                        message: "MP4 recording is available only on Windows".into(),
                        cleanup_failed: false,
                        cancellation_transition: false,
                        session_transition_race: false,
                    })
                }
            }
        }
    }

    fn consume_latest(&mut self) -> Result<bool, WriterError> {
        match self {
            Self::Gif(writer) => writer
                .consume_latest()
                .map(|elapsed| elapsed.is_some())
                .map_err(WriterError::from),
            #[cfg(windows)]
            Self::Mp4(writer) => writer
                .consume_latest()
                .map(|elapsed| elapsed.is_some())
                .map_err(WriterError::from),
        }
    }

    fn finish_and_publish(self) -> Result<PathBuf, WriterError> {
        match self {
            Self::Gif(writer) => writer.finish_and_publish().map_err(WriterError::from),
            #[cfg(windows)]
            Self::Mp4(writer) => writer.finish_and_publish().map_err(WriterError::from),
        }
    }

    fn discard(self) -> Result<(), WriterError> {
        match self {
            Self::Gif(writer) => writer.discard().map_err(WriterError::from),
            #[cfg(windows)]
            Self::Mp4(writer) => writer.discard().map_err(WriterError::from),
        }
    }
}

fn spawn_writer(
    session: Arc<RecordingSession>,
    generation: u64,
    destination: PathBuf,
    status: Arc<Mutex<WriterStatus>>,
    settled: Arc<AtomicBool>,
    wake: Arc<WriterWake>,
    notify_ui: Arc<dyn Fn() + Send + Sync + 'static>,
    completion: CompletionNotifier,
) -> std::io::Result<JoinHandle<()>> {
    thread::Builder::new()
        .name("launcher-recording-writer".into())
        .spawn(move || {
            let _completion = completion;
            let _settled = WriterSettled::new(settled, Arc::clone(&wake), Arc::clone(&notify_ui));
            let panic_session = Arc::clone(&session);
            let result = catch_unwind(AssertUnwindSafe(|| {
                run_writer(
                    session,
                    generation,
                    destination,
                    Arc::clone(&status),
                    Arc::clone(&wake),
                    Arc::clone(&notify_ui),
                )
            }));
            if result.is_err() {
                let message = "recording writer panicked during capture or finalization";
                if let Ok(mut status) = status.lock() {
                    status
                        .outcome
                        .get_or_insert_with(|| RecordingControllerOutcome::Failed(message.into()));
                }
                let view = panic_session.view();
                if view.generation() == generation && !view.finalization_claimed() {
                    if matches!(
                        view.state(),
                        RecorderState::Recording { .. } | RecorderState::Finalizing { .. }
                    ) {
                        let _ = panic_session.fail_sink(generation, message);
                    } else if matches!(
                        view.state(),
                        RecorderState::Selecting | RecorderState::Countdown { .. }
                    ) {
                        let _ = panic_session.cancel();
                    }
                }
                wake.signal();
                notify_ui();
            }
        })
}

fn run_writer(
    session: Arc<RecordingSession>,
    generation: u64,
    destination: PathBuf,
    status: Arc<Mutex<WriterStatus>>,
    wake: Arc<WriterWake>,
    notify_ui: Arc<dyn Fn() + Send + Sync + 'static>,
) {
    let mut output = match RecordingOutput::create(Arc::clone(&session), generation, &destination) {
        Ok(output) => output,
        Err(error) => {
            let view = session.view();
            if view.generation() != generation || matches!(view.state(), RecorderState::Cancelled) {
                if let Ok(mut status) = status.lock() {
                    status.preparation = Some(RecordingWriterPreparation::Failed(
                        "writer setup was cancelled".into(),
                    ));
                    status.outcome.get_or_insert_with(|| {
                        if error.cancellation_transition && !error.cleanup_failed {
                            RecordingControllerOutcome::Discarded
                        } else {
                            RecordingControllerOutcome::Failed(error.message)
                        }
                    });
                }
                wake.signal();
                notify_ui();
                return;
            }
            if let Ok(mut status) = status.lock() {
                status.preparation =
                    Some(RecordingWriterPreparation::Failed(error.message.clone()));
                status
                    .outcome
                    .get_or_insert(RecordingControllerOutcome::Failed(error.message));
            }
            let _ = session.cancel();
            wake.signal();
            notify_ui();
            return;
        }
    };
    if let Ok(mut status) = status.lock() {
        status.preparation = Some(RecordingWriterPreparation::Ready);
    }
    wake.signal();
    notify_ui();

    loop {
        let observed = wake.current();
        let view = session.view();
        if view.generation() != generation {
            finish_discarded_output(output, &status, &wake, &notify_ui);
            return;
        }
        match view.state() {
            RecorderState::Selecting | RecorderState::Countdown { .. } => {}
            RecorderState::Recording { .. } | RecorderState::Finalizing { .. } => {
                match output.consume_latest() {
                    Ok(true) => continue,
                    Ok(false) => {}
                    Err(error) => {
                        finish_failed_output(
                            output, error, &session, generation, &status, &wake, &notify_ui,
                        );
                        return;
                    }
                }
                let latest = session.view();
                if matches!(latest.state(), RecorderState::Finalizing { .. })
                    && !latest.worker_busy()
                {
                    loop {
                        match output.consume_latest() {
                            Ok(true) => continue,
                            Ok(false) => break,
                            Err(error) => {
                                finish_failed_output(
                                    output, error, &session, generation, &status, &wake, &notify_ui,
                                );
                                return;
                            }
                        }
                    }
                    let outcome = match output.finish_and_publish() {
                        Ok(path) => RecordingControllerOutcome::Saved(path),
                        Err(error) => {
                            let latest = session.view();
                            if (latest.generation() != generation
                                || matches!(latest.state(), RecorderState::Cancelled))
                                && error.cancellation_transition
                                && !error.cleanup_failed
                            {
                                RecordingControllerOutcome::Discarded
                            } else {
                                RecordingControllerOutcome::Failed(error.message)
                            }
                        }
                    };
                    if let Ok(mut status) = status.lock() {
                        status.outcome.get_or_insert(outcome);
                    }
                    wake.signal();
                    notify_ui();
                    return;
                }
            }
            RecorderState::Cancelled => {
                finish_discarded_output(output, &status, &wake, &notify_ui);
                return;
            }
            RecorderState::Failed(failure) => {
                let failure = failure.to_string();
                let discard = output.discard();
                if let Ok(mut status) = status.lock() {
                    status.outcome.get_or_insert(match discard {
                        Ok(()) => RecordingControllerOutcome::Failed(failure),
                        Err(cleanup) => RecordingControllerOutcome::Failed(format!(
                            "{failure}; output cleanup failed: {}",
                            cleanup.message
                        )),
                    });
                }
                wake.signal();
                notify_ui();
                return;
            }
            RecorderState::Completed(_) => {
                if let Ok(mut status) = status.lock() {
                    status.outcome.get_or_insert_with(|| {
                        RecordingControllerOutcome::Failed(
                            "session completed before the writer published its output".into(),
                        )
                    });
                }
                let _ = output.discard();
                wake.signal();
                notify_ui();
                return;
            }
            RecorderState::Idle => {
                finish_discarded_output(output, &status, &wake, &notify_ui);
                return;
            }
        }
        wake.wait_for_change(observed);
    }
}

fn finish_discarded_output(
    output: RecordingOutput,
    status: &Mutex<WriterStatus>,
    wake: &WriterWake,
    notify_ui: &Arc<dyn Fn() + Send + Sync + 'static>,
) {
    let result = output.discard();
    if let Ok(mut status) = status.lock() {
        status.outcome.get_or_insert(match result {
            Ok(()) => RecordingControllerOutcome::Discarded,
            Err(error) => RecordingControllerOutcome::Failed(format!(
                "could not discard the unclaimed recording output: {}",
                error.message
            )),
        });
    }
    wake.signal();
    notify_ui();
}

fn finish_failed_output(
    output: RecordingOutput,
    error: WriterError,
    session: &RecordingSession,
    generation: u64,
    status: &Mutex<WriterStatus>,
    wake: &WriterWake,
    notify_ui: &Arc<dyn Fn() + Send + Sync + 'static>,
) {
    let view = session.view();
    let source_failure = if view.generation() == generation {
        match view.state() {
            RecorderState::Failed(failure) => Some(failure.to_string()),
            _ => None,
        }
    } else {
        None
    };
    let cancelled = (view.generation() != generation
        || matches!(view.state(), RecorderState::Cancelled))
        && error.cancellation_transition
        && source_failure.is_none();
    let cleanup = output.discard();
    let outcome = if cancelled && !error.cleanup_failed {
        match cleanup {
            Ok(()) => RecordingControllerOutcome::Discarded,
            Err(cleanup) => RecordingControllerOutcome::Failed(format!(
                "recording was cancelled but its output could not be removed: {}",
                cleanup.message
            )),
        }
    } else {
        let mut message = if error.session_transition_race {
            source_failure.unwrap_or(error.message)
        } else {
            error.message
        };
        if let Err(cleanup) = cleanup {
            message.push_str("; output cleanup failed: ");
            message.push_str(&cleanup.message);
        }
        RecordingControllerOutcome::Failed(message)
    };
    if let Ok(mut status) = status.lock() {
        status.outcome.get_or_insert(outcome);
    }
    wake.signal();
    notify_ui();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::ScreenRect;
    use crate::recording::{
        CapturePoll, CaptureSource, CaptureStopToken, CaptureTarget, FrameRate, RecordingEndReason,
        RecordingOptions, RecordingRequest, RgbaFrame,
    };
    use std::sync::atomic::AtomicUsize;
    use std::sync::mpsc::{self, Receiver, Sender};
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    struct RetirementGatedBackend {
        drop_gate: Mutex<Option<(Sender<()>, Receiver<()>)>>,
        source_count: AtomicUsize,
    }

    struct RetirementGatedSource {
        generation: u64,
        canvas: super::super::CanvasSize,
        sent_frame: bool,
        dropped: Option<(Sender<()>, Receiver<()>)>,
    }

    struct EndWithoutFrameBackend;

    struct EndWithoutFrameSource;

    impl RecordingBackend for EndWithoutFrameBackend {
        fn create_source(
            &self,
            _request: &RecordingRequest,
            _generation: u64,
        ) -> Result<Box<dyn CaptureSource>, String> {
            Ok(Box::new(EndWithoutFrameSource))
        }
    }

    impl CaptureSource for EndWithoutFrameSource {
        fn poll(&mut self, _max_wait: Duration, _stop: &CaptureStopToken) -> CapturePoll {
            CapturePoll::Ended(RecordingEndReason::TargetLost)
        }
    }

    impl RecordingBackend for RetirementGatedBackend {
        fn create_source(
            &self,
            request: &RecordingRequest,
            generation: u64,
        ) -> Result<Box<dyn CaptureSource>, String> {
            self.source_count.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(RetirementGatedSource {
                generation,
                canvas: request.canvas(),
                sent_frame: false,
                dropped: self.drop_gate.lock().unwrap().take(),
            }))
        }
    }

    impl CaptureSource for RetirementGatedSource {
        fn poll(&mut self, max_wait: Duration, stop: &CaptureStopToken) -> CapturePoll {
            if !self.sent_frame {
                self.sent_frame = true;
                return CapturePoll::Frame {
                    generation: self.generation,
                    frame: RgbaFrame::new(self.canvas, vec![255; self.canvas.rgba_bytes()])
                        .unwrap(),
                };
            }
            if stop.stop_requested() {
                return CapturePoll::Ended(RecordingEndReason::UserStopped);
            }
            thread::sleep(max_wait.min(Duration::from_millis(5)));
            CapturePoll::Pending
        }
    }

    impl Drop for RetirementGatedSource {
        fn drop(&mut self) {
            if let Some((dropped, release)) = self.dropped.take() {
                let _ = dropped.send(());
                let _ = release.recv_timeout(Duration::from_secs(8));
            }
        }
    }

    fn request() -> RecordingRequest {
        let bounds = ScreenRect::new(-32, 18, 32, 24);
        let target = CaptureTarget::window(100, 200, 300, bounds).unwrap();
        let options = RecordingOptions::new(
            RecordingFormat::Gif,
            FrameRate::new(12).unwrap(),
            Duration::from_secs(120),
            false,
        )
        .unwrap();
        RecordingRequest::new(target, bounds, options).unwrap()
    }

    fn wait_until(
        controller: &mut RecordingController,
        timeout: Duration,
        predicate: impl Fn(&RecordingControllerSnapshot) -> bool,
    ) -> RecordingControllerSnapshot {
        let deadline = Instant::now() + timeout;
        loop {
            let snapshot = controller.snapshot();
            if predicate(&snapshot) {
                return snapshot;
            }
            controller.poll();
            assert!(
                Instant::now() < deadline,
                "controller did not settle: {snapshot:?}"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn recording_controller_prepares_writer_before_countdown_and_publishes_only_after_capture_retirement()
     {
        let directory = tempfile::tempdir().unwrap();
        let (dropped_tx, dropped_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let backend = Arc::new(RetirementGatedBackend {
            drop_gate: Mutex::new(Some((dropped_tx, release_rx))),
            source_count: AtomicUsize::new(0),
        });
        let mut controller = RecordingController::new(backend.clone(), || {});
        let activity = ActivityCenter::new();
        let destination = directory.path().join("retired-after-capture.gif");
        let generation = controller
            .prepare(request(), destination.clone(), &activity)
            .unwrap();

        let ready_deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let snapshot = controller.snapshot();
            if matches!(
                snapshot.preparation,
                Some(RecordingWriterPreparation::Ready)
            ) {
                assert!(matches!(snapshot.session.state(), RecorderState::Selecting));
                assert_eq!(backend.source_count.load(Ordering::SeqCst), 0);
                controller.poll();
                assert!(matches!(
                    controller.snapshot().session.state(),
                    RecorderState::Countdown { .. }
                ));
                break;
            }
            assert!(Instant::now() < ready_deadline);
            thread::sleep(Duration::from_millis(5));
        }

        let recording = wait_until(&mut controller, Duration::from_secs(9), |snapshot| {
            matches!(snapshot.session.state(), RecorderState::Recording { .. })
                && snapshot.session.frames_written() > 0
        });
        assert_eq!(recording.active_generation, Some(generation));
        assert_eq!(backend.source_count.load(Ordering::SeqCst), 1);
        assert!(
            controller
                .stop_generation(generation.wrapping_add(1))
                .is_err()
        );
        assert!(controller.discard(generation.wrapping_add(1)).is_err());
        assert!(matches!(
            controller.snapshot().session.state(),
            RecorderState::Recording { .. }
        ));
        assert!(controller.stop_generation(generation).unwrap());
        dropped_rx
            .recv_timeout(Duration::from_secs(4))
            .expect("capture source reached its retirement gate");
        controller.poll();
        let gated = controller.snapshot();
        assert!(gated.session.worker_busy());
        assert!(gated.writer_outcome.is_none());
        assert!(!destination.exists());

        release_tx.send(()).unwrap();
        let saved = wait_until(&mut controller, Duration::from_secs(6), |snapshot| {
            matches!(
                snapshot.result.as_ref(),
                Some(RecordingControllerOutcome::Saved(_))
            )
        });
        assert_eq!(
            saved.result,
            Some(RecordingControllerOutcome::Saved(destination.clone()))
        );
        assert!(destination.exists());
        assert_eq!(backend.source_count.load(Ordering::SeqCst), 1);

        let discarded_path = directory.path().join("exact-discard.gif");
        let next_generation = controller
            .prepare(request(), discarded_path.clone(), &activity)
            .unwrap();
        assert_ne!(next_generation, generation);
        assert!(controller.stop_generation(generation).is_err());
        assert!(controller.discard(generation).is_err());
        assert!(controller.discard(next_generation).unwrap());
        wait_until(&mut controller, Duration::from_secs(5), |snapshot| {
            snapshot.active_generation.is_none()
                && snapshot.result == Some(RecordingControllerOutcome::Discarded)
        });
        assert!(!discarded_path.exists());
        assert_eq!(backend.source_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn finish_failed_output_preserves_concrete_cleanup_and_prefers_matching_session_failure() {
        let directory = tempfile::tempdir().unwrap();
        let session = Arc::new(RecordingSession::new(
            Arc::new(SystemRecorderClock::default()),
            Arc::new(EndWithoutFrameBackend),
            || {},
        ));
        let generation = session.select(request()).unwrap();
        let destination = directory.path().join("diagnostic.gif");
        let concrete_output =
            RecordingOutput::create(Arc::clone(&session), generation, &destination).unwrap_or_else(
                |error| panic!("could not create test GIF output: {}", error.message),
            );
        session.start().unwrap();

        let source_failure_deadline = Instant::now() + Duration::from_secs(4);
        let source_failure = loop {
            let view = session.view();
            if let RecorderState::Failed(failure) = view.state() {
                break failure.to_string();
            }
            session.poll().unwrap();
            assert!(
                Instant::now() < source_failure_deadline,
                "capture source did not produce its expected empty-clip failure"
            );
            thread::sleep(Duration::from_millis(5));
        };

        let wake = WriterWake::default();
        let notify_ui: Arc<dyn Fn() + Send + Sync + 'static> = Arc::new(|| {});
        let surviving_path = directory.path().join("owned-partial.gif");
        let concrete_error = RecordingOutputError::WithCleanup {
            cause: "could not publish staged GIF".into(),
            cleanup: format!(
                "owned partial file remains at {}: access denied",
                surviving_path.display()
            ),
        };
        let concrete_message = concrete_error.to_string();
        let concrete_status = Mutex::new(WriterStatus::default());
        finish_failed_output(
            concrete_output,
            WriterError::from(concrete_error),
            &session,
            generation,
            &concrete_status,
            &wake,
            &notify_ui,
        );
        assert_eq!(
            concrete_status.lock().unwrap().outcome,
            Some(RecordingControllerOutcome::Failed(concrete_message))
        );

        let transition_output =
            RecordingOutput::create(Arc::clone(&session), generation, &destination).unwrap_or_else(
                |error| panic!("could not create test GIF output: {}", error.message),
            );
        let transition_status = Mutex::new(WriterStatus::default());
        finish_failed_output(
            transition_output,
            WriterError::from(RecordingOutputError::Session(
                RecordingSessionError::NotFinalizing,
            )),
            &session,
            generation,
            &transition_status,
            &wake,
            &notify_ui,
        );
        assert_eq!(
            transition_status.lock().unwrap().outcome,
            Some(RecordingControllerOutcome::Failed(source_failure))
        );
    }
}
