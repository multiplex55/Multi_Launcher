//! Single-flight ownership for ordinary external launcher actions.
use std::sync::Arc;

use gpui_kit::*;
use multi_launcher::actions::Action;
use multi_launcher::commands::{
    ActivationSource, CommandBus, CommandError, CommandOutcome, ExternalCommand,
    ExternalCommandHost, HistoryPolicy,
};
use multi_launcher::history::HistoryEntry;

use super::{Context, FocusHandle, Ordering, RootWindow, Window};
use crate::gpui_restart::{EffectPermit, RestartAdmission};
use gpui_kit::component::{
    WindowExt as _,
    notification::{Notification, NotificationDelivery},
};

pub(super) fn saved_external(action: &Action) -> Option<ExternalCommand> {
    ordinary_external(&multi_launcher::commands::parse_action(action).ok()?).cloned()
}

pub(super) fn ordinary_external(
    command: &multi_launcher::commands::Command,
) -> Option<&ExternalCommand> {
    let multi_launcher::commands::Command::External(command) = command else {
        return None;
    };
    (command.namespace.is_none() && ordinary_executable(&command.target)).then_some(command)
}

fn ordinary_executable(target: &str) -> bool {
    if target.is_empty()
        || target.trim() != target
        || target.starts_with("\\\\?\\")
        || target.starts_with("\\\\.\\")
        || target.starts_with("//?/")
        || target.starts_with("//./")
        || target
            .chars()
            .any(|c| c.is_control() || matches!(c, '"' | '<' | '>' | '|' | '?' | '*'))
    {
        return false;
    }
    let bytes = target.as_bytes();
    let drive = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
    if target
        .bytes()
        .enumerate()
        .any(|(i, b)| b == b':' && !(drive && i == 1))
    {
        return false;
    }
    let path = if drive { &target[2..] } else { target };
    if path.is_empty() {
        return false;
    }
    let unc = path.starts_with("\\\\") || path.starts_with("//");
    let parts: Vec<_> = path
        .split(['\\', '/'])
        .filter(|part| !part.is_empty())
        .collect();
    if parts.is_empty()
        || (unc && parts.len() < 3)
        || !std::path::Path::new(parts[parts.len() - 1])
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
    {
        return false;
    }
    parts.iter().all(|part| {
        if *part == "." || *part == ".." {
            return true;
        }
        if part.ends_with(' ') || part.ends_with('.') {
            return false;
        }
        let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        !matches!(
            stem.as_str(),
            "CON"
                | "PRN"
                | "AUX"
                | "NUL"
                | "COM1"
                | "COM2"
                | "COM3"
                | "COM4"
                | "COM5"
                | "COM6"
                | "COM7"
                | "COM8"
                | "COM9"
                | "LPT1"
                | "LPT2"
                | "LPT3"
                | "LPT4"
                | "LPT5"
                | "LPT6"
                | "LPT7"
                | "LPT8"
                | "LPT9"
        )
    })
}

fn presentation_owner_matches<T: PartialEq>(
    origin: Option<&T>,
    current: Option<&T>,
    blocked: bool,
) -> bool {
    !blocked && origin.is_some() && origin == current
}

fn deferred_inline_is_current(
    expected_generation: u64,
    generation: u64,
    expected_query: &str,
    results_query: &str,
    live_query: &str,
    owner_current: bool,
) -> bool {
    owner_current
        && generation == expected_generation
        && results_query == expected_query
        && live_query == expected_query
}

pub(super) const CLOSE_STATUS: &str =
    "Wait for the application launch to finish before closing Multi Launcher.";

#[derive(Clone)]
pub(crate) struct Settings {
    pub(crate) clear_query_after_run: bool,
    pub(crate) hide_after_run: bool,
    pub(crate) history_limit: usize,
    pub(crate) enable_toasts: bool,
    pub(crate) show_error_toasts: bool,
    pub(crate) show_inline_errors: bool,
    pub(crate) toast_duration: f32,
}

pub(super) struct Stamp {
    pub root: WindowId,
    pub generation: u64,
    pub visibility_revision: u64,
    pub query: String,
    pub focus: Option<FocusHandle>,
}

impl Stamp {
    pub fn presentation_is_current(
        &self,
        root: WindowId,
        generation: u64,
        revision: u64,
        query: &str,
        visible: bool,
    ) -> bool {
        visible
            && self.root == root
            && self.generation == generation
            && self.visibility_revision == revision
            && self.query == query
    }

    pub fn may_refocus(&self, root: WindowId, active: bool) -> bool {
        active && self.root == root
    }
}

#[derive(Clone)]
pub(super) struct Request {
    pub action: Action,
    pub command: ExternalCommand,
    pub query: String,
    pub source: ActivationSource,
    pub settings: Settings,
}

pub(super) struct WorkerResult {
    pub outcome: Result<CommandOutcome, CommandError>,
    pub history_error: Option<String>,
    pub usage_result: Option<Result<u32, String>>,
}

pub(super) struct Receipt {
    pub stamp: Stamp,
    pub request: Request,
    pub result: WorkerResult,
    // The restart fence remains held until the caller consumes this receipt.
    _permit: EffectPermit,
}

struct Pending {
    ticket: u64,
    stamp: Stamp,
    permit: EffectPermit,
}

#[derive(Default)]
pub(super) struct ExternalLaunch {
    pending: Option<Pending>,
    next_ticket: u64,
}

impl ExternalLaunch {
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub fn begin(
        &mut self,
        admission: &Arc<RestartAdmission>,
        stamp: Stamp,
        request: &Request,
    ) -> Option<u64> {
        if self.pending.is_some() || request.query != stamp.query {
            return None;
        }
        let permit = admission.admit()?;
        self.next_ticket = self.next_ticket.wrapping_add(1).max(1);
        let ticket = self.next_ticket;
        self.pending = Some(Pending {
            ticket,
            stamp,
            permit,
        });
        Some(ticket)
    }

    pub fn finish(
        &mut self,
        ticket: u64,
        request: Request,
        result: WorkerResult,
    ) -> Option<Receipt> {
        if self.pending.as_ref()?.ticket != ticket {
            return None;
        }
        let pending = self.pending.take()?;
        Some(Receipt {
            stamp: pending.stamp,
            request,
            result,
            _permit: pending.permit,
        })
    }
}

struct Host {
    settings: Settings,
}
impl ExternalCommandHost for Host {
    fn execute_external_command(
        &mut self,
        command: &ExternalCommand,
        _: &Action,
    ) -> anyhow::Result<()> {
        multi_launcher::actions::exec::launch(&command.target, command.args.as_deref())
    }
    fn external_clear_query_after_run(&self) -> bool {
        self.settings.clear_query_after_run
    }
    fn external_hide_after_run(&self) -> bool {
        self.settings.hide_after_run
    }
    fn external_launcher_should_refocus(&self) -> bool {
        true
    }
}

pub(super) fn run(request: &Request) -> WorkerResult {
    let mut host = Host {
        settings: request.settings.clone(),
    };
    let outcome = CommandBus.dispatch_external(
        &request.command,
        &request.action,
        request.source,
        None,
        &mut host,
    );
    if let Ok(outcome) = &outcome {
        if let multi_launcher::commands::FavoriteLogPolicy::Ran { label, command } =
            &outcome.favorite_log
        {
            tracing::info!(fav = %label, command = %command, "ran favorite");
        }
    } else if let Err(error) = &outcome {
        multi_launcher::toast_log::append_toast_log(&format!("[error:launcher] {}", error.message));
        if let Some(favorite) = &error.favorite {
            tracing::warn!(fav = %favorite, error = %error.message, "favorite failed");
        }
    }
    let result = bookkeep(
        outcome,
        &request,
        |entry, limit| {
            multi_launcher::history::append_history(entry, limit).map_err(|error| error.to_string())
        },
        |key| {
            multi_launcher::usage::record_usage(multi_launcher::usage::USAGE_FILE, key)
                .map_err(|error| error.to_string())
        },
    );
    for error in result
        .history_error
        .iter()
        .chain(result.usage_result.as_ref().and_then(|r| r.as_ref().err()))
    {
        multi_launcher::toast_log::append_toast_log(&format!("[error:launcher] {error}"));
    }
    result
}

struct ExternalNotification;

impl RootWindow {
    fn external_owner_is_current(
        &self,
        origin: Option<&FocusHandle>,
        window: &Window,
        cx: &Context<Self>,
    ) -> bool {
        let contexts = window.context_stack();
        let blocked = self.action_sheet.is_some()
            || self.context_favorite_popup.is_some()
            || self.file_menu_open
            || self.settings_menu_open
            || self.help_menu_open
            || super::context_stack_has_primary(&contexts, "PopupMenu")
            || super::context_stack_has_primary(&contexts, "AlertDialog")
            || super::context_stack_has_primary(&contexts, "Dialog");
        let current = window.focused(cx);
        presentation_owner_matches(origin, current.as_ref(), blocked)
    }

    pub(super) fn start_external_launch(
        &mut self,
        action: Action,
        command: ExternalCommand,
        query: String,
        source: ActivationSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(root) = self.root_window else {
            return;
        };
        let stamp = Stamp {
            root: root.window_id(),
            generation: self.launcher_results.generation,
            visibility_revision: self
                .root_visibility
                .request_revision
                .load(Ordering::Acquire),
            query: query.clone(),
            focus: window.focused(cx),
        };
        let request = Request {
            action,
            command,
            query,
            source,
            settings: self.external_launch_preferences.clone(),
        };
        let Some(ticket) = self
            .external_launch
            .begin(&self.restart_admission, stamp, &request)
        else {
            return;
        };
        self.query_history.reset();
        let task = cx.background_executor().spawn(async move {
            let result = run(&request);
            (request, result)
        });
        let root = cx.entity().downgrade();
        let window_handle = window.window_handle();
        cx.spawn(async move |_, cx| {
            let (request, result) = task.await;
            let _ = window_handle.update(cx, |_, window, app| {
                let _ = root.update(app, |root, cx| {
                    let Some(receipt) = root.external_launch.finish(ticket, request, result) else {
                        return;
                    };
                    root.apply_external_receipt(ticket, receipt, window, cx);
                });
            });
        })
        .detach();
    }

    fn apply_external_receipt(
        &mut self,
        ticket: u64,
        receipt: Receipt,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.restart_status.as_deref() == Some(CLOSE_STATUS) {
            self.restart_status = None;
        }
        if receipt.result.outcome.is_ok() {
            let count = self
                .search_catalog
                .usage
                .entry(receipt.request.action.action.clone())
                .or_default();
            *count = match &receipt.result.usage_result {
                Some(Ok(canonical)) => *canonical,
                _ => count.saturating_add(1),
            };
        }
        let Some(root) = self.root_window else {
            cx.notify();
            return;
        };
        let revision = self
            .root_visibility
            .request_revision
            .load(Ordering::Acquire);
        let mut visible = self.root_visibility.visible.load(Ordering::Acquire)
            && self.root_visibility.active.load(Ordering::Acquire);
        #[cfg(windows)]
        {
            visible &= self.native_window.is_some_and(|native| {
                self.root_visibility
                    .is_current_visible_revision(revision, native)
            });
        }
        let local_focus_owned =
            self.external_owner_is_current(receipt.stamp.focus.as_ref(), window, cx);
        let current = receipt.stamp.presentation_is_current(
            root.window_id(),
            self.launcher_results.generation,
            revision,
            self.query.read(cx).value().as_str(),
            visible,
        ) && window.window_handle().window_id() == root.window_id()
            && local_focus_owned;
        if !current {
            cx.notify();
            return;
        }
        let focus_owned = receipt
            .stamp
            .may_refocus(root.window_id(), window.is_window_active())
            && local_focus_owned;
        let settings = &receipt.request.settings;
        let mut changed_query = None;
        match &receipt.result.outcome {
            Ok(outcome) => {
                self.launcher_search_error = None;
                if let multi_launcher::commands::QueryPolicy::Set(query) = &outcome.query {
                    changed_query = Some(query.clone());
                    self.query_history.reset();
                    self.search_completion.clear_for_query_replacement();
                    self.query
                        .update(cx, |state, cx| state.replace_all(query.clone(), window, cx));
                }
                if outcome.visibility == multi_launcher::commands::VisibilityPolicy::Hide {
                    self.root_visibility.request_external_hide(revision);
                }
                if outcome.focus && focus_owned {
                    self.query.focus_handle(cx).focus(window, cx);
                }
                if settings.enable_toasts {
                    for (index, toast) in outcome.toasts.iter().enumerate() {
                        let message = match toast {
                            multi_launcher::commands::ToastPolicy::Launched(label) => {
                                format!("Launched {label}")
                            }
                            multi_launcher::commands::ToastPolicy::Copied(label) => {
                                format!("Copied {label}")
                            }
                            multi_launcher::commands::ToastPolicy::Info(message)
                            | multi_launcher::commands::ToastPolicy::Success(message)
                            | multi_launcher::commands::ToastPolicy::Error(message) => {
                                message.clone()
                            }
                        };
                        self.external_notification(
                            ticket,
                            index,
                            message,
                            false,
                            settings.toast_duration,
                            window,
                            cx,
                        );
                    }
                }
            }
            Err(error) => {
                if settings.show_inline_errors {
                    self.launcher_search_error = Some(error.message.clone());
                }
                if error.toast && settings.enable_toasts && settings.show_error_toasts {
                    self.external_notification(
                        ticket,
                        0,
                        error.message.clone(),
                        true,
                        settings.toast_duration,
                        window,
                        cx,
                    );
                }
                if error.refocus && focus_owned {
                    self.query.focus_handle(cx).focus(window, cx);
                }
            }
        }
        let mut deferred_inline = None;
        for (index, error) in receipt
            .result
            .history_error
            .iter()
            .chain(
                receipt
                    .result
                    .usage_result
                    .as_ref()
                    .and_then(|r| r.as_ref().err()),
            )
            .enumerate()
        {
            let message = format!("Application launched, but bookkeeping failed: {error}");
            if settings.show_inline_errors {
                if changed_query.is_some() {
                    deferred_inline = Some(message.clone());
                } else {
                    self.launcher_search_error = Some(message.clone());
                }
            }
            if settings.enable_toasts && settings.show_error_toasts {
                self.external_notification(
                    ticket,
                    index + 100,
                    message,
                    true,
                    settings.toast_duration,
                    window,
                    cx,
                );
            }
        }
        if let (Some(expected_query), Some(message)) = (changed_query, deferred_inline) {
            let expected_generation = receipt.stamp.generation.wrapping_add(1).max(1);
            let origin_focus = receipt.stamp.focus.clone();
            let root_id = root.window_id();
            let weak_root = cx.entity().downgrade();
            window.defer(cx, move |window, app| {
                let _ = weak_root.update(app, |root, cx| {
                    if root
                        .root_window
                        .is_some_and(|handle| handle.window_id() == root_id)
                        && window.window_handle().window_id() == root_id
                        && root
                            .root_visibility
                            .request_revision
                            .load(Ordering::Acquire)
                            == revision
                        && root.root_visibility.visible.load(Ordering::Acquire)
                        && deferred_inline_is_current(
                            expected_generation,
                            root.launcher_results.generation,
                            &expected_query,
                            &root.launcher_results.query,
                            root.query.read(cx).value().as_str(),
                            root.external_owner_is_current(origin_focus.as_ref(), window, cx),
                        )
                    {
                        root.launcher_search_error = Some(message);
                        cx.notify();
                    }
                });
            });
        }
        cx.notify();
    }

    fn external_notification(
        &self,
        ticket: u64,
        index: usize,
        message: String,
        error: bool,
        duration: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        multi_launcher::toast_log::append_toast_log(&message);
        let key = format!("{ticket}:{index}");
        let notification = if error {
            Notification::error(message)
        } else {
            Notification::success(message)
        };
        window.push_notification(
            notification
                .id1::<ExternalNotification>(key.clone())
                .delivery(NotificationDelivery::InApp)
                .autohide(false),
            cx,
        );
        let window_handle = window.window_handle();
        let delay = std::time::Duration::try_from_secs_f32(duration)
            .unwrap_or(std::time::Duration::from_secs(5));
        cx.spawn(async move |_, cx| {
            cx.background_executor().timer(delay).await;
            let _ = window_handle.update(cx, |_, window, app| {
                window.remove_notification1::<ExternalNotification>(key, app)
            });
        })
        .detach();
    }
}

fn bookkeep(
    outcome: Result<CommandOutcome, CommandError>,
    request: &Request,
    history: impl FnOnce(HistoryEntry, usize) -> Result<(), String>,
    usage: impl FnOnce(&str) -> Result<u32, String>,
) -> WorkerResult {
    let mut history_error = None;
    let mut usage_result = None;
    if outcome
        .as_ref()
        .is_ok_and(|outcome| outcome.history == HistoryPolicy::Record)
    {
        history_error = history(
            HistoryEntry {
                query: request.query.clone(),
                query_lc: String::new(),
                action: request.action.clone(),
                source: Some(request.source.label().to_owned()),
                timestamp: 0,
            },
            request.settings.history_limit,
        )
        .err();
        usage_result = Some(usage(&request.action.action));
    }
    WorkerResult {
        outcome,
        history_error,
        usage_result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_executable_admission_rejects_protocols_and_invalid_windows_paths() {
        for target in [
            r"C:\Apps\Tool.EXE",
            r"..\Tool.exe",
            "tool.exe",
            r"\\server\share\Tool.exe",
        ] {
            assert!(ordinary_executable(target), "{target}");
        }
        for target in [
            "https://host/tool.exe",
            "shell:tool.exe",
            r"\\?\C:\tool.exe",
            r"C:\NUL.exe",
            "tool.exe:stream",
            r"C:\bad?name.exe",
            "tool.exe ",
            "tool.bat",
            ".exe",
        ] {
            assert!(!ordinary_executable(target), "{target}");
        }
        let action = Action {
            label: "same".into(),
            desc: String::new(),
            action: r"C:\app.exe".into(),
            args: Some("--exact value".into()),
        };
        assert_eq!(saved_external(&action).unwrap().args, action.args);
    }

    #[test]
    fn queued_external_hide_requires_current_visible_revision_and_no_new_request() {
        let hide = super::super::RootVisibilityRequest::HideAfterExternalLaunch(3);
        assert!(hide.is_current(3, true, false));
        assert!(!hide.is_current(4, true, false));
        assert!(!hide.is_current(3, false, false));
        assert!(!hide.is_current(3, true, true));
    }

    #[test]
    fn presentation_requires_original_local_owner_without_native_foreground() {
        assert!(presentation_owner_matches(Some(&1), Some(&1), false));
        assert!(!presentation_owner_matches(Some(&1), Some(&2), false));
        assert!(!presentation_owner_matches(Some(&1), Some(&1), true));
        assert!(!presentation_owner_matches::<u8>(None, None, false));
    }

    #[test]
    fn pending_hotkey_supersedes_only_conditional_hide() {
        use super::super::RootVisibilityRequest::{HideAfterExternalLaunch, Set};
        let hide = HideAfterExternalLaunch(2);
        assert!(hide.superseded_by_hotkey(|| true));
        assert!(!hide.superseded_by_hotkey(|| false));
        for explicit in [Set(false), Set(true)] {
            assert!(!explicit.superseded_by_hotkey(|| panic!("explicit request is checked first")));
        }
    }

    #[test]
    fn deferred_inline_error_requires_exact_internal_change_and_owner() {
        assert!(deferred_inline_is_current(5, 5, "", "", "", true));
        assert!(!deferred_inline_is_current(5, 4, "", "old", "", true));
        assert!(!deferred_inline_is_current(5, 6, "", "", "", true));
        assert!(!deferred_inline_is_current(5, 5, "", "", "new", true));
        assert!(!deferred_inline_is_current(5, 5, "", "", "", false));
    }

    #[test]
    fn external_ticket_retires_once_and_stale_presentation_cannot_apply() {
        let admission = Arc::new(RestartAdmission::default());
        let mut state = ExternalLaunch::default();
        let request = Request {
            action: Action {
                label: "Tool".into(),
                desc: String::new(),
                action: "tool.exe".into(),
                args: None,
            },
            command: ExternalCommand {
                target: "tool.exe".into(),
                args: None,
                namespace: None,
            },
            query: "tool".into(),
            source: ActivationSource::Enter,
            settings: Settings {
                clear_query_after_run: false,
                hide_after_run: false,
                history_limit: 10,
                enable_toasts: false,
                show_error_toasts: false,
                show_inline_errors: false,
                toast_duration: 1.0,
            },
        };
        let stamp = Stamp {
            root: WindowId::from(1),
            generation: 4,
            visibility_revision: 8,
            query: "tool".into(),
            focus: None,
        };
        let ticket = state.begin(&admission, stamp, &request).unwrap();
        let duplicate = Stamp {
            root: WindowId::from(1),
            generation: 4,
            visibility_revision: 8,
            query: "tool".into(),
            focus: None,
        };
        assert!(state.begin(&admission, duplicate, &request).is_none());
        assert_eq!(admission.pending_effects(), 1);
        let stale_result = WorkerResult {
            outcome: Ok(CommandOutcome::default()),
            history_error: None,
            usage_result: None,
        };
        assert!(
            state
                .finish(ticket + 1, request.clone(), stale_result)
                .is_none()
        );
        assert!(state.is_pending());
        let result = WorkerResult {
            outcome: Ok(CommandOutcome::default()),
            history_error: None,
            usage_result: None,
        };
        let receipt = state.finish(ticket, request.clone(), result).unwrap();
        assert!(!state.is_pending());
        let repeated = WorkerResult {
            outcome: Ok(CommandOutcome::default()),
            history_error: None,
            usage_result: None,
        };
        assert!(state.finish(ticket, request, repeated).is_none());
        assert!(
            !receipt
                .stamp
                .presentation_is_current(WindowId::from(1), 5, 8, "tool", true)
        );
        assert!(
            !receipt
                .stamp
                .presentation_is_current(WindowId::from(1), 4, 9, "tool", true)
        );
        assert!(
            !receipt
                .stamp
                .presentation_is_current(WindowId::from(1), 4, 8, "changed", true)
        );
        assert!(!receipt.stamp.may_refocus(WindowId::from(1), false));
        assert_eq!(admission.pending_effects(), 1);
        drop(receipt);
        assert_eq!(admission.pending_effects(), 0);
    }

    #[test]
    fn bookkeeping_attempts_both_stores_after_success_without_relaunch() {
        let request = Request {
            action: Action {
                label: "Tool".into(),
                desc: String::new(),
                action: "tool.exe".into(),
                args: Some("--arg".into()),
            },
            command: ExternalCommand {
                target: "tool.exe".into(),
                args: Some("--arg".into()),
                namespace: None,
            },
            query: "tool".into(),
            source: ActivationSource::Click,
            settings: Settings {
                clear_query_after_run: false,
                hide_after_run: false,
                history_limit: 10,
                enable_toasts: true,
                show_error_toasts: true,
                show_inline_errors: true,
                toast_duration: 2.0,
            },
        };
        let outcome = CommandOutcome {
            history: HistoryPolicy::Record,
            ..CommandOutcome::default()
        };
        let result = bookkeep(
            Ok(outcome),
            &request,
            |entry, limit| {
                assert_eq!(entry.query, "tool");
                assert_eq!(entry.action, request.action);
                assert_eq!(entry.source.as_deref(), Some("click"));
                assert_eq!(limit, 10);
                Err("history failed".into())
            },
            |key| {
                assert_eq!(key, "tool.exe");
                Ok(4)
            },
        );
        assert!(result.outcome.is_ok());
        assert_eq!(result.history_error.as_deref(), Some("history failed"));
        assert_eq!(result.usage_result, Some(Ok(4)));
    }
}
