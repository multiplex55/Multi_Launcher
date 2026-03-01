use chrono::Local;
use std::borrow::Cow;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::plugins::screenshot::screenshot_dir;
use screenshots::Screen;
use windows::Win32::Foundation::RECT;
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowRect};

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
        if let Some(cur) = get_image() {
            if old.as_ref() != Some(&cur) {
                return Ok(cur);
            }
        }
        if start.elapsed() > timeout {
            anyhow::bail!(ScreenshotCaptureError::RegionSelectionCancelled);
        }
        std::thread::sleep(poll_interval);
    }
}

pub fn capture_raw(mode: Mode) -> anyhow::Result<image::RgbaImage> {
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
            let mut rect = RECT::default();
            unsafe { GetWindowRect(hwnd, &mut rect) }?;
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
