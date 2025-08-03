use crate::actions::Action;
use crate::plugin::Plugin;

pub struct BrowserTabsPlugin;

impl Plugin for BrowserTabsPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        const PREFIX: &str = "tab";
        let trimmed = query.trim();
        let rest = match crate::common::strip_prefix_ci(trimmed, PREFIX) {
            Some(r) => r.trim(),
            None => return Vec::new(),
        };
        #[cfg(target_os = "windows")]
        {
            let filter = rest.to_lowercase();
            let mut actions = Vec::new();
            if let Ok(tabs) = fetch_tabs() {
                for (title, url) in tabs {
                    if title.to_lowercase().contains(&filter)
                        || url.to_lowercase().contains(&filter)
                    {
                        actions.push(Action {
                            label: title,
                            desc: url.clone(),
                            action: url,
                            args: None,
                        });
                    }
                }
            }
            actions
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = rest; // suppress unused variable warning
            Vec::new()
        }
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
        vec![Action {
            label: "tab".into(),
            desc: "Browser tabs".into(),
            action: "query:tab ".into(),
            args: None,
        }]
    }
}

#[cfg(target_os = "windows")]
fn fetch_tabs() -> anyhow::Result<Vec<(String, String)>> {
    use windows::core::{Interface, Variant};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_MULTITHREADED,
    };
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, TreeScope_Subtree, UIA_ControlTypePropertyId,
        UIA_LegacyIAccessibleValuePropertyId, UIA_NamePropertyId, UIA_TabItemControlTypeId,
    };

    unsafe {
        CoInitializeEx(std::ptr::null_mut(), COINIT_MULTITHREADED).ok();
        let automation: IUIAutomation =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
        let root = automation.GetRootElement()?;
        let condition = automation.CreatePropertyCondition(
            UIA_ControlTypePropertyId,
            Variant::from(UIA_TabItemControlTypeId.0 as i32),
        )?;
        let collection = root.FindAll(TreeScope_Subtree, &condition)?;
        let count = collection.Length()?;
        let mut out = Vec::new();
        for i in 0..count {
            if let Ok(el) = collection.GetElement(i) {
                let title = el.CurrentName()?.to_string();
                // Try to get the URL from legacy value property, may be empty
                let url_variant = el
                    .GetCurrentPropertyValue(UIA_LegacyIAccessibleValuePropertyId)
                    .unwrap_or_default();
                let url = url_variant.to_string();
                out.push((title, url));
            }
        }
        CoUninitialize();
        Ok(out)
    }
}
