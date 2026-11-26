//! Checked native presentation for the retained XML workspace.
use super::json_host::{
    ShellGeometry, retire_failed_utility_candidate, shell_geometry_mount_is_current,
};
use super::utility_mount::UtilityPopup;
use super::xml_utility::XmlWorkspace;
use super::*;
use multi_launcher::settings::{RememberedPresentation, UtilityToolKey};

struct XmlShell {
    workspace: Entity<XmlWorkspace>,
}
impl Render for XmlShell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("xml-popout-scroll")
            .size_full()
            .overflow_y_scroll()
            .when(
                self.workspace
                    .read(cx)
                    .is_mounted_in(window.window_handle().window_id(), true),
                |div| div.child(self.workspace.clone()),
            )
    }
}
impl RootWindow {
    fn reattach_xml_geometry(&mut self, cx: &mut Context<Self>) {
        let Some(view) = self.xml_view.as_ref() else {
            return;
        };
        let Some(popup) = self.xml_popup.as_ref() else {
            return;
        };
        let Some(native) = popup.native else {
            return;
        };
        let workspace = view.read(cx);
        let receipt = workspace.mount_receipt();
        if !shell_geometry_mount_is_current(
            workspace.is_mounted_in(popup.window.window_id(), true),
            &receipt,
        ) {
            return;
        }
        let Ok(key) = UtilityToolKey::parse("xml-toolkit") else {
            return;
        };
        match ShellGeometry::attach(
            native,
            key,
            receipt,
            self.hash_geometry_sequence.clone(),
            self.hash_wake_signal.clone(),
        ) {
            Ok(binding) => {
                if let Some(popup) = self.xml_popup.as_mut() {
                    popup.geometry = Some(binding);
                }
            }
            Err(error) => self.set_xml_size_error(
                Some(format!(
                    "Could not observe XML Toolkit window geometry: {error}"
                )),
                cx,
            ),
        }
    }
    pub(super) fn drain_xml_native_geometry(&mut self, cx: &mut Context<Self>, detach: bool) {
        let Some(popup) = self.xml_popup.as_mut() else {
            return;
        };
        let receipt = popup
            .geometry
            .as_ref()
            .map(|geometry| geometry.receipt.clone());
        let sample = popup.geometry.as_ref().and_then(ShellGeometry::take);
        if detach {
            if let Some(geometry) = popup.geometry.take() {
                drop(geometry);
            }
        }
        let Some((sample, factory)) = sample else {
            return;
        };
        let key = UtilityToolKey::parse("xml-toolkit").unwrap();
        let mut store = self
            .utility_size_store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let factory = store
            .effective_native_geometry(key)
            .is_none()
            .then_some(factory)
            .flatten();
        let result = store.observe_native_geometry(key, sample, factory);
        drop(store);
        match result {
            Ok(Some(_)) => self.start_utility_size_ack_poll(cx),
            Err(error) => {
                let message = format!("Could not save XML Toolkit window geometry: {error}");
                tracing::warn!(%message);
                if let (Some(view), Some(receipt)) = (self.xml_view.as_ref(), receipt) {
                    let view = view.downgrade();
                    cx.spawn(async move |_, cx| {
                        let _ = view.update(cx, |view, cx| {
                            if receipt.is_current() && view.open {
                                view.geometry_error = Some(message);
                                cx.notify();
                            }
                        });
                    })
                    .detach();
                }
            }
            _ => {}
        }
    }
    pub(super) fn set_xml_size_error(&mut self, message: Option<String>, cx: &mut Context<Self>) {
        if let Some(view) = self.xml_view.clone() {
            view.update(cx, |view, cx| {
                view.geometry_error = message;
                cx.notify();
            });
        }
    }
    fn remember_xml_presentation(
        &mut self,
        presentation: RememberedPresentation,
        cx: &mut Context<Self>,
    ) {
        let Ok(key) = UtilityToolKey::parse("xml-toolkit") else {
            return;
        };
        let store = self.utility_presentations.clone();
        let update = store.remember(key, presentation);
        if update == UtilityPresentationUpdate::Unchanged {
            return;
        }
        let root = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            loop {
                let failures = store.poll();
                if !failures.is_empty() {
                    let _ = root.update(cx, |root, cx| {
                        root.report_presentation_failures(failures, cx)
                    });
                }
                if !store.has_pending() {
                    break;
                }
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
            }
        })
        .detach();
    }
    pub(super) fn start_xml_rehost(
        &mut self,
        popout: bool,
        gesture: Option<hash_dock::Gesture>,
        source: utility_mount::MountReceipt,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.xml_view.clone() else {
            return;
        };
        if !source.is_current()
            || !workspace.read(cx).rehosting
            || workspace.read(cx).popout == popout
        {
            return;
        }
        let Some(permit) = self.restart_admission.admit() else {
            workspace.update(cx, |view, _| view.rehosting = false);
            return;
        };
        let Some(root_window) = self.root_window else {
            workspace.update(cx, |view, _| view.rehosting = false);
            return;
        };
        let root = cx.entity().downgrade();
        let visibility = self.root_visibility.clone();
        let key = UtilityToolKey::parse("xml-toolkit").ok();
        let geometry = key.and_then(|key| {
            let mut store = self
                .utility_size_store
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            store.track_key(key);
            store.effective_native_geometry(key)
        });
        let root_native = self.native_window;
        cx.spawn(async move |_, cx| {
            let _permit = permit;
            if popout {
                let shell_workspace = workspace.clone();
                let opened = cx.update(|app| {
                    let options = WindowOptions {
                        window_bounds: Some(WindowBounds::centered(
                            size(px(1120.0), px(760.0)),
                            app,
                        )),
                        window_min_size: Some(size(px(480.0), px(340.0))),
                        titlebar: Some(TitlebarOptions {
                            title: Some("XML Toolkit".into()),
                            ..Default::default()
                        }),
                        show: false,
                        focus: false,
                        ..Default::default()
                    };
                    gpui_kit::open_window(options, app, move |window, app| {
                        let weak = shell_workspace.downgrade();
                        let local_id = window.window_handle().window_id();
                        window.on_window_should_close(app, move |_, app| {
                            weak.update(app, |view, cx| {
                                let tracked_source = view.root.upgrade().and_then(|root| {
                                    root.read(cx)
                                        .xml_popup
                                        .as_ref()
                                        .map(|popup| popup.window.window_id())
                                });
                                let source_in_transfer = utility_mount::transfer_source_is_owned(
                                    view.rehosting,
                                    tracked_source,
                                    local_id,
                                );
                                if source_in_transfer {
                                    return false;
                                }
                                if !view.is_mounted_in(local_id, true) {
                                    return true;
                                }
                                if view.native_close_blocked() {
                                    return false;
                                }
                                let _ = view.root.update(cx, |root, cx| {
                                    root.drain_xml_native_geometry(cx, true)
                                });
                                view.shutdown(cx);
                                true
                            })
                            .unwrap_or(true)
                        });
                        app.new(|_| XmlShell {
                            workspace: shell_workspace,
                        })
                    })
                });
                let (window, _shell) = match opened {
                    Ok(opened) => opened,
                    Err(error) => {
                        let _ = workspace.update(cx, |view, cx| {
                            view.rehosting = false;
                            view.feedback = Some(format!("Could not open XML window: {error}"));
                            cx.notify();
                        });
                        return;
                    }
                };
                let mounted =
                    update_kit_window_content(window, &workspace, cx, |view, window_ref, cx| {
                        if !source.is_current()
                            || !view.open
                            || !view.rehosting
                            || !view.transfer_effects_idle()
                        {
                            return Err("XML mount changed".to_owned());
                        }
                        let native = checked_native_window(window_ref)?;
                        if cfg!(windows) && native.is_none() {
                            return Err("XML window has no checked native handle".to_owned());
                        }
                        view.replace_mount(window, native, window_ref, cx);
                        view.popout = true;
                        Ok(native)
                    });
                let native = match mounted {
                    Ok(Ok(native)) => native,
                    _ => {
                        let candidate_native = cx
                            .update(|app| {
                                window.update(app, |_, window, _| {
                                    checked_native_window(window).ok().flatten()
                                })
                            })
                            .ok()
                            .flatten();
                        retire_failed_utility_candidate(
                            &root,
                            window,
                            candidate_native,
                            utility_mount::UtilityKind::Xml,
                            cx,
                        )
                        .await;
                        let _ = workspace.update(cx, |view, cx| {
                            view.rehosting = false;
                            view.feedback = Some("Could not attach XML window.".into());
                            cx.notify();
                        });
                        return;
                    }
                };
                let mounted_receipt = cx.update(|app| workspace.read(app).mount_receipt());
                if let (Some(native), Some(geometry)) = (native, geometry) {
                    if mounted_receipt.is_current() && native_window_is_valid(native) {
                        if let Some(target) = checked_geometry_target(native) {
                            if let Err(error) = restore_native_window_geometry(
                                target,
                                Some(geometry),
                                (480, 340),
                                1.0,
                            ) {
                                tracing::warn!(%error, "could not restore XML window geometry");
                            }
                        }
                    }
                }
                let installed = root
                    .update(cx, |root, cx| {
                        if root.xml_popup.is_some()
                            || !root
                                .xml_view
                                .as_ref()
                                .is_some_and(|view| view.entity_id() == workspace.entity_id())
                            || !mounted_receipt.is_current()
                            || root.restart_admission.is_fenced()
                        {
                            return false;
                        }
                        let root_weak = cx.entity().downgrade();
                        let receipt = mounted_receipt.clone();
                        let id = window.window_id();
                        let subscription = cx.on_window_closed(move |app, closed| {
                            if closed != id {
                                return;
                            }
                            let _ = root_weak.update(app, |root, cx| {
                                if root
                                    .xml_popup
                                    .as_ref()
                                    .is_some_and(|popup| popup.window.window_id() == id)
                                    && (receipt.is_current()
                                        || root
                                            .xml_view
                                            .as_ref()
                                            .is_some_and(|view| !view.read(cx).open))
                                {
                                    root.drain_xml_native_geometry(cx, true);
                                    root.xml_popup = None;
                                    let retiring = RetiringHashWindow {
                                        window,
                                        native_window: native,
                                    };
                                    root.xml_retiring.push(retiring);
                                    let retirement_owner = cx.entity().downgrade();
                                    cx.spawn(async move |_, cx| {
                                        if wait_for_hash_window_retirement(cx, native).await {
                                            let _ = retirement_owner.update(cx, |root, _| {
                                                root.xml_retiring
                                                    .retain(|item| item.window.window_id() != id)
                                            });
                                        }
                                    })
                                    .detach();
                                    root.retire_xml_workspace(cx);
                                    panel_stack::retire(
                                        &mut root.panel_stack,
                                        Panel::XmlToolkitDialog,
                                    );
                                    cx.notify();
                                }
                            });
                        });
                        root.xml_popup = Some(UtilityPopup {
                            window,
                            native,
                            _subscription: subscription,
                            geometry: None,
                        });
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                let mut displayed = false;
                if installed {
                    if let Some(native) = native {
                        if mounted_receipt.is_current() && native_window_is_valid(native) {
                            displayed = match set_native_window_visible(native, true) {
                                Ok(()) => true,
                                Err(error) => {
                                    tracing::warn!(%error, "could not show XML window");
                                    false
                                }
                            };
                            let authorized = gesture.is_some_and(|gesture| {
                                gesture.foreground_authorized
                                    && super::json_utility::native_host_current(
                                        &visibility,
                                        gesture.visibility_revision,
                                        Some(gesture.source),
                                        false,
                                    )
                                    && current_foreground_identity()
                                        == Some(gesture.source.identity)
                            });
                            if displayed && authorized && mounted_receipt.is_current() {
                                let _ = activate_native_window(native);
                            }
                            if displayed && mounted_receipt.is_current() {
                                let _ = update_kit_window_content(
                                    window,
                                    &workspace,
                                    cx,
                                    |view, window, cx| {
                                        if mounted_receipt.is_current()
                                            && view.is_mounted_in(
                                                window.window_handle().window_id(),
                                                true,
                                            )
                                        {
                                            view.focus_editor(window, cx);
                                        }
                                    },
                                );
                            }
                        }
                    }
                }
                if installed && displayed && mounted_receipt.is_current() {
                    let _ = root.update(cx, |root, cx| {
                        if let (Some(native), Some(key)) = (native, key) {
                            match ShellGeometry::attach(
                                native,
                                key,
                                mounted_receipt.clone(),
                                root.hash_geometry_sequence.clone(),
                                root.hash_wake_signal.clone(),
                            ) {
                                Ok(binding) => {
                                    if let Some(popup) = root.xml_popup.as_mut() {
                                        popup.geometry = Some(binding);
                                    }
                                }
                                Err(error) => root.set_xml_size_error(
                                    Some(format!(
                                        "Could not observe XML Toolkit window geometry: {error}"
                                    )),
                                    cx,
                                ),
                            }
                        }
                        root.remember_xml_presentation(RememberedPresentation::Popout, cx);
                        workspace.update(cx, |view, cx| {
                            view.rehosting = false;
                            cx.notify();
                        });
                    });
                    let _ =
                        update_kit_window_content(window, &workspace, cx, |view, window, cx| {
                            if mounted_receipt.is_current()
                                && view.is_mounted_in(window.window_handle().window_id(), true)
                                && view.transfer_anchor_is_focused(window)
                            {
                                view.focus_editor(window, cx);
                            }
                        });
                } else {
                    let _ = root.update(cx, |root, _| {
                        if root
                            .xml_popup
                            .as_ref()
                            .is_some_and(|popup| popup.window.window_id() == window.window_id())
                        {
                            root.xml_popup = None;
                        }
                    });
                    let _ = update_kit_window_content(
                        root_window,
                        &workspace,
                        cx,
                        |view, window, cx| {
                            if mounted_receipt.is_current() {
                                view.replace_mount(root_window, root_native, window, cx);
                                view.popout = false;
                                view.rehosting = false;
                                view.feedback = Some("Could not show XML Toolkit window.".into());
                                if root_native.is_some_and(|native| {
                                    current_foreground_identity() == Some(native.identity)
                                }) {
                                    view.focus_editor(window, cx);
                                }
                                cx.notify();
                            }
                        },
                    );
                    retire_failed_utility_candidate(
                        &root,
                        window,
                        native,
                        utility_mount::UtilityKind::Xml,
                        cx,
                    )
                    .await;
                }
            } else {
                let source_popup = root
                    .update(cx, |root, _| {
                        root.xml_popup
                            .as_ref()
                            .map(|popup| (popup.window, popup.native))
                    })
                    .ok()
                    .flatten();
                let Some((popup, popup_native)) = source_popup else {
                    let _ = workspace.update(cx, |view, _| view.rehosting = false);
                    return;
                };
                let _ = root.update(cx, |root, cx| root.drain_xml_native_geometry(cx, true));
                let mounted =
                    update_kit_window_content(root_window, &workspace, cx, |view, window, cx| {
                        if !source.is_current()
                            || !view.open
                            || !view.rehosting
                            || !view.transfer_effects_idle()
                        {
                            return None;
                        }
                        view.replace_mount(root_window, root_native, window, cx);
                        view.popout = false;
                        Some(view.mount_receipt())
                    })
                    .ok()
                    .flatten();
                let Some(mount) = mounted else {
                    let _ = workspace.update(cx, |view, _| view.rehosting = false);
                    let _ = root.update(cx, |root, cx| {
                        if root
                            .xml_popup
                            .as_ref()
                            .is_some_and(|old| old.window.window_id() == popup.window_id())
                            && source.is_current()
                        {
                            root.reattach_xml_geometry(cx);
                        }
                    });
                    return;
                };
                let committed = root
                    .update(cx, |root, cx| {
                        if !root
                            .xml_popup
                            .as_ref()
                            .is_some_and(|old| old.window.window_id() == popup.window_id())
                            || !mount.is_current()
                        {
                            return false;
                        }
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if !committed {
                    let _ = update_kit_window_content(popup, &workspace, cx, |view, window, cx| {
                        if mount.is_current() {
                            view.replace_mount(popup, popup_native, window, cx);
                            view.popout = true;
                            view.rehosting = false;
                        }
                    });
                    let _ = root.update(cx, |root, cx| {
                        if root
                            .xml_popup
                            .as_ref()
                            .is_some_and(|old| old.window.window_id() == popup.window_id())
                        {
                            root.reattach_xml_geometry(cx);
                        }
                    });
                    return;
                }
                {
                    let ack = gesture
                        .and_then(|gesture| visibility.request_xml_visible(gesture, mount.clone()));
                    let Some((ack, accepted)) = ack else {
                        let _ =
                            update_kit_window_content(popup, &workspace, cx, |view, window, cx| {
                                if mount.is_current() {
                                    view.replace_mount(popup, popup_native, window, cx);
                                    view.popout = true;
                                    view.rehosting = false;
                                    view.feedback = Some(
                                        "Could not show the launcher to dock XML Toolkit.".into(),
                                    );
                                    if popup_native.is_some_and(|native| {
                                        current_foreground_identity() == Some(native.identity)
                                    }) {
                                        view.focus_editor(window, cx);
                                    }
                                    cx.notify();
                                }
                            });
                        let _ = root.update(cx, |root, cx| root.reattach_xml_geometry(cx));
                        return;
                    };
                    ack.wait().await;
                    let revision = accepted.load(Ordering::Acquire);
                    let accepted = revision != 0
                        && mount.is_current()
                        && root_native.is_some_and(|native| {
                            super::json_utility::native_host_current(
                                &visibility,
                                revision,
                                Some(native),
                                false,
                            ) && current_foreground_identity() == Some(native.identity)
                        });
                    if !accepted {
                        let _ =
                            update_kit_window_content(popup, &workspace, cx, |view, window, cx| {
                                if mount.is_current() {
                                    view.replace_mount(popup, popup_native, window, cx);
                                    view.popout = true;
                                    view.rehosting = false;
                                    view.feedback = Some(
                                        "Could not focus the launcher to dock XML Toolkit.".into(),
                                    );
                                    if popup_native.is_some_and(|native| {
                                        current_foreground_identity() == Some(native.identity)
                                    }) {
                                        view.focus_editor(window, cx);
                                    }
                                    cx.notify();
                                }
                            });
                        let _ = root.update(cx, |root, cx| root.reattach_xml_geometry(cx));
                        return;
                    }
                }
                let _ = root.update(cx, |root, cx| {
                    if root
                        .xml_popup
                        .as_ref()
                        .is_some_and(|old| old.window.window_id() == popup.window_id())
                    {
                        root.xml_popup = None;
                        root.xml_retiring.push(RetiringHashWindow {
                            window: popup,
                            native_window: popup_native,
                        });
                        root.remember_xml_presentation(RememberedPresentation::Docked, cx);
                        cx.notify();
                    }
                });
                let retired = remove_and_wait_for_hash_window(
                    cx,
                    RetiringHashWindow {
                        window: popup,
                        native_window: popup_native,
                    },
                )
                .await;
                if retired {
                    let _ = root.update(cx, |root, _| {
                        root.xml_retiring
                            .retain(|item| item.window.window_id() != popup.window_id())
                    });
                }
                let _ =
                    update_kit_window_content(root_window, &workspace, cx, |view, window, cx| {
                        view.rehosting = false;
                        if mount.is_current()
                            && view.is_mounted_in(window.window_handle().window_id(), false)
                            && view.transfer_anchor_is_focused(window)
                            && root_native.is_some_and(|native| {
                                current_foreground_identity() == Some(native.identity)
                            })
                        {
                            view.focus_editor(window, cx);
                        }
                        cx.notify();
                    });
            }
        })
        .detach();
    }
}
