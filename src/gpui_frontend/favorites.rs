use std::sync::Arc;

use gpui_kit::base::Disableable;
use gpui_kit::component::{
    button::Button,
    input::{Input, InputEvent, InputState, Textarea, TextareaState},
    label::Label,
    select::{Select, SelectEvent, SelectState},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use multi_launcher::actions::Action;
use multi_launcher::gui::theme::{SemanticPalette, ThemeMetrics};
use multi_launcher::plugins::fav::{
    FAV_FILE, FavEntry, FavoriteRowTarget, ManualFavoriteOrigin, PrefilledFavoriteDraft,
    PrefilledFavoriteSaveError, favorite_picker_categories, favorite_picker_choices,
    favorite_picker_pick, favorite_row_target, load_favs, named_favorite_origin,
    prefilled_favorite_draft, remove_favorite_row, resolve_with_plugin, save_manual_favorite,
    save_prefilled_favorite,
};

use super::{RootWindow, appearance};

pub(super) const FAVORITES_WRITE_PENDING: &str =
    "Wait for the Favorites write to finish before closing Multi Launcher.";
const FAVORITES_RESTART_WRITE_PENDING: &str =
    "Wait for the Favorites write to finish before switching to egui.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Request {
    Load(u64),
    Write(u64),
}

#[derive(Default)]
pub(super) struct ResultFavorites {
    rows: Arc<[FavEntry]>,
    browse_scroll: ScrollHandle,
    owner: Option<FocusHandle>,
    return_focus: Option<FocusHandle>,
    return_offset: Option<Point<Pixels>>,
    draft: Option<FavoriteFields>,
    status: Option<String>,
    pending: Option<Request>,
    pending_draft: Option<OpenDraft>,
    loaded: bool,
    sequence: u64,
}

impl ResultFavorites {
    pub(super) fn is_open(&self) -> bool {
        self.owner.is_some()
    }

    pub(super) fn owns_focus(&self, window: &Window, cx: &App) -> bool {
        self.owner
            .as_ref()
            .is_some_and(|owner| owner.contains_focused(window, cx))
    }

    pub(super) fn write_pending(&self) -> bool {
        matches!(self.pending, Some(Request::Write(_)))
    }

    pub(super) fn restart_blocker(&self) -> Option<&'static str> {
        if self.write_pending() {
            Some(FAVORITES_RESTART_WRITE_PENDING)
        } else if self.draft.is_some() || self.pending_draft.is_some() {
            Some("Save or cancel the current Favorites draft before switching to egui.")
        } else {
            None
        }
    }

    fn next(&mut self) -> u64 {
        self.sequence = self.sequence.wrapping_add(1);
        self.sequence
    }

    fn consume(&mut self, request: Request) -> bool {
        if self.pending != Some(request) {
            return false;
        }
        self.pending = None;
        true
    }

    fn row_target(
        &self,
        index: usize,
        displayed: &FavEntry,
        sequence: u64,
    ) -> Option<FavoriteRowTarget> {
        if !self.is_open()
            || !self.loaded
            || self.pending.is_some()
            || self.draft.is_some()
            || self.sequence != sequence
            || self.rows.get(index) != Some(displayed)
        {
            return None;
        }
        favorite_row_target(&self.rows, index)
    }

    fn finish_write(
        &mut self,
        request: Request,
        operation: WriteKind,
        result: Result<Vec<FavEntry>, PrefilledFavoriteSaveError>,
    ) -> bool {
        if !self.consume(request) {
            return false;
        }
        match result {
            Ok(rows) => {
                self.rows = rows.into();
                if matches!(operation, WriteKind::Save { .. }) {
                    self.draft = None;
                }
                self.status = Some(
                    match operation {
                        WriteKind::Save { .. } => "Favorite saved.",
                        WriteKind::Remove => "Favorite removed.",
                    }
                    .to_owned(),
                );
            }
            Err(error) => {
                self.browse_scroll.set_offset(point(px(0.0), px(0.0)));
                if let Some(draft) = &self.draft {
                    draft.scroll.set_offset(point(px(0.0), px(0.0)));
                }
                let verb = match operation {
                    WriteKind::Save { .. } => "save",
                    WriteKind::Remove => "remove",
                };
                if matches!(error, PrefilledFavoriteSaveError::Store(_)) {
                    self.loaded = false;
                    self.status = Some(format!(
                        "Could not {verb} Favorite: {error}. Close and reopen Favorites to reload."
                    ));
                } else if matches!(error, PrefilledFavoriteSaveError::StaleTarget) {
                    self.status = Some(match operation {
                        WriteKind::Remove => "Favorites changed before this row could be removed. Close and reopen Favorites to reload.".to_owned(),
                        WriteKind::Save { prefilled: false } => "Favorites changed while this draft was open. Reopen it before saving.".to_owned(),
                        WriteKind::Save { prefilled: true } => format!("Could not save Favorite: {error}"),
                    });
                } else {
                    self.status = Some(format!("Could not {verb} Favorite: {error}"));
                }
            }
        }
        true
    }

    fn retire_load(&mut self) {
        if matches!(self.pending, Some(Request::Load(_))) {
            self.pending = None;
        }
    }

    fn cancel_draft(&mut self) -> bool {
        let had_draft = self.draft.take().is_some();
        let was_prefilling = self.pending_draft.take().is_some();
        if had_draft && self.loaded {
            self.status = None;
        }
        had_draft || was_prefilling
    }
}

#[derive(Clone, Copy)]
enum WriteKind {
    Save { prefilled: bool },
    Remove,
}

enum FavoriteWrite {
    Save {
        origin: DraftOrigin,
        entry: FavEntry,
    },
    Remove(FavoriteRowTarget),
}

#[derive(Clone)]
enum OpenDraft {
    Prefilled(Action),
    Named(String),
}

#[derive(Clone)]
enum DraftOrigin {
    Prefilled(PrefilledFavoriteDraft),
    Manual(ManualFavoriteOrigin),
}

struct FavoriteFields {
    scroll: ScrollHandle,
    origin: DraftOrigin,
    original: FavEntry,
    label: FavoriteField,
    command: FavoriteField,
    args: FavoriteField,
    category: Entity<SelectState<Vec<String>>>,
    filter: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
}

#[derive(Clone)]
struct FavoriteField {
    input: Entity<TextareaState>,
    initial: String,
}

impl FavoriteFields {
    fn new(
        origin: DraftOrigin,
        original: FavEntry,
        categories: Vec<String>,
        window: &mut Window,
        cx: &mut Context<RootWindow>,
    ) -> Self {
        let category = cx.new(|cx| SelectState::new(categories, None, window, cx));
        let filter = cx.new(|cx| InputState::new(window, cx));
        let category_subscription = cx.subscribe(
            &category,
            |root, source, event: &SelectEvent<Vec<String>>, cx| {
                if matches!(event, SelectEvent::Confirm(_))
                    && root
                        .result_favorites
                        .draft
                        .as_ref()
                        .is_some_and(|draft| draft.category == source)
                {
                    cx.notify();
                }
            },
        );
        let filter_subscription = cx.subscribe(&filter, |root, source, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change)
                && root
                    .result_favorites
                    .draft
                    .as_ref()
                    .is_some_and(|draft| draft.filter == source)
            {
                cx.notify();
            }
        });
        Self {
            scroll: ScrollHandle::default(),
            category,
            filter,
            _subscriptions: vec![category_subscription, filter_subscription],
            label: FavoriteField::new(&original.label, window, cx),
            command: FavoriteField::new(&original.action, window, cx),
            args: FavoriteField::new(original.args.as_deref().unwrap_or_default(), window, cx),
            origin,
            original,
        }
    }
}

impl FavoriteField {
    fn new(value: &str, window: &mut Window, cx: &mut Context<RootWindow>) -> Self {
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .default_value(value.to_owned())
                .auto_grow(1, 3)
        });
        let initial = input.read(cx).value().to_string();
        Self { input, initial }
    }

    fn replace(
        &mut self,
        value: String,
        window: &mut Window,
        cx: &mut Context<RootWindow>,
    ) -> String {
        self.input
            .update(cx, |input, cx| input.replace_all(value.clone(), window, cx));
        self.initial = self.input.read(cx).value().to_string();
        value
    }

    fn exact_value(&self, original: &str, cx: &App) -> String {
        let current = self.input.read(cx).value();
        // Unedited legacy text must survive even if the editor normalized it.
        if current.as_str() == self.initial {
            original.to_owned()
        } else {
            current.to_string()
        }
    }
}

impl RootWindow {
    pub(super) fn open_result_favorites(
        &mut self,
        action: Action,
        previous_focus: Option<FocusHandle>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_favorites(
            Some(OpenDraft::Prefilled(action)),
            previous_focus,
            window,
            cx,
        );
    }

    pub(super) fn open_favorites_browse(
        &mut self,
        previous_focus: Option<FocusHandle>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_favorites(None, previous_focus, window, cx);
    }

    pub(super) fn open_favorites_named(
        &mut self,
        label: String,
        previous_focus: Option<FocusHandle>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_favorites(Some(OpenDraft::Named(label)), previous_focus, window, cx);
    }

    fn open_favorites(
        &mut self,
        action: Option<OpenDraft>,
        previous_focus: Option<FocusHandle>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.restart_admission.is_fenced() {
            return;
        }
        let scroll_item = self.quick_help_scroll_item() + usize::from(self.quick_help.open);
        let state = &mut self.result_favorites;
        if action.is_none() && state.is_open() {
            state.return_focus = previous_focus;
            state.return_offset = Some(self.root_scroll.offset());
            self.root_scroll.scroll_to_top_of_item(scroll_item);
            if let Some(owner) = &state.owner {
                owner.focus(window, cx);
            }
            cx.notify();
            return;
        }
        if state.restart_blocker().is_some() {
            if state.loaded && state.pending.is_none() {
                state.status = Some("Finish the current Favorites draft first.".to_owned());
            }
            self.root_scroll.scroll_to_top_of_item(scroll_item);
            if let Some(owner) = &state.owner {
                owner.focus(window, cx);
            }
            cx.notify();
            return;
        }
        if state.is_open() && matches!(state.pending, Some(Request::Load(_))) {
            state.pending_draft = action;
            state.return_focus = previous_focus;
            state.return_offset = Some(self.root_scroll.offset());
            self.root_scroll.scroll_to_top_of_item(scroll_item);
            if let Some(owner) = &state.owner {
                owner.focus(window, cx);
            }
            cx.notify();
            return;
        }
        let owner = cx.focus_handle();
        state.owner = Some(owner.clone());
        state.browse_scroll.set_offset(point(px(0.0), px(0.0)));
        state.return_focus = previous_focus;
        state.return_offset = Some(self.root_scroll.offset());
        state.status = Some("Loading Favorites…".to_owned());
        state.loaded = false;
        state.pending_draft = action;
        let request = Request::Load(state.next());
        state.pending = Some(request);
        self.fresh_search_enter = None;
        self.retire_search_jump_mode();
        self.root_scroll.scroll_to_top_of_item(scroll_item);
        owner.focus(window, cx);

        let task = cx
            .background_executor()
            .spawn(async { load_favs(FAV_FILE) });
        cx.spawn_in(window, async move |root, cx| {
            let rows = task.await;
            let _ = root.update_in(cx, |root, window, cx| {
                let state = &mut root.result_favorites;
                if !state.consume(request) {
                    return;
                }
                let prefill = state.pending_draft.take();
                match rows {
                    Ok(rows) => {
                        if let Some(draft) = prefill {
                            let categories = favorite_picker_categories(
                                root.search_catalog
                                    .providers
                                    .plugins()
                                    .into_iter()
                                    .map(|p| p.name()),
                            );
                            let (origin, original) = match draft {
                                OpenDraft::Prefilled(action) => {
                                    let draft = prefilled_favorite_draft(&rows, &action);
                                    let original = FavEntry {
                                        label: action.label,
                                        action: action.action,
                                        args: action.args,
                                    };
                                    (DraftOrigin::Prefilled(draft), original)
                                }
                                OpenDraft::Named(label) => {
                                    let origin = named_favorite_origin(&rows, &label);
                                    let original = rows
                                        .iter()
                                        .find(|row| row.label == label)
                                        .cloned()
                                        .unwrap_or(FavEntry {
                                            label,
                                            action: String::new(),
                                            args: None,
                                        });
                                    (DraftOrigin::Manual(origin), original)
                                }
                            };
                            state.draft = Some(FavoriteFields::new(
                                origin, original, categories, window, cx,
                            ));
                        }
                        state.loaded = true;
                        state.rows = rows.into();
                        state.status = None;
                    }
                    Err(error) => state.status = Some(format!("Could not load Favorites: {error}")),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn edit_result_favorite(
        &mut self,
        row: Option<(usize, FavEntry, u64)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = &mut self.result_favorites;
        if !state.is_open() || !state.loaded || state.pending.is_some() || state.draft.is_some() {
            return;
        }
        let (origin, entry) = if let Some((index, displayed, sequence)) = row {
            let Some(target) = state.row_target(index, &displayed, sequence) else {
                return;
            };
            (
                ManualFavoriteOrigin::Existing(target),
                state.rows[index].clone(),
            )
        } else {
            (
                ManualFavoriteOrigin::Append,
                FavEntry {
                    label: String::new(),
                    action: String::new(),
                    args: None,
                },
            )
        };
        let categories = favorite_picker_categories(
            self.search_catalog
                .providers
                .plugins()
                .into_iter()
                .map(|p| p.name()),
        );
        state.draft = Some(FavoriteFields::new(
            DraftOrigin::Manual(origin),
            entry,
            categories,
            window,
            cx,
        ));
        state.status = None;
        if let Some(owner) = &state.owner {
            owner.focus(window, cx);
        }
        cx.notify();
    }

    fn save_result_favorite(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let state = &mut self.result_favorites;
        let Some(fields) = state
            .draft
            .as_ref()
            .filter(|_| state.pending.is_none() && state.loaded)
        else {
            return;
        };
        let args = fields
            .args
            .exact_value(fields.original.args.as_deref().unwrap_or_default(), cx);
        let mut entry = FavEntry {
            label: fields.label.exact_value(&fields.original.label, cx),
            action: fields.command.exact_value(&fields.original.action, cx),
            args: (!args.trim().is_empty()).then_some(args),
        };
        if entry.label.trim().is_empty() || entry.action.trim().is_empty() {
            fields.scroll.set_offset(point(px(0.0), px(0.0)));
            state.status = Some("Label and command are required".to_owned());
            cx.notify();
            return;
        }
        {
            if let Some(category) = fields.category.read(cx).selected_value() {
                if let Some(plugin) = self
                    .search_catalog
                    .providers
                    .plugins()
                    .into_iter()
                    .find(|p| p.name() == category)
                {
                    let args = entry.args.as_deref().filter(|args| !args.trim().is_empty());
                    (entry.action, entry.args) = resolve_with_plugin(plugin, &entry.action, args);
                }
            }
        }
        let origin = fields.origin.clone();
        self.write_result_favorite(FavoriteWrite::Save { origin, entry }, window, cx);
    }

    fn pick_result_favorite(
        &mut self,
        category: String,
        filter: String,
        choice: Action,
        draft_id: Entity<SelectState<Vec<String>>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = &mut self.result_favorites;
        if !state.is_open() || !state.loaded || state.pending.is_some() {
            return;
        }
        let Some(draft) = state.draft.as_mut() else {
            return;
        };
        if draft.category != draft_id
            || draft.category.read(cx).selected_value() != Some(&category)
            || draft.filter.read(cx).value().as_str() != filter
        {
            return;
        }
        let Some(plugin) = self
            .search_catalog
            .providers
            .plugins()
            .into_iter()
            .find(|p| p.name() == category)
        else {
            return;
        };
        if !favorite_picker_choices(plugin, &filter).contains(&choice) {
            return;
        }
        let args = draft
            .args
            .exact_value(draft.original.args.as_deref().unwrap_or_default(), cx);
        let (command, args) = favorite_picker_pick(plugin, &choice, &args);
        draft.original.action = draft.command.replace(command, window, cx);
        draft.original.args = Some(draft.args.replace(args.unwrap_or_default(), window, cx));
        cx.notify();
    }

    fn remove_result_favorite(
        &mut self,
        index: usize,
        displayed: FavEntry,
        sequence: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self
            .result_favorites
            .row_target(index, &displayed, sequence)
        else {
            return;
        };
        self.write_result_favorite(FavoriteWrite::Remove(target), window, cx);
    }

    fn write_result_favorite(
        &mut self,
        write: FavoriteWrite,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let kind = match &write {
            FavoriteWrite::Save { origin, .. } => WriteKind::Save {
                prefilled: matches!(origin, DraftOrigin::Prefilled(_)),
            },
            FavoriteWrite::Remove(_) => WriteKind::Remove,
        };
        let Some(permit) = self.restart_admission.admit() else {
            return;
        };
        let state = &mut self.result_favorites;
        let request = Request::Write(state.next());
        state.pending = Some(request);
        state.status = Some(
            match kind {
                WriteKind::Save { .. } => "Saving Favorite…",
                WriteKind::Remove => "Removing Favorite…",
            }
            .to_owned(),
        );
        if let Some(owner) = &state.owner {
            owner.focus(window, cx);
        }

        let task = cx.background_executor().spawn(async move {
            match write {
                FavoriteWrite::Save {
                    origin: DraftOrigin::Prefilled(mut draft),
                    entry,
                } => {
                    draft.label = entry.label;
                    draft.action = entry.action;
                    draft.args = entry.args;
                    save_prefilled_favorite(FAV_FILE, draft)
                }
                FavoriteWrite::Save {
                    origin: DraftOrigin::Manual(origin),
                    entry,
                } => save_manual_favorite(FAV_FILE, origin, entry),
                FavoriteWrite::Remove(target) => remove_favorite_row(FAV_FILE, target),
            }
        });
        let root = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let result = task.await;
            let _ = root.update(cx, |root, cx| {
                if !root.result_favorites.finish_write(request, kind, result) {
                    return;
                }
                if matches!(
                    root.restart_status.as_deref(),
                    Some(FAVORITES_WRITE_PENDING | FAVORITES_RESTART_WRITE_PENDING)
                ) {
                    root.restart_status = None;
                }
                cx.notify();
            });
            drop(permit);
        })
        .detach();
        cx.notify();
    }

    pub(super) fn dismiss_result_favorites(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let state = &mut self.result_favorites;
        if state.write_pending() {
            return;
        }
        if state.cancel_draft() {
            if let Some(owner) = &state.owner {
                owner.focus(window, cx);
            }
        } else {
            let restore = window.is_window_active() && state.owns_focus(window, cx);
            let focus = state.return_focus.take();
            let offset = state.return_offset.take();
            state.retire_load();
            state.owner = None;
            state.status = None;
            if restore {
                if let Some(offset) = offset {
                    self.root_scroll.set_offset(offset);
                }
                focus
                    .filter(|focus| self.focus_handle.contains(focus, window))
                    .unwrap_or_else(|| self.query.focus_handle(cx))
                    .focus(window, cx);
            }
        }
        cx.notify();
    }

    pub(super) fn favorites_panel(
        &self,
        width: f32,
        metrics: ThemeMetrics,
        palette: SemanticPalette,
        cx: &Context<Self>,
    ) -> Option<FavoritesPanel> {
        let state = &self.result_favorites;
        let owner = state.owner.clone()?;
        let draft = state.draft.as_ref().map(|draft| {
            let choices = draft
                .category
                .read(cx)
                .selected_value()
                .and_then(|category| {
                    self.search_catalog
                        .providers
                        .plugins()
                        .into_iter()
                        .find(|p| p.name() == category)
                        .map(|plugin| {
                            let filter = draft.filter.read(cx).value().to_string();
                            (
                                category.clone(),
                                filter.clone(),
                                favorite_picker_choices(plugin, &filter),
                            )
                        })
                });
            FavoritesPanelDraft {
                scroll: draft.scroll.clone(),
                label: draft.label.input.clone(),
                command: draft.command.input.clone(),
                args: draft.args.input.clone(),
                category: draft.category.clone(),
                filter: draft.filter.clone(),
                choices,
            }
        });
        Some(FavoritesPanel {
            root: cx.entity().downgrade(),
            owner,
            draft,
            rows: state.rows.clone(),
            browse_scroll: state.browse_scroll.clone(),
            status: state.status.clone(),
            pending: state.pending.is_some(),
            pending_draft: state.pending_draft.is_some(),
            waiting: state.write_pending(),
            loaded: state.loaded,
            sequence: state.sequence,
            width,
            metrics,
            palette,
        })
    }
}

struct FavoritesPanelDraft {
    scroll: ScrollHandle,
    label: Entity<TextareaState>,
    command: Entity<TextareaState>,
    args: Entity<TextareaState>,
    category: Entity<SelectState<Vec<String>>>,
    filter: Entity<InputState>,
    choices: Option<(String, String, Vec<Action>)>,
}

#[derive(IntoElement)]
pub(super) struct FavoritesPanel {
    root: WeakEntity<RootWindow>,
    owner: FocusHandle,
    draft: Option<FavoritesPanelDraft>,
    rows: Arc<[FavEntry]>,
    browse_scroll: ScrollHandle,
    status: Option<String>,
    pending: bool,
    pending_draft: bool,
    waiting: bool,
    loaded: bool,
    sequence: u64,
    width: f32,
    metrics: ThemeMetrics,
    palette: SemanticPalette,
}

fn favorites_viewport_budget(
    viewport_width: f32,
    viewport_height: f32,
    preferred_width: f32,
    metrics: ThemeMetrics,
    control_height: f32,
) -> (f32, f32) {
    let width = preferred_width.min((viewport_width - 2.0 * metrics.window_padding).max(1.0));
    let reserved_height = 2.0 * metrics.window_padding
        + 2.0 * metrics.panel_padding
        + metrics.heading_text_size.max(metrics.line_height)
        + control_height
        + 2.0 * metrics.item_spacing_y;
    (width, (viewport_height - reserved_height).clamp(0.0, 280.0))
}

impl RenderOnce for FavoritesPanel {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        let state = &self;
        let owner = &state.owner;
        let metrics = state.metrics;
        let (width, body_height) = favorites_viewport_budget(
            window.viewport_size().width.as_f32(),
            window.viewport_size().height.as_f32(),
            state.width,
            metrics,
            2.0 * window.rem_size().as_f32(),
        );
        let palette = state.palette;
        let editing = state.draft.is_some();
        let waiting = state.waiting;
        let dismiss_label = if editing || state.pending_draft {
            "Cancel"
        } else {
            "Close"
        };
        let controls = div()
            .flex()
            .gap_x(px(metrics.item_spacing_y))
            .when(editing, |this| {
                this.child(
                    Button::new("result-favorites-save")
                        .label("Save")
                        .tab_stop(true)
                        .disabled(waiting || !state.loaded)
                        .on_click({
                            let root = state.root.clone();
                            move |_, window, app| {
                                let _ = root
                                    .update(app, |root, cx| root.save_result_favorite(window, cx));
                            }
                        }),
                )
            })
            .when(!editing, |this| {
                this.child(
                    Button::new("result-favorites-add")
                        .label("Add Fav")
                        .tab_stop(true)
                        .disabled(!state.loaded || state.pending)
                        .on_click({
                            let root = state.root.clone();
                            move |_, window, app| {
                                let _ = root.update(app, |root, cx| {
                                    root.edit_result_favorite(None, window, cx)
                                });
                            }
                        }),
                )
            })
            .child(
                Button::new("result-favorites-close")
                    .label(dismiss_label)
                    .tab_stop(true)
                    .disabled(waiting)
                    .on_click({
                        let root = state.root.clone();
                        move |_, window, app| {
                            let _ = root
                                .update(app, |root, cx| root.dismiss_result_favorites(window, cx));
                        }
                    }),
            );
        let mut panel = div()
            .id("result-favorites-panel")
            .track_focus(owner)
            .key_context("ResultFavorites")
            .flex()
            .flex_col()
            .w(px(width))
            .gap_y(px(metrics.item_spacing_y))
            .p(px(metrics.panel_padding))
            .bg(appearance::to_hsla(palette.panel_surface))
            .child(
                Label::new("Favorites")
                    .id("result-favorites-title")
                    .text_size(px(metrics.heading_text_size)),
            );
        if let Some(draft) = &state.draft {
            let mut body = div()
                .id("result-favorites-form")
                .flex()
                .flex_col()
                .gap_y(px(metrics.item_spacing_y))
                .w_full()
                .max_h(px(body_height))
                .overflow_y_scroll()
                .track_scroll(&draft.scroll)
                .children(state.status.as_ref().map(|status| {
                    Label::new(status.clone())
                        .id("result-favorites-status")
                        .flex_shrink_0()
                        .w_full()
                }));
            for (label, field) in [
                ("Label", &draft.label),
                ("Command", &draft.command),
                ("Args", &draft.args),
            ] {
                body = body.child(
                    div()
                        .id(label)
                        .flex_shrink_0()
                        .flex()
                        .flex_col()
                        .child(Label::new(label).id("result-favorite-field-label"))
                        .child(
                            Textarea::new(field)
                                .aria_label(label)
                                .tab_index(0)
                                .readonly(waiting),
                        ),
                );
            }
            let disabled = waiting || !state.loaded;
            body = body
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_shrink_0()
                        .child(Label::new("Category").id("result-favorite-category-label"))
                        .child(
                            Select::new(&draft.category)
                                .accessibility_label("Category")
                                .placeholder("Select")
                                .disabled(disabled),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_shrink_0()
                        .child(Label::new("Filter").id("result-favorite-filter-label"))
                        .child(
                            Input::new(&draft.filter)
                                .aria_label("Filter")
                                .tab_index(0)
                                .readonly(disabled),
                        ),
                );
            if let Some((category, filter, choices)) = &draft.choices {
                let category = category.clone();
                let filter = filter.clone();
                let draft_id = draft.category.clone();
                body = body.child(
                    div()
                        .id("result-favorite-choices")
                        .flex_shrink_0()
                        .flex()
                        .flex_col()
                        .children(choices.iter().cloned().enumerate().map(|(index, choice)| {
                            let caption = format!("{} - {}", choice.label, choice.desc);
                            let category = category.clone();
                            let filter = filter.clone();
                            let draft_id = draft_id.clone();
                            let root = state.root.clone();
                            Button::new(SharedString::from(format!(
                                "result-favorite-choice-{index}"
                            )))
                            .label(caption)
                            .tab_stop(true)
                            .disabled(disabled)
                            .on_click(move |_, window, app| {
                                let _ = root.update(app, |root, cx| {
                                    root.pick_result_favorite(
                                        category.clone(),
                                        filter.clone(),
                                        choice.clone(),
                                        draft_id.clone(),
                                        window,
                                        cx,
                                    )
                                });
                            })
                        })),
                );
            }
            panel = panel.child(body);
        } else {
            let mut body = div()
                .id("result-favorites-rows")
                .flex()
                .flex_col()
                .gap_y(px(metrics.item_spacing_y))
                .w_full()
                .max_h(px(body_height))
                .overflow_y_scroll()
                .track_scroll(&state.browse_scroll)
                .children(state.status.as_ref().map(|status| {
                    Label::new(status.clone())
                        .id("result-favorites-status")
                        .flex_shrink_0()
                        .w_full()
                }));
            if state.rows.is_empty() && state.loaded && !state.pending {
                body = body.child(Label::new("No Favorites.").id("result-favorites-empty"));
            } else if !state.rows.is_empty() {
                body = body.children(state.rows.iter().enumerate().map(|(index, row)| {
                    let caption = match &row.args {
                        Some(args) => format!("{} · {} {}", row.label, row.action, args),
                        None => format!("{} · {}", row.label, row.action),
                    };
                    let displayed = row.clone();
                    let remove_displayed = displayed.clone();
                    let sequence = state.sequence;
                    let edit_root = state.root.clone();
                    let remove_root = state.root.clone();
                    div()
                        .flex()
                        .flex_col()
                        .flex_shrink_0()
                        .w_full()
                        .child(
                            Label::new(caption)
                                .id(format!("result-favorite-row-{index}"))
                                .w_full(),
                        )
                        .child(
                            div()
                                .flex()
                                .gap_x(px(metrics.item_spacing_x))
                                .child(
                                    Button::new(SharedString::from(format!(
                                        "result-favorite-edit-{index}"
                                    )))
                                    .label("Edit")
                                    .tab_stop(true)
                                    .disabled(!state.loaded || state.pending)
                                    .on_click(
                                        move |_, window, app| {
                                            let _ = edit_root.update(app, |root, cx| {
                                                root.edit_result_favorite(
                                                    Some((index, displayed.clone(), sequence)),
                                                    window,
                                                    cx,
                                                )
                                            });
                                        },
                                    ),
                                )
                                .child(
                                    Button::new(SharedString::from(format!(
                                        "result-favorite-remove-{index}"
                                    )))
                                    .label("Remove")
                                    .tab_stop(true)
                                    .disabled(!state.loaded || state.pending)
                                    .on_click(
                                        move |_, window, app| {
                                            let _ = remove_root.update(app, |root, cx| {
                                                root.remove_result_favorite(
                                                    index,
                                                    remove_displayed.clone(),
                                                    sequence,
                                                    window,
                                                    cx,
                                                )
                                            });
                                        },
                                    ),
                                ),
                        )
                }));
            }
            panel = panel.child(body);
        }
        panel.child(controls)
    }
}

#[cfg(test)]
mod tests {
    use super::{Request, ResultFavorites, favorites_viewport_budget};
    use multi_launcher::gui::theme::ThemeMetrics;

    #[test]
    fn favorites_viewport_budget_fits_logical_viewport() {
        let metrics = ThemeMetrics {
            body_text_size: 14.0,
            heading_text_size: 18.0,
            small_text_size: 12.0,
            line_height: 18.0,
            item_spacing_x: 6.0,
            item_spacing_y: 4.0,
            section_spacing: 8.0,
            window_padding: 8.0,
            panel_padding: 6.0,
            button_padding_x: 8.0,
            button_padding_y: 4.0,
            corner_radius: 4.0,
            stroke_width: 1.0,
            shadow_blur: 8.0,
            icon_size: 16.0,
            row_min_height: 28.0,
        };
        assert_eq!(
            favorites_viewport_budget(220.0, 180.0, 400.0, metrics, 32.0),
            (204.0, 94.0)
        );
        assert_eq!(
            favorites_viewport_budget(600.0, 900.0, 400.0, metrics, 32.0),
            (400.0, 280.0)
        );
        assert_eq!(
            favorites_viewport_budget(10.0, 50.0, 400.0, metrics, 32.0),
            (1.0, 0.0)
        );
    }

    #[test]
    fn result_favorites_rejects_stale_receipts_and_retires_the_matching_save_once() {
        let mut state = ResultFavorites::default();
        assert!(!state.consume(Request::Load(1)));
        state.pending = Some(Request::Load(2));
        assert!(!state.consume(Request::Load(1)));
        assert!(state.consume(Request::Load(2)));
        state.pending = Some(Request::Write(3));
        assert!(state.write_pending());
        assert!(state.restart_blocker().is_some());
        assert!(!state.consume(Request::Load(3)));
        assert_eq!(state.pending, Some(Request::Write(3)));
        assert!(state.consume(Request::Write(3)));
        assert!(!state.consume(Request::Write(3)));
        assert!(!state.write_pending());
        assert!(state.restart_blocker().is_none());
    }

    #[test]
    fn remove_acknowledgement_is_matched_and_preserves_last_good_on_errors() {
        use super::{FavEntry, PrefilledFavoriteSaveError, WriteKind};
        let row = FavEntry {
            label: "same".into(),
            action: "cmd".into(),
            args: Some("one".into()),
        };
        let other = FavEntry {
            args: Some("two".into()),
            ..row.clone()
        };
        let mut state = ResultFavorites::default();
        state.rows = vec![row.clone(), other.clone()].into();
        state.loaded = true;
        state.pending = Some(Request::Write(4));
        assert!(!state.finish_write(Request::Write(3), WriteKind::Remove, Ok(vec![])));
        assert_eq!(&*state.rows, &[row.clone(), other.clone()]);
        assert!(state.finish_write(
            Request::Write(4),
            WriteKind::Remove,
            Err(PrefilledFavoriteSaveError::StaleTarget)
        ));
        assert!(state.loaded);
        assert!(state.status.as_deref().unwrap().contains("removed"));
        assert_eq!(&*state.rows, &[row.clone(), other.clone()]);
        state.pending = Some(Request::Write(5));
        assert!(state.finish_write(
            Request::Write(5),
            WriteKind::Remove,
            Err(PrefilledFavoriteSaveError::Store(anyhow::anyhow!(
                "malformed"
            )))
        ));
        assert!(!state.loaded);
        assert_eq!(&*state.rows, &[row.clone(), other]);
        assert!(state.status.as_deref().unwrap().contains("reopen"));
        state.loaded = true; // A fresh successful load re-enables mutations.
        state.pending = Some(Request::Write(6));
        assert!(state.finish_write(Request::Write(6), WriteKind::Remove, Ok(vec![row.clone()])));
        assert_eq!(&*state.rows, &[row]);
        assert_eq!(state.status.as_deref(), Some("Favorite removed."));
        assert!(!state.finish_write(Request::Write(6), WriteKind::Remove, Ok(vec![])));
    }

    #[test]
    fn result_favorites_cancels_prefill_without_discarding_browse_load() {
        let mut state = ResultFavorites::default();
        state.pending = Some(Request::Load(1));
        state.pending_draft = Some(super::OpenDraft::Prefilled(
            multi_launcher::actions::Action {
                label: "test".into(),
                desc: String::new(),
                action: "test".into(),
                args: None,
            },
        ));
        assert!(state.restart_blocker().is_some());
        assert!(state.cancel_draft());
        assert!(state.pending_draft.is_none());
        assert!(state.restart_blocker().is_none());
        assert_eq!(state.pending, Some(Request::Load(1)));
        assert!(state.consume(Request::Load(1)));
    }

    #[test]
    fn named_draft_cancellation_keeps_the_load_receipt() {
        let mut state = ResultFavorites::default();
        state.pending = Some(Request::Load(9));
        state.pending_draft = Some(super::OpenDraft::Named("Case".into()));
        assert!(state.restart_blocker().is_some());
        assert!(state.cancel_draft());
        assert_eq!(state.pending, Some(Request::Load(9)));
        assert!(state.consume(Request::Load(9)));
    }

    #[test]
    fn result_favorites_close_reopen_rejects_old_browse_receipt() {
        let mut state = ResultFavorites::default();
        let first = Request::Load(state.next());
        state.pending = Some(first);
        state.retire_load();
        let second = Request::Load(state.next());
        state.pending = Some(second);
        assert!(!state.consume(first));
        assert!(state.consume(second));
    }
}
