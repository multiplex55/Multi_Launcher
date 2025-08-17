#[cfg(target_os = "windows")]
use chrono::Local;
#[cfg(target_os = "windows")]
use std::borrow::Cow;
use std::path::PathBuf;

#[cfg(target_os = "windows")]
use crate::plugins::screenshot::screenshot_dir;
#[cfg(target_os = "windows")]
use screenshots::Screen;
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::RECT;
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowRect};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    Window,
    Region,
    Desktop,
}

#[cfg(target_os = "windows")]
pub fn capture(mode: Mode, clipboard: bool) -> anyhow::Result<PathBuf> {
    let dir = screenshot_dir();
    std::fs::create_dir_all(&dir)?;
    let filename = format!(
        "multi_launcher_{}.png",
        Local::now().format("%Y%m%d_%H%M%S")
    );
    let path = dir.join(filename);
    match mode {
        Mode::Desktop => {
            let screen = Screen::from_point(0, 0)?;
            let image = screen.capture()?;
            image.save(&path)?;
        }
        Mode::Window => {
            let hwnd = unsafe { GetForegroundWindow() };
            if !hwnd.is_invalid() {
                let mut rect = RECT::default();
                unsafe { GetWindowRect(hwnd, &mut rect) }?;
                let width = (rect.right - rect.left) as u32;
                let height = (rect.bottom - rect.top) as u32;
                let screen = Screen::from_point(rect.left + 1, rect.top + 1)?;
                let image = screen.capture_area(
                    rect.left - screen.display_info.x,
                    rect.top - screen.display_info.y,
                    width,
                    height,
                )?;
                image.save(&path)?;
            }
        }
        Mode::Region => {
            use std::process::Command;
            use std::thread::sleep;
            use std::time::{Duration, Instant};

            // Wait for the snipping tool to provide a new clipboard image
            let mut cb = arboard::Clipboard::new()?;
            let old = cb
                .get_image()
                .ok()
                .map(|img| (img.width, img.height, img.bytes.into_owned()));

            let _ = Command::new("explorer").arg("ms-screenclip:").status();

            let start = Instant::now();
            let img = loop {
                match cb.get_image() {
                    Ok(img) => {
                        let cur = (img.width, img.height, img.bytes.as_ref().to_vec());
                        if Some(cur.clone()) != old {
                            break img;
                        }
                    }
                    Err(_) => {}
                }
                if start.elapsed() > Duration::from_secs(30) {
                    anyhow::bail!("timed out waiting for snip");
                }
                sleep(Duration::from_millis(200));
            };

            let buf = image::RgbaImage::from_raw(
                img.width as u32,
                img.height as u32,
                img.bytes.into_owned(),
            )
            .ok_or_else(|| anyhow::anyhow!("invalid clipboard image"))?;
            buf.save(&path)?;
        }
    }
    if clipboard {
        let img = image::load_from_memory(&std::fs::read(&path)?)?.to_rgba8();
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

#[cfg(not(target_os = "windows"))]
pub fn capture(_mode: Mode, _clipboard: bool) -> anyhow::Result<PathBuf> {
    anyhow::bail!("screenshot not supported on this platform")
}
