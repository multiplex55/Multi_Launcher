use crate::actions::Action;
use crate::plugin::{Plugin, PluginSearchUpdates};
use crate::window_catalog::{WindowCatalog, WindowDescriptor};
use std::sync::Arc;

fn actions_from_windows(windows: &[WindowDescriptor], filter: &str) -> Vec<Action> {
    windows
        .iter()
        .filter(|window| filter.is_empty() || window.title.to_lowercase().contains(filter))
        .flat_map(|window| {
            [
                Action {
                    label: format!("Switch to {}", window.title),
                    desc: "Windows".into(),
                    action: format!("window:switch:{}", window.hwnd),
                    args: None,
                },
                Action {
                    label: format!("Close {}", window.title),
                    desc: "Windows".into(),
                    action: format!("window:close:{}", window.hwnd),
                    args: None,
                },
            ]
        })
        .collect()
}

pub struct WindowsPlugin {
    catalog: Arc<WindowCatalog>,
}

impl WindowsPlugin {
    pub(crate) fn new(catalog: Arc<WindowCatalog>) -> Self {
        Self { catalog }
    }

    pub(crate) fn with_updates(updates: Arc<PluginSearchUpdates>) -> Self {
        Self::new(WindowCatalog::production(updates))
    }
}

impl Default for WindowsPlugin {
    fn default() -> Self {
        Self::with_updates(Arc::new(PluginSearchUpdates::default()))
    }
}

impl Plugin for WindowsPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        let Some(rest) = crate::common::strip_prefix_ci(trimmed, "win") else {
            return Vec::new();
        };
        let windows = self.catalog.snapshot_and_refresh();
        actions_from_windows(&windows, &rest.trim().to_lowercase())
    }

    fn name(&self) -> &str {
        "windows"
    }
    fn description(&self) -> &str {
        "Switch or close windows (prefix: `win`)"
    }
    fn capabilities(&self) -> &[&str] {
        &["search"]
    }
    fn query_prefixes(&self) -> &[&str] {
        &["win"]
    }
    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "win".into(),
            desc: "Windows".into(),
            action: "query:win ".into(),
            args: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_window_action_labels_and_protocols_are_preserved() {
        let rows = actions_from_windows(
            &[WindowDescriptor {
                title: "Editor".into(),
                hwnd: 42,
                pid: 7,
                executable: None,
                process_path: None,
                class_name: None,
            }],
            "editor",
        );
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].label, "Switch to Editor");
        assert_eq!(rows[0].action, "window:switch:42");
        assert_eq!(rows[1].action, "window:close:42");
    }
}
