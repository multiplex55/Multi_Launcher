use eframe::egui;

use super::jump_mode::{SurfaceId, TargetGeneration};

use super::jump_mode::ui::{
    JumpModeScope, JumpModeUi, NavigationDialogKind, NavigationOwner, NavigationViewportBinding,
    focused_response_layer, mouse_gesture_binding_layer_id, mouse_gesture_delete_layer_id,
    visible_top_middle_layer,
};
use super::utility_host::{
    AcceptedParentOpen, HostIdentity, UtilityHostState, UtilityPresentation, ViewportPassProvenance,
};
use super::utility_window_geometry::NativeViewportIdentity;

#[derive(Clone, Copy)]
pub(super) struct MouseFormSpec {
    pub kind: NavigationDialogKind,
    pub key: &'static str,
    pub navigation_key: &'static str,
    pub title: &'static str,
    pub size: [f32; 2],
    pub minimum: [f32; 2],
    pub maximum: Option<[f32; 2]>,
}

pub(super) const MOUSE_SETTINGS: MouseFormSpec = MouseFormSpec {
    kind: NavigationDialogKind::MouseSettings,
    key: "mouse-settings",
    navigation_key: "mouse-settings",
    title: "Mouse Settings",
    size: [420.0, 560.0],
    minimum: [240.0, 200.0],
    maximum: None,
};

pub(super) const GESTURE_SETTINGS: MouseFormSpec = MouseFormSpec {
    kind: NavigationDialogKind::MouseGestureSettings,
    key: "gesture-settings",
    navigation_key: "mouse-gesture-settings",
    title: "Mouse Gesture Settings",
    size: [520.0, 480.0],
    minimum: [360.0, 260.0],
    maximum: None,
};

pub(super) const MOUSE_GESTURES: MouseFormSpec = MouseFormSpec {
    kind: NavigationDialogKind::MouseGestures,
    key: "mouse-gestures",
    navigation_key: "mouse-gestures",
    title: "Mouse Gestures",
    size: [720.0, 420.0],
    minimum: [520.0, 320.0],
    maximum: Some([980.0, 760.0]),
};

pub(super) trait MouseFormDraft: Clone {
    type Checkpoint;
    fn checkpoint(&self) -> Self::Checkpoint;
    fn restore_checkpoint(&mut self, checkpoint: Self::Checkpoint);
    fn child_owner(&self, _instance: u64) -> Option<NavigationOwner> {
        None
    }

    fn target_generation(&self, instance: u64) -> TargetGeneration {
        TargetGeneration(instance)
    }

    fn owned_popup(&self, _ctx: &egui::Context, _owner: NavigationOwner) -> bool {
        true
    }

    fn canvas_active(&self) -> bool {
        false
    }

    fn reconcile_canvas(&mut self, _ctx: &egui::Context, _owner_has_input: bool) {}
}

impl MouseFormDraft for super::MouseSettingsDialog {
    type Checkpoint = Self;
    fn checkpoint(&self) -> Self {
        self.clone()
    }
    fn restore_checkpoint(&mut self, checkpoint: Self) {
        *self = checkpoint;
    }
}
impl MouseFormDraft for super::MouseGestureSettingsDialog {
    type Checkpoint = super::mouse_gesture_settings_dialog::GestureSettingsMainCheckpoint;
    fn checkpoint(&self) -> Self::Checkpoint {
        Self::Checkpoint::capture(self)
    }
    fn restore_checkpoint(&mut self, checkpoint: Self::Checkpoint) {
        checkpoint.restore(self);
    }
}

impl MouseFormDraft for super::MgGesturesDialog {
    type Checkpoint = super::mouse_gestures_dialog::MouseGesturesMainCheckpoint;
    fn checkpoint(&self) -> Self::Checkpoint {
        Self::Checkpoint::capture(self)
    }
    fn restore_checkpoint(&mut self, checkpoint: Self::Checkpoint) {
        checkpoint.restore(self);
    }
    fn child_owner(&self, instance: u64) -> Option<NavigationOwner> {
        self.jump_mode_child_owner(instance)
            .filter(|owner| !matches!(owner, NavigationOwner::MouseGestureBinding { .. }))
    }

    fn target_generation(&self, _instance: u64) -> TargetGeneration {
        self.jump_mode_target_generation()
    }

    fn owned_popup(&self, ctx: &egui::Context, owner: NavigationOwner) -> bool {
        match owner {
            NavigationOwner::MouseGestureBinding { .. } => {
                self.jump_mode_binding_popup_is_open(ctx)
            }
            NavigationOwner::MouseGestureDelete { .. } => false,
            _ => self.jump_mode_main_popup_is_open(ctx),
        }
    }

    fn canvas_active(&self) -> bool {
        self.recorder_canvas_active()
    }

    fn reconcile_canvas(&mut self, ctx: &egui::Context, owner_has_input: bool) {
        self.reconcile_canvas_drag_with_owner(ctx, owner_has_input);
    }
}

pub(super) fn mouse_gestures_navigation_surface(
    instance: u64,
    owner: NavigationOwner,
) -> (SurfaceId, &'static str) {
    let parent = SurfaceId::dialog(MOUSE_GESTURES.navigation_key, instance);
    match owner {
        NavigationOwner::MouseGestureBinding {
            binding_instance, ..
        } => (
            SurfaceId::popup("mouse-gesture-binding", binding_instance, parent),
            "mouse-gestures/binding",
        ),
        NavigationOwner::MouseGestureDelete {
            confirmation_instance,
            ..
        } => (
            SurfaceId::popup("mouse-gesture-delete", confirmation_instance, parent),
            "mouse-gestures/delete",
        ),
        _ => (parent, "mouse-gestures/form"),
    }
}

fn binding_for<D: MouseFormDraft>(
    draft: &D,
    spec: MouseFormSpec,
    viewport: egui::ViewportId,
    instance: u64,
) -> NavigationViewportBinding {
    let owner = draft
        .child_owner(instance)
        .unwrap_or(NavigationOwner::Dialog {
            kind: spec.kind,
            instance,
        });
    let base_layer = match owner {
        NavigationOwner::MouseGestureBinding {
            parent_instance,
            binding_instance,
        } => mouse_gesture_binding_layer_id(parent_instance, binding_instance),
        NavigationOwner::MouseGestureDelete {
            parent_instance,
            confirmation_instance,
        } => mouse_gesture_delete_layer_id(parent_instance, confirmation_instance),
        _ => egui::LayerId::background(),
    };
    NavigationViewportBinding {
        viewport,
        owner,
        base_layer,
    }
}

use crate::settings_editor::SettingsEditor;
use crate::settings_editor::render::{SettingsNavigationFrameObserver, SettingsNavigationSnapshot};

#[derive(Clone)]
pub(super) struct NavigationObservation {
    before: SettingsNavigationSnapshot,
    after: SettingsNavigationSnapshot,
    pub observer: Option<SettingsNavigationFrameObserver>,
    pub current: bool,
    time: f64,
}

impl NavigationObservation {
    pub fn new(
        before: SettingsNavigationSnapshot,
        after: SettingsNavigationSnapshot,
        observer: Option<SettingsNavigationFrameObserver>,
        time: f64,
    ) -> Option<Self> {
        (before != after || observer.is_some()).then_some(Self {
            before,
            after,
            observer,
            current: false,
            time,
        })
    }

    pub fn finish(self, editor: &mut SettingsEditor) {
        if SettingsNavigationSnapshot::capture(editor) != self.before {
            return;
        }
        self.after.restore(editor);
        if let Some(observer) = self.observer {
            editor.finish_dedicated_navigation_frame(&observer, self.current, true, self.time);
        }
    }
}

pub(super) type MouseSettingsHost = MouseFormHost<
    super::MouseSettingsDialog,
    (
        super::mouse_settings_dialog::MouseSettingsUiActions,
        Option<NavigationObservation>,
    ),
>;

pub(super) type GestureSettingsHost = MouseFormHost<
    super::MouseGestureSettingsDialog,
    (
        super::mouse_gesture_settings_dialog::GestureSettingsUiActions,
        Option<NavigationObservation>,
    ),
>;

pub(super) type MouseGesturesHost =
    MouseFormHost<super::MgGesturesDialog, super::mouse_gestures_dialog::MouseGesturesUiActions>;

impl MouseFormSpec {
    fn viewport(self, instance: u64) -> egui::ViewportId {
        egui::ViewportId::from_hash_of((self.key, instance))
    }

    fn title(self, instance: u64) -> String {
        format!("{} â€” {instance}", self.title)
    }

    fn identity(self, host: HostIdentity, instance: u64) -> Option<NativeViewportIdentity> {
        (host.host == UtilityPresentation::Popout).then_some(NativeViewportIdentity {
            key: crate::settings::UtilityToolKey::parse(self.key).ok()?,
            viewport_id: self.viewport(instance),
            owner_generation: instance,
            host_generation: host.generation,
        })
    }
}

pub(super) struct MouseFormHost<D: MouseFormDraft, A> {
    state: UtilityHostState,
    presentation_prepared: bool,
    instance: u64,
    pending: Option<FormFrame<D, A>>,
    accepted_origin: Option<AcceptedParentOpen<u64>>,
    docked_focus_pending: bool,
    close_pending: Option<ViewportPassProvenance>,
    native_jump: JumpModeUi,
    native_jump_frame: Option<u64>,
    #[cfg(test)]
    test_button: Option<(egui::Rect, egui::Rect, bool)>,
}

impl<D: MouseFormDraft, A> Default for MouseFormHost<D, A> {
    fn default() -> Self {
        Self {
            state: Default::default(),
            presentation_prepared: false,
            instance: 0,
            pending: None,
            accepted_origin: None,
            docked_focus_pending: false,
            close_pending: None,
            native_jump: Default::default(),
            native_jump_frame: None,
            #[cfg(test)]
            test_button: None,
        }
    }
}

struct FormFrame<D: MouseFormDraft, A> {
    root: ViewportPassProvenance,
    owner: Option<ViewportPassProvenance>,
    host: HostIdentity,
    instance: u64,
    authorized: bool,
    actions: Option<A>,
    checkpoint: Option<D::Checkpoint>,
    os_close: bool,
    close: bool,
    focus: bool,
    close_allowed: bool,
    viewport_info: Option<egui::ViewportInfo>,
    embedded_layer: Option<egui::LayerId>,
    embedded_focus: Option<egui::Id>,
}

pub(super) struct FormInput {
    pub first: bool,
    pub root_input: bool,
    pub root_owner: bool,
    pub root_jump_active: bool,
    pub application_input: bool,
    pub close_shortcut: bool,
}

pub(super) struct FormUi<'a> {
    pub ctx: &'a egui::Context,
    pub child: Option<&'a mut egui::Ui>,
    pub scope: Option<&'a JumpModeScope>,
    pub first: bool,
    pub input: bool,
    pub owner: bool,
    pub native: bool,
    pub native_owner: Option<NavigationOwner>,
    pub popout: bool,
}

impl<D: MouseFormDraft, A> MouseFormHost<D, A> {
    pub fn take_accepted_origin(&mut self) -> Option<AcceptedParentOpen<u64>> {
        self.accepted_origin.take()
    }

    pub fn accepts_origin(&self, open: bool, origin: &AcceptedParentOpen<u64>) -> bool {
        open && origin.is_current(self.state.identity(), &self.instance)
    }

    pub fn session(&self) -> u64 {
        self.instance
    }

    #[cfg(test)]
    pub fn record_test_button(
        &mut self,
        button: impl FnOnce(&A) -> Option<(egui::Rect, egui::Rect, bool)>,
    ) {
        self.test_button = self
            .pending
            .as_ref()
            .and_then(|frame| frame.actions.as_ref())
            .and_then(button);
    }

    #[cfg(test)]
    pub fn test_button(&self) -> (egui::Rect, egui::Rect, bool) {
        self.test_button
            .expect("the Mouse Settings host button was rendered")
    }

    #[cfg(test)]
    pub fn test_target_rect(&self, label: &str) -> egui::Rect {
        self.native_jump
            .retained_registry()
            .targets()
            .iter()
            .find(|target| target.eligible() && target.metadata().label() == label)
            .unwrap_or_else(|| {
                let owner = self.native_jump.frame_navigation_owner();
                let targets: Vec<_> = self
                    .native_jump
                    .retained_registry()
                    .targets()
                    .iter()
                    .map(|target| target.metadata().label())
                    .collect();
                panic!(
                    "missing native target {label:?} for {owner:?}; retained targets: {targets:?}"
                )
            })
            .clipped_rect()
    }

    #[cfg(test)]
    pub fn test_viewport_id(&self, spec: MouseFormSpec) -> egui::ViewportId {
        spec.viewport(self.instance)
    }

    pub fn is_popped_out(&self) -> bool {
        self.state.presentation() == UtilityPresentation::Popout
    }

    pub fn root_layer(&self, spec: MouseFormSpec) -> Option<egui::LayerId> {
        if self.is_popped_out() {
            self.state.embedded_root_layer()
        } else {
            spec.kind.layer_id()
        }
    }

    pub fn needs_root_ui_when_hidden(&self, open: bool) -> bool {
        (open && self.is_popped_out()) || self.state.bootstrap_pending() || self.pending.is_some()
    }

    pub fn prepare_open(&mut self, ctx: &egui::Context, spec: MouseFormSpec, open: bool) {
        if open && !self.presentation_prepared {
            let key = crate::settings::UtilityToolKey::parse(spec.key)
                .expect("registered mouse form key");
            super::utility_host::restore_remembered_presentation(ctx, key, &mut self.state);
            self.presentation_prepared = true;
        }
    }

    pub fn open_from_origin<S>(
        &mut self,
        ctx: &egui::Context,
        spec: MouseFormSpec,
        was_open: bool,
        origin: &super::utility_host::ChildOpenOrigin<S>,
    ) -> Option<UtilityPresentation> {
        self.prepare_open(ctx, spec, true);
        let key =
            crate::settings::UtilityToolKey::parse(spec.key).expect("registered mouse form key");
        let remembered = super::utility_presentation::UtilityPresentationManager::for_context(ctx)
            .and_then(|manager| manager.preference(key))
            .map(Into::into);
        let desired = super::utility_host::resolve_child_presentation(
            was_open.then(|| self.state.presentation()),
            remembered,
            self.state.presentation(),
            origin,
        );
        if self.pending.is_some() {
            return None;
        }
        if self.state.transition(desired) {
            self.clear_jump();
        }
        if matches!(origin, super::utility_host::ChildOpenOrigin::Parent(_))
            && desired == UtilityPresentation::Popout
        {
            super::utility_host::remember_presentation(ctx, key, desired);
        }
        self.request_focus();
        Some(desired)
    }

    pub fn remember_current(&self, ctx: &egui::Context, spec: MouseFormSpec) {
        let key =
            crate::settings::UtilityToolKey::parse(spec.key).expect("registered mouse form key");
        super::utility_host::remember_presentation(ctx, key, self.state.presentation());
    }

    pub fn request_focus(&mut self) {
        if self.is_popped_out() {
            self.state.request_focus();
        } else {
            self.docked_focus_pending = true;
        }
    }

    pub fn request_close(&mut self, ctx: &egui::Context) {
        self.close_pending = Some(ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT));
    }

    fn close_viewport(&self, ctx: &egui::Context, spec: MouseFormSpec) {
        if let Some(identity) = spec.identity(self.state.identity(), self.instance) {
            super::utility_dialog_size::close_native_utility_geometry_viewport(ctx, identity);
            ctx.send_viewport_cmd_to(identity.viewport_id, egui::ViewportCommand::Close);
        }
    }

    pub fn retire(&mut self, ctx: &egui::Context, spec: MouseFormSpec) {
        self.close_viewport(ctx, spec);
        self.state.retire_to_docked();
        self.presentation_prepared = false;
        self.pending = None;
        self.docked_focus_pending = false;
        self.close_pending = None;
        self.clear_jump();
    }

    fn clear_jump(&mut self) {
        self.native_jump.cancel_all();
        self.native_jump_frame = None;
    }

    pub fn configure_jump(&mut self, launcher_jump: &JumpModeUi, enabled: bool) {
        let (chord, alphabet) = launcher_jump.configuration();
        self.native_jump.set_configuration(chord, alphabet);
        self.native_jump.set_enabled(enabled);
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        spec: MouseFormSpec,
        instance: u64,
        draft: &mut D,
        input: FormInput,
        root_scope: Option<&JumpModeScope>,
        mut show: impl FnMut(&mut D, FormUi<'_>) -> A,
    ) {
        let host = self.state.identity();
        let root = ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT);
        let mut os_close = false;
        if let Some(previous) = self.pending.take()
            && previous.root.frame_nr == root.frame_nr
            && previous.instance == instance
            && previous.host == host
        {
            if let Some(checkpoint) = previous.checkpoint {
                draft.restore_checkpoint(checkpoint);
            }
            os_close = previous.os_close;
        }
        self.instance = instance;
        let root_first = input.first && !ctx.will_discard();
        let mut frame = FormFrame {
            root,
            owner: Some(root),
            host,
            instance,
            authorized: root_first,
            actions: None,
            checkpoint: None,
            os_close,
            close: self.close_pending.take() == Some(root),
            focus: false,
            close_allowed: spec.kind != NavigationDialogKind::MouseGestures || input.close_shortcut,
            viewport_info: None,
            embedded_layer: None,
            embedded_focus: None,
        };
        if host.host == UtilityPresentation::Docked {
            frame.focus = self.docked_focus_pending && input.root_input;
            if root_first && input.root_owner && !input.root_jump_active && !frame.close {
                frame.close = close_shortcut(ctx, input.close_shortcut);
            }
            let body_input = root_first && input.root_input && !frame.close;
            frame.checkpoint = snapshot(ctx, draft, body_input, root_scope);
            frame.actions = Some(show(
                draft,
                FormUi {
                    ctx,
                    child: None,
                    scope: root_scope,
                    first: root_first,
                    input: body_input,
                    owner: body_input && input.root_owner,
                    native: false,
                    native_owner: None,
                    popout: false,
                },
            ));
        } else {
            let viewport = spec.viewport(instance);
            let mut builder = egui::ViewportBuilder::default()
                .with_title(spec.title(instance))
                .with_inner_size(spec.size)
                .with_min_inner_size(spec.minimum)
                .with_resizable(true)
                .with_visible(true);
            if let Some(maximum) = spec.maximum {
                builder = builder.with_max_inner_size(maximum);
            }
            let launcher_close = frame.close;
            ctx.show_viewport_immediate(viewport, builder, |child, class| {
                let child_ctx = child.ctx().clone();
                let native =
                    class == egui::ViewportClass::Immediate && child_ctx.viewport_id() == viewport;
                let child_first = child.current_pass_index() == 0;
                if native && !child_first {
                    if let Some(checkpoint) = frame.checkpoint.take() {
                        draft.restore_checkpoint(checkpoint);
                    }
                    frame.close = launcher_close;
                }
                frame.owner = Some(ViewportPassProvenance::current(
                    &child_ctx,
                    child_ctx.viewport_id(),
                ));
                let first = root_first && child_first && !child_ctx.will_discard();
                frame.authorized = first;
                let modal = child_ctx.memory(|memory| memory.top_modal_layer().is_some());
                let popup = egui::Popup::is_any_open(&child_ctx);
                let visible = native || (child.is_visible() && !child.is_sizing_pass());
                let focused = child_ctx.input(|state| state.focused);
                let body_input = first
                    && visible
                    && focused
                    && !modal
                    && if native {
                        input.application_input
                    } else {
                        input.root_input
                    };
                frame.checkpoint = snapshot(
                    &child_ctx,
                    draft,
                    body_input,
                    (!native).then_some(root_scope).flatten(),
                );
                let child_owner = draft.child_owner(instance);
                if first {
                    let canvas_owner = body_input
                        && child_owner.is_none()
                        && !popup
                        && if native {
                            visible_top_middle_layer(&child_ctx).is_none()
                        } else {
                            input.root_owner
                        };
                    draft.reconcile_canvas(&child_ctx, canvas_owner);
                }
                if native {
                    let info = child.input(|state| state.viewport().clone());
                    frame.os_close |= info.close_requested();
                    frame.viewport_info = Some(info);
                    frame.focus = self.state.focus_pending() && input.application_input && !modal;
                    let binding = binding_for(draft, spec, viewport, instance);
                    let frame_nr = child_ctx.cumulative_frame_nr();
                    if self.native_jump_frame != Some(frame_nr) {
                        self.native_jump.begin_frame();
                        self.native_jump_frame = Some(frame_nr);
                    }
                    let owner = body_input
                        && binding.base_is_current(&child_ctx)
                        && (!popup || draft.owned_popup(&child_ctx, binding.owner))
                        && focused_response_layer(&child_ctx).is_none_or(|layer| {
                            layer == binding.base_layer || layer.order != egui::Order::Middle
                        });
                    let jump_input = input.application_input && !draft.canvas_active();
                    let navigation_owner = self
                        .native_jump
                        .update_navigation_owner_for_viewport(&child_ctx, binding, jump_input);
                    self.native_jump.handle_viewport_input(
                        &child_ctx,
                        self.native_jump.is_enabled() && navigation_owner == binding.owner,
                        first,
                        viewport,
                    );
                    let jump_active = self.native_jump.active_session().is_some();
                    if owner
                        && child_owner.is_none()
                        && !draft.canvas_active()
                        && !popup
                        && !jump_active
                        && !frame.close
                        && !frame.os_close
                    {
                        frame.close = close_shortcut(&child_ctx, input.close_shortcut);
                    }
                    self.native_jump.begin_ui_pass();
                    let enabled = !frame.close && !frame.os_close;
                    let scope = if owner
                        && enabled
                        && !draft.canvas_active()
                        && navigation_owner == binding.owner
                    {
                        let (surface, namespace) =
                            if spec.kind == NavigationDialogKind::MouseGestures {
                                mouse_gestures_navigation_surface(instance, binding.owner)
                            } else {
                                (SurfaceId::dialog(spec.navigation_key, instance), spec.key)
                            };
                        self.native_jump.scope_for_viewport_binding(
                            surface,
                            draft.target_generation(instance),
                            namespace,
                            binding,
                        )
                    } else {
                        None
                    };
                    if frame.checkpoint.is_none()
                        && scope
                            .as_ref()
                            .is_some_and(JumpModeScope::has_pending_focus_intent)
                    {
                        frame.checkpoint = Some(draft.checkpoint());
                    }
                    egui::CentralPanel::default().show(child, |ui| {
                        frame.actions = Some(show(
                            draft,
                            FormUi {
                                ctx: &child_ctx,
                                child: Some(ui),
                                scope: scope.as_ref(),
                                first,
                                input: body_input && enabled,
                                owner: owner && enabled,
                                native: true,
                                native_owner: (owner && enabled).then_some(binding.owner),
                                popout: true,
                            },
                        ));
                    });
                    self.native_jump.finish_ui_pass();
                    let current_binding = binding_for(draft, spec, viewport, instance);
                    if current_binding != binding {
                        // The old layer can still be in the hit-test history.
                        // Repaint until the newly selected owner is drawn live.
                        child_ctx.request_repaint();
                    }
                    let binding = current_binding;
                    let jump_input = input.application_input && !draft.canvas_active();
                    let owner = self
                        .native_jump
                        .update_navigation_owner_for_viewport(&child_ctx, binding, jump_input);
                    if self
                        .native_jump
                        .prepare_pass_for_viewport(&child_ctx, binding, owner, jump_input)
                    {
                        super::jump_mode::paint::paint(&child_ctx, &self.native_jump);
                    }
                    if !child_ctx.will_discard() {
                        self.native_jump
                            .complete_frame_for_viewport(&child_ctx, binding, owner, jump_input);
                    }
                } else {
                    frame.embedded_layer = Some(child.layer_id());
                    frame.embedded_focus = child_ctx.memory(|memory| memory.focused());
                    frame.focus = self.state.focus_pending() && input.root_input && visible;
                    let close_owner = input.root_owner
                        && child_owner.is_none()
                        && !draft.canvas_active()
                        && !popup
                        && focused_response_layer(&child_ctx).is_none_or(|layer| {
                            layer == child.layer_id() || layer.order != egui::Order::Middle
                        });
                    if body_input && close_owner && !input.root_jump_active && !frame.close {
                        frame.close = close_shortcut(&child_ctx, input.close_shortcut);
                    }
                    frame.actions = Some(show(
                        draft,
                        FormUi {
                            ctx: &child_ctx,
                            child: Some(child),
                            scope: root_scope,
                            first,
                            input: body_input && !frame.close,
                            owner: body_input && input.root_owner && !frame.close,
                            native: false,
                            native_owner: None,
                            popout: true,
                        },
                    ));
                }
            });
        }
        self.pending = Some(frame);
    }

    pub fn complete(
        &mut self,
        ctx: &egui::Context,
        spec: MouseFormSpec,
        draft: &mut D,
        open: bool,
    ) -> Option<(A, bool)> {
        self.accepted_origin = None;
        let mut frame = self.pending.take()?;
        let current =
            open && frame.instance == self.instance && frame.host == self.state.identity();
        let retained = frame.authorized
            && frame.root.was_retained(ctx)
            && frame.owner.is_some_and(|pass| pass.was_retained(ctx));
        if frame.os_close {
            ctx.send_viewport_cmd_to(
                spec.viewport(frame.instance),
                egui::ViewportCommand::CancelClose,
            );
        }
        if !current || !retained {
            if current && let Some(checkpoint) = frame.checkpoint.take() {
                draft.restore_checkpoint(checkpoint);
            }
            self.clear_jump();
            if current && let Some(owner) = frame.owner {
                // A replay may have painted the controls disabled. Refresh
                // hit testing before the next interaction in the same owner.
                ctx.request_repaint_of(owner.viewport_id);
            }
            return None;
        }
        self.accepted_origin = frame.owner.and_then(|owner| {
            AcceptedParentOpen::from_retained(ctx, frame.host, frame.instance, frame.root, owner)
        });
        self.state
            .complete_bootstrap(frame.host, frame.embedded_layer, frame.embedded_focus);
        let close = frame.close || (frame.os_close && frame.close_allowed);
        if frame.focus && !close {
            if frame.viewport_info.is_some() {
                ctx.send_viewport_cmd_to(
                    spec.viewport(frame.instance),
                    egui::ViewportCommand::Focus,
                );
            } else if let Some(layer) = frame.embedded_layer {
                ctx.move_to_top(layer);
            } else if frame.host.host == UtilityPresentation::Docked
                && let Some(layer) = spec.kind.layer_id()
            {
                ctx.move_to_top(layer);
            }
            self.state.complete_focus(frame.host);
            self.docked_focus_pending = false;
        }
        if !close
            && let (Some(identity), Some(info)) = (
                spec.identity(frame.host, frame.instance),
                frame.viewport_info.as_ref(),
            )
        {
            super::utility_dialog_size::apply_native_utility_geometry_restore(
                ctx,
                egui::ViewportClass::Immediate,
                true,
                &spec.title(frame.instance),
                identity,
                info,
                spec.size.into(),
                Some(spec.minimum.into()),
                spec.maximum.map(Into::into),
            );
        }
        Some((frame.actions?, close))
    }

    pub fn transition(&mut self, ctx: &egui::Context, spec: MouseFormSpec, popout: bool) -> bool {
        if self.is_popped_out() == popout {
            return false;
        }
        if !popout {
            self.close_viewport(ctx, spec);
        }
        self.state.transition(if popout {
            UtilityPresentation::Popout
        } else {
            UtilityPresentation::Docked
        });
        self.remember_current(ctx, spec);
        self.clear_jump();
        if !popout && let Some(layer) = spec.kind.layer_id() {
            ctx.move_to_top(layer);
        }
        ctx.request_repaint_of(egui::ViewportId::ROOT);
        !popout
    }
}

fn snapshot<D: MouseFormDraft>(
    ctx: &egui::Context,
    draft: &D,
    allowed: bool,
    scope: Option<&JumpModeScope>,
) -> Option<D::Checkpoint> {
    (allowed
        && (draft.canvas_active()
            || scope.is_some_and(JumpModeScope::has_pending_focus_intent)
            || ctx.input(|input| {
                input.pointer.any_down()
                    || input.events.iter().any(|event| {
                        !matches!(
                            event,
                            egui::Event::PointerMoved(_)
                                | egui::Event::MouseMoved(_)
                                | egui::Event::PointerGone
                        )
                    })
            })))
    .then(|| draft.checkpoint())
}

fn close_shortcut(ctx: &egui::Context, allowed: bool) -> bool {
    allowed
        && ctx.input_mut(|input| {
            input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                || input.consume_key(egui::Modifiers::COMMAND, egui::Key::W)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn child_open_receipt_rejects_parent_close_host_move_and_reopen() {
        let ctx = egui::Context::default();
        let mut host = MouseGesturesHost::default();
        host.instance = 7;
        host.state.transition(UtilityPresentation::Popout);
        let root = ViewportPassProvenance::current(&ctx, egui::ViewportId::ROOT);
        assert!(
            AcceptedParentOpen::from_retained(&ctx, host.state.identity(), 7, root, root).is_none()
        );
        ctx.run_ui(egui::RawInput::default(), |_| {})
            .drop_without_applying_deltas();
        let receipt =
            AcceptedParentOpen::from_retained(&ctx, host.state.identity(), 7, root, root).unwrap();
        assert!(host.accepts_origin(true, &receipt));
        assert!(!host.accepts_origin(false, &receipt));
        host.state.transition(UtilityPresentation::Docked);
        assert!(!host.accepts_origin(true, &receipt));
        host.state.transition(UtilityPresentation::Popout);
        host.instance = 8;
        assert!(!host.accepts_origin(true, &receipt));
    }
}
