//! Checked History clear admission and Cancel-first review.
use super::*;
use std::collections::VecDeque;
use multi_launcher::history::HistoryEntry;

#[derive(Default)]
pub(super) struct Owner { serial: u64, active: bool, claimed: bool }

pub(super) fn current_provider(query: &str, action: &Action, plugins: Option<&HashSet<String>>, caps: Option<&HashMap<String, Vec<String>>>) -> bool {
    multi_launcher::plugins::history::clear_action("hi clear").as_ref() == Some(action)
        && plugins.is_none_or(|set| set.contains("history"))
        && multi_launcher::settings::provider_search_capability_enabled(caps, "history")
        && search_providers::search_routed([&super::history_search::Provider::default() as &dyn multi_launcher::plugin::Plugin], query, plugins, caps)
            .iter().any(|(provider, row)| provider == "history" && row == action)
}

#[derive(Clone)]
struct Request { serial: u64, generation: u64, index: usize, query: String, action: Action, source: ActivationSource,
    native: NativeWindow, revision: u64, focus: FocusHandle }

impl RootWindow {
    fn history_clear_current(&self, request: &Request, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.history_clear.active && self.history_clear.serial == request.serial
            && !self.restart_admission.is_fenced() && window.is_window_active()
            && self.native_window == Some(request.native) && current_foreground_identity() == Some(request.native.identity)
            && json_utility::native_host_current(&self.root_visibility, request.revision, Some(request.native), false)
            && self.query.read(cx).value().as_str() == request.query
            && self.launcher_results.resolve_result(request.generation, request.index, &request.query).is_some_and(|row|
                row.action == request.action && matches!(&row.origin, LauncherResultOrigin::Provider(name) if name == "history"))
            && current_provider(&request.query, &request.action, self.enabled_plugins.as_ref(), self.enabled_capabilities.as_ref())
    }
    pub(super) fn start_history_clear(&mut self, generation: u64, index: usize, query: String, action: Action, source: ActivationSource, window: &mut Window, cx: &mut Context<Self>) {
        if self.history_clear.active || self.external_launch.operation_pending() || self.restart_admission.is_fenced() || !window.is_window_active() || window.has_active_dialog(cx)
            || !self.external_owner_is_current(window.focused(cx).as_ref(), window, cx) { return; }
        let Some(native) = self.native_window.filter(|native| current_foreground_identity() == Some(native.identity)) else { return; };
        let Some(focus) = window.focused(cx) else { return; };
        let Some(permit) = self.restart_admission.admit() else { return; };
        self.history_clear.serial = self.history_clear.serial.wrapping_add(1); self.history_clear.active = true; self.history_clear.claimed = false;
        let request = Request { serial: self.history_clear.serial, generation, index, query, action, source, native,
            revision: self.root_visibility.request_revision.load(Ordering::Acquire), focus: focus.clone() };
        if !self.history_clear_current(&request, window, cx) { self.history_clear.active = false; return; }
        let lease = Arc::new(AtomicBool::new(true)); let revoked = lease.clone();
        let blur = cx.on_blur(&focus, window, move |_, _, _| { revoked.store(false, Ordering::Release); });
        let worker_lease = lease.clone(); let visibility = self.root_visibility.clone(); let admission = self.restart_admission.clone(); let revision = request.revision;
        let task = cx.background_executor().spawn(async move {
            multi_launcher::history::checked_clear_snapshot(multi_launcher::history::HISTORY_FILE, || worker_lease.load(Ordering::Acquire)
                && !admission.is_fenced() && current_foreground_identity() == Some(native.identity)
                && json_utility::native_host_current(&visibility, revision, Some(native), false))
        });
        let host = window.window_handle();
        cx.spawn(async move |weak, cx| {
            let snapshot = task.await; let _permit = permit; let _blur = blur;
            let _ = host.update(cx, |_, window, cx| { let _ = weak.update(cx, |root, cx| {
                if !root.history_clear_current(&request, window, cx) || !lease.load(Ordering::Acquire)
                    || !request.focus.is_focused(window) || window.has_active_dialog(cx)
                    || !root.external_owner_is_current(Some(&request.focus), window, cx) {
                    if root.history_clear.serial == request.serial { root.history_clear.active = false; } return;
                }
                match snapshot {
                    Ok(snapshot) => { if root.require_confirm_destructive { root.show_history_clear_review(request, Arc::new(snapshot), window, cx); }
                        else { root.begin_history_clear_effect(request, Arc::new(snapshot), window, cx); } }
                    Err(_) => { root.history_clear.active = false; root.launcher_search_error = Some("History could not be read; no changes were made.".into()); cx.notify(); }
                }
            }); });
        }).detach();
    }
    fn begin_history_clear_effect(&mut self, request: Request, snapshot: Arc<VecDeque<HistoryEntry>>, window: &mut Window, cx: &mut Context<Self>) {
        if !self.history_clear_current(&request, window, cx) || window.has_active_dialog(cx)
            || !request.focus.is_focused(window) || !self.external_owner_is_current(Some(&request.focus), window, cx) { self.history_clear.active = false; return; }
        self.history_clear.active = false;
        let guard = external_launch::OmniGuard { catalog: Arc::new(AtomicBool::new(true)), lease: Arc::new(AtomicBool::new(true)), effect_gate: Arc::new(external_launch::EffectGate::default()), visibility: self.root_visibility.clone(), revision: request.revision, native: request.native, admission: self.restart_admission.clone() };
        self.start_checked_history_clear(request.action, snapshot, guard, request.query, request.source, window, cx);
    }
    fn show_history_clear_review(&mut self, request: Request, snapshot: Arc<VecDeque<HistoryEntry>>, window: &mut Window, cx: &mut Context<Self>) {
        let cancel_focus = cx.focus_handle(); let owner = cx.focus_handle(); let root = cx.entity().downgrade();
        let first = Cell::new(true);
        window.open_dialog(cx, move |dialog, window, cx| {
            if first.replace(false) { cancel_focus.focus(window, cx); }
            let valid = root.upgrade().is_some_and(|root| {
                let root = root.read(cx);
                root.history_clear.active && root.history_clear.serial == request.serial
                    && root.root_visibility.request_revision.load(Ordering::Acquire) == request.revision
                    && root.root_visibility.visible.load(Ordering::Acquire) && root.root_visibility.active.load(Ordering::Acquire)
                    && root.launcher_results.resolve_result(request.generation, request.index, &request.query).is_some_and(|row| row.action == request.action && matches!(&row.origin, LauncherResultOrigin::Provider(name) if name == "history"))
                    && root.query.read(cx).value().as_str() == request.query
                    && current_provider(&request.query, &request.action, root.enabled_plugins.as_ref(), root.enabled_capabilities.as_ref())
            });
            let cancel = root.clone(); let accept = root.clone(); let keys = root.clone(); let escape = root.clone();
            let request = request.clone(); let chosen = request.clone(); let serial = request.serial;
            let snapshot = snapshot.clone(); let cancel_focus = cancel_focus.clone(); let owner = owner.clone(); let key_owner = owner.clone();
            dialog.title("Clear history?").keyboard(false).close_button(false).overlay_closable(false)
                .child(div().track_focus(&owner).capture_key_down(move |event: &KeyDownEvent, window, cx| {
                    if event.keystroke.modifiers != Modifiers::default() || !key_owner.contains_focused(window, cx) { return; }
                    let escape_key = event.keystroke.key == "escape";
                    if !escape_key && !(event.is_held && matches!(event.keystroke.key.as_str(), "enter" | "space")) { return; }
                    let _ = keys.update(cx, |root, cx| { if root.history_clear.active && root.history_clear.serial == serial {
                        window.prevent_default(); cx.stop_propagation(); if escape_key && !event.is_held { root.history_clear.active = false; root.pane_escape_latched = true; window.close_dialog(cx); cx.notify(); }
                    }});
                }).capture_action(move |_: &gpui_kit::base::input::Escape, window, cx| { let _ = escape.update(cx, |root, cx| {
                    if root.history_clear.active && root.history_clear.serial == serial { root.history_clear.active = false; root.pane_escape_latched = true; window.close_dialog(cx); cx.stop_propagation(); cx.notify(); }
                }); }).flex().flex_col().gap_2().child(Label::new(if valid { "Remove all history entries?" } else { "History selection changed. Cancel and open it again." }))
                    .child(div().flex().gap_2().child(Button::new("history-clear-cancel").label("Cancel").track_focus(&cancel_focus).on_click(move |_, window, cx| {
                        let _ = cancel.update(cx, |root, cx| { if root.history_clear.active && root.history_clear.serial == serial { root.history_clear.active = false; window.close_dialog(cx); cx.notify(); } });
                    })).child(Button::new("history-clear-confirm").label("Clear").disabled(!valid).on_click(move |_, window, cx| {
                        let _ = accept.update(cx, |root, cx| {
                            if root.history_clear.claimed || !root.history_clear_current(&chosen, window, cx) || !owner.contains_focused(window, cx)
                                || context_stack_has_primary(&window.context_stack(), "PopupMenu") || context_stack_has_primary(&window.context_stack(), "AlertDialog") { return; }
                            root.history_clear.claimed = true;
                            window.close_dialog(cx); let weak = cx.entity().downgrade(); let chosen = chosen.clone(); let snapshot = snapshot.clone();
                            window.defer(cx, move |window, cx| { let _ = weak.update(cx, |root, cx| root.begin_history_clear_effect(chosen, snapshot, window, cx)); });
                        });
                    }))))
        });
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn clear_provider_requires_exact_live_row() {
        let enabled = std::collections::HashSet::from(["history".to_owned()]);
        let action = multi_launcher::plugins::history::clear_action("hi clear").unwrap();
        assert!(super::current_provider("hi clear", &action, Some(&enabled), None));
        assert!(!super::current_provider("hi clear extra", &action, Some(&enabled), None));
        assert!(!super::current_provider("hi clear", &action, Some(&std::collections::HashSet::new()), None));
        let no_search = std::collections::HashMap::from([("history".to_owned(), Vec::new())]);
        assert!(!super::current_provider("hi clear", &action, Some(&enabled), Some(&no_search)));
        assert!(super::current_provider("-id:unrelated hi clear", &action, Some(&enabled), None));
        assert!(!super::current_provider("-id:history:clear hi clear", &action, Some(&enabled), None));
        let mut forged = action.clone(); forged.label.push('!');
        assert!(!super::current_provider("hi clear", &forged, Some(&enabled), None));
    }
}
