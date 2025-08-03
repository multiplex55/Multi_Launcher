use crate::actions::Action;
use crate::plugin::Plugin;

pub struct BrowserTabsPlugin;

impl Plugin for BrowserTabsPlugin {
    #[cfg(target_os = "windows")]
    fn search(&self, query: &str) -> Vec<Action> {
        const PREFIX: &str = "tab";
        let trimmed = query.trim();
        let rest = match crate::common::strip_prefix_ci(trimmed, PREFIX) {
            Some(r) => r.trim(),
            None => return Vec::new(),
        };
        let filter = rest.to_lowercase();

        use std::mem::ManuallyDrop;
        use windows::core::BSTR;
        use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
        use windows::Win32::System::Com::{
            CoCreateInstance, CoInitializeEx, CoUninitialize, VariantClear, CLSCTX_ALL,
            COINIT_APARTMENTTHREADED, VARIANT, VT_BSTR,
        };
        use windows::Win32::UI::Accessibility::{
            CUIAutomation, IUIAutomation, TreeScope_Subtree, UIA_ControlTypePropertyId,
            UIA_LegacyIAccessibleValuePropertyId, UIA_TabItemControlTypeId,
        };
        use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, GetClassNameW};

        #[derive(Default)]
        struct TabInfo {
            title: String,
            url: String,
        }

        unsafe fn variant_to_string(v: &VARIANT) -> String {
            if v.Anonymous.Anonymous.vt as u32 == VT_BSTR.0 {
                let ptr = v.Anonymous.Anonymous.Anonymous.bstrVal;
                if !ptr.is_null() {
                    let bstr = ManuallyDrop::new(BSTR::from_raw(ptr));
                    return bstr.to_string().unwrap_or_default();
                }
            }
            String::new()
        }

        unsafe fn tabs_in_window(ui: &IUIAutomation, hwnd: HWND, out: &mut Vec<TabInfo>) {
            if let Ok(elem) = ui.ElementFromHandle(hwnd) {
                if let Ok(cond) = ui.CreatePropertyCondition(
                    UIA_ControlTypePropertyId,
                    UIA_TabItemControlTypeId.0.into(),
                ) {
                    if let Ok(found) = elem.FindAll(TreeScope_Subtree, cond) {
                        let len = found.Length().unwrap_or(0);
                        for i in 0..len {
                            if let Ok(tab_el) = found.GetElement(i) {
                                let title = tab_el
                                    .CurrentName()
                                    .unwrap_or_default()
                                    .to_string()
                                    .unwrap_or_default();
                                let mut var = tab_el
                                    .GetCurrentPropertyValue(UIA_LegacyIAccessibleValuePropertyId)
                                    .unwrap_or_default();
                                let url = variant_to_string(&var);
                                VariantClear(&mut var);
                                out.push(TabInfo { title, url });
                            }
                        }
                    }
                }
            }
        }

        struct EnumCtx {
            ui: IUIAutomation,
            tabs: Vec<TabInfo>,
        }

        unsafe extern "system" fn enum_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
            let ctx = &mut *(lparam.0 as *mut EnumCtx);
            let mut buf = [0u16; 256];
            let len = GetClassNameW(hwnd, &mut buf);
            if len > 0 {
                let class = String::from_utf16_lossy(&buf[..len as usize]);
                if class == "Chrome_WidgetWin_1" || class == "MozillaWindowClass" {
                    tabs_in_window(&ctx.ui, hwnd, &mut ctx.tabs);
                }
            }
            BOOL(1)
        }

        unsafe {
            if CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_err() {
                return Vec::new();
            }
            let ui = match CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_ALL) {
                Ok(u) => u,
                Err(_) => {
                    CoUninitialize();
                    return Vec::new();
                }
            };
            let mut ctx = EnumCtx {
                ui,
                tabs: Vec::new(),
            };
            let ctx_ptr = &mut ctx as *mut _;
            let _ = EnumWindows(Some(enum_cb), LPARAM(ctx_ptr as isize));
            CoUninitialize();

            ctx.tabs
                .into_iter()
                .filter(|t| {
                    if filter.is_empty() {
                        true
                    } else {
                        t.title.to_lowercase().contains(&filter)
                            || t.url.to_lowercase().contains(&filter)
                    }
                })
                .map(|t| Action {
                    label: t.title.clone(),
                    desc: t.url.clone(),
                    action: if t.url.is_empty() {
                        String::new()
                    } else {
                        format!("open:{}", t.url)
                    },
                    args: None,
                })
                .collect()
        }
    }

    #[cfg(not(target_os = "windows"))]
    fn search(&self, _query: &str) -> Vec<Action> {
        Vec::new()
    }

    fn name(&self) -> &str {
        "browser_tabs"
    }

    fn description(&self) -> &str {
        "Search open browser tabs (prefix: `tab`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "tab".into(),
            desc: "Browser Tabs".into(),
            action: "query:tab ".into(),
            args: None,
        }]
    }
}
