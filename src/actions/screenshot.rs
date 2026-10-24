use chrono::Local;
use std::borrow::Cow;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::plugins::screenshot::screenshot_dir;
use screenshots::Screen;
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, RECT};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GA_ROOTOWNER, GetAncestor, GetForegroundWindow, GetWindowRect,
    GetWindowThreadProcessId, IsIconic, IsWindowVisible,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    Window,
    Region,
    Desktop,
}

#[derive(Debug)]
pub enum ScreenshotCaptureError {
    RegionSelectionCancelled,
    RegionSelectionTimedOut,
    ExcludedForegroundWindow,
    UnexpectedForegroundWindow,
    ForeignWindowCoversCapture,
}

impl std::fmt::Display for ScreenshotCaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RegionSelectionCancelled => {
                write!(f, "region capture cancelled by user")
            }
            Self::RegionSelectionTimedOut => {
                write!(f, "region capture timed out waiting for clipboard image")
            }
            Self::ExcludedForegroundWindow => {
                write!(
                    f,
                    "the foreground window belongs to the screenshot launcher"
                )
            }
            Self::UnexpectedForegroundWindow => {
                write!(
                    f,
                    "the foreground window does not match the QA-owned capture target"
                )
            }
            Self::ForeignWindowCoversCapture => {
                write!(f, "a foreign window covers the requested screenshot target")
            }
        }
    }
}

impl std::error::Error for ScreenshotCaptureError {}

#[derive(Clone, Debug, PartialEq)]
pub struct ClipboardSnapshot {
    pub width: usize,
    pub height: usize,
    pub bytes: Vec<u8>,
}

const REGION_CAPTURE_TIMEOUT: Duration = Duration::from_secs(10);
const REGION_CAPTURE_POLL_INTERVAL: Duration = Duration::from_millis(100);

pub fn is_cancel_or_timeout(err: &anyhow::Error) -> bool {
    err.downcast_ref::<ScreenshotCaptureError>()
        .is_some_and(|inner| {
            matches!(
                inner,
                ScreenshotCaptureError::RegionSelectionCancelled
                    | ScreenshotCaptureError::RegionSelectionTimedOut
            )
        })
}

pub fn wait_for_new_clipboard_image<F>(
    mut get_image: F,
    old: Option<ClipboardSnapshot>,
    timeout: Duration,
    poll_interval: Duration,
) -> anyhow::Result<ClipboardSnapshot>
where
    F: FnMut() -> Option<ClipboardSnapshot>,
{
    let start = Instant::now();
    loop {
        if let Some(cur) = get_image()
            && old.as_ref() != Some(&cur)
        {
            return Ok(cur);
        }
        if start.elapsed() > timeout {
            anyhow::bail!(ScreenshotCaptureError::RegionSelectionCancelled);
        }
        std::thread::sleep(poll_interval);
    }
}

pub fn capture_raw(mode: Mode) -> anyhow::Result<image::RgbaImage> {
    capture_raw_excluding(mode, &[])
}

/// Capture using one foreground-window lookup and refuse a delayed target that
/// resolves to one of the launcher's own native windows.
pub(crate) fn capture_raw_excluding(
    mode: Mode,
    excluded_windows: &[usize],
) -> anyhow::Result<image::RgbaImage> {
    capture_raw_for_owned_target(mode, excluded_windows, None)
}

/// Capture through the same foreground lookup used to choose the window. The
/// optional expectation is supplied only by an explicit QA capture request;
/// normal application captures have no target allowlist.
pub(crate) fn capture_raw_for_owned_target(
    mode: Mode,
    excluded_windows: &[usize],
    expected_target: Option<WindowCaptureTarget>,
) -> anyhow::Result<image::RgbaImage> {
    if expected_target.is_some() && mode != Mode::Window {
        anyhow::bail!(ScreenshotCaptureError::UnexpectedForegroundWindow);
    }
    match mode {
        Mode::Desktop => {
            let screen = Screen::from_point(0, 0)?;
            Ok(screen.capture()?)
        }
        Mode::Window => {
            let hwnd = unsafe { GetForegroundWindow() };
            if hwnd.is_invalid() {
                anyhow::bail!("invalid window");
            }
            if foreground_is_excluded(hwnd.0 as usize, excluded_windows) {
                anyhow::bail!(ScreenshotCaptureError::ExcludedForegroundWindow);
            }
            let mut rect = RECT::default();
            unsafe { GetWindowRect(hwnd, &mut rect) }?;
            if let Some(expected) = expected_target {
                let mut process_id = 0;
                let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
                if thread_id == 0
                    || process_id == 0
                    || hwnd.0 as usize != expected.hwnd
                    || process_id != expected.process_id
                    || !unsafe { IsWindowVisible(hwnd) }.as_bool()
                    || unsafe { IsIconic(hwnd) }.as_bool()
                    || rect.right <= rect.left
                    || rect.bottom <= rect.top
                {
                    anyhow::bail!(ScreenshotCaptureError::UnexpectedForegroundWindow);
                }
                ensure_no_foreign_capture_coverage(hwnd, expected, rect)?;
            }
            let width = (rect.right - rect.left) as u32;
            let height = (rect.bottom - rect.top) as u32;
            let screen = Screen::from_point(rect.left + 1, rect.top + 1)?;
            Ok(screen.capture_area(
                rect.left - screen.display_info.x,
                rect.top - screen.display_info.y,
                width,
                height,
            )?)
        }
        Mode::Region => {
            use std::process::Command;

            // Wait for the snipping tool to provide a new clipboard image
            let mut cb = arboard::Clipboard::new()?;
            let old = cb.get_image().ok().map(|img| ClipboardSnapshot {
                width: img.width,
                height: img.height,
                bytes: img.bytes.into_owned(),
            });

            let _ = Command::new("explorer").arg("ms-screenclip:").status();

            let img = wait_for_new_clipboard_image(
                || {
                    cb.get_image().ok().map(|img| ClipboardSnapshot {
                        width: img.width,
                        height: img.height,
                        bytes: img.bytes.into_owned(),
                    })
                },
                old,
                REGION_CAPTURE_TIMEOUT,
                REGION_CAPTURE_POLL_INTERVAL,
            )?;

            let buf = image::RgbaImage::from_raw(img.width as u32, img.height as u32, img.bytes)
                .ok_or_else(|| anyhow::anyhow!("invalid clipboard image"))?;
            Ok(buf)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WindowCaptureTarget {
    pub process_id: u32,
    pub hwnd: usize,
}

#[cfg(windows)]
struct CaptureCoverageScan {
    target: HWND,
    target_process_id: u32,
    target_rect: RECT,
    target_seen: bool,
    obstruction: Option<usize>,
    query_failed: Option<usize>,
}

#[cfg(windows)]
unsafe extern "system" fn inspect_capture_coverage(hwnd: HWND, parameter: LPARAM) -> BOOL {
    let scan = unsafe { &mut *(parameter.0 as *mut CaptureCoverageScan) };
    if hwnd == scan.target {
        scan.target_seen = true;
        return BOOL(0);
    }
    if !unsafe { IsWindowVisible(hwnd) }.as_bool() {
        return BOOL(1);
    }
    let mut rect = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut rect) }.is_err() {
        scan.query_failed = Some(hwnd.0 as usize);
        return BOOL(0);
    }
    if !rects_intersect(rect, scan.target_rect) {
        return BOOL(1);
    }
    let mut process_id = 0;
    let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
    if thread_id == 0 || process_id == 0 {
        scan.query_failed = Some(hwnd.0 as usize);
        return BOOL(0);
    }
    // Same-process popup windows owned by the selected application window
    // (menus and tooltips) are part of its surface. Any other intersecting
    // window above it makes the sampled pixels ambiguous and is refused.
    let owned_by_target = process_id == scan.target_process_id
        && unsafe { GetAncestor(hwnd, GA_ROOTOWNER) } == scan.target;
    if owned_by_target {
        BOOL(1)
    } else {
        scan.obstruction = Some(hwnd.0 as usize);
        BOOL(0)
    }
}

#[cfg(windows)]
fn ensure_no_foreign_capture_coverage(
    target: HWND,
    expected: WindowCaptureTarget,
    target_rect: RECT,
) -> anyhow::Result<()> {
    let mut scan = CaptureCoverageScan {
        target,
        target_process_id: expected.process_id,
        target_rect,
        target_seen: false,
        obstruction: None,
        query_failed: None,
    };
    let enumeration = unsafe {
        EnumWindows(
            Some(inspect_capture_coverage),
            LPARAM((&mut scan as *mut CaptureCoverageScan) as isize),
        )
    };
    if let Some(hwnd) = scan.query_failed {
        anyhow::bail!("could not verify coverage above the QA screenshot target (HWND 0x{hwnd:x})");
    }
    if !scan.target_seen {
        if let Err(error) = enumeration {
            anyhow::bail!("could not enumerate windows above the QA screenshot target: {error}");
        }
        anyhow::bail!(ScreenshotCaptureError::UnexpectedForegroundWindow);
    }
    if scan.obstruction.is_some() {
        anyhow::bail!(ScreenshotCaptureError::ForeignWindowCoversCapture);
    }
    Ok(())
}

fn rects_intersect(a: RECT, b: RECT) -> bool {
    a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom
}

fn foreground_is_excluded(hwnd: usize, excluded_windows: &[usize]) -> bool {
    excluded_windows.contains(&hwnd)
}

#[cfg(test)]
mod delayed_capture_tests {
    use super::{
        Mode, ScreenshotCaptureError, WindowCaptureTarget, capture_raw_for_owned_target,
        foreground_is_excluded,
    };

    #[test]
    fn screenshot_countdown_window_target_exclusion_is_exact() {
        let excluded = [0x10usize, 0x20];
        assert!(foreground_is_excluded(0x10, &excluded));
        assert!(foreground_is_excluded(0x20, &excluded));
        assert!(!foreground_is_excluded(0x21, &excluded));
        assert!(!foreground_is_excluded(0x10, &[]));
    }

    #[test]
    fn screenshot_countdown_owned_target_refuses_non_window_acquisition_before_side_effects() {
        let expected = WindowCaptureTarget {
            process_id: 1234,
            hwnd: 0x4567,
        };
        for mode in [Mode::Region, Mode::Desktop] {
            let error = capture_raw_for_owned_target(mode, &[], Some(expected))
                .expect_err("an explicitly owned QA target authorizes only Window capture");
            assert!(matches!(
                error.downcast_ref::<ScreenshotCaptureError>(),
                Some(ScreenshotCaptureError::UnexpectedForegroundWindow)
            ));
        }
    }
}

pub fn capture(mode: Mode, clipboard: bool) -> anyhow::Result<PathBuf> {
    let dir = screenshot_dir();
    std::fs::create_dir_all(&dir)?;
    let filename = format!(
        "multi_launcher_{}.png",
        Local::now().format("%Y%m%d_%H%M%S")
    );
    let path = dir.join(filename);
    let img = capture_raw(mode)?;
    img.save(&path)?;
    if clipboard {
        let (w, h) = img.dimensions();
        let mut cb = arboard::Clipboard::new()?;
        cb.set_image(arboard::ImageData {
            width: w as usize,
            height: h as usize,
            bytes: Cow::Owned(img.into_raw()),
        })?;
    } else {
        open::that(&path)?;
    }
    Ok(path)
}
