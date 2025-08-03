use crate::actions::Action;
use crate::plugin::Plugin;

#[cfg(target_os = "windows")]
use once_cell::sync::Lazy;

#[cfg(target_os = "windows")]
use std::sync::Mutex;

#[cfg(target_os = "windows")]
use std::time::{Duration, Instant};

#[cfg(target_os = "windows")]
use windows::core::{BSTR, VARIANT};

#[cfg(target_os = "windows")]
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED,
};

#[cfg(target_os = "windows")]
use windows::Win32::UI::Accessibility::*;

pub struct BrowserTabsPlugin;

#[cfg(target_os = "windows")]
#[derive(Clone)]
struct Tab {
    title: String,
    url: String,
    elem: IUIAutomationElement,
}

#[cfg(target_os = "windows")]
struct TabCache {
    tabs: Vec<Tab>,
    last_update: Instant,
}

#[cfg(target_os = "windows")]
static CACHE: Lazy<Mutex<TabCache>> = Lazy::new(|| {
    Mutex::new(TabCache {
        tabs: Vec::new(),
        // ensure the first lookup refreshes immediately
        last_update: Instant::now() - Duration::from_secs(60),
    })
});

#[cfg(target_os = "windows")]
fn get_tabs() -> Vec<Tab> {
    const TTL: Duration = Duration::from_secs(2);
    let mut cache = CACHE.lock().unwrap();
    if cache.last_update.elapsed() > TTL {
        cache.tabs = enumerate_tabs();
        cache.last_update = Instant::now();
    }
    cache.tabs.clone()
}

#[cfg(target_os = "windows")]
fn enumerate_tabs() -> Vec<Tab> {
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
                                    if let Ok(var) = elem.GetCurrentPropertyValue(
                                        UIA_LegacyIAccessibleValuePropertyId,
                                    ) {
                                        if let Ok(bstr) = BSTR::try_from(&var) {
                                            url = bstr.to_string();
                                        }
                                    }
                                    out.push(Tab { title, url, elem });
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

        let tabs = get_tabs();
        let mut out = Vec::new();
        for tab in tabs {
            if filter.is_empty()
                || tab.title.to_lowercase().contains(&filter)
                || tab.url.to_lowercase().contains(&filter)
            {
                let encoded = urlencoding::encode(&tab.title);
                out.push(Action {
                    label: format!("Switch to {}", tab.title),
                    desc: if tab.url.is_empty() {
                        "Browser Tab".into()
                    } else {
                        tab.url.clone()
                    },
                    action: format!("tab:switch:{encoded}"),
                    args: None,
                });
            }
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

#[cfg(target_os = "windows")]
pub fn switch_tab(title: &str) {
    let tabs = get_tabs();
    if let Some(tab) = tabs.into_iter().find(|t| t.title == title) {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let _ = tab.elem.SetFocus();
            CoUninitialize();
        }
    }
}
