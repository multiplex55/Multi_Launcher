use eframe::egui;
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};

use crate::hotkey::HotkeyTrigger;


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
                apply_visibility(c, trigger, next);
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
                apply_visibility(c, trigger, next);
                *queued_visibility = None;
                tracing::debug!("Applied queued visibility: {}", next);
            }
        }
    }
}

fn apply_visibility<C: ViewportCtx>(ctx: &C, trigger: &HotkeyTrigger, visible: bool) {
    if visible {
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        let (x, y) = trigger.mouse_pos();
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::Pos2::new(x as f32, y as f32)));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    } else {
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
    }
    ctx.request_repaint();
}

