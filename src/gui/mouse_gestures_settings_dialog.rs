use eframe::egui;

#[derive(Default)]
pub struct MouseGesturesSettingsDialog {
    pub open: bool,
}

impl MouseGesturesSettingsDialog {
    pub fn open(&mut self) {
        self.open = true;
    }

    pub fn ui(&mut self, ctx: &egui::Context, _app: &mut crate::gui::LauncherApp) {
        if !self.open {
            return;
        }
        egui::Window::new("Mouse Gestures Settings")
            .open(&mut self.open)
            .show(ctx, |ui| {
                ui.label("Configure mouse gesture bindings and preferences.");
                ui.label("Settings UI is coming soon.");
            });
    }
}
