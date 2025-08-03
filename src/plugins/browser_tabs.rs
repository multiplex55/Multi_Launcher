use crate::actions::Action;
use crate::plugin::Plugin;

pub struct BrowserTabsPlugin;

#[cfg(target_os = "windows")]
mod imp {
    use super::*;
    use once_cell::sync::Lazy;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Mutex, RwLock};
    use std::time::{Duration, Instant};
    use tracing::{error, warn};

    #[derive(Clone)]
    pub(super) struct TabInfo {
        title: String,
        url: String,
    }

    static CACHE: Lazy<RwLock<Vec<TabInfo>>> = Lazy::new(|| RwLock::new(Vec::new()));
    static LAST_REFRESH: Lazy<Mutex<Instant>> =
        Lazy::new(|| Mutex::new(Instant::now() - Duration::from_secs(60)));
    static REFRESHING: AtomicBool = AtomicBool::new(false);
    static LAST_ENUM_ERR: Lazy<Mutex<Instant>> = Lazy::new(|| {
        Mutex::new(Instant::now() - Duration::from_secs(60))
    });

    fn log_enum_error(msg: &str, err: windows::core::Error) {
        let mut last = LAST_ENUM_ERR.lock().unwrap();
        if last.elapsed() > Duration::from_secs(30) {
            error!(?err, "BrowserTabsPlugin: {msg}");
            *last = Instant::now();
        }
    }

    fn enumerate_tabs() -> Vec<TabInfo> {
        use windows::core::{BSTR, VARIANT};
        use windows::Win32::System::Com::{
            CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
            COINIT_APARTMENTTHREADED,
        };
        use windows::Win32::UI::Accessibility::*;

        let mut out = Vec::new();
        unsafe {
            let init = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            if let Err(e) = init {
                log_enum_error("CoInitializeEx failed", e);
                return out;
            }

            let automation = match CoCreateInstance::<_, IUIAutomation>(
                &CUIAutomation,
                None,
                CLSCTX_INPROC_SERVER,
            ) {
                Ok(a) => a,
                Err(e) => {
                    log_enum_error("CoCreateInstance(IUIAutomation) failed", e);
                    CoUninitialize();
                    return out;
                }
            };

            let root = match automation.GetRootElement() {
                Ok(r) => r,
                Err(e) => {
                    log_enum_error("GetRootElement failed", e);
                    CoUninitialize();
                    return out;
                }
            };

            let cond = match automation.CreatePropertyCondition(
                UIA_ControlTypePropertyId,
                &VARIANT::from(UIA_TabItemControlTypeId.0),
            ) {
                Ok(c) => c,
                Err(e) => {
                    warn!(?e, "BrowserTabsPlugin: CreatePropertyCondition failed");
                    CoUninitialize();
                    return out;
                }
            };

            let tabs = match root.FindAll(TreeScope_Subtree, &cond) {
                Ok(t) => t,
                Err(e) => {
                    warn!(?e, "BrowserTabsPlugin: FindAll failed");
                    CoUninitialize();
                    return out;
                }
            };

            let count = match tabs.Length() {
                Ok(c) => c,
                Err(e) => {
                    warn!(?e, "BrowserTabsPlugin: tabs.Length failed");
                    CoUninitialize();
                    return out;
                }
            };

            for i in 0..count {
                let elem = match tabs.GetElement(i) {
                    Ok(e) => e,
                    Err(e) => {
                        warn!(?e, "BrowserTabsPlugin: GetElement failed");
                        continue;
                    }
                };
                let title = elem.CurrentName().unwrap_or_default().to_string();
                let mut url = String::new();
                match elem.GetCurrentPropertyValue(UIA_LegacyIAccessibleValuePropertyId) {
                    Ok(var) => {
                        if let Ok(bstr) = BSTR::try_from(&var) {
                            url = bstr.to_string();
                        }
                    }
                    Err(e) => warn!(?e, "BrowserTabsPlugin: GetCurrentPropertyValue failed"),
                }
                out.push(TabInfo { title, url });
            }

            CoUninitialize();
        }
        out
    }

    fn refresh_cache() {
        let tabs = enumerate_tabs();
        if let Ok(mut cache) = CACHE.write() {
            *cache = tabs;
        } else {
            warn!("BrowserTabsPlugin: failed to lock cache for writing");
        }
        if let Ok(mut last) = LAST_REFRESH.lock() {
            *last = Instant::now();
        } else {
            warn!("BrowserTabsPlugin: failed to lock last refresh time");
        }
        REFRESHING.store(false, Ordering::Release);
    }

    fn trigger_refresh() {
        let refresh_needed = {
            if let Ok(last) = LAST_REFRESH.lock() {
                last.elapsed() > Duration::from_secs(2)
            } else {
                false
            }
        };
        if refresh_needed
            && REFRESHING
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
        {
            std::thread::spawn(refresh_cache);
        }
    }

    pub(super) fn cached_actions(filter: &str) -> Vec<Action> {
        trigger_refresh();

        let mut out = Vec::new();
        if let Ok(cache) = CACHE.read() {
            for tab in cache.iter() {
                if filter.is_empty()
                    || tab.title.to_lowercase().contains(filter)
                    || tab.url.to_lowercase().contains(filter)
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
        }
        out
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::thread::sleep;
        use std::time::{Duration, Instant};
        use super::super::BrowserTabsPlugin;

        #[test]
        fn search_refreshes_cache() {
            {
                let mut cache = CACHE.write().unwrap();
                cache.clear();
                cache.push(TabInfo {
                    title: "Dummy".into(),
                    url: "about:blank".into(),
                });
            }

            {
                let mut last = LAST_REFRESH.lock().unwrap();
                *last = Instant::now();
            }

            let plugin = BrowserTabsPlugin;
            let first = plugin.search("tab ");
            assert_eq!(first.len(), 1);
            assert!(first[0].label.contains("Dummy"));

            {
                let mut last = LAST_REFRESH.lock().unwrap();
                *last = Instant::now() - Duration::from_secs(60);
            }

            let _ = plugin.search("tab ");
            sleep(Duration::from_millis(500));
            let refreshed = plugin.search("tab ");
            assert!(refreshed.iter().all(|a| !a.label.contains("Dummy")));
        }
    }
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

        imp::cached_actions(&filter)
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

#[cfg(all(test, not(target_os = "windows")))]
mod tests {
    use super::*;

    #[test]
    fn search_is_empty_on_non_windows() {
        let plugin = BrowserTabsPlugin;
        assert!(plugin.search("tab ").is_empty());
    }
}

