use eframe::egui;

#[derive(Default)]
pub struct MouseGesturesAddDialog {
    pub open: bool,
}

impl MouseGesturesAddDialog {
    pub fn open(&mut self) {
        self.open = true;
    }

    pub fn ui(&mut self, ctx: &egui::Context, _app: &mut crate::gui::LauncherApp) {
        if !self.open {
            return;
        }
        egui::Window::new("Add Mouse Gesture Binding")
            .open(&mut self.open)
            .show(ctx, |ui| {
                ui.label("Create a new gesture binding.");
                ui.label("Binding editor UI is coming soon.");
            });
    }
}
