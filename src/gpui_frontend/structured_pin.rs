//! Saved structured-tool membership and checked unpin ownership.
use super::*;

pub(super) struct StructuredPins {
    json: bool,
    xml: bool,
    pub pending: Option<PendingUnpin>,
    pub closing: Option<(Panel, u64)>,
    pub error: Option<String>,
    pub activation: Option<PendingActivation>,
    pub activation_revision: Arc<AtomicU64>,
    sequence: u64,
}
pub(super) struct PendingActivation {
    pub panel: Panel,
    pub mount: utility_mount::MountReceipt,
    pub window: WindowId,
    pub native: NativeWindow,
    pub revision: u64,
    pub stack: Vec<Panel>,
    pub workspace_guard: Arc<AtomicU64>,
    pub workspace_revision: u64,
    pub root_native: NativeWindow,
    pub visibility_revision: u64,
    pub accepted: bool,
    pub observed: bool,
}
pub(super) struct PendingUnpin {
    pub panel: Panel,
    pub sequence: u64,
    pub mount: utility_mount::MountReceipt,
}
impl StructuredPins {
    pub fn new(saved: &[Panel]) -> Self {
        Self {
            json: saved.contains(&Panel::JsonUtilityDialog),
            xml: saved.contains(&Panel::XmlToolkitDialog),
            pending: None,
            closing: None,
            error: None,
            activation: None,
            activation_revision: Arc::new(AtomicU64::new(0)),
            sequence: 0,
        }
    }
    pub fn busy(&self) -> bool {
        self.pending.is_some() || self.closing.is_some()
    }
    pub fn saved(&self, panel: Panel) -> bool {
        match panel {
            Panel::JsonUtilityDialog => self.json,
            Panel::XmlToolkitDialog => self.xml,
            _ => false,
        }
    }
    pub fn begin(&mut self, panel: Panel, mount: utility_mount::MountReceipt) -> Option<u64> {
        if !self.saved(panel) || self.busy() || !mount.is_current() {
            return None;
        }
        self.sequence = self.sequence.checked_add(1)?;
        self.error = None;
        self.pending = Some(PendingUnpin {
            panel,
            sequence: self.sequence,
            mount,
        });
        Some(self.sequence)
    }
    pub fn finish(
        &mut self,
        panel: Panel,
        sequence: u64,
        result: Result<(), String>,
    ) -> Option<bool> {
        if !self
            .pending
            .as_ref()
            .is_some_and(|p| p.panel == panel && p.sequence == sequence)
        {
            return None;
        }
        self.pending = None;
        match result {
            Ok(()) => {
                match panel {
                    Panel::JsonUtilityDialog => self.json = false,
                    Panel::XmlToolkitDialog => self.xml = false,
                    _ => return None,
                };
                Some(true)
            }
            Err(error) => {
                self.error = Some(format!(
                    "Could not unpin {}: {error}",
                    panel.display_label()
                ));
                Some(false)
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::super::utility_mount::MountOwner;
    use super::{Panel, StructuredPins};
    #[test]
    fn failed_unpin_keeps_membership_and_releases_lane() {
        let mut pins = StructuredPins::new(&[Panel::JsonUtilityDialog, Panel::XmlToolkitDialog]);
        let owner = MountOwner::default();
        let sequence = pins.begin(Panel::XmlToolkitDialog, owner.attach()).unwrap();
        assert_eq!(
            pins.finish(Panel::XmlToolkitDialog, sequence, Err("save failed".into())),
            Some(false)
        );
        assert!(!pins.busy());
        assert!(pins.saved(Panel::JsonUtilityDialog));
        assert!(pins.saved(Panel::XmlToolkitDialog));
        assert!(
            pins.error
                .as_deref()
                .is_some_and(|message| message.contains("save failed"))
        );
    }
    #[test]
    fn unpin_requires_exact_saved_panel_and_current_mount() {
        let mut pins = StructuredPins::new(&[Panel::JsonUtilityDialog, Panel::XmlToolkitDialog]);
        let owner = MountOwner::default();
        let first = owner.attach();
        let seq = pins.begin(Panel::JsonUtilityDialog, first).unwrap();
        assert!(
            pins.begin(Panel::XmlToolkitDialog, owner.attach())
                .is_none()
        );
        assert!(pins.finish(Panel::XmlToolkitDialog, seq, Ok(())).is_none());
        assert_eq!(
            pins.finish(Panel::JsonUtilityDialog, seq, Ok(())),
            Some(true)
        );
        assert!(!pins.saved(Panel::JsonUtilityDialog));
        assert!(pins.pending.is_none());
        assert!(pins.saved(Panel::XmlToolkitDialog));
    }
}

#[cfg(windows)]
fn structured_activation_current(
    mount: &utility_mount::MountReceipt,
    workspace_guard: &AtomicU64,
    workspace_revision: u64,
    visibility: &RootVisibility,
    visibility_revision: u64,
    root: NativeWindow,
) -> bool {
    mount.is_current()
        && workspace_guard.load(Ordering::Acquire) == workspace_revision
        && visibility.is_current_visible_revision(visibility_revision, root)
}
#[cfg(not(windows))]
fn structured_activation_current(
    _: &utility_mount::MountReceipt,
    _: &AtomicU64,
    _: u64,
    _: &RootVisibility,
    _: u64,
    _: NativeWindow,
) -> bool {
    false
}

impl RootWindow {
    pub(super) fn pinned_structured_click(
        &mut self,
        panel: Panel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.structured_pins.saved(panel)
            || self.structured_pins.busy()
            || self.hash_pin.pending.is_some()
            || !self.restart_admission.is_ready()
            || self.action_sheet.is_some()
            || self.context_favorite_popup.is_some()
            || window.has_active_dialog(cx)
        {
            return;
        }
        let contexts = window.context_stack();
        if context_stack_has_primary(&contexts, "PopupMenu")
            || context_stack_has_primary(&contexts, "Dialog")
            || context_stack_has_primary(&contexts, "AlertDialog")
        {
            return;
        }
        let Some(visibility_revision) = self.favorites_toolbar_visibility_revision(window) else {
            return;
        };
        let (mount, popup, focus_ready, unpin_ready, workspace_guard, workspace_revision) =
            match panel {
                Panel::JsonUtilityDialog => {
                    let Some(view) = self.json_view.as_ref() else {
                        return;
                    };
                    let state = view.read(cx);
                    (
                        state.mount_receipt(),
                        self.json_popup.as_ref().map(|p| (p.window, p.native)),
                        state.open && !state.rehosting,
                        state.can_rehost(),
                        state.pin_activation_guard().0,
                        state.pin_activation_guard().1,
                    )
                }
                Panel::XmlToolkitDialog => {
                    let Some(view) = self.xml_view.as_ref() else {
                        return;
                    };
                    let state = view.read(cx);
                    (
                        state.mount_receipt(),
                        self.xml_popup.as_ref().map(|p| (p.window, p.native)),
                        state.open && !state.rehosting,
                        state.can_rehost(),
                        state.pin_activation_guard().0,
                        state.pin_activation_guard().1,
                    )
                }
                _ => return,
            };
        if !focus_ready || !mount.is_current() {
            return;
        }
        match panel_stack::saved_toolbar_action(&self.panel_stack, panel) {
            panel_stack::SavedToolbarAction::Focus => {
                if let Some((popup_window, Some(native))) = popup {
                    let Some(root_native) = self.native_window else {
                        return;
                    };
                    if current_foreground_identity() != Some(root_native.identity) {
                        return;
                    }
                    let Some(permit) = self.restart_admission.admit() else {
                        return;
                    };
                    let guard = self.structured_pins.activation_revision.clone();
                    let revision = guard.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
                    self.structured_pins.activation = Some(PendingActivation {
                        panel,
                        mount: mount.clone(),
                        window: popup_window.window_id(),
                        native,
                        revision,
                        stack: self.panel_stack.clone(),
                        workspace_guard: workspace_guard.clone(),
                        workspace_revision,
                        root_native,
                        visibility_revision,
                        accepted: false,
                        observed: false,
                    });
                    let worker = guard.clone();
                    let worker_mount = mount.clone();
                    let worker_workspace = workspace_guard.clone();
                    let worker_visibility = self.root_visibility.clone();
                    let root = cx.entity().downgrade();
                    cx.spawn(async move |_, cx| {
                        let _permit = permit;
                        let accepted = cx
                            .background_executor()
                            .spawn(async move {
                                if !structured_activation_current(
                                    &worker_mount,
                                    &worker_workspace,
                                    workspace_revision,
                                    &worker_visibility,
                                    visibility_revision,
                                    root_native,
                                ) {
                                    return false;
                                }
                                activate_for_hash_pin(
                                    &worker,
                                    revision,
                                    native,
                                    Some(root_native.identity),
                                )
                            })
                            .await;
                        let _ = root.update(cx, |root, cx| {
                            if root
                                .structured_pins
                                .activation
                                .as_ref()
                                .is_some_and(|r| r.revision == revision)
                            {
                                if accepted {
                                    if let Some(receipt) = root.structured_pins.activation.as_mut()
                                    {
                                        receipt.accepted = true;
                                    }
                                    root.confirm_structured_pin_activation(cx);
                                } else {
                                    root.structured_pins.activation = None;
                                }
                            }
                        });
                    })
                    .detach();
                } else if popup.is_none() {
                    let exact = match panel {
                        Panel::JsonUtilityDialog => self.json_view.as_ref().is_some_and(|view| {
                            view.update(cx, |view, cx| {
                                view.focus_editor(window, cx);
                                view.contains_focus(window, cx)
                            })
                        }),
                        Panel::XmlToolkitDialog => self.xml_view.as_ref().is_some_and(|view| {
                            view.update(cx, |view, cx| {
                                view.focus_editor(window, cx);
                                view.contains_focus(window, cx)
                            })
                        }),
                        _ => false,
                    };
                    if exact && mount.is_current() {
                        match panel {
                            Panel::JsonUtilityDialog => self.scroll_to_json(cx),
                            Panel::XmlToolkitDialog => self.scroll_to_xml(cx),
                            _ => {}
                        }
                        panel_stack::promote(&mut self.panel_stack, panel);
                        cx.notify();
                    }
                }
            }
            panel_stack::SavedToolbarAction::Unpin => {
                if !unpin_ready {
                    return;
                }
                let Some(permit) = self.restart_admission.admit() else {
                    return;
                };
                let Some(sequence) = self.structured_pins.begin(panel, mount.clone()) else {
                    return;
                };
                self.structured_pins
                    .activation_revision
                    .fetch_add(1, Ordering::AcqRel);
                self.structured_pins.activation = None;
                let target_window = popup.map(|p| p.0).or(self.root_window);
                let json = self.json_view.clone();
                let xml = self.xml_view.clone();
                match panel {
                    Panel::JsonUtilityDialog => {
                        if let Some(view) = &json {
                            view.update(cx, |view, cx| {
                                view.rehosting = true;
                                cx.notify();
                            });
                        }
                    }
                    Panel::XmlToolkitDialog => {
                        if let Some(view) = &xml {
                            view.update(cx, |view, cx| {
                                view.rehosting = true;
                                cx.notify();
                            });
                        }
                    }
                    _ => {}
                }
                let path = self.settings_path.clone();
                let task = cx.background_executor().spawn(async move {
                    multi_launcher::settings::Settings::unpin_panel_checked(path, panel)
                        .map_err(|error| format!("{error:#}"))
                });
                cx.spawn_in(window, async move |root, cx| {
                    let result = task.await;
                    let _permit = permit;
                    let _ = root.update_in(cx, |root, window, cx| {
                        let current =
                            root.structured_pins
                                .pending
                                .as_ref()
                                .is_some_and(|pending| {
                                    pending.panel == panel
                                        && pending.sequence == sequence
                                        && pending.mount.is_current()
                                });
                        let Some(success) = root.structured_pins.finish(panel, sequence, result)
                        else {
                            return;
                        };
                        if !success || !current {
                            match panel {
                                Panel::JsonUtilityDialog => {
                                    if let Some(view) = &json {
                                        view.update(cx, |view, cx| {
                                            view.rehosting = false;
                                            cx.notify();
                                        });
                                    }
                                }
                                Panel::XmlToolkitDialog => {
                                    if let Some(view) = &xml {
                                        view.update(cx, |view, cx| {
                                            view.rehosting = false;
                                            cx.notify();
                                        });
                                    }
                                }
                                _ => {}
                            }
                        }
                        if success && current {
                            let same_root = root.favorites_toolbar_visibility_revision(window)
                                == Some(visibility_revision);
                            let focused = match panel {
                                Panel::JsonUtilityDialog => {
                                    root.json_pin_focus_handle.is_focused(window)
                                        || (popup.is_none()
                                            && json.as_ref().is_some_and(|view| {
                                                view.read(cx).contains_focus(window, cx)
                                            }))
                                }
                                Panel::XmlToolkitDialog => {
                                    root.xml_pin_focus_handle.is_focused(window)
                                        || (popup.is_none()
                                            && xml.as_ref().is_some_and(|view| {
                                                view.read(cx).contains_focus(window, cx)
                                            }))
                                }
                                _ => false,
                            };
                            if same_root && focused {
                                root.query.focus_handle(cx).focus(window, cx);
                            }
                            if let Some(target) = target_window {
                                root.structured_pins.closing = Some((panel, sequence));
                                let owner = cx.entity().downgrade();
                                cx.spawn(async move |_, cx| {
                                    match panel {
                                        Panel::JsonUtilityDialog => {
                                            if let Some(view) = json {
                                                let result = update_kit_window_content(
                                                    target,
                                                    &view,
                                                    cx,
                                                    |view, window, cx| {
                                                        if mount.is_current() {
                                                            view.rehosting = false;
                                                            view.close(window, cx);
                                                        }
                                                    },
                                                );
                                                if result.is_err() {
                                                    let _ = view.update(cx, |view, cx| {
                                                        view.rehosting = false;
                                                        cx.notify();
                                                    });
                                                }
                                            }
                                        }
                                        Panel::XmlToolkitDialog => {
                                            if let Some(view) = xml {
                                                let result = update_kit_window_content(
                                                    target,
                                                    &view,
                                                    cx,
                                                    |view, window, cx| {
                                                        if mount.is_current() {
                                                            view.rehosting = false;
                                                            view.close(window, cx);
                                                        }
                                                    },
                                                );
                                                if result.is_err() {
                                                    let _ = view.update(cx, |view, cx| {
                                                        view.rehosting = false;
                                                        cx.notify();
                                                    });
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                    let _ = owner.update(cx, |root, cx| {
                                        if root.structured_pins.closing == Some((panel, sequence)) {
                                            root.structured_pins.closing = None;
                                            cx.notify();
                                        }
                                    });
                                })
                                .detach();
                            } else {
                                match panel {
                                    Panel::JsonUtilityDialog => {
                                        if let Some(view) = &json {
                                            view.update(cx, |view, cx| {
                                                view.rehosting = false;
                                                cx.notify();
                                            });
                                        }
                                    }
                                    Panel::XmlToolkitDialog => {
                                        if let Some(view) = &xml {
                                            view.update(cx, |view, cx| {
                                                view.rehosting = false;
                                                cx.notify();
                                            });
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        cx.notify();
                    });
                })
                .detach();
                cx.notify();
            }
        }
    }
    pub(super) fn observe_structured_pin_activation(
        &mut self,
        panel: Panel,
        window: WindowId,
        cx: &mut Context<Self>,
    ) {
        if let Some(receipt) = self.structured_pins.activation.as_mut() {
            if receipt.panel == panel && receipt.window == window {
                receipt.observed = true;
            }
        }
        self.confirm_structured_pin_activation(cx);
    }
    fn confirm_structured_pin_activation(&mut self, cx: &mut Context<Self>) {
        let Some(receipt) = self.structured_pins.activation.as_ref() else {
            return;
        };
        let popup = match receipt.panel {
            Panel::JsonUtilityDialog => self.json_popup.as_ref(),
            Panel::XmlToolkitDialog => self.xml_popup.as_ref(),
            _ => None,
        };
        let valid = self.structured_pins.saved(receipt.panel)
            && !self.structured_pins.busy()
            && structured_activation_current(
                &receipt.mount,
                &receipt.workspace_guard,
                receipt.workspace_revision,
                &self.root_visibility,
                receipt.visibility_revision,
                receipt.root_native,
            )
            && self.panel_stack == receipt.stack
            && self
                .structured_pins
                .activation_revision
                .load(Ordering::Acquire)
                == receipt.revision
            && popup.is_some_and(|popup| {
                popup.window.window_id() == receipt.window && popup.native == Some(receipt.native)
            })
            && !self.restart_admission.is_fenced();
        if !valid {
            self.structured_pins.activation = None;
            return;
        }
        if receipt.accepted
            && receipt.observed
            && current_foreground_identity() == Some(receipt.native.identity)
        {
            let panel = receipt.panel;
            self.structured_pins.activation = None;
            panel_stack::promote(&mut self.panel_stack, panel);
            cx.notify();
        }
    }
}
