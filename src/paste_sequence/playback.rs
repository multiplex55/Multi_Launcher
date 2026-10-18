use super::admission::{PasteAdmissionBridge, PhysicalPasteAdmission};
use super::{PlaybackMode, PreparedSequenceRun, SequenceTarget};
use std::sync::Arc;
use std::time::{Duration, Instant};
mod paste_input;

const CLIPBOARD_CONSUMPTION_SETTLE: Duration = Duration::from_millis(180);
const TARGET_RETRY_INTERVAL: Duration = Duration::from_millis(100);
const TARGET_WAIT_LIMIT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SequencePlaybackError {
    Busy,
    InvalidState,
    TooSoon,
    ClipboardUnavailable,
    InvalidTarget,
    FocusChanged,
    PasswordField,
    ClipboardWriteFailed,
    InputRejected,
    ObserverUnavailable,
    RouteConflict,
}

impl SequencePlaybackError {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::Busy => "Another paste sequence is already active.",
            Self::InvalidState => "The paste sequence is not ready for that action.",
            Self::TooSoon => "Wait for the current clipboard step before advancing.",
            Self::ClipboardUnavailable => "The clipboard could not be opened.",
            Self::InvalidTarget => "The current destination window is no longer valid.",
            Self::FocusChanged => {
                "The destination changed before paste; no further steps were pasted."
            }
            Self::PasswordField => "Pasting into a detected password field is blocked.",
            Self::ClipboardWriteFailed => "Sequence text could not be published to the clipboard.",
            Self::InputRejected => "The paste keystroke was rejected by Windows.",
            Self::RouteConflict => {
                "Paste-to-Advance cannot start because Ctrl+V ownership is unavailable; see active shortcut details."
            }
            Self::ObserverUnavailable => {
                "Physical paste input is unavailable. Restart the sequence after the input service is available."
            }
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TargetObservation {
    Target { password_control: bool },
    Launcher,
    Other,
    Invalid,
    Unavailable,
}

/// Minimal platform boundary for the playback state machine. Tests supply a
/// deterministic fake; production only implements these narrow native calls.
pub(crate) trait SequencePlaybackPlatform {
    fn input_is_eligible(&mut self) -> bool {
        true
    }
    fn now(&mut self) -> Instant;
    fn observe_target(&mut self, target: &SequenceTarget) -> TargetObservation;
    /// Capture the actual foreground identity without reading its title.
    fn current_target(&mut self) -> Result<SequenceTarget, SequencePlaybackError>;
    /// Replace the clipboard with this step's text, regardless of its prior contents.
    fn publish_text(&mut self, text: &str) -> Result<(), SequencePlaybackError>;
    fn paste_chord(&mut self) -> Result<(), SequencePlaybackError>;
}

struct PlaybackSession {
    input_lease: Option<SessionInputLease>,
    physical_paste: Option<PendingPhysicalPaste>,
    generation: u64,
    run: PreparedSequenceRun,
    next_step: usize,
    last_step_io_at: Option<Instant>,
    next_step_at: Option<Instant>,
    cleanup_at: Option<Instant>,
    target_wait_deadline: Option<Instant>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StartedSequence {
    pub generation: u64,
    pub mode: PlaybackMode,
}

#[derive(Clone)]
pub(crate) struct SessionInputLease {
    pub owner: u64,
    pub generation: u64,
    pub bridge: Arc<PasteAdmissionBridge>,
}

struct PendingPhysicalPaste {
    admission: PhysicalPasteAdmission,
    bridge: Arc<PasteAdmissionBridge>,
    settle_at: Option<Instant>,
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
    pub(crate) fn started_run(&self) -> Option<StartedSequence> {
        self.session
            .as_ref()
            .filter(|_| {
                matches!(
                    self.status,
                    SequencePlaybackStatus::Running
                        | SequencePlaybackStatus::AwaitAdvance
                        | SequencePlaybackStatus::AwaitTarget
                        | SequencePlaybackStatus::AwaitPaste
                )
            })
            .map(|session| StartedSequence {
                generation: session.generation,
                mode: session.run.mode(),
            })
    }

    pub(crate) fn set_input_lease(&mut self, lease: Option<SessionInputLease>) {
        if let Some(session) = self.session.as_mut() {
            session.input_lease = lease.filter(|lease| lease.generation == session.generation);
        }
    }

    pub(crate) fn has_input_lease(&self) -> bool {
        self.session
            .as_ref()
            .is_some_and(|session| session.input_lease.is_some())
    }

    pub(crate) fn escape_cancel_available(&self) -> bool {
        self.started_run().is_some()
            && self
                .session
                .as_ref()
                .and_then(|session| session.input_lease.as_ref())
                .is_some_and(|lease| lease.bridge.escape_available(lease.owner, lease.generation))
    }
    /// A session generation is published only after Start succeeds. Preflight
    /// and prepared previews never own physical input.
    pub(crate) fn physical_paste_generation(&self) -> Option<u64> {
        self.started_run()
            .filter(|run| run.mode == PlaybackMode::PasteToAdvance)
            .map(|run| run.generation)
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
        self.session.as_ref().is_some_and(|session| {
            session.next_step_at.is_some()
                || session.cleanup_at.is_some()
                || session
                    .physical_paste
                    .as_ref()
                    .is_some_and(|pending| pending.settle_at.is_some())
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

    #[cfg(test)]
    pub(crate) fn start(
        &mut self,
        platform: &mut impl SequencePlaybackPlatform,
    ) -> Result<(), SequencePlaybackError> {
        self.start_with_admission(platform, |_| Ok(None))
    }

    pub(crate) fn prepared_mode(&self) -> Option<PlaybackMode> {
        self.prepared.as_ref().map(PreparedSequenceRun::mode)
    }

    pub(crate) fn start_with_admission(
        &mut self,
        platform: &mut impl SequencePlaybackPlatform,
        admit: impl FnOnce(StartedSequence) -> Result<Option<SessionInputLease>, SequencePlaybackError>,
    ) -> Result<(), SequencePlaybackError> {
        if self.status != SequencePlaybackStatus::Ready {
            return Err(SequencePlaybackError::InvalidState);
        }
        let Some(run) = self.prepared.take() else {
            self.status = SequencePlaybackStatus::Failed(SequencePlaybackError::InvalidState);
            return Err(SequencePlaybackError::InvalidState);
        };
        self.session = Some(PlaybackSession {
            input_lease: None,
            physical_paste: None,
            generation: self.generation,
            run,
            next_step: 0,
            last_step_io_at: None,
            next_step_at: None,
            cleanup_at: None,
            target_wait_deadline: None,
        });
        let physical = self
            .session
            .as_ref()
            .is_some_and(|session| session.run.mode() == PlaybackMode::PasteToAdvance);
        self.status = if physical {
            SequencePlaybackStatus::AwaitPaste
        } else {
            SequencePlaybackStatus::Running
        };
        let started = self
            .started_run()
            .ok_or(SequencePlaybackError::InvalidState)?;
        match admit(started) {
            Ok(lease) => self.set_input_lease(lease),
            Err(error) => {
                self.fail(error, platform.now());
                return Err(error);
            }
        }
        if physical {
            return Ok(());
        }
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
        if let Some(lease) = self
            .session
            .as_ref()
            .and_then(|session| session.input_lease.as_ref())
        {
            lease.bridge.invalidate_owner(lease.owner);
        }
        if let Some(pending) = self
            .session
            .as_ref()
            .and_then(|session| session.physical_paste.as_ref())
        {
            pending.bridge.invalidate_owner(pending.admission.owner);
        }
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
                    if let Some(cleanup_at) = Self::safe_cleanup_deadline(session, now) {
                        session.cleanup_at = Some(cleanup_at);
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
        if self.status == SequencePlaybackStatus::Running
            && self
                .session
                .as_ref()
                .and_then(|session| session.physical_paste.as_ref())
                .and_then(|pending| pending.settle_at)
                .is_some_and(|deadline| deadline <= now)
        {
            self.settle_physical_paste(now);
        }
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
            self.cleanup();
        }
        self.next_wake_after(platform.now())
    }

    pub(crate) fn next_wake_after(&self, now: Instant) -> Option<Duration> {
        let deadline = self
            .session
            .as_ref()
            .into_iter()
            .flat_map(|session| {
                [
                    session.next_step_at,
                    session.cleanup_at,
                    session
                        .physical_paste
                        .as_ref()
                        .and_then(|pending| pending.settle_at),
                ]
            })
            .flatten()
            .min()?;
        Some(deadline.saturating_duration_since(now))
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
        // Configured Next chords can still have their own modifiers held.
        // Existing modes wait boundedly before publication; a physical queued
        // gesture uses its exact target below and must never gain a late retry.
        let target = match if platform.input_is_eligible() {
            platform.current_target()
        } else {
            Err(SequencePlaybackError::FocusChanged)
        } {
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
        self.paste_step_at(platform, &target, None)
    }

    pub(crate) fn physical_paste(
        &mut self,
        admission: PhysicalPasteAdmission,
        bridge: Arc<PasteAdmissionBridge>,
        platform: &mut impl SequencePlaybackPlatform,
    ) -> Result<(), SequencePlaybackError> {
        if !bridge.valid(&admission)
            || self.physical_paste_generation() != Some(admission.generation)
            || self.status != SequencePlaybackStatus::AwaitPaste
            || self
                .session
                .as_ref()
                .is_none_or(|session| session.physical_paste.is_some())
        {
            return Err(SequencePlaybackError::InvalidState);
        }
        let focus = admission.focus;
        let target = SequenceTarget::from_external_window(
            crate::active_window::ExternalForegroundWindow {
                hwnd: focus.hwnd,
                process_id: focus.process_id,
                thread_id: focus.thread_id,
                title: None,
            },
            std::process::id(),
        )
        .map_err(|_| SequencePlaybackError::InvalidTarget)?;
        self.status = SequencePlaybackStatus::Running;
        if let Some(session) = self.session.as_mut() {
            session.physical_paste = Some(PendingPhysicalPaste {
                admission: admission.clone(),
                bridge: Arc::clone(&bridge),
                settle_at: None,
            });
        }
        let result = self.paste_step_at(platform, &target, Some((&admission, &bridge)));
        if let Err(error) = result {
            bridge.invalidate_owner(admission.owner);
            self.fail(error, platform.now());
        }
        result
    }

    fn paste_step_at(
        &mut self,
        platform: &mut impl SequencePlaybackPlatform,
        target: &SequenceTarget,
        physical: Option<(&PhysicalPasteAdmission, &Arc<PasteAdmissionBridge>)>,
    ) -> Result<(), SequencePlaybackError> {
        let input_lease = self
            .session
            .as_ref()
            .and_then(|session| session.input_lease.clone());
        let check_lease = || {
            if physical.is_none_or(|(admission, bridge)| bridge.valid(admission))
                && input_lease.as_ref().is_none_or(|lease| {
                    lease
                        .bridge
                        .session_execution_valid(lease.owner, lease.generation)
                })
            {
                Ok(())
            } else {
                Err(SequencePlaybackError::InvalidState)
            }
        };
        check_lease()?;
        Self::ensure_target(platform, target)?;
        let text = {
            let session = self
                .session
                .as_ref()
                .ok_or(SequencePlaybackError::InvalidState)?;
            session
                .run
                .step_text(session.next_step)
                .ok_or(SequencePlaybackError::InvalidState)?
                .to_owned()
        };

        check_lease()?;
        platform.publish_text(&text)?;
        let published_at = platform.now();
        if let Some(session) = self.session.as_mut() {
            session.last_step_io_at = Some(published_at);
        }

        check_lease()?;
        // Recheck the exact current target immediately before input so a
        // launcher or unrelated foreground transition after publication fails.
        Self::ensure_target(platform, target)?;
        check_lease()?;
        let input_result = platform.paste_chord();
        let pasted_at = platform.now();

        let session = self
            .session
            .as_mut()
            .ok_or(SequencePlaybackError::InvalidState)?;
        session.target_wait_deadline = None;
        session.last_step_io_at = Some(pasted_at);
        input_result?;
        check_lease()?;
        if physical.is_some() {
            if let Some(pending) = session.physical_paste.as_mut() {
                pending.settle_at = Some(pasted_at + CLIPBOARD_CONSUMPTION_SETTLE);
            }
            return Ok(());
        }
        session.next_step += 1;
        if session.next_step >= session.run.step_count() {
            if let Some(lease) = session.input_lease.take() {
                lease
                    .bridge
                    .publish_session(lease.owner, None, Default::default());
            }
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

    fn settle_physical_paste(
        &mut self,
        now: Instant,
    ) {
        let Some(session) = self.session.as_ref() else {
            return;
        };
        let Some(pending) = session.physical_paste.as_ref() else {
            return;
        };
        if !pending.bridge.valid(&pending.admission) {
            pending.bridge.invalidate_owner(pending.admission.owner);
            self.fail(SequencePlaybackError::InvalidState, now);
            return;
        }
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let Some(pending) = session.physical_paste.take() else {
            return;
        };
        session.next_step += 1;
        if session.next_step == session.run.step_count() {
            // Disarm with the old permit still in flight. Releasing its busy
            // slot first would let a fourth gesture be claimed and discarded
            // between two bridge locks.
            pending.bridge.publish(pending.admission.owner, None, true);
            self.status = SequencePlaybackStatus::Completed;
            // Completion relinquishes cleanup ownership. The final published
            // text remains for the next ordinary OS paste; never restore here.
            self.session = None;
        } else {
            pending.bridge.finish(&pending.admission);
            self.status = SequencePlaybackStatus::AwaitPaste;
        }
    }

    fn ensure_target(
        platform: &mut impl SequencePlaybackPlatform,
        target: &SequenceTarget,
    ) -> Result<(), SequencePlaybackError> {
        if !platform.input_is_eligible() {
            return Err(SequencePlaybackError::InputRejected);
        }
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
        if self.status == SequencePlaybackStatus::Cancelled {
            return;
        }
        if self
            .session
            .as_ref()
            .and_then(|session| session.input_lease.as_ref())
            .is_some_and(|lease| lease.bridge.cancellation_for(lease.owner, lease.generation))
        {
            self.cancel(now);
            return;
        }
        if let Some(lease) = self
            .session
            .as_ref()
            .and_then(|session| session.input_lease.as_ref())
        {
            lease.bridge.invalidate_owner(lease.owner);
        }
        self.status = SequencePlaybackStatus::Failed(error);
        let mut drop_session = false;
        if let Some(session) = self.session.as_mut() {
            session.next_step_at = None;
            if let Some(cleanup_at) = Self::safe_cleanup_deadline(session, now) {
                session.cleanup_at = Some(cleanup_at);
            } else {
                drop_session = true;
            }
        }
        if drop_session {
            self.session = None;
        }
    }

    fn safe_cleanup_deadline(session: &PlaybackSession, now: Instant) -> Option<Instant> {
        session
            .last_step_io_at
            .map(|last_io| (last_io + CLIPBOARD_CONSUMPTION_SETTLE).max(now))
    }

    // Keep the consumption cooldown, but never mutate the clipboard on cleanup.
    fn cleanup(&mut self) {
        self.session = None;
    }
}

#[cfg(test)]
mod tests {
    use super::{
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
        Observe,
        Publish,
        Paste,
    }

    struct FakePlatform {
        on_publish: Option<Box<dyn FnOnce()>>,
        input_eligible: bool,
        block_after_publish: bool,
        clock: Instant,
        clipboard: Option<String>,
        target: TargetObservation,
        publish_error: Option<SequencePlaybackError>,
        paste_error: Option<SequencePlaybackError>,
        publish_delay: Duration,
        paste_delay: Duration,
        foreground: SequenceTarget,
        switch_after_publication: Option<SequenceTarget>,
        switch_on_target_observation: Option<(usize, SequenceTarget)>,
        target_observations: usize,
        pasted_targets: Vec<isize>,
        events: Vec<Event>,
    }

    impl FakePlatform {
        fn at(clock: Instant) -> Self {
            Self {
                on_publish: None,
                input_eligible: true,
                block_after_publish: false,
                clock,
                clipboard: Some("user clipboard".into()),
                target: TargetObservation::Target {
                    password_control: false,
                },
                publish_error: None,
                paste_error: None,
                publish_delay: Duration::ZERO,
                paste_delay: Duration::ZERO,
                foreground: target(),
                switch_after_publication: None,
                switch_on_target_observation: None,
                target_observations: 0,
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
            self.clipboard = Some(text.to_owned());
        }
    }

    impl SequencePlaybackPlatform for FakePlatform {
        fn input_is_eligible(&mut self) -> bool {
            self.input_eligible
        }
        fn now(&mut self) -> Instant {
            self.clock
        }

        fn observe_target(&mut self, expected: &SequenceTarget) -> TargetObservation {
            self.target_observations += 1;
            if self
                .switch_on_target_observation
                .as_ref()
                .is_some_and(|(observation, _)| *observation == self.target_observations)
                && let Some((_, target)) = self.switch_on_target_observation.take()
            {
                self.foreground = target;
            }
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

        fn publish_text(&mut self, text: &str) -> Result<(), SequencePlaybackError> {
            self.events.push(Event::Publish);
            self.advance_by(self.publish_delay);
            if let Some(error) = self.publish_error {
                if error == SequencePlaybackError::ClipboardWriteFailed {
                    self.clipboard = None;
                }
                return Err(error);
            }
            self.clipboard = Some(text.to_owned());
            if let Some(on_publish) = self.on_publish.take() {
                on_publish();
            }
            if self.block_after_publish {
                self.input_eligible = false;
            }
            if let Some(target) = self.switch_after_publication.take() {
                self.foreground = target;
            }
            Ok(())
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

    fn physical_fixture() -> (
        SequencePlaybackController,
        FakePlatform,
        std::sync::Arc<crate::paste_sequence::admission::PasteAdmissionBridge>,
        crate::paste_sequence::admission::PhysicalPasteReducer,
        std::sync::mpsc::Receiver<()>,
    ) {
        physical_fixture_for(&["Step 1", "Step 2", "Step 3"])
    }

    fn physical_fixture_for(
        texts: &[&str],
    ) -> (
        SequencePlaybackController,
        FakePlatform,
        std::sync::Arc<crate::paste_sequence::admission::PasteAdmissionBridge>,
        crate::paste_sequence::admission::PhysicalPasteReducer,
        std::sync::mpsc::Receiver<()>,
    ) {
        use crate::paste_sequence::admission::{PasteAdmissionBridge, PhysicalPasteReducer};
        let mut controller =
            ready_controller(prepared_run(PlaybackMode::PasteToAdvance, 0, texts));
        let mut platform = FakePlatform::at(Instant::now());
        let bridge = std::sync::Arc::new(PasteAdmissionBridge::default());
        let (wake, rx) = std::sync::mpsc::channel();
        bridge.install_wake(wake);
        bridge.allow_test_routes();
        bridge.observer_live(true);
        start_ready_physical_run(&mut controller, &mut platform, &bridge);
        let mut reducer = PhysicalPasteReducer::new([false; 9]);
        physical_event(
            &mut reducer,
            &bridge,
            &platform.foreground,
            0xA2,
            crate::hotkey::launcher_invocation::KeyTransition::Down,
        );
        (controller, platform, bridge, reducer, rx)
    }

    fn start_ready_physical_run(
        controller: &mut SequencePlaybackController,
        platform: &mut FakePlatform,
        bridge: &std::sync::Arc<crate::paste_sequence::admission::PasteAdmissionBridge>,
    ) -> u64 {
        use crate::paste_sequence::admission::SessionCapabilities;
        let mut generation = None;
        controller
            .start_with_admission(platform, |run| {
                generation = Some(run.generation);
                bridge
                    .configure_session(
                        4,
                        run.generation,
                        SessionCapabilities {
                            paste: true,
                            escape: false,
                        },
                    )
                    .map_err(|_| super::SequencePlaybackError::ObserverUnavailable)?;
                Ok(Some(super::SessionInputLease {
                    owner: 4,
                    generation: run.generation,
                    bridge: std::sync::Arc::clone(bridge),
                }))
            })
            .unwrap();
        generation.unwrap()
    }

    fn prepare_and_start_physical_run(
        controller: &mut SequencePlaybackController,
        platform: &mut FakePlatform,
        bridge: &std::sync::Arc<crate::paste_sequence::admission::PasteAdmissionBridge>,
        texts: &[&str],
    ) -> u64 {
        controller.begin_preparing().unwrap();
        controller
            .set_ready(prepared_run(PlaybackMode::PasteToAdvance, 0, texts))
            .unwrap();
        start_ready_physical_run(controller, platform, bridge)
    }

    fn physical_event(
        reducer: &mut crate::paste_sequence::admission::PhysicalPasteReducer,
        bridge: &std::sync::Arc<crate::paste_sequence::admission::PasteAdmissionBridge>,
        target: &SequenceTarget,
        vk: u32,
        transition: crate::hotkey::launcher_invocation::KeyTransition,
    ) -> bool {
        use crate::hotkey::launcher_invocation::{InputProvenance, KeyEvent};
        reducer.process(
            KeyEvent {
                vk,
                transition,
                provenance: InputProvenance::Physical,
                at: 0,
            },
            Some(crate::paste_sequence::admission::PasteFocus {
                hwnd: target.hwnd(),
                process_id: target.process_id(),
                thread_id: target.thread_id(),
            }),
            999,
            true,
            bridge,
        )
    }

    fn human_paste(
        reducer: &mut crate::paste_sequence::admission::PhysicalPasteReducer,
        bridge: &std::sync::Arc<crate::paste_sequence::admission::PasteAdmissionBridge>,
        target: &SequenceTarget,
    ) -> crate::paste_sequence::admission::PhysicalPasteAdmission {
        use crate::hotkey::launcher_invocation::KeyTransition;
        assert!(physical_event(
            reducer,
            bridge,
            target,
            0x56,
            KeyTransition::Down
        ));
        assert!(physical_event(
            reducer,
            bridge,
            target,
            0x56,
            KeyTransition::Repeat
        ));
        assert!(physical_event(
            reducer,
            bridge,
            target,
            0x56,
            KeyTransition::Up
        ));
        match bridge.take_notice().unwrap() {
            crate::paste_sequence::admission::PhysicalPasteNotice::Paste(admission) => admission,
            other => panic!("expected admitted paste, got {other:?}"),
        }
    }

    fn physical_ctrl_v(
        reducer: &mut crate::paste_sequence::admission::PhysicalPasteReducer,
        bridge: &std::sync::Arc<crate::paste_sequence::admission::PasteAdmissionBridge>,
        target: &SequenceTarget,
    ) -> crate::paste_sequence::admission::PhysicalPasteAdmission {
        use crate::hotkey::launcher_invocation::KeyTransition;
        assert!(!physical_event(
            reducer,
            bridge,
            target,
            0xA2,
            KeyTransition::Down
        ));
        let admission = human_paste(reducer, bridge, target);
        assert!(!physical_event(
            reducer,
            bridge,
            target,
            0xA2,
            KeyTransition::Up
        ));
        admission
    }

    fn ordinary_ctrl_v(
        reducer: &mut crate::paste_sequence::admission::PhysicalPasteReducer,
        bridge: &std::sync::Arc<crate::paste_sequence::admission::PasteAdmissionBridge>,
        target: &SequenceTarget,
    ) {
        use crate::hotkey::launcher_invocation::KeyTransition;
        for (vk, transition) in [
            (0xA2, KeyTransition::Down),
            (0x56, KeyTransition::Down),
            (0x56, KeyTransition::Up),
            (0xA2, KeyTransition::Up),
        ] {
            assert!(!physical_event(reducer, bridge, target, vk, transition));
        }
        assert!(bridge.take_notice().is_none());
    }

    #[test]
    fn paste_to_advance_three_lazy_cross_app_steps_settle_then_fourth_is_normal_paste() {
        use crate::hotkey::launcher_invocation::KeyTransition;
        use crate::paste_sequence::admission::PhysicalPasteNotice;
        let (mut controller, mut platform, bridge, mut reducer, _wake) = physical_fixture();
        let first_generation = controller.physical_paste_generation().unwrap();
        assert_eq!(platform.publication_count(), 0);
        assert_eq!(platform.clipboard.as_deref(), Some("user clipboard"));
        for index in 0..3 {
            if index == 1 {
                platform.foreground = other_target(40);
            }
            let admission = physical_ctrl_v(&mut reducer, &bridge, &platform.foreground);
            assert_eq!(admission.generation, first_generation);
            controller
                .physical_paste(
                    admission.clone(),
                    std::sync::Arc::clone(&bridge),
                    &mut platform,
                )
                .unwrap();
            assert_eq!(
                controller.current_step_number(),
                Some((index + 1, 3)),
                "index does not advance before settle"
            );
            assert_eq!(
                platform.clipboard.as_deref(),
                Some(format!("Step {}", index + 1).as_str())
            );
            assert!(!physical_event(
                &mut reducer,
                &bridge,
                &platform.foreground,
                0xA2,
                KeyTransition::Down
            ));
            assert!(physical_event(
                &mut reducer,
                &bridge,
                &platform.foreground,
                0x56,
                KeyTransition::Down
            ));
            assert!(physical_event(
                &mut reducer,
                &bridge,
                &platform.foreground,
                0x56,
                KeyTransition::Up
            ));
            assert!(!physical_event(
                &mut reducer,
                &bridge,
                &platform.foreground,
                0xA2,
                KeyTransition::Up
            ));
            assert!(matches!(
                bridge.take_notice(),
                Some(PhysicalPasteNotice::Busy { .. })
            ));
            let at = platform.clock;
            tick(
                &mut controller,
                &mut platform,
                at + Duration::from_millis(179),
            );
            assert_eq!(controller.status(), SequencePlaybackStatus::Running);
            assert!(bridge.valid(&admission));
            assert_eq!(
                platform.publication_count(),
                index + 1,
                "delayed consumer still reads its own step"
            );
            tick(
                &mut controller,
                &mut platform,
                at + Duration::from_millis(180),
            );
            assert!(!bridge.valid(&admission));
            assert_eq!(
                controller.status(),
                if index == 2 {
                    SequencePlaybackStatus::Completed
                } else {
                    SequencePlaybackStatus::AwaitPaste
                }
            );
        }
        assert_eq!(platform.pasted_targets, vec![10, 40, 40]);
        assert_eq!(platform.paste_count(), 3);
        assert_eq!(platform.clipboard.as_deref(), Some("Step 3"));
        assert!(controller.session.is_none());
        ordinary_ctrl_v(&mut reducer, &bridge, &platform.foreground);
        assert_eq!(
            platform.clipboard.as_deref(),
            Some("Step 3"),
            "fourth OS paste retains final text"
        );
        let mut prior_generation = first_generation;
        for run_index in 1..3 {
            let generation = prepare_and_start_physical_run(
                &mut controller,
                &mut platform,
                &bridge,
                &["ALPHA", "BRAVO"],
            );
            assert!(generation > prior_generation);
            prior_generation = generation;
            assert_eq!(controller.status(), SequencePlaybackStatus::AwaitPaste);
            for index in 0..2 {
                if index == 0 {
                    platform.foreground = target();
                } else if index == 1 {
                    platform.foreground = other_target(40 + run_index as isize);
                }
                let admission = physical_ctrl_v(&mut reducer, &bridge, &platform.foreground);
                assert_eq!(admission.generation, generation);
                controller
                    .physical_paste(
                        admission.clone(),
                        std::sync::Arc::clone(&bridge),
                        &mut platform,
                    )
                    .unwrap();
                assert_eq!(
                    platform.clipboard.as_deref(),
                    Some(["ALPHA", "BRAVO"][index])
                );
                let at = platform.clock;
                tick(
                    &mut controller,
                    &mut platform,
                    at + Duration::from_millis(180),
                );
                assert_eq!(
                    controller.status(),
                    if index == 1 {
                        SequencePlaybackStatus::Completed
                    } else {
                        SequencePlaybackStatus::AwaitPaste
                    }
                );
                assert!(!bridge.valid(&admission));
            }
            assert!(controller.session.is_none());
            assert_eq!(platform.clipboard.as_deref(), Some("BRAVO"));
            if run_index == 1 {
                // A completed session leaves the ordinary OS paste available.
                ordinary_ctrl_v(&mut reducer, &bridge, &platform.foreground);
                assert_eq!(platform.clipboard.as_deref(), Some("BRAVO"));
            }
        }

        let cancelled_generation = prepare_and_start_physical_run(
            &mut controller,
            &mut platform,
            &bridge,
            &["ALPHA", "BRAVO"],
        );
        controller.cancel(platform.clock);
        assert_eq!(controller.status(), SequencePlaybackStatus::Cancelled);
        assert!(controller.session.is_none());

        let restarted_generation = prepare_and_start_physical_run(
            &mut controller,
            &mut platform,
            &bridge,
            &["ALPHA", "BRAVO"],
        );
        assert!(restarted_generation > cancelled_generation);
        for (index, text) in ["ALPHA", "BRAVO"].into_iter().enumerate() {
            let admission = physical_ctrl_v(&mut reducer, &bridge, &platform.foreground);
            assert_eq!(admission.generation, restarted_generation);
            controller
                .physical_paste(admission, std::sync::Arc::clone(&bridge), &mut platform)
                .unwrap();
            assert_eq!(platform.clipboard.as_deref(), Some(text));
            let at = platform.clock;
            tick(
                &mut controller,
                &mut platform,
                at + Duration::from_millis(180),
            );
            assert_eq!(
                controller.status(),
                if index == 0 {
                    SequencePlaybackStatus::AwaitPaste
                } else {
                    SequencePlaybackStatus::Completed
                }
            );
        }
        assert!(controller.session.is_none());
    }

    #[test]
    fn paste_to_advance_admitted_target_and_input_owner_are_rechecked_without_rebinding() {
        for boundary in 0..4 {
            let (mut controller, mut platform, bridge, mut reducer, _wake) = physical_fixture();
            let admission = human_paste(&mut reducer, &bridge, &platform.foreground);
            match boundary {
                0 => platform.foreground = other_target(50),
                1 => platform.switch_after_publication = Some(other_target(50)),
                2 => platform.switch_on_target_observation = Some((2, other_target(50))),
                _ => platform.block_after_publish = true,
            }
            assert!(
                controller
                    .physical_paste(admission, std::sync::Arc::clone(&bridge), &mut platform)
                    .is_err()
            );
            assert_eq!(platform.paste_count(), 0);
            assert_eq!(platform.publication_count(), usize::from(boundary != 0));
            let at = platform.clock + Duration::from_millis(180);
            tick(&mut controller, &mut platform, at);
            assert_eq!(
                platform.clipboard.as_deref(),
                if boundary == 0 {
                    Some("user clipboard")
                } else {
                    Some("Step 1")
                }
            );
            assert!(!controller.needs_tick());
        }
    }

    #[test]
    fn paste_to_advance_force_clipboard_overwrites_external_copies_across_two_steps() {
        use crate::hotkey::launcher_invocation::KeyTransition;
        use crate::paste_sequence::admission::PhysicalPasteNotice;

        let (mut controller, mut platform, bridge, mut reducer, _wake) =
            physical_fixture_for(&["ALPHA", "BRAVO"]);
        platform.external_copy("copy after Run, before ALPHA");
        let first = human_paste(&mut reducer, &bridge, &platform.foreground);
        controller
            .physical_paste(first.clone(), std::sync::Arc::clone(&bridge), &mut platform)
            .unwrap();
        assert_eq!(platform.clipboard.as_deref(), Some("ALPHA"));
        assert_eq!(platform.paste_count(), 1);
        assert_eq!(controller.current_step_number(), Some((1, 2)));

        assert!(physical_event(
            &mut reducer,
            &bridge,
            &platform.foreground,
            0x56,
            KeyTransition::Down
        ));
        assert!(physical_event(
            &mut reducer,
            &bridge,
            &platform.foreground,
            0x56,
            KeyTransition::Repeat
        ));
        assert!(physical_event(
            &mut reducer,
            &bridge,
            &platform.foreground,
            0x56,
            KeyTransition::Up
        ));
        assert!(matches!(
            bridge.take_notice(),
            Some(PhysicalPasteNotice::Busy { .. })
        ));

        platform.external_copy("copy during ALPHA settling");
        let at = platform.clock;
        tick(
            &mut controller,
            &mut platform,
            at + Duration::from_millis(179),
        );
        assert_eq!(controller.status(), SequencePlaybackStatus::Running);
        assert_eq!(controller.current_step_number(), Some((1, 2)));
        assert_eq!(platform.clipboard.as_deref(), Some("copy during ALPHA settling"));
        assert!(bridge.valid(&first));
        tick(
            &mut controller,
            &mut platform,
            at + Duration::from_millis(180),
        );
        assert_eq!(controller.status(), SequencePlaybackStatus::AwaitPaste);
        assert_eq!(controller.current_step_number(), Some((2, 2)));

        platform.external_copy("copy after ALPHA, before BRAVO");
        let second = human_paste(&mut reducer, &bridge, &platform.foreground);
        controller
            .physical_paste(second.clone(), std::sync::Arc::clone(&bridge), &mut platform)
            .unwrap();
        assert_eq!(platform.clipboard.as_deref(), Some("BRAVO"));
        assert_eq!(platform.paste_count(), 2);
        assert_eq!(controller.current_step_number(), Some((2, 2)));
        let at = platform.clock;
        tick(
            &mut controller,
            &mut platform,
            at + Duration::from_millis(180),
        );
        assert_eq!(controller.status(), SequencePlaybackStatus::Completed);
        assert_eq!(platform.clipboard.as_deref(), Some("BRAVO"));
        assert_eq!(platform.paste_count(), 2);
        assert!(controller.session.is_none());
        assert!(!physical_event(
            &mut reducer,
            &bridge,
            &platform.foreground,
            0x56,
            KeyTransition::Down
        ));
        assert!(bridge.take_notice().is_none());
    }

    #[test]
    fn paste_to_advance_cancel_and_input_failure_hold_cleanup_until_safe_boundary() {
        for failure in [false, true] {
            let (mut controller, mut platform, bridge, mut reducer, _wake) = physical_fixture();
            let admission = human_paste(&mut reducer, &bridge, &platform.foreground);
            if failure {
                platform.paste_error = Some(SequencePlaybackError::InputRejected);
            }
            let result =
                controller.physical_paste(admission, std::sync::Arc::clone(&bridge), &mut platform);
            if failure {
                assert_eq!(result, Err(SequencePlaybackError::InputRejected));
            } else {
                result.unwrap();
                assert_eq!(
                    controller.begin_preparing(),
                    Err(SequencePlaybackError::Busy)
                );
                controller.cancel(platform.clock);
            }
            let at = platform.clock;
            tick(
                &mut controller,
                &mut platform,
                at + Duration::from_millis(179),
            );
            assert_eq!(platform.clipboard.as_deref(), Some("Step 1"));
            tick(
                &mut controller,
                &mut platform,
                at + Duration::from_millis(180),
            );
            assert_eq!(platform.clipboard.as_deref(), Some("Step 1"));
            assert!(!controller.needs_tick());
        }
        let (mut controller, mut platform, bridge, mut reducer, _wake) = physical_fixture();
        let admission = human_paste(&mut reducer, &bridge, &platform.foreground);
        platform.publish_error = Some(SequencePlaybackError::ClipboardWriteFailed);
        assert_eq!(
            controller.physical_paste(admission, bridge, &mut platform),
            Err(SequencePlaybackError::ClipboardWriteFailed)
        );
        let at = platform.clock + Duration::from_millis(180);
        tick(&mut controller, &mut platform, at);
        assert_eq!(platform.clipboard, None);
    }

    #[test]
    fn session_escape_arms_after_start_before_first_publication_in_every_mode() {
        use crate::hotkey::launcher_invocation::{InputProvenance, KeyEvent, KeyTransition};
        use crate::paste_sequence::admission::{
            PasteAdmissionBridge, PhysicalPasteReducer, SessionCapabilities,
        };
        for mode in [
            PlaybackMode::Automatic,
            PlaybackMode::ManualAdvance,
            PlaybackMode::PasteToAdvance,
        ] {
            let bridge = std::sync::Arc::new(PasteAdmissionBridge::default());
            let (wake, _rx) = std::sync::mpsc::channel();
            bridge.install_wake(wake);
            bridge.allow_test_routes();
            bridge.observer_live(true);
            let mut controller = ready_controller(prepared_run(mode, 0, &["one", "two"]));
            assert!(controller.started_run().is_none());
            let mut platform = FakePlatform::at(Instant::now());
            let cancel_bridge = std::sync::Arc::clone(&bridge);
            platform.on_publish = Some(Box::new(move || {
                let mut reducer = PhysicalPasteReducer::new([false; 9]);
                assert!(reducer.process(
                    KeyEvent {
                        vk: 0x1B,
                        transition: KeyTransition::Down,
                        provenance: InputProvenance::Physical,
                        at: 0
                    },
                    Some(crate::paste_sequence::admission::PasteFocus {
                        hwnd: 99,
                        process_id: 999,
                        thread_id: 90
                    }),
                    999,
                    true,
                    &cancel_bridge
                ));
            }));
            let result = controller.start_with_admission(&mut platform, |run| {
                bridge
                    .configure_session(
                        4,
                        run.generation,
                        SessionCapabilities {
                            paste: mode == PlaybackMode::PasteToAdvance,
                            escape: true,
                        },
                    )
                    .unwrap();
                Ok(Some(super::SessionInputLease {
                    owner: 4,
                    generation: run.generation,
                    bridge: std::sync::Arc::clone(&bridge),
                }))
            });
            if mode == PlaybackMode::PasteToAdvance {
                result.unwrap();
                let mut reducer = PhysicalPasteReducer::new([false; 9]);
                physical_event(
                    &mut reducer,
                    &bridge,
                    &platform.foreground,
                    0xA2,
                    KeyTransition::Down,
                );
                let admission = human_paste(&mut reducer, &bridge, &platform.foreground);
                assert!(
                    controller
                        .physical_paste(admission, std::sync::Arc::clone(&bridge), &mut platform)
                        .is_err()
                );
            } else {
                assert!(result.is_err());
            }
            assert_eq!(controller.status(), SequencePlaybackStatus::Cancelled);
            assert_eq!(
                platform.paste_count(),
                0,
                "accepted Esc between publication and injection fences the first step"
            );
            let at = platform.clock + Duration::from_millis(180);
            tick(&mut controller, &mut platform, at);
            assert_eq!(platform.clipboard.as_deref(), Some("one"));
            assert!(!controller.needs_tick());
        }
    }

    #[test]
    fn session_escape_await_target_and_settling_cancel_preserve_external_clipboard() {
        use crate::hotkey::launcher_invocation::KeyTransition;
        use crate::paste_sequence::admission::{
            PasteAdmissionBridge, PhysicalPasteNotice, PhysicalPasteReducer, SessionCapabilities,
        };
        for mode in [
            PlaybackMode::Automatic,
            PlaybackMode::ManualAdvance,
            PlaybackMode::PasteToAdvance,
        ] {
            let bridge = std::sync::Arc::new(PasteAdmissionBridge::default());
            let (wake, _rx) = std::sync::mpsc::channel();
            bridge.install_wake(wake);
            bridge.allow_test_routes();
            bridge.observer_live(true);
            let mut controller = ready_controller(prepared_run(mode, 0, &["one", "two"]));
            let mut platform = FakePlatform::at(Instant::now());
            if mode != PlaybackMode::PasteToAdvance {
                platform.target = TargetObservation::Unavailable;
            }
            controller
                .start_with_admission(&mut platform, |run| {
                    bridge
                        .configure_session(
                            4,
                            run.generation,
                            SessionCapabilities {
                                paste: mode == PlaybackMode::PasteToAdvance,
                                escape: true,
                            },
                        )
                        .unwrap();
                    Ok(Some(super::SessionInputLease {
                        owner: 4,
                        generation: run.generation,
                        bridge: std::sync::Arc::clone(&bridge),
                    }))
                })
                .unwrap();
            let mut reducer = PhysicalPasteReducer::new([false; 9]);
            if mode == PlaybackMode::PasteToAdvance {
                physical_event(
                    &mut reducer,
                    &bridge,
                    &platform.foreground,
                    0xA2,
                    KeyTransition::Down,
                );
                let admission = human_paste(&mut reducer, &bridge, &platform.foreground);
                controller
                    .physical_paste(admission, std::sync::Arc::clone(&bridge), &mut platform)
                    .unwrap();
            } else {
                assert_eq!(controller.status(), SequencePlaybackStatus::AwaitTarget);
            }
            platform.foreground = other_target(88);
            assert!(physical_event(
                &mut reducer,
                &bridge,
                &platform.foreground,
                0x1B,
                KeyTransition::Down
            ));
            let Some(PhysicalPasteNotice::Cancel { generation, .. }) = bridge.take_notice() else {
                panic!("expected cancellation")
            };
            assert_eq!(controller.started_run().unwrap().generation, generation);
            controller.cancel(platform.clock);
            platform.external_copy("external during cancellation");
            let at = platform.clock + Duration::from_millis(500);
            tick(&mut controller, &mut platform, at);
            assert_eq!(
                platform.clipboard.as_deref(),
                Some("external during cancellation")
            );
            assert_eq!(
                platform.paste_count(),
                usize::from(mode == PlaybackMode::PasteToAdvance)
            );
            assert!(!controller.needs_tick());
        }
    }

    #[test]
    fn paste_playback_existing_modes_wait_for_modifier_release_without_skipping_step() {
        for mode in [PlaybackMode::Automatic, PlaybackMode::ManualAdvance] {
            let mut controller = ready_controller(prepared_run(mode, 0, &["one", "two"]));
            let mut platform = FakePlatform::at(Instant::now());
            platform.input_eligible = false;
            controller.start(&mut platform).unwrap();
            assert_eq!(controller.status(), SequencePlaybackStatus::AwaitTarget);
            assert_eq!(platform.publication_count(), 0);
            assert_eq!(platform.paste_count(), 0);
            platform.input_eligible = true;
            let first = platform.clock + Duration::from_millis(100);
            tick(&mut controller, &mut platform, first);
            assert_eq!(platform.clipboard.as_deref(), Some("one"));
            assert_eq!(platform.paste_count(), 1);
            platform.input_eligible = false;
            let next_at = first + Duration::from_millis(180);
            platform.advance_to(next_at);
            if mode == PlaybackMode::ManualAdvance {
                controller.next(&mut platform, next_at).unwrap();
            } else {
                controller.tick(&mut platform, next_at);
            }
            assert_eq!(controller.status(), SequencePlaybackStatus::AwaitTarget);
            assert_eq!(
                platform.publication_count(),
                1,
                "held modifiers cannot stage or skip the next step"
            );
            assert_eq!(platform.paste_count(), 1);
            platform.input_eligible = true;
            tick(
                &mut controller,
                &mut platform,
                next_at + Duration::from_millis(100),
            );
            assert_eq!(platform.paste_count(), 2);
            assert_eq!(platform.clipboard.as_deref(), Some("two"));
        }
    }

    #[test]
    fn paste_to_advance_open_and_write_failures_never_send_or_republish() {
        for error in [
            SequencePlaybackError::ClipboardUnavailable,
            SequencePlaybackError::ClipboardWriteFailed,
        ] {
            let (mut controller, mut platform, bridge, mut reducer, _wake) = physical_fixture();
            let admission = human_paste(&mut reducer, &bridge, &platform.foreground);
            platform.publish_error = Some(error);
            assert!(
                controller
                    .physical_paste(admission, std::sync::Arc::clone(&bridge), &mut platform)
                    .is_err()
            );
            assert_eq!(platform.paste_count(), 0);
            assert!(controller.session.is_none());
            assert_eq!(platform.publication_count(), 1);
            assert_eq!(
                platform.clipboard.as_deref(),
                if error == SequencePlaybackError::ClipboardUnavailable {
                    Some("user clipboard")
                } else {
                    None
                }
            );
            platform.external_copy("external after unknown publication");
            let at = platform.clock + Duration::from_millis(500);
            tick(&mut controller, &mut platform, at);
            assert_eq!(
                platform.clipboard.as_deref(),
                Some("external after unknown publication")
            );
            assert_eq!(platform.publication_count(), 1);
            assert!(!controller.needs_tick());
        }
    }

    #[test]
    fn paste_to_advance_checked_arm_rejects_unavailable_observer() {
        let bridge = crate::paste_sequence::admission::PasteAdmissionBridge::default();
        assert!(bridge.arm(4, 7).is_err());
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
            assert_eq!(platform.clipboard.as_deref(), Some(expected[3]));
        }
    }

    #[test]
    fn shutdown_cancellation_leaves_latest_clipboard_untouched() {
        for external in [false, true] {
            let start = Instant::now();
            let mut controller = ready_controller(prepared_run(
                PlaybackMode::ManualAdvance,
                0,
                &["sequence text", "never"],
            ));
            let mut platform = FakePlatform::at(start);
            controller.start(&mut platform).unwrap();
            if external {
                platform.external_copy("new user data");
            }
            let expected = platform.clipboard.clone();
            controller.cancel(start);
            tick(
                &mut controller,
                &mut platform,
                start + Duration::from_secs(1),
            );
            assert_eq!(platform.clipboard, expected);
            assert_eq!(platform.paste_count(), 1);
            assert!(!controller.needs_tick());
        }
    }

    #[test]
    fn automatic_playback_obeys_deadlines_and_retains_final_text() {
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
        platform.external_copy("interim copy before automatic second step");
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
        assert_eq!(platform.clipboard.as_deref(), Some("second"));
        assert!(!controller.needs_tick());
        tick(
            &mut controller,
            &mut platform,
            start + Duration::from_secs(1),
        );
        assert!(!controller.needs_tick());
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
        assert_eq!(platform.clipboard.as_deref(), Some("second"));
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
        platform.external_copy("interim copy before manual Next");
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
        platform.external_copy("interim copy before manual second publication");
        tick(
            &mut controller,
            &mut platform,
            start + Duration::from_millis(380),
        );
        assert_eq!(controller.status(), SequencePlaybackStatus::Completed);
        assert_eq!(platform.pasted_targets, [10, 11]);
    }

    #[test]
    fn cancellation_stops_future_steps_and_retains_last_text() {
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
        assert_eq!(platform.clipboard.as_deref(), Some("first"));
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
    fn identical_external_clipboard_content_is_overwritten_on_manual_advance() {
        let start = Instant::now();
        let mut controller = ready_controller(prepared_run(
            PlaybackMode::ManualAdvance,
            0,
            &["same", "next"],
        ));
        let mut platform = FakePlatform::at(start);
        controller.start(&mut platform).unwrap();
        platform.external_copy("same");
        next(
            &mut controller,
            &mut platform,
            start + Duration::from_secs(1),
        )
        .unwrap();
        assert_eq!(platform.clipboard.as_deref(), Some("next"));
        assert_eq!(platform.publication_count(), 2);
        assert_eq!(platform.paste_count(), 2);
        assert_eq!(controller.status(), SequencePlaybackStatus::Completed);
    }

    #[test]
    fn clipboard_open_and_input_failures_never_run_future_steps_and_cleanup_once() {
        let start = Instant::now();
        let mut unavailable = ready_controller(prepared_run(PlaybackMode::Automatic, 0, &["one"]));
        let mut unavailable_platform = FakePlatform::at(start);
        unavailable_platform.publish_error = Some(SequencePlaybackError::ClipboardUnavailable);
        assert_eq!(
            unavailable.start(&mut unavailable_platform),
            Err(SequencePlaybackError::ClipboardUnavailable)
        );
        assert_eq!(unavailable_platform.publication_count(), 1);
        assert_eq!(unavailable_platform.paste_count(), 0);
        assert_eq!(unavailable_platform.clipboard.as_deref(), Some("user clipboard"));
        assert!(!unavailable.needs_tick());

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
        assert_eq!(input_platform.clipboard.as_deref(), Some("one"));
        assert_eq!(input_platform.paste_count(), 1);
        assert!(!input_failure.needs_tick());
    }

    #[test]
    fn failed_clipboard_write_does_not_restore_original_or_retry() {
        let start = Instant::now();
        let mut controller = ready_controller(prepared_run(PlaybackMode::Automatic, 0, &["one"]));
        let mut platform = FakePlatform::at(start);
        platform.publish_error = Some(SequencePlaybackError::ClipboardWriteFailed);
        assert_eq!(
            controller.start(&mut platform),
            Err(SequencePlaybackError::ClipboardWriteFailed)
        );
        assert_eq!(platform.clipboard, None);
        assert_eq!(
            controller.status(),
            SequencePlaybackStatus::Failed(SequencePlaybackError::ClipboardWriteFailed)
        );
        tick(
            &mut controller,
            &mut platform,
            start + Duration::from_millis(180),
        );
        assert_eq!(platform.clipboard.as_deref(), None);
        assert!(!controller.needs_tick());
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
                platform.switch_on_target_observation = Some((2, other_target(11)));
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
            assert_eq!(platform.clipboard.as_deref(), Some("do not paste"));
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
    fn non_text_clipboard_is_not_a_startup_requirement_and_modes_overwrite_new_copies() {
        for mode in [
            PlaybackMode::Automatic,
            PlaybackMode::ManualAdvance,
            PlaybackMode::PasteToAdvance,
        ] {
            let start = Instant::now();
            let mut controller = ready_controller(prepared_run(mode, 0, &["one", "two"]));
            let mut platform = FakePlatform::at(start);
            platform.clipboard = None;
            platform.target = TargetObservation::Launcher;
            controller.start(&mut platform).unwrap();
            assert_eq!(platform.publication_count(), 0);
            assert!(controller.started_run().is_some());
            platform.external_copy("new copy after Run");
            platform.target = TargetObservation::Target {
                password_control: false,
            };
            if mode != PlaybackMode::PasteToAdvance {
                tick(
                    &mut controller,
                    &mut platform,
                    start + Duration::from_millis(100),
                );
                assert_eq!(platform.clipboard.as_deref(), Some("one"));
                assert_eq!(platform.paste_count(), 1);
                assert_eq!(
                    controller.status(),
                    if mode == PlaybackMode::Automatic {
                        SequencePlaybackStatus::Running
                    } else {
                        SequencePlaybackStatus::AwaitAdvance
                    }
                );
            }
            controller.cancel(platform.clock);
            assert_eq!(
                platform.clipboard.as_deref(),
                if mode == PlaybackMode::PasteToAdvance {
                    Some("new copy after Run")
                } else {
                    Some("one")
                }
            );
        }
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
        assert!(controller.session.is_none());
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
}

#[cfg(not(windows))]
impl SequencePlaybackPlatform for NativeSequencePlaybackPlatform {
    fn now(&mut self) -> Instant {
        Instant::now()
    }

    fn observe_target(&mut self, _: &SequenceTarget) -> TargetObservation {
        TargetObservation::Unavailable
    }

    fn current_target(&mut self) -> Result<SequenceTarget, SequencePlaybackError> {
        Err(SequencePlaybackError::FocusChanged)
    }

    fn publish_text(&mut self, _: &str) -> Result<(), SequencePlaybackError> {
        Err(SequencePlaybackError::ClipboardUnavailable)
    }

    fn paste_chord(&mut self) -> Result<(), SequencePlaybackError> {
        Err(SequencePlaybackError::InputRejected)
    }
}
