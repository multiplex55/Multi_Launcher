use once_cell::sync::OnceCell;
use std::path::PathBuf;

static SETTINGS_PATH: OnceCell<PathBuf> = OnceCell::new();

pub fn set_settings_path(path: impl Into<PathBuf>) {
    let _ = SETTINGS_PATH.set(path.into());
}

pub fn settings_path() -> PathBuf {
    SETTINGS_PATH
        .get()
        .cloned()
        .unwrap_or_else(|| PathBuf::from("settings.json"))
}
