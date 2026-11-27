//! Checked Mouse Gesture Query Set receipts for the root presentation owner.
use super::*;
use multi_launcher::plugins::mouse_gestures::{GestureRelease, GpuiGestureOwner};

#[cfg(windows)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PriorWindowState {
    Hidden,
    Minimized,
    Visible,
}

#[cfg(windows)]
pub(super) fn prior_window_state(native: NativeWindow) -> Option<PriorWindowState> {
    use windows::Win32::{Foundation::HWND, UI::WindowsAndMessaging::IsIconic};
    let visible = native_window_is_visible(native).ok()?;
    if !visible {
        Some(PriorWindowState::Hidden)
    } else if unsafe { IsIconic(HWND(native.identity.hwnd as *mut c_void)) }.as_bool() {
        Some(PriorWindowState::Minimized)
    } else {
        Some(PriorWindowState::Visible)
    }
}

#[cfg(windows)]
pub(super) fn rollback_show(native: NativeWindow, prior: PriorWindowState) -> bool {
    use windows::Win32::{
        Foundation::HWND,
        UI::WindowsAndMessaging::{SW_HIDE, SW_SHOWMINNOACTIVE, ShowWindow},
    };
    if !native_window_is_valid(native) {
        return false;
    }
    let Some(observed) = prior_window_state(native) else {
        return false;
    };
    if observed == prior {
        return true;
    }
    if observed != PriorWindowState::Visible {
        return false;
    }
    let command = match prior {
        PriorWindowState::Hidden => SW_HIDE,
        PriorWindowState::Minimized => SW_SHOWMINNOACTIVE,
        PriorWindowState::Visible => return true,
    };
    unsafe {
        let _ = ShowWindow(HWND(native.identity.hwnd as *mut c_void), command);
    }
    prior_window_state(native) == Some(prior)
}

fn receipt_current(
    root_matches: bool,
    foreground_matches: bool,
    revision_matches: bool,
    fenced: bool,
    binding_current: bool,
) -> bool {
    root_matches && foreground_matches && revision_matches && !fenced && binding_current
}

pub(super) fn supersedes_pending_gesture(
    pending_gesture: bool,
    incoming_gesture: bool,
    pending_current: bool,
) -> bool {
    pending_gesture && (!incoming_gesture || !pending_current)
}

pub(super) fn committed_return_target(
    previous: Option<ForegroundIdentity>,
    source: ForegroundIdentity,
    root: ForegroundIdentity,
) -> Option<ForegroundIdentity> {
    if source == root {
        previous
    } else {
        Some(source)
    }
}

#[cfg(any(test, windows))]
pub(super) fn owns_native_show(
    owner_active: bool,
    revision_current: bool,
    root_current: bool,
    show_issued: bool,
) -> bool {
    owner_active && revision_current && root_current && show_issued
}

fn query_outcome(action: Action) -> Option<CommandOutcome> {
    let invocation = parse_command(action, None, ActivationSource::Gesture).ok()?;
    let Command::Query(command @ QueryCommand::Set { .. }) = invocation.command else {
        return None;
    };
    Some(handle_query(&command, ActivationSource::Gesture))
}

#[derive(Clone)]
pub(super) struct Request {
    pub owner: Arc<GpuiGestureOwner>,
    pub release: GestureRelease,
    pub root: NativeWindow,
    pub revision: u64,
}

impl Request {
    pub fn source(&self) -> ForegroundIdentity {
        let source = self.release.foreground;
        ForegroundIdentity {
            hwnd: source.hwnd,
            process_id: source.process_id,
            thread_id: source.thread_id,
        }
    }
    pub fn current(
        &self,
        root: NativeWindow,
        foreground: Option<ForegroundIdentity>,
        revision: u64,
        fenced: bool,
    ) -> bool {
        receipt_current(
            self.root == root,
            foreground == Some(self.source()),
            self.revision == revision,
            fenced,
            self.owner.is_current(&self.release),
        )
    }
    pub fn outcome(&self) -> Option<CommandOutcome> {
        query_outcome(self.release.action.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::super::ForegroundIdentity;
    use super::super::query_set_outcome_text;
    use super::{
        committed_return_target, owns_native_show, query_outcome, receipt_current,
        supersedes_pending_gesture,
    };
    use multi_launcher::actions::Action;
    use multi_launcher::commands::{QueryPolicy, VisibilityPolicy};
    #[test]
    fn gesture_receipt_requires_exact_root_foreground_revision_and_binding() {
        assert!(receipt_current(true, true, true, false, true));
        assert!(!receipt_current(false, true, true, false, true));
        assert!(!receipt_current(true, false, true, false, true));
        assert!(!receipt_current(true, true, false, false, true));
        assert!(!receipt_current(true, true, true, true, true));
        assert!(!receipt_current(true, true, true, false, false));
    }
    #[test]
    fn gesture_query_uses_stock_show_and_exact_text() {
        let action = Action {
            label: "gesture".into(),
            desc: "Mouse gesture".into(),
            action: "query:  exact text".into(),
            args: None,
        };
        let outcome = query_outcome(action).unwrap();
        assert_eq!(outcome.query, QueryPolicy::Set("  exact text".into()));
        assert_eq!(outcome.visibility, VisibilityPolicy::Show);
        assert!(outcome.restore && outcome.focus && outcome.move_cursor_end);
        assert!(query_set_outcome_text(&outcome).is_some());
        let other = Action {
            label: "other".into(),
            desc: "Mouse gesture".into(),
            action: "launcher:toggle".into(),
            args: None,
        };
        assert!(query_outcome(other).is_none());
        let multiline = Action {
            label: "gesture".into(),
            desc: "Mouse gesture".into(),
            action: "query:one\ntwo".into(),
            args: None,
        };
        assert!(query_set_outcome_text(&query_outcome(multiline).unwrap()).is_none());
    }
    #[test]
    fn established_visibility_requests_take_priority_over_gestures() {
        assert!(supersedes_pending_gesture(true, false, true));
        assert!(!supersedes_pending_gesture(true, true, true));
        assert!(supersedes_pending_gesture(true, true, false));
        assert!(!supersedes_pending_gesture(false, false, false));
    }
    #[test]
    fn gesture_commit_preserves_prior_return_target_when_root_was_foreground() {
        let root = ForegroundIdentity {
            hwnd: 1,
            process_id: 2,
            thread_id: 3,
        };
        let previous = ForegroundIdentity {
            hwnd: 4,
            process_id: 5,
            thread_id: 6,
        };
        let external = ForegroundIdentity {
            hwnd: 7,
            process_id: 8,
            thread_id: 9,
        };
        assert_eq!(
            committed_return_target(Some(previous), root, root),
            Some(previous)
        );
        assert_eq!(
            committed_return_target(Some(previous), external, root),
            Some(external)
        );
    }
    #[test]
    fn failed_show_compensation_uses_effect_ownership_not_source_authority() {
        assert!(owns_native_show(true, true, true, true));
        assert!(!owns_native_show(true, false, true, true));
        assert!(!owns_native_show(true, true, false, true));
        assert!(!owns_native_show(true, true, true, false));
        assert!(!owns_native_show(false, true, true, true));
    }
}

impl RootWindow {
    pub(super) fn start_gesture_dispatch(
        &mut self,
        startup_error: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if let Some(error) = startup_error {
            self.gesture_status = Some(format!("Mouse Gestures unavailable: {error}"));
        }
        let Some(owner) = self.gesture_owner.clone() else {
            return;
        };
        let wake = self.gesture_wake.clone();
        cx.spawn(async move |root, cx| {
            while wake.wait().await {
                let releases = owner.take();
                let status = owner.take_status();
                if root
                    .update(cx, |root, cx| {
                        if let Some(status) = status {
                            root.gesture_status =
                                status.map(|error| format!("Mouse Gestures: {error}"));
                        }
                        for release in releases {
                            let Some(native) = root.native_window else {
                                continue;
                            };
                            let revision = release.visibility_revision;
                            let request = Request {
                                owner: owner.clone(),
                                release,
                                root: native,
                                revision,
                            };
                            if request.current(
                                native,
                                current_foreground_identity(),
                                request.revision,
                                root.restart_admission.is_fenced(),
                            ) && native_window_is_valid(NativeWindow {
                                identity: request.source(),
                            }) {
                                root.root_visibility
                                    .queue_visible(RootVisibilityRequest::GestureQuery(request));
                            }
                        }
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }
}
