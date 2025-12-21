use crate::actions::Action;
use crate::plugin::Plugin;
use crate::settings::Settings;
use eframe::egui;
use rfd::FileDialog;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Return the directory used to store screenshots.
///
/// The path is loaded from `settings.json` if available. When no directory is
/// configured, a `MultiLauncher_Screenshots` folder in the current working
/// directory is used.
pub fn screenshot_dir() -> PathBuf {
    if let Ok(s) = Settings::load("settings.json") {
        if let Some(dir) = s.screenshot_dir {
            return PathBuf::from(dir);
        }
    }
    std::env::current_dir()
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("MultiLauncher_Screenshots")
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ScreenshotPluginSettings {
    pub screenshot_dir: String,
    pub screenshot_save_file: bool,
    pub screenshot_auto_save: bool,
    pub screenshot_use_editor: bool,
}

impl Default for ScreenshotPluginSettings {
    fn default() -> Self {
        Self {
            screenshot_dir: screenshot_dir().to_string_lossy().to_string(),
            screenshot_save_file: true,
            screenshot_auto_save: true,
            screenshot_use_editor: true,
        }
    }
}

pub fn launch_editor(
    app: &mut crate::gui::LauncherApp,
    mode: crate::actions::screenshot::Mode,
    clip: bool,
) -> anyhow::Result<()> {
    use chrono::Local;
    use std::borrow::Cow;
    let capture = crate::actions::screenshot::capture_raw(mode, app.developer_debug_enabled())?;
    if let Some(info) = &capture.active_window {
        app.record_captured_window(info.clone());
    }
    let img = capture.image;
    if app.get_screenshot_use_editor() {
        app.open_screenshot_editor(img, clip);
    } else {
        if clip {
            let (w, h) = img.dimensions();
            let mut cb = arboard::Clipboard::new()?;
            cb.set_image(arboard::ImageData {
                width: w as usize,
                height: h as usize,
                bytes: Cow::Owned(img.clone().into_raw()),
            })?;
            if app.get_screenshot_save_file() {
                let dir = screenshot_dir();
                std::fs::create_dir_all(&dir)?;
                let filename = format!(
                    "multi_launcher_{}.png",
                    Local::now().format("%Y%m%d_%H%M%S")
                );
                let path = dir.join(filename);
                img.save(&path)?;
            }
        } else {
            let dir = screenshot_dir();
            std::fs::create_dir_all(&dir)?;
            let filename = format!(
                "multi_launcher_{}.png",
                Local::now().format("%Y%m%d_%H%M%S")
            );
            let path = dir.join(filename);
            img.save(&path)?;
            open::that(&path)?;
        }
    }
    Ok(())
}

pub struct ScreenshotPlugin;

impl Plugin for ScreenshotPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if crate::common::strip_prefix_ci(query.trim(), "ss").is_none() {
            return Vec::new();
        }
        vec![
            Action {
                label: "Screenshot active window".into(),
                desc: "Screenshot".into(),
                action: "screenshot:window".into(),
                args: None,
            },
            Action {
                label: "Screenshot region".into(),
                desc: "Screenshot".into(),
                action: "screenshot:region".into(),
                args: None,
            },
            Action {
                label: "Screenshot desktop".into(),
                desc: "Screenshot".into(),
                action: "screenshot:desktop".into(),
                args: None,
            },
            Action {
                label: "Screenshot active window to clipboard".into(),
                desc: "Screenshot".into(),
                action: "screenshot:window_clip".into(),
                args: None,
            },
            Action {
                label: "Screenshot region to clipboard".into(),
                desc: "Screenshot".into(),
                action: "screenshot:region_clip".into(),
                args: None,
            },
            Action {
                label: "Screenshot desktop to clipboard".into(),
                desc: "Screenshot".into(),
                action: "screenshot:desktop_clip".into(),
                args: None,
            },
        ]
    }

    fn name(&self) -> &str {
        "screenshot"
    }

    fn description(&self) -> &str {
        "Take screenshots with optional editor (prefix: `ss`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "shot".into(),
                desc: "Screenshot".into(),
                action: "query:ss ".into(),
                args: None,
            },
            Action {
                label: "shot clip".into(),
                desc: "Screenshot".into(),
                action: "query:ss clip".into(),
                args: None,
            },
        ]
    }

    fn default_settings(&self) -> Option<serde_json::Value> {
        serde_json::to_value(ScreenshotPluginSettings::default()).ok()
    }

    fn settings_ui(&mut self, ui: &mut egui::Ui, value: &mut serde_json::Value) {
        let mut cfg: ScreenshotPluginSettings =
            serde_json::from_value(value.clone()).unwrap_or_default();
        ui.horizontal(|ui| {
            ui.label("Screenshot directory");
            ui.text_edit_singleline(&mut cfg.screenshot_dir);
            if ui.button("Browse").clicked() {
                if let Some(dir) = FileDialog::new().pick_folder() {
                    cfg.screenshot_dir = dir.display().to_string();
                }
            }
        });
        ui.checkbox(
            &mut cfg.screenshot_save_file,
            "Save file when copying screenshot",
        );
        ui.checkbox(&mut cfg.screenshot_use_editor, "Enable screenshot editor");
        ui.checkbox(&mut cfg.screenshot_auto_save, "Auto-save after editing");
        if let Ok(v) = serde_json::to_value(&cfg) {
            *value = v;
        }
    }
}
