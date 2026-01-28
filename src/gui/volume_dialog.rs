use crate::actions::Action;
use crate::gui::volume_data::{get_process_volumes, get_system_volume, ProcessVolume};
use crate::gui::LauncherApp;
use crate::launcher::launch_action;
use eframe::egui;

pub struct VolumeDialog {
    pub open: bool,
    value: u8,
    processes: Vec<ProcessVolume>,
}

impl Default for VolumeDialog {
    fn default() -> Self {
        Self {
            open: false,
            value: 50,
            processes: Vec::new(),
        }
    }
}

impl VolumeDialog {
    pub fn open(&mut self) {
        self.open = true;
        self.value = get_system_volume().unwrap_or(50);
        self.processes = get_process_volumes();
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open {
            return;
        }
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
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
                ui.separator();
                for proc in &mut self.processes {
                    ui.horizontal(|ui| {
                        ui.label(format!("{} (PID {})", proc.name, proc.pid));
                        let resp =
                            ui.add(egui::Slider::new(&mut proc.value, 0..=100).text("Level"));
                        if resp.changed() {
                            if let Some(action) = proc.slider_changed() {
                                let _ = launch_action(&Action {
                                    label: String::new(),
                                    desc: "Volume".into(),
                                    action,
                                    args: None,
                                });
                            }
                        }
                        if ui.button("Set").clicked() {
                            let _ = launch_action(&Action {
                                label: String::new(),
                                desc: "Volume".into(),
                                action: format!("volume:pid:{}:{}", proc.pid, proc.value),
                                args: None,
                            });
                        }
                        if ui.button("Mute").clicked() {
                            let _ = launch_action(&Action {
                                label: String::new(),
                                desc: "Volume".into(),
                                action: format!("volume:pid_toggle_mute:{}", proc.pid),
                                args: None,
                            });
                            proc.muted = !proc.muted;
                        }
                        if proc.muted {
                            ui.colored_label(egui::Color32::RED, "muted");
                        }
                    });
                }
            });
        if close {
            self.open = false;
        }
    }
}
