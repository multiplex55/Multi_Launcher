use eframe::egui;

use super::file_search_dialog::{FileSearchHostActions, FileSearchHostCheckpoint};
use super::file_search_preview_dialog::FileSearchPreviewHostActions;
use super::jump_mode::SurfaceId;
use super::jump_mode::ui::{
    JumpModeScope, JumpModeUi, NavigationDialogKind, NavigationOwner, NavigationViewportBinding,
    focused_response_layer, visible_top_middle_layer,
};
use super::utility_dialog_size::{
    UtilityDialogSizeWindowExt, UtilityWindowSizeDefault, UtilityWindowSizeRecipe,
    apply_native_utility_geometry_restore, close_native_utility_geometry_viewport,
};
use super::utility_host::{
    HostIdentity, UtilityHostState, UtilityPresentation, ViewportPassProvenance,
};
use super::utility_window_geometry::NativeViewportIdentity;
use super::{FileSearchDialogState, FileSearchUiCommand};
use crate::file_search::coordinator::SearchCoordinator;

const KEY: &str = "file-search";
const SIZE: egui::Vec2 = egui::vec2(760.0, 560.0);
const MINIMUM: egui::Vec2 = egui::vec2(520.0, 360.0);

fn docked_layer() -> egui::LayerId {
    NavigationDialogKind::FileSearch
        .layer_id()
        .expect("File Search window layer")
}

fn viewport(instance: u64) -> egui::ViewportId {
    egui::ViewportId::from_hash_of((KEY, instance))
}

fn title(instance: u64) -> String {
    format!("File Search — {instance}")
}

fn identity(instance: u64, host: HostIdentity) -> NativeViewportIdentity {
    NativeViewportIdentity {
        key: crate::settings::UtilityToolKey::parse(KEY).expect("File Search geometry key"),
        viewport_id: viewport(instance),
        owner_generation: instance,
        host_generation: host.generation,
    }
}

#[derive(Default)]
pub(super) struct FileSearchWindowHost {
    state: UtilityHostState,
    presentation_prepared: bool,
    instance: u64,
    pending: Option<HostFrame>,
    close_pending: Option<ViewportPassProvenance>,
    native_jump: JumpModeUi,
    native_jump_frame: Option<u64>,
    #[cfg(test)]
    test_button: Option<(egui::Rect, egui::Rect, bool)>,
}

struct HostFrame {
    root: ViewportPassProvenance,
    owner: ViewportPassProvenance,
    identity: HostIdentity,
    instance: u64,
    first: bool,
    preview_only: bool,
    actions: Option<FileSearchHostActions>,
    checkpoint: Option<FileSearchHostCheckpoint>,
    preview: Option<FileSearchPreviewHostActions>,
    close: bool,
    os_close: bool,
    native_info: Option<egui::ViewportInfo>,
    embedded_layer: Option<egui::LayerId>,
    embedded_focus: Option<egui::Id>,
    focus: bool,
}

pub(super) struct HostInput {
    pub first: bool,
    pub root_visible: bool,
    pub root_input: bool,
    pub root_owner: bool,
    pub native_input: bool,
    pub close_shortcut: bool,
}

pub(super) struct HostCompletion {
    pub root_pass: ViewportPassProvenance,
    pub commands: Vec<FileSearchUiCommand>,
    pub show_root: bool,
    pub closed: bool,
}

impl FileSearchWindowHost {
    pub(super) fn root_layer(&self) -> Option<egui::LayerId> {
        if self.is_popped_out() {
            self.state.embedded_root_layer()
        } else {
            Some(docked_layer())
        }
    }

    pub(super) fn is_popped_out(&self) -> bool {
        self.state.presentation() == UtilityPresentation::Popout
    }

    pub(super) fn needs_root_ui_when_hidden(&self, open: bool) -> bool {
        (open && self.is_popped_out()) || self.state.bootstrap_pending() || self.pending.is_some()
    }

    pub(super) fn prepare_open(&mut self, ctx: &egui::Context, open: bool) {
        if open && !self.presentation_prepared {
            let key = crate::settings::UtilityToolKey::parse(KEY).expect("registered utility key");
            super::utility_host::restore_remembered_presentation(ctx, key, &mut self.state);
            self.presentation_prepared = true;
        }
    }

    pub(super) fn remember_current(&self, ctx: &egui::Context) {
        let key = crate::settings::UtilityToolKey::parse(KEY).expect("registered utility key");
        super::utility_host::remember_presentation(ctx, key, self.state.presentation());
    }

    pub(super) fn request_focus(&mut self) {
        self.state.request_focus();
    }

    pub(super) fn request_close(&mut self, ctx: &egui::Context) {
        self.close_pending = Some(ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT));
    }

    fn close_viewport(&self, ctx: &egui::Context) {
        if self.is_popped_out() {
            let identity = identity(self.instance, self.state.identity());
            close_native_utility_geometry_viewport(ctx, identity);
            ctx.send_viewport_cmd_to(identity.viewport_id, egui::ViewportCommand::Close);
        }
    }

    pub(super) fn retire(&mut self, ctx: &egui::Context) {
        self.close_viewport(ctx);
        self.state.retire_to_docked();
        self.presentation_prepared = false;
        self.pending = None;
        self.close_pending = None;
        self.cancel_jump();
    }

    fn cancel_jump(&mut self) {
        self.native_jump.cancel_all();
        self.native_jump_frame = None;
    }

    pub(super) fn configure_jump(&mut self, root: &JumpModeUi, enabled: bool) {
        let (chord, alphabet) = root.configuration();
        self.native_jump.set_configuration(chord, alphabet);
        self.native_jump.set_enabled(enabled);
    }

    pub(super) fn begin_pass(
        &mut self,
        ctx: &egui::Context,
        dialog: &mut FileSearchDialogState,
        coordinator: &mut SearchCoordinator,
        first: bool,
    ) -> bool {
        let root = ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT);
        let mut os_close = false;
        if let Some(previous) = self.pending.take()
            && previous.root.frame_nr == root.frame_nr
            && previous.identity == self.state.identity()
        {
            if let Some(checkpoint) = previous.checkpoint {
                checkpoint.restore(dialog);
            }
            os_close = previous.os_close;
        }
        if first {
            dialog.drain_events(coordinator);
        }
        if dialog.consume_immediate_repaint_request() {
            ctx.request_repaint();
        }
        if dialog.requires_repaint_polling() {
            ctx.request_repaint_after(std::time::Duration::from_millis(32));
        }
        os_close
    }

    pub(super) fn show(
        &mut self,
        ctx: &egui::Context,
        dialog: &mut FileSearchDialogState,
        coordinator: &mut SearchCoordinator,
        input: HostInput,
        instance: u64,
        root_scopes: (Option<JumpModeScope>, Option<JumpModeScope>),
        os_close: bool,
    ) {
        self.prepare_open(ctx, dialog.open);
        if !dialog.open && self.is_popped_out() {
            self.retire(ctx);
        }
        if !input.root_visible && !self.is_popped_out() {
            return;
        }
        if !dialog.open && !dialog.preview_dialog.open {
            return;
        }
        self.instance = instance;
        let root = ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT);
        let first = input.first && !ctx.will_discard();
        let mut frame = HostFrame {
            root,
            owner: root,
            identity: self.state.identity(),
            instance,
            first,
            preview_only: !dialog.open,
            actions: None,
            checkpoint: None,
            preview: None,
            close: self.close_pending.take() == Some(root),
            os_close,
            native_info: None,
            embedded_layer: None,
            embedded_focus: None,
            focus: false,
        };
        if frame.preview_only {
            frame.preview = draw_preview(ctx, dialog, first && input.root_input);
            self.pending = Some(frame);
            return;
        }
        if !self.is_popped_out() {
            let mut open = true;
            let current = first && input.root_owner && !frame.close;
            if first && !input.root_owner {
                surrender_search_focus(ctx, docked_layer());
            }
            frame.checkpoint = capture(ctx, dialog, current, &root_scopes);
            egui::Window::new("File Search")
                .default_size(SIZE)
                .min_size(MINIMUM)
                .resizable(true)
                .show_utility(
                    ctx,
                    KEY,
                    egui::Id::new("File Search"),
                    Some(&mut open),
                    UtilityWindowSizeRecipe::new(
                        UtilityWindowSizeDefault::outer(SIZE),
                        egui::Vec2::ZERO,
                        [None, None],
                    )
                    .with_minimum(MINIMUM),
                    |ui| {
                        frame.actions = Some(dialog.contents_in_host(
                            ui,
                            coordinator,
                            root_scopes.0.clone(),
                            root_scopes.1.clone(),
                            first,
                            current,
                            false,
                        ));
                    },
                );
            frame.close |= current && !open;
            frame.preview = draw_preview(ctx, dialog, first && input.root_input);
        } else {
            let viewport_id = viewport(instance);
            let builder = egui::ViewportBuilder::default()
                .with_title(title(instance))
                .with_inner_size(SIZE)
                .with_min_inner_size(MINIMUM)
                .with_resizable(true);
            let root_close = frame.close;
            ctx.show_viewport_immediate(viewport_id, builder, |child, class| {
                let child_ctx = child.ctx().clone();
                let native = class == egui::ViewportClass::Immediate
                    && child_ctx.viewport_id() == viewport_id;
                let child_first = child.current_pass_index() == 0;
                if native && !child_first {
                    if let Some(checkpoint) = frame.checkpoint.take() {
                        checkpoint.restore(dialog);
                    }
                    frame.close = root_close;
                }
                frame.owner = ViewportPassProvenance::current(&child_ctx, child_ctx.viewport_id());
                frame.first = first && child_first && !child_ctx.will_discard();
                if native {
                    let info = child.input(|input| input.viewport().clone());
                    frame.os_close |= info.close_requested();
                    frame.native_info = Some(info);
                    frame.focus = self.state.focus_pending() && input.native_input;
                    let layer = docked_layer();
                    let binding = NavigationViewportBinding {
                        viewport: viewport_id,
                        owner: NavigationOwner::Dialog {
                            kind: NavigationDialogKind::FileSearch,
                            instance,
                        },
                        base_layer: layer,
                    };
                    let native_frame = child_ctx.cumulative_frame_nr();
                    if self.native_jump_frame != Some(native_frame) {
                        self.native_jump.begin_frame();
                        self.native_jump_frame = Some(native_frame);
                    }
                    let owner = self.native_jump.update_navigation_owner_for_viewport(
                        &child_ctx,
                        binding,
                        input.native_input,
                    );
                    let jump_current = self.native_jump.is_enabled() && owner == binding.owner;
                    self.native_jump.handle_viewport_input(
                        &child_ctx,
                        jump_current,
                        frame.first,
                        viewport_id,
                    );
                    self.native_jump.begin_ui_pass();
                    let surface = SurfaceId::dialog(KEY, instance);
                    let scopes = if jump_current {
                        (
                            self.native_jump.scope_for_viewport_binding(
                                surface.clone(),
                                dialog.jump_mode_target_generation(),
                                "file-search-dialog",
                                binding,
                            ),
                            self.native_jump.scope_for_viewport_binding(
                                surface,
                                dialog.jump_mode_results_target_generation(instance),
                                "file-search-results",
                                binding,
                            ),
                        )
                    } else {
                        (None, None)
                    };
                    let popup = dialog.current_owned_native_popup_layer(&child_ctx);
                    let native_owner = input.native_input
                        && binding.base_is_current(&child_ctx)
                        && (!egui::Popup::is_any_open(&child_ctx) || popup.is_some())
                        && focused_response_layer(&child_ctx)
                            .is_none_or(|focus| focus == layer || Some(focus) == popup);
                    if frame.first && !native_owner {
                        surrender_search_focus(&child_ctx, layer);
                    }
                    let current = frame.first && native_owner && !frame.os_close && !frame.close;
                    if current
                        && input.close_shortcut
                        && !egui::Popup::is_any_open(&child_ctx)
                        && self.native_jump.active_session().is_none()
                        && child_ctx.input_mut(|input| {
                            input.consume_key(egui::Modifiers::COMMAND, egui::Key::W)
                        })
                    {
                        frame.close = true;
                    }
                    let current = current && !frame.close;
                    frame.checkpoint = capture(&child_ctx, dialog, current, &scopes);
                    // Keep the Search and Preview as ordinary egui layers inside
                    // this HWND so clicking either window can raise its own layer.
                    let rect = child_ctx.content_rect();
                    egui::Window::new("File Search")
                        .id(layer.id)
                        .title_bar(false)
                        .fixed_rect(rect)
                        .movable(false)
                        .resizable(false)
                        .show(&child_ctx, |ui| {
                            frame.actions = Some(dialog.contents_in_host(
                                ui,
                                coordinator,
                                scopes.0.clone(),
                                scopes.1.clone(),
                                frame.first,
                                current,
                                true,
                            ));
                        });
                    frame.preview = draw_preview(
                        &child_ctx,
                        dialog,
                        frame.first && input.native_input && !frame.os_close,
                    );
                    self.native_jump.finish_ui_pass();
                    let owner = self.native_jump.update_navigation_owner_for_viewport(
                        &child_ctx,
                        binding,
                        input.native_input,
                    );
                    if self.native_jump.prepare_pass_for_viewport(
                        &child_ctx,
                        binding,
                        owner,
                        input.native_input,
                    ) {
                        super::jump_mode::paint::paint(&child_ctx, &self.native_jump);
                    }
                    if !child_ctx.will_discard() {
                        self.native_jump.complete_frame_for_viewport(
                            &child_ctx,
                            binding,
                            owner,
                            input.native_input,
                        );
                    }
                } else {
                    let layer = child.layer_id();
                    frame.embedded_layer = Some(layer);
                    frame.embedded_focus = child_ctx.memory(|memory| memory.focused());
                    let visible = child.is_visible() && !child.is_sizing_pass();
                    frame.focus = self.state.focus_pending() && input.root_input && visible;
                    let current = frame.first && input.root_owner && visible && !frame.close;
                    if frame.first && !input.root_owner {
                        surrender_search_focus(&child_ctx, layer);
                    }
                    frame.checkpoint = capture(&child_ctx, dialog, current, &root_scopes);
                    frame.actions = Some(dialog.contents_in_host(
                        child,
                        coordinator,
                        root_scopes.0.clone(),
                        root_scopes.1.clone(),
                        frame.first,
                        current,
                        true,
                    ));
                    frame.preview = draw_preview(
                        &child_ctx,
                        dialog,
                        frame.first && input.root_input && visible,
                    );
                }
            });
        }
        #[cfg(test)]
        {
            self.test_button = frame.actions.as_ref().map(|actions| actions.host_button);
        }
        self.pending = Some(frame);
    }

    pub(super) fn complete(
        &mut self,
        ctx: &egui::Context,
        dialog: &mut FileSearchDialogState,
        coordinator: &mut SearchCoordinator,
    ) -> Option<HostCompletion> {
        let mut frame = self.pending.take()?;
        let current = (dialog.open || (frame.preview_only && dialog.preview_dialog.open))
            && frame.identity == self.state.identity()
            && frame.instance == self.instance;
        if frame.os_close {
            ctx.send_viewport_cmd_to(viewport(frame.instance), egui::ViewportCommand::CancelClose);
        }
        if !current
            || !frame.first
            || !frame.root.was_retained(ctx)
            || !frame.owner.was_retained(ctx)
        {
            if current {
                if let Some(checkpoint) = frame.checkpoint.take() {
                    checkpoint.restore(dialog);
                }
                ctx.request_repaint_of(frame.owner.viewport_id);
            }
            self.cancel_jump();
            return None;
        }
        if frame.preview_only {
            if let Some(preview) = frame.preview.take() {
                dialog.preview_dialog.complete_host_actions(preview, None);
            }
            return None;
        }
        self.state
            .complete_bootstrap(frame.identity, frame.embedded_layer, frame.embedded_focus);
        let close = frame.close || frame.os_close;
        let native_identity = frame
            .native_info
            .as_ref()
            .map(|_| identity(frame.instance, frame.identity));
        if !close
            && let (Some(identity), Some(info)) = (native_identity, frame.native_info.as_ref())
        {
            apply_native_utility_geometry_restore(
                ctx,
                egui::ViewportClass::Immediate,
                true,
                &title(frame.instance),
                identity,
                info,
                SIZE,
                Some(MINIMUM),
                None,
            );
        }
        if frame.focus && !close {
            if native_identity.is_some() {
                ctx.send_viewport_cmd_to(viewport(frame.instance), egui::ViewportCommand::Focus);
            } else if let Some(layer) = frame.embedded_layer {
                ctx.move_to_top(layer);
            }
            self.state.complete_focus(frame.identity);
        }

        let mut show_root = false;
        let mut commands = Vec::new();
        if close {
            dialog.open = false;
        } else {
            if let Some(actions) = frame.actions.take() {
                let request = actions.host_request;
                #[cfg(windows)]
                let hwnd = super::utility_dialog_size::utility_picker_hwnd(ctx, native_identity);
                #[cfg(not(windows))]
                let hwnd = None;
                commands = dialog.complete_host_actions(coordinator, actions, hwnd);
                if dialog.open
                    && let Some(popout) = request
                    && popout != self.is_popped_out()
                {
                    if !popout {
                        self.close_viewport(ctx);
                    }
                    self.state.transition(if popout {
                        UtilityPresentation::Popout
                    } else {
                        UtilityPresentation::Docked
                    });
                    self.remember_current(ctx);
                    self.cancel_jump();
                    show_root = !popout;
                    if show_root {
                        ctx.move_to_top(docked_layer());
                    }
                    ctx.request_repaint_of(egui::ViewportId::ROOT);
                }
            }
            if let Some(preview) = frame.preview.take() {
                dialog.preview_dialog.complete_host_actions(preview, None);
            }
        }
        let closed = !dialog.open;
        if closed {
            self.remember_current(ctx);
            self.retire(ctx);
        }
        Some(HostCompletion {
            root_pass: frame.root,
            commands,
            show_root,
            closed,
        })
    }

    #[cfg(test)]
    pub(super) fn test_button(&self) -> (egui::Rect, egui::Rect, bool) {
        self.test_button.expect("File Search host button")
    }

    #[cfg(test)]
    pub(super) fn test_target(&self, label: &str) -> egui::Rect {
        self.native_jump
            .retained_registry()
            .targets()
            .iter()
            .find(|target| target.eligible() && target.metadata().label() == label)
            .unwrap_or_else(|| panic!("File Search native target {label:?} not registered"))
            .clipped_rect()
    }

    #[cfg(test)]
    pub(super) fn test_viewport(&self) -> egui::ViewportId {
        viewport(self.instance)
    }
}

fn surrender_search_focus(ctx: &egui::Context, layer: egui::LayerId) {
    if let Some(focused) = ctx.memory(|memory| memory.focused())
        && ctx
            .read_response(focused)
            .is_some_and(|response| response.layer_id == layer)
    {
        ctx.memory_mut(|memory| memory.surrender_focus(focused));
    }
}

fn draw_preview(
    ctx: &egui::Context,
    dialog: &mut FileSearchDialogState,
    allowed: bool,
) -> Option<FileSearchPreviewHostActions> {
    let layer = egui::LayerId::new(
        egui::Order::Middle,
        egui::Id::new(super::file_search_preview_dialog::preview_window_id_source()),
    );
    let allowed = allowed
        && ctx.input(|input| input.focused)
        && ctx.memory(|memory| memory.top_modal_layer().is_none())
        && !egui::Popup::is_any_open(ctx)
        && visible_top_middle_layer(ctx) == Some(layer)
        && focused_response_layer(ctx)
            .is_none_or(|focus| focus == layer || focus.order != egui::Order::Middle);
    dialog
        .preview_dialog
        .ui_in_host(ctx, &dialog.settings, allowed)
}

fn capture(
    ctx: &egui::Context,
    dialog: &FileSearchDialogState,
    allowed: bool,
    scopes: &(Option<JumpModeScope>, Option<JumpModeScope>),
) -> Option<FileSearchHostCheckpoint> {
    (allowed
        && (scopes
            .0
            .as_ref()
            .is_some_and(JumpModeScope::has_pending_focus_intent)
            || scopes
                .1
                .as_ref()
                .is_some_and(JumpModeScope::has_pending_focus_intent)
            || ctx.input(|input| input.pointer.any_down() || !input.events.is_empty())))
    .then(|| FileSearchHostCheckpoint::capture(dialog))
}
