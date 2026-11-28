//! Retained Calculation Pad editor, backed by the shared session.
use super::*;
use gpui_kit::EntityInputHandler;
#[path = "calculation_pad_find.rs"]
mod find;
#[path = "calculation_pad_format.rs"]
mod format;
use multi_launcher::calculation_pad::{
    self, CalculationPadSession, CalculationPolicy, EvaluationBasis, LineId, LineOutcome,
};
use multi_launcher::plugin::Plugin;

pub(super) const WAIT: &str = "Finish the Calculation Pad operation, or save or discard its changes before closing.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CopyTarget {
    CanonicalAll,
    CanonicalLine(LineId),
    FormattedLine { line: LineId, revision: u64 },
}

fn copy_payload(
    evaluation: &calculation_pad::SessionEvaluation,
    target: CopyTarget,
    format_revision: u64,
    uncertain: bool,
    display: multi_launcher::settings::CalculationPadDisplayPreferences,
) -> Option<String> {
    match target {
        CopyTarget::CanonicalAll => Some(calculation_pad::copy_all_results(
            &evaluation.source,
            evaluation,
        )),
        CopyTarget::CanonicalLine(id) | CopyTarget::FormattedLine { line: id, .. } => {
            if matches!(target, CopyTarget::FormattedLine { revision, .. } if uncertain || revision != format_revision)
            {
                return None;
            }
            let row = evaluation.lines.iter().find(|row| row.id == id)?;
            let LineOutcome::Value(value) = &row.outcome else {
                return None;
            };
            match target {
                CopyTarget::FormattedLine { .. } => {
                    calculation_pad::result_format::display(value, display)
                }
                _ => value.canonical_result(),
            }
        }
    }
}

fn settle_copy_effect(
    root: &mut RootWindow,
    lease: &Arc<AtomicBool>,
    cx: &mut Context<RootWindow>,
) {
    root.external_launch.finish_auxiliary_effect(lease);
    if let Some(view) = &root.clipboard_modify_view {
        view.update(cx, |view, cx| view.launcher_effect_settled(cx));
    }
    if root.launcher_search_error.as_deref() == Some(clipboard_modify_view::LAUNCHER_EFFECT_WAIT) {
        root.launcher_search_error = None;
    }
    cx.notify();
}

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

fn navigation_source_is_current(session: Option<&CalculationPadSession>, basis: EvaluationBasis, editor: &str, composing: bool) -> bool {
    !composing && session.is_some_and(|session| session.pending_transition().is_none()
        && session.evaluation_basis() == Some(basis) && session.source() == Some(editor))
}

fn result_context_current(context: Option<(EvaluationBasis, LineId, u64, u64)>, basis: EvaluationBasis, line: LineId, revision: u64, serial: u64) -> bool {
    context == Some((basis, line, revision, serial))
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
    format: format::FormatOwner,
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
    result_context: Option<(EvaluationBasis, LineId, u64, u64)>,
    result_context_serial: u64,
    result_focus: HashMap<LineId, FocusHandle>,
    result_context_focus: FocusHandle,
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
            format: format::FormatOwner::new(cx),
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
            result_context: None,
            result_context_serial: 0,
            result_focus: HashMap::new(),
            result_context_focus: cx.focus_handle(),
        }
    }
    pub(super) fn blocks_close(&self) -> bool {
        self.open && (self.session.as_ref().is_none_or(|session| session.is_dirty() || session.pending_transition().is_some()) || self.pending_copy || self.format.busy || (self.format.dirty() && !self.format.uncertain))
    }
    pub(super) fn format_requires_relaunch(&self) -> bool { self.format.uncertain }
    fn revoke_copy(&mut self) {
        if let Some(lease) = &self.copy_lease {
            lease.store(false, Ordering::Release);
        }
    }
    pub(super) fn revoke_document(&mut self) {
        if let Some(lease) = &self.document_lease { lease.store(false, Ordering::Release); }
        self.revoke_copy();
        self.format.revoke();
    }
    fn document_command(&mut self, command: DocumentCommand, expected: Option<EvaluationBasis>, expected_pending: Option<calculation_pad::PendingTransition>, revision: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending_copy || self.format.busy || (matches!(command, DocumentCommand::Close) && self.format.dirty() && !self.format.uncertain) || !self.current(window, cx)
            || !document_receipt_current(self.session.as_ref(), expected, expected_pending, self.document_revision, revision) { return; }
        let Some(permit) = self.admission.admit() else { return; };
        if matches!(command, DocumentCommand::New | DocumentCommand::Open(_) | DocumentCommand::Close | DocumentCommand::Resolve(_)) {
            self.reset_find(window, cx);
            self.result_context = None;
            self.result_focus.clear();
        }
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
                    view.result_context = None;
                    view.result_focus.clear();
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
    fn navigation_current(&self, basis: EvaluationBasis, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.current(window, cx) { return false; }
        let composing = self.editor.update(cx, |editor, cx| editor.marked_text_range(window, cx).is_some());
        navigation_source_is_current(self.session.as_ref(), basis, self.editor.read(cx).value().as_str(), composing)
    }
    fn reveal_source(&self, window: &mut Window, cx: &mut Context<Self>) {
        let root = self.root.clone();
        let entity = cx.entity_id();
        let mount = self.mount.clone();
        let host = window.window_handle().window_id();
        let revision = self.visibility.request_revision.load(Ordering::Acquire);
        let Some(focused) = window.focused(cx) else { return; };
        window.defer(cx, move |window, cx| {
            let _ = root.update(cx, |root, cx| {
                if mount.is_current() && window.is_window_active()
                    && focused.is_focused(window)
                    && root.root_visibility.request_revision.load(Ordering::Acquire) == revision
                    && root.native_window.is_some_and(|native| current_foreground_identity() == Some(native.identity))
                    && root.calculation_pad_view.as_ref().is_some_and(|view| {
                        view.entity_id() == entity && view.read(cx).mount.matches(&mount)
                    }) && root.root_window.is_some_and(|window| window.window_id() == host) {
                    root.reveal_calculation_pad(host, cx);
                }
            });
        });
    }

    fn navigate_result(&mut self, basis: EvaluationBasis, line: LineId, mount: &utility_mount::MountReceipt, revision: u64, serial: u64, window: &mut Window, cx: &mut Context<Self>) {
        if !mount.is_current() || !mount.matches(&self.mount)
            || self.visibility.request_revision.load(Ordering::Acquire) != revision
            || !result_context_current(self.result_context, basis, line, revision, serial)
            || !self.navigation_current(basis, window, cx) { return; }
        let Some(session) = self.session.as_ref() else { return; };
        if session.displayed_evaluation().is_none_or(|evaluation| evaluation.basis != basis) { return; }
        let Some(location) = session.resolve_dependency_line(basis.document, basis.incarnation, line) else { return; };
        let offset = location.source_range.start;
        self.editor.update(cx, |editor, cx| editor.set_selected_range(offset..offset, cx));
        self.editor.focus_handle(cx).focus(window, cx);
        self.reveal_source(window, cx);
        self.result_context = None;
        cx.notify();
    }
    fn dismiss_result_context(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some((basis, line, revision, _)) = self.result_context.take() else { return false; };
        let Some(row_focus) = self.result_focus.get(&line).cloned() else { return false; };
        let owned_focus = row_focus.is_focused(window) || self.result_context_focus.is_focused(window);
        if owned_focus && self.visibility.request_revision.load(Ordering::Acquire) == revision
            && self.navigation_current(basis, window, cx)
            && self.session.as_ref().is_some_and(|session| session.resolve_dependency_line(basis.document, basis.incarnation, line).is_some()) {
            row_focus.focus(window, cx);
            cx.notify();
            true
        } else {
            cx.notify();
            false
        }
    }
    fn copy(
        &mut self,
        target: CopyTarget,
        basis: EvaluationBasis,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pending_copy
            || (matches!(target, CopyTarget::FormattedLine { .. })
                && (self.format.busy || self.format.uncertain || self.format.baseline.is_none()))
            || !self.current(window, cx)
            || !copy_basis_is_current(self.session.as_ref(), basis)
        {
            return;
        }
        let Some(evaluation) = self
            .session
            .as_mut()
            .and_then(|session| session.shared_evaluation())
        else {
            return;
        };
        let text = copy_payload(
            &evaluation,
            target,
            self.format.revision,
            self.format.uncertain,
            self.format.display(),
        );
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
        let weak_root = self.root.clone();
        let weak = cx.entity().downgrade();
        window.defer(cx, move |window, app| {
          let fallback_root = weak_root.clone(); let fallback_lease = lease.clone();
          let launched = weak.update(app, |view, cx| {
            if !mount.is_current() || !mount.matches(&view.mount) || !view.pending_copy
                || !lease.load(Ordering::Acquire) || !focus.is_focused(window) || !view.current(window, cx)
                || view.visibility.request_revision.load(Ordering::Acquire) != revision
                || !copy_basis_is_current(view.session.as_ref(), basis)
                || matches!(target, CopyTarget::FormattedLine { revision, .. } if view.format.uncertain || view.format.busy || revision != view.format.revision) {
                view.pending_copy = false; view.copy_lease = None; view.focus_guard = None; view.active_guard = None;
                let _ = weak_root.update(cx, |root, cx| settle_copy_effect(root, &lease, cx));
                view.message = Some("Copy was superseded; try again.".into()); view.settle_format_wait(cx); cx.notify(); return;
            }
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
        cx.spawn(async move |weak, cx| {
            let result = task.await;
            let _ = weak_root.update(cx, |root, cx| settle_copy_effect(root, &lease, cx));
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
                    && !matches!(target, CopyTarget::FormattedLine { revision, .. } if view.format.uncertain || revision != view.format.revision)
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
          });
          if launched.is_err() { let _ = fallback_root.update(app, |root, cx| settle_copy_effect(root, &fallback_lease, cx)); }
        });
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
            return div().id("calculation-pad-busy").key_context("CalculationPad").track_focus(&self.focus)
                .capture_action(cx.listener(|_, _: &CalculationPadGoToLine, _, cx| cx.stop_propagation()))
                .flex().flex_col().gap_2()
                .child(Label::new("Calculation Pad"))
                .child(Textarea::new(&self.editor).readonly(true).aria_label("Calculation Pad source"))
                .child(Label::new("Working with the saved Pad…")).into_any_element();
        };
        session.advance_automatic();
        let basis = session.evaluation_basis();
        let displayed = session.displayed_evaluation();
        let current_result = session.shared_evaluation();
        let current_evaluation = current_result.is_some();
        let values = || current_result.iter().flat_map(|result| result.lines.iter()).filter_map(|row| if let LineOutcome::Value(value) = &row.outcome { Some(value) } else { None });
        let has_precision = values().any(calculation_pad::result_format::supports_precision);
        let has_percent = values().any(calculation_pad::result_format::supports_percent);
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
            .capture_action(cx.listener(|view, _: &CalculationPadGoToLine, window, cx| {
                view.open_goto(window, cx);
                cx.stop_propagation();
            }))
            .capture_action(cx.listener(|view, _: &gpui_kit::base::input::Search, window, cx| {
                if view.open_find(false, window, cx) { cx.stop_propagation(); }
            }))
            .capture_action(cx.listener(|view, _: &gpui_kit::base::input::Replace, window, cx| {
                if view.open_find(true, window, cx) { cx.stop_propagation(); }
            }))
            .capture_action(cx.listener(|view, _: &gpui_kit::base::input::Escape, window, cx| {
                if view.dismiss_result_context(window, cx) || view.close_find(window, cx) { cx.stop_propagation(); }
            }))
            .capture_key_down(cx.listener(|view, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" && event.keystroke.modifiers == Modifiers::default()
                    && !event.is_held {
                    if view.dismiss_result_context(window, cx) || view.close_find(window, cx) {
                        window.prevent_default(); cx.stop_propagation();
                    }
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
            ;
        if let Some(find) = self.find_panel(cx) { panel = panel.child(find); }
        panel = panel.child(Textarea::new(&self.editor).readonly(pending.is_some()).aria_label("Calculation Pad source"))
            .child(Label::new(format!("Calculation: {status}")));
        panel = panel.child(self.format_panel(basis, has_precision, has_percent, pending.is_some(), cx));
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
                Button::new(SharedString::from(if kind == 5 { format!("calculation-pad-{kind}-{:?}", basis) } else { format!("calculation-pad-{kind}") }))
                    .label(name)
                    .disabled(pending.is_some() || (kind == 5 && (!current_evaluation || self.pending_copy)) || (kind == 6 && (self.format.busy || (self.format.dirty() && !self.format.uncertain))))
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
                                    view.copy(CopyTarget::CanonicalAll, basis, window, cx);
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
                .label(label).disabled(pending.is_some() || self.format.busy).on_click(cx.listener(move |view, _, window, cx| {
                    if receipt.is_current() { view.document_command(command.clone(), basis, pending, document_revision, window, cx); }
                })));
        }
        let save_receipt = self.mount.clone();
        documents = documents.child(document_controls)
            .child(Input::new(&self.save_name).aria_label("Saved Pad name").readonly(self.format.busy))
            .child(Button::new("pad-save-as").label("Save As").disabled(self.format.busy || pending.is_some()).on_click(cx.listener(move |view, _, window, cx| {
                if save_receipt.is_current() {
                    let name = view.save_name.read(cx).value().to_string();
                    view.document_command(DocumentCommand::SaveAs(name), basis, pending, document_revision, window, cx);
                }
            })));
        for document in &self.documents {
            let id = document.id;
            let receipt = self.mount.clone();
            documents = documents.child(Button::new(SharedString::from(format!("pad-open-{id}")))
                .label(format!("Open {}", document.name)).disabled(pending.is_some() || self.format.busy).on_click(cx.listener(move |view, _, window, cx| {
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
        if displayed.is_none() { self.result_focus.clear(); self.result_context = None; }
        if let Some(evaluation) = displayed {
            let current_lines = evaluation.lines.iter().map(|row| row.id).collect::<HashSet<_>>();
            self.result_focus.retain(|id, _| current_lines.contains(id));
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
                    LineOutcome::Value(value) => {
                        calculation_pad::result_format::display(value, self.format.display())
                            .unwrap_or_else(|| "unavailable".into())
                    }
                    LineOutcome::Error(error) => format!("error: {error}"),
                    LineOutcome::Continuation { .. } => "continued".into(),
                    _ => String::new(),
                };
                let id = row.id;
                let row_basis = evaluation.basis;
                let row_mount = self.mount.clone();
                let open_mount = row_mount.clone();
                let key_mount = row_mount.clone();
                let row_focus = self.result_focus.entry(id).or_insert_with(|| cx.focus_handle()).clone();
                let context_focus = self.result_context_focus.clone();
                let key_focus = row_focus.clone();
                let row_visibility_revision = self.visibility.request_revision.load(Ordering::Acquire);
                let context_serial = self.result_context.and_then(|(basis, line, revision, serial)|
                    (basis == row_basis && line == id && revision == row_visibility_revision).then_some(serial));
                rows = rows.child(
                    div()
                        .id(SharedString::from(format!(
                            "calculation-pad-result-{:?}-{}",
                            row_basis,
                            id.get()
                        )))
                        .flex()
                        .gap_2()
                        .capture_key_down(cx.listener(move |view, event: &KeyDownEvent, window, cx| {
                            let modifiers = event.keystroke.modifiers;
                            if !event.is_held && event.keystroke.key == "f10" && modifiers.shift
                                && !modifiers.control && !modifiers.alt && !modifiers.platform
                                && key_focus.is_focused(window) {
                                window.prevent_default(); cx.stop_propagation();
                                if key_mount.is_current() && key_mount.matches(&view.mount)
                                    && view.visibility.request_revision.load(Ordering::Acquire) == row_visibility_revision
                                    && view.navigation_current(row_basis, window, cx) {
                                    view.result_context_serial = view.result_context_serial.wrapping_add(1);
                                    view.result_context = Some((row_basis, id, row_visibility_revision, view.result_context_serial));
                                    cx.notify();
                                }
                            }
                        }))
                        .on_mouse_down(MouseButton::Right, cx.listener(move |view, _, window, cx| {
                            if open_mount.is_current() && open_mount.matches(&view.mount)
                                && view.visibility.request_revision.load(Ordering::Acquire) == row_visibility_revision
                                && view.navigation_current(row_basis, window, cx) {
                                view.result_context_serial = view.result_context_serial.wrapping_add(1);
                                view.result_context = Some((row_basis, id, row_visibility_revision, view.result_context_serial));
                                cx.notify();
                            }
                        }))
                        .child(Button::new(SharedString::from(format!("calculation-pad-row-focus-{:?}-{}", row_basis, id.get())))
                            .label(format!("{}  {value}", row.line_index + 1)).tab_stop(true).track_focus(&row_focus))
                        .when_some(context_serial, |row, serial| row.child(
                            Button::new(SharedString::from(format!("calculation-pad-go-source-{:?}-{}", row_basis, id.get())))
                                .label("Go to source")
                                .track_focus(&context_focus)
                                .disabled(pending.is_some() || !current_evaluation || stale)
                                .on_click(cx.listener(move |view, _, window, cx| view.navigate_result(row_basis, id, &row_mount, row_visibility_revision, serial, window, cx)))
                        ))
                        .child(
                            Button::new(SharedString::from(format!(
                                "calculation-pad-copy-{:?}-{}",
                                row_basis,
                                id.get()
                            )))
                            .label("Copy Result")
                            .disabled(
                                pending.is_some()
                                    || self.pending_copy
                                    || !current_evaluation
                                    || stale
                                    || !matches!(row.outcome, LineOutcome::Value(_)),
                            )
                            .on_click(cx.listener(
                                move |view, _, window, cx| {
                                    view.copy(CopyTarget::CanonicalLine(id), row_basis, window, cx)
                                },
                            )),
                        )
                        .child({
                            let format_revision = self.format.revision;
                            Button::new(SharedString::from(format!(
                                "calculation-pad-copy-formatted-{:?}-{}-{}",
                                row_basis,
                                id.get(),
                                format_revision
                            )))
                            .label("Copy Formatted Result")
                            .disabled(
                                pending.is_some()
                                    || self.pending_copy
                                    || self.format.busy
                                    || self.format.uncertain
                                    || self.format.baseline.is_none()
                                    || !current_evaluation
                                    || stale
                                    || !matches!(row.outcome, LineOutcome::Value(_)),
                            )
                            .on_click(cx.listener(
                                move |view, _, window, cx| {
                                    view.copy(
                                        CopyTarget::FormattedLine {
                                            line: id,
                                            revision: format_revision,
                                        },
                                        row_basis,
                                        window,
                                        cx,
                                    )
                                },
                            ))
                        }),
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
        let format_view = view.clone();
        window.defer(cx, move |_, cx| {
            format_view.update(cx, |view, cx| {
                if view.open && view.format.baseline.is_none() {
                    view.load_format(cx);
                }
            });
        });
        self.reveal_calculation_pad(window.window_handle().window_id(), cx);
    }

    fn reveal_calculation_pad(&mut self, id: WindowId, cx: &mut Context<Self>) {
        let index = self.clipboard_scroll_item(id, cx)
            + usize::from(self.clipboard_history.as_ref().is_some_and(|view| view.read(cx).is_mounted_in(id, false)))
            + usize::from(self.clipboard_modify_view.as_ref().is_some_and(|view| view.read(cx).is_mounted_in(id, false)))
            + usize::from(self.snippets_view.as_ref().is_some_and(|view| view.read(cx).is_mounted_in(id, false)))
            + usize::from(self.snippet_fill.as_ref().is_some_and(|view| view.read(cx).open))
            + usize::from(self.tempfile_view.as_ref().is_some_and(|view| view.read(cx).open))
            + usize::from(self.folder_size_view.as_ref().is_some_and(|view| view.read(cx).is_mounted_in(id, false)))
            + usize::from(self.file_lock_view.as_ref().is_some_and(|view| view.read(cx).is_mounted_in(id, false)))
            + usize::from(self.duplicate_finder_view.as_ref().is_some_and(|view| view.read(cx).open))
            + usize::from(self.quick_notes_view.as_ref().is_some_and(|view| view.read(cx).open))
            + usize::from(self.note_editor_view.as_ref().is_some_and(|view| view.read(cx).open))
            + usize::from(self.multi_manager_view.as_ref().is_some_and(|view| view.read(cx).open))
            + usize::from(self.multi_manager_paths.as_ref().is_some_and(|view| view.read(cx).open));
        self.root_scroll.scroll_to_top_of_item(index);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{CopyTarget, copy_basis_is_current, copy_payload, current_provider, document_receipt_current};

    #[test]
    fn source_navigation_rejects_stale_basis_queued_edit_and_composition() {
        let dir = tempfile::tempdir().unwrap();
        let root = multi_launcher::platform::app_data::AppDataRoot::from_settings_path(dir.path().join("settings.json")).unwrap();
        let mut session = multi_launcher::calculation_pad::CalculationPadSession::new(&root);
        session.replace_source("雪\r\nnext\n").unwrap();
        let basis = session.evaluation_basis().unwrap();
        assert!(super::navigation_source_is_current(Some(&session), basis, "雪\r\nnext\n", false));
        assert!(!super::navigation_source_is_current(Some(&session), basis, "雪\r\nchanged\n", false));
        assert!(!super::navigation_source_is_current(Some(&session), basis, "雪\r\nnext\n", true));
        session.replace_source("new").unwrap();
        assert!(!super::navigation_source_is_current(Some(&session), basis, "new", false));
    }

    #[test]
    fn result_navigation_context_rejects_superseded_and_reopened_rows() {
        let dir = tempfile::tempdir().unwrap();
        let root = multi_launcher::platform::app_data::AppDataRoot::from_settings_path(dir.path().join("settings.json")).unwrap();
        let mut session = multi_launcher::calculation_pad::CalculationPadSession::new(&root);
        session.replace_source("1\n2").unwrap();
        let basis = session.evaluation_basis().unwrap();
        let lines = session.line_ids().unwrap();
        let first = lines[0]; let second = lines[1];
        let context = Some((basis, first, 5, 7));
        assert!(super::result_context_current(context, basis, first, 5, 7));
        assert!(!super::result_context_current(context, basis, second, 5, 7));
        assert!(!super::result_context_current(context, basis, first, 5, 8));
        assert!(!super::result_context_current(context, basis, first, 6, 7));
    }
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

    #[test]
    fn copy_target_uses_current_line_and_keeps_canonical_payload() {
        let temp = tempfile::tempdir().unwrap();
        let root = multi_launcher::platform::app_data::AppDataRoot::from_settings_path(temp.path().join("settings.json")).unwrap();
        let mut session = CalculationPadSession::new(&root);
        session.replace_source("1234.567\n2").unwrap();
        let stale = session.line_id(1).unwrap();
        session.replace_source("1234.567").unwrap();
        session.request_calculate();
        let evaluation = session.shared_evaluation().unwrap();
        let line = evaluation.lines[0].id;
        let prefs = multi_launcher::settings::CalculationPadDisplayPreferences { number_mode: multi_launcher::settings::CalculationPadNumberMode::Fixed, decimal_places: 2, group_digits: true, ..Default::default() };
        let canonical = copy_payload(&evaluation, CopyTarget::CanonicalLine(line), 7, false, prefs).unwrap();
        assert_eq!(copy_payload(&evaluation, CopyTarget::FormattedLine { line, revision: 7 }, 7, false, prefs).as_deref(), Some("1,234.57"));
        assert_ne!(canonical, "1,234.57");
        assert_eq!(copy_payload(&evaluation, CopyTarget::CanonicalLine(line), 8, true, prefs), Some(canonical));
        assert!(copy_payload(&evaluation, CopyTarget::FormattedLine { line, revision: 7 }, 8, false, prefs).is_none());
        assert!(copy_payload(&evaluation, CopyTarget::FormattedLine { line, revision: 7 }, 7, true, prefs).is_none());
        assert!(copy_payload(&evaluation, CopyTarget::CanonicalLine(stale), 7, false, prefs).is_none());
    }
}
