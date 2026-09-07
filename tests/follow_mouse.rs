use eframe::egui;
use multi_launcher::visibility::{VisiblePlacementPolicy, apply_visibility};
use multi_launcher::window_manager::{
    MOCK_MOUSE_LOCK, clear_mock_mouse_position, set_mock_mouse_position,
};
use std::sync::atomic::Ordering;

#[path = "support/mock_ctx.rs"]
mod mock_ctx;
use mock_ctx::MockCtx;

#[test]
fn cursor_failure_does_not_move_window() {
    let _lock = MOCK_MOUSE_LOCK.lock().unwrap();
    let ctx = MockCtx::default();
    set_mock_mouse_position(None);

    apply_visibility(
        true,
        VisiblePlacementPolicy::ApplyConfiguredPlacement,
        &ctx,
        (0.0, 0.0),
        true,
        false,
        None,
        None,
        (400.0, 220.0),
    );

    clear_mock_mouse_position();

    let cmds = ctx.commands.lock().unwrap();
    assert_eq!(cmds.len(), 3);
    match cmds[0] {
        egui::ViewportCommand::Visible(v) => assert!(v),
        _ => panic!("unexpected command"),
    }
    match cmds[1] {
        egui::ViewportCommand::Minimized(m) => assert!(!m),
        _ => panic!("unexpected command"),
    }
    match cmds[2] {
        egui::ViewportCommand::Focus => {}
        _ => panic!("unexpected command"),
    }
}

#[test]
fn follow_mouse_applies_position_on_true_show() {
    let _lock = MOCK_MOUSE_LOCK.lock().unwrap();
    let ctx = MockCtx::default();
    set_mock_mouse_position(Some((900.0, 600.0)));

    apply_visibility(
        true,
        VisiblePlacementPolicy::ApplyConfiguredPlacement,
        &ctx,
        (0.0, 0.0),
        true,
        false,
        None,
        None,
        (400.0, 220.0),
    );

    clear_mock_mouse_position();
    let cmds = ctx.commands.lock().unwrap();
    assert_eq!(cmds.len(), 4);
    match cmds[0] {
        egui::ViewportCommand::OuterPosition(p) => assert_eq!(p, egui::pos2(700.0, 490.0)),
        _ => panic!("unexpected command"),
    }
}

#[test]
fn configured_placement_applies_position_and_size_on_true_show() {
    let ctx = MockCtx::default();

    apply_visibility(
        true,
        VisiblePlacementPolicy::ApplyConfiguredPlacement,
        &ctx,
        (0.0, 0.0),
        false,
        true,
        Some((400.0, 200.0)),
        Some((640.0, 360.0)),
        (400.0, 220.0),
    );

    let cmds = ctx.commands.lock().unwrap();
    assert_eq!(cmds.len(), 5);
    match cmds[0] {
        egui::ViewportCommand::OuterPosition(p) => assert_eq!(p, egui::pos2(400.0, 200.0)),
        _ => panic!("unexpected command"),
    }
    match cmds[1] {
        egui::ViewportCommand::InnerSize(size) => assert_eq!(size, egui::vec2(640.0, 360.0)),
        _ => panic!("unexpected command"),
    }
}

#[test]
fn configured_placement_is_ignored_when_preserving_current_geometry() {
    let ctx = MockCtx::default();

    apply_visibility(
        true,
        VisiblePlacementPolicy::PreserveCurrentGeometry,
        &ctx,
        (0.0, 0.0),
        false,
        true,
        Some((400.0, 200.0)),
        Some((640.0, 360.0)),
        (400.0, 220.0),
    );

    assert_visible_restore_commands(&ctx);
}

#[test]
fn follow_mouse_is_ignored_when_preserving_current_geometry() {
    let _lock = MOCK_MOUSE_LOCK.lock().unwrap();
    let ctx = MockCtx::default();
    set_mock_mouse_position(Some((900.0, 600.0)));

    apply_visibility(
        true,
        VisiblePlacementPolicy::PreserveCurrentGeometry,
        &ctx,
        (0.0, 0.0),
        true,
        false,
        None,
        None,
        (400.0, 220.0),
    );

    clear_mock_mouse_position();
    assert_visible_restore_commands(&ctx);
}

fn assert_visible_restore_commands(ctx: &MockCtx) {
    let cmds = ctx.commands.lock().unwrap();
    assert_eq!(cmds.len(), 3);
    assert!(matches!(cmds[0], egui::ViewportCommand::Visible(true)));
    assert!(matches!(cmds[1], egui::ViewportCommand::Minimized(false)));
    assert!(matches!(cmds[2], egui::ViewportCommand::Focus));
    assert_eq!(ctx.repaint_requests.load(Ordering::SeqCst), 1);
}
