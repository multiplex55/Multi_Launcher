pub mod config;
pub mod dashboard;
pub mod layout;
pub mod widgets;

pub use dashboard::{Dashboard, DashboardContext, DashboardEvent, WidgetActivation};
pub use widgets::{WidgetAction, WidgetFactory, WidgetRegistry};
