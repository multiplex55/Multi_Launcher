//! Retained read-only Clipboard History browser and cancellable edit draft.
use super::*;
use gpui_kit::base::Selectable;
use multi_launcher::clipboard_modify::clipboard::ArboardClipboardBackend;
use multi_launcher::common::persistence::{LoadState, PersistenceError};
use multi_launcher::plugins::clipboard::{
    GuardedHistoryOutcome, clipboard_version, copy_history_if_unchanged, load_history_typed,
    replace_history_guarded,
};
use std::path::{Path, PathBuf};

pub(super) fn current_provider(
    query: &str,
    action: &Action,
    enabled: Option<&HashSet<String>>,
    caps: Option<&HashMap<String, Vec<String>>>,
) -> bool {
    enabled.is_none_or(|enabled| enabled.contains("clipboard"))
        && multi_launcher::settings::provider_search_capability_enabled(caps, "clipboard")
        && multi_launcher::plugins::clipboard::bare_history_opener(query).as_ref() == Some(action)
}
fn close_key(key: &str, modifiers: Modifiers) -> bool {
    (key == "escape" && modifiers == Modifiers::default())
        || (key == "w"
            && modifiers
                == Modifiers {
                    control: true,
                    ..Modifiers::default()
                })
}
fn preview(text: &str) -> String {
    let mut chars = text.chars();
    let mut result: String = chars
        .by_ref()
        .take(160)
        .map(|c| if matches!(c, '\n' | '\r') { ' ' } else { c })
        .collect();
    if chars.next().is_some() {
        result.push('…');
    }
    result
}
fn load(path: &Path) -> Result<Vec<String>, String> {
    match load_history_typed(path).map_err(|error| match error {
        PersistenceError::MalformedJson { source, .. } => format!(
            "Invalid history JSON at line {}, column {}",
            source.line(),
            source.column()
        ),
        PersistenceError::Read { source, .. } => {
            format!("Could not read history ({:?})", source.kind())
        }
        _ => "Could not load history".to_owned(),
    })? {
        LoadState::Missing | LoadState::Empty => Ok(Vec::new()),
        LoadState::Loaded(rows) => Ok(rows),
    }
}
fn same_unreadable_snapshot(
    old_version: u64,
    old_rows: &Arc<Vec<String>>,
    version: u64,
    current_rows: &Arc<Vec<String>>,
    loaded: &[String],
) -> bool {
    old_version == version && Arc::ptr_eq(old_rows, current_rows) && old_rows.as_slice() == loaded
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct EditReceipt {
    version: u64,
    generation: u64,
    index: usize,
    rows: Arc<Vec<String>>,
    instance: u64,
}
impl EditReceipt {
    fn current(
        &self,
        version: Option<u64>,
        live_version: u64,
        generation: u64,
        rows: &Arc<Vec<String>>,
        loading: bool,
        failed: bool,
    ) -> bool {
        !loading
            && !failed
            && version == Some(self.version)
            && live_version == self.version
            && generation == self.generation
            && Arc::ptr_eq(&self.rows, rows)
            && self.rows.get(self.index).is_some()
    }
}
#[derive(Clone)]
enum Effect {
    Save(EditReceipt, String),
    Remove(EditReceipt),
    Copy(EditReceipt),
}
impl Effect {
    fn receipt(&self) -> &EditReceipt {
        match self {
            Self::Save(receipt, _) | Self::Remove(receipt) | Self::Copy(receipt) => receipt,
        }
    }
}
pub(super) const WAIT: &str = "Wait for Clipboard History to finish its operation.";
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RestartDraft {
    edit: Option<(EditReceipt, String)>,
    decision: Option<EditReceipt>,
}
pub(super) struct ClipboardHistory {
    pub open: bool,
    pub(super) popout: bool,
    pub(super) rehosting: bool,
    pub(super) presentation_error: Option<String>,
    pub(super) geometry_error: Option<String>,
    host: AnyWindowHandle,
    native: Option<NativeWindow>,
    pub(super) activation_revision: Arc<AtomicU64>,
    owner: utility_mount::MountOwner,
    receipt: utility_mount::MountReceipt,
    pub(super) root: WeakEntity<RootWindow>,
    path: PathBuf,
    focus: FocusHandle,
    decision_focus: FocusHandle,
    save_focus: FocusHandle,
    copy_focus: FocusHandle,
    editor: Option<Entity<TextareaState>>,
    edit: Option<EditReceipt>,
    selected: Option<(u64, usize)>,
    rows: Arc<Vec<String>>,
    generation: u64,
    version: Option<u64>,
    loading: bool,
    diagnostic: Option<String>,
    preserve_unreadable: Option<(u64, Arc<Vec<String>>)>,
    feedback: Option<String>,
    decision: Option<EditReceipt>,
    instance: u64,
    busy: bool,
    effect_epoch: Arc<AtomicU64>,
    pub(super) admission: Arc<RestartAdmission>,
    visibility: RootVisibility,
}
impl ClipboardHistory {
    fn new(
        _window: &mut Window,
        cx: &mut Context<Self>,
        root: WeakEntity<RootWindow>,
        path: PathBuf,
        admission: Arc<RestartAdmission>,
        visibility: RootVisibility,
        host: AnyWindowHandle,
        native: Option<NativeWindow>,
    ) -> Self {
        let owner = utility_mount::MountOwner::default();
        let receipt = owner.attach();
        Self {
            open: false,
            popout: false,
            rehosting: false,
            presentation_error: None,
            geometry_error: None,
            host,
            native,
            activation_revision: Arc::new(AtomicU64::new(0)),
            owner,
            receipt,
            root,
            path,
            focus: cx.focus_handle(),
            decision_focus: cx.focus_handle(),
            save_focus: cx.focus_handle(),
            copy_focus: cx.focus_handle(),
            editor: None,
            edit: None,
            selected: None,
            rows: Arc::new(Vec::new()),
            generation: 0,
            version: None,
            loading: false,
            diagnostic: None,
            preserve_unreadable: None,
            feedback: None,
            decision: None,
            instance: 0,
            busy: false,
            effect_epoch: Arc::new(AtomicU64::new(0)),
            admission,
            visibility,
        }
    }
    pub(super) fn mount_receipt(&self) -> utility_mount::MountReceipt {
        self.receipt.clone()
    }
    pub(super) fn is_mounted_in(&self, id: WindowId, popout: bool) -> bool {
        self.open
            && self.popout == popout
            && self.receipt.is_current()
            && self.host.window_id() == id
    }
    pub(super) fn replace_mount(
        &mut self,
        host: AnyWindowHandle,
        native: Option<NativeWindow>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let draft = self
            .editor
            .as_ref()
            .map(|editor| editor.read(cx).value().to_string());
        let pending = self.loading;
        if pending && self.preserve_unreadable.is_none() {
            self.preserve_unreadable = self.version.map(|version| (version, self.rows.clone()));
        }
        self.receipt = self.owner.attach();
        self.host = host;
        self.native = native;
        self.focus = cx.focus_handle();
        self.decision_focus = cx.focus_handle();
        self.save_focus = cx.focus_handle();
        self.copy_focus = cx.focus_handle();
        let editor = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(6, 14)
                .placeholder("Clipboard entry")
        });
        if let Some(draft) = draft {
            editor.update(cx, |input, cx| input.set_value(draft, window, cx));
        }
        self.editor = Some(editor);
        self.loading = false;
        if pending {
            self.version = None;
            self.refresh(cx);
        }
    }
    pub(super) fn contains_focus(&self, window: &Window, cx: &App) -> bool {
        self.focus.is_focused(window) || self.focus.contains_focused(window, cx)
    }
    pub(super) fn focus_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.rehosting {
            self.focus.focus(window, cx);
            return;
        }
        if self.decision.is_some() {
            self.decision_focus.focus(window, cx);
        } else if self.edit.is_some() {
            if let Some(editor) = &self.editor {
                editor.focus_handle(cx).focus(window, cx);
            }
        } else {
            self.focus.focus(window, cx);
        }
    }
    pub(super) fn finish_dock_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.focus.is_focused(window)
            && self
                .native
                .is_some_and(|native| current_foreground_identity() == Some(native.identity))
        {
            self.focus_editor(window, cx);
        }
    }
    pub(super) fn finish_popout_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.focus.is_focused(window)
            && window.is_window_active()
            && self
                .native
                .is_some_and(|native| current_foreground_identity() == Some(native.identity))
        {
            self.focus_editor(window, cx);
        }
    }
    pub(super) fn request_rehost(&mut self, popout: bool, cx: &mut Context<Self>) {
        if !self.open
            || self.busy
            || self.rehosting
            || self.admission.is_fenced()
            || self.popout == popout
        {
            return;
        }
        let source = self.receipt.clone();
        let gesture = self.native.map(|source| hash_dock::Gesture {
            source,
            foreground_authorized: native_window_is_valid(source)
                && current_foreground_identity() == Some(source.identity),
            visibility_revision: self.visibility.request_revision.load(Ordering::Acquire),
        });
        self.rehosting = true;
        self.activation_revision.fetch_add(1, Ordering::AcqRel);
        let root = self.root.clone();
        cx.spawn(async move |_, cx| {
            let _ = root.update(cx, |root, cx| {
                root.start_clipboard_rehost(popout, gesture, source, cx)
            });
        })
        .detach();
        cx.notify();
    }
    pub(super) fn shutdown(&mut self) {
        self.open = false;
        self.owner.retire();
        self.effect_epoch.fetch_add(1, Ordering::AcqRel);
        self.activation_revision.fetch_add(1, Ordering::AcqRel);
        self.editor = None;
        self.edit = None;
        self.decision = None;
        self.loading = false;
    }
    fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.open {
            if self.diagnostic.is_some() && !self.loading && !self.busy {
                self.version = None;
                self.refresh(cx);
            }
            if self.decision.is_some() {
                self.decision_focus.focus(window, cx);
            } else if self.edit.is_some() {
                if let Some(editor) = &self.editor {
                    editor.focus_handle(cx).focus(window, cx);
                }
            } else {
                self.focus.focus(window, cx);
            }
            return;
        }
        if !self.receipt.is_current() {
            self.receipt = self.owner.attach();
        }
        if self.editor.is_none() {
            self.editor = Some(cx.new(|cx| {
                TextareaState::new(window, cx)
                    .auto_grow(6, 14)
                    .placeholder("Clipboard entry")
            }));
        }
        self.open = true;
        self.edit = None;
        self.decision = None;
        self.instance = self.instance.wrapping_add(1);
        self.selected = None;
        self.loading = false;
        self.version = None;
        self.preserve_unreadable = None;
        self.focus.focus(window, cx);
        self.refresh(cx);
        cx.notify();
    }
    fn refresh(&mut self, cx: &mut Context<Self>) {
        if !self.open || self.loading || self.busy {
            return;
        }
        self.loading = true;
        let version = clipboard_version();
        let path = self.path.clone();
        let receipt = self.receipt.clone();
        let task = cx.background_executor().spawn(async move { load(&path) });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |view, cx| {
                if !view.open || !receipt.is_current() || !receipt.matches(&view.receipt) {
                    return;
                }
                view.loading = false;
                if version != clipboard_version() {
                    view.refresh(cx);
                    return;
                }
                view.version = Some(version);
                match result {
                    Ok(rows) => {
                        let preserve = view.preserve_unreadable.take().is_some_and(
                            |(old_version, old_rows)| {
                                same_unreadable_snapshot(
                                    old_version,
                                    &old_rows,
                                    version,
                                    &view.rows,
                                    &rows,
                                )
                            },
                        );
                        if !preserve {
                            view.rows = Arc::new(rows);
                            view.generation = view.generation.wrapping_add(1);
                            view.selected = None;
                        }
                        view.diagnostic = None;
                    }
                    Err(error) => {
                        view.preserve_unreadable = None;
                        view.diagnostic = Some(error);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn restart_draft(&self, cx: &App) -> Option<RestartDraft> {
        if !self.open {
            return None;
        }
        let edit = self.edit.as_ref().and_then(|edit| {
            self.editor
                .as_ref()
                .map(|editor| (edit.clone(), editor.read(cx).value().to_string()))
        });
        if edit.is_none() && self.decision.is_none() {
            None
        } else {
            Some(RestartDraft {
                edit,
                decision: self.decision.clone(),
            })
        }
    }
    fn current_event(&self, receipt: &utility_mount::MountReceipt, window: &Window) -> bool {
        self.open
            && !self.rehosting
            && receipt.is_current()
            && receipt.matches(&self.receipt)
            && self.host.window_id() == window.window_handle().window_id()
            && window.is_window_active()
            && self
                .native
                .is_some_and(|native| current_foreground_identity() == Some(native.identity))
    }
    pub(super) fn busy(&self) -> bool {
        self.busy
    }
    fn valid(&self, receipt: &EditReceipt) -> bool {
        receipt.instance == self.instance
            && receipt.current(
                self.version,
                clipboard_version(),
                self.generation,
                &self.rows,
                self.loading,
                self.diagnostic.is_some(),
            )
    }
    fn effect(&mut self, effect: Effect, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx)
            || self.busy
            || self.rehosting
            || !self.open
            || !self.receipt.is_current()
            || self.admission.is_fenced()
            || !self.valid(effect.receipt())
        {
            return;
        }
        let target = effect.receipt();
        let owns_target = match &effect {
            Effect::Save(_, _) => self
                .edit
                .as_ref()
                .is_some_and(|current| current.instance == target.instance),
            Effect::Remove(_) => self
                .decision
                .as_ref()
                .is_some_and(|current| current.instance == target.instance),
            Effect::Copy(_) => {
                self.edit.is_none()
                    && self.decision.is_none()
                    && self.selected == Some((target.generation, target.index))
            }
        };
        if !owns_target {
            return;
        }
        let is_copy = matches!(&effect, Effect::Copy(_));
        let host = window.window_handle();
        let native = self.native;
        let Some(native) = native else {
            return;
        };
        if !window.is_window_active()
            || self.host.window_id() != window.window_handle().window_id()
            || current_foreground_identity() != Some(native.identity)
        {
            return;
        }
        let Some(permit) = self.admission.admit() else {
            return;
        };
        self.busy = true;
        self.feedback = None;
        let receipt = self.receipt.clone();
        let worker_receipt = receipt.clone();
        let epoch = self.effect_epoch.clone();
        let expected_epoch = epoch.load(Ordering::Acquire);
        let visibility = self.visibility.clone();
        let popout = self.popout;
        let visibility_revision = visibility.request_revision.load(Ordering::Acquire);
        let admission = self.admission.clone();
        let path = self.path.clone();
        let version = effect.receipt().version;
        let task = cx.background_executor().spawn(async move {
            let current = || {
                worker_receipt.is_current()
                    && epoch.load(Ordering::Acquire) == expected_epoch
                    && !admission.is_fenced()
                    && clipboard_version() == version
                    && json_utility::native_host_current(
                        &visibility,
                        visibility_revision,
                        Some(native),
                        popout,
                    )
                    && current_foreground_identity() == Some(native.identity)
            };
            if !current() {
                return GuardedHistoryOutcome::Stale;
            }
            let target = effect.receipt().clone();
            let mut replacement = target.rows.as_ref().clone();
            match effect {
                Effect::Copy(_) => copy_history_if_unchanged(
                    &path,
                    &target.rows,
                    target.index,
                    &ArboardClipboardBackend,
                    &current,
                ),
                Effect::Save(_, value) => {
                    replacement[target.index] = value;
                    replace_history_guarded(&path, &target.rows, &replacement, &current)
                }
                Effect::Remove(_) => {
                    replacement.remove(target.index);
                    replace_history_guarded(&path, &target.rows, &replacement, &current)
                }
            }
        });
        cx.spawn(async move |weak, cx| {
            let _permit = permit;
            let result = task.await;
            let Some(view) = weak.upgrade() else {
                return;
            };
            let apply = |view: &mut Self, window: Option<&mut Window>, cx: &mut Context<Self>| {
                if !receipt.is_current() || !receipt.matches(&view.receipt) {
                    return;
                }
                view.busy = false;
                match result {
                    GuardedHistoryOutcome::Updated => {
                        if !is_copy {
                            if let Some(window) = window {
                                if window.window_handle().window_id() == host.window_id()
                                    && window.is_window_active()
                                    && current_foreground_identity() == Some(native.identity)
                                    && (view.focus.is_focused(window)
                                        || view.focus.contains_focused(window, cx))
                                {
                                    view.focus.focus(window, cx);
                                }
                            }
                            view.edit = None;
                            view.decision = None;
                            view.instance = view.instance.wrapping_add(1);
                            view.version = None;
                            view.refresh(cx);
                        } else {
                            view.feedback = Some("Copied clipboard entry.".into());
                        }
                    }
                    GuardedHistoryOutcome::Conflict | GuardedHistoryOutcome::Unreadable => {
                        view.feedback = Some(
                            if is_copy {
                                "History changed or is unreadable; nothing was copied."
                            } else {
                                "History changed or is unreadable; no change was saved."
                            }
                            .into(),
                        );
                        view.preserve_unreadable = if result == GuardedHistoryOutcome::Unreadable {
                            view.version.map(|version| (version, view.rows.clone()))
                        } else {
                            None
                        };
                        view.version = None;
                        view.refresh(cx);
                    }
                    GuardedHistoryOutcome::Stale => {
                        view.feedback = Some(
                            "The operation lost foreground or mount authority; retry when ready."
                                .into(),
                        )
                    }
                    GuardedHistoryOutcome::Failed => {
                        view.feedback = Some(
                            if is_copy {
                                "Could not copy clipboard entry."
                            } else {
                                "Could not update clipboard history."
                            }
                            .into(),
                        )
                    }
                }
                let _ = view.root.update(cx, |root, cx| {
                    if root.restart_status.as_deref() == Some(WAIT) {
                        root.restart_status = None;
                        cx.notify();
                    }
                });
                cx.notify();
            };
            let update = update_kit_window_content(host, &view, cx, |view, window, cx| {
                apply(view, Some(window), cx)
            });
            if update.is_err() {
                let _ = view.update(cx, |view, cx| apply(view, None, cx));
            }
        })
        .detach();
        cx.notify();
    }
    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open || self.busy || self.rehosting {
            return;
        }
        let popout = self.popout;
        if popout {
            let _ = self.root.update(cx, |root, cx| {
                root.drain_clipboard_native_geometry(cx, true)
            });
        }
        self.shutdown();
        if popout {
            window.remove_window();
            cx.notify();
            return;
        }
        let _ = self.root.update(cx, |root, cx| {
            panel_stack::retire(&mut root.panel_stack, Panel::ClipboardDialog);
            root.query.focus_handle(cx).focus(window, cx);
            cx.notify();
        });
        cx.notify();
    }
}
impl Render for ClipboardHistory {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.open && !self.loading && self.version != Some(clipboard_version()) {
            self.refresh(cx);
        }
        let receipt = self.receipt.clone();
        let close = receipt.clone();
        let cancel = receipt.clone();
        let edit_instance = self.edit.as_ref().map(|edit| edit.instance);
        let key_mount = receipt.clone();
        let rehost_mount = receipt.clone();
        let mut panel = div()
            .id("clipboard-history")
            .track_focus(&self.focus)
            .capture_key_down(cx.listener(move |view, event: &KeyDownEvent, window, cx| {
                if !view.current_event(&key_mount, window)
                    || window.default_prevented()
                    || window.has_active_dialog(cx)
                    || !window.is_window_active()
                    || !view.native.is_some_and(|native| {
                        current_foreground_identity() == Some(native.identity)
                    })
                {
                    return;
                }
                if view.decision.is_some()
                    && !view.busy
                    && !view.rehosting
                    && event.keystroke.key == "escape"
                    && event.keystroke.modifiers == Modifiers::default()
                    && (view.decision_focus.is_focused(window)
                        || view.decision_focus.contains_focused(window, cx))
                {
                    window.prevent_default();
                    cx.stop_propagation();
                    if !event.is_held {
                        view.decision = None;
                        view.instance = view.instance.wrapping_add(1);
                        view.focus.focus(window, cx);
                        view.version = None;
                        view.refresh(cx);
                        cx.notify();
                    }
                    return;
                }
                let contexts = window.context_stack();
                if close_key(&event.keystroke.key, event.keystroke.modifiers)
                    && view.contains_focus(window, cx)
                    && window.is_window_active()
                    && view.native.is_some_and(|native| {
                        current_foreground_identity() == Some(native.identity)
                    })
                    && !window.has_active_dialog(cx)
                    && !context_stack_has_primary(&contexts, "PopupMenu")
                    && !context_stack_has_primary(&contexts, "Dialog")
                    && !context_stack_has_primary(&contexts, "AlertDialog")
                {
                    window.prevent_default();
                    cx.stop_propagation();
                    if !event.is_held {
                        view.close(window, cx);
                    }
                    return;
                }
                let protected_focus = (view.decision.is_some()
                    && (view.decision_focus.is_focused(window)
                        || view.decision_focus.contains_focused(window, cx)))
                    || (view.edit.is_some() && view.save_focus.is_focused(window))
                    || (view.edit.is_none()
                        && view.decision.is_none()
                        && view.copy_focus.is_focused(window));
                if protected_focus
                    && event.is_held
                    && event.keystroke.modifiers == Modifiers::default()
                    && matches!(event.keystroke.key.as_str(), "enter" | "space")
                {
                    window.prevent_default();
                    cx.stop_propagation();
                }
            }))
            .w_full()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .child(Label::new("Clipboard History"));
        if let Some(error) = &self.diagnostic {
            panel = panel.child(Label::new(format!(
                "History could not be loaded; showing last available entries: {error}"
            )));
        }
        if self.loading {
            panel = panel.child(Label::new("Loading clipboard history…"));
        }
        if let Some(feedback) = &self.feedback {
            panel = panel.child(Label::new(feedback.clone()));
        }
        if let Some(edit) = &self.edit {
            if !edit.current(
                self.version,
                clipboard_version(),
                self.generation,
                &self.rows,
                self.loading,
                self.diagnostic.is_some(),
            ) {
                panel = panel.child(Label::new(
                    "History changed or could not be loaded; this draft is stale.",
                ));
            }
            let Some(editor) = &self.editor else {
                return panel;
            };
            panel = panel
                .child(
                    Textarea::new(editor)
                        .readonly(self.busy || self.rehosting)
                        .aria_label("Edit clipboard entry"),
                )
                .child(
                    Button::new("clipboard-edit-save")
                        .label("Save")
                        .track_focus(&self.save_focus)
                        .disabled(self.busy || !self.valid(edit))
                        .on_click({
                            let edit = edit.clone();
                            let save_mount = receipt.clone();
                            cx.listener(move |view, _, window, cx| {
                                if !view.current_event(&save_mount, window)
                                    || view.busy
                                    || !view
                                        .edit
                                        .as_ref()
                                        .is_some_and(|current| current.instance == edit.instance)
                                {
                                    return;
                                }
                                let Some(editor) = view.editor.as_ref() else {
                                    return;
                                };
                                let text = editor.read(cx).value().to_string();
                                if text.trim().is_empty() {
                                    view.feedback = Some("Clipboard entry cannot be blank.".into());
                                    cx.notify();
                                    return;
                                }
                                view.effect(Effect::Save(edit.clone(), text), window, cx);
                            })
                        }),
                )
                .child(
                    Button::new("clipboard-edit-cancel")
                        .label("Cancel")
                        .disabled(self.busy || self.rehosting)
                        .on_click(cx.listener(move |view, _, window, cx| {
                            if view.current_event(&cancel, window)
                                && !window.has_active_dialog(cx)
                                && edit_instance.is_some()
                                && view.edit.as_ref().map(|edit| edit.instance) == edit_instance
                                && !view.busy
                            {
                                view.edit = None;
                                view.instance = view.instance.wrapping_add(1);
                                view.version = None;
                                view.refresh(cx);
                                view.focus.focus(window, cx);
                                cx.notify();
                            }
                        })),
                );
        } else if let Some(decision) = &self.decision {
            let confirm = decision.clone();
            let cancel = decision.clone();
            let confirm_mount = receipt.clone();
            let cancel_mount = receipt.clone();
            panel =
                panel.child(
                    div()
                        .track_focus(&self.decision_focus)
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(Label::new(format!(
                            "Remove this entry? {}",
                            preview(&decision.rows[decision.index])
                        )))
                        .child(
                            Button::new("clipboard-remove-confirm")
                                .label("Confirm remove")
                                .disabled(self.busy || !self.valid(decision))
                                .on_click(cx.listener(move |view, _, window, cx| {
                                    if view.current_event(&confirm_mount, window)
                                        && view.decision.as_ref().is_some_and(|current| {
                                            current.instance == confirm.instance
                                        })
                                    {
                                        view.effect(Effect::Remove(confirm.clone()), window, cx);
                                    }
                                })),
                        )
                        .child(
                            Button::new("clipboard-remove-cancel")
                                .label("Cancel")
                                .disabled(self.busy || self.rehosting)
                                .on_click(cx.listener(move |view, _, window, cx| {
                                    if view.current_event(&cancel_mount, window)
                                        && !window.has_active_dialog(cx)
                                        && !view.busy
                                        && view.decision.as_ref().is_some_and(|current| {
                                            current.instance == cancel.instance
                                        })
                                    {
                                        view.decision = None;
                                        view.instance = view.instance.wrapping_add(1);
                                        view.focus.focus(window, cx);
                                        view.version = None;
                                        view.refresh(cx);
                                        cx.notify();
                                    }
                                })),
                        ),
                );
        } else {
            let rows = self.rows.clone();
            let generation = self.generation;
            let version = self.version;
            let ready = !self.loading
                && !self.busy
                && self.diagnostic.is_none()
                && version == Some(clipboard_version());
            let selected = self.selected;
            let owner = cx.entity().downgrade();
            let row_receipt = receipt.clone();
            let list =
                gpui_kit::uniform_list("clipboard-history-rows", rows.len(), move |range, _, _| {
                    range
                        .filter_map(|index| {
                            let entry = rows.get(index)?;
                            let label = preview(entry);
                            let owner_select = owner.clone();
                            let owner_edit = owner.clone();
                            let select_receipt = row_receipt.clone();
                            let edit_receipt = row_receipt.clone();
                            let selected_rows = rows.clone();
                            let edit_rows = rows.clone();
                            let row_live =
                                move |view: &ClipboardHistory,
                                      receipt: &utility_mount::MountReceipt,
                                      expected: &Arc<Vec<String>>| {
                                    view.open
                                        && !view.rehosting
                                        && receipt.is_current()
                                        && receipt.matches(&view.receipt)
                                        && view.edit.is_none()
                                        && view.decision.is_none()
                                        && !view.busy
                                        && !view.loading
                                        && view.diagnostic.is_none()
                                        && view.generation == generation
                                        && view.version == version
                                        && version == Some(clipboard_version())
                                        && Arc::ptr_eq(&view.rows, expected)
                                };
                            Some(
                                div()
                                    .id(SharedString::from(format!(
                                        "clipboard-row-{generation}-{index}"
                                    )))
                                    .h(px(36.))
                                    .flex()
                                    .gap_2()
                                    .child(
                                        Button::new(SharedString::from(format!(
                                            "clipboard-select-{generation}-{index}"
                                        )))
                                        .label(label)
                                        .selected(selected == Some((generation, index)))
                                        .disabled(!ready)
                                        .on_click(
                                            move |_, window, app| {
                                                let _ = owner_select.update(app, |view, cx| {
                                                    if view.current_event(&select_receipt, window)
                                                        && !window.has_active_dialog(cx)
                                                        && row_live(
                                                            view,
                                                            &select_receipt,
                                                            &selected_rows,
                                                        )
                                                        && view.rows.get(index).is_some()
                                                    {
                                                        view.selected = Some((generation, index));
                                                        cx.notify();
                                                    }
                                                });
                                            },
                                        ),
                                    )
                                    .child(
                                        Button::new(SharedString::from(format!(
                                            "clipboard-edit-{generation}-{index}"
                                        )))
                                        .label("Edit")
                                        .disabled(!ready)
                                        .on_click(
                                            move |_, window, app| {
                                                let _ =
                                                    owner_edit.update(app, |view, cx| {
                                                        if !view
                                                            .current_event(&edit_receipt, window)
                                                            || window.has_active_dialog(cx)
                                                            || !row_live(
                                                                view,
                                                                &edit_receipt,
                                                                &edit_rows,
                                                            )
                                                        {
                                                            return;
                                                        }
                                                        let (
                                                            Some(entry),
                                                            Some(version),
                                                            Some(editor),
                                                        ) = (
                                                            view.rows.get(index).cloned(),
                                                            version,
                                                            view.editor.clone(),
                                                        )
                                                        else {
                                                            return;
                                                        };
                                                        view.selected = Some((generation, index));
                                                        view.instance =
                                                            view.instance.wrapping_add(1);
                                                        view.edit = Some(EditReceipt {
                                                            version,
                                                            generation,
                                                            index,
                                                            rows: edit_rows.clone(),
                                                            instance: view.instance,
                                                        });
                                                        editor.update(cx, |input, cx| {
                                                            input.set_value(entry, window, cx)
                                                        });
                                                        editor.focus_handle(cx).focus(window, cx);
                                                        cx.notify();
                                                    });
                                            },
                                        ),
                                    )
                                    .into_any_element(),
                            )
                        })
                        .collect::<Vec<_>>()
                })
                .h(px(240.));
            panel = panel.child(list);
            if let (Some((selected_generation, index)), Some(version)) =
                (self.selected, self.version)
            {
                if selected_generation == self.generation && self.rows.get(index).is_some() {
                    let item = EditReceipt {
                        version,
                        generation: self.generation,
                        index,
                        rows: self.rows.clone(),
                        instance: self.instance,
                    };
                    let copy = item.clone();
                    let remove = item.clone();
                    let copy_mount = receipt.clone();
                    let remove_mount = receipt.clone();
                    panel = panel.child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                Button::new("clipboard-copy-selected")
                                    .label("Copy")
                                    .track_focus(&self.copy_focus)
                                    .disabled(!ready)
                                    .on_click(cx.listener(move |view, _, window, cx| {
                                        if view.current_event(&copy_mount, window) {
                                            view.effect(Effect::Copy(copy.clone()), window, cx);
                                        }
                                    })),
                            )
                            .child(
                                Button::new("clipboard-remove-selected")
                                    .label("Remove…")
                                    .disabled(!ready)
                                    .on_click(cx.listener(move |view, _, window, cx| {
                                        if view.current_event(&remove_mount, window)
                                            && !window.has_active_dialog(cx)
                                            && !view.busy
                                            && !view.rehosting
                                            && view.valid(&remove)
                                            && view.selected
                                                == Some((remove.generation, remove.index))
                                            && view.edit.is_none()
                                            && view.decision.is_none()
                                        {
                                            view.instance = view.instance.wrapping_add(1);
                                            let mut decision = remove.clone();
                                            decision.instance = view.instance;
                                            view.decision = Some(decision);
                                            view.decision_focus.focus(window, cx);
                                            cx.notify();
                                        }
                                    })),
                            ),
                    );
                }
            }
            panel = panel.child(
                Button::new("clipboard-history-close")
                    .disabled(self.busy || self.rehosting)
                    .label("Close")
                    .on_click(cx.listener(move |view, _, window, cx| {
                        if view.current_event(&close, window) {
                            view.close(window, cx);
                        }
                    })),
            );
        }
        panel = panel.child(
            Button::new("clipboard-rehost")
                .label(if self.popout { "Dock" } else { "Pop Out" })
                .disabled(self.busy || self.rehosting)
                .on_click(cx.listener(move |view, _, window, cx| {
                    if view.current_event(&rehost_mount, window)
                        && !window.has_active_dialog(cx)
                        && window.is_window_active()
                        && view.native.is_some_and(|native| {
                            current_foreground_identity() == Some(native.identity)
                        })
                    {
                        view.request_rehost(!view.popout, cx);
                    }
                })),
        );
        if let Some(error) = &self.presentation_error {
            panel = panel.child(Label::new(error.clone()));
        }
        if let Some(error) = &self.geometry_error {
            panel = panel.child(Label::new(error.clone()));
        }
        panel
    }
}
impl RootWindow {
    pub(super) fn open_clipboard_history(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.restart_admission.is_fenced()
            || self
                .clipboard_history
                .as_ref()
                .is_some_and(|view| view.read(cx).rehosting)
        {
            return;
        }
        if let (Some(popup), Some(view), Some(root_native)) = (
            &self.clipboard_popup,
            &self.clipboard_history,
            self.native_window,
        ) {
            let Some(native) = popup.native else {
                return;
            };
            if current_foreground_identity() != Some(root_native.identity) {
                return;
            }
            let Some(permit) = self.restart_admission.admit() else {
                return;
            };
            let popup_window = popup.window;
            let view = view.clone();
            let receipt = view.read(cx).mount_receipt();
            let guard = view.read(cx).activation_revision.clone();
            let revision = guard.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
            let worker = guard.clone();
            cx.spawn(async move |_, cx| {
                let _permit = permit;
                cx.background_executor()
                    .spawn(async move {
                        activate_for_secondary_window(
                            &worker,
                            revision,
                            native,
                            Some(root_native.identity),
                        );
                    })
                    .await;
                if !receipt.is_current()
                    || guard.load(Ordering::Acquire) != revision
                    || current_foreground_identity() != Some(native.identity)
                {
                    return;
                }
                let _ = update_kit_window_content(popup_window, &view, cx, |view, window, cx| {
                    if receipt.is_current() && view.popout && !view.rehosting {
                        view.open(window, cx);
                    }
                });
            })
            .detach();
            return;
        }
        let path = {
            #[cfg(windows)]
            {
                self.app_data_root
                    .path()
                    .join(multi_launcher::plugins::clipboard::CLIPBOARD_FILE)
            }
            #[cfg(not(windows))]
            {
                PathBuf::from(multi_launcher::plugins::clipboard::CLIPBOARD_FILE)
            }
        };
        let Some(host) = self.root_window else {
            return;
        };
        let native = self.native_window;
        let root = cx.entity().downgrade();
        let admission = self.restart_admission.clone();
        let visibility = self.root_visibility.clone();
        let view = self.clipboard_history.clone().unwrap_or_else(|| {
            cx.new(|cx| {
                ClipboardHistory::new(window, cx, root, path, admission, visibility, host, native)
            })
        });
        let was_open = view.read(cx).open;
        view.update(cx, |view, cx| {
            if !view.open && !view.is_mounted_in(host.window_id(), false) {
                view.popout = false;
                view.replace_mount(host, native, window, cx);
            }
            view.open(window, cx);
        });
        self.clipboard_history = Some(view.clone());
        panel_stack::ensure(&mut self.panel_stack, Panel::ClipboardDialog);
        self.scroll_to_clipboard(window.window_handle().window_id(), cx);
        if !was_open {
            let popout = multi_launcher::settings::UtilityToolKey::parse("clipboard-history")
                .ok()
                .is_some_and(|key| {
                    self.utility_presentations.preference(key)
                        == Some(multi_launcher::settings::RememberedPresentation::Popout)
                });
            if popout {
                view.update(cx, |view, cx| view.request_rehost(true, cx));
            }
        }
        cx.notify();
    }
    pub(super) fn scroll_to_clipboard(&self, host: WindowId, cx: &App) {
        self.root_scroll
            .scroll_to_top_of_item(self.clipboard_scroll_item(host, cx));
    }
    pub(super) fn clipboard_scroll_item(&self, host: WindowId, cx: &App) -> usize {
        let mut index = self.file_search_scroll_item();
        index += usize::from(
            self.file_search
                .as_ref()
                .is_some_and(|view| view.read(cx).open),
        );
        #[cfg(windows)]
        {
            index += usize::from(
                self.downloads_triage
                    .as_ref()
                    .is_some_and(|view| view.read(cx).open),
            );
        }
        index += usize::from(
            self.plugin_editor
                .as_ref()
                .is_some_and(|view| view.read(cx).open),
        );
        index += usize::from(
            self.activity_view
                .as_ref()
                .is_some_and(|view| view.read(cx).open),
        );
        index += usize::from(
            self.json_view
                .as_ref()
                .is_some_and(|view| view.read(cx).is_mounted_in(host, false)),
        );
        index += usize::from(
            self.xml_view
                .as_ref()
                .is_some_and(|view| view.read(cx).is_mounted_in(host, false)),
        );
        index += usize::from(
            self.regex_view
                .as_ref()
                .is_some_and(|view| view.read(cx).is_mounted_in(host, false)),
        );
        index
    }
}
#[cfg(test)]
mod tests {
    use super::super::Modifiers;
    use super::super::utility_mount::MountOwner;
    use super::{
        EditReceipt, RestartDraft, close_key, current_provider, load, preview,
        same_unreadable_snapshot,
    };
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;
    #[test]
    fn restart_snapshot_detects_exact_draft_and_review_changes() {
        let rows = Arc::new(vec!["duplicate".to_owned(), "duplicate".to_owned()]);
        let entry = EditReceipt {
            version: 3,
            generation: 5,
            index: 1,
            rows,
            instance: 2,
        };
        let edit = RestartDraft {
            edit: Some((entry.clone(), "  猫\ntext ".into())),
            decision: None,
        };
        let mut changed = edit.clone();
        changed.edit.as_mut().unwrap().1.push('!');
        assert_ne!(edit, changed);
        let decision = RestartDraft {
            edit: None,
            decision: Some(entry),
        };
        assert_ne!(edit, decision);
        let mut changed = decision.clone();
        changed.decision.as_mut().unwrap().index = 0;
        assert_ne!(decision, changed);
    }
    #[test]
    fn close_keys_are_exact_and_do_not_capture_modified_escape() {
        assert!(close_key("escape", Modifiers::default()));
        assert!(close_key(
            "w",
            Modifiers {
                control: true,
                ..Modifiers::default()
            }
        ));
        assert!(!close_key(
            "escape",
            Modifiers {
                control: true,
                ..Modifiers::default()
            }
        ));
        assert!(!close_key("w", Modifiers::default()));
    }
    #[test]
    fn rehost_retires_old_mount_without_changing_reviewed_history() {
        let owner = MountOwner::default();
        let old = owner.attach();
        let rows = Arc::new(vec![
            "same".to_owned(),
            "other".to_owned(),
            "same".to_owned(),
        ]);
        let review = EditReceipt {
            version: 4,
            generation: 9,
            index: 2,
            rows: rows.clone(),
            instance: 7,
        };
        let new = owner.attach();
        assert!(!old.is_current());
        assert!(new.is_current());
        assert!(!old.matches(&new));
        assert!(review.current(Some(4), 4, 9, &rows, false, false));
        assert_eq!(review.rows[review.index], "same");
    }
    #[test]
    fn unreadable_retry_preserves_only_exact_unchanged_publication() {
        let rows = Arc::new(vec!["a".to_owned(), "a".to_owned()]);
        assert!(same_unreadable_snapshot(
            5,
            &rows,
            5,
            &rows,
            &["a".into(), "a".into()]
        ));
        assert!(!same_unreadable_snapshot(
            5,
            &rows,
            6,
            &rows,
            &["a".into(), "a".into()]
        ));
        assert!(!same_unreadable_snapshot(5, &rows, 5, &rows, &["a".into()]));
        assert!(!same_unreadable_snapshot(
            5,
            &rows,
            5,
            &Arc::new((*rows).clone()),
            &["a".into(), "a".into()]
        ));
    }
    #[test]
    fn guarded_save_and_remove_preserve_duplicate_occurrences_and_exact_text() {
        use multi_launcher::plugins::clipboard::{GuardedHistoryOutcome, replace_history_guarded};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.json");
        let original = vec![
            "duplicate".to_owned(),
            "middle".to_owned(),
            "duplicate".to_owned(),
            "tail".to_owned(),
        ];
        std::fs::write(&path, serde_json::to_vec(&original).unwrap()).unwrap();
        let mut edited = original.clone();
        edited[2] = "  猫\n café  ".to_owned();
        assert_eq!(
            replace_history_guarded(&path, &original, &edited, &|| true),
            GuardedHistoryOutcome::Updated
        );
        assert_eq!(load(&path).unwrap(), edited);
        let mut removed = edited.clone();
        removed.remove(0);
        assert_eq!(
            replace_history_guarded(&path, &edited, &removed, &|| true),
            GuardedHistoryOutcome::Updated
        );
        assert_eq!(load(&path).unwrap(), ["middle", "  猫\n café  ", "tail"]);
        assert_eq!(
            replace_history_guarded(&path, &original, &[], &|| true),
            GuardedHistoryOutcome::Conflict
        );
        assert_eq!(load(&path).unwrap(), removed);
    }
    #[test]
    fn revoked_final_guard_preserves_exact_history_and_version() {
        use multi_launcher::plugins::clipboard::{
            GuardedHistoryOutcome, clipboard_version, replace_history_guarded,
        };
        use std::sync::atomic::{AtomicUsize, Ordering};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.json");
        let raw = b"[\"old\"]";
        std::fs::write(&path, raw).unwrap();
        let version = clipboard_version();
        let calls = AtomicUsize::new(0);
        let result = replace_history_guarded(&path, &["old".into()], &["new".into()], &|| {
            calls.fetch_add(1, Ordering::SeqCst) < 2
        });
        assert_eq!(result, GuardedHistoryOutcome::Stale);
        assert_eq!(std::fs::read(&path).unwrap(), raw);
        assert_eq!(clipboard_version(), version);
    }
    #[test]
    fn checked_copy_requires_whole_current_history_and_writes_exact_entry() {
        use multi_launcher::clipboard_modify::clipboard::{ClipboardBackend, ClipboardError};
        use multi_launcher::plugins::clipboard::{
            GuardedHistoryOutcome, copy_history_if_unchanged,
        };
        use std::sync::Mutex;
        #[derive(Default)]
        struct Fake(Mutex<Vec<String>>);
        impl ClipboardBackend for Fake {
            fn read_text(&self) -> Result<String, ClipboardError> {
                panic!("copy must not read")
            }
            fn write_text(&self, value: &str) -> Result<(), ClipboardError> {
                self.0.lock().unwrap().push(value.to_owned());
                Ok(())
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.json");
        std::fs::write(&path, "[\"same\",\"猫\\ntext\"]").unwrap();
        let expected = vec!["same".into(), "猫\ntext".into()];
        let backend = Fake::default();
        assert_eq!(
            copy_history_if_unchanged(&path, &expected, 1, &backend, &|| false),
            GuardedHistoryOutcome::Stale
        );
        assert!(backend.0.lock().unwrap().is_empty());
        assert_eq!(
            copy_history_if_unchanged(&path, &expected, 1, &backend, &|| true),
            GuardedHistoryOutcome::Updated
        );
        assert_eq!(&*backend.0.lock().unwrap(), &["猫\ntext"]);
        std::fs::write(&path, "[\"changed\",\"猫\\ntext\"]").unwrap();
        assert_eq!(
            copy_history_if_unchanged(&path, &expected, 1, &backend, &|| true),
            GuardedHistoryOutcome::Conflict
        );
        assert_eq!(backend.0.lock().unwrap().len(), 1);
        #[cfg(windows)]
        {
            let nul = vec!["before\0after".to_owned()];
            std::fs::write(&path, "[\"before\\u0000after\"]").unwrap();
            assert_eq!(
                copy_history_if_unchanged(&path, &nul, 0, &backend, &|| true),
                GuardedHistoryOutcome::Failed
            );
            assert_eq!(backend.0.lock().unwrap().len(), 1);
        }
    }
    #[test]
    fn exact_edit_receipt_rejects_stale_version_and_replaced_occurrence() {
        let rows = Arc::new(vec!["same".to_owned(), "same".to_owned()]);
        let edit = EditReceipt {
            instance: 0,
            version: 4,
            generation: 7,
            index: 1,
            rows: rows.clone(),
        };
        assert!(edit.current(Some(4), 4, 7, &rows, false, false));
        assert!(!edit.current(Some(4), 5, 7, &rows, false, false));
        assert!(!edit.current(Some(4), 4, 7, &rows, true, false));
        assert!(!edit.current(Some(4), 4, 7, &rows, false, true));
        assert!(!edit.current(Some(4), 4, 8, &Arc::new((*rows).clone()), false, false));
    }
    #[test]
    fn opener_requires_exact_action_query_and_live_capability() {
        let action = multi_launcher::plugins::clipboard::bare_history_opener("Cb").unwrap();
        assert!(current_provider("Cb", &action, None, None));
        assert!(!current_provider("cb list", &action, None, None));
        let mut changed = action.clone();
        changed.label.push('!');
        assert!(!current_provider("cb", &changed, None, None));
        assert!(!current_provider(
            "cb",
            &action,
            Some(&HashSet::new()),
            None
        ));
        let caps = HashMap::from([("clipboard".to_owned(), Vec::new())]);
        assert!(!current_provider("cb", &action, None, Some(&caps)));
    }
    #[test]
    fn previews_are_stock_unicode_and_full_data_is_not_changed() {
        let value = format!("{}\nend", "é".repeat(160));
        assert_eq!(preview(&value), format!("{}…", "é".repeat(160)));
        assert!(value.ends_with("\nend"));
    }
    #[test]
    fn typed_load_preserves_duplicates_and_reports_corruption() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.json");
        assert!(load(&path).unwrap().is_empty());
        std::fs::write(&path, "[\"a\",\"a\",\"猫\"]").unwrap();
        assert_eq!(load(&path).unwrap(), ["a", "a", "猫"]);
        std::fs::write(&path, "{bad").unwrap();
        assert!(load(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"{bad");
        let secret = "SYNTHETIC_SECRET_4291";
        let bytes = format!("\"{secret}\"");
        std::fs::write(&path, &bytes).unwrap();
        assert!(!load(&path).unwrap_err().contains(secret));
        assert_eq!(std::fs::read(&path).unwrap(), bytes.as_bytes());
    }
}
