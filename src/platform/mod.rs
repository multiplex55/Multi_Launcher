pub mod app_data;
pub mod gdi_stroke;
pub mod pixels;
pub mod single_instance;
pub mod windows_api;
pub(crate) mod windows_capture;
#[cfg(all(test, windows))]
pub(crate) mod windows_capture_test_support;

pub(crate) mod passive_tooltip;
