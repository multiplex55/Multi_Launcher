use crate::actions::Action;
use crate::plugin::Plugin;
use crate::settings::Settings;
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

pub struct ScreenshotPlugin;

impl Plugin for ScreenshotPlugin {
    #[cfg(target_os = "windows")]
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

    #[cfg(not(target_os = "windows"))]
    fn search(&self, _query: &str) -> Vec<Action> {
        Vec::new()
    }

    fn name(&self) -> &str {
        "screenshot"
    }

    fn description(&self) -> &str {
        "Take screenshots (prefix: `ss`)"
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
}
