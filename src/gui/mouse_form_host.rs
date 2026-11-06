use eframe::egui;

use super::jump_mode::ui::{
    JumpModeScope, JumpModeUi, NavigationDialogKind, NavigationOwner, NavigationViewportBinding,
    focused_response_layer, visible_top_middle_layer,
};
use super::utility_host::{
    HostIdentity, UtilityHostState, UtilityPresentation, ViewportPassProvenance,
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
}

pub(super) const MOUSE_SETTINGS: MouseFormSpec = MouseFormSpec {
    kind: NavigationDialogKind::MouseSettings,
    key: "mouse-settings",
    navigation_key: "mouse-settings",
    title: "Mouse Settings",
    size: [420.0, 560.0],
    minimum: [240.0, 200.0],
};

pub(super) const GESTURE_SETTINGS: MouseFormSpec = MouseFormSpec {
    kind: NavigationDialogKind::MouseGestureSettings,
    key: "gesture-settings",
    navigation_key: "mouse-gesture-settings",
    title: "Mouse Gesture Settings",
    size: [520.0, 480.0],
    minimum: [360.0, 260.0],
};

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

pub(super) struct MouseFormHost<D, A> {
    state: UtilityHostState,
    instance: u64,
    pending: Option<FormFrame<D, A>>,
    close_pending: Option<ViewportPassProvenance>,
    native_jump: JumpModeUi,
    native_jump_frame: Option<u64>,
    #[cfg(test)]
    test_button: Option<(egui::Rect, egui::Rect, bool)>,
}

impl<D, A> Default for MouseFormHost<D, A> {
    fn default() -> Self {
        Self {
            state: Default::default(),
            instance: 0,
            pending: None,
            close_pending: None,
            native_jump: Default::default(),
            native_jump_frame: None,
            #[cfg(test)]
            test_button: None,
        }
    }
}

struct FormFrame<D, A> {
    root: ViewportPassProvenance,
    owner: Option<ViewportPassProvenance>,
    host: HostIdentity,
    instance: u64,
    authorized: bool,
    actions: Option<A>,
    checkpoint: Option<D>,
    os_close: bool,
    close: bool,
    focus: bool,
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
    pub popout: bool,
}

impl<D: Clone, A> MouseFormHost<D, A> {
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
            .unwrap_or_else(|| panic!("missing native Mouse Settings target {label}"))
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

    pub fn request_focus(&mut self) {
        self.state.request_focus();
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
        self.pending = None;
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
                *draft = checkpoint;
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
            viewport_info: None,
            embedded_layer: None,
            embedded_focus: None,
        };
        if host.host == UtilityPresentation::Docked {
            if root_first && input.root_owner && !input.root_jump_active && !frame.close {
                frame.close = close_shortcut(ctx, input.close_shortcut);
            }
            let body_input = root_first && input.root_input && !frame.close;
            frame.checkpoint = snapshot(ctx, draft, body_input);
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
                    popout: false,
                },
            ));
        } else {
            let viewport = spec.viewport(instance);
            let builder = egui::ViewportBuilder::default()
                .with_title(spec.title(instance))
                .with_inner_size(spec.size)
                .with_min_inner_size(spec.minimum)
                .with_resizable(true)
                .with_visible(true);
            let launcher_close = frame.close;
            ctx.show_viewport_immediate(viewport, builder, |child, class| {
                let child_ctx = child.ctx().clone();
                let native =
                    class == egui::ViewportClass::Immediate && child_ctx.viewport_id() == viewport;
                let child_first = child.current_pass_index() == 0;
                if native && !child_first {
                    if let Some(checkpoint) = frame.checkpoint.take() {
                        *draft = checkpoint;
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
                frame.checkpoint = snapshot(&child_ctx, draft, body_input);
                if native {
                    let info = child.input(|state| state.viewport().clone());
                    frame.os_close |= info.close_requested();
                    frame.viewport_info = Some(info);
                    frame.focus = self.state.focus_pending() && input.application_input && !modal;
                    let binding = NavigationViewportBinding {
                        viewport,
                        owner: NavigationOwner::Dialog {
                            kind: spec.kind,
                            instance,
                        },
                        base_layer: egui::LayerId::background(),
                    };
                    let frame_nr = child_ctx.cumulative_frame_nr();
                    if self.native_jump_frame != Some(frame_nr) {
                        self.native_jump.begin_frame();
                        self.native_jump_frame = Some(frame_nr);
                    }
                    let owner = body_input
                        && visible_top_middle_layer(&child_ctx).is_none()
                        && focused_response_layer(&child_ctx)
                            .is_none_or(|layer| layer.order != egui::Order::Middle);
                    let navigation_owner = self.native_jump.update_navigation_owner_for_viewport(
                        &child_ctx,
                        binding,
                        input.application_input,
                    );
                    self.native_jump.handle_viewport_input(
                        &child_ctx,
                        self.native_jump.is_enabled() && navigation_owner == binding.owner,
                        first,
                        viewport,
                    );
                    let jump_active = self.native_jump.active_session().is_some();
                    if owner && !popup && !jump_active && !frame.close && !frame.os_close {
                        frame.close = close_shortcut(&child_ctx, input.close_shortcut);
                    }
                    self.native_jump.begin_ui_pass();
                    let enabled = !frame.close && !frame.os_close;
                    let scope = (owner && enabled && navigation_owner == binding.owner)
                        .then(|| {
                            self.native_jump.scope_for_viewport_binding(
                                super::jump_mode::SurfaceId::dialog(spec.navigation_key, instance),
                                super::jump_mode::TargetGeneration(instance),
                                spec.key,
                                binding,
                            )
                        })
                        .flatten();
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
                                popout: true,
                            },
                        ));
                    });
                    self.native_jump.finish_ui_pass();
                    let owner = self.native_jump.update_navigation_owner_for_viewport(
                        &child_ctx,
                        binding,
                        input.application_input,
                    );
                    if self.native_jump.prepare_pass_for_viewport(
                        &child_ctx,
                        binding,
                        owner,
                        input.application_input,
                    ) {
                        super::jump_mode::paint::paint(&child_ctx, &self.native_jump);
                    }
                    if !child_ctx.will_discard() {
                        self.native_jump.complete_frame_for_viewport(
                            &child_ctx,
                            binding,
                            owner,
                            input.application_input,
                        );
                    }
                } else {
                    frame.embedded_layer = Some(child.layer_id());
                    frame.embedded_focus = child_ctx.memory(|memory| memory.focused());
                    frame.focus = self.state.focus_pending() && input.root_input && visible;
                    let close_owner = input.root_owner
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
                *draft = checkpoint;
            }
            self.clear_jump();
            return None;
        }
        self.state
            .complete_bootstrap(frame.host, frame.embedded_layer, frame.embedded_focus);
        let close = frame.close || frame.os_close;
        if frame.focus && !close {
            if frame.viewport_info.is_some() {
                ctx.send_viewport_cmd_to(
                    spec.viewport(frame.instance),
                    egui::ViewportCommand::Focus,
                );
            } else if let Some(layer) = frame.embedded_layer {
                ctx.move_to_top(layer);
            }
            self.state.complete_focus(frame.host);
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
                None,
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
        self.clear_jump();
        if !popout && let Some(layer) = spec.kind.layer_id() {
            ctx.move_to_top(layer);
        }
        ctx.request_repaint_of(egui::ViewportId::ROOT);
        !popout
    }
}

fn snapshot<D: Clone>(ctx: &egui::Context, draft: &D, allowed: bool) -> Option<D> {
    (allowed
        && ctx.input(|input| {
            input.pointer.any_down()
                || input.events.iter().any(|event| {
                    !matches!(
                        event,
                        egui::Event::PointerMoved(_)
                            | egui::Event::MouseMoved(_)
                            | egui::Event::PointerGone
                    )
                })
        }))
    .then(|| draft.clone())
}

fn close_shortcut(ctx: &egui::Context, allowed: bool) -> bool {
    allowed
        && ctx.input_mut(|input| {
            input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                || input.consume_key(egui::Modifiers::COMMAND, egui::Key::W)
        })
}
