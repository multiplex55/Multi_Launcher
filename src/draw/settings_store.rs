use crate::draw::settings::DrawSettings;
use crate::settings::Settings;
use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};

pub const DRAW_SETTINGS_FILE_NAME: &str = "draw_settings.json";
const LEGACY_DRAW_PLUGIN_NAME: &str = "draw";

pub fn settings_path_from_exe_path(exe_path: &Path) -> Result<PathBuf> {
    let parent = exe_path
        .parent()
        .ok_or_else(|| anyhow!("executable path has no parent: {}", exe_path.display()))?;
    Ok(parent.join(DRAW_SETTINGS_FILE_NAME))
}

pub fn resolve_settings_path() -> Result<PathBuf> {
    let exe_path = std::env::current_exe().context("resolve current executable")?;
    settings_path_from_exe_path(&exe_path)
}

pub fn load(legacy_settings_path: &str) -> Result<DrawSettings> {
    let draw_settings_path = resolve_settings_path()?;
    load_from_path(&draw_settings_path, legacy_settings_path)
}

pub fn load_dedicated() -> Result<Option<DrawSettings>> {
    let draw_settings_path = resolve_settings_path()?;
    load_dedicated_from_path(&draw_settings_path)
}

pub fn save(settings: &DrawSettings) -> Result<PathBuf> {
    let draw_settings_path = resolve_settings_path()?;
    save_to_path(&draw_settings_path, settings)?;
    Ok(draw_settings_path)
}

fn load_from_path(draw_settings_path: &Path, legacy_settings_path: &str) -> Result<DrawSettings> {
    if let Some(loaded) = load_dedicated_from_path(draw_settings_path)? {
        return Ok(loaded);
    }

    let settings = Settings::load(legacy_settings_path)
        .with_context(|| format!("load legacy settings from {legacy_settings_path}"))?;

    let Some(value) = settings.plugin_settings.get(LEGACY_DRAW_PLUGIN_NAME) else {
        return Ok(DrawSettings::default());
    };

    let mut loaded: DrawSettings = serde_json::from_value(value.clone())
        .context("deserialize legacy draw plugin settings payload")?;
    loaded.sanitize_for_first_pass_transparency();
    Ok(loaded)
}

fn load_dedicated_from_path(draw_settings_path: &Path) -> Result<Option<DrawSettings>> {
    if !draw_settings_path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(draw_settings_path)
        .with_context(|| format!("read draw settings file {}", draw_settings_path.display()))?;

    if content.trim().is_empty() {
        return Ok(Some(DrawSettings::default()));
    }

    let mut loaded: DrawSettings = serde_json::from_str(&content).with_context(|| {
        format!(
            "deserialize draw settings file {}",
            draw_settings_path.display()
        )
    })?;
    loaded.sanitize_for_first_pass_transparency();
    Ok(Some(loaded))
}

fn save_to_path(draw_settings_path: &Path, settings: &DrawSettings) -> Result<()> {
    if let Some(parent) = draw_settings_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create draw settings parent folder {}", parent.display()))?;
    }

    let mut sanitized = settings.clone();
    sanitized.sanitize_for_first_pass_transparency();
    let json = serde_json::to_string_pretty(&sanitized)
        .context("serialize draw settings for dedicated draw settings file")?;
    std::fs::write(draw_settings_path, json)
        .with_context(|| format!("write draw settings file {}", draw_settings_path.display()))
}

#[cfg(test)]
mod tests {
    use super::{
        load_dedicated_from_path, load_from_path, save_to_path, settings_path_from_exe_path,
        DRAW_SETTINGS_FILE_NAME,
    };
    use crate::draw::settings::{DrawColor, DrawSettings};
    use std::path::Path;

    #[test]
    fn settings_path_is_resolved_next_to_executable() {
        let exe = Path::new("/tmp/myapp/bin/multi_launcher");
        let path = settings_path_from_exe_path(exe).expect("path");
        assert_eq!(
            path,
            Path::new("/tmp/myapp/bin").join(DRAW_SETTINGS_FILE_NAME)
        );
    }

    #[test]
    fn dedicated_load_returns_none_when_file_is_missing() {
        let dir = tempfile::tempdir().expect("temp dir");
        let draw_settings_path = dir.path().join(DRAW_SETTINGS_FILE_NAME);

        let loaded = load_dedicated_from_path(&draw_settings_path).expect("load dedicated");
        assert_eq!(loaded, None);
    }

    #[test]
    fn dedicated_store_roundtrip_serialization() {
        let dir = tempfile::tempdir().expect("temp dir");
        let draw_settings_path = dir.path().join(DRAW_SETTINGS_FILE_NAME);

        let mut settings = DrawSettings::default();
        settings.exit_timeout_seconds = 12;
        settings.last_color = DrawColor::rgba(1, 2, 3, 255);

        save_to_path(&draw_settings_path, &settings).expect("save settings");
        let loaded = load_from_path(&draw_settings_path, "settings.json").expect("load settings");

        assert_eq!(loaded, settings);
    }

    #[test]
    fn load_migrates_legacy_plugin_settings_when_dedicated_file_is_missing() {
        let dir = tempfile::tempdir().expect("temp dir");
        let draw_settings_path = dir.path().join(DRAW_SETTINGS_FILE_NAME);
        let legacy_settings_path = dir.path().join("settings.json");

        let mut legacy_settings = crate::settings::Settings::default();
        let mut expected = DrawSettings::default();
        expected.last_width = 77;
        expected.enable_pressure = false;
        legacy_settings.plugin_settings.insert(
            "draw".to_string(),
            serde_json::to_value(&expected).expect("serialize legacy draw settings"),
        );
        legacy_settings
            .save(&legacy_settings_path.to_string_lossy())
            .expect("save legacy settings");

        let loaded = load_from_path(&draw_settings_path, &legacy_settings_path.to_string_lossy())
            .expect("load with migration fallback");
        assert_eq!(loaded, expected);
    }
}
