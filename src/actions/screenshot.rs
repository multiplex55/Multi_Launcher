#[cfg(target_os = "windows")]
use chrono::Local;
#[cfg(target_os = "windows")]
use std::borrow::Cow;
use std::path::PathBuf;
#[cfg(target_os = "windows")]
use std::process::Command;

#[cfg(target_os = "windows")]
use crate::plugins::screenshot::screenshot_dir;
#[cfg(target_os = "windows")]
use win_screenshot::prelude::*;
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

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
    let path = dir.join(&filename);
    match mode {
        Mode::Window => {
            let hwnd = unsafe { GetForegroundWindow() };
            let buf = capture_window(hwnd.0 as isize)?;
            let img = image::RgbaImage::from_raw(buf.width, buf.height, buf.pixels)
                .ok_or_else(|| anyhow::anyhow!("invalid screenshot buffer"))?;
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
        }
        Mode::Desktop => {
            let buf = capture_display()?;
            let img = image::RgbaImage::from_raw(buf.width, buf.height, buf.pixels)
                .ok_or_else(|| anyhow::anyhow!("invalid screenshot buffer"))?;
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
        }
        Mode::Region => {
            Command::new("snippingtool").arg("/clip").status()?;
            let mut cb = arboard::Clipboard::new()?;
            let img_data = cb.get_image()?;
            let img = image::RgbaImage::from_raw(
                img_data.width as u32,
                img_data.height as u32,
                img_data.bytes.into_owned(),
            )
            .ok_or_else(|| anyhow::anyhow!("invalid clipboard data"))?;
            img.save(&path)?;
            if !clipboard {
                open::that(&path)?;
            }
        }
    }
    Ok(path)
}

#[cfg(not(target_os = "windows"))]
pub fn capture(_mode: Mode, _clipboard: bool) -> anyhow::Result<PathBuf> {
    anyhow::bail!("screenshot not supported on this platform")
}
