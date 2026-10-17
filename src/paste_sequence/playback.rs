use super::{PlaybackMode, PreparedSequenceRun, SequenceTarget};
use std::time::{Duration, Instant};

const CLIPBOARD_CONSUMPTION_SETTLE: Duration = Duration::from_millis(180);
const TARGET_RETRY_INTERVAL: Duration = Duration::from_millis(100);
const TARGET_WAIT_LIMIT: Duration = Duration::from_secs(30);

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
            Self::InvalidTarget => "The current destination window is no longer valid.",
            Self::FocusChanged => {
                "The destination changed before paste; no further steps were pasted."
            }
            Self::PasswordField => "Pasting into a detected password field is blocked.",
            Self::ClipboardChanged => {
                "The clipboard changed during playback; remaining steps were stopped."
            }
            Self::ClipboardWriteFailed => "Sequence text could not be published to the clipboard.",
            Self::ClipboardRestoreFailed => "The original clipboard could not be restored.",
            Self::InputRejected => "The paste keystroke was rejected by Windows.",
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
    AwaitTarget,
    AwaitPaste,
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

/// Minimal platform boundary for the playback state machine. Tests supply a
/// deterministic fake; production only implements these narrow native calls.
pub(crate) trait SequencePlaybackPlatform {
    fn now(&mut self) -> Instant;
    fn capture_clipboard(&mut self) -> Result<ClipboardSnapshot, SequencePlaybackError>;
    fn observe_target(&mut self, target: &SequenceTarget) -> TargetObservation;
    /// Capture the actual foreground identity without reading its title.
    fn current_target(&mut self) -> Result<SequenceTarget, SequencePlaybackError>;
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
    target_wait_deadline: Option<Instant>,
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
    /// A session generation is published only after Start succeeds. Preflight
    /// and prepared previews never own physical input.
    pub(crate) fn physical_paste_generation(&self) -> Option<u64> {
        self.session
            .as_ref()
            .filter(|session| {
                session.run.mode() == PlaybackMode::PasteToAdvance
                    && matches!(
                        self.status,
                        SequencePlaybackStatus::AwaitPaste | SequencePlaybackStatus::Running
                    )
            })
            .map(|session| session.generation)
    }

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
    /// Timed transitions are woken by the scheduled repaint deadline.
    pub(crate) fn needs_tick(&self) -> bool {
        self.session
            .as_ref()
            .is_some_and(|session| session.next_step_at.is_some() || session.cleanup_at.is_some())
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
                    | SequencePlaybackStatus::AwaitTarget
                    | SequencePlaybackStatus::AwaitPaste
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
            target_wait_deadline: None,
        });
        if self
            .session
            .as_ref()
            .is_some_and(|session| session.run.mode() == PlaybackMode::PasteToAdvance)
        {
            self.status = SequencePlaybackStatus::AwaitPaste;
            return Ok(());
        }
        self.status = SequencePlaybackStatus::Running;
        if let Err(error) = self.paste_next_step(platform) {
            self.fail(error, platform.now());
            return Err(error);
        }
        Ok(())
    }

    /// Manual advancement captures the current foreground for this step. It
    /// never redirects focus to an earlier destination.
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
        if let Err(error) = self.paste_next_step(platform) {
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
            SequencePlaybackStatus::Running
            | SequencePlaybackStatus::AwaitAdvance
            | SequencePlaybackStatus::AwaitTarget
            | SequencePlaybackStatus::AwaitPaste => {
                self.generation = self.generation.wrapping_add(1);
                if let Some(session) = self.session.as_mut() {
                    session.next_step_at = None;
                    if session.last_clipboard_sequence.is_some() {
                        session.cleanup_at = Some(Self::safe_cleanup_deadline(session, now));
                    } else {
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
        if matches!(
            self.status,
            SequencePlaybackStatus::Running | SequencePlaybackStatus::AwaitTarget
        ) && self
            .session
            .as_ref()
            .and_then(|session| session.next_step_at)
            .is_some_and(|deadline| deadline <= now)
        {
            if let Some(session) = self.session.as_mut() {
                session.next_step_at = None;
            }
            if let Err(error) = self.paste_next_step(platform) {
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
                session.last_clipboard_sequence.is_none() && session.cleanup_at.is_none()
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
    /// shutdown. The GUI joins the short-lived conditional cleanup worker.
    pub(crate) fn take_shutdown_restore_plan(
        &mut self,
        now: Instant,
    ) -> Option<ClipboardRestorePlan> {
        self.cancel(now);
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
    ) -> Result<(), SequencePlaybackError> {
        let session = self
            .session
            .as_ref()
            .ok_or(SequencePlaybackError::InvalidState)?;
        if session.generation != self.generation {
            return Err(SequencePlaybackError::InvalidState);
        }
        if session
            .target_wait_deadline
            .is_some_and(|deadline| platform.now() >= deadline)
        {
            return Err(SequencePlaybackError::FocusChanged);
        }
        let target = match platform.current_target() {
            Ok(target) => target,
            Err(SequencePlaybackError::FocusChanged) => {
                let now = platform.now();
                let session = self
                    .session
                    .as_mut()
                    .ok_or(SequencePlaybackError::InvalidState)?;
                let deadline = *session
                    .target_wait_deadline
                    .get_or_insert(now + TARGET_WAIT_LIMIT);
                if now >= deadline {
                    return Err(SequencePlaybackError::FocusChanged);
                }
                session.next_step_at = Some((now + TARGET_RETRY_INTERVAL).min(deadline));
                self.status = SequencePlaybackStatus::AwaitTarget;
                return Ok(());
            }
            Err(error) => return Err(error),
        };
        Self::ensure_target(platform, &target)?;
        let (expected_sequence, rollback_text, text) = {
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

        Self::ensure_target(platform, &target)?;
        if !platform.clipboard_sequence_matches(published_sequence) {
            return Err(SequencePlaybackError::ClipboardChanged);
        }
        // Recheck focus immediately before input so a launcher or unrelated
        // foreground transition between clipboard publication and Ctrl+V fails.
        Self::ensure_target(platform, &target)?;
        platform.paste_chord()?;
        let pasted_at = platform.now();

        let session = self
            .session
            .as_mut()
            .ok_or(SequencePlaybackError::InvalidState)?;
        session.target_wait_deadline = None;
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
                PlaybackMode::PasteToAdvance => SequencePlaybackStatus::AwaitPaste,
            };
        }
        Ok(())
    }

    fn ensure_target(
        platform: &mut impl SequencePlaybackPlatform,
        target: &SequenceTarget,
    ) -> Result<(), SequencePlaybackError> {
        match platform.observe_target(target) {
            TargetObservation::Target {
                password_control: true,
            } => Err(SequencePlaybackError::PasswordField),
            TargetObservation::Target {
                password_control: false,
            } => Ok(()),
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
            if session.last_clipboard_sequence.is_some() {
                session.cleanup_at = Some(Self::safe_cleanup_deadline(session, now));
            } else {
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
        self.session = None;
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
        ClipboardRestoreOutcome, ClipboardSnapshot, ClipboardWriteFailure,
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
    use std::time::{Duration, Instant};

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Event {
        Capture,
        Observe,
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
        capture_error: Option<SequencePlaybackError>,
        write_error: Option<SequencePlaybackError>,
        rollback_succeeds: bool,
        paste_error: Option<SequencePlaybackError>,
        publish_delay: Duration,
        paste_delay: Duration,
        foreground: SequenceTarget,
        switch_after_publication: Option<SequenceTarget>,
        switch_on_final_check: Option<SequenceTarget>,
        pasted_targets: Vec<isize>,
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
                capture_error: None,
                write_error: None,
                rollback_succeeds: true,
                paste_error: None,
                publish_delay: Duration::ZERO,
                paste_delay: Duration::ZERO,
                foreground: target(),
                switch_after_publication: None,
                switch_on_final_check: None,
                pasted_targets: Vec::new(),
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

        fn observe_target(&mut self, expected: &SequenceTarget) -> TargetObservation {
            self.events.push(Event::Observe);
            if expected.hwnd() != self.foreground.hwnd()
                || expected.process_id() != self.foreground.process_id()
                || expected.thread_id() != self.foreground.thread_id()
            {
                TargetObservation::Other
            } else {
                self.target
            }
        }

        fn current_target(&mut self) -> Result<SequenceTarget, SequencePlaybackError> {
            match self.target {
                TargetObservation::Target { .. } => Ok(self.foreground.clone()),
                TargetObservation::Invalid => Err(SequencePlaybackError::InvalidTarget),
                TargetObservation::Launcher
                | TargetObservation::Other
                | TargetObservation::Unavailable => Err(SequencePlaybackError::FocusChanged),
            }
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
            if let Some(target) = self.switch_after_publication.take() {
                self.foreground = target;
            }
            Ok(self.sequence)
        }

        fn clipboard_sequence_matches(&mut self, expected_sequence: u32) -> bool {
            self.events.push(Event::CheckSequence);
            if let Some(target) = self.switch_on_final_check.take() {
                self.foreground = target;
            }
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
            self.pasted_targets.push(self.foreground.hwnd());
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
                // The title/template context stays frozen while destinations change.
                platform.foreground = other_target(10 + step_index as isize);
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
    fn step_delays_start_after_publication_and_input_complete() {
        let start = Instant::now();
        let mut controller = ready_controller(prepared_run(
            PlaybackMode::Automatic,
            20,
            &["first", "second"],
        ));
        let mut platform = FakePlatform::at(start);
        platform.publish_delay = Duration::from_millis(50);
        platform.paste_delay = Duration::from_millis(70);

        controller.start(&mut platform).unwrap();
        assert_eq!(platform.now(), start + Duration::from_millis(120));
        assert_eq!(platform.paste_count(), 1);
        assert_eq!(
            controller.next_wake_after(start + Duration::from_millis(180)),
            Some(Duration::from_millis(120))
        );

        tick(
            &mut controller,
            &mut platform,
            start + Duration::from_millis(299),
        );
        assert_eq!(platform.paste_count(), 1);
        tick(
            &mut controller,
            &mut platform,
            start + Duration::from_millis(300),
        );
        assert_eq!(platform.paste_count(), 2);
        assert_eq!(
            controller.next_wake_after(platform.now()),
            Some(Duration::from_millis(180))
        );
        tick(
            &mut controller,
            &mut platform,
            start + Duration::from_millis(600),
        );
        assert_eq!(platform.clipboard.as_deref(), Some("user clipboard"));
    }

    #[test]
    fn manual_next_waits_for_current_target_without_restoring_launcher_origin() {
        let start = Instant::now();
        let mut controller = ready_controller(prepared_run(
            PlaybackMode::ManualAdvance,
            0,
            &["first", "second"],
        ));
        let mut platform = FakePlatform::at(start);
        platform.target = TargetObservation::Launcher;
        controller.start(&mut platform).unwrap();
        assert_eq!(controller.status(), SequencePlaybackStatus::AwaitTarget);
        assert_eq!(controller.current_step_number(), Some((1, 2)));
        assert_eq!(platform.publication_count(), 0);
        platform.target = TargetObservation::Target {
            password_control: false,
        };
        tick(
            &mut controller,
            &mut platform,
            start + Duration::from_millis(100),
        );
        assert_eq!(controller.status(), SequencePlaybackStatus::AwaitAdvance);
        assert_eq!(platform.paste_count(), 1);
        assert_eq!(
            next(
                &mut controller,
                &mut platform,
                start + Duration::from_millis(279)
            ),
            Err(SequencePlaybackError::TooSoon)
        );
        platform.target = TargetObservation::Launcher;
        next(
            &mut controller,
            &mut platform,
            start + Duration::from_millis(280),
        )
        .unwrap();
        assert_eq!(controller.status(), SequencePlaybackStatus::AwaitTarget);
        assert_eq!(platform.paste_count(), 1);
        platform.foreground = other_target(11);
        platform.target = TargetObservation::Target {
            password_control: false,
        };
        tick(
            &mut controller,
            &mut platform,
            start + Duration::from_millis(380),
        );
        assert_eq!(controller.status(), SequencePlaybackStatus::Completed);
        assert_eq!(platform.pasted_targets, [10, 11]);
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
    fn password_unknown_controls_and_reused_target_identity_refuse_paste() {
        let start = Instant::now();
        for (observation, error) in [
            (
                TargetObservation::Target {
                    password_control: true,
                },
                SequencePlaybackError::PasswordField,
            ),
            (
                TargetObservation::Invalid,
                SequencePlaybackError::InvalidTarget,
            ),
        ] {
            let mut controller =
                ready_controller(prepared_run(PlaybackMode::Automatic, 0, &["secret"]));
            let mut platform = FakePlatform::at(start);
            platform.target = observation;
            assert_eq!(controller.start(&mut platform), Err(error));
            assert_eq!(platform.publication_count(), 0);
            assert_eq!(platform.paste_count(), 0);
        }
        let mut controller =
            ready_controller(prepared_run(PlaybackMode::Automatic, 0, &["secret"]));
        let mut platform = FakePlatform::at(start);
        platform.target = TargetObservation::Unavailable;
        controller.start(&mut platform).unwrap();
        assert_eq!(controller.status(), SequencePlaybackStatus::AwaitTarget);
        tick(
            &mut controller,
            &mut platform,
            start + Duration::from_secs(30),
        );
        assert_eq!(
            controller.status(),
            SequencePlaybackStatus::Failed(SequencePlaybackError::FocusChanged)
        );
        assert_eq!(platform.publication_count(), 0);
        assert_eq!(platform.paste_count(), 0);
        assert!(!controller.needs_tick());
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

    fn other_target(hwnd: isize) -> SequenceTarget {
        SequenceTarget::from_external_window(
            ExternalForegroundWindow {
                hwnd,
                process_id: 21,
                thread_id: 31,
                title: None,
            },
            999,
        )
        .unwrap()
    }

    #[test]
    fn current_target_can_change_a_b_a_between_automatic_and_manual_steps() {
        for mode in [PlaybackMode::Automatic, PlaybackMode::ManualAdvance] {
            let start = Instant::now();
            let mut controller = ready_controller(prepared_run(mode, 0, &["one", "two", "three"]));
            let mut platform = FakePlatform::at(start);
            controller.start(&mut platform).unwrap();
            platform.foreground = other_target(11);
            if mode == PlaybackMode::Automatic {
                tick(
                    &mut controller,
                    &mut platform,
                    start + Duration::from_millis(180),
                );
            } else {
                next(
                    &mut controller,
                    &mut platform,
                    start + Duration::from_millis(180),
                )
                .unwrap();
            }
            platform.foreground = target();
            if mode == PlaybackMode::Automatic {
                tick(
                    &mut controller,
                    &mut platform,
                    start + Duration::from_millis(360),
                );
            } else {
                next(
                    &mut controller,
                    &mut platform,
                    start + Duration::from_millis(360),
                )
                .unwrap();
            }
            assert_eq!(platform.pasted_targets, [10, 11, 10]);
            assert_eq!(platform.clipboard.as_deref(), Some("three"));
            assert_eq!(controller.status(), SequencePlaybackStatus::Completed);
        }
    }

    #[test]
    fn step_local_target_identity_is_rechecked_after_publish_and_immediately_before_input() {
        for before_final_check in [false, true] {
            let start = Instant::now();
            let mut controller =
                ready_controller(prepared_run(PlaybackMode::Automatic, 0, &["do not paste"]));
            let mut platform = FakePlatform::at(start);
            if before_final_check {
                platform.switch_on_final_check = Some(other_target(11));
            } else {
                platform.switch_after_publication = Some(other_target(11));
            }
            assert_eq!(
                controller.start(&mut platform),
                Err(SequencePlaybackError::FocusChanged)
            );
            assert_eq!(platform.publication_count(), 1);
            assert_eq!(platform.paste_count(), 0);
            assert_eq!(controller.current_step_number(), Some((1, 1)));
            tick(
                &mut controller,
                &mut platform,
                start + Duration::from_millis(180),
            );
            assert_eq!(platform.clipboard.as_deref(), Some("user clipboard"));
        }
    }

    #[test]
    fn cancelling_target_wait_invalidates_generation_and_prevents_late_paste() {
        let start = Instant::now();
        let mut controller = ready_controller(prepared_run(PlaybackMode::Automatic, 0, &["never"]));
        let generation = controller.generation;
        let mut platform = FakePlatform::at(start);
        platform.target = TargetObservation::Launcher;
        controller.start(&mut platform).unwrap();
        controller.cancel(start);
        assert_ne!(controller.generation, generation);
        platform.target = TargetObservation::Target {
            password_control: false,
        };
        tick(
            &mut controller,
            &mut platform,
            start + Duration::from_millis(100),
        );
        assert_eq!(platform.paste_count(), 0);
        assert_eq!(platform.publication_count(), 0);
        assert_eq!(controller.status(), SequencePlaybackStatus::Cancelled);
        assert!(!controller.needs_tick());
        controller.begin_preparing().unwrap();
        controller
            .set_ready(prepared_run(PlaybackMode::Automatic, 0, &["new run"]))
            .unwrap();
        controller.start(&mut platform).unwrap();
        assert_eq!(platform.paste_count(), 1);
        assert_eq!(platform.clipboard.as_deref(), Some("new run"));
    }

    #[test]
    fn paste_to_advance_start_only_arms_without_publication_or_input() {
        let start = Instant::now();
        let mut controller = ready_controller(prepared_run(
            PlaybackMode::PasteToAdvance,
            0,
            &["one", "two"],
        ));
        let mut platform = FakePlatform::at(start);
        platform.target = TargetObservation::Launcher;
        controller.start(&mut platform).unwrap();
        assert_eq!(controller.status(), SequencePlaybackStatus::AwaitPaste);
        assert_eq!(controller.current_step_number(), Some((1, 2)));
        assert_eq!(platform.clipboard.as_deref(), Some("user clipboard"));
        assert_eq!(platform.publication_count(), 0);
        assert_eq!(platform.paste_count(), 0);
        assert!(!controller.needs_tick());
        assert_eq!(
            next(&mut controller, &mut platform, start),
            Err(SequencePlaybackError::InvalidState)
        );
        controller.cancel(start);
        assert_eq!(controller.status(), SequencePlaybackStatus::Cancelled);
        assert!(controller.take_shutdown_restore_plan(start).is_none());
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
    pub(crate) fn new(_: Option<usize>) -> Self {
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

    fn current_target(&mut self) -> Result<SequenceTarget, SequencePlaybackError> {
        Err(SequencePlaybackError::FocusChanged)
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
