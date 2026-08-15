//! UI-neutral state for the regional Image Search editor widget.
use crate::mkmacro::{AlphaPolicy, MkWindowMatcher, ReturnPoint, SearchRegion};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotFoundPolicy {
    Error,
    Continue,
    SetOutputs,
}
#[derive(Debug, Clone)]
pub struct ImageSearchEditorState {
    pub asset_reference: String,
    pub preview_error: Option<String>,
    pub region: SearchRegion,
    pub tolerance: u8,
    pub alpha: AlphaPolicy,
    pub return_point: ReturnPoint,
    pub x_variable: String,
    pub y_variable: String,
    pub found_variable: String,
    pub not_found: NotFoundPolicy,
}
impl Default for ImageSearchEditorState {
    fn default() -> Self {
        Self {
            asset_reference: String::new(),
            preview_error: None,
            region: SearchRegion::Desktop,
            tolerance: 0,
            alpha: AlphaPolicy::Compare,
            return_point: ReturnPoint::Center,
            x_variable: String::new(),
            y_variable: String::new(),
            found_variable: String::new(),
            not_found: NotFoundPolicy::Error,
        }
    }
}
pub fn default_window_region() -> SearchRegion {
    SearchRegion::ClientArea {
        matcher: MkWindowMatcher {
            title: None,
            title_regex: None,
            process: None,
            class: None,
        },
    }
}
