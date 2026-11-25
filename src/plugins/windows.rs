use crate::actions::Action;
use crate::plugin::{Plugin, PluginSearchUpdates};
use crate::window_catalog::{WindowCatalog, WindowDescriptor};
use std::sync::Arc;

fn switch_action(window: &WindowDescriptor) -> Action {
    Action {
        label: format!("Switch to {}", window.title),
        desc: "Windows".into(),
        action: format!("window:switch:{}", window.hwnd),
        args: None,
    }
}

fn close_action(window: &WindowDescriptor) -> Action {
    Action {
        label: format!("Close {}", window.title),
        desc: "Windows".into(),
        action: format!("window:close:{}", window.hwnd),
        args: None,
    }
}

fn title_matches(window: &WindowDescriptor, filter: &str) -> bool {
    filter.is_empty() || window.title.to_lowercase().contains(filter)
}

fn actions_from_windows(windows: &[WindowDescriptor], filter: &str) -> Vec<Action> {
    windows
        .iter()
        .filter(|window| title_matches(window, filter))
        .flat_map(|window| {
            [
                switch_action(window),
                close_action(window),
            ]
        })
        .collect()
}

/// A stock Switch action paired with its enriched native target.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowSwitchRow {
    pub action: Action,
    pub identity: crate::window_catalog::WindowSwitchIdentity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowOperation { Switch, Close }

/// Exact stock operation bound to the one freshly enriched native target.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowOperationRow {
    pub operation: WindowOperation,
    pub action: Action,
    pub identity: crate::window_catalog::WindowSwitchIdentity,
    pub published_title: String,
}

#[cfg(test)]
fn switch_rows_from_windows(
    windows: &[WindowDescriptor],
    filter: &str,
    current: impl FnMut(usize) -> Option<(WindowDescriptor, crate::window_catalog::WindowSwitchIdentity)>,
) -> Vec<WindowSwitchRow> {
    switch_rows_from_windows_if_current(windows, filter, current, || true)
}

fn switch_rows_from_windows_if_current(
    windows: &[WindowDescriptor],
    filter: &str,
    mut current: impl FnMut(usize) -> Option<(WindowDescriptor, crate::window_catalog::WindowSwitchIdentity)>,
    is_current: impl Fn() -> bool,
) -> Vec<WindowSwitchRow> {
    operation_rows_from_windows_if_current(windows, filter, &mut current, is_current)
        .into_iter().filter(|row| row.operation == WindowOperation::Switch)
        .map(|row| WindowSwitchRow { action: row.action, identity: row.identity }).collect()
}

fn operation_rows_from_windows_if_current(
    windows: &[WindowDescriptor],
    filter: &str,
    mut current: impl FnMut(usize) -> Option<(WindowDescriptor, crate::window_catalog::WindowSwitchIdentity)>,
    is_current: impl Fn() -> bool,
) -> Vec<WindowOperationRow> {
    windows
        .iter()
        .take_while(|_| is_current())
        .filter_map(|window| {
            let (current, identity) = current(window.hwnd)?;
            if identity.native.hwnd != window.hwnd || identity.native.pid != window.pid
                || current.hwnd != identity.native.hwnd || current.pid != identity.native.pid
                || identity.native.tid == 0 || !identity.descriptor.matches(&current)
                || current.title.is_empty() || !title_matches(&current, filter)
            {
                return None;
            }
            Some([
                WindowOperationRow { operation: WindowOperation::Switch, action: switch_action(&current), identity: identity.clone(), published_title: current.title.clone() },
                WindowOperationRow { operation: WindowOperation::Close, action: close_action(&current), identity, published_title: current.title.clone() },
            ])
        })
        .flatten()
        .collect()
}

/// Retained stock Windows catalog for a frontend's Search owner.
pub struct WindowSwitchCatalog {
    catalog: Arc<WindowCatalog>,
    updates: Arc<PluginSearchUpdates>,
}

impl WindowSwitchCatalog {
    pub fn new() -> Self {
        let updates = Arc::new(PluginSearchUpdates::default());
        Self { catalog: WindowCatalog::production(updates.clone()), updates }
    }

    pub fn set_update_callback(&self, callback: Arc<dyn Fn() + Send + Sync>) {
        self.updates.set_repaint_callback(callback);
    }

    /// Request an asynchronous refresh and enrich matching rows off the UI thread.
    pub fn search_switches(&self, query: &str) -> Vec<WindowSwitchRow> {
        self.search_switches_if_current(query, || true)
    }

    /// Stop enrichment when the caller's Search receipt is superseded.
    pub fn search_switches_if_current(&self, query: &str, is_current: impl Fn() -> bool) -> Vec<WindowSwitchRow> {
        if !is_current() { return Vec::new(); }
        let Some(rest) = crate::common::strip_prefix_ci(query.trim(), "win") else {
            return Vec::new();
        };
        let windows = self.catalog.snapshot_and_refresh();
        let filter = rest.trim().to_lowercase();
        switch_rows_from_windows_if_current(&windows, &filter, crate::window_catalog::current_switch_window, is_current)
    }

    /// Enrich each target once, then project its stock Switch and Close actions.
    pub fn search_operations_if_current(&self, query: &str, is_current: impl Fn() -> bool) -> Vec<WindowOperationRow> {
        if !is_current() { return Vec::new(); }
        let Some(rest) = crate::common::strip_prefix_ci(query.trim(), "win") else { return Vec::new(); };
        let windows = self.catalog.snapshot_and_refresh();
        let filter = rest.trim().to_lowercase();
        operation_rows_from_windows_if_current(&windows, &filter, crate::window_catalog::current_switch_window, is_current)
    }
}

impl Default for WindowSwitchCatalog {
    fn default() -> Self { Self::new() }
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

    #[test]
    fn switch_projection_reuses_stock_filter_and_rejects_reused_process() {
        use crate::window_catalog::{WindowNativeIdentity, WindowSwitchIdentity, WindowTargetIdentity};
        let window = WindowDescriptor {
            title: "Editor".into(), hwnd: 42, pid: 7,
            executable: None, process_path: None, class_name: None,
        };
        let identity = WindowSwitchIdentity {
            native: WindowNativeIdentity { hwnd: 42, pid: 7, tid: 8 },
            descriptor: WindowTargetIdentity::from_descriptor(&window),
        };
        let rows = switch_rows_from_windows(std::slice::from_ref(&window), "edit", |_| Some((window.clone(), identity.clone())));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].action, actions_from_windows(std::slice::from_ref(&window), "edit")[0]);
        assert!(switch_rows_from_windows(std::slice::from_ref(&window), "other", |_| Some((window.clone(), identity.clone()))).is_empty());
        let mut renamed = window.clone();
        renamed.title = "Current title".into();
        assert!(switch_rows_from_windows(std::slice::from_ref(&window), "edit", |_| Some((renamed.clone(), identity.clone()))).is_empty());
        let renamed_rows = switch_rows_from_windows(std::slice::from_ref(&window), "current", |_| Some((renamed.clone(), identity.clone())));
        assert_eq!(renamed_rows[0].action.label, "Switch to Current title");
        let mut reused = identity;
        reused.native.pid += 1;
        assert!(switch_rows_from_windows(std::slice::from_ref(&window), "", |_| Some((window.clone(), reused.clone()))).is_empty());
    }

    #[test]
    fn superseded_window_search_stops_before_next_enrichment() {
        use crate::window_catalog::{WindowNativeIdentity, WindowSwitchIdentity, WindowTargetIdentity};
        use std::cell::Cell;
        let windows = [1, 2, 3].map(|hwnd| WindowDescriptor {
            title: format!("Window {hwnd}"), hwnd, pid: 7,
            executable: None, process_path: None, class_name: None,
        });
        let calls = Cell::new(0);
        let rows = switch_rows_from_windows_if_current(&windows, "", |hwnd| {
            calls.set(calls.get() + 1);
            let descriptor = windows.iter().find(|window| window.hwnd == hwnd).unwrap().clone();
            let identity = WindowSwitchIdentity {
                native: WindowNativeIdentity { hwnd, pid: 7, tid: 9 },
                descriptor: WindowTargetIdentity::from_descriptor(&descriptor),
            };
            Some((descriptor, identity))
        }, || calls.get() < 1);
        assert_eq!(calls.get(), 1);
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn paired_operations_share_one_enriched_target_and_stock_actions() {
        use crate::window_catalog::{WindowNativeIdentity, WindowSwitchIdentity, WindowTargetIdentity};
        use std::cell::Cell;
        let window = WindowDescriptor { title: "Editor".into(), hwnd: 42, pid: 7,
            executable: Some("editor.exe".into()), process_path: None, class_name: None };
        let identity = WindowSwitchIdentity { native: WindowNativeIdentity { hwnd: 42, pid: 7, tid: 9 },
            descriptor: WindowTargetIdentity::from_descriptor(&window) };
        let calls = Cell::new(0);
        let rows = operation_rows_from_windows_if_current(std::slice::from_ref(&window), "edit", |_| {
            calls.set(calls.get() + 1);
            Some((window.clone(), identity.clone()))
        }, || true);
        assert_eq!(calls.get(), 1);
        assert_eq!(rows.iter().map(|row| row.operation).collect::<Vec<_>>(), [WindowOperation::Switch, WindowOperation::Close]);
        assert_eq!(rows.iter().map(|row| row.action.clone()).collect::<Vec<_>>(), actions_from_windows(std::slice::from_ref(&window), "edit"));
        assert!(rows.iter().all(|row| row.identity == identity));
        assert!(rows.iter().all(|row| row.published_title == window.title));
        let mut wrong = identity.clone(); wrong.descriptor.executable = Some("other.exe".into());
        assert!(operation_rows_from_windows_if_current(std::slice::from_ref(&window), "", |_| Some((window.clone(), wrong.clone())), || true).is_empty());
        let mut wrong = identity; wrong.native.tid = 0;
        assert!(operation_rows_from_windows_if_current(std::slice::from_ref(&window), "", |_| Some((window.clone(), wrong.clone())), || true).is_empty());
    }
}
