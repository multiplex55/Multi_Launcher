//! Retained checked editor for the supported Mouse Gestures subset.
use super::*;
use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::mouse_gestures::{GestureEdit, GestureEditorSnapshot, GpuiGestureOwner};
use multi_launcher::settings::{ScopedSetting as Field, ScopedSettingChange as Change, ScopedSettingValue as Value};

fn native_current(visibility: &RootVisibility, revision: u64, native: NativeWindow) -> bool {
    #[cfg(windows)]
    { visibility.is_current_visible_revision(revision, native) && current_foreground_identity() == Some(native.identity) }
    #[cfg(not(windows))]
    { let _ = (visibility, revision, native); false }
}

pub(super) fn current_provider(query: &str, action: &Action, enabled: Option<&HashSet<String>>, caps: Option<&HashMap<String, Vec<String>>>) -> bool {
    enabled.is_none_or(|set| set.contains("mouse_gestures"))
        && multi_launcher::settings::provider_search_capability_enabled(caps, "mouse_gestures")
        && search_providers::search_routed([&search_providers::MouseGestureEdit as &dyn Plugin], query, enabled, caps)
            .iter().any(|(name, row)| name == "mouse_gestures" && row == action)
}

pub(super) struct GestureEditor {
    pub open: bool,
    root: WeakEntity<RootWindow>,
    owner: Arc<GpuiGestureOwner>,
    mount_owner: utility_mount::MountOwner,
    mount: utility_mount::MountReceipt,
    host: AnyWindowHandle,
    focus: FocusHandle,
    row_enabled_focus: FocusHandle,
    name: Entity<InputState>,
    tokens: Entity<InputState>,
    binding: Entity<InputState>,
    query: Entity<InputState>,
    snapshot: Option<GestureEditorSnapshot>,
    selected: Option<usize>,
    selected_basis: Option<multi_launcher::mouse_gestures::db::GestureEntry>,
    renaming: bool,
    rename_draft: Option<(usize, multi_launcher::mouse_gestures::db::GestureEntry, String)>,
    settings_baseline_valid: bool,
    pub(super) busy: bool,
    enabled: bool,
    practice: bool,
    pending_apply: Option<(Option<bool>, Option<bool>)>,
    status: Option<String>,
    settings_error: Option<String>,
    enable_lease: Option<Arc<AtomicBool>>,
    enable_blur: Option<Subscription>,
    enable_active: Option<Subscription>,
}

impl GestureEditor {
    fn new(root: WeakEntity<RootWindow>, owner: Arc<GpuiGestureOwner>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mount_owner = utility_mount::MountOwner::default();
        let mount = mount_owner.attach();
        Self { open: true, root, owner, mount_owner, mount, host: window.window_handle(), focus: cx.focus_handle(), row_enabled_focus: cx.focus_handle(),
            name: cx.new(|cx| InputState::new(window, cx).placeholder("Gesture name")),
            tokens: cx.new(|cx| InputState::new(window, cx).placeholder("Directions: U D L R")),
            binding: cx.new(|cx| InputState::new(window, cx).placeholder("Binding name")),
            query: cx.new(|cx| InputState::new(window, cx).placeholder("Search query")),
            snapshot: None, selected: None, selected_basis: None, renaming: false, rename_draft: None, settings_baseline_valid: false, busy: false, enabled: true, practice: false, pending_apply: None, status: None, settings_error: None, enable_lease: None, enable_blur: None, enable_active: None }
    }

    fn current(&self, window: &Window, cx: &App) -> bool {
        self.open && self.mount.is_current() && self.host.window_id() == window.window_handle().window_id()
            && self.root.upgrade().is_some_and(|root| { let root = root.read(cx);
                !root.restart_admission.is_fenced() && root.action_sheet.is_none() && root.root_visibility.active.load(Ordering::Acquire)
                    && root.root_visibility.visible.load(Ordering::Acquire) && root.native_window.is_some_and(|native| current_foreground_identity() == Some(native.identity)) })
    }

    fn select(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self.snapshot.as_ref().and_then(|snapshot| snapshot.database.gestures.get(index)) else { return; };
        self.selected = Some(index);
        self.selected_basis = Some(row.clone());
        self.renaming = false;
        self.name.update(cx, |input, cx| input.set_value(&row.label, window, cx));
        self.tokens.update(cx, |input, cx| input.set_value(&row.tokens, window, cx));
        let binding = row.bindings.iter().find(|item| item.kind == multi_launcher::mouse_gestures::db::BindingKind::SetQuery);
        self.binding.update(cx, |input, cx| input.set_value(binding.map_or("", |item| item.label.as_str()), window, cx));
        self.query.update(cx, |input, cx| input.set_value(binding.map_or("", |item| item.action.as_str()), window, cx));
        cx.notify();
    }

    fn dirty(&self, cx: &App) -> bool {
        let preferences = self.snapshot.as_ref().is_some_and(|snapshot| self.enabled != snapshot.enabled || self.practice != snapshot.practice_mode);
        let row = self.selected_basis.as_ref().is_some_and(|row| {
            let binding = row.bindings.iter().find(|item| item.kind == multi_launcher::mouse_gestures::db::BindingKind::SetQuery);
            self.tokens.read(cx).value().to_string() != row.tokens || self.binding.read(cx).value().to_string() != binding.map_or("", |item| item.label.as_str()) || self.query.read(cx).value().to_string() != binding.map_or("", |item| item.action.as_str())
        });
        preferences || row || self.renaming || self.rename_draft.is_some()
    }

    pub(super) fn blocks_root_close(&self, cx: &App) -> bool { self.busy || (self.open && self.dirty(cx)) }

    pub(super) fn revoke_enabled_operation(&mut self) {
        if let Some(lease) = &self.enable_lease { lease.store(false, Ordering::Release); }
    }

    fn present_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || !self.current(window, cx) || window.has_active_dialog(cx) { return; }
        let Some((index, expected)) = self.selected.zip(self.selected_basis.clone()) else { return; };
        let initial = self.rename_draft.as_ref().filter(|(saved_index, saved_row, _)| *saved_index == index && saved_row == &expected).map(|(_, _, name)| name.clone()).unwrap_or_else(|| expected.label.clone());
        self.name.update(cx, |input, cx| input.set_value(&initial, window, cx));
        self.renaming = true;
        let owner = cx.entity().downgrade();
        let mount = self.mount.clone();
        let input = self.name.clone();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let save = owner.clone(); let cancel = owner.clone(); let close = owner.clone();
            let save_mount = mount.clone(); let cancel_mount = mount.clone(); let close_mount = mount.clone();
            let expected = expected.clone();
            alert.title(Label::new("Rename gesture"))
                .description(Input::new(&input).aria_label("Gesture name"))
                .close_button(true).keyboard(true).confirm()
                .on_cancel(move |_, _, app| cancel.update(app, |view, cx| {
                    if !cancel_mount.is_current() || !cancel_mount.matches(&view.mount) || !view.renaming { return false; }
                    view.renaming = false; view.rename_draft = None; cx.notify(); true
                }).unwrap_or(false))
                .on_close(move |_, _, app| { let _ = close.update(app, |view, cx| {
                    if close_mount.is_current() && close_mount.matches(&view.mount) && view.renaming { view.renaming = false; view.rename_draft = None; cx.notify(); }
                }); })
                .on_ok(move |_, window, app| {
                    let expected = expected.clone();
                    let admitted = save.update(app, |view, cx| {
                        if !save_mount.is_current() || !save_mount.matches(&view.mount) || !view.renaming || view.busy || !view.current(window, cx) || view.selected != Some(index) || view.selected_basis.as_ref() != Some(&expected) { return None; }
                        let label = view.name.read(cx).value().to_string();
                        if label.trim().is_empty() { view.status = Some("Gesture name cannot be empty.".into()); cx.notify(); return None; }
                        view.renaming = false;
                        view.rename_draft = Some((index, expected.clone(), label.clone()));
                        Some(label)
                    }).ok().flatten();
                    let Some(label) = admitted else { return false; };
                    let owner = save.clone(); let mount = save_mount.clone();
                    window.defer(app, move |window, app| { let _ = owner.update(app, |view, cx| {
                        if mount.is_current() && mount.matches(&view.mount) && view.selected == Some(index) && view.selected_basis.as_ref() == Some(&expected) {
                            view.run(Some(GestureEdit::Rename { index, expected, label }), window, cx);
                        }
                    }); });
                    true
                })
        });
        if self.mount.is_current() && self.current(window, cx) && window.has_active_dialog(cx) { self.name.focus_handle(cx).focus(window, cx); }
    }

    fn run(&mut self, edit: Option<GestureEdit>, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || !self.current(window, cx) || window.has_active_dialog(cx) { return; }
        let Some(root) = self.root.upgrade() else { return; };
        let root = root.read(cx);
        let Some(permit) = root.restart_admission.admit() else { return; };
        let Some(native) = root.native_window else { return; };
        let visibility = root.root_visibility.clone();
        let admission = root.restart_admission.clone();
        let settings_path = root.settings_path.clone();
        let revision = visibility.request_revision.load(Ordering::Acquire);
        let receipt = self.mount.clone();
        let effect_receipt = receipt.clone();
        let owner = self.owner.clone();
        let snapshot = self.snapshot.clone();
        let added = matches!(edit, Some(GestureEdit::Add));
        let renamed = matches!(edit, Some(GestureEdit::Rename { .. }));
        let directions = matches!(edit, Some(GestureEdit::Tokens { .. }));
        let row_enabled = matches!(edit, Some(GestureEdit::Enabled { .. }));
        let edited = edit.is_some();
        if added && self.dirty(cx) { self.status = Some("Save or discard the current draft before adding a gesture.".into()); cx.notify(); return; }
        let enabled_lease = if row_enabled {
            if self.dirty(cx) || !window.is_window_active() { return; }
            let Some(focus) = window.focused(cx) else { return; };
            if !root.external_owner_is_current(Some(&focus), window, cx) { return; }
            let lease = Arc::new(AtomicBool::new(true));
            let blur = lease.clone();
            self.enable_blur = Some(cx.on_blur(&focus, window, move |_, _, _| blur.store(false, Ordering::Release)));
            let inactive = lease.clone();
            self.enable_active = Some(cx.observe_window_activation(window, move |_, window, _| {
                if !window.is_window_active() { inactive.store(false, Ordering::Release); }
            }));
            self.enable_lease = Some(lease.clone());
            Some(lease)
        } else { None };
        self.busy = true;
        self.status = None;
        let host = self.host;
        let task = cx.background_executor().spawn(async move {
            let durable_settings = (!row_enabled).then(|| multi_launcher::settings::Settings::read_mouse_gesture_preferences(&settings_path));
            let current = || effect_receipt.is_current() && native_current(&visibility, revision, native)
                && enabled_lease.as_ref().is_none_or(|lease| lease.load(Ordering::Acquire) && !admission.is_fenced());
            let result = match edit {
                Some(edit) => owner.commit_editor(&snapshot.ok_or_else(|| anyhow::anyhow!("Reload the gesture list first"))?, edit, current),
                None => owner.editor_snapshot(),
            };
            Ok::<_, anyhow::Error>((result, durable_settings))
        });
        cx.spawn(async move |weak, cx| {
            let _permit = permit;
            let outcome = task.await;
            if let Some(view) = weak.upgrade() {
                let _ = update_kit_window_content(host, &view, cx, |view, window, cx| {
                    if !receipt.is_current() || !receipt.matches(&view.mount) { return; }
                    view.busy = false;
                    if row_enabled { view.enable_lease = None; view.enable_blur = None; view.enable_active = None; }
                    match outcome {
                        Ok((Ok(mut snapshot), durable)) if view.owner.editor_snapshot_is_current(&snapshot) => {
                            if let Some(durable) = durable {
                                view.settings_baseline_valid = durable.is_ok();
                                match durable { Ok((enabled, practice)) => { snapshot.enabled = enabled; snapshot.practice_mode = practice; view.settings_error = None; }, Err(error) => view.settings_error = Some(format!("Could not read Mouse Gestures settings: {error}")) }
                                if view.snapshot.as_ref().is_none_or(|old| view.enabled == old.enabled) { view.enabled = snapshot.enabled; }
                                if view.snapshot.as_ref().is_none_or(|old| view.practice == old.practice_mode) { view.practice = snapshot.practice_mode; }
                            } else if let Some(old) = &view.snapshot {
                                snapshot.enabled = old.enabled;
                                snapshot.practice_mode = old.practice_mode;
                            }
                            let selection = if added { snapshot.database.gestures.len().checked_sub(1) } else { view.selected.filter(|index| *index < snapshot.database.gestures.len()) };
                            view.snapshot = Some(snapshot);
                            if added { if let Some(index) = selection { view.select(index, window, cx); } }
                            else if edited && let Some(index) = selection { view.selected_basis = view.snapshot.as_ref().and_then(|snapshot| snapshot.database.gestures.get(index)).cloned(); }
                            if directions && let Some(row) = &view.selected_basis { view.tokens.update(cx, |input, cx| input.set_value(&row.tokens, window, cx)); }
                            if row_enabled { view.status = Some("Gesture enabled setting saved.".into()); }
                            if added && view.current(window, cx) { view.present_rename(window, cx); }
                            else if renamed { view.renaming = false; view.rename_draft = None; }
                        }
                        Ok((Ok(_), _)) => view.status = Some("Gesture list changed; reload it.".into()),
                        Ok((Err(error), _)) | Err(error) => view.status = Some(error.to_string()),
                    }
                    cx.notify();
                });
            }
        }).detach();
        cx.notify();
    }

    fn save_preferences(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || !self.settings_baseline_valid || !self.current(window, cx) || window.has_active_dialog(cx) { return; }
        let Some(snapshot) = self.snapshot.clone() else { return; };
        let changes: Vec<_> = [Change { field: Field::MouseGesturesEnabled, baseline: Value::Bool(snapshot.enabled), desired: Value::Bool(self.enabled) }, Change { field: Field::MouseGesturesPracticeMode, baseline: Value::Bool(snapshot.practice_mode), desired: Value::Bool(self.practice) }].into_iter().filter(|change| change.baseline != change.desired).collect();
        let apply_enabled = if self.enabled != snapshot.enabled { Some(self.enabled) } else { self.pending_apply.and_then(|pair| pair.0) };
        let apply_practice = if self.practice != snapshot.practice_mode { Some(self.practice) } else { self.pending_apply.and_then(|pair| pair.1) };
        let Some(root) = self.root.upgrade() else { return; };
        let root = root.read(cx);
        let Some(permit) = root.restart_admission.admit() else { return; };
        let Some(native) = root.native_window else { return; };
        let workers = root.settings_save_workers.clone();
        let settings_path = root.settings_path.clone();
        let (receiver, token) = match workers.begin_scoped_preferences_save(settings_path.clone(), changes.clone()) { Ok(result) => result, Err(error) => { self.status = Some(error); cx.notify(); return; } };
        let visibility = root.root_visibility.clone();
        let revision = visibility.request_revision.load(Ordering::Acquire);
        let mount = self.mount.clone();
        let check = mount.clone();
        let owner = self.owner.clone();
        let enabled = self.enabled; let practice = self.practice;
        let host = self.host;
        self.busy = true;
        let task = cx.background_executor().spawn(async move {
            let ack = match receiver.recv() { Ok(Ok(ack)) => ack, Ok(Err(error)) => return (false, Err(error.to_string())), Err(_) => return (false, Err("Settings writer ended without acknowledgement".to_owned())) };
            if !settings_actions::ack_matches(&changes, &ack) { return (false, Err("Settings acknowledgement changed".into())); }
            let current = || check.is_current() && native_current(&visibility, revision, native);
            let result = multi_launcher::settings::Settings::with_checked_mouse_gesture_preferences(&settings_path, Some(enabled), Some(practice), || owner.apply_saved_preferences(apply_enabled, apply_practice, current)).and_then(|result| match result {
                multi_launcher::plugins::mouse_gestures::GpuiGestureCapture::Failed(error) => Err(error),
                other => Ok((format!("Saved. Capture: {other:?}"), owner.editor_snapshot().ok())),
            });
            (true, result)
        });
        cx.spawn(async move |weak, cx| {
            let _permit = permit;
            let (saved, result) = task.await;
            workers.acknowledge_boolean_preference_save(token.id());
            if let Some(view) = weak.upgrade() {
                let _ = update_kit_window_content(host, &view, cx, |view, _window, cx| {
                    if !mount.is_current() || !mount.matches(&view.mount) { return; }
                    view.busy = false;
                    if saved && let Some(snapshot) = view.snapshot.as_mut() { snapshot.enabled = enabled; snapshot.practice_mode = practice; }
                    if saved { view.pending_apply = Some((apply_enabled, apply_practice)); }
                    view.status = Some(match result {
                        Ok((message, snapshot)) => { view.pending_apply = None; if let Some(mut snapshot) = snapshot.filter(|snapshot| view.owner.editor_snapshot_is_current(snapshot)) { snapshot.enabled = enabled; snapshot.practice_mode = practice; view.snapshot = Some(snapshot); } message }
                        Err(error) if saved => format!("Settings saved; capture could not be applied: {error}. Retry capture."),
                        Err(error) => format!("Could not save preferences: {error}"),
                    });
                    cx.notify();
                });
            }
        }).detach();
        cx.notify();
    }
}

impl Render for GestureEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let escape = self.mount.clone();
        let mut panel = div().id("gesture-editor").track_focus(&self.focus)
            .capture_action(cx.listener(move |view, _: &gpui_kit::base::input::Escape, window, cx| {
                if !view.busy && escape.is_current() && view.current(window, cx) && !window.has_active_dialog(cx) && view.focus.contains_focused(window, cx) {
                    if view.renaming { view.renaming = false; cx.notify(); }
                    else if view.dirty(cx) { view.status = Some("Save or discard the current draft before closing.".into()); cx.notify(); }
                    else { view.open = false; view.mount_owner.retire(); let _ = view.root.update(cx, |root, cx| root.query.focus_handle(cx).focus(window, cx)); cx.notify(); }
                    cx.stop_propagation();
                }
            }))
            .flex().flex_col().gap_2().p_3()
            .child(Label::new("Mouse Gestures"));
        let busy = self.busy;
        let receipt = self.mount.clone();
        panel = panel.child(Button::new("gesture-enabled").label(format!("Enabled: {}", self.enabled)).disabled(busy).on_click(cx.listener(move |view, _, window, cx| {
            if !view.busy && receipt.is_current() && view.current(window, cx) { view.enabled = !view.enabled; cx.notify(); }
        })));
        let receipt = self.mount.clone();
        panel = panel.child(Button::new("gesture-practice").label(format!("Practice: {}", self.practice)).disabled(busy).on_click(cx.listener(move |view, _, window, cx| {
            if !view.busy && receipt.is_current() && view.current(window, cx) { view.practice = !view.practice; cx.notify(); }
        })));
        panel = panel.child(Button::new("gesture-save-settings").label("Save settings / retry capture").disabled(busy || self.snapshot.is_none() || !self.settings_baseline_valid).on_click(cx.listener(|view, _, window, cx| view.save_preferences(window, cx))));
        panel = panel.child(Button::new("gesture-reload").label("Reload gestures").disabled(busy).on_click(cx.listener(|view, _, window, cx| view.run(None, window, cx))));
        panel = panel.child(Button::new("gesture-add").label("Add gesture").disabled(busy || self.snapshot.is_none()).on_click(cx.listener(|view, _, window, cx| view.run(Some(GestureEdit::Add), window, cx))));
        if let Some(snapshot) = &self.snapshot {
            for (index, row) in snapshot.database.gestures.iter().enumerate() {
                let label = multi_launcher::mouse_gestures::db::format_gesture_label(row);
                let mount = self.mount.clone();
                let expected = row.clone();
                panel = panel.child(Button::new(SharedString::from(format!("gesture-row-{index}"))).label(label).disabled(busy).on_click(cx.listener(move |view, _, window, cx| {
                    if !view.busy && mount.is_current() && view.current(window, cx) && view.snapshot.as_ref().is_some_and(|snapshot| view.owner.editor_snapshot_is_current(snapshot) && snapshot.database.gestures.get(index) == Some(&expected)) {
                        if view.selected == Some(index) { return; }
                        if view.dirty(cx) { view.status = Some("Save or discard the current draft before selecting another gesture.".into()); cx.notify(); }
                        else { view.select(index, window, cx); }
                    }
                })));
            }
        }
        if let Some((index, row)) = self.selected.zip(self.selected_basis.clone()) {
            for (binding_index, binding) in row.bindings.iter().enumerate().filter(|(_, binding)| binding.kind != multi_launcher::mouse_gestures::db::BindingKind::SetQuery) {
                panel = panel.child(div().id(SharedString::from(format!("gesture-unsupported-{index}-{binding_index}"))).child(Label::new(format!("{} (read-only)", binding.label))));
            }
            let rename_row = row.clone();
            let rename_mount = self.mount.clone();
            panel = panel.child(Button::new("gesture-rename").label("Rename").disabled(busy).on_click(cx.listener(move |view, _, window, cx| {
                if rename_mount.is_current() && rename_mount.matches(&view.mount) && view.selected == Some(index) && view.selected_basis.as_ref() == Some(&rename_row) { view.present_rename(window, cx); }
            })));
            if row.dir_mode == multi_launcher::mouse_gestures::engine::DirMode::Four {
                let dirty = self.dirty(cx);
                let expected = row.clone();
                let mount = self.mount.clone();
                let revision = self.snapshot.as_ref().map(|snapshot| snapshot.revision);
                let snapshot_current = self.snapshot.as_ref().is_some_and(|snapshot| self.owner.editor_snapshot_is_current(snapshot));
                let weak = cx.entity().downgrade();
                let palette = appearance::current(cx).palette;
                let focused = self.row_enabled_focus.is_focused(window);
                panel = panel.child(Checkbox::new("gesture-row-enabled")
                    .checked(row.enabled)
                    .disabled(!(busy && self.enable_lease.is_some()) && (busy || dirty || revision.is_none() || !snapshot_current))
                    .accessibility_label("Gesture enabled")
                    .track_focus(&self.row_enabled_focus)
                    .on_change(move |state, _, window, app| {
                        let _ = weak.update(app, |view, cx| {
                            let enabled = state == CheckboxState::Checked;
                            if view.busy || !mount.is_current() || !mount.matches(&view.mount)
                                || view.selected != Some(index) || view.selected_basis.as_ref() != Some(&expected) { return; }
                            if view.snapshot.as_ref().is_none_or(|snapshot| !view.owner.editor_snapshot_is_current(snapshot)) {
                                view.status = Some("Gesture list changed; reload it.".into()); cx.notify(); return;
                            }
                            if view.busy || view.dirty(cx) || enabled == expected.enabled
                                || !mount.is_current() || !mount.matches(&view.mount)
                                || view.selected != Some(index) || view.selected_basis.as_ref() != Some(&expected)
                                || !view.snapshot.as_ref().is_some_and(|snapshot| Some(snapshot.revision) == revision
                                    && view.owner.editor_snapshot_is_current(snapshot)
                                    && snapshot.database.gestures.get(index) == Some(&expected))
                                || !view.current(window, cx) || !window.is_window_active() || window.has_active_dialog(cx) {
                                return;
                            }
                            let focus = window.focused(cx);
                            if !view.root.upgrade().is_some_and(|root| root.read(cx).external_owner_is_current(focus.as_ref(), window, cx)) { return; }
                            view.run(Some(GestureEdit::Enabled { index, expected: expected.clone(), enabled }), window, cx);
                        });
                    })
                    .child(div().flex().items_center().gap_2()
                        .child(CheckboxIndicator::new().checked(row.enabled).size(px(18.0)).rounded(px(3.0)).border_1()
                            .border_color(appearance::to_hsla(if focused { palette.focus } else { palette.control_border }))
                            .when(row.enabled, |indicator| indicator.bg(appearance::to_hsla(palette.accent)).text_color(appearance::to_hsla(palette.primary_text)))
                            .items_center().justify_center().child(if row.enabled { "✓" } else { "" }))
                        .child(Label::new("Gesture enabled"))));
                if dirty { panel = panel.child(Label::new("Save or discard the current draft before changing gesture enablement.")); }
                if !snapshot_current { panel = panel.child(Label::new("Gesture list changed; reload it.")); }
                panel = panel.child(Input::new(&self.tokens).readonly(busy).aria_label("Four direction tokens"));
                let expected = row.clone();
                let token_mount = self.mount.clone();
                panel = panel.child(Button::new("gesture-save-tokens").label("Save directions").disabled(busy).on_click(cx.listener(move |view, _, window, cx| {
                    if !token_mount.is_current() || !token_mount.matches(&view.mount) || view.selected != Some(index) || view.selected_basis.as_ref() != Some(&expected) { return; }
                    let tokens = view.tokens.read(cx).value().to_string();
                    view.run(Some(GestureEdit::Tokens { index, expected: expected.clone(), tokens }), window, cx);
                })));
                panel = panel.child(Input::new(&self.binding).readonly(busy).aria_label("Binding name"));
                panel = panel.child(Input::new(&self.query).readonly(busy).aria_label("Set query"));
                let binding = row.bindings.iter().position(|item| item.kind == multi_launcher::mouse_gestures::db::BindingKind::SetQuery);
                let query_mount = self.mount.clone();
                panel = panel.child(Button::new("gesture-save-query").label("Save Set Query").disabled(busy).on_click(cx.listener(move |view, _, window, cx| {
                    if !query_mount.is_current() || !query_mount.matches(&view.mount) || view.selected != Some(index) || view.selected_basis.as_ref() != Some(&row) { return; }
                    let label = view.binding.read(cx).value().to_string();
                    let query = view.query.read(cx).value().to_string();
                    view.run(Some(GestureEdit::SetQuery { index, expected: row.clone(), binding, label, query }), window, cx);
                })));
            } else { panel = panel.child(Label::new("This direction mode is read-only.")); }
        }
        panel = panel.child(Button::new("gesture-discard-draft").label("Discard drafts").disabled(busy || !self.dirty(cx)).on_click(cx.listener(|view, _, window, cx| {
            if !view.busy && view.current(window, cx) {
                if let Some(snapshot) = &view.snapshot { view.enabled = snapshot.enabled; view.practice = snapshot.practice_mode; }
                view.rename_draft = None;
                if let Some(index) = view.selected { view.select(index, window, cx); }
                view.status = None; cx.notify();
            }
        })));
        if let Some(error) = &self.settings_error { panel = panel.child(Label::new(error.clone()).id("gesture-settings-error")); }
        if let Some(status) = &self.status { panel = panel.child(Label::new(status.clone()).id("gesture-editor-status")); }
        panel.child(Button::new("gesture-close").label("Close").disabled(busy).on_click(cx.listener(|view, _, window, cx| {
            if view.current(window, cx) && !view.busy {
                if view.dirty(cx) { view.status = Some("Save or discard the current draft before closing.".into()); cx.notify(); return; }
                let focused = view.focus.contains_focused(window, cx);
                view.open = false; view.mount_owner.retire();
                if focused { let _ = view.root.update(cx, |root, cx| root.query.focus_handle(cx).focus(window, cx)); }
                cx.notify();
            }
        })))
    }
}

impl RootWindow {
    pub(super) fn open_gesture_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.restart_admission.is_fenced() { return; }
        let Some(owner) = self.gesture_owner.clone() else { self.gesture_status = Some("Mouse Gestures is unavailable.".into()); cx.notify(); return; };
        let view = if let Some(view) = self.gesture_editor.clone() {
            view.update(cx, |view, _| { if !view.open { view.mount = view.mount_owner.attach(); view.open = true; } });
            view
        } else {
            let root = cx.entity().downgrade();
            let view = cx.new(|cx| GestureEditor::new(root, owner, window, cx));
            self.gesture_editor = Some(view.clone());
            view
        };
        view.update(cx, |view, cx| view.focus.focus(window, cx));
        let weak = view.downgrade();
        window.defer(cx, move |window, cx| {
            if let Some(view) = weak.upgrade() {
                view.update(cx, |view, cx| { if !view.busy { view.run(None, window, cx); } });
            }
        });
        self.root_scroll.scroll_to_bottom();
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::current_provider;
    use multi_launcher::actions::Action;
    #[test]
    fn editor_route_accepts_only_exact_stock_search_row() {
        let row = Action { label: "Edit mouse gestures".into(), desc: "Mouse gestures".into(), action: "mg:dialog".into(), args: None };
        assert!(current_provider("mg edit", &row, None, None));
        assert!(!current_provider("mg", &row, None, None));
    }
}
