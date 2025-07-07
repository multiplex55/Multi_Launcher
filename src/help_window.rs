use crate::gui::LauncherApp;
use eframe::egui;

#[derive(Default)]
pub struct HelpWindow {
    pub open: bool,
}

impl HelpWindow {
    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        egui::Window::new("Command Help")
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label("Available commands:");
                for (_, desc, _) in app.plugins.plugin_infos() {
                    ui.label(desc);
                }
            });
        self.open = open;
    }
}
