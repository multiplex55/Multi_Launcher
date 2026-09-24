use eframe::egui;
use multi_launcher::visibility::{VisiblePlacementPolicy, apply_visibility};
use std::sync::atomic::Ordering;

#[path = "../support/mock_ctx.rs"]
mod mock_ctx;
use mock_ctx::MockCtx;

#[test]
fn offscreen_position_when_hidden() {
    let ctx = MockCtx::default();
    apply_visibility(
        false,
        VisiblePlacementPolicy::PreserveCurrentGeometry,
        &ctx,
        (42.0, 84.0),
        true,
        false,
        None,
        None,
        (400.0, 220.0),
    );
    let cmds = ctx.commands.lock().unwrap();
    assert_eq!(cmds.len(), 1);
    match cmds[0] {
        egui::ViewportCommand::OuterPosition(p) => {
            #[cfg(target_os = "windows")]
            {
                use windows::Win32::UI::WindowsAndMessaging::{
                    GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
                    SM_YVIRTUALSCREEN,
                };

                let (desktop_x, desktop_y, desktop_width, desktop_height) = unsafe {
                    (
                        GetSystemMetrics(SM_XVIRTUALSCREEN),
                        GetSystemMetrics(SM_YVIRTUALSCREEN),
                        GetSystemMetrics(SM_CXVIRTUALSCREEN),
                        GetSystemMetrics(SM_CYVIRTUALSCREEN),
                    )
                };
                assert!(desktop_width > 0 && desktop_height > 0);

                let (left, top) = (p.x as i64, p.y as i64);
                let (right, bottom) = (left + 400, top + 220);
                let (desktop_left, desktop_top) = (i64::from(desktop_x), i64::from(desktop_y));
                let (desktop_right, desktop_bottom) = (
                    desktop_left + i64::from(desktop_width),
                    desktop_top + i64::from(desktop_height),
                );
                let intersects_virtual_screen = left < desktop_right
                    && right > desktop_left
                    && top < desktop_bottom
                    && bottom > desktop_top;
                assert!(
                    !intersects_virtual_screen,
                    "hidden launcher rectangle ({left}, {top}, {right}, {bottom}) intersects virtual screen ({desktop_left}, {desktop_top}, {desktop_right}, {desktop_bottom})"
                );
            }
            #[cfg(not(target_os = "windows"))]
            {
                assert_eq!(p.x, 42.0);
                assert_eq!(p.y, 84.0);
            }
        }
        _ => panic!("unexpected command"),
    }
    assert_eq!(ctx.repaint_requests.load(Ordering::SeqCst), 1);
}
