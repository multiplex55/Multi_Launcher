use multi_launcher::visibility::apply_visibility;
use eframe::egui;

#[path = "mock_ctx.rs"]
mod mock_ctx;
use mock_ctx::MockCtx;

#[test]
fn offscreen_position_when_hidden() {
    let ctx = MockCtx::default();
    apply_visibility(false, &ctx, (42.0, 84.0));
    let cmds = ctx.commands.lock().unwrap();
    assert_eq!(cmds.len(), 1);
    match cmds[0] {
        egui::ViewportCommand::OuterPosition(p) => {
            assert_eq!(p.x, 42.0);
            assert_eq!(p.y, 84.0);
        }
        _ => panic!("unexpected command"),
    }
}
