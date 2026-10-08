use std::sync::Arc;

use crate::commands::CoordinateToolCommand;
use crate::coordinate_tool::{
    CoordinateRuntimeFactory, CoordinateToolController, CoordinateToolPreferences,
    CoordinateUnavailable, format_coordinate,
};
use crate::settings::Settings;

pub(crate) trait CoordinateClipboardWriter {
    fn set_text(&mut self, text: &str) -> anyhow::Result<()>;
}

struct SystemCoordinateClipboard;

impl CoordinateClipboardWriter for SystemCoordinateClipboard {
    fn set_text(&mut self, text: &str) -> anyhow::Result<()> {
        crate::actions::clipboard::set_text(text)
    }
}

pub(crate) struct CoordinateToolGui {
    controller: CoordinateToolController,
    settings_path: String,
    preferences: CoordinateToolPreferences,
    clipboard: Box<dyn CoordinateClipboardWriter>,
    reported_error: Option<String>,
}

impl CoordinateToolGui {
    pub(crate) fn new(settings_path: String, preferences: CoordinateToolPreferences) -> Self {
        Self::with_backends(
            settings_path,
            preferences,
            Arc::new(crate::coordinate_tool::NativeCoordinateRuntimeFactory),
            Box::new(SystemCoordinateClipboard),
        )
    }

    fn with_backends(
        settings_path: String,
        preferences: CoordinateToolPreferences,
        factory: Arc<dyn CoordinateRuntimeFactory>,
        clipboard: Box<dyn CoordinateClipboardWriter>,
    ) -> Self {
        let preferences = preferences.normalized();
        Self {
            controller: CoordinateToolController::new_with_preferences(
                factory,
                preferences.clone(),
            ),
            settings_path,
            preferences,
            clipboard,
            reported_error: None,
        }
    }

    pub(crate) fn apply_loaded_preferences(
        &mut self,
        preferences: CoordinateToolPreferences,
    ) -> Result<(), String> {
        let preferences = preferences.normalized();
        self.controller
            .set_preferences(preferences.clone())
            .map_err(|error| format!("Could not apply coordinate preferences: {error}"))?;
        self.preferences = preferences;
        Ok(())
    }

    pub(crate) fn execute(
        &mut self,
        command: &CoordinateToolCommand,
    ) -> Result<Option<String>, String> {
        use CoordinateToolCommand as Command;

        match command {
            Command::ToggleHud => {
                let enabled = !self.controller.runtime_state().hud_enabled();
                self.controller.set_hud_enabled(enabled)?;
            }
            Command::SetHudEnabled(enabled) => self.controller.set_hud_enabled(*enabled)?,
            Command::SetSpace(space) => {
                let space = *space;
                self.update_preferences(|preferences| preferences.space = space)?;
            }
            Command::SetHudDetail(detail) => {
                let detail = *detail;
                self.update_preferences(|preferences| preferences.hud_detail = detail)?;
            }
            Command::SetOffset(offset) => {
                let offset = *offset;
                self.update_preferences(|preferences| preferences.cursor_offset = offset)?;
            }
            Command::Freeze => self.controller.freeze(),
            Command::Unfreeze => self.controller.unfreeze(),
            Command::Copy => return self.copy_displayed_sample().map(Some),
            Command::HudHelp | Command::CrosshairHelp => {
                return Err("help is handled by the coordinate command handler".into());
            }
            Command::ToggleCrosshair => {
                let enabled = !self.controller.runtime_state().crosshair_enabled();
                self.controller.set_crosshair_enabled(enabled)?;
            }
            Command::SetCrosshairEnabled(enabled) => {
                self.controller.set_crosshair_enabled(*enabled)?;
            }
            Command::SetCrosshairColor(color) => {
                let color = *color;
                self.update_preferences(|preferences| preferences.crosshair.color = color)?;
            }
            Command::SetCrosshairThickness(thickness) => {
                let thickness = *thickness;
                self.update_preferences(|preferences| preferences.crosshair.thickness = thickness)?;
            }
            Command::SetCrosshairLength(length) => {
                let length = *length;
                self.update_preferences(|preferences| preferences.crosshair.arm_length = length)?;
            }
            Command::SetCrosshairOpacity(opacity) => {
                let opacity = *opacity;
                self.update_preferences(|preferences| preferences.crosshair.opacity = opacity)?;
            }
            Command::SetGuides(enabled) => {
                let enabled = *enabled;
                self.update_preferences(|preferences| {
                    preferences.crosshair.virtual_desktop_guides = enabled
                })?;
            }
            Command::SetContrast(enabled) => {
                let enabled = *enabled;
                self.update_preferences(|preferences| {
                    preferences.crosshair.high_contrast_outline = enabled
                })?;
            }
            Command::Invalid { error, .. } => return Err(error.clone()),
        }
        Ok(None)
    }

    pub(crate) fn poll_error(&mut self) -> Option<String> {
        match self.controller.last_error() {
            Some(error) if self.reported_error.as_deref() != Some(error.as_str()) => {
                self.reported_error = Some(error.clone());
                Some(error)
            }
            Some(_) => None,
            None => {
                self.reported_error = None;
                None
            }
        }
    }

    pub(crate) fn shutdown(&mut self) -> Result<(), String> {
        self.controller.shutdown()
    }

    #[cfg(test)]
    pub(crate) fn preferences(&self) -> &CoordinateToolPreferences {
        &self.preferences
    }

    #[cfg(test)]
    pub(crate) fn runtime_state(&self) -> crate::coordinate_tool::CoordinateToolRuntimeState {
        self.controller.runtime_state()
    }

    fn update_preferences(
        &mut self,
        update: impl FnOnce(&mut CoordinateToolPreferences),
    ) -> Result<(), String> {
        let settings_path = self.settings_path.clone();
        let committed = Settings::update(&settings_path, |settings| {
            update(&mut settings.coordinate_tool);
            settings.coordinate_tool = settings.coordinate_tool.clone().normalized();
            Ok(())
        })
        .map_err(|error| format!("Could not save coordinate preferences: {error}"))?;
        self.apply_loaded_preferences(committed.coordinate_tool)
    }

    fn copy_displayed_sample(&mut self) -> Result<String, String> {
        let sample = self.controller.sample_for_copy()?;
        let formatted = format_coordinate(&sample, self.preferences.space)
            .map_err(coordinate_unavailable_message)?;
        self.clipboard
            .set_text(&formatted.text)
            .map_err(|error| format!("Could not copy coordinates: {error}"))?;
        let text = formatted.text.clone();
        self.controller.record_successful_copy(formatted);
        Ok(text)
    }
}

fn coordinate_unavailable_message(reason: CoordinateUnavailable) -> String {
    match reason {
        CoordinateUnavailable::MonitorUnavailable => {
            "Monitor-relative coordinates are unavailable for the current sample.".into()
        }
        CoordinateUnavailable::ForegroundClientUnavailable => {
            "Foreground-client coordinates are unavailable for the current sample.".into()
        }
        CoordinateUnavailable::ArithmeticOverflow => {
            "Coordinate conversion exceeded the supported physical-pixel range.".into()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex, mpsc};
    use std::time::Duration;

    use super::*;
    use crate::coordinate_tool::{
        CoordinateOffset, CoordinateRenderFrame, CoordinateSample, CoordinateSampler,
        CoordinateSurfaceBackend, ForegroundClientGeometry, MonitorGeometry, MonitorId,
        PhysicalPoint, PhysicalRect,
    };

    #[derive(Clone)]
    struct FakeFactory {
        sample: Arc<Mutex<CoordinateSample>>,
        rendered: mpsc::Sender<CoordinateRenderFrame>,
    }

    struct FakeSampler(Arc<Mutex<CoordinateSample>>);

    impl CoordinateSampler for FakeSampler {
        fn sample(&mut self) -> Result<CoordinateSample, String> {
            Ok(self.0.lock().unwrap().clone())
        }
    }

    struct FakeBackend(mpsc::Sender<CoordinateRenderFrame>);

    impl CoordinateSurfaceBackend for FakeBackend {
        fn render(&mut self, frame: &CoordinateRenderFrame) -> Result<(), String> {
            self.0
                .send(frame.clone())
                .map_err(|error| error.to_string())
        }

        fn shutdown(&mut self) -> Result<(), String> {
            Ok(())
        }
    }

    impl CoordinateRuntimeFactory for FakeFactory {
        fn create_sampler(&self) -> Result<Box<dyn CoordinateSampler>, String> {
            Ok(Box::new(FakeSampler(Arc::clone(&self.sample))))
        }

        fn create_backend(&self) -> Result<Box<dyn CoordinateSurfaceBackend>, String> {
            Ok(Box::new(FakeBackend(self.rendered.clone())))
        }
    }

    #[derive(Clone)]
    struct FakeClipboard {
        fail: Arc<AtomicBool>,
        writes: Arc<Mutex<Vec<String>>>,
    }

    impl CoordinateClipboardWriter for FakeClipboard {
        fn set_text(&mut self, text: &str) -> anyhow::Result<()> {
            if self.fail.load(Ordering::Acquire) {
                anyhow::bail!("injected clipboard failure");
            }
            self.writes.lock().unwrap().push(text.to_owned());
            Ok(())
        }
    }

    fn sample(x: i32, y: i32) -> CoordinateSample {
        CoordinateSample::new(
            PhysicalPoint::new(x, y),
            Some(PhysicalRect::new(-1920, 0, 1920, 1080).unwrap()),
            Some(MonitorGeometry {
                id: MonitorId::new("DISPLAY1"),
                bounds: PhysicalRect::new(-1920, 0, 0, 1080).unwrap(),
                work_area: PhysicalRect::new(-1920, 0, 0, 1040).unwrap(),
                effective_dpi: Some((96, 96)),
            }),
            Some(ForegroundClientGeometry::new(
                PhysicalPoint::new(-1800, 40),
                Some(PhysicalRect::new(-1800, 40, -100, 800).unwrap()),
            )),
        )
    }

    fn adapter(
        settings_path: String,
    ) -> (
        CoordinateToolGui,
        mpsc::Receiver<CoordinateRenderFrame>,
        Arc<Mutex<CoordinateSample>>,
        FakeClipboard,
    ) {
        let (rendered, frames) = mpsc::channel();
        let current_sample = Arc::new(Mutex::new(sample(-1800, 200)));
        let factory = FakeFactory {
            sample: Arc::clone(&current_sample),
            rendered,
        };
        let clipboard = FakeClipboard {
            fail: Arc::new(AtomicBool::new(false)),
            writes: Arc::new(Mutex::new(Vec::new())),
        };
        let adapter = CoordinateToolGui::with_backends(
            settings_path,
            CoordinateToolPreferences::default(),
            Arc::new(factory),
            Box::new(clipboard.clone()),
        );
        (adapter, frames, current_sample, clipboard)
    }

    fn receive_frame(frames: &mpsc::Receiver<CoordinateRenderFrame>) -> CoordinateRenderFrame {
        frames
            .recv_timeout(Duration::from_secs(2))
            .expect("fake worker should publish its frame")
    }

    #[test]
    fn preference_updates_persist_transactionally_before_runtime_publication() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let path = path.to_string_lossy().to_string();
        let (mut gui, _, _, _) = adapter(path.clone());
        gui.execute(&CoordinateToolCommand::SetSpace(
            crate::coordinate_tool::CoordinateSpace::ForegroundClient,
        ))
        .unwrap();
        let committed = Settings::load(&path).unwrap();
        assert_eq!(
            committed.coordinate_tool.space,
            crate::coordinate_tool::CoordinateSpace::ForegroundClient
        );
        assert_eq!(gui.preferences().space, committed.coordinate_tool.space);

        let invalid_path = directory.path().to_string_lossy().to_string();
        let (mut failed_gui, _, _, _) = adapter(invalid_path);
        let before = failed_gui.preferences().clone();
        assert!(
            failed_gui
                .execute(&CoordinateToolCommand::SetOffset(CoordinateOffset::new(
                    80, -40
                )))
                .is_err()
        );
        assert_eq!(failed_gui.preferences(), &before);
    }

    #[test]
    fn copy_tracks_only_successful_clipboard_writes_and_uses_frozen_display_sample() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory
            .path()
            .join("settings.json")
            .to_string_lossy()
            .to_string();
        let (mut gui, frames, current_sample, clipboard) = adapter(path);
        gui.execute(&CoordinateToolCommand::SetHudEnabled(true))
            .unwrap();
        assert!(receive_frame(&frames).current_sample.is_some());

        clipboard.fail.store(true, Ordering::Release);
        assert!(gui.execute(&CoordinateToolCommand::Copy).is_err());
        assert!(gui.runtime_state().last_successful_copy().is_none());
        assert!(clipboard.writes.lock().unwrap().is_empty());

        gui.execute(&CoordinateToolCommand::Freeze).unwrap();
        *current_sample.lock().unwrap() = sample(-1600, 300);
        let mut saw_moved_cursor = false;
        for _ in 0..8 {
            let frame = receive_frame(&frames);
            if frame
                .current_sample
                .as_ref()
                .is_some_and(|sample| sample.desktop_point == PhysicalPoint::new(-1600, 300))
            {
                saw_moved_cursor = true;
                break;
            }
        }
        assert!(
            saw_moved_cursor,
            "the fake sampler should publish the moved point"
        );

        clipboard.fail.store(false, Ordering::Release);
        assert_eq!(
            gui.execute(&CoordinateToolCommand::Copy).unwrap(),
            Some("-1800,200".into())
        );
        assert_eq!(*clipboard.writes.lock().unwrap(), ["-1800,200"]);
        assert_eq!(
            gui.runtime_state().last_successful_copy().unwrap().text,
            "-1800,200"
        );
        gui.shutdown().unwrap();
    }

    #[test]
    fn hud_and_crosshair_enablement_remain_independent_through_the_adapter() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory
            .path()
            .join("settings.json")
            .to_string_lossy()
            .to_string();
        let (mut gui, frames, _, _) = adapter(path);
        gui.execute(&CoordinateToolCommand::SetHudEnabled(true))
            .unwrap();
        receive_frame(&frames);
        gui.execute(&CoordinateToolCommand::SetCrosshairEnabled(true))
            .unwrap();
        gui.execute(&CoordinateToolCommand::SetHudEnabled(false))
            .unwrap();
        let state = gui.runtime_state();
        assert!(!state.hud_enabled());
        assert!(state.crosshair_enabled());
        gui.shutdown().unwrap();
    }
}
