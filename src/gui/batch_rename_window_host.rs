use eframe::egui;

use super::batch_rename_dialog::{BatchRenameHostActions, BatchRenameInteractionCheckpoint};
use super::jump_mode::ui::{
    JumpModeScope, JumpModeUi, NavigationDialogKind, NavigationOwner, NavigationViewportBinding,
    batch_rename_confirmation_layer_id,
};
use super::jump_mode::{SurfaceId, TargetGeneration};
use super::utility_dialog_size::{
    apply_native_utility_geometry_restore, close_native_utility_geometry_viewport,
};
use super::utility_host::{
    HostIdentity, UtilityHostState, UtilityPresentation, ViewportPassProvenance,
};
use super::utility_window_geometry::NativeViewportIdentity;
use super::{BatchRenameDialogState, BatchRenameUiCommand};
use crate::file_operations::RenameRuntimeSnapshot;

const KEY: &str = "batch-rename";
const SIZE: egui::Vec2 = egui::vec2(1040.0, 760.0);
const MINIMUM: egui::Vec2 = egui::vec2(520.0, 360.0);

fn docked_layer() -> egui::LayerId {
    NavigationDialogKind::BatchRename
        .layer_id()
        .expect("Batch Rename layer")
}

fn viewport(session: u64) -> egui::ViewportId {
    egui::ViewportId::from_hash_of((KEY, session))
}

fn title(session: u64) -> String {
    format!("Batch Renamer — {session}")
}

fn geometry_identity(session: u64, host: HostIdentity) -> NativeViewportIdentity {
    NativeViewportIdentity {
        key: crate::settings::UtilityToolKey::parse(KEY).expect("Batch Rename geometry key"),
        viewport_id: viewport(session),
        owner_generation: session,
        host_generation: host.generation,
    }
}

#[derive(Default)]
pub(super) struct BatchRenameWindowHost {
    state: UtilityHostState,
    native_session: Option<u64>,
    pending: Option<HostFrame>,
    deferred_close: Option<(u64, HostIdentity)>,
    native_jump: JumpModeUi,
    native_jump_frame: Option<u64>,
    #[cfg(test)]
    pub(super) test_button: Option<(egui::Rect, egui::Rect, bool)>,
}

struct HostFrame {
    root: ViewportPassProvenance,
    owner: ViewportPassProvenance,
    host: HostIdentity,
    session: u64,
    first: bool,
    os_close: bool,
    close: bool,
    actions: Option<BatchRenameHostActions>,
    checkpoint: Option<BatchRenameInteractionCheckpoint>,
    native_info: Option<egui::ViewportInfo>,
    embedded_layer: Option<egui::LayerId>,
    embedded_focus: Option<egui::Id>,
    focus: bool,
}

pub(super) struct HostInput<'a> {
    pub first: bool,
    pub root_visible: bool,
    pub root_input: bool,
    pub native_input: bool,
    pub root_owner: NavigationOwner,
    pub parent_owner: bool,
    pub root_jump_active: bool,
    pub close_shortcut: bool,
    pub confirmation_owner: NavigationOwner,
    pub parent_instance: u64,
    pub root_scopes: (Option<&'a JumpModeScope>, Option<&'a JumpModeScope>),
}

pub(super) struct HostCompletion {
    pub commands: Vec<BatchRenameUiCommand>,
    pub show_root: bool,
}

impl BatchRenameWindowHost {
    #[cfg(test)]
    pub(super) fn test_native_target(&self, label: &str) -> Option<egui::Rect> {
        self.native_jump
            .retained_registry()
            .targets()
            .iter()
            .find(|target| target.metadata().label() == label && target.enabled())
            .map(|target| target.rect())
    }

    pub(super) fn root_layer(&self) -> Option<egui::LayerId> {
        if self.state.presentation() == UtilityPresentation::Docked {
            Some(docked_layer())
        } else {
            self.state.embedded_root_layer()
        }
    }

    pub(super) fn needs_root_ui_when_hidden(&self, open: bool) -> bool {
        (open && self.state.presentation() == UtilityPresentation::Popout)
            || self.state.bootstrap_pending()
            || self.pending.is_some()
    }

    pub(super) fn request_focus(&mut self) {
        self.state.request_focus();
    }

    pub(super) fn configure_jump(&mut self, root: &JumpModeUi, enabled: bool) {
        let (chord, alphabet) = root.configuration();
        self.native_jump.set_configuration(chord, alphabet);
        self.native_jump.set_enabled(enabled);
    }

    pub(super) fn retire(&mut self, ctx: &egui::Context) {
        self.close_viewport(ctx);
        self.pending = None;
        self.deferred_close = None;
        self.state.retire_to_docked();
        self.cancel_jump();
    }

    fn close_viewport(&mut self, ctx: &egui::Context) {
        if let Some(session) = self.native_session.take() {
            let identity = geometry_identity(session, self.state.identity());
            close_native_utility_geometry_viewport(ctx, identity);
            ctx.send_viewport_cmd_to(identity.viewport_id, egui::ViewportCommand::Close);
        }
    }

    fn cancel_jump(&mut self) {
        self.native_jump.cancel_all();
        self.native_jump_frame = None;
    }

    pub(super) fn show(
        &mut self,
        ctx: &egui::Context,
        dialog: &mut BatchRenameDialogState,
        runtime: &RenameRuntimeSnapshot,
        input: HostInput<'_>,
    ) {
        if !dialog.open {
            if self.native_session.is_some() {
                self.retire(ctx);
            }
            return;
        }
        let root = ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT);
        let host = self.state.identity();
        let session = dialog.jump_mode_target_generation();
        let mut os_close = self.deferred_close.take() == Some((session, host));
        if let Some(previous) = self.pending.take()
            && previous.root.frame_nr == root.frame_nr
            && previous.host == host
            && previous.session == session
        {
            if let Some(checkpoint) = previous.checkpoint {
                checkpoint.restore(dialog);
            }
            os_close |= previous.os_close;
        }
        let popped_out = host.host == UtilityPresentation::Popout;
        if !input.root_visible && !popped_out {
            return;
        }
        let first = input.first && !ctx.will_discard();
        let mut frame = HostFrame {
            root,
            owner: root,
            host,
            session,
            first,
            os_close,
            close: false,
            actions: None,
            checkpoint: None,
            native_info: None,
            embedded_layer: None,
            embedded_focus: None,
            focus: false,
        };
        let root_close = first
            && input.close_shortcut
            && input.parent_owner
            && !input.root_jump_active
            && !egui::Popup::is_any_open(ctx)
            && consume_close(ctx);
        if !popped_out {
            frame.close = root_close;
            draw(
                ctx,
                dialog,
                runtime,
                &mut frame,
                None,
                input.root_scopes,
                input.root_owner,
                input.parent_owner,
                input.confirmation_owner,
                input.parent_instance,
            );
        } else {
            if self.native_session != Some(session) {
                self.close_viewport(ctx);
                self.native_session = Some(session);
                self.state.request_focus();
            }
            let id = viewport(session);
            let builder = egui::ViewportBuilder::default()
                .with_title(title(session))
                .with_inner_size(SIZE)
                .with_min_inner_size(MINIMUM)
                .with_resizable(true);
            ctx.show_viewport_immediate(id, builder, |child, class| {
                let child_ctx = child.ctx().clone();
                let native =
                    class == egui::ViewportClass::Immediate && child_ctx.viewport_id() == id;
                let child_first = child.current_pass_index() == 0;
                if !child_first && let Some(checkpoint) = frame.checkpoint.take() {
                    checkpoint.restore(dialog);
                }
                frame.owner = ViewportPassProvenance::current(&child_ctx, child_ctx.viewport_id());
                frame.first = first && child_first && !child_ctx.will_discard();
                frame.actions = None;
                if native {
                    let info = child.input(|input| input.viewport().clone());
                    frame.os_close |= info.close_requested();
                    frame.native_info = Some(info);
                    frame.focus = self.state.focus_pending() && input.native_input;
                    self.draw_native(
                        &child_ctx,
                        dialog,
                        runtime,
                        &mut frame,
                        id,
                        input.native_input,
                        input.close_shortcut,
                    );
                } else {
                    frame.close = root_close;
                    frame.embedded_layer = Some(child.layer_id());
                    frame.embedded_focus = child_ctx.memory(|memory| memory.focused());
                    frame.focus = self.state.focus_pending() && input.root_input;
                    draw(
                        &child_ctx,
                        dialog,
                        runtime,
                        &mut frame,
                        Some(child),
                        input.root_scopes,
                        input.root_owner,
                        input.parent_owner,
                        input.confirmation_owner,
                        input.parent_instance,
                    );
                }
            });
        }
        #[cfg(test)]
        {
            self.test_button = frame
                .actions
                .as_ref()
                .and_then(|actions| actions.host_button);
        }
        self.pending = Some(frame);
    }

    fn draw_native(
        &mut self,
        ctx: &egui::Context,
        dialog: &mut BatchRenameDialogState,
        runtime: &RenameRuntimeSnapshot,
        frame: &mut HostFrame,
        viewport: egui::ViewportId,
        input: bool,
        close_shortcut: bool,
    ) {
        let instance = frame.session;
        let layer = docked_layer();
        let parent = NavigationViewportBinding {
            viewport,
            base_layer: layer,
            owner: NavigationOwner::Dialog {
                kind: NavigationDialogKind::BatchRename,
                instance,
            },
        };
        let child = confirmation_binding(dialog, viewport, instance);
        let binding = child.unwrap_or(parent);
        let native_frame = ctx.cumulative_frame_nr();
        if self.native_jump_frame != Some(native_frame) {
            self.native_jump.begin_frame();
            self.native_jump_frame = Some(native_frame);
        }
        let owner = self
            .native_jump
            .update_navigation_owner_for_viewport(ctx, binding, input);
        let jump_current = self.native_jump.is_enabled() && owner == binding.owner;
        let popup_layer = dialog.owned_combo_layer(ctx, layer);
        let parent_current = input
            && child.is_none()
            && parent.base_is_current(ctx)
            && parent.focus_is_owned(ctx, popup_layer)
            && (!egui::Popup::is_any_open(ctx) || popup_layer.is_some());
        let confirmation_owner = if input
            && child.is_some()
            && binding.base_is_current(ctx)
            && binding.focus_is_owned(ctx, None)
            && !egui::Popup::is_any_open(ctx)
        {
            binding.owner
        } else {
            NavigationOwner::Blocked
        };
        self.native_jump
            .handle_viewport_input(ctx, jump_current, frame.first, viewport);
        self.native_jump.begin_ui_pass();
        let surface = SurfaceId::dialog(KEY, instance);
        let parent_scope =
            (jump_current && child.is_none() && dialog.jump_mode_parent_targets_allowed())
                .then(|| {
                    self.native_jump.scope_for_viewport_binding(
                        surface.clone(),
                        TargetGeneration(instance),
                        "batch-rename/form",
                        binding,
                    )
                })
                .flatten();
        let child_scope = if jump_current
            && let NavigationOwner::BatchRenameConfirmation {
                confirmation_instance,
                kind,
                ..
            } = owner
        {
            self.native_jump.scope_for_viewport_binding(
                SurfaceId::popup(kind.surface_kind(), confirmation_instance, surface),
                TargetGeneration(confirmation_instance),
                "batch-rename/confirmation",
                binding,
            )
        } else {
            None
        };
        if frame.first
            && close_shortcut
            && parent_current
            && !frame.os_close
            && self.native_jump.active_session().is_none()
            && !egui::Popup::is_any_open(ctx)
            && consume_close(ctx)
        {
            frame.close = true;
        }
        egui::Window::new("Batch Renamer")
            .id(layer.id)
            .title_bar(false)
            .fixed_rect(ctx.content_rect())
            .movable(false)
            .resizable(false)
            .vscroll(true)
            .show(ctx, |ui| {
                draw(
                    ctx,
                    dialog,
                    runtime,
                    frame,
                    Some(ui),
                    (parent_scope.as_ref(), child_scope.as_ref()),
                    owner,
                    parent_current,
                    confirmation_owner,
                    instance,
                );
            });
        self.native_jump.finish_ui_pass();
        let binding_after = confirmation_binding(dialog, viewport, instance).unwrap_or(parent);
        if binding_after != binding {
            ctx.request_repaint_of(viewport);
        }
        let binding = binding_after;
        let owner = self
            .native_jump
            .update_navigation_owner_for_viewport(ctx, binding, input);
        if self
            .native_jump
            .prepare_pass_for_viewport(ctx, binding, owner, input)
        {
            super::jump_mode::paint::paint(ctx, &self.native_jump);
        }
        if !ctx.will_discard() {
            self.native_jump
                .complete_frame_for_viewport(ctx, binding, owner, input);
        }
    }

    pub(super) fn complete(
        &mut self,
        ctx: &egui::Context,
        dialog: &mut BatchRenameDialogState,
    ) -> Option<HostCompletion> {
        let mut frame = self.pending.take()?;
        let current = dialog.open
            && frame.session == dialog.jump_mode_target_generation()
            && frame.host == self.state.identity();
        if frame.os_close {
            ctx.send_viewport_cmd_to(viewport(frame.session), egui::ViewportCommand::CancelClose);
        }
        if !current
            || !frame.first
            || !frame.root.was_retained(ctx)
            || !frame.owner.was_retained(ctx)
        {
            if current {
                if frame.os_close {
                    self.deferred_close = Some((frame.session, frame.host));
                }
                if let Some(checkpoint) = frame.checkpoint.take() {
                    checkpoint.restore(dialog);
                }
                ctx.request_repaint_of(frame.owner.viewport_id);
            }
            self.cancel_jump();
            return None;
        }
        if frame.close || frame.os_close {
            self.retire(ctx);
            return Some(HostCompletion {
                commands: vec![BatchRenameUiCommand::Closed],
                show_root: false,
            });
        }
        self.state
            .complete_bootstrap(frame.host, frame.embedded_layer, frame.embedded_focus);
        let native = frame
            .native_info
            .as_ref()
            .map(|_| geometry_identity(frame.session, frame.host));
        if let (Some(identity), Some(info)) = (native, frame.native_info.as_ref()) {
            apply_native_utility_geometry_restore(
                ctx,
                egui::ViewportClass::Immediate,
                true,
                &title(frame.session),
                identity,
                info,
                SIZE,
                Some(MINIMUM),
                None,
            );
        }
        if frame.focus {
            if let Some(identity) = native {
                ctx.send_viewport_cmd_to(identity.viewport_id, egui::ViewportCommand::Focus);
            } else if let Some(layer) = frame.embedded_layer {
                ctx.move_to_top(layer);
            }
            self.state.complete_focus(frame.host);
        }
        let actions = frame.actions.take()?;
        let presentation = actions.host_request.map(|popout| {
            if popout {
                UtilityPresentation::Popout
            } else {
                UtilityPresentation::Docked
            }
        });
        #[cfg(windows)]
        let hwnd = super::utility_dialog_size::utility_picker_hwnd(ctx, native);
        #[cfg(not(windows))]
        let hwnd = None;
        let commands = dialog.complete_host_actions(actions, hwnd);
        let mut show_root = false;
        if commands.contains(&BatchRenameUiCommand::Closed) {
            self.retire(ctx);
        } else if let Some(presentation) = presentation
            && presentation != frame.host.host
        {
            self.close_viewport(ctx);
            self.state.transition(presentation);
            self.cancel_jump();
            show_root = presentation == UtilityPresentation::Docked;
            if show_root {
                ctx.move_to_top(docked_layer());
            }
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
        Some(HostCompletion {
            commands,
            show_root,
        })
    }
}

fn confirmation_binding(
    dialog: &BatchRenameDialogState,
    viewport: egui::ViewportId,
    instance: u64,
) -> Option<NavigationViewportBinding> {
    dialog
        .jump_mode_pending_confirmation_identity()
        .map(|(confirmation_instance, kind)| NavigationViewportBinding {
            viewport,
            base_layer: batch_rename_confirmation_layer_id(instance, confirmation_instance, kind),
            owner: NavigationOwner::BatchRenameConfirmation {
                parent_instance: instance,
                confirmation_instance,
                kind,
            },
        })
}

fn consume_close(ctx: &egui::Context) -> bool {
    ctx.input_mut(|input| {
        input.consume_key(egui::Modifiers::COMMAND, egui::Key::W)
            || input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
    })
}

#[cfg(test)]
pub(super) fn test_viewport(session: u64) -> egui::ViewportId {
    viewport(session)
}

fn draw(
    ctx: &egui::Context,
    dialog: &mut BatchRenameDialogState,
    runtime: &RenameRuntimeSnapshot,
    frame: &mut HostFrame,
    child_ui: Option<&mut egui::Ui>,
    scopes: (Option<&JumpModeScope>, Option<&JumpModeScope>),
    owner: NavigationOwner,
    parent_current: bool,
    child_owner: NavigationOwner,
    parent_instance: u64,
) {
    let allow = frame.first && !frame.os_close && !frame.close;
    let capture = allow
        && (parent_current || child_owner != NavigationOwner::Blocked)
        && (scopes.0.is_some()
            || scopes.1.is_some()
            || ctx.input(|input| {
                input.pointer.any_down()
                    || input.events.iter().any(|event| {
                        !matches!(
                            event,
                            egui::Event::PointerMoved(_) | egui::Event::MouseMoved(_)
                        )
                    })
            }));
    if capture {
        frame.checkpoint = Some(BatchRenameInteractionCheckpoint::capture(dialog));
    }
    frame.actions = Some(dialog.ui_with_host(
        ctx,
        scopes.0,
        scopes.1,
        owner,
        allow && parent_current,
        if allow {
            child_owner
        } else {
            NavigationOwner::Blocked
        },
        allow,
        Some(parent_instance),
        runtime,
        child_ui,
        frame.host.host == UtilityPresentation::Popout,
    ));
}
