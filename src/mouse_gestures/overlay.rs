use crate::plugins::mouse_gestures::engine::Point;
use crate::plugins::mouse_gestures::settings::{
    MouseGestureOverlaySettings, MouseGesturePluginSettings,
};
use once_cell::sync::OnceCell;
use std::sync::{Arc, Mutex};

/// Rendering surface for gesture overlays.
///
/// Implementations are expected to use transparent, always-on-top windows
/// so the stroke can be drawn over existing applications.
trait OverlayWindow: Send {
    fn update_settings(&mut self, settings: &MouseGestureOverlaySettings);
    fn begin_stroke(&mut self, settings: &MouseGestureOverlaySettings, start: Point);
    fn push_point(&mut self, settings: &MouseGestureOverlaySettings, point: Point);
    fn end_stroke(&mut self, settings: &MouseGestureOverlaySettings);
}

#[derive(Default)]
struct NoopOverlayWindow {
    _settings: MouseGestureOverlaySettings,
}

impl OverlayWindow for NoopOverlayWindow {
    fn update_settings(&mut self, settings: &MouseGestureOverlaySettings) {
        self._settings = settings.clone();
    }

    fn begin_stroke(&mut self, settings: &MouseGestureOverlaySettings, _start: Point) {
        self._settings = settings.clone();
    }

    fn push_point(&mut self, settings: &MouseGestureOverlaySettings, _point: Point) {
        self._settings = settings.clone();
    }

    fn end_stroke(&mut self, settings: &MouseGestureOverlaySettings) {
        self._settings = settings.clone();
    }
}

pub struct StrokeOverlay {
    settings: MouseGestureOverlaySettings,
    window: Box<dyn OverlayWindow>,
}

impl StrokeOverlay {
    pub fn new() -> Self {
        Self {
            settings: MouseGestureOverlaySettings::default(),
            window: Box::<NoopOverlayWindow>::default(),
        }
    }

    pub fn update_settings(&mut self, plugin_settings: &MouseGesturePluginSettings) {
        self.settings = plugin_settings.overlay.clone();
        self.window.update_settings(&self.settings);
    }

    pub fn begin_stroke(&mut self, start: Point) {
        self.window.begin_stroke(&self.settings, start);
    }

    pub fn push_point(&mut self, point: Point) {
        self.window.push_point(&self.settings, point);
    }

    pub fn end_stroke(&mut self) {
        self.window.end_stroke(&self.settings);
    }
}

impl Default for StrokeOverlay {
    fn default() -> Self {
        Self::new()
    }
}

static OVERLAY: OnceCell<Arc<Mutex<StrokeOverlay>>> = OnceCell::new();

pub fn mouse_gesture_overlay() -> Arc<Mutex<StrokeOverlay>> {
    OVERLAY
        .get_or_init(|| Arc::new(Mutex::new(StrokeOverlay::new())))
        .clone()
}
