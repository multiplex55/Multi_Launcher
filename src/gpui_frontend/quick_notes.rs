//! Retained Quick Notes, using the canonical note cache and checked writer.
use super::*;
use multi_launcher::plugins::note::{self, Note, NotePlugin, QuickNoteDraft, QuickNoteWrite};

pub(super) const WAIT: &str = "Save or Cancel the Quick Notes draft before closing.";

fn notes_owner_needed(direct: bool, omni_search: bool, include_notes: bool) -> bool {
    direct || (omni_search && include_notes)
}

fn notes_wake_reprojects(query: &str, omni_allowed: bool) -> bool {
    let effective = multi_launcher::common::query::split_action_filters(query).0;
    let first = effective.split_whitespace().next().unwrap_or("");
    first.eq_ignore_ascii_case("note")
        || first.eq_ignore_ascii_case("notes")
        || (omni_allowed && super::omni_search::is_omni_query(query))
}

pub(super) fn current_provider(
    query: &str,
    action: &Action,
    enabled: Option<&HashSet<String>>,
    capabilities: Option<&HashMap<String, Vec<String>>>,
) -> bool {
    let effective = multi_launcher::common::query::split_action_filters(query).0;
    (effective.trim().eq_ignore_ascii_case("note")
        || effective.trim().eq_ignore_ascii_case("notes"))
        && enabled.is_none_or(|set| set.contains("notes"))
        && multi_launcher::settings::provider_search_capability_enabled(capabilities, "notes")
        && *action == note::quick_notes_action()
        && super::search_providers::search_routed(
            [&super::search_providers::QuickNotesBare::default()
                as &dyn multi_launcher::plugin::Plugin],
            query,
            enabled,
            capabilities,
        )
        .into_iter()
        .any(|(name, row)| name == "notes" && row == *action)
}

#[derive(Clone)]
struct Row {
    note: Note,
    search: String,
    preview: String,
}
struct Draft {
    original: Option<Note>,
    baseline: String,
}

fn draft_blocks(pending: bool, baseline: Option<&str>, value: &str) -> bool {
    pending || baseline.is_some_and(|baseline| value != baseline)
}

fn settle_save(
    draft: &mut Option<Draft>,
    result: anyhow::Result<QuickNoteWrite>,
) -> (bool, Option<String>) {
    match result {
        Ok(QuickNoteWrite::Published(_)) => {
            *draft = None;
            (true, None)
        }
        Ok(QuickNoteWrite::Committed { cache_error, .. }) => {
            *draft = None;
            (
                false,
                Some(format!(
                    "Saved, but the notes list could not refresh: {cache_error:#}"
                )),
            )
        }
        Err(error) => (false, Some(format!("Could not save note: {error:#}"))),
    }
}

fn row_receipt_current(
    mount: &utility_mount::MountReceipt,
    current: &utility_mount::MountReceipt,
    issued_version: u64,
    current_version: u64,
    issued_filter: &str,
    current_filter: &str,
) -> bool {
    mount.is_current()
        && mount.matches(current)
        && issued_version == current_version
        && issued_filter == current_filter
}

pub(super) struct QuickNotesView {
    pub(super) open: bool,
    root: WeakEntity<RootWindow>,
    host: AnyWindowHandle,
    native: Option<NativeWindow>,
    visibility: RootVisibility,
    admission: Arc<RestartAdmission>,
    owner: utility_mount::MountOwner,
    mount: utility_mount::MountReceipt,
    focus: FocusHandle,
    filter: Entity<InputState>,
    body: Entity<TextareaState>,
    _subscriptions: Vec<Subscription>,
    note_owner: Arc<Mutex<Option<Arc<NotePlugin>>>>,
    reload_error: Arc<Mutex<Option<String>>>,
    notes_wake: Arc<HashWakeSignal>,
    loading: bool,
    pending: bool,
    reload_requested: bool,
    focus_guard: Option<Subscription>,
    activation_guard: Option<Subscription>,
    rows: Arc<Vec<Row>>,
    filtered: Arc<Vec<usize>>,
    version: u64,
    draft: Option<Draft>,
    scroll: gpui_kit::UniformListScrollHandle,
    message: Option<String>,
}

impl QuickNotesView {
    fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        root: WeakEntity<RootWindow>,
        native: Option<NativeWindow>,
        visibility: RootVisibility,
        admission: Arc<RestartAdmission>,
        note_owner: Arc<Mutex<Option<Arc<NotePlugin>>>>,
        reload_error: Arc<Mutex<Option<String>>>,
        notes_wake: Arc<HashWakeSignal>,
    ) -> Self {
        let owner = utility_mount::MountOwner::default();
        let mount = owner.attach();
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter notes"));
        let body = cx.new(|cx| TextareaState::new(window, cx).auto_grow(5, 12));
        let subscriptions = vec![
            cx.subscribe(&filter, |view: &mut Self, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    view.reproject(cx);
                    cx.notify();
                }
            }),
            cx.subscribe(&body, |_: &mut Self, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
        ];
        let mut view = Self {
            open: true,
            root,
            host: window.window_handle(),
            native,
            visibility,
            admission,
            owner,
            mount,
            focus: cx.focus_handle(),
            filter,
            body,
            _subscriptions: subscriptions,
            note_owner,
            reload_error,
            notes_wake,
            loading: false,
            pending: false,
            reload_requested: false,
            focus_guard: None,
            activation_guard: None,
            rows: Arc::new(Vec::new()),
            filtered: Arc::new(Vec::new()),
            version: 0,
            draft: None,
            scroll: gpui_kit::UniformListScrollHandle::default(),
            message: None,
        };
        view.refresh(cx);
        view
    }
    pub(super) fn blocks_close(&self, cx: &App) -> bool {
        draft_blocks(
            self.pending,
            self.draft.as_ref().map(|draft| draft.baseline.as_str()),
            self.body.read(cx).value().as_str(),
        )
    }
    fn reproject(&mut self, cx: &App) {
        let filter = self.filter.read(cx).value().to_lowercase();
        self.filtered = Arc::new(
            self.rows
                .iter()
                .enumerate()
                .filter_map(|(index, row)| {
                    (filter.is_empty() || row.search.contains(&filter)).then_some(index)
                })
                .collect(),
        );
    }
    pub(super) fn open_receipt_current(
        &self,
        mount: &utility_mount::MountReceipt,
        version: u64,
        filter: &str,
        note: &Note,
        cx: &App,
    ) -> bool {
        self.open
            && self.draft.is_none()
            && !self.pending
            && row_receipt_current(
                mount,
                &self.mount,
                version,
                self.version,
                filter,
                self.filter.read(cx).value().as_str(),
            )
            && self.rows.iter().any(|row| {
                &row.note == note
                    && (filter.is_empty() || row.search.contains(&filter.to_lowercase()))
            })
    }
    pub(super) fn handle_escape(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        held: bool,
    ) -> bool {
        if !self.open || !self.focus.contains_focused(window, cx) || !self.current(window, cx) {
            return false;
        }
        if !held && !self.pending {
            if self.draft.is_some() {
                if self.blocks_close(cx) {
                    self.message = Some(WAIT.into());
                    cx.notify();
                    return true;
                }
                self.draft = None;
                self.message = None;
                if self.reload_requested {
                    self.reload_requested = false;
                    self.refresh(cx);
                }
                self.filter.focus_handle(cx).focus(window, cx);
            } else {
                self.close(window, cx);
            }
            cx.notify();
        }
        true
    }
    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.current(window, cx) {
            return;
        }
        if self.blocks_close(cx) {
            self.message = Some(WAIT.into());
            cx.notify();
            return;
        }
        let owns = self.focus.contains_focused(window, cx);
        let focus = self.focus.clone();
        let root = self.root.clone();
        let entity = cx.entity_id();
        let native = self.native;
        let revision = self.visibility.request_revision.load(Ordering::Acquire);
        self.open = false;
        self.owner.retire();
        self.draft = None;
        if owns {
            window.defer(cx, move |window, app| {
                let _ =
                    root.update(app, |root, cx| {
                        let valid =
                            root.quick_notes_view.as_ref().is_some_and(|view| {
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
                        if valid {
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
    pub(super) fn refresh(&mut self, cx: &mut Context<Self>) {
        if !self.open {
            return;
        }
        if self.loading || self.draft.is_some() {
            self.reload_requested = true;
            return;
        }
        if self
            .note_owner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_none()
        {
            self.message = Some("Loading notes…".into());
            cx.notify();
            return;
        }
        self.loading = true;
        let mount = self.mount.clone();
        let task = cx.background_executor().spawn(async move {
            note::note_cache_snapshot_with_version().map(|(version, notes)| {
                (
                    version,
                    notes
                        .into_iter()
                        .map(|note| Row {
                            search: note::quick_note_search_text(&note),
                            preview: note::quick_note_preview(&note.content),
                            note,
                        })
                        .collect::<Vec<_>>(),
                )
            })
        });
        cx.spawn(async move |weak, cx| {
            let result = task.await;
            let _ = weak.update(cx, |view, cx| {
                view.loading = false;
                if !mount.is_current() || !mount.matches(&view.mount) || !view.open {
                    if view.open {
                        view.refresh(cx);
                    }
                    return;
                }
                if view.draft.is_some() {
                    view.reload_requested = true;
                    return;
                }
                match result {
                    Ok((version, rows)) => {
                        view.version = version;
                        view.rows = Arc::new(rows);
                        view.reproject(cx);
                        view.message = view
                            .reload_error
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .as_ref()
                            .map(|error| format!("Could not refresh notes: {error}"));
                    }
                    Err(error) => view.message = Some(format!("Could not load notes: {error:#}")),
                }
                if std::mem::take(&mut view.reload_requested) {
                    view.refresh(cx);
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn begin_edit(&mut self, original: Option<Note>, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending || self.draft.is_some() || !self.current(window, cx) {
            return;
        }
        let baseline = original
            .as_ref()
            .map_or_else(String::new, |note| note.content.clone());
        self.body
            .update(cx, |body, cx| body.set_value(baseline.clone(), window, cx));
        self.draft = Some(Draft { original, baseline });
        self.body.focus_handle(cx).focus(window, cx);
        cx.notify();
    }
    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending || !self.current(window, cx) {
            return;
        }
        let Some(draft) = &self.draft else {
            return;
        };
        let original = draft.original.clone();
        let content = self.body.read(cx).value().to_string();
        let Some(permit) = self.admission.admit() else {
            return;
        };
        self.body.focus_handle(cx).focus(window, cx);
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
        let mount = self.mount.clone();
        let admission = self.admission.clone();
        let revision = self.visibility.request_revision.load(Ordering::Acquire);
        let visibility = self.visibility.clone();
        let native = self.native;
        let task = cx.background_executor().spawn(async move {
            let draft = match original.as_ref() {
                Some(original) => QuickNoteDraft::Edit {
                    original,
                    content: &content,
                },
                None => QuickNoteDraft::New(&content),
            };
            let result = note::write_quick_note_if(draft, || {
                mount.is_current()
                    && !admission.is_fenced()
                    && json_utility::native_host_current(&visibility, revision, native, false)
                    && lease.load(Ordering::Acquire)
                    && native.is_some_and(|native| {
                        current_foreground_identity() == Some(native.identity)
                    })
            });
            (permit, result)
        });
        let receipt = self.mount.clone();
        let host = self.host;
        cx.spawn(async move |weak, cx| {
            let (permit, result) = task.await;
            let committed = weak
                .update(cx, |view, cx| {
                    if !receipt.is_current() || !receipt.matches(&view.mount) {
                        return false;
                    }
                    view.pending = false;
                    view.focus_guard = None;
                    view.activation_guard = None;
                    view.body
                        .update(cx, |body, cx| body.set_readonly(false, cx));
                    let (refresh, message) = settle_save(&mut view.draft, result);
                    view.message = message;
                    if refresh {
                        view.notes_wake.notify();
                        view.refresh(cx);
                    }
                    cx.notify();
                    view.draft.is_none()
                })
                .unwrap_or(false);
            if committed {
                let _ = host.update(cx, |_, window, cx| {
                    let _ = weak.update(cx, |view, cx| {
                        if receipt.is_current()
                            && receipt.matches(&view.mount)
                            && view.draft.is_none()
                            && view.body.focus_handle(cx).is_focused(window)
                            && view.current(window, cx)
                        {
                            view.filter.focus_handle(cx).focus(window, cx);
                        }
                    });
                });
            }
            drop(permit);
        })
        .detach();
        cx.notify();
    }
}

impl Drop for QuickNotesView {
    fn drop(&mut self) {
        self.owner.retire();
    }
}

impl Render for QuickNotesView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut panel = div()
            .id("quick-notes-pane")
            .key_context("QuickNotesPane")
            .track_focus(&self.focus)
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
            )
            .flex()
            .flex_col()
            .gap_2()
            .w_full()
            .p_3()
            .child(Label::new("Quick Notes"));
        if let Some(message) = &self.message {
            panel = panel.child(Label::new(message.clone()));
        }
        if self.loading {
            panel = panel.child(Label::new("Loading notes…"));
        }
        if self.draft.is_some() {
            panel = panel
                .child(Textarea::new(&self.body).aria_label("Note text"))
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            Button::new("quick-notes-save")
                                .label("Save")
                                .disabled(self.pending)
                                .on_click(cx.listener(|view, _, window, cx| view.save(window, cx))),
                        )
                        .child(
                            Button::new("quick-notes-cancel")
                                .label("Cancel")
                                .disabled(self.pending)
                                .on_click(cx.listener(|view, _, window, cx| {
                                    if view.current(window, cx) && !view.pending {
                                        view.draft = None;
                                        view.message = None;
                                        if view.reload_requested {
                                            view.reload_requested = false;
                                            view.refresh(cx);
                                        }
                                        view.filter.focus_handle(cx).focus(window, cx);
                                        cx.notify();
                                    }
                                })),
                        ),
                );
        } else {
            let rows = self.rows.clone();
            let filtered = self.filtered.clone();
            let count = filtered.len();
            let weak = cx.entity().downgrade();
            let issuing_filter = self.filter.read(cx).value().to_string();
            let mount = self.mount.clone();
            let version = self.version;
            panel = panel
                .child(Input::new(&self.filter).aria_label("Filter notes"))
                .child(Button::new("quick-notes-add").label("Add note").on_click(
                    cx.listener(|view, _, window, cx| view.begin_edit(None, window, cx)),
                ));
            let list = gpui_kit::uniform_list("quick-notes-list", count, move |range, _, _| {
                range
                    .filter_map(|index| {
                        let row = rows.get(*filtered.get(index)?)?.clone();
                        let expected = row.note.clone();
                        let owner = weak.clone();
                        let receipt = mount.clone();
                        let issuing_filter = issuing_filter.clone();
                        let open_owner = owner.clone();
                        let open_note = expected.clone();
                        let open_receipt = receipt.clone();
                        let open_filter = issuing_filter.clone();
                        let title = row.note.alias.as_ref().unwrap_or(&row.note.title).clone();
                        Some(
                            div()
                                .h(px(90.0))
                                .w_full()
                                .flex()
                                .flex_col()
                                .child(
                                    Button::new(SharedString::from(format!("quick-note-{index}")))
                                        .label(title)
                                        .on_click(move |_, window, cx| {
                                            let _ = owner.update(cx, |view, cx| {
                                                if row_receipt_current(
                                                    &receipt,
                                                    &view.mount,
                                                    version,
                                                    view.version,
                                                    &issuing_filter,
                                                    view.filter.read(cx).value().as_str(),
                                                ) && view
                                                    .rows
                                                    .iter()
                                                    .any(|row| row.note == expected)
                                                {
                                                    view.begin_edit(
                                                        Some(expected.clone()),
                                                        window,
                                                        cx,
                                                    );
                                                }
                                            });
                                        }),
                                )
                                .child(
                                    Label::new(row.preview).id(SharedString::from(format!(
                                        "quick-note-preview-{index}"
                                    ))),
                                )
                                .child(
                                    Button::new(SharedString::from(format!(
                                        "quick-note-open-{index}"
                                    )))
                                    .label("Open")
                                    .on_click(
                                        move |_, window, cx| {
                                            let owner = open_owner.clone();
                                            let note = open_note.clone();
                                            let mount = open_receipt.clone();
                                            let filter = open_filter.clone();
                                            let _ = owner.update(cx, |view, cx| {
                                                if !view.open_receipt_current(
                                                    &mount, version, &filter, &note, cx,
                                                ) || !view.current(window, cx)
                                                {
                                                    return;
                                                }
                                                let root = view.root.clone();
                                                window.defer(cx, move |window, app| {
                                                    let _ = root.update(app, |root, cx| {
                                                        root.open_checked_note(
                                                            note,
                                                            version,
                                                            super::note_editor::OpenOrigin::Quick {
                                                                mount,
                                                                version,
                                                                filter,
                                                            },
                                                            window,
                                                            cx,
                                                        );
                                                    });
                                                });
                                            });
                                        },
                                    ),
                                )
                                .into_any_element(),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .track_scroll(&self.scroll)
            .h(px(300.0))
            .w_full();
            panel = panel.child(list);
        }
        panel.child(
            Button::new("quick-notes-close")
                .label("Close")
                .on_click(cx.listener(|view, _, window, cx| view.close(window, cx))),
        )
    }
}

impl RootWindow {
    pub(super) fn start_notes_owner(&mut self, cx: &mut Context<Self>) {
        let direct = self
            .enabled_plugins
            .as_ref()
            .is_none_or(|enabled| enabled.contains("notes"));
        let omni = self
            .enabled_plugins
            .as_ref()
            .is_none_or(|enabled| enabled.contains("omni_search"));
        if !notes_owner_needed(
            direct,
            omni,
            self.search_catalog.providers.omni.settings().include_notes,
        ) {
            return;
        }
        let owner = self.search_catalog.providers.notes_owner();
        let wake = self.search_catalog.providers.notes_wake.clone();
        let errors = self.search_catalog.providers.notes_error.clone();
        let settings = self.search_catalog.note_provider_settings.clone();
        let admission = self.restart_admission.clone();
        let worker_wake = wake.clone();
        let task = cx.background_executor().spawn(async move {
            if admission.is_fenced() {
                return;
            }
            let callback_wake = worker_wake.clone();
            let mut plugin = NotePlugin::new_with_wake(move |result| {
                *errors
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = result.err();
                callback_wake.notify();
            });
            if let Some(settings) = settings {
                multi_launcher::plugin::Plugin::apply_settings(&mut plugin, &settings);
            }
            if admission.is_fenced() {
                return;
            }
            let plugin = Arc::new(plugin);
            *owner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(plugin);
            worker_wake.notify();
        });
        cx.spawn(async move |_, _| {
            task.await;
        })
        .detach();
        cx.spawn(async move |weak, cx| {
            while wake.wait().await {
                if weak
                    .update(cx, |root, cx| {
                        if root.restart_admission.is_fenced() {
                            return;
                        }
                        if let Some(view) = root.quick_notes_view.clone() {
                            view.update(cx, |view, cx| view.refresh(cx));
                        }
                        let query = root.query.read(cx).value().to_string();
                        let omni = super::omni_search::notes_enabled(
                            root.enabled_plugins.as_ref(),
                            root.enabled_capabilities.as_ref(),
                            root.search_catalog.providers.omni.settings().include_notes,
                        );
                        if notes_wake_reprojects(&query, omni) {
                            if !(omni
                                && super::omni_search::is_omni_query(&query)
                                && root.external_launch.defer_notes_wake_for_omni())
                            {
                                root.refresh_launcher_results_with_viewport(query, false, cx);
                            }
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    pub(super) fn open_quick_notes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.restart_admission.is_fenced()
            || window.has_active_dialog(cx)
            || !window.is_window_active()
            || !self.external_owner_is_current(window.focused(cx).as_ref(), window, cx)
            || !self
                .native_window
                .is_some_and(|native| current_foreground_identity() == Some(native.identity))
        {
            return;
        }
        let view = if let Some(view) = &self.quick_notes_view {
            let view = view.clone();
            view.update(cx, |view, cx| {
                if !view.open {
                    view.open = true;
                    view.mount = view.owner.attach();
                    view.refresh(cx);
                }
                if view.draft.is_some() {
                    view.body.focus_handle(cx).focus(window, cx);
                } else {
                    view.filter.focus_handle(cx).focus(window, cx);
                }
            });
            view
        } else {
            let root = cx.entity().downgrade();
            let native = self.native_window;
            let visibility = self.root_visibility.clone();
            let admission = self.restart_admission.clone();
            let note_owner = self.search_catalog.providers.notes_owner();
            let reload_error = self.search_catalog.providers.notes_error.clone();
            let notes_wake = self.search_catalog.providers.notes_wake.clone();
            let view = cx.new(|cx| {
                QuickNotesView::new(
                    window,
                    cx,
                    root,
                    native,
                    visibility,
                    admission,
                    note_owner,
                    reload_error,
                    notes_wake,
                )
            });
            view.update(cx, |view, cx| {
                view.filter.focus_handle(cx).focus(window, cx)
            });
            self.quick_notes_view = Some(view.clone());
            view
        };
        let id = window.window_handle().window_id();
        let history = self
            .clipboard_history
            .as_ref()
            .is_some_and(|view| view.read(cx).is_mounted_in(id, false));
        let modify = self
            .clipboard_modify_view
            .as_ref()
            .is_some_and(|view| view.read(cx).is_mounted_in(id, false));
        let snippets = self
            .snippets_view
            .as_ref()
            .is_some_and(|view| view.read(cx).open);
        let fill = self
            .snippet_fill
            .as_ref()
            .is_some_and(|view| view.read(cx).open);
        let tempfile = self
            .tempfile_view
            .as_ref()
            .is_some_and(|view| view.read(cx).open);
        let folder = self
            .folder_size_view
            .as_ref()
            .is_some_and(|view| view.read(cx).is_mounted_in(id, false));
        let lock = self
            .file_lock_view
            .as_ref()
            .is_some_and(|view| view.read(cx).is_mounted_in(id, false));
        let duplicates = self
            .duplicate_finder_view
            .as_ref()
            .is_some_and(|view| view.read(cx).open);
        let _ = view;
        self.root_scroll.scroll_to_top_of_item(
            self.clipboard_scroll_item(id, cx)
                + usize::from(history)
                + usize::from(modify)
                + usize::from(snippets)
                + usize::from(fill)
                + usize::from(tempfile)
                + usize::from(folder)
                + usize::from(lock)
                + usize::from(duplicates),
        );
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{
        current_provider, draft_blocks, notes_owner_needed, notes_wake_reprojects,
        row_receipt_current,
    };
    use std::collections::{HashMap, HashSet};

    #[test]
    fn bare_notes_admission_rejects_typed_disabled_and_forged_rows() {
        let action = multi_launcher::plugins::note::quick_notes_action();
        assert!(current_provider("NoTeS", &action, None, None));
        let routed = super::super::search_providers::search_routed(
            [&super::super::search_providers::QuickNotesBare::default()
                as &dyn multi_launcher::plugin::Plugin],
            "kind:note note",
            None,
            None,
        );
        assert_eq!(routed.len(), 1);
        assert!(current_provider("kind:note note", &routed[0].1, None, None));
        assert!(!current_provider("note list", &action, None, None));
        assert!(!current_provider(
            "note",
            &action,
            Some(&HashSet::new()),
            None
        ));
        assert!(!current_provider(
            "note",
            &action,
            None,
            Some(&HashMap::from([("notes".into(), vec![])]))
        ));
        let mut forged = action;
        forged.args = Some("other".into());
        assert!(!current_provider("note", &forged, None, None));
    }

    #[test]
    fn omni_notes_owner_and_wake_are_independent_of_direct_gate() {
        assert!(notes_owner_needed(false, true, true));
        let enabled = HashSet::from(["omni_search".to_owned()]);
        let initially_disabled = HashMap::from([("omni_search".to_owned(), Vec::new())]);
        assert!(!super::super::omni_search::notes_enabled(
            Some(&enabled),
            Some(&initially_disabled),
            true
        ));
        assert!(notes_owner_needed(
            false,
            enabled.contains("omni_search"),
            true
        ));
        assert!(super::super::omni_search::notes_enabled(
            Some(&enabled),
            None,
            true
        ));
        assert!(!notes_owner_needed(false, true, false));
        assert!(!notes_owner_needed(false, false, true));
        assert!(notes_owner_needed(true, false, false));
        assert!(notes_wake_reprojects("kind:omni o missing", true));
        assert!(!notes_wake_reprojects("o missing", false));
        assert!(!notes_wake_reprojects("= 2+2", true));
    }

    #[test]
    fn draft_and_row_receipts_reject_pending_dirty_and_stale() {
        assert!(!draft_blocks(false, Some("saved"), "saved"));
        assert!(draft_blocks(false, Some("saved"), "changed"));
        assert!(draft_blocks(true, Some("saved"), "saved"));
        let owner = super::super::utility_mount::MountOwner::default();
        let mount = owner.attach();
        assert!(row_receipt_current(&mount, &mount, 1, 1, "a", "a"));
        assert!(!row_receipt_current(&mount, &mount, 1, 2, "a", "a"));
        assert!(!row_receipt_current(&mount, &mount, 1, 1, "a", "b"));
        owner.retire();
        assert!(!row_receipt_current(&mount, &mount, 1, 1, "a", "a"));
    }

    #[test]
    fn save_settlement_keeps_retry_draft_but_retires_committed_draft() {
        let mut draft = Some(super::Draft {
            original: None,
            baseline: String::new(),
        });
        let (refresh, message) = super::settle_save(&mut draft, Err(anyhow::anyhow!("conflict")));
        assert!(!refresh && message.is_some() && draft.is_some());
        let note = multi_launcher::plugins::note::Note {
            title: "Title".into(),
            path: "title.md".into(),
            content: "# Title".into(),
            tags: vec![],
            links: vec![],
            slug: "title".into(),
            alias: None,
            aliases: vec![],
            entity_refs: vec![],
        };
        let (refresh, message) = super::settle_save(
            &mut draft,
            Ok(multi_launcher::plugins::note::QuickNoteWrite::Committed {
                note,
                cache_error: anyhow::anyhow!("cache"),
            }),
        );
        assert!(!refresh && message.is_some() && draft.is_none());
    }
}
