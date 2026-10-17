use super::{PlaybackMode, PreparedSequenceRun, SequenceTarget};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, TryRecvError},
};
use std::time::{Duration, Instant};

const CLIPBOARD_CONSUMPTION_SETTLE: Duration = Duration::from_millis(180);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SequencePlaybackError {
    Busy,
    InvalidState,
    TooSoon,
    ClipboardUnavailable,
    UnsupportedClipboard,
    InvalidTarget,
    FocusChanged,
    PasswordField,
    ClipboardChanged,
    ClipboardWriteFailed,
    ClipboardRestoreFailed,
    InputRejected,
    ActivationRejected,
}

impl SequencePlaybackError {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::Busy => "Another paste sequence is already active.",
            Self::InvalidState => "The paste sequence is not ready for that action.",
            Self::TooSoon => "Wait for the current clipboard step before advancing.",
            Self::ClipboardUnavailable => "The clipboard could not be captured or opened.",
            Self::UnsupportedClipboard => {
                "The current clipboard contains data that cannot be safely preserved."
            }
            Self::InvalidTarget => "The original destination window is no longer valid.",
            Self::FocusChanged => "The original destination no longer owns foreground focus.",
            Self::PasswordField => "Pasting into a detected password field is blocked.",
            Self::ClipboardChanged => {
                "The clipboard changed during playback; remaining steps were stopped."
            }
            Self::ClipboardWriteFailed => "Sequence text could not be published to the clipboard.",
            Self::ClipboardRestoreFailed => "The original clipboard could not be restored.",
            Self::InputRejected => "The paste keystroke was rejected by Windows.",
            Self::ActivationRejected => "The original destination window could not be activated.",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SequencePlaybackStatus {
    Idle,
    Preparing,
    Ready,
    Running,
    AwaitAdvance,
    Cancelled,
    Completed,
    Failed(SequencePlaybackError),
}

#[derive(Clone)]
pub(crate) struct ClipboardSnapshot {
    sequence_number: u32,
    text: Option<String>,
}

impl ClipboardSnapshot {
    pub(crate) fn new(sequence_number: u32, text: Option<String>) -> Self {
        Self {
            sequence_number,
            text,
        }
    }
}

impl std::fmt::Debug for ClipboardSnapshot {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ClipboardSnapshot")
            .field("sequence_available", &(self.sequence_number != 0))
            .field("text_available", &self.text.is_some())
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TargetObservation {
    Target { password_control: bool },
    Launcher,
    Other,
    Invalid,
    Unavailable,
}

pub(crate) struct ClipboardWriteFailure {
    pub(crate) error: SequencePlaybackError,
    /// The clipboard sequence number after this process changed it. Cleanup
    /// can still restore the captured value when an in-lock rollback failed.
    pub(crate) owned_sequence: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ClipboardRestoreOutcome {
    Restored,
    OwnershipLost,
    Failed,
}

pub(crate) enum ActivationPoll {
    Pending,
    Complete(Result<(), SequencePlaybackError>),
}

/// Owns a bounded activation worker until completion or shutdown. The worker
/// reports through a channel and only requests an egui repaint; it never
/// mutates application state.
pub(crate) struct ActivationJob {
    ready: Option<Result<(), SequencePlaybackError>>,
    receiver: Option<Receiver<Result<(), SequencePlaybackError>>>,
    worker: Option<std::thread::JoinHandle<()>>,
    cancelled: Arc<AtomicBool>,
}

impl ActivationJob {
    pub(crate) fn completed(result: Result<(), SequencePlaybackError>) -> Self {
        Self {
            ready: Some(result),
            receiver: None,
            worker: None,
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(crate) fn spawn(
        operation: impl FnOnce(Arc<AtomicBool>) -> Result<(), SequencePlaybackError> + Send + 'static,
        wake: impl FnOnce() + Send + 'static,
    ) -> Result<Self, SequencePlaybackError> {
        let (sender, receiver) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        let worker = std::thread::Builder::new()
            .name("paste-sequence-target-activation".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    operation(Arc::clone(&worker_cancelled))
                }))
                .unwrap_or(Err(SequencePlaybackError::ActivationRejected));
                let _ = sender.send(result);
                wake();
            })
            .map_err(|_| SequencePlaybackError::ActivationRejected)?;
        Ok(Self {
            ready: None,
            receiver: Some(receiver),
            worker: Some(worker),
            cancelled,
        })
    }

    pub(crate) fn poll(&mut self) -> ActivationPoll {
        if let Some(result) = self.ready.take() {
            return ActivationPoll::Complete(result);
        }
        let result = match self.receiver.as_ref().map(Receiver::try_recv) {
            Some(Ok(result)) => result,
            Some(Err(TryRecvError::Empty)) => return ActivationPoll::Pending,
            Some(Err(TryRecvError::Disconnected)) | None => {
                Err(SequencePlaybackError::ActivationRejected)
            }
        };
        self.receiver = None;
        let worker_ok = self
            .worker
            .take()
            .is_none_or(|worker| worker.join().is_ok());
        ActivationPoll::Complete(if worker_ok {
            result
        } else {
            Err(SequencePlaybackError::ActivationRejected)
        })
    }

    pub(crate) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    fn cancel_and_join(&mut self) {
        self.cancel();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        self.receiver = None;
        self.ready = None;
    }
}

/// Minimal platform boundary for the playback state machine. Tests supply a
/// deterministic fake; production only implements these narrow native calls.
pub(crate) trait SequencePlaybackPlatform {
    fn now(&mut self) -> Instant;
    fn capture_clipboard(&mut self) -> Result<ClipboardSnapshot, SequencePlaybackError>;
    fn observe_target(&mut self, target: &SequenceTarget) -> TargetObservation;
    fn activate_target(
        &mut self,
        target: &SequenceTarget,
    ) -> Result<ActivationJob, SequencePlaybackError>;
    fn publish_text(
        &mut self,
        expected_sequence: u32,
        rollback_text: Option<&str>,
        text: &str,
    ) -> Result<u32, ClipboardWriteFailure>;
    fn clipboard_sequence_matches(&mut self, expected_sequence: u32) -> bool;
    fn restore_clipboard_if_owned(
        &mut self,
        expected_sequence: u32,
        original: &ClipboardSnapshot,
    ) -> ClipboardRestoreOutcome;
    fn paste_chord(&mut self) -> Result<(), SequencePlaybackError>;
}

struct PlaybackSession {
    generation: u64,
    run: PreparedSequenceRun,
    next_step: usize,
    original_clipboard: ClipboardSnapshot,
    last_clipboard_sequence: Option<u32>,
    last_clipboard_text: Option<String>,
    last_paste_at: Option<Instant>,
    next_step_at: Option<Instant>,
    cleanup_at: Option<Instant>,
    pending_activation: Option<ActivationJob>,
}

/// Nonblocking owner for a prepared run. Every time-based transition is driven
/// by the caller's monotonic clock; it never sleeps or starts a playback worker.
pub(crate) struct SequencePlaybackController {
    generation: u64,
    status: SequencePlaybackStatus,
    prepared: Option<PreparedSequenceRun>,
    session: Option<PlaybackSession>,
}

impl Default for SequencePlaybackController {
    fn default() -> Self {
        Self {
            generation: 0,
            status: SequencePlaybackStatus::Idle,
            prepared: None,
            session: None,
        }
    }
}

impl SequencePlaybackController {
    pub(crate) fn status(&self) -> SequencePlaybackStatus {
        self.status
    }

    pub(crate) fn prepared_preview(&self) -> Option<super::PreparedSequencePreview> {
        self.prepared.as_ref().map(PreparedSequenceRun::preview)
    }

    pub(crate) fn current_step_number(&self) -> Option<(usize, usize)> {
        self.session.as_ref().map(|session| {
            let current = session
                .next_step
                .min(session.run.step_count())
                .saturating_add(1);
            (
                current.min(session.run.step_count()),
                session.run.step_count(),
            )
        })
    }

    pub(crate) fn is_preparing(&self) -> bool {
        self.status == SequencePlaybackStatus::Preparing
    }

    /// Whether the GUI must give the controller a tick on the next frame.
    /// Pending activation is woken by its worker; timed transitions are woken
    /// by the scheduled repaint deadline.
    pub(crate) fn needs_tick(&self) -> bool {
        self.session.as_ref().is_some_and(|session| {
            session.pending_activation.is_some()
                || session.next_step_at.is_some()
                || session.cleanup_at.is_some()
        })
    }

    pub(crate) fn begin_preparing(&mut self) -> Result<(), SequencePlaybackError> {
        if self.prepared.is_some()
            || self.session.is_some()
            || matches!(
                self.status,
                SequencePlaybackStatus::Preparing
                    | SequencePlaybackStatus::Ready
                    | SequencePlaybackStatus::Running
                    | SequencePlaybackStatus::AwaitAdvance
            )
        {
            return Err(SequencePlaybackError::Busy);
        }
        self.generation = self.generation.wrapping_add(1);
        self.status = SequencePlaybackStatus::Preparing;
        Ok(())
    }

    pub(crate) fn preparation_failed(&mut self) {
        if self.status == SequencePlaybackStatus::Preparing {
            self.status = SequencePlaybackStatus::Failed(SequencePlaybackError::InvalidState);
        }
    }

    pub(crate) fn set_ready(
        &mut self,
        run: PreparedSequenceRun,
    ) -> Result<(), SequencePlaybackError> {
        if self.status != SequencePlaybackStatus::Preparing || self.prepared.is_some() {
            return Err(SequencePlaybackError::InvalidState);
        }
        self.prepared = Some(run);
        self.status = SequencePlaybackStatus::Ready;
        Ok(())
    }

    pub(crate) fn start(
        &mut self,
        platform: &mut impl SequencePlaybackPlatform,
    ) -> Result<(), SequencePlaybackError> {
        if self.status != SequencePlaybackStatus::Ready {
            return Err(SequencePlaybackError::InvalidState);
        }
        let Some(run) = self.prepared.take() else {
            self.status = SequencePlaybackStatus::Failed(SequencePlaybackError::InvalidState);
            return Err(SequencePlaybackError::InvalidState);
        };
        let original_clipboard = match platform.capture_clipboard() {
            Ok(snapshot) if snapshot.sequence_number != 0 => snapshot,
            Ok(_) | Err(SequencePlaybackError::ClipboardUnavailable) => {
                self.status =
                    SequencePlaybackStatus::Failed(SequencePlaybackError::ClipboardUnavailable);
                return Err(SequencePlaybackError::ClipboardUnavailable);
            }
            Err(error) => {
                self.status = SequencePlaybackStatus::Failed(error);
                return Err(error);
            }
        };

        self.session = Some(PlaybackSession {
            generation: self.generation,
            run,
            next_step: 0,
            original_clipboard,
            last_clipboard_sequence: None,
            last_clipboard_text: None,
            last_paste_at: None,
            next_step_at: None,
            cleanup_at: None,
            pending_activation: None,
        });
        self.status = SequencePlaybackStatus::Running;
        if let Err(error) = self.paste_next_step(platform, true) {
            self.fail(error, platform.now());
            return Err(error);
        }
        Ok(())
    }

    /// Manually advance only from the explicit AwaitAdvance state. Launcher
    /// foreground may be restored to the captured target for this user action.
    pub(crate) fn next(
        &mut self,
        platform: &mut impl SequencePlaybackPlatform,
        now: Instant,
    ) -> Result<(), SequencePlaybackError> {
        let now = now.max(platform.now());
        if self.status != SequencePlaybackStatus::AwaitAdvance {
            return Err(SequencePlaybackError::InvalidState);
        }
        let session = self
            .session
            .as_mut()
            .ok_or(SequencePlaybackError::InvalidState)?;
        if session.next_step_at.is_some_and(|deadline| now < deadline) {
            return Err(SequencePlaybackError::TooSoon);
        }
        session.next_step_at = None;
        self.status = SequencePlaybackStatus::Running;
        if let Err(error) = self.paste_next_step(platform, true) {
            self.fail(error, platform.now());
            return Err(error);
        }
        Ok(())
    }

    pub(crate) fn cancel(&mut self, now: Instant) {
        match self.status {
            SequencePlaybackStatus::Preparing | SequencePlaybackStatus::Ready => {
                self.generation = self.generation.wrapping_add(1);
                self.prepared = None;
                self.status = SequencePlaybackStatus::Cancelled;
            }
            SequencePlaybackStatus::Running | SequencePlaybackStatus::AwaitAdvance => {
                self.generation = self.generation.wrapping_add(1);
                if let Some(session) = self.session.as_mut() {
                    session.next_step_at = None;
                    if let Some(activation) = session.pending_activation.as_ref() {
                        activation.cancel();
                    }
                    if session.last_clipboard_sequence.is_some() {
                        session.cleanup_at = Some(Self::safe_cleanup_deadline(session, now));
                    } else if session.pending_activation.is_none() {
                        self.session = None;
                    }
                }
                self.status = SequencePlaybackStatus::Cancelled;
            }
            SequencePlaybackStatus::Idle
            | SequencePlaybackStatus::Cancelled
            | SequencePlaybackStatus::Completed
            | SequencePlaybackStatus::Failed(_) => {}
        }
    }

    /// Perform at most one automatic step or one due cleanup per call.
    pub(crate) fn tick(
        &mut self,
        platform: &mut impl SequencePlaybackPlatform,
        now: Instant,
    ) -> Option<Duration> {
        let now = now.max(platform.now());
        let activation_poll = self.session.as_mut().and_then(|session| {
            session
                .pending_activation
                .as_mut()
                .map(|job| (session.generation, job.poll()))
        });
        if let Some((generation, ActivationPoll::Complete(result))) = activation_poll {
            if let Some(session) = self.session.as_mut() {
                session.pending_activation = None;
            }
            if generation == self.generation && self.status == SequencePlaybackStatus::Running {
                match result {
                    Ok(()) => {
                        if let Err(error) = self.paste_next_step(platform, false) {
                            self.fail(error, platform.now());
                        }
                    }
                    Err(error) => self.fail(error, platform.now()),
                }
            }
        }

        if self.status == SequencePlaybackStatus::Running
            && self
                .session
                .as_ref()
                .is_some_and(|session| session.pending_activation.is_none())
            && self
                .session
                .as_ref()
                .and_then(|session| session.next_step_at)
                .is_some_and(|deadline| deadline <= now)
        {
            if let Some(session) = self.session.as_mut() {
                session.next_step_at = None;
            }
            if let Err(error) = self.paste_next_step(platform, false) {
                self.fail(error, platform.now());
            }
        }

        if self.status == SequencePlaybackStatus::AwaitAdvance
            && self
                .session
                .as_ref()
                .and_then(|session| session.next_step_at)
                .is_some_and(|deadline| deadline <= now)
            && let Some(session) = self.session.as_mut()
        {
            session.next_step_at = None;
        }

        if self
            .session
            .as_ref()
            .and_then(|session| session.cleanup_at)
            .is_some_and(|deadline| deadline <= now)
        {
            self.cleanup(platform);
        }
        if self.status == SequencePlaybackStatus::Cancelled
            && self.session.as_ref().is_some_and(|session| {
                session.pending_activation.is_none()
                    && session.last_clipboard_sequence.is_none()
                    && session.cleanup_at.is_none()
            })
        {
            self.session = None;
        }
        self.next_wake_after(platform.now())
    }

    pub(crate) fn next_wake_after(&self, now: Instant) -> Option<Duration> {
        let deadline = self
            .session
            .as_ref()
            .into_iter()
            .flat_map(|session| [session.next_step_at, session.cleanup_at])
            .flatten()
            .min()?;
        Some(deadline.saturating_duration_since(now))
    }

    /// Stop future steps and extract the one pending conditional restore for
    /// shutdown. Pending activation is cancelled and joined here; the GUI then
    /// joins the short-lived conditional clipboard cleanup worker.
    pub(crate) fn take_shutdown_restore_plan(
        &mut self,
        now: Instant,
    ) -> Option<ClipboardRestorePlan> {
        self.cancel(now);
        if let Some(activation) = self
            .session
            .as_mut()
            .and_then(|session| session.pending_activation.as_mut())
        {
            activation.cancel_and_join();
            if let Some(session) = self.session.as_mut() {
                session.pending_activation = None;
            }
        }
        let session = self.session.take()?;
        let expected_sequence = session.last_clipboard_sequence?;
        Some(ClipboardRestorePlan {
            due: session
                .cleanup_at
                .unwrap_or_else(|| Self::safe_cleanup_deadline(&session, now)),
            expected_sequence,
            original: session.original_clipboard,
        })
    }

    fn paste_next_step(
        &mut self,
        platform: &mut impl SequencePlaybackPlatform,
        allow_launcher_restore: bool,
    ) -> Result<(), SequencePlaybackError> {
        let (target, expected_sequence, rollback_text, text) = {
            let session = self
                .session
                .as_ref()
                .ok_or(SequencePlaybackError::InvalidState)?;
            let text = session
                .run
                .step_text(session.next_step)
                .ok_or(SequencePlaybackError::InvalidState)?
                .to_owned();
            (
                session.run.target().clone(),
                session
                    .last_clipboard_sequence
                    .unwrap_or(session.original_clipboard.sequence_number),
                session
                    .last_clipboard_text
                    .clone()
                    .or_else(|| session.original_clipboard.text.clone()),
                text,
            )
        };

        if !self.ensure_target(platform, &target, allow_launcher_restore)? {
            return Ok(());
        }
        let published_sequence =
            match platform.publish_text(expected_sequence, rollback_text.as_deref(), &text) {
                Ok(sequence) => sequence,
                Err(failure) => {
                    if let Some(session) = self.session.as_mut()
                        && let Some(sequence) = failure.owned_sequence
                    {
                        session.last_clipboard_sequence = Some(sequence);
                        session.last_clipboard_text = rollback_text;
                    }
                    return Err(failure.error);
                }
            };
        if let Some(session) = self.session.as_mut() {
            session.last_clipboard_sequence = Some(published_sequence);
            session.last_clipboard_text = Some(text);
        }

        if !self.ensure_target(platform, &target, false)? {
            return Err(SequencePlaybackError::ActivationRejected);
        }
        if !platform.clipboard_sequence_matches(published_sequence) {
            return Err(SequencePlaybackError::ClipboardChanged);
        }
        // Recheck focus immediately before input so a launcher or unrelated
        // foreground transition between clipboard publication and Ctrl+V fails.
        if !self.ensure_target(platform, &target, false)? {
            return Err(SequencePlaybackError::ActivationRejected);
        }
        platform.paste_chord()?;
        let pasted_at = platform.now();

        let session = self
            .session
            .as_mut()
            .ok_or(SequencePlaybackError::InvalidState)?;
        session.next_step += 1;
        session.last_paste_at = Some(pasted_at);
        if session.next_step >= session.run.step_count() {
            session.next_step_at = None;
            session.cleanup_at = Some(pasted_at + CLIPBOARD_CONSUMPTION_SETTLE);
            self.status = SequencePlaybackStatus::Completed;
        } else {
            let configured = Duration::from_millis(u64::from(session.run.delay_ms()));
            session.next_step_at = Some(pasted_at + configured.max(CLIPBOARD_CONSUMPTION_SETTLE));
            self.status = match session.run.mode() {
                PlaybackMode::Automatic => SequencePlaybackStatus::Running,
                PlaybackMode::ManualAdvance => SequencePlaybackStatus::AwaitAdvance,
            };
        }
        Ok(())
    }

    fn ensure_target(
        &mut self,
        platform: &mut impl SequencePlaybackPlatform,
        target: &SequenceTarget,
        allow_launcher_restore: bool,
    ) -> Result<bool, SequencePlaybackError> {
        match platform.observe_target(target) {
            TargetObservation::Target {
                password_control: true,
            } => Err(SequencePlaybackError::PasswordField),
            TargetObservation::Target {
                password_control: false,
            } => Ok(true),
            TargetObservation::Launcher if allow_launcher_restore => {
                let mut activation = platform.activate_target(target)?;
                match activation.poll() {
                    ActivationPoll::Complete(Ok(())) => self.ensure_target(platform, target, false),
                    ActivationPoll::Complete(Err(error)) => Err(error),
                    ActivationPoll::Pending => {
                        let session = self
                            .session
                            .as_mut()
                            .ok_or(SequencePlaybackError::InvalidState)?;
                        session.pending_activation = Some(activation);
                        self.status = SequencePlaybackStatus::Running;
                        Ok(false)
                    }
                }
            }
            TargetObservation::Invalid => Err(SequencePlaybackError::InvalidTarget),
            TargetObservation::Launcher
            | TargetObservation::Other
            | TargetObservation::Unavailable => Err(SequencePlaybackError::FocusChanged),
        }
    }

    fn fail(&mut self, error: SequencePlaybackError, now: Instant) {
        self.status = SequencePlaybackStatus::Failed(error);
        let mut drop_session = false;
        if let Some(session) = self.session.as_mut() {
            session.next_step_at = None;
            if let Some(activation) = session.pending_activation.as_ref() {
                activation.cancel();
            }
            if session.last_clipboard_sequence.is_some() {
                session.cleanup_at = Some(Self::safe_cleanup_deadline(session, now));
            } else if session.pending_activation.is_none() {
                drop_session = true;
            }
        }
        if drop_session {
            self.session = None;
        }
    }

    fn safe_cleanup_deadline(session: &PlaybackSession, now: Instant) -> Instant {
        session
            .last_paste_at
            .map(|last_paste| (last_paste + CLIPBOARD_CONSUMPTION_SETTLE).max(now))
            .unwrap_or(now + CLIPBOARD_CONSUMPTION_SETTLE)
    }

    fn cleanup(&mut self, platform: &mut impl SequencePlaybackPlatform) {
        let Some(session) = self.session.as_ref() else {
            return;
        };
        let outcome = session.last_clipboard_sequence.map(|sequence| {
            platform.restore_clipboard_if_owned(sequence, &session.original_clipboard)
        });
        if outcome == Some(ClipboardRestoreOutcome::Failed)
            && !matches!(self.status, SequencePlaybackStatus::Failed(_))
        {
            self.status =
                SequencePlaybackStatus::Failed(SequencePlaybackError::ClipboardRestoreFailed);
        }
        let mut drop_session = false;
        if let Some(session) = self.session.as_mut() {
            session.last_clipboard_sequence = None;
            session.last_clipboard_text = None;
            session.cleanup_at = None;
            drop_session = session.pending_activation.is_none();
        }
        if drop_session {
            self.session = None;
        }
    }
}

/// Payload-bearing state used only to finish one final conditional clipboard
/// restore after the GUI begins shutting down. Debug output remains redacted.
pub(crate) struct ClipboardRestorePlan {
    due: Instant,
    expected_sequence: u32,
    original: ClipboardSnapshot,
}

impl std::fmt::Debug for ClipboardRestorePlan {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ClipboardRestorePlan")
            .field("due", &self.due)
            .field("sequence_available", &(self.expected_sequence != 0))
            .field("original", &self.original)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ActivationJob, ClipboardRestoreOutcome, ClipboardSnapshot, ClipboardWriteFailure,
        SequencePlaybackController, SequencePlaybackError, SequencePlaybackPlatform,
        SequencePlaybackStatus, TargetObservation,
    };
    use crate::active_window::ExternalForegroundWindow;
    use crate::paste_sequence::{
        PlaybackMode, Sequence, SequenceDraft, SequenceId, SequenceInvocationContext,
        SequencePreflight, SequenceStore, SequenceTarget, Step,
    };
    use crate::plugins::snippets::{SnippetEntry, SnippetFieldDefinition};
    use chrono::DateTime;
    use std::sync::mpsc::{self, Receiver, Sender};
    use std::time::{Duration, Instant};

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Event {
        Capture,
        Observe,
        Activate,
        Publish,
        CheckSequence,
        Paste,
        Restore,
        RestoreSkipped,
    }

    struct FakePlatform {
        clock: Instant,
        sequence: u32,
        clipboard: Option<String>,
        target: TargetObservation,
        activate_succeeds: bool,
        capture_error: Option<SequencePlaybackError>,
        write_error: Option<SequencePlaybackError>,
        rollback_succeeds: bool,
        paste_error: Option<SequencePlaybackError>,
        activation_delay: Duration,
        publish_delay: Duration,
        paste_delay: Duration,
        activation_gate: Option<Receiver<()>>,
        activation_finished: Option<Sender<()>>,
        events: Vec<Event>,
    }

    impl FakePlatform {
        fn at(clock: Instant) -> Self {
            Self {
                clock,
                sequence: 100,
                clipboard: Some("user clipboard".into()),
                target: TargetObservation::Target {
                    password_control: false,
                },
                activate_succeeds: true,
                capture_error: None,
                write_error: None,
                rollback_succeeds: true,
                paste_error: None,
                activation_delay: Duration::ZERO,
                publish_delay: Duration::ZERO,
                paste_delay: Duration::ZERO,
                activation_gate: None,
                activation_finished: None,
                events: Vec::new(),
            }
        }

        fn advance_to(&mut self, instant: Instant) {
            self.clock = self.clock.max(instant);
        }

        fn advance_by(&mut self, duration: Duration) {
            self.clock += duration;
        }

        fn paste_count(&self) -> usize {
            self.events
                .iter()
                .filter(|event| **event == Event::Paste)
                .count()
        }

        fn publication_count(&self) -> usize {
            self.events
                .iter()
                .filter(|event| **event == Event::Publish)
                .count()
        }

        fn external_copy(&mut self, text: &str) {
            self.sequence += 1;
            self.clipboard = Some(text.to_owned());
        }
    }

    impl SequencePlaybackPlatform for FakePlatform {
        fn now(&mut self) -> Instant {
            self.clock
        }

        fn capture_clipboard(&mut self) -> Result<ClipboardSnapshot, SequencePlaybackError> {
            self.events.push(Event::Capture);
            if let Some(error) = self.capture_error {
                return Err(error);
            }
            Ok(ClipboardSnapshot::new(
                self.sequence,
                self.clipboard.clone(),
            ))
        }

        fn observe_target(&mut self, _: &SequenceTarget) -> TargetObservation {
            self.events.push(Event::Observe);
            self.target
        }

        fn activate_target(
            &mut self,
            _: &SequenceTarget,
        ) -> Result<ActivationJob, SequencePlaybackError> {
            self.events.push(Event::Activate);
            if !self.activate_succeeds {
                return Ok(ActivationJob::completed(Err(
                    SequencePlaybackError::ActivationRejected,
                )));
            }
            if let Some(activation_gate) = self.activation_gate.take() {
                let finished = self
                    .activation_finished
                    .take()
                    .expect("test activation completion sender is paired with its gate");
                return ActivationJob::spawn(
                    move |cancelled| {
                        activation_gate
                            .recv()
                            .map_err(|_| SequencePlaybackError::ActivationRejected)?;
                        if cancelled.load(std::sync::atomic::Ordering::Acquire) {
                            Err(SequencePlaybackError::ActivationRejected)
                        } else {
                            Ok(())
                        }
                    },
                    move || {
                        let _ = finished.send(());
                    },
                );
            }
            self.advance_by(self.activation_delay);
            self.target = TargetObservation::Target {
                password_control: false,
            };
            Ok(ActivationJob::completed(Ok(())))
        }

        fn publish_text(
            &mut self,
            expected_sequence: u32,
            rollback_text: Option<&str>,
            text: &str,
        ) -> Result<u32, ClipboardWriteFailure> {
            self.events.push(Event::Publish);
            self.advance_by(self.publish_delay);
            if self.sequence != expected_sequence {
                return Err(ClipboardWriteFailure {
                    error: SequencePlaybackError::ClipboardChanged,
                    owned_sequence: None,
                });
            }
            if let Some(error) = self.write_error {
                self.sequence += 1;
                self.clipboard = self
                    .rollback_succeeds
                    .then(|| rollback_text.map(str::to_owned))
                    .flatten();
                return Err(ClipboardWriteFailure {
                    error,
                    owned_sequence: Some(self.sequence),
                });
            }
            self.sequence += 1;
            self.clipboard = Some(text.to_owned());
            Ok(self.sequence)
        }

        fn clipboard_sequence_matches(&mut self, expected_sequence: u32) -> bool {
            self.events.push(Event::CheckSequence);
            self.sequence == expected_sequence
        }

        fn restore_clipboard_if_owned(
            &mut self,
            expected_sequence: u32,
            original: &ClipboardSnapshot,
        ) -> ClipboardRestoreOutcome {
            if self.sequence != expected_sequence {
                self.events.push(Event::RestoreSkipped);
                return ClipboardRestoreOutcome::OwnershipLost;
            }
            self.events.push(Event::Restore);
            self.sequence += 1;
            self.clipboard = original.text.clone();
            ClipboardRestoreOutcome::Restored
        }

        fn paste_chord(&mut self) -> Result<(), SequencePlaybackError> {
            self.events.push(Event::Paste);
            self.advance_by(self.paste_delay);
            self.paste_error.map_or(Ok(()), Err)
        }
    }

    fn tick(
        controller: &mut SequencePlaybackController,
        platform: &mut FakePlatform,
        at: Instant,
    ) -> Option<Duration> {
        platform.advance_to(at);
        controller.tick(platform, at)
    }

    fn next(
        controller: &mut SequencePlaybackController,
        platform: &mut FakePlatform,
        at: Instant,
    ) -> Result<(), SequencePlaybackError> {
        platform.advance_to(at);
        controller.next(platform, at)
    }

    fn target() -> SequenceTarget {
        SequenceTarget::from_external_window(
            ExternalForegroundWindow {
                hwnd: 10,
                process_id: 20,
                thread_id: 30,
                title: Some("destination".into()),
            },
            999,
        )
        .unwrap()
    }

    fn prepared_run(
        mode: PlaybackMode,
        delay_ms: u32,
        texts: &[&str],
    ) -> super::PreparedSequenceRun {
        let sequence = Sequence {
            id: SequenceId(1),
            name: "Playback test".into(),
            hotkey: None,
            mode,
            delay_ms,
            steps: texts
                .iter()
                .map(|text| Step::LiteralText {
                    text: (*text).into(),
                })
                .collect(),
        };
        let context = SequenceInvocationContext::new(
            DateTime::parse_from_rfc3339("2024-01-02T03:04:05+00:00").unwrap(),
            None,
            target(),
        );
        SequencePreflight::begin(sequence, &[], &[], context)
            .unwrap()
            .finish(&[], &[])
            .unwrap()
    }

    fn ready_controller(run: super::PreparedSequenceRun) -> SequencePlaybackController {
        let mut controller = SequencePlaybackController::default();
        controller.begin_preparing().unwrap();
        controller.set_ready(run).unwrap();
        controller
    }

    #[test]
    fn reloaded_mixed_sequence_resolves_before_both_playback_modes_and_preserves_order() {
        let directory = tempfile::tempdir().unwrap();
        let store = SequenceStore::new(directory.path().join(super::super::PASTE_SEQUENCES_FILE));
        let history_text = "history snapshot\n雪";
        let mut draft = SequenceDraft::new(
            "Mixed saved sequence",
            vec![
                Step::SnippetRef {
                    alias: "hidden_reply".into(),
                },
                Step::SnippetRef {
                    alias: "context_values".into(),
                },
                Step::clipboard_history_reference(history_text),
                Step::LiteralText {
                    text: "Temporary 🌌\nsecond line".into(),
                },
            ],
        );
        draft.mode = PlaybackMode::ManualAdvance;
        draft.delay_ms = 275;
        let mut snapshot = store.load().unwrap();
        let id = snapshot.upsert(draft).unwrap();
        store.commit(&mut snapshot).unwrap();
        let loaded = store.load().unwrap();
        let snippets = vec![
            crate::plugins::snippets::SnippetEntry {
                alias: "hidden_reply".into(),
                text: "Hello {{name}} at {{sys.date:%Y-%m-%d}}".into(),
                hide_contents: true,
                prompt_for_fields: true,
                fields: vec![SnippetFieldDefinition::new("name")],
                tags: Vec::new(),
            },
            SnippetEntry {
                alias: "context_values".into(),
                text: "{{sys.time:%H:%M}}|{{sys.clipboard}}|{{sys.window_title}}".into(),
                hide_contents: false,
                prompt_for_fields: false,
                fields: Vec::new(),
                tags: Vec::new(),
            },
        ];
        let history = vec![history_text.to_owned()];
        let context = SequenceInvocationContext::new(
            DateTime::parse_from_rfc3339("2024-01-02T03:04:05+00:00").unwrap(),
            Some("captured clipboard".into()),
            SequenceTarget::from_external_window(
                ExternalForegroundWindow {
                    hwnd: 10,
                    process_id: 20,
                    thread_id: 30,
                    title: Some("origin editor".into()),
                },
                999,
            )
            .unwrap(),
        );
        let expected = [
            "Hello Ada at 2024-01-02",
            "03:04|captured clipboard|origin editor",
            history_text,
            "Temporary 🌌\nsecond line",
        ];

        for mode in [PlaybackMode::Automatic, PlaybackMode::ManualAdvance] {
            let mut sequence = loaded.sequence(id).unwrap().clone();
            sequence.mode = mode;
            let mut preflight =
                SequencePreflight::begin(sequence, &snippets, &history, context.clone()).unwrap();
            assert_eq!(preflight.current_prompt().unwrap().step_index, 0);

            let start = Instant::now();
            let mut platform = FakePlatform::at(start);
            assert!(
                platform.events.is_empty(),
                "preflight must not touch the clipboard or input"
            );
            preflight
                .submit_current_prompt(&std::collections::HashMap::from([(
                    "name".into(),
                    "Ada".into(),
                )]))
                .unwrap();
            let run = preflight.finish(&snippets, &history).unwrap();
            assert_eq!(
                (0..expected.len())
                    .map(|index| run.step_text(index).unwrap())
                    .collect::<Vec<_>>(),
                expected
            );
            let preview = run.preview();
            assert!(preview.steps[0].hidden);
            assert!(preview.steps[0].text.is_none());
            assert_eq!(preview.steps[1].text.as_deref(), Some(expected[1]));

            let mut controller = ready_controller(run);
            assert!(platform.events.is_empty(), "preparing Ready must not paste");
            controller.start(&mut platform).unwrap();
            assert_eq!(platform.clipboard.as_deref(), Some(expected[0]));
            assert_eq!(platform.paste_count(), 1);
            assert_eq!(
                controller.status(),
                if mode == PlaybackMode::ManualAdvance {
                    SequencePlaybackStatus::AwaitAdvance
                } else {
                    SequencePlaybackStatus::Running
                }
            );

            for (step_index, elapsed_ms) in [(1usize, 275u64), (2, 550), (3, 825)] {
                let before_deadline = start + Duration::from_millis(elapsed_ms - 1);
                let deadline = start + Duration::from_millis(elapsed_ms);
                if mode == PlaybackMode::ManualAdvance {
                    assert_eq!(
                        next(&mut controller, &mut platform, before_deadline),
                        Err(SequencePlaybackError::TooSoon)
                    );
                    tick(&mut controller, &mut platform, deadline);
                    next(&mut controller, &mut platform, deadline).unwrap();
                } else {
                    tick(&mut controller, &mut platform, before_deadline);
                    assert_eq!(platform.paste_count(), step_index);
                    tick(&mut controller, &mut platform, deadline);
                }
                assert_eq!(platform.clipboard.as_deref(), Some(expected[step_index]));
                assert_eq!(platform.paste_count(), step_index + 1);
            }

            assert_eq!(controller.status(), SequencePlaybackStatus::Completed);
            tick(
                &mut controller,
                &mut platform,
                start + Duration::from_millis(1_005),
            );
            assert_eq!(platform.clipboard.as_deref(), Some("user clipboard"));
        }
    }

    #[test]
    fn shutdown_restore_plan_restores_only_if_sequence_still_owns_clipboard() {
        let start = Instant::now();
        let mut controller = ready_controller(prepared_run(
            PlaybackMode::ManualAdvance,
            0,
            &["sequence text", "not pasted after shutdown"],
        ));
        let mut platform = FakePlatform::at(start);
        controller.start(&mut platform).unwrap();
        assert_eq!(platform.clipboard.as_deref(), Some("sequence text"));

        let plan = controller
            .take_shutdown_restore_plan(start + Duration::from_millis(40))
            .expect("shutdown must retain one conditional restore");
        assert!(plan.due >= start + Duration::from_millis(180));
        assert_eq!(controller.status(), SequencePlaybackStatus::Cancelled);
        assert_eq!(platform.paste_count(), 1);
        assert_eq!(
            platform.restore_clipboard_if_owned(plan.expected_sequence, &plan.original),
            ClipboardRestoreOutcome::Restored
        );
        assert_eq!(platform.clipboard.as_deref(), Some("user clipboard"));

        let mut interrupted = ready_controller(prepared_run(
            PlaybackMode::ManualAdvance,
            0,
            &["sequence text", "not pasted after shutdown"],
        ));
        let mut interrupted_platform = FakePlatform::at(start);
        interrupted.start(&mut interrupted_platform).unwrap();
        let plan = interrupted
            .take_shutdown_restore_plan(start + Duration::from_millis(40))
            .unwrap();
        interrupted_platform.external_copy("user replacement");
        assert_eq!(
            interrupted_platform.restore_clipboard_if_owned(plan.expected_sequence, &plan.original),
            ClipboardRestoreOutcome::OwnershipLost
        );
        assert_eq!(
            interrupted_platform.clipboard.as_deref(),
            Some("user replacement")
        );
        assert_eq!(interrupted_platform.paste_count(), 1);
    }

    #[test]
    fn automatic_playback_obeys_deadlines_and_restores_owned_clipboard_once() {
        let start = Instant::now();
        let mut controller = ready_controller(prepared_run(
            PlaybackMode::Automatic,
            20,
            &["first", "second"],
        ));
        let mut platform = FakePlatform::at(start);
        controller.start(&mut platform).unwrap();
        assert_eq!(controller.status(), SequencePlaybackStatus::Running);
        assert_eq!(platform.paste_count(), 1);
        assert_eq!(platform.clipboard.as_deref(), Some("first"));
        assert_eq!(
            controller.next_wake_after(start),
            Some(Duration::from_millis(180))
        );

        tick(
            &mut controller,
            &mut platform,
            start + Duration::from_millis(179),
        );
        assert_eq!(platform.paste_count(), 1);
        tick(
            &mut controller,
            &mut platform,
            start + Duration::from_millis(180),
        );
        assert_eq!(controller.status(), SequencePlaybackStatus::Completed);
        assert_eq!(platform.paste_count(), 2);
        assert_eq!(platform.clipboard.as_deref(), Some("second"));

        tick(
            &mut controller,
            &mut platform,
            start + Duration::from_millis(359),
        );
        assert_eq!(platform.clipboard.as_deref(), Some("second"));
        tick(
            &mut controller,
            &mut platform,
            start + Duration::from_millis(360),
        );
        assert_eq!(platform.clipboard.as_deref(), Some("user clipboard"));
        assert_eq!(
            platform
                .events
                .iter()
                .filter(|event| **event == Event::Restore)
                .count(),
            1
        );
        tick(
            &mut controller,
            &mut platform,
            start + Duration::from_secs(1),
        );
        assert_eq!(
            platform
                .events
                .iter()
                .filter(|event| **event == Event::Restore)
                .count(),
            1
        );
    }

    #[test]
    fn step_delays_start_after_slow_activation_publication_and_input_complete() {
        let start = Instant::now();
        let mut controller = ready_controller(prepared_run(
            PlaybackMode::Automatic,
            20,
            &["first", "second"],
        ));
        let mut platform = FakePlatform::at(start);
        platform.target = TargetObservation::Launcher;
        platform.activation_delay = Duration::from_millis(70);
        platform.publish_delay = Duration::from_millis(50);
        platform.paste_delay = Duration::from_millis(70);

        controller.start(&mut platform).unwrap();
        assert_eq!(platform.now(), start + Duration::from_millis(190));
        assert_eq!(platform.paste_count(), 1);
        assert_eq!(
            controller.next_wake_after(start + Duration::from_millis(180)),
            Some(Duration::from_millis(190))
        );

        tick(
            &mut controller,
            &mut platform,
            start + Duration::from_millis(369),
        );
        assert_eq!(platform.paste_count(), 1);
        tick(
            &mut controller,
            &mut platform,
            start + Duration::from_millis(370),
        );
        assert_eq!(platform.paste_count(), 2);
        assert_eq!(
            controller.next_wake_after(platform.now()),
            Some(Duration::from_millis(180))
        );
        tick(
            &mut controller,
            &mut platform,
            start + Duration::from_millis(670),
        );
        assert_eq!(platform.clipboard.as_deref(), Some("user clipboard"));
    }

    #[test]
    fn manual_next_is_explicit_and_restores_the_target_only_from_launcher() {
        let start = Instant::now();
        let mut controller = ready_controller(prepared_run(
            PlaybackMode::ManualAdvance,
            0,
            &["first", "second"],
        ));
        let mut platform = FakePlatform::at(start);
        platform.target = TargetObservation::Launcher;
        controller.start(&mut platform).unwrap();
        assert_eq!(controller.status(), SequencePlaybackStatus::AwaitAdvance);
        assert_eq!(platform.paste_count(), 1);
        assert_eq!(
            platform
                .events
                .iter()
                .filter(|event| **event == Event::Activate)
                .count(),
            1
        );

        assert_eq!(
            next(
                &mut controller,
                &mut platform,
                start + Duration::from_millis(179)
            ),
            Err(SequencePlaybackError::TooSoon)
        );
        assert_eq!(platform.paste_count(), 1);
        assert_eq!(platform.publication_count(), 1);
        assert_eq!(
            tick(
                &mut controller,
                &mut platform,
                start + Duration::from_millis(180)
            ),
            None
        );
        assert_eq!(platform.paste_count(), 1);
        platform.target = TargetObservation::Launcher;
        next(
            &mut controller,
            &mut platform,
            start + Duration::from_millis(180),
        )
        .unwrap();
        assert_eq!(controller.status(), SequencePlaybackStatus::Completed);
        assert_eq!(platform.paste_count(), 2);
        assert_eq!(
            platform
                .events
                .iter()
                .filter(|event| **event == Event::Activate)
                .count(),
            2
        );
        assert_eq!(
            next(
                &mut controller,
                &mut platform,
                start + Duration::from_secs(3)
            ),
            Err(SequencePlaybackError::InvalidState)
        );
    }

    #[test]
    fn cancellation_stops_future_steps_and_defers_conditional_restore() {
        let start = Instant::now();
        let mut controller = ready_controller(prepared_run(
            PlaybackMode::Automatic,
            5,
            &["first", "second", "third"],
        ));
        let mut platform = FakePlatform::at(start);
        controller.start(&mut platform).unwrap();
        controller.cancel(start + Duration::from_millis(40));
        assert_eq!(controller.status(), SequencePlaybackStatus::Cancelled);
        assert_eq!(platform.paste_count(), 1);
        assert_eq!(
            controller.next_wake_after(start + Duration::from_millis(40)),
            Some(Duration::from_millis(140))
        );

        tick(
            &mut controller,
            &mut platform,
            start + Duration::from_millis(179),
        );
        assert_eq!(platform.paste_count(), 1);
        assert_eq!(platform.clipboard.as_deref(), Some("first"));
        tick(
            &mut controller,
            &mut platform,
            start + Duration::from_millis(180),
        );
        assert_eq!(platform.clipboard.as_deref(), Some("user clipboard"));
        assert_eq!(controller.status(), SequencePlaybackStatus::Cancelled);
    }

    #[test]
    fn stolen_focus_password_controls_and_reused_target_identity_refuse_paste() {
        let start = Instant::now();
        let mut password = ready_controller(prepared_run(PlaybackMode::Automatic, 0, &["secret"]));
        let mut password_platform = FakePlatform::at(start);
        password_platform.target = TargetObservation::Target {
            password_control: true,
        };
        assert_eq!(
            password.start(&mut password_platform),
            Err(SequencePlaybackError::PasswordField)
        );
        assert_eq!(password_platform.publication_count(), 0);
        assert_eq!(password_platform.paste_count(), 0);

        let mut changed_focus =
            ready_controller(prepared_run(PlaybackMode::Automatic, 0, &["one", "two"]));
        let mut focus_platform = FakePlatform::at(start);
        changed_focus.start(&mut focus_platform).unwrap();
        focus_platform.target = TargetObservation::Other;
        assert_eq!(
            tick(
                &mut changed_focus,
                &mut focus_platform,
                start + Duration::from_millis(180)
            ),
            None
        );
        assert_eq!(
            changed_focus.status(),
            SequencePlaybackStatus::Failed(SequencePlaybackError::FocusChanged)
        );
        assert_eq!(focus_platform.paste_count(), 1);
        assert_eq!(focus_platform.publication_count(), 1);

        let mut reused =
            ready_controller(prepared_run(PlaybackMode::Automatic, 0, &["do not paste"]));
        let mut reused_platform = FakePlatform::at(start);
        reused_platform.target = TargetObservation::Invalid;
        assert_eq!(
            reused.start(&mut reused_platform),
            Err(SequencePlaybackError::InvalidTarget)
        );
        assert_eq!(reused_platform.publication_count(), 0);
    }

    #[test]
    fn identical_external_clipboard_revision_stops_before_replacement_and_restore() {
        let start = Instant::now();
        let mut controller = ready_controller(prepared_run(
            PlaybackMode::ManualAdvance,
            0,
            &["same", "next"],
        ));
        let mut platform = FakePlatform::at(start);
        controller.start(&mut platform).unwrap();
        platform.external_copy("same");
        assert_eq!(
            next(
                &mut controller,
                &mut platform,
                start + Duration::from_secs(1)
            ),
            Err(SequencePlaybackError::ClipboardChanged)
        );
        assert_eq!(platform.clipboard.as_deref(), Some("same"));
        assert_eq!(platform.publication_count(), 2);
        tick(
            &mut controller,
            &mut platform,
            start + Duration::from_secs(2),
        );
        assert_eq!(platform.clipboard.as_deref(), Some("same"));
        assert_eq!(
            platform
                .events
                .iter()
                .filter(|event| **event == Event::RestoreSkipped)
                .count(),
            1
        );
        assert_eq!(platform.paste_count(), 1);
    }

    #[test]
    fn capture_and_input_failures_never_run_future_steps_and_cleanup_once() {
        let start = Instant::now();
        let mut unavailable = ready_controller(prepared_run(PlaybackMode::Automatic, 0, &["one"]));
        let mut unavailable_platform = FakePlatform::at(start);
        unavailable_platform.capture_error = Some(SequencePlaybackError::UnsupportedClipboard);
        assert_eq!(
            unavailable.start(&mut unavailable_platform),
            Err(SequencePlaybackError::UnsupportedClipboard)
        );
        assert_eq!(unavailable_platform.publication_count(), 0);

        let mut input_failure =
            ready_controller(prepared_run(PlaybackMode::Automatic, 0, &["one", "never"]));
        let mut input_platform = FakePlatform::at(start);
        input_platform.paste_error = Some(SequencePlaybackError::InputRejected);
        assert_eq!(
            input_failure.start(&mut input_platform),
            Err(SequencePlaybackError::InputRejected)
        );
        assert_eq!(
            input_failure.status(),
            SequencePlaybackStatus::Failed(SequencePlaybackError::InputRejected)
        );
        assert_eq!(input_platform.publication_count(), 1);
        tick(
            &mut input_failure,
            &mut input_platform,
            start + Duration::from_millis(180),
        );
        assert_eq!(input_platform.clipboard.as_deref(), Some("user clipboard"));
        assert_eq!(input_platform.paste_count(), 1);
        assert_eq!(
            input_platform
                .events
                .iter()
                .filter(|event| **event == Event::Restore)
                .count(),
            1
        );
    }

    #[test]
    fn failed_clipboard_write_keeps_cleanup_ownership_even_if_in_lock_rollback_fails() {
        let start = Instant::now();
        let mut controller = ready_controller(prepared_run(PlaybackMode::Automatic, 0, &["one"]));
        let mut platform = FakePlatform::at(start);
        platform.write_error = Some(SequencePlaybackError::ClipboardWriteFailed);
        platform.rollback_succeeds = false;

        assert_eq!(
            controller.start(&mut platform),
            Err(SequencePlaybackError::ClipboardWriteFailed)
        );
        assert_eq!(platform.clipboard, None);
        assert_eq!(
            controller.status(),
            SequencePlaybackStatus::Failed(SequencePlaybackError::ClipboardWriteFailed)
        );
        assert_eq!(
            controller.next_wake_after(start),
            Some(Duration::from_millis(180))
        );

        tick(
            &mut controller,
            &mut platform,
            start + Duration::from_millis(180),
        );
        assert_eq!(platform.clipboard.as_deref(), Some("user clipboard"));
        assert_eq!(
            platform
                .events
                .iter()
                .filter(|event| **event == Event::Restore)
                .count(),
            1
        );
        assert_eq!(platform.paste_count(), 0);
    }

    #[test]
    fn preparation_admission_includes_prompt_ready_playback_and_pending_cleanup() {
        let mut controller = SequencePlaybackController::default();
        controller.begin_preparing().unwrap();
        assert!(controller.begin_preparing().is_err());
        controller.preparation_failed();
        controller.begin_preparing().unwrap();
        controller
            .set_ready(prepared_run(
                PlaybackMode::ManualAdvance,
                0,
                &["one", "two"],
            ))
            .unwrap();
        assert!(controller.begin_preparing().is_err());

        let start = Instant::now();
        let mut platform = FakePlatform::at(start);
        controller.start(&mut platform).unwrap();
        assert_eq!(controller.status(), SequencePlaybackStatus::AwaitAdvance);
        assert!(controller.begin_preparing().is_err());
        controller.cancel(start + Duration::from_millis(20));
        assert!(controller.begin_preparing().is_err());
        tick(
            &mut controller,
            &mut platform,
            start + Duration::from_millis(180),
        );
        controller.begin_preparing().unwrap();
    }

    #[test]
    fn cancelled_pending_activation_is_reaped_without_late_publication_or_input() {
        let start = Instant::now();
        let mut controller = ready_controller(prepared_run(
            PlaybackMode::Automatic,
            0,
            &["must not paste"],
        ));
        let mut platform = FakePlatform::at(start);
        platform.target = TargetObservation::Launcher;
        let (release, activation_gate) = mpsc::channel();
        let (activation_finished, finished) = mpsc::channel();
        platform.activation_gate = Some(activation_gate);
        platform.activation_finished = Some(activation_finished);

        controller.start(&mut platform).unwrap();
        assert_eq!(controller.status(), SequencePlaybackStatus::Running);
        assert!(controller.needs_tick());
        assert_eq!(platform.publication_count(), 0);
        assert_eq!(platform.paste_count(), 0);
        assert_eq!(
            controller.begin_preparing(),
            Err(SequencePlaybackError::Busy)
        );

        controller.cancel(start + Duration::from_millis(1));
        assert_eq!(controller.status(), SequencePlaybackStatus::Cancelled);
        release.send(()).unwrap();
        finished.recv_timeout(Duration::from_secs(2)).unwrap();
        tick(
            &mut controller,
            &mut platform,
            start + Duration::from_millis(2),
        );
        assert_eq!(platform.publication_count(), 0);
        assert_eq!(platform.paste_count(), 0);
        assert!(!controller.needs_tick());
        controller.begin_preparing().unwrap();
    }

    #[test]
    fn completed_pending_activation_revalidates_before_publication_and_input() {
        let start = Instant::now();
        let mut controller = ready_controller(prepared_run(
            PlaybackMode::Automatic,
            0,
            &["activated destination"],
        ));
        let mut platform = FakePlatform::at(start);
        platform.target = TargetObservation::Launcher;
        let (release, activation_gate) = mpsc::channel();
        let (activation_finished, finished) = mpsc::channel();
        platform.activation_gate = Some(activation_gate);
        platform.activation_finished = Some(activation_finished);

        controller.start(&mut platform).unwrap();
        assert_eq!(platform.publication_count(), 0);
        assert!(controller.needs_tick());

        release.send(()).unwrap();
        finished.recv_timeout(Duration::from_secs(2)).unwrap();
        platform.target = TargetObservation::Target {
            password_control: false,
        };
        tick(
            &mut controller,
            &mut platform,
            start + Duration::from_millis(1),
        );

        assert_eq!(controller.status(), SequencePlaybackStatus::Completed);
        assert_eq!(platform.clipboard.as_deref(), Some("activated destination"));
        assert_eq!(platform.publication_count(), 1);
        assert_eq!(platform.paste_count(), 1);
        assert!(controller.needs_tick());
    }
}

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub(crate) use windows::NativeSequencePlaybackPlatform;

#[cfg(not(windows))]
pub(crate) struct NativeSequencePlaybackPlatform;

#[cfg(not(windows))]
impl NativeSequencePlaybackPlatform {
    pub(crate) fn new(_: Option<usize>, _: Option<egui::Context>) -> Self {
        Self
    }

    pub(crate) fn restore_after_shutdown(
        _: ClipboardRestorePlan,
        _: Option<usize>,
    ) -> Result<(), SequencePlaybackError> {
        Err(SequencePlaybackError::ClipboardUnavailable)
    }
}

#[cfg(not(windows))]
impl SequencePlaybackPlatform for NativeSequencePlaybackPlatform {
    fn now(&mut self) -> Instant {
        Instant::now()
    }

    fn capture_clipboard(&mut self) -> Result<ClipboardSnapshot, SequencePlaybackError> {
        Err(SequencePlaybackError::ClipboardUnavailable)
    }

    fn observe_target(&mut self, _: &SequenceTarget) -> TargetObservation {
        TargetObservation::Unavailable
    }

    fn activate_target(
        &mut self,
        _: &SequenceTarget,
    ) -> Result<ActivationJob, SequencePlaybackError> {
        Err(SequencePlaybackError::ActivationRejected)
    }

    fn publish_text(
        &mut self,
        _: u32,
        _: Option<&str>,
        _: &str,
    ) -> Result<u32, ClipboardWriteFailure> {
        Err(ClipboardWriteFailure {
            error: SequencePlaybackError::ClipboardUnavailable,
            owned_sequence: None,
        })
    }

    fn clipboard_sequence_matches(&mut self, _: u32) -> bool {
        false
    }

    fn restore_clipboard_if_owned(
        &mut self,
        _: u32,
        _: &ClipboardSnapshot,
    ) -> ClipboardRestoreOutcome {
        ClipboardRestoreOutcome::Failed
    }

    fn paste_chord(&mut self) -> Result<(), SequencePlaybackError> {
        Err(SequencePlaybackError::InputRejected)
    }
}
