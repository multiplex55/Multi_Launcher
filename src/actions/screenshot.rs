use chrono::Local;
use std::borrow::Cow;
use std::path::PathBuf;

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
            use std::thread::sleep;
            use std::time::{Duration, Instant};

            // Wait for the snipping tool to provide a new clipboard image
            let mut cb = arboard::Clipboard::new()?;
            let old = cb.get_image().ok().map(|img| {
                (img.width, img.height, img.bytes.into_owned())
            });

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

