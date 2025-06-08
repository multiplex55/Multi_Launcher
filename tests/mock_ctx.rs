use eframe::egui;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct MockCtx {
    pub commands: Arc<Mutex<Vec<egui::ViewportCommand>>>,
}

impl MockCtx {
    pub fn send_viewport_cmd(&self, cmd: egui::ViewportCommand) {
        self.commands.lock().unwrap().push(cmd);
    }

    pub fn request_repaint(&self) {}
}

// Implement the trait from the main crate so tests can reuse visibility logic.
impl multi_launcher::visibility::ViewportCtx for MockCtx {
    fn send_viewport_cmd(&self, cmd: egui::ViewportCommand) {
        self.send_viewport_cmd(cmd);
    }

    fn request_repaint(&self) {
        self.request_repaint();
    }
}
