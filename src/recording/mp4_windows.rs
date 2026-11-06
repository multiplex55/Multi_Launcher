//! A lazy, bounded Windows Media Foundation H.264 sink.
//!
//! This module has no Media Foundation static import. Systems without the
//! media components can start the launcher and receive a capability error
//! only when they request MP4.

use std::ffi::c_void;
use std::fs::OpenOptions;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use libloading::os::windows::{LOAD_LIBRARY_SEARCH_SYSTEM32, Library};
use windows::Win32::Media::MediaFoundation::{
    IMFMediaBuffer, IMFMediaType, IMFSample, IMFSinkWriter, IMFSourceReader, MF_MT_AVG_BITRATE,
    MF_MT_DEFAULT_STRIDE, MF_MT_FIXED_SIZE_SAMPLES, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE,
    MF_MT_INTERLACE_MODE, MF_MT_MAJOR_TYPE, MF_MT_MPEG2_PROFILE, MF_MT_PIXEL_ASPECT_RATIO,
    MF_MT_SAMPLE_SIZE, MF_MT_SUBTYPE, MF_MT_TRANSFER_FUNCTION, MF_MT_VIDEO_CHROMA_SITING,
    MF_MT_VIDEO_NOMINAL_RANGE, MF_MT_VIDEO_PRIMARIES, MF_MT_YUV_MATRIX, MF_VERSION,
    MFMediaType_Video, MFNominalRange_16_235, MFSTARTUP_FULL, MFVideoChromaSubsampling_MPEG1,
    MFVideoFormat_H264, MFVideoFormat_NV12, MFVideoInterlace_Progressive, MFVideoPrimaries_BT709,
    MFVideoTransFunc_709, MFVideoTransferMatrix_BT709, eAVEncH264VProfile_Base,
};
use windows::Win32::Media::MediaFoundation::{
    IMFMediaBuffer as MediaBuffer, IMFMediaType as MediaType, IMFSourceReader as SourceReader,
};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
use windows::core::HRESULT;
use windows::core::{Interface, PCWSTR};

use super::output::{FinalizedOutputStage, OutputStage, RecordingOutputError, publish_finalized};
use super::{
    CanvasSize, CaptureTerminal, ConsumerFrame, FrameRate, MAX_FRAME_BYTES, MAX_RECORDING_SECONDS,
    RecorderState, RecordingFormat, RecordingSession, RecordingSessionError,
};

const HNS_PER_SECOND: u128 = 10_000_000;
const MIN_BITRATE: u32 = 128_000;
const MAX_BITRATE: u32 = 24_000_000;
const BT709_STUDIO_BLACK: u8 = 16;
const BT709_CHROMA_CENTER: u8 = 128;
const NATIVE_READER_RGB_TOLERANCE: u8 = 32;

type MfStartup = unsafe extern "system" fn(u32, u32) -> HRESULT;
type MfShutdown = unsafe extern "system" fn() -> HRESULT;
type MfCreateMediaType = unsafe extern "system" fn(*mut *mut c_void) -> HRESULT;
type MfCreateMemoryBuffer = unsafe extern "system" fn(u32, *mut *mut c_void) -> HRESULT;
type MfCreateSample = unsafe extern "system" fn(*mut *mut c_void) -> HRESULT;
type MfCreateSinkWriterFromUrl =
    unsafe extern "system" fn(PCWSTR, *mut c_void, *mut c_void, *mut *mut c_void) -> HRESULT;
type MfCreateSourceReaderFromUrl =
    unsafe extern "system" fn(PCWSTR, *mut c_void, *mut *mut c_void) -> HRESULT;

/// COM must be initialized MTA on the thread that creates and uses this sink.
/// This guard is intentionally !Send so the owning operation cannot migrate.
struct MtaGuard(PhantomData<Rc<()>>);

impl MtaGuard {
    fn initialize() -> Result<Self, Mp4RecordingError> {
        let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        result.ok().map_err(|error| {
            Mp4RecordingError::Capability(format!(
                "H.264 recording needs a dedicated COM MTA worker: {error}"
            ))
        })?;
        Ok(Self(PhantomData))
    }
}

impl Drop for MtaGuard {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

/// Dynamically loaded system-only Media Foundation entry points.
struct MfRuntime {
    _mfplat: Library,
    _mfreadwrite: Library,
    create_media_type: MfCreateMediaType,
    create_memory_buffer: MfCreateMemoryBuffer,
    create_sample: MfCreateSample,
    create_sink_writer: MfCreateSinkWriterFromUrl,
    create_source_reader: MfCreateSourceReaderFromUrl,
    shutdown: MfShutdown,
    started: bool,
}

impl MfRuntime {
    fn start() -> Result<Self, Mp4RecordingError> {
        // MF DLLs are optional on Windows N editions; do not put them in the
        // executable's import table or search PATH for a similarly named DLL.
        let mfplat =
            unsafe { Library::load_with_flags("mfplat.dll", LOAD_LIBRARY_SEARCH_SYSTEM32) }
                .map_err(|error| capability_error("mfplat.dll is unavailable", error))?;
        let mfreadwrite =
            unsafe { Library::load_with_flags("mfreadwrite.dll", LOAD_LIBRARY_SEARCH_SYSTEM32) }
                .map_err(|error| capability_error("mfreadwrite.dll is unavailable", error))?;

        let startup: MfStartup = unsafe { load_symbol(&mfplat, b"MFStartup\0")? };
        let shutdown: MfShutdown = unsafe { load_symbol(&mfplat, b"MFShutdown\0")? };
        let create_media_type: MfCreateMediaType =
            unsafe { load_symbol(&mfplat, b"MFCreateMediaType\0")? };
        let create_memory_buffer: MfCreateMemoryBuffer =
            unsafe { load_symbol(&mfplat, b"MFCreateMemoryBuffer\0")? };
        let create_sample: MfCreateSample = unsafe { load_symbol(&mfplat, b"MFCreateSample\0")? };
        let create_sink_writer: MfCreateSinkWriterFromUrl =
            unsafe { load_symbol(&mfreadwrite, b"MFCreateSinkWriterFromURL\0")? };
        let create_source_reader: MfCreateSourceReaderFromUrl =
            unsafe { load_symbol(&mfreadwrite, b"MFCreateSourceReaderFromURL\0")? };
        let result = unsafe { startup(MF_VERSION, MFSTARTUP_FULL) };
        result.ok().map_err(|error| {
            Mp4RecordingError::Capability(format!(
                "Windows Media Foundation could not start H.264 encoding: {error}"
            ))
        })?;
        Ok(Self {
            _mfplat: mfplat,
            _mfreadwrite: mfreadwrite,
            create_media_type,
            create_memory_buffer,
            create_sample,
            create_sink_writer,
            create_source_reader,
            shutdown,
            started: true,
        })
    }

    fn create_media_type(&self) -> Result<MediaType, Mp4RecordingError> {
        let mut raw = std::ptr::null_mut();
        let result = unsafe { (self.create_media_type)(&mut raw) };
        result
            .ok()
            .map_err(|error| native_error("MFCreateMediaType failed", error))?;
        if raw.is_null() {
            return Err(Mp4RecordingError::Native(
                "MFCreateMediaType returned no media type".into(),
            ));
        }
        Ok(unsafe { MediaType::from_raw(raw) })
    }

    fn create_buffer(&self, capacity: u32) -> Result<MediaBuffer, Mp4RecordingError> {
        let mut raw = std::ptr::null_mut();
        let result = unsafe { (self.create_memory_buffer)(capacity, &mut raw) };
        result
            .ok()
            .map_err(|error| native_error("MFCreateMemoryBuffer failed", error))?;
        if raw.is_null() {
            return Err(Mp4RecordingError::Native(
                "MFCreateMemoryBuffer returned no buffer".into(),
            ));
        }
        Ok(unsafe { MediaBuffer::from_raw(raw) })
    }

    fn create_sample(&self) -> Result<IMFSample, Mp4RecordingError> {
        let mut raw = std::ptr::null_mut();
        let result = unsafe { (self.create_sample)(&mut raw) };
        result
            .ok()
            .map_err(|error| native_error("MFCreateSample failed", error))?;
        if raw.is_null() {
            return Err(Mp4RecordingError::Native(
                "MFCreateSample returned no sample".into(),
            ));
        }
        Ok(unsafe { IMFSample::from_raw(raw) })
    }

    fn create_sink_writer(&self, path: &Path) -> Result<IMFSinkWriter, Mp4RecordingError> {
        let path = wide_path(path)?;
        let mut raw = std::ptr::null_mut();
        let result = unsafe {
            (self.create_sink_writer)(
                PCWSTR(path.as_ptr()),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut raw,
            )
        };
        result
            .ok()
            .map_err(|error| capability_error("the local MP4 sink writer is unavailable", error))?;
        if raw.is_null() {
            return Err(Mp4RecordingError::Native(
                "MFCreateSinkWriterFromURL returned no writer".into(),
            ));
        }
        Ok(unsafe { IMFSinkWriter::from_raw(raw) })
    }

    fn create_source_reader(&self, path: &Path) -> Result<SourceReader, Mp4RecordingError> {
        let path = wide_path(path)?;
        let mut raw = std::ptr::null_mut();
        let result = unsafe {
            (self.create_source_reader)(PCWSTR(path.as_ptr()), std::ptr::null_mut(), &mut raw)
        };
        result.ok().map_err(|error| {
            capability_error("the local MP4 source reader is unavailable", error)
        })?;
        if raw.is_null() {
            return Err(Mp4RecordingError::Native(
                "MFCreateSourceReaderFromURL returned no reader".into(),
            ));
        }
        Ok(unsafe { SourceReader::from_raw(raw) })
    }

    fn shutdown(&mut self) -> Result<(), Mp4RecordingError> {
        if !self.started {
            return Ok(());
        }
        let result = unsafe { (self.shutdown)() };
        self.started = false;
        result
            .ok()
            .map_err(|error| native_error("MFShutdown failed", error))
    }
}

impl Drop for MfRuntime {
    fn drop(&mut self) {
        if self.started {
            let _ = unsafe { (self.shutdown)() };
            self.started = false;
        }
    }
}

unsafe fn load_symbol<T: Copy>(
    library: &Library,
    name: &'static [u8],
) -> Result<T, Mp4RecordingError> {
    let symbol = unsafe { library.get::<T>(name) }.map_err(|error| {
        Mp4RecordingError::Capability(format!(
            "Windows Media Foundation entry point {} is unavailable: {error}",
            String::from_utf8_lossy(&name[..name.len().saturating_sub(1)])
        ))
    })?;
    Ok(*symbol)
}

fn capability_error(context: &str, error: impl std::fmt::Display) -> Mp4RecordingError {
    Mp4RecordingError::Capability(format!(
        "{context}. Install the Windows Media Feature Pack or choose GIF: {error}"
    ))
}

fn native_error(context: &str, error: impl std::fmt::Display) -> Mp4RecordingError {
    Mp4RecordingError::Native(format!("{context}: {error}"))
}

fn wide_path(path: &Path) -> Result<Vec<u16>, Mp4RecordingError> {
    use std::os::windows::ffi::OsStrExt;

    if path.as_os_str().as_encoded_bytes().contains(&0) {
        return Err(Mp4RecordingError::InvalidInput(
            "the MP4 staging path contains a NUL".into(),
        ));
    }
    Ok(path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Nv12Layout {
    width: u32,
    height: u32,
    y_bytes: usize,
    total_bytes: usize,
}

impl Nv12Layout {
    fn for_canvas(canvas: CanvasSize) -> Result<Self, Mp4RecordingError> {
        let (width, height) = canvas.mp4_padded_dimensions();
        if width > super::MAX_CANVAS_WIDTH || height > super::MAX_CANVAS_HEIGHT {
            return Err(Mp4RecordingError::InvalidInput(
                "the MP4 padded dimensions exceed the recording canvas limit".into(),
            ));
        }
        let y_bytes = usize::try_from(u64::from(width) * u64::from(height))
            .map_err(|_| Mp4RecordingError::AllocationFailed)?;
        let total_bytes = y_bytes
            .checked_add(y_bytes / 2)
            .filter(|length| *length <= MAX_FRAME_BYTES)
            .ok_or(Mp4RecordingError::AllocationFailed)?;
        Ok(Self {
            width,
            height,
            y_bytes,
            total_bytes,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TimedNv12Frame {
    source_tick: u64,
    display_tick: u64,
    bytes: Vec<u8>,
}

fn ticks_for(elapsed: Duration) -> Result<u64, Mp4RecordingError> {
    if elapsed > Duration::from_secs(MAX_RECORDING_SECONDS) {
        return Err(Mp4RecordingError::InvalidInput(
            "the MP4 timestamp exceeds the 120-second recording limit".into(),
        ));
    }
    let ticks = elapsed.as_nanos() / 100;
    u64::try_from(ticks)
        .ok()
        .filter(|tick| *tick <= (u128::from(MAX_RECORDING_SECONDS) * HNS_PER_SECOND) as u64)
        .ok_or(Mp4RecordingError::InvalidInput(
            "the MP4 timestamp exceeds the 120-second recording limit".into(),
        ))
}

fn frame_tick_for(
    previous_elapsed: Option<Duration>,
    elapsed: Duration,
    previous_source_tick: Option<u64>,
) -> Result<u64, Mp4RecordingError> {
    if previous_elapsed.is_some_and(|previous| elapsed < previous) {
        return Err(Mp4RecordingError::TimestampMovedBackwards);
    }
    let tick = ticks_for(elapsed)?;
    if previous_source_tick.is_some_and(|previous| tick < previous) {
        return Err(Mp4RecordingError::TimestampMovedBackwards);
    }
    Ok(tick)
}

fn terminal_tick_for(
    terminal_elapsed: Duration,
    previous_elapsed: Option<Duration>,
    pending_source_tick: u64,
) -> Result<u64, Mp4RecordingError> {
    if previous_elapsed.is_some_and(|previous| terminal_elapsed < previous) {
        return Err(Mp4RecordingError::FrameAfterTerminal);
    }
    let tick = ticks_for(terminal_elapsed)?;
    if tick < pending_source_tick {
        return Err(Mp4RecordingError::FrameAfterTerminal);
    }
    Ok(tick)
}

fn h264_bitrate(width: u32, height: u32, fps: u32) -> Result<u32, Mp4RecordingError> {
    u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(u64::from(fps)))
        .map(|pixel_frames| {
            u32::try_from(pixel_frames / 8)
                .unwrap_or(u32::MAX)
                .clamp(MIN_BITRATE, MAX_BITRATE)
        })
        .ok_or(Mp4RecordingError::InvalidInput(
            "MP4 bitrate overflow".into(),
        ))
}

fn frame_interval_ticks(rate: FrameRate) -> Result<u64, Mp4RecordingError> {
    u64::try_from(HNS_PER_SECOND / u128::from(rate.frames_per_second()))
        .map_err(|_| Mp4RecordingError::InvalidInput("invalid MP4 frame rate".into()))
}

/// Converts RGBA to even-sized BT.709 limited-range NV12. Odd right/bottom
/// edges are padded with black. Chroma is the mean of each 2×2 block.
fn rgba_to_nv12(
    canvas: CanvasSize,
    frame_size: CanvasSize,
    rgba: &[u8],
) -> Result<(Nv12Layout, Vec<u8>), Mp4RecordingError> {
    if frame_size != canvas {
        return Err(Mp4RecordingError::InvalidInput(
            "the captured frame dimensions do not match the frozen canvas".into(),
        ));
    }
    if rgba.len() != canvas.rgba_bytes() {
        return Err(Mp4RecordingError::InvalidInput(
            "the captured RGBA frame length does not match the frozen canvas".into(),
        ));
    }
    let layout = Nv12Layout::for_canvas(canvas)?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(layout.total_bytes)
        .map_err(|_| Mp4RecordingError::AllocationFailed)?;
    output.resize(layout.total_bytes, 0);
    output[layout.y_bytes..].fill(BT709_CHROMA_CENTER);

    for y in 0..layout.height {
        for x in 0..layout.width {
            let (r, g, b) = rgba_pixel(canvas, rgba, x, y);
            output[(y * layout.width + x) as usize] = bt709_y(r, g, b);
        }
    }
    let uv_start = layout.y_bytes;
    for y in (0..layout.height).step_by(2) {
        for x in (0..layout.width).step_by(2) {
            let mut cb = 0_i32;
            let mut cr = 0_i32;
            for dy in 0..2 {
                for dx in 0..2 {
                    let (r, g, b) = rgba_pixel(canvas, rgba, x + dx, y + dy);
                    let (_, pixel_cb, pixel_cr) = bt709_yuv(r, g, b);
                    cb += i32::from(pixel_cb);
                    cr += i32::from(pixel_cr);
                }
            }
            let index = uv_start + (y as usize / 2) * layout.width as usize + x as usize;
            output[index] = ((cb + 2) / 4) as u8;
            output[index + 1] = ((cr + 2) / 4) as u8;
        }
    }
    Ok((layout, output))
}

fn rgba_pixel(canvas: CanvasSize, rgba: &[u8], x: u32, y: u32) -> (u8, u8, u8) {
    if x >= canvas.width() || y >= canvas.height() {
        return (0, 0, 0);
    }
    let offset = ((y * canvas.width() + x) * 4) as usize;
    let alpha = i32::from(rgba[offset + 3]);
    let flatten = |channel: u8| ((i32::from(channel) * alpha + 127) / 255) as u8;
    (
        flatten(rgba[offset]),
        flatten(rgba[offset + 1]),
        flatten(rgba[offset + 2]),
    )
}

fn bt709_y(r: u8, g: u8, b: u8) -> u8 {
    bt709_yuv(r, g, b).0
}

fn bt709_yuv(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    let r = i32::from(r);
    let g = i32::from(g);
    let b = i32::from(b);
    let y = 16 + (47 * r + 157 * g + 16 * b + 128).div_euclid(256);
    let cb = 128 + (-26 * r - 87 * g + 113 * b + 128).div_euclid(256);
    let cr = 128 + (112 * r - 102 * g - 10 * b + 128).div_euclid(256);
    (
        y.clamp(i32::from(BT709_STUDIO_BLACK), 235) as u8,
        cb.clamp(16, 240) as u8,
        cr.clamp(16, 240) as u8,
    )
}

fn set_size_attribute(
    media_type: &MediaType,
    key: &windows::core::GUID,
    width: u32,
    height: u32,
) -> Result<(), Mp4RecordingError> {
    let packed = (u64::from(width) << 32) | u64::from(height);
    unsafe { media_type.SetUINT64(key, packed) }
        .map_err(|error| native_error("could not configure MP4 dimensions", error))
}

fn set_ratio_attribute(
    media_type: &MediaType,
    key: &windows::core::GUID,
    numerator: u32,
    denominator: u32,
) -> Result<(), Mp4RecordingError> {
    let packed = (u64::from(numerator) << 32) | u64::from(denominator);
    unsafe { media_type.SetUINT64(key, packed) }
        .map_err(|error| native_error("could not configure MP4 ratio", error))
}

fn configure_video_attributes(
    media_type: &MediaType,
    subtype: &windows::core::GUID,
    width: u32,
    height: u32,
    fps: u32,
    input_nv12: bool,
) -> Result<(), Mp4RecordingError> {
    unsafe {
        media_type
            .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
            .map_err(|error| native_error("could not set MP4 video stream type", error))?;
        media_type
            .SetGUID(&MF_MT_SUBTYPE, subtype)
            .map_err(|error| native_error("could not set MP4 pixel format", error))?;
    }
    set_size_attribute(media_type, &MF_MT_FRAME_SIZE, width, height)?;
    set_ratio_attribute(media_type, &MF_MT_FRAME_RATE, fps, 1)?;
    set_ratio_attribute(media_type, &MF_MT_PIXEL_ASPECT_RATIO, 1, 1)?;
    unsafe {
        media_type
            .SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)
            .map_err(|error| native_error("could not set progressive MP4 video", error))?;
    }
    if input_nv12 {
        let sample_size = u32::try_from(
            usize::try_from(u64::from(width) * u64::from(height))
                .ok()
                .and_then(|pixels| pixels.checked_mul(3))
                .map(|bytes| bytes / 2)
                .ok_or(Mp4RecordingError::AllocationFailed)?,
        )
        .map_err(|_| Mp4RecordingError::AllocationFailed)?;
        unsafe {
            media_type
                .SetUINT32(&MF_MT_DEFAULT_STRIDE, width)
                .map_err(|error| native_error("could not set NV12 stride", error))?;
            media_type
                .SetUINT32(&MF_MT_FIXED_SIZE_SAMPLES, 1)
                .map_err(|error| native_error("could not set NV12 sample type", error))?;
            media_type
                .SetUINT32(&MF_MT_SAMPLE_SIZE, sample_size)
                .map_err(|error| native_error("could not set NV12 sample size", error))?;
        }
    } else {
        let bitrate = h264_bitrate(width, height, fps)?;
        unsafe {
            media_type
                .SetUINT32(&MF_MT_AVG_BITRATE, bitrate)
                .map_err(|error| native_error("could not set the H.264 bitrate", error))?;
            // H.264 Baseline profile has standardized profile_idc 66.
            media_type
                .SetUINT32(&MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_Base.0 as u32)
                .map_err(|error| native_error("could not request H.264 Baseline", error))?;
        }
    }
    // The sink's encoded media type and NV12 input describe the same color
    // interpretation, so a reader never has to guess a different matrix.
    unsafe {
        media_type
            .SetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE, MFNominalRange_16_235.0 as u32)
            .map_err(|error| native_error("could not set BT.709 nominal range", error))?;
        media_type
            .SetUINT32(&MF_MT_VIDEO_PRIMARIES, MFVideoPrimaries_BT709.0 as u32)
            .map_err(|error| native_error("could not set BT.709 primaries", error))?;
        media_type
            .SetUINT32(&MF_MT_TRANSFER_FUNCTION, MFVideoTransFunc_709.0 as u32)
            .map_err(|error| native_error("could not set BT.709 transfer", error))?;
        media_type
            .SetUINT32(&MF_MT_YUV_MATRIX, MFVideoTransferMatrix_BT709.0 as u32)
            .map_err(|error| native_error("could not set BT.709 matrix", error))?;
        media_type
            .SetUINT32(
                &MF_MT_VIDEO_CHROMA_SITING,
                MFVideoChromaSubsampling_MPEG1.0 as u32,
            )
            .map_err(|error| native_error("could not set BT.709 chroma siting", error))?;
    }
    Ok(())
}

struct PendingSample {
    source_tick: u64,
    display_tick: u64,
    bytes: Vec<u8>,
}

/// Session-bound MP4 transaction. It is deliberately !Send: construct and
/// drive it on its dedicated MTA writer thread, then retire it before shutdown.
pub(crate) struct Mp4Recording {
    session: Arc<RecordingSession>,
    generation: u64,
    canvas: CanvasSize,
    output_layout: Nv12Layout,
    fps: FrameRate,
    apartment: Option<MtaGuard>,
    runtime: Option<MfRuntime>,
    sink: Option<IMFSinkWriter>,
    stream_index: u32,
    stage: Option<OutputStage>,
    pending: Option<PendingSample>,
    last_elapsed: Option<Duration>,
    acknowledged_inputs: u64,
    encoded_samples: u64,
    _thread_affinity: PhantomData<Rc<()>>,
}

impl Mp4Recording {
    pub(crate) fn create(
        session: Arc<RecordingSession>,
        generation: u64,
        destination: impl AsRef<Path>,
    ) -> Result<Self, Mp4RecordingError> {
        let view = session.view();
        if view.generation() != generation {
            return Err(Mp4RecordingError::Session(
                RecordingSessionError::StaleGeneration,
            ));
        }
        if view.worker_busy() || !matches!(view.state(), RecorderState::Selecting) {
            return Err(Mp4RecordingError::InvalidTransition);
        }
        let request = view.request().ok_or(Mp4RecordingError::Session(
            RecordingSessionError::MissingSelection,
        ))?;
        if request.options().format() != RecordingFormat::Mp4 {
            return Err(Mp4RecordingError::UnsupportedFormat);
        }
        let canvas = request.canvas();
        let fps = request.options().frame_rate();
        let output_layout = Nv12Layout::for_canvas(canvas)?;

        let apartment = MtaGuard::initialize()?;
        let runtime = MfRuntime::start()?;
        let (stage, initial_file) = OutputStage::create(destination, RecordingFormat::Mp4)
            .map_err(|error| Mp4RecordingError::Stage(error.to_string()))?;
        drop(initial_file);

        let setup = Self::create_sink(&runtime, stage.temporary_path(), output_layout, fps);
        let (sink, stream_index) = match setup {
            Ok(setup) => setup,
            Err(error) => {
                drop(runtime);
                drop(apartment);
                let cleanup = stage.discard();
                return Err(with_cleanup(error, cleanup));
            }
        };

        Ok(Self {
            session,
            generation,
            canvas,
            output_layout,
            fps,
            apartment: Some(apartment),
            runtime: Some(runtime),
            sink: Some(sink),
            stream_index,
            stage: Some(stage),
            pending: None,
            last_elapsed: None,
            acknowledged_inputs: 0,
            encoded_samples: 0,
            _thread_affinity: PhantomData,
        })
    }

    fn create_sink(
        runtime: &MfRuntime,
        stage_path: &Path,
        layout: Nv12Layout,
        fps: FrameRate,
    ) -> Result<(IMFSinkWriter, u32), Mp4RecordingError> {
        let writer = runtime.create_sink_writer(stage_path)?;
        let output_type = runtime.create_media_type()?;
        configure_video_attributes(
            &output_type,
            &MFVideoFormat_H264,
            layout.width,
            layout.height,
            u32::from(fps.frames_per_second()),
            false,
        )?;
        let input_type = runtime.create_media_type()?;
        configure_video_attributes(
            &input_type,
            &MFVideoFormat_NV12,
            layout.width,
            layout.height,
            u32::from(fps.frames_per_second()),
            true,
        )?;
        let stream = unsafe { writer.AddStream(&output_type) }
            .map_err(|error| capability_error("no H.264 MP4 encoder accepts this size", error))?;
        unsafe { writer.SetInputMediaType(stream, &input_type, None) }
            .map_err(|error| capability_error("no NV12 to H.264 encoder is available", error))?;
        unsafe { writer.BeginWriting() }
            .map_err(|error| capability_error("H.264 MP4 encoding could not begin", error))?;
        Ok((writer, stream))
    }

    /// Encodes and leases only frames from this transaction's session. It
    /// acknowledges a frame only after the one-sample staging boundary accepts it.
    pub(crate) fn consume_latest(&mut self) -> Result<Option<Duration>, Mp4RecordingError> {
        let view = self.session.view();
        if view.generation() != self.generation {
            return Err(self.fail_sink(Mp4RecordingError::Session(
                RecordingSessionError::StaleGeneration,
            )));
        }
        if !matches!(
            view.state(),
            RecorderState::Recording { .. } | RecorderState::Finalizing { .. }
        ) {
            // Selection, countdown, and native source startup are normal wait
            // states. Do not turn them into a failed writer transaction.
            return Ok(None);
        }
        match self.session.take_latest_frame(self.generation) {
            Ok(Some(frame)) => self.write_frame(frame).map(Some),
            Ok(None) => Ok(None),
            Err(error) => Err(self.fail_sink(Mp4RecordingError::Session(error))),
        }
    }

    /// Actual encoded dimensions, including any one-pixel black matte padding.
    pub(crate) const fn output_dimensions(&self) -> (u32, u32) {
        (self.output_layout.width, self.output_layout.height)
    }

    fn write_frame(&mut self, frame: ConsumerFrame) -> Result<Duration, Mp4RecordingError> {
        if frame.generation() != self.generation {
            drop(frame);
            return Err(self.fail_sink(Mp4RecordingError::Session(
                RecordingSessionError::StaleGeneration,
            )));
        }
        let timed = frame.frame();
        let elapsed = timed.elapsed();
        let tick = match frame_tick_for(
            self.last_elapsed,
            elapsed,
            self.pending.as_ref().map(|pending| pending.source_tick),
        ) {
            Ok(tick) => tick,
            Err(error) => {
                drop(frame);
                return Err(self.fail_sink(error));
            }
        };
        let (layout, bytes) =
            match rgba_to_nv12(self.canvas, timed.frame().size(), timed.frame().pixels()) {
                Ok(converted) => converted,
                Err(error) => {
                    drop(frame);
                    return Err(self.fail_sink(error));
                }
            };
        if layout != self.output_layout {
            drop(frame);
            return Err(self.fail_sink(Mp4RecordingError::InvalidInput(
                "NV12 conversion changed the frozen output dimensions".into(),
            )));
        }
        if let Some(pending) = self.pending.as_mut() {
            if tick == pending.source_tick {
                // Coalesce updates on the same 100ns boundary while retaining
                // the original displayed boundary.
                pending.source_tick = tick;
                pending.bytes = bytes;
            } else {
                let pending = self.pending.take().expect("pending sample was observed");
                if let Err(error) = self.write_sample(pending, tick) {
                    drop(frame);
                    return Err(self.fail_sink(error));
                }
                self.pending = Some(PendingSample {
                    source_tick: tick,
                    display_tick: tick,
                    bytes,
                });
            }
        } else {
            self.pending = Some(PendingSample {
                source_tick: tick,
                display_tick: 0,
                bytes,
            });
        }
        if let Err(error) = frame.acknowledge_written() {
            return Err(self.fail_sink(Mp4RecordingError::Session(error)));
        }
        self.last_elapsed = Some(elapsed);
        self.acknowledged_inputs = self.acknowledged_inputs.saturating_add(1);
        Ok(elapsed)
    }

    fn write_sample(
        &mut self,
        pending: PendingSample,
        next_tick: u64,
    ) -> Result<(), Mp4RecordingError> {
        let duration = next_tick
            .checked_sub(pending.display_tick)
            .ok_or(Mp4RecordingError::TimestampMovedBackwards)?;
        if duration == 0 {
            return Err(Mp4RecordingError::TimestampMovedBackwards);
        }
        let runtime = self
            .runtime
            .as_ref()
            .ok_or(Mp4RecordingError::InvalidTransition)?;
        let sink = self
            .sink
            .as_ref()
            .ok_or(Mp4RecordingError::InvalidTransition)?;
        let buffer = runtime.create_buffer(
            u32::try_from(pending.bytes.len()).map_err(|_| Mp4RecordingError::AllocationFailed)?,
        )?;
        write_buffer(&buffer, &pending.bytes)?;
        let sample = runtime.create_sample()?;
        unsafe {
            sample
                .AddBuffer(&buffer)
                .map_err(|error| native_error("could not attach NV12 to an H.264 sample", error))?;
            sample
                .SetSampleTime(i64::try_from(pending.display_tick).map_err(|_| {
                    Mp4RecordingError::InvalidInput("MP4 timestamp overflow".into())
                })?)
                .map_err(|error| native_error("could not timestamp an H.264 sample", error))?;
            sample
                .SetSampleDuration(i64::try_from(duration).map_err(|_| {
                    Mp4RecordingError::InvalidInput("MP4 frame duration overflow".into())
                })?)
                .map_err(|error| native_error("could not set H.264 sample duration", error))?;
            sink.WriteSample(self.stream_index, &sample)
                .map_err(|error| native_error("H.264 MP4 sink rejected an NV12 sample", error))?;
        }
        self.encoded_samples = self.encoded_samples.saturating_add(1);
        Ok(())
    }

    /// Finalizes, releases Media Foundation, syncs the exact stage, and crosses
    /// the existing one-use session publication claim.
    pub(crate) fn finish_and_publish(mut self) -> Result<PathBuf, Mp4RecordingError> {
        let view = self.session.view();
        if view.generation() != self.generation {
            return Err(self.fail_sink(Mp4RecordingError::Session(
                RecordingSessionError::StaleGeneration,
            )));
        }
        if !matches!(view.state(), RecorderState::Finalizing { .. }) || view.worker_busy() {
            return Err(self.fail_sink(Mp4RecordingError::Session(
                RecordingSessionError::NotFinalizing,
            )));
        }
        let Some(terminal) = view
            .terminal()
            .filter(|terminal| terminal.generation() == self.generation)
        else {
            return Err(self.fail_sink(Mp4RecordingError::Session(
                RecordingSessionError::NotFinalizing,
            )));
        };
        let Some(last_pending) = self.pending.as_ref() else {
            return Err(self.fail_sink(Mp4RecordingError::EmptyClip));
        };
        let terminal_tick = match terminal_tick_for(
            terminal.elapsed(),
            self.last_elapsed,
            last_pending.source_tick,
        ) {
            Ok(tick) => tick,
            Err(error) => return Err(self.fail_sink(error)),
        };
        let Some(pending) = self.pending.take() else {
            return Err(self.fail_sink(Mp4RecordingError::EmptyClip));
        };
        if terminal_tick < pending.source_tick {
            return Err(self.fail_sink(Mp4RecordingError::FrameAfterTerminal));
        }
        if terminal_tick == pending.display_tick {
            if self.encoded_samples == 0 {
                let minimum = match frame_interval_ticks(self.fps) {
                    Ok(ticks) => ticks.max(1),
                    Err(error) => return Err(self.fail_sink(error)),
                };
                let mut pending = pending;
                pending.display_tick = terminal_tick;
                let next_tick = terminal_tick.checked_add(minimum).ok_or_else(|| {
                    Mp4RecordingError::InvalidInput("MP4 timestamp overflow".into())
                });
                match next_tick.and_then(|tick| self.write_sample(pending, tick).map(|()| tick)) {
                    Ok(_) => {}
                    Err(error) => return Err(self.fail_sink(error)),
                }
            }
        } else if let Err(error) = self.write_sample(pending, terminal_tick) {
            return Err(self.fail_sink(error));
        }

        let Some(sink) = self.sink.take() else {
            return Err(self.fail_sink(Mp4RecordingError::InvalidTransition));
        };
        let finalize = unsafe { sink.Finalize() };
        drop(sink);
        if let Err(error) = finalize {
            return Err(self.fail_sink(native_error("H.264 MP4 finalization failed", error)));
        }
        if let Some(runtime) = self.runtime.as_mut()
            && let Err(error) = runtime.shutdown()
        {
            return Err(self.fail_sink(error));
        }
        self.runtime.take();
        self.apartment.take();

        let Some(stage) = self.stage.take() else {
            return Err(Mp4RecordingError::InvalidTransition);
        };
        let staged_path = stage.temporary_path().to_path_buf();
        let file = match OpenOptions::new().read(true).write(true).open(&staged_path) {
            Ok(file) => file,
            Err(error) => {
                let stage_error = Mp4RecordingError::Stage(format!(
                    "could not open finalized MP4 for sync at {}: {error}",
                    staged_path.display()
                ));
                let cleanup = stage.discard();
                let failure = with_cleanup(stage_error, cleanup);
                let _ = self.session.fail_sink(self.generation, failure.to_string());
                return Err(failure);
            }
        };
        let finalized = match stage.finalize_file(file) {
            Ok(finalized) => finalized,
            Err(error) => {
                let failure = Mp4RecordingError::Stage(error.to_string());
                let _ = self.session.fail_sink(self.generation, failure.to_string());
                return Err(failure);
            }
        };
        let result = publish_finalized(
            &self.session,
            self.generation,
            terminal,
            self.acknowledged_inputs,
            finalized,
        );
        result.map_err(Mp4RecordingError::Publication)
    }

    pub(crate) fn discard(mut self) -> Result<(), Mp4RecordingError> {
        self.release_native();
        match self.stage.take() {
            Some(stage) => stage
                .discard()
                .map_err(|error| Mp4RecordingError::Stage(error.to_string())),
            None => Ok(()),
        }
    }

    fn fail_sink(&mut self, error: Mp4RecordingError) -> Mp4RecordingError {
        let _ = self.session.fail_sink(self.generation, error.to_string());
        self.release_native();
        let Some(stage) = self.stage.take() else {
            return error;
        };
        match stage.discard() {
            Ok(()) => error,
            Err(cleanup) => Mp4RecordingError::WithCleanup {
                cause: error.to_string(),
                cleanup: cleanup.to_string(),
            },
        }
    }

    fn release_native(&mut self) {
        self.pending.take();
        self.sink.take();
        if let Some(runtime) = self.runtime.as_mut() {
            let _ = runtime.shutdown();
        }
        self.runtime.take();
        self.apartment.take();
    }
}

impl Drop for Mp4Recording {
    fn drop(&mut self) {
        self.release_native();
        if let Some(stage) = self.stage.take() {
            let _ = stage.discard();
        }
    }
}

struct LockedMediaBuffer<'a> {
    buffer: &'a IMFMediaBuffer,
    data: *mut u8,
    capacity: u32,
    locked: bool,
}

impl<'a> LockedMediaBuffer<'a> {
    fn lock(buffer: &'a IMFMediaBuffer) -> Result<Self, Mp4RecordingError> {
        let mut data = std::ptr::null_mut();
        let mut capacity = 0_u32;
        unsafe { buffer.Lock(&mut data, Some(&mut capacity), None) }
            .map_err(|error| native_error("could not lock an NV12 media buffer", error))?;
        Ok(Self {
            buffer,
            data,
            capacity,
            locked: true,
        })
    }

    fn copy_from(&mut self, bytes: &[u8]) -> Result<u32, Mp4RecordingError> {
        let expected =
            u32::try_from(bytes.len()).map_err(|_| Mp4RecordingError::AllocationFailed)?;
        if self.data.is_null() || self.capacity < expected {
            return Err(Mp4RecordingError::Native(format!(
                "Media Foundation returned an undersized NV12 buffer ({} < {expected})",
                self.capacity
            )));
        }
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), self.data, bytes.len()) };
        Ok(expected)
    }

    fn unlock(&mut self) -> Result<(), Mp4RecordingError> {
        if !self.locked {
            return Ok(());
        }
        unsafe { self.buffer.Unlock() }
            .map_err(|error| native_error("could not unlock an NV12 media buffer", error))?;
        self.locked = false;
        Ok(())
    }
}

impl Drop for LockedMediaBuffer<'_> {
    fn drop(&mut self) {
        if self.locked {
            let _ = unsafe { self.buffer.Unlock() };
            self.locked = false;
        }
    }
}

fn write_buffer(buffer: &IMFMediaBuffer, bytes: &[u8]) -> Result<(), Mp4RecordingError> {
    let mut locked = LockedMediaBuffer::lock(buffer)?;
    let expected = locked.copy_from(bytes)?;
    locked.unlock()?;
    unsafe { buffer.SetCurrentLength(expected) }
        .map_err(|error| native_error("could not set NV12 buffer length", error))
}

fn with_cleanup<T, C>(cause: Mp4RecordingError, cleanup: Result<T, C>) -> Mp4RecordingError
where
    C: std::fmt::Display,
{
    match cleanup {
        Ok(_) => cause,
        Err(cleanup) => Mp4RecordingError::WithCleanup {
            cause: cause.to_string(),
            cleanup: cleanup.to_string(),
        },
    }
}

#[derive(Debug)]
pub(crate) enum Mp4RecordingError {
    Capability(String),
    Native(String),
    Stage(String),
    Session(RecordingSessionError),
    Publication(RecordingOutputError),
    UnsupportedFormat,
    InvalidTransition,
    InvalidInput(String),
    TimestampMovedBackwards,
    FrameAfterTerminal,
    EmptyClip,
    AllocationFailed,
    WithCleanup { cause: String, cleanup: String },
}

impl std::fmt::Display for Mp4RecordingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Capability(message)
            | Self::Native(message)
            | Self::Stage(message)
            | Self::InvalidInput(message) => formatter.write_str(message),
            Self::Publication(error) => error.fmt(formatter),
            Self::Session(error) => error.fmt(formatter),
            Self::UnsupportedFormat => {
                formatter.write_str("this recording output only accepts MP4 requests")
            }
            Self::InvalidTransition => formatter.write_str("the MP4 recording is no longer active"),
            Self::TimestampMovedBackwards => {
                formatter.write_str("MP4 frame timestamps moved backwards")
            }
            Self::FrameAfterTerminal => {
                formatter.write_str("an MP4 frame arrived after the terminal time")
            }
            Self::EmptyClip => formatter.write_str("cannot publish an MP4 with no captured frames"),
            Self::AllocationFailed => {
                formatter.write_str("could not allocate the bounded NV12 frame")
            }
            Self::WithCleanup { cause, cleanup } => write!(formatter, "{cause}; {cleanup}"),
        }
    }
}

impl std::error::Error for Mp4RecordingError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::ScreenRect;
    use crate::recording::{
        CapturePoll, CaptureSource, CaptureStopToken, CaptureTarget, FrameRate, RecordingBackend,
        RecordingOptions, RecordingRequest, RgbaFrame, SystemRecorderClock,
    };
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
    use std::thread;
    use std::time::Instant;
    use windows::Win32::Media::MediaFoundation::{
        IMF2DBuffer, MF_E_INVALIDSTREAMNUMBER, MF_PD_DURATION, MF_SOURCE_READER_FIRST_VIDEO_STREAM,
        MF_SOURCE_READER_MEDIASOURCE, MF_SOURCE_READERF_ENDOFSTREAM,
    };

    fn canvas(width: u32, height: u32) -> CanvasSize {
        CanvasSize::new(width, height).unwrap()
    }

    fn solid_rgba(width: u32, height: u32, color: (u8, u8, u8)) -> Vec<u8> {
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for _ in 0..width * height {
            pixels.extend_from_slice(&[color.0, color.1, color.2, 255]);
        }
        pixels
    }

    fn pixels_rgba(
        width: u32,
        height: u32,
        mut color_at: impl FnMut(u32, u32) -> (u8, u8, u8),
    ) -> Vec<u8> {
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                let (r, g, b) = color_at(x, y);
                pixels.extend_from_slice(&[r, g, b, 255]);
            }
        }
        pixels
    }

    #[test]
    fn nv12_layout_pads_odd_dimensions_and_checks_canvas_bounds() {
        let odd = Nv12Layout::for_canvas(canvas(161, 91)).unwrap();
        assert_eq!((odd.width, odd.height), (162, 92));
        assert_eq!(odd.y_bytes, 162 * 92);
        assert_eq!(odd.total_bytes, 162 * 92 * 3 / 2);

        let even = Nv12Layout::for_canvas(canvas(160, 90)).unwrap();
        assert_eq!((even.width, even.height), (160, 90));
        assert_eq!(even.total_bytes, 160 * 90 * 3 / 2);

        let largest = Nv12Layout::for_canvas(canvas(1_919, 1_079)).unwrap();
        assert_eq!((largest.width, largest.height), (1_920, 1_080));
        assert!(largest.total_bytes <= MAX_FRAME_BYTES);
    }

    #[test]
    fn rgba_to_nv12_rejects_shape_and_byte_mismatches() {
        let size = canvas(2, 2);
        let pixels = solid_rgba(2, 2, (10, 20, 30));
        assert!(matches!(
            rgba_to_nv12(size, canvas(1, 4), &pixels),
            Err(Mp4RecordingError::InvalidInput(_))
        ));
        assert!(matches!(
            rgba_to_nv12(size, size, &pixels[..pixels.len() - 1]),
            Err(Mp4RecordingError::InvalidInput(_))
        ));
    }

    #[test]
    fn bt709_limited_range_known_colors_and_gray_are_neutral() {
        assert_eq!(bt709_yuv(0, 0, 0), (16, 128, 128));
        assert_eq!(bt709_yuv(255, 255, 255), (235, 128, 128));
        assert_eq!(bt709_yuv(128, 128, 128), (126, 128, 128));
        assert_eq!(bt709_yuv(255, 0, 0), (63, 102, 240));
        assert_eq!(bt709_yuv(0, 255, 0), (172, 41, 26));
        assert_eq!(bt709_yuv(0, 0, 255), (32, 240, 118));
    }

    #[test]
    fn nv12_chroma_averages_centered_two_by_two_blocks_and_black_matte() {
        let size = canvas(2, 2);
        let pixels = pixels_rgba(2, 2, |_, y| if y == 0 { (255, 0, 0) } else { (0, 0, 255) });
        let (layout, nv12) = rgba_to_nv12(size, size, &pixels).unwrap();
        assert_eq!(&nv12[layout.y_bytes..], &[171, 179]);

        let size = canvas(3, 3);
        let pixels = pixels_rgba(3, 3, |x, y| {
            if x == 2 && y < 2 {
                (0, 255, 0)
            } else {
                (0, 0, 0)
            }
        });
        let (layout, nv12) = rgba_to_nv12(size, size, &pixels).unwrap();
        assert_eq!((layout.width, layout.height), (4, 4));
        assert_eq!(nv12[3], BT709_STUDIO_BLACK);
        assert_eq!(nv12[3 * layout.width as usize], BT709_STUDIO_BLACK);
        assert_eq!(&nv12[layout.y_bytes + 2..layout.y_bytes + 4], &[85, 77]);
        assert_eq!(
            &nv12[layout.y_bytes + layout.width as usize..][..2],
            &[128, 128]
        );
    }

    #[test]
    fn timestamps_use_absolute_hundred_nanosecond_boundaries_and_cap() {
        assert_eq!(ticks_for(Duration::ZERO).unwrap(), 0);
        assert_eq!(ticks_for(Duration::from_nanos(1)).unwrap(), 0);
        assert_eq!(ticks_for(Duration::from_nanos(99)).unwrap(), 0);
        assert_eq!(ticks_for(Duration::from_nanos(100)).unwrap(), 1);
        assert_eq!(ticks_for(Duration::from_nanos(199)).unwrap(), 1);
        assert_eq!(ticks_for(Duration::from_millis(30)).unwrap(), 300_000);
        assert_eq!(
            ticks_for(Duration::from_secs(MAX_RECORDING_SECONDS)).unwrap(),
            1_200_000_000
        );
        assert!(matches!(
            ticks_for(Duration::from_secs(MAX_RECORDING_SECONDS) + Duration::from_nanos(1)),
            Err(Mp4RecordingError::InvalidInput(_))
        ));
        assert_eq!(
            frame_interval_ticks(FrameRate::new(30).unwrap()).unwrap(),
            333_333
        );
    }

    #[test]
    fn timestamps_reject_raw_subtick_backwards_and_terminal_regressions() {
        let previous = Duration::from_nanos(501);
        assert!(matches!(
            frame_tick_for(Some(previous), Duration::from_nanos(500), Some(5)),
            Err(Mp4RecordingError::TimestampMovedBackwards)
        ));
        assert!(matches!(
            terminal_tick_for(Duration::from_nanos(500), Some(previous), 5),
            Err(Mp4RecordingError::FrameAfterTerminal)
        ));
        assert_eq!(
            frame_tick_for(Some(previous), Duration::from_nanos(599), Some(5)).unwrap(),
            5
        );
        assert_eq!(
            terminal_tick_for(Duration::from_nanos(599), Some(previous), 5).unwrap(),
            5
        );
    }

    #[test]
    fn h264_bitrate_is_scaled_and_clamped() {
        assert_eq!(h264_bitrate(640, 480, 30).unwrap(), 1_152_000);
        assert_eq!(h264_bitrate(16, 16, 1).unwrap(), MIN_BITRATE);
        assert_eq!(h264_bitrate(1_920, 1_080, 60).unwrap(), 15_552_000);
        assert_eq!(h264_bitrate(10_000, 10_000, 60).unwrap(), MAX_BITRATE);
    }

    #[test]
    fn mp4_create_rejects_stale_and_not_ready_sessions_before_native_start() {
        let rect = ScreenRect::new(0, 0, 4, 4);
        let request = RecordingRequest::new(
            CaptureTarget::window(1, 1, 1, rect).unwrap(),
            rect,
            RecordingOptions::defaults_for(RecordingFormat::Mp4),
        )
        .unwrap();
        let session = Arc::new(RecordingSession::new(
            Arc::new(SystemRecorderClock::default()),
            Arc::new(NeverBackend),
            || {},
        ));
        let generation = session.select(request).unwrap();
        let destination = std::env::temp_dir().join("mp4-create-state-check.mp4");
        assert!(matches!(
            Mp4Recording::create(Arc::clone(&session), generation + 1, &destination),
            Err(Mp4RecordingError::Session(
                RecordingSessionError::StaleGeneration
            ))
        ));
        session.begin_countdown(Duration::from_secs(1)).unwrap();
        assert!(matches!(
            Mp4Recording::create(session, generation, destination),
            Err(Mp4RecordingError::InvalidTransition)
        ));
    }

    #[test]
    #[ignore = "requires Windows Media Foundation H.264 encoder and decoder"]
    fn native_mf_video_only_mp4_round_trips_through_source_reader()
    -> Result<(), Box<dyn std::error::Error>> {
        const FRAME_COUNT: usize = 12;
        let frame_size = canvas(159, 95);
        let rect = ScreenRect::new(0, 0, frame_size.width(), frame_size.height());
        let rate = FrameRate::new(30)?;
        let interval = rate.interval();
        let clock = Arc::new(TestClock::default());
        let gate = Arc::new(SyntheticGate {
            grants: AtomicUsize::new(0),
            emitted: AtomicUsize::new(0),
            clock: Arc::clone(&clock),
            interval,
            gap_at_frame: 6,
            gap: Duration::from_millis(90),
        });
        let session = Arc::new(RecordingSession::new(
            clock,
            Arc::new(SyntheticBackend {
                gate: Arc::clone(&gate),
            }),
            || {},
        ));
        let _close_session = CloseSessionOnDrop(Arc::clone(&session));
        let request = RecordingRequest::new(
            CaptureTarget::window(1, 1, 1, rect)?,
            rect,
            RecordingOptions::new(RecordingFormat::Mp4, rate, Duration::from_secs(5), false)?,
        )?;
        let generation = session.select(request)?;
        let directory = tempfile::tempdir()?;
        let destination = directory.path().join("round-trip.mp4");
        let mut writer = Mp4Recording::create(Arc::clone(&session), generation, &destination)?;
        assert_eq!(writer.output_dimensions(), (160, 96));
        session.start()?;

        wait_until(Duration::from_secs(5), || {
            matches!(session.view().state(), RecorderState::Recording { .. })
        })?;
        for next_count in 1..=FRAME_COUNT {
            gate.grants.store(next_count, Ordering::Release);
            wait_until(Duration::from_secs(5), || {
                session.view().accepted_frames() >= next_count as u64
            })?;
            wait_until(Duration::from_secs(5), || match writer.consume_latest() {
                Ok(Some(_)) => true,
                Ok(None) => false,
                Err(error) => panic!("MP4 frame write failed: {error}"),
            })?;
        }
        // The final input's positive terminal tail keeps it in the MP4 instead
        // of correctly coalescing it as a zero-duration update.
        gate.clock.advance(interval / 2);
        assert!(session.stop()?);
        wait_until(Duration::from_secs(5), || {
            let _ = session.poll();
            let view = session.view();
            view.terminal().is_some() && !view.worker_busy()
        })?;
        assert_eq!(session.view().accepted_frames(), FRAME_COUNT as u64);
        let terminal = session
            .view()
            .terminal()
            .expect("retired session has terminal");
        let expected_duration = terminal.elapsed().as_nanos() / 100;
        let published = writer.finish_and_publish()?;
        assert_eq!(published, destination);
        assert!(published.is_file());

        let apartment = MtaGuard::initialize()?;
        let mut runtime = MfRuntime::start()?;
        let reader = runtime.create_source_reader(&published)?;
        let native_type = unsafe { reader.GetNativeMediaType(0, 0) }?;
        assert_eq!(
            unsafe { native_type.GetGUID(&MF_MT_MAJOR_TYPE)? },
            MFMediaType_Video
        );
        assert_eq!(
            unsafe { native_type.GetGUID(&MF_MT_SUBTYPE)? },
            MFVideoFormat_H264
        );
        let dimensions = unsafe { native_type.GetUINT64(&MF_MT_FRAME_SIZE)? };
        assert_eq!((dimensions >> 32, dimensions as u32), (160, 96));
        match unsafe { reader.GetNativeMediaType(1, 0) } {
            Err(error) => assert_eq!(error.code(), MF_E_INVALIDSTREAMNUMBER),
            Ok(_) => panic!("the MP4 unexpectedly contains a second stream"),
        }

        let duration = unsafe {
            reader.GetPresentationAttribute(MF_SOURCE_READER_MEDIASOURCE.0 as u32, &MF_PD_DURATION)
        }?;
        let duration_ticks = u64::try_from(&duration)?;
        assert!(duration_ticks.abs_diff(expected_duration as u64) <= frame_interval_ticks(rate)?);

        let output_type = runtime.create_media_type()?;
        unsafe {
            output_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            output_type.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)?;
            reader.SetCurrentMediaType(
                MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                None,
                &output_type,
            )?;
        }
        let decoded_type =
            unsafe { reader.GetCurrentMediaType(MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32) }?;
        assert_eq!(
            unsafe { decoded_type.GetGUID(&MF_MT_SUBTYPE)? },
            MFVideoFormat_NV12
        );
        let decoded_dimensions = unsafe { decoded_type.GetUINT64(&MF_MT_FRAME_SIZE)? };
        assert_eq!(
            (decoded_dimensions >> 32, decoded_dimensions as u32),
            (160, 96)
        );
        let mut decoded = Vec::new();
        let mut previous_timestamp = None;
        let mut saw_end_of_stream = false;
        for _ in 0..FRAME_COUNT + 8 {
            let mut actual_stream = 0;
            let mut flags = 0;
            let mut timestamp = 0_i64;
            let mut sample = None;
            unsafe {
                reader.ReadSample(
                    MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                    0,
                    Some(&mut actual_stream),
                    Some(&mut flags),
                    Some(&mut timestamp),
                    Some(&mut sample),
                )?;
            }
            if let Some(sample) = sample {
                if let Some(previous) = previous_timestamp {
                    assert!(
                        timestamp > previous,
                        "decoded MP4 timestamps are not increasing"
                    );
                }
                assert_eq!(actual_stream, 0);
                let buffer = unsafe { sample.GetBufferByIndex(0)? };
                let (left, right) = decoded_nv12_luma_pair(&buffer, &decoded_type, 160, 96)?;
                decoded.push((timestamp, left, right));
                previous_timestamp = Some(timestamp);
            }
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                saw_end_of_stream = true;
                break;
            }
        }
        assert!(
            saw_end_of_stream,
            "source reader did not reach end of stream"
        );
        assert_eq!(decoded.len(), FRAME_COUNT);
        assert!(i32::from(decoded[0].1).abs_diff(63) <= u32::from(NATIVE_READER_RGB_TOLERANCE));
        assert!(i32::from(decoded[0].2).abs_diff(172) <= u32::from(NATIVE_READER_RGB_TOLERANCE));
        assert!(i32::from(decoded[4].1).abs_diff(172) <= u32::from(NATIVE_READER_RGB_TOLERANCE));
        assert!(i32::from(decoded[4].2).abs_diff(63) <= u32::from(NATIVE_READER_RGB_TOLERANCE));

        drop(duration);
        drop(decoded_type);
        drop(output_type);
        drop(native_type);
        drop(reader);
        runtime.shutdown()?;
        drop(runtime);
        drop(apartment);
        Ok(())
    }

    fn wait_until(
        timeout: Duration,
        mut ready: impl FnMut() -> bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if ready() {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(2));
        }
        Err("timed out waiting for the isolated MP4 fixture".into())
    }

    fn decoded_nv12_luma_pair(
        buffer: &IMFMediaBuffer,
        media_type: &MediaType,
        width: u32,
        height: u32,
    ) -> Result<(u8, u8), Box<dyn std::error::Error>> {
        let width = width as usize;
        let height = height as usize;
        let rows = height
            .checked_add(height / 2)
            .ok_or("decoded NV12 row count overflow")?;
        let max_stride = width
            .checked_add(256)
            .ok_or("decoded NV12 stride overflow")?;
        let max_length = max_stride
            .checked_mul(rows)
            .filter(|length| *length <= MAX_FRAME_BYTES)
            .ok_or("decoded NV12 buffer limit overflow")?;

        let (pixels, stride, flip_vertical) = if let Ok(two_d) = buffer.cast::<IMF2DBuffer>() {
            let length = unsafe { two_d.GetContiguousLength()? } as usize;
            if length == 0 || length > max_length || length % rows != 0 {
                return Err("Media Foundation returned an invalid bounded NV12 extent".into());
            }
            let stride = length / rows;
            if stride < width || stride > max_stride {
                return Err("Media Foundation returned an invalid contiguous NV12 stride".into());
            }
            let mut pixels = vec![0_u8; length];
            unsafe { two_d.ContiguousCopyTo(&mut pixels)? };
            (pixels, stride, false)
        } else {
            // Some decoder buffers expose only IMFMediaBuffer. Accept that
            // path only when its exact signed stride and sample length prove
            // a complete NV12 layout.
            let raw_stride = unsafe { media_type.GetUINT32(&MF_MT_DEFAULT_STRIDE)? };
            let signed_stride = i32::from_ne_bytes(raw_stride.to_ne_bytes());
            let stride = signed_stride.unsigned_abs() as usize;
            let required = stride
                .checked_mul(rows)
                .filter(|length| *length <= max_length)
                .ok_or("plain NV12 sample extent overflow")?;
            let current_length = unsafe { buffer.GetCurrentLength()? } as usize;
            if signed_stride == 0 || stride < width || current_length != required {
                return Err("plain NV12 sample does not match its reported stride".into());
            }
            let mut pixels = vec![0_u8; required];
            let mut locked = LockedMediaBuffer::lock(buffer)?;
            if (locked.capacity as usize) < required || locked.data.is_null() {
                return Err("plain NV12 sample buffer is shorter than its declared layout".into());
            }
            unsafe { std::ptr::copy_nonoverlapping(locked.data, pixels.as_mut_ptr(), required) };
            locked.unlock()?;
            (pixels, stride, signed_stride < 0)
        };

        let y = if flip_vertical { height - 1 - 45 } else { 45 };
        let row_offset = y
            .checked_mul(stride)
            .ok_or("decoded NV12 row offset overflow")?;
        let left = *pixels
            .get(row_offset + 20)
            .ok_or("decoded NV12 left sample is outside the luma plane")?;
        let right = *pixels
            .get(row_offset + 140)
            .ok_or("decoded NV12 right sample is outside the luma plane")?;
        Ok((left, right))
    }

    #[derive(Default)]
    struct TestClock(AtomicU64);

    impl TestClock {
        fn advance(&self, duration: Duration) {
            let nanos = u64::try_from(duration.as_nanos()).expect("fixture duration fits u64");
            self.0.fetch_add(nanos, Ordering::AcqRel);
        }
    }

    impl super::super::RecorderClock for TestClock {
        fn now(&self) -> Duration {
            Duration::from_nanos(self.0.load(Ordering::Acquire))
        }
    }

    struct NeverBackend;

    struct NeverSource;

    impl CaptureSource for NeverSource {
        fn poll(&mut self, max_wait: Duration, _stop: &CaptureStopToken) -> CapturePoll {
            thread::sleep(max_wait.min(Duration::from_millis(2)));
            CapturePoll::Pending
        }
    }

    impl RecordingBackend for NeverBackend {
        fn create_source(
            &self,
            _request: &RecordingRequest,
            _generation: u64,
        ) -> Result<Box<dyn CaptureSource>, String> {
            Ok(Box::new(NeverSource))
        }
    }

    struct SyntheticGate {
        grants: AtomicUsize,
        emitted: AtomicUsize,
        clock: Arc<TestClock>,
        interval: Duration,
        gap_at_frame: usize,
        gap: Duration,
    }

    struct SyntheticBackend {
        gate: Arc<SyntheticGate>,
    }

    struct SyntheticSource {
        generation: u64,
        size: CanvasSize,
        gate: Arc<SyntheticGate>,
    }

    impl RecordingBackend for SyntheticBackend {
        fn create_source(
            &self,
            request: &RecordingRequest,
            generation: u64,
        ) -> Result<Box<dyn CaptureSource>, String> {
            Ok(Box::new(SyntheticSource {
                generation,
                size: request.canvas(),
                gate: Arc::clone(&self.gate),
            }))
        }
    }

    impl CaptureSource for SyntheticSource {
        fn poll(&mut self, max_wait: Duration, stop: &CaptureStopToken) -> CapturePoll {
            if stop.stop_requested() {
                return CapturePoll::Ended(super::super::RecordingEndReason::UserStopped);
            }
            let emitted = self.gate.emitted.load(Ordering::Acquire);
            if emitted < self.gate.grants.load(Ordering::Acquire) {
                self.gate.emitted.fetch_add(1, Ordering::AcqRel);
                let interval = if emitted == self.gate.gap_at_frame {
                    self.gate.interval + self.gate.gap
                } else {
                    self.gate.interval
                };
                self.gate.clock.advance(interval);
                return CapturePoll::Frame {
                    generation: self.generation,
                    frame: synthetic_frame(self.size, emitted),
                };
            }
            thread::sleep(max_wait.min(Duration::from_millis(2)));
            CapturePoll::Pending
        }
    }

    fn synthetic_frame(size: CanvasSize, index: usize) -> RgbaFrame {
        let swap = (index / 4) % 2 == 1;
        let pixels = pixels_rgba(size.width(), size.height(), |x, _| {
            if (x < size.width() / 2) ^ swap {
                (255, 0, 0)
            } else {
                (0, 255, 0)
            }
        });
        RgbaFrame::new(size, pixels).unwrap()
    }

    struct CloseSessionOnDrop(Arc<RecordingSession>);

    impl Drop for CloseSessionOnDrop {
        fn drop(&mut self) {
            self.0.close();
            let deadline = Instant::now() + Duration::from_secs(2);
            while self.0.view().worker_busy() && Instant::now() < deadline {
                let _ = self.0.poll();
                thread::sleep(Duration::from_millis(2));
            }
        }
    }
}
