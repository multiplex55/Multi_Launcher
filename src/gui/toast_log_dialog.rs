use crate::toast_log::TOAST_LOG_FILE;
use eframe::egui;

#[derive(Default)]
pub struct ToastLogDialog {
    pub open: bool,
    lines: Vec<String>,
}

impl ToastLogDialog {
    pub fn open(&mut self) {
        self.lines = Self::read_last_lines(TOAST_LOG_FILE, 20);
        self.open = true;
    }

    fn read_last_lines(path: &str, count: usize) -> Vec<String> {
        if let Ok(content) = std::fs::read_to_string(path) {
            let mut lines: Vec<String> = content.lines().map(|s| s.to_owned()).collect();
            if lines.len() > count {
                lines.drain(0..lines.len() - count);
            }
            lines
        } else {
            Vec::new()
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, _app: &mut crate::gui::LauncherApp) {
        if !self.open {
            return;
        }
        let mut close = false;
        egui::Window::new("Toast Log")
            .resizable(true)
            .default_size((360.0, 200.0))
            .open(&mut self.open)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for line in &self.lines {
                        ui.label(line);
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("Refresh").clicked() {
                        self.lines = Self::read_last_lines(TOAST_LOG_FILE, 20);
                    }
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                });
            });
        if close {
            self.open = false;
        }
    }
}
