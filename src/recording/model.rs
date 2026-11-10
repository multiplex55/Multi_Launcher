use crate::mkmacro::ScreenRect;
use std::time::Duration;

pub const MAX_CANVAS_WIDTH: u32 = 1_920;
pub const MAX_CANVAS_HEIGHT: u32 = 1_080;
pub const MAX_FRAME_BYTES: usize = MAX_CANVAS_WIDTH as usize * MAX_CANVAS_HEIGHT as usize * 4;
pub const MAX_RECORDING_SECONDS: u64 = 1_000;
pub const DEFAULT_RECORDING_SECONDS: u64 = 30;
pub const MIN_FRAMES_PER_SECOND: u8 = 1;
pub const MAX_FRAMES_PER_SECOND: u8 = 60;
pub const MAX_DISPLAY_DEVICE_NAME_BYTES: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordingFormat {
    Gif,
    Mp4,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OutputScale {
    #[default]
    Fit,
    Half,
    Quarter,
}

impl OutputScale {
    const fn divisor(self) -> u32 {
        match self {
            Self::Fit => 1,
            Self::Half => 2,
            Self::Quarter => 4,
        }
    }
}

impl RecordingFormat {
    pub const fn default_frame_rate(self) -> u8 {
        match self {
            Self::Gif => 10,
            Self::Mp4 => 30,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameRate(u8);

impl FrameRate {
    pub fn new(frames_per_second: u8) -> Result<Self, RequestError> {
        if !(MIN_FRAMES_PER_SECOND..=MAX_FRAMES_PER_SECOND).contains(&frames_per_second) {
            return Err(RequestError::InvalidFrameRate);
        }
        Ok(Self(frames_per_second))
    }

    pub const fn frames_per_second(self) -> u8 {
        self.0
    }

    pub fn interval(self) -> Duration {
        Duration::from_nanos(1_000_000_000_u64.div_ceil(u64::from(self.0)))
    }
}

/// An explicit capture deadline; unlimited recordings have no artificial cap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordingDurationLimit {
    Unlimited,
    Timed(Duration),
}

impl RecordingDurationLimit {
    pub fn from_seconds(seconds: u64) -> Result<Self, RequestError> {
        if seconds == 0 {
            return Ok(Self::Unlimited);
        }
        if seconds > MAX_RECORDING_SECONDS {
            return Err(RequestError::InvalidDuration);
        }
        Ok(Self::Timed(Duration::from_secs(seconds)))
    }

    pub fn clamp(self, elapsed: Duration) -> Duration {
        match self {
            Self::Unlimited => elapsed,
            Self::Timed(limit) => elapsed.min(limit),
        }
    }

    pub fn reached(self, elapsed: Duration) -> bool {
        matches!(self, Self::Timed(limit) if elapsed >= limit)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecordingOptions {
    format: RecordingFormat,
    frame_rate: FrameRate,
    duration_limit: RecordingDurationLimit,
    include_cursor: bool,
}

impl RecordingOptions {
    pub fn new(
        format: RecordingFormat,
        frame_rate: FrameRate,
        duration_limit: RecordingDurationLimit,
        include_cursor: bool,
    ) -> Result<Self, RequestError> {
        if matches!(duration_limit, RecordingDurationLimit::Timed(duration)
            if duration < Duration::from_secs(1) || duration > Duration::from_secs(MAX_RECORDING_SECONDS))
        {
            return Err(RequestError::InvalidDuration);
        }
        Ok(Self {
            format,
            frame_rate,
            duration_limit,
            include_cursor,
        })
    }

    pub fn defaults_for(format: RecordingFormat) -> Self {
        Self {
            format,
            frame_rate: FrameRate(format.default_frame_rate()),
            duration_limit: RecordingDurationLimit::Timed(Duration::from_secs(
                DEFAULT_RECORDING_SECONDS,
            )),
            include_cursor: false,
        }
    }

    pub const fn format(self) -> RecordingFormat {
        self.format
    }

    pub const fn frame_rate(self) -> FrameRate {
        self.frame_rate
    }

    pub const fn duration_limit(self) -> RecordingDurationLimit {
        self.duration_limit
    }

    pub const fn includes_cursor(self) -> bool {
        self.include_cursor
    }
}

/// Stable, portable identity for the capture source selected by the user.
/// Native handles are stored as integers so the model does not own platform APIs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CaptureIdentity {
    Window {
        hwnd: usize,
        process_id: u32,
        thread_id: u32,
    },
    Display {
        hmonitor: usize,
        device_name: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaptureTarget {
    identity: CaptureIdentity,
    target_bounds: ScreenRect,
}

impl CaptureTarget {
    pub fn window(
        hwnd: usize,
        process_id: u32,
        thread_id: u32,
        target_bounds: ScreenRect,
    ) -> Result<Self, RequestError> {
        let target = Self {
            identity: CaptureIdentity::Window {
                hwnd,
                process_id,
                thread_id,
            },
            target_bounds,
        };
        target.validate()?;
        Ok(target)
    }

    pub fn display(
        hmonitor: usize,
        device_name: String,
        bounds: ScreenRect,
    ) -> Result<Self, RequestError> {
        let target = Self {
            identity: CaptureIdentity::Display {
                hmonitor,
                device_name,
            },
            target_bounds: bounds,
        };
        target.validate()?;
        Ok(target)
    }

    fn validate(&self) -> Result<(), RequestError> {
        match &self.identity {
            CaptureIdentity::Window {
                hwnd,
                process_id,
                thread_id,
            } if *hwnd == 0 || *process_id == 0 || *thread_id == 0 => {
                return Err(RequestError::InvalidTargetIdentity);
            }
            CaptureIdentity::Display {
                hmonitor,
                device_name,
            } if *hmonitor == 0
                || device_name.is_empty()
                || device_name.len() > MAX_DISPLAY_DEVICE_NAME_BYTES
                || device_name.contains('\0') =>
            {
                return Err(RequestError::InvalidTargetIdentity);
            }
            CaptureIdentity::Window { .. } | CaptureIdentity::Display { .. } => {}
        }

        validate_physical_rect(self.target_bounds)?;
        Ok(())
    }

    pub fn identity(&self) -> &CaptureIdentity {
        &self.identity
    }

    pub const fn target_bounds(&self) -> ScreenRect {
        self.target_bounds
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordingRequest {
    target: CaptureTarget,
    region: ScreenRect,
    canvas: CanvasSize,
    scale: OutputScale,
    options: RecordingOptions,
}

impl RecordingRequest {
    pub fn new(
        target: CaptureTarget,
        region: ScreenRect,
        options: RecordingOptions,
    ) -> Result<Self, RequestError> {
        Self::new_with_scale(target, region, options, OutputScale::Fit)
    }

    pub fn new_with_scale(
        target: CaptureTarget,
        region: ScreenRect,
        options: RecordingOptions,
        scale: OutputScale,
    ) -> Result<Self, RequestError> {
        target.validate()?;
        validate_physical_rect(region)?;
        match &target.identity {
            // Window capture targets the selected window item itself. It may
            // straddle monitors; requiring a display rectangle would reject a
            // valid WGC window target. Desktop-region capture uses a Display
            // target and is constrained to that single display's bounds.
            CaptureIdentity::Window { .. } if region != target.target_bounds => {
                return Err(RequestError::RegionOutsideTarget);
            }
            CaptureIdentity::Display { .. } if !target.target_bounds.contains(region) => {
                return Err(RequestError::RegionOutsideDisplay);
            }
            CaptureIdentity::Window { .. } | CaptureIdentity::Display { .. } => {}
        }
        let fitted = CanvasSize::fit(region.width, region.height)?;
        let canvas = fitted.scaled(scale)?;
        Ok(Self {
            target,
            region,
            canvas,
            scale,
            options,
        })
    }

    pub fn target(&self) -> &CaptureTarget {
        &self.target
    }

    pub const fn region(&self) -> ScreenRect {
        self.region
    }

    pub const fn canvas(&self) -> CanvasSize {
        self.canvas
    }

    pub const fn scale(&self) -> OutputScale {
        self.scale
    }

    /// Dimensions written by the selected encoder. MP4 pads odd dimensions by
    /// one pixel to satisfy Media Foundation's NV12 layout requirements.
    pub const fn encoded_dimensions(&self) -> (u32, u32) {
        match self.options.format() {
            RecordingFormat::Gif => (self.canvas.width(), self.canvas.height()),
            RecordingFormat::Mp4 => self.canvas.mp4_padded_dimensions(),
        }
    }

    pub const fn options(&self) -> RecordingOptions {
        self.options
    }
}

fn validate_physical_rect(rect: ScreenRect) -> Result<(), RequestError> {
    rect.validate_capture()
        .map_err(|_| RequestError::InvalidGeometry)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CanvasSize {
    width: u32,
    height: u32,
    rgba_bytes: usize,
}

impl CanvasSize {
    fn fit(width: u32, height: u32) -> Result<Self, RequestError> {
        if width == 0 || height == 0 {
            return Err(RequestError::InvalidGeometry);
        }
        let (fitted_width, fitted_height) = if width <= MAX_CANVAS_WIDTH
            && height <= MAX_CANVAS_HEIGHT
        {
            (width, height)
        } else if u64::from(width) * u64::from(MAX_CANVAS_HEIGHT)
            >= u64::from(height) * u64::from(MAX_CANVAS_WIDTH)
        {
            (
                MAX_CANVAS_WIDTH,
                (u64::from(height) * u64::from(MAX_CANVAS_WIDTH) / u64::from(width)).max(1) as u32,
            )
        } else {
            (
                (u64::from(width) * u64::from(MAX_CANVAS_HEIGHT) / u64::from(height)).max(1) as u32,
                MAX_CANVAS_HEIGHT,
            )
        };
        Self::new(fitted_width, fitted_height)
    }

    fn scaled(self, scale: OutputScale) -> Result<Self, RequestError> {
        let divisor = scale.divisor();
        Self::new(self.width.div_ceil(divisor), self.height.div_ceil(divisor))
    }

    pub fn new(width: u32, height: u32) -> Result<Self, RequestError> {
        if width == 0 || height == 0 || width > MAX_CANVAS_WIDTH || height > MAX_CANVAS_HEIGHT {
            return Err(RequestError::CanvasOutOfBounds);
        }
        let rgba_bytes = usize::try_from(u64::from(width) * u64::from(height))
            .ok()
            .and_then(|pixels| pixels.checked_mul(4))
            .filter(|bytes| *bytes <= isize::MAX as usize)
            .ok_or(RequestError::FrameBytesOverflow)?;
        Ok(Self {
            width,
            height,
            rgba_bytes,
        })
    }

    pub const fn width(self) -> u32 {
        self.width
    }

    pub const fn height(self) -> u32 {
        self.height
    }

    pub const fn rgba_bytes(self) -> usize {
        self.rgba_bytes
    }

    pub const fn mp4_padded_dimensions(self) -> (u32, u32) {
        (self.width + self.width % 2, self.height + self.height % 2)
    }

    pub fn allocate_rgba(self) -> Result<Vec<u8>, FrameError> {
        let mut pixels = Vec::new();
        pixels
            .try_reserve_exact(self.rgba_bytes)
            .map_err(|_| FrameError::AllocationFailed)?;
        pixels.resize(self.rgba_bytes, 0);
        Ok(pixels)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RequestError {
    InvalidTargetIdentity,
    InvalidGeometry,
    RegionOutsideTarget,
    RegionOutsideDisplay,
    CanvasOutOfBounds,
    FrameBytesOverflow,
    InvalidFrameRate,
    InvalidDuration,
}

impl std::fmt::Display for RequestError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::InvalidTargetIdentity => "capture target identity is invalid",
            Self::InvalidGeometry => "capture region has invalid physical geometry",
            Self::RegionOutsideTarget => "capture region is outside the selected target",
            Self::RegionOutsideDisplay => "capture region crosses the selected display bounds",
            Self::CanvasOutOfBounds => "recording canvas exceeds the supported dimensions",
            Self::FrameBytesOverflow => "recording frame byte size overflows the supported limit",
            Self::InvalidFrameRate => "frame rate must be between 1 and 60 frames per second",
            Self::InvalidDuration => {
                "recording duration must be unlimited or between 1 and 1000 seconds"
            }
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for RequestError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RgbaFrame {
    size: CanvasSize,
    pixels: Box<[u8]>,
}

impl RgbaFrame {
    pub fn new(size: CanvasSize, pixels: Vec<u8>) -> Result<Self, FrameError> {
        if pixels.len() != size.rgba_bytes() {
            return Err(FrameError::WrongByteLength {
                expected: size.rgba_bytes(),
                actual: pixels.len(),
            });
        }
        // Normalize spare capacity out of the retained mailbox payload.
        Ok(Self {
            size,
            pixels: pixels.into_boxed_slice(),
        })
    }

    pub const fn size(&self) -> CanvasSize {
        self.size
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub fn into_pixels(self) -> Box<[u8]> {
        self.pixels
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FrameError {
    AllocationFailed,
    WrongByteLength { expected: usize, actual: usize },
    WrongDimensions,
    TimeBeforeStart,
    ClockMovedBackwards,
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AllocationFailed => {
                formatter.write_str("could not allocate a bounded RGBA frame")
            }
            Self::WrongByteLength { expected, actual } => {
                write!(
                    formatter,
                    "RGBA frame has {actual} bytes; expected {expected}"
                )
            }
            Self::WrongDimensions => {
                formatter.write_str("RGBA frame dimensions do not match the recording canvas")
            }
            Self::TimeBeforeStart => {
                formatter.write_str("frame timestamp precedes recording start")
            }
            Self::ClockMovedBackwards => {
                formatter.write_str("monotonic recording clock moved backwards")
            }
        }
    }
}

impl std::error::Error for FrameError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimedFrame {
    elapsed: Duration,
    frame: RgbaFrame,
}

impl TimedFrame {
    pub fn new(elapsed: Duration, frame: RgbaFrame) -> Self {
        Self { elapsed, frame }
    }

    pub const fn elapsed(&self) -> Duration {
        self.elapsed
    }

    pub const fn frame(&self) -> &RgbaFrame {
        &self.frame
    }

    pub fn into_parts(self) -> (Duration, RgbaFrame) {
        (self.elapsed, self.frame)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameStamp {
    Accepted(Duration),
    DroppedForCadence,
    DurationLimit(Duration),
}

/// Turns an injected monotonic clock into source timestamps. It never
/// synthesizes timestamps from frame count, so mailbox drops remain visible as
/// elapsed gaps to later encoders.
#[derive(Clone, Debug)]
pub struct FrameTimeline {
    started_at: Duration,
    last_observed: Duration,
    last_accepted: Option<Duration>,
    interval: Duration,
    duration_limit: RecordingDurationLimit,
}

impl FrameTimeline {
    pub fn new(started_at: Duration, options: RecordingOptions) -> Self {
        Self {
            started_at,
            last_observed: started_at,
            last_accepted: None,
            interval: options.frame_rate.interval(),
            duration_limit: options.duration_limit,
        }
    }

    pub fn stamp(&mut self, now: Duration) -> Result<FrameStamp, FrameError> {
        let elapsed = self.elapsed_at(now)?;
        if let RecordingDurationLimit::Timed(limit) = self.duration_limit
            && elapsed >= limit
        {
            return Ok(FrameStamp::DurationLimit(limit));
        }
        if let Some(last_accepted) = self.last_accepted
            && elapsed.saturating_sub(last_accepted) < self.interval
        {
            return Ok(FrameStamp::DroppedForCadence);
        }
        self.last_accepted = Some(elapsed);
        Ok(FrameStamp::Accepted(elapsed))
    }

    /// Gets elapsed time for terminal or idle polls even when no frame arrived.
    /// Values past the configured cap are clamped so stop/target-loss cannot
    /// extend the recording beyond its admitted duration.
    pub fn elapsed_at(&mut self, now: Duration) -> Result<Duration, FrameError> {
        if now < self.started_at {
            return Err(FrameError::TimeBeforeStart);
        }
        if now < self.last_observed {
            return Err(FrameError::ClockMovedBackwards);
        }
        self.last_observed = now;
        Ok(self.duration_limit.clamp(now - self.started_at))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordingEndReason {
    UserStopped,
    DurationLimit,
    TargetLost,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordingSummary {
    pub reason: RecordingEndReason,
    pub elapsed: Duration,
    pub frames_written: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecordingFailure {
    Source(String),
    Sink(String),
    NoFrames,
    InvalidFrame(FrameError),
    WorkerPanicked,
    WorkerCapacity(String),
    WorkerStart(String),
    WorkerRetirement(String),
    InvalidFinalizationReceipt,
}

impl std::fmt::Display for RecordingFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Source(message)
            | Self::Sink(message)
            | Self::WorkerCapacity(message)
            | Self::WorkerStart(message)
            | Self::WorkerRetirement(message) => formatter.write_str(message),
            Self::NoFrames => formatter.write_str("capture ended before any frame was accepted"),
            Self::InvalidFrame(error) => write!(formatter, "invalid captured frame: {error}"),
            Self::WorkerPanicked => formatter.write_str("recording worker panicked"),
            Self::InvalidFinalizationReceipt => formatter
                .write_str("sink did not confirm finalization for this recording generation"),
        }
    }
}

impl std::error::Error for RecordingFailure {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecorderState {
    Idle,
    Selecting,
    Countdown {
        remaining: Duration,
    },
    Recording {
        elapsed: Duration,
        accepted_frames: u64,
    },
    Finalizing {
        reason: RecordingEndReason,
        elapsed: Duration,
    },
    Completed(RecordingSummary),
    Failed(RecordingFailure),
    Cancelled,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(x: i32, y: i32, width: u32, height: u32) -> ScreenRect {
        ScreenRect::new(x, y, width, height)
    }

    fn window_target() -> CaptureTarget {
        CaptureTarget::window(10, 20, 30, screen(-1920, 0, 1920, 1080)).unwrap()
    }

    #[test]
    fn recording_model_accepts_negative_physical_origin_and_fits_canvas() {
        let options = RecordingOptions::defaults_for(RecordingFormat::Mp4);
        assert_eq!(options.frame_rate().frames_per_second(), 30);
        assert_eq!(
            options.duration_limit(),
            RecordingDurationLimit::Timed(Duration::from_secs(30))
        );
        assert!(!options.includes_cursor());

        let target = CaptureTarget::window(10, 20, 30, screen(-2560, -200, 2560, 1440)).unwrap();
        let request =
            RecordingRequest::new(target, screen(-2560, -200, 2560, 1440), options).unwrap();
        assert_eq!(request.canvas(), CanvasSize::new(1920, 1080).unwrap());
        assert_eq!(request.region().x, -2560);
    }

    #[test]
    fn recording_model_rejects_invalid_region_or_target_identity() {
        let target = window_target();
        assert_eq!(
            RecordingRequest::new(
                target.clone(),
                screen(-1920, 0, 1921, 1080),
                RecordingOptions::defaults_for(RecordingFormat::Gif),
            )
            .unwrap_err(),
            RequestError::RegionOutsideTarget
        );
        assert_eq!(
            RecordingRequest::new(
                target,
                screen(-100, 0, 100, 100),
                RecordingOptions::defaults_for(RecordingFormat::Gif),
            )
            .unwrap_err(),
            RequestError::RegionOutsideTarget
        );
        assert_eq!(
            CaptureTarget::window(0, 20, 30, screen(0, 0, 10, 10)).unwrap_err(),
            RequestError::InvalidTargetIdentity
        );
        assert_eq!(
            CaptureTarget::display(1, String::new(), screen(0, 0, 10, 10)).unwrap_err(),
            RequestError::InvalidTargetIdentity
        );
    }

    #[test]
    fn recording_model_enforces_frame_and_duration_bounds_before_allocation() {
        assert_eq!(FrameRate::new(0), Err(RequestError::InvalidFrameRate));
        assert_eq!(FrameRate::new(61), Err(RequestError::InvalidFrameRate));
        assert_eq!(
            RecordingOptions::new(
                RecordingFormat::Gif,
                FrameRate::new(10).unwrap(),
                RecordingDurationLimit::Timed(Duration::ZERO),
                false,
            ),
            Err(RequestError::InvalidDuration)
        );
        assert_eq!(
            RecordingOptions::new(
                RecordingFormat::Mp4,
                FrameRate::new(30).unwrap(),
                RecordingDurationLimit::Timed(Duration::from_secs(MAX_RECORDING_SECONDS + 1)),
                true,
            ),
            Err(RequestError::InvalidDuration)
        );
        assert_eq!(
            CanvasSize::new(MAX_CANVAS_WIDTH + 1, 1),
            Err(RequestError::CanvasOutOfBounds)
        );
        assert_eq!(
            CanvasSize::new(MAX_CANVAS_WIDTH, MAX_CANVAS_HEIGHT)
                .unwrap()
                .rgba_bytes(),
            MAX_FRAME_BYTES
        );

        let size = CanvasSize::new(2, 3).unwrap();
        assert_eq!(size.rgba_bytes(), 24);
        assert_eq!(size.allocate_rgba().unwrap().len(), 24);
        assert!(matches!(
            RgbaFrame::new(size, vec![0; 23]),
            Err(FrameError::WrongByteLength {
                expected: 24,
                actual: 23
            })
        ));
    }

    #[test]
    fn recording_timeline_unlimited_and_finite_boundaries_are_exact() {
        assert_eq!(
            RecordingDurationLimit::from_seconds(0),
            Ok(RecordingDurationLimit::Unlimited)
        );
        assert_eq!(
            RecordingDurationLimit::from_seconds(1001),
            Err(RequestError::InvalidDuration)
        );
        let rate = FrameRate::new(10).unwrap();
        let unlimited = RecordingOptions::new(
            RecordingFormat::Gif,
            rate,
            RecordingDurationLimit::Unlimited,
            false,
        )
        .unwrap();
        let mut timeline = FrameTimeline::new(Duration::ZERO, unlimited);
        for elapsed in [
            Duration::from_secs(121),
            Duration::from_millis(655_350),
            Duration::from_secs(1001),
        ] {
            assert_eq!(timeline.stamp(elapsed), Ok(FrameStamp::Accepted(elapsed)));
        }
        assert_eq!(
            timeline.elapsed_at(Duration::from_secs(1000)),
            Err(FrameError::ClockMovedBackwards)
        );
        for seconds in [17, 1000] {
            let options = RecordingOptions::new(
                RecordingFormat::Gif,
                rate,
                RecordingDurationLimit::Timed(Duration::from_secs(seconds)),
                false,
            )
            .unwrap();
            let mut timeline = FrameTimeline::new(Duration::ZERO, options);
            assert_eq!(
                timeline.stamp(Duration::from_secs(seconds) - Duration::from_millis(100)),
                Ok(FrameStamp::Accepted(
                    Duration::from_secs(seconds) - Duration::from_millis(100)
                ))
            );
            assert_eq!(
                timeline.stamp(Duration::from_secs(seconds)),
                Ok(FrameStamp::DurationLimit(Duration::from_secs(seconds)))
            );
            assert_eq!(
                timeline.elapsed_at(Duration::from_secs(seconds + 1)),
                Ok(Duration::from_secs(seconds))
            );
        }
    }

    #[test]
    fn recording_timeline_throttles_without_rewriting_elapsed_gaps() {
        let options = RecordingOptions::new(
            RecordingFormat::Gif,
            FrameRate::new(10).unwrap(),
            RecordingDurationLimit::Timed(Duration::from_secs(1)),
            false,
        )
        .unwrap();
        let start = Duration::from_secs(17);
        let mut timeline = FrameTimeline::new(start, options);

        assert_eq!(
            timeline.stamp(start),
            Ok(FrameStamp::Accepted(Duration::ZERO))
        );
        assert_eq!(
            timeline.stamp(start + Duration::from_millis(50)),
            Ok(FrameStamp::DroppedForCadence)
        );
        assert_eq!(
            timeline.stamp(start + Duration::from_millis(100)),
            Ok(FrameStamp::Accepted(Duration::from_millis(100)))
        );
        assert_eq!(
            timeline.stamp(start + Duration::from_millis(350)),
            Ok(FrameStamp::Accepted(Duration::from_millis(350)))
        );
        assert_eq!(
            timeline.stamp(start + Duration::from_secs(1)),
            Ok(FrameStamp::DurationLimit(Duration::from_secs(1)))
        );
        assert_eq!(
            timeline.stamp(start + Duration::from_millis(340)),
            Err(FrameError::ClockMovedBackwards)
        );
        assert_eq!(
            FrameTimeline::new(start, options).stamp(start - Duration::from_millis(1)),
            Err(FrameError::TimeBeforeStart)
        );
    }

    #[test]
    fn recording_model_allows_window_targets_that_straddle_displays() {
        let window = CaptureTarget::window(11, 20, 30, screen(-100, 10, 200, 100)).unwrap();
        let request = RecordingRequest::new(
            window,
            screen(-100, 10, 200, 100),
            RecordingOptions::defaults_for(RecordingFormat::Gif),
        )
        .unwrap();
        assert_eq!(request.canvas(), CanvasSize::new(200, 100).unwrap());

        let display =
            CaptureTarget::display(42, "\\\\.\\DISPLAY1".into(), screen(0, 0, 100, 100)).unwrap();
        assert_eq!(
            RecordingRequest::new(
                display,
                screen(-1, 0, 100, 100),
                RecordingOptions::defaults_for(RecordingFormat::Gif),
            )
            .unwrap_err(),
            RequestError::RegionOutsideDisplay
        );
    }

    #[test]
    fn recording_timeline_checks_idle_time_and_clamps_terminal_elapsed() {
        let options = RecordingOptions::defaults_for(RecordingFormat::Gif);
        let start = Duration::from_secs(5);
        let mut timeline = FrameTimeline::new(start, options);
        assert_eq!(
            timeline.elapsed_at(start + Duration::from_secs(40)),
            Ok(Duration::from_secs(DEFAULT_RECORDING_SECONDS))
        );
    }
}
