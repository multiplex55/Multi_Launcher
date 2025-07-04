use eframe::egui;
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};

use crate::hotkey::HotkeyTrigger;
use poll_promise::Promise;
use std::time::Duration;


/// Trait abstracting over an `egui::Context` for viewport commands.
pub trait ViewportCtx {
    fn send_viewport_cmd(&self, cmd: egui::ViewportCommand);
    fn request_repaint(&self);
}

impl ViewportCtx for egui::Context {
    fn send_viewport_cmd(&self, cmd: egui::ViewportCommand) {
        egui::Context::send_viewport_cmd(self, cmd);
    }

    fn request_repaint(&self) {
        egui::Context::request_repaint(self);
    }
}

/// Process a hotkey trigger and update visibility, issuing viewport commands
/// when possible. This mirrors the logic from `main.rs`.
pub fn handle_visibility_trigger<C: ViewportCtx>(
    trigger: &HotkeyTrigger,
    visibility: &Arc<AtomicBool>,
    ctx_handle: &Arc<Mutex<Option<C>>>,
    queued_visibility: &mut Option<bool>,
) {
    if trigger.take() {
        let old = visibility.load(Ordering::SeqCst);
        let next = !old;
        tracing::debug!(from=?old, to=?next, "visibility updated");
        visibility.store(next, Ordering::SeqCst);
        if let Ok(guard) = ctx_handle.lock() {
            if let Some(c) = &*guard {
                c.send_viewport_cmd(egui::ViewportCommand::Visible(next));
                c.request_repaint();
                *queued_visibility = None;
                tracing::debug!("Applied queued visibility: {}", next);
            } else {
                *queued_visibility = Some(next);
            }
        } else {
            *queued_visibility = Some(next);
        }
    } else if let Some(next) = *queued_visibility {
        tracing::debug!("Processing previously queued visibility: {}", next);
        if let Ok(guard) = ctx_handle.lock() {
            if let Some(c) = &*guard {
                let old = visibility.load(Ordering::SeqCst);
                visibility.store(next, Ordering::SeqCst);
                tracing::debug!(from=?old, to=?next, "visibility updated");
                c.send_viewport_cmd(egui::ViewportCommand::Visible(next));
                c.request_repaint();
                *queued_visibility = None;
                tracing::debug!("Applied queued visibility: {}", next);
            }
        }
    }
}

pub struct HotkeyPoller {
    stop: Arc<AtomicBool>,
    promise: Promise<()>,
}

pub fn start_hotkey_poller(trigger: Arc<HotkeyTrigger>) -> HotkeyPoller {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_clone = stop.clone();
    let promise = Promise::spawn_thread("hotkey_poller", move || {
        while !stop_clone.load(Ordering::SeqCst) {
            if trigger.take() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    });
    HotkeyPoller { stop, promise }
}

impl HotkeyPoller {
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }

    pub fn ready(&self) -> bool {
        self.promise.ready().is_some()
    }
}

