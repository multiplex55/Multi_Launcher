use crate::actions::Action;
use crate::plugin::Plugin;

pub struct WindowsPlugin;

impl Plugin for WindowsPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        const PREFIX: &str = "win";
        let trimmed = query.trim();
        let rest = match crate::common::strip_prefix_ci(trimmed, PREFIX) {
            Some(r) => r.trim(),
            None => return Vec::new(),
        };
        let filter = rest.to_lowercase();
        use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{
            EnumWindows, GetWindow, GetWindowTextLengthW, GetWindowTextW, IsWindowVisible, GW_OWNER,
        };
        struct Ctx {
            filter: String,
            out: Vec<Action>,
        }
        unsafe extern "system" fn enum_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
            let ctx = &mut *(lparam.0 as *mut Ctx);
            if IsWindowVisible(hwnd).as_bool()
                && GetWindow(hwnd, GW_OWNER).unwrap_or_default().0.is_null()
            {
                let len = GetWindowTextLengthW(hwnd);
                if len > 0 {
                    let mut buf = vec![0u16; len as usize + 1];
                    let read = GetWindowTextW(hwnd, &mut buf);
                    let title = String::from_utf16_lossy(&buf[..read as usize]);
                    if title.to_lowercase().contains(&ctx.filter) {
                        ctx.out.push(Action {
                            label: format!("Switch to {title}"),
                            desc: "Windows".into(),
                            action: format!("window:switch:{}", hwnd.0 as usize),
                            args: None,
                            preview_text: None,
                            risk_level: None,
                            icon: None,
                        });
                        ctx.out.push(Action {
                            label: format!("Close {title}"),
                            desc: "Windows".into(),
                            action: format!("window:close:{}", hwnd.0 as usize),
                            args: None,
                            preview_text: None,
                            risk_level: None,
                            icon: None,
                        });
                    }
                }
            }
            BOOL(1)
        }
        let mut ctx = Ctx {
            filter,
            out: Vec::new(),
        };
        unsafe {
            let ctx_ptr = &mut ctx as *mut Ctx;
            let _ = EnumWindows(Some(enum_cb), LPARAM(ctx_ptr as isize));
        }
        ctx.out
    }

    fn name(&self) -> &str {
        "windows"
    }

    fn description(&self) -> &str {
        "Switch or close windows (prefix: `win`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "win".into(),
            desc: "Windows".into(),
            action: "query:win ".into(),
            args: None,
            preview_text: None,
            risk_level: None,
            icon: None,
        }]
    }
}
