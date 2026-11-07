mod controller;
mod gif;
mod model;
#[cfg(windows)]
mod mp4_windows;
mod output;
mod session;
mod stop_hotkey;
mod windows;

pub use controller::{
    RecordingController, RecordingControllerOutcome, RecordingControllerSnapshot,
    RecordingWriterPreparation,
};
pub(crate) use gif::GifRecording;
pub use model::{
    CanvasSize, CaptureIdentity, CaptureTarget, FrameError, FrameRate, FrameStamp, FrameTimeline,
    MAX_CANVAS_HEIGHT, MAX_CANVAS_WIDTH, MAX_DISPLAY_DEVICE_NAME_BYTES, MAX_FRAME_BYTES,
    MAX_FRAMES_PER_SECOND, MAX_RECORDING_SECONDS, MIN_FRAMES_PER_SECOND, OutputScale,
    RecorderState, RecordingEndReason, RecordingFailure, RecordingFormat, RecordingOptions,
    RecordingRequest, RecordingSummary, RequestError, RgbaFrame, TimedFrame,
};
#[cfg(windows)]
pub(crate) use mp4_windows::Mp4Recording;
pub use session::{
    CapturePoll, CaptureSource, CaptureStopToken, CaptureTerminal, ConsumerFrame,
    FinalizationPermit, RecorderClock, RecordingBackend, RecordingSession, RecordingSessionError,
    RecordingSessionView, SinkReceipt, SystemRecorderClock,
};
pub use stop_hotkey::RecordingStopHotkeyBridge;
pub use windows::WindowsRecordingBackend;
pub(crate) use windows::{
    RecordingDisplayTarget, describe_recording_window, enumerate_recording_displays,
    validate_recording_target,
};
