use crate::draw::composite::{
    composite_annotation_over_blank, composite_annotation_over_desktop, Rgba, RgbaBuffer,
};
use crate::draw::messages::ExitReason;
use crate::draw::settings::DrawSettings;
use anyhow::{anyhow, Context, Result};
use chrono::Local;
use image::RgbaImage;
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

impl SaveConfig {
    pub fn from_draw_settings(settings: &DrawSettings) -> Self {
        let color = settings.export_blank_background_color;
        Self {
            blank_background: Rgba {
                r: color.r,
                g: color.g,
                b: color.b,
                a: 255,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExitPromptState {
    pub reason: ExitReason,
    pub frozen_input: bool,
    pub phase: ExitPromptPhase,
    pub overlay_hidden_for_capture: bool,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitPromptPhase {
    PromptVisible,
    Saving,
}

impl ExitPromptState {
    pub fn from_exit_reason(reason: ExitReason, show_prompt: bool) -> Self {
        Self {
            reason,
            frozen_input: true,
            phase: if show_prompt {
                ExitPromptPhase::PromptVisible
            } else {
                ExitPromptPhase::Saving
            },
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveCompositionStats {
    pub desktop_capture_count: usize,
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

pub fn compose_and_persist_saves<C>(
    annotation: &RgbaBuffer,
    config: SaveConfig,
    targets: &SaveTargets,
    mut desktop_capture: C,
) -> Result<SaveCompositionStats>
where
    C: FnMut() -> Result<RgbaBuffer>,
{
    let mut desktop_background = None;
    let mut desktop_capture_count = 0;

    if targets.desktop.is_some() {
        desktop_background = Some(desktop_capture()?);
        desktop_capture_count += 1;
    }

    if let Some(path) = &targets.desktop {
        let composed = composite_annotation_over_desktop(
            desktop_background
                .as_ref()
                .ok_or_else(|| anyhow!("desktop capture was not available"))?,
            annotation,
        );
        save_rgba_buffer_png(path, &composed)?;
    }

    if let Some(path) = &targets.blank {
        let composed = composite_annotation_over_blank(annotation, config.blank_background);
        save_rgba_buffer_png(path, &composed)?;
    }

    Ok(SaveCompositionStats {
        desktop_capture_count,
    })
}

fn save_rgba_buffer_png(path: &Path, buffer: &RgbaBuffer) -> Result<()> {
    let image = RgbaImage::from_raw(buffer.width, buffer.height, buffer.pixels.clone())
        .ok_or_else(|| anyhow!("invalid RGBA buffer dimensions for PNG export"))?;
    image
        .save(path)
        .with_context(|| format!("save draw export {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::{
        build_filename, compose_and_persist_saves, dispatch_save_choice,
        exe_relative_output_folder_from_path, ExitPromptPhase, ExitPromptState, SaveChoice,
        SaveConfig, SaveDispatchOutcome, SaveTargets, DRAW_EXPORT_SUBDIR,
    };
    use crate::draw::composite::{Rgba, RgbaBuffer};
    use crate::draw::messages::ExitReason;
    use crate::draw::settings::{DrawColor, DrawSettings, LiveBackgroundMode};
    use chrono::{Local, TimeZone};
    use image::io::Reader as ImageReader;
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

    #[test]
    fn exit_prompt_phase_respects_configuration() {
        assert_eq!(
            ExitPromptState::from_exit_reason(ExitReason::UserRequest, true).phase,
            ExitPromptPhase::PromptVisible
        );
        assert_eq!(
            ExitPromptState::from_exit_reason(ExitReason::UserRequest, false).phase,
            ExitPromptPhase::Saving
        );
    }

    #[test]
    fn save_branching_uses_desktop_capture_only_when_requested() {
        let dir = tempfile::tempdir().expect("temp dir");
        let annotation = RgbaBuffer::from_pixels(1, 1, vec![255, 0, 0, 255]);
        let desktop_bg = RgbaBuffer::from_pixels(1, 1, vec![10, 20, 30, 255]);

        let desktop_path = dir.path().join("desktop.png");
        let stats = compose_and_persist_saves(
            &annotation,
            SaveConfig {
                blank_background: Rgba::BLACK,
            },
            &SaveTargets {
                desktop: Some(desktop_path.clone()),
                blank: None,
            },
            || Ok(desktop_bg.clone()),
        )
        .expect("desktop save should succeed");
        assert_eq!(stats.desktop_capture_count, 1);
        assert!(desktop_path.exists());

        let blank_path = dir.path().join("blank.png");
        let mut captures = 0;
        let stats = compose_and_persist_saves(
            &annotation,
            SaveConfig {
                blank_background: Rgba::BLACK,
            },
            &SaveTargets {
                desktop: None,
                blank: Some(blank_path.clone()),
            },
            || {
                captures += 1;
                Ok(desktop_bg.clone())
            },
        )
        .expect("blank save should succeed");
        assert_eq!(stats.desktop_capture_count, 0);
        assert_eq!(captures, 0);
        assert!(blank_path.exists());
    }

    #[test]
    fn export_with_desktop_uses_capture_even_if_live_blank_mode() {
        let dir = tempfile::tempdir().expect("temp dir");
        let desktop_path = dir.path().join("desktop.png");
        let annotation = RgbaBuffer::from_pixels(1, 1, vec![0, 0, 0, 0]);
        let desktop_bg = RgbaBuffer::from_pixels(1, 1, vec![9, 99, 199, 255]);

        let mut settings = DrawSettings::default();
        settings.live_background_mode = LiveBackgroundMode::SolidColor;
        settings.live_blank_color = DrawColor::rgba(1, 2, 3, 255);
        settings.export_blank_background_color = DrawColor::rgba(200, 100, 50, 255);

        compose_and_persist_saves(
            &annotation,
            SaveConfig::from_draw_settings(&settings),
            &SaveTargets {
                desktop: Some(desktop_path.clone()),
                blank: None,
            },
            || Ok(desktop_bg.clone()),
        )
        .expect("desktop save should succeed");

        let png = ImageReader::open(&desktop_path)
            .expect("open desktop png")
            .decode()
            .expect("decode desktop png")
            .to_rgba8();
        assert_eq!(png.get_pixel(0, 0).0, [9, 99, 199, 255]);
    }

    #[test]
    fn export_without_desktop_uses_export_background() {
        let dir = tempfile::tempdir().expect("temp dir");
        let blank_path = dir.path().join("blank.png");
        let annotation = RgbaBuffer::from_pixels(1, 1, vec![0, 0, 0, 0]);

        let mut settings = DrawSettings::default();
        settings.live_background_mode = LiveBackgroundMode::DesktopTransparent;
        settings.live_blank_color = DrawColor::rgba(10, 20, 30, 255);
        settings.export_blank_background_color = DrawColor::rgba(44, 55, 66, 255);

        compose_and_persist_saves(
            &annotation,
            SaveConfig::from_draw_settings(&settings),
            &SaveTargets {
                desktop: None,
                blank: Some(blank_path.clone()),
            },
            || Ok(RgbaBuffer::from_pixels(1, 1, vec![255, 0, 0, 255])),
        )
        .expect("blank save should succeed");

        let png = ImageReader::open(&blank_path)
            .expect("open blank png")
            .decode()
            .expect("decode blank png")
            .to_rgba8();
        assert_eq!(png.get_pixel(0, 0).0, [44, 55, 66, 255]);
    }
}
