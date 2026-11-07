use super::{
    DiffDialogState, DiffUiAction, TextInputDraft, TextInputPurpose, effects::PickerOwner,
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
    text_input: Option<TextInputDraft>,
    text: Option<TextUiCheckpoint>,
    text_rules: Option<super::rules_dialog::RulesDialogState>,
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
            text_input: dialog.text_input.clone(),
            text: dialog.text_views.get(&view).map(TextUiCheckpoint::capture),
            text_rules: super::rules_dialog::state(ctx, dialog.workspace.workspace_id, view),
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

    fn restore(self, ctx: &egui::Context, dialog: &mut DiffDialogState) {
        if self.view != dialog.workspace.current_view.id {
            return;
        }
        dialog.workspace.left_visible = self.left;
        dialog.workspace.right_visible = self.right;
        dialog.workspace.focus_left_requested = self.focus_left;
        dialog.text_input = self.text_input;
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
        let rules_id = super::rules_dialog::state_id(dialog.workspace.workspace_id, self.view);
        ctx.data_mut(|data| {
            if let Some(rules) = self.text_rules {
                data.insert_temp(rules_id, rules);
            } else {
                data.remove::<super::rules_dialog::RulesDialogState>(rules_id);
            }
        });
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
    fn rules_layer(&self, ctx: &egui::Context) -> Option<egui::Id> {
        match &self.workspace.current_view.view {
            DiffView::TextCompare(_) => {
                let view = self.workspace.current_view.id;
                super::rules_dialog::state(ctx, self.workspace.workspace_id, view)
                    .filter(|draft| draft.open)
                    .map(|_| super::rules_dialog::window_id(self.workspace.workspace_id, view))
            }
            DiffView::FolderCompare(state) if state.folder_rules_open => {
                Some(super::folder_rules_dialog::window_id())
            }
            _ => None,
        }
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
                || self.text_input.as_ref().is_some_and(|draft| {
                    let id = match draft.purpose {
                        TextInputPurpose::NewComparison => 0,
                        TextInputPurpose::EditInputs { view_id } => view_id,
                    };
                    layer.id == egui::Id::new((workspace, "text_input", id))
                })
                || (self.operation_preview.is_some()
                    && layer.id == egui::Id::new(("folder-operation-preview", view)))
                || self.rules_layer(ctx) == Some(layer.id)
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
            && self.text_input.is_none()
            && self.operation_preview.is_none()
            && self.rules_layer(ctx).is_none()
            && (!hosted
                || (visible_top_middle_layer(ctx).is_none_or(|top| top == layer)
                    && focused_response_layer(ctx)
                        .is_none_or(|focus| focus.order != egui::Order::Middle || focus == layer)))
    }

    pub(crate) fn needs_root_ui_when_hidden(&self) -> bool {
        (self.open && self.host.state.presentation() == UtilityPresentation::Popout)
            || self.host.state.bootstrap_pending()
            || self.host.pending.is_some()
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
    ) {
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
    }

    pub(crate) fn on_host_frame_complete(&mut self, ctx: &egui::Context) -> bool {
        let mut host = std::mem::take(&mut self.host);
        let show_root = host.complete(ctx, self);
        self.host = host;
        show_root
    }

    #[cfg(test)]
    pub(crate) fn test_host_viewport(&self) -> egui::ViewportId {
        viewport(self.workspace.workspace_id)
    }

    #[cfg(test)]
    pub(crate) fn test_host_button(&self) -> (egui::Rect, egui::Rect, bool) {
        self.host_button.expect("Diff host button was rendered")
    }
}

impl DiffWindowHost {
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
            dialog.apply_ui_actions(frame.actions, PickerOwner::Hosted(hwnd));
        }
        if !dialog.open {
            self.close_viewport(ctx, frame.workspace);
            self.state.retire_to_docked();
        } else if let Some(presentation) = presentation
            && presentation != frame.host.host
        {
            self.close_viewport(ctx, frame.workspace);
            self.state.transition(presentation);
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
