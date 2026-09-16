use crate::actions::Action;
pub use crate::commands::ActivationSource;
use crate::dashboard::DashboardEvent;
use std::fmt::Display;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipboardModifyGuiEvent {
    ImmediateOperationComplete,
    ImmediateOperationFailed,
    ConfigurationReloadSuccess,
    ConfigurationReloadFailure(String),
    StartupDiagnosticChanged(Option<String>),
}

#[derive(Clone)]
pub enum WatchEvent {
    Actions,
    Folders,
    Bookmarks,
    Clipboard,
    Snippets,
    Notes,
    Todos,
    Favorites,
    Gestures,
    Dashboard(DashboardEvent),
    Recycle(Result<(), String>),
    ExecuteAction(Action),
    RadialDispatch(crate::radial::handoff::RadialDispatchRequest),
    RadialPrepare(crate::radial::bindings::RadialPrepareEnvelope),
    RadialInvalidate,
    RadialConfigDiagnostic(Option<String>),
    RadialRuntimeDiagnostic(String),
    RadialDiagnostic(crate::radial::diagnostics::RadialDiagnostic),
    RadialSubmenuPlacementFailure(RadialPlacementFailureNotice),
    RadialPlacementActionResult {
        session_id: crate::radial::model::SessionId,
        parent_frame_id: crate::radial::session::FrameId,
        result: Result<(), String>,
    },
    RadialMigrationNotice(String),
    RadialMigrationState {
        receipt: Option<crate::settings::SubmenuPresentationMigrationReceipt>,
        default_submenu_presentation: crate::radial::model::SubmenuPresentation,
    },
    /// Event-driven request from the process-wide launcher hotkey listener.
    ScreenDrawStart,
    /// Launcher hotkey was pressed while Screen Draw owns the foreground flow.
    ScreenDrawRecover,
    /// Process-wide emergency chord was pressed.
    ScreenDrawEmergency,
    ClipboardModify(ClipboardModifyGuiEvent),
    VirtualDesktop(VirtualDesktopGuiCompletion),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RadialPlacementFailureNotice {
    pub session_id: crate::radial::model::SessionId,
    pub parent_frame_id: crate::radial::session::FrameId,
    pub parent_menu_id: crate::radial::model::MenuId,
    pub child_menu_id: crate::radial::model::MenuId,
    pub parent_presentation: crate::radial::model::SubmenuPresentation,
    pub message: String,
}

impl RadialPlacementFailureNotice {
    pub fn can_switch_parent_to_cascade(&self) -> bool {
        self.parent_presentation == crate::radial::model::SubmenuPresentation::SameCenter
    }
}

/// GUI lifecycle for the independent recovery viewport. The request remains
/// active until the renderer has sent Close to that child viewport, allowing
/// successful asynchronous actions to close their surface without touching
/// the launcher viewport.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RadialPlacementViewportState {
    notice: Option<RadialPlacementFailureNotice>,
    present_requested: bool,
    focus_requested: bool,
}

impl RadialPlacementViewportState {
    pub(crate) fn request(&mut self, notice: RadialPlacementFailureNotice) {
        self.notice = Some(notice);
        self.present_requested = true;
        self.focus_requested = true;
    }

    pub(crate) fn notice(&self) -> Option<&RadialPlacementFailureNotice> {
        self.notice.as_ref()
    }

    pub(crate) fn present_requested(&self) -> bool {
        self.present_requested
    }

    pub(crate) fn take_focus_request(&mut self) -> bool {
        std::mem::take(&mut self.focus_requested)
    }

    /// Applies an asynchronous Cascade result only to the exact active
    /// session/frame notice. A successful result clears the message but keeps
    /// the viewport requested for one final frame so it can close itself.
    pub(crate) fn apply_action_result(
        &mut self,
        session_id: &crate::radial::model::SessionId,
        parent_frame_id: crate::radial::session::FrameId,
        result: Result<(), String>,
    ) -> bool {
        let matches = self.notice.as_ref().is_some_and(|notice| {
            notice.session_id == *session_id && notice.parent_frame_id == parent_frame_id
        });
        if !matches {
            return false;
        }
        match result {
            Ok(()) => self.notice = None,
            Err(error) => {
                if let Some(notice) = self.notice.as_mut() {
                    notice.message = format!(
                        "Could not switch {} to Cascade: {error}",
                        notice.parent_menu_id
                    );
                }
            }
        }
        true
    }

    pub(crate) fn update_message(&mut self, message: impl Into<String>) {
        if let Some(notice) = self.notice.as_mut() {
            notice.message = message.into();
        }
    }

    /// Dismiss the active notice while retaining a one-frame viewport request
    /// so the child can receive its own Close command.
    pub(crate) fn dismiss_notice(&mut self) {
        self.notice = None;
        self.focus_requested = false;
    }

    /// The native window was closed or the renderer sent its Close command.
    pub(crate) fn mark_viewport_closed(&mut self) {
        self.notice = None;
        self.present_requested = false;
        self.focus_requested = false;
    }
}

#[derive(Clone, Debug)]
pub struct VirtualDesktopGuiCompletion {
    pub invocation: crate::commands::CommandInvocation,
    pub completion_outcome: crate::commands::CommandOutcome,
    pub history_query: String,
    pub interaction_token: u64,
    pub expected_query: String,
    pub expected_visible: bool,
    pub root_policy: crate::universal_actions::RootLauncherPolicy,
    pub result: Result<(), String>,
}

#[cfg(test)]
mod tests {
    use crate::commands::ActivationSource;

    fn placement_notice() -> super::RadialPlacementFailureNotice {
        super::RadialPlacementFailureNotice {
            session_id: crate::radial::model::SessionId::new("session-a"),
            parent_frame_id: crate::radial::session::FrameId(7),
            parent_menu_id: crate::radial::model::MenuId::new("parent"),
            child_menu_id: crate::radial::model::MenuId::new("child"),
            parent_presentation: crate::radial::model::SubmenuPresentation::SameCenter,
            message: "fixed center does not fit".into(),
        }
    }

    #[test]
    fn activation_source_labels_are_stable() {
        assert_eq!(ActivationSource::Enter.label(), "enter");
        assert_eq!(ActivationSource::Click.label(), "click");
        assert_eq!(ActivationSource::Dashboard.label(), "dashboard");
        assert_eq!(ActivationSource::Gesture.label(), "gesture");
        assert_eq!(ActivationSource::Macro.label(), "macro");
        assert_eq!(ActivationSource::RadialRelease.label(), "radial_release");
        assert_eq!(ActivationSource::RadialShortcut.label(), "radial_shortcut");
        assert_eq!(
            ActivationSource::RadialHotstring.label(),
            "radial_hotstring"
        );
    }

    #[test]
    fn placement_viewport_correlates_actions_and_waits_for_child_close() {
        let mut viewport = super::RadialPlacementViewportState::default();
        let notice = placement_notice();
        viewport.request(notice.clone());

        assert!(viewport.present_requested());
        assert_eq!(viewport.notice(), Some(&notice));
        assert!(viewport.take_focus_request());
        assert!(!viewport.take_focus_request());

        assert!(!viewport.apply_action_result(
            &crate::radial::model::SessionId::new("stale-session"),
            notice.parent_frame_id,
            Err("stale".into()),
        ));
        assert_eq!(viewport.notice(), Some(&notice));

        assert!(viewport.apply_action_result(
            &notice.session_id,
            notice.parent_frame_id,
            Err("revision conflict".into()),
        ));
        assert!(
            viewport
                .notice()
                .is_some_and(|active| active.message.contains("revision conflict"))
        );
        assert!(viewport.present_requested());

        assert!(viewport.apply_action_result(&notice.session_id, notice.parent_frame_id, Ok(()),));
        assert!(viewport.notice().is_none());
        assert!(viewport.present_requested());
        viewport.mark_viewport_closed();
        assert!(!viewport.present_requested());
    }

    #[test]
    fn placement_viewport_dismissal_keeps_its_close_request_not_the_launcher() {
        let mut viewport = super::RadialPlacementViewportState::default();
        viewport.request(placement_notice());

        viewport.dismiss_notice();
        assert!(viewport.notice().is_none());
        assert!(viewport.present_requested());

        viewport.mark_viewport_closed();
        assert!(!viewport.present_requested());
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiErrorEvent {
    pub context: &'static str,
    pub message: String,
}

impl UiErrorEvent {
    pub fn new(context: &'static str, err: impl Display) -> Self {
        Self {
            context,
            message: err.to_string(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct PendingConfirmCommand {
    pub(crate) invocation: crate::commands::CommandInvocation,
}

#[derive(Clone)]
pub(crate) struct AuthoringActionRevalidation {
    pub(crate) binding: crate::radial::model::ActionBinding,
    pub(crate) invocation: crate::radial::context::InvocationContext,
    pub(crate) captured_identity: Option<crate::window_catalog::WindowTargetIdentity>,
    pub(crate) window_catalog_generation: u64,
}

#[derive(Clone)]
pub(crate) struct PendingUniversalActionInvocation {
    pub(crate) action: crate::universal_actions::UniversalAction,
    pub(crate) context: crate::universal_actions::UniversalActionInvocationContext,
    pub(crate) radial_request: Option<crate::radial::handoff::RadialDispatchRequest>,
    pub(crate) authoring_revalidation: Option<AuthoringActionRevalidation>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TestWatchEvent {
    Actions,
    Folders,
    Bookmarks,
    ScreenDrawRecover,
    ScreenDrawEmergency,
    ClipboardModify(ClipboardModifyGuiEvent),
}

impl From<WatchEvent> for TestWatchEvent {
    fn from(value: WatchEvent) -> Self {
        match value {
            WatchEvent::Actions => TestWatchEvent::Actions,
            WatchEvent::Folders => TestWatchEvent::Folders,
            WatchEvent::Bookmarks => TestWatchEvent::Bookmarks,
            WatchEvent::Clipboard => TestWatchEvent::Actions,
            WatchEvent::Snippets => TestWatchEvent::Actions,
            WatchEvent::Notes => TestWatchEvent::Actions,
            WatchEvent::Todos => TestWatchEvent::Actions,
            WatchEvent::Favorites => TestWatchEvent::Actions,
            WatchEvent::Gestures => TestWatchEvent::Actions,
            WatchEvent::Dashboard(_) => TestWatchEvent::Actions,
            WatchEvent::Recycle(_) => unreachable!(),
            WatchEvent::ExecuteAction(_) => TestWatchEvent::Actions,
            WatchEvent::RadialDispatch(_) => TestWatchEvent::Actions,
            WatchEvent::RadialPrepare(_) => TestWatchEvent::Actions,
            WatchEvent::RadialInvalidate => TestWatchEvent::Actions,
            WatchEvent::RadialConfigDiagnostic(_) => TestWatchEvent::Actions,
            WatchEvent::RadialRuntimeDiagnostic(_) => TestWatchEvent::Actions,
            WatchEvent::RadialDiagnostic(_) => TestWatchEvent::Actions,
            WatchEvent::RadialSubmenuPlacementFailure(_) => TestWatchEvent::Actions,
            WatchEvent::RadialPlacementActionResult { .. } => TestWatchEvent::Actions,
            WatchEvent::RadialMigrationNotice(_) => TestWatchEvent::Actions,
            WatchEvent::RadialMigrationState { .. } => TestWatchEvent::Actions,
            WatchEvent::ScreenDrawStart => TestWatchEvent::Actions,
            WatchEvent::ScreenDrawRecover => TestWatchEvent::ScreenDrawRecover,
            WatchEvent::ScreenDrawEmergency => TestWatchEvent::ScreenDrawEmergency,
            WatchEvent::ClipboardModify(event) => TestWatchEvent::ClipboardModify(event),
            WatchEvent::VirtualDesktop(_) => TestWatchEvent::Actions,
        }
    }
}
