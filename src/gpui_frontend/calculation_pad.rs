//! Retained Calculation Pad editor, backed by the shared session.
use super::*;
#[path = "calculation_pad_find.rs"]
mod find;
use multi_launcher::calculation_pad::{
    self, CalculationPadSession, CalculationPolicy, EvaluationBasis, LineId, LineOutcome,
};
use multi_launcher::plugin::Plugin;

pub(super) const WAIT: &str = "Save the Calculation Pad document before closing.";

#[derive(Clone)]
enum DocumentCommand {
    List,
    New,
    Open(calculation_pad::DocumentId),
    Save,
    SaveAs(String),
    Close,
    Resolve(calculation_pad::TransitionDecision),
}

fn document_receipt_current(session: Option<&CalculationPadSession>, basis: Option<EvaluationBasis>, pending: Option<calculation_pad::PendingTransition>, revision: u64, expected_revision: u64) -> bool {
    session.is_some_and(|session| session.evaluation_basis() == basis && session.pending_transition() == pending)
        && revision == expected_revision
}

fn copy_basis_is_current(session: Option<&CalculationPadSession>, basis: EvaluationBasis) -> bool {
    session.is_some_and(|session| session.pending_transition().is_none() && session.evaluation_basis() == Some(basis))
}

pub(super) fn current_provider(
    query: &str,
    action: &Action,
    enabled: Option<&HashSet<String>>,
    capabilities: Option<&HashMap<String, Vec<String>>>,
) -> bool {
    enabled.is_none_or(|set| set.contains("calculator"))
        && multi_launcher::settings::provider_search_capability_enabled(capabilities, "calculator")
        && action.args.is_none()
        && action.action == "calc:pad"
        && super::search_providers::search_routed(
            [&super::search_providers::CalculatorMetadata::default() as &dyn Plugin],
            query,
            enabled,
            capabilities,
        )
        .into_iter()
        .any(|(name, row)| name == "calculator" && row == *action)
}

pub(super) struct CalculationPadView {
    pub(super) open: bool,
    root: WeakEntity<RootWindow>,
    host: AnyWindowHandle,
    native: Option<NativeWindow>,
    visibility: RootVisibility,
    admission: Arc<RestartAdmission>,
    owner: utility_mount::MountOwner,
    mount: utility_mount::MountReceipt,
    editor: Entity<TextareaState>,
    find: find::FindOwner,
    _editor_subscription: Subscription,
    session: Option<CalculationPadSession>,
    documents: Vec<calculation_pad::PadDocumentSummary>,
    document_revision: u64,
    save_name: Entity<InputState>,
    focus: FocusHandle,
    document_lease: Option<Arc<AtomicBool>>,
    document_focus_guard: Option<Subscription>,
    document_active_guard: Option<Subscription>,
    wake: Arc<HashWakeSignal>,
    updating: bool,
    pending_copy: bool,
    copy_lease: Option<Arc<AtomicBool>>,
    focus_guard: Option<Subscription>,
    active_guard: Option<Subscription>,
    message: Option<String>,
}

impl CalculationPadView {
    fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        root: WeakEntity<RootWindow>,
        native: Option<NativeWindow>,
        visibility: RootVisibility,
        admission: Arc<RestartAdmission>,
        data: multi_launcher::platform::app_data::AppDataRoot,
    ) -> Self {
        let owner = utility_mount::MountOwner::default();
        let mount = owner.attach();
        let editor = cx.new(|cx| TextareaState::new(window, cx).auto_grow(8, 20));
        let save_name = cx.new(|cx| InputState::new(window, cx).placeholder("Saved Pad name"));
        let subscription = cx.subscribe_in(
            &editor,
            window,
            |view: &mut Self, editor, event: &InputEvent, window, cx| {
                if !matches!(event, InputEvent::Change) || view.updating {
                    return;
                }
                view.revoke_copy();
                let text = editor.read(cx).value().to_string();
                let Some(session) = view.session.as_mut() else { return; };
                match session.replace_source(text) {
                    Ok(_) => {
                        view.message = None;
                        session.advance_automatic();
                    }
                    Err(error) => {
                        view.message = Some(format!("Could not edit document: {error}"));
                        view.updating = true;
                        let source = session.source().unwrap_or("").to_owned();
                        editor.update(cx, |editor, cx| editor.set_value(source, window, cx));
                        view.updating = false;
                    }
                }
                cx.notify();
            },
        );
        let find = find::FindOwner::new(window, cx);
        let wake = Arc::new(HashWakeSignal::default());
        let notify = wake.clone();
        let mut session = CalculationPadSession::new(&data);
        session.enable_background_evaluation(move || notify.notify());
        session.advance_automatic();
        let receiver = wake.clone();
        cx.spawn(async move |weak, cx| {
            while receiver.wait().await {
                if weak
                    .update(cx, |view, cx| {
                        if let Some(session) = view.session.as_mut() { session.advance_automatic(); }
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        Self {
            open: true,
            root,
            host: window.window_handle(),
            native,
            visibility,
            admission,
            owner,
            mount,
            editor,
            find,
            _editor_subscription: subscription,
            session: Some(session),
            documents: Vec::new(),
            document_revision: 0,
            save_name,
            focus: cx.focus_handle(),
            document_lease: None,
            document_focus_guard: None,
            document_active_guard: None,
            wake,
            updating: false,
            pending_copy: false,
            copy_lease: None,
            focus_guard: None,
            active_guard: None,
            message: None,
        }
    }
    pub(super) fn blocks_close(&self) -> bool {
        self.open && (self.session.as_ref().is_none_or(|session| session.is_dirty() || session.pending_transition().is_some()) || self.pending_copy)
    }
    fn revoke_copy(&mut self) {
        if let Some(lease) = &self.copy_lease {
            lease.store(false, Ordering::Release);
        }
    }
    pub(super) fn revoke_document(&mut self) {
        if let Some(lease) = &self.document_lease { lease.store(false, Ordering::Release); }
    }
    fn document_command(&mut self, command: DocumentCommand, expected: Option<EvaluationBasis>, expected_pending: Option<calculation_pad::PendingTransition>, revision: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending_copy || !self.current(window, cx)
            || !document_receipt_current(self.session.as_ref(), expected, expected_pending, self.document_revision, revision) { return; }
        let Some(permit) = self.admission.admit() else { return; };
        self.editor.focus_handle(cx).focus(window, cx);
        let Some(mut session) = self.session.take() else { return; };
        let was_dirty = session.is_dirty();
        self.document_revision = self.document_revision.wrapping_add(1);
        let attempt = self.document_revision;
        self.revoke_copy();
        let lease = Arc::new(AtomicBool::new(true));
        let blur = lease.clone();
        self.document_focus_guard = Some(cx.on_focus_out(&self.focus, window, move |_, _, _, _| blur.store(false, Ordering::Release)));
        let active = lease.clone();
        self.document_active_guard = Some(cx.observe_window_activation(window, move |_, window, _| {
            if !window.is_window_active() { active.store(false, Ordering::Release); }
        }));
        self.document_lease = Some(lease.clone());
        let worker_lease = lease.clone();
        let save_transition = expected_pending.is_some() && matches!(command, DocumentCommand::SaveAs(_) | DocumentCommand::Resolve(calculation_pad::TransitionDecision::Save));
        let mount = self.mount.clone();
        let worker_mount = mount.clone();
        let admission = self.admission.clone();
        let visibility = self.visibility.clone();
        let revision = visibility.request_revision.load(Ordering::Acquire);
        let native = self.native;
        let task = cx.background_executor().spawn(async move {
            let current = || worker_mount.is_current() && !admission.is_fenced()
                && worker_lease.load(Ordering::Acquire)
                && json_utility::native_host_current(&visibility, revision, native, false)
                && native.is_some_and(|native| current_foreground_identity() == Some(native.identity));
            let mut documents = None;
            let result = if !current() {
                Err(calculation_pad::SessionError::Store(calculation_pad::PadStoreError::Revoked))
            } else {
                match command {
                    DocumentCommand::List => session.list_saved_documents().and_then(|list| {
                        if !current() { return Err(calculation_pad::SessionError::Store(calculation_pad::PadStoreError::Revoked)); }
                        documents = Some(list); Ok(())
                    }),
                    DocumentCommand::New => session.request_new_if(&current).map(|_| ()),
                    DocumentCommand::Open(id) => session.request_open_if(id, &current).map(|_| ()),
                    DocumentCommand::Save => session.save_if(&current),
                    DocumentCommand::SaveAs(name) => session.save_as_if(&name, &current).and_then(|_| {
                        if session.pending_transition().is_some() {
                            session.resolve_pending_if(calculation_pad::TransitionDecision::Save, &current).map(|_| ())
                        } else { Ok(()) }
                    }),
                    DocumentCommand::Close => session.request_close_if(&current).map(|_| ()),
                    DocumentCommand::Resolve(decision) => session.resolve_pending_if(decision, &current).map(|_| ()),
                }
            };
            (session, documents, result, permit)
        });
        let host = self.host;
        cx.spawn(async move |weak, cx| {
            let (session, documents, result, _permit) = task.await;
            let Some(view) = weak.upgrade() else { return; };
            let _ = update_kit_window_content(host, &view, cx, |view, window, cx| {
                if !mount.is_current() || !mount.matches(&view.mount) || view.document_revision != attempt || view.session.is_some() { return; }
                view.session = Some(session);
                view.document_lease = None;
                view.document_focus_guard = None;
                view.document_active_guard = None;
                if lease.load(Ordering::Acquire) && window.is_window_active() {
                    if let Some(documents) = documents { view.documents = documents; }
                }
                let needs_name = matches!(&result, Err(calculation_pad::SessionError::SaveAsRequired));
                let partial_save = save_transition && was_dirty && result.is_err() && view.session.as_ref().is_some_and(|session| !session.is_dirty() && session.pending_transition().is_some());
                view.message = result.err().map(|error| if partial_save { format!("Saved the current Pad, but could not complete the requested action: {error}") } else { error.to_string() });
                if needs_name && lease.load(Ordering::Acquire) && window.is_window_active() && view.focus.contains_focused(window, cx) {
                    view.message = Some("Enter a name in Save As, then choose Save As to continue.".into());
                    view.save_name.focus_handle(cx).focus(window, cx);
                }
                if view.session.as_ref().is_some_and(|session| session.source().is_none()) {
                    view.reset_find(window, cx);
                    view.open = false;
                    view.owner.retire();
                    if lease.load(Ordering::Acquire) && view.focus.contains_focused(window, cx) && window.is_window_active() && let Some(root) = view.root.upgrade() {
                        root.update(cx, |root, cx| root.query.focus_handle(cx).focus(window, cx));
                    }
                } else {
                    view.set_editor_from_session(window, cx);
                }
                if !view.blocks_close() {
                    let _ = view.root.update(cx, |root, cx| {
                        if root.restart_status.as_deref() == Some(WAIT) {
                            root.restart_status = None;
                            cx.notify();
                        }
                    });
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }
    fn current(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.open
            && self.mount.is_current()
            && !self.admission.is_fenced()
            && self.host.window_id() == window.window_handle().window_id()
            && window.is_window_active()
            && !window.has_active_dialog(cx)
            && self.native.is_some_and(|native| {
                current_foreground_identity() == Some(native.identity)
                    && json_utility::native_host_current(
                        &self.visibility,
                        self.visibility.request_revision.load(Ordering::Acquire),
                        Some(native),
                        false,
                    )
            })
            && self.root.upgrade().is_some_and(|root| {
                root.read(cx)
                    .external_owner_is_current(window.focused(cx).as_ref(), window, cx)
            })
    }
    fn copy(
        &mut self,
        line: Option<LineId>,
        basis: EvaluationBasis,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pending_copy
            || !self.current(window, cx)
            || !copy_basis_is_current(self.session.as_ref(), basis)
        {
            return;
        }
        let Some(evaluation) = self.session.as_mut().and_then(|session| session.shared_evaluation()) else {
            return;
        };
        let text = if let Some(id) = line {
            let Some(row) = evaluation.lines.iter().find(|row| row.id == id) else {
                return;
            };
            match &row.outcome {
                LineOutcome::Value(value) => value.canonical_result(),
                _ => None,
            }
        } else {
            Some(calculation_pad::copy_all_results(
                &evaluation.source,
                &evaluation,
            ))
        };
        let Some(text) = text else {
            return;
        };
        let Some(native) = self.native else {
            return;
        };
        let Some(root) = self.root.upgrade() else {
            return;
        };
        let Some(permit) = self.admission.admit() else {
            return;
        };
        let focus = self.editor.focus_handle(cx);
        focus.focus(window, cx);
        let lease = Arc::new(AtomicBool::new(true));
        let allowed = root.update(cx, |root, cx| {
            if !root.external_owner_is_current(Some(&focus), window, cx)
                || window.has_active_dialog(cx)
                || root.clipboard_modify_startup.pending
                || root
                    .clipboard_modify_view
                    .as_ref()
                    .is_some_and(|view| view.read(cx).blocks_close())
            {
                return false;
            }
            if !root
                .external_launch
                .begin_auxiliary_effect(lease.clone(), true)
            {
                return false;
            }
            if let Some(view) = &root.clipboard_modify_view {
                view.update(cx, |view, cx| {
                    view.retire_read_for_effect();
                    cx.notify();
                });
            }
            true
        });
        if !allowed {
            self.message = Some("Another clipboard action is active.".into());
            cx.notify();
            return;
        }
        let blur = lease.clone();
        self.focus_guard = Some(cx.on_blur(&focus, window, move |_, _, _| {
            blur.store(false, Ordering::Release)
        }));
        let active = lease.clone();
        self.active_guard = Some(cx.observe_window_activation(window, move |_, window, _| {
            if !window.is_window_active() {
                active.store(false, Ordering::Release);
            }
        }));
        self.pending_copy = true;
        self.copy_lease = Some(lease.clone());
        let mount = self.mount.clone();
        let worker_mount = mount.clone();
        let visibility = self.visibility.clone();
        let revision = visibility.request_revision.load(Ordering::Acquire);
        let admission = self.admission.clone();
        let worker_lease = lease.clone();
        let task = cx.background_executor().spawn(async move {
            let current = || {
                worker_mount.is_current()
                    && !admission.is_fenced()
                    && worker_lease.load(Ordering::Acquire)
                    && json_utility::native_host_current(&visibility, revision, Some(native), false)
                    && current_foreground_identity() == Some(native.identity)
            };
            super::external_launch::copy_if_current(
                &multi_launcher::clipboard_modify::ArboardClipboardBackend,
                &text,
                &current,
            )
        });
        let weak_root = self.root.clone();
        cx.spawn(async move |weak, cx| {
            let result = task.await;
            let _ = weak_root.update(cx, |root, cx| {
                root.external_launch.finish_auxiliary_effect(&lease);
                if let Some(view) = &root.clipboard_modify_view {
                    view.update(cx, |view, cx| view.launcher_effect_settled(cx));
                }
                if root.launcher_search_error.as_deref()
                    == Some(clipboard_modify_view::LAUNCHER_EFFECT_WAIT)
                {
                    root.launcher_search_error = None;
                }
                cx.notify();
            });
            let _ = weak.update(cx, |view, cx| {
                view.pending_copy = false;
                view.copy_lease = None;
                view.focus_guard = None;
                view.active_guard = None;
                if !view.blocks_close() {
                    let _ = view.root.update(cx, |root, cx| {
                        if root.restart_status.as_deref() == Some(WAIT) {
                            root.restart_status = None;
                            cx.notify();
                        }
                    });
                }
                if mount.is_current()
                    && mount.matches(&view.mount)
                    && view.session.as_ref().and_then(|session| session.evaluation_basis()) == Some(basis)
                    && lease.load(Ordering::Acquire)
                {
                    view.message = Some(match result {
                        Ok(()) => "Copied to clipboard.".into(),
                        Err(error) => error,
                    });
                }
                cx.notify();
            });
            drop(permit);
        })
        .detach();
    }
    fn set_editor_from_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.reset_find_if_document_changed(window, cx);
        let Some(session) = self.session.as_mut() else { return; };
        let source = session.source().unwrap_or("").to_owned();
        let selection = self.editor.read(cx).selected_range();
        let reversed = self.editor.read(cx).is_selection_reversed();
        self.updating = true;
        self.editor
            .update(cx, |editor, cx| {
                editor.set_value(source, window, cx);
                editor.set_selected_range_with_direction(selection, reversed, cx);
            });
        self.updating = false;
        session.advance_automatic();
    }
}
impl Drop for CalculationPadView {
    fn drop(&mut self) {
        self.revoke_copy();
        self.revoke_document();
        self.wake.close();
        self.owner.retire();
    }
}

impl Render for CalculationPadView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(session) = self.session.as_mut() else {
            return div().id("calculation-pad-busy").track_focus(&self.focus).flex().flex_col().gap_2()
                .child(Label::new("Calculation Pad"))
                .child(Textarea::new(&self.editor).readonly(true).aria_label("Calculation Pad source"))
                .child(Label::new("Working with the saved Pad…")).into_any_element();
        };
        session.advance_automatic();
        let basis = session.evaluation_basis();
        let displayed = session.displayed_evaluation();
        let current_evaluation = session.shared_evaluation().is_some();
        let status = match session.evaluation_status() {
            calculation_pad::EvaluationStatus::Ready => "Ready".to_owned(),
            calculation_pad::EvaluationStatus::Pending => "Calculating…".to_owned(),
            calculation_pad::EvaluationStatus::NotCalculated => "Not calculated".to_owned(),
            calculation_pad::EvaluationStatus::Outdated => "Results are out of date".to_owned(),
            calculation_pad::EvaluationStatus::Canceled => "Calculation canceled".to_owned(),
            calculation_pad::EvaluationStatus::Failed(message) => format!("Calculation failed: {message}"),
            calculation_pad::EvaluationStatus::Inactive => "No document open".to_owned(),
        };
        let policy = session.document_options().map(|options| options.calculation);
        let pending = session.pending_transition();
        let document_revision = self.document_revision;
        let name = session.current_name().unwrap_or("Untitled").to_owned();
        let dirty = session.is_dirty();
        let stale = displayed
            .as_ref()
            .is_some_and(|result| Some(result.basis) != basis);
        let mut panel = div()
            .id("calculation-pad")
            .key_context("CalculationPad")
            .track_focus(&self.focus)
            .capture_action(cx.listener(|view, _: &gpui_kit::base::input::Search, window, cx| {
                if view.open_find(false, window, cx) { cx.stop_propagation(); }
            }))
            .capture_action(cx.listener(|view, _: &gpui_kit::base::input::Replace, window, cx| {
                if view.open_find(true, window, cx) { cx.stop_propagation(); }
            }))
            .capture_action(cx.listener(|view, _: &gpui_kit::base::input::Escape, window, cx| {
                if view.close_find(window, cx) { cx.stop_propagation(); }
            }))
            .capture_key_down(cx.listener(|view, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" && event.keystroke.modifiers == Modifiers::default()
                    && !event.is_held && view.close_find(window, cx) {
                    window.prevent_default(); cx.stop_propagation();
                }
            }))
            .capture_action(cx.listener(|view, _: &gpui_kit::base::input::Undo, window, cx| {
                if view.editor.focus_handle(cx).is_focused(window) && view.current(window, cx) && view.session.as_ref().is_some_and(|session| session.pending_transition().is_none()) {
                    view.revoke_copy();
                    if view.session.as_mut().is_some_and(|session| session.undo()) {
                        view.set_editor_from_session(window, cx);
                    }
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .capture_action(cx.listener(|view, _: &gpui_kit::base::input::Redo, window, cx| {
                if view.editor.focus_handle(cx).is_focused(window) && view.current(window, cx) && view.session.as_ref().is_some_and(|session| session.pending_transition().is_none()) {
                    view.revoke_copy();
                    if view.session.as_mut().is_some_and(|session| session.redo()) {
                        view.set_editor_from_session(window, cx);
                    }
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .child(Label::new("Calculation Pad"))
            .child(Label::new(format!("{name}{}", if dirty { " · Unsaved" } else { "" })))
            .child(Textarea::new(&self.editor).readonly(pending.is_some()).aria_label("Calculation Pad source"))
            .child(Label::new(format!("Calculation: {status}")));
        if let Some(find) = self.find_panel(cx) { panel = panel.child(find); }
        let mut controls = div().flex().gap_2();
        for (name, kind) in [
            ("Calculate", 0),
            ("Cancel", 1),
            ("Undo", 2),
            ("Redo", 3),
            (if policy == Some(CalculationPolicy::Automatic) { "Automatic" } else { "Manual" }, 4),
            ("Copy All", 5),
            ("Close", 6),
        ] {
            controls = controls.child(
                Button::new(SharedString::from(format!("calculation-pad-{kind}")))
                    .label(name)
                    .disabled(pending.is_some() || (kind == 5 && (!current_evaluation || self.pending_copy)))
                    .on_click(cx.listener(move |view, _, window, cx| {
                        if !view.current(window, cx) || view.session.as_ref().is_none_or(|session| session.pending_transition().is_some()) {
                            return;
                        }
                        match kind {
                            0 => { view.revoke_copy(); if let Some(session) = view.session.as_mut() { session.request_calculate(); } },
                            1 => {
                                if let Some(basis) = basis {
                                    if view.session.as_ref().is_some_and(|session| session.evaluation_basis() == Some(basis) && session.evaluation_status() == calculation_pad::EvaluationStatus::Pending) {
                                        view.revoke_copy();
                                        if let Some(session) = view.session.as_mut() { session.cancel_calculation(basis); }
                                    }
                                }
                            }
                            2 | 3 => {
                                view.revoke_copy();
                                if if kind == 2 {
                                    view.session.as_mut().is_some_and(|session| session.undo())
                                } else {
                                    view.session.as_mut().is_some_and(|session| session.redo())
                                } {
                                    view.set_editor_from_session(window, cx);
                                }
                            }
                            4 => {
                                view.revoke_copy();
                                if let Some(session) = view.session.as_mut() {
                                  if let Some(mut options) = session.document_options() {
                                    options.calculation =
                                        if options.calculation == CalculationPolicy::Automatic {
                                            CalculationPolicy::Manual
                                        } else {
                                            CalculationPolicy::Automatic
                                        };
                                    let _ = session.set_document_options(options);
                                    session.advance_automatic();
                                  }
                                }
                            }
                            5 => {
                                if let Some(basis) = basis {
                                    view.copy(None, basis, window, cx);
                                }
                            }
                            6 => {
                                view.document_command(DocumentCommand::Close, basis, pending, document_revision, window, cx);
                            }
                            _ => {}
                        }
                        cx.notify();
                    })),
            );
        }
        panel = panel.child(controls);
        let mut documents = div().flex().flex_col().gap_2().child(Label::new("Saved Pads"));
        let mut document_controls = div().flex().gap_2();
        for (label, command) in [("Refresh", DocumentCommand::List), ("New", DocumentCommand::New), ("Save", DocumentCommand::Save)] {
            let receipt = self.mount.clone();
            document_controls = document_controls.child(Button::new(SharedString::from(format!("pad-doc-{label}")))
                .label(label).disabled(pending.is_some()).on_click(cx.listener(move |view, _, window, cx| {
                    if receipt.is_current() { view.document_command(command.clone(), basis, pending, document_revision, window, cx); }
                })));
        }
        let save_receipt = self.mount.clone();
        documents = documents.child(document_controls)
            .child(Input::new(&self.save_name).aria_label("Saved Pad name"))
            .child(Button::new("pad-save-as").label("Save As").on_click(cx.listener(move |view, _, window, cx| {
                if save_receipt.is_current() {
                    let name = view.save_name.read(cx).value().to_string();
                    view.document_command(DocumentCommand::SaveAs(name), basis, pending, document_revision, window, cx);
                }
            })));
        for document in &self.documents {
            let id = document.id;
            let receipt = self.mount.clone();
            documents = documents.child(Button::new(SharedString::from(format!("pad-open-{id}")))
                .label(format!("Open {}", document.name)).disabled(pending.is_some()).on_click(cx.listener(move |view, _, window, cx| {
                    if receipt.is_current() { view.document_command(DocumentCommand::Open(id), basis, pending, document_revision, window, cx); }
                })));
        }
        if pending.is_some() {
            let mut decision = div().flex().gap_2().child(Label::new("Save changes before continuing?"));
            for (label, choice) in [("Save", calculation_pad::TransitionDecision::Save), ("Discard", calculation_pad::TransitionDecision::Discard), ("Cancel", calculation_pad::TransitionDecision::Cancel)] {
                let receipt = self.mount.clone();
                decision = decision.child(Button::new(SharedString::from(format!("pad-decision-{label}"))).label(label)
                    .on_click(cx.listener(move |view, _, window, cx| {
                        if receipt.is_current() { view.document_command(DocumentCommand::Resolve(choice), basis, pending, document_revision, window, cx); }
                    })));
            }
            documents = documents.child(decision);
        }
        panel = panel.child(documents);
        if let Some(message) = &self.message {
            panel = panel.child(Label::new(message.clone()));
        }
        if stale {
            panel = panel.child(Label::new("Results are out of date."));
        }
        if let Some(evaluation) = displayed {
            if let Some(diagnostic) = &evaluation.diagnostic {
                panel = panel.child(Label::new(format!("Document error: {diagnostic}")));
            }
            let mut rows = div()
                .id("calculation-pad-results")
                .flex()
                .flex_col()
                .max_h(px(360.))
                .overflow_y_scroll();
            for row in &evaluation.lines {
                let value = match &row.outcome {
                    LineOutcome::Value(value) => value
                        .formatted_result()
                        .unwrap_or_else(|| "unavailable".into()),
                    LineOutcome::Error(error) => format!("error: {error}"),
                    LineOutcome::Continuation { .. } => "continued".into(),
                    _ => String::new(),
                };
                let id = row.id;
                let row_basis = evaluation.basis;
                rows = rows.child(
                    div()
                        .id(SharedString::from(format!("calculation-pad-result-{}", id.get())))
                        .flex()
                        .gap_2()
                        .child(Label::new(format!("{}  {value}", row.line_index + 1)))
                        .child(
                            Button::new(SharedString::from(format!(
                                "calculation-pad-copy-{}",
                                row.line_index
                            )))
                            .label("Copy Result")
                            .disabled(pending.is_some() || self.pending_copy || !current_evaluation || stale || !matches!(row.outcome, LineOutcome::Value(_)))
                            .on_click(cx.listener(
                                move |view, _, window, cx| {
                                    view.copy(Some(id), row_basis, window, cx)
                                },
                            )),
                        ),
                );
            }
            panel = panel.child(rows);
        }
        panel.into_any_element()
    }
}

impl RootWindow {
    pub(super) fn open_calculation_pad(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = if let Some(view) = &self.calculation_pad_view {
            view.clone()
        } else {
            let root = cx.entity().downgrade();
            let native = self.native_window;
            let visibility = self.root_visibility.clone();
            let admission = self.restart_admission.clone();
            let data = self.app_data_root.clone();
            let view = cx.new(|cx| {
                CalculationPadView::new(window, cx, root, native, visibility, admission, data)
            });
            self.calculation_pad_view = Some(view.clone());
            view
        };
        view.update(cx, |view, cx| {
            if !view.open {
                view.reset_find(window, cx);
                view.mount = view.owner.attach();
                view.open = true;
                if let Some(session) = view.session.as_mut() && session.source().is_none() {
                    let _ = session.request_new();
                    view.set_editor_from_session(window, cx);
                }
            }
            view.editor.focus_handle(cx).focus(window, cx);
            cx.notify();
        });
        let id = window.window_handle().window_id();
        let index = self.clipboard_scroll_item(id, cx)
            + usize::from(self.clipboard_history.as_ref().is_some_and(|view| view.read(cx).is_mounted_in(id, false)))
            + usize::from(self.clipboard_modify_view.as_ref().is_some_and(|view| view.read(cx).is_mounted_in(id, false)))
            + usize::from(self.snippets_view.as_ref().is_some_and(|view| view.read(cx).open))
            + usize::from(self.snippet_fill.as_ref().is_some_and(|view| view.read(cx).open))
            + usize::from(self.tempfile_view.as_ref().is_some_and(|view| view.read(cx).open))
            + usize::from(self.folder_size_view.as_ref().is_some_and(|view| view.read(cx).is_mounted_in(id, false)))
            + usize::from(self.file_lock_view.as_ref().is_some_and(|view| view.read(cx).is_mounted_in(id, false)))
            + usize::from(self.duplicate_finder_view.as_ref().is_some_and(|view| view.read(cx).open))
            + usize::from(self.quick_notes_view.as_ref().is_some_and(|view| view.read(cx).open))
            + usize::from(self.note_editor_view.as_ref().is_some_and(|view| view.read(cx).open))
            + usize::from(self.multi_manager_view.as_ref().is_some_and(|view| view.read(cx).open));
        self.root_scroll.scroll_to_top_of_item(index);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{copy_basis_is_current, current_provider, document_receipt_current};
    use multi_launcher::calculation_pad::CalculationPadSession;
    use multi_launcher::plugin::Plugin;
    use std::collections::HashSet;
    #[test]
    fn stock_pad_opener_obeys_query_capability_and_exact_action() {
        let action = multi_launcher::plugins_builtin::CalculatorPlugin::default()
            .search("calc pad")
            .remove(0);
        assert!(current_provider(" CALC PAD ", &action, None, None));
        assert!(!current_provider("calc list", &action, None, None));
        assert!(!current_provider(
            "calc pad",
            &action,
            Some(&HashSet::new()),
            None
        ));
        let mut forged = action.clone();
        forged.args = Some("unexpected".into());
        assert!(!current_provider("calc pad", &forged, None, None));
    }

    #[test]
    fn document_receipt_rejects_stale_edit_decision_and_loan() {
        let temp = tempfile::tempdir().unwrap();
        let root = multi_launcher::platform::app_data::AppDataRoot::from_settings_path(temp.path().join("settings.json")).unwrap();
        let mut session = CalculationPadSession::new(&root);
        let basis = session.evaluation_basis();
        assert!(document_receipt_current(Some(&session), basis, None, 4, 4));
        assert!(!document_receipt_current(None, basis, None, 4, 4));
        assert!(!document_receipt_current(Some(&session), basis, None, 5, 4));
        session.replace_source("a = 1").unwrap();
        assert!(!document_receipt_current(Some(&session), basis, None, 4, 4));
        let basis = session.evaluation_basis();
        session.request_new().unwrap();
        assert!(!document_receipt_current(Some(&session), basis, None, 4, 4));
    }

    #[test]
    fn copy_receipt_rejects_changed_basis_and_pending_decision() {
        let temp = tempfile::tempdir().unwrap();
        let root = multi_launcher::platform::app_data::AppDataRoot::from_settings_path(temp.path().join("settings.json")).unwrap();
        let mut session = CalculationPadSession::new(&root);
        let initial = session.evaluation_basis().unwrap();
        assert!(copy_basis_is_current(Some(&session), initial));
        session.replace_source("x = 1").unwrap();
        assert!(!copy_basis_is_current(Some(&session), initial));
        let edited = session.evaluation_basis().unwrap();
        session.request_new().unwrap();
        assert!(!copy_basis_is_current(Some(&session), edited));
    }
}
