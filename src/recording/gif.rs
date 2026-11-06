use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::time::Duration;

use gif::{Encoder, Frame};

use super::output::{OutputStage, RecordingOutputError, publish_finalized};
use super::{
    CanvasSize, CaptureTerminal, ConsumerFrame, MAX_RECORDING_SECONDS, RecordingFormat,
    RecordingSession, RecordingSessionError, TimedFrame,
};

const QUANTIZER_SPEED: i32 = 10;
const FILE_BUFFER_BYTES: usize = 64 * 1024;
const CENTISECOND_NANOS: u128 = 10_000_000;
const HALF_CENTISECOND_NANOS: u128 = CENTISECOND_NANOS / 2;

struct PendingFrame {
    source_bucket: u16,
    display_bucket: u16,
    frame: Frame<'static>,
}

/// A streaming GIF encoder. It retains one quantized image plus one reusable
/// RGBA scratch buffer; acknowledged source images are never accumulated.
struct GifStream<W: Write> {
    encoder: Encoder<W>,
    canvas: CanvasSize,
    scratch: Vec<u8>,
    pending: Option<PendingFrame>,
    last_input_elapsed: Option<Duration>,
    staged_inputs: u64,
    encoded_images: u64,
}

impl<W: Write> GifStream<W> {
    fn new(writer: W, canvas: CanvasSize) -> Result<Self, RecordingOutputError> {
        let width = u16::try_from(canvas.width())
            .map_err(|_| RecordingOutputError::InvalidFrame("GIF width exceeds u16"))?;
        let height = u16::try_from(canvas.height())
            .map_err(|_| RecordingOutputError::InvalidFrame("GIF height exceeds u16"))?;
        let encoder = Encoder::new(writer, width, height, &[])
            .map_err(|error| RecordingOutputError::Encoding(error.to_string()))?;
        Ok(Self {
            encoder,
            canvas,
            scratch: Vec::new(),
            pending: None,
            last_input_elapsed: None,
            staged_inputs: 0,
            encoded_images: 0,
        })
    }

    fn write_timed_frame(&mut self, timed: &TimedFrame) -> Result<(), RecordingOutputError> {
        self.write_frame_data(
            timed.elapsed(),
            timed.frame().size(),
            timed.frame().pixels(),
        )
    }

    fn write_frame_data(
        &mut self,
        elapsed: Duration,
        size: CanvasSize,
        pixels: &[u8],
    ) -> Result<(), RecordingOutputError> {
        if elapsed > Duration::from_secs(MAX_RECORDING_SECONDS) {
            return Err(RecordingOutputError::ClipTooLong);
        }
        if self
            .last_input_elapsed
            .is_some_and(|last_elapsed| elapsed < last_elapsed)
        {
            return Err(RecordingOutputError::TimestampMovedBackwards);
        }
        validate_frame_data(self.canvas, size, pixels.len())?;

        let bucket = centisecond_bucket(elapsed)?;
        let display_bucket = match self.pending.take() {
            None => 0,
            Some(pending) if bucket < pending.source_bucket => {
                self.pending = Some(pending);
                return Err(RecordingOutputError::TimestampMovedBackwards);
            }
            Some(pending) if bucket == pending.source_bucket => {
                let display_bucket = pending.display_bucket;
                drop(pending);
                display_bucket
            }
            Some(pending) => {
                let delay = bucket.saturating_sub(pending.display_bucket).max(1);
                self.pending = Some(pending);
                self.flush_pending(delay)?;
                bucket
            }
        };

        self.scratch.clear();
        self.scratch
            .try_reserve_exact(pixels.len())
            .map_err(|_| RecordingOutputError::AllocationFailed)?;
        self.scratch.extend_from_slice(pixels);
        // GIF transparency is player-disposal dependent for full-canvas
        // updates. Flatten each source frame onto a fixed black matte so
        // earlier opaque pixels cannot leak through transparent capture.
        for pixel in self.scratch.chunks_exact_mut(4) {
            let alpha = u16::from(pixel[3]);
            for channel in &mut pixel[..3] {
                *channel = ((u16::from(*channel) * alpha + 127) / 255) as u8;
            }
            pixel[3] = 255;
        }

        // The dimensions, byte count and speed have been checked before this
        // panic-prone quantizer call.
        let width = u16::try_from(size.width())
            .map_err(|_| RecordingOutputError::InvalidFrame("GIF width exceeds u16"))?;
        let height = u16::try_from(size.height())
            .map_err(|_| RecordingOutputError::InvalidFrame("GIF height exceeds u16"))?;
        let frame = Frame::from_rgba_speed(width, height, &mut self.scratch, QUANTIZER_SPEED);
        // Only the newest image in a source bucket is retained, including
        // several updates to the first image displayed from time zero.
        self.pending = Some(PendingFrame {
            source_bucket: bucket,
            display_bucket,
            frame,
        });

        self.last_input_elapsed = Some(elapsed);
        self.staged_inputs = self.staged_inputs.saturating_add(1);
        Ok(())
    }

    fn flush_pending(&mut self, delay: u16) -> Result<(), RecordingOutputError> {
        let Some(mut pending) = self.pending.take() else {
            return Ok(());
        };
        pending.frame.delay = delay;
        self.encoder
            .write_frame(&pending.frame)
            .map_err(|error| RecordingOutputError::Encoding(error.to_string()))?;
        self.encoded_images = self.encoded_images.saturating_add(1);
        Ok(())
    }

    fn finish(mut self, terminal_elapsed: Duration) -> Result<(W, u64, u64), RecordingOutputError> {
        if terminal_elapsed > Duration::from_secs(MAX_RECORDING_SECONDS) {
            return Err(RecordingOutputError::ClipTooLong);
        }
        if self.staged_inputs == 0 {
            return Err(RecordingOutputError::EmptyClip);
        }
        if self
            .last_input_elapsed
            .is_some_and(|last_elapsed| last_elapsed > terminal_elapsed)
        {
            return Err(RecordingOutputError::FrameAfterTerminal);
        }

        let terminal_bucket = centisecond_bucket(terminal_elapsed)?;
        if let Some(pending) = self.pending.as_ref() {
            if pending.display_bucket == terminal_bucket && self.encoded_images > 0 {
                // The newest update landed on the rounded terminal boundary
                // and would have no display time. Keep the prior image.
                self.pending = None;
            } else {
                let delay = terminal_bucket
                    .saturating_sub(pending.display_bucket)
                    .max(1);
                self.flush_pending(delay)?;
            }
        }

        let writer = self
            .encoder
            .into_inner()
            .map_err(|error| RecordingOutputError::Encoding(error.to_string()))?;
        Ok((writer, self.staged_inputs, self.encoded_images))
    }
}

fn validate_frame_data(
    canvas: CanvasSize,
    size: CanvasSize,
    byte_length: usize,
) -> Result<(), RecordingOutputError> {
    if size != canvas {
        return Err(RecordingOutputError::InvalidFrame(
            "captured frame dimensions do not match the recording canvas",
        ));
    }
    let expected = canvas.rgba_bytes();
    if byte_length != expected {
        return Err(RecordingOutputError::InvalidRgbaLength {
            expected,
            actual: byte_length,
        });
    }
    Ok(())
}

fn centisecond_bucket(elapsed: Duration) -> Result<u16, RecordingOutputError> {
    let ticks = elapsed.as_nanos().saturating_add(HALF_CENTISECOND_NANOS) / CENTISECOND_NANOS;
    u16::try_from(ticks).map_err(|_| RecordingOutputError::ClipTooLong)
}

/// A GIF recording bound to one session generation at creation.
pub(crate) struct GifRecording {
    session: std::sync::Arc<RecordingSession>,
    generation: u64,
    stream: Option<GifStream<BufWriter<File>>>,
    stage: Option<OutputStage>,
    acknowledged_inputs: u64,
}

impl GifRecording {
    pub(crate) fn create(
        session: std::sync::Arc<RecordingSession>,
        generation: u64,
        destination: impl AsRef<std::path::Path>,
    ) -> Result<Self, RecordingOutputError> {
        let view = session.view();
        if view.generation() != generation {
            return Err(RecordingOutputError::Session(
                RecordingSessionError::StaleGeneration,
            ));
        }
        let request = view.request().ok_or_else(|| {
            RecordingOutputError::Session(RecordingSessionError::MissingSelection)
        })?;
        if request.options().format() != RecordingFormat::Gif {
            return Err(RecordingOutputError::UnsupportedFormat);
        }
        let canvas = request.canvas();
        let (stage, file) = OutputStage::create(destination, RecordingFormat::Gif)
            .map_err(|error| RecordingOutputError::Stage(error.to_string()))?;
        let writer = BufWriter::with_capacity(FILE_BUFFER_BYTES, file);
        let stream = match GifStream::new(writer, canvas) {
            Ok(stream) => stream,
            Err(error) => return Err(discard_stage(stage, error)),
        };
        Ok(Self {
            session,
            generation,
            stream: Some(stream),
            stage: Some(stage),
            acknowledged_inputs: 0,
        })
    }

    /// Quantizes and stages one leased frame, then acknowledges it only after
    /// the encoder has accepted its bounded pending image.
    fn write_frame(&mut self, frame: ConsumerFrame) -> Result<(), RecordingOutputError> {
        if frame.generation() != self.generation {
            drop(frame);
            return Err(self.fail_sink(RecordingOutputError::Session(
                RecordingSessionError::StaleGeneration,
            )));
        }

        let result = self
            .stream
            .as_mut()
            .ok_or(RecordingOutputError::InvalidTransition)
            .and_then(|stream| stream.write_timed_frame(frame.frame()));
        if let Err(error) = result {
            drop(frame);
            return Err(self.fail_sink(error));
        }

        match frame.acknowledge_written() {
            Ok(()) => {
                self.acknowledged_inputs = self.acknowledged_inputs.saturating_add(1);
                Ok(())
            }
            Err(error) => Err(self.fail_sink(RecordingOutputError::Session(error))),
        }
    }

    pub(crate) fn consume_latest(&mut self) -> Result<Option<Duration>, RecordingOutputError> {
        self.consume_latest_with(|_| {})
    }

    #[cfg(test)]
    pub(crate) fn consume_latest_observing(
        &mut self,
        observer: impl FnMut(&TimedFrame),
    ) -> Result<Option<Duration>, RecordingOutputError> {
        self.consume_latest_with(observer)
    }

    fn consume_latest_with(
        &mut self,
        mut observer: impl FnMut(&TimedFrame),
    ) -> Result<Option<Duration>, RecordingOutputError> {
        match self.session.take_latest_frame(self.generation) {
            Ok(Some(frame)) => {
                let elapsed = frame.frame().elapsed();
                observer(frame.frame());
                self.write_frame(frame)?;
                Ok(Some(elapsed))
            }
            Ok(None) => Ok(None),
            Err(error) => Err(RecordingOutputError::Session(error)),
        }
    }

    /// Finalizes bytes while the recording remains cancellable, then claims
    /// the exact session permit immediately before the no-clobber move.
    pub(crate) fn finish_and_publish(mut self) -> Result<std::path::PathBuf, RecordingOutputError> {
        let view = self.session.view();
        if view.generation() != self.generation {
            return Err(self.fail_sink(RecordingOutputError::Session(
                RecordingSessionError::StaleGeneration,
            )));
        }
        if !matches!(view.state(), super::RecorderState::Finalizing { .. }) {
            return Err(self.fail_sink(RecordingOutputError::Session(
                RecordingSessionError::NotFinalizing,
            )));
        }
        let Some(terminal) = view
            .terminal()
            .filter(|terminal| terminal.generation() == self.generation)
        else {
            return Err(self.fail_sink(RecordingOutputError::Session(
                RecordingSessionError::NotFinalizing,
            )));
        };
        let Some(stream) = self.stream.take() else {
            return Err(self.fail_sink(RecordingOutputError::InvalidTransition));
        };
        let (buffered, staged_inputs, _encoded_images) = match stream.finish(terminal.elapsed()) {
            Ok(result) => result,
            Err(error) => return Err(self.fail_sink(error)),
        };
        if staged_inputs != self.acknowledged_inputs {
            drop(buffered);
            return Err(self.fail_sink(RecordingOutputError::ReceiptMismatch));
        }
        let file = match buffered.into_inner() {
            Ok(file) => file,
            Err(error) => {
                let message = error.error().to_string();
                drop(error.into_inner());
                return Err(self.fail_sink(RecordingOutputError::Encoding(message)));
            }
        };

        let Some(stage) = self.stage.take() else {
            drop(file);
            return Err(self.fail_sink(RecordingOutputError::InvalidTransition));
        };
        let finalized = match stage.finalize_file(file) {
            Ok(stage) => stage,
            Err(error) => {
                let output_error = RecordingOutputError::Stage(error.to_string());
                let _ = self
                    .session
                    .fail_sink(self.generation, output_error.to_string());
                return Err(output_error);
            }
        };
        publish_finalized(
            &self.session,
            self.generation,
            terminal,
            self.acknowledged_inputs,
            finalized,
        )
    }

    pub(crate) fn discard(mut self) -> Result<(), RecordingOutputError> {
        self.stream.take();
        match self.stage.take() {
            Some(stage) => stage
                .discard()
                .map_err(|error| RecordingOutputError::Stage(error.to_string())),
            None => Ok(()),
        }
    }

    fn fail_sink(&mut self, error: RecordingOutputError) -> RecordingOutputError {
        let _ = self.session.fail_sink(self.generation, error.to_string());
        self.stream.take();
        let Some(stage) = self.stage.take() else {
            return error;
        };
        match stage.discard() {
            Ok(()) => error,
            Err(cleanup) => RecordingOutputError::WithCleanup {
                cause: error.to_string(),
                cleanup: cleanup.to_string(),
            },
        }
    }
}

impl Drop for GifRecording {
    fn drop(&mut self) {
        self.stream.take();
        if let Some(stage) = self.stage.take() {
            let _ = stage.discard();
        }
    }
}

fn discard_stage(stage: OutputStage, error: RecordingOutputError) -> RecordingOutputError {
    match stage.discard() {
        Ok(()) => error,
        Err(cleanup) => RecordingOutputError::WithCleanup {
            cause: error.to_string(),
            cleanup: cleanup.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{AnimationDecoder, GenericImageView};
    use std::io::Cursor;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Instant;

    use crate::mkmacro::ScreenRect;
    use crate::recording::{
        CapturePoll, CaptureSource, CaptureStopToken, CaptureTarget, FrameRate, RecorderClock,
        RecorderState, RecordingBackend, RecordingEndReason, RecordingOptions, RecordingRequest,
        RecordingSession, RgbaFrame,
    };

    fn canvas() -> CanvasSize {
        CanvasSize::new(2, 2).unwrap()
    }

    fn timed(elapsed_ms: u64, color: [u8; 4]) -> TimedFrame {
        let pixels = color.repeat((canvas().width() * canvas().height()) as usize);
        TimedFrame::new(
            Duration::from_millis(elapsed_ms),
            super::super::RgbaFrame::new(canvas(), pixels).unwrap(),
        )
    }

    fn decode(bytes: Vec<u8>) -> Vec<image::Frame> {
        image::codecs::gif::GifDecoder::new(Cursor::new(bytes))
            .unwrap()
            .into_frames()
            .collect_frames()
            .unwrap()
    }

    fn sample(frame: &image::Frame) -> [u8; 4] {
        frame.buffer().get_pixel(0, 0).0
    }

    struct OneFrameSource {
        generation: u64,
        canvas: CanvasSize,
        ended: bool,
    }

    impl CaptureSource for OneFrameSource {
        fn poll(&mut self, _max_wait: Duration, _stop: &CaptureStopToken) -> CapturePoll {
            if self.ended {
                CapturePoll::Ended(RecordingEndReason::TargetLost)
            } else {
                self.ended = true;
                CapturePoll::Frame {
                    generation: self.generation,
                    frame: RgbaFrame::new(
                        self.canvas,
                        vec![220, 20, 30, 255].repeat(self.canvas.rgba_bytes() / 4),
                    )
                    .unwrap(),
                }
            }
        }
    }

    struct OneFrameBackend;

    impl RecordingBackend for OneFrameBackend {
        fn create_source(
            &self,
            request: &RecordingRequest,
            generation: u64,
        ) -> Result<Box<dyn CaptureSource>, String> {
            Ok(Box::new(OneFrameSource {
                generation,
                canvas: request.canvas(),
                ended: false,
            }))
        }
    }

    struct FixedClock;

    impl RecorderClock for FixedClock {
        fn now(&self) -> Duration {
            Duration::ZERO
        }
    }

    fn started_one_frame_recording(
        destination: &std::path::Path,
    ) -> (Arc<RecordingSession>, u64, GifRecording) {
        let canvas = canvas();
        let bounds = ScreenRect::new(0, 0, canvas.width(), canvas.height());
        let target = CaptureTarget::window(1, 1, 1, bounds).unwrap();
        let options = RecordingOptions::new(
            RecordingFormat::Gif,
            FrameRate::new(30).unwrap(),
            Duration::from_secs(1),
            false,
        )
        .unwrap();
        let request = RecordingRequest::new(target, bounds, options).unwrap();
        let session = Arc::new(RecordingSession::new(
            Arc::new(FixedClock),
            Arc::new(OneFrameBackend),
            || {},
        ));
        let generation = session.select(request).unwrap();
        let recording =
            GifRecording::create(Arc::clone(&session), generation, destination).unwrap();
        session.start().unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            session.poll().unwrap();
            let view = session.view();
            if matches!(view.state(), RecorderState::Finalizing { .. }) && !view.worker_busy() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "fake capture did not finalize: {view:?}"
            );
            thread::yield_now();
        }
        (session, generation, recording)
    }

    #[test]
    fn recording_gif_stream_coalesces_centisecond_buckets_and_uses_terminal_elapsed() {
        let mut stream = GifStream::new(Vec::new(), canvas()).unwrap();
        stream
            .write_timed_frame(&timed(23, [220, 20, 30, 255]))
            .unwrap();
        stream
            .write_timed_frame(&timed(135, [20, 220, 30, 255]))
            .unwrap();
        stream
            .write_timed_frame(&timed(142, [20, 30, 220, 255]))
            .unwrap();
        let (bytes, staged, encoded) = stream.finish(Duration::from_millis(307)).unwrap();

        assert_eq!(staged, 3);
        assert_eq!(encoded, 2);
        let frames = decode(bytes);
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].buffer().dimensions(), (2, 2));
        assert_eq!(sample(&frames[0]), [220, 20, 30, 255]);
        assert_eq!(sample(&frames[1]), [20, 30, 220, 255]);
        assert_eq!(frames[0].delay().numer_denom_ms(), (140, 1));
        assert_eq!(frames[1].delay().numer_denom_ms(), (170, 1));
    }

    #[test]
    fn recording_gif_stream_starts_at_zero_and_gives_short_clips_one_tick() {
        let mut stream = GifStream::new(Vec::new(), canvas()).unwrap();
        stream
            .write_timed_frame(&timed(4, [220, 20, 30, 255]))
            .unwrap();
        let (bytes, staged, encoded) = stream.finish(Duration::from_millis(4)).unwrap();
        assert_eq!((staged, encoded), (1, 1));
        let frames = decode(bytes);
        assert_eq!(frames.len(), 1);
        assert_eq!(sample(&frames[0]), [220, 20, 30, 255]);
        assert_eq!(frames[0].delay().numer_denom_ms(), (10, 1));
    }

    #[test]
    fn recording_gif_stream_coalesces_initial_bucket_but_keeps_zero_display_anchor() {
        let mut stream = GifStream::new(Vec::new(), canvas()).unwrap();
        stream
            .write_timed_frame(&timed(8, [220, 20, 30, 255]))
            .unwrap();
        stream
            .write_timed_frame(&timed(9, [20, 220, 30, 255]))
            .unwrap();
        let (bytes, staged, encoded) = stream.finish(Duration::from_millis(50)).unwrap();

        assert_eq!((staged, encoded), (2, 1));
        let frames = decode(bytes);
        assert_eq!(frames.len(), 1);
        assert_eq!(sample(&frames[0]), [20, 220, 30, 255]);
        assert_eq!(frames[0].delay().numer_denom_ms(), (50, 1));
    }

    #[test]
    fn recording_gif_stream_omits_zero_tick_trailing_update() {
        let mut stream = GifStream::new(Vec::new(), canvas()).unwrap();
        stream
            .write_timed_frame(&timed(0, [220, 20, 30, 255]))
            .unwrap();
        stream
            .write_timed_frame(&timed(14, [20, 220, 30, 255]))
            .unwrap();
        let (bytes, staged, encoded) = stream.finish(Duration::from_millis(14)).unwrap();
        assert_eq!((staged, encoded), (2, 1));
        let frames = decode(bytes);
        assert_eq!(frames.len(), 1);
        assert_eq!(sample(&frames[0]), [220, 20, 30, 255]);
        assert_eq!(frames[0].delay().numer_denom_ms(), (10, 1));
    }

    #[test]
    fn recording_gif_stream_flattens_transparency_on_a_stable_black_matte() {
        let canvas = canvas();
        let mut stream = GifStream::new(Vec::new(), canvas).unwrap();
        let opaque = vec![200, 30, 40, 255].repeat(canvas.rgba_bytes() / 4);
        let transparent = vec![0, 250, 0, 0].repeat(canvas.rgba_bytes() / 4);
        stream
            .write_frame_data(Duration::ZERO, canvas, &opaque)
            .unwrap();
        stream
            .write_frame_data(Duration::from_millis(10), canvas, &transparent)
            .unwrap();
        let (bytes, _, _) = stream.finish(Duration::from_millis(20)).unwrap();

        let frames = decode(bytes);
        assert_eq!(frames.len(), 2);
        assert_eq!(sample(&frames[0]), [200, 30, 40, 255]);
        assert_eq!(sample(&frames[1]), [0, 0, 0, 255]);
    }

    #[test]
    fn recording_gif_stream_rejects_empty_wrong_canvas_bad_bytes_and_bad_timing() {
        let empty = GifStream::new(Vec::new(), canvas()).unwrap();
        assert!(matches!(
            empty.finish(Duration::ZERO),
            Err(RecordingOutputError::EmptyClip)
        ));

        let mut wrong_canvas = GifStream::new(Vec::new(), canvas()).unwrap();
        let other_canvas = CanvasSize::new(1, 1).unwrap();
        let other = TimedFrame::new(
            Duration::ZERO,
            super::super::RgbaFrame::new(other_canvas, vec![0, 0, 0, 255]).unwrap(),
        );
        assert!(matches!(
            wrong_canvas.write_timed_frame(&other),
            Err(RecordingOutputError::InvalidFrame(_))
        ));
        assert!(matches!(
            validate_frame_data(canvas(), canvas(), canvas().rgba_bytes() - 1),
            Err(RecordingOutputError::InvalidRgbaLength { .. })
        ));

        let mut decreasing = GifStream::new(Vec::new(), canvas()).unwrap();
        decreasing
            .write_timed_frame(&timed(20, [220, 20, 30, 255]))
            .unwrap();
        assert!(matches!(
            decreasing.write_timed_frame(&timed(19, [20, 220, 30, 255])),
            Err(RecordingOutputError::TimestampMovedBackwards)
        ));

        let mut after_terminal = GifStream::new(Vec::new(), canvas()).unwrap();
        after_terminal
            .write_timed_frame(&timed(20, [220, 20, 30, 255]))
            .unwrap();
        assert!(matches!(
            after_terminal.finish(Duration::from_millis(19)),
            Err(RecordingOutputError::FrameAfterTerminal)
        ));
    }

    #[test]
    fn recording_gif_stream_surfaces_frame_write_and_trailer_errors() {
        let (writer, control) = ToggleWriter::new();
        let mut stream = GifStream::new(writer, canvas()).unwrap();
        stream
            .write_timed_frame(&timed(0, [220, 20, 30, 255]))
            .unwrap();
        control.fail_next.store(true, Ordering::SeqCst);
        assert!(matches!(
            stream.finish(Duration::from_millis(10)),
            Err(RecordingOutputError::Encoding(_))
        ));

        let (writer, control) = ToggleWriter::new();
        let mut stream = GifStream::new(writer, canvas()).unwrap();
        stream
            .write_timed_frame(&timed(0, [220, 20, 30, 255]))
            .unwrap();
        stream.flush_pending(1).unwrap();
        control.fail_next.store(true, Ordering::SeqCst);
        assert!(matches!(
            stream.finish(Duration::from_millis(10)),
            Err(RecordingOutputError::Encoding(_))
        ));
    }

    #[test]
    fn recording_gif_flush_failure_marks_sink_and_removes_owned_stage() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("failed.gif");
        let (session, generation, mut recording) = started_one_frame_recording(&destination);
        assert_eq!(recording.consume_latest().unwrap(), Some(Duration::ZERO));
        let temporary_path = recording
            .stage
            .as_ref()
            .unwrap()
            .temporary_path()
            .to_path_buf();
        let read_only = File::open(&temporary_path).unwrap();
        *recording
            .stream
            .as_mut()
            .unwrap()
            .encoder
            .get_mut()
            .get_mut() = read_only;

        assert!(matches!(
            recording.finish_and_publish(),
            Err(RecordingOutputError::Encoding(_))
        ));
        assert!(matches!(
            session.view().state(),
            RecorderState::Failed(super::super::RecordingFailure::Sink(_))
        ));
        assert_eq!(session.view().generation(), generation);
        assert!(!destination.exists());
        assert!(!temporary_path.exists());
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    }

    struct ToggleWriter {
        shared: Arc<ToggleState>,
    }

    struct ToggleState {
        bytes: Mutex<Vec<u8>>,
        fail_next: AtomicBool,
    }

    impl ToggleWriter {
        fn new() -> (Self, Arc<ToggleState>) {
            let shared = Arc::new(ToggleState {
                bytes: Mutex::new(Vec::new()),
                fail_next: AtomicBool::new(false),
            });
            (
                Self {
                    shared: Arc::clone(&shared),
                },
                shared,
            )
        }
    }

    impl Write for ToggleWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.shared.fail_next.swap(false, Ordering::SeqCst) {
                return Err(io::Error::other("injected GIF output failure"));
            }
            self.shared.bytes.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            if self.shared.fail_next.swap(false, Ordering::SeqCst) {
                return Err(io::Error::other("injected GIF flush failure"));
            }
            Ok(())
        }
    }
}
