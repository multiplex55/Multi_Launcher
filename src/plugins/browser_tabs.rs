use crate::actions::Action;
use crate::plugin::Plugin;
use tracing::warn;

#[cfg(not(windows))]
use serde::Deserialize;
#[cfg(not(windows))]
use std::time::Duration;

#[cfg(not(windows))]
#[derive(Deserialize)]
struct DebugTab {
    title: String,
    url: String,
}

#[cfg(not(windows))]
fn fetch_tabs_remote_debug() -> Vec<(String, String)> {
    let ports = [9222u16, 9223, 9224, 9225, 9226, 9227, 9228, 9229];
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(200))
        .build();
    let mut tabs = Vec::new();

    // Stop after a few consecutive failures to avoid long hangs when no browsers are
    // exposing a debugging port.
    const MAX_CONSECUTIVE_FAILS: u8 = 3;
    let mut fails = 0u8;

    if let Ok(client) = client {
        for port in ports {
            let url = format!("http://127.0.0.1:{port}/json");
            match client.get(&url).send() {
                Ok(resp) => {
                    fails = 0; // reset after a success
                    match resp.text() {
                        Ok(text) => match serde_json::from_str::<Vec<DebugTab>>(&text) {
                            Ok(list) => {
                                for item in list {
                                    if !item.title.is_empty() && !item.url.is_empty() {
                                        tabs.push((item.title, item.url));
                                    }
                                }
                            }
                            Err(e) => warn!(?e, %url, "failed to parse tab list"),
                        },
                        Err(e) => warn!(?e, %url, "failed to read tab list"),
                    }
                }
                Err(e) => {
                    fails += 1;
                    warn!(?e, %url, "failed to query debug port");
                    if fails >= MAX_CONSECUTIVE_FAILS {
                        break;
                    }
                }
            }
        }
    }
    tabs
}

#[cfg(windows)]
fn fetch_tabs_uia() -> Vec<(String, String)> {
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
    use windows::Win32::System::Variant::VARIANT;
    use windows::Win32::UI::Accessibility::{CUIAutomation, TreeScope_Subtree, UIA_ControlTypePropertyId, UIA_TabItemControlTypeId};

    let mut tabs = Vec::new();
    unsafe {
        if CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_err() {
            return tabs;
        }
        let automation = match CUIAutomation::new() {
            Ok(a) => a,
            Err(e) => {
                warn!(?e, "failed to create UIAutomation instance");
                CoUninitialize();
                return tabs;
            }
        };
        let root = match automation.GetRootElement() {
            Ok(r) => r,
            Err(e) => {
                warn!(?e, "failed to get root element");
                CoUninitialize();
                return tabs;
            }
        };
        let cond = match automation.CreatePropertyCondition(
            UIA_ControlTypePropertyId,
            &VARIANT::from(UIA_TabItemControlTypeId.0 as i32),
        ) {
            Ok(c) => c,
            Err(e) => {
                warn!(?e, "failed to create property condition");
                CoUninitialize();
                return tabs;
            }
        };
        let elements = match root.FindAll(TreeScope_Subtree, &cond) {
            Ok(arr) => arr,
            Err(e) => {
                warn!(?e, "failed to search for tab items");
                CoUninitialize();
                return tabs;
            }
        };
        let length = elements.Length().unwrap_or(0);
        for i in 0..length {
            if let Ok(el) = elements.GetElement(i) {
                if let Ok(name) = el.CurrentName() {
                    let title = name.to_string();
                    if !title.is_empty() {
                        // UI Automation does not expose URLs directly.
                        tabs.push((title, String::new()));
                    }
                }
            }
        }
        CoUninitialize();
    }
    tabs
}

#[cfg(windows)]
fn default_fetch_tabs() -> Vec<(String, String)> {
    fetch_tabs_uia()
}

#[cfg(not(windows))]
fn default_fetch_tabs() -> Vec<(String, String)> {
    fetch_tabs_remote_debug()
}

/// A plugin that lists open browser tabs.
///
/// # Supported browsers and platforms
///
/// - **Windows**: Uses Windows UI Automation to enumerate tab items from browser
///   windows. Only tab titles are available; URLs are currently unavailable
///   through this API.
/// - **Non-Windows**: Queries Chromium-based browsers such as Chrome and Edge via
///   their remote debugging HTTP endpoints on ports `9222-9229`. Browsers must be
///   launched with `--remote-debugging-port` enabled for detection.
///
/// The plugin returns an empty list if no supported browsers are found or the
/// above mechanisms are unavailable.
pub struct BrowserTabsPlugin {
    fetch_tabs: fn() -> Vec<(String, String)>,
}

impl Default for BrowserTabsPlugin {
    fn default() -> Self {
        Self {
            fetch_tabs: default_fetch_tabs,
        }
    }
}

impl BrowserTabsPlugin {
    pub fn new_with_fetch(fetch_tabs: fn() -> Vec<(String, String)>) -> Self {
        Self { fetch_tabs }
    }
}

impl Plugin for BrowserTabsPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        const PREFIX: &str = "tab";
        let rest = match crate::common::strip_prefix_ci(trimmed, PREFIX) {
            Some(r) => r.trim(),
            None => return Vec::new(),
        };

        let tabs = (self.fetch_tabs)();
        let filter = rest.to_lowercase();
        tabs.into_iter()
            .filter(|(title, url)| {
                if filter.is_empty() {
                    true
                } else {
                    title.to_lowercase().contains(&filter) || url.to_lowercase().contains(&filter)
                }
            })
            .map(|(title, url)| Action {
                label: title,
                desc: url.clone(),
                action: url,
                args: None,
            })
            .collect()
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
            desc: "Browser tabs".into(),
            action: "query:tab ".into(),
            args: None,
        }]
    }
}
