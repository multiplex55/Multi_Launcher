pub mod config;
pub mod dashboard;
pub mod data_cache;
pub mod diagnostics;
pub mod layout;
pub mod repaint;
pub mod widgets;

pub use dashboard::{Dashboard, DashboardContext, DashboardEvent, WidgetActivation};
pub use data_cache::{
    DashboardDataCache, DashboardDataSnapshot, DashboardRefreshRequest, DashboardRuntime,
};
pub use diagnostics::{DIAGNOSTICS_REFRESH_INTERVAL, DashboardDiagnosticsSnapshot};
pub use repaint::{
    RepaintDemand, RepaintPolicyInput, aggregate_demands, dashboard_should_render, repaint_interval,
};
pub use widgets::{WidgetAction, WidgetFactory, WidgetRegistry};
