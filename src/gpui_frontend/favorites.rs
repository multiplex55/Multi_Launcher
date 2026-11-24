use std::sync::Arc;

use gpui_kit::base::Disableable;
use gpui_kit::component::{
    button::Button,
    input::{Textarea, TextareaState},
    label::Label,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use multi_launcher::actions::Action;
use multi_launcher::gui::theme::{SemanticPalette, ThemeMetrics};
use multi_launcher::plugins::fav::{
    FAV_FILE, FavEntry, ManualFavoriteOrigin, PrefilledFavoriteDraft, PrefilledFavoriteSaveError,
    favorite_row_target, load_favs, named_favorite_origin, prefilled_favorite_draft,
    save_manual_favorite, save_prefilled_favorite,
};

use super::{RootWindow, appearance};

pub(super) const FAVORITES_SAVE_PENDING: &str =
    "Wait for the Favorites save to finish before closing Multi Launcher.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Request {
    Load(u64),
    Save(u64),
}

#[derive(Default)]
pub(super) struct ResultFavorites {
    rows: Arc<[FavEntry]>,
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

    pub(super) fn save_pending(&self) -> bool {
        matches!(self.pending, Some(Request::Save(_)))
    }

    pub(super) fn restart_blocker(&self) -> Option<&'static str> {
        if self.save_pending() {
            Some("Wait for the Favorites save to finish before switching to egui.")
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

#[derive(Clone)]
struct FavoriteFields {
    origin: DraftOrigin,
    original: FavEntry,
    label: FavoriteField,
    command: FavoriteField,
    args: FavoriteField,
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
        window: &mut Window,
        cx: &mut Context<RootWindow>,
    ) -> Self {
        Self {
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
                            state.draft = Some(FavoriteFields::new(origin, original, window, cx));
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
            if state.sequence != sequence || state.rows.get(index) != Some(&displayed) {
                return;
            }
            let Some(target) = favorite_row_target(&state.rows, index) else {
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
        state.draft = Some(FavoriteFields::new(
            DraftOrigin::Manual(origin),
            entry,
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
        let entry = FavEntry {
            label: fields.label.exact_value(&fields.original.label, cx),
            action: fields.command.exact_value(&fields.original.action, cx),
            args: Some(args),
        };
        let origin = fields.origin.clone();
        let prefilled = matches!(&origin, DraftOrigin::Prefilled(_));
        let Some(permit) = self.restart_admission.admit() else {
            return;
        };
        let request = Request::Save(state.next());
        state.pending = Some(request);
        state.status = Some("Saving Favorite…".to_owned());
        if let Some(owner) = &state.owner {
            owner.focus(window, cx);
        }

        let task = cx.background_executor().spawn(async move {
            match origin {
                DraftOrigin::Prefilled(mut draft) => {
                    draft.label = entry.label;
                    draft.action = entry.action;
                    draft.args = Some(entry.args.unwrap_or_default());
                    save_prefilled_favorite(FAV_FILE, draft)
                }
                DraftOrigin::Manual(origin) => {
                    let mut entry = entry;
                    if entry
                        .args
                        .as_ref()
                        .is_some_and(|args| args.trim().is_empty())
                    {
                        entry.args = None;
                    }
                    save_manual_favorite(FAV_FILE, origin, entry)
                }
            }
        });
        let root = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let result = task.await;
            let _ = root.update(cx, |root, cx| {
                let state = &mut root.result_favorites;
                if !state.consume(request) {
                    return;
                }
                if root.restart_status.as_deref() == Some(FAVORITES_SAVE_PENDING) {
                    root.restart_status = None;
                }
                match result {
                    Ok(rows) => {
                        state.rows = rows.into();
                        state.draft = None;
                        state.status = Some("Favorite saved.".to_owned());
                    }
                    Err(error) => {
                        if matches!(error, PrefilledFavoriteSaveError::Store(_)) {
                            state.loaded = false;
                            state.status = Some(format!("Could not save Favorite: {error}. Close and reopen Favorites to reload."));
                        } else if !prefilled
                            && matches!(error, PrefilledFavoriteSaveError::StaleTarget)
                        {
                            state.status = Some(
                                "Favorites changed while this draft was open. Reopen it before saving."
                                    .to_owned(),
                            );
                        } else {
                            state.status = Some(format!("Could not save Favorite: {error}"));
                        }
                    }
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
        if state.save_pending() {
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

    pub(super) fn render_result_favorites(
        &self,
        width: f32,
        metrics: ThemeMetrics,
        palette: SemanticPalette,
        cx: &Context<Self>,
    ) -> Option<impl IntoElement> {
        let state = &self.result_favorites;
        let owner = state.owner.as_ref()?;
        let editing = state.draft.is_some();
        let waiting = state.save_pending();
        let dismiss_label = if editing || state.pending_draft.is_some() {
            "Cancel"
        } else {
            "Close"
        };
        let controls =
            div()
                .flex()
                .gap_x(px(metrics.item_spacing_y))
                .when(editing, |this| {
                    this.child(
                        Button::new("result-favorites-save")
                            .label("Save")
                            .tab_stop(true)
                            .disabled(waiting || !state.loaded)
                            .on_click(cx.listener(|root, _, window, cx| {
                                root.save_result_favorite(window, cx)
                            })),
                    )
                })
                .when(!editing, |this| {
                    this.child(
                        Button::new("result-favorites-add")
                            .label("Add Fav")
                            .tab_stop(true)
                            .disabled(!state.loaded || state.pending.is_some())
                            .on_click(cx.listener(|root, _, window, cx| {
                                root.edit_result_favorite(None, window, cx)
                            })),
                    )
                })
                .child(
                    Button::new("result-favorites-close")
                        .label(dismiss_label)
                        .tab_stop(true)
                        .disabled(waiting)
                        .on_click(cx.listener(|root, _, window, cx| {
                            root.dismiss_result_favorites(window, cx)
                        })),
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
            for (label, field) in [
                ("Label", &draft.label),
                ("Command", &draft.command),
                ("Args", &draft.args),
            ] {
                panel = panel.child(
                    div()
                        .id(label)
                        .flex()
                        .flex_col()
                        .child(Label::new(label).id("result-favorite-field-label"))
                        .child(
                            Textarea::new(&field.input)
                                .aria_label(label)
                                .tab_index(0)
                                .readonly(waiting),
                        ),
                );
            }
        } else if state.rows.is_empty() && state.pending.is_none() {
            panel = panel.child(Label::new("No Favorites.").id("result-favorites-empty"));
        } else if !state.rows.is_empty() {
            panel = panel.child(
                div()
                    .id("result-favorites-rows")
                    .flex()
                    .flex_col()
                    .max_h(px(160.0))
                    .overflow_y_scroll()
                    .children(state.rows.iter().enumerate().map(|(index, row)| {
                        let caption = match &row.args {
                            Some(args) => format!("{} · {} {}", row.label, row.action, args),
                            None => format!("{} · {}", row.label, row.action),
                        };
                        let displayed = row.clone();
                        let sequence = state.sequence;
                        div()
                            .flex()
                            .child(Label::new(caption).id(format!("result-favorite-row-{index}")))
                            .child(
                                Button::new(SharedString::from(format!(
                                    "result-favorite-edit-{index}"
                                )))
                                .label("Edit")
                                .tab_stop(true)
                                .disabled(!state.loaded || state.pending.is_some())
                                .on_click(cx.listener(
                                    move |root, _, window, cx| {
                                        root.edit_result_favorite(
                                            Some((index, displayed.clone(), sequence)),
                                            window,
                                            cx,
                                        )
                                    },
                                )),
                            )
                    })),
            );
        }
        Some(
            panel
                .children(
                    state
                        .status
                        .as_ref()
                        .map(|status| Label::new(status.clone()).id("result-favorites-status")),
                )
                .child(controls),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{Request, ResultFavorites};

    #[test]
    fn result_favorites_rejects_stale_receipts_and_retires_the_matching_save_once() {
        let mut state = ResultFavorites::default();
        assert!(!state.consume(Request::Load(1)));
        state.pending = Some(Request::Load(2));
        assert!(!state.consume(Request::Load(1)));
        assert!(state.consume(Request::Load(2)));
        state.pending = Some(Request::Save(3));
        assert!(state.save_pending());
        assert!(state.restart_blocker().is_some());
        assert!(!state.consume(Request::Load(3)));
        assert_eq!(state.pending, Some(Request::Save(3)));
        assert!(state.consume(Request::Save(3)));
        assert!(!state.consume(Request::Save(3)));
        assert!(!state.save_pending());
        assert!(state.restart_blocker().is_none());
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
