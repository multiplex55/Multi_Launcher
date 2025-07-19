use crate::gui::LauncherApp;
use crate::launcher::launch_action;
use crate::actions::Action;
use eframe::egui;

#[derive(Default)]
pub struct VolumeDialog {
    pub open: bool,
    value: u8,
}

impl VolumeDialog {
    pub fn open(&mut self) {
        self.open = true;
        self.value = 50;
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open { return; }
        let mut close = false;
        egui::Window::new("Volume")
            .resizable(false)
            .open(&mut self.open)
            .show(ctx, |ui| {
                ui.add(egui::Slider::new(&mut self.value, 0..=100).text("Level"));
                ui.horizontal(|ui| {
                    if ui.button("Set").clicked() {
                        let _ = launch_action(&Action {
                            label: String::new(),
                            desc: "Volume".into(),
                            action: format!("volume:set:{}", self.value),
                            args: None,
                        });
                        close = true;
                        app.focus_input();
                    }
                    if ui.button("Mute").clicked() {
                        let _ = launch_action(&Action {
                            label: String::new(),
                            desc: "Volume".into(),
                            action: "volume:mute_active".into(),
                            args: None,
                        });
                        close = true;
                        app.focus_input();
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });
        if close { self.open = false; }
    }
}
