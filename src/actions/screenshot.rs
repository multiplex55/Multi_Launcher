use chrono::Local;
use std::borrow::Cow;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    Window,
    Region,
    Desktop,
}

#[cfg(target_os = "windows")]
pub fn capture(mode: Mode, clipboard: bool) -> anyhow::Result<PathBuf> {
    let dir = dirs_next::picture_dir().unwrap_or_else(|| std::env::current_dir().unwrap());
    std::fs::create_dir_all(&dir)?;
    let filename = format!("multi_launcher_{}.png", Local::now().format("%Y%m%d_%H%M%S"));
    let path = dir.join(filename);
    let path_str = path.to_string_lossy().to_string();
    match mode {
        Mode::Window => screenshot::screenshot_window(path_str.clone()),
        Mode::Region => screenshot::screenshot_area(path_str.clone(), false),
        Mode::Desktop => screenshot::screenshot_full(path_str.clone()),
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
