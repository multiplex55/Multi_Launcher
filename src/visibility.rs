use eframe::egui;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

use crate::hotkey::HotkeyTrigger;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisibilityIntent {
    Toggle,
    Show,
}

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
    follow_mouse: bool,
    static_enabled: bool,
    static_pos: Option<(f32, f32)>,
    static_size: Option<(f32, f32)>,
    window_size: (f32, f32),
) -> bool {
    process_visibility_trigger(
        trigger.take(),
        visibility,
        restore_flag,
        ctx_handle,
        queued_visibility,
        offscreen,
        follow_mouse,
        static_enabled,
        static_pos,
        static_size,
        window_size,
    )
}

/// Process launcher hotkey with draw-runtime guard. If draw is active, exit is
/// requested and launcher visibility is left unchanged.
pub fn handle_visibility_trigger_with_draw_guard<C, A, E>(
    trigger: &HotkeyTrigger,
    visibility: &Arc<AtomicBool>,
    restore_flag: &Arc<AtomicBool>,
    ctx_handle: &Arc<Mutex<Option<C>>>,
    queued_visibility: &mut Option<bool>,
    intent: VisibilityIntent,
    is_draw_active: A,
    mut request_draw_exit: E,
    offscreen: (f32, f32),
    follow_mouse: bool,
    static_enabled: bool,
    static_pos: Option<(f32, f32)>,
    static_size: Option<(f32, f32)>,
    window_size: (f32, f32),
) -> bool
where
    C: ViewportCtx,
    A: FnOnce() -> bool,
    E: FnMut(),
{
    let trigger_fired = trigger.take();
    if trigger_fired && is_draw_active() {
        let visible = visibility.load(Ordering::SeqCst);
        let next = resolve_visibility_intent(intent, visible);
        *queued_visibility = Some(next);
        if next {
            restore_flag.store(true, Ordering::SeqCst);
        }
        request_draw_exit();
        return false;
    }

    process_visibility_trigger(
        trigger_fired,
        visibility,
        restore_flag,
        ctx_handle,
        queued_visibility,
        offscreen,
        follow_mouse,
        static_enabled,
        static_pos,
        static_size,
        window_size,
    )
}

fn resolve_visibility_intent(intent: VisibilityIntent, current_visible: bool) -> bool {
    match intent {
        VisibilityIntent::Toggle => !current_visible,
        VisibilityIntent::Show => true,
    }
}

fn process_visibility_trigger<C: ViewportCtx>(
    trigger_fired: bool,
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
    if trigger_fired {
        let old = visibility.load(Ordering::SeqCst);
        let next = !old;
        tracing::debug!(from=?old, to=?next, "visibility updated");
        visibility.store(next, Ordering::SeqCst);
        changed = old != next;
        if let Ok(guard) = ctx_handle.lock() {
            if let Some(c) = &*guard {
                apply_visibility(
                    next,
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
        if let Ok(guard) = ctx_handle.lock() {
            if let Some(c) = &*guard {
                let old = visibility.load(Ordering::SeqCst);
                visibility.store(next, Ordering::SeqCst);
                changed = old != next;
                tracing::debug!(from=?old, to=?next, "visibility updated");
                apply_visibility(
                    next,
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
    }
    changed
}

/// Apply the current visibility state to the viewport.
pub fn apply_visibility<C: ViewportCtx>(
    visible: bool,
    ctx: &C,
    offscreen: (f32, f32),
    follow_mouse: bool,
    static_enabled: bool,
    static_pos: Option<(f32, f32)>,
    static_size: Option<(f32, f32)>,
    window_size: (f32, f32),
) {
    if visible {
        if static_enabled {
            if let Some((x, y)) = static_pos {
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(x, y)));
            }
            if let Some((w, h)) = static_size {
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(w, h)));
            }
        } else if follow_mouse {
            if let Some((x, y)) = crate::window_manager::current_mouse_position() {
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

#[cfg(test)]
mod tests {
    use super::{handle_visibility_trigger_with_draw_guard, ViewportCtx, VisibilityIntent};
    use eframe::egui;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    };

    #[derive(Default)]
    struct FakeViewportCtx;

    impl ViewportCtx for FakeViewportCtx {
        fn send_viewport_cmd(&self, _cmd: egui::ViewportCommand) {}

        fn request_repaint(&self) {}
    }

    fn trigger_fire_once() -> crate::hotkey::HotkeyTrigger {
        let hotkey = crate::hotkey::parse_hotkey("ctrl+shift+space").expect("valid hotkey");
        let trigger = crate::hotkey::HotkeyTrigger::new(hotkey);
        if let Ok(mut open) = trigger.open.lock() {
            *open = true;
        }
        trigger
    }

    #[test]
    fn draw_active_requests_exit_without_toggling_visibility() {
        let trigger = trigger_fire_once();
        let visibility = Arc::new(AtomicBool::new(true));
        let restore_flag = Arc::new(AtomicBool::new(false));
        let ctx_handle = Arc::new(Mutex::new(Some(FakeViewportCtx)));
        let mut queued_visibility = None;
        let exit_requested = Arc::new(AtomicBool::new(false));
        let exit_requested_clone = Arc::clone(&exit_requested);

        let changed = handle_visibility_trigger_with_draw_guard(
            &trigger,
            &visibility,
            &restore_flag,
            &ctx_handle,
            &mut queued_visibility,
            VisibilityIntent::Show,
            || true,
            move || exit_requested_clone.store(true, Ordering::SeqCst),
            (2000.0, 2000.0),
            false,
            false,
            None,
            None,
            (400.0, 220.0),
        );

        assert!(!changed);
        assert!(visibility.load(Ordering::SeqCst));
        assert!(exit_requested.load(Ordering::SeqCst));
        assert_eq!(queued_visibility, Some(true));
    }

    #[test]
    fn queued_show_intent_applies_after_draw_exit() {
        let trigger = trigger_fire_once();
        let visibility = Arc::new(AtomicBool::new(false));
        let restore_flag = Arc::new(AtomicBool::new(false));
        let ctx_handle = Arc::new(Mutex::new(Some(FakeViewportCtx)));
        let mut queued_visibility = None;

        let changed = handle_visibility_trigger_with_draw_guard(
            &trigger,
            &visibility,
            &restore_flag,
            &ctx_handle,
            &mut queued_visibility,
            VisibilityIntent::Show,
            || true,
            || {},
            (2000.0, 2000.0),
            false,
            false,
            None,
            None,
            (400.0, 220.0),
        );

        assert!(!changed);
        assert_eq!(queued_visibility, Some(true));
        assert!(!visibility.load(Ordering::SeqCst));

        let changed = handle_visibility_trigger_with_draw_guard(
            &trigger,
            &visibility,
            &restore_flag,
            &ctx_handle,
            &mut queued_visibility,
            VisibilityIntent::Show,
            || false,
            || {},
            (2000.0, 2000.0),
            false,
            false,
            None,
            None,
            (400.0, 220.0),
        );

        assert!(changed);
        assert!(visibility.load(Ordering::SeqCst));
        assert_eq!(queued_visibility, None);
    }

    #[test]
    fn draw_inactive_keeps_existing_visibility_toggle_behavior() {
        let trigger = trigger_fire_once();
        let visibility = Arc::new(AtomicBool::new(true));
        let restore_flag = Arc::new(AtomicBool::new(false));
        let ctx_handle = Arc::new(Mutex::new(Some(FakeViewportCtx)));
        let mut queued_visibility = None;
        let exit_requested = Arc::new(AtomicBool::new(false));
        let exit_requested_clone = Arc::clone(&exit_requested);

        let changed = handle_visibility_trigger_with_draw_guard(
            &trigger,
            &visibility,
            &restore_flag,
            &ctx_handle,
            &mut queued_visibility,
            VisibilityIntent::Toggle,
            || false,
            move || exit_requested_clone.store(true, Ordering::SeqCst),
            (2000.0, 2000.0),
            false,
            false,
            None,
            None,
            (400.0, 220.0),
        );

        assert!(changed);
        assert!(!visibility.load(Ordering::SeqCst));
        assert!(!exit_requested.load(Ordering::SeqCst));
    }
}
