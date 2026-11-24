use super::*;

const ACTIONS_FILE: &str = "actions.json";
type CatalogAction = multi_launcher::actions::Action;

fn catalog_filter(filter: &str) -> String {
    if filter.trim().is_empty() {
        String::new()
    } else {
        filter.to_lowercase()
    }
}

fn catalog_matches(action: &CatalogAction, query: &str) -> bool {
    query.is_empty()
        || action.label.to_lowercase().contains(query)
        || action.desc.to_lowercase().contains(query)
        || action.action.to_lowercase().contains(query)
}

#[derive(Default)]
struct CatalogReads {
    revision: u64,
    pending: Option<u64>,
}

impl CatalogReads {
    fn invalidate(&mut self) {
        self.revision = self.revision.wrapping_add(1).max(1);
        self.pending = None;
    }

    fn begin(&mut self) -> u64 {
        self.invalidate();
        self.pending = Some(self.revision);
        self.revision
    }

    fn finish(&mut self, receipt: u64) -> bool {
        if self.pending != Some(receipt) {
            return false;
        }
        self.pending = None;
        true
    }
}

pub(super) const APP_WRITE_CLOSE_STATUS: &str =
    "Wait for the Add App save to finish before closing Multi Launcher.";
pub(super) const APP_WRITE_RESTART_STATUS: &str =
    "Wait for the Add App save to finish before restarting in egui.";
pub(super) const APP_DRAFT_RESTART_STATUS: &str =
    "Save or Cancel the Add App draft before restarting in egui.";

fn clear_app_diagnostic(status: &mut Option<String>) {
    if matches!(
        status.as_deref(),
        Some(APP_WRITE_CLOSE_STATUS | APP_WRITE_RESTART_STATUS | APP_DRAFT_RESTART_STATUS)
    ) {
        *status = None;
    }
}

pub(super) struct AppEditorState {
    pub(super) open: bool,
    draft: Option<AddAppDraft>,
    filter: Option<Entity<InputState>>,
    filter_subscription: Option<Subscription>,
    catalog: Arc<[CatalogAction]>,
    catalog_error: Option<String>,
    reads: CatalogReads,
    catalog_scroll: ScrollHandle,
    status: Option<String>,
    writes: AddAppWrites,
    owner_focus: FocusHandle,
    new_focus: FocusHandle,
    arguments_focus: FocusHandle,
    return_focus: Option<FocusHandle>,
    return_scroll: Option<Point<Pixels>>,
}

#[derive(Default)]
struct AddAppWrites {
    pending: Option<u64>,
    next: u64,
}

impl AddAppWrites {
    fn begin(&mut self) -> Option<u64> {
        if self.pending.is_some() {
            return None;
        }
        self.next = self.next.wrapping_add(1).max(1);
        self.pending = Some(self.next);
        self.pending
    }

    fn finish(&mut self, receipt: u64) -> bool {
        if self.pending != Some(receipt) {
            return false;
        }
        self.pending = None;
        true
    }
}

struct AddAppDraft {
    label: Entity<InputState>,
    description: Entity<InputState>,
    path: Entity<InputState>,
    args: Entity<InputState>,
    show_args: bool,
    scroll: ScrollHandle,
    _focus_subscriptions: Vec<Subscription>,
}

pub(super) struct AddAppSnapshot {
    pub(super) label: String,
    pub(super) description: String,
    pub(super) path: String,
    pub(super) show_args: bool,
    pub(super) args: String,
}

impl AppEditorState {
    pub(super) fn new(cx: &mut Context<RootWindow>) -> Self {
        Self {
            open: false,
            draft: None,
            filter: None,
            filter_subscription: None,
            catalog: Arc::from([]),
            catalog_error: None,
            reads: CatalogReads::default(),
            catalog_scroll: ScrollHandle::new(),
            status: None,
            writes: AddAppWrites::default(),
            owner_focus: cx.focus_handle(),
            new_focus: cx.focus_handle(),
            arguments_focus: cx.focus_handle(),
            return_focus: None,
            return_scroll: None,
        }
    }

    pub(super) fn restart_blocker(&self, cx: &App) -> Option<&'static str> {
        if self.write_pending() {
            return Some(APP_WRITE_RESTART_STATUS);
        }
        let draft = self.draft.as_ref()?;
        (draft.show_args
            || !draft.label.read(cx).value().is_empty()
            || !draft.description.read(cx).value().is_empty()
            || !draft.path.read(cx).value().is_empty()
            || !draft.args.read(cx).value().is_empty())
        .then_some(APP_DRAFT_RESTART_STATUS)
    }

    fn is_current_draft(&self, id: EntityId) -> bool {
        self.open
            && !self.write_pending()
            && self
                .draft
                .as_ref()
                .is_some_and(|draft| draft.label.entity_id() == id)
    }

    pub(super) fn write_pending(&self) -> bool {
        self.writes.pending.is_some()
    }

    fn begin_write(&mut self) -> Option<u64> {
        if self.draft.is_none() {
            return None;
        }
        let receipt = self.writes.begin()?;
        self.status = Some("Saving Add App…".to_owned());
        Some(receipt)
    }

    fn finish_write(&mut self, receipt: u64, result: Result<Vec<CatalogAction>, String>) -> bool {
        if !self.writes.finish(receipt) {
            return false;
        }
        match result {
            Ok(rows) => {
                self.reads.invalidate();
                self.catalog = rows.into();
                self.catalog_error = None;
                self.draft = None;
                self.status = None;
            }
            Err(error) => self.status = Some(error),
        }
        true
    }

    pub(super) fn snapshot(&self, cx: &App) -> Option<AddAppSnapshot> {
        let draft = self.draft.as_ref()?;
        Some(AddAppSnapshot {
            label: draft.label.read(cx).value().to_string(),
            description: draft.description.read(cx).value().to_string(),
            path: draft.path.read(cx).value().to_string(),
            show_args: draft.show_args,
            args: draft.args.read(cx).value().to_string(),
        })
    }
}

impl RootWindow {
    fn app_editor_scroll_item(&self) -> usize {
        self.quick_help_scroll_item()
            + usize::from(self.quick_help.open)
            + usize::from(self.result_favorites.is_open())
    }

    /// The menu route supplies its live origin before dismissing the menu.
    pub(super) fn open_app_editor(
        &mut self,
        origin: Option<FocusHandle>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.restart_admission.is_fenced() {
            return;
        }
        if !self.app_editor.open {
            self.app_editor.return_focus =
                origin.filter(|focus| self.focus_handle.contains(focus, window));
            self.app_editor.return_scroll = Some(self.root_scroll.offset());
            self.app_editor.open = true;
            if self.app_editor.filter.is_none() {
                let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter apps"));
                let subscription = cx.subscribe(&filter, |root, source, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change)
                        && root
                            .app_editor
                            .filter
                            .as_ref()
                            .is_some_and(|filter| *filter == source)
                    {
                        cx.notify();
                    }
                });
                self.app_editor.filter = Some(filter);
                self.app_editor.filter_subscription = Some(subscription);
            }
            self.refresh_app_catalog(window, cx);
        }
        self.root_scroll
            .scroll_to_top_of_item(self.app_editor_scroll_item());
        if let Some(draft) = &self.app_editor.draft {
            draft.label.focus_handle(cx).focus(window, cx);
        } else {
            self.app_editor.new_focus.focus(window, cx);
        }
        cx.notify();
    }

    fn refresh_app_catalog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.app_editor.open {
            return;
        }
        let receipt = self.app_editor.reads.begin();
        let task = cx.background_executor().spawn(async move {
            multi_launcher::actions::load_actions(ACTIONS_FILE)
                .map_err(|error| format!("Could not load app catalog: {error:#}"))
        });
        let root = cx.entity().downgrade();
        let window_handle = window.window_handle();
        cx.spawn(async move |_, cx| {
            let result = task.await;
            let _ = window_handle.update(cx, |_, _, app| {
                let _ = root.update(app, |root, cx| {
                    if root.app_editor.open && root.app_editor.reads.finish(receipt) {
                        match result {
                            Ok(rows) => {
                                root.app_editor.catalog = rows.into();
                                root.app_editor.catalog_error = None;
                            }
                            Err(error) => root.app_editor.catalog_error = Some(error),
                        }
                        cx.notify();
                    }
                });
            });
        })
        .detach();
    }

    fn new_app_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.app_editor.open
            || self.restart_admission.is_fenced()
            || self.app_editor.draft.is_some()
        {
            return;
        }
        let label = cx.new(|cx| InputState::new(window, cx));
        let description = cx.new(|cx| InputState::new(window, cx));
        let path = cx.new(|cx| InputState::new(window, cx));
        let args = cx.new(|cx| InputState::new(window, cx));
        let scroll = ScrollHandle::new();
        let draft_id = label.entity_id();
        let handles = [
            label.focus_handle(cx),
            description.focus_handle(cx),
            path.focus_handle(cx),
            self.app_editor.arguments_focus.clone(),
            args.focus_handle(cx),
        ];
        let mut subscriptions: Vec<Subscription> = handles
            .iter()
            .enumerate()
            .map(|(index, handle)| {
                cx.on_focus(handle, window, move |root, window, cx| {
                    if window.is_window_active() && root.app_editor.is_current_draft(draft_id) {
                        root.root_scroll
                            .scroll_to_top_of_item(root.app_editor_scroll_item());
                        if let Some(draft) = &root.app_editor.draft {
                            draft.scroll.scroll_to_item(index);
                            cx.notify();
                        }
                    }
                })
            })
            .collect();
        let mut last_viewport = window.viewport_size();
        subscriptions.push(cx.observe_window_bounds(window, move |root, window, cx| {
            let viewport = window.viewport_size();
            if viewport == last_viewport {
                return;
            }
            last_viewport = viewport;
            if !window.is_window_active()
                || !root.app_editor.is_current_draft(draft_id)
                || !root.app_editor.owner_focus.contains_focused(window, cx)
            {
                return;
            }
            root.root_scroll
                .scroll_to_top_of_item(root.app_editor_scroll_item());
            if let Some(index) = handles.iter().position(|handle| handle.is_focused(window)) {
                if let Some(draft) = &root.app_editor.draft {
                    draft.scroll.scroll_to_item(index);
                }
            }
            cx.notify();
        }));
        self.app_editor.draft = Some(AddAppDraft {
            label,
            description,
            path,
            args,
            show_args: false,
            scroll,
            _focus_subscriptions: subscriptions,
        });
        if let Some(draft) = &self.app_editor.draft {
            draft.label.focus_handle(cx).focus(window, cx);
        }
        self.app_editor.status = None;
        cx.notify();
    }

    fn submit_add_app(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.app_editor.write_pending() {
            return;
        }
        let Some(snapshot) = self.app_editor.snapshot(cx) else {
            return;
        };
        let Some(operation) = self.restart_admission.admit() else {
            return;
        };
        let Some(receipt) = self.app_editor.begin_write() else {
            return;
        };
        cx.notify();
        let task = cx.background_executor().spawn(async move {
            multi_launcher::actions::AddAction::new(
                &snapshot.label,
                &snapshot.description,
                &snapshot.path,
                snapshot.show_args,
                &snapshot.args,
            )
            .map_err(str::to_owned)
            .and_then(|action| {
                action
                    .commit(ACTIONS_FILE)
                    .map_err(|error| format!("Could not save Add App: {error:#}"))
            })
        });
        let root = cx.entity().downgrade();
        let window_handle = window.window_handle();
        cx.spawn(async move |_, cx| {
            let result = task.await;
            let _ = window_handle.update(cx, |_, window, app| {
                let _ = root.update(app, |root, cx| {
                    let restore = result.is_ok()
                        && root.app_editor.open
                        && window.is_window_active()
                        && root.app_editor.owner_focus.contains_focused(window, cx)
                        && !root.restart_admission.is_fenced();
                    let failed = result.is_err();
                    if root.app_editor.finish_write(receipt, result) {
                        if failed && root.app_editor.open {
                            if let Some(draft) = &root.app_editor.draft {
                                draft.scroll.scroll_to_item(5);
                            }
                        }
                        clear_app_diagnostic(&mut root.restart_status);
                        if restore {
                            root.app_editor.new_focus.focus(window, cx);
                        }
                        cx.notify();
                    }
                });
            });
            drop(operation);
        })
        .detach();
    }

    fn cancel_add_app(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.app_editor.write_pending() || self.app_editor.draft.take().is_none() {
            return;
        }
        self.app_editor.status = None;
        clear_app_diagnostic(&mut self.restart_status);
        if self.app_editor.owner_focus.contains_focused(window, cx)
            && !self.restart_admission.is_fenced()
        {
            self.app_editor.new_focus.focus(window, cx);
        }
        cx.notify();
    }

    fn close_app_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.app_editor.open
            || self.app_editor.write_pending()
            || self.app_editor.draft.is_some()
        {
            return;
        }
        let restore = self.app_editor.owner_focus.contains_focused(window, cx)
            && !self.restart_admission.is_fenced();
        self.app_editor.open = false;
        self.app_editor.reads.invalidate();
        self.app_editor.draft = None;
        self.app_editor.status = None;
        let origin = self.app_editor.return_focus.take();
        let offset = self.app_editor.return_scroll.take();
        if restore {
            if let Some(origin) = origin.filter(|focus| self.focus_handle.contains(focus, window)) {
                if let Some(offset) = offset {
                    self.root_scroll.set_offset(offset);
                }
                origin.focus(window, cx);
            }
        }
        cx.notify();
    }

    pub(super) fn app_editor_escape(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.app_editor.open
            || !quick_help_escape_capture_should_close(
                &window.context_stack(),
                self.app_editor.owner_focus.contains_focused(window, cx),
            )
        {
            return false;
        }
        if self.app_editor.draft.is_some() {
            self.cancel_add_app(window, cx);
        } else {
            self.close_app_editor(window, cx);
        }
        true
    }
}

#[derive(IntoElement)]
pub(super) struct AppEditorPanel {
    root: WeakEntity<RootWindow>,
    width: f32,
    metrics: multi_launcher::gui::theme::ThemeMetrics,
    palette: multi_launcher::gui::theme::SemanticPalette,
    owner_focus: FocusHandle,
    new_focus: FocusHandle,
    arguments_focus: FocusHandle,
    draft: Option<AddAppPanelDraft>,
    filter: Option<Entity<InputState>>,
    catalog: Arc<[CatalogAction]>,
    catalog_error: Option<String>,
    catalog_loading: bool,
    catalog_scroll: ScrollHandle,
    status: Option<String>,
    pending: bool,
}

struct AddAppPanelDraft {
    label: Entity<InputState>,
    description: Entity<InputState>,
    path: Entity<InputState>,
    args: Entity<InputState>,
    show_args: bool,
    scroll: ScrollHandle,
}

impl AppEditorPanel {
    pub(super) fn new(
        root: WeakEntity<RootWindow>,
        state: &AppEditorState,
        width: f32,
        metrics: multi_launcher::gui::theme::ThemeMetrics,
        palette: multi_launcher::gui::theme::SemanticPalette,
    ) -> Self {
        Self {
            root,
            width,
            metrics,
            palette,
            owner_focus: state.owner_focus.clone(),
            new_focus: state.new_focus.clone(),
            arguments_focus: state.arguments_focus.clone(),
            status: state.status.clone(),
            filter: state.filter.clone(),
            catalog: state.catalog.clone(),
            catalog_error: state.catalog_error.clone(),
            catalog_loading: state.reads.pending.is_some(),
            catalog_scroll: state.catalog_scroll.clone(),
            pending: state.write_pending(),
            draft: state.draft.as_ref().map(|draft| AddAppPanelDraft {
                label: draft.label.clone(),
                description: draft.description.clone(),
                path: draft.path.clone(),
                args: draft.args.clone(),
                show_args: draft.show_args,
                scroll: draft.scroll.clone(),
            }),
        }
    }
}

impl RenderOnce for AppEditorPanel {
    fn render(self, window: &mut Window, app: &mut App) -> impl IntoElement {
        let metrics = self.metrics;
        let mut panel = div()
            .id("app-editor")
            .track_focus(&self.owner_focus)
            .key_context("AppEditor")
            .flex()
            .flex_col()
            .gap_y(px(metrics.item_spacing_y))
            .w(px(self.width))
            .max_w_full()
            .p(px(metrics.panel_padding))
            .bg(appearance::to_hsla(self.palette.panel_surface))
            .rounded(px(metrics.corner_radius))
            .text_color(appearance::to_hsla(self.palette.primary_text))
            .text_size(px(metrics.body_text_size))
            .line_height(px(metrics.line_height))
            .child(Label::new("App Editor").text_size(px(metrics.heading_text_size)));
        if let Some(draft) = self.draft {
            let draft_id = draft.label.entity_id();
            let checkbox_root = self.root.clone();
            let args_state = draft.args.clone();
            let arguments_focus = self.arguments_focus.clone();
            let checkbox = Checkbox::new("add-app-arguments")
                .checked(draft.show_args)
                .disabled(self.pending)
                .track_focus(&self.arguments_focus)
                .accessibility_label("Add arguments")
                .on_change(move |state, _, window, app| {
                    let _ = checkbox_root.update(app, |root, cx| {
                        if !root.app_editor.is_current_draft(draft_id) {
                            return;
                        }
                        if let Some(draft) = root.app_editor.draft.as_mut() {
                            let enabled = state == CheckboxState::Checked;
                            if !enabled && args_state.focus_handle(cx).is_focused(window) {
                                arguments_focus.focus(window, cx);
                            }
                            draft.show_args = enabled;
                            cx.notify();
                        }
                    });
                })
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_x(px(metrics.item_spacing_x))
                        .child(
                            CheckboxIndicator::new()
                                .checked(draft.show_args)
                                .size(px(18.0))
                                .rounded(px(3.0))
                                .border_1()
                                .border_color(appearance::to_hsla(self.palette.control_border))
                                .when(draft.show_args, |indicator| {
                                    indicator.bg(appearance::to_hsla(self.palette.accent))
                                })
                                .items_center()
                                .justify_center()
                                .child(if draft.show_args { "✓" } else { "" }),
                        )
                        .child(Label::new("Add arguments")),
                );
            let cancel_root = self.root.clone();
            let submit_root = self.root.clone();
            let tab_root = self.root.clone();
            panel = panel.capture_key_down(move |event: &KeyDownEvent, window, app| {
                if event.keystroke.key != "tab" || !window.is_window_active() {
                    return;
                }
                let _ = tab_root.update(app, |root, cx| {
                    if root.app_editor.is_current_draft(draft_id)
                        && root.app_editor.owner_focus.contains_focused(window, cx)
                    {
                        root.root_scroll
                            .scroll_to_top_of_item(root.app_editor_scroll_item());
                        cx.notify();
                    }
                });
            });
            let reserved_height = 2.0 * metrics.window_padding
                + 2.0 * metrics.panel_padding
                + metrics.heading_text_size.max(metrics.line_height)
                + metrics.line_height
                + metrics.row_min_height
                + 2.0 * metrics.button_padding_y
                + 4.0 * metrics.item_spacing_y;
            let body_height = (window.viewport_size().height.as_f32() - reserved_height).max(40.0);
            let body = div()
                .id("add-app-body")
                .flex()
                .flex_col()
                .gap_y(px(metrics.item_spacing_y))
                .max_h(px(body_height))
                .flex_shrink_0()
                .overflow_y_scroll()
                .track_scroll(&draft.scroll)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_shrink_0()
                        .child(Label::new("Label"))
                        .child(
                            Input::new(&draft.label)
                                .disabled(self.pending)
                                .aria_label("Label"),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_shrink_0()
                        .child(Label::new("Description"))
                        .child(
                            Input::new(&draft.description)
                                .disabled(self.pending)
                                .aria_label("Description"),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_shrink_0()
                        .child(Label::new("Path"))
                        .child(
                            Input::new(&draft.path)
                                .disabled(self.pending)
                                .aria_label("Path"),
                        ),
                )
                .child(div().flex_shrink_0().child(checkbox))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_shrink_0()
                        .when(draft.show_args, |row| {
                            row.child(Label::new("Args")).child(
                                Input::new(&draft.args)
                                    .disabled(self.pending)
                                    .aria_label("Args"),
                            )
                        }),
                )
                .child(div().flex_shrink_0().children(self.status.map(Label::new)));
            panel = panel.child(Label::new("Add App")).child(body).child(
                div()
                    .flex()
                    .flex_row()
                    .gap_x(px(metrics.item_spacing_x))
                    .child(
                        Button::new("add-app-submit")
                            .label("Add")
                            .disabled(self.pending)
                            .on_click(move |_, window, app| {
                                let _ = submit_root.update(app, |root, cx| {
                                    if root.app_editor.is_current_draft(draft_id) {
                                        root.submit_add_app(window, cx);
                                    }
                                });
                            }),
                    )
                    .child(
                        Button::new("add-app-cancel")
                            .label("Cancel")
                            .disabled(self.pending)
                            .on_click(move |_, window, app| {
                                let _ = cancel_root.update(app, |root, cx| {
                                    if root.app_editor.is_current_draft(draft_id) {
                                        root.cancel_add_app(window, cx);
                                    }
                                });
                            }),
                    ),
            );
        } else {
            let new_root = self.root.clone();
            let close_root = self.root.clone();
            let refresh_root = self.root;
            let filter_text = self
                .filter
                .as_ref()
                .map(|filter| catalog_filter(filter.read(app).value().as_str()))
                .unwrap_or_default();
            let rows = self
                .catalog
                .iter()
                .enumerate()
                .filter(|(_, row)| catalog_matches(row, &filter_text))
                .map(|(index, row)| {
                    let mut item = div()
                        .id(("app-catalog-row", index))
                        .flex()
                        .flex_col()
                        .flex_shrink_0()
                        .child(Label::new(row.label.clone()))
                        .child(Label::new(row.desc.clone()))
                        .child(Label::new(row.action.clone()));
                    if let Some(args) = &row.args {
                        item = item
                            .child(Label::new("Arguments"))
                            .child(Label::new(args.clone()));
                    }
                    item
                })
                .collect::<Vec<_>>();
            let reserved_height = 2.0 * metrics.window_padding
                + 2.0 * metrics.panel_padding
                + metrics.heading_text_size.max(metrics.line_height)
                + 3.0 * metrics.row_min_height
                + 6.0 * metrics.item_spacing_y;
            let body_height = (window.viewport_size().height.as_f32() - reserved_height).max(40.0);
            panel = panel
                .children(
                    self.filter
                        .as_ref()
                        .map(|filter| Input::new(filter).aria_label("Filter apps")),
                )
                .child(
                    div()
                        .id("app-catalog-list")
                        .flex()
                        .flex_col()
                        .gap_y(px(metrics.item_spacing_y))
                        .max_h(px(body_height))
                        .flex_shrink_0()
                        .overflow_y_scroll()
                        .track_scroll(&self.catalog_scroll)
                        .children(self.catalog_loading.then(|| Label::new("Loading apps…")))
                        .children(self.catalog_error.map(Label::new))
                        .children(rows),
                )
                .child(Button::new("app-editor-refresh").label("Refresh").on_click(
                    move |_, window, app| {
                        let _ = refresh_root
                            .update(app, |root, cx| root.refresh_app_catalog(window, cx));
                    },
                ))
                .child(
                    Button::new("app-editor-new")
                        .label("New App")
                        .track_focus(&self.new_focus)
                        .on_click(move |_, window, app| {
                            let _ = new_root.update(app, |root, cx| root.new_app_draft(window, cx));
                        }),
                )
                .child(Button::new("app-editor-close").label("Close").on_click(
                    move |_, window, app| {
                        let _ =
                            close_root.update(app, |root, cx| root.close_app_editor(window, cx));
                    },
                ));
        }
        panel
    }
}

#[cfg(test)]
mod tests {
    use super::{AddAppWrites, CatalogAction, CatalogReads, catalog_filter, catalog_matches};

    #[test]
    fn add_app_write_accepts_only_live_receipt_once() {
        let mut writes = AddAppWrites::default();
        let first = writes.begin().unwrap();
        assert!(writes.begin().is_none());
        assert!(!writes.finish(first.wrapping_add(1)));
        assert_eq!(writes.pending, Some(first));
        assert!(writes.finish(first));
        assert!(!writes.finish(first));
        let second = writes.begin().unwrap();
        assert_ne!(first, second);
        assert!(!writes.finish(first));
        assert_eq!(writes.pending, Some(second));
        assert!(writes.finish(second));
    }

    #[test]
    fn catalog_filter_matches_egui_literal_rules() {
        let row = CatalogAction {
            label: "My App".into(),
            desc: "A Description".into(),
            action: "C:/Tools/Run.exe".into(),
            args: Some("secret args".into()),
        };
        let matches = |filter| catalog_matches(&row, &catalog_filter(filter));
        assert!(matches("  "));
        assert!(matches("APP"));
        assert!(matches("description"));
        assert!(matches("TOOLS"));
        assert!(!matches(" app "));
        assert!(!matches("secret"));
    }

    #[test]
    fn catalog_reads_reject_superseded_and_retired_receipts() {
        let mut reads = CatalogReads::default();
        let first = reads.begin();
        let second = reads.begin();
        assert!(!reads.finish(first));
        assert!(reads.finish(second));
        assert!(!reads.finish(second));
        let before_close_or_commit = reads.begin();
        reads.invalidate();
        let reopened = reads.begin();
        assert!(!reads.finish(before_close_or_commit));
        assert!(reads.finish(reopened));
    }
}
