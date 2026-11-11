use super::{
    DiffDialogState, DiffUiAction, effects::PickerOwner,
};
use crate::activity::ActivityCenter;
use crate::diff::model::{DiffView, FolderUiCheckpoint, TextUiCheckpoint};
use crate::gui::jump_mode::ui::{focused_response_layer, visible_top_middle_layer};
use crate::gui::utility_dialog_size::{
    apply_native_utility_geometry_restore, close_native_utility_geometry_viewport,
};
use crate::gui::utility_host::{
    HostIdentity, UtilityHostState, UtilityPresentation, ViewportPassProvenance,
};
use crate::gui::utility_window_geometry::NativeViewportIdentity;
use eframe::egui;

const SIZE: egui::Vec2 = egui::vec2(980.0, 660.0);
const MINIMUM: egui::Vec2 = egui::vec2(520.0, 360.0);

#[derive(Default)]
pub(super) struct DiffWindowHost {
    state: UtilityHostState,
    presentation_prepared: bool,
    pending: Option<DiffHostFrame>,
    native_workspace: Option<u64>,
    deferred_close: Option<(u64, HostIdentity)>,
    docked_focus: Option<u64>,
}

struct DiffHostFrame {
    root: ViewportPassProvenance,
    owner: ViewportPassProvenance,
    host: HostIdentity,
    workspace: u64,
    first: bool,
    actions: Vec<DiffUiAction>,
    checkpoint: Option<DiffCheckpoint>,
    close_requested: bool,
    native_info: Option<egui::ViewportInfo>,
    embedded_layer: Option<egui::LayerId>,
    embedded_focus: Option<egui::Id>,
    focus: bool,
}

struct DiffCheckpoint {
    view: u64,
    left: String,
    right: String,
    focus_left: bool,
    text: Option<TextUiCheckpoint>,
    folder: Option<FolderUiCheckpoint>,
    binary_position: Option<(Option<usize>, u64, Option<u64>)>,
}

impl DiffCheckpoint {
    fn capture(dialog: &DiffDialogState, ctx: &egui::Context, input: bool) -> Option<Self> {
        if !input
            || !ctx.input(|input| {
                input.pointer.any_down()
                    || input.events.iter().any(|event| {
                        !matches!(
                            event,
                            egui::Event::PointerMoved(_) | egui::Event::MouseMoved(_)
                        )
                    })
            })
        {
            return None;
        }
        let view = dialog.workspace.current_view.id;
        Some(Self {
            view,
            left: dialog.workspace.left_visible.clone(),
            right: dialog.workspace.right_visible.clone(),
            focus_left: dialog.workspace.focus_left_requested,
            text: dialog.text_views.get(&view).map(TextUiCheckpoint::capture),
            folder: match &dialog.workspace.current_view.view {
                DiffView::FolderCompare(state) => Some(FolderUiCheckpoint::capture(state)),
                _ => None,
            },
            binary_position: dialog.binary_views.get(&view).map(|model| {
                (
                    model.current_difference,
                    model.visible_byte_offset,
                    model.pending_scroll_offset,
                )
            }),
        })
    }

    fn restore(self, _ctx: &egui::Context, dialog: &mut DiffDialogState) {
        if self.view != dialog.workspace.current_view.id {
            return;
        }
        dialog.workspace.left_visible = self.left;
        dialog.workspace.right_visible = self.right;
        dialog.workspace.focus_left_requested = self.focus_left;
        if let Some(text) = self.text
            && let Some(model) = dialog.text_views.get_mut(&self.view)
        {
            text.restore(model);
            model.row_measurements = Default::default();
        }
        if let Some(folder) = self.folder
            && let DiffView::FolderCompare(state) = &mut dialog.workspace.current_view.view
        {
            folder.restore(state);
        }
        if let Some((difference, offset, pending)) = self.binary_position
            && let Some(model) = dialog.binary_views.get_mut(&self.view)
        {
            model.current_difference = difference;
            model.visible_byte_offset = offset;
            model.pending_scroll_offset = pending;
        }
    }
}

fn viewport(workspace: u64) -> egui::ViewportId {
    egui::ViewportId::from_hash_of(("diff", workspace))
}

fn title(workspace: u64) -> String {
    format!("Diff — {workspace}")
}

fn identity(workspace: u64, host: HostIdentity) -> NativeViewportIdentity {
    NativeViewportIdentity {
        key: crate::settings::UtilityToolKey::parse("diff").expect("Diff geometry key"),
        viewport_id: viewport(workspace),
        owner_generation: workspace,
        host_generation: host.generation,
    }
}

pub(super) fn docked_layer(workspace: u64) -> egui::LayerId {
    egui::LayerId::new(
        egui::Order::Middle,
        egui::Id::new(("diff_window", workspace)),
    )
}

impl DiffDialogState {
    pub(crate) fn child_root_layers(&self) -> Vec<egui::LayerId> {
        self.children.root_layers()
    }

    pub(crate) fn host_layer_if_root(&self) -> Option<egui::LayerId> {
        if !self.open {
            None
        } else if self.host.state.presentation() == UtilityPresentation::Docked {
            Some(docked_layer(self.workspace.workspace_id))
        } else {
            self.host.state.embedded_root_layer()
        }
    }

    pub(crate) fn owns_root_input(&self, ctx: &egui::Context) -> bool {
        let Some(parent) = self.host_layer_if_root() else {
            return false;
        };
        let workspace = self.workspace.workspace_id;
        let view = self.workspace.current_view.id;
        let owned = |layer: egui::LayerId| {
            layer == parent
                || (self.close_prompt
                    && layer.id == egui::Id::new(("diff_close_prompt", workspace)))
                || (self.operation_preview.is_some()
                    && layer.id == egui::Id::new(("folder-operation-preview", view)))
        };
        ctx.viewport_id() == egui::ViewportId::ROOT
            && visible_top_middle_layer(ctx).is_some_and(owned)
            && focused_response_layer(ctx)
                .is_none_or(|layer| layer.order != egui::Order::Middle || owned(layer))
    }

    pub(super) fn main_input_allowed(
        &self,
        ctx: &egui::Context,
        layer: egui::LayerId,
        hosted: bool,
    ) -> bool {
        !self.close_prompt
            && self.operation_preview.is_none()
            && (!hosted
                || (visible_top_middle_layer(ctx).is_none_or(|top| top == layer)
                    && focused_response_layer(ctx)
                        .is_none_or(|focus| focus.order != egui::Order::Middle || focus == layer)))
    }

    pub(crate) fn needs_root_ui_when_hidden(&self) -> bool {
        (self.open && self.host.state.presentation() == UtilityPresentation::Popout)
            || self.children.needs_root_ui_when_hidden()
            || self.host.state.bootstrap_pending()
            || self.host.pending.is_some()
    }

    pub(crate) fn prepare_open_presentation(&mut self, ctx: &egui::Context) {
        if self.open && !self.host.presentation_prepared {
            let key = crate::settings::UtilityToolKey::parse("diff").expect("stable Diff key");
            super::super::utility_host::restore_remembered_presentation(
                ctx,
                key,
                &mut self.host.state,
            );
            self.host.presentation_prepared = true;
        }
    }

    pub(crate) fn is_popped_out(&self) -> bool {
        self.host.state.presentation() == UtilityPresentation::Popout
    }

    pub(crate) fn request_host_focus(&mut self) {
        if self.host.state.presentation() == UtilityPresentation::Docked {
            self.host.docked_focus = Some(self.workspace.workspace_id);
        } else {
            self.host.state.request_focus();
        }
    }

    pub(crate) fn show_hosted(
        &mut self,
        ctx: &egui::Context,
        activity: &ActivityCenter,
        first: bool,
        root_visible: bool,
        root_input: bool,
        native_input: bool,
        jump_active: bool,
    ) {
        self.prepare_open_presentation(ctx);
        let root_owned = root_input && self.owns_root_input(ctx);
        let mut host = std::mem::take(&mut self.host);
        host.show(
            ctx,
            self,
            activity,
            first,
            root_visible,
            root_input,
            root_owned,
            native_input,
        );
        self.host = host;
        self.children
            .show(ctx, first, root_visible, root_input, native_input, jump_active);
    }

    pub(crate) fn on_host_frame_complete(&mut self, ctx: &egui::Context) -> bool {
        let mut host = std::mem::take(&mut self.host);
        let mut show_root = host.complete(ctx, self);
        self.host = host;
        let mut children = std::mem::take(&mut self.children);
        let child_root_requests = children.complete(ctx, self);
        for (request, origin) in std::mem::take(&mut self.pending_child_opens) {
            show_root |= children.open(ctx, self, request, &origin)
                == Some(UtilityPresentation::Docked);
        }
        if !self.open {
            children.retire(ctx);
        }
        show_root |= children.has_requested_root(&child_root_requests);
        self.children = children;
        show_root
    }

    #[cfg(test)]
    pub(crate) fn test_host_viewport(&self) -> egui::ViewportId {
        viewport(self.workspace.workspace_id)
    }

    #[cfg(test)]
    pub(crate) fn test_compare_text_button(&self) -> Option<(egui::Rect, egui::Rect, bool)> {
        self.compare_text_button
    }

    #[cfg(test)]
    pub(crate) fn test_reveal_header_control(&self, ctx: &egui::Context, target: egui::Rect) {
        let (id, inner, content, offset) = self.header_scroll.expect("Diff header scroll area");
        let mut state = egui::scroll_area::State::load(ctx, id).expect("Diff header scroll state");
        state.offset.x = (offset.x + target.max.x - inner.max.x + 4.0).clamp(0.0, (content.x - inner.width()).max(0.0));
        state.store(ctx, id);
    }

    #[cfg(test)]
    pub(crate) fn test_input_child(&self) -> Option<(u64, bool, &str)> {
        self.children.test_input()
    }

    #[cfg(test)]
    pub(crate) fn test_input_viewport(&self) -> Option<egui::ViewportId> {
        self.children.test_input_viewport()
    }

    #[cfg(test)]
    pub(crate) fn test_host_button(&self) -> (egui::Rect, egui::Rect, bool) {
        self.host_button.expect("Diff host button was rendered")
    }
}

impl DiffWindowHost {
    #[cfg(test)]
    pub(super) fn test_identity(&self) -> HostIdentity {
        self.state.identity()
    }
    #[cfg(test)]
    pub(super) fn test_transition(&mut self, presentation: UtilityPresentation) {
        self.state.transition(presentation);
    }

    pub(super) fn accepts_child_origin(&self, origin: &crate::gui::utility_host::AcceptedParentOpen<(u64, u64)>, workspace: u64, view: u64) -> bool {
        origin.is_current(self.state.identity(), &(workspace, view))
    }

    fn show(
        &mut self,
        ctx: &egui::Context,
        dialog: &mut DiffDialogState,
        activity: &ActivityCenter,
        first: bool,
        root_visible: bool,
        root_input: bool,
        root_owned: bool,
        native_input: bool,
    ) {
        let root = ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT);
        let host = self.state.identity();
        let workspace = dialog.workspace.workspace_id;
        let mut close_requested = self.deferred_close.take() == Some((workspace, host));
        if let Some(previous) = self.pending.take()
            && previous.root.frame_nr == root.frame_nr
            && previous.host == host
            && previous.workspace == workspace
        {
            if let Some(checkpoint) = previous.checkpoint {
                checkpoint.restore(ctx, dialog);
            }
            close_requested |= previous.close_requested;
        }
        if let Some(previous) = self.native_workspace
            && (previous != workspace || !dialog.open)
        {
            self.close_viewport(ctx, previous);
            if dialog.open {
                self.state.request_focus();
            } else {
                self.state.retire_to_docked();
                self.presentation_prepared = false;
            }
        }
        if !dialog.open || (!root_visible && host.host == UtilityPresentation::Docked) {
            return;
        }
        if first {
            dialog.advance_view_runtime(activity);
        }
        let first = first && !ctx.will_discard();
        let mut frame = DiffHostFrame {
            root,
            owner: root,
            host,
            workspace,
            first,
            actions: Vec::new(),
            checkpoint: None,
            close_requested,
            native_info: None,
            embedded_layer: None,
            embedded_focus: None,
            focus: false,
        };
        if host.host == UtilityPresentation::Docked {
            frame.focus = root_input && self.docked_focus == Some(workspace);
            frame.checkpoint = DiffCheckpoint::capture(dialog, ctx, first && root_owned);
            frame.actions = dialog.render_docked(ctx, Some(host.host), first && root_owned);
            if !root_owned {
                frame
                    .actions
                    .retain(|action| matches!(action, DiffUiAction::DockedPosition(_)));
            }
            if first
                && root_owned
                && dialog.main_input_allowed(ctx, docked_layer(workspace), true)
                && consume_close(ctx)
            {
                frame.actions.push(DiffUiAction::RequestClose);
            }
        } else {
            self.native_workspace = Some(workspace);
            let viewport_id = viewport(workspace);
            let builder = egui::ViewportBuilder::default()
                .with_title(title(workspace))
                .with_inner_size(SIZE)
                .with_min_inner_size(MINIMUM)
                .with_resizable(true);
            ctx.show_viewport_immediate(viewport_id, builder, |child, class| {
                let child_ctx = child.ctx().clone();
                let native = class == egui::ViewportClass::Immediate
                    && child_ctx.viewport_id() == viewport_id;
                let child_first = child.current_pass_index() == 0;
                if native
                    && !child_first
                    && let Some(checkpoint) = frame.checkpoint.take()
                {
                    checkpoint.restore(&child_ctx, dialog);
                }
                frame.owner = ViewportPassProvenance::current(&child_ctx, child_ctx.viewport_id());
                frame.first = first && child_first && !child_ctx.will_discard();
                frame.actions.clear();
                let child_owned = if native {
                    let info = child.input(|input| input.viewport().clone());
                    frame.close_requested |= info.close_requested();
                    frame.native_info = Some(info);
                    frame.focus = self.state.focus_pending() && native_input;
                    native_input
                        && child_ctx.input(|input| input.focused)
                        && child_ctx.memory(|memory| memory.top_modal_layer().is_none())
                } else {
                    frame.embedded_layer = Some(child.layer_id());
                    frame.embedded_focus = child_ctx.memory(|memory| memory.focused());
                    frame.focus = self.state.focus_pending() && root_visible;
                    root_owned && child.is_visible() && !child.is_sizing_pass()
                };
                let allow = frame.first && child_owned && !frame.close_requested;
                frame.checkpoint = DiffCheckpoint::capture(dialog, &child_ctx, allow);
                if native {
                    egui::CentralPanel::default().show(child, |ui| {
                        dialog.render_host_body(
                            &child_ctx,
                            ui,
                            &mut frame.actions,
                            Some(host.host),
                            allow,
                        );
                    });
                } else {
                    dialog.render_host_body(
                        &child_ctx,
                        child,
                        &mut frame.actions,
                        Some(host.host),
                        allow,
                    );
                }
                dialog.render_diff_children(
                    &child_ctx,
                    &mut frame.actions,
                    allow,
                    true,
                    child.layer_id(),
                );
                if !allow {
                    frame.actions.clear();
                } else if dialog.main_input_allowed(&child_ctx, child.layer_id(), true)
                    && consume_close(&child_ctx)
                {
                    frame.actions.push(DiffUiAction::RequestClose);
                }
            });
        }
        self.pending = Some(frame);
    }

    fn complete(&mut self, ctx: &egui::Context, dialog: &mut DiffDialogState) -> bool {
        let Some(mut frame) = self.pending.take() else {
            return false;
        };
        let current = dialog.open
            && frame.workspace == dialog.workspace.workspace_id
            && frame.host == self.state.identity();
        if frame.close_requested {
            ctx.send_viewport_cmd_to(
                viewport(frame.workspace),
                egui::ViewportCommand::CancelClose,
            );
        }
        if !current
            || !frame.first
            || !frame.root.was_retained(ctx)
            || !frame.owner.was_retained(ctx)
        {
            if current {
                if frame.close_requested {
                    self.deferred_close = Some((frame.workspace, frame.host));
                }
                if let Some(checkpoint) = frame.checkpoint.take() {
                    checkpoint.restore(ctx, dialog);
                }
                ctx.request_repaint_of(frame.owner.viewport_id);
            }
            return false;
        }
        self.state
            .complete_bootstrap(frame.host, frame.embedded_layer, frame.embedded_focus);
        let native_identity = frame
            .native_info
            .as_ref()
            .map(|_| identity(frame.workspace, frame.host));
        if let (Some(identity), Some(info)) = (native_identity, frame.native_info.as_ref()) {
            apply_native_utility_geometry_restore(
                ctx,
                egui::ViewportClass::Immediate,
                true,
                &title(frame.workspace),
                identity,
                info,
                SIZE,
                Some(MINIMUM),
                None,
            );
        }
        if frame.focus && !frame.close_requested {
            if native_identity.is_some() {
                ctx.send_viewport_cmd_to(viewport(frame.workspace), egui::ViewportCommand::Focus);
            } else if let Some(layer) = frame.embedded_layer {
                ctx.move_to_top(layer);
            } else if frame.host.host == UtilityPresentation::Docked {
                ctx.move_to_top(docked_layer(frame.workspace));
                self.docked_focus = None;
            }
            self.state.complete_focus(frame.host);
        }
        let presentation = frame.actions.iter().find_map(|action| match action {
            DiffUiAction::MoveHost(presentation) => Some(*presentation),
            _ => None,
        });
        if frame.close_requested {
            dialog.request_host_close();
        } else {
            #[cfg(windows)]
            let hwnd = crate::gui::utility_dialog_size::utility_picker_hwnd(ctx, native_identity);
            #[cfg(not(windows))]
            let hwnd = None;
            let view = dialog.workspace.current_view.id;
            let origin = crate::gui::utility_host::AcceptedParentOpen::from_retained(
                ctx,
                frame.host,
                (frame.workspace, view),
                frame.root,
                frame.owner,
            );
            let mut ordinary = Vec::new();
            for action in frame.actions {
                match action {
                    DiffUiAction::BeginTextInput(purpose) => {
                        if let (Some(origin), Some(request)) =
                            (&origin, super::child_hosts::Open::input(dialog, purpose))
                        {
                            dialog.pending_child_opens.push((request, origin.clone()));
                        }
                    }
                    DiffUiAction::Folder(view, super::folder_view::FolderViewAction::OpenRules) => {
                        if let (Some(origin), Some(request)) =
                            (&origin, super::child_hosts::Open::folder(dialog, view))
                        {
                            dialog.pending_child_opens.push((request, origin.clone()));
                        }
                    }
                    DiffUiAction::ViewEffects(view, effects) => {
                        let mut remaining = Vec::new();
                        for effect in effects {
                            if matches!(effect, super::effects::ViewEffect::OpenTextRules) {
                                if let (Some(origin), Some(request)) =
                                    (&origin, super::child_hosts::Open::rules(dialog, view))
                                {
                                    dialog.pending_child_opens.push((request, origin.clone()));
                                }
                            } else {
                                remaining.push(effect);
                            }
                        }
                        if !remaining.is_empty() {
                            ordinary.push(DiffUiAction::ViewEffects(view, remaining));
                        }
                    }
                    other => ordinary.push(other),
                }
            }
            dialog.apply_ui_actions(ordinary, PickerOwner::Hosted(hwnd));
        }
        if !dialog.open {
            let key = crate::settings::UtilityToolKey::parse("diff").expect("stable Diff key");
            super::super::utility_host::remember_presentation(ctx, key, frame.host.host);
            self.close_viewport(ctx, frame.workspace);
            self.state.retire_to_docked();
            self.presentation_prepared = false;
        } else if let Some(presentation) = presentation
            && presentation != frame.host.host
        {
            self.close_viewport(ctx, frame.workspace);
            self.state.transition(presentation);
            let key = crate::settings::UtilityToolKey::parse("diff").expect("stable Diff key");
            super::super::utility_host::remember_presentation(ctx, key, presentation);
            ctx.request_repaint_of(egui::ViewportId::ROOT);
            if presentation == UtilityPresentation::Docked {
                ctx.move_to_top(docked_layer(frame.workspace));
                return true;
            }
        }
        false
    }

    fn close_viewport(&mut self, ctx: &egui::Context, workspace: u64) {
        self.native_workspace = None;
        let host = self.state.identity();
        if host.host == UtilityPresentation::Popout {
            let native = identity(workspace, host);
            close_native_utility_geometry_viewport(ctx, native);
            ctx.send_viewport_cmd_to(native.viewport_id, egui::ViewportCommand::Close);
        }
    }
}

fn consume_close(ctx: &egui::Context) -> bool {
    !egui::Popup::is_any_open(ctx)
        && ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::W))
}

pub(super) fn child_input_allowed(
    ctx: &egui::Context,
    id: egui::Id,
    parent: egui::LayerId,
) -> bool {
    let layer = egui::LayerId::new(egui::Order::Middle, id);
    visible_top_middle_layer(ctx) == Some(layer)
        && focused_response_layer(ctx).is_none_or(|focus| {
            focus == layer || focus == parent || focus.order != egui::Order::Middle
        })
}
