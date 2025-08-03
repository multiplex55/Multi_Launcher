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

        use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
        use windows::Win32::UI::Accessibility::{IUIAutomation, CUIAutomation, UIA_ControlTypePropertyId, UIA_NamePropertyId, UIA_LegacyIAccessibleValuePropertyId, UIA_TabItemControlTypeId, TreeScope_Subtree};
        use windows::Win32::System::Variant::VARIANT;

        unsafe {
            if CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_err() {
                return Vec::new();
            }

            let automation: IUIAutomation = match CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) {
                Ok(a) => a,
                Err(_) => {
                    CoUninitialize();
                    return Vec::new();
                }
            };

            let root = match automation.GetRootElement() {
                Ok(r) => r,
                Err(_) => {
                    CoUninitialize();
                    return Vec::new();
                }
            };

            let condition = match automation.CreatePropertyCondition(
                UIA_ControlTypePropertyId,
                VARIANT::from(UIA_TabItemControlTypeId.0 as i32),
            ) {
                Ok(c) => c,
                Err(_) => {
                    CoUninitialize();
                    return Vec::new();
                }
            };

            let elements = match root.FindAll(TreeScope_Subtree, &condition) {
                Ok(e) => e,
                Err(_) => {
                    CoUninitialize();
                    return Vec::new();
                }
            };

            let len = match elements.Length() {
                Ok(l) => l,
                Err(_) => {
                    CoUninitialize();
                    return Vec::new();
                }
            };

            let mut actions = Vec::new();
            for i in 0..len {
                if let Ok(element) = elements.GetElement(i) {
                    let name = element.CurrentName().unwrap_or_default().to_string();
                    let url_variant = element
                        .GetCurrentPropertyValue(UIA_LegacyIAccessibleValuePropertyId, 0)
                        .unwrap_or_default();
                    let url = url_variant.ToString().unwrap_or_default().to_string();
                    let haystack = format!("{name} {url}").to_lowercase();
                    if haystack.contains(&filter) {
                        actions.push(Action {
                            label: if url.is_empty() {
                                name.clone()
                            } else {
                                format!("{name} - {url}")
                            },
                            desc: "Browser Tabs".into(),
                            action: "noop".into(),
                            args: None,
                        });
                    }
                }
            }

            CoUninitialize();
            actions
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
        "List open browser tabs (prefix: `tab`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action { label: "tab".into(), desc: "Browser Tabs".into(), action: "query:tab ".into(), args: None }]
    }
}

