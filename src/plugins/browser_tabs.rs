use crate::actions::Action;
use crate::plugin::Plugin;

pub struct BrowserTabsPlugin;

#[cfg(target_os = "windows")]
use once_cell::sync::Lazy;
#[cfg(target_os = "windows")]
use std::sync::Mutex;
#[cfg(target_os = "windows")]
use std::time::{Duration, Instant};

#[cfg(target_os = "windows")]
#[derive(Clone)]
struct TabInfo {
    title: String,
    url: String,
}

#[cfg(target_os = "windows")]
struct TabCache {
    tabs: Vec<TabInfo>,
    last_update: Instant,
    updating: bool,
}

#[cfg(target_os = "windows")]
static TAB_CACHE: Lazy<Mutex<TabCache>> = Lazy::new(|| Mutex::new(TabCache {
    tabs: Vec::new(),
    last_update: Instant::now() - Duration::from_secs(3600),
    updating: false,
}));

#[cfg(target_os = "windows")]
fn refresh_tabs_async() {
    use std::thread;

    let mut cache = TAB_CACHE.lock().unwrap();
    if cache.last_update.elapsed() > Duration::from_secs(5) && !cache.updating {
        cache.updating = true;
        drop(cache);
        thread::spawn(|| {
            let tabs = enumerate_tabs();
            let mut cache = TAB_CACHE.lock().unwrap();
            cache.tabs = tabs;
            cache.last_update = Instant::now();
            cache.updating = false;
        });
    }
}

#[cfg(target_os = "windows")]
fn enumerate_tabs() -> Vec<TabInfo> {
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
                                    if let Ok(var) =
                                        elem.GetCurrentPropertyValue(
                                            UIA_LegacyIAccessibleValuePropertyId,
                                        )
                                    {
                                        if let Ok(bstr) = BSTR::try_from(&var) {
                                            url = bstr.to_string();
                                        }
                                    }
                                    out.push(TabInfo { title, url });
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

        refresh_tabs_async();
        let tabs = {
            let cache = TAB_CACHE.lock().unwrap();
            cache.tabs.clone()
        };

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

