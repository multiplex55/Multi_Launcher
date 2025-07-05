use multi_launcher::visibility::{apply_visibility, OFFSCREEN_POS};
use eframe::egui;

#[path = "mock_ctx.rs"]
mod mock_ctx;
use mock_ctx::MockCtx;

#[test]
fn hide_moves_window_offscreen() {
    let ctx = MockCtx::default();
    apply_visibility(false, &ctx);
    let cmds = ctx.commands.lock().unwrap();
    assert_eq!(cmds.len(), 2); // OuterPosition + Visible(true)
    match cmds[0] {
        egui::ViewportCommand::OuterPosition(pos) => {
            assert_eq!(pos.x, OFFSCREEN_POS.0);
            assert_eq!(pos.y, OFFSCREEN_POS.1);
        }
        _ => panic!("unexpected command"),
    }
    match cmds[1] {
        egui::ViewportCommand::Visible(v) => assert!(v),
        _ => panic!("unexpected command"),
    }
}
