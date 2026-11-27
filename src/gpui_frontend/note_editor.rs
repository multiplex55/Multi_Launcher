//! Retained existing-note editor and checked canonical open/save.
use super::*;
use gpui_kit::component::text::{MarkdownExtensions, TextView, TextViewState};
use multi_launcher::plugins::note::{self, Note, QuickNoteDraft, QuickNoteWrite};
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
    Quick {
        mount: utility_mount::MountReceipt,
        version: u64,
        filter: String,
    },
}

#[derive(Clone)]
enum Decision {
    Close,
    Switch(Note),
}
enum AfterSave {
    Stay,
    Close,
    Switch(Note),
}

#[derive(Debug, PartialEq, Eq)]
enum ClosePolicy {
    Close,
    Save,
    Decide,
}
fn close_policy(dirty: bool, save_on_close: bool, confirm_discard: bool) -> ClosePolicy {
    if !dirty {
        ClosePolicy::Close
    } else if save_on_close {
        ClosePolicy::Save
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

fn open_attempt_current(epoch: &AtomicU64, attempt: u64) -> bool {
    epoch.load(Ordering::Acquire) == attempt
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
    baseline: String,
    mode: NoteViewMode,
    settings: NoteSettings,
    save_on_close: bool,
    confirm_discard: bool,
    pending: bool,
    decision: Option<Decision>,
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
        note: Note,
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
            baseline,
            mode,
            settings,
            save_on_close,
            confirm_discard,
            pending: false,
            decision: None,
            focus_guard: None,
            activation_guard: None,
            message: None,
        }
    }
    fn dirty(&self, cx: &App) -> bool {
        self.body.read(cx).value().as_str() != self.baseline
    }
    pub(super) fn blocks_close(&self, cx: &App) -> bool {
        self.open
            && self.mount.is_current()
            && (self.pending || self.decision.is_some() || self.dirty(cx))
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
                self.decision = None;
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
    fn install(&mut self, note: Note, window: &mut Window, cx: &mut Context<Self>) {
        self.baseline = note.content.clone();
        self.note = note;
        self.body.update(cx, |body, cx| {
            body.set_value(self.baseline.clone(), window, cx)
        });
        self.preview.update(cx, |preview, cx| {
            preview.set_text(preview_source(&self.baseline), cx)
        });
        self.decision = None;
        self.message = None;
        cx.notify();
    }
    fn verify_switch(&mut self, target: Note, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending || !self.current(window, cx) {
            return;
        }
        self.pending = true;
        self.decision = Some(Decision::Switch(target.clone()));
        self.focus.focus(window, cx);
        let receipt = self.mount.clone();
        let worker_receipt = receipt.clone();
        let native = self.native;
        let visibility = self.visibility.clone();
        let revision = visibility.request_revision.load(Ordering::Acquire);
        let admission = self.admission.clone();
        let expected = target.clone();
        let host = self.host;
        let task = cx.background_executor().spawn(async move {
            note::read_note_exact_if(&expected.slug, Some(&expected.path), || {
                worker_receipt.is_current()
                    && !admission.is_fenced()
                    && json_utility::native_host_current(&visibility, revision, native, false)
                    && native.is_some_and(|native| {
                        current_foreground_identity() == Some(native.identity)
                    })
            })
        });
        cx.spawn(async move |weak, cx| {
            let result = task.await;
            let delivered = host.update(cx, |_, window, cx| {
                let _ = weak.update(cx, |view, cx| {
                    if !receipt.is_current() || !receipt.matches(&view.mount) {
                        return;
                    }
                    view.pending = false;
                    match result {
                        Ok(Some(note))
                            if view.focus.is_focused(window)
                                && view.current(window, cx)
                                && same_source(&note, &target) =>
                        {
                            view.install(note, window, cx);
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
                    if receipt.is_current() && receipt.matches(&view.mount) {
                        view.pending = false;
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
    fn open_note(&mut self, note: Note, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending {
            self.message = Some(WAIT.into());
            cx.notify();
            return;
        }
        if same_note(&self.note, &note) {
            self.open = true;
            self.focus_content(window, cx);
            return;
        }
        if self.dirty(cx) {
            self.decision = Some(Decision::Switch(note));
        } else {
            self.install(note, window, cx);
            self.focus_content(window, cx);
        }
        cx.notify();
    }
    fn request_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.current(window, cx) || self.pending {
            return;
        }
        match close_policy(self.dirty(cx), self.save_on_close, self.confirm_discard) {
            ClosePolicy::Close => self.retire(window, cx),
            ClosePolicy::Save => self.save(AfterSave::Close, window, cx),
            ClosePolicy::Decide => {
                self.decision = Some(Decision::Close);
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
        self.decision = None;
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
        let original = self.note.clone();
        let content = self.body.read(cx).value().to_string();
        let saved_input = content.clone();
        let receipt = self.mount.clone();
        let worker_receipt = receipt.clone();
        let admission = self.admission.clone();
        let visibility = self.visibility.clone();
        let revision = visibility.request_revision.load(Ordering::Acquire);
        let native = self.native;
        let task = cx.background_executor().spawn(async move {
            let result = note::write_quick_note_if(
                QuickNoteDraft::Editor {
                    original: &original,
                    content: &content,
                },
                || {
                    worker_receipt.is_current()
                        && !admission.is_fenced()
                        && lease.load(Ordering::Acquire)
                        && json_utility::native_host_current(&visibility, revision, native, false)
                        && native.is_some_and(|native| {
                            current_foreground_identity() == Some(native.identity)
                        })
                },
            );
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
                            || !view.current(window, cx)
                            || !issuing_focus.is_focused(window)
                        {
                            return;
                        }
                        match after {
                            AfterSave::Stay => view.focus_content(window, cx),
                            AfterSave::Close => view.retire(window, cx),
                            AfterSave::Switch(note) => view.verify_switch(note, window, cx),
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
                    .child(Textarea::new(&self.body).aria_label("Note Markdown")),
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
                                .on_click(cx.listener(|view, _, window, cx| {
                                    if !view.current(window, cx) {
                                        return;
                                    }
                                    let after = match view.decision.clone() {
                                        Some(Decision::Close) => AfterSave::Close,
                                        Some(Decision::Switch(note)) => AfterSave::Switch(note),
                                        None => return,
                                    };
                                    view.save(after, window, cx);
                                })),
                        )
                        .child(
                            Button::new("note-decision-discard")
                                .label("Discard")
                                .disabled(self.pending)
                                .on_click(cx.listener(|view, _, window, cx| {
                                    if !view.current(window, cx) || view.pending {
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
                                .on_click(cx.listener(|view, _, window, cx| {
                                    if view.current(window, cx) && !view.pending {
                                        view.decision = None;
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
            || self
                .enabled_plugins
                .as_ref()
                .is_some_and(|enabled| !enabled.contains("notes"))
            || !multi_launcher::settings::provider_search_capability_enabled(
                self.enabled_capabilities.as_ref(),
                "notes",
            )
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
                        || root
                            .enabled_plugins
                            .as_ref()
                            .is_some_and(|enabled| !enabled.contains("notes"))
                        || !multi_launcher::settings::provider_search_capability_enabled(
                            root.enabled_capabilities.as_ref(),
                            "notes",
                        )
                    {
                        return;
                    }
                    let origin_current = match &origin {
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
                    if let Some(view) = root.note_editor_view.clone() {
                        view.update(cx, |view, cx| {
                            if !view.open {
                                view.open = true;
                                view.mount = view.owner.attach();
                                view.install(note, window, cx);
                                view.focus_content(window, cx);
                            } else {
                                view.open_note(note, window, cx);
                            }
                        });
                    } else {
                        let owner = cx.entity().downgrade();
                        let settings = root.search_catalog.note_settings.clone();
                        let save = root.search_catalog.note_save_on_close;
                        let confirm = root.search_catalog.note_confirm_discard;
                        let visibility = root.root_visibility.clone();
                        let admission = root.restart_admission.clone();
                        let notes_wake = root.search_catalog.providers.notes_wake.clone();
                        let open_epoch = root.note_open_epoch.clone();
                        let view = cx.new(|cx| {
                            NoteEditor::new(
                                note,
                                settings,
                                save,
                                confirm,
                                owner,
                                Some(native),
                                visibility,
                                admission,
                                notes_wake,
                                open_epoch,
                                window,
                                cx,
                            )
                        });
                        view.update(cx, |view, cx| view.focus_content(window, cx));
                        root.note_editor_view = Some(view);
                    }
                    let id = window.window_handle().window_id();
                    let history = root
                        .clipboard_history
                        .as_ref()
                        .is_some_and(|view| view.read(cx).is_mounted_in(id, false));
                    let modify = root
                        .clipboard_modify_view
                        .as_ref()
                        .is_some_and(|view| view.read(cx).is_mounted_in(id, false));
                    let snippets = root
                        .snippets_view
                        .as_ref()
                        .is_some_and(|view| view.read(cx).open);
                    let fill = root
                        .snippet_fill
                        .as_ref()
                        .is_some_and(|view| view.read(cx).open);
                    let tempfile = root
                        .tempfile_view
                        .as_ref()
                        .is_some_and(|view| view.read(cx).open);
                    let folder = root
                        .folder_size_view
                        .as_ref()
                        .is_some_and(|view| view.read(cx).is_mounted_in(id, false));
                    let lock = root
                        .file_lock_view
                        .as_ref()
                        .is_some_and(|view| view.read(cx).is_mounted_in(id, false));
                    let duplicates = root
                        .duplicate_finder_view
                        .as_ref()
                        .is_some_and(|view| view.read(cx).open);
                    let quick = root
                        .quick_notes_view
                        .as_ref()
                        .is_some_and(|view| view.read(cx).open);
                    root.root_scroll.scroll_to_top_of_item(
                        root.clipboard_scroll_item(id, cx)
                            + usize::from(history)
                            + usize::from(modify)
                            + usize::from(snippets)
                            + usize::from(fill)
                            + usize::from(tempfile)
                            + usize::from(folder)
                            + usize::from(lock)
                            + usize::from(duplicates)
                            + usize::from(quick),
                    );
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
        ClosePolicy, close_policy, open_attempt_current, preview_source, same_note, same_source,
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
        assert_eq!(close_policy(false, true, true), ClosePolicy::Close);
        assert_eq!(close_policy(true, true, true), ClosePolicy::Save);
        assert_eq!(close_policy(true, false, true), ClosePolicy::Decide);
        assert_eq!(close_policy(true, false, false), ClosePolicy::Close);
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
