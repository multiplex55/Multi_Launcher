//! Mount-owned geometry collection for the stock Hash Select popup rows.
use super::jump_mode::{JumpGeometryDraft, JumpGeometryFrame, clipped_jump_geometry};
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HashPopupKind {
    Source,
    Algorithm,
}

pub(super) struct PopupCollector {
    enabled: bool,
    kind: HashPopupKind,
    mount: u64,
    epoch: u64,
    owner: Option<WeakEntity<HashGeneratorWindow>>,
    trigger: Option<FocusHandle>,
    pending: Option<JumpGeometryDraft>,
    pub frame: Option<JumpGeometryFrame>,
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
            frame: None,
        }))
    }
    pub fn bind(&mut self, owner: WeakEntity<HashGeneratorWindow>, trigger: FocusHandle) {
        self.owner = Some(owner);
        self.trigger = Some(trigger);
    }
    pub fn dismiss(&mut self) {
        self.epoch = self.epoch.wrapping_add(1).max(1);
        self.pending = None;
        self.frame = None;
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
        PopupRow {
            inner,
            label: self.title(),
            index: self.index,
            collector: Rc::clone(&self.collector),
        }
        .into_any_element()
    }
}

struct PopupRow<E> {
    inner: E,
    label: SharedString,
    index: usize,
    collector: Rc<RefCell<PopupCollector>>,
}
impl<E: Element> IntoElement for PopupRow<E> {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl<E: Element> Element for PopupRow<E> {
    type RequestLayoutState = E::RequestLayoutState;
    type PrepaintState = E::PrepaintState;
    fn id(&self) -> Option<ElementId> {
        self.inner.id()
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        self.inner.source_location()
    }
    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.inner.request_layout(id, inspector, window, cx)
    }
    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let prepaint = self
            .inner
            .prepaint(id, inspector, bounds.clone(), layout, window, cx);
        let (target_id, epoch) = {
            let collector = self.collector.borrow();
            (collector.target(self.index), collector.epoch)
        };
        let metadata = TargetMetadata::new(
            target_id,
            self.label.to_string(),
            TargetKind::Selectable,
            self.index as u64,
        );
        if let Some(target) = clipped_jump_geometry(metadata, bounds, window.content_mask().bounds)
        {
            let schedule = {
                let mut collector = self.collector.borrow_mut();
                let schedule = collector.pending.is_none();
                let draft = collector
                    .pending
                    .get_or_insert_with(|| JumpGeometryDraft::new(epoch));
                if !draft
                    .targets
                    .iter()
                    .any(|prior| prior.metadata.id() == target.metadata.id())
                {
                    draft.targets.push(target);
                }
                schedule
            };
            if schedule {
                let collector = Rc::clone(&self.collector);
                let window_id = window.window_handle().window_id();
                window.defer(cx, move |window, app| {
                    let (owner, trigger, kind, mount, draft) = {
                        let mut state = collector.borrow_mut();
                        if state.epoch != epoch {
                            return;
                        }
                        (
                            state.owner.clone(),
                            state.trigger.clone(),
                            state.kind,
                            state.mount,
                            state.pending.take(),
                        )
                    };
                    let (Some(owner), Some(trigger), Some(draft)) = (owner, trigger, draft) else {
                        return;
                    };
                    let _ = owner.update(app, |owner, cx| {
                        if !owner.is_current_mount()
                            || owner.mount_generation != mount
                            || !window.is_window_active()
                            || window.window_handle().window_id() != window_id
                        {
                            return;
                        }
                        let focus = match kind {
                            HashPopupKind::Source => owner.source.read(cx).focus_handle(cx),
                            HashPopupKind::Algorithm => owner.algorithm.read(cx).focus_handle(cx),
                        };
                        if focus == trigger
                            || !focus.is_focused(window)
                            || !context_stack_has_primary(&window.context_stack(), "List")
                        {
                            return;
                        }
                        let metrics = appearance::current(cx).metrics;
                        let frame =
                            draft.finish(None, &[], &owner.jump.controller.alphabet, metrics);
                        let mut state = collector.borrow_mut();
                        if state.epoch == epoch {
                            state.frame = Some(frame);
                        }
                    });
                });
            }
        }
        prepaint
    }
    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.inner
            .paint(id, inspector, bounds, layout, prepaint, window, cx);
    }
    fn a11y_role(&self) -> Option<Role> {
        self.inner.a11y_role()
    }
    fn write_a11y_info(&self, node: &mut gpui_kit::accesskit::Node) {
        self.inner.write_a11y_info(node);
    }
    fn a11y_synthetic_children(
        &mut self,
        prepaint: &mut Self::PrepaintState,
        builder: &mut A11ySubtreeBuilder,
    ) {
        self.inner.a11y_synthetic_children(prepaint, builder);
    }
}
