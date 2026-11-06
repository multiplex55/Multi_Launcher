//! Windows Graphics Capture source for the recorder.
//!
//! Capture objects are created, polled, and dropped by the recording worker;
//! this module does not own UI, encoding, or publication state.

use super::{
    CanvasSize, CaptureIdentity, CapturePoll, CaptureSource, CaptureStopToken, RecordingBackend,
    RecordingRequest, RgbaFrame,
};
use crate::mkmacro::ScreenRect;

const FRAME_POOL_BUFFERS: usize = 2;
const MAX_CAPTURE_SOURCE_DIMENSION: u32 = 16_384;
const MAX_CAPTURE_GPU_BYTES: usize = 256 * 1024 * 1024;
const MAX_CAPTURE_CPU_SCRATCH_BYTES: usize = 128 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SourceCrop {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

fn display_crop(bounds: ScreenRect, region: ScreenRect) -> Result<SourceCrop, String> {
    if !bounds.contains(region) {
        return Err("recording region is outside the selected display".into());
    }
    let x = i64::from(region.x) - i64::from(bounds.x);
    let y = i64::from(region.y) - i64::from(bounds.y);
    Ok(SourceCrop {
        x: u32::try_from(x).map_err(|_| "display crop X offset is invalid")?,
        y: u32::try_from(y).map_err(|_| "display crop Y offset is invalid")?,
        width: region.width,
        height: region.height,
    })
}

fn full_window_crop(width: u32, height: u32) -> Result<SourceCrop, String> {
    if width == 0 || height == 0 {
        return Err("capture source has zero dimensions".into());
    }
    Ok(SourceCrop {
        x: 0,
        y: 0,
        width,
        height,
    })
}

fn checked_source_bytes(width: u32, height: u32) -> Result<usize, String> {
    if width == 0
        || height == 0
        || width > MAX_CAPTURE_SOURCE_DIMENSION
        || height > MAX_CAPTURE_SOURCE_DIMENSION
    {
        return Err("capture source dimensions are outside the supported extent".into());
    }
    usize::try_from(u64::from(width) * u64::from(height))
        .ok()
        .and_then(|pixels| pixels.checked_mul(4))
        .filter(|bytes| *bytes <= isize::MAX as usize)
        .ok_or_else(|| "capture source pixel extent exceeds the supported limit".into())
}

fn checked_cpu_scratch_bytes(crop: SourceCrop, canvas: CanvasSize) -> Result<usize, String> {
    checked_source_bytes(crop.width, crop.height)?
        .checked_add(canvas.rgba_bytes())
        .filter(|bytes| *bytes <= MAX_CAPTURE_CPU_SCRATCH_BYTES)
        .ok_or_else(|| "capture conversion scratch exceeds the bounded CPU budget".into())
}

fn checked_crop_extent(crop: SourceCrop, source: (u32, u32)) -> Result<(), String> {
    let right = crop
        .x
        .checked_add(crop.width)
        .ok_or_else(|| "recording crop right edge overflowed".to_owned())?;
    let bottom = crop
        .y
        .checked_add(crop.height)
        .ok_or_else(|| "recording crop bottom edge overflowed".to_owned())?;
    if crop.width == 0 || crop.height == 0 || right > source.0 || bottom > source.1 {
        return Err("recording crop is outside the captured source extent".into());
    }
    let _ = checked_source_bytes(crop.width, crop.height)?;
    Ok(())
}

fn checked_gpu_bytes(source: (u32, u32), crop: SourceCrop) -> Result<usize, String> {
    checked_crop_extent(crop, source)?;
    let source_bytes = checked_source_bytes(source.0, source.1)?;
    let crop_bytes = checked_source_bytes(crop.width, crop.height)?;
    let estimate = source_bytes
        .checked_mul(FRAME_POOL_BUFFERS)
        .and_then(|bytes| bytes.checked_add(crop_bytes))
        .filter(|bytes| *bytes <= MAX_CAPTURE_GPU_BYTES)
        .ok_or_else(|| {
            "capture pool and staging textures exceed the bounded GPU budget".to_owned()
        })?;
    Ok(estimate)
}

fn fitted_dimensions(source: (u32, u32), canvas: CanvasSize) -> Result<(u32, u32), String> {
    if source.0 == 0 || source.1 == 0 {
        return Err("capture frame has zero dimensions".into());
    }
    let canvas_width = canvas.width();
    let canvas_height = canvas.height();
    if u64::from(source.0) * u64::from(canvas_height)
        >= u64::from(source.1) * u64::from(canvas_width)
    {
        Ok((
            canvas_width,
            (u64::from(source.1) * u64::from(canvas_width) / u64::from(source.0)).max(1) as u32,
        ))
    } else {
        Ok((
            (u64::from(source.0) * u64::from(canvas_height) / u64::from(source.1)).max(1) as u32,
            canvas_height,
        ))
    }
}

fn fit_rgba_frame(
    pixels: Vec<u8>,
    source: (u32, u32),
    canvas: CanvasSize,
) -> Result<RgbaFrame, String> {
    let expected_bytes = checked_source_bytes(source.0, source.1)?;
    if pixels.len() != expected_bytes {
        return Err("captured RGBA frame does not match its checked extent".into());
    }
    if source == (canvas.width(), canvas.height()) {
        return RgbaFrame::new(canvas, pixels)
            .map_err(|error| format!("could not construct bounded recording frame: {error}"));
    }
    let fitted = fitted_dimensions(source, canvas)?;
    let mut output = canvas
        .allocate_rgba()
        .map_err(|error| format!("could not allocate recording canvas: {error}"))?;
    for pixel in output.chunks_exact_mut(4) {
        pixel[3] = 255;
    }
    let left = (canvas.width() - fitted.0) / 2;
    let top = (canvas.height() - fitted.1) / 2;
    for destination_y in 0..fitted.1 {
        let source_y = (((u64::from(destination_y) * 2 + 1) * u64::from(source.1))
            / (u64::from(fitted.1) * 2))
            .min(u64::from(source.1 - 1)) as usize;
        for destination_x in 0..fitted.0 {
            let source_x = (((u64::from(destination_x) * 2 + 1) * u64::from(source.0))
                / (u64::from(fitted.0) * 2))
                .min(u64::from(source.0 - 1)) as usize;
            let source_pixel = source_y
                .checked_mul(source.0 as usize)
                .and_then(|row| row.checked_add(source_x))
                .and_then(|pixel| pixel.checked_mul(4))
                .ok_or_else(|| "captured frame sample offset overflowed".to_owned())?;
            let destination_pixel = (top as usize)
                .checked_add(destination_y as usize)
                .and_then(|row| row.checked_mul(canvas.width() as usize))
                .and_then(|pixel| pixel.checked_add(left as usize + destination_x as usize))
                .and_then(|pixel| pixel.checked_mul(4))
                .ok_or_else(|| "recording canvas sample offset overflowed".to_owned())?;
            output[destination_pixel..destination_pixel + 4]
                .copy_from_slice(&pixels[source_pixel..source_pixel + 4]);
        }
    }
    RgbaFrame::new(canvas, output)
        .map_err(|error| format!("could not construct bounded recording frame: {error}"))
}

#[derive(Clone, Copy, Debug, Default)]
pub struct WindowsRecordingBackend;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RecordingDisplayTarget {
    pub(crate) target: super::CaptureTarget,
    pub(crate) device_name: String,
}

#[cfg(windows)]
impl RecordingBackend for WindowsRecordingBackend {
    fn create_source(
        &self,
        request: &RecordingRequest,
        generation: u64,
    ) -> Result<Box<dyn CaptureSource>, String> {
        Ok(Box::new(native::WindowsCaptureSource::new(
            request.clone(),
            generation,
        )))
    }
}

#[cfg(windows)]
pub(crate) fn enumerate_recording_displays() -> Result<Vec<RecordingDisplayTarget>, String> {
    native::enumerate_recording_displays()
}

#[cfg(not(windows))]
pub(crate) fn enumerate_recording_displays() -> Result<Vec<RecordingDisplayTarget>, String> {
    Err("Windows display enumeration is available only on Windows".into())
}

#[cfg(windows)]
pub(crate) fn describe_recording_window(
    hwnd: usize,
    process_id: u32,
) -> Result<super::CaptureTarget, String> {
    native::describe_recording_window(hwnd, process_id)
}

/// Rechecks the signed capture identity and physical bounds captured before
/// the user chose an output path. The capture worker repeats this check when
/// it acquires the native source.
#[cfg(windows)]
pub(crate) fn validate_recording_target(target: &super::CaptureTarget) -> Result<(), String> {
    match target.identity() {
        CaptureIdentity::Window {
            hwnd,
            process_id,
            thread_id,
        } => {
            let current = describe_recording_window(*hwnd, *process_id)
                .map_err(|_| "the selected window changed while choosing the output path")?;
            if current.identity() != target.identity()
                || current.target_bounds() != target.target_bounds()
            {
                return Err("the selected window changed while choosing the output path".into());
            }
            let CaptureIdentity::Window {
                thread_id: current_thread,
                ..
            } = current.identity()
            else {
                return Err("the selected window changed while choosing the output path".into());
            };
            if current_thread != thread_id {
                return Err("the selected window changed while choosing the output path".into());
            }
            Ok(())
        }
        CaptureIdentity::Display {
            hmonitor,
            device_name,
        } => native::validate_display_target(*hmonitor, device_name, target.target_bounds()),
    }
}

#[cfg(not(windows))]
pub(crate) fn describe_recording_window(
    _hwnd: usize,
    _process_id: u32,
) -> Result<super::CaptureTarget, String> {
    Err("Windows window capture is available only on Windows".into())
}

#[cfg(not(windows))]
pub(crate) fn validate_recording_target(_target: &super::CaptureTarget) -> Result<(), String> {
    Err("Windows recording targets are available only on Windows".into())
}

#[cfg(not(windows))]
impl RecordingBackend for WindowsRecordingBackend {
    fn create_source(
        &self,
        _request: &RecordingRequest,
        _generation: u64,
    ) -> Result<Box<dyn CaptureSource>, String> {
        Err("Windows Graphics Capture is available only on Windows".into())
    }
}

#[cfg(windows)]
mod native {
    use super::*;
    use crate::platform::windows_capture;
    use std::sync::{Arc, Condvar, Mutex};
    use std::time::{Duration, Instant};
    use windows::Foundation::{EventRegistrationToken, TypedEventHandler};
    use windows::Graphics::Capture::{
        Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureItem,
        GraphicsCaptureSession,
    };
    use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
    use windows::Graphics::DirectX::DirectXPixelFormat;
    use windows::Graphics::SizeInt32;
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows::Win32::Graphics::Direct3D11::{
        D3D11_BOX, D3D11_TEXTURE2D_DESC, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
    };
    use windows::Win32::Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
    use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
    use windows::Win32::Graphics::Gdi::{
        EnumDisplayMonitors, GetMonitorInfoW, HMONITOR, MONITORINFOEXW,
    };
    use windows::Win32::System::WinRT::Direct3D11::IDirect3DDxgiInterfaceAccess;
    use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
    use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize};
    use windows::Win32::UI::WindowsAndMessaging::{GetWindowThreadProcessId, IsWindow};
    use windows::core::{Interface, Type, factory};

    #[derive(Clone, Copy, Debug)]
    enum CaptureTargetIdentity {
        Window {
            hwnd: usize,
            process_id: u32,
            thread_id: u32,
        },
        Display {
            hmonitor: usize,
        },
    }

    struct RoApartment;

    impl RoApartment {
        fn initialize() -> Result<Self, String> {
            unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.map_err(|error| {
                format!("could not initialize capture worker COM apartment: {error}")
            })?;
            Ok(Self)
        }
    }

    impl Drop for RoApartment {
        fn drop(&mut self) {
            // SAFETY: this value exists only after one successful MTA
            // initialization on the capture worker.
            unsafe { RoUninitialize() };
        }
    }

    #[derive(Default)]
    struct SignalState {
        frame_available: bool,
        closed: bool,
    }

    #[derive(Default)]
    struct CaptureSignal {
        state: Mutex<SignalState>,
        changed: Condvar,
    }

    impl CaptureSignal {
        fn frame_arrived(&self) {
            if let Ok(mut state) = self.state.lock() {
                state.frame_available = true;
                self.changed.notify_one();
            }
        }

        fn source_closed(&self) {
            if let Ok(mut state) = self.state.lock() {
                state.closed = true;
                self.changed.notify_all();
            }
        }

        fn wait(&self, max_wait: Duration, stop: &CaptureStopToken) -> SignalResult {
            let deadline = Instant::now().checked_add(max_wait);
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            while !state.frame_available
                && !state.closed
                && !stop.stop_requested()
                && !stop.is_cancelled()
            {
                let remaining = deadline
                    .map(|deadline| deadline.saturating_duration_since(Instant::now()))
                    .unwrap_or(max_wait);
                if remaining.is_zero() {
                    break;
                }
                let (next, result) = self
                    .changed
                    .wait_timeout(state, remaining)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                state = next;
                if result.timed_out() {
                    break;
                }
            }
            if stop.is_cancelled() || stop.stop_requested() {
                SignalResult::Stopped
            } else if state.closed {
                SignalResult::Closed
            } else if state.frame_available {
                state.frame_available = false;
                SignalResult::FrameAvailable
            } else {
                SignalResult::TimedOut
            }
        }

        fn is_closed(&self) -> bool {
            self.state.lock().is_ok_and(|state| state.closed)
        }
    }

    enum SignalResult {
        FrameAvailable,
        Closed,
        Stopped,
        TimedOut,
    }

    struct MonitorSnapshot {
        hmonitor: usize,
        device_name: String,
        bounds: ScreenRect,
    }

    fn read_monitor_snapshot(hmonitor: HMONITOR) -> Result<MonitorSnapshot, String> {
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        if !unsafe { GetMonitorInfoW(hmonitor, (&mut info as *mut MONITORINFOEXW).cast()) }
            .as_bool()
        {
            return Err("could not query the selected display identity".into());
        }
        let name_end = info
            .szDevice
            .iter()
            .position(|character| *character == 0)
            .unwrap_or(info.szDevice.len());
        let device_name = String::from_utf16(&info.szDevice[..name_end])
            .map_err(|_| "selected display device name is malformed")?;
        if device_name.is_empty() {
            return Err("selected display has an empty device name".into());
        }
        let rect = info.monitorInfo.rcMonitor;
        let width = u32::try_from(i64::from(rect.right) - i64::from(rect.left))
            .map_err(|_| "selected display has invalid physical bounds")?;
        let height = u32::try_from(i64::from(rect.bottom) - i64::from(rect.top))
            .map_err(|_| "selected display has invalid physical bounds")?;
        let bounds = ScreenRect::new(rect.left, rect.top, width, height);
        bounds
            .validate_capture()
            .map_err(|_| "selected display has invalid physical bounds")?;
        Ok(MonitorSnapshot {
            hmonitor: hmonitor.0 as usize,
            device_name,
            bounds,
        })
    }

    pub(super) fn enumerate_recording_displays() -> Result<Vec<RecordingDisplayTarget>, String> {
        struct Search {
            snapshots: Vec<MonitorSnapshot>,
            error: Option<String>,
        }

        unsafe extern "system" fn enumerate_monitor(
            hmonitor: HMONITOR,
            _hdc: windows::Win32::Graphics::Gdi::HDC,
            _rect: *mut windows::Win32::Foundation::RECT,
            data: LPARAM,
        ) -> BOOL {
            // SAFETY: EnumDisplayMonitors invokes this callback synchronously
            // while the stack-owned Search remains alive.
            let search = unsafe { &mut *(data.0 as *mut Search) };
            match read_monitor_snapshot(hmonitor) {
                Ok(snapshot) => search.snapshots.push(snapshot),
                Err(error) => {
                    search.error = Some(error);
                    return BOOL(0);
                }
            }
            BOOL(1)
        }

        let mut search = Search {
            snapshots: Vec::new(),
            error: None,
        };
        let enumerated = unsafe {
            EnumDisplayMonitors(
                None,
                None,
                Some(enumerate_monitor),
                LPARAM((&mut search as *mut Search) as isize),
            )
        }
        .as_bool();
        if let Some(error) = search.error {
            return Err(error);
        }
        if !enumerated {
            return Err("Windows display enumeration failed".into());
        }
        search.snapshots.sort_by(|left, right| {
            left.device_name
                .to_ascii_lowercase()
                .cmp(&right.device_name.to_ascii_lowercase())
        });
        search
            .snapshots
            .into_iter()
            .map(|snapshot| {
                let target = super::super::CaptureTarget::display(
                    snapshot.hmonitor,
                    snapshot.device_name.clone(),
                    snapshot.bounds,
                )
                .map_err(|error| error.to_string())?;
                Ok(RecordingDisplayTarget {
                    target,
                    device_name: snapshot.device_name,
                })
            })
            .collect()
    }

    pub(super) fn describe_recording_window(
        hwnd_value: usize,
        expected_process_id: u32,
    ) -> Result<super::super::CaptureTarget, String> {
        let hwnd = HWND(hwnd_value as *mut core::ffi::c_void);
        if hwnd_value == 0 {
            return Err("the selected window is no longer available".into());
        }
        let read_identity = || {
            if !unsafe { IsWindow(hwnd).as_bool() } {
                return Err("the selected window is no longer available".to_owned());
            }
            let mut process_id = 0;
            let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
            if thread_id == 0 {
                return Err("could not query the selected window identity".to_owned());
            }
            Ok((process_id, thread_id))
        };
        let before = read_identity()?;
        if before.0 != expected_process_id {
            return Err("the selected window identity changed".into());
        }
        let mut rect = windows::Win32::Foundation::RECT::default();
        unsafe {
            DwmGetWindowAttribute(
                hwnd,
                DWMWA_EXTENDED_FRAME_BOUNDS,
                (&mut rect as *mut windows::Win32::Foundation::RECT).cast(),
                std::mem::size_of::<windows::Win32::Foundation::RECT>() as u32,
            )
        }
        .map_err(|error| format!("could not query physical window bounds: {error}"))?;
        let after = read_identity()?;
        if after != before || after.0 != expected_process_id {
            return Err("the selected window identity changed while reading its bounds".into());
        }
        let width = u32::try_from(i64::from(rect.right) - i64::from(rect.left))
            .map_err(|_| "the selected window has invalid physical bounds")?;
        let height = u32::try_from(i64::from(rect.bottom) - i64::from(rect.top))
            .map_err(|_| "the selected window has invalid physical bounds")?;
        let target = super::super::CaptureTarget::window(
            hwnd_value,
            after.0,
            after.1,
            ScreenRect::new(rect.left, rect.top, width, height),
        )
        .map_err(|error| error.to_string())?;
        let final_identity = read_identity()?;
        if final_identity != after {
            return Err("the selected window identity changed after reading its bounds".into());
        }
        Ok(target)
    }

    fn target_identity(request: &RecordingRequest) -> CaptureTargetIdentity {
        match request.target().identity() {
            CaptureIdentity::Window {
                hwnd,
                process_id,
                thread_id,
            } => CaptureTargetIdentity::Window {
                hwnd: *hwnd,
                process_id: *process_id,
                thread_id: *thread_id,
            },
            CaptureIdentity::Display { hmonitor, .. } => CaptureTargetIdentity::Display {
                hmonitor: *hmonitor,
            },
        }
    }

    fn native_window_identity_is_current(
        hwnd_value: usize,
        expected_process_id: u32,
        expected_thread_id: u32,
    ) -> bool {
        let hwnd = HWND(hwnd_value as *mut core::ffi::c_void);
        if !unsafe { IsWindow(hwnd).as_bool() } {
            return false;
        }
        let mut process_id = 0;
        let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
        thread_id == expected_thread_id && process_id == expected_process_id
    }

    fn monitor_snapshot_for(hmonitor_value: usize) -> Result<Option<MonitorSnapshot>, String> {
        struct Search {
            target: HMONITOR,
            result: Option<MonitorSnapshot>,
            error: Option<String>,
        }

        unsafe extern "system" fn enumerate_monitor(
            hmonitor: HMONITOR,
            _hdc: windows::Win32::Graphics::Gdi::HDC,
            _rect: *mut windows::Win32::Foundation::RECT,
            data: LPARAM,
        ) -> BOOL {
            // SAFETY: `EnumDisplayMonitors` receives a pointer to the live
            // stack-owned Search for the duration of this synchronous call.
            let search = unsafe { &mut *(data.0 as *mut Search) };
            if hmonitor != search.target {
                return BOOL(1);
            }
            match read_monitor_snapshot(hmonitor) {
                Ok(snapshot) => search.result = Some(snapshot),
                Err(error) => {
                    search.error = Some(error);
                    return BOOL(0);
                }
            }
            BOOL(0)
        }

        let target = HMONITOR(hmonitor_value as *mut core::ffi::c_void);
        let mut search = Search {
            target,
            result: None,
            error: None,
        };
        let enumerated = unsafe {
            EnumDisplayMonitors(
                None,
                None,
                Some(enumerate_monitor),
                LPARAM((&mut search as *mut Search) as isize),
            )
        }
        .as_bool();
        if let Some(error) = search.error {
            return Err(error);
        }
        if !enumerated && search.result.is_none() {
            return Err("Windows display enumeration failed".into());
        }
        Ok(search.result)
    }

    pub(super) fn validate_display_target(
        hmonitor: usize,
        expected_device_name: &str,
        expected_bounds: ScreenRect,
    ) -> Result<(), String> {
        let Some(current) = monitor_snapshot_for(hmonitor)? else {
            return Err("the selected display changed while choosing the output path".into());
        };
        if current.device_name != expected_device_name || current.bounds != expected_bounds {
            return Err("the selected display changed while choosing the output path".into());
        }
        Ok(())
    }

    fn expected_display(request: &RecordingRequest) -> Option<MonitorSnapshot> {
        match request.target().identity() {
            CaptureIdentity::Display {
                hmonitor,
                device_name,
            } => Some(MonitorSnapshot {
                hmonitor: *hmonitor,
                device_name: device_name.clone(),
                bounds: request.target().target_bounds(),
            }),
            CaptureIdentity::Window { .. } => None,
        }
    }

    fn same_display(expected: &MonitorSnapshot, actual: &MonitorSnapshot) -> bool {
        expected.hmonitor == actual.hmonitor
            && expected.device_name == actual.device_name
            && expected.bounds == actual.bounds
    }

    fn create_capture_item(
        request: &RecordingRequest,
        identity: CaptureTargetIdentity,
    ) -> Result<GraphicsCaptureItem, String> {
        let interop: IGraphicsCaptureItemInterop =
            factory::<GraphicsCaptureItem, _>().map_err(|error| {
                format!("Windows Graphics Capture item interop is unavailable: {error}")
            })?;
        match identity {
            CaptureTargetIdentity::Window {
                hwnd,
                process_id,
                thread_id,
            } => {
                if !native_window_identity_is_current(hwnd, process_id, thread_id) {
                    return Err("the selected window identity changed before capture".into());
                }
                let hwnd = HWND(hwnd as *mut core::ffi::c_void);
                let item = unsafe { interop.CreateForWindow::<_, GraphicsCaptureItem>(hwnd) }
                    .map_err(|error| {
                        format!("could not bind capture to the selected window: {error}")
                    })?;
                if !native_window_identity_is_current(hwnd.0 as usize, process_id, thread_id) {
                    return Err("the selected window identity changed during capture setup".into());
                }
                Ok(item)
            }
            CaptureTargetIdentity::Display { hmonitor } => {
                let expected = expected_display(request)
                    .ok_or_else(|| "selected display identity is missing".to_owned())?;
                let before = monitor_snapshot_for(hmonitor)?
                    .ok_or_else(|| "the selected display is no longer connected".to_owned())?;
                if !same_display(&expected, &before) {
                    return Err("the selected display identity or bounds changed".into());
                }
                let monitor = HMONITOR(hmonitor as *mut core::ffi::c_void);
                let item = unsafe { interop.CreateForMonitor::<_, GraphicsCaptureItem>(monitor) }
                    .map_err(|error| {
                    format!("could not bind capture to the selected display: {error}")
                })?;
                let after = monitor_snapshot_for(hmonitor)?.ok_or_else(|| {
                    "the selected display disappeared during capture setup".to_owned()
                })?;
                if !same_display(&expected, &after) {
                    return Err("the selected display changed during capture setup".into());
                }
                Ok(item)
            }
        }
    }

    fn dimensions(size: SizeInt32) -> Result<(u32, u32), String> {
        if size.Width <= 0 || size.Height <= 0 {
            return Err("capture source has zero or negative dimensions".into());
        }
        let dimensions = (size.Width as u32, size.Height as u32);
        checked_source_bytes(dimensions.0, dimensions.1)?;
        Ok(dimensions)
    }

    fn crop_for_request(
        request: &RecordingRequest,
        dimensions: (u32, u32),
    ) -> Result<SourceCrop, String> {
        match request.target().identity() {
            CaptureIdentity::Window { .. } => full_window_crop(dimensions.0, dimensions.1),
            CaptureIdentity::Display { .. } => {
                let bounds = request.target().target_bounds();
                if dimensions != (bounds.width, bounds.height) {
                    return Err(
                        "captured display extent no longer matches its physical bounds".into(),
                    );
                }
                let crop = display_crop(bounds, request.region())?;
                checked_crop_extent(crop, dimensions)?;
                Ok(crop)
            }
        }
    }

    fn try_next_frame(
        pool: &Direct3D11CaptureFramePool,
    ) -> windows::core::Result<Option<Direct3D11CaptureFrame>> {
        // The generated `TryGetNextFrame` wrapper converts a successful null
        // ABI value into an error. Preserve success-null as an empty pool while
        // retaining genuine HRESULT failures.
        unsafe {
            let mut result = std::ptr::null_mut();
            (pool.vtable().TryGetNextFrame)(pool.as_raw(), &mut result).and_then(|| {
                if result.is_null() {
                    Ok(None)
                } else {
                    Type::from_abi(result).map(Some)
                }
            })
        }
    }

    struct CaptureFrameGuard(Option<Direct3D11CaptureFrame>);

    impl CaptureFrameGuard {
        fn new(frame: Direct3D11CaptureFrame) -> Self {
            Self(Some(frame))
        }

        fn get(&self) -> Result<&Direct3D11CaptureFrame, CaptureError> {
            self.0
                .as_ref()
                .ok_or_else(|| CaptureError::Failed("capture frame was already closed".into()))
        }
    }

    impl Drop for CaptureFrameGuard {
        fn drop(&mut self) {
            if let Some(frame) = self.0.take() {
                let _ = frame.Close();
            }
        }
    }

    struct StagingTexture {
        width: u32,
        height: u32,
        texture: ID3D11Texture2D,
    }

    struct CaptureResources {
        target_identity: CaptureTargetIdentity,
        expected_display: Option<MonitorSnapshot>,
        request: RecordingRequest,
        item: Option<GraphicsCaptureItem>,
        device: Option<ID3D11Device>,
        context: Option<ID3D11DeviceContext>,
        winrt_device: Option<IDirect3DDevice>,
        pool: Option<Direct3D11CaptureFramePool>,
        pool_size: (u32, u32),
        session: Option<GraphicsCaptureSession>,
        frame_token: Option<EventRegistrationToken>,
        closed_token: Option<EventRegistrationToken>,
        signal: Arc<CaptureSignal>,
        staging: Option<StagingTexture>,
    }

    impl CaptureResources {
        fn new(
            request: RecordingRequest,
            target_identity: CaptureTargetIdentity,
            item: GraphicsCaptureItem,
            source_size: (u32, u32),
            device: ID3D11Device,
            context: ID3D11DeviceContext,
            winrt_device: IDirect3DDevice,
        ) -> Self {
            Self {
                target_identity,
                expected_display: expected_display(&request),
                request,
                item: Some(item),
                device: Some(device),
                context: Some(context),
                winrt_device: Some(winrt_device),
                pool: None,
                pool_size: source_size,
                session: None,
                frame_token: None,
                closed_token: None,
                signal: Arc::new(CaptureSignal::default()),
                staging: None,
            }
        }

        fn start(&mut self) -> Result<(), String> {
            let item = self
                .item
                .as_ref()
                .ok_or_else(|| "capture item is unavailable".to_owned())?;
            let signal = Arc::clone(&self.signal);
            self.closed_token = Some(
                item.Closed(&TypedEventHandler::<
                    GraphicsCaptureItem,
                    windows::core::IInspectable,
                >::new(move |_, _| {
                    signal.source_closed();
                    Ok(())
                }))
                .map_err(|error| {
                    format!("could not subscribe to capture target closure: {error}")
                })?,
            );

            let winrt_device = self
                .winrt_device
                .as_ref()
                .ok_or_else(|| "capture device is unavailable".to_owned())?;
            let pool = windows_capture::create_free_threaded_frame_pool(
                winrt_device,
                DirectXPixelFormat::B8G8R8A8UIntNormalized,
                FRAME_POOL_BUFFERS as i32,
                SizeInt32 {
                    Width: self.pool_size.0 as i32,
                    Height: self.pool_size.1 as i32,
                },
            )
            .map_err(|error| format!("could not create the capture frame pool: {error}"))?;
            self.pool = Some(pool);
            let pool = self
                .pool
                .as_ref()
                .ok_or_else(|| "capture frame pool was not stored".to_owned())?;
            let signal = Arc::clone(&self.signal);
            self.frame_token = Some(
                pool.FrameArrived(&TypedEventHandler::<
                    Direct3D11CaptureFramePool,
                    windows::core::IInspectable,
                >::new(move |_, _| {
                    signal.frame_arrived();
                    Ok(())
                }))
                .map_err(|error| format!("could not subscribe to capture frames: {error}"))?,
            );
            let item = self
                .item
                .as_ref()
                .ok_or_else(|| "capture item is unavailable".to_owned())?;
            self.session = Some(
                pool.CreateCaptureSession(item)
                    .map_err(|error| format!("could not create the capture session: {error}"))?,
            );
            let session = self
                .session
                .as_ref()
                .ok_or_else(|| "capture session was not created".to_owned())?;
            session
                .SetIsCursorCaptureEnabled(self.request.options().includes_cursor())
                .map_err(|error| {
                    if self.request.options().includes_cursor() {
                        format!(
                            "Windows could not apply the requested cursor capture setting: {error}"
                        )
                    } else {
                        format!("Windows does not support disabling cursor capture: {error}")
                    }
                })?;
            session
                .StartCapture()
                .map_err(|error| format!("could not start Windows Graphics Capture: {error}"))
        }

        fn target_is_current(&self) -> Result<bool, String> {
            match self.target_identity {
                CaptureTargetIdentity::Window {
                    hwnd,
                    process_id,
                    thread_id,
                } => Ok(native_window_identity_is_current(
                    hwnd, process_id, thread_id,
                )),
                CaptureTargetIdentity::Display { hmonitor } => {
                    let Some(expected) = self.expected_display.as_ref() else {
                        return Ok(false);
                    };
                    let Some(actual) = monitor_snapshot_for(hmonitor)? else {
                        return Ok(false);
                    };
                    Ok(same_display(expected, &actual))
                }
            }
        }

        fn wait_and_read(
            &mut self,
            max_wait: Duration,
            stop: &CaptureStopToken,
        ) -> Result<Option<RgbaFrame>, CaptureError> {
            if !self.target_is_current().map_err(CaptureError::Failed)? {
                return Err(CaptureError::TargetLost);
            }
            match self.signal.wait(max_wait, stop) {
                SignalResult::Stopped | SignalResult::TimedOut => return Ok(None),
                SignalResult::Closed => return Err(CaptureError::TargetLost),
                SignalResult::FrameAvailable => {}
            }
            if stop.stop_requested() || stop.is_cancelled() {
                return Ok(None);
            }
            if !self.target_is_current().map_err(CaptureError::Failed)? {
                return Err(CaptureError::TargetLost);
            }
            let pool = self
                .pool
                .as_ref()
                .ok_or_else(|| CaptureError::Failed("capture frame pool is unavailable".into()))?;
            let mut latest = None;
            for _ in 0..FRAME_POOL_BUFFERS {
                if self.signal.is_closed() {
                    return Err(CaptureError::TargetLost);
                }
                match try_next_frame(pool).map_err(|error| {
                    CaptureError::Failed(format!(
                        "could not retrieve the next capture frame: {error}"
                    ))
                })? {
                    Some(frame) => {
                        drop(latest.replace(CaptureFrameGuard::new(frame)));
                    }
                    None => break,
                }
            }
            let Some(frame) = latest else {
                return Ok(None);
            };
            self.convert_frame(frame)
        }

        fn convert_frame(
            &mut self,
            frame: CaptureFrameGuard,
        ) -> Result<Option<RgbaFrame>, CaptureError> {
            let content_size = frame.get()?.ContentSize().map_err(|error| {
                CaptureError::Failed(format!("could not query capture frame size: {error}"))
            })?;
            let dimensions = dimensions(content_size).map_err(CaptureError::Failed)?;
            if matches!(
                self.request.target().identity(),
                CaptureIdentity::Display { .. }
            ) && dimensions
                != (
                    self.request.target().target_bounds().width,
                    self.request.target().target_bounds().height,
                )
            {
                return Err(CaptureError::TargetLost);
            }
            let crop = crop_for_request(&self.request, dimensions).map_err(CaptureError::Failed)?;
            checked_gpu_bytes(dimensions, crop).map_err(CaptureError::Failed)?;
            checked_cpu_scratch_bytes(crop, self.request.canvas()).map_err(CaptureError::Failed)?;
            if dimensions != self.pool_size {
                self.staging.take();
                drop(frame);
                self.drain_pool()?;
                let pool = self.pool.as_ref().ok_or_else(|| {
                    CaptureError::Failed("capture frame pool is unavailable".into())
                })?;
                let device = self
                    .winrt_device
                    .as_ref()
                    .ok_or_else(|| CaptureError::Failed("capture device is unavailable".into()))?;
                pool.Recreate(
                    device,
                    DirectXPixelFormat::B8G8R8A8UIntNormalized,
                    FRAME_POOL_BUFFERS as i32,
                    content_size,
                )
                .map_err(|error| {
                    CaptureError::Failed(format!("could not resize capture frame pool: {error}"))
                })?;
                self.pool_size = dimensions;
                return Ok(None);
            }

            let rgba = {
                let surface = frame.get()?.Surface().map_err(|error| {
                    CaptureError::Failed(format!("capture frame surface is unavailable: {error}"))
                })?;
                let access: IDirect3DDxgiInterfaceAccess = surface.cast().map_err(|error| {
                    CaptureError::Failed(format!("capture frame surface is unsupported: {error}"))
                })?;
                let source_texture: ID3D11Texture2D =
                    unsafe { access.GetInterface() }.map_err(|error| {
                        CaptureError::Failed(format!(
                            "capture frame texture is inaccessible: {error}"
                        ))
                    })?;
                let mut source_desc = D3D11_TEXTURE2D_DESC::default();
                // SAFETY: GetDesc writes a descriptor for the live frame texture.
                unsafe { source_texture.GetDesc(&mut source_desc) };
                if source_desc.Width != dimensions.0
                    || source_desc.Height != dimensions.1
                    || source_desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM
                    || source_desc.SampleDesc.Count != 1
                    || source_desc.MipLevels == 0
                    || source_desc.ArraySize == 0
                {
                    return Err(CaptureError::Failed(
                        "capture frame texture extent, format, or sample count is unsupported"
                            .into(),
                    ));
                }
                checked_crop_extent(crop, dimensions).map_err(CaptureError::Failed)?;
                if self.staging.as_ref().is_none_or(|staging| {
                    (staging.width, staging.height) != (crop.width, crop.height)
                }) {
                    self.staging.take();
                    let device = self.device.as_ref().ok_or_else(|| {
                        CaptureError::Failed("capture device is unavailable".into())
                    })?;
                    let texture = windows_capture::create_staging_texture(
                        device,
                        crop.width,
                        crop.height,
                        source_desc.Format,
                    )
                    .map_err(CaptureError::Failed)?;
                    self.staging = Some(StagingTexture {
                        width: crop.width,
                        height: crop.height,
                        texture,
                    });
                }
                let staging = self.staging.as_ref().ok_or_else(|| {
                    CaptureError::Failed("crop staging texture was not created".into())
                })?;
                let crop_box = D3D11_BOX {
                    left: crop.x,
                    top: crop.y,
                    front: 0,
                    right: crop.x + crop.width,
                    bottom: crop.y + crop.height,
                    back: 1,
                };
                let context = self.context.as_ref().ok_or_else(|| {
                    CaptureError::Failed("capture device context is unavailable".into())
                })?;
                // SAFETY: source and staging textures are same-format, single
                // sample D3D resources; the checked crop is contained by source.
                unsafe {
                    context.CopySubresourceRegion(
                        &staging.texture,
                        0,
                        0,
                        0,
                        0,
                        &source_texture,
                        0,
                        Some(&crop_box),
                    );
                }
                windows_capture::read_staging_rgba(
                    context,
                    &staging.texture,
                    crop.width,
                    crop.height,
                )
                .map_err(CaptureError::Failed)?
            };
            // Surface and texture references above are gone before the native
            // frame is explicitly closed by its guard.
            drop(frame);
            let frame = fit_rgba_frame(rgba, (crop.width, crop.height), self.request.canvas())
                .map_err(CaptureError::Failed)?;
            Ok(Some(frame))
        }

        fn drain_pool(&self) -> Result<(), CaptureError> {
            let Some(pool) = self.pool.as_ref() else {
                return Ok(());
            };
            for _ in 0..FRAME_POOL_BUFFERS {
                match try_next_frame(pool).map_err(|error| {
                    CaptureError::Failed(format!("could not drain capture frame pool: {error}"))
                })? {
                    Some(frame) => drop(CaptureFrameGuard::new(frame)),
                    None => break,
                }
            }
            Ok(())
        }
    }

    impl Drop for CaptureResources {
        fn drop(&mut self) {
            if let Some(session) = self.session.take() {
                let _ = session.Close();
            }
            if let Some(pool) = self.pool.take() {
                if let Some(token) = self.frame_token.take() {
                    let _ = pool.RemoveFrameArrived(token);
                }
                for _ in 0..FRAME_POOL_BUFFERS {
                    match try_next_frame(&pool) {
                        Ok(Some(frame)) => drop(CaptureFrameGuard::new(frame)),
                        Ok(None) | Err(_) => break,
                    }
                }
                let _ = pool.Close();
            }
            if let (Some(item), Some(token)) = (self.item.as_ref(), self.closed_token.take()) {
                let _ = item.RemoveClosed(token);
            }
            self.staging.take();
            self.item.take();
            self.winrt_device.take();
            self.context.take();
            self.device.take();
        }
    }

    enum CaptureError {
        TargetLost,
        Failed(String),
    }

    struct NativeCapture {
        resources: Option<CaptureResources>,
        apartment: Option<RoApartment>,
    }

    impl NativeCapture {
        fn start(request: RecordingRequest) -> Result<Self, String> {
            let apartment = RoApartment::initialize()?;
            if !windows_capture::graphics_capture_is_supported().map_err(|error| {
                format!("Windows Graphics Capture support check failed: {error}")
            })? {
                return Err(
                    "Windows Graphics Capture is not supported on this Windows version".into(),
                );
            }
            let identity = target_identity(&request);
            let (device, context) = windows_capture::create_device()?;
            let winrt_device = windows_capture::create_winrt_device(&device)?;
            let item = create_capture_item(&request, identity)?;
            let source_size = item
                .Size()
                .map_err(|error| format!("could not query capture target size: {error}"))?;
            let source_dimensions = dimensions(source_size)?;
            let crop = crop_for_request(&request, source_dimensions)?;
            checked_gpu_bytes(source_dimensions, crop)?;
            checked_cpu_scratch_bytes(crop, request.canvas())?;
            if matches!(identity, CaptureTargetIdentity::Display { .. })
                && source_dimensions
                    != (
                        request.target().target_bounds().width,
                        request.target().target_bounds().height,
                    )
            {
                return Err("captured display extent does not match its physical bounds".into());
            }
            let mut resources = CaptureResources::new(
                request,
                identity,
                item,
                source_dimensions,
                device,
                context,
                winrt_device,
            );
            resources.start()?;
            Ok(Self {
                resources: Some(resources),
                apartment: Some(apartment),
            })
        }

        fn poll(
            &mut self,
            max_wait: Duration,
            stop: &CaptureStopToken,
        ) -> Result<Option<RgbaFrame>, CaptureError> {
            let Some(resources) = self.resources.as_mut() else {
                return Err(CaptureError::Failed(
                    "native capture resources are unavailable".into(),
                ));
            };
            match resources.wait_and_read(max_wait, stop) {
                Err(CaptureError::Failed(_error))
                    if resources.signal.is_closed()
                        || resources
                            .target_is_current()
                            .is_ok_and(|is_current| !is_current) =>
                {
                    Err(CaptureError::TargetLost)
                }
                result => result,
            }
        }
    }

    impl Drop for NativeCapture {
        fn drop(&mut self) {
            self.resources.take();
            self.apartment.take();
        }
    }

    pub(super) struct WindowsCaptureSource {
        request: RecordingRequest,
        generation: u64,
        capture: Option<NativeCapture>,
    }

    impl WindowsCaptureSource {
        pub(super) fn new(request: RecordingRequest, generation: u64) -> Self {
            Self {
                request,
                generation,
                capture: None,
            }
        }
    }

    impl CaptureSource for WindowsCaptureSource {
        fn start(
            &mut self,
            request: &RecordingRequest,
            _stop: &CaptureStopToken,
        ) -> Result<(), String> {
            if request != &self.request || self.capture.is_some() {
                return Err("capture source request changed or was started twice".into());
            }
            self.capture = Some(NativeCapture::start(self.request.clone())?);
            Ok(())
        }

        fn poll(&mut self, max_wait: Duration, stop: &CaptureStopToken) -> CapturePoll {
            if stop.is_cancelled() || stop.stop_requested() {
                return CapturePoll::Pending;
            }
            let Some(capture) = self.capture.as_mut() else {
                return CapturePoll::Failed("capture source was polled before start".into());
            };
            match capture.poll(max_wait, stop) {
                Ok(Some(frame)) => CapturePoll::Frame {
                    generation: self.generation,
                    frame,
                },
                Ok(None) => CapturePoll::Pending,
                Err(CaptureError::TargetLost) => {
                    CapturePoll::Ended(super::super::RecordingEndReason::TargetLost)
                }
                Err(CaptureError::Failed(error)) => CapturePoll::Failed(error),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(x: i32, y: i32, width: u32, height: u32) -> ScreenRect {
        ScreenRect::new(x, y, width, height)
    }

    #[test]
    fn recording_windows_display_crop_uses_signed_desktop_origin_and_rejects_spanning_region() {
        let display = screen(-1920, -120, 1920, 1080);
        assert_eq!(
            display_crop(display, screen(-1800, 20, 640, 400)).unwrap(),
            SourceCrop {
                x: 120,
                y: 140,
                width: 640,
                height: 400,
            }
        );
        assert!(display_crop(display, screen(-100, 20, 640, 400)).is_err());
    }

    #[test]
    fn recording_windows_window_crop_tracks_the_exact_current_source_extent() {
        assert_eq!(
            full_window_crop(720, 480).unwrap(),
            SourceCrop {
                x: 0,
                y: 0,
                width: 720,
                height: 480,
            }
        );
        assert!(full_window_crop(0, 480).is_err());
    }

    #[test]
    fn recording_windows_checked_gpu_budget_covers_two_pool_buffers_and_staging() {
        let source = (100, 50);
        let crop = SourceCrop {
            x: 5,
            y: 8,
            width: 20,
            height: 10,
        };
        assert_eq!(
            checked_gpu_bytes(source, crop).unwrap(),
            (100 * 50 * 4 * 2) + (20 * 10 * 4)
        );
        assert!(checked_gpu_bytes((u32::MAX, u32::MAX), crop).is_err());
        assert!(checked_source_bytes(MAX_CAPTURE_SOURCE_DIMENSION + 1, 1).is_err());
        assert!(
            checked_gpu_bytes(
                source,
                SourceCrop {
                    x: 95,
                    y: 0,
                    width: 10,
                    height: 1
                }
            )
            .is_err()
        );
        let scratch_crop = SourceCrop {
            x: 0,
            y: 0,
            width: MAX_CAPTURE_SOURCE_DIMENSION,
            height: 2_048,
        };
        assert!(checked_cpu_scratch_bytes(scratch_crop, CanvasSize::new(1, 1).unwrap()).is_err());
    }

    #[test]
    fn recording_windows_fit_keeps_original_canvas_and_aspect_ratio() {
        let canvas = CanvasSize::new(320, 180).unwrap();
        assert_eq!(fitted_dimensions((640, 480), canvas).unwrap(), (240, 180));
        let pixels = vec![255; 640 * 480 * 4];
        let frame = fit_rgba_frame(pixels, (640, 480), canvas).unwrap();
        assert_eq!(frame.size(), canvas);
        assert_eq!(frame.pixels().len(), canvas.rgba_bytes());
        assert_eq!(&frame.pixels()[0..4], &[0, 0, 0, 255]);
        assert!(fit_rgba_frame(vec![0; 4], (2, 2), canvas).is_err());
    }

    #[cfg(windows)]
    mod native_smoke {
        use super::*;
        use crate::platform::windows_capture_test_support::{OwnedSourceFixture, SourceReady};
        use crate::recording::gif::GifRecording;
        use crate::recording::{
            CaptureTarget, FrameRate, RecorderClock, RecorderState, RecordingEndReason,
            RecordingFormat, RecordingOptions, RecordingRequest, RecordingSession,
        };
        use std::io::Cursor;
        use std::sync::Arc;
        use std::thread;
        use std::time::{Duration, Instant, SystemTime};
        use windows::Win32::Foundation::{HWND, RECT};
        use windows::Win32::Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
        use windows::Win32::UI::WindowsAndMessaging::{GetWindowThreadProcessId, IsWindow};

        struct FixtureClock(Instant);

        impl RecorderClock for FixtureClock {
            fn now(&self) -> Duration {
                self.0.elapsed()
            }
        }

        fn fixture_nonce() -> String {
            format!(
                "{:x}",
                SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |duration| duration.as_nanos())
            )
        }

        fn phase_anchors(phase: char) -> [[u8; 4]; 2] {
            match phase {
                'A' => [[250, 210, 25, 255], [20, 180, 245, 255]],
                'B' => [[35, 240, 70, 255], [250, 90, 15, 255]],
                _ => unreachable!(),
            }
        }

        const GIF_ANCHOR_RGB_TOLERANCE: u8 = 24;

        fn anchors_match_with_gif_tolerance(actual: [[u8; 4]; 2], expected: [[u8; 4]; 2]) -> bool {
            actual.iter().zip(expected).all(|(actual, expected)| {
                actual[3] == 255
                    && (0..3).all(|channel| {
                        actual[channel].abs_diff(expected[channel]) <= GIF_ANCHOR_RGB_TOLERANCE
                    })
            })
        }

        fn anchors(frame: &RgbaFrame) -> [[u8; 4]; 2] {
            assert_eq!((frame.size().width(), frame.size().height()), (360, 240));
            let sample = |x: u32, y: u32| {
                let offset = ((y * frame.size().width() + x) * 4) as usize;
                frame.pixels()[offset..offset + 4]
                    .try_into()
                    .expect("four-byte RGBA pixel")
            };
            [sample(51, 65), sample(86, 89)]
        }

        fn take_and_ack_latest(
            session: &RecordingSession,
            generation: u64,
            last_elapsed: &mut Option<Duration>,
        ) -> Result<Option<([u8; 4], [u8; 4], Duration)>, String> {
            let view = session.view();
            if !matches!(
                view.state(),
                RecorderState::Recording { .. } | RecorderState::Finalizing { .. }
            ) {
                return Ok(None);
            }
            let Some(frame) = session
                .take_latest_frame(generation)
                .map_err(|error| error.to_string())?
            else {
                return Ok(None);
            };
            let timed = frame.frame();
            let elapsed = timed.elapsed();
            if last_elapsed.is_some_and(|previous| elapsed < previous) {
                return Err("native capture timestamps moved backwards".into());
            }
            *last_elapsed = Some(elapsed);
            let anchors = anchors(timed.frame());
            frame
                .acknowledge_written()
                .map_err(|error| error.to_string())?;
            Ok(Some((anchors[0], anchors[1], elapsed)))
        }

        fn fail_if_session_failed(session: &RecordingSession) -> Result<(), String> {
            if let RecorderState::Failed(failure) = session.view().state() {
                return Err(format!("native recorder failed: {failure}"));
            }
            Ok(())
        }

        fn wait_for_phase(
            session: &RecordingSession,
            generation: u64,
            phase: char,
            last_elapsed: &mut Option<Duration>,
            timeout: Duration,
        ) -> Result<Duration, String> {
            let expected = phase_anchors(phase);
            let deadline = Instant::now() + timeout;
            loop {
                session.poll().map_err(|error| error.to_string())?;
                if let Some((first, second, elapsed)) =
                    take_and_ack_latest(session, generation, last_elapsed)?
                    && [first, second] == expected
                {
                    return Ok(elapsed);
                }
                fail_if_session_failed(session)?;
                if Instant::now() >= deadline {
                    return Err(format!(
                        "native WGC source did not deliver paint phase {phase}"
                    ));
                }
                thread::sleep(Duration::from_millis(5));
            }
        }

        fn verify_fixture_identity(ready: &SourceReady, fixture: &OwnedSourceFixture) {
            let hwnd = HWND(ready.hwnd as *mut core::ffi::c_void);
            assert_eq!(ready.process_id, fixture.process_id());
            assert_ne!(ready.process_id, std::process::id());
            assert_eq!((ready.bounds.width, ready.bounds.height), (360, 240));
            assert!(ready.bounds.validate_capture().is_ok());
            assert!(unsafe { IsWindow(hwnd) }.as_bool());
            let mut process_id = 0;
            let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
            assert_eq!(process_id, ready.process_id);
            assert_eq!(thread_id, ready.thread_id);
            let mut bounds = RECT::default();
            unsafe {
                DwmGetWindowAttribute(
                    hwnd,
                    DWMWA_EXTENDED_FRAME_BOUNDS,
                    (&mut bounds as *mut RECT).cast(),
                    std::mem::size_of::<RECT>() as u32,
                )
            }
            .expect("could not revalidate fixture physical bounds");
            assert_eq!(
                ready.bounds,
                ScreenRect::new(
                    bounds.left,
                    bounds.top,
                    u32::try_from(i64::from(bounds.right) - i64::from(bounds.left)).unwrap(),
                    u32::try_from(i64::from(bounds.bottom) - i64::from(bounds.top)).unwrap(),
                )
            );
        }

        #[test]
        #[ignore = "requires an interactive Windows desktop with Windows Graphics Capture support"]
        fn native_owned_hwnd_capture_observes_paint_and_retires() {
            let nonce = fixture_nonce();
            let mut fixture = OwnedSourceFixture::spawn(&nonce)
                .unwrap_or_else(|error| panic!("could not launch owned HWND fixture: {error}"));
            let ready = fixture
                .wait_ready(&nonce)
                .unwrap_or_else(|error| panic!("owned HWND fixture was not ready: {error}"));
            verify_fixture_identity(&ready, &fixture);
            fixture.command("PAINT A").unwrap();

            let frame_rate = FrameRate::new(30).unwrap();
            let options = RecordingOptions::new(
                RecordingFormat::Gif,
                frame_rate,
                Duration::from_secs(10),
                false,
            )
            .unwrap();
            let target =
                CaptureTarget::window(ready.hwnd, ready.process_id, ready.thread_id, ready.bounds)
                    .unwrap();
            let request = RecordingRequest::new(target, ready.bounds, options).unwrap();
            let clock = Arc::new(FixtureClock(Instant::now()));
            let session =
                RecordingSession::new(clock.clone(), Arc::new(WindowsRecordingBackend), || {});
            let generation = session.select(request).unwrap();
            session.start().unwrap();

            let mut last_elapsed = None;
            let phase_a = wait_for_phase(
                &session,
                generation,
                'A',
                &mut last_elapsed,
                Duration::from_secs(10),
            )
            .unwrap_or_else(|error| panic!("native phase A was not captured: {error}"));
            let cadence_boundary = clock.now() + frame_rate.interval();
            while clock.now() < cadence_boundary {
                session.poll().unwrap();
                fail_if_session_failed(&session).unwrap();
                let remaining = cadence_boundary.saturating_sub(clock.now());
                if !remaining.is_zero() {
                    thread::sleep(remaining.min(Duration::from_millis(5)));
                }
            }
            fixture.command("PAINT B").unwrap();
            let phase_b = wait_for_phase(
                &session,
                generation,
                'B',
                &mut last_elapsed,
                Duration::from_secs(5),
            )
            .unwrap_or_else(|error| panic!("native phase B was not captured: {error}"));
            assert!(phase_b > phase_a, "paint timestamps did not increase");
            assert!(
                phase_b >= phase_a + frame_rate.interval(),
                "phase B preceded the next recording cadence boundary"
            );

            fixture.command("CLOSE_SOURCE").unwrap();
            let close_deadline = Instant::now() + Duration::from_secs(5);
            loop {
                session.poll().unwrap();
                if let Some((_, _, elapsed)) =
                    take_and_ack_latest(&session, generation, &mut last_elapsed).unwrap()
                    && elapsed < phase_b
                {
                    panic!("a stale frame preceded the acknowledged phase B timestamp");
                }
                let view = session.view();
                if let Some(terminal) = view.terminal() {
                    assert_eq!(terminal.generation(), generation);
                    assert_eq!(terminal.reason(), RecordingEndReason::TargetLost);
                    if !view.worker_busy() {
                        break;
                    }
                }
                fail_if_session_failed(&session).unwrap();
                assert!(
                    Instant::now() < close_deadline,
                    "capture worker did not retire after HWND close"
                );
                thread::sleep(Duration::from_millis(5));
            }
            let retired = session.view();
            assert!(!retired.worker_busy());
            assert!(retired.accepted_frames() >= 2);
            assert!(retired.frames_written() >= 2);
            session.close();
            fixture.finish().unwrap();
        }

        #[test]
        #[ignore = "requires an interactive Windows desktop with Windows Graphics Capture support"]
        fn native_owned_hwnd_capture_writes_decodable_gif() {
            use image::{AnimationDecoder, GenericImageView};

            let nonce = fixture_nonce();
            let mut fixture = OwnedSourceFixture::spawn(&nonce)
                .unwrap_or_else(|error| panic!("could not launch owned HWND fixture: {error}"));
            let ready = fixture
                .wait_ready(&nonce)
                .unwrap_or_else(|error| panic!("owned HWND fixture was not ready: {error}"));
            verify_fixture_identity(&ready, &fixture);
            fixture.command("PAINT A").unwrap();

            let frame_rate = FrameRate::new(30).unwrap();
            let options = RecordingOptions::new(
                RecordingFormat::Gif,
                frame_rate,
                Duration::from_secs(10),
                false,
            )
            .unwrap();
            let target =
                CaptureTarget::window(ready.hwnd, ready.process_id, ready.thread_id, ready.bounds)
                    .unwrap();
            let request = RecordingRequest::new(target, ready.bounds, options).unwrap();
            let clock = Arc::new(FixtureClock(Instant::now()));
            let session = Arc::new(RecordingSession::new(
                clock.clone(),
                Arc::new(WindowsRecordingBackend),
                || {},
            ));
            let generation = session.select(request).unwrap();
            let output_directory = tempfile::tempdir().unwrap();
            let destination = output_directory.path().join("owned-window.gif");
            let mut recording =
                GifRecording::create(Arc::clone(&session), generation, &destination).unwrap();
            session.start().unwrap();

            let expected_a = phase_anchors('A');
            let phase_a_deadline = Instant::now() + Duration::from_secs(10);
            let phase_a = loop {
                session.poll().unwrap();
                fail_if_session_failed(&session).unwrap();
                let state = session.view().state().clone();
                if !matches!(
                    state,
                    RecorderState::Recording { .. } | RecorderState::Finalizing { .. }
                ) {
                    assert!(
                        Instant::now() < phase_a_deadline,
                        "native GIF capture did not enter Recording"
                    );
                    thread::sleep(Duration::from_millis(5));
                    continue;
                }
                let mut frame_anchors = None;
                let elapsed = recording
                    .consume_latest_observing(|timed| {
                        frame_anchors = Some(anchors(timed.frame()));
                    })
                    .unwrap();
                if let (Some(elapsed), Some(frame_anchors)) = (elapsed, frame_anchors)
                    && frame_anchors == expected_a
                {
                    break elapsed;
                }
                assert!(
                    Instant::now() < phase_a_deadline,
                    "native GIF capture did not consume phase A"
                );
                thread::sleep(Duration::from_millis(5));
            };

            // Cadence wait is measured from the wall instant when A's frame
            // lease was accepted by the staged encoder.
            let phase_a_ack_wall = Instant::now();
            let cadence_boundary = phase_a_ack_wall + frame_rate.interval();
            while Instant::now() < cadence_boundary {
                session.poll().unwrap();
                fail_if_session_failed(&session).unwrap();
                recording.consume_latest().unwrap();
                let remaining = cadence_boundary.saturating_duration_since(Instant::now());
                if !remaining.is_zero() {
                    thread::sleep(remaining.min(Duration::from_millis(5)));
                }
            }

            fixture.command("PAINT B").unwrap();
            let expected_b = phase_anchors('B');
            let phase_b_deadline = Instant::now() + Duration::from_secs(5);
            let phase_b = loop {
                session.poll().unwrap();
                fail_if_session_failed(&session).unwrap();
                let mut frame_anchors = None;
                let elapsed = recording
                    .consume_latest_observing(|timed| {
                        frame_anchors = Some(anchors(timed.frame()));
                    })
                    .unwrap();
                if let (Some(elapsed), Some(frame_anchors)) = (elapsed, frame_anchors)
                    && frame_anchors == expected_b
                    && elapsed >= phase_a + frame_rate.interval()
                {
                    break elapsed;
                }
                assert!(
                    Instant::now() < phase_b_deadline,
                    "native GIF capture did not consume phase B after cadence"
                );
                thread::sleep(Duration::from_millis(5));
            };
            assert!(phase_b >= phase_a + frame_rate.interval());

            // Keep B visible long enough to receive at least one GIF tick;
            // a zero-duration trailing update is intentionally omitted.
            let hold_b_until = Instant::now() + Duration::from_millis(40);
            while Instant::now() < hold_b_until {
                session.poll().unwrap();
                fail_if_session_failed(&session).unwrap();
                recording.consume_latest().unwrap();
                let remaining = hold_b_until.saturating_duration_since(Instant::now());
                if !remaining.is_zero() {
                    thread::sleep(remaining.min(Duration::from_millis(5)));
                }
            }

            assert!(
                session.stop().unwrap(),
                "user Stop should own this terminal"
            );
            let terminal_deadline = Instant::now() + Duration::from_secs(5);
            let terminal = loop {
                session.poll().unwrap();
                fail_if_session_failed(&session).unwrap();
                recording.consume_latest().unwrap();
                let view = session.view();
                if let Some(terminal) = view.terminal()
                    && matches!(view.state(), RecorderState::Finalizing { .. })
                    && !view.worker_busy()
                    && recording.consume_latest().unwrap().is_none()
                {
                    break terminal;
                }
                assert!(
                    Instant::now() < terminal_deadline,
                    "capture worker did not retire before GIF finalization"
                );
                thread::sleep(Duration::from_millis(5));
            };
            assert_eq!(terminal.generation(), generation);
            assert_eq!(terminal.reason(), RecordingEndReason::UserStopped);
            let published = recording.finish_and_publish().unwrap();
            assert_eq!(published, destination);
            assert!(matches!(
                session.view().state(),
                RecorderState::Completed(_)
            ));

            let bytes = std::fs::read(&published).unwrap();
            let decoded = image::codecs::gif::GifDecoder::new(Cursor::new(bytes))
                .unwrap()
                .into_frames()
                .collect_frames()
                .unwrap();
            assert!(!decoded.is_empty(), "published GIF contains no images");
            let mut observed_a = false;
            let mut observed_b = false;
            let mut total_delay_ms = 0_u128;
            for frame in &decoded {
                assert_eq!(frame.buffer().dimensions(), (360, 240));
                let pixel = |x: u32, y: u32| frame.buffer().get_pixel(x, y).0;
                let samples = [pixel(51, 65), pixel(86, 89)];
                observed_a |= anchors_match_with_gif_tolerance(samples, expected_a);
                observed_b |= anchors_match_with_gif_tolerance(samples, expected_b);
                let (numerator, denominator) = frame.delay().numer_denom_ms();
                assert_ne!(denominator, 0);
                total_delay_ms += u128::from(numerator) / u128::from(denominator);
            }
            assert!(observed_a, "decoded GIF omitted fixture paint phase A");
            assert!(observed_b, "decoded GIF omitted fixture paint phase B");
            assert!(
                total_delay_ms.abs_diff(terminal.elapsed().as_millis()) <= 5,
                "decoded GIF duration {total_delay_ms}ms differs from terminal {}ms",
                terminal.elapsed().as_millis()
            );

            session.close();
            fixture.finish().unwrap();
        }
    }
}
