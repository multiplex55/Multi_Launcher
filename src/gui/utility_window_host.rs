use eframe::egui;

use super::batch_rename_dialog::{BatchRenameHostActions, BatchRenameInteractionCheckpoint};
use super::clipboard_dialog::{
    ClipboardDialogEffects, ClipboardHostActions, ClipboardInteractionCheckpoint,
};
use super::jump_mode::ui::{
    JumpModeScope, JumpModeUi, NavigationDialogKind, NavigationOwner, NavigationViewportBinding,
    batch_rename_confirmation_layer_id,
};
use super::jump_mode::{SurfaceId, TargetGeneration};
use super::json_utility_dialog::{
    JsonUtilityHostActions, JsonUtilityInteractionCheckpoint,
};
use super::paste_sequence_dialog::{
    PasteSequenceDialog, PasteSequenceHostActions, PasteSequenceHostRuntime,
    PasteSequenceInteractionCheckpoint,
};
use super::search_replace_dialog::{SearchReplaceHostActions, SearchReplaceInteractionCheckpoint};
use super::snippet_dialog::{SnippetHostActions, SnippetHostEffects, SnippetInteractionCheckpoint};
use super::clipboard_modify_dialog::{
    ClipboardModifyHostActions, ClipboardModifyInteractionCheckpoint,
};
use super::utility_dialog_size::{
    apply_native_utility_geometry_restore, close_native_utility_geometry_viewport,
};
use super::utility_host::{
    HostIdentity, UtilityHostState, UtilityPresentation, ViewportPassProvenance,
};
use super::utility_window_geometry::NativeViewportIdentity;
use super::{
    BatchRenameDialogState, BatchRenameUiCommand, ClipboardDialog, ClipboardModifyDialogState, JsonUtilityDialogState, SearchReplaceDialogState,
    SearchReplaceUiCommand, SnippetDialog,
};
use crate::file_operations::RenameRuntimeSnapshot;

pub(super) trait HostedUtilityDialog: Sized {
    const KIND: Option<NavigationDialogKind> = None;
    const KEY: &'static str;
    const SURFACE_KIND: &'static str = Self::KEY;
    const TITLE: &'static str;
    const SIZE: egui::Vec2;
    const MINIMUM: egui::Vec2;
    const SCROLL_NATIVE_WINDOW: bool;
    const RESIZABLE: bool = true;
    const REGISTER_TARGETS_ON_REPLAY: bool = false;
    type Runtime;
    type Actions;
    type Checkpoint;
    type Command;

    fn layer() -> egui::LayerId {
        Self::KIND.and_then(NavigationDialogKind::layer_id).expect("utility window layer")
    }

    fn dialog_layer(&self) -> egui::LayerId { Self::layer() }

    fn navigation_owner(instance: u64) -> NavigationOwner {
        NavigationOwner::Dialog {
            kind: Self::KIND.expect("utility window navigation kind"),
            instance,
        }
    }

    fn dialog_navigation_owner(&self, instance: u64) -> NavigationOwner {
        Self::navigation_owner(instance)
    }

    fn dialog_surface(&self, instance: u64) -> SurfaceId {
        SurfaceId::dialog(Self::SURFACE_KIND, instance)
    }

    fn dialog_namespace(&self) -> String {
        format!("{}/form", Self::KEY)
    }

    fn is_open(&self) -> bool;
    fn session(&self) -> u64;
    fn target_generation(&self) -> TargetGeneration {
        TargetGeneration(self.session())
    }
    fn parent_targets_allowed(&self, runtime: &Self::Runtime) -> bool;
    fn parent_input_allowed(&self) -> bool;
    fn native_parent_current(
        &self,
        binding: NavigationViewportBinding,
        ctx: &egui::Context,
        popup: Option<egui::LayerId>,
    ) -> bool {
        binding.base_is_current(ctx) && binding.focus_is_owned(ctx, popup)
    }
    fn confirmation_can_close() -> bool {
        false
    }
    fn popup_layer(&self, ctx: &egui::Context, parent: egui::LayerId) -> Option<egui::LayerId>;
    fn confirmation_binding(
        &self,
        viewport: egui::ViewportId,
        instance: u64,
    ) -> Option<NavigationViewportBinding>;
    fn confirmation_scope(
        &self,
        jump: &JumpModeUi,
        owner: NavigationOwner,
        binding: NavigationViewportBinding,
        parent: SurfaceId,
    ) -> Option<JumpModeScope> {
        file_confirmation_scope(jump, owner, binding, parent)
    }
    fn capture(&self) -> Self::Checkpoint;
    fn restore(&mut self, checkpoint: Self::Checkpoint);
    fn render_host(
        &mut self,
        ctx: &egui::Context,
        scopes: (Option<&JumpModeScope>, Option<&JumpModeScope>),
        owner: NavigationOwner,
        parent_input: bool,
        child_owner: NavigationOwner,
        first: bool,
        instance: u64,
        runtime: &Self::Runtime,
        child_ui: Option<&mut egui::Ui>,
        presentation: UtilityPresentation,
    ) -> Self::Actions;
    fn host_request(actions: &Self::Actions) -> Option<UtilityPresentation>;
    fn closed_command() -> Self::Command;
    fn command_closes(command: &Self::Command) -> bool;
    fn complete_actions(
        &mut self,
        actions: Self::Actions,
        hwnd: Option<usize>,
    ) -> Vec<Self::Command>;
    #[cfg(test)]
    fn test_button(actions: &Self::Actions) -> Option<(egui::Rect, egui::Rect, bool)>;
}

pub(super) type BatchRenameWindowHost = UtilityWindowHost<BatchRenameDialogState>;
pub(super) type SearchReplaceWindowHost = UtilityWindowHost<SearchReplaceDialogState>;
pub(super) type ClipboardHistoryWindowHost = UtilityWindowHost<ClipboardDialog>;
pub(super) type ClipboardModifyWindowHost = UtilityWindowHost<ClipboardModifyDialogState>;
pub(super) type PasteSequenceWindowHost = UtilityWindowHost<PasteSequenceDialog>;
pub(super) type JsonUtilityWindowHost = UtilityWindowHost<JsonUtilityDialogState>;

pub(super) enum JsonUtilityHostCommand {
    Closed,
    Apply(JsonUtilityHostActions),
}

pub(super) struct HostedPasteSequenceRuntime {
    pub dialog: PasteSequenceHostRuntime,
    pub pinned: bool,
}

pub(super) enum PasteSequenceHostCommand {
    Closed,
    Apply(PasteSequenceHostActions),
}

#[cfg(test)]
pub(super) type ClipboardModifyTestService = std::sync::Arc<
    crate::clipboard_modify::clipboard::ClipboardService<
        crate::clipboard_modify::clipboard::fake::FakeClipboardBackend,
    >,
>;

pub(super) struct ClipboardModifyHostRuntime {
    pub legacy_window_size: egui::Vec2,
    pub catalog: std::sync::Arc<crate::clipboard_modify::model::ClipboardModifierCatalog>,
    pub store: crate::clipboard_modify::store::ClipboardModifierStore,
    pub pinned: bool,
    #[cfg(test)]
    pub test_service: Option<ClipboardModifyTestService>,
}

pub(super) enum ClipboardModifyHostCommand {
    Closed,
    Apply(ClipboardModifyHostActions),
}

pub(super) enum ClipboardHostCommand {
    Closed,
    Apply(ClipboardDialogEffects),
}

pub(super) type SnippetWindowHost = UtilityWindowHost<SnippetDialog>;

pub(super) enum SnippetHostCommand {
    Closed,
    Apply(SnippetHostEffects),
}

pub(super) struct UtilityWindowHost<D: HostedUtilityDialog> {
    state: UtilityHostState,
    dialog_layer: Option<egui::LayerId>,
    presentation_session: Option<u64>,
    native_session: Option<u64>,
    pending: Option<HostFrame<D>>,
    deferred_open: Option<(u64, UtilityPresentation, bool)>,
    deferred_close: Option<(u64, HostIdentity)>,
    docked_focus_pending: bool,
    native_jump: JumpModeUi,
    native_jump_frame: Option<u64>,
    #[cfg(test)]
    pub(super) test_button: Option<(egui::Rect, egui::Rect, bool)>,
}

impl<D: HostedUtilityDialog> Default for UtilityWindowHost<D> {
    fn default() -> Self {
        Self {
            state: UtilityHostState::default(),
            dialog_layer: None,
            presentation_session: None,
            native_session: None,
            pending: None,
            deferred_open: None,
            deferred_close: None,
            docked_focus_pending: false,
            native_jump: JumpModeUi::default(),
            native_jump_frame: None,
            #[cfg(test)]
            test_button: None,
        }
    }
}

struct HostFrame<D: HostedUtilityDialog> {
    root: ViewportPassProvenance,
    owner: ViewportPassProvenance,
    host: HostIdentity,
    session: u64,
    first: bool,
    os_close: bool,
    close: bool,
    actions: Option<D::Actions>,
    checkpoint: Option<D::Checkpoint>,
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
    pub allow_os_close: bool,
    pub confirmation_owner: NavigationOwner,
    pub parent_instance: u64,
    pub root_scopes: (Option<&'a JumpModeScope>, Option<&'a JumpModeScope>),
}

pub(super) struct HostCompletion<C> {
    pub commands: Vec<C>,
    pub show_root: bool,
    pub owner_viewport: egui::ViewportId,
    pub origin: Option<super::utility_host::AcceptedParentOpen<u64>>,
}

pub(super) struct CompanionPass {
    pub root: ViewportPassProvenance,
    pub owner: ViewportPassProvenance,
    pub session: u64,
    pub first: bool,
    pub input_owned: bool,
    pub initial_focus: bool,
}

pub(super) trait UtilityWindowCompanion {
    fn binding(&self, viewport: egui::ViewportId) -> Option<NavigationViewportBinding>;
    fn permits_jump_mode(&self) -> bool {
        true
    }
    fn scope(&self, jump: &JumpModeUi, binding: NavigationViewportBinding)
    -> Option<JumpModeScope>;
    fn render(&mut self, ctx: &egui::Context, scope: Option<JumpModeScope>, pass: CompanionPass);
}

impl<D: HostedUtilityDialog> UtilityWindowHost<D> {
    #[cfg(test)]
    pub(super) fn test_target_rect(&self, label: &str) -> egui::Rect {
        self.native_jump.retained_registry().targets().iter()
            .find(|target| target.eligible() && target.metadata().label() == label)
            .unwrap_or_else(|| {
                let labels: Vec<_> = self.native_jump.retained_registry().targets().iter()
                    .map(|target| (target.metadata().label(), target.eligible())).collect();
                panic!("missing hosted target {label:?}, owner {:?}, targets {labels:?}", self.native_jump.frame_navigation_owner())
            })
            .clipped_rect()
    }

    #[cfg(test)]
    pub(super) fn test_native_owner(&self) -> NavigationOwner {
        self.native_jump.frame_navigation_owner()
    }
    fn docked_layer(&self) -> egui::LayerId {
        self.dialog_layer.unwrap_or_else(D::layer)
    }

    fn viewport(session: u64) -> egui::ViewportId {
        egui::ViewportId::from_hash_of((D::KEY, session))
    }

    fn title(session: u64) -> String {
        format!("{} — {session}", D::TITLE)
    }

    fn geometry_identity(session: u64, host: HostIdentity) -> NativeViewportIdentity {
        NativeViewportIdentity {
            key: crate::settings::UtilityToolKey::parse(D::KEY)
                .expect("utility window geometry key"),
            viewport_id: Self::viewport(session),
            owner_generation: session,
            host_generation: host.generation,
        }
    }

    #[cfg(test)]
    pub(super) fn test_viewport(session: u64) -> egui::ViewportId {
        Self::viewport(session)
    }

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
            Some(self.docked_layer())
        } else {
            self.state.embedded_root_layer()
        }
    }

    pub(super) fn is_popped_out(&self) -> bool {
        self.state.presentation() == UtilityPresentation::Popout
    }
    pub(super) fn identity(&self) -> HostIdentity {
        self.state.identity()
    }

    pub(super) fn accepts_origin(&self, dialog: &D, origin: &super::utility_host::AcceptedParentOpen<u64>) -> bool {
        dialog.is_open() && origin.is_current(self.state.identity(), &dialog.session())
    }

    pub(super) fn owner_viewport(&self) -> egui::ViewportId {
        if self.root_layer().is_none()
            && let Some(session) = self.native_session
        {
            return Self::viewport(session);
        }
        egui::ViewportId::ROOT
    }

    pub(super) fn needs_root_ui_when_hidden(&self, open: bool) -> bool {
        (open && (self.state.presentation() == UtilityPresentation::Popout
            || self.presentation_session.is_none()))
            || self.state.bootstrap_pending()
            || self.pending.is_some()
    }

    pub(super) fn request_focus(&mut self) {
        if self.state.presentation() == UtilityPresentation::Docked {
            self.docked_focus_pending = true;
        } else {
            self.state.request_focus();
        }
    }

    pub(super) fn cancel_docked_focus_request(&mut self) {
        self.docked_focus_pending = false;
    }

    pub(super) fn has_pending_frame(&self) -> bool {
        self.pending.is_some()
    }

    pub(super) fn has_deferred_close(&self) -> bool {
        self.deferred_close.is_some()
    }

    pub(super) fn configure_jump(&mut self, root: &JumpModeUi, enabled: bool) {
        let (chord, alphabet) = root.configuration();
        self.native_jump.set_configuration(chord, alphabet);
        self.native_jump.set_enabled(enabled);
    }

    pub(super) fn close_from_app(&mut self, ctx: &egui::Context) {
        self.remember_presentation(ctx, self.state.presentation());
        self.retire(ctx);
    }

    pub(super) fn retire(&mut self, ctx: &egui::Context) {
        self.close_viewport(ctx);
        self.pending = None;
        self.deferred_open = None;
        self.deferred_close = None;
        self.docked_focus_pending = false;
        self.presentation_session = None;
        self.state.retire_to_docked();
        self.cancel_jump();
    }

    pub(super) fn suspend(&mut self, ctx: &egui::Context) {
        self.close_viewport(ctx);
        self.pending = None;
        self.deferred_open = None;
        self.deferred_close = None;
        self.docked_focus_pending = false;
        let identity = self.state.identity();
        self.state.complete_bootstrap(identity, None, None);
        self.state.complete_focus(identity);
        self.cancel_jump();
    }

    fn close_viewport(&mut self, ctx: &egui::Context) {
        if let Some(session) = self.native_session.take() {
            let identity = Self::geometry_identity(session, self.state.identity());
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
        dialog: &mut D,
        runtime: &D::Runtime,
        input: HostInput<'_>,
    ) {
        self.show_with_companion(ctx, dialog, runtime, input, None);
    }

    pub(super) fn prepare_open(&mut self, ctx: &egui::Context, dialog: &D) {
        if !dialog.is_open() {
            return;
        }
        let session = dialog.session();
        self.dialog_layer = Some(dialog.dialog_layer());
        if self.presentation_session != Some(session) {
            self.presentation_session = Some(session);
            let remembered =
                super::utility_presentation::UtilityPresentationManager::for_context(ctx)
                    .and_then(|manager| {
                        crate::settings::UtilityToolKey::parse(D::KEY)
                            .ok()
                            .and_then(|key| manager.preference(key))
                    })
                    .map(Into::into);
            let presentation = super::utility_host::resolve_child_presentation(
                None,
                remembered,
                self.state.presentation(),
                &super::utility_host::ChildOpenOrigin::<()>::Root,
            );
            self.state.transition(presentation);
        }
    }

    /// Apply an accepted open without replacing the dialog or losing a frame
    /// that was already staged for its current host.
    pub(super) fn open_from_origin<S>(
        &mut self,
        ctx: &egui::Context,
        dialog: &D,
        was_open: bool,
        origin: &super::utility_host::ChildOpenOrigin<S>,
    ) -> Option<UtilityPresentation> {
        self.prepare_open(ctx, dialog);
        let remembered = super::utility_presentation::UtilityPresentationManager::for_context(ctx)
            .and_then(|manager| {
                crate::settings::UtilityToolKey::parse(D::KEY)
                    .ok()
                    .and_then(|key| manager.preference(key))
            })
            .map(Into::into);
        let desired = super::utility_host::resolve_child_presentation(
            was_open.then(|| self.state.presentation()),
            remembered,
            self.state.presentation(),
            origin,
        );
        let remember = matches!(origin, super::utility_host::ChildOpenOrigin::Parent(_))
            && desired == UtilityPresentation::Popout;
        if self.pending.is_some() {
            self.deferred_open = Some((dialog.session(), desired, remember));
            ctx.request_repaint_of(egui::ViewportId::ROOT);
            None
        } else {
            self.apply_open_presentation(ctx, desired, remember);
            Some(desired)
        }
    }

    fn apply_open_presentation(
        &mut self,
        ctx: &egui::Context,
        desired: UtilityPresentation,
        remember: bool,
    ) {
        if self.state.presentation() != desired {
            self.close_viewport(ctx);
            self.state.transition(desired);
            if desired == UtilityPresentation::Popout {
                self.docked_focus_pending = false;
            }
            self.deferred_close = None;
            self.cancel_jump();
        }
        if remember {
            self.remember_presentation(ctx, desired);
        }
        self.request_focus();
        ctx.request_repaint_of(egui::ViewportId::ROOT);
    }

    fn apply_deferred_open(
        &mut self,
        ctx: &egui::Context,
        dialog: &D,
    ) -> Option<UtilityPresentation> {
        if let Some((session, desired, remember)) = self.deferred_open.take()
            && dialog.is_open()
            && dialog.session() == session
        {
            let desired = super::utility_host::resolve_child_presentation(
                Some(self.state.presentation()),
                Some(desired),
                desired,
                &super::utility_host::ChildOpenOrigin::<()>::Root,
            );
            self.apply_open_presentation(ctx, desired, remember);
            return Some(desired);
        }
        None
    }

    pub(super) fn show_with_companion(
        &mut self,
        ctx: &egui::Context,
        dialog: &mut D,
        runtime: &D::Runtime,
        input: HostInput<'_>,
        mut companion: Option<(&mut dyn UtilityWindowCompanion, Option<JumpModeScope>)>,
    ) {
        if !dialog.is_open() {
            if self.native_session.is_some() {
                self.retire(ctx);
            }
            self.presentation_session = None;
            return;
        }
        self.prepare_open(ctx, dialog);
        let session = dialog.session();
        let root = ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT);
        let host = self.state.identity();
        let mut os_close = self.deferred_close.take() == Some((session, host));
        if let Some(previous) = self.pending.take()
            && previous.root.frame_nr == root.frame_nr
            && previous.host == host
            && previous.session == session
        {
            if let Some(checkpoint) = previous.checkpoint {
                dialog.restore(checkpoint);
            }
            os_close |= previous.os_close;
        }
        os_close &= input.allow_os_close;
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
            && (input.parent_owner
                || (D::confirmation_can_close()
                    && input.confirmation_owner != NavigationOwner::Blocked))
            && !input.root_jump_active
            && !egui::Popup::is_any_open(ctx)
            && consume_close(ctx);
        if !popped_out {
            frame.close = root_close;
            frame.focus = input.root_input && self.docked_focus_pending;
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
            if let Some((companion, scope)) = &mut companion {
                draw_root_companion(
                    ctx,
                    *companion,
                    scope.clone(),
                    &frame,
                    self.docked_layer(),
                    input.root_input,
                );
            }
        } else {
            if self.native_session != Some(session) {
                self.close_viewport(ctx);
                self.native_session = Some(session);
                self.state.request_focus();
            }
            let id = Self::viewport(session);
            let builder = egui::ViewportBuilder::default()
                .with_title(Self::title(session))
                .with_inner_size(D::SIZE)
                .with_min_inner_size(D::MINIMUM)
                .with_resizable(D::RESIZABLE);
            ctx.show_viewport_immediate(id, builder, |child, class| {
                let child_ctx = child.ctx().clone();
                let native =
                    class == egui::ViewportClass::Immediate && child_ctx.viewport_id() == id;
                let child_first = child.current_pass_index() == 0;
                if !child_first && let Some(checkpoint) = frame.checkpoint.take() {
                    dialog.restore(checkpoint);
                }
                frame.owner = ViewportPassProvenance::current(&child_ctx, child_ctx.viewport_id());
                frame.first = first && child_first && !child_ctx.will_discard();
                frame.actions = None;
                if native {
                    let info = child.input(|input| input.viewport().clone());
                    if info.close_requested() {
                        if input.allow_os_close {
                            frame.os_close = true;
                        } else {
                            child_ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                        }
                    }
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
                        &mut companion,
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
                    if let Some((companion, scope)) = &mut companion {
                        draw_root_companion(
                            &child_ctx,
                            *companion,
                            scope.clone(),
                            &frame,
                            child.layer_id(),
                            input.root_input,
                        );
                    }
                }
            });
        }
        #[cfg(test)]
        {
            self.test_button = frame.actions.as_ref().and_then(D::test_button);
        }
        self.pending = Some(frame);
    }

    fn draw_native(
        &mut self,
        ctx: &egui::Context,
        dialog: &mut D,
        runtime: &D::Runtime,
        frame: &mut HostFrame<D>,
        viewport: egui::ViewportId,
        input: bool,
        close_shortcut: bool,
        companion: &mut Option<(&mut dyn UtilityWindowCompanion, Option<JumpModeScope>)>,
    ) {
        let instance = frame.session;
        let layer = self.docked_layer();
        let parent = NavigationViewportBinding {
            viewport,
            base_layer: layer,
            owner: dialog.dialog_navigation_owner(instance),
        };
        let dialog_child = dialog.confirmation_binding(viewport, instance);
        let companion_binding = companion
            .as_ref()
            .and_then(|(child, _)| child.binding(viewport));
        let child = companion_binding.or(dialog_child);
        let binding = child.unwrap_or(parent);
        let native_frame = ctx.cumulative_frame_nr();
        if self.native_jump_frame != Some(native_frame) {
            self.native_jump.begin_frame();
            self.native_jump_frame = Some(native_frame);
        }
        let owner = self
            .native_jump
            .update_navigation_owner_for_viewport(ctx, binding, input);
        let companion_permits_jump = companion.as_ref()
            .is_none_or(|(child, _)| child.permits_jump_mode());
        let jump_current = self.native_jump.is_enabled()
            && owner == binding.owner && companion_permits_jump;
        let popup_layer = dialog.popup_layer(ctx, layer);
        let parent_current = input
            && child.is_none()
            && dialog.parent_input_allowed()
            && dialog.native_parent_current(parent, ctx, popup_layer)
            && (!egui::Popup::is_any_open(ctx) || popup_layer.is_some());
        let confirmation_owner = if input
            && companion_binding.is_none()
            && dialog_child.is_some()
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
        let surface = dialog.dialog_surface(instance);
        let parent_scope =
            (jump_current && child.is_none() && dialog.parent_targets_allowed(runtime))
                .then(|| {
                    self.native_jump.scope_for_viewport_binding(
                        surface.clone(),
                        dialog.target_generation(),
                        dialog.dialog_namespace(),
                        binding,
                    )
                })
                .flatten();
        let child_scope = if jump_current {
            dialog.confirmation_scope(&self.native_jump, owner, binding, surface)
        } else {
            None
        };
        if frame.first
            && close_shortcut
            && (parent_current
                || (D::confirmation_can_close() && confirmation_owner != NavigationOwner::Blocked))
            && !frame.os_close
            && self.native_jump.active_session().is_none()
            && !egui::Popup::is_any_open(ctx)
            && consume_close(ctx)
        {
            frame.close = true;
        }
        egui::Window::new(D::TITLE)
            .id(layer.id)
            .title_bar(false)
            .fixed_rect(ctx.content_rect())
            .movable(false)
            .resizable(false)
            .interactable(companion_binding.is_none())
            .vscroll(D::SCROLL_NATIVE_WINDOW)
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
        if let Some((companion, _)) = companion.as_mut()
            && let Some(binding) = companion_binding
        {
            let owns = input
                && binding.base_is_current(ctx)
                && (binding.focus_is_owned(ctx, None)
                    || super::jump_mode::ui::focused_response_layer(ctx) == Some(layer))
                && !egui::Popup::is_any_open(ctx);
            let scope = jump_current
                .then(|| companion.scope(&self.native_jump, binding))
                .flatten();
            let initial_focus = input
                && !owns
                && !egui::Popup::is_any_open(ctx)
                && super::jump_mode::ui::visible_top_middle_layer(ctx)
                    .is_none_or(|top| top == layer || top == binding.base_layer);
            companion.render(
                ctx,
                scope,
                CompanionPass {
                    root: frame.root,
                    owner: frame.owner,
                    session: frame.session,
                    first: frame.first && !frame.os_close && !frame.close,
                    input_owned: owns,
                    initial_focus,
                },
            );
        }
        self.native_jump.finish_ui_pass();
        let binding_after = companion
            .as_ref()
            .and_then(|(child, _)| child.binding(viewport))
            .or_else(|| dialog.confirmation_binding(viewport, instance))
            .unwrap_or(parent);
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
        dialog: &mut D,
    ) -> Option<HostCompletion<D::Command>> {
        let mut frame = self.pending.take()?;
        let current = dialog.is_open()
            && frame.session == dialog.session()
            && frame.host == self.state.identity();
        if frame.os_close {
            ctx.send_viewport_cmd_to(
                Self::viewport(frame.session),
                egui::ViewportCommand::CancelClose,
            );
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
                    dialog.restore(checkpoint);
                }
                ctx.request_repaint_of(frame.owner.viewport_id);
            }
            self.cancel_jump();
            if self.deferred_close.is_none()
                && let Some(presentation) = self.apply_deferred_open(ctx, dialog)
            {
                return Some(HostCompletion {
                    commands: Vec::new(),
                    show_root: presentation == UtilityPresentation::Docked,
                    owner_viewport: frame.owner.viewport_id,
                    origin: None,
                });
            }
            return None;
        }
        if frame.close || frame.os_close {
            self.remember_presentation(ctx, frame.host.host);
            self.retire(ctx);
            return Some(HostCompletion {
                commands: vec![D::closed_command()],
                show_root: false,
                owner_viewport: frame.owner.viewport_id,
                origin: None,
            });
        }
        self.state
            .complete_bootstrap(frame.host, frame.embedded_layer, frame.embedded_focus);
        let native = frame
            .native_info
            .as_ref()
            .map(|_| Self::geometry_identity(frame.session, frame.host));
        if let (Some(identity), Some(info)) = (native, frame.native_info.as_ref()) {
            apply_native_utility_geometry_restore(
                ctx,
                egui::ViewportClass::Immediate,
                true,
                &Self::title(frame.session),
                identity,
                info,
                D::SIZE,
                Some(D::MINIMUM),
                None,
            );
        }
        if frame.focus {
            if let Some(identity) = native {
                ctx.send_viewport_cmd_to(identity.viewport_id, egui::ViewportCommand::Focus);
            } else if let Some(layer) = frame.embedded_layer {
                ctx.move_to_top(layer);
            } else if frame.host.host == UtilityPresentation::Docked {
                ctx.move_to_top(self.docked_layer());
                self.docked_focus_pending = false;
            }
            self.state.complete_focus(frame.host);
        }
        let Some(actions) = frame.actions.take() else {
            return self
                .apply_deferred_open(ctx, dialog)
                .map(|presentation| HostCompletion {
                    commands: Vec::new(),
                    show_root: presentation == UtilityPresentation::Docked,
                    owner_viewport: frame.owner.viewport_id,
                    origin: None,
                });
        };
        let presentation = D::host_request(&actions);
        #[cfg(windows)]
        let hwnd = super::utility_dialog_size::utility_picker_hwnd(ctx, native);
        #[cfg(not(windows))]
        let hwnd = None;
        let commands = dialog.complete_actions(actions, hwnd);
        let origin = if presentation.is_none() && !commands.iter().any(D::command_closes) {
            super::utility_host::AcceptedParentOpen::from_retained(ctx, frame.host, frame.session, frame.root, frame.owner)
        } else {
            None
        };
        let mut show_root = false;
        if commands.iter().any(D::command_closes) {
            self.remember_presentation(ctx, frame.host.host);
            self.retire(ctx);
        } else if let Some(presentation) = presentation
            && presentation != frame.host.host
        {
            self.remember_presentation(ctx, presentation);
            self.close_viewport(ctx);
            self.state.transition(presentation);
            self.cancel_jump();
            show_root = presentation == UtilityPresentation::Docked;
            if show_root {
                ctx.move_to_top(self.docked_layer());
            }
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
        let deferred = self.apply_deferred_open(ctx, dialog);
        Some(HostCompletion {
            commands,
            show_root: (show_root || deferred == Some(UtilityPresentation::Docked))
                && self.state.presentation() == UtilityPresentation::Docked,
            owner_viewport: frame.owner.viewport_id,
            origin,
        })
    }

    fn remember_presentation(&self, ctx: &egui::Context, presentation: UtilityPresentation) {
        if let (Some(manager), Ok(key)) = (
            super::utility_presentation::UtilityPresentationManager::for_context(ctx),
            crate::settings::UtilityToolKey::parse(D::KEY),
        ) {
            manager.remember(ctx, key, presentation.into());
        }
    }
}

fn file_confirmation_scope(
    jump: &JumpModeUi,
    owner: NavigationOwner,
    binding: NavigationViewportBinding,
    parent: SurfaceId,
) -> Option<JumpModeScope> {
    let (instance, kind, namespace) = match owner {
        NavigationOwner::BatchRenameConfirmation {
            confirmation_instance,
            kind,
            ..
        } => (
            confirmation_instance,
            kind.surface_kind(),
            "batch-rename/confirmation".to_owned(),
        ),
        NavigationOwner::SearchReplaceConfirmation {
            confirmation_instance,
            kind,
            subject,
            ..
        } => (
            confirmation_instance,
            kind.surface_kind(),
            format!(
                "search-replace/confirmation/{}/{:016x}",
                kind.surface_kind(),
                egui::Id::new(("search-replace-confirmation-subject", subject)).value()
            ),
        ),
        _ => return None,
    };
    jump.scope_for_viewport_binding(
        SurfaceId::popup(kind, instance, parent),
        TargetGeneration(instance),
        namespace,
        binding,
    )
}

fn consume_close(ctx: &egui::Context) -> bool {
    ctx.input_mut(|input| {
        input.consume_key(egui::Modifiers::COMMAND, egui::Key::W)
            || input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
    })
}

fn draw_root_companion<D: HostedUtilityDialog>(
    ctx: &egui::Context,
    companion: &mut dyn UtilityWindowCompanion,
    scope: Option<JumpModeScope>,
    frame: &HostFrame<D>,
    parent_layer: egui::LayerId,
    input: bool,
) {
    use super::jump_mode::ui::{focused_response_layer, visible_top_middle_layer};
    let Some(binding) = companion.binding(egui::ViewportId::ROOT) else {
        return;
    };
    let input = input && !egui::Popup::is_any_open(ctx);
    let top = visible_top_middle_layer(ctx);
    let focus_is_owned = focused_response_layer(ctx).is_none_or(|focused| {
        focused == binding.base_layer
            || focused == parent_layer
            || focused.order != egui::Order::Middle
    });
    companion.render(
        ctx,
        scope,
        CompanionPass {
            root: frame.root,
            owner: frame.owner,
            session: frame.session,
            first: frame.first && !frame.close,
            input_owned: input && focus_is_owned && top == Some(binding.base_layer),
            initial_focus: input
                && focus_is_owned
                && top.is_none_or(|top| top == parent_layer || top == binding.base_layer),
        },
    );
    // An exposed parent margin can raise the docked or embedded window.
    // Reassert this exact child only over its own parent, never a foreign layer.
    if input
        && focus_is_owned
        && top == Some(parent_layer)
        && ctx.memory(|memory| memory.areas().visible_last_frame(&binding.base_layer))
    {
        ctx.move_to_top(binding.base_layer);
        ctx.request_repaint();
    }
}

fn draw<D: HostedUtilityDialog>(
    ctx: &egui::Context,
    dialog: &mut D,
    runtime: &D::Runtime,
    frame: &mut HostFrame<D>,
    child_ui: Option<&mut egui::Ui>,
    scopes: (Option<&JumpModeScope>, Option<&JumpModeScope>),
    owner: NavigationOwner,
    parent_current: bool,
    child_owner: NavigationOwner,
    parent_instance: u64,
) {
    let open = !frame.os_close && !frame.close;
    let allow = frame.first && open;
    // A sizing replay can publish current geometry without executing input.
    let render_owned = open && (allow || D::REGISTER_TARGETS_ON_REPLAY);
    let capture = allow
        && (parent_current || child_owner != NavigationOwner::Blocked)
        && (scopes
            .0
            .is_some_and(JumpModeScope::has_pending_focus_intent)
            || scopes
                .1
                .is_some_and(JumpModeScope::has_pending_focus_intent)
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
        frame.checkpoint = Some(dialog.capture());
    }
    let replay_scopes = (!allow && render_owned).then(|| {
        (
            scopes.0.map(JumpModeScope::registration_only),
            scopes.1.map(JumpModeScope::registration_only),
        )
    });
    let scopes = replay_scopes
        .as_ref()
        .map_or(scopes, |(parent, child)| (parent.as_ref(), child.as_ref()));
    frame.actions = Some(dialog.render_host(
        ctx,
        scopes,
        owner,
        render_owned && parent_current,
        if render_owned {
            child_owner
        } else {
            NavigationOwner::Blocked
        },
        allow,
        parent_instance,
        runtime,
        child_ui,
        frame.host.host,
    ));
}

impl HostedUtilityDialog for BatchRenameDialogState {
    const KIND: Option<NavigationDialogKind> = Some(NavigationDialogKind::BatchRename);
    const KEY: &'static str = "batch-rename";
    const TITLE: &'static str = "Batch Renamer";
    const SIZE: egui::Vec2 = egui::vec2(1040.0, 760.0);
    const MINIMUM: egui::Vec2 = egui::vec2(520.0, 360.0);
    const SCROLL_NATIVE_WINDOW: bool = true;
    type Runtime = RenameRuntimeSnapshot;
    type Actions = BatchRenameHostActions;
    type Checkpoint = BatchRenameInteractionCheckpoint;
    type Command = BatchRenameUiCommand;

    fn is_open(&self) -> bool {
        self.open
    }
    fn session(&self) -> u64 {
        self.jump_mode_target_generation()
    }
    fn parent_targets_allowed(&self, _: &Self::Runtime) -> bool {
        self.jump_mode_parent_targets_allowed()
    }
    fn parent_input_allowed(&self) -> bool {
        !self.jump_mode_has_pending_confirmation()
    }
    fn popup_layer(&self, ctx: &egui::Context, parent: egui::LayerId) -> Option<egui::LayerId> {
        self.owned_combo_layer(ctx, parent)
    }
    fn confirmation_binding(
        &self,
        viewport: egui::ViewportId,
        instance: u64,
    ) -> Option<NavigationViewportBinding> {
        self.jump_mode_pending_confirmation_identity()
            .map(|(confirmation_instance, kind)| NavigationViewportBinding {
                viewport,
                base_layer: batch_rename_confirmation_layer_id(
                    instance,
                    confirmation_instance,
                    kind,
                ),
                owner: NavigationOwner::BatchRenameConfirmation {
                    parent_instance: instance,
                    confirmation_instance,
                    kind,
                },
            })
    }
    fn capture(&self) -> Self::Checkpoint {
        BatchRenameInteractionCheckpoint::capture(self)
    }
    fn restore(&mut self, checkpoint: Self::Checkpoint) {
        checkpoint.restore(self);
    }
    fn render_host(
        &mut self,
        ctx: &egui::Context,
        scopes: (Option<&JumpModeScope>, Option<&JumpModeScope>),
        owner: NavigationOwner,
        parent_input: bool,
        child_owner: NavigationOwner,
        first: bool,
        instance: u64,
        runtime: &Self::Runtime,
        child_ui: Option<&mut egui::Ui>,
        presentation: UtilityPresentation,
    ) -> Self::Actions {
        self.ui_with_host(
            ctx,
            scopes.0,
            scopes.1,
            owner,
            parent_input,
            child_owner,
            first,
            Some(instance),
            runtime,
            child_ui,
            presentation == UtilityPresentation::Popout,
        )
    }
    fn host_request(actions: &Self::Actions) -> Option<UtilityPresentation> {
        actions.host_request.map(|popout| {
            if popout {
                UtilityPresentation::Popout
            } else {
                UtilityPresentation::Docked
            }
        })
    }
    fn closed_command() -> Self::Command {
        BatchRenameUiCommand::Closed
    }

    fn command_closes(command: &Self::Command) -> bool {
        matches!(command, BatchRenameUiCommand::Closed)
    }
    fn complete_actions(
        &mut self,
        actions: Self::Actions,
        hwnd: Option<usize>,
    ) -> Vec<Self::Command> {
        self.complete_host_actions(actions, hwnd)
    }
    #[cfg(test)]
    fn test_button(actions: &Self::Actions) -> Option<(egui::Rect, egui::Rect, bool)> {
        actions.host_button
    }
}

impl HostedUtilityDialog for SearchReplaceDialogState {
    const KIND: Option<NavigationDialogKind> = Some(NavigationDialogKind::SearchReplace);
    const KEY: &'static str = "search-replace";
    const TITLE: &'static str = "Search and Replace";
    const SIZE: egui::Vec2 = egui::vec2(1120.0, 820.0);
    const MINIMUM: egui::Vec2 = egui::vec2(560.0, 360.0);
    const SCROLL_NATIVE_WINDOW: bool = false;
    type Runtime = crate::replace_executor::ReplaceWorkerSnapshot;
    type Actions = SearchReplaceHostActions;
    type Checkpoint = SearchReplaceInteractionCheckpoint;
    type Command = SearchReplaceUiCommand;

    fn is_open(&self) -> bool {
        self.open
    }

    fn session(&self) -> u64 {
        self.host_session_epoch()
    }

    fn parent_targets_allowed(&self, runtime: &Self::Runtime) -> bool {
        self.jump_mode_parent_targets_allowed(runtime.active)
    }

    fn parent_input_allowed(&self) -> bool {
        self.native_parent_ui_allowed()
    }

    fn popup_layer(&self, ctx: &egui::Context, _: egui::LayerId) -> Option<egui::LayerId> {
        self.current_owned_native_popup_layer(ctx)
    }

    fn confirmation_binding(
        &self,
        viewport: egui::ViewportId,
        instance: u64,
    ) -> Option<NavigationViewportBinding> {
        let identity = self.jump_mode_confirmation_identity()?;
        Some(NavigationViewportBinding {
            viewport,
            base_layer: identity.kind.layer_id(),
            owner: NavigationOwner::SearchReplaceConfirmation {
                parent_instance: instance,
                confirmation_instance: identity.confirmation_instance,
                kind: identity.kind,
                subject: identity.subject,
            },
        })
    }

    fn capture(&self) -> Self::Checkpoint {
        SearchReplaceInteractionCheckpoint::capture(self)
    }

    fn restore(&mut self, checkpoint: Self::Checkpoint) {
        checkpoint.restore(self);
    }

    fn render_host(
        &mut self,
        ctx: &egui::Context,
        scopes: (Option<&JumpModeScope>, Option<&JumpModeScope>),
        _: NavigationOwner,
        parent_input: bool,
        child_owner: NavigationOwner,
        first: bool,
        instance: u64,
        runtime: &Self::Runtime,
        child_ui: Option<&mut egui::Ui>,
        presentation: UtilityPresentation,
    ) -> Self::Actions {
        let child_is_current = self
            .confirmation_binding(ctx.viewport_id(), instance)
            .is_some_and(|binding| binding.owner == child_owner);
        self.ui_with_host(
            ctx,
            scopes.0,
            scopes.1,
            parent_input,
            first,
            child_is_current,
            instance,
            runtime,
            child_ui,
            presentation == UtilityPresentation::Popout,
        )
    }

    fn host_request(actions: &Self::Actions) -> Option<UtilityPresentation> {
        actions.host_request.map(|popout| {
            if popout {
                UtilityPresentation::Popout
            } else {
                UtilityPresentation::Docked
            }
        })
    }

    fn closed_command() -> Self::Command {
        SearchReplaceUiCommand::Closed
    }

    fn command_closes(command: &Self::Command) -> bool {
        matches!(command, SearchReplaceUiCommand::Closed)
    }

    fn complete_actions(
        &mut self,
        actions: Self::Actions,
        hwnd: Option<usize>,
    ) -> Vec<Self::Command> {
        self.complete_host_actions(actions, hwnd)
    }

    #[cfg(test)]
    fn test_button(actions: &Self::Actions) -> Option<(egui::Rect, egui::Rect, bool)> {
        actions.host_button
    }
}

impl HostedUtilityDialog for ClipboardDialog {
    const KIND: Option<NavigationDialogKind> = Some(NavigationDialogKind::ClipboardHistory);
    const KEY: &'static str = "clipboard-history";
    const TITLE: &'static str = "Clipboard History";
    const SIZE: egui::Vec2 = egui::vec2(360.0, 240.0);
    const MINIMUM: egui::Vec2 = egui::vec2(100.0, 100.0);
    const SCROLL_NATIVE_WINDOW: bool = false;
    type Runtime = bool;
    type Actions = ClipboardHostActions;
    type Checkpoint = ClipboardInteractionCheckpoint;
    type Command = ClipboardHostCommand;

    fn is_open(&self) -> bool {
        self.open
    }

    fn session(&self) -> u64 {
        self.host_session_epoch()
    }

    fn target_generation(&self) -> TargetGeneration {
        self.jump_mode_target_generation()
    }

    fn parent_targets_allowed(&self, _: &Self::Runtime) -> bool {
        self.parent_input_allowed()
    }

    fn parent_input_allowed(&self) -> bool {
        self.jump_mode_pending_removal_identity().is_none()
    }

    fn confirmation_can_close() -> bool {
        true
    }

    fn popup_layer(&self, ctx: &egui::Context, parent: egui::LayerId) -> Option<egui::LayerId> {
        self.owned_popup_layer(ctx, parent)
    }

    fn confirmation_binding(
        &self,
        viewport: egui::ViewportId,
        instance: u64,
    ) -> Option<NavigationViewportBinding> {
        let (confirmation_instance, _) = self.jump_mode_pending_removal_identity()?;
        Some(NavigationViewportBinding {
            viewport,
            base_layer: Self::layer(),
            owner: NavigationOwner::ClipboardRemoval {
                parent_instance: instance,
                confirmation_instance,
            },
        })
    }

    fn confirmation_scope(
        &self,
        jump: &JumpModeUi,
        owner: NavigationOwner,
        binding: NavigationViewportBinding,
        parent: SurfaceId,
    ) -> Option<JumpModeScope> {
        let (instance, entry_token) = self.jump_mode_pending_removal_identity()?;
        if !matches!(owner, NavigationOwner::ClipboardRemoval { confirmation_instance, .. }
            if confirmation_instance == instance)
        {
            return None;
        }
        let generation = self.jump_mode_target_generation();
        jump.scope_for_viewport_binding(
            SurfaceId::popup("clipboard-removal", instance, parent),
            generation,
            format!(
                "clipboard-removal/{}/{instance}/{entry_token}",
                generation.0
            ),
            binding,
        )
    }

    fn capture(&self) -> Self::Checkpoint {
        ClipboardInteractionCheckpoint::capture(self)
    }

    fn restore(&mut self, checkpoint: Self::Checkpoint) {
        checkpoint.restore(self);
    }

    fn render_host(
        &mut self,
        ctx: &egui::Context,
        scopes: (Option<&JumpModeScope>, Option<&JumpModeScope>),
        _: NavigationOwner,
        parent_input: bool,
        child_owner: NavigationOwner,
        first: bool,
        instance: u64,
        pinned: &bool,
        child_ui: Option<&mut egui::Ui>,
        presentation: UtilityPresentation,
    ) -> Self::Actions {
        let child_is_current = self
            .confirmation_binding(ctx.viewport_id(), instance)
            .is_some_and(|binding| binding.owner == child_owner);
        self.render_host(
            ctx,
            scopes.0.or(scopes.1),
            parent_input || child_is_current,
            first,
            *pinned,
            child_ui,
            presentation,
        )
    }

    fn host_request(actions: &Self::Actions) -> Option<UtilityPresentation> {
        actions.host_request
    }

    fn closed_command() -> Self::Command {
        ClipboardHostCommand::Closed
    }

    fn command_closes(command: &Self::Command) -> bool {
        matches!(command, ClipboardHostCommand::Closed)
    }

    fn complete_actions(&mut self, actions: Self::Actions, _: Option<usize>) -> Vec<Self::Command> {
        if actions.close {
            vec![ClipboardHostCommand::Closed]
        } else if actions.effects.needs_completion() {
            vec![ClipboardHostCommand::Apply(actions.effects)]
        } else {
            Vec::new()
        }
    }

    #[cfg(test)]
    fn test_button(actions: &Self::Actions) -> Option<(egui::Rect, egui::Rect, bool)> {
        actions.host_button
    }
}

impl HostedUtilityDialog for SnippetDialog {
    const KIND: Option<NavigationDialogKind> = Some(NavigationDialogKind::Snippets);
    const KEY: &'static str = "snippets";
    const TITLE: &'static str = "Snippets";
    const SIZE: egui::Vec2 = egui::vec2(600.0, 500.0);
    const MINIMUM: egui::Vec2 = egui::vec2(160.0, 160.0);
    const SCROLL_NATIVE_WINDOW: bool = false;
    const REGISTER_TARGETS_ON_REPLAY: bool = true;
    type Runtime = bool;
    type Actions = SnippetHostActions;
    type Checkpoint = SnippetInteractionCheckpoint;
    type Command = SnippetHostCommand;

    fn is_open(&self) -> bool {
        self.open
    }

    fn session(&self) -> u64 {
        self.host_session_epoch()
    }

    fn target_generation(&self) -> TargetGeneration {
        self.jump_mode_target_generation()
    }

    fn parent_targets_allowed(&self, _: &Self::Runtime) -> bool {
        self.parent_input_allowed()
    }

    fn parent_input_allowed(&self) -> bool {
        self.jump_mode_pending_removal_identity().is_none()
    }

    fn confirmation_can_close() -> bool {
        true
    }

    fn popup_layer(&self, ctx: &egui::Context, parent: egui::LayerId) -> Option<egui::LayerId> {
        self.owned_popup_layer(ctx, parent)
    }

    fn confirmation_binding(
        &self,
        viewport: egui::ViewportId,
        instance: u64,
    ) -> Option<NavigationViewportBinding> {
        let (confirmation_instance, _) = self.jump_mode_pending_removal_identity()?;
        Some(NavigationViewportBinding {
            viewport,
            base_layer: Self::layer(),
            owner: NavigationOwner::SnippetRemoval {
                parent_instance: instance,
                confirmation_instance,
            },
        })
    }

    fn confirmation_scope(
        &self,
        jump: &JumpModeUi,
        owner: NavigationOwner,
        binding: NavigationViewportBinding,
        parent: SurfaceId,
    ) -> Option<JumpModeScope> {
        let (instance, index) = self.jump_mode_pending_removal_identity()?;
        if !matches!(owner, NavigationOwner::SnippetRemoval { confirmation_instance, .. }
            if confirmation_instance == instance)
        {
            return None;
        }
        let generation = self.jump_mode_target_generation();
        jump.scope_for_viewport_binding(
            SurfaceId::popup("snippet-removal", instance, parent),
            generation,
            format!("snippet-removal/{}/{instance}/{index}", generation.0),
            binding,
        )
    }

    fn capture(&self) -> Self::Checkpoint {
        SnippetInteractionCheckpoint::capture(self)
    }

    fn restore(&mut self, checkpoint: Self::Checkpoint) {
        checkpoint.restore(self);
    }

    fn render_host(
        &mut self,
        ctx: &egui::Context,
        scopes: (Option<&JumpModeScope>, Option<&JumpModeScope>),
        _: NavigationOwner,
        parent_input: bool,
        child_owner: NavigationOwner,
        first: bool,
        instance: u64,
        pinned: &bool,
        child_ui: Option<&mut egui::Ui>,
        presentation: UtilityPresentation,
    ) -> Self::Actions {
        let child_is_current = self
            .confirmation_binding(ctx.viewport_id(), instance)
            .is_some_and(|binding| binding.owner == child_owner);
        self.render_host(
            ctx,
            scopes.0.or(scopes.1),
            parent_input || child_is_current,
            first,
            *pinned,
            child_ui,
            presentation,
        )
    }

    fn host_request(actions: &Self::Actions) -> Option<UtilityPresentation> {
        actions.host_request
    }

    fn closed_command() -> Self::Command {
        SnippetHostCommand::Closed
    }

    fn command_closes(command: &Self::Command) -> bool {
        matches!(command, SnippetHostCommand::Closed)
    }

    fn complete_actions(&mut self, actions: Self::Actions, _: Option<usize>) -> Vec<Self::Command> {
        if actions.close {
            vec![SnippetHostCommand::Closed]
        } else if actions.effects.needs_completion() {
            vec![SnippetHostCommand::Apply(actions.effects)]
        } else {
            Vec::new()
        }
    }

    #[cfg(test)]
    fn test_button(actions: &Self::Actions) -> Option<(egui::Rect, egui::Rect, bool)> {
        actions.host_button
    }
}

impl HostedUtilityDialog for ClipboardModifyDialogState {
    const KIND: Option<NavigationDialogKind> = Some(NavigationDialogKind::ClipboardModify);
    const KEY: &'static str = "clipboard-modify";
    const TITLE: &'static str = "Clipboard Modify";
    const SIZE: egui::Vec2 = egui::vec2(900.0, 640.0);
    const MINIMUM: egui::Vec2 = egui::vec2(520.0, 360.0);
    const SCROLL_NATIVE_WINDOW: bool = false;
    type Runtime = ClipboardModifyHostRuntime;
    type Actions = ClipboardModifyHostActions;
    type Checkpoint = ClipboardModifyInteractionCheckpoint;
    type Command = ClipboardModifyHostCommand;

    fn is_open(&self) -> bool {
        self.open
    }

    fn session(&self) -> u64 {
        self.host_session_epoch()
    }

    fn parent_targets_allowed(&self, _: &Self::Runtime) -> bool {
        false
    }

    fn parent_input_allowed(&self) -> bool {
        true
    }

    fn popup_layer(&self, ctx: &egui::Context, parent: egui::LayerId) -> Option<egui::LayerId> {
        self.owned_popup_layer(ctx, parent)
    }

    fn confirmation_binding(
        &self, _: egui::ViewportId, _: u64,
    ) -> Option<NavigationViewportBinding> {
        None
    }

    fn confirmation_scope(
        &self, _: &JumpModeUi, _: NavigationOwner, _: NavigationViewportBinding, _: SurfaceId,
    ) -> Option<JumpModeScope> {
        None
    }

    fn capture(&self) -> Self::Checkpoint {
        ClipboardModifyInteractionCheckpoint::capture(self)
    }

    fn restore(&mut self, checkpoint: Self::Checkpoint) {
        checkpoint.restore(self);
    }

    fn render_host(
        &mut self,
        ctx: &egui::Context,
        _: (Option<&JumpModeScope>, Option<&JumpModeScope>),
        _: NavigationOwner,
        parent_input: bool,
        _: NavigationOwner,
        first: bool,
        _: u64,
        runtime: &Self::Runtime,
        child_ui: Option<&mut egui::Ui>,
        presentation: UtilityPresentation,
    ) -> Self::Actions {
        #[cfg(test)]
        if let Some(service) = &runtime.test_service {
            return self.render_host(
                ctx, runtime.legacy_window_size, service, runtime.catalog.clone(),
                Some(&runtime.store), parent_input, first, runtime.pinned, child_ui, presentation,
            );
        }
        self.render_host(
            ctx, runtime.legacy_window_size, &crate::clipboard_modify::runtime::clipboard_service(),
            runtime.catalog.clone(), Some(&runtime.store), parent_input, first, runtime.pinned,
            child_ui, presentation,
        )
    }

    fn host_request(actions: &Self::Actions) -> Option<UtilityPresentation> {
        actions.host_request
    }

    fn closed_command() -> Self::Command {
        ClipboardModifyHostCommand::Closed
    }

    fn command_closes(command: &Self::Command) -> bool {
        matches!(command, ClipboardModifyHostCommand::Closed)
    }

    fn complete_actions(&mut self, actions: Self::Actions, _: Option<usize>) -> Vec<Self::Command> {
        if actions.close {
            vec![ClipboardModifyHostCommand::Closed]
        } else if actions.needs_completion() {
            vec![ClipboardModifyHostCommand::Apply(actions)]
        } else {
            Vec::new()
        }
    }

    #[cfg(test)]
    fn test_button(actions: &Self::Actions) -> Option<(egui::Rect, egui::Rect, bool)> {
        actions.host_button
    }
}

impl HostedUtilityDialog for PasteSequenceDialog {
    const KEY: &'static str = "paste-sequences";
    const SURFACE_KIND: &'static str = "paste-sequence";
    const TITLE: &'static str = "Paste Sequences";
    const SIZE: egui::Vec2 = egui::vec2(760.0, 640.0);
    const MINIMUM: egui::Vec2 = egui::vec2(520.0, 420.0);
    const SCROLL_NATIVE_WINDOW: bool = false;
    type Runtime = HostedPasteSequenceRuntime;
    type Actions = PasteSequenceHostActions;
    type Checkpoint = PasteSequenceInteractionCheckpoint;
    type Command = PasteSequenceHostCommand;

    fn layer() -> egui::LayerId {
        super::jump_mode::ui::paste_sequence_dialog_layer_id()
    }

    fn navigation_owner(instance: u64) -> NavigationOwner {
        NavigationOwner::PasteSequence { instance }
    }

    fn is_open(&self) -> bool {
        self.open
    }

    fn session(&self) -> u64 {
        self.host_session_epoch()
    }

    fn parent_targets_allowed(&self, _: &Self::Runtime) -> bool {
        true
    }

    fn parent_input_allowed(&self) -> bool {
        true
    }

    fn popup_layer(&self, _: &egui::Context, _: egui::LayerId) -> Option<egui::LayerId> {
        None
    }

    fn confirmation_binding(
        &self,
        _: egui::ViewportId,
        _: u64,
    ) -> Option<NavigationViewportBinding> {
        // Inline picker and confirmation derive exact child scopes from the
        // current Paste Sequence owner on this same layer.
        None
    }

    fn capture(&self) -> Self::Checkpoint {
        PasteSequenceInteractionCheckpoint::capture(self)
    }

    fn restore(&mut self, checkpoint: Self::Checkpoint) {
        checkpoint.restore(self);
    }

    fn render_host(
        &mut self,
        ctx: &egui::Context,
        scopes: (Option<&JumpModeScope>, Option<&JumpModeScope>),
        _: NavigationOwner,
        parent_input: bool,
        _: NavigationOwner,
        first: bool,
        _: u64,
        runtime: &Self::Runtime,
        child_ui: Option<&mut egui::Ui>,
        presentation: UtilityPresentation,
    ) -> Self::Actions {
        self.render_host(
            ctx, &runtime.dialog, scopes.0.cloned(), parent_input, first,
            runtime.pinned, child_ui, presentation,
        )
    }

    fn host_request(actions: &Self::Actions) -> Option<UtilityPresentation> {
        (!actions.needs_completion()).then_some(actions.host_request).flatten()
    }

    fn closed_command() -> Self::Command {
        PasteSequenceHostCommand::Closed
    }

    fn command_closes(command: &Self::Command) -> bool {
        matches!(command, PasteSequenceHostCommand::Closed)
    }

    fn complete_actions(&mut self, actions: Self::Actions, _: Option<usize>) -> Vec<Self::Command> {
        if actions.needs_completion() {
            vec![PasteSequenceHostCommand::Apply(actions)]
        } else {
            Vec::new()
        }
    }

    #[cfg(test)]
    fn test_button(actions: &Self::Actions) -> Option<(egui::Rect, egui::Rect, bool)> {
        actions.host_button
    }
}

impl HostedUtilityDialog for JsonUtilityDialogState {
    const KIND: Option<NavigationDialogKind> = Some(NavigationDialogKind::JsonUtility);
    const KEY: &'static str = "json-utility";
    const TITLE: &'static str = "JSON Utility";
    const SIZE: egui::Vec2 = egui::vec2(700.0, 480.0);
    const MINIMUM: egui::Vec2 = egui::vec2(340.0, 220.0);
    const SCROLL_NATIVE_WINDOW: bool = false;
    const REGISTER_TARGETS_ON_REPLAY: bool = true;
    type Runtime = bool;
    type Actions = JsonUtilityHostActions;
    type Checkpoint = JsonUtilityInteractionCheckpoint;
    type Command = JsonUtilityHostCommand;

    fn is_open(&self) -> bool {
        self.open
    }

    fn session(&self) -> u64 {
        self.host_session_epoch()
    }

    fn parent_targets_allowed(&self, _: &Self::Runtime) -> bool {
        true
    }

    fn parent_input_allowed(&self) -> bool {
        true
    }

    fn popup_layer(&self, _: &egui::Context, _: egui::LayerId) -> Option<egui::LayerId> {
        None
    }

    fn confirmation_binding(
        &self, _: egui::ViewportId, _: u64,
    ) -> Option<NavigationViewportBinding> {
        None
    }

    fn capture(&self) -> Self::Checkpoint {
        JsonUtilityInteractionCheckpoint::capture(self)
    }

    fn restore(&mut self, checkpoint: Self::Checkpoint) {
        checkpoint.restore(self);
    }

    fn render_host(
        &mut self,
        ctx: &egui::Context,
        scopes: (Option<&JumpModeScope>, Option<&JumpModeScope>),
        owner: NavigationOwner,
        parent_input: bool,
        _: NavigationOwner,
        first: bool,
        _: u64,
        pinned: &bool,
        child_ui: Option<&mut egui::Ui>,
        presentation: UtilityPresentation,
    ) -> Self::Actions {
        let instance = match owner {
            NavigationOwner::Dialog { kind: NavigationDialogKind::JsonUtility, instance } => Some(instance),
            _ => None,
        };
        self.render_host(ctx, scopes.0, instance, parent_input, first, *pinned, child_ui, presentation)
    }

    fn host_request(actions: &Self::Actions) -> Option<UtilityPresentation> {
        actions.host_request
    }

    fn closed_command() -> Self::Command {
        JsonUtilityHostCommand::Closed
    }

    fn command_closes(command: &Self::Command) -> bool {
        matches!(command, JsonUtilityHostCommand::Closed)
    }

    fn complete_actions(&mut self, actions: Self::Actions, _: Option<usize>) -> Vec<Self::Command> {
        if actions.close {
            vec![JsonUtilityHostCommand::Closed]
        } else if actions.needs_completion() {
            vec![JsonUtilityHostCommand::Apply(actions)]
        } else {
            Vec::new()
        }
    }

    #[cfg(test)]
    fn test_button(actions: &Self::Actions) -> Option<(egui::Rect, egui::Rect, bool)> {
        actions.host_button
    }
}

pub(super) type ProgrammerInspectorWindowHost =
    UtilityWindowHost<super::programmer_inspector_dialog::ProgrammerInspectorDialog>;
pub(super) enum ProgrammerInspectorHostCommand {
    Closed,
    Apply(super::programmer_inspector_dialog::InspectorActions, Option<usize>),
}
impl HostedUtilityDialog for super::programmer_inspector_dialog::ProgrammerInspectorDialog {
    const KEY: &'static str = "programmer-inspector";
    const REGISTER_TARGETS_ON_REPLAY: bool = true;
    const TITLE: &'static str = "Programmer Inspector";
    const SIZE: egui::Vec2 = egui::vec2(720.0, 650.0);
    const MINIMUM: egui::Vec2 = egui::vec2(360.0, 280.0);
    const SCROLL_NATIVE_WINDOW: bool = true;
    type Runtime = super::programmer_inspector_dialog::InspectorRuntime;
    type Actions = super::programmer_inspector_dialog::InspectorActions;
    type Checkpoint = ();
    type Command = ProgrammerInspectorHostCommand;
    fn layer() -> egui::LayerId {
        egui::LayerId::new(
            egui::Order::Middle,
            egui::Id::new("programmer-inspector-window"),
        )
    }
    fn navigation_owner(_: u64) -> NavigationOwner {
        NavigationOwner::Blocked
    }
    fn is_open(&self) -> bool {
        self.open
    }
    fn session(&self) -> u64 {
        self.epoch()
    }
    fn parent_targets_allowed(&self, _: &Self::Runtime) -> bool {
        false
    }
    fn parent_input_allowed(&self) -> bool {
        true
    }
    fn native_parent_current(
        &self,
        binding: NavigationViewportBinding,
        ctx: &egui::Context,
        _: Option<egui::LayerId>,
    ) -> bool {
        ctx.viewport_id() == binding.viewport
            && ctx.input(|input| input.focused)
            && ctx.memory(|memory| memory.top_modal_layer().is_none())
            && super::jump_mode::ui::visible_top_middle_layer(ctx)
                .is_none_or(|layer| layer == binding.base_layer)
            && super::jump_mode::ui::focused_response_layer(ctx).is_none_or(|layer| {
                layer == binding.base_layer || layer == egui::LayerId::background()
            })
    }
    fn popup_layer(&self, _: &egui::Context, _: egui::LayerId) -> Option<egui::LayerId> {
        None
    }
    fn confirmation_binding(
        &self,
        _: egui::ViewportId,
        _: u64,
    ) -> Option<NavigationViewportBinding> {
        None
    }
    fn capture(&self) {}
    fn restore(&mut self, _: ()) {}
    fn render_host(
        &mut self,
        ctx: &egui::Context,
        _: (Option<&JumpModeScope>, Option<&JumpModeScope>),
        _: NavigationOwner,
        parent: bool,
        _: NavigationOwner,
        first: bool,
        _: u64,
        runtime: &Self::Runtime,
        child: Option<&mut egui::Ui>,
        presentation: UtilityPresentation,
    ) -> Self::Actions {
        self.render_host(ctx, runtime, parent, first, child, presentation)
    }
    fn host_request(actions: &Self::Actions) -> Option<UtilityPresentation> {
        actions.host_request
    }
    fn closed_command() -> Self::Command {
        ProgrammerInspectorHostCommand::Closed
    }
    fn command_closes(command: &Self::Command) -> bool {
        matches!(command, ProgrammerInspectorHostCommand::Closed)
    }
    fn complete_actions(&mut self, actions: Self::Actions, hwnd: Option<usize>) -> Vec<Self::Command> {
        if actions.close {
            vec![ProgrammerInspectorHostCommand::Closed]
        } else {
            vec![ProgrammerInspectorHostCommand::Apply(actions, hwnd)]
        }
    }
    #[cfg(test)]
    fn test_button(actions: &Self::Actions) -> Option<(egui::Rect, egui::Rect, bool)> {
        actions.host_button
    }
}

pub(super) type ImageWindowHost = UtilityWindowHost<super::image_panel::ImagePanel>;
pub(super) enum ImageHostCommand {
    Closed,
    Apply(super::image_panel::ImageHostActions),
}
impl HostedUtilityDialog for super::image_panel::ImagePanel {
    const KEY: &'static str = "image-viewer";
    const TITLE: &'static str = "Image Viewer";
    const SIZE: egui::Vec2 = egui::vec2(620.0, 480.0);
    const MINIMUM: egui::Vec2 = egui::vec2(200.0, 160.0);
    const SCROLL_NATIVE_WINDOW: bool = false;
    const REGISTER_TARGETS_ON_REPLAY: bool = true;
    type Runtime = ();
    type Actions = super::image_panel::ImageHostActions;
    type Checkpoint = ();
    type Command = ImageHostCommand;
    fn layer() -> egui::LayerId {
        egui::LayerId::new(egui::Order::Middle, egui::Id::new("image-viewer"))
    }
    fn dialog_layer(&self) -> egui::LayerId {
        self.query_focus_layer_id()
    }
    fn navigation_owner(_: u64) -> NavigationOwner {
        NavigationOwner::Blocked
    }
    fn is_open(&self) -> bool {
        self.open
    }
    fn session(&self) -> u64 {
        self.session()
    }
    fn parent_targets_allowed(&self, _: &()) -> bool {
        false
    }
    fn parent_input_allowed(&self) -> bool {
        true
    }
    fn native_parent_current(
        &self,
        binding: NavigationViewportBinding,
        ctx: &egui::Context,
        popup: Option<egui::LayerId>,
    ) -> bool {
        ctx.viewport_id() == binding.viewport
            && ctx.input(|input| input.focused)
            && ctx.memory(|memory| memory.top_modal_layer().is_none())
            && super::jump_mode::ui::visible_top_middle_layer(ctx)
                .is_none_or(|layer| layer == binding.base_layer)
            && super::jump_mode::ui::focused_response_layer(ctx).is_none_or(|layer| {
                layer == binding.base_layer
                    || Some(layer) == popup
                    || layer == egui::LayerId::background()
            })
    }
    fn popup_layer(&self, _: &egui::Context, _: egui::LayerId) -> Option<egui::LayerId> {
        None
    }
    fn confirmation_binding(
        &self,
        _: egui::ViewportId,
        _: u64,
    ) -> Option<NavigationViewportBinding> {
        None
    }
    fn capture(&self) {}
    fn restore(&mut self, _: ()) {}
    fn render_host(
        &mut self,
        ctx: &egui::Context,
        _: (Option<&JumpModeScope>, Option<&JumpModeScope>),
        _: NavigationOwner,
        allowed: bool,
        _: NavigationOwner,
        first: bool,
        _: u64,
        _: &(),
        child: Option<&mut egui::Ui>,
        presentation: UtilityPresentation,
    ) -> Self::Actions {
        self.render_host(ctx, allowed, first, child, presentation)
    }
    fn host_request(actions: &Self::Actions) -> Option<UtilityPresentation> {
        actions.host_request
    }
    fn closed_command() -> Self::Command {
        ImageHostCommand::Closed
    }
    fn command_closes(command: &Self::Command) -> bool {
        matches!(command, ImageHostCommand::Closed)
    }
    fn complete_actions(&mut self, actions: Self::Actions, _: Option<usize>) -> Vec<Self::Command> {
        if actions.close {
            vec![ImageHostCommand::Closed]
        } else {
            vec![ImageHostCommand::Apply(actions)]
        }
    }
    #[cfg(test)]
    fn test_button(_: &Self::Actions) -> Option<(egui::Rect, egui::Rect, bool)> {
        None
    }
}

pub(super) type NoteTemplatesWindowHost =
    UtilityWindowHost<super::notes_dialog::TemplateManagerState>;
pub(super) enum NoteTemplatesHostCommand {
    Closed,
    Apply(super::notes_dialog::TemplateHostActions),
}
impl HostedUtilityDialog for super::notes_dialog::TemplateManagerState {
    const KEY: &'static str = "note-templates";
    const TITLE: &'static str = "Note Templates";
    const SIZE: egui::Vec2 = egui::vec2(520.0, 360.0);
    const MINIMUM: egui::Vec2 = egui::vec2(320.0, 220.0);
    const SCROLL_NATIVE_WINDOW: bool = true;
    const REGISTER_TARGETS_ON_REPLAY: bool = true;
    type Runtime = ();
    type Actions = super::notes_dialog::TemplateHostActions;
    type Checkpoint = ();
    type Command = NoteTemplatesHostCommand;
    fn layer() -> egui::LayerId {
        egui::LayerId::new(egui::Order::Middle, egui::Id::new(Some("Note Templates")))
    }
    fn navigation_owner(_: u64) -> NavigationOwner {
        NavigationOwner::Blocked
    }
    fn is_open(&self) -> bool {
        self.is_open()
    }
    fn session(&self) -> u64 {
        self.epoch()
    }
    fn parent_targets_allowed(&self, _: &()) -> bool {
        false
    }
    fn parent_input_allowed(&self) -> bool {
        true
    }
    fn native_parent_current(
        &self,
        binding: NavigationViewportBinding,
        ctx: &egui::Context,
        popup: Option<egui::LayerId>,
    ) -> bool {
        ctx.viewport_id() == binding.viewport
            && ctx.input(|input| input.focused)
            && (self.owns_input(ctx)
                || (ctx.memory(|memory| memory.top_modal_layer().is_none())
                    && super::jump_mode::ui::visible_top_middle_layer(ctx)
                        .is_none_or(|layer| layer == binding.base_layer)
                    && super::jump_mode::ui::focused_response_layer(ctx).is_none_or(|layer| {
                        layer == binding.base_layer
                            || Some(layer) == popup
                            || layer == egui::LayerId::background()
                    })))
    }
    fn popup_layer(&self, _: &egui::Context, _: egui::LayerId) -> Option<egui::LayerId> {
        None
    }
    fn confirmation_binding(
        &self,
        _: egui::ViewportId,
        _: u64,
    ) -> Option<NavigationViewportBinding> {
        None
    }
    fn capture(&self) {}
    fn restore(&mut self, _: ()) {}
    fn render_host(
        &mut self,
        ctx: &egui::Context,
        _: (Option<&JumpModeScope>, Option<&JumpModeScope>),
        _: NavigationOwner,
        allowed: bool,
        _: NavigationOwner,
        first: bool,
        _: u64,
        _: &(),
        child: Option<&mut egui::Ui>,
        presentation: UtilityPresentation,
    ) -> Self::Actions {
        super::notes_dialog::TemplateManagerState::render_host(
            self,
            ctx,
            allowed,
            first,
            child,
            presentation,
        )
    }
    fn host_request(actions: &Self::Actions) -> Option<UtilityPresentation> {
        actions.host_request
    }
    fn closed_command() -> Self::Command {
        NoteTemplatesHostCommand::Closed
    }
    fn command_closes(command: &Self::Command) -> bool {
        matches!(command, NoteTemplatesHostCommand::Closed)
    }
    fn complete_actions(&mut self, actions: Self::Actions, _: Option<usize>) -> Vec<Self::Command> {
        if actions.close {
            vec![NoteTemplatesHostCommand::Closed]
        } else {
            vec![NoteTemplatesHostCommand::Apply(actions)]
        }
    }
    #[cfg(test)]
    fn test_button(_: &Self::Actions) -> Option<(egui::Rect, egui::Rect, bool)> {
        None
    }
}

pub(super) type UnusedAssetsWindowHost =
    UtilityWindowHost<super::unused_assets_dialog::UnusedAssetsDialog>;
pub(super) enum UnusedAssetsHostCommand {
    Closed,
    Apply(super::unused_assets_dialog::UnusedAssetsHostActions),
}
impl HostedUtilityDialog for super::unused_assets_dialog::UnusedAssetsDialog {
    const KEY: &'static str = "unused-note-assets";
    const TITLE: &'static str = "Unused Note Assets";
    const SIZE: egui::Vec2 = egui::vec2(300.0, 200.0);
    const MINIMUM: egui::Vec2 = egui::vec2(160.0, 120.0);
    const SCROLL_NATIVE_WINDOW: bool = false;
    const REGISTER_TARGETS_ON_REPLAY: bool = true;
    type Runtime = bool;
    type Actions = super::unused_assets_dialog::UnusedAssetsHostActions;
    type Checkpoint = ();
    type Command = UnusedAssetsHostCommand;
    fn layer() -> egui::LayerId {
        egui::LayerId::new(
            egui::Order::Middle,
            egui::Id::new(Some("Unused Note Assets")),
        )
    }
    fn navigation_owner(_: u64) -> NavigationOwner {
        NavigationOwner::Blocked
    }
    fn is_open(&self) -> bool {
        self.open
    }
    fn session(&self) -> u64 {
        self.epoch()
    }
    fn parent_targets_allowed(&self, _: &Self::Runtime) -> bool {
        false
    }
    fn parent_input_allowed(&self) -> bool {
        true
    }
    fn native_parent_current(
        &self,
        binding: NavigationViewportBinding,
        ctx: &egui::Context,
        popup: Option<egui::LayerId>,
    ) -> bool {
        ctx.viewport_id() == binding.viewport
            && ctx.input(|input| input.focused)
            && ctx.memory(|memory| memory.top_modal_layer().is_none())
            && super::jump_mode::ui::visible_top_middle_layer(ctx)
                .is_none_or(|layer| layer == binding.base_layer)
            && super::jump_mode::ui::focused_response_layer(ctx).is_none_or(|layer| {
                layer == binding.base_layer
                    || Some(layer) == popup
                    || layer == egui::LayerId::background()
            })
    }
    fn popup_layer(&self, _: &egui::Context, _: egui::LayerId) -> Option<egui::LayerId> {
        None
    }
    fn confirmation_binding(
        &self,
        _: egui::ViewportId,
        _: u64,
    ) -> Option<NavigationViewportBinding> {
        None
    }
    fn capture(&self) {}
    fn restore(&mut self, _: ()) {}
    fn render_host(
        &mut self,
        ctx: &egui::Context,
        _: (Option<&JumpModeScope>, Option<&JumpModeScope>),
        _: NavigationOwner,
        allowed: bool,
        _: NavigationOwner,
        first: bool,
        _: u64,
        pinned: &bool,
        child: Option<&mut egui::Ui>,
        presentation: UtilityPresentation,
    ) -> Self::Actions {
        let mut actions = self.render_host(ctx, allowed, first, child, presentation);
        if *pinned {
            actions.close = false;
        }
        actions
    }
    fn host_request(actions: &Self::Actions) -> Option<UtilityPresentation> {
        actions.host_request
    }
    fn closed_command() -> Self::Command {
        UnusedAssetsHostCommand::Closed
    }
    fn command_closes(command: &Self::Command) -> bool {
        matches!(command, UnusedAssetsHostCommand::Closed)
    }
    fn complete_actions(&mut self, actions: Self::Actions, _: Option<usize>) -> Vec<Self::Command> {
        if actions.close {
            vec![UnusedAssetsHostCommand::Closed]
        } else {
            vec![UnusedAssetsHostCommand::Apply(actions)]
        }
    }
    #[cfg(test)]
    fn test_button(_: &Self::Actions) -> Option<(egui::Rect, egui::Rect, bool)> {
        None
    }
}

pub(super) enum PluginHostCommand {
    Close,
    Save(
        u64,
        u64,
        crate::plugin_editor::PluginSelection,
        crate::plugin_editor::PluginSelection,
    ),
    Navigation(crate::settings_editor::render::SettingsNavigationFrameObserver),
}
impl HostedUtilityDialog for crate::plugin_editor::PluginEditor {
    const KEY: &'static str = "plugin-settings";
    const TITLE: &'static str = "Plugin Settings";
    const SIZE: egui::Vec2 = egui::vec2(540.0, 440.0);
    const MINIMUM: egui::Vec2 = egui::vec2(320.0, 240.0);
    const SCROLL_NATIVE_WINDOW: bool = false;
    const REGISTER_TARGETS_ON_REPLAY: bool = true;
    type Runtime = crate::plugin_editor::PluginHostRuntime;
    type Actions = crate::plugin_editor::PluginHostActions;
    type Checkpoint = Self;
    type Command = PluginHostCommand;
    fn layer() -> egui::LayerId {
        egui::LayerId::new(egui::Order::Middle, egui::Id::new("Plugin Settings"))
    }
    fn navigation_owner(_: u64) -> NavigationOwner {
        NavigationOwner::Blocked
    }
    fn is_open(&self) -> bool {
        self.host_open
    }
    fn session(&self) -> u64 {
        self.host_session()
    }
    fn parent_targets_allowed(&self, _: &Self::Runtime) -> bool {
        false
    }
    fn parent_input_allowed(&self) -> bool {
        true
    }
    fn native_parent_current(
        &self,
        binding: NavigationViewportBinding,
        ctx: &egui::Context,
        popup: Option<egui::LayerId>,
    ) -> bool {
        ctx.viewport_id() == binding.viewport
            && ctx.input(|i| i.focused)
            && ctx.memory(|m| m.top_modal_layer().is_none())
            && super::jump_mode::ui::visible_top_middle_layer(ctx)
                .is_none_or(|l| l == binding.base_layer)
            && super::jump_mode::ui::focused_response_layer(ctx).is_none_or(|l| {
                l == binding.base_layer || Some(l) == popup || l == egui::LayerId::background()
            })
    }
    fn popup_layer(&self, _: &egui::Context, _: egui::LayerId) -> Option<egui::LayerId> {
        None
    }
    fn confirmation_binding(
        &self,
        _: egui::ViewportId,
        _: u64,
    ) -> Option<NavigationViewportBinding> {
        None
    }
    fn capture(&self) -> Self {
        self.clone()
    }
    fn restore(&mut self, checkpoint: Self) {
        *self = checkpoint;
    }
    fn render_host(
        &mut self,
        ctx: &egui::Context,
        _: (Option<&JumpModeScope>, Option<&JumpModeScope>),
        _: NavigationOwner,
        allowed: bool,
        _: NavigationOwner,
        first: bool,
        _: u64,
        runtime: &Self::Runtime,
        child: Option<&mut egui::Ui>,
        presentation: UtilityPresentation,
    ) -> Self::Actions {
        self.render_host(ctx, runtime, allowed, first, child, presentation)
    }
    fn host_request(actions: &Self::Actions) -> Option<UtilityPresentation> {
        actions.presentation_request
    }
    fn closed_command() -> PluginHostCommand {
        PluginHostCommand::Close
    }
    fn command_closes(command: &PluginHostCommand) -> bool {
        matches!(command, PluginHostCommand::Close)
    }
    fn complete_actions(
        &mut self,
        actions: Self::Actions,
        _: Option<usize>,
    ) -> Vec<PluginHostCommand> {
        self.apply_host_filter(&actions.filter_before, actions.filter_after);
        let mut commands = Vec::new();
        if actions.close {
            commands.push(PluginHostCommand::Close);
        } else if actions.changed {
            commands.push(PluginHostCommand::Save(
                actions.catalog_revision,
                actions.selection_revision,
                actions.before,
                actions.candidate,
            ));
        }
        if let Some(observer) = actions.navigation {
            commands.push(PluginHostCommand::Navigation(observer));
        }
        commands
    }
    #[cfg(test)]
    fn test_button(actions: &Self::Actions) -> Option<(egui::Rect, egui::Rect, bool)> {
        actions.test_button
    }
}

pub(super) type FilePreviewWindowHost =
    UtilityWindowHost<super::file_search_preview_dialog::FileSearchPreviewDialogState>;
pub(super) enum FilePreviewHostCommand {
    Closed,
    Apply(super::file_search_preview_dialog::FileSearchPreviewHostActions),
}
impl HostedUtilityDialog for super::file_search_preview_dialog::FileSearchPreviewDialogState {
    const KEY: &'static str = "file-preview";
    const TITLE: &'static str = "File preview";
    const SIZE: egui::Vec2 = egui::vec2(860.0, 620.0);
    const MINIMUM: egui::Vec2 = egui::vec2(320.0, 240.0);
    const SCROLL_NATIVE_WINDOW: bool = false;
    type Runtime = crate::file_search::settings::FileSearchSettings;
    type Actions = super::file_search_preview_dialog::FileSearchPreviewHostActions;
    type Checkpoint = ();
    type Command = FilePreviewHostCommand;
    fn layer() -> egui::LayerId {
        egui::LayerId::new(
            egui::Order::Middle,
            egui::Id::new(super::file_search_preview_dialog::preview_window_id_source()),
        )
    }
    fn navigation_owner(_: u64) -> NavigationOwner {
        NavigationOwner::Blocked
    }
    fn is_open(&self) -> bool {
        self.open
    }
    fn session(&self) -> u64 {
        self.epoch()
    }
    fn parent_targets_allowed(&self, _: &Self::Runtime) -> bool {
        false
    }
    fn parent_input_allowed(&self) -> bool {
        true
    }
    fn native_parent_current(
        &self,
        binding: NavigationViewportBinding,
        ctx: &egui::Context,
        popup: Option<egui::LayerId>,
    ) -> bool {
        ctx.viewport_id() == binding.viewport
            && ctx.input(|input| input.focused)
            && ctx.memory(|memory| memory.top_modal_layer().is_none())
            && super::jump_mode::ui::visible_top_middle_layer(ctx)
                .is_none_or(|layer| layer == binding.base_layer)
            && super::jump_mode::ui::focused_response_layer(ctx).is_none_or(|layer| {
                layer == binding.base_layer
                    || Some(layer) == popup
                    || layer == egui::LayerId::background()
            })
    }
    fn popup_layer(&self, _: &egui::Context, _: egui::LayerId) -> Option<egui::LayerId> {
        None
    }
    fn confirmation_binding(
        &self,
        _: egui::ViewportId,
        _: u64,
    ) -> Option<NavigationViewportBinding> {
        None
    }
    fn capture(&self) {}
    fn restore(&mut self, _: ()) {}
    fn render_host(
        &mut self,
        ctx: &egui::Context,
        _: (Option<&JumpModeScope>, Option<&JumpModeScope>),
        _: NavigationOwner,
        allowed: bool,
        _: NavigationOwner,
        first: bool,
        _: u64,
        runtime: &Self::Runtime,
        child: Option<&mut egui::Ui>,
        presentation: UtilityPresentation,
    ) -> Self::Actions {
        self.render_host(ctx, runtime, allowed, first, child, presentation)
    }
    fn host_request(actions: &Self::Actions) -> Option<UtilityPresentation> {
        actions.host_request
    }
    fn closed_command() -> Self::Command {
        FilePreviewHostCommand::Closed
    }
    fn command_closes(command: &Self::Command) -> bool {
        matches!(command, FilePreviewHostCommand::Closed)
    }
    fn complete_actions(&mut self, actions: Self::Actions, _: Option<usize>) -> Vec<Self::Command> {
        if actions.close {
            vec![FilePreviewHostCommand::Closed]
        } else {
            vec![FilePreviewHostCommand::Apply(actions)]
        }
    }
    #[cfg(test)]
    fn test_button(actions: &Self::Actions) -> Option<(egui::Rect, egui::Rect, bool)> {
        actions.host_button
    }
}

pub(super) type HashGeneratorWindowHost =
    UtilityWindowHost<super::hash_generator_dialog::HashGeneratorDialogState>;
pub(super) enum HashGeneratorHostCommand {
    Closed,
    Apply(super::hash_generator_dialog::HashHostActions, Option<usize>),
}
impl HostedUtilityDialog for super::hash_generator_dialog::HashGeneratorDialogState {
    const KEY: &'static str = "hash-generator";
    const TITLE: &'static str = "Hash Generator";
    const SIZE: egui::Vec2 = egui::vec2(620.0, 440.0);
    const MINIMUM: egui::Vec2 = egui::vec2(420.0, 320.0);
    const SCROLL_NATIVE_WINDOW: bool = true;
    const REGISTER_TARGETS_ON_REPLAY: bool = true;
    type Runtime = super::hash_generator_dialog::HashRuntime;
    type Actions = super::hash_generator_dialog::HashHostActions;
    type Checkpoint = ();
    type Command = HashGeneratorHostCommand;
    fn layer() -> egui::LayerId {
        egui::LayerId::new(egui::Order::Middle, egui::Id::new("hash_generator_dialog"))
    }
    fn navigation_owner(_: u64) -> NavigationOwner {
        NavigationOwner::Blocked
    }
    fn is_open(&self) -> bool {
        self.open
    }
    fn session(&self) -> u64 {
        self.epoch()
    }
    fn parent_targets_allowed(&self, _: &Self::Runtime) -> bool {
        false
    }
    fn parent_input_allowed(&self) -> bool {
        true
    }
    fn native_parent_current(
        &self,
        binding: NavigationViewportBinding,
        ctx: &egui::Context,
        popup: Option<egui::LayerId>,
    ) -> bool {
        ctx.viewport_id() == binding.viewport
            && ctx.input(|input| input.focused)
            && ctx.memory(|memory| memory.top_modal_layer().is_none())
            && super::jump_mode::ui::visible_top_middle_layer(ctx)
                .is_none_or(|layer| layer == binding.base_layer)
            && super::jump_mode::ui::focused_response_layer(ctx).is_none_or(|layer| {
                layer == binding.base_layer
                    || Some(layer) == popup
                    || layer == egui::LayerId::background()
            })
    }
    fn popup_layer(&self, ctx: &egui::Context, base: egui::LayerId) -> Option<egui::LayerId> {
        self.owned_popup_layer(ctx, base)
    }
    fn confirmation_binding(
        &self,
        _: egui::ViewportId,
        _: u64,
    ) -> Option<NavigationViewportBinding> {
        None
    }
    fn capture(&self) -> Self::Checkpoint {}
    fn restore(&mut self, _: Self::Checkpoint) {}
    fn render_host(
        &mut self,
        ctx: &egui::Context,
        _: (Option<&JumpModeScope>, Option<&JumpModeScope>),
        _: NavigationOwner,
        allowed: bool,
        _: NavigationOwner,
        first: bool,
        _: u64,
        runtime: &Self::Runtime,
        child: Option<&mut egui::Ui>,
        presentation: UtilityPresentation,
    ) -> Self::Actions {
        self.render_host(ctx, runtime, allowed, first, child, presentation)
    }
    fn host_request(actions: &Self::Actions) -> Option<UtilityPresentation> {
        actions.host_request
    }
    fn closed_command() -> Self::Command {
        HashGeneratorHostCommand::Closed
    }
    fn command_closes(command: &Self::Command) -> bool {
        matches!(command, HashGeneratorHostCommand::Closed)
    }
    fn complete_actions(
        &mut self,
        actions: Self::Actions,
        hwnd: Option<usize>,
    ) -> Vec<Self::Command> {
        if actions.close {
            vec![HashGeneratorHostCommand::Closed]
        } else {
            vec![HashGeneratorHostCommand::Apply(actions, hwnd)]
        }
    }
    #[cfg(test)]
    fn test_button(_: &Self::Actions) -> Option<(egui::Rect, egui::Rect, bool)> {
        None
    }
}

pub(super) type TodoViewWindowHost =
    UtilityWindowHost<super::todo_view_dialog::TodoViewDialog>;
pub(super) enum TodoViewHostCommand {
    Closed,
    Apply(super::todo_view_dialog::TodoViewHostActions),
}
impl HostedUtilityDialog for super::todo_view_dialog::TodoViewDialog {
    const KEY: &'static str = "todo-view";
    const TITLE: &'static str = "View Todos";
    const SIZE: egui::Vec2 = egui::vec2(360.0, 260.0);
    const MINIMUM: egui::Vec2 = egui::vec2(360.0, 260.0);
    const SCROLL_NATIVE_WINDOW: bool = false;
    const RESIZABLE: bool = false;
    const REGISTER_TARGETS_ON_REPLAY: bool = true;
    type Runtime = super::todo_view_dialog::TodoViewRuntime;
    type Actions = super::todo_view_dialog::TodoViewHostActions;
    type Checkpoint = ();
    type Command = TodoViewHostCommand;
    fn layer() -> egui::LayerId {
        egui::LayerId::new(egui::Order::Middle, egui::Id::new(Some("View Todos")))
    }
    fn navigation_owner(_: u64) -> NavigationOwner {
        NavigationOwner::Blocked
    }
    fn is_open(&self) -> bool {
        self.open
    }
    fn session(&self) -> u64 {
        self.epoch()
    }
    fn parent_targets_allowed(&self, _: &Self::Runtime) -> bool {
        false
    }
    fn parent_input_allowed(&self) -> bool {
        true
    }
    fn native_parent_current(
        &self,
        binding: NavigationViewportBinding,
        ctx: &egui::Context,
        popup: Option<egui::LayerId>,
    ) -> bool {
        ctx.viewport_id() == binding.viewport
            && ctx.input(|input| input.focused)
            && ctx.memory(|memory| memory.top_modal_layer().is_none())
            && super::jump_mode::ui::visible_top_middle_layer(ctx)
                .is_none_or(|layer| layer == binding.base_layer)
            && super::jump_mode::ui::focused_response_layer(ctx).is_none_or(|layer| {
                layer == binding.base_layer
                    || Some(layer) == popup
                    || layer == egui::LayerId::background()
            })
    }
    fn popup_layer(&self, ctx: &egui::Context, base: egui::LayerId) -> Option<egui::LayerId> {
        self.owned_popup_layer(ctx, base)
    }
    fn confirmation_binding(
        &self,
        _: egui::ViewportId,
        _: u64,
    ) -> Option<NavigationViewportBinding> {
        None
    }
    fn capture(&self) -> Self::Checkpoint {}
    fn restore(&mut self, _: Self::Checkpoint) {}
    fn render_host(
        &mut self,
        ctx: &egui::Context,
        _: (Option<&JumpModeScope>, Option<&JumpModeScope>),
        _: NavigationOwner,
        allowed: bool,
        _: NavigationOwner,
        first: bool,
        _: u64,
        runtime: &Self::Runtime,
        child: Option<&mut egui::Ui>,
        presentation: UtilityPresentation,
    ) -> Self::Actions {
        self.render_host(ctx, runtime, allowed, first, child, presentation)
    }
    fn host_request(actions: &Self::Actions) -> Option<UtilityPresentation> {
        actions.host_request
    }
    fn closed_command() -> Self::Command {
        TodoViewHostCommand::Closed
    }
    fn command_closes(command: &Self::Command) -> bool {
        matches!(command, TodoViewHostCommand::Closed)
    }
    fn complete_actions(
        &mut self,
        actions: Self::Actions,
        _: Option<usize>,
    ) -> Vec<Self::Command> {
        if actions.close {
            vec![TodoViewHostCommand::Closed]
        } else {
            vec![TodoViewHostCommand::Apply(actions)]
        }
    }
    #[cfg(test)]
    fn test_button(_: &Self::Actions) -> Option<(egui::Rect, egui::Rect, bool)> {
        None
    }
}

pub(super) type CalculationDependencyWindowHost =
    UtilityWindowHost<super::calculation_dependency_dialog::CalculationDependencyDialog>;
pub(super) enum CalculationDependencyHostCommand {
    Closed,
    Apply(super::calculation_dependency_dialog::DependencyActions),
}
impl HostedUtilityDialog for super::calculation_dependency_dialog::CalculationDependencyDialog {
    const KEY: &'static str = "calculation-pad-dependencies";
    const TITLE: &'static str = "Dependency Explorer";
    const SIZE: egui::Vec2 = egui::vec2(560.0, 500.0);
    const MINIMUM: egui::Vec2 = egui::vec2(320.0, 240.0);
    const SCROLL_NATIVE_WINDOW: bool = false;
    type Runtime = super::calculation_dependency_dialog::DependencyRuntime;
    type Actions = super::calculation_dependency_dialog::DependencyActions;
    type Checkpoint = ();
    type Command = CalculationDependencyHostCommand;
    fn layer() -> egui::LayerId {
        egui::LayerId::new(
            egui::Order::Middle,
            egui::Id::new("calculation-dependencies-window"),
        )
    }
    fn navigation_owner(_: u64) -> NavigationOwner {
        NavigationOwner::Blocked
    }
    fn is_open(&self) -> bool {
        self.open
    }
    fn session(&self) -> u64 {
        self.epoch()
    }
    fn parent_targets_allowed(&self, _: &Self::Runtime) -> bool {
        false
    }
    fn parent_input_allowed(&self) -> bool {
        true
    }
    fn native_parent_current(
        &self,
        binding: NavigationViewportBinding,
        ctx: &egui::Context,
        _: Option<egui::LayerId>,
    ) -> bool {
        ctx.viewport_id() == binding.viewport
            && ctx.input(|input| input.focused)
            && ctx.memory(|memory| memory.top_modal_layer().is_none())
            && super::jump_mode::ui::visible_top_middle_layer(ctx)
                .is_none_or(|layer| layer == binding.base_layer)
            && super::jump_mode::ui::focused_response_layer(ctx).is_none_or(|layer| {
                layer == binding.base_layer || layer == egui::LayerId::background()
            })
    }
    fn popup_layer(&self, _: &egui::Context, _: egui::LayerId) -> Option<egui::LayerId> {
        None
    }
    fn confirmation_binding(
        &self,
        _: egui::ViewportId,
        _: u64,
    ) -> Option<NavigationViewportBinding> {
        None
    }
    fn capture(&self) {}
    fn restore(&mut self, _: ()) {}
    fn render_host(
        &mut self,
        ctx: &egui::Context,
        _: (Option<&JumpModeScope>, Option<&JumpModeScope>),
        _: NavigationOwner,
        parent: bool,
        _: NavigationOwner,
        first: bool,
        _: u64,
        runtime: &Self::Runtime,
        child: Option<&mut egui::Ui>,
        presentation: UtilityPresentation,
    ) -> Self::Actions {
        self.render_host(ctx, runtime, parent, first, child, presentation)
    }
    fn host_request(actions: &Self::Actions) -> Option<UtilityPresentation> {
        actions.host_request
    }
    fn closed_command() -> Self::Command {
        CalculationDependencyHostCommand::Closed
    }
    fn command_closes(command: &Self::Command) -> bool {
        matches!(command, CalculationDependencyHostCommand::Closed)
    }
    fn complete_actions(&mut self, actions: Self::Actions, _: Option<usize>) -> Vec<Self::Command> {
        if actions.close {
            vec![CalculationDependencyHostCommand::Closed]
        } else {
            vec![CalculationDependencyHostCommand::Apply(actions)]
        }
    }
    #[cfg(test)]
    fn test_button(_: &Self::Actions) -> Option<(egui::Rect, egui::Rect, bool)> {
        None
    }
}

#[cfg(test)]
mod deferred_open_tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn inspector_label_file_action_rejects_discard_and_retired_source_host() {
        let ctx = egui::Context::default();
        let mut dialog =
            super::super::programmer_inspector_dialog::ProgrammerInspectorDialog::default();
        dialog.open();
        let context =
            Arc::new(super::super::calculation_pad_dialog::InspectorPadContext::default());
        dialog.publish(Arc::clone(&context));
        let mut host = ProgrammerInspectorWindowHost::default();
        host.prepare_open(&ctx, &dialog);
        let make_frame = |host: &ProgrammerInspectorWindowHost, dialog: &super::super::programmer_inspector_dialog::ProgrammerInspectorDialog| {
            let pass = ViewportPassProvenance::current(&ctx, egui::ViewportId::ROOT);
            HostFrame {
                root: pass, owner: pass, host: host.state.identity(), session: dialog.epoch(),
                first: true, os_close: false, close: false,
                actions: Some(dialog.test_pending_label_file_action(&ctx, Arc::clone(&context), host.identity())),
                checkpoint: None, native_info: None, embedded_layer: None, embedded_focus: None, focus: false,
            }
        };
        host.pending = Some(make_frame(&host, &dialog));
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            if ui.ctx().current_pass_index() == 0 {
                ui.ctx().request_discard("discard label-file action");
            }
        });
        assert!(host.complete(&ctx, &mut dialog).is_none());
        assert_eq!(dialog.test_file_feedback(), None);
        output.drop_without_applying_deltas();

        host.pending = Some(make_frame(&host, &dialog));
        let output = ctx.run_ui(egui::RawInput::default(), |_| {});
        let completion = host.complete(&ctx, &mut dialog).unwrap();
        host.state.transition(UtilityPresentation::Popout);
        for command in completion.commands {
            if let ProgrammerInspectorHostCommand::Apply(actions, hwnd) = command {
                dialog.commit_with_file(actions, &context, hwnd, &host);
            }
        }
        assert_eq!(
            dialog.test_file_feedback(),
            None,
            "retired source must not start a picker"
        );
        output.drop_without_applying_deltas();
    }

    #[test]
    fn pending_popout_and_edit_win_over_deferred_root_open() {
        let ctx = egui::Context::default();
        let mut dialog = super::super::programmer_inspector_dialog::ProgrammerInspectorDialog::default();
        dialog.open();
        let context = Arc::new(super::super::calculation_pad_dialog::InspectorPadContext::default());
        dialog.publish(Arc::clone(&context));
        let before = dialog.test_value().bit_pattern();
        let epoch = dialog.epoch();
        let mut host = ProgrammerInspectorWindowHost::default();
        host.prepare_open(&ctx, &dialog);
        let pass = ViewportPassProvenance::current(&ctx, egui::ViewportId::ROOT);
        host.pending = Some(HostFrame {
            root: pass,
            owner: pass,
            host: host.state.identity(),
            session: epoch,
            first: true,
            os_close: false,
            close: false,
            actions: Some(dialog.test_pending_popout_with_bit_edit(&ctx, Arc::clone(&context))),
            checkpoint: None,
            native_info: None,
            embedded_layer: None,
            embedded_focus: None,
            focus: false,
        });
        assert_eq!(
            host.open_from_origin(
                &ctx,
                &dialog,
                true,
                &super::super::utility_host::ChildOpenOrigin::<()>::Root,
            ),
            None
        );
        let output = ctx.run_ui(egui::RawInput::default(), |_| {});
        let completion = host.complete(&ctx, &mut dialog).unwrap();
        assert!(!completion.show_root);
        assert!(host.is_popped_out());
        assert_eq!(dialog.epoch(), epoch);
        for command in completion.commands {
            if let ProgrammerInspectorHostCommand::Apply(actions, _) = command {
                dialog.commit(actions, &context);
            }
        }
        assert_eq!(dialog.test_value().bit_pattern(), before ^ 1);
        output.drop_without_applying_deltas();
    }
}
