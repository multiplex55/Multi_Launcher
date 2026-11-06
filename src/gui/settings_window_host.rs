use eframe::egui;

use super::jump_mode::ui::{
    JumpModeScope, JumpModeUi, NavigationOwner, NavigationViewportBinding, focused_response_layer,
    settings_layer_id, visible_top_middle_layer,
};
use super::utility_host::{
    HostIdentity, UtilityHostState, UtilityPresentation, ViewportPassProvenance,
};
use super::utility_window_geometry::NativeViewportIdentity;
use super::{LauncherApp, Panel};
use crate::settings_editor::SettingsEditor;
use crate::settings_editor::render::SettingsUiActions;

const KEY: &str = "settings";

fn viewport(instance: u64) -> egui::ViewportId {
    egui::ViewportId::from_hash_of((KEY, instance))
}

fn native_title(instance: u64) -> String {
    format!("Settings — {instance}")
}

fn identity(host: HostIdentity, instance: u64) -> Option<NativeViewportIdentity> {
    (host.host == UtilityPresentation::Popout).then_some(NativeViewportIdentity {
        key: crate::settings::UtilityToolKey::parse(KEY).ok()?,
        viewport_id: viewport(instance),
        owner_generation: instance,
        host_generation: host.generation,
    })
}

#[derive(Default)]
pub(super) struct SettingsWindowHost {
    state: UtilityHostState,
    instance: u64,
    pending: Option<SettingsHostFrame>,
    close_pending: Option<ViewportPassProvenance>,
    native_jump: JumpModeUi,
    native_jump_frame: Option<u64>,
    #[cfg(test)]
    test_button: Option<(egui::Rect, egui::Rect, bool)>,
}

struct SettingsHostFrame {
    root: ViewportPassProvenance,
    owner: Option<ViewportPassProvenance>,
    host: HostIdentity,
    instance: u64,
    authorized: bool,
    actions: Option<SettingsUiActions>,
    checkpoint: Option<SettingsEditor>,
    os_close: bool,
    close: bool,
    focus: bool,
    class: egui::ViewportClass,
    viewport_info: Option<egui::ViewportInfo>,
    embedded_layer: Option<egui::LayerId>,
    embedded_focus: Option<egui::Id>,
}

impl SettingsWindowHost {
    #[cfg(test)]
    pub(super) fn test_host_button(&self) -> (egui::Rect, egui::Rect, bool) {
        self.test_button.expect("Settings host button was rendered")
    }

    #[cfg(test)]
    pub(super) fn test_viewport_id(&self) -> egui::ViewportId {
        viewport(self.instance)
    }

    pub(super) fn is_popped_out(&self) -> bool {
        self.state.presentation() == UtilityPresentation::Popout
    }

    pub(super) fn root_layer(&self) -> Option<egui::LayerId> {
        if self.is_popped_out() {
            self.state.embedded_root_layer()
        } else {
            Some(settings_layer_id())
        }
    }

    pub(super) fn needs_root_ui_when_hidden(&self, editor_open: bool) -> bool {
        (editor_open && self.is_popped_out())
            || self.state.bootstrap_pending()
            || self.pending.is_some()
    }

    pub(super) fn request_focus(&mut self) {
        self.state.request_focus();
    }

    pub(super) fn request_close(&mut self, ctx: &egui::Context) {
        self.close_pending = Some(ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT));
    }

    pub(super) fn retire(&mut self, ctx: &egui::Context) {
        self.retire_native_viewport(ctx);
        self.state.retire_to_docked();
        self.pending = None;
        self.close_pending = None;
        self.native_jump.cancel_all();
        self.native_jump_frame = None;
    }

    fn retire_native_viewport(&self, ctx: &egui::Context) {
        if let Some(identity) = identity(self.state.identity(), self.instance) {
            super::utility_dialog_size::close_native_utility_geometry_viewport(ctx, identity);
            ctx.send_viewport_cmd_to(identity.viewport_id, egui::ViewportCommand::Close);
        }
    }

    pub(super) fn show(
        &mut self,
        ctx: &egui::Context,
        app: &mut LauncherApp,
        root_scope: Option<JumpModeScope>,
        instance: u64,
        first: bool,
        root_input: bool,
        root_owner: bool,
        root_jump_active: bool,
        application_input: bool,
        jump_enabled: bool,
    ) {
        if !app.show_settings {
            self.retire(ctx);
            return;
        }
        let host = self.state.identity();
        let root = ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT);
        let mut editor = std::mem::take(&mut app.settings_editor);
        let mut os_close = false;
        if let Some(mut previous) = self.pending.take()
            && previous.root.frame_nr == root.frame_nr
            && previous.instance == instance
            && previous.host == host
        {
            if let Some(checkpoint) = previous.checkpoint.take() {
                editor = checkpoint;
            }
            os_close = previous.os_close;
        }
        self.instance = instance;
        let root_first = first && !ctx.will_discard();
        let mut frame = SettingsHostFrame {
            root,
            owner: None,
            host,
            instance,
            authorized: false,
            actions: None,
            checkpoint: None,
            os_close,
            close: self.close_pending.take() == Some(root),
            focus: false,
            class: egui::ViewportClass::Root,
            viewport_info: None,
            embedded_layer: None,
            embedded_focus: None,
        };
        let close_shortcut_allowed = !app.pinned_panels.contains(&Panel::Settings);

        if host.host == UtilityPresentation::Docked {
            frame.owner = Some(root);
            frame.authorized = root_first;
            frame.checkpoint = snapshot_for_input(ctx, &editor, root_first && root_input);
            if root_first && root_owner && !root_jump_active && !frame.close {
                frame.close =
                    handle_close_shortcut(ctx, &mut editor, instance, close_shortcut_allowed);
            }
            let body_input = root_first && root_input && !frame.close;
            frame.actions = Some(editor.ui_with_host(
                ctx,
                app,
                root_scope,
                root_owner.then_some(instance),
                root_jump_active,
                root_first,
                None,
                body_input,
                body_input && root_owner,
                false,
            ));
        } else {
            let viewport_id = viewport(instance);
            let size = SettingsEditor::settings_window_default_size(ctx);
            let builder = egui::ViewportBuilder::default()
                .with_title(native_title(instance))
                .with_inner_size(size)
                .with_min_inner_size([460.0, SettingsEditor::SETTINGS_WINDOW_MIN_HEIGHT])
                .with_resizable(true)
                .with_visible(true);
            let focus_pending = self.state.focus_pending();
            let (chord, alphabet) = app.jump_mode_ui.configuration();
            self.native_jump.set_configuration(chord, alphabet);
            self.native_jump.set_enabled(jump_enabled);
            let launcher_close = frame.close;
            ctx.show_viewport_immediate(viewport_id, builder, |child, class| {
                let child_ctx = child.ctx().clone();
                let native = class == egui::ViewportClass::Immediate
                    && child_ctx.viewport_id() == viewport_id;
                if native && child.current_pass_index() > 0 {
                    if let Some(checkpoint) = frame.checkpoint.take() {
                        editor = checkpoint;
                    }
                    frame.close = launcher_close;
                }
                frame.class = class;
                frame.owner = Some(ViewportPassProvenance::current(
                    &child_ctx,
                    child_ctx.viewport_id(),
                ));
                let owner_first =
                    root_first && child.current_pass_index() == 0 && !child_ctx.will_discard();
                frame.authorized = owner_first;
                let focused = child_ctx.input(|input| input.focused);
                let popup = egui::Popup::is_any_open(&child_ctx);
                let modal = child_ctx.memory(|memory| memory.top_modal_layer().is_some());
                let visible = native || (child.is_visible() && !child.is_sizing_pass());
                let body_input = owner_first
                    && visible
                    && focused
                    && !modal
                    && if native {
                        application_input && visible_top_middle_layer(&child_ctx).is_none()
                    } else {
                        root_input
                    };
                frame.checkpoint = snapshot_for_input(&child_ctx, &editor, body_input);
                if native {
                    let viewport_info = child.input(|input| input.viewport().clone());
                    frame.os_close |= viewport_info.close_requested();
                    frame.viewport_info = Some(viewport_info);
                    frame.focus = focus_pending && application_input && visible && !modal;
                    let binding = NavigationViewportBinding {
                        viewport: viewport_id,
                        owner: NavigationOwner::Settings { instance },
                        base_layer: egui::LayerId::background(),
                    };
                    let frame_nr = child_ctx.cumulative_frame_nr();
                    if self.native_jump_frame != Some(frame_nr) {
                        self.native_jump.begin_frame();
                        self.native_jump_frame = Some(frame_nr);
                    }
                    let owner_input = body_input
                        && !popup
                        && focused_response_layer(&child_ctx)
                            .is_none_or(|layer| layer == binding.base_layer);
                    let navigation_owner = self.native_jump.update_navigation_owner_for_viewport(
                        &child_ctx,
                        binding,
                        application_input,
                    );
                    let jump_current = jump_enabled && navigation_owner == binding.owner;
                    self.native_jump.handle_viewport_input(
                        &child_ctx,
                        jump_current,
                        owner_first,
                        viewport_id,
                    );
                    let jump_active = self.native_jump.active_session().is_some();
                    if owner_input && !jump_active && !frame.close && !frame.os_close {
                        frame.close = handle_close_shortcut(
                            &child_ctx,
                            &mut editor,
                            instance,
                            close_shortcut_allowed,
                        );
                    }
                    self.native_jump.begin_ui_pass();
                    let scope = if jump_current && !frame.close && !frame.os_close {
                        self.native_jump.scope_for_viewport_binding(
                            super::jump_mode::SurfaceId::Settings { instance },
                            super::jump_mode::TargetGeneration(instance),
                            KEY,
                            binding,
                        )
                    } else {
                        None
                    };
                    let input = body_input && !frame.close && !frame.os_close;
                    egui::CentralPanel::default().show(child, |ui| {
                        frame.actions = Some(editor.ui_with_host(
                            &child_ctx,
                            app,
                            scope,
                            owner_input.then_some(instance),
                            jump_active,
                            owner_first,
                            Some(ui),
                            input,
                            input && owner_input,
                            true,
                        ));
                    });
                    self.native_jump.finish_ui_pass();
                    let owner = self.native_jump.update_navigation_owner_for_viewport(
                        &child_ctx,
                        binding,
                        application_input,
                    );
                    if self.native_jump.prepare_pass_for_viewport(
                        &child_ctx,
                        binding,
                        owner,
                        application_input,
                    ) {
                        super::jump_mode::paint::paint(&child_ctx, &self.native_jump);
                    }
                    if !child_ctx.will_discard() {
                        self.native_jump.complete_frame_for_viewport(
                            &child_ctx,
                            binding,
                            owner,
                            application_input,
                        );
                    }
                } else {
                    let layer = child.layer_id();
                    frame.embedded_layer = Some(layer);
                    frame.embedded_focus = child_ctx.memory(|memory| memory.focused());
                    frame.focus = focus_pending && root_input && visible;
                    if body_input && root_owner && !root_jump_active && !frame.close {
                        frame.close = handle_close_shortcut(
                            &child_ctx,
                            &mut editor,
                            instance,
                            close_shortcut_allowed,
                        );
                    }
                    let input = body_input && !frame.close;
                    frame.actions = Some(editor.ui_with_host(
                        &child_ctx,
                        app,
                        root_scope.clone(),
                        root_owner.then_some(instance),
                        root_jump_active,
                        owner_first,
                        Some(child),
                        input,
                        input && root_owner,
                        true,
                    ));
                }
            });
        }
        app.settings_editor = editor;
        #[cfg(test)]
        {
            self.test_button = frame
                .actions
                .as_ref()
                .and_then(|actions| actions.host_button);
        }
        self.pending = Some(frame);
    }

    pub(super) fn complete(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        let Some(mut frame) = self.pending.take() else {
            return;
        };
        let current = app.show_settings
            && frame.instance == self.instance
            && frame.host == self.state.identity();
        let retained = frame.authorized
            && frame.root.was_retained(ctx)
            && frame.owner.is_some_and(|pass| pass.was_retained(ctx));
        if frame.os_close {
            ctx.send_viewport_cmd_to(viewport(frame.instance), egui::ViewportCommand::CancelClose);
        }
        if !current || !retained {
            if current && let Some(checkpoint) = frame.checkpoint.take() {
                app.settings_editor = checkpoint;
            }
            self.native_jump.cancel_all();
            self.native_jump_frame = None;
            return;
        }
        self.state
            .complete_bootstrap(frame.host, frame.embedded_layer, frame.embedded_focus);
        if frame.focus && !frame.close && !frame.os_close {
            if frame.class == egui::ViewportClass::Immediate {
                ctx.send_viewport_cmd_to(viewport(frame.instance), egui::ViewportCommand::Focus);
            } else if let Some(layer) = frame.embedded_layer {
                ctx.move_to_top(layer);
            }
            self.state.complete_focus(frame.host);
        }
        let mut actions = frame.actions.unwrap_or_default();
        if frame.close || frame.os_close {
            actions = SettingsUiActions::default();
            actions.request_close();
        }
        let requested = actions.host_request;
        let mut editor = std::mem::take(&mut app.settings_editor);
        let show_root = editor.complete_host_actions(ctx, app, actions);
        app.settings_editor = editor;
        if show_root {
            app.request_launcher_visibility(true);
        }
        if !app.show_settings {
            app.settings_editor.retire_all_settings_navigation();
            app.panel_states.settings = false;
            app.panel_stack.retain(|panel| *panel != Panel::Settings);
            self.retire(ctx);
        } else if let Some(popout) = requested {
            let next = if popout {
                UtilityPresentation::Popout
            } else {
                UtilityPresentation::Docked
            };
            if next != self.state.presentation() {
                if !popout {
                    self.retire_native_viewport(ctx);
                }
                self.state.transition(next);
                self.native_jump.cancel_all();
                self.native_jump_frame = None;
                if !popout {
                    ctx.move_to_top(settings_layer_id());
                    app.request_launcher_visibility(true);
                }
                ctx.request_repaint_of(egui::ViewportId::ROOT);
            }
        } else if frame.class == egui::ViewportClass::Immediate
            && let (Some(identity), Some(viewport_info)) = (
                identity(frame.host, frame.instance),
                frame.viewport_info.as_ref(),
            )
        {
            super::utility_dialog_size::apply_native_utility_geometry_restore(
                ctx,
                frame.class,
                true,
                &native_title(frame.instance),
                identity,
                viewport_info,
                egui::Vec2::from(SettingsEditor::settings_window_default_size(ctx)),
                Some(egui::vec2(
                    460.0,
                    SettingsEditor::SETTINGS_WINDOW_MIN_HEIGHT,
                )),
                None,
            );
        }
    }
}

fn snapshot_for_input(
    ctx: &egui::Context,
    editor: &SettingsEditor,
    allowed: bool,
) -> Option<SettingsEditor> {
    (allowed
        && (editor.pending_settings_navigation.is_some()
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
    .then(|| editor.clone())
}

fn handle_close_shortcut(
    ctx: &egui::Context,
    editor: &mut SettingsEditor,
    instance: u64,
    close_allowed: bool,
) -> bool {
    if ctx.input(|input| input.key_pressed(egui::Key::Escape))
        && editor.handle_search_escape_before_container_close(ctx, instance)
    {
        return false;
    }
    close_allowed
        && ctx.input_mut(|input| {
            input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                || input.consume_key(egui::Modifiers::COMMAND, egui::Key::W)
        })
}
