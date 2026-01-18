use eframe::egui;

#[derive(Default)]
pub struct MouseGesturesGestureDialog {
    pub open: bool,
}

impl MouseGesturesGestureDialog {
    pub fn open(&mut self) {
        self.open = true;
    }

    pub fn ui(&mut self, ctx: &egui::Context, _app: &mut crate::gui::LauncherApp) {
        if !self.open {
            return;
        }
        egui::Window::new("Mouse Gesture Recorder")
            .open(&mut self.open)
            .show(ctx, |ui| {
                ui.label("Record a new mouse gesture.");
                ui.label("Gesture recording UI is coming soon.");
            });
    }
}
