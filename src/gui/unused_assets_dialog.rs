use crate::gui::LauncherApp;
use crate::plugins::note::{assets_dir, unused_assets};
use eframe::egui;

#[derive(Default)]
pub struct UnusedAssetsDialog {
    pub open: bool,
    assets: Vec<String>,
    selected: Vec<bool>,
}

impl UnusedAssetsDialog {
    pub fn open(&mut self) {
        self.assets = unused_assets();
        self.selected = vec![false; self.assets.len()];
        self.open = true;
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open {
            return;
        }
        let mut open_flag = self.open;
        let mut close = false;
        egui::Window::new("Unused Note Assets")
            .open(&mut open_flag)
            .resizable(true)
            .default_size((300.0, 200.0))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(200.0)
                    .show(ui, |ui| {
                        if self.assets.is_empty() {
                            ui.label("No unused assets found");
                        } else {
                            for (idx, asset) in self.assets.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    ui.checkbox(&mut self.selected[idx], "");
                                    ui.label(asset);
                                });
                            }
                        }
                    });
                ui.separator();
                if ui.button("Delete Selected").clicked() {
                    for i in (0..self.assets.len()).rev() {
                        if self.selected[i] {
                            let path = assets_dir().join(&self.assets[i]);
                            if let Err(e) = std::fs::remove_file(&path) {
                                app.set_error(format!("Failed to delete {}: {e}", self.assets[i]));
                            } else {
                                self.assets.remove(i);
                                self.selected.remove(i);
                            }
                        }
                    }
                }
                if ui.button("Close").clicked() {
                    close = true;
                }
            });
        if close {
            open_flag = false;
        }
        self.open = open_flag;
    }
}
