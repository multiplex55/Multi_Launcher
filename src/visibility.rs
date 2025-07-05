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

/// Process a hotkey trigger and update the minimized state, issuing viewport
/// commands when possible. This mirrors the logic from `main.rs`.
pub fn handle_visibility_trigger<C: ViewportCtx>(
    trigger: &HotkeyTrigger,
    visibility: &Arc<AtomicBool>,
    restore_flag: &Arc<AtomicBool>,
    ctx_handle: &Arc<Mutex<Option<C>>>,
    queued_visibility: &mut Option<bool>,
    offscreen: (f32, f32),
) {
    if trigger.take() {
        let old = visibility.load(Ordering::SeqCst);
        let next = !old;
        tracing::debug!(from=?old, to=?next, "visibility updated");
        visibility.store(next, Ordering::SeqCst);
        if let Ok(guard) = ctx_handle.lock() {
            if let Some(c) = &*guard {
                apply_visibility(next, c, offscreen);
                if next {
                    restore_flag.store(true, Ordering::SeqCst);
                }
                *queued_visibility = None;
                tracing::debug!("Applied queued visibility: {}", next);
            } else {
                *queued_visibility = Some(next);
                if next {
                    restore_flag.store(true, Ordering::SeqCst);
                }
            }
        } else {
            *queued_visibility = Some(next);
            if next {
                restore_flag.store(true, Ordering::SeqCst);
            }
        }
    } else if let Some(next) = *queued_visibility {
        tracing::debug!("Processing previously queued visibility: {}", next);
        if let Ok(guard) = ctx_handle.lock() {
            if let Some(c) = &*guard {
                let old = visibility.load(Ordering::SeqCst);
                visibility.store(next, Ordering::SeqCst);
                tracing::debug!(from=?old, to=?next, "visibility updated");
                apply_visibility(next, c, offscreen);
                if next {
                    restore_flag.store(true, Ordering::SeqCst);
                }
                *queued_visibility = None;
                tracing::debug!("Applied queued visibility: {}", next);
            }
        }
    }
}

/// Apply the current visibility state to the viewport.
pub fn apply_visibility<C: ViewportCtx>(visible: bool, ctx: &C, offscreen: (f32, f32)) {
    if visible {
        if let Some((x, y)) = crate::window_manager::current_mouse_position() {
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(x, y)));
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    } else {
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(offscreen.0, offscreen.1)));
    }
    ctx.request_repaint();
}

