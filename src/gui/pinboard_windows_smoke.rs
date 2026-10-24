//! One bounded native compositor exercise for the real screenshot-pin presenter.
//!
//! The fixture paints only generated colors and samples only explicitly owned
//! points. A successful run is evidence for the generated static presenter
//! windows, not for an external/OS capture source.

use super::*;
use crate::pinboard::live::PinLiveCaptureSession;
use crate::pinboard::source::{PinSourceBinding, PinSourceCrop, PinSourceWindowIdentity};
use crate::pinboard::{Pin, PinSnapshot};
use image::RgbaImage;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use windows::Win32::Foundation::{BOOL, COLORREF, FILETIME, HWND, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Dwm::DwmFlush;
use windows::Win32::Graphics::Gdi::{ClientToScreen, GetDC, GetPixel, ReleaseDC};
use windows::Win32::System::SystemInformation::GetTickCount64;
use windows::Win32::System::Threading::{GetCurrentProcess, GetCurrentProcessId, GetProcessTimes};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetLastInputInfo, INPUT, INPUT_0, INPUT_MOUSE, LASTINPUTINFO,
    MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE,
    MOUSEEVENTF_VIRTUALDESK, MOUSEINPUT, SendInput,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClientRect, GetForegroundWindow, GetSystemMetrics, GetWindowRect,
    GetWindowThreadProcessId, IsWindow, IsWindowVisible, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
    SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SetForegroundWindow, WindowFromPoint,
};

const ROOT_COLOR: [u8; 4] = [24, 52, 88, 255];
const FIRST_COLOR: [u8; 4] = [236, 42, 38, 255];
const SECOND_COLOR: [u8; 4] = [24, 210, 92, 255];
const UPDATED_SECOND_COLOR: [u8; 4] = [36, 92, 224, 255];
const SAMPLE_TOLERANCE: u8 = 14;
const MIN_INPUT_IDLE: Duration = Duration::from_millis(750);
const FRAME_REPAINT: Duration = Duration::from_millis(40);
const PHASE_TIMEOUT: Duration = Duration::from_secs(12);
const RUN_TIMEOUT: Duration = Duration::from_secs(90);
const CLOSE_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone)]
struct SmokeLog(Arc<Mutex<File>>);

impl SmokeLog {
    fn line(&self, line: impl AsRef<str>) {
        if let Ok(mut file) = self.0.lock() {
            let _ = writeln!(file, "{}", line.as_ref());
            let _ = file.flush();
        }
    }
}

#[derive(Clone)]
struct MemoryImageClipboard(Arc<Mutex<Option<RgbaImage>>>);

impl PinImageClipboard for MemoryImageClipboard {
    fn copy_image(&self, image: &RgbaImage) -> Result<(), String> {
        let mut copied = self
            .0
            .lock()
            .map_err(|_| "generated image clipboard lock was poisoned".to_owned())?;
        *copied = Some(image.clone());
        Ok(())
    }
}

struct FixedSavePicker(PathBuf);

impl PinSavePicker for FixedSavePicker {
    fn save_file(
        &self,
        pin_id: PinId,
        cancellation: &PinPickerCancellation,
    ) -> Result<Option<PathBuf>, String> {
        if !cancellation.try_admit_dialog() {
            return Ok(None);
        }
        Ok(Some(
            self.0.join(format!("generated-pin-{}.png", pin_id.get())),
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    AwaitInitialWindows,
    SampleInitial,
    RequestStaticFifty,
    AwaitStaticFifty,
    SampleStaticFifty,
    RequestLiveFifty,
    AwaitLiveFifty,
    SampleBothFifty,
    RequestLiveFrame,
    AwaitLiveFrame,
    SampleLiveFrame,
    CopyStaticImage,
    AwaitStaticCopy,
    SaveStaticImage,
    AwaitStaticSave,
    CopyLiveImage,
    AwaitLiveCopy,
    SaveLiveImage,
    AwaitLiveSave,
    RequestHide,
    AwaitHidden,
    RequestRestore,
    AwaitRestoredOpacity,
    SampleRestored,
    InjectReset,
    AwaitReset,
    SampleReset,
    RequestClickThrough,
    AwaitClickThrough,
    InjectRootClick,
    AwaitRootClick,
    ClosePins,
    AwaitRetirement,
    Finish,
}

#[derive(Clone, Copy)]
enum ExpectedLivePixels {
    Initial,
    Updated,
}

#[derive(Clone, Copy)]
struct PixelRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl PixelRect {
    fn contains(self, point: POINT) -> bool {
        point.x >= self.left && point.x < self.right && point.y >= self.top && point.y < self.bottom
    }

    fn contains_rect(self, other: Self) -> bool {
        other.left >= self.left
            && other.top >= self.top
            && other.right <= self.right
            && other.bottom <= self.bottom
    }
}

#[derive(Clone)]
struct SamplePoint {
    point: POINT,
    root: HWND,
    visual_pin: Option<(PinId, u8, bool, NativeWindowBinding)>,
    expected_hit: HWND,
}

struct ZOrderScan {
    point: POINT,
    root: usize,
    visual_pin: Option<usize>,
    pin_seen: bool,
    complete: bool,
    unknown_bounds: Option<usize>,
    obstruction: Option<usize>,
}

unsafe extern "system" fn inspect_z_order(hwnd: HWND, parameter: LPARAM) -> BOOL {
    let scan = unsafe { &mut *(parameter.0 as *mut ZOrderScan) };
    if !unsafe { IsWindowVisible(hwnd) }.as_bool() {
        return BOOL(1);
    }
    let mut rect = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut rect) }.is_err() {
        scan.unknown_bounds = Some(hwnd.0 as usize);
        return BOOL(0);
    }
    if scan.point.x < rect.left
        || scan.point.x >= rect.right
        || scan.point.y < rect.top
        || scan.point.y >= rect.bottom
    {
        return BOOL(1);
    }
    let handle = hwnd.0 as usize;
    if let Some(pin) = scan.visual_pin {
        if !scan.pin_seen {
            if handle == pin {
                scan.pin_seen = true;
                return BOOL(1);
            }
            scan.obstruction = Some(handle);
            return BOOL(0);
        }
        if handle == scan.root {
            scan.complete = true;
            return BOOL(0);
        }
        scan.obstruction = Some(handle);
        return BOOL(0);
    }
    if handle == scan.root {
        scan.complete = true;
        return BOOL(0);
    }
    scan.obstruction = Some(handle);
    BOOL(0)
}

struct SmokeApp {
    board: Pinboard,
    presenter: PinWindowPresenter,
    visibility: Box<dyn PinWindowVisibilityCoordinator>,
    visibility_identity: crate::pinboard::PinCaptureIdentity,
    first: PinId,
    second: PinId,
    first_pixels: RgbaImage,
    second_pixels: RgbaImage,
    live_frame_pixels: RgbaImage,
    live_frame_sender: SyncSender<RgbaImage>,
    clipboard: Arc<Mutex<Option<RgbaImage>>>,
    first_save_path: PathBuf,
    second_save_path: PathBuf,
    log: SmokeLog,
    result: Arc<Mutex<Option<Result<(), String>>>>,
    nonce: u128,
    process_id: u32,
    process_birth: u64,
    started: Instant,
    stage_started: Instant,
    stage: Stage,
    fatal: Option<String>,
    root_hwnd: Option<HWND>,
    canvas_rect: Option<egui::Rect>,
    root_pixels_per_point: f32,
    click_count: usize,
    click_count_before: Option<usize>,
    window_positions_configured: bool,
    copy_request_id: Option<u64>,
    save_request_id: Option<u64>,
    shutdown_succeeded: bool,
    shutdown_attempted: bool,
    closed_normally: bool,
    last_readiness_reason: Option<String>,
    samples: Vec<serde_json::Value>,
    receipt_written: bool,
}

impl SmokeApp {
    fn new(
        cc: &eframe::CreationContext<'_>,
        output_dir: PathBuf,
        nonce: u128,
        log: SmokeLog,
        result: Arc<Mutex<Option<Result<(), String>>>>,
    ) -> Result<Self, String> {
        if cc.egui_ctx.embed_viewports() {
            return Err("GAP: native deferred pin viewports are unavailable".into());
        }
        let first_pixels = RgbaImage::from_pixel(240, 160, image::Rgba(FIRST_COLOR));
        let second_pixels = RgbaImage::from_pixel(240, 160, image::Rgba(SECOND_COLOR));
        let live_frame_pixels = RgbaImage::from_pixel(240, 160, image::Rgba(UPDATED_SECOND_COLOR));
        let mut board = Pinboard::default();
        let first = insert_generated_pin(&mut board, 1, first_pixels.clone())?;
        let (second, live_frame_sender) =
            insert_generated_live_pin(&mut board, 2, second_pixels.clone())?;
        let clipboard = Arc::new(Mutex::new(None));
        let first_save_path = output_dir.join(format!("generated-pin-{}.png", first.get()));
        let second_save_path = output_dir.join(format!("generated-pin-{}.png", second.get()));
        let presenter = PinWindowPresenter::with_native_api_and_targets(
            Arc::new(SystemPinNativeApi),
            Arc::new(FixedSavePicker(output_dir.clone())),
            Arc::new(MemoryImageClipboard(Arc::clone(&clipboard))),
        );
        let visibility_identity = board
            .reserve_capture(
                crate::pinboard::PinCaptureOperationId::new(99)
                    .ok_or_else(|| "visibility operation ID was invalid".to_owned())?,
            )
            .map_err(|error| format!("reserve visibility fixture: {error}"))?;
        let visibility = presenter.visibility_coordinator();
        let process_id = unsafe { GetCurrentProcessId() };
        let process_birth = current_process_birth()?;
        log.line(format!(
            "phase=created nonce={nonce:032x} pid={process_id} process_birth={process_birth} backend=eframe-glow source=generated-static-plus-live-budget-session"
        ));
        Ok(Self {
            board,
            presenter,
            visibility,
            visibility_identity,
            first,
            second,
            first_pixels,
            second_pixels,
            live_frame_pixels,
            live_frame_sender,
            clipboard,
            first_save_path,
            second_save_path,
            log,
            result,
            nonce,
            process_id,
            process_birth,
            started: Instant::now(),
            stage_started: Instant::now(),
            stage: Stage::AwaitInitialWindows,
            fatal: None,
            root_hwnd: None,
            canvas_rect: None,
            root_pixels_per_point: 1.0,
            click_count: 0,
            click_count_before: None,
            window_positions_configured: false,
            copy_request_id: None,
            save_request_id: None,
            shutdown_succeeded: false,
            shutdown_attempted: false,
            closed_normally: false,
            last_readiness_reason: None,
            samples: Vec::new(),
            receipt_written: false,
        })
    }

    fn set_stage(&mut self, stage: Stage) {
        self.stage = stage;
        self.stage_started = Instant::now();
        self.last_readiness_reason = None;
        self.log.line(format!(
            "phase={stage:?} elapsed_ms={}",
            self.started.elapsed().as_millis()
        ));
    }

    fn observe_readiness(&mut self, phase: &str, result: Result<(), String>) -> bool {
        match result {
            Ok(()) => {
                self.last_readiness_reason = None;
                true
            }
            Err(reason) if reason.starts_with("FAIL: ") => {
                self.defect(format!("{phase}: {}", reason.trim_start_matches("FAIL: ")));
                false
            }
            Err(reason) => {
                if self.last_readiness_reason.as_deref() != Some(reason.as_str()) {
                    self.log.line(format!(
                        "readiness phase={phase} elapsed_ms={} reason={reason}",
                        self.started.elapsed().as_millis()
                    ));
                }
                self.last_readiness_reason = Some(reason);
                false
            }
        }
    }

    fn visibility_failure(&mut self, phase: &str, error: String) {
        if Self::native_failure_is_gap(&error) {
            self.gap(format!("{phase} readiness failed: {error}"));
        } else {
            self.defect(format!("{phase} native operation failed: {error}"));
        }
    }

    fn native_failure_is_gap(error: &str) -> bool {
        error.contains("unavailable")
            || error.contains("incompatible")
            || error.contains("not yet")
            || error.contains("not available")
            || error.contains("unsupported")
            || error.contains("waiting for the owned pin window to appear")
    }

    fn fail(&mut self, kind: &str, message: impl Into<String>) {
        if self.fatal.is_none() {
            let message = message.into();
            self.log.line(format!("result={kind} reason={message}"));
            self.fatal = Some(format!("{kind}: {message}"));
            self.set_stage(Stage::ClosePins);
        }
    }

    fn gap(&mut self, message: impl Into<String>) {
        self.fail("GAP", message);
    }

    fn defect(&mut self, message: impl Into<String>) {
        self.fail("FAIL", message);
    }

    fn composition_result(&mut self, message: String) {
        if let Some(message) = message.strip_prefix("GAP: ") {
            self.gap(message.to_owned());
        } else {
            self.defect(message);
        }
    }

    fn advance(&mut self, ctx: &egui::Context) {
        if self.stage != Stage::ClosePins
            && self.stage != Stage::AwaitRetirement
            && self.stage != Stage::Finish
            && self.started.elapsed() > RUN_TIMEOUT
        {
            let detail = self
                .last_readiness_reason
                .as_deref()
                .map(|reason| format!(": {reason}"))
                .unwrap_or_default();
            self.gap(format!("the bounded native phase deadline expired{detail}"));
        }
        if self.stage != Stage::ClosePins
            && self.stage != Stage::AwaitRetirement
            && self.stage != Stage::Finish
            && self.stage_started.elapsed() > PHASE_TIMEOUT
        {
            let detail = self
                .last_readiness_reason
                .as_deref()
                .map(|reason| format!(": {reason}"))
                .unwrap_or_default();
            self.gap(format!(
                "phase {:?} did not reach its receipt{detail}",
                self.stage
            ));
        }

        match self.stage {
            Stage::AwaitInitialWindows => {
                let readiness = self.presentation_ready(100, 100);
                if self.observe_readiness("initial presentation", readiness) {
                    self.set_stage(Stage::SampleInitial);
                }
            }
            Stage::SampleInitial => match self.verify_composition(
                100,
                100,
                "initial-static-live-100-100",
                ExpectedLivePixels::Initial,
            ) {
                Ok(()) => self.set_stage(Stage::RequestStaticFifty),
                Err(error) => self.composition_result(error),
            },
            Stage::RequestStaticFifty => match self.apply_control(
                ctx,
                self.first,
                PinControlChange {
                    opacity_percent: Some(50),
                    ..Default::default()
                },
            ) {
                Ok(()) => self.set_stage(Stage::AwaitStaticFifty),
                Err(error) => self.native_control_error(error),
            },
            Stage::AwaitStaticFifty => {
                let readiness = self.presentation_ready(50, 100);
                if self.observe_readiness("static 50/live 100 presentation", readiness) {
                    self.set_stage(Stage::SampleStaticFifty);
                }
            }
            Stage::SampleStaticFifty => {
                match self.verify_composition(
                    50,
                    100,
                    "static-50-live-100",
                    ExpectedLivePixels::Initial,
                ) {
                    Ok(()) => self.set_stage(Stage::RequestLiveFifty),
                    Err(error) => self.composition_result(error),
                }
            }
            Stage::RequestLiveFifty => match self.apply_control(
                ctx,
                self.second,
                PinControlChange {
                    opacity_percent: Some(50),
                    ..Default::default()
                },
            ) {
                Ok(()) => self.set_stage(Stage::AwaitLiveFifty),
                Err(error) => self.native_control_error(error),
            },
            Stage::AwaitLiveFifty => {
                let readiness = self.presentation_ready(50, 50);
                if self.observe_readiness("static/live 50 presentation", readiness) {
                    self.set_stage(Stage::SampleBothFifty);
                }
            }
            Stage::SampleBothFifty => {
                match self.verify_composition(
                    50,
                    50,
                    "static-live-50-50",
                    ExpectedLivePixels::Initial,
                ) {
                    Ok(()) => self.set_stage(Stage::RequestLiveFrame),
                    Err(error) => self.composition_result(error),
                }
            }
            Stage::RequestLiveFrame => match self
                .live_frame_sender
                .try_send(self.live_frame_pixels.clone())
            {
                Ok(()) => {
                    self.log
                        .line("phase=generated-live-frame request=sequence-2");
                    self.set_stage(Stage::AwaitLiveFrame);
                }
                Err(error) => self.defect(format!(
                    "generated live-frame update could not be queued: {error}"
                )),
            },
            Stage::AwaitLiveFrame => {
                let frame = self.board.get(self.second).and_then(Pin::live_frame);
                if let Some(frame) = frame
                    && frame.sequence >= 2
                    && frame.image.pixels() == &self.live_frame_pixels
                {
                    self.log.line(format!(
                        "phase=generated-live-frame result=published sequence={} dimensions=240x160 source=synthetic-production-budget-session",
                        frame.sequence
                    ));
                    self.set_stage(Stage::SampleLiveFrame);
                }
            }
            Stage::SampleLiveFrame => match self.verify_composition(
                50,
                50,
                "updated-live-frame-static-live-50-50",
                ExpectedLivePixels::Updated,
            ) {
                Ok(()) => self.set_stage(Stage::CopyStaticImage),
                Err(error) => self.composition_result(error),
            },
            Stage::CopyStaticImage => match self.start_action(self.first, PinActionKind::Copy) {
                Ok(id) => {
                    self.copy_request_id = Some(id);
                    self.set_stage(Stage::AwaitStaticCopy);
                }
                Err(error) => self.defect(error),
            },
            Stage::AwaitStaticCopy => {
                match self.action_finished(self.first, self.copy_request_id) {
                    Ok(true) => {
                        let copied = self.clipboard.lock().ok().and_then(|value| value.clone());
                        if copied.as_ref() == Some(&self.first_pixels) {
                            self.log
                                .line("phase=generated-static-clipboard-copy result=exact-rgba");
                            self.set_stage(Stage::SaveStaticImage);
                        } else {
                            self.defect(
                                "the fake image clipboard did not receive the static pin RGBA",
                            );
                        }
                    }
                    Ok(false) => {}
                    Err(error) => self.defect(error),
                }
            }
            Stage::SaveStaticImage => match self.start_action(self.first, PinActionKind::SaveAs) {
                Ok(id) => {
                    self.save_request_id = Some(id);
                    self.set_stage(Stage::AwaitStaticSave);
                }
                Err(error) => self.defect(error),
            },
            Stage::AwaitStaticSave => {
                match self.action_finished(self.first, self.save_request_id) {
                    Ok(true) => match image::open(&self.first_save_path) {
                        Ok(image) if image.to_rgba8() == self.first_pixels => {
                            self.log.line(format!(
                                "phase=generated-static-png-export result=exact-rgba dimensions=240x160 path={}",
                                self.first_save_path.display()
                            ));
                            self.set_stage(Stage::CopyLiveImage);
                        }
                        Ok(_) => self
                            .defect("the static pin PNG export changed its original RGBA pixels"),
                        Err(error) => self.defect(format!(
                            "the static pin PNG export could not be read: {error}"
                        )),
                    },
                    Ok(false) => {}
                    Err(error) => self.defect(error),
                }
            }
            Stage::CopyLiveImage => match self.start_action(self.second, PinActionKind::Copy) {
                Ok(id) => {
                    self.copy_request_id = Some(id);
                    self.set_stage(Stage::AwaitLiveCopy);
                }
                Err(error) => self.defect(error),
            },
            Stage::AwaitLiveCopy => match self.action_finished(self.second, self.copy_request_id) {
                Ok(true) => {
                    let copied = self.clipboard.lock().ok().and_then(|value| value.clone());
                    if copied.as_ref() == Some(&self.live_frame_pixels) {
                        self.log
                            .line("phase=generated-live-clipboard-copy result=exact-rgba");
                        self.set_stage(Stage::SaveLiveImage);
                    } else {
                        self.defect(
                            "the fake image clipboard did not receive the updated live RGBA",
                        );
                    }
                }
                Ok(false) => {}
                Err(error) => self.defect(error),
            },
            Stage::SaveLiveImage => match self.start_action(self.second, PinActionKind::SaveAs) {
                Ok(id) => {
                    self.save_request_id = Some(id);
                    self.set_stage(Stage::AwaitLiveSave);
                }
                Err(error) => self.defect(error),
            },
            Stage::AwaitLiveSave => match self.action_finished(self.second, self.save_request_id) {
                Ok(true) => match image::open(&self.second_save_path) {
                    Ok(image) if image.to_rgba8() == self.live_frame_pixels => {
                        self.log.line(format!(
                            "phase=generated-live-png-export result=exact-rgba dimensions=240x160 path={}",
                            self.second_save_path.display()
                        ));
                        self.set_stage(Stage::RequestHide);
                    }
                    Ok(_) => self.defect("the live pin PNG export changed its updated RGBA pixels"),
                    Err(error) => self.defect(format!(
                        "the live pin PNG export could not be read: {error}"
                    )),
                },
                Ok(false) => {}
                Err(error) => self.defect(error),
            },
            Stage::RequestHide => {
                match self.visibility.request_hide_all(self.visibility_identity) {
                    Ok(()) => {
                        self.set_stage(Stage::AwaitHidden);
                    }
                    Err(error) => self.gap(format!("pin hide request was refused: {error}")),
                }
            }
            Stage::AwaitHidden => match self.visibility.poll_hidden(self.visibility_identity) {
                PinVisibilityAck::Acknowledged => match self.all_pins_have_visibility(false) {
                    Ok(true) => {
                        self.log
                            .line("phase=hide result=all-owned-pin-windows-hidden");
                        self.set_stage(Stage::RequestRestore);
                    }
                    Ok(false) => {
                        self.observe_readiness(
                            "hide visibility",
                            Err("GAP: an owned pin window is still visible after hide acknowledgement".into()),
                        );
                    }
                    Err(error) => {
                        self.observe_readiness("hide visibility", Err(error));
                    }
                },
                PinVisibilityAck::Pending => {
                    self.observe_readiness(
                        "hide acknowledgement",
                        Err("GAP: owned pin visibility hide is still pending".into()),
                    );
                }
                PinVisibilityAck::Failed(error) => self.visibility_failure("pin hide", error),
            },
            Stage::RequestRestore => match self
                .visibility
                .request_restore_all(self.visibility_identity)
            {
                Ok(()) => {
                    self.set_stage(Stage::AwaitRestoredOpacity);
                }
                Err(error) => self.gap(format!("pin restore request was refused: {error}")),
            },
            Stage::AwaitRestoredOpacity => match self
                .visibility
                .poll_restored(self.visibility_identity)
            {
                PinVisibilityAck::Acknowledged => {
                    let readiness = self.presentation_ready(50, 50);
                    if self.observe_readiness("restored presentation", readiness) {
                        self.log
                            .line("phase=restore result=visible-and-opacity-reapplied");
                        self.set_stage(Stage::SampleRestored);
                    }
                }
                PinVisibilityAck::Pending => {
                    self.observe_readiness(
                        "restore acknowledgement",
                        Err("GAP: owned pin visibility restore is still pending".into()),
                    );
                }
                PinVisibilityAck::Failed(error) => self.visibility_failure("pin restore", error),
            },
            Stage::SampleRestored => match self.verify_composition(
                50,
                50,
                "after-hide-restore-static-live-50-50",
                ExpectedLivePixels::Updated,
            ) {
                Ok(()) => {
                    self.log
                        .line("phase=hide-restore-opacity result=reapplied-and-composited");
                    self.set_stage(Stage::InjectReset);
                }
                Err(error) => self.composition_result(error),
            },
            Stage::InjectReset => match self.inject_opacity_reset(ctx, self.second, 50) {
                Ok(()) => self.set_stage(Stage::AwaitReset),
                Err((is_gap, error)) => {
                    if is_gap {
                        self.gap(error);
                    } else {
                        self.defect(error);
                    }
                }
            },
            Stage::AwaitReset => {
                let readiness = self.presentation_ready(50, 100);
                if self.observe_readiness("actual Reset 100% control", readiness) {
                    self.set_stage(Stage::SampleReset);
                }
            }
            Stage::SampleReset => match self.verify_composition(
                50,
                100,
                "reset-live-100-static-remains-50",
                ExpectedLivePixels::Updated,
            ) {
                Ok(()) => self.set_stage(Stage::RequestClickThrough),
                Err(error) => self.composition_result(error),
            },
            Stage::RequestClickThrough => match self.apply_control(
                ctx,
                self.first,
                PinControlChange {
                    click_through: Some(true),
                    ..Default::default()
                },
            ) {
                Ok(()) => self.set_stage(Stage::AwaitClickThrough),
                Err(error) => self.native_control_error(error),
            },
            Stage::AwaitClickThrough => {
                let readiness = self.presentation_ready(50, 100).and_then(|()| {
                    self.click_through_enabled(self.first).and_then(|enabled| {
                        if enabled {
                            Ok(())
                        } else {
                            Err("GAP: first pin click-through state is not active".into())
                        }
                    })
                });
                if self.observe_readiness("click-through presentation", readiness) {
                    self.set_stage(Stage::InjectRootClick);
                }
            }
            Stage::InjectRootClick => match self.click_through_input_idle_age() {
                Ok(age) if age < MIN_INPUT_IDLE => {
                    self.last_readiness_reason = Some(format!(
                        "waiting for owned input idle guard: {}ms / {}ms",
                        age.as_millis(),
                        MIN_INPUT_IDLE.as_millis()
                    ));
                }
                Ok(_) => match self.inject_click_through(ctx) {
                    Ok(before) => {
                        self.click_count_before = Some(before);
                        self.set_stage(Stage::AwaitRootClick);
                    }
                    Err((is_gap, error)) => {
                        if is_gap {
                            self.gap(error);
                        } else {
                            self.defect(error);
                        }
                    }
                },
                Err((is_gap, error)) => {
                    if is_gap {
                        self.gap(error);
                    } else {
                        self.defect(error);
                    }
                }
            },
            Stage::AwaitRootClick => {
                if self
                    .click_count_before
                    .is_some_and(|before| self.click_count == before + 1)
                {
                    self.log.line(format!(
                        "phase=click-through-routing result=observed root_click_count={}",
                        self.click_count
                    ));
                    self.set_stage(Stage::ClosePins);
                }
            }
            Stage::ClosePins => {
                let first_closed = self.presenter.close(self.first, &mut self.board);
                let second_closed = self.presenter.close(self.second, &mut self.board);
                if (!first_closed && self.board.get(self.first).is_some())
                    || (!second_closed && self.board.get(self.second).is_some())
                {
                    self.defect("normal pin retirement could not remove both generated models");
                }
                let _ = self.board.cancel_capture(self.visibility_identity);
                self.set_stage(Stage::AwaitRetirement);
            }
            Stage::AwaitRetirement => {
                let registry_check = {
                    match self.presenter.registry.lock() {
                        Ok(registry) => Ok(registry.pins.is_empty() && registry.retired.is_empty()),
                        Err(_) => Err(()),
                    }
                };
                let registry_empty = match registry_check {
                    Ok(empty) => empty,
                    Err(()) => {
                        self.defect("pin registry lock was poisoned during worker retirement");
                        false
                    }
                };
                if registry_empty {
                    self.shutdown_attempted = true;
                    match self.presenter.shutdown() {
                        Ok(()) => {
                            self.shutdown_succeeded = true;
                            self.closed_normally = true;
                            self.set_stage(Stage::Finish);
                        }
                        Err(error) => {
                            self.defect(format!(
                                "normal presenter worker shutdown failed: {error}"
                            ));
                            self.force_shutdown();
                        }
                    }
                } else if self.stage_started.elapsed() > CLOSE_TIMEOUT {
                    self.gap("owned child windows did not retire within the normal close deadline");
                    self.force_shutdown();
                }
            }
            Stage::Finish => {}
        }

        if self.stage == Stage::Finish {
            self.finish_run(ctx);
        } else {
            ctx.request_repaint_after(FRAME_REPAINT);
        }
    }

    fn initialize_root(
        &mut self,
        ctx: &egui::Context,
        frame: &eframe::Frame,
    ) -> Result<(), String> {
        let hwnd = crate::window_manager::get_hwnd(frame)
            .ok_or_else(|| "GAP: eframe did not expose the native root HWND".to_owned())?;
        let process_id = window_process_id(hwnd)?;
        if process_id != self.process_id || !unsafe { IsWindow(hwnd) }.as_bool() {
            return Err("GAP: native root HWND does not belong to the smoke process".into());
        }
        let birth = current_process_birth()?;
        if birth != self.process_birth {
            return Err("GAP: root process lifetime changed during initialization".into());
        }
        let mut origin = POINT::default();
        if !unsafe { ClientToScreen(hwnd, &mut origin) }.as_bool() {
            return Err("GAP: root client screen origin is unavailable".into());
        }
        let client = client_rect(hwnd)?;
        let pixels_per_point = ctx.pixels_per_point().max(f32::EPSILON);
        let geometry = BTreeMap::from([
            (
                self.first,
                (
                    egui::pos2(
                        origin.x as f32 / pixels_per_point + 34.0,
                        origin.y as f32 / pixels_per_point + 55.0,
                    ),
                    egui::vec2(240.0, 420.0),
                ),
            ),
            (
                self.second,
                (
                    egui::pos2(
                        origin.x as f32 / pixels_per_point + 356.0,
                        origin.y as f32 / pixels_per_point + 55.0,
                    ),
                    egui::vec2(240.0, 420.0),
                ),
            ),
        ]);
        self.presenter.configure_smoke_geometry(geometry);
        self.presenter
            .set_root_native_identity((hwnd.0 as usize, birth));
        self.root_hwnd = Some(hwnd);
        self.root_pixels_per_point = pixels_per_point;
        self.window_positions_configured = true;
        self.log.line(format!(
            "phase=root-owned hwnd=0x{:x} pid={} process_birth={} client=[{},{},{},{}] viewport={:?}",
            hwnd.0 as usize,
            process_id,
            birth,
            client.left,
            client.top,
            client.right,
            client.bottom,
            egui::ViewportId::ROOT
        ));
        Ok(())
    }

    fn presentation_ready(&self, first_opacity: u8, second_opacity: u8) -> Result<(), String> {
        if !self.window_positions_configured {
            return Err("GAP: child positions have not been bound to the owned root".into());
        }
        self.current_pin(self.first, first_opacity)?;
        self.current_pin(self.second, second_opacity)?;
        self.current_root_canvas()?;
        Ok(())
    }

    fn current_pin(
        &self,
        id: PinId,
        expected_opacity: u8,
    ) -> Result<
        (
            Arc<PinWindowRuntime>,
            NativeWindowBinding,
            POINT,
            PixelRect,
            PixelRect,
            bool,
        ),
        String,
    > {
        let root_hwnd = self
            .root_hwnd
            .ok_or_else(|| "GAP: exact root HWND is unavailable".to_owned())?;
        let runtime = self
            .presenter
            .registry
            .lock()
            .map_err(|_| "FAIL: pin registry lock was poisoned".to_owned())?
            .pins
            .get(&id)
            .cloned()
            .ok_or_else(|| format!("GAP: generated pin {} is not registered", id.get()))?;
        if runtime.pin_id != id
            || runtime.process_id != self.process_id
            || runtime.viewport_id != pin_viewport_id(id, runtime.lifetime)
            || runtime.viewport_id == egui::ViewportId::ROOT
        {
            return Err(format!(
                "GAP: generated pin {} has an invalid viewport lifetime",
                id.get()
            ));
        }
        let (
            binding,
            desired,
            current,
            pending,
            root_registered,
            closed,
            error,
            image_region,
            click_through,
        ) = {
            let state = runtime
                .state
                .lock()
                .map_err(|_| "FAIL: pin state lock was poisoned".to_owned())?;
            (
                state.native.clone(),
                state.opacity_percent,
                state.applied_opacity.clone(),
                state.opacity_sync_pending,
                state.root_registered,
                state.close_requested || state.native_destroyed,
                state.native_error.clone(),
                state.smoke_image_region,
                state.click_through,
            )
        };
        if !root_registered || closed {
            return Err(format!(
                "GAP: generated pin {} is not an active root-owned child",
                id.get()
            ));
        }
        if let Some(error) = error {
            let kind = if Self::native_failure_is_gap(&error) {
                "GAP"
            } else {
                "FAIL"
            };
            return Err(format!(
                "{kind}: generated pin {} native state: {error}",
                id.get()
            ));
        }
        if desired != expected_opacity || pending {
            return Err(format!(
                "GAP: generated pin {} opacity is not settled at {expected_opacity}%",
                id.get()
            ));
        }
        let binding = binding
            .ok_or_else(|| format!("GAP: generated pin {} has no native binding", id.get()))?;
        if binding.hwnd == 0
            || binding.hwnd == root_hwnd.0 as usize
            || binding.pin_id != id
            || binding.process_id != self.process_id
            || binding.root_hwnd != root_hwnd.0 as usize
            || binding.viewport_id != runtime.viewport_id
            || binding.lifetime != runtime.lifetime
            || binding.title != runtime.title
        {
            return Err(format!(
                "GAP: generated pin {} native binding identity changed",
                id.get()
            ));
        }
        let receipt = current.ok_or_else(|| {
            format!(
                "GAP: generated pin {} has no current opacity acknowledgement",
                id.get()
            )
        })?;
        if receipt.binding != binding || receipt.opacity_percent != expected_opacity {
            return Err(format!(
                "GAP: generated pin {} opacity receipt does not match its binding",
                id.get()
            ));
        }
        let native_api = SystemPinNativeApi;
        match native_api.lookup(
            &runtime.title,
            id,
            runtime.viewport_id,
            runtime.lifetime,
            Some(&binding),
            root_hwnd.0 as usize,
        ) {
            NativeLookup::Found(found) if found == binding => {}
            NativeLookup::Found(_) => {
                return Err(format!(
                    "FAIL: generated pin {} lookup returned a different native binding",
                    id.get()
                ));
            }
            NativeLookup::Missing => {
                return Err(format!(
                    "GAP: generated pin {} exact HWND is not currently registered",
                    id.get()
                ));
            }
            NativeLookup::Destroyed => {
                return Err(format!(
                    "GAP: generated pin {} HWND was destroyed before sampling",
                    id.get()
                ));
            }
            NativeLookup::Unsupported => {
                return Err(format!(
                    "GAP: generated pin {} native lookup is unsupported",
                    id.get()
                ));
            }
            NativeLookup::Failed(error) => {
                let kind = if Self::native_failure_is_gap(&error) {
                    "GAP"
                } else {
                    "FAIL"
                };
                return Err(format!(
                    "{kind}: generated pin {} exact HWND revalidation failed: {error}",
                    id.get()
                ));
            }
        }
        match native_api.is_visible(&binding) {
            Ok(true) => {}
            Ok(false) => {
                return Err(format!(
                    "GAP: generated pin {} is not visibly presented",
                    id.get()
                ));
            }
            Err(error) => {
                let kind = if Self::native_failure_is_gap(&error) {
                    "GAP"
                } else {
                    "FAIL"
                };
                return Err(format!(
                    "{kind}: generated pin {} visibility check failed: {error}",
                    id.get()
                ));
            }
        }
        let bounds = native_api
            .bounds(&binding)
            .ok_or_else(|| format!("GAP: generated pin {} bounds are unavailable", id.get()))?;
        let outer = PixelRect {
            left: bounds.x,
            top: bounds.y,
            right: i32::try_from(bounds.right()).map_err(|_| "GAP: pin right edge overflowed")?,
            bottom: i32::try_from(bounds.bottom())
                .map_err(|_| "GAP: pin bottom edge overflowed")?,
        };
        let region = image_region.ok_or_else(|| {
            format!(
                "GAP: generated pin {} has no rendered image Response",
                id.get()
            )
        })?;
        let visible_image = region.0.intersect(region.1);
        if !visible_image.is_positive()
            || !visible_image.is_finite()
            || !region.2.is_finite()
            || region.2 <= 0.0
        {
            return Err(format!(
                "GAP: generated pin {} image Response is clipped: response={:?} clip={:?} child_client={} child_frame={} pass={}",
                id.get(),
                region.0,
                region.1,
                client_rect(HWND(binding.hwnd as *mut _))
                    .map(|client| format!(
                        "[{},{},{},{}]",
                        client.left, client.top, client.right, client.bottom
                    ))
                    .unwrap_or_else(|error| format!("unavailable:{error}")),
                region.3,
                region.4
            ));
        }
        let client_origin = client_screen_origin(HWND(binding.hwnd as *mut _))?;
        let image_px = logical_rect_to_pixels(visible_image, client_origin, region.2);
        let sample = POINT {
            x: image_px.left + (image_px.right - image_px.left) / 2,
            y: image_px.top + (image_px.bottom - image_px.top) / 2,
        };
        if !outer.contains_rect(image_px) {
            return Err(format!(
                "GAP: generated pin {} rendered image is outside its exact HWND",
                id.get()
            ));
        }
        if !outer.contains(sample) {
            return Err(format!(
                "GAP: generated pin {} image sample lies outside its exact HWND",
                id.get()
            ));
        }
        let child_client = client_rect(HWND(binding.hwnd as *mut _))?;
        if !child_client.contains_rect(image_px) || !child_client.contains(sample) {
            return Err(format!(
                "GAP: generated pin {} image sample lies outside its client area",
                id.get()
            ));
        }
        Ok((runtime, binding, sample, outer, image_px, click_through))
    }

    fn current_root_canvas(&self) -> Result<(HWND, PixelRect, PixelRect, POINT), String> {
        let hwnd = self
            .root_hwnd
            .ok_or_else(|| "GAP: root HWND is unavailable".to_owned())?;
        if unsafe { !IsWindow(hwnd) }.as_bool() || !unsafe { IsWindowVisible(hwnd) }.as_bool() {
            return Err("GAP: owned root window is not visible".into());
        }
        if window_process_id(hwnd)? != self.process_id || self.process_birth == 0 {
            return Err("GAP: owned root HWND/process lifetime changed".into());
        }
        let root_identity_matches = self
            .presenter
            .registry
            .lock()
            .map_err(|_| {
                "FAIL: pin registry lock was poisoned during root identity validation".to_owned()
            })?
            .root_native_identity
            == (hwnd.0 as usize, self.process_birth);
        if !root_identity_matches {
            return Err(
                "FAIL: owned root HWND/process identity no longer matches the presenter".into(),
            );
        }
        let canvas = self
            .canvas_rect
            .filter(|rect| rect.is_finite() && rect.is_positive())
            .ok_or_else(|| "GAP: generated root canvas has no current paint region".to_owned())?;
        let origin = client_screen_origin(hwnd)?;
        let canvas_px = logical_rect_to_pixels(canvas, origin, self.root_pixels_per_point);
        let client = client_rect(hwnd)?;
        if !client.contains_rect(canvas_px) {
            return Err("GAP: generated canvas is not fully inside the owned root client".into());
        }
        let point = POINT {
            x: canvas_px.right - 18,
            y: canvas_px.bottom - 18,
        };
        if !canvas_px.contains(point) {
            return Err("GAP: generated background sample is outside its canvas".into());
        }
        Ok((hwnd, canvas_px, client, point))
    }

    fn verify_composition(
        &mut self,
        first_opacity: u8,
        second_opacity: u8,
        label: &str,
        expected_live_pixels: ExpectedLivePixels,
    ) -> Result<(), String> {
        let expected_second_pixel = match expected_live_pixels {
            ExpectedLivePixels::Initial => self.second_pixels.get_pixel(0, 0).0,
            ExpectedLivePixels::Updated => self.live_frame_pixels.get_pixel(0, 0).0,
        };
        self.presentation_ready(first_opacity, second_opacity)?;
        let (root, canvas, _client, root_point) = self.current_root_canvas()?;
        let (first_runtime, first_binding, first_point, _, _, first_click_through) =
            self.current_pin(self.first, first_opacity)?;
        let (second_runtime, second_binding, second_point, _, _, second_click_through) =
            self.current_pin(self.second, second_opacity)?;
        let first_hwnd = HWND(first_binding.hwnd as *mut _);
        let second_hwnd = HWND(second_binding.hwnd as *mut _);
        let root_sample = SamplePoint {
            point: root_point,
            root,
            visual_pin: None,
            expected_hit: root,
        };
        let first_sample = SamplePoint {
            point: first_point,
            root,
            visual_pin: Some((
                self.first,
                first_opacity,
                first_click_through,
                first_binding.clone(),
            )),
            expected_hit: if first_click_through {
                root
            } else {
                first_hwnd
            },
        };
        let second_sample = SamplePoint {
            point: second_point,
            root,
            visual_pin: Some((
                self.second,
                second_opacity,
                second_click_through,
                second_binding.clone(),
            )),
            expected_hit: if second_click_through {
                root
            } else {
                second_hwnd
            },
        };
        if !canvas.contains(root_sample.point)
            || !canvas.contains(first_sample.point)
            || !canvas.contains(second_sample.point)
        {
            return Err("GAP: one generated composition sample is outside the owned canvas".into());
        }
        let background = self.read_owned_sample(&root_sample)?;
        let first_actual = self.read_owned_sample(&first_sample)?;
        let second_actual = self.read_owned_sample(&second_sample)?;
        let first_expected = blend(FIRST_COLOR, ROOT_COLOR, first_opacity);
        let second_expected = blend(expected_second_pixel, ROOT_COLOR, second_opacity);
        self.samples.push(serde_json::json!({
            "phase": label,
            "root": {
                    "hwnd": root.0 as usize,
                "sample": { "x": root_sample.point.x, "y": root_sample.point.y },
                    "visual_stack": [root.0 as usize],
                    "expected_hit_hwnd": root.0 as usize,
                "actual_rgb": background,
                "expected_rgb": [ROOT_COLOR[0], ROOT_COLOR[1], ROOT_COLOR[2]]
            },
            "pins": [
                {
                    "id": self.first.get(),
                    "hwnd": first_binding.hwnd,
                    "viewport": format!("{:?}", first_runtime.viewport_id),
                    "lifetime": first_runtime.lifetime,
                    "sample": { "x": first_sample.point.x, "y": first_sample.point.y },
                    "visual_stack": [first_binding.hwnd, root.0 as usize],
                    "expected_hit_hwnd": first_sample.expected_hit.0 as usize,
                    "opacity_percent": first_opacity,
                    "actual_rgb": first_actual,
                    "expected_rgb": first_expected
                },
                {
                    "id": self.second.get(),
                    "hwnd": second_binding.hwnd,
                    "viewport": format!("{:?}", second_runtime.viewport_id),
                    "lifetime": second_runtime.lifetime,
                    "sample": { "x": second_sample.point.x, "y": second_sample.point.y },
                    "visual_stack": [second_binding.hwnd, root.0 as usize],
                    "expected_hit_hwnd": second_sample.expected_hit.0 as usize,
                    "opacity_percent": second_opacity,
                    "actual_rgb": second_actual,
                    "expected_rgb": second_expected
                }
            ]
        }));
        let expected_background = [ROOT_COLOR[0], ROOT_COLOR[1], ROOT_COLOR[2]];
        if !close_color(background, expected_background, 2) {
            return Err(format!(
                "{label}: root canvas sample differs from generated opaque background"
            ));
        }
        if !close_color(first_actual, first_expected, SAMPLE_TOLERANCE) {
            return Err(format!(
                "{label}: first pin pixel did not match {first_opacity}% composition"
            ));
        }
        if !close_color(second_actual, second_expected, SAMPLE_TOLERANCE) {
            return Err(format!(
                "{label}: second pin pixel did not match independent {second_opacity}% composition"
            ));
        }
        let original_a = self
            .board
            .get(self.first)
            .and_then(Pin::snapshot)
            .map(PinSnapshot::pixels);
        let current_b = self
            .board
            .get(self.second)
            .and_then(Pin::live_frame)
            .map(|frame| frame.image);
        let second_image_matches =
            current_b
                .as_ref()
                .is_some_and(|image| match expected_live_pixels {
                    ExpectedLivePixels::Initial => image.pixels() == &self.second_pixels,
                    ExpectedLivePixels::Updated => image.pixels() == &self.live_frame_pixels,
                });
        if original_a != Some(&self.first_pixels) || !second_image_matches {
            return Err("opacity changed a generated pin's original RGBA pixels".into());
        }
        let _ = (first_runtime, second_runtime);
        Ok(())
    }

    fn read_owned_sample(&self, sample: &SamplePoint) -> Result<[u8; 3], String> {
        let point = sample.point;
        unsafe { DwmFlush() }.map_err(|error| format!("GAP: compositor fence failed: {error}"))?;

        // DwmFlush precedes the final owner, geometry, stack and hit-test fence.
        // No desktop pixel is read until the current generated regions still match.
        let (root, canvas, client, _) = self.current_root_canvas()?;
        if root != sample.root || !canvas.contains(point) || !client.contains(point) {
            return Err("GAP: sample point left the current owned root canvas/client".into());
        }
        let visual_pin = if let Some((pin_id, opacity, expected_click_through, expected_binding)) =
            sample.visual_pin.as_ref()
        {
            let (_, binding, _, outer, image, click_through) =
                self.current_pin(*pin_id, *opacity)?;
            let hwnd = HWND(binding.hwnd as *mut _);
            if &binding != expected_binding
                || binding.root_hwnd != root.0 as usize
                || click_through != *expected_click_through
                || !outer.contains_rect(image)
                || !image.contains(point)
            {
                return Err(
                    "GAP: exact pin identity, input style, or rendered image region changed".into(),
                );
            }
            Some(hwnd)
        } else {
            None
        };
        if visual_pin.is_none() && sample.expected_hit != root {
            return Err("GAP: root background sample has a non-root expected hit owner".into());
        }
        let expected_hit = match (visual_pin, sample.visual_pin.as_ref()) {
            (Some(_), Some((_, _, true, _))) if sample.expected_hit == root => root,
            (Some(pin), Some((_, _, false, _))) if sample.expected_hit == pin => pin,
            (None, None) if sample.expected_hit == root => root,
            _ => return Err("GAP: visual target and expected input hit owner disagree".into()),
        };
        if unsafe { !IsWindow(root) }.as_bool()
            || !unsafe { IsWindowVisible(root) }.as_bool()
            || unsafe { !IsWindow(expected_hit) }.as_bool()
            || !unsafe { IsWindowVisible(expected_hit) }.as_bool()
        {
            return Err("GAP: exact sample root or hit target is not visible".into());
        }

        let mut scan = ZOrderScan {
            point,
            root: root.0 as usize,
            visual_pin: visual_pin.map(|pin| pin.0 as usize),
            pin_seen: false,
            complete: false,
            unknown_bounds: None,
            obstruction: None,
        };
        let enumeration = unsafe {
            EnumWindows(
                Some(inspect_z_order),
                LPARAM((&mut scan as *mut ZOrderScan) as isize),
            )
        };
        if let Some(hwnd) = scan.unknown_bounds {
            return Err(format!(
                "GAP: visible HWND 0x{hwnd:x} had unknown bounds before the owned root in the sample stack"
            ));
        }
        if let Some(obstruction) = scan.obstruction {
            return Err(format!(
                "GAP: native visual stack contains unexpected HWND 0x{obstruction:x} before the owned root"
            ));
        }
        // EnumWindows reports ERROR_CANCELLED when our callback returns FALSE
        // after the exact expected root is reached. That is a successful bounded
        // scan, unlike an error before the completion flag is set.
        if !scan.complete {
            if let Err(error) = enumeration {
                return Err(format!(
                    "GAP: could not enumerate native sample z-order: {error}"
                ));
            }
            return Err("GAP: native sample stack did not reach the exact owned root".into());
        }
        let hit = unsafe { WindowFromPoint(point) };
        if hit != expected_hit {
            return Err(format!(
                "GAP: sample hit target 0x{:x} differs from expected 0x{:x}",
                hit.0 as usize, expected_hit.0 as usize
            ));
        }
        let desktop = unsafe { GetDC(HWND::default()) };
        if desktop.0.is_null() {
            return Err("GAP: desktop pixel sampler is unavailable".into());
        }
        let color = unsafe { GetPixel(desktop, point.x, point.y) };
        unsafe { ReleaseDC(HWND::default(), desktop) };
        if color == COLORREF(u32::MAX) {
            return Err("GAP: owned generated-region pixel could not be sampled".into());
        }
        Ok([color.0 as u8, (color.0 >> 8) as u8, (color.0 >> 16) as u8])
    }

    fn apply_control(
        &mut self,
        ctx: &egui::Context,
        id: PinId,
        change: PinControlChange,
    ) -> Result<(), String> {
        let root_hwnd = self
            .root_hwnd
            .ok_or_else(|| "GAP: root HWND is unavailable".to_owned())?;
        let runtime = self
            .presenter
            .registry
            .lock()
            .ok()
            .and_then(|registry| registry.pins.get(&id).cloned())
            .ok_or_else(|| "GAP: exact pin runtime is unavailable".to_owned())?;
        apply_pin_control_change(
            ctx,
            &runtime,
            &SystemPinNativeApi,
            root_hwnd.0 as usize,
            change,
        )
    }

    fn click_through_enabled(&self, id: PinId) -> Result<bool, String> {
        let registry = self.presenter.registry.lock().map_err(|_| {
            "FAIL: pin registry lock was poisoned during click-through readiness".to_owned()
        })?;
        let runtime = registry.pins.get(&id).ok_or_else(|| {
            format!(
                "GAP: generated pin {} disappeared before click-through readiness",
                id.get()
            )
        })?;
        let state = runtime.state.lock().map_err(|_| {
            "FAIL: pin state lock was poisoned during click-through readiness".to_owned()
        })?;
        Ok(state.click_through)
    }

    fn inject_opacity_reset(
        &self,
        _ctx: &egui::Context,
        id: PinId,
        expected_opacity: u8,
    ) -> Result<(), (bool, String)> {
        let root = self
            .root_hwnd
            .ok_or_else(|| (true, "exact root HWND is unavailable".into()))?;
        self.current_root_canvas().map_err(|error| (true, error))?;
        let (runtime, binding, _, _, _, click_through) = self
            .current_pin(id, expected_opacity)
            .map_err(|error| (true, error))?;
        if click_through {
            return Err((
                true,
                "generated Reset pin is click-through; no input sent".into(),
            ));
        }
        let child = HWND(binding.hwnd as *mut _);
        let response = Self::current_opacity_reset_response(&runtime, &binding)
            .map_err(|error| (true, error))?;
        let (response_id, response_rect, clip_rect, pixels_per_point, enabled, pass_index) =
            response;
        if !enabled
            || !response_rect.is_finite()
            || !response_rect.is_positive()
            || !clip_rect.contains_rect(response_rect)
            || !pixels_per_point.is_finite()
            || pixels_per_point <= 0.0
        {
            return Err((
                true,
                "the actual Reset response is disabled, stale, or clipped; no input sent".into(),
            ));
        }

        let initial_foreground = unsafe { GetForegroundWindow() };
        if initial_foreground != root && initial_foreground != child {
            return Err((
                true,
                "neither the exact root nor its generated pin is foreground; no input sent".into(),
            ));
        }
        if initial_foreground == root {
            let _ = unsafe { SetForegroundWindow(child) };
            let deadline = Instant::now() + Duration::from_secs(1);
            while unsafe { GetForegroundWindow() } != child {
                if unsafe { GetForegroundWindow() } != root {
                    return Err((
                        true,
                        "foreground left the owned root while activating the exact pin; no input sent"
                            .into(),
                    ));
                }
                if Instant::now() >= deadline {
                    return Err((
                        true,
                        "the exact generated pin could not become foreground; no input sent".into(),
                    ));
                }
                thread::yield_now();
            }
        }

        let origin = client_screen_origin(child).map_err(|error| (true, error))?;
        let button = logical_rect_to_pixels(response_rect, origin, pixels_per_point);
        let client = client_rect(child).map_err(|error| (true, error))?;
        let mut native_bounds = RECT::default();
        unsafe { GetWindowRect(child, &mut native_bounds) }
            .map_err(|error| (true, format!("exact pin bounds are unavailable: {error}")))?;
        let outer = PixelRect {
            left: native_bounds.left,
            top: native_bounds.top,
            right: native_bounds.right,
            bottom: native_bounds.bottom,
        };
        if button.right <= button.left
            || button.bottom <= button.top
            || !client.contains_rect(button)
            || !outer.contains_rect(button)
        {
            return Err((
                true,
                "the actual Reset response is outside the exact pin client; no input sent".into(),
            ));
        }
        let point = POINT {
            x: button.left + (button.right - button.left) / 2,
            y: button.top + (button.bottom - button.top) / 2,
        };
        if !button.contains(point) || unsafe { WindowFromPoint(point) } != child {
            return Err((
                true,
                "the actual Reset response is not the current native hit target; no input sent"
                    .into(),
            ));
        }
        let idle = input_idle_age()
            .map_err(|error| (true, error))?
            .ok_or_else(|| {
                (
                    true,
                    "input idle state is unavailable; no input sent".into(),
                )
            })?;
        if idle < MIN_INPUT_IDLE {
            return Err((
                true,
                format!("input idle guard refused Reset at {}ms", idle.as_millis()),
            ));
        }
        if let Some(held) = first_held_virtual_key() {
            return Err((
                true,
                format!("virtual key/button 0x{held:02x} is held; no input sent"),
            ));
        }

        let move_event = absolute_mouse_move(point).map_err(|error| (true, error))?;
        let (again, again_binding, _, _, _, click_through) = self
            .current_pin(id, expected_opacity)
            .map_err(|error| (true, error))?;
        let current_response = Self::current_opacity_reset_response(&again, &again_binding)
            .map_err(|error| (true, error))?;
        let current_root = self.current_root_canvas().map_err(|error| (true, error))?.0;
        let mut current_bounds = RECT::default();
        let current_bounds_valid = unsafe { GetWindowRect(child, &mut current_bounds) }.is_ok()
            && (PixelRect {
                left: current_bounds.left,
                top: current_bounds.top,
                right: current_bounds.right,
                bottom: current_bounds.bottom,
            })
            .contains_rect(button);
        let final_idle = input_idle_age()
            .map_err(|error| (true, error))?
            .ok_or_else(|| {
                (
                    true,
                    "input idle state became unavailable; no input sent".into(),
                )
            })?;
        if final_idle < MIN_INPUT_IDLE
            || first_held_virtual_key().is_some()
            || unsafe { GetForegroundWindow() } != child
            || current_root != root
            || again_binding != binding
            || click_through
            || current_response != response
            || unsafe { !IsWindow(child) }.as_bool()
            || !unsafe { IsWindowVisible(child) }.as_bool()
            || unsafe { WindowFromPoint(point) } != child
            || !client_rect(child)
                .is_ok_and(|rect| rect.contains_rect(button) && rect.contains(point))
            || !current_bounds_valid
        {
            return Err((
                true,
                "pin identity, current Reset response, foreground, or hit target changed at the final input fence; no input sent"
                    .into(),
            ));
        }
        let events = [move_event, left_button(true), left_button(false)];
        self.log.line(format!(
            "phase=reset-100-input preflight=passed pin={} hwnd=0x{:x} lifetime={} response={response_id:?} pass={pass_index} point={},{} idle_ms={} held_keys=none",
            id.get(), binding.hwnd, binding.lifetime, point.x, point.y, final_idle.as_millis()
        ));
        let inserted = unsafe { SendInput(&events, std::mem::size_of::<INPUT>() as i32) } as usize;
        if inserted != events.len() {
            if inserted >= 2 {
                let release = [left_button(false)];
                let _ = unsafe { SendInput(&release, std::mem::size_of::<INPUT>() as i32) };
            }
            return Err((
                true,
                format!("SendInput inserted {inserted}/3 events for the guarded Reset click"),
            ));
        }
        let _ = unsafe { SetForegroundWindow(root) };
        let restore_deadline = Instant::now() + Duration::from_secs(1);
        while unsafe { GetForegroundWindow() } != root {
            if unsafe { GetForegroundWindow() } != child {
                return Err((
                    true,
                    "foreground left the owned pin before the root could be restored".into(),
                ));
            }
            if Instant::now() >= restore_deadline {
                return Err((
                    true,
                    "the exact generated root could not regain foreground after Reset".into(),
                ));
            }
            thread::yield_now();
        }
        Ok(())
    }

    fn current_opacity_reset_response(
        runtime: &PinWindowRuntime,
        binding: &NativeWindowBinding,
    ) -> Result<(egui::Id, egui::Rect, egui::Rect, f32, bool, usize), String> {
        let state = runtime
            .state
            .lock()
            .map_err(|_| "pin state lock was poisoned during Reset validation".to_owned())?;
        if state.native.as_ref() != Some(binding)
            || state.close_requested
            || state.native_destroyed
            || !state.root_registered
        {
            return Err("the exact pin identity is not an active root-owned child".into());
        }
        state
            .smoke_opacity_reset_region
            .ok_or_else(|| "the current Reset response has not been rendered".into())
    }

    fn click_through_input_idle_age(&self) -> Result<Duration, (bool, String)> {
        let root = self
            .root_hwnd
            .ok_or_else(|| (true, "root HWND is unavailable".into()))?;
        if unsafe { GetForegroundWindow() } != root {
            return Err((
                true,
                "the exact generated root is not foreground while waiting for idle input; no input sent"
                    .into(),
            ));
        }
        let (current_root, canvas, client, _) =
            self.current_root_canvas().map_err(|error| (true, error))?;
        if current_root != root {
            return Err((true, "the exact root canvas changed; no input sent".into()));
        }
        let (_, binding, point, _, _, click_through) = self
            .current_pin(self.first, 50)
            .map_err(|error| (true, error))?;
        if !canvas.contains(point)
            || !client.contains(point)
            || !click_through
            || unsafe { WindowFromPoint(point) } != root
        {
            return Err((
                true,
                "the exact click-through target is no longer owned and hit-testable; no input sent"
                    .into(),
            ));
        }
        if let Some(held) = first_held_virtual_key() {
            return Err((
                true,
                format!("virtual key/button 0x{held:02x} is held; no input sent"),
            ));
        }
        input_idle_age()
            .map_err(|error| (true, error))?
            .ok_or_else(|| {
                (
                    true,
                    format!(
                        "input idle state is unavailable for exact pin {} (HWND 0x{:x}); no input sent",
                        self.first.get(),
                        binding.hwnd
                    ),
                )
            })
    }

    fn inject_click_through(&self, _ctx: &egui::Context) -> Result<usize, (bool, String)> {
        let root = self
            .root_hwnd
            .ok_or_else(|| (true, "root HWND is unavailable".into()))?;
        if unsafe { GetForegroundWindow() } != root {
            return Err((
                true,
                "the exact generated root is not foreground; no input sent".into(),
            ));
        }
        let (current_root, canvas, client, _) =
            self.current_root_canvas().map_err(|error| (true, error))?;
        if current_root != root {
            return Err((true, "the exact root canvas changed; no input sent".into()));
        }
        let (_, binding, point, _, _, click_through) = self
            .current_pin(self.first, 50)
            .map_err(|error| (true, error))?;
        if !canvas.contains(point) || !client.contains(point) {
            return Err((
                true,
                "click-through target is outside the owned generated canvas; no input sent".into(),
            ));
        }
        if !click_through {
            return Err((
                true,
                "the exact first pin is not click-through; no input sent".into(),
            ));
        }
        if unsafe { WindowFromPoint(point) } != root {
            return Err((
                true,
                "click-through hit testing does not resolve to the exact root; no input sent"
                    .into(),
            ));
        }
        let _ = self
            .read_owned_sample(&SamplePoint {
                point,
                root,
                visual_pin: Some((self.first, 50, true, binding.clone())),
                expected_hit: root,
            })
            .map_err(|error| {
                (
                    true,
                    format!("click-through point ownership failed: {error}"),
                )
            })?;
        if let Some(age) = input_idle_age().map_err(|error| (true, error))? {
            if age < MIN_INPUT_IDLE {
                return Err((
                    true,
                    format!(
                        "input idle guard refused interaction at {}ms",
                        age.as_millis()
                    ),
                ));
            }
        } else {
            return Err((
                true,
                "input idle state is unavailable; no input sent".into(),
            ));
        }
        if let Some(held) = first_held_virtual_key() {
            return Err((
                true,
                format!("virtual key/button 0x{held:02x} is held; no input sent"),
            ));
        }
        let current_geometry = self.current_root_canvas();
        if unsafe { GetForegroundWindow() } != root
            || !current_geometry.is_ok_and(|(current_root, current_canvas, current_client, _)| {
                current_root == root
                    && current_canvas.contains(point)
                    && current_client.contains(point)
            })
            || !self.current_pin(self.first, 50).is_ok_and(
                |(_, current, _, _, _, click_through)| current == binding && click_through,
            )
            || unsafe { WindowFromPoint(point) } != root
        {
            return Err((
                true,
                "root or pin ownership changed at the final input fence; no input sent".into(),
            ));
        }
        let event = absolute_mouse_move(point).map_err(|error| (true, error))?;
        let final_idle = input_idle_age()
            .map_err(|error| (true, error))?
            .ok_or_else(|| {
                (
                    true,
                    "input idle state became unavailable; no input sent".into(),
                )
            })?;
        if final_idle < MIN_INPUT_IDLE {
            return Err((
                true,
                "input became active before the final click fence; no input sent".into(),
            ));
        }
        if let Some(held) = first_held_virtual_key() {
            return Err((
                true,
                format!("virtual key/button 0x{held:02x} became held; no input sent"),
            ));
        }
        let final_geometry = self.current_root_canvas().map_err(|error| (true, error))?;
        if unsafe { GetForegroundWindow() } != root
            || final_geometry.0 != root
            || !final_geometry.1.contains(point)
            || !final_geometry.2.contains(point)
            || !self.current_pin(self.first, 50).is_ok_and(
                |(_, current, _, _, _, click_through)| current == binding && click_through,
            )
            || unsafe { WindowFromPoint(point) } != root
        {
            return Err((
                true,
                "owned root, pin, or hit target changed at the final input fence; no input sent"
                    .into(),
            ));
        }
        let before = self.click_count;
        self.log.line(format!(
            "phase=click-through-input preflight=passed root=0x{:x} pin=0x{:x} point={},{} idle_ms={} held_keys=none",
            root.0 as usize,
            binding.hwnd,
            point.x,
            point.y,
            final_idle.as_millis()
        ));
        let events = [event, left_button(true), left_button(false)];
        let inserted = unsafe { SendInput(&events, std::mem::size_of::<INPUT>() as i32) } as usize;
        if inserted != events.len() {
            if inserted >= 2 {
                let release = [left_button(false)];
                let _ = unsafe { SendInput(&release, std::mem::size_of::<INPUT>() as i32) };
            }
            return Err((
                true,
                format!("SendInput inserted {inserted}/3 events for the guarded click"),
            ));
        }
        Ok(before)
    }

    fn start_action(&self, id: PinId, kind: PinActionKind) -> Result<u64, String> {
        let runtime = self
            .presenter
            .registry
            .lock()
            .ok()
            .and_then(|registry| registry.pins.get(&id).cloned())
            .ok_or_else(|| "exact pin runtime is unavailable for image action".to_owned())?;
        let request_id = {
            let mut state = runtime
                .state
                .lock()
                .map_err(|_| "pin action state lock was poisoned".to_owned())?;
            if state.close_requested || state.native_destroyed || state.pending_request_id.is_some()
            {
                return Err("pin is not available for an image action".into());
            }
            let request_id = state.next_request_id;
            state.next_request_id = state
                .next_request_id
                .checked_add(1)
                .ok_or_else(|| "pin action request identity was exhausted".to_owned())?;
            state.pending_request_id = Some(request_id);
            state.action_status = PinActionStatus::Pending(match kind {
                PinActionKind::Copy => "Copying generated image…".into(),
                PinActionKind::SaveAs => "Saving generated image…".into(),
            });
            request_id
        };
        submit_pin_action(&runtime, &self.presenter.actions.client(), request_id, kind);
        Ok(request_id)
    }

    fn action_finished(&self, id: PinId, request: Option<u64>) -> Result<bool, String> {
        let runtime = self
            .presenter
            .registry
            .lock()
            .ok()
            .and_then(|registry| registry.pins.get(&id).cloned())
            .ok_or_else(|| "pin disappeared before the image action completed".to_owned())?;
        let state = runtime
            .state
            .lock()
            .map_err(|_| "pin action state lock was poisoned".to_owned())?;
        if state.pending_request_id.is_some() || state.next_request_id.checked_sub(1) != request {
            return Ok(false);
        }
        match &state.action_status {
            PinActionStatus::Finished(_) => Ok(true),
            PinActionStatus::Failed(error) => {
                Err(format!("generated image action failed: {error}"))
            }
            PinActionStatus::Idle | PinActionStatus::Pending(_) => Ok(false),
        }
    }

    fn all_pins_have_visibility(&self, visible: bool) -> Result<bool, String> {
        let runtimes = {
            let registry = self.presenter.registry.lock().map_err(|_| {
                "FAIL: pin registry lock was poisoned during visibility verification".to_owned()
            })?;
            [self.first, self.second]
                .into_iter()
                .map(|id| {
                    registry
                        .pins
                        .get(&id)
                        .cloned()
                        .ok_or_else(|| {
                            format!(
                                "GAP: generated pin {} is not registered during visibility verification",
                                id.get()
                            )
                        })
                })
                .collect::<Result<Vec<_>, _>>()?
        };
        for runtime in runtimes {
            let id = runtime.pin_id;
            let binding = runtime.state.lock()
                .map_err(|_| "FAIL: pin state lock was poisoned during visibility verification".to_owned())?
                .native.clone()
                .ok_or_else(|| format!("GAP: generated pin {} has no native binding during visibility verification", id.get()))?;
            let actual = SystemPinNativeApi.is_visible(&binding).map_err(|error| {
                let kind = if Self::native_failure_is_gap(&error) {
                    "GAP"
                } else {
                    "FAIL"
                };
                format!(
                    "{kind}: generated pin {} visibility read failed: {error}",
                    id.get()
                )
            })?;
            if actual != visible {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn native_control_error(&mut self, error: String) {
        if Self::native_failure_is_gap(&error) {
            self.gap(format!("native pin control could not be verified: {error}"));
        } else {
            self.defect(format!("native pin control failed: {error}"));
        }
    }

    fn force_shutdown(&mut self) {
        if !self.shutdown_succeeded {
            self.shutdown_attempted = true;
            match self.presenter.shutdown() {
                Ok(()) => {
                    self.shutdown_succeeded = true;
                    self.log.line("cleanup=forced presenter shutdown completed");
                }
                Err(error) => {
                    self.log.line(format!("cleanup=forced error={error}"));
                }
            }
        }
        self.closed_normally = false;
        self.set_stage(Stage::Finish);
    }

    fn finish_run(&mut self, ctx: &egui::Context) {
        if self.receipt_written {
            return;
        }
        self.receipt_written = true;
        if !self.closed_normally && self.fatal.is_none() {
            self.fatal = Some("FAIL: presenter did not complete normal child retirement".into());
        }
        let error = self.fatal.take();
        let status = match error.as_deref() {
            Some(error) if error.starts_with("GAP:") => "GAP",
            Some(_) => "FAIL",
            None => "PASS",
        };
        let result = error.map_or(Ok(()), Err);
        let receipt = serde_json::json!({
            "result": status,
            "reason": result.as_ref().err(),
            "nonce": format!("{:032x}", self.nonce),
            "pid": self.process_id,
            "process_birth": self.process_birth,
            "backend": "eframe-glow",
            "source": "generated-static-plus-live-budget-session",
            "os_source_capture": "unverified",
            "root_hwnd": self.root_hwnd.map(|hwnd| hwnd.0 as usize),
            "pin_ids": [self.first.get(), self.second.get()],
            "samples": std::mem::take(&mut self.samples),
            "normal_close": self.closed_normally,
            "shutdown_attempted": self.shutdown_attempted,
            "worker_joined": self.shutdown_succeeded
        });
        self.log.line(receipt.to_string());
        if let Ok(mut output) = self.result.lock() {
            *output = Some(result);
        }
        ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Close);
    }
}

impl eframe::App for SmokeApp {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if self.receipt_written {
            ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Close);
            return;
        }
        if self.root_hwnd.is_none() {
            match self.initialize_root(ctx, frame) {
                Ok(()) => {}
                Err(error) => self.gap(error),
            }
        }
        if let Some(root) = self.root_hwnd {
            if crate::window_manager::get_hwnd(frame) != Some(root) {
                self.gap("the eframe root HWND changed during the smoke run");
            }
            self.presenter
                .set_root_native_identity((root.0 as usize, self.process_birth));
            self.presenter.poll(ctx, &mut self.board);
        }
        self.root_pixels_per_point = ctx.pixels_per_point().max(f32::EPSILON);
        self.advance(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.receipt_written {
            ui.ctx()
                .send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Close);
            return;
        }
        let ctx = ui.ctx().clone();
        let response = egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                let canvas = ui.max_rect();
                ui.painter().rect_filled(
                    canvas,
                    0.0,
                    egui::Color32::from_rgba_unmultiplied(
                        ROOT_COLOR[0],
                        ROOT_COLOR[1],
                        ROOT_COLOR[2],
                        ROOT_COLOR[3],
                    ),
                );
                let click = ui.interact(
                    canvas,
                    egui::Id::new(("pin-opacity-generated-root-canvas", self.nonce)),
                    egui::Sense::click(),
                );
                (canvas, click.clicked())
            });
        self.canvas_rect = Some(response.inner.0);
        if response.inner.1 {
            self.click_count = self.click_count.saturating_add(1);
        }
        if self.root_hwnd.is_some() {
            self.presenter.show(&ctx, &self.board);
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.stage != Stage::Finish {
            self.log
                .line("result=GAP root_closed_before_bounded_phases_finished");
            if let Ok(mut result) = self.result.lock() {
                *result = Some(Err(
                    "GAP: smoke root closed before bounded phases finished".into()
                ));
            }
        }
        if !self.shutdown_attempted {
            self.shutdown_attempted = true;
            match self.presenter.shutdown() {
                Ok(()) => {
                    self.shutdown_succeeded = true;
                    self.log
                        .line("cleanup=on-exit presenter shutdown completed");
                }
                Err(error) => self.log.line(format!("cleanup=on-exit error={error}")),
            }
        }
        let _ = self.board.cancel_capture(self.visibility_identity);
    }
}

fn insert_generated_pin(
    board: &mut Pinboard,
    operation_id: u64,
    image: RgbaImage,
) -> Result<PinId, String> {
    let identity = board
        .reserve_capture(
            crate::pinboard::PinCaptureOperationId::new(operation_id)
                .ok_or_else(|| "generated pin operation identity was invalid".to_owned())?,
        )
        .map_err(|error| format!("reserve generated pin: {error}"))?;
    let (width, height) = image.dimensions();
    let rect = crate::mkmacro::ScreenRect::new(1, 1, width, height);
    board
        .reserve_selected_rect(identity, rect)
        .map_err(|error| error.to_string())?;
    board
        .stage_capture(identity, rect, image)
        .map_err(|error| error.to_string())?;
    board
        .commit_capture(identity)
        .map_err(|error| error.to_string())
}

fn insert_generated_live_pin(
    board: &mut Pinboard,
    operation_id: u64,
    initial_image: RgbaImage,
) -> Result<(PinId, SyncSender<RgbaImage>), String> {
    let operation_id = crate::pinboard::PinCaptureOperationId::new(operation_id)
        .ok_or_else(|| "generated live-pin operation identity was invalid".to_owned())?;
    let capture = board
        .reserve_live_capture(operation_id)
        .map_err(|error| format!("reserve generated live pin: {error}"))?;
    let (width, height) = initial_image.dimensions();
    let rect = crate::mkmacro::ScreenRect::new(0, 0, width, height);
    board
        .reserve_selected_rect(capture, rect)
        .map_err(|error| error.to_string())?;
    let binding = PinSourceBinding {
        // This feature-only source is explicitly synthetic. It has no HWND
        // and is never sent to the Windows Graphics Capture backend.
        window: PinSourceWindowIdentity {
            hwnd: 0,
            process_id: 0,
            thread_id: 0,
        },
        captured_bounds: rect,
        crop: PinSourceCrop {
            x: 0,
            y: 0,
            width,
            height,
        },
    };
    let reservation = board
        .take_live_reservation(capture)
        .map_err(|error| error.to_string())?;
    let (mut session, frame_sender) = match PinLiveCaptureSession::spawn_generated_smoke(
        binding,
        capture.generation,
        reservation,
        board.gpu_budget(),
        initial_image,
    ) {
        Ok(session) => session,
        Err(error) => {
            let _ = board.cancel_capture(capture);
            return Err(error);
        }
    };
    let first_frame_deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(result) = session.try_first_frame_result() {
            match result {
                Ok(()) => break,
                Err(error) => {
                    board.retire_live_session(session);
                    let _ = board.cancel_capture(capture);
                    return Err(error);
                }
            }
        }
        if Instant::now() >= first_frame_deadline {
            board.retire_live_session(session);
            let _ = board.cancel_capture(capture);
            return Err("GAP: generated live session did not publish its first frame".into());
        }
        thread::yield_now();
    }
    if let Err(error) = board.stage_live_capture(capture, binding, session) {
        let _ = board.cancel_capture(capture);
        return Err(error.to_string());
    }
    let pin_id = match board.commit_capture(capture) {
        Ok(pin_id) => pin_id,
        Err(error) => {
            let _ = board.cancel_capture(capture);
            return Err(error.to_string());
        }
    };
    Ok((pin_id, frame_sender))
}

fn current_process_birth() -> Result<u64, String> {
    let mut created = FILETIME::default();
    let mut exited = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        )
    }
    .map_err(|error| format!("GAP: current process creation identity unavailable: {error}"))?;
    Ok((u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
}

fn window_process_id(hwnd: HWND) -> Result<u32, String> {
    let mut process_id = 0;
    let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
    if thread_id == 0 || process_id == 0 {
        Err("GAP: native HWND process identity is unavailable".into())
    } else {
        Ok(process_id)
    }
}

fn client_rect(hwnd: HWND) -> Result<PixelRect, String> {
    let mut rect = RECT::default();
    unsafe { GetClientRect(hwnd, &mut rect) }
        .map_err(|error| format!("GAP: native client bounds are unavailable: {error}"))?;
    let mut origin = POINT {
        x: rect.left,
        y: rect.top,
    };
    if !unsafe { ClientToScreen(hwnd, &mut origin) }.as_bool() {
        return Err("GAP: native client origin is unavailable".into());
    }
    Ok(PixelRect {
        left: origin.x,
        top: origin.y,
        right: origin.x + rect.right - rect.left,
        bottom: origin.y + rect.bottom - rect.top,
    })
}

fn client_screen_origin(hwnd: HWND) -> Result<POINT, String> {
    let mut origin = POINT::default();
    if unsafe { ClientToScreen(hwnd, &mut origin) }.as_bool() {
        Ok(origin)
    } else {
        Err("GAP: client-to-screen conversion failed".into())
    }
}

fn logical_rect_to_pixels(rect: egui::Rect, origin: POINT, pixels_per_point: f32) -> PixelRect {
    PixelRect {
        left: origin.x + (rect.left() * pixels_per_point).round() as i32,
        top: origin.y + (rect.top() * pixels_per_point).round() as i32,
        right: origin.x + (rect.right() * pixels_per_point).round() as i32,
        bottom: origin.y + (rect.bottom() * pixels_per_point).round() as i32,
    }
}

fn blend(foreground: [u8; 4], background: [u8; 4], opacity_percent: u8) -> [u8; 3] {
    let alpha = ((u16::from(opacity_percent) * 255 + 50) / 100) as u32;
    let blend_channel = |foreground: u8, background: u8| {
        ((u32::from(foreground) * alpha + u32::from(background) * (255 - alpha) + 127) / 255) as u8
    };
    [
        blend_channel(foreground[0], background[0]),
        blend_channel(foreground[1], background[1]),
        blend_channel(foreground[2], background[2]),
    ]
}

fn close_color(actual: [u8; 3], expected: [u8; 3], tolerance: u8) -> bool {
    actual
        .iter()
        .zip(expected)
        .all(|(actual, expected)| actual.abs_diff(expected) <= tolerance)
}

fn input_idle_age() -> Result<Option<Duration>, String> {
    let mut info = LASTINPUTINFO {
        cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    if !unsafe { GetLastInputInfo(&mut info) }.as_bool() {
        return Ok(None);
    }
    let tick = unsafe { GetTickCount64() } as u32;
    Ok(Some(Duration::from_millis(u64::from(
        tick.wrapping_sub(info.dwTime),
    ))))
}

fn first_held_virtual_key() -> Option<u8> {
    (1_u16..=255).find_map(|key| {
        (unsafe { GetAsyncKeyState(i32::from(key)) } & i16::MIN != 0).then_some(key as u8)
    })
}

fn absolute_mouse_move(point: POINT) -> Result<INPUT, String> {
    let left = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    let top = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    let width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
    let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };
    if width <= 1
        || height <= 1
        || point.x < left
        || point.y < top
        || point.x >= left + width
        || point.y >= top + height
    {
        return Err("GAP: generated click point is outside the virtual desktop".into());
    }
    let map = |value: i32, origin: i32, extent: i32| {
        (((i64::from(value - origin) * 65_535) + i64::from(extent - 1) / 2) / i64::from(extent - 1))
            as i32
    };
    Ok(INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: map(point.x, left, width),
                dy: map(point.y, top, height),
                mouseData: 0,
                dwFlags: MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    })
}

fn left_button(down: bool) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: 0,
                dwFlags: if down {
                    MOUSEEVENTF_LEFTDOWN
                } else {
                    MOUSEEVENTF_LEFTUP
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

pub(super) fn run() -> Result<(), String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("could not create smoke nonce: {error}"))?
        .as_nanos();
    let nonce = now ^ u128::from(unsafe { GetCurrentProcessId() });
    let output_dir = std::env::current_dir()
        .map_err(|error| format!("could not resolve smoke output directory: {error}"))?
        .join("target")
        .join("pin-opacity-smoke")
        .join(format!("{nonce:032x}"));
    fs::create_dir_all(&output_dir)
        .map_err(|error| format!("could not create owned smoke output directory: {error}"))?;
    let log_path = output_dir.join("pin-opacity-smoke.log");
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&log_path)
        .map_err(|error| format!("could not create owned smoke log: {error}"))?;
    let log = SmokeLog(Arc::new(Mutex::new(file)));
    log.line(format!(
        "run=begin nonce={nonce:032x} pid={} log=pin-opacity-smoke.log",
        unsafe { GetCurrentProcessId() }
    ));
    let result = Arc::new(Mutex::new(None));
    let result_for_app = Arc::clone(&result);
    let log_for_app = log.clone();
    let output_for_app = output_dir.clone();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Pin Opacity Generated Composition")
            .with_inner_size([1000.0, 650.0])
            .with_position([80.0, 50.0])
            .with_resizable(false),
        renderer: eframe::Renderer::Glow,
        event_loop_builder: Some(Box::new(|builder| {
            use winit::platform::windows::EventLoopBuilderExtWindows;
            builder.with_any_thread(true);
        })),
        ..Default::default()
    };
    let native_run = eframe::run_native(
        "Pin Opacity Generated Composition",
        options,
        Box::new(move |cc| {
            SmokeApp::new(cc, output_for_app, nonce, log_for_app, result_for_app)
                .map(|app| Box::new(app) as Box<dyn eframe::App>)
                .map_err(|error| {
                    Box::new(std::io::Error::other(error))
                        as Box<dyn std::error::Error + Send + Sync>
                })
        }),
    );
    if let Err(error) = native_run {
        log.line(
            serde_json::json!({
                "result": "GAP",
                "reason": error.to_string(),
                "nonce": format!("{nonce:032x}"),
                "pid": unsafe { GetCurrentProcessId() },
                "backend": "eframe-glow",
                "source": "generated-static-presenter",
                "os_source_capture": "unverified"
            })
            .to_string(),
        );
        return Err(format!("GAP: eframe Glow smoke window failed: {error}"));
    }
    let outcome = result
        .lock()
        .ok()
        .and_then(|mut result| result.take())
        .unwrap_or_else(|| Err("GAP: smoke app exited without a terminal receipt".into()));
    if let Err(error) = &outcome {
        log.line(format!("run=terminal result={error}"));
    }
    outcome
}
