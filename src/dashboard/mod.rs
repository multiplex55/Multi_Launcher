pub mod config;
pub mod dashboard;
pub mod data_cache;
pub mod diagnostics;
pub(crate) mod editor_presentation;
pub mod layout;
pub mod layout_snapshot;
pub mod checked_editor;
pub mod repaint;
pub mod widgets;

#[cfg(feature = "perf-harness")]
pub use dashboard::perf_fixture::{DashboardCpuFixture, DashboardFrame};
pub use dashboard::{Dashboard, DashboardContext, DashboardEvent, WidgetActivation};
pub use data_cache::{
    DashboardDataCache, DashboardDataSnapshot, DashboardRefreshRequest, DashboardRuntime,
};
pub use diagnostics::{DIAGNOSTICS_REFRESH_INTERVAL, DashboardDiagnosticsSnapshot};
pub use layout_snapshot::DashboardLayout;
pub use repaint::{
    RepaintDemand, RepaintPolicyInput, aggregate_demands, dashboard_should_render, repaint_interval,
};
pub use widgets::{ActiveTimersConfig, QuickToolEntry, QuickToolsConfig, WidgetAction, WidgetFactory, WidgetRegistry};
