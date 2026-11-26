//! Regex workspace Jump scope, with exact evaluated match targets.
use super::regex_tester::RegexWorkspace;
use super::*;
use multi_launcher::launcher::jump_mode::{SemanticTargetId, TargetId, TargetKind};
use multi_launcher::regex_tester::MatchId;
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Scope {
    revision: Option<u64>,
    count: usize,
    text: bool,
    blocked: bool,
}
impl Scope {
    pub fn new(view: &RegexWorkspace) -> Self {
        Self {
            revision: view.session.evaluated_revision(),
            count: view.session.matches().len(),
            text: view.session.draft.test_text.len() <= view.session.policy().text_bytes,
            blocked: !view.open
                || view.rehosting
                || view.admission.is_fenced()
                || view.presets.has_confirmation(),
        }
    }
}
#[derive(Clone, Copy)]
pub(super) enum Role {
    Pattern,
    Text,
    Previous,
    Next,
    Match(u64, MatchId),
}
impl Role {
    fn name(self) -> String {
        match self {
            Self::Pattern => "pattern".into(),
            Self::Text => "text".into(),
            Self::Previous => "previous".into(),
            Self::Next => "next".into(),
            Self::Match(revision, id) => format!("match-{revision}-{}", id.index()),
        }
    }
    fn metadata(self) -> (&'static str, TargetKind, u64) {
        match self {
            Self::Pattern => ("Pattern", TargetKind::TextEntry, 0),
            Self::Text => ("Test Text", TargetKind::TextEntry, 1),
            Self::Previous => ("Previous match", TargetKind::Button, 2),
            Self::Next => ("Next match", TargetKind::Button, 3),
            Self::Match(_, id) => ("Regex match", TargetKind::Button, 100 + id.index() as u64),
        }
    }
    fn allowed(self, scope: Scope) -> bool {
        !scope.blocked
            && match self {
                Self::Pattern => true,
                Self::Text => scope.text,
                Self::Previous | Self::Next => scope.revision.is_some() && scope.count > 0,
                Self::Match(revision, _) => scope.revision == Some(revision) && scope.count > 0,
            }
    }
}
pub(super) fn exact_match_index(
    session: &multi_launcher::regex_tester::RegexSession,
    revision: u64,
    id: MatchId,
) -> Option<usize> {
    (session.evaluated_revision() == Some(revision))
        .then(|| session.matches().iter().position(|item| item.id == id))
        .flatten()
}
fn review_held_activation(held: bool, unmodified: bool, key: u16, review_focused: bool) -> bool {
    held && unmodified && review_focused && matches!(key, 0x0d | 0x20)
}
type RowKey = (u64, MatchId);
pub(super) struct RegexJump {
    pub core: super::scoped_jump::ScopedJump,
    scope: Option<Scope>,
    pub previous: FocusHandle,
    pub next: FocusHandle,
    rows: Rc<RefCell<HashMap<TargetId, (u64, MatchId, FocusHandle)>>>,
    focus: Rc<RefCell<HashMap<RowKey, FocusHandle>>>,
    rendered: Rc<RefCell<HashSet<RowKey>>>,
}
impl RegexJump {
    pub fn new(controller: JumpModeController, cx: &mut Context<RegexWorkspace>) -> Self {
        Self {
            core: super::scoped_jump::ScopedJump::new("regex-tester", controller),
            scope: None,
            previous: cx.focus_handle(),
            next: cx.focus_handle(),
            rows: Default::default(),
            focus: Default::default(),
            rendered: Default::default(),
        }
    }
    pub fn begin(
        &mut self,
        scope: Scope,
        window: &Window,
    ) -> Option<Rc<RefCell<JumpGeometryDraft>>> {
        let rendered = self.rendered.borrow();
        self.focus
            .borrow_mut()
            .retain(|key, focus| rendered.contains(key) || focus.is_focused(window));
        drop(rendered);
        self.rendered.borrow_mut().clear();
        if self.scope != Some(scope) {
            self.scope = Some(scope);
            self.core.controller.retire();
            *self.core.frame.borrow_mut() = None;
            self.rows.borrow_mut().clear();
            self.focus
                .borrow_mut()
                .retain(|(revision, _), _| Some(*revision) == scope.revision);
        }
        if scope.blocked {
            None
        } else {
            self.core.begin()
        }
    }
    pub fn wrap<E: IntoElement>(
        &self,
        element: E,
        role: Role,
        draft: Option<Rc<RefCell<JumpGeometryDraft>>>,
        app: &App,
    ) -> AnyElement {
        let (label, kind, order) = role.metadata();
        self.core.wrap(
            element,
            role.name(),
            label,
            kind,
            order,
            draft.filter(|_| self.scope.is_some_and(|scope| role.allowed(scope))),
            app,
        )
    }
    pub fn row_focus(&self, revision: u64, id: MatchId, app: &App) -> FocusHandle {
        let key = (revision, id);
        self.rendered.borrow_mut().insert(key);
        let focus = self
            .focus
            .borrow_mut()
            .entry(key)
            .or_insert_with(|| app.focus_handle())
            .clone();
        self.rows.borrow_mut().insert(
            self.core.id(Role::Match(revision, id).name()),
            (revision, id, focus.clone()),
        );
        focus
    }
    pub fn publish(
        &self,
        draft: Rc<RefCell<JumpGeometryDraft>>,
        mount: utility_mount::MountReceipt,
        host: WindowId,
        scope: Scope,
        cx: &mut Context<RegexWorkspace>,
    ) -> AnyElement {
        self.core
            .publish(draft, cx, move |view, frame, window, cx, changed| {
                if !mount.is_current()
                    || !mount.matches(&view.mount.receipt)
                    || host != window.window_handle().window_id()
                    || view.mount.jump.scope != Some(scope)
                    || Scope::new(view) != scope
                {
                    return;
                }
                view.mount
                    .jump
                    .rows
                    .borrow_mut()
                    .retain(|id, _| frame.has_target(id));
                let rendered = view.mount.jump.rendered.borrow();
                view.mount
                    .jump
                    .focus
                    .borrow_mut()
                    .retain(|key, focus| rendered.contains(key) || focus.is_focused(window));
                drop(rendered);
                if scope.blocked || !view.contains_focus(window, cx) || !window.is_window_active() {
                    view.retire_jump(cx);
                    return;
                }
                let retired = view.mount.jump.core.controller.reconcile_frame(Some(frame));
                if retired
                    || (changed
                        && view.mount.jump.core.controller.is_active()
                        && frame.matches_active_session)
                {
                    cx.notify();
                }
            })
    }
}
impl RegexWorkspace {
    pub(super) fn retire_jump(&mut self, cx: &mut Context<Self>) {
        if self.mount.jump.core.controller.retire() {
            cx.notify();
        }
    }
    pub(super) fn handle_regex_jump(
        &mut self,
        receipt: PhysicalKeydown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.presets.has_confirmation() {
            if !self.open
                || self.rehosting
                || self.admission.is_fenced()
                || !self.mount.receipt.is_current()
                || self.mount.window.window_id() != window.window_handle().window_id()
                || !window.is_window_active()
                || !self.contains_focus(window, cx)
                || window.has_active_dialog(cx)
            {
                return false;
            }
            let focus = &self.mount.preset_confirmation_focus;
            let review_focused = focus.is_focused(window) || focus.contains_focused(window, cx);
            if review_held_activation(
                receipt.held,
                receipt.modifiers == Modifiers::default(),
                receipt.virtual_key,
                review_focused,
            ) {
                return true;
            }
            return super::scoped_jump::consume_owned_held(
                &mut self.mount.jump.core.controller,
                receipt,
            );
        }
        let live = Scope::new(self);
        if live.blocked
            || !self.mount.receipt.is_current()
            || self.mount.window.window_id() != window.window_handle().window_id()
            || !self.contains_focus(window, cx)
            || !window.is_window_active()
            || window.has_active_dialog(cx)
        {
            self.retire_jump(cx);
            return false;
        }
        let navigation_focused =
            self.mount.jump.previous.is_focused(window) || self.mount.jump.next.is_focused(window);
        let held_nav = live.count > 0
            && review_held_activation(
                receipt.held,
                receipt.modifiers == Modifiers::default(),
                receipt.virtual_key,
                navigation_focused,
            );
        if !self
            .mount
            .jump
            .core
            .controller
            .should_process_key_down(receipt)
        {
            return held_nav;
        }
        if self.mount.jump.scope != Some(live) {
            self.retire_jump(cx);
            *self.mount.jump.core.frame.borrow_mut() = None;
            return super::scoped_jump::consume_owned_held(
                &mut self.mount.jump.core.controller,
                receipt,
            ) || held_nav;
        }
        let slot = self.mount.jump.core.frame.clone();
        let frame = slot.borrow();
        let generation = self.mount.jump.core.generation;
        let retired = self
            .mount
            .jump
            .core
            .controller
            .reconcile_frame(frame.as_ref());
        let outcome =
            self.mount
                .jump
                .core
                .controller
                .process_key_down(receipt, frame.as_ref(), generation);
        if let Some(target) = outcome
            .selected_target
            .filter(|target| frame.as_ref().is_some_and(|frame| frame.has_target(target)))
        {
            match target.semantic() {
                SemanticTargetId::Named(name) if name == "pattern" => {
                    self.mount.pattern.focus_handle(cx).focus(window, cx)
                }
                SemanticTargetId::Named(name) if name == "text" && live.text => {
                    self.mount.text.focus_handle(cx).focus(window, cx)
                }
                SemanticTargetId::Named(name) if name == "previous" && live.count > 0 => {
                    self.mount.jump.previous.focus(window, cx)
                }
                SemanticTargetId::Named(name) if name == "next" && live.count > 0 => {
                    self.mount.jump.next.focus(window, cx)
                }
                _ => {
                    let row = self.mount.jump.rows.borrow().get(&target).cloned();
                    if let Some((revision, id, focus)) = row {
                        if self.select_exact_match(revision, id, cx) {
                            focus.focus(window, cx);
                        }
                    }
                }
            }
        }
        if retired || outcome.repaint {
            cx.notify();
        }
        outcome.consumed || held_nav
    }
}
#[cfg(test)]
mod tests {
    use super::{Role, Scope, exact_match_index, review_held_activation};
    use multi_launcher::regex_tester::MatchId;
    #[test]
    fn review_blocks_only_held_unmodified_activation_in_its_focus_scope() {
        assert!(review_held_activation(true, true, 0x0d, true));
        assert!(review_held_activation(true, true, 0x20, true));
        assert!(!review_held_activation(false, true, 0x0d, true));
        assert!(!review_held_activation(true, true, 0x1b, true));
        assert!(!review_held_activation(true, false, 0x20, true));
        assert!(!review_held_activation(true, true, 0x20, false));
    }
    #[test]
    fn replaced_evaluation_rejects_reused_match_id() {
        use multi_launcher::regex_tester::RegexSession;
        use std::time::{Duration, Instant};
        let mut session = RegexSession::default();
        let now = Instant::now();
        session.draft.pattern = "a".into();
        session.draft.test_text = "a".into();
        session.mark_changed(now);
        session.tick(now + Duration::from_millis(200));
        let old = session.evaluated_revision().unwrap();
        let id = session.matches()[0].id;
        assert_eq!(exact_match_index(&session, old, id), Some(0));
        session.draft.test_text = "aa".into();
        session.mark_changed(now + Duration::from_millis(300));
        assert_eq!(exact_match_index(&session, old, id), None);
        session.tick(now + Duration::from_millis(500));
        assert_eq!(session.matches()[0].id, id);
        assert_eq!(exact_match_index(&session, old, id), None);
        assert_eq!(
            exact_match_index(&session, session.evaluated_revision().unwrap(), id),
            Some(0)
        );
    }
    #[test]
    fn stale_and_reviewed_matches_are_not_eligible() {
        let live = Scope {
            revision: Some(7),
            count: 2,
            text: true,
            blocked: false,
        };
        assert!(Role::Match(7, MatchId::new(1)).allowed(live));
        assert!(!Role::Match(6, MatchId::new(1)).allowed(live));
        let reviewed = Scope {
            blocked: true,
            ..live
        };
        assert!(!Role::Pattern.allowed(reviewed));
        assert!(!Role::Match(7, MatchId::new(1)).allowed(reviewed));
        assert!(!Role::Text.allowed(Scope {
            text: false,
            ..live
        }));
    }
}
