//! Mount-owned geometry collection for the stock Hash Select popup rows.
use super::{
    HashAlgorithmChoice, HashGeneratorWindow, HashSourceChoice, PhysicalKeydown, appearance,
    context_stack_has_primary,
    hash_jump::{HashJump, HashJumpScope},
    jump_mode::{
        JumpGeometryDraft, JumpGeometryElement, JumpGeometryFrame, JumpTargetGeometry,
        jump_badge_height, jump_hint_badge,
    },
};
#[cfg(windows)]
use super::{checked_native_window, current_foreground_identity, native_window_is_valid};
use gpui_kit::{
    App, Context, FocusHandle, Focusable, InteractiveElement, IntoElement, ParentElement,
    SharedString, Styled, WeakEntity, Window, component::searchable_list::SearchableListItem, div,
    px,
};
use multi_launcher::gui::theme::ThemeMetrics;
use multi_launcher::launcher::jump_mode::{
    SemanticTargetId, SurfaceId, TargetGeneration, TargetId, TargetKind, TargetMetadata,
};
use std::{cell::RefCell, rc::Rc, sync::atomic::Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HashPopupKind {
    Source,
    Algorithm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CursorMove {
    Up,
    Down,
}

pub(super) fn cursor_move(current: Option<usize>, target: usize) -> Option<CursorMove> {
    if current == Some(target) {
        None
    } else if current.is_some_and(|current| current > target) {
        Some(CursorMove::Up)
    } else {
        Some(CursorMove::Down)
    }
}

pub(super) struct PopupCollector {
    enabled: bool,
    kind: HashPopupKind,
    mount: u64,
    pub(super) epoch: u64,
    owner: Option<WeakEntity<HashGeneratorWindow>>,
    pub(super) trigger: Option<FocusHandle>,
    pending: Option<JumpGeometryDraft>,
    pub frame: Rc<RefCell<Option<JumpGeometryFrame>>>,
}

impl PopupCollector {
    pub fn new(kind: HashPopupKind, mount: u64, enabled: bool) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Self {
            enabled,
            kind,
            mount,
            epoch: 1,
            owner: None,
            trigger: None,
            pending: None,
            frame: Rc::new(RefCell::new(None)),
        }))
    }
    pub fn bind(&mut self, owner: WeakEntity<HashGeneratorWindow>, trigger: FocusHandle) {
        self.owner = Some(owner);
        self.trigger = Some(trigger);
    }
    pub fn dismiss(&mut self) {
        self.epoch = self.epoch.wrapping_add(1).max(1);
        self.pending = None;
        *self.frame.borrow_mut() = None;
    }
    pub(super) fn target(&self, index: usize) -> TargetId {
        let owner = SurfaceId::dialog("hash-generator", self.mount);
        TargetId::new(
            SurfaceId::popup(
                match self.kind {
                    HashPopupKind::Source => "hash-source",
                    HashPopupKind::Algorithm => "hash-algorithm",
                },
                self.epoch,
                owner,
            ),
            SemanticTargetId::named(format!("item-{index}")),
            TargetGeneration(self.epoch),
        )
    }

    pub(super) fn pending_frame(
        &self,
        jump: &HashJump,
        metrics: ThemeMetrics,
    ) -> Option<JumpGeometryFrame> {
        self.pending
            .as_ref()
            .map(|draft| self.finish_frame(draft, jump, metrics))
    }

    fn finish_frame(
        &self,
        draft: &JumpGeometryDraft,
        jump: &HashJump,
        metrics: ThemeMetrics,
    ) -> JumpGeometryFrame {
        let active = jump.scope == Some(HashJumpScope::Popup(self.kind, self.epoch));
        let expected = if active {
            jump.controller.target_ids()
        } else {
            &[]
        };
        draft.finish(
            active.then_some(self.epoch),
            expected,
            &jump.controller.alphabet,
            metrics,
        )
    }
}

#[derive(Clone)]
pub(super) struct HashSelectItem<T> {
    pub value: T,
    pub index: usize,
    pub collector: Rc<RefCell<PopupCollector>>,
}

impl<T> SearchableListItem for HashSelectItem<T>
where
    T: SearchableListItem<Value = T> + Clone + PartialEq + 'static,
{
    type Value = T;
    fn title(&self) -> SharedString {
        self.value.title()
    }
    fn value(&self) -> &T {
        &self.value
    }
    fn render(&self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let inner = self.value.render(window, cx).into_any_element();
        if !self.collector.borrow().enabled {
            return inner;
        }
        let (target, owner, frame, scope) = {
            let state = self.collector.borrow();
            (
                state.target(self.index),
                state.owner.clone(),
                Rc::clone(&state.frame),
                HashJumpScope::Popup(state.kind, state.epoch),
            )
        };
        let hint = owner.and_then(|owner| owner.upgrade()).and_then(|owner| {
            let owner = owner.read(cx);
            (owner.jump.scope == Some(scope))
                .then(|| owner.jump.controller.hint_for(&target).cloned())
                .flatten()
        });
        let appearance = appearance::current(cx);
        let badge = jump_hint_badge(
            hint.as_ref(),
            &target,
            target.generation().0,
            frame,
            appearance.palette,
            appearance.metrics,
        );
        let inner = div()
            .id(format!(
                "hash-popup-row-{}-{}-{}",
                match scope {
                    HashJumpScope::Popup(HashPopupKind::Source, _) => "source",
                    _ => "algorithm",
                },
                target.generation().0,
                self.index
            ))
            .relative()
            .flex()
            .items_center()
            .w_full()
            .min_w(px(0.0))
            .min_h(px(jump_badge_height(appearance.metrics)))
            .child(inner)
            .children(badge)
            .into_any_element();
        let epoch = target.generation().0;
        let metadata = TargetMetadata::new(
            target,
            self.title().to_string(),
            TargetKind::Selectable,
            self.index as u64,
        );
        let collector = Rc::clone(&self.collector);
        JumpGeometryElement::observe(inner, metadata, move |geometry, window, cx| {
            collect_popup_row(&collector, epoch, geometry, window, cx);
        })
        .into_any_element()
    }
}

fn collect_popup_row(
    collector: &Rc<RefCell<PopupCollector>>,
    epoch: u64,
    geometry: Option<JumpTargetGeometry>,
    window: &Window,
    cx: &mut App,
) {
    let schedule = {
        let mut collector = collector.borrow_mut();
        if collector.epoch != epoch {
            return;
        }
        let schedule = collector.pending.is_none();
        let draft = collector
            .pending
            .get_or_insert_with(|| JumpGeometryDraft::new(epoch));
        if let Some(target) = geometry
            && !draft
                .targets
                .iter()
                .any(|prior| prior.metadata.id() == target.metadata.id())
        {
            draft.targets.push(target);
        }
        schedule
    };
    if !schedule {
        return;
    }

    // GPUI drains the outer effect queue after all deferred popup rows
    // have prepainted. Revalidate the opening after releasing the collector.
    let collector = Rc::clone(collector);
    let window_id = window.window_handle().window_id();
    window.defer(cx, move |window, app| {
        let (owner, kind, mount, draft) = {
            let mut state = collector.borrow_mut();
            if state.epoch != epoch {
                return;
            }
            (
                state.owner.clone(),
                state.kind,
                state.mount,
                state.pending.take(),
            )
        };
        let (Some(owner), Some(draft)) = (owner, draft) else {
            return;
        };
        let _ = owner.update(app, |owner, cx| {
            if window.window_handle().window_id() != window_id
                || owner.mount_generation != mount
                || !owner.hash_popup_scope(window, cx).is_some_and(
                    |(focused_kind, focused_epoch, _)| {
                        focused_kind == kind && focused_epoch == epoch
                    },
                )
            {
                return;
            }
            let metrics = appearance::current(cx).metrics;
            let scope = HashJumpScope::Popup(kind, epoch);
            let active = owner.jump.scope == Some(scope);
            let frame = collector
                .borrow()
                .finish_frame(&draft, &owner.jump, metrics);
            let retired = active && owner.jump.reconcile_frame(Some(&frame));
            let repaint = {
                let state = collector.borrow();
                let changed = state.frame.borrow().as_ref().is_none_or(|old| {
                    !old.same_geometry(&frame)
                        || old.matches_active_session != frame.matches_active_session
                });
                *state.frame.borrow_mut() = Some(frame);
                retired || (active && changed)
            };
            if repaint {
                match kind {
                    HashPopupKind::Source => owner.source.update(cx, |_, cx| cx.notify()),
                    HashPopupKind::Algorithm => owner.algorithm.update(cx, |_, cx| cx.notify()),
                }
                cx.notify();
            }
        });
    });
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum HashPopupCommitted {
    Source(Option<HashSourceChoice>),
    Algorithm(Option<HashAlgorithmChoice>),
}

impl HashGeneratorWindow {
    pub(super) fn hash_popup_scope(
        &self,
        window: &Window,
        cx: &App,
    ) -> Option<(HashPopupKind, u64, FocusHandle)> {
        if !self.is_current_mount()
            || !window.is_window_active()
            || self.file_picker_pending.load(Ordering::Acquire)
            || !context_stack_has_primary(&window.context_stack(), "List")
            || context_stack_has_primary(&window.context_stack(), "AlertDialog")
            || context_stack_has_primary(&window.context_stack(), "Dialog")
        {
            return None;
        }
        for (kind, collector, focus) in [
            (
                HashPopupKind::Source,
                &self.source_popup,
                self.source.read(cx).focus_handle(cx),
            ),
            (
                HashPopupKind::Algorithm,
                &self.algorithm_popup,
                self.algorithm.read(cx).focus_handle(cx),
            ),
        ] {
            let collector = collector.borrow();
            if collector
                .trigger
                .as_ref()
                .is_some_and(|trigger| *trigger != focus)
                && focus.is_focused(window)
            {
                return Some((kind, collector.epoch, focus));
            }
        }
        None
    }

    fn popup_cursor_snapshot(
        &self,
        kind: HashPopupKind,
        epoch: u64,
        target: &TargetId,
        window: &Window,
        cx: &App,
    ) -> Option<(FocusHandle, Option<usize>, HashPopupCommitted)> {
        let (current_kind, current_epoch, focus) = self.hash_popup_scope(window, cx)?;
        if current_kind != kind || current_epoch != epoch {
            return None;
        }
        #[cfg(windows)]
        {
            let native = checked_native_window(window).ok().flatten()?;
            if !native_window_is_valid(native)
                || current_foreground_identity() != Some(native.identity)
            {
                return None;
            }
        }
        let collector = match kind {
            HashPopupKind::Source => &self.source_popup,
            HashPopupKind::Algorithm => &self.algorithm_popup,
        };
        let collector = collector.borrow();
        let pending = collector.pending_frame(&self.jump, appearance::current(cx).metrics);
        let frame = collector.frame.borrow();
        if !pending
            .as_ref()
            .or(frame.as_ref())
            .is_some_and(|frame| frame.generation == epoch && frame.has_target(target))
        {
            return None;
        }
        let (cursor, committed) = match kind {
            HashPopupKind::Source => {
                let state = self.source.read(cx);
                (
                    state.selected_index(cx).map(|index| index.row),
                    HashPopupCommitted::Source(state.selected_value().copied()),
                )
            }
            HashPopupKind::Algorithm => {
                let state = self.algorithm.read(cx);
                (
                    state.selected_index(cx).map(|index| index.row),
                    HashPopupCommitted::Algorithm(state.selected_value().copied()),
                )
            }
        };
        Some((focus, cursor, committed))
    }

    fn schedule_popup_cursor(
        &self,
        kind: HashPopupKind,
        epoch: u64,
        target: TargetId,
        row: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if row >= 3 {
            return;
        }
        let owner = cx.entity().downgrade();
        let window_id = window.window_handle().window_id();
        window.defer(cx, move |window, app| {
            let mut original = None;
            for _ in 0..4 {
                if window.window_handle().window_id() != window_id {
                    return;
                }
                let snapshot = owner
                    .update(app, |owner, cx| {
                        owner.popup_cursor_snapshot(kind, epoch, &target, window, cx)
                    })
                    .ok()
                    .flatten();
                let Some((focus, cursor, committed)) = snapshot else {
                    return;
                };
                if let Some(prior) = original.as_ref() {
                    if prior != &committed {
                        return;
                    }
                } else {
                    original = Some(committed);
                }
                let Some(movement) = cursor_move(cursor, row) else {
                    return;
                };
                let before = cursor;
                match movement {
                    CursorMove::Up => {
                        focus.dispatch_action(&gpui_kit::base::actions::SelectUp, window, app)
                    }
                    CursorMove::Down => {
                        focus.dispatch_action(&gpui_kit::base::actions::SelectDown, window, app)
                    }
                }
                let next = owner
                    .update(app, |owner, cx| {
                        owner.popup_cursor_snapshot(kind, epoch, &target, window, cx)
                    })
                    .ok()
                    .flatten();
                if !next.is_some_and(|(next_focus, next_cursor, committed)| {
                    next_focus == focus
                        && next_cursor != before
                        && original.as_ref() == Some(&committed)
                }) {
                    return;
                }
            }
        });
    }

    pub(super) fn handle_popup_jump_keydown(
        &mut self,
        receipt: PhysicalKeydown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<bool> {
        let (kind, epoch, _) = self.hash_popup_scope(window, cx)?;
        let collector = match kind {
            HashPopupKind::Source => &self.source_popup,
            HashPopupKind::Algorithm => &self.algorithm_popup,
        };
        // A key can draw a dirty popup before GPUI drains its deferred
        // publication. The just-prepainted bounds take precedence.
        let (frame, pending) = {
            let collector = collector.borrow();
            (
                Rc::clone(&collector.frame),
                collector.pending_frame(&self.jump, appearance::current(cx).metrics),
            )
        };
        let scope = HashJumpScope::Popup(kind, epoch);
        let (consumed, repaint, target) = self.jump.key(
            receipt,
            scope,
            pending.as_ref().or(frame.borrow().as_ref()),
            epoch,
        );
        if let Some(target) = target {
            if let SemanticTargetId::Named(name) = target.semantic()
                && let Some(row) = name
                    .strip_prefix("item-")
                    .and_then(|v| v.parse::<usize>().ok())
            {
                self.schedule_popup_cursor(kind, epoch, target.clone(), row, window, cx);
            }
        }
        if repaint {
            match kind {
                HashPopupKind::Source => self.source.update(cx, |_, cx| cx.notify()),
                HashPopupKind::Algorithm => self.algorithm.update(cx, |_, cx| cx.notify()),
            }
            cx.notify();
        }
        Some(consumed)
    }
}
