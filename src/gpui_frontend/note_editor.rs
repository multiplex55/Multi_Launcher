//! Retained existing-note editor and checked canonical open/save.
use super::*;
use gpui_kit::component::text::{MarkdownExtensions, TextView, TextViewState};
use multi_launcher::plugins::note::{self, CheckedNewNote, Note, QuickNoteDraft, QuickNoteWrite};
use multi_launcher::settings::{NoteSettings, NoteViewMode};

pub(super) const WAIT: &str = "Save or discard the Note Editor draft before closing.";

#[derive(Clone)]
pub(super) enum OpenOrigin {
    Search {
        generation: u64,
        index: usize,
        query: String,
        action: Action,
    },
    Omni {
        generation: u64,
        index: usize,
        query: String,
        action: Action,
    },
    Quick {
        mount: utility_mount::MountReceipt,
        version: u64,
        filter: String,
    },
}

#[derive(Clone)]
enum Identity { Existing, New(String) }

#[derive(Clone)]
struct Destination { note: Note, identity: Identity, resolve_slug: Option<String>, omni: bool }

impl Destination {
    fn existing(note: Note) -> Self { Self { note, identity: Identity::Existing, resolve_slug: None, omni: false } }
    fn resolved(slug: String, result: CheckedNewNote) -> Self {
        match result {
            CheckedNewNote::Existing(note) => Self { note, identity: Identity::Existing, resolve_slug: Some(slug), omni: false },
            CheckedNewNote::Draft(note) => Self { note, identity: Identity::New(slug.clone()), resolve_slug: Some(slug), omni: false },
        }
    }
}

#[derive(Clone)]
enum Decision {
    Close,
    Switch(Destination),
}
enum AfterSave {
    Stay,
    Close,
    Switch(Destination, u64),
}

#[derive(Debug, PartialEq, Eq)]
enum ClosePolicy {
    Close,
    Save,
    Decide,
}
fn close_policy(dirty: bool, save_on_close: bool, confirm_discard: bool, is_new: bool) -> ClosePolicy {
    if save_on_close && (dirty || is_new) {
        ClosePolicy::Save
    } else if !dirty {
        ClosePolicy::Close
    } else if confirm_discard {
        ClosePolicy::Decide
    } else {
        ClosePolicy::Close
    }
}

fn preview_source(text: &str) -> &str {
    const LIMIT: usize = 128 * 1024;
    if text.len() <= LIMIT {
        return text;
    }
    let mut end = LIMIT;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

fn same_note(left: &Note, right: &Note) -> bool {
    left.slug == right.slug && left.path == right.path
}

fn same_source(left: &Note, right: &Note) -> bool {
    same_note(left, right) && left.content == right.content
}

fn same_identity(left: &Identity, left_note: &Note, right: &Identity, right_note: &Note) -> bool {
    match (left, right) {
        (Identity::Existing, Identity::Existing) => same_note(left_note, right_note),
        (Identity::New(a), Identity::New(b)) => a == b,
        _ => false,
    }
}

fn same_destination(left: &Destination, right: &Destination) -> bool {
    same_identity(&left.identity, &left.note, &right.identity, &right.note)
        && same_source(&left.note, &right.note)
}

fn open_attempt_current(epoch: &AtomicU64, attempt: u64) -> bool {
    epoch.load(Ordering::Acquire) == attempt
}

fn decision_receipt_current(epoch: &AtomicU64, revision: u64, mount: &utility_mount::MountReceipt) -> bool {
    epoch.load(Ordering::Acquire) == revision && mount.is_current()
}
fn issued_receipt_current(lease: &AtomicBool, issued_revision: u64, current_revision: u64) -> bool {
    lease.load(Ordering::Acquire) && issued_revision == current_revision
}

pub(super) struct NoteEditor {
    pub(super) open: bool,
    root: WeakEntity<RootWindow>,
    host: AnyWindowHandle,
    native: Option<NativeWindow>,
    visibility: RootVisibility,
    admission: Arc<RestartAdmission>,
    owner: utility_mount::MountOwner,
    mount: utility_mount::MountReceipt,
    focus: FocusHandle,
    body: Entity<TextareaState>,
    preview: Entity<TextViewState>,
    _subscription: Subscription,
    notes_wake: Arc<HashWakeSignal>,
    open_epoch: Arc<AtomicU64>,
    note: Note,
    identity: Identity,
    baseline: String,
    mode: NoteViewMode,
    settings: NoteSettings,
    save_on_close: bool,
    confirm_discard: bool,
    pending: bool,
    decision: Option<Decision>,
    decision_epoch: Arc<AtomicU64>,
    switch_pending: bool,
    switch_lease: Option<Arc<AtomicBool>>,
    focus_guard: Option<Subscription>,
    activation_guard: Option<Subscription>,
    message: Option<String>,
}

impl NoteEditor {
    fn focus_content(&self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.mode, NoteViewMode::Edit | NoteViewMode::Split) {
            self.body.focus_handle(cx).focus(window, cx);
        } else if self.settings.rich_preview_enabled() {
            self.preview
                .read(cx)
                .focus_handle()
                .clone()
                .focus(window, cx);
        } else {
            self.focus.focus(window, cx);
        }
    }
    fn new(
        destination: Destination,
        settings: NoteSettings,
        save_on_close: bool,
        confirm_discard: bool,
        root: WeakEntity<RootWindow>,
        native: Option<NativeWindow>,
        visibility: RootVisibility,
        admission: Arc<RestartAdmission>,
        notes_wake: Arc<HashWakeSignal>,
        open_epoch: Arc<AtomicU64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let Destination { note, identity, .. } = destination;
        let owner = utility_mount::MountOwner::default();
        let mount = owner.attach();
        let body = cx.new(|cx| TextareaState::new(window, cx).auto_grow(8, 24));
        body.update(cx, |body, cx| {
            body.set_value(note.content.clone(), window, cx)
        });
        let preview = cx.new(|cx| TextViewState::markdown(preview_source(&note.content), cx));
        let subscription = cx.subscribe(&body, |view: &mut Self, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                let content = view.body.read(cx).value().to_string();
                view.preview.update(cx, |preview, cx| {
                    preview.set_text(preview_source(&content), cx)
                });
                cx.notify();
            }
        });
        let baseline = note.content.clone();
        let mode = settings.effective_default_view_mode();
        Self {
            open: true,
            root,
            host: window.window_handle(),
            native,
            visibility,
            admission,
            owner,
            mount,
            focus: cx.focus_handle(),
            body,
            preview,
            _subscription: subscription,
            notes_wake,
            open_epoch,
            note,
            identity,
            baseline,
            mode,
            settings,
            save_on_close,
            confirm_discard,
            pending: false,
            decision: None,
            decision_epoch: Arc::new(AtomicU64::new(0)),
            switch_pending: false,
            switch_lease: None,
            focus_guard: None,
            activation_guard: None,
            message: None,
        }
    }
    fn dirty(&self, cx: &App) -> bool {
        self.body.read(cx).value().as_str() != self.baseline
    }
    fn set_decision(&mut self, decision: Option<Decision>) -> u64 {
        let revision = self.decision_epoch.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
        self.decision = decision;
        revision
    }
    pub(super) fn revoke_switch(&mut self, cx: &mut Context<Self>) {
        if !matches!(&self.decision, Some(Decision::Switch(_))) && !self.switch_pending { return; }
        self.set_decision(None);
        if self.switch_pending {
            if let Some(lease) = self.switch_lease.take() { lease.store(false, Ordering::Release); }
            self.switch_pending = false;
            self.pending = false;
            self.focus_guard = None;
            self.activation_guard = None;
            self.body.update(cx, |body, cx| body.set_readonly(false, cx));
        }
        cx.notify();
    }
    pub(super) fn revoke_omni_switch(&mut self, cx: &mut Context<Self>) {
        if matches!(&self.decision, Some(Decision::Switch(target)) if target.omni) { self.revoke_switch(cx); }
    }
    pub(super) fn blocks_close(&self, cx: &App) -> bool {
        self.open
            && self.mount.is_current()
            && (self.pending || self.decision.is_some() || self.dirty(cx) || (self.save_on_close && matches!(&self.identity, Identity::New(_))))
    }
    pub(super) fn handle_escape(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        held: bool,
    ) -> bool {
        if !self.focus.contains_focused(window, cx) || !self.current(window, cx) {
            return false;
        }
        if !held {
            if self.decision.is_some() && !self.pending {
                self.set_decision(None);
                cx.notify();
            } else {
                self.request_close(window, cx);
            }
        }
        true
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
    fn install(&mut self, destination: Destination, window: &mut Window, cx: &mut Context<Self>) {
        let Destination { note, identity, .. } = destination;
        self.baseline = note.content.clone();
        self.note = note;
        self.identity = identity;
        self.body.update(cx, |body, cx| {
            body.set_value(self.baseline.clone(), window, cx)
        });
        self.preview.update(cx, |preview, cx| {
            preview.set_text(preview_source(&self.baseline), cx)
        });
        self.set_decision(None);
        self.message = None;
        cx.notify();
    }
    fn verify_switch(&mut self, target: Destination, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending || !self.current(window, cx) {
            return;
        }
        let Some(permit) = self.admission.admit() else { return; };
        self.pending = true;
        self.switch_pending = true;
        self.body.update(cx, |body, cx| body.set_readonly(true, cx));
        let decision_revision = self.set_decision(Some(Decision::Switch(target.clone())));
        self.focus.focus(window, cx);
        let lease = Arc::new(AtomicBool::new(true));
        self.switch_lease = Some(lease.clone());
        let blur = lease.clone();
        self.focus_guard = Some(cx.on_focus_out(&self.focus, window, move |_, _, _, _| { blur.store(false, Ordering::Release); }));
        let active = lease.clone();
        self.activation_guard = Some(cx.observe_window_activation(window, move |_, window, _| { if !window.is_window_active() { active.store(false, Ordering::Release); } }));
        let completion_lease = lease.clone();
        let receipt = self.mount.clone();
        let worker_receipt = receipt.clone();
        let decision_epoch = self.decision_epoch.clone();
        let worker_decision = decision_epoch.clone();
        let native = self.native;
        let visibility = self.visibility.clone();
        let revision = visibility.request_revision.load(Ordering::Acquire);
        let admission = self.admission.clone();
        let expected = target.clone();
        let host = self.host;
        let task = cx.background_executor().spawn(async move {
            let current = || {
                decision_receipt_current(&worker_decision, decision_revision, &worker_receipt)
                    && lease.load(Ordering::Acquire)
                    && !admission.is_fenced()
                    && json_utility::native_host_current(&visibility, revision, native, false)
                    && native.is_some_and(|native| {
                        current_foreground_identity() == Some(native.identity)
                    })
            };
            let result = if let Some(slug) = &expected.resolve_slug {
                note::resolve_plain_new_note_if(slug, current).map(|resolved| Some(Destination::resolved(slug.clone(), resolved)))
            } else {
                note::read_note_exact_if(&expected.note.slug, Some(&expected.note.path), current).map(|note| note.map(Destination::existing))
            };
            (permit, result)
        });
        cx.spawn(async move |weak, cx| {
            let (_permit, result) = task.await;
            let delivered = host.update(cx, |_, window, cx| {
                let _ = weak.update(cx, |view, cx| {
                    if !decision_receipt_current(&decision_epoch, decision_revision, &receipt) || !receipt.matches(&view.mount) || !view.switch_pending {
                        return;
                    }
                    view.pending = false;
                    view.switch_pending = false;
                    view.switch_lease = None;
                    view.focus_guard = None;
                    view.activation_guard = None;
                    view.body.update(cx, |body, cx| body.set_readonly(false, cx));
                    match result {
                        Ok(Some(destination))
                            if issued_receipt_current(&completion_lease, revision, view.visibility.request_revision.load(Ordering::Acquire)) && view.focus.is_focused(window)
                                && json_utility::native_host_current(&view.visibility, revision, view.native, false)
                                && view.current(window, cx)
                                && same_destination(&destination, &target) =>
                        {
                            view.install(destination, window, cx);
                            view.focus_content(window, cx);
                        }
                        _ => {
                            view.message = Some(
                                "The requested note changed or is unavailable; open it again."
                                    .into(),
                            )
                        }
                    }
                    cx.notify();
                });
            });
            if delivered.is_err() {
                let _ = weak.update(cx, |view, cx| {
                    if decision_receipt_current(&decision_epoch, decision_revision, &receipt) && receipt.matches(&view.mount) && view.switch_pending {
                        view.pending = false;
                        view.switch_pending = false;
                        view.switch_lease = None;
                        view.focus_guard = None;
                        view.activation_guard = None;
                        view.body.update(cx, |body, cx| body.set_readonly(false, cx));
                        view.message =
                            Some("The requested note could not be opened; try again.".into());
                        cx.notify();
                    }
                });
            }
        })
        .detach();
        cx.notify();
    }
    fn open_note(&mut self, destination: Destination, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending {
            self.message = Some(WAIT.into());
            cx.notify();
            return;
        }
        if same_identity(&self.identity, &self.note, &destination.identity, &destination.note) {
            self.revoke_switch(cx);
            self.open = true;
            self.focus_content(window, cx);
            return;
        }
        if self.dirty(cx) {
            self.set_decision(Some(Decision::Switch(destination)));
        } else {
            self.install(destination, window, cx);
            self.focus_content(window, cx);
        }
        cx.notify();
    }
    fn request_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.current(window, cx) || self.pending {
            return;
        }
        match close_policy(self.dirty(cx), self.save_on_close, self.confirm_discard, matches!(&self.identity, Identity::New(_))) {
            ClosePolicy::Close => self.retire(window, cx),
            ClosePolicy::Save => self.save(AfterSave::Close, window, cx),
            ClosePolicy::Decide => {
                self.set_decision(Some(Decision::Close));
                cx.notify();
            }
        }
    }
    fn retire(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let owns = self.focus.contains_focused(window, cx);
        let focus = self.focus.clone();
        let root = self.root.clone();
        let entity = cx.entity_id();
        let native = self.native;
        let revision = self.visibility.request_revision.load(Ordering::Acquire);
        self.open = false;
        self.open_epoch.fetch_add(1, Ordering::AcqRel);
        self.owner.retire();
        self.set_decision(None);
        self.baseline = self.note.content.clone();
        self.body.update(cx, |body, cx| {
            body.set_value(self.baseline.clone(), window, cx)
        });
        if owns {
            window.defer(cx, move |window, app| {
                let _ =
                    root.update(app, |root, cx| {
                        let current =
                            root.note_editor_view.as_ref().is_some_and(|view| {
                                view.entity_id() == entity && !view.read(cx).open
                            }) && root.root_window.is_some_and(|host| {
                                host.window_id() == window.window_handle().window_id()
                            }) && native.is_some_and(|native| {
                                root.native_window == Some(native)
                                    && current_foreground_identity() == Some(native.identity)
                                    && json_utility::native_host_current(
                                        &root.root_visibility,
                                        revision,
                                        Some(native),
                                        false,
                                    )
                            }) && window.is_window_active()
                                && !window.has_active_dialog(cx)
                                && root.action_sheet.is_none()
                                && root.context_favorite_popup.is_none()
                                && !context_stack_has_primary(&window.context_stack(), "PopupMenu")
                                && window
                                    .focused(cx)
                                    .is_none_or(|current| focus.contains(&current, window));
                        if current {
                            root.query.focus_handle(cx).focus(window, cx);
                            root.root_scroll
                                .scroll_to_top_of_item(ROOT_SCROLL_SEARCH_ITEM);
                        }
                        cx.notify();
                    });
            });
        }
        cx.notify();
    }
    fn save(&mut self, after: AfterSave, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending || !self.current(window, cx) {
            return;
        }
        let Some(permit) = self.admission.admit() else {
            return;
        };
        self.focus_content(window, cx);
        let issuing_focus = if matches!(self.mode, NoteViewMode::Edit | NoteViewMode::Split) {
            self.body.focus_handle(cx)
        } else if self.settings.rich_preview_enabled() {
            self.preview.read(cx).focus_handle().clone()
        } else {
            self.focus.clone()
        };
        self.pending = true;
        self.body.update(cx, |body, cx| body.set_readonly(true, cx));
        let lease = Arc::new(AtomicBool::new(true));
        let blur = lease.clone();
        self.focus_guard = Some(cx.on_focus_out(&self.focus, window, move |_, _, _, _| {
            blur.store(false, Ordering::Release)
        }));
        let active = lease.clone();
        self.activation_guard = Some(cx.observe_window_activation(window, move |_, window, _| {
            if !window.is_window_active() {
                active.store(false, Ordering::Release);
            }
        }));
        let completion_lease = lease.clone();
        let original = self.note.clone();
        let is_new = matches!(&self.identity, Identity::New(_));
        let content = self.body.read(cx).value().to_string();
        let saved_input = content.clone();
        let receipt = self.mount.clone();
        let worker_receipt = receipt.clone();
        let admission = self.admission.clone();
        let visibility = self.visibility.clone();
        let revision = visibility.request_revision.load(Ordering::Acquire);
        let native = self.native;
        let task = cx.background_executor().spawn(async move {
            let current = || {
                    worker_receipt.is_current()
                        && !admission.is_fenced()
                        && lease.load(Ordering::Acquire)
                        && json_utility::native_host_current(&visibility, revision, native, false)
                        && native.is_some_and(|native| {
                            current_foreground_identity() == Some(native.identity)
                        })
                };
            let result = if is_new {
                note::write_plain_new_note_if(&original, &content, current)
            } else {
                note::write_quick_note_if(QuickNoteDraft::Editor { original: &original, content: &content }, current)
            };
            (permit, result)
        });
        let host = self.host;
        cx.spawn(async move |weak, cx| {
            let (permit, result) = task.await;
            let accepted = weak
                .update(cx, |view, cx| {
                    if !receipt.is_current() || !receipt.matches(&view.mount) {
                        return false;
                    }
                    view.pending = false;
                    view.focus_guard = None;
                    view.activation_guard = None;
                    view.body
                        .update(cx, |body, cx| body.set_readonly(false, cx));
                    let note = match result {
                        Ok(QuickNoteWrite::Published(note)) => {
                            view.message = None;
                            view.notes_wake.notify();
                            Some(note)
                        }
                        Ok(QuickNoteWrite::Committed { note, cache_error }) => {
                            view.message = Some(format!(
                                "Saved, but the notes list could not refresh: {cache_error:#}"
                            ));
                            Some(note)
                        }
                        Err(error) => {
                            view.message = Some(format!("Could not save note: {error:#}"));
                            None
                        }
                    };
                    if let Some(note) = note {
                        view.baseline = saved_input;
                        view.note = note;
                        view.identity = Identity::Existing;
                        cx.notify();
                        true
                    } else {
                        cx.notify();
                        false
                    }
                })
                .unwrap_or(false);
            if accepted {
                let _ = host.update(cx, |_, window, cx| {
                    let _ = weak.update(cx, |view, cx| {
                        if !receipt.is_current()
                            || !receipt.matches(&view.mount)
                            || !issued_receipt_current(&completion_lease, revision, view.visibility.request_revision.load(Ordering::Acquire))
                            || !json_utility::native_host_current(&view.visibility, revision, view.native, false)
                            || !view.current(window, cx)
                            || !issuing_focus.is_focused(window)
                        {
                            return;
                        }
                        match after {
                            AfterSave::Stay => view.focus_content(window, cx),
                            AfterSave::Close => view.retire(window, cx),
                            AfterSave::Switch(note, decision_revision) => {
                                if decision_receipt_current(&view.decision_epoch, decision_revision, &receipt) { view.verify_switch(note, window, cx); }
                            }
                        }
                        cx.notify();
                    });
                });
            }
            drop(permit);
        })
        .detach();
        cx.notify();
    }
}

impl Render for NoteEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut panel = div()
            .id("note-editor-pane")
            .key_context("NoteEditorPane")
            .track_focus(&self.focus)
            .flex()
            .flex_col()
            .gap_2()
            .w_full()
            .p_3()
            .child(Label::new(format!("Note: {}", self.note.title)))
            .capture_action(
                cx.listener(|view, _: &gpui_kit::base::input::Escape, window, cx| {
                    if view.focus.contains_focused(window, cx) {
                        let root = view.root.clone();
                        let latched = root
                            .update(cx, |root, _| {
                                let was = root.pane_escape_latched;
                                root.pane_escape_latched = true;
                                was
                            })
                            .unwrap_or(true);
                        if !latched {
                            view.handle_escape(window, cx, false);
                        }
                        cx.stop_propagation();
                    }
                }),
            );
        let mut modes = div().flex().gap_2();
        for (label, mode) in [
            ("Edit", NoteViewMode::Edit),
            ("Render", NoteViewMode::Preview),
            ("Split", NoteViewMode::Split),
        ] {
            if mode == NoteViewMode::Split && !self.settings.can_use_split() {
                continue;
            }
            modes = modes.child(
                Button::new(SharedString::from(format!("note-mode-{label}")))
                    .label(label)
                    .on_click(cx.listener(move |view, _, window, cx| {
                        if view.current(window, cx) && !view.pending && view.decision.is_none() {
                            view.mode = mode;
                            view.focus_content(window, cx);
                            cx.notify();
                        }
                    })),
            );
        }
        panel = panel.child(modes);
        let mut content = div().flex().gap_2().w_full();
        if matches!(self.mode, NoteViewMode::Edit | NoteViewMode::Split) {
            content = content.child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(Textarea::new(&self.body).readonly(self.pending).aria_label("Note Markdown")),
            );
        }
        if matches!(self.mode, NoteViewMode::Preview | NoteViewMode::Split) {
            if self.settings.rich_preview_enabled() {
                let text = TextView::new(&self.preview)
                    .selectable(true)
                    .markdown_extensions(MarkdownExtensions::default().raw_html(false))
                    .on_link_click(|_, _, _, _| {})
                    .image_source(|_| gpui_kit::ImageSource::Custom(Arc::new(|_, _| None)));
                content = content.child(
                    div()
                        .id("note-render-scroll")
                        .flex_1()
                        .min_w(px(0.0))
                        .h(px(400.0))
                        .overflow_y_scroll()
                        .child(text),
                );
            } else {
                content = content.child(
                    div()
                        .id("note-plain-scroll")
                        .flex_1()
                        .min_w(px(0.0))
                        .h(px(400.0))
                        .overflow_y_scroll()
                        .child(Label::new(
                            preview_source(self.body.read(cx).value().as_str()).to_owned(),
                        )),
                );
            }
        }
        panel = panel.child(content);
        if self.body.read(cx).value().len() > 128 * 1024 {
            panel = panel.child(Label::new("Preview shows the beginning of this note."));
        }
        if let Some(message) = &self.message {
            panel = panel.child(Label::new(message.clone()));
        }
        if self.decision.is_some() {
            let decision_revision = self.decision_epoch.load(Ordering::Acquire);
            let save_receipt = self.mount.clone();
            let discard_receipt = self.mount.clone();
            let keep_receipt = self.mount.clone();
            panel = panel
                .child(Label::new("Save changes before continuing?"))
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            Button::new("note-decision-save")
                                .label("Save")
                                .disabled(self.pending)
                                .on_click(cx.listener(move |view, _, window, cx| {
                                    if !decision_receipt_current(&view.decision_epoch, decision_revision, &save_receipt) || !save_receipt.matches(&view.mount) || !view.current(window, cx) {
                                        return;
                                    }
                                    let after = match view.decision.clone() {
                                        Some(Decision::Close) => AfterSave::Close,
                                        Some(Decision::Switch(note)) => AfterSave::Switch(note, decision_revision),
                                        None => return,
                                    };
                                    view.save(after, window, cx);
                                })),
                        )
                        .child(
                            Button::new("note-decision-discard")
                                .label("Discard")
                                .disabled(self.pending)
                                .on_click(cx.listener(move |view, _, window, cx| {
                                    if !decision_receipt_current(&view.decision_epoch, decision_revision, &discard_receipt) || !discard_receipt.matches(&view.mount) || !view.current(window, cx) || view.pending {
                                        return;
                                    }
                                    match view.decision.clone() {
                                        Some(Decision::Close) => view.retire(window, cx),
                                        Some(Decision::Switch(note)) => {
                                            view.verify_switch(note, window, cx)
                                        }
                                        None => {}
                                    }
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("note-decision-keep")
                                .label("Keep Editing")
                                .disabled(self.pending)
                                .on_click(cx.listener(move |view, _, window, cx| {
                                    if decision_receipt_current(&view.decision_epoch, decision_revision, &keep_receipt) && keep_receipt.matches(&view.mount) && view.current(window, cx) && !view.pending {
                                        view.set_decision(None);
                                        view.focus.focus(window, cx);
                                        cx.notify();
                                    }
                                })),
                        ),
                );
        } else {
            panel = panel.child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("note-save")
                            .label("Save")
                            .disabled(self.pending)
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.save(AfterSave::Stay, window, cx)
                            })),
                    )
                    .child(
                        Button::new("note-close")
                            .label("Close")
                            .disabled(self.pending)
                            .on_click(
                                cx.listener(|view, _, window, cx| view.request_close(window, cx)),
                            ),
                    ),
            );
        }
        panel
    }
}

impl RootWindow {
    fn mount_note_destination(&mut self, destination: Destination, native: NativeWindow, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(view) = self.note_editor_view.clone() {
            view.update(cx, |view, cx| {
                if !view.open {
                    view.open = true;
                    view.mount = view.owner.attach();
                    view.install(destination, window, cx);
                    view.focus_content(window, cx);
                } else { view.open_note(destination, window, cx); }
            });
        } else {
            let owner = cx.entity().downgrade();
            let settings = self.search_catalog.note_settings.clone();
            let save = self.search_catalog.note_save_on_close;
            let confirm = self.search_catalog.note_confirm_discard;
            let visibility = self.root_visibility.clone();
            let admission = self.restart_admission.clone();
            let notes_wake = self.search_catalog.providers.notes_wake.clone();
            let open_epoch = self.note_open_epoch.clone();
            let view = cx.new(|cx| NoteEditor::new(destination, settings, save, confirm, owner, Some(native), visibility, admission, notes_wake, open_epoch, window, cx));
            view.update(cx, |view, cx| view.focus_content(window, cx));
            self.note_editor_view = Some(view);
        }
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
            + usize::from(self.quick_notes_view.as_ref().is_some_and(|view| view.read(cx).open));
        self.root_scroll.scroll_to_top_of_item(index);
    }

    fn note_origin_enabled(&self, origin: &OpenOrigin, expected: &Note, version: u64) -> bool {
        match origin {
            OpenOrigin::Omni { query, action, .. } => {
                super::omni_search::notes_enabled(
                    self.enabled_plugins.as_ref(),
                    self.enabled_capabilities.as_ref(),
                    self.search_catalog.providers.omni.settings().include_notes,
                ) && self
                    .search_catalog
                    .providers
                    .omni
                    .projection(query)
                    .is_some_and(|projection| {
                        projection.source_for(action)
                            == Some(multi_launcher::plugins::omni_search::OmniSource::Note)
                            && projection.note_for(action).is_some_and(|(current, note)| {
                                current == version && note.as_ref() == expected
                            })
                    })
            }
            OpenOrigin::Search { .. } | OpenOrigin::Quick { .. } => {
                self.enabled_plugins
                    .as_ref()
                    .is_none_or(|enabled| enabled.contains("notes"))
                    && multi_launcher::settings::provider_search_capability_enabled(
                        self.enabled_capabilities.as_ref(),
                        "notes",
                    )
            }
        }
    }
    pub(super) fn open_checked_new_note(&mut self, version: u64, generation: u64, index: usize, query: String, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        let Ok(Command::Note(multi_launcher::commands::NoteCommand::New { slug, template: None })) = parse_action(&action) else { return; };
        let effective = multi_launcher::common::query::split_action_filters(&query).0;
        if !action.action.starts_with("note:new:") || action.args.is_some()
            || self.enabled_plugins.as_ref().is_some_and(|enabled| !enabled.contains("notes"))
            || !multi_launcher::settings::provider_search_capability_enabled(self.enabled_capabilities.as_ref(), "notes")
            || self.search_catalog.providers.note_new_source(&effective, &action) != Some(version)
            || self.restart_admission.is_fenced() || !window.is_window_active() || window.has_active_dialog(cx)
            || !self.external_owner_is_current(window.focused(cx).as_ref(), window, cx) { return; }
        let Some(native) = self.native_window.filter(|native| current_foreground_identity() == Some(native.identity)) else { return; };
        let Some(focus) = window.focused(cx) else { return; };
        let Some(permit) = self.restart_admission.admit() else { return; };
        if let Some(view) = &self.note_editor_view { view.update(cx, |view, cx| view.revoke_switch(cx)); }
        let revision = self.root_visibility.request_revision.load(Ordering::Acquire);
        let lease = Arc::new(AtomicBool::new(true));
        let blur = lease.clone();
        let focus_guard = cx.on_blur(&focus, window, move |_, _, _| { blur.store(false, Ordering::Release); });
        let active = lease.clone();
        let activation_guard = cx.observe_window_activation(window, move |_, window, _| { if !window.is_window_active() { active.store(false, Ordering::Release); } });
        let open_epoch = self.note_open_epoch.clone();
        let attempt = open_epoch.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
        let worker_epoch = open_epoch.clone(); let worker_lease = lease.clone();
        let visibility = self.root_visibility.clone(); let admission = self.restart_admission.clone();
        let worker_slug = slug.clone(); let host = window.window_handle();
        let task = cx.background_executor().spawn(async move {
            let result = note::resolve_plain_new_note_if(&worker_slug, || open_attempt_current(&worker_epoch, attempt)
                && worker_lease.load(Ordering::Acquire) && note::note_version() == version && !admission.is_fenced()
                && json_utility::native_host_current(&visibility, revision, Some(native), false)
                && current_foreground_identity() == Some(native.identity));
            (permit, result)
        });
        let root = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let (_permit, result) = task.await;
            let _guards = (focus_guard, activation_guard);
            let _ = host.update(cx, |_, window, app| { let _ = root.update(app, |root, cx| {
                let effective = multi_launcher::common::query::split_action_filters(&query).0;
                if !open_attempt_current(&open_epoch, attempt) || !lease.load(Ordering::Acquire)
                    || !focus.is_focused(window) || !window.is_window_active() || window.has_active_dialog(cx)
                    || root.restart_admission.is_fenced() || root.native_window != Some(native)
                    || current_foreground_identity() != Some(native.identity)
                    || !json_utility::native_host_current(&root.root_visibility, revision, Some(native), false)
                    || !root.external_owner_is_current(window.focused(cx).as_ref(), window, cx)
                    || root.enabled_plugins.as_ref().is_some_and(|enabled| !enabled.contains("notes"))
                    || !multi_launcher::settings::provider_search_capability_enabled(root.enabled_capabilities.as_ref(), "notes")
                    || root.query.read(cx).value().as_str() != query
                    || root.search_catalog.providers.note_new_source(&effective, &action) != Some(version)
                    || root.launcher_results.resolve_result(generation, index, &query).is_none_or(|row| row.action != action || row.origin != LauncherResultOrigin::NoteNew { version }) { return; }
                match result {
                    Ok(resolved) => { root.launcher_search_error = None; root.mount_note_destination(Destination::resolved(slug, resolved), native, window, cx); }
                    Err(error) => root.launcher_search_error = Some(format!("Could not open note: {error:#}")),
                }
                cx.notify();
            }); });
        }).detach();
    }
    pub(super) fn open_checked_note(
        &mut self,
        expected: Note,
        version: u64,
        origin: OpenOrigin,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.restart_admission.is_fenced()
            || !window.is_window_active()
            || window.has_active_dialog(cx)
            || !self.external_owner_is_current(window.focused(cx).as_ref(), window, cx)
            || !self.note_origin_enabled(&origin, &expected, version)
        {
            return;
        }
        let Some(native) = self
            .native_window
            .filter(|native| current_foreground_identity() == Some(native.identity))
        else {
            return;
        };
        let revision = self
            .root_visibility
            .request_revision
            .load(Ordering::Acquire);
        let Some(focus) = window.focused(cx) else {
            return;
        };
        if let Some(view) = &self.note_editor_view { view.update(cx, |view, cx| view.revoke_switch(cx)); }
        let open_epoch = self.note_open_epoch.clone();
        let attempt = open_epoch.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
        let worker_epoch = open_epoch.clone();
        let visibility = self.root_visibility.clone();
        let admission = self.restart_admission.clone();
        let expected_worker = expected.clone();
        let host = window.window_handle();
        let task = cx.background_executor().spawn(async move {
            note::read_note_exact_if(&expected_worker.slug, Some(&expected_worker.path), || {
                open_attempt_current(&worker_epoch, attempt)
                    && note::note_version() == version
                    && !admission.is_fenced()
                    && json_utility::native_host_current(&visibility, revision, Some(native), false)
                    && current_foreground_identity() == Some(native.identity)
            })
        });
        let root = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let result = task.await;
            let _ = host.update(cx, |_, window, cx| {
                let _ = root.update(cx, |root, cx| {
                    if !open_attempt_current(&open_epoch, attempt)
                        || !focus.is_focused(window)
                        || !window.is_window_active()
                        || window.has_active_dialog(cx)
                        || root.restart_admission.is_fenced()
                        || root.native_window != Some(native)
                        || current_foreground_identity() != Some(native.identity)
                        || !json_utility::native_host_current(
                            &root.root_visibility,
                            revision,
                            Some(native),
                            false,
                        )
                        || !root.external_owner_is_current(window.focused(cx).as_ref(), window, cx)
                        || !root.note_origin_enabled(&origin, &expected, version)
                    {
                        return;
                    }
                    let origin_current = match &origin {
                        OpenOrigin::Omni {
                            generation,
                            index,
                            query,
                            action,
                        } => {
                            root.launcher_results
                                .resolve_result(*generation, *index, query)
                                .is_some_and(|row| {
                                    row.action == *action
                                        && row.origin
                                            == LauncherResultOrigin::OmniNote {
                                                version,
                                                note: Arc::new(expected.clone()),
                                            }
                                })
                                && root.query.read(cx).value().as_str() == query
                        }
                        OpenOrigin::Search {
                            generation,
                            index,
                            query,
                            action,
                        } => {
                            root.launcher_results
                                .resolve_result(*generation, *index, query)
                                .is_some_and(|row| {
                                    row.action == *action
                                        && row.origin
                                            == LauncherResultOrigin::Note {
                                                version,
                                                note: Arc::new(expected.clone()),
                                            }
                                })
                                && root.query.read(cx).value().as_str() == query
                        }
                        OpenOrigin::Quick {
                            mount,
                            version,
                            filter,
                        } => root.quick_notes_view.as_ref().is_some_and(|view| {
                            view.read(cx)
                                .open_receipt_current(mount, *version, filter, &expected, cx)
                        }),
                    };
                    if !origin_current {
                        return;
                    }
                    if note::note_version() != version {
                        root.launcher_search_error = Some(
                            "That note changed or is unavailable. Refresh and try again.".into(),
                        );
                        cx.notify();
                        return;
                    }
                    let note = match result {
                        Ok(Some(note)) if same_source(&note, &expected) => note,
                        _ => {
                            root.launcher_search_error = Some(
                                "That note changed or is unavailable. Refresh and try again."
                                    .into(),
                            );
                            cx.notify();
                            return;
                        }
                    };
                    let mut destination = Destination::existing(note);
                    destination.omni = matches!(origin, OpenOrigin::Omni { .. });
                    root.mount_note_destination(destination, native, window, cx);
                    cx.notify();
                });
            });
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ClosePolicy, Destination, Identity, close_policy, decision_receipt_current, issued_receipt_current, open_attempt_current, preview_source, same_destination, same_identity, same_note, same_source,
    };
    use multi_launcher::plugins::note::Note;

    fn note(slug: &str, path: &str) -> Note {
        Note {
            title: "Title".into(),
            path: path.into(),
            content: "# Title".into(),
            tags: vec![],
            links: vec![],
            slug: slug.into(),
            alias: None,
            aliases: vec![],
            entity_refs: vec![],
        }
    }

    #[test]
    fn editor_identity_and_preview_boundaries_preserve_unicode() {
        let original = note("alpha", "alpha.md");
        assert!(same_note(&original, &note("alpha", "alpha.md")));
        assert!(!same_note(&original, &note("alpha", "other.md")));
        assert!(!same_note(&original, &note("beta", "alpha.md")));
        let text = format!("{}é後", "a".repeat(128 * 1024 - 1));
        let preview = preview_source(&text);
        assert_eq!(preview.len(), 128 * 1024 - 1);
        assert!(text.starts_with(preview));
    }

    #[test]
    fn editor_close_policy_honors_save_and_discard_preferences() {
        assert_eq!(close_policy(false, true, true, false), ClosePolicy::Close);
        assert_eq!(close_policy(false, true, true, true), ClosePolicy::Save);
        assert_eq!(close_policy(false, false, true, true), ClosePolicy::Close);
        assert_eq!(close_policy(true, true, true, false), ClosePolicy::Save);
        assert_eq!(close_policy(true, false, true, false), ClosePolicy::Decide);
        assert_eq!(close_policy(true, false, false, false), ClosePolicy::Close);
    }

    #[test]
    fn plain_new_identity_and_deferred_destination_do_not_alias_pathless_drafts() {
        let draft = note("", "");
        assert!(same_identity(&Identity::New("alpha".into()), &draft, &Identity::New("alpha".into()), &draft));
        assert!(!same_identity(&Identity::New("alpha".into()), &draft, &Identity::New("beta".into()), &draft));
        let destination = Destination { note: draft.clone(), identity: Identity::New("alpha".into()), resolve_slug: Some("alpha".into()), omni: false };
        let existing = Destination { note: note("alpha", "alpha.md"), identity: Identity::Existing, resolve_slug: Some("alpha".into()), omni: false };
        assert!(!same_destination(&destination, &existing));
        assert!(same_destination(&destination, &destination.clone()));
    }

    #[test]
    fn newer_same_identity_retires_switch_decision_and_terminal_lease() {
        use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
        let draft = note("", "");
        let a = Identity::New("alpha".into());
        let b = Identity::New("beta".into());
        let owner = super::utility_mount::MountOwner::default();
        let mount = owner.attach();
        let epoch = AtomicU64::new(1);
        assert!(!same_identity(&a, &draft, &b, &draft));
        assert!(decision_receipt_current(&epoch, 1, &mount));
        assert!(same_identity(&a, &draft, &Identity::New("alpha".into()), &draft));
        epoch.fetch_add(1, Ordering::AcqRel);
        assert!(!decision_receipt_current(&epoch, 1, &mount));
        assert!(decision_receipt_current(&epoch, 2, &mount));
        owner.retire();
        assert!(!decision_receipt_current(&epoch, 2, &mount));
        let lease = AtomicBool::new(true);
        assert!(issued_receipt_current(&lease, 7, 7));
        assert!(!issued_receipt_current(&lease, 7, 8));
        lease.store(false, Ordering::Release);
        assert!(!issued_receipt_current(&lease, 7, 7));
    }

    #[test]
    fn latest_open_attempt_supersedes_previous_and_retirement() {
        use std::sync::atomic::{AtomicU64, Ordering};
        let epoch = AtomicU64::new(1);
        assert!(open_attempt_current(&epoch, 1));
        epoch.fetch_add(1, Ordering::AcqRel);
        assert!(!open_attempt_current(&epoch, 1));
        assert!(open_attempt_current(&epoch, 2));
        epoch.fetch_add(1, Ordering::AcqRel);
        assert!(!open_attempt_current(&epoch, 2));
    }

    #[test]
    fn linked_cached_note_matches_canonical_source_not_derived_links() {
        let dir = tempfile::tempdir().unwrap();
        let previous = std::env::var_os("ML_NOTES_DIR");
        unsafe {
            std::env::set_var("ML_NOTES_DIR", dir.path());
        }
        std::fs::write(dir.path().join("alpha.md"), "# Alpha\n\n[[Alternate]]").unwrap();
        std::fs::write(dir.path().join("beta.md"), "# Beta\nAlias: Alternate\n").unwrap();
        multi_launcher::plugins::note::refresh_cache().unwrap();
        let (_, notes) = multi_launcher::plugins::note::note_cache_snapshot_with_version().unwrap();
        let cached = notes.iter().find(|note| note.slug == "alpha").unwrap();
        let mut raw =
            multi_launcher::plugins::note::read_note_exact_if("alpha", Some(&cached.path), || true)
                .unwrap()
                .unwrap();
        assert_ne!(cached.links, raw.links);
        assert!(same_source(cached, &raw));
        raw.content.push('!');
        assert!(!same_source(cached, &raw));
        if let Some(previous) = previous {
            unsafe {
                std::env::set_var("ML_NOTES_DIR", previous);
            }
        } else {
            unsafe {
                std::env::remove_var("ML_NOTES_DIR");
            }
        }
    }
}
