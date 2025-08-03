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

        use windows::core::{BSTR, VARIANT};
        use windows::Win32::System::Com::{
            CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
            COINIT_APARTMENTTHREADED,
        };
        use windows::Win32::UI::Accessibility::*;

        let mut out = Vec::new();
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            if let Ok(automation) =
                CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
            {
                if let Ok(root) = automation.GetRootElement() {
                    if let Ok(cond) = automation.CreatePropertyCondition(
                        UIA_ControlTypePropertyId,
                        &VARIANT::from(UIA_TabItemControlTypeId.0),
                    ) {
                        if let Ok(tabs) = root.FindAll(TreeScope_Subtree, &cond) {
                            if let Ok(count) = tabs.Length() {
                                for i in 0..count {
                                    if let Ok(elem) = tabs.GetElement(i) {
                                        let title = elem.CurrentName().unwrap_or_default().to_string();
                                        let mut url = String::new();
                                        if let Ok(var) = elem
                                            .GetCurrentPropertyValue(
                                                UIA_LegacyIAccessibleValuePropertyId,
                                            )
                                        {
                                            if let Ok(bstr) = BSTR::try_from(&var) {
                                                url = bstr.to_string();
                                            }
                                        }
                                        if filter.is_empty()
                                            || title.to_lowercase().contains(&filter)
                                            || url.to_lowercase().contains(&filter)
                                        {
                                            let encoded = urlencoding::encode(&title);
                                            out.push(Action {
                                                label: format!("Switch to {title}"),
                                                desc: if url.is_empty() {
                                                    "Browser Tab".into()
                                                } else {
                                                    url.clone()
                                                },
                                                action: format!("tab:switch:{encoded}"),
                                                args: None,
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            CoUninitialize();
        }
        out
    }

    #[cfg(not(target_os = "windows"))]
    fn search(&self, _query: &str) -> Vec<Action> {
        Vec::new()
    }

    fn name(&self) -> &str {
        "browser_tabs"
    }

    fn description(&self) -> &str {
        "Switch between browser tabs (prefix: `tab`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "tab".into(),
            desc: "Browser tabs".into(),
            action: "query:tab ".into(),
            args: None,
        }]
    }
}

