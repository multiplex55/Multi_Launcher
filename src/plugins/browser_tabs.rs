use crate::actions::Action;
use crate::plugin::Plugin;

#[cfg(target_os = "windows")]
use std::sync::{Arc, Mutex};
#[cfg(target_os = "windows")]
use std::thread;
#[cfg(target_os = "windows")]
use std::time::{Duration, Instant};

#[cfg(target_os = "windows")]
#[derive(Clone)]
struct TabInfo {
    title: String,
    url: String,
}

#[cfg(target_os = "windows")]
struct Cache {
    tabs: Vec<TabInfo>,
    last_refresh: Instant,
    loading: bool,
}

pub struct BrowserTabsPlugin {
    #[cfg(target_os = "windows")]
    cache: Arc<Mutex<Cache>>,
}

impl Default for BrowserTabsPlugin {
    fn default() -> Self {
        #[cfg(target_os = "windows")]
        {
            Self {
                cache: Arc::new(Mutex::new(Cache {
                    tabs: Vec::new(),
                    last_refresh: Instant::now(),
                    loading: false,
                })),
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            Self {}
        }
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

    let mut tabs_out = Vec::new();
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
                                    tabs_out.push(TabInfo { title, url });
                                }
                            }
                        }
                    }
                }
            }
        }
        CoUninitialize();
    }
    tabs_out
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

        let cache = self.cache.clone();
        let (tabs, loading) = {
            let mut state = cache.lock().unwrap();
            if !state.loading
                && (state.tabs.is_empty() || state.last_refresh.elapsed() > Duration::from_secs(2))
            {
                state.loading = true;
                let cache_clone = cache.clone();
                thread::spawn(move || {
                    let tabs = enumerate_tabs();
                    let mut st = cache_clone.lock().unwrap();
                    st.tabs = tabs;
                    st.last_refresh = Instant::now();
                    st.loading = false;
                });
            }
            (state.tabs.clone(), state.loading)
        };

        if tabs.is_empty() {
            return if loading { Vec::new() } else { Vec::new() };
        }

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

