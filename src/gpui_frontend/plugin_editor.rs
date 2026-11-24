//! Retained Search capability editor for installed providers.
use super::*;
use gpui_kit::KeyUpEvent;
use multi_launcher::settings::{
    SearchCapabilitySaveRequest, provider_capability_enabled, provider_search_capability_enabled,
};

fn receipt_is_current(
    open: bool,
    pending: bool,
    session: u64,
    request: u64,
    expected_session: u64,
    expected_request: u64,
) -> bool {
    open && pending && session == expected_session && request == expected_request
}

pub(super) struct PluginEditorView {
    pub open: bool,
    pub pending: bool,
    root: WeakEntity<RootWindow>,
    filter: Entity<InputState>,
    focus: FocusHandle,
    return_focus: Option<FocusHandle>,
    return_scroll: Option<Point<Pixels>>,
    focus_pending: bool,
    escape_waiting_for_release: bool,
    _focus_subscriptions: Vec<Subscription>,
    session: u64,
    request: u64,
    status: Option<String>,
}

impl PluginEditorView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>, root: WeakEntity<RootWindow>) -> Self {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter plugins"));
        cx.subscribe(&filter, |_, _, _: &InputEvent, cx| cx.notify())
            .detach();
        let focus = cx.focus_handle();
        let subscriptions = vec![
            cx.on_focus_out(&focus, window, |this, _, _, _| {
                this.escape_waiting_for_release = false
            }),
            cx.observe_window_activation(window, |this, window, _| {
                if !window.is_window_active() {
                    this.escape_waiting_for_release = false;
                }
            }),
        ];
        Self {
            open: false,
            pending: false,
            root,
            filter,
            focus,
            return_focus: None,
            return_scroll: None,
            focus_pending: false,
            escape_waiting_for_release: false,
            _focus_subscriptions: subscriptions,
            session: 0,
            request: 0,
            status: None,
        }
    }
    pub fn show(
        &mut self,
        return_focus: Option<FocusHandle>,
        return_scroll: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.open {
            self.session = self.session.wrapping_add(1).max(1);
            self.open = true;
            self.return_focus = return_focus;
            self.return_scroll = Some(return_scroll);
        }
        self.focus_pending = true;
        let _ = window;
        cx.notify();
    }
    pub fn session(&self) -> u64 {
        self.session
    }
    pub fn focus_filter(&mut self, session: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.open && self.focus_pending && self.session == session {
            self.focus_pending = false;
            self.filter.focus_handle(cx).focus(window, cx);
        }
    }
    pub fn owns_focus(&self, window: &Window, cx: &App) -> bool {
        self.open && self.focus.contains_focused(window, cx)
    }
    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending {
            self.status = Some("Wait for the Search capability save.".into());
            cx.notify();
            return;
        }
        let owned_focus = self.owns_focus(window, cx);
        self.open = false;
        self.focus_pending = false;
        self.session = self.session.wrapping_add(1).max(1);
        let return_focus = self.return_focus.take();
        let return_scroll = self.return_scroll.take();
        let _ = self.root.update(cx, |root, cx| {
            if owned_focus {
                if let Some(focus) =
                    return_focus.filter(|focus| root.focus_handle.contains(focus, window))
                {
                    if let Some(offset) = return_scroll {
                        root.root_scroll.set_offset(offset);
                    }
                    focus.focus(window, cx);
                } else {
                    root.root_scroll
                        .scroll_to_top_of_item(ROOT_SCROLL_SEARCH_ITEM);
                    root.query.focus_handle(cx).focus(window, cx);
                }
            }
            cx.notify();
        });
        cx.notify();
    }
    pub(super) fn handle_escape(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        held: Option<bool>,
    ) -> bool {
        let contexts = window.context_stack();
        if !self.owns_focus(window, cx)
            || window.has_active_dialog(cx)
            || context_stack_has_primary(&contexts, "List")
            || context_stack_has_primary(&contexts, "Dialog")
            || context_stack_has_primary(&contexts, "PopupMenu")
            || context_stack_has_primary(&contexts, "AlertDialog")
        {
            return false;
        }
        if !held.unwrap_or(self.escape_waiting_for_release) {
            self.escape_waiting_for_release = true;
            self.close(window, cx);
        }
        true
    }

    fn save(
        &mut self,
        name: String,
        baseline: bool,
        desired: bool,
        rendered_session: u64,
        rendered_filter: &str,
        rendered_member: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.open
            || self.pending
            || baseline == desired
            || self.session != rendered_session
            || self.filter.read(cx).value().to_lowercase() != rendered_filter
        {
            return;
        }
        let root = self.root.clone();
        let owner_id = cx.entity_id();
        let start = root.update(cx, |root, root_cx| {
            if !root
                .plugin_editor
                .as_ref()
                .is_some_and(|view| view.entity_id() == owner_id)
                || !root.root_visibility.active.load(Ordering::Acquire)
                || !root.root_visibility.visible.load(Ordering::Acquire)
                || !root
                    .root_window
                    .is_some_and(|handle| handle.window_id() == window.window_handle().window_id())
                || !window.is_window_active()
                || window.has_active_dialog(root_cx)
                || root.restart_admission.is_fenced()
                || root.action_sheet.is_some()
                || root.context_favorite_popup.is_some()
                || root.file_menu_open
                || root.settings_menu_open
                || root.help_menu_open
            {
                return Err("Plugin Editor is no longer the active window.".to_owned());
            }
            #[cfg(windows)]
            if !root.native_window.is_some_and(|native| {
                native_window_is_valid(native)
                    && current_foreground_identity() == Some(native.identity)
            }) {
                return Err("Plugin Editor is no longer the active window.".to_owned());
            }
            let mut providers = root
                .search_catalog
                .providers
                .plugins()
                .filter(|provider| provider.name() == name);
            let Some(provider) = providers.next() else {
                return Err("Plugin catalog changed. Try again.".to_owned());
            };
            if providers.next().is_some()
                || !provider.capabilities().contains(&"search")
                || !(provider.name().to_lowercase().contains(rendered_filter)
                    || provider
                        .description()
                        .to_lowercase()
                        .contains(rendered_filter))
                || root
                    .enabled_plugins
                    .as_ref()
                    .is_none_or(|plugins| plugins.contains(&name))
                    != rendered_member
                || provider_search_capability_enabled(root.enabled_capabilities.as_ref(), &name)
                    != baseline
            {
                return Err("Plugin selection changed. Try again.".to_owned());
            }
            let permit = root
                .restart_admission
                .admit()
                .ok_or_else(|| "Application is closing.".to_owned())?;
            let request = SearchCapabilitySaveRequest {
                provider: name.clone(),
                inherited_enabled_siblings: provider
                    .capabilities()
                    .iter()
                    .copied()
                    .filter(|capability| {
                        *capability != "search"
                            && provider_capability_enabled(None, &name, capability)
                    })
                    .map(str::to_owned)
                    .collect(),
                expected_baseline: baseline,
                desired,
            };
            let receiver = root
                .settings_save_workers
                .begin_search_capability_save(root.settings_path.clone(), request)?;
            Ok::<_, String>((
                receiver,
                permit,
                root.settings_save_workers.clone(),
                root.settings_path.clone(),
            ))
        });
        let (receiver, permit, workers, settings_path) = match start {
            Ok(Ok(start)) => start,
            Ok(Err(error)) => {
                self.status = Some(error);
                cx.notify();
                return;
            }
            Err(_) => return,
        };
        self.pending = true;
        self.status = Some(format!("Saving {name} Search…"));
        self.request = self.request.wrapping_add(1).max(1);
        let (session, request) = (self.session, self.request);
        let owner = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let _permit = permit;
            let result = loop {
                match receiver.try_recv() {
                    Ok(result) => break result,
                    Err(TryRecvError::Disconnected) => {
                        break Err(multi_launcher::settings::SearchCapabilityError::Store(
                            "Settings writer ended without an acknowledgement.".to_owned(),
                        ));
                    }
                    Err(TryRecvError::Empty) => {
                        cx.background_executor()
                            .timer(Duration::from_millis(20))
                            .await
                    }
                }
            };
            let refresh = if matches!(
                &result,
                Err(multi_launcher::settings::SearchCapabilityError::Conflict { .. })
            ) {
                match workers.begin_search_capability_read(settings_path, name.clone()) {
                    Ok(receiver) => loop {
                        match receiver.try_recv() {
                            Ok(result) => break result.ok(),
                            Err(TryRecvError::Disconnected) => break None,
                            Err(TryRecvError::Empty) => {
                                cx.background_executor()
                                    .timer(Duration::from_millis(20))
                                    .await
                            }
                        }
                    },
                    Err(_) => None,
                }
            } else {
                None
            };
            let _ = root.update(cx, |root, cx| {
                let Some(view) = owner.upgrade() else {
                    return;
                };
                if !root
                    .plugin_editor
                    .as_ref()
                    .is_some_and(|mounted| mounted.entity_id() == view.entity_id())
                {
                    return;
                }
                let committed = view.update(cx, |view, cx| {
                    if !receipt_is_current(
                        view.open,
                        view.pending,
                        view.session,
                        view.request,
                        session,
                        request,
                    ) {
                        return None;
                    }
                    view.pending = false;
                    match result {
                        Ok(ack)
                            if ack.snapshot.search_enabled == desired
                                && provider_search_capability_enabled(
                                    ack.snapshot.capabilities.as_ref(),
                                    &name,
                                ) == desired =>
                        {
                            view.status = Some(format!("Saved {name} Search."));
                            cx.notify();
                            Some(ack.snapshot.capabilities)
                        }
                        Ok(_) => {
                            view.status = Some(
                                "Settings acknowledgement did not match the requested value."
                                    .into(),
                            );
                            cx.notify();
                            None
                        }
                        Err(error) => {
                            view.status = Some(format!("Could not save {name} Search: {error}"));
                            cx.notify();
                            refresh.map(|snapshot| snapshot.capabilities)
                        }
                    }
                });
                if root.restart_status.as_deref()
                    == Some("Wait for the plugin Search capability save.")
                    || root.restart_status.as_deref()
                        == Some("A plugin Search capability save is still pending.")
                {
                    root.restart_status = None;
                }
                if let Some(map) = committed {
                    root.enabled_capabilities = map;
                    let query = root.query.read(cx).value().to_string();
                    root.refresh_launcher_results_with_viewport(query, false);
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }
}

impl Render for PluginEditorView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.filter.read(cx).value().to_lowercase();
        let session = self.session;
        let rows = self
            .root
            .upgrade()
            .map(|root| {
                let root = root.read(cx);
                root.search_catalog
                    .providers
                    .plugins()
                    .filter(|provider| {
                        provider.capabilities().contains(&"search")
                            && (provider.name().to_lowercase().contains(&query)
                                || provider.description().to_lowercase().contains(&query))
                    })
                    .map(|provider| {
                        let name = provider.name();
                        let enabled = provider_search_capability_enabled(
                            root.enabled_capabilities.as_ref(),
                            name,
                        );
                        let member = root
                            .enabled_plugins
                            .as_ref()
                            .is_none_or(|plugins| plugins.contains(name));
                        (name.to_owned(), enabled, member)
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let items = rows
            .into_iter()
            .map(|(name, enabled, member)| {
                let target = name.clone();
                let rendered_filter = query.clone();
                let owner = cx.entity().downgrade();
                let id = SharedString::from(format!("plugin-editor-search-{name}"));
                Checkbox::new(id)
                    .checked(enabled)
                    .disabled(self.pending)
                    .accessibility_label(format!("{name} Search"))
                    .on_change(move |state, _, window, app| {
                        let _ = owner.update(app, |view, cx| {
                            view.save(
                                target.clone(),
                                enabled,
                                state == CheckboxState::Checked,
                                session,
                                &rendered_filter,
                                member,
                                window,
                                cx,
                            )
                        });
                    })
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                CheckboxIndicator::new()
                                    .checked(enabled)
                                    .size(px(18.0))
                                    .border_1()
                                    .child(if enabled { "✓" } else { "" }),
                            )
                            .child(Label::new(format!(
                                "{name} Search · {}",
                                if member {
                                    "enabled plugin"
                                } else {
                                    "plugin disabled"
                                }
                            ))),
                    )
            })
            .collect::<Vec<_>>();
        div()
            .id("plugin-editor-pane")
            .key_context("PluginEditorPane")
            .role(Role::Group)
            .track_focus(&self.focus)
            .flex()
            .flex_col()
            .gap_2()
            .w_full()
            .p_3()
            .capture_action(
                cx.listener(|this, _: &gpui_kit::base::input::Escape, window, cx| {
                    if this.handle_escape(window, cx, None) {
                        cx.stop_propagation();
                    }
                }),
            )
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape"
                    && event.keystroke.modifiers == Modifiers::default()
                    && this.handle_escape(window, cx, Some(event.is_held))
                {
                    cx.stop_propagation();
                }
            }))
            .capture_key_up(cx.listener(|this, event: &KeyUpEvent, _, _| {
                if event.keystroke.key == "escape" {
                    this.escape_waiting_for_release = false;
                }
            }))
            .child(Label::new("Plugin Editor · Search capabilities"))
            .child(Input::new(&self.filter).aria_label("Filter plugins"))
            .child(
                Button::new("plugin-editor-close")
                    .tab_stop(true)
                    .label("Close")
                    .on_click(cx.listener(|this, _, window, cx| this.close(window, cx))),
            )
            .when_some(self.status.clone(), |pane, status| {
                pane.child(Label::new(status))
            })
            .child(
                div()
                    .id("plugin-editor-rows")
                    .flex()
                    .flex_col()
                    .max_h(px(300.0))
                    .overflow_y_scroll()
                    .children(items),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::receipt_is_current;
    #[test]
    fn save_receipt_requires_current_open_session_and_request() {
        assert!(receipt_is_current(true, true, 4, 8, 4, 8));
        assert!(!receipt_is_current(false, true, 4, 8, 4, 8));
        assert!(!receipt_is_current(true, false, 4, 8, 4, 8));
        assert!(!receipt_is_current(true, true, 5, 8, 4, 8));
        assert!(!receipt_is_current(true, true, 4, 9, 4, 8));
    }
}
