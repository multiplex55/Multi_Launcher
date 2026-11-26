//! JSON scoped Jump adapter for exact mounted controls and visible tree rows.
use super::json_utility::JsonWorkspace;
use super::*;
use multi_launcher::json_transform::JsonTreeNodeId;
use multi_launcher::json_utility_session::TreeStamp;
use multi_launcher::launcher::jump_mode::{
    SemanticTargetId, SurfaceId, TargetGeneration, TargetId, TargetKind, TargetMetadata,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
static NEXT_MOUNT: AtomicU64 = AtomicU64::new(1);
#[derive(Clone, Copy)]
pub(super) enum Role {
    Source,
    Search,
    Format,
    Row(JsonTreeNodeId, TreeStamp),
}
impl Role {
    fn name(self) -> String {
        match self {
            Self::Source => "source".into(),
            Self::Search => "search".into(),
            Self::Format => "format".into(),
            Self::Row(id, stamp) => format!("node-{}-{stamp:?}", id.index()),
        }
    }
    fn metadata(self) -> (&'static str, TargetKind, u64) {
        match self {
            Self::Source => ("JSON text", TargetKind::TextEntry, 0),
            Self::Search => ("Tree search", TargetKind::TextEntry, 1),
            Self::Format => ("Format", TargetKind::Button, 2),
            Self::Row(id, _) => (
                "JSON tree node",
                TargetKind::Button,
                100 + id.index() as u64,
            ),
        }
    }
}
fn target_id(generation: u64, role: Role) -> TargetId {
    TargetId::new(
        SurfaceId::dialog("json-utility", generation),
        SemanticTargetId::named(role.name()),
        TargetGeneration(generation),
    )
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Scope {
    pub tree: bool,
    pub checking: bool,
    pub stamp: Option<TreeStamp>,
}
impl Scope {
    pub fn new(view: &JsonWorkspace) -> Self {
        Self {
            tree: view.tree_mode,
            checking: view.checking.is_some(),
            stamp: view.tree_mode.then(|| view.session.tree_stamp()).flatten(),
        }
    }
}
fn scope_allows(captured: Scope, live: Scope) -> bool {
    captured == live
}
type RowMap = Rc<RefCell<HashMap<TargetId, (TreeStamp, JsonTreeNodeId, FocusHandle)>>>;
pub(super) struct JsonJump {
    pub controller: JumpModeController,
    generation: u64,
    frame: Rc<RefCell<Option<JumpGeometryFrame>>>,
    rows: RowMap,
    row_focus: Rc<RefCell<HashMap<(u64, JsonTreeNodeId), FocusHandle>>>,
    scope: Option<Scope>,
    rendered: Rc<RefCell<std::collections::HashSet<(u64, JsonTreeNodeId)>>>,
    pub format: FocusHandle,
}
impl JsonJump {
    pub fn new(controller: JumpModeController, cx: &mut Context<JsonWorkspace>) -> Self {
        Self {
            controller,
            generation: NEXT_MOUNT.fetch_add(1, Ordering::Relaxed),
            frame: Rc::new(RefCell::new(None)),
            rows: Rc::new(RefCell::new(HashMap::new())),
            row_focus: Rc::new(RefCell::new(HashMap::new())),
            scope: None,
            rendered: Rc::new(RefCell::new(std::collections::HashSet::new())),
            format: cx.focus_handle(),
        }
    }
    fn id(&self, role: Role) -> TargetId {
        target_id(self.generation, role)
    }
    pub fn begin(
        &mut self,
        scope: Scope,
        window: &Window,
    ) -> Option<Rc<RefCell<JumpGeometryDraft>>> {
        let rendered = self.rendered.borrow();
        self.row_focus
            .borrow_mut()
            .retain(|key, focus| rendered.contains(key) || focus.is_focused(window));
        drop(rendered);
        self.rendered.borrow_mut().clear();
        if self.scope != Some(scope) {
            self.scope = Some(scope);
            self.controller.retire();
            *self.frame.borrow_mut() = None;
            self.rows.borrow_mut().clear();
            let source_generation = scope.stamp.map(|stamp| stamp.source_generation());
            self.row_focus
                .borrow_mut()
                .retain(|(generation, _), _| Some(*generation) == source_generation);
        }
        self.controller
            .enabled
            .then(|| Rc::new(RefCell::new(JumpGeometryDraft::new(self.generation))))
    }
    pub fn wrap<E: IntoElement>(
        &self,
        element: E,
        role: Role,
        draft: Option<Rc<RefCell<JumpGeometryDraft>>>,
        app: &App,
    ) -> AnyElement {
        let Some(draft) = draft else {
            return element.into_any_element();
        };
        let id = self.id(role);
        let (label, kind, order) = role.metadata();
        let resolved = appearance::current(app);
        let badge = jump_hint_badge(
            self.controller.hint_for(&id),
            &id,
            self.generation,
            self.frame.clone(),
            resolved.palette,
            resolved.metrics,
        );
        let body = div()
            .id(SharedString::from(format!("json-jump-{}", role.name())))
            .relative()
            .child(element)
            .children(badge);
        JumpGeometryElement::new(body, TargetMetadata::new(id, label, kind, order), draft)
            .into_any_element()
    }
    pub fn row_focus(&self, stamp: TreeStamp, id: JsonTreeNodeId, app: &App) -> FocusHandle {
        self.rendered
            .borrow_mut()
            .insert((stamp.source_generation(), id));
        let key = self.id(Role::Row(id, stamp));
        let focus = self
            .row_focus
            .borrow_mut()
            .entry((stamp.source_generation(), id))
            .or_insert_with(|| app.focus_handle())
            .clone();
        self.rows
            .borrow_mut()
            .insert(key, (stamp, id, focus.clone()));
        focus
    }
    pub fn publish_draft(
        &self,
        draft: Rc<RefCell<JumpGeometryDraft>>,
        cx: &mut Context<JsonWorkspace>,
        receipt: utility_mount::MountReceipt,
        host: WindowId,
        scope: Scope,
    ) -> AnyElement {
        let resolved = appearance::current(cx);
        jump_geometry_publisher(
            draft,
            self.frame.clone(),
            cx.entity().downgrade(),
            self.controller.session_generation(),
            Rc::new(self.controller.target_ids().to_vec()),
            self.controller.alphabet.clone(),
            resolved.metrics,
            move |view, frame, window, cx, geometry_changed| {
                if !receipt.is_current()
                    || !receipt.matches(&view.mount_receipt())
                    || host != window.window_handle().window_id()
                    || view.mount.jump.scope != Some(scope)
                    || !scope_allows(scope, Scope::new(view))
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
                    .row_focus
                    .borrow_mut()
                    .retain(|key, focus| rendered.contains(key) || focus.is_focused(window));
                drop(rendered);
                if !view.open
                    || view.rehosting
                    || view.admission.is_fenced()
                    || !view.mount_receipt().is_current()
                    || !view.contains_focus(window, cx)
                {
                    if view.mount.jump.controller.retire() {
                        cx.notify();
                    }
                    return;
                }
                let retired = view.mount.jump.controller.reconcile_frame(Some(frame));
                if retired
                    || (geometry_changed
                        && view.mount.jump.controller.is_active()
                        && frame.matches_active_session)
                {
                    cx.notify();
                }
            },
        )
        .into_any_element()
    }
}
impl JsonWorkspace {
    pub(super) fn retire_jump(&mut self, cx: &mut Context<Self>) {
        if self.mount.jump.controller.retire() {
            cx.notify();
        }
    }
    pub(super) fn handle_json_jump(
        &mut self,
        receipt: PhysicalKeydown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.mount.jump.controller.should_process_key_down(receipt) {
            return false;
        }
        if !self.open
            || self.rehosting
            || self.admission.is_fenced()
            || !self.mount_receipt().is_current()
            || !self.contains_focus(window, cx)
            || window.has_active_dialog(cx)
        {
            self.mount.jump.controller.retire();
            return false;
        }
        if !self
            .mount
            .jump
            .scope
            .is_some_and(|captured| scope_allows(captured, Scope::new(self)))
        {
            if self.mount.jump.controller.retire() {
                cx.notify();
            }
            *self.mount.jump.frame.borrow_mut() = None;
            if self.mount.jump.controller.owns_key(receipt.virtual_key) {
                if receipt.held {
                    return true;
                }
                self.mount.jump.controller.release_fresh_key(receipt);
            }
            return false;
        }
        let frame_slot = self.mount.jump.frame.clone();
        let frame = frame_slot.borrow();
        let retired = self.mount.jump.controller.reconcile_frame(frame.as_ref());
        let outcome = self.mount.jump.controller.process_key_down(
            receipt,
            frame.as_ref(),
            self.mount.jump.generation,
        );
        if let Some(target) = outcome.selected_target {
            if frame
                .as_ref()
                .is_some_and(|frame| frame.has_target(&target))
            {
                match target.semantic() {
                    SemanticTargetId::Named(name) if name == "source" && !self.tree_mode => {
                        self.mount.input.focus_handle(cx).focus(window, cx)
                    }
                    SemanticTargetId::Named(name) if name == "search" && self.tree_mode => {
                        self.mount.tree_search.focus_handle(cx).focus(window, cx)
                    }
                    SemanticTargetId::Named(name)
                        if name == "format" && self.checking.is_none() =>
                    {
                        self.mount.jump.format.focus(window, cx)
                    }
                    _ if self.tree_mode => {
                        let row = self.mount.jump.rows.borrow().get(&target).cloned();
                        if let Some((stamp, id, focus)) = row {
                            if self.session.tree_stamp() == Some(stamp) {
                                super::json_tree::apply_row(self, stamp, id, false, cx);
                                focus.focus(window, cx);
                            }
                        }
                    }
                    _ => {}
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
    use super::{Role, Scope, scope_allows, target_id};
    #[test]
    fn scope_changes_on_mode_and_availability() {
        let text = Scope {
            tree: false,
            checking: false,
            stamp: None,
        };
        let tree = Scope { tree: true, ..text };
        let checking = Scope {
            checking: true,
            ..text
        };
        assert_ne!(text, tree);
        assert_ne!(text, checking);
    }
    #[test]
    fn real_tree_selection_and_source_change_revoke_captured_scope() {
        use multi_launcher::json_utility_session::JsonEditorState;
        use std::sync::{Arc, mpsc};
        use std::time::Duration;
        let mut editor = JsonEditorState::default();
        editor.set_source(r#"{"name":"Café"}"#.into());
        let (sender, receiver) = mpsc::channel();
        let service = |editor: &mut JsonEditorState| {
            editor.service_projection(|| {
                let sender = sender.clone();
                Arc::new(move || {
                    let _ = sender.send(());
                })
            })
        };
        service(&mut editor);
        receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("projection worker notification");
        service(&mut editor);
        let first = editor.tree_stamp().expect("accepted projection");
        let captured = Scope {
            tree: true,
            checking: false,
            stamp: Some(first),
        };
        let id = editor.tree_row(1).expect("visible node").id;
        assert!(editor.tree_select(first, id));
        let next = Scope {
            stamp: editor.tree_stamp(),
            ..captured
        };
        assert!(!scope_allows(captured, next));
        assert!(!editor.tree_select(first, id));
        editor.set_source("[]".into());
        assert!(!scope_allows(
            next,
            Scope {
                stamp: editor.tree_stamp(),
                ..next
            }
        ));
    }
    #[test]
    fn json_control_targets_are_distinct_and_mount_scoped() {
        assert_eq!(target_id(1, Role::Source), target_id(1, Role::Source));
        assert_ne!(target_id(1, Role::Source), target_id(2, Role::Source));
        assert_ne!(target_id(1, Role::Source), target_id(1, Role::Search));
        assert_ne!(target_id(1, Role::Search), target_id(1, Role::Format));
    }
}
