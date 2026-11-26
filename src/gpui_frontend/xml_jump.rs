//! XML-specific scope and focus-only targets over the shared Jump transport.
use super::xml_utility::XmlWorkspace;
use super::*;
use multi_launcher::launcher::jump_mode::{SemanticTargetId, TargetKind};
use std::{cell::RefCell, rc::Rc};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Scope {
    source: bool,
    xpath: bool,
    run: bool,
    save: bool,
    blocked: bool,
}
impl Scope {
    pub fn new(view: &XmlWorkspace) -> Self {
        let snapshot = view.current_snapshot();
        let blocked = !view.open
            || view.rehosting
            || view.admission.is_fenced()
            || view.save_review_pending()
            || view.file_effect_pending()
            || view.clipboard_pending;
        let run = view.xpath.namespace_error().is_none()
            && snapshot.as_ref().is_some_and(|s| {
                matches!(
                    s.tree,
                    multi_launcher::xml_toolkit_session::XmlTreeStatus::Ready(_)
                ) && s.parse == multi_launcher::xml_toolkit_session::XmlParseStatus::Ready
            });
        Self {
            source: !view.output_tab && !view.tree_tab && !view.xpath_tab && !view.schema_tab,
            xpath: view.xpath_tab,
            run,
            save: !blocked && snapshot.is_some(),
            blocked,
        }
    }
}
#[derive(Clone, Copy)]
pub(super) enum Role {
    Source,
    XPathTab,
    Expression,
    Run,
    Save,
}
impl Role {
    fn spec(self) -> (&'static str, &'static str, TargetKind, u64) {
        match self {
            Self::Source => ("source", "XML source", TargetKind::TextEntry, 0),
            Self::XPathTab => ("xpath-tab", "XPath", TargetKind::Button, 1),
            Self::Expression => ("expression", "XPath expression", TargetKind::TextEntry, 2),
            Self::Run => ("run", "Run XPath", TargetKind::Button, 3),
            Self::Save => ("save", "Save Source As", TargetKind::Button, 4),
        }
    }
    fn allowed(self, scope: Scope) -> bool {
        !scope.blocked
            && match self {
                Self::Source => scope.source,
                Self::XPathTab => true,
                Self::Expression => scope.xpath,
                Self::Run => scope.xpath && scope.run,
                Self::Save => scope.save,
            }
    }
}
pub(super) struct XmlJump {
    pub core: super::scoped_jump::ScopedJump,
    scope: Option<Scope>,
    pub save_focus: FocusHandle,
    pub run_focus: FocusHandle,
}
impl XmlJump {
    pub fn new(controller: JumpModeController, cx: &mut Context<XmlWorkspace>) -> Self {
        Self {
            core: super::scoped_jump::ScopedJump::new("xml-toolkit", controller),
            scope: None,
            save_focus: cx.focus_handle(),
            run_focus: cx.focus_handle(),
        }
    }
    pub fn begin(&mut self, scope: Scope) -> Option<Rc<RefCell<JumpGeometryDraft>>> {
        if self.scope != Some(scope) {
            self.scope = Some(scope);
            self.core.controller.retire();
            *self.core.frame.borrow_mut() = None;
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
        let (name, label, kind, order) = role.spec();
        self.core.wrap(
            element,
            name.into(),
            label,
            kind,
            order,
            draft.filter(|_| self.scope.is_some_and(|scope| role.allowed(scope))),
            app,
        )
    }
    pub fn publish(
        &self,
        draft: Rc<RefCell<JumpGeometryDraft>>,
        receipt: utility_mount::MountReceipt,
        host: WindowId,
        scope: Scope,
        cx: &mut Context<XmlWorkspace>,
    ) -> AnyElement {
        self.core
            .publish(draft, cx, move |view, frame, window, cx, changed| {
                if !receipt.is_current()
                    || !receipt.matches(&view.mount)
                    || host != window.window_handle().window_id()
                    || view.host.jump.scope != Some(scope)
                    || Scope::new(view) != scope
                {
                    return;
                }
                if !view.contains_focus(window, cx) || !window.is_window_active() {
                    view.retire_jump(cx);
                    return;
                }
                let retired = view.host.jump.core.controller.reconcile_frame(Some(frame));
                if retired
                    || (changed
                        && view.host.jump.core.controller.is_active()
                        && frame.matches_active_session)
                {
                    cx.notify();
                }
            })
    }
}
impl XmlWorkspace {
    pub(super) fn retire_jump(&mut self, cx: &mut Context<Self>) {
        let parent = self.host.jump.core.controller.retire();
        let child = self.retire_review_jump();
        if parent || child {
            cx.notify();
        }
    }
    pub(super) fn handle_xml_jump(
        &mut self,
        receipt: PhysicalKeydown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self
            .host
            .jump
            .core
            .controller
            .should_process_key_down(receipt)
        {
            return false;
        }
        if !self.mount.is_current()
            || !self.contains_focus(window, cx)
            || !window.is_window_active()
            || window.has_active_dialog(cx)
        {
            self.retire_jump(cx);
            return false;
        }
        let live = Scope::new(self);
        if self.host.jump.scope != Some(live) || live.blocked {
            self.retire_jump(cx);
            *self.host.jump.core.frame.borrow_mut() = None;
            if self.host.jump.core.controller.owns_key(receipt.virtual_key) {
                if receipt.held {
                    return true;
                }
                self.host.jump.core.controller.release_fresh_key(receipt);
            }
            return false;
        }
        let slot = self.host.jump.core.frame.clone();
        let frame = slot.borrow();
        let retired = self
            .host
            .jump
            .core
            .controller
            .reconcile_frame(frame.as_ref());
        let generation = self.host.jump.core.generation;
        let outcome =
            self.host
                .jump
                .core
                .controller
                .process_key_down(receipt, frame.as_ref(), generation);
        if let Some(target) = outcome
            .selected_target
            .filter(|target| frame.as_ref().is_some_and(|frame| frame.has_target(target)))
        {
            if let SemanticTargetId::Named(name) = target.semantic() {
                for role in [
                    Role::Source,
                    Role::XPathTab,
                    Role::Expression,
                    Role::Run,
                    Role::Save,
                ] {
                    if role.spec().0 == name && role.allowed(live) {
                        match role {
                            Role::Source => self.host.input.focus_handle(cx).focus(window, cx),
                            Role::XPathTab => self.host.xpath_focus.focus(window, cx),
                            Role::Expression => self
                                .host
                                .xpath_expression
                                .focus_handle(cx)
                                .focus(window, cx),
                            Role::Run => self.host.jump.run_focus.focus(window, cx),
                            Role::Save => self.host.jump.save_focus.focus(window, cx),
                        }
                    }
                }
            }
        }
        if retired || outcome.repaint {
            cx.notify();
        }
        outcome.consumed
    }
}
#[cfg(test)]
mod tests {
    use super::{Role, Scope};
    #[test]
    fn blocked_review_or_chooser_denies_every_parent_target() {
        let scope = Scope {
            source: true,
            xpath: true,
            run: true,
            save: true,
            blocked: true,
        };
        for role in [
            Role::Source,
            Role::XPathTab,
            Role::Expression,
            Role::Run,
            Role::Save,
        ] {
            assert!(!role.allowed(scope));
        }
    }
    #[test]
    fn unavailable_actions_and_hidden_tabs_are_not_targets() {
        let scope = Scope {
            source: true,
            xpath: false,
            run: false,
            save: false,
            blocked: false,
        };
        assert!(Role::Source.allowed(scope));
        assert!(Role::XPathTab.allowed(scope));
        assert!(!Role::Expression.allowed(scope));
        assert!(!Role::Run.allowed(scope));
        assert!(!Role::Save.allowed(scope));
    }
}
