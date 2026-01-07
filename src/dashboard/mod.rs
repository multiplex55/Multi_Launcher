pub mod config;
pub mod dashboard;
pub mod data_cache;
pub mod layout;
pub mod widgets;

pub use dashboard::{Dashboard, DashboardContext, DashboardEvent, WidgetActivation};
pub use data_cache::{DashboardDataCache, DashboardDataSnapshot};
pub use widgets::{WidgetAction, WidgetFactory, WidgetRegistry};
