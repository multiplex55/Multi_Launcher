use eframe::egui;
use multi_launcher::visibility::apply_visibility;
use multi_launcher::window_manager::{
    clear_mock_mouse_position, set_mock_mouse_position, MOCK_MOUSE_LOCK,
};

#[path = "mock_ctx.rs"]
mod mock_ctx;
use mock_ctx::MockCtx;

#[test]
fn cursor_failure_does_not_move_window() {
    let _lock = MOCK_MOUSE_LOCK.lock().unwrap();
    let ctx = MockCtx::default();
    set_mock_mouse_position(None);

    apply_visibility(
        true,
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
