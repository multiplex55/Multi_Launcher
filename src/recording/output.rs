use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

use tempfile::{Builder, TempPath};

use super::{
    CaptureTerminal, RecorderState, RecordingFormat, RecordingSession, RecordingSessionError,
    SinkReceipt,
};

/// A same-directory temporary file owned by one recording transaction.
///
/// The file handle is returned separately so encoders can own it while this
/// stage keeps the path alive for a later no-clobber publication.
pub(crate) struct OutputStage {
    destination: PathBuf,
    temporary_path: TempPath,
}

/// An output stage after its encoder has closed and the file has been synced.
/// Only `publish_finalized` can consume it, so sibling encoders cannot bypass
/// the session's finalization permit.
pub(crate) struct FinalizedOutputStage(OutputStage);

#[derive(Debug)]
pub(crate) struct StageError {
    message: String,
}

impl StageError {
    pub(crate) fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for StageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for StageError {}

impl OutputStage {
    pub(crate) fn create(
        destination: impl AsRef<Path>,
        format: RecordingFormat,
    ) -> io::Result<(Self, File)> {
        let destination = anchor_destination(destination.as_ref())?;
        let extension = match format {
            RecordingFormat::Gif => "gif",
            RecordingFormat::Mp4 => "mp4",
        };
        if !destination
            .extension()
            .is_some_and(|value| value.eq_ignore_ascii_case(extension))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("recording destination must use the .{extension} extension"),
            ));
        }
        let parent = destination
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let suffix = format!(".{extension}");
        let named = Builder::new()
            .prefix(".multi-launcher-recording-")
            .suffix(&suffix)
            .tempfile_in(parent)?;
        let (file, temporary_path) = named.into_parts();
        Ok((
            Self {
                destination,
                temporary_path,
            },
            file,
        ))
    }

    pub(crate) fn temporary_path(&self) -> &Path {
        self.temporary_path.as_ref()
    }

    /// Completes staging after the encoder has flushed and closed its writer.
    ///
    /// A sync failure closes the handle before attempting cleanup so Windows
    /// can remove the owned stage too.
    pub(crate) fn finalize_file(self, file: File) -> Result<FinalizedOutputStage, StageError> {
        match file.sync_all() {
            Ok(()) => {
                drop(file);
                Ok(FinalizedOutputStage(self))
            }
            Err(error) => {
                drop(file);
                Err(self.cleanup_after(format!("could not sync staged recording: {error}")))
            }
        }
    }

    /// Publishes the closed and synced stage without replacing an existing
    /// destination. A failed move removes only this transaction's temporary
    /// file and reports its exact path if cleanup also fails.
    fn publish(self) -> Result<PathBuf, StageError> {
        let Self {
            destination,
            temporary_path,
        } = self;
        match temporary_path.persist_noclobber(&destination) {
            Ok(()) => Ok(destination),
            Err(error) => {
                let message = error.error.to_string();
                let path = error.path.to_path_buf();
                match error.path.close() {
                    Ok(()) => Err(StageError {
                        message: format!(
                            "could not publish recording to {}: {message}; staged file was removed",
                            destination.display()
                        ),
                    }),
                    Err(cleanup) => Err(StageError {
                        message: format!(
                            "could not publish recording to {}: {message}; owned partial file remains at {}: {cleanup}",
                            destination.display(),
                            path.display()
                        ),
                    }),
                }
            }
        }
    }

    /// Explicitly removes this transaction's stage, returning cleanup failure
    /// with the path that may still need attention.
    pub(crate) fn discard(self) -> Result<(), StageError> {
        let Self { temporary_path, .. } = self;
        let path = temporary_path.to_path_buf();
        temporary_path.close().map_err(|error| StageError {
            message: format!("could not remove stage at {}: {error}", path.display()),
        })
    }

    fn cleanup_after(self, primary: String) -> StageError {
        match self.discard() {
            Ok(()) => StageError { message: primary },
            Err(cleanup) => StageError {
                message: format!("{primary}; {}", cleanup.message()),
            },
        }
    }
}

impl FinalizedOutputStage {
    pub(crate) fn discard(self) -> Result<(), StageError> {
        self.0.discard()
    }
}

fn anchor_destination(destination: &Path) -> io::Result<PathBuf> {
    if destination.as_os_str().as_encoded_bytes().contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "recording destination contains a NUL byte",
        ));
    }
    #[cfg(windows)]
    if !destination.is_absolute()
        && matches!(
            destination.components().next(),
            Some(std::path::Component::Prefix(_))
        )
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "recording destination must not be drive-relative",
        ));
    }
    std::path::absolute(destination)
}

/// Crosses the existing session-owned publication boundary after checking the
/// exact terminal and all acknowledged consumer leases. The physical move is
/// deliberately outside the session lock.
pub(crate) fn publish_finalized(
    session: &RecordingSession,
    generation: u64,
    terminal: CaptureTerminal,
    acknowledged_inputs: u64,
    stage: FinalizedOutputStage,
) -> Result<PathBuf, RecordingOutputError> {
    let view = session.view();
    if view.generation() != generation || terminal.generation() != generation {
        return Err(discard_with_error(
            stage,
            RecordingOutputError::Session(RecordingSessionError::StaleGeneration),
        ));
    }
    if view.terminal() != Some(terminal)
        || !matches!(view.state(), RecorderState::Finalizing { .. })
    {
        return Err(discard_with_error(
            stage,
            RecordingOutputError::Session(RecordingSessionError::NotFinalizing),
        ));
    }
    if terminal.accepted_frames() != view.accepted_frames() || acknowledged_inputs == 0 {
        return Err(discard_with_error(
            stage,
            RecordingOutputError::ReceiptMismatch,
        ));
    }

    let mut permit = match session.claim_finalization(generation) {
        Ok(permit) => permit,
        Err(error) => {
            return Err(discard_with_error(
                stage,
                RecordingOutputError::Session(error),
            ));
        }
    };

    if permit.frames_written() != acknowledged_inputs {
        let _ = permit.fail("staged frame acknowledgements do not match the frozen recording");
        return Err(discard_with_error(
            stage,
            RecordingOutputError::ReceiptMismatch,
        ));
    }

    match stage.0.publish() {
        Ok(destination) => {
            session
                .complete_finalization(
                    &mut permit,
                    SinkReceipt::new(generation, acknowledged_inputs),
                )
                .map_err(RecordingOutputError::PublishedButUnsettled)?;
            Ok(destination)
        }
        Err(error) => {
            let message = error.to_string();
            let _ = permit.fail(message.clone());
            Err(RecordingOutputError::Stage(message))
        }
    }
}

fn discard_with_error(
    stage: FinalizedOutputStage,
    error: RecordingOutputError,
) -> RecordingOutputError {
    match stage.0.discard() {
        Ok(()) => error,
        Err(cleanup) => RecordingOutputError::WithCleanup {
            cause: error.to_string(),
            cleanup: cleanup.to_string(),
        },
    }
}

#[derive(Debug)]
pub(crate) enum RecordingOutputError {
    Stage(String),
    Encoding(String),
    Session(RecordingSessionError),
    InvalidTransition,
    UnsupportedFormat,
    InvalidFrame(&'static str),
    InvalidRgbaLength { expected: usize, actual: usize },
    TimestampMovedBackwards,
    FrameAfterTerminal,
    TimestampOverflow,
    EmptyClip,
    AllocationFailed,
    ReceiptMismatch,
    PublishedButUnsettled(RecordingSessionError),
    WithCleanup { cause: String, cleanup: String },
}

impl std::fmt::Display for RecordingOutputError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stage(message) => formatter.write_str(message),
            Self::Encoding(message) => write!(formatter, "GIF encoding failed: {message}"),
            Self::Session(error) => error.fmt(formatter),
            Self::InvalidTransition => formatter.write_str("recording output is no longer active"),
            Self::UnsupportedFormat => {
                formatter.write_str("this recording output only accepts GIF requests")
            }
            Self::InvalidFrame(message) => formatter.write_str(message),
            Self::InvalidRgbaLength { expected, actual } => write!(
                formatter,
                "RGBA frame has {actual} bytes; expected {expected}"
            ),
            Self::TimestampMovedBackwards => {
                formatter.write_str("captured frame timestamps moved backwards")
            }
            Self::FrameAfterTerminal => {
                formatter.write_str("captured frame timestamp is after the terminal time")
            }
            Self::TimestampOverflow => {
                formatter.write_str("recording timestamp or frame count exceeds the supported range")
            }
            Self::EmptyClip => formatter.write_str("cannot publish a GIF with no captured frames"),
            Self::AllocationFailed => {
                formatter.write_str("could not allocate bounded GIF quantization scratch")
            }
            Self::ReceiptMismatch => {
                formatter.write_str("staged frame acknowledgements do not match the recording")
            }
            Self::PublishedButUnsettled(error) => write!(
                formatter,
                "recording was published, but the session rejected its receipt: {error}"
            ),
            Self::WithCleanup { cause, cleanup } => write!(formatter, "{cause}; {cleanup}"),
        }
    }
}

impl std::error::Error for RecordingOutputError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::ScreenRect;
    use crate::recording::{
        CanvasSize, CapturePoll, CaptureSource, CaptureStopToken, CaptureTarget, FrameRate,
        RecorderClock, RecordingBackend, RecordingEndReason, RecordingOptions, RecordingRequest,
        RgbaFrame,
    };
    use std::io::Write;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex, Weak};
    use std::thread;
    use std::time::{Duration, Instant};

    struct TestSource {
        generation: u64,
        canvas: CanvasSize,
        ended: bool,
    }

    impl CaptureSource for TestSource {
        fn poll(&mut self, _max_wait: Duration, _stop: &CaptureStopToken) -> CapturePoll {
            if self.ended {
                CapturePoll::Ended(RecordingEndReason::TargetLost)
            } else {
                self.ended = true;
                CapturePoll::Frame {
                    generation: self.generation,
                    frame: RgbaFrame::new(
                        self.canvas,
                        vec![80, 120, 160, 255].repeat(self.canvas.rgba_bytes() / 4),
                    )
                    .unwrap(),
                }
            }
        }
    }

    struct TestBackend;

    impl RecordingBackend for TestBackend {
        fn create_source(
            &self,
            request: &RecordingRequest,
            generation: u64,
        ) -> Result<Box<dyn CaptureSource>, String> {
            Ok(Box::new(TestSource {
                generation,
                canvas: request.canvas(),
                ended: false,
            }))
        }
    }

    struct TestClock;

    impl RecorderClock for TestClock {
        fn now(&self) -> Duration {
            Duration::ZERO
        }
    }

    fn one_frame_finalizing_session(
        close_after_claim: Arc<AtomicBool>,
    ) -> (Arc<RecordingSession>, u64, CaptureTerminal, Arc<AtomicBool>) {
        let owner = Arc::new(Mutex::new(None::<Weak<RecordingSession>>));
        let wake_owner = Arc::clone(&owner);
        let close_observed = Arc::new(AtomicBool::new(false));
        let wake_close_observed = Arc::clone(&close_observed);
        let claim_thread = thread::current().id();
        let wake_claim_thread = claim_thread;
        let session = Arc::new(RecordingSession::new(
            Arc::new(TestClock),
            Arc::new(TestBackend),
            move || {
                let session = wake_owner.lock().unwrap().as_ref().and_then(Weak::upgrade);
                if let Some(session) = session
                    && close_after_claim.load(Ordering::SeqCst)
                    && thread::current().id() == wake_claim_thread
                    && session.view().finalization_claimed()
                    && close_after_claim.swap(false, Ordering::SeqCst)
                {
                    session.close();
                    wake_close_observed.store(true, Ordering::SeqCst);
                }
            },
        ));
        *owner.lock().unwrap() = Some(Arc::downgrade(&session));

        let bounds = ScreenRect::new(0, 0, 2, 2);
        let target = CaptureTarget::window(1, 1, 1, bounds).unwrap();
        let options = RecordingOptions::new(
            RecordingFormat::Gif,
            FrameRate::new(30).unwrap(),
            crate::recording::RecordingDurationLimit::Timed(Duration::from_secs(1)),
            false,
        )
        .unwrap();
        let request = RecordingRequest::new(target, bounds, options).unwrap();
        let generation = session.select(request).unwrap();
        session.start().unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        let terminal = loop {
            session.poll().unwrap();
            let view = session.view();
            if matches!(view.state(), RecorderState::Finalizing { .. }) && !view.worker_busy() {
                break view.terminal().unwrap();
            }
            assert!(
                Instant::now() < deadline,
                "session did not finalize: {view:?}"
            );
            thread::yield_now();
        };
        session
            .take_latest_frame(generation)
            .unwrap()
            .expect("fake source publishes one frame")
            .acknowledge_written()
            .unwrap();
        (session, generation, terminal, close_observed)
    }

    fn finalized_stage(destination: &Path) -> FinalizedOutputStage {
        let (stage, mut file) = OutputStage::create(destination, RecordingFormat::Gif).unwrap();
        file.write_all(b"finished gif bytes").unwrap();
        file.flush().unwrap();
        stage.finalize_file(file).unwrap()
    }

    #[test]
    fn recording_output_stage_uses_destination_directory_and_extension() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("capture.gif");
        let (stage, mut file) = OutputStage::create(&destination, RecordingFormat::Gif).unwrap();
        assert_eq!(stage.temporary_path().parent(), Some(directory.path()));
        assert!(
            stage
                .temporary_path()
                .file_name()
                .unwrap()
                .to_string_lossy()
                .ends_with(".gif")
        );
        file.write_all(b"stage").unwrap();
        drop(file);
        stage.discard().unwrap();
        assert!(!destination.exists());
    }

    #[test]
    fn recording_output_stage_anchors_relative_destinations_and_rejects_nul() {
        assert!(
            anchor_destination(Path::new("relative.gif"))
                .unwrap()
                .is_absolute()
        );

        assert_eq!(
            OutputStage::create("bad\0name.gif", RecordingFormat::Gif)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }

    #[test]
    fn recording_output_discards_staged_file_when_cancel_wins_before_claim() {
        let (session, generation, terminal, _) =
            one_frame_finalizing_session(Arc::new(AtomicBool::new(false)));
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("cancelled.gif");
        let stage = finalized_stage(&destination);
        assert!(session.cancel().unwrap());

        assert!(matches!(
            publish_finalized(&session, generation, terminal, 1, stage),
            Err(RecordingOutputError::Session(
                RecordingSessionError::StaleGeneration
            ))
        ));
        assert!(!destination.exists());
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
        assert_eq!(session.view().state(), &RecorderState::Cancelled);
    }

    #[test]
    fn recording_output_claim_survives_close_and_publishes_exact_receipt() {
        let close_after_claim = Arc::new(AtomicBool::new(false));
        let (session, generation, terminal, close_observed) =
            one_frame_finalizing_session(Arc::clone(&close_after_claim));
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("claimed.gif");
        let stage = finalized_stage(&destination);
        close_after_claim.store(true, Ordering::SeqCst);

        assert_eq!(
            publish_finalized(&session, generation, terminal, 1, stage).unwrap(),
            destination
        );
        assert_eq!(std::fs::read(&destination).unwrap(), b"finished gif bytes");
        assert!(
            close_observed.load(Ordering::SeqCst),
            "claim wake closed the session"
        );
        assert!(!close_after_claim.load(Ordering::SeqCst));
        assert!(matches!(
            session.view().state(),
            RecorderState::Completed(_)
        ));
        assert!(!session.view().finalization_claimed());
        assert_eq!(session.start(), Err(RecordingSessionError::Closed));
    }

    #[test]
    fn recording_output_compares_acknowledgements_with_frozen_claim_before_move() {
        let (session, generation, terminal, _) =
            one_frame_finalizing_session(Arc::new(AtomicBool::new(false)));
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("mismatched.gif");
        let stage = finalized_stage(&destination);

        assert!(matches!(
            publish_finalized(&session, generation, terminal, 2, stage),
            Err(RecordingOutputError::ReceiptMismatch)
        ));
        assert!(!destination.exists());
        assert!(matches!(
            session.view().state(),
            RecorderState::Failed(super::super::RecordingFailure::Sink(_))
        ));
    }

    #[test]
    fn recording_output_collision_preserves_destination_and_fails_claimed_session() {
        let (session, generation, terminal, _) =
            one_frame_finalizing_session(Arc::new(AtomicBool::new(false)));
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("existing.gif");
        std::fs::write(&destination, b"existing output").unwrap();
        let stage = finalized_stage(&destination);

        assert!(matches!(
            publish_finalized(&session, generation, terminal, 1, stage),
            Err(RecordingOutputError::Stage(message))
                if message.contains("could not publish recording")
        ));
        assert_eq!(std::fs::read(&destination).unwrap(), b"existing output");
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
        assert!(matches!(
            session.view().state(),
            RecorderState::Failed(super::super::RecordingFailure::Sink(_))
        ));
    }
}
