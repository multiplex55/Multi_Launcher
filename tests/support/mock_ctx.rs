use eframe::egui;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

#[derive(Clone, Default)]
pub struct MockCtx {
    pub commands: Arc<Mutex<Vec<egui::ViewportCommand>>>,
    pub repaint_requests: Arc<AtomicUsize>,
}

impl MockCtx {
    pub fn send_viewport_cmd(&self, cmd: egui::ViewportCommand) {
        self.commands.lock().unwrap().push(cmd);
    }

    pub fn request_repaint(&self) {
        self.repaint_requests.fetch_add(1, Ordering::SeqCst);
    }
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
