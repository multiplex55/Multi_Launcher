use crate::actions::Action;
use crate::plugin::Plugin;
use tracing::warn;

use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::{CUIAutomation, TreeScope_Subtree, UIA_ControlTypePropertyId, UIA_TabItemControlTypeId};

fn fetch_tabs_uia() -> Vec<(String, String)> {
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

fn default_fetch_tabs() -> Vec<(String, String)> {
    fetch_tabs_uia()
}

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
