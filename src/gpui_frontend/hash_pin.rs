use super::*;

/// Saved membership is distinct from the current host and session.
pub(super) struct HashPinState {
    pub(super) saved: Arc<AtomicBool>,
    pub(super) remount_revision: Option<u64>,
    pub(super) order: Vec<Panel>,
    pub(super) pending: Option<UnpinReceipt>,
    pub(super) error: Option<String>,
    pub(super) activation: Option<ActivationReceipt>,
    sequence: u64,
}

pub(super) const UNPIN_PENDING: &str = "Wait for Hash Generator unpinning to finish.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct UnpinReceipt {
    pub(super) sequence: u64,
    pub(super) mount: Option<u64>,
    pub(super) epoch: u64,
    pub(super) revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ActivationReceipt {
    pub(super) mount: u64,
    pub(super) epoch: u64,
    pub(super) revision: u64,
    pub(super) window: WindowId,
    pub(super) view: EntityId,
    pub(super) native: NativeWindow,
    pub(super) stack: Vec<Panel>,
    pub(super) accepted: bool,
    pub(super) observed: bool,
}

impl ActivationReceipt {
    fn observe(&mut self, mount: u64, epoch: u64, window: WindowId, view: EntityId, native: Option<NativeWindow>) -> bool {
        if self.mount != mount || self.epoch != epoch || self.window != window
            || self.view != view || Some(self.native) != native { return false; }
        self.observed = true;
        true
    }

    pub(super) fn accept(&mut self, revision: u64) -> bool {
        if self.revision != revision { return false; }
        self.accepted = true;
        true
    }

    fn ready(&self) -> bool { self.accepted && self.observed }
}

impl UnpinReceipt {
    fn matches_current(self, pending: Option<Self>, mount: Option<u64>, epoch: u64, revision: u64, transitioning: bool) -> bool {
        pending == Some(self) && mount == self.mount && epoch == self.epoch
            && revision == self.revision && transitioning
    }
}

impl HashPinState {
    pub(super) fn new(panels: &[Panel]) -> Self {
        Self { saved: Arc::new(AtomicBool::new(panels.contains(&Panel::HashGeneratorDialog))), remount_revision: None,
            order: startup_order(panels), pending: None, error: None, activation: None, sequence: 0 }
    }
    pub(super) fn is_saved(&self) -> bool { self.saved.load(Ordering::Acquire) }

    pub(super) fn begin(&mut self, mount: Option<u64>, epoch: u64, revision: u64) -> Option<UnpinReceipt> {
        if !self.is_saved() || self.pending.is_some() { return None; }
        self.sequence = self.sequence.wrapping_add(1).max(1);
        let receipt = UnpinReceipt { sequence: self.sequence, mount, epoch, revision };
        self.pending = Some(receipt);
        self.error = None;
        Some(receipt)
    }

    pub(super) fn finish(&mut self, receipt: UnpinReceipt, result: Result<(), String>) -> bool {
        if self.pending != Some(receipt) { return false; }
        self.pending = None;
        match result {
            Ok(()) => { self.saved.store(false, Ordering::Release); self.remount_revision = None; }
            Err(error) => self.error = Some(format!("Could not unpin Hash Generator: {error}")),
        }
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CloseOrigin { Keyboard, Native, Button, Unpin }

pub(super) fn allows_close(saved: bool, origin: CloseOrigin) -> bool {
    !saved || origin == CloseOrigin::Button
}

pub(super) fn remount_is_current(saved: bool, pending: Option<u64>, revision: u64, closed: bool, retired: bool, fenced: bool) -> bool {
    saved && pending == Some(revision) && closed && retired && !fenced
}

pub(super) fn remount_visibility_allows(visible: bool) -> bool {
    visible
}

pub(super) fn restore_after_unpin(same_visible_root: bool, toolbar_focused: bool, exact_docked_focus: bool) -> bool {
    same_visible_root && (toolbar_focused || exact_docked_focus)
}

pub(super) fn activation_receipt_is_current(saved: bool, same_mount: bool, same_session: bool,
    same_revision: bool, same_native: bool, foreground_matches: bool, fenced: bool) -> bool {
    saved && same_mount && same_session && same_revision && same_native && foreground_matches && !fenced
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HashMountIntent {
    User,
    Passive,
}

impl HashMountIntent {
    pub(super) fn is_user_activation(self) -> bool { self == Self::User }

    pub(super) fn record_stack(self, stack: &mut Vec<Panel>) {
        match self {
            Self::User => panel_stack::promote(stack, Panel::HashGeneratorDialog),
            Self::Passive => panel_stack::ensure(stack, Panel::HashGeneratorDialog),
        }
    }
}

/// Only these saved panels have passive GPUI owners in this increment.
pub(super) fn startup_order(saved: &[Panel]) -> Vec<Panel> {
    let mut order = Vec::new();
    for panel in saved.iter().copied() {
        if matches!(panel, Panel::FavDialog | Panel::HashGeneratorDialog | Panel::JsonUtilityDialog | Panel::XmlToolkitDialog) {
            panel_stack::ensure(&mut order, panel);
        }
    }
    order
}

impl RootWindow {
    pub(super) fn observe_hash_pin_activation(&mut self, mount: u64, epoch: u64, window: WindowId,
        view: EntityId, native: Option<NativeWindow>, cx: &mut Context<Self>) {
        let Some(mut receipt) = self.hash_pin.activation.take() else { return; };
        if !receipt.observe(mount, epoch, window, view, native) {
            self.hash_pin.activation = Some(receipt);
            return;
        }
        self.hash_pin.activation = Some(receipt);
        self.confirm_hash_pin_activation(cx);
    }

    pub(super) fn retire_hash_pin_activation_on_blur(&mut self, mount: u64, view: EntityId) {
        if self.hash_pin.activation.as_ref().is_some_and(|receipt| {
            receipt.mount == mount && receipt.view == view && (receipt.observed || receipt.accepted)
        }) {
            self.hash_pin.activation = None;
        }
    }

    pub(super) fn confirm_hash_pin_activation(&mut self, cx: &mut Context<Self>) {
        let Some(receipt) = self.hash_pin.activation.as_ref() else { return; };
        let same_mount = matches!(&self.hash_window,
            HashWindowState::Popout { mount_generation, window, view }
            if *mount_generation == receipt.mount && window.window_id() == receipt.window && view.entity_id() == receipt.view)
            && self.hash_mount_generation.load(Ordering::Acquire) == receipt.mount
            && !self.hash_rehost_pending.load(Ordering::Acquire) && self.hash_pin.pending.is_none();
        let same_session = self.hash_session.lock().unwrap_or_else(std::sync::PoisonError::into_inner).epoch() == receipt.epoch;
        let current = activation_receipt_is_current(self.hash_pin.is_saved(), same_mount, same_session,
            self.hash_window_revision.load(Ordering::Acquire) == receipt.revision,
            native_window_is_valid(receipt.native),
            current_foreground_identity() == Some(receipt.native.identity), self.restart_admission.is_fenced())
            && self.panel_stack == receipt.stack;
        if !current {
            // A not-yet-foreground native window may still deliver its accepted activation.
            if !same_mount || !same_session || self.hash_window_revision.load(Ordering::Acquire) != receipt.revision
                || !self.hash_pin.is_saved() || self.restart_admission.is_fenced() || self.panel_stack != receipt.stack {
                self.hash_pin.activation = None;
            }
            return;
        }
        if receipt.ready() {
            self.hash_pin.activation = None;
            panel_stack::promote(&mut self.panel_stack, Panel::HashGeneratorDialog);
            cx.notify();
        }
    }

    pub(super) fn pinned_hash_click(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.hash_pin.is_saved() || self.hash_pin.pending.is_some() || self.structured_pins.busy()
            || !self.restart_admission.is_ready() || self.hash_pin.remount_revision.is_some()
            || self.hash_rehost_pending.load(Ordering::Acquire)
            || self.hash_file_picker_pending.load(Ordering::Acquire)
            || self.hash_clipboard_pending.is_pending() || self.action_sheet.is_some()
            || self.context_favorite_popup.is_some() { return; }
        let contexts = window.context_stack();
        if context_stack_has_primary(&contexts, "PopupMenu")
            || context_stack_has_primary(&contexts, "AlertDialog")
            || context_stack_has_primary(&contexts, "Dialog") { return; }
        let Some(visibility_revision) = self.favorites_toolbar_visibility_revision(window) else { return; };
        match panel_stack::saved_toolbar_action(&self.panel_stack, Panel::HashGeneratorDialog) {
            panel_stack::SavedToolbarAction::Focus => {
                if self.hash_window.view().is_none() { return; }
                let Some(operation) = self.restart_admission.admit() else { return; };
                self.hash_pin.error = self.open_hash_generator_window(HashOpenIntent::Reactivate, operation, window, cx)
                    .err().map(|error| format!("Could not focus Hash Generator: {error}"));
                cx.notify();
            }
            panel_stack::SavedToolbarAction::Unpin => {
                let Some(permit) = self.restart_admission.admit() else { return; };
                if self.hash_rehost_pending.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_err() { return; }
                let mount = self.hash_window.mount_generation();
                let epoch = self.hash_session.lock().unwrap_or_else(std::sync::PoisonError::into_inner).epoch();
                let revision = self.hash_rehost_revision.fetch_add(1, Ordering::AcqRel).wrapping_add(1).max(1);
                let Some(receipt) = self.hash_pin.begin(mount, epoch, revision) else {
                    self.hash_rehost_pending.store(false, Ordering::Release);
                    return;
                };
                self.hash_window_revision.fetch_add(1, Ordering::AcqRel);
                if let Some(view) = self.hash_window.view().cloned() {
                    view.update(cx, |tool, cx| tool.set_editors_readonly(true, cx));
                }
                let path = self.settings_path.clone();
                let task = cx.background_executor().spawn(async move {
                    multi_launcher::settings::Settings::unpin_panel_checked(path, Panel::HashGeneratorDialog)
                        .map_err(|error| format!("{error:#}"))
                });
                cx.spawn_in(window, async move |root, cx| {
                    let result = task.await;
                    let _ = root.update_in(cx, |root, window, cx| {
                        let epoch = root.hash_session.lock().unwrap_or_else(std::sync::PoisonError::into_inner).epoch();
                        let current = receipt.matches_current(root.hash_pin.pending, root.hash_window.mount_generation(), epoch,
                            root.hash_rehost_revision.load(Ordering::Acquire), root.hash_rehost_pending.load(Ordering::Acquire));
                        if !current { return; }
                        let success = result.is_ok();
                        if !root.hash_pin.finish(receipt, result) { return; }
                        if success {
                            let same_root = root.favorites_toolbar_visibility_revision(window) == Some(visibility_revision);
                            let docked_focused = matches!(&root.hash_window,
                                HashWindowState::Docked { mount_generation, view } if Some(*mount_generation) == receipt.mount && {
                                    let tool = view.read(cx);
                                    tool.focus_handle.contains_focused(window, cx) || tool.editable_focus_is_exact(window, cx)
                                });
                            if restore_after_unpin(same_root, root.hash_pin_focus_handle.is_focused(window), docked_focused) {
                                root.query.focus_handle(cx).focus(window, cx);
                            }
                            if let Some(mount) = receipt.mount {
                                root.start_hash_close(mount, receipt.epoch, receipt.revision, CloseOrigin::Unpin, permit, cx);
                            } else {
                                root.hash_rehost_pending.store(false, Ordering::Release);
                                panel_stack::retire(&mut root.panel_stack, Panel::HashGeneratorDialog);
                                drop(permit);
                            }
                        } else {
                            root.hash_rehost_pending.store(false, Ordering::Release);
                            if let Some(view) = root.hash_window.view().cloned() {
                                view.update(cx, |tool, cx| tool.set_editors_readonly(false, cx));
                            }
                            drop(permit);
                        }
                        if root.restart_status.as_deref() == Some(UNPIN_PENDING) { root.restart_status = None; }
                        cx.notify();
                    });
                }).detach();
                cx.notify();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ActivationReceipt, CloseOrigin, EntityId, ForegroundIdentity, HashMountIntent, HashPinState, NativeWindow, Panel, WindowId, activation_receipt_is_current, allows_close, remount_is_current, remount_visibility_allows, restore_after_unpin, startup_order};
    use multi_launcher::gui::panel_stack::{SavedToolbarAction, saved_toolbar_action};

    #[test]
    fn saved_order_and_mount_intents_preserve_panel_stack_policy() {
        let order = startup_order(&[Panel::HashGeneratorDialog, Panel::Settings, Panel::FavDialog, Panel::HashGeneratorDialog]);
        assert_eq!(order, vec![Panel::HashGeneratorDialog, Panel::FavDialog]);
        let mut stack = Vec::new();
        for panel in order {
            if panel == Panel::HashGeneratorDialog {
                HashMountIntent::Passive.record_stack(&mut stack);
                HashMountIntent::Passive.record_stack(&mut stack);
            } else {
                stack.push(panel);
            }
        }
        assert_eq!(saved_toolbar_action(&stack, Panel::HashGeneratorDialog), SavedToolbarAction::Focus);
        HashMountIntent::User.record_stack(&mut stack);
        assert_eq!(saved_toolbar_action(&stack, Panel::HashGeneratorDialog), SavedToolbarAction::Unpin);
        assert!(!HashMountIntent::Passive.is_user_activation());
        assert!(HashMountIntent::User.is_user_activation());
    }

    #[test]
    fn structured_pins_preserve_saved_order_with_hash_and_favorites() {
        let saved = [Panel::XmlToolkitDialog, Panel::FavDialog, Panel::JsonUtilityDialog, Panel::HashGeneratorDialog, Panel::XmlToolkitDialog];
        assert_eq!(startup_order(&saved), vec![Panel::XmlToolkitDialog, Panel::FavDialog, Panel::JsonUtilityDialog, Panel::HashGeneratorDialog]);
    }

    #[test]
    fn pinned_close_origin_and_remount_are_generation_checked() {
        assert!(!allows_close(true, CloseOrigin::Keyboard));
        assert!(!allows_close(true, CloseOrigin::Native));
        assert!(allows_close(true, CloseOrigin::Button));
        assert!(allows_close(false, CloseOrigin::Keyboard));
        assert!(remount_is_current(true, Some(4), 4, true, true, false));
        assert!(!remount_is_current(true, Some(4), 5, true, true, false));
        assert!(!remount_is_current(false, Some(4), 4, true, true, false));
        assert!(!remount_is_current(true, Some(4), 4, true, false, false));
        assert!(!remount_is_current(true, Some(4), 4, true, true, true));
    }

    #[test]
    fn unpin_receipt_rejects_stale_and_failure_preserves_membership() {
        let mut state = HashPinState::new(&[Panel::FavDialog, Panel::HashGeneratorDialog]);
        let first = state.begin(Some(3), 4, 5).unwrap();
        assert!(first.matches_current(Some(first), Some(3), 4, 5, true));
        assert!(!first.matches_current(Some(first), Some(7), 4, 5, true));
        assert!(!first.matches_current(Some(first), Some(3), 6, 5, true));
        assert!(!first.matches_current(Some(first), Some(3), 4, 6, true));
        assert!(!first.matches_current(Some(first), Some(3), 4, 5, false));
        assert!(state.begin(Some(3), 4, 5).is_none());
        let mut stale = first;
        stale.epoch += 1;
        assert!(!state.finish(stale, Ok(())));
        assert!(state.is_saved());
        assert!(state.finish(first, Err("locked".into())));
        assert!(state.is_saved());
        assert!(state.error.as_deref().unwrap().contains("locked"));
        let second = state.begin(Some(3), 4, 6).unwrap();
        assert!(!state.finish(first, Ok(())));
        assert!(state.finish(second, Ok(())));
        assert!(!state.is_saved());
        assert_eq!(state.order, vec![Panel::FavDialog, Panel::HashGeneratorDialog]);
    }

    #[test]
    fn saved_hash_focus_and_hidden_remount_require_current_authority() {
        assert!(!remount_visibility_allows(false));
        assert!(remount_visibility_allows(true));
        assert!(restore_after_unpin(true, false, true));
        assert!(restore_after_unpin(true, true, false));
        assert!(!restore_after_unpin(false, true, true));
        assert!(!restore_after_unpin(true, false, false));
        assert!(activation_receipt_is_current(true, true, true, true, true, true, false));
        assert!(!activation_receipt_is_current(true, true, true, true, true, false, false));
        assert!(!activation_receipt_is_current(true, false, true, true, true, true, false));
        assert!(!activation_receipt_is_current(true, true, true, false, true, true, false));
        assert!(!activation_receipt_is_current(true, true, true, true, true, true, true));
    }

    #[test]
    fn native_activation_confirmation_handles_both_event_orders() {
        let native = NativeWindow { identity: ForegroundIdentity { hwnd: 10, process_id: 11, thread_id: 12 } };
        let receipt = ActivationReceipt {
            mount: 1, epoch: 2, revision: 3, window: WindowId::from(4), view: EntityId::from(5),
            native, stack: vec![Panel::FavDialog], accepted: false, observed: false,
        };
        let mut first = receipt.clone();
        assert!(first.accept(3));
        assert!(!first.ready());
        assert!(!first.observe(9, 2, WindowId::from(4), EntityId::from(5), Some(native)));
        assert!(!first.ready());
        assert!(first.observe(1, 2, WindowId::from(4), EntityId::from(5), Some(native)));
        assert!(first.ready());
        let mut second = receipt;
        assert!(second.observe(1, 2, WindowId::from(4), EntityId::from(5), Some(native)));
        assert!(!second.ready());
        assert!(!second.accept(99));
        assert!(!second.ready());
        assert!(second.accept(3));
        assert!(second.ready());
    }
}
