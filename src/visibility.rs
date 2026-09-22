use eframe::egui;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use crate::hotkey::HotkeyTrigger;
use crate::radial::acceptance_trace::{self, Event, RootCommandKind, VisibilitySource};

/// A small, explicit wake boundary for work owned by one egui viewport.
/// Keeping the viewport id with the callback prevents background producers
/// from accidentally waking whichever viewport happened to be current on the
/// GUI thread.
#[derive(Clone)]
pub struct ViewportWake {
    viewport: egui::ViewportId,
    request: Arc<dyn Fn(egui::ViewportId) + Send + Sync>,
}

impl ViewportWake {
    pub fn for_context(ctx: &egui::Context, viewport: egui::ViewportId) -> Self {
        let ctx = ctx.clone();
        Self {
            viewport,
            request: Arc::new(move |viewport| ctx.request_repaint_of(viewport)),
        }
    }

    pub fn root(ctx: &egui::Context) -> Self {
        Self::for_context(ctx, egui::ViewportId::ROOT)
    }

    #[cfg(test)]
    pub(crate) fn from_callback(
        viewport: egui::ViewportId,
        request: impl Fn(egui::ViewportId) + Send + Sync + 'static,
    ) -> Self {
        Self {
            viewport,
            request: Arc::new(request),
        }
    }

    pub(crate) fn wake(&self) {
        (self.request)(self.viewport);
    }
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

/// Controls whether making the launcher visible also reapplies its configured
/// placement. Restoring an already-visible launcher must preserve any geometry
/// changes made during the current visible session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VisiblePlacementPolicy {
    ApplyConfiguredPlacement,
    PreserveCurrentGeometry,
}

/// Ordered visibility toggles accumulated while one main-loop batch is being
/// routed.  `record_toggle` returns the state immediately before that toggle,
/// allowing other owners (such as radial keyboard scope) to follow each edge
/// even though viewport commands are applied later in the loop.
#[derive(Clone, Debug, Default)]
pub struct VisibilityToggleBatch {
    targets: Vec<bool>,
}

impl VisibilityToggleBatch {
    pub fn record_toggle(&mut self, visibility: &AtomicBool) -> bool {
        let was_visible = visibility.load(Ordering::SeqCst);
        let next_visible = !was_visible;
        self.targets.push(next_visible);
        visibility.store(next_visible, Ordering::SeqCst);
        acceptance_trace::emit(Event::DesiredVisibility {
            visible: next_visible,
            source: VisibilitySource::ToggleBatch,
        });
        was_visible
    }

    pub fn final_visible(&self) -> Option<bool> {
        self.targets.last().copied()
    }
}

/// A root-bound command boundary for visibility work issued outside the root
/// viewport's own frame callback.  An `egui::Context` can be shared by the
/// root and deferred child viewports; its unqualified command methods target
/// whichever viewport is current at the call site.  Visibility ownership is
/// always the root launcher, so make that target explicit here.
#[derive(Clone)]
pub struct RootViewportCtx {
    ctx: egui::Context,
}

impl RootViewportCtx {
    pub fn new(ctx: &egui::Context) -> Self {
        Self { ctx: ctx.clone() }
    }

    #[cfg(test)]
    fn viewport_id(&self) -> egui::ViewportId {
        egui::ViewportId::ROOT
    }
}

impl ViewportCtx for RootViewportCtx {
    fn send_viewport_cmd(&self, cmd: egui::ViewportCommand) {
        let command = match &cmd {
            egui::ViewportCommand::OuterPosition(_) => RootCommandKind::Position,
            egui::ViewportCommand::InnerSize(_) => RootCommandKind::Size,
            egui::ViewportCommand::Visible(true) => RootCommandKind::Show,
            egui::ViewportCommand::Visible(false) => RootCommandKind::ParkingBoundary,
            egui::ViewportCommand::Minimized(_) => RootCommandKind::Minimize,
            egui::ViewportCommand::Focus => RootCommandKind::Focus,
            _ => return self.ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, cmd),
        };
        acceptance_trace::emit(Event::RootCommand { command });
        self.ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, cmd);
    }

    fn request_repaint(&self) {
        self.ctx.request_repaint_of(egui::ViewportId::ROOT);
    }
}

/// Apply every queued toggle in order. This deliberately applies each edge,
/// rather than reducing a batch to its final parity, so viewport side effects
/// and owners that follow those edges remain synchronized.
pub fn handle_visibility_toggle_batch<C: ViewportCtx>(
    batch: &VisibilityToggleBatch,
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
    for next in &batch.targets {
        apply_visibility_owner(
            *next,
            restore_flag,
            ctx_handle,
            queued_visibility,
            offscreen,
            follow_mouse,
            static_enabled,
            static_pos,
            static_size,
            window_size,
        );
    }
    !batch.targets.is_empty()
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
    handle_visibility_trigger_with_owner(
        trigger,
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
        |_| {},
    )
}

pub fn handle_visibility_trigger_with_owner<C: ViewportCtx>(
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
    mut on_grid_toggle: impl FnMut(bool),
) -> bool {
    let mut changed = false;
    if trigger.take() {
        let old = visibility.load(Ordering::SeqCst);
        let next = !old;
        acceptance_trace::emit(Event::DesiredVisibility {
            visible: next,
            source: VisibilitySource::LegacyTrigger,
        });
        on_grid_toggle(old);
        changed = apply_visibility_target(
            next,
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
        );
    } else if let Some(next) = *queued_visibility {
        acceptance_trace::emit(Event::DesiredVisibility {
            visible: next,
            source: VisibilitySource::Queued,
        });
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
            restore_flag.store(next, Ordering::SeqCst);
            *queued_visibility = None;
            tracing::debug!("Applied queued visibility: {}", next);
        }
    }
    changed
}

fn apply_visibility_target<C: ViewportCtx>(
    next: bool,
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
    let old = visibility.load(Ordering::SeqCst);
    visibility.store(next, Ordering::SeqCst);
    tracing::debug!(from=?old, to=?next, "visibility updated");
    apply_visibility_owner(
        next,
        restore_flag,
        ctx_handle,
        queued_visibility,
        offscreen,
        follow_mouse,
        static_enabled,
        static_pos,
        static_size,
        window_size,
    );
    old != next
}

fn apply_visibility_owner<C: ViewportCtx>(
    next: bool,
    restore_flag: &Arc<AtomicBool>,
    ctx_handle: &Arc<Mutex<Option<C>>>,
    queued_visibility: &mut Option<bool>,
    offscreen: (f32, f32),
    follow_mouse: bool,
    static_enabled: bool,
    static_pos: Option<(f32, f32)>,
    static_size: Option<(f32, f32)>,
    window_size: (f32, f32),
) {
    if let Ok(guard) = ctx_handle.lock() {
        if let Some(ctx) = &*guard {
            apply_visibility(
                next,
                VisiblePlacementPolicy::ApplyConfiguredPlacement,
                ctx,
                offscreen,
                follow_mouse,
                static_enabled,
                static_pos,
                static_size,
                window_size,
            );
            restore_flag.store(next, Ordering::SeqCst);
            *queued_visibility = None;
            tracing::debug!("Applied queued visibility: {}", next);
        } else {
            *queued_visibility = Some(next);
            restore_flag.store(next, Ordering::SeqCst);
        }
    } else {
        *queued_visibility = Some(next);
        restore_flag.store(next, Ordering::SeqCst);
    }
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
        acceptance_trace::emit(Event::RootCommand {
            command: RootCommandKind::ParkingBoundary,
        });
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
            offscreen.0,
            offscreen.1,
        )));
    }
    ctx.request_repaint();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hotkey::parse_hotkey;
    use std::sync::atomic::AtomicUsize;

    #[derive(Clone, Default)]
    struct RecordingViewport {
        commands: Arc<Mutex<Vec<egui::ViewportCommand>>>,
        repaint_count: Arc<AtomicUsize>,
    }

    impl ViewportCtx for RecordingViewport {
        fn send_viewport_cmd(&self, cmd: egui::ViewportCommand) {
            self.commands.lock().unwrap().push(cmd);
        }

        fn request_repaint(&self) {
            self.repaint_count.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn trigger() -> HotkeyTrigger {
        HotkeyTrigger::new(parse_hotkey("End").expect("test hotkey parses"))
    }

    fn toggle(trigger: &HotkeyTrigger) {
        *trigger.open.lock().unwrap() = true;
    }

    fn handle(
        trigger: &HotkeyTrigger,
        visibility: &Arc<AtomicBool>,
        restore_flag: &Arc<AtomicBool>,
        ctx: &Arc<Mutex<Option<RecordingViewport>>>,
        queued_visibility: &mut Option<bool>,
    ) {
        handle_visibility_trigger(
            trigger,
            visibility,
            restore_flag,
            ctx,
            queued_visibility,
            (-10_000.0, -10_000.0),
            false,
            false,
            None,
            None,
            (400.0, 220.0),
        );
    }

    #[test]
    fn root_visibility_boundary_targets_root_from_child_context() {
        let ctx = egui::Context::default();
        let child_id = egui::ViewportId::from_hash_of("designer-test");
        ctx.set_embed_viewports(false);
        ctx.show_viewport_deferred(
            child_id,
            egui::ViewportBuilder::default(),
            |_child, _class| {},
        );
        let mut input = egui::RawInput::default();
        input.viewport_id = child_id;
        input.viewports.insert(
            child_id,
            egui::ViewportInfo {
                parent: Some(egui::ViewportId::ROOT),
                ..Default::default()
            },
        );
        let _ = ctx.run(input, |child| {
            assert_eq!(child.viewport_id(), child_id);
            let root = RootViewportCtx::new(child);
            assert_eq!(root.viewport_id(), egui::ViewportId::ROOT);
            root.request_repaint();
            assert!(child.has_requested_repaint_for(&egui::ViewportId::ROOT));
            root.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        });
    }

    #[test]
    fn hide_before_the_next_frame_invalidates_show_restore() {
        let trigger = trigger();
        let visibility = Arc::new(AtomicBool::new(false));
        let restore_flag = Arc::new(AtomicBool::new(false));
        let viewport = RecordingViewport::default();
        let ctx = Arc::new(Mutex::new(Some(viewport)));
        let mut queued_visibility = None;

        toggle(&trigger);
        handle(
            &trigger,
            &visibility,
            &restore_flag,
            &ctx,
            &mut queued_visibility,
        );
        assert!(visibility.load(Ordering::SeqCst));
        assert!(restore_flag.load(Ordering::SeqCst));

        toggle(&trigger);
        handle(
            &trigger,
            &visibility,
            &restore_flag,
            &ctx,
            &mut queued_visibility,
        );
        assert!(!visibility.load(Ordering::SeqCst));
        assert!(!restore_flag.load(Ordering::SeqCst));
        assert!(queued_visibility.is_none());
    }

    #[test]
    fn queued_hide_replaces_show_and_does_not_restore_when_context_attaches() {
        let trigger = trigger();
        let visibility = Arc::new(AtomicBool::new(false));
        let restore_flag = Arc::new(AtomicBool::new(false));
        let ctx = Arc::new(Mutex::new(None));
        let mut queued_visibility = None;

        toggle(&trigger);
        handle(
            &trigger,
            &visibility,
            &restore_flag,
            &ctx,
            &mut queued_visibility,
        );
        assert_eq!(queued_visibility, Some(true));
        assert!(restore_flag.load(Ordering::SeqCst));

        toggle(&trigger);
        handle(
            &trigger,
            &visibility,
            &restore_flag,
            &ctx,
            &mut queued_visibility,
        );
        assert_eq!(queued_visibility, Some(false));
        assert!(!restore_flag.load(Ordering::SeqCst));

        *ctx.lock().unwrap() = Some(RecordingViewport::default());
        handle(
            &trigger,
            &visibility,
            &restore_flag,
            &ctx,
            &mut queued_visibility,
        );
        assert!(!visibility.load(Ordering::SeqCst));
        assert!(!restore_flag.load(Ordering::SeqCst));
        assert!(queued_visibility.is_none());
    }

    #[test]
    fn two_queued_grid_toggles_preserve_order_and_restore_keyboard_ownership() {
        let trigger = trigger();
        let visibility = Arc::new(AtomicBool::new(false));
        let restore_flag = Arc::new(AtomicBool::new(false));
        let viewport = RecordingViewport::default();
        let ctx = Arc::new(Mutex::new(Some(viewport.clone())));
        let mut queued_visibility = None;
        let mut batch = VisibilityToggleBatch::default();
        let mut keyboard_suspended = false;
        let mut transitions = Vec::new();

        for _ in 0..2 {
            let was_visible = batch.record_toggle(&visibility);
            transitions.push(was_visible);
            keyboard_suspended = !was_visible;
        }

        assert_eq!(transitions, [false, true]);
        assert_eq!(batch.final_visible(), Some(false));
        assert!(!visibility.load(Ordering::SeqCst));
        assert!(handle_visibility_toggle_batch(
            &batch,
            &restore_flag,
            &ctx,
            &mut queued_visibility,
            (-10_000.0, -10_000.0),
            false,
            false,
            None,
            None,
            (400.0, 220.0),
        ));

        assert!(!visibility.load(Ordering::SeqCst));
        assert!(!restore_flag.load(Ordering::SeqCst));
        assert!(!keyboard_suspended);
        assert!(queued_visibility.is_none());
        assert_eq!(viewport.repaint_count.load(Ordering::SeqCst), 2);
        assert_eq!(viewport.commands.lock().unwrap().len(), 4);
        assert!(!trigger.take());
    }

    #[test]
    fn legacy_hotkey_visibility_edges_report_grid_keyboard_transfer() {
        let trigger = trigger();
        let visibility = Arc::new(AtomicBool::new(false));
        let restore_flag = Arc::new(AtomicBool::new(false));
        let ctx = Arc::new(Mutex::new(Some(RecordingViewport::default())));
        let mut queued_visibility = None;
        let mut keyboard_suspended = false;

        for (index, expected_was_visible) in [false, true].into_iter().enumerate() {
            toggle(&trigger);
            let mut reported_was_visible = None;
            assert!(handle_visibility_trigger_with_owner(
                &trigger,
                &visibility,
                &restore_flag,
                &ctx,
                &mut queued_visibility,
                (-10_000.0, -10_000.0),
                false,
                false,
                None,
                None,
                (400.0, 220.0),
                |was_visible| reported_was_visible = Some(was_visible),
            ));
            assert_eq!(reported_was_visible, Some(expected_was_visible));
            keyboard_suspended = !reported_was_visible.unwrap();
            assert_eq!(keyboard_suspended, index == 0);
        }

        assert!(!visibility.load(Ordering::SeqCst));
        assert!(!keyboard_suspended);
    }
}
