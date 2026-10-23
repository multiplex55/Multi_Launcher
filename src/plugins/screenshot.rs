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
    if let Ok(s) = Settings::load("settings.json")
        && let Some(dir) = s.screenshot_dir
    {
        return PathBuf::from(dir);
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScreenshotLaunchOutcome {
    Completed,
    Cancelled,
}

pub fn launch_editor(
    app: &mut crate::gui::LauncherApp,
    mode: crate::actions::screenshot::Mode,
    clip: bool,
    tool: crate::gui::MarkupTool,
) -> anyhow::Result<ScreenshotLaunchOutcome> {
    let img = match crate::actions::screenshot::capture_raw(mode) {
        Ok(img) => img,
        Err(err) if crate::actions::screenshot::is_cancel_or_timeout(&err) => {
            return Ok(ScreenshotLaunchOutcome::Cancelled);
        }
        Err(err) => return Err(err),
    };
    deliver_captured_image(app, img, clip, tool)
}

/// Applies screenshot destination settings after pixel acquisition has
/// completed. Delayed capture calls this only on the GUI thread and only for
/// the still-current request generation.
pub(crate) fn deliver_captured_image(
    app: &mut crate::gui::LauncherApp,
    img: image::RgbaImage,
    clip: bool,
    tool: crate::gui::MarkupTool,
) -> anyhow::Result<ScreenshotLaunchOutcome> {
    use chrono::Local;
    use std::borrow::Cow;
    if app.get_screenshot_use_editor() {
        app.open_screenshot_editor(img, clip, tool);
    } else if clip {
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
    Ok(ScreenshotLaunchOutcome::Completed)
}

pub struct ScreenshotPlugin;

impl Plugin for ScreenshotPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let Some(suffix) = crate::common::strip_prefix_ci(query.trim(), "ss") else {
            return Vec::new();
        };
        let suffix = suffix.trim();
        let mut tokens = suffix.split_whitespace();
        match tokens.next() {
            Some(command) if command.eq_ignore_ascii_case("cancel") => {
                return if tokens.next().is_none() {
                    vec![Action {
                        label: "Cancel delayed screenshot".into(),
                        desc: "Screenshot".into(),
                        action: "screenshot:cancel".into(),
                        args: None,
                    }]
                } else {
                    Vec::new()
                };
            }
            Some(command) if command.eq_ignore_ascii_case("delay") => {
                let delay = match tokens.next() {
                    None => None,
                    Some("3") => Some(("3", "3 seconds")),
                    Some("5") => Some(("5", "5 seconds")),
                    Some("10") => Some(("10", "10 seconds")),
                    Some(_) => return Vec::new(),
                };
                if tokens.next().is_some() {
                    return Vec::new();
                }
                return delayed_actions(delay);
            }
            _ => {}
        }
        let mut actions = vec![
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
                label: "Screenshot region (markup pen)".into(),
                desc: "Screenshot".into(),
                action: "screenshot:region_markup".into(),
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
        ];
        actions.extend(delayed_actions(None));
        actions.push(Action {
            label: "Cancel delayed screenshot".into(),
            desc: "Screenshot".into(),
            action: "screenshot:cancel".into(),
            args: None,
        });
        actions
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
            Action {
                label: "shot region markup".into(),
                desc: "Screenshot".into(),
                action: "screenshot:region_markup".into(),
                args: None,
            },
            Action {
                label: "shot delay".into(),
                desc: "Screenshot".into(),
                action: "query:ss delay".into(),
                args: None,
            },
            Action {
                label: "shot cancel".into(),
                desc: "Screenshot".into(),
                action: "query:ss cancel".into(),
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
        ui.label("Screenshot directory");
        let response = ui.add(
            egui::TextEdit::singleline(&mut cfg.screenshot_dir).desired_width(ui.available_width()),
        );
        crate::settings_editor::render::observe_scoped_settings_navigation_response(
            ui,
            &response,
            "plugin.screenshot.directory",
            crate::gui::jump_mode::TargetKind::TextEntry,
        );
        ui.horizontal_wrapped(|ui| {
            if ui.button("Browse").clicked()
                && let Some(dir) = FileDialog::new().pick_folder()
            {
                cfg.screenshot_dir = dir.display().to_string();
            }
        });
        let response = ui.checkbox(
            &mut cfg.screenshot_save_file,
            "Save file when copying screenshot",
        );
        crate::settings_editor::render::observe_scoped_settings_navigation_response(
            ui,
            &response,
            "plugin.screenshot.save_file",
            crate::gui::jump_mode::TargetKind::Checkbox,
        );
        let response = ui.checkbox(&mut cfg.screenshot_use_editor, "Enable screenshot editor");
        crate::settings_editor::render::observe_scoped_settings_navigation_response(
            ui,
            &response,
            "plugin.screenshot.use_editor",
            crate::gui::jump_mode::TargetKind::Checkbox,
        );
        let response = ui.checkbox(&mut cfg.screenshot_auto_save, "Auto-save after editing");
        crate::settings_editor::render::observe_scoped_settings_navigation_response(
            ui,
            &response,
            "plugin.screenshot.auto_save",
            crate::gui::jump_mode::TargetKind::Checkbox,
        );
        if let Ok(v) = serde_json::to_value(&cfg) {
            *value = v;
        }
    }
}

fn delayed_actions(delay: Option<(&str, &str)>) -> Vec<Action> {
    const DELAYS: [(&str, &str); 3] =
        [("3", "3 seconds"), ("5", "5 seconds"), ("10", "10 seconds")];
    const CAPTURES: [(&str, &str); 7] = [
        ("window", "active window"),
        ("region", "region"),
        ("region_markup", "region (markup pen)"),
        ("desktop", "desktop"),
        ("window_clip", "active window to clipboard"),
        ("region_clip", "region to clipboard"),
        ("desktop_clip", "desktop to clipboard"),
    ];
    let delays: Vec<(&str, &str)> = match delay {
        Some(delay) => vec![delay],
        None => DELAYS.to_vec(),
    };
    delays
        .iter()
        .flat_map(|(wire_delay, description)| {
            CAPTURES.iter().map(move |(capture, label)| Action {
                label: format!("Screenshot {label} in {description}"),
                desc: "Screenshot".into(),
                action: format!("screenshot:delay:{wire_delay}:{capture}"),
                args: None,
            })
        })
        .collect()
}
