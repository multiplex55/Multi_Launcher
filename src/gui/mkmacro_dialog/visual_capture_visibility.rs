//! UI-thread bridge between the capture workflow and Launcher viewport state.
use super::visual_capture_workflow::{SavedVisibility, VisibilityAdapter};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct State {
    launcher_visible: bool,
    mkmacro_open: bool,
    hide_pending: bool,
    hide_applied: bool,
    hidden_observed: bool,
    restore: Option<SavedVisibility>,
}

#[derive(Clone, Default)]
pub struct LauncherVisibilityBridge(Arc<Mutex<State>>);

/// Adapter owned by the workflow; it contains no egui or Launcher borrow.
pub struct LauncherVisualCaptureVisibility(pub LauncherVisibilityBridge);

pub enum VisibilityRequest {
    None,
    Hide,
    Restore(SavedVisibility),
}

impl LauncherVisibilityBridge {
    /// Called once, early, by the UI thread. A hide is only acknowledged on
    /// the frame after the frame which emitted `ViewportCommand::Visible(false)`.
    pub fn process_frame(&self, launcher_visible: bool, mkmacro_open: bool) -> VisibilityRequest {
        let mut s = self.0.lock().unwrap();
        s.launcher_visible = launcher_visible;
        s.mkmacro_open = mkmacro_open;
        if let Some(saved) = s.restore.take() {
            s.hide_pending = false;
            s.hide_applied = false;
            s.hidden_observed = false;
            return VisibilityRequest::Restore(saved);
        }
        if s.hide_applied {
            s.hidden_observed = true;
        }
        if s.hide_pending && !s.hide_applied {
            s.hide_applied = true;
            return VisibilityRequest::Hide;
        }
        VisibilityRequest::None
    }
    pub fn pending(&self) -> bool {
        let s = self.0.lock().unwrap();
        s.hide_pending || s.restore.is_some()
    }
}
impl VisibilityAdapter for LauncherVisualCaptureVisibility {
    fn snapshot(&self) -> SavedVisibility {
        let s = self.0.0.lock().unwrap();
        SavedVisibility {
            launcher: s.launcher_visible,
            mkmacro_dialog: s.mkmacro_open,
        }
    }
    fn request_hidden(&mut self) {
        let mut s = self.0.0.lock().unwrap();
        s.hide_pending = true;
        s.hide_applied = false;
        s.hidden_observed = false;
    }
    fn hidden_observed(&self) -> bool {
        self.0.0.lock().unwrap().hidden_observed
    }
    fn restore(&mut self, saved: SavedVisibility) {
        self.0.0.lock().unwrap().restore = Some(saved);
    }
}
