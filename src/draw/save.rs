use crate::draw::composite::Rgba;
use crate::draw::messages::ExitReason;
use anyhow::{anyhow, Context, Result};
use chrono::Local;
use std::fs;
use std::path::{Path, PathBuf};

pub const DRAW_EXPORT_SUBDIR: &str = "draw_exports";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveChoice {
    Desktop,
    Blank,
    Both,
    Discard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SaveConfig {
    pub blank_background: Rgba,
}

impl Default for SaveConfig {
    fn default() -> Self {
        Self {
            blank_background: Rgba::BLACK,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExitPromptState {
    pub reason: ExitReason,
    pub frozen_input: bool,
    pub overlay_hidden_for_capture: bool,
    pub last_error: Option<String>,
}

impl ExitPromptState {
    pub fn from_exit_reason(reason: ExitReason) -> Self {
        Self {
            reason,
            frozen_input: true,
            overlay_hidden_for_capture: false,
            last_error: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveTargets {
    pub desktop: Option<PathBuf>,
    pub blank: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveDispatchOutcome {
    Save(SaveTargets),
    Discard,
}

pub fn exe_relative_output_folder_from_path(exe_path: &Path) -> Result<PathBuf> {
    let parent = exe_path
        .parent()
        .ok_or_else(|| anyhow!("executable path has no parent: {}", exe_path.display()))?;
    Ok(parent.join(DRAW_EXPORT_SUBDIR))
}

pub fn ensure_output_folder() -> Result<PathBuf> {
    let exe_path = std::env::current_exe().context("resolve current executable")?;
    let output = exe_relative_output_folder_from_path(&exe_path)?;
    fs::create_dir_all(&output)
        .with_context(|| format!("create draw output folder {}", output.display()))?;
    Ok(output)
}

pub fn timestamped_stem(now: chrono::DateTime<Local>) -> String {
    now.format("%Y%m%d_%H%M%S").to_string()
}

pub fn build_filename(stem: &str, suffix: &str) -> String {
    format!("{}_{}.png", stem, suffix)
}

pub fn dispatch_save_choice(
    choice: SaveChoice,
    output_dir: &Path,
    now: chrono::DateTime<Local>,
) -> SaveDispatchOutcome {
    let stem = timestamped_stem(now);
    let desktop_path = output_dir.join(build_filename(&stem, "desktop"));
    let blank_path = output_dir.join(build_filename(&stem, "blank"));

    match choice {
        SaveChoice::Desktop => SaveDispatchOutcome::Save(SaveTargets {
            desktop: Some(desktop_path),
            blank: None,
        }),
        SaveChoice::Blank => SaveDispatchOutcome::Save(SaveTargets {
            desktop: None,
            blank: Some(blank_path),
        }),
        SaveChoice::Both => SaveDispatchOutcome::Save(SaveTargets {
            desktop: Some(desktop_path),
            blank: Some(blank_path),
        }),
        SaveChoice::Discard => SaveDispatchOutcome::Discard,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_filename, dispatch_save_choice, exe_relative_output_folder_from_path, SaveChoice,
        SaveDispatchOutcome, DRAW_EXPORT_SUBDIR,
    };
    use chrono::{Local, TimeZone};
    use std::path::Path;

    #[test]
    fn exe_relative_output_folder_is_sibling_of_exe() {
        let exe = Path::new("/tmp/myapp/bin/multi_launcher");
        let output = exe_relative_output_folder_from_path(exe).expect("output path");
        assert_eq!(output, Path::new("/tmp/myapp/bin").join(DRAW_EXPORT_SUBDIR));
    }

    #[test]
    fn filename_generator_formats_timestamp_and_suffix() {
        let dt = Local
            .with_ymd_and_hms(2026, 1, 2, 3, 4, 5)
            .single()
            .expect("date time");

        let output_dir = Path::new("/tmp/exports");
        let desktop = dispatch_save_choice(SaveChoice::Desktop, output_dir, dt.clone());
        let blank = dispatch_save_choice(SaveChoice::Blank, output_dir, dt.clone());

        match desktop {
            SaveDispatchOutcome::Save(targets) => {
                assert!(targets
                    .desktop
                    .unwrap()
                    .ends_with("20260102_030405_desktop.png"));
                assert!(targets.blank.is_none());
            }
            SaveDispatchOutcome::Discard => panic!("unexpected discard"),
        }

        match blank {
            SaveDispatchOutcome::Save(targets) => {
                assert!(targets
                    .blank
                    .unwrap()
                    .ends_with("20260102_030405_blank.png"));
                assert!(targets.desktop.is_none());
            }
            SaveDispatchOutcome::Discard => panic!("unexpected discard"),
        }
    }

    #[test]
    fn dispatcher_returns_expected_actions_for_each_choice() {
        let dt = Local
            .with_ymd_and_hms(2026, 1, 2, 3, 4, 5)
            .single()
            .expect("date time");
        let output_dir = Path::new("/tmp/exports");

        assert!(matches!(
            dispatch_save_choice(SaveChoice::Desktop, output_dir, dt.clone()),
            SaveDispatchOutcome::Save(targets) if targets.desktop.is_some() && targets.blank.is_none()
        ));

        assert!(matches!(
            dispatch_save_choice(SaveChoice::Blank, output_dir, dt.clone()),
            SaveDispatchOutcome::Save(targets) if targets.desktop.is_none() && targets.blank.is_some()
        ));

        assert!(matches!(
            dispatch_save_choice(SaveChoice::Both, output_dir, dt.clone()),
            SaveDispatchOutcome::Save(targets) if targets.desktop.is_some() && targets.blank.is_some()
        ));

        assert!(matches!(
            dispatch_save_choice(SaveChoice::Discard, output_dir, dt),
            SaveDispatchOutcome::Discard
        ));
    }

    #[test]
    fn filename_builder_suffixes_match_contract() {
        assert_eq!(
            build_filename("20260102_030405", "desktop"),
            "20260102_030405_desktop.png"
        );
        assert_eq!(
            build_filename("20260102_030405", "blank"),
            "20260102_030405_blank.png"
        );
    }
}
