//! Browser tab search and switching on Windows using UI Automation.
//!
//! The plugin enumerates `TabItem` elements exposed by browsers via the
//! Windows UI Automation (UIA) tree. This currently works reliably with
//! Chromium-based browsers such as Microsoft Edge and Google Chrome; other
//! browsers may not expose their tabs as `TabItem` controls and will therefore
//! not appear in search results.
//!
//! Only top-level windows on the active desktop session are scanned. Tabs in
//! minimized or non‑UIA compliant windows might be missed, and changes in a
//! browser's accessibility implementation could break enumeration.
//!
//! When activation patterns like `SelectionItem` or `Invoke` are missing or
//! fail, the plugin falls back to simulating a mouse click on the tab's center.
//! This requires the window to be visible and may briefly move the cursor before
//! restoring its position.
//!
//! The plugin is Windows-only; on other platforms it returns no results.
use crate::actions::Action;
use crate::plugin::Plugin;
use eframe::egui;
use serde::{Deserialize, Serialize};

pub struct BrowserTabsPlugin {
    recalc_each_query: bool,
}

impl Default for BrowserTabsPlugin {
    fn default() -> Self {
        Self {
            recalc_each_query: false,
        }
    }
}

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
        runtime_id: Vec<i32>,
    }

    static CACHE: Lazy<RwLock<Vec<TabInfo>>> = Lazy::new(|| RwLock::new(Vec::new()));
    static LAST_REFRESH: Lazy<Mutex<Instant>> =
        Lazy::new(|| Mutex::new(Instant::now() - Duration::from_secs(60)));
    static REFRESHING: AtomicBool = AtomicBool::new(false);
    static LAST_ENUM_ERR: Lazy<Mutex<Instant>> =
        Lazy::new(|| Mutex::new(Instant::now() - Duration::from_secs(60)));
    static MESSAGES: Lazy<Mutex<Vec<String>>> = Lazy::new(|| Mutex::new(Vec::new()));

    pub(super) fn take_messages() -> Vec<String> {
        if let Ok(mut list) = MESSAGES.lock() {
            let out = list.clone();
            list.clear();
            out
        } else {
            Vec::new()
        }
    }

    fn push_message(msg: String) {
        if let Ok(mut list) = MESSAGES.lock() {
            list.push(msg);
        }
    }

    fn log_enum_error(msg: &str, err: windows::core::Error) {
        let mut last = LAST_ENUM_ERR.lock().unwrap();
        if last.elapsed() > Duration::from_secs(30) {
            error!(?err, "BrowserTabsPlugin: {msg}");
            *last = Instant::now();
        }
    }

    #[cfg(not(test))]
    fn enumerate_tabs() -> Vec<TabInfo> {
        use windows::core::{BSTR, VARIANT};
        use windows::Win32::System::Com::{
            CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
            COINIT_APARTMENTTHREADED,
        };
        use windows::Win32::UI::Accessibility::*;

        let mut out = Vec::new();
        unsafe {
            if let Err(e) = CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok() {
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
                let mut runtime_id = Vec::new();
                if let Ok(sa_ptr) = elem.GetRuntimeId() {
                    use windows::Win32::System::Ole::{
                        SafeArrayDestroy, SafeArrayLock, SafeArrayUnlock,
                    };
                    if !sa_ptr.is_null() {
                        let psa = sa_ptr as *const _;
                        if SafeArrayLock(psa).is_ok() {
                            let len = (*psa).rgsabound[0].cElements as usize;
                            let data = (*psa).pvData as *const i32;
                            if !data.is_null() {
                                runtime_id = std::slice::from_raw_parts(data, len).to_vec();
                            }
                            let _ = SafeArrayUnlock(psa);
                        }
                        let _ = SafeArrayDestroy(psa);
                    }
                }
                if runtime_id.is_empty() {
                    continue;
                }
                out.push(TabInfo {
                    title,
                    url,
                    runtime_id,
                });
            }

            CoUninitialize();
        }
        out
    }

    #[cfg(not(test))]
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
        push_message("Tab cache refreshed".into());
    }

    #[cfg(test)]
    fn refresh_cache() {
        if let Ok(mut cache) = CACHE.write() {
            cache.clear();
        }
        if let Ok(mut last) = LAST_REFRESH.lock() {
            *last = Instant::now();
        }
        REFRESHING.store(false, Ordering::Release);
    }

    #[cfg(not(test))]
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

    #[cfg(test)]
    fn trigger_refresh() {
        let refresh_needed = {
            if let Ok(last) = LAST_REFRESH.lock() {
                last.elapsed() > Duration::from_secs(2)
            } else {
                false
            }
        };
        if refresh_needed {
            refresh_cache();
        }
    }

    #[cfg(not(test))]
    pub(super) fn force_refresh() {
        if REFRESHING
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            std::thread::spawn(refresh_cache);
        }
    }

    #[cfg(test)]
    pub(super) fn force_refresh() {
        refresh_cache();
    }

    pub(super) fn clear_cache() {
        if let Ok(mut cache) = CACHE.write() {
            cache.clear();
        }
        if let Ok(mut last) = LAST_REFRESH.lock() {
            *last = Instant::now() - Duration::from_secs(60);
        }
        REFRESHING.store(false, Ordering::Release);
        push_message("Tab cache cleared".into());
    }

    pub(super) fn cached_actions(filter: &str, force: bool) -> Vec<Action> {
        if force {
            force_refresh();
        } else {
            trigger_refresh();
        }

        let mut out = Vec::new();
        if let Ok(cache) = CACHE.read() {
            for tab in cache.iter() {
                if filter.is_empty()
                    || tab.title.to_lowercase().contains(filter)
                    || tab.url.to_lowercase().contains(filter)
                {
                    if tab.runtime_id.is_empty() {
                        continue;
                    }
                    let id_str = tab
                        .runtime_id
                        .iter()
                        .map(|i| i.to_string())
                        .collect::<Vec<_>>()
                        .join("_");
                    out.push(Action {
                        label: format!("Switch to {}", tab.title),
                        desc: if tab.url.is_empty() {
                            "Browser Tab".into()
                        } else {
                            tab.url.clone()
                        },
                        action: format!("tab:switch:{id_str}"),
                        args: None,
                    });
                }
            }
        }
        out
    }

    #[cfg(test)]
    mod tests {
        use super::super::BrowserTabsPlugin;
        use super::*;
        use std::thread::sleep;
        use std::time::{Duration, Instant};

        #[test]
        fn search_refreshes_cache() {
            {
                let mut cache = CACHE.write().unwrap();
                cache.clear();
                cache.push(TabInfo {
                    title: "Dummy".into(),
                    url: "about:blank".into(),
                    runtime_id: vec![1],
                });
            }

            {
                let mut last = LAST_REFRESH.lock().unwrap();
                *last = Instant::now();
            }

            let plugin = BrowserTabsPlugin::default();
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

        #[test]
        fn force_refresh_clears_cache() {
            {
                let mut cache = CACHE.write().unwrap();
                cache.clear();
                cache.push(TabInfo {
                    title: "Dummy".into(),
                    url: String::new(),
                    runtime_id: vec![1],
                });
            }

            {
                let mut last = LAST_REFRESH.lock().unwrap();
                *last = Instant::now() - Duration::from_secs(60);
            }

            force_refresh();

            {
                let cache = CACHE.read().unwrap();
                assert!(cache.is_empty());
            }

            {
                let last = LAST_REFRESH.lock().unwrap();
                assert!(last.elapsed() < Duration::from_secs(1));
            }
        }
    }
}

impl Plugin for BrowserTabsPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        const PREFIX: &str = "tab";
        let trimmed = query.trim();
        let rest = match crate::common::strip_prefix_ci(trimmed, PREFIX) {
            Some(r) => r.trim(),
            None => return Vec::new(),
        };

        if rest.eq_ignore_ascii_case("clear") {
            return vec![Action {
                label: "Clear tab cache".into(),
                desc: "Remove cached browser tabs".into(),
                action: "tab:clear".into(),
                args: None,
            }];
        }
        if rest.eq_ignore_ascii_case("cache") {
            return vec![Action {
                label: "Rebuild tab cache".into(),
                desc: "Enumerate browser tabs".into(),
                action: "tab:cache".into(),
                args: None,
            }];
        }

        let filter = rest.to_lowercase();

        imp::cached_actions(&filter, self.recalc_each_query)
    }

    fn name(&self) -> &str {
        "browser_tabs"
    }

    fn description(&self) -> &str {
        "Switch between browser tabs (prefix: `tab`). Uses UI Automation and may simulate a mouse click when activation patterns are unsupported"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "tab".into(),
                desc: "Browser tabs".into(),
                action: "query:tab ".into(),
                args: None,
            },
            Action {
                label: "tab cache".into(),
                desc: "Rebuild browser tab cache".into(),
                action: "tab:cache".into(),
                args: None,
            },
            Action {
                label: "tab clear".into(),
                desc: "Clear browser tab cache".into(),
                action: "tab:clear".into(),
                args: None,
            },
        ]
    }

    fn default_settings(&self) -> Option<serde_json::Value> {
        serde_json::to_value(BrowserTabsPluginSettings {
            recalc_each_query: self.recalc_each_query,
        })
        .ok()
    }

    fn apply_settings(&mut self, value: &serde_json::Value) {
        if let Ok(cfg) = serde_json::from_value::<BrowserTabsPluginSettings>(value.clone()) {
            self.recalc_each_query = cfg.recalc_each_query;
        }
    }

    fn settings_ui(&mut self, ui: &mut egui::Ui, value: &mut serde_json::Value) {
        let mut cfg: BrowserTabsPluginSettings =
            serde_json::from_value(value.clone()).unwrap_or_default();
        ui.checkbox(
            &mut cfg.recalc_each_query,
            "Recalculate cache on each query",
        );
        ui.label(
            "If UI Automation can't activate a tab, a mouse click is simulated and the cursor may briefly move",
        );
        self.recalc_each_query = cfg.recalc_each_query;
        if let Ok(v) = serde_json::to_value(&cfg) {
            *value = v;
        } else {
            tracing::error!("failed to serialize browser tabs settings");
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct BrowserTabsPluginSettings {
    #[serde(default)]
    pub recalc_each_query: bool,
}

impl Default for BrowserTabsPluginSettings {
    fn default() -> Self {
        Self {
            recalc_each_query: false,
        }
    }
}

pub fn take_cache_messages() -> Vec<String> {
    imp::take_messages()
}

pub fn rebuild_cache() {
    imp::force_refresh();
}

pub fn clear_cache() {
    imp::clear_cache();
}
