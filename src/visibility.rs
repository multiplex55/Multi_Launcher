use eframe::egui;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

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

/// Controls whether making the launcher visible also reapplies its configured
/// placement. Restoring an already-visible launcher must preserve any geometry
/// changes made during the current visible session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VisiblePlacementPolicy {
    ApplyConfiguredPlacement,
    PreserveCurrentGeometry,
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
    follow_mouse: bool,
    static_enabled: bool,
    static_pos: Option<(f32, f32)>,
    static_size: Option<(f32, f32)>,
    window_size: (f32, f32),
) -> bool {
    let mut changed = false;
    if trigger.take() {
        let old = visibility.load(Ordering::SeqCst);
        let next = !old;
        tracing::debug!(from=?old, to=?next, "visibility updated");
        visibility.store(next, Ordering::SeqCst);
        changed = old != next;
        if let Ok(guard) = ctx_handle.lock() {
            if let Some(c) = &*guard {
                apply_visibility(
                    next,
                    VisiblePlacementPolicy::ApplyConfiguredPlacement,
                    c,
                    offscreen,
                    follow_mouse,
                    static_enabled,
                    static_pos,
                    static_size,
                    window_size,
                );
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
        if let Ok(guard) = ctx_handle.lock()
            && let Some(c) = &*guard
        {
            let old = visibility.load(Ordering::SeqCst);
            visibility.store(next, Ordering::SeqCst);
            changed = old != next;
            tracing::debug!(from=?old, to=?next, "visibility updated");
            apply_visibility(
                next,
                VisiblePlacementPolicy::ApplyConfiguredPlacement,
                c,
                offscreen,
                follow_mouse,
                static_enabled,
                static_pos,
                static_size,
                window_size,
            );
            if next {
                restore_flag.store(true, Ordering::SeqCst);
            }
            *queued_visibility = None;
            tracing::debug!("Applied queued visibility: {}", next);
        }
    }
    changed
}

/// Apply the current visibility state to the viewport.
pub fn apply_visibility<C: ViewportCtx>(
    visible: bool,
    placement_policy: VisiblePlacementPolicy,
    ctx: &C,
    offscreen: (f32, f32),
    follow_mouse: bool,
    static_enabled: bool,
    static_pos: Option<(f32, f32)>,
    static_size: Option<(f32, f32)>,
    window_size: (f32, f32),
) {
    if visible {
        if placement_policy == VisiblePlacementPolicy::ApplyConfiguredPlacement {
            if static_enabled {
                if let Some((x, y)) = static_pos {
                    ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(x, y)));
                }
                if let Some((w, h)) = static_size {
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(w, h)));
                }
            } else if follow_mouse
                && let Some((x, y)) = crate::window_manager::current_mouse_position()
            {
                let pos_x = x - window_size.0 / 2.0;
                let pos_y = y - window_size.1 / 2.0;
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
                    pos_x, pos_y,
                )));
            }
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    } else {
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
            offscreen.0,
            offscreen.1,
        )));
    }
    ctx.request_repaint();
}

/// Hides the root viewport specifically for Screen Draw capture. Ordinary
/// launcher hiding continues to use the established off-screen placement
/// behavior in [`apply_visibility`]; capture exclusion additionally needs the
/// native viewport to become non-visible before pixels are read.
pub fn hide_root_for_screen_draw_capture<C: ViewportCtx>(ctx: &C) {
    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
    ctx.request_repaint();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct RecordingCtx {
        commands: Mutex<Vec<egui::ViewportCommand>>,
        repaints: AtomicBool,
    }

    impl ViewportCtx for RecordingCtx {
        fn send_viewport_cmd(&self, command: egui::ViewportCommand) {
            self.commands.lock().unwrap().push(command);
        }

        fn request_repaint(&self) {
            self.repaints.store(true, Ordering::SeqCst);
        }
    }

    #[test]
    fn screen_draw_hide_uses_native_viewport_visibility_without_ordinary_placement() {
        let ctx = RecordingCtx::default();
        hide_root_for_screen_draw_capture(&ctx);
        let commands = ctx.commands.lock().unwrap();
        assert_eq!(commands.len(), 1);
        assert!(matches!(commands[0], egui::ViewportCommand::Visible(false)));
        assert!(ctx.repaints.load(Ordering::SeqCst));
    }
}
