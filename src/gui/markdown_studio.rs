//! Retained-frame Markdown Studio editing and document lifecycle.

use crate::diagram::{DiagramSourceCopy, DiagramSourceCopyOrigin};
use crate::markdown_studio::{
    MarkdownDocument, MarkdownSaveOutcome, MarkdownTransitionDecision, MarkdownTransitionOutcome,
    PendingMarkdownTransition,
};
use crate::notes_markdown::{
    MarkdownBlockSnippet, MarkdownExportArtifact, MarkdownExportFormat, MarkdownInlineFormat,
    MarkdownPreview, PreparedMarkdownEdit, analyze_markdown, export_html, export_markdown,
    prepare_block_snippet, prepare_inline_format, prepare_insert_column, prepare_insert_row,
    prepare_insert_table, prepare_remove_column, prepare_remove_row, prepare_set_cell,
    table_at_byte,
};
use crate::platform::app_data::AppDataRoot;
use eframe::egui;
use pulldown_cmark::{Event, Options, Parser, Tag};
use std::collections::HashSet;
use std::fs;
use std::ops::Range;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use super::markdown_export::{ensure_extension, publish_export};
use super::markdown_preview::{MarkdownAssetImageLoader, MarkdownPreviewCache};
use super::utility_dialog_size::{UtilityDialogSizeWindowExt, UtilityWindowSizeRecipe};
use super::utility_host::{
    HostIdentity, UtilityHostState, UtilityPresentation, ViewportPassProvenance,
    presentation_button,
};
use super::utility_window_geometry::NativeViewportIdentity;

const STUDIO_WINDOW_ID: &str = "markdown-studio-window";
const STUDIO_VIEWPORT_ID: &str = "markdown_studio_viewport";
const SOURCE_EDITOR_ID: &str = "markdown-studio-source";
const STUDIO_DEFAULT_SIZE: egui::Vec2 = egui::vec2(1180.0, 760.0);
const STUDIO_MINIMUM_SIZE: egui::Vec2 = egui::vec2(460.0, 320.0);
const MAX_LOCAL_ASSET_COUNT: usize = 128;
const MAX_LOCAL_ASSET_AGGREGATE_BYTES: usize =
    crate::notes_markdown::MAX_LOCAL_RASTER_AGGREGATE_BYTES;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ViewMode {
    Source,
    Preview,
    #[default]
    Split,
}

#[derive(Clone, Debug)]
enum StudioIntent {
    New,
    Open,
    Save,
    SaveAs,
    Export(MarkdownExportFormat),
    Close,
    Decision(MarkdownTransitionDecision),
    CopyDiagram {
        index: usize,
        source_snapshot: String,
    },
    Undo,
    Redo,
}

struct RootEffects {
    root_pass: ViewportPassProvenance,
    host: HostIdentity,
    owner_pass: Option<ViewportPassProvenance>,
    owner_viewport_id: egui::ViewportId,
    owner_pass_accepted: bool,
    owner_context: egui::Context,
    callback_observed: bool,
    callback_class: egui::ViewportClass,
    native_viewport: Option<egui::ViewportInfo>,
    os_close_requested: bool,
    focus_host: bool,
    embedded_root_layer: Option<egui::LayerId>,
    embedded_root_focused_id: Option<egui::Id>,
    source: Option<String>,
    selection: Option<Range<usize>>,
    selection_to_install: Option<Range<usize>>,
    focus_editor_after_open: bool,
    view_mode: Option<ViewMode>,
    intent: Option<StudioIntent>,
    host_request: Option<UtilityPresentation>,
}

#[derive(Debug)]
pub(crate) enum MarkdownStudioEffect {
    OpenDiagram {
        source: DiagramSourceCopy,
        origin: MarkdownFenceOrigin,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct MarkdownFenceOrigin {
    pub(crate) identity: DiagramSourceCopyOrigin,
    pub(crate) source_snapshot: String,
    pub(crate) fence_index: usize,
    pub(super) host_origin: Option<super::utility_host::AcceptedParentOpen<(u64, Option<PathBuf>, Option<u64>)>>,
}

pub(crate) struct MarkdownStudioDialog {
    pub(crate) open: bool,
    host_state: UtilityHostState,
    presentation_prepared: bool,
    host_epoch: u64,
    show_root_after_host_change: bool,
    document: MarkdownDocument,
    default_directory: PathBuf,
    mode: ViewMode,
    selection: Range<usize>,
    deferred_selection: Option<Range<usize>>,
    open_focus_pending: bool,
    cache: Option<MarkdownPreviewCache>,
    commonmark_cache: egui_commonmark::CommonMarkCache,
    image_loader: Arc<MarkdownAssetImageLoader>,
    preview_generation: u64,
    active_diagram_origin: Option<MarkdownFenceOrigin>,
    pending_diagram_origin: Option<MarkdownFenceOrigin>,
    pending_effects: Option<RootEffects>,
    message: Option<(bool, String)>,
    close_requested: bool,
    #[cfg(windows)]
    picker_owner: Option<(egui::Context, Option<NativeViewportIdentity>)>,
    #[cfg(test)]
    test_editor_id: Option<egui::Id>,
    #[cfg(test)]
    test_editor_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_save_as_id: Option<egui::Id>,
    #[cfg(test)]
    test_save_as_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_export_id: Option<egui::Id>,
    #[cfg(test)]
    test_edit_id: Option<egui::Id>,
    #[cfg(test)]
    test_format_menu_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_bold_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_next_picker_result: Option<Option<PathBuf>>,
    #[cfg(test)]
    test_picker_calls: usize,
    #[cfg(test)]
    test_discard_id: Option<egui::Id>,
    #[cfg(test)]
    test_presentation_id: Option<egui::Id>,
    #[cfg(test)]
    test_presentation_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_presentation_clip_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_presentation_enabled: Option<bool>,
}

fn studio_viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of(STUDIO_VIEWPORT_ID)
}

fn studio_viewport_builder() -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title("Markdown Studio")
        .with_inner_size(STUDIO_DEFAULT_SIZE)
        .with_min_inner_size(STUDIO_MINIMUM_SIZE)
        .with_resizable(true)
        .with_visible(true)
}

fn native_geometry_identity(host: HostIdentity) -> Option<NativeViewportIdentity> {
    (host.host == UtilityPresentation::Popout).then_some(NativeViewportIdentity {
        key: crate::settings::UtilityToolKey::parse("markdown-studio").ok()?,
        viewport_id: studio_viewport_id(),
        owner_generation: 0,
        host_generation: host.generation,
    })
}

impl MarkdownStudioDialog {
    pub(crate) fn new(root: &AppDataRoot) -> Self {
        Self {
            open: false,
            host_state: UtilityHostState::docked(),
            presentation_prepared: false,
            host_epoch: 0,
            show_root_after_host_change: false,
            document: MarkdownDocument::new(),
            default_directory: root.path().to_path_buf(),
            mode: ViewMode::Split,
            selection: 0..0,
            deferred_selection: None,
            open_focus_pending: false,
            cache: None,
            commonmark_cache: egui_commonmark::CommonMarkCache::default(),
            image_loader: MarkdownAssetImageLoader::new(),
            preview_generation: 0,
            active_diagram_origin: None,
            pending_diagram_origin: None,
            pending_effects: None,
            message: None,
            close_requested: false,
            #[cfg(windows)]
            picker_owner: None,
            #[cfg(test)]
            test_editor_id: None,
            #[cfg(test)]
            test_editor_rect: None,
            #[cfg(test)]
            test_save_as_id: None,
            #[cfg(test)]
            test_save_as_rect: None,
            #[cfg(test)]
            test_export_id: None,
            #[cfg(test)]
            test_edit_id: None,
            #[cfg(test)]
            test_format_menu_rect: None,
            #[cfg(test)]
            test_bold_rect: None,
            #[cfg(test)]
            test_next_picker_result: None,
            #[cfg(test)]
            test_picker_calls: 0,
            #[cfg(test)]
            test_discard_id: None,
            #[cfg(test)]
            test_presentation_id: None,
            #[cfg(test)]
            test_presentation_rect: None,
            #[cfg(test)]
            test_presentation_clip_rect: None,
            #[cfg(test)]
            test_presentation_enabled: None,
        }
    }

    /// Explicit opener focuses the source editor and starts a new blank draft
    /// only after the previous document was accepted as closed.
    pub(crate) fn open(&mut self) {
        if self.document.source().is_none() {
            if let Err(error) = self.document.request_new() {
                self.set_error(error.to_string());
                return;
            }
            self.reset_presentation();
            self.active_diagram_origin = None;
            self.pending_diagram_origin = None;
        }
        if !self.open {
            self.presentation_prepared = false;
            self.host_epoch = self.host_epoch.wrapping_add(1);
        }
        self.open = true;
        self.deferred_selection = None;
        // The launcher can open the Studio while the Enter event that
        // activated the query result is still in this root pass. Defer source
        // focus until that pass has been retained, then install the saved
        // selection before the next editor pass.
        self.open_focus_pending = true;
        self.close_requested = false;
        self.host_state.request_focus();
    }

    /// Pinned maintenance is passive and cannot resurrect a retired document.
    pub(crate) fn ensure_open(&mut self) {
        if self.document.source().is_some() {
            if !self.open {
                self.presentation_prepared = false;
                self.host_epoch = self.host_epoch.wrapping_add(1);
            }
            self.open = true;
        }
    }

    pub(super) fn prepare_open_presentation(&mut self, ctx: &egui::Context) {
        if self.open && !self.presentation_prepared {
            let key = crate::settings::UtilityToolKey::parse("markdown-studio")
                .expect("stable Markdown Studio key");
            super::utility_host::restore_remembered_presentation(ctx, key, &mut self.host_state);
            self.presentation_prepared = true;
        }
    }

    pub(crate) fn request_close(&mut self) {
        if self.open {
            self.close_requested = true;
        }
    }

    pub(crate) fn needs_root_ui_when_hidden(&self) -> bool {
        self.host_state.bootstrap_pending()
            || (self.open && self.host_state.presentation() == UtilityPresentation::Popout)
            || self.close_requested
            || self.pending_effects.is_some()
    }

    pub(crate) fn is_popped_out(&self) -> bool {
        self.host_state.presentation() == UtilityPresentation::Popout
    }

    #[cfg(test)]
    pub(crate) fn test_pop_out(&mut self) {
        self.host_state.transition(UtilityPresentation::Popout);
    }

    pub(crate) fn owns_root_input(&self, ctx: &egui::Context) -> bool {
        if ctx.viewport_id() != egui::ViewportId::ROOT || !ctx.input(|input| input.focused) {
            return false;
        }
        let Some(layer) = self.host_layer_if_root() else {
            return false;
        };
        let focused = ctx.memory(|memory| memory.focused());
        if !focused
            .and_then(|id| ctx.read_response(id))
            .is_some_and(|response| response.layer_id == layer)
        {
            return false;
        }
        let pointer_position = ctx.input(|input| {
            (input.pointer.any_pressed() || input.pointer.any_released())
                .then(|| input.pointer.interact_pos())
                .flatten()
        });
        !pointer_position.is_some_and(|position| {
            ctx.layer_id_at(position)
                .is_some_and(|hit_layer| hit_layer != layer)
        })
    }

    pub(crate) fn take_show_root_after_host_change(&mut self) -> bool {
        std::mem::take(&mut self.show_root_after_host_change)
    }

    pub(crate) fn host_layer_if_root(&self) -> Option<egui::LayerId> {
        if !self.open {
            return None;
        }
        match self.host_state.presentation() {
            UtilityPresentation::Docked => Some(egui::LayerId::new(
                egui::Order::Middle,
                egui::Id::new(STUDIO_WINDOW_ID),
            )),
            UtilityPresentation::Popout => self.host_state.embedded_root_layer(),
        }
    }

    pub(crate) fn reconcile_diagram_source_origin(
        &mut self,
        active: Option<DiagramSourceCopyOrigin>,
        pending: Option<DiagramSourceCopyOrigin>,
    ) {
        let pending_matches_active = self
            .pending_diagram_origin
            .as_ref()
            .is_some_and(|origin| active.as_ref() == Some(&origin.identity));
        if pending_matches_active {
            self.active_diagram_origin = self.pending_diagram_origin.take();
        } else if self
            .pending_diagram_origin
            .as_ref()
            .is_some_and(|origin| pending.as_ref() != Some(&origin.identity))
        {
            // A failed or cancelled replacement leaves the old active origin
            // untouched and drops only its unaccepted candidate.
            self.pending_diagram_origin = None;
        }

        if self
            .active_diagram_origin
            .as_ref()
            .is_some_and(|origin| active.as_ref() != Some(&origin.identity))
        {
            self.active_diagram_origin = None;
        }
    }

    pub(super) fn accepts_diagram_origin(&self, origin: &MarkdownFenceOrigin) -> bool {
        self.open
            && !self.close_requested
            && self.document.pending_transition().is_none()
            && self.document.source() == Some(origin.source_snapshot.as_str())
            && origin.host_origin.as_ref().is_some_and(|receipt| {
                receipt.is_current(
                    self.host_state.identity(),
                    &(
                        self.host_epoch,
                        self.document.path().map(Path::to_path_buf),
                        self.document.revision(),
                    ),
                )
            })
    }

    pub(crate) fn track_diagram_source_origin(&mut self, origin: MarkdownFenceOrigin) {
        self.pending_diagram_origin = Some(origin);
    }

    pub(crate) fn owns_active_diagram_source_origin(
        &self,
        identity: &DiagramSourceCopyOrigin,
    ) -> bool {
        self.open
            && self
                .active_diagram_origin
                .as_ref()
                .is_some_and(|origin| origin.identity == *identity)
    }

    pub(crate) fn apply_diagram_source_handoff(&mut self, handoff: super::DiagramMarkdownSource) {
        if !self.open {
            self.set_error("Markdown Studio is closed; reopen the originating document before applying Diagram source.".into());
            return;
        }
        if self.document.pending_transition().is_some() {
            self.set_error(
                "Resolve the pending Markdown document action before applying Diagram source."
                    .into(),
            );
            return;
        }
        let Some(origin) = self
            .active_diagram_origin
            .as_ref()
            .filter(|origin| origin.identity == handoff.origin)
            .cloned()
        else {
            self.set_error(
                "This Diagram is not linked to the currently open Mermaid block.".into(),
            );
            return;
        };
        if self.document.source() != Some(origin.source_snapshot.as_str()) {
            self.set_error("The Markdown source changed after this Diagram was opened.".into());
            return;
        }
        let preview = match MarkdownPreview::analyze(&origin.source_snapshot) {
            Ok(preview) => preview,
            Err(error) => {
                self.set_error(error.to_string());
                return;
            }
        };
        let edit = match preview.prepare_fence_replacement(origin.fence_index, &handoff.source) {
            Ok(edit) => edit,
            Err(error) => {
                self.set_error(error.to_string());
                return;
            }
        };
        let original = origin.source_snapshot;
        match self.document.apply_prepared_edit(&edit) {
            Ok(applied) if applied.source != original => {
                self.cache = None;
                self.selection = applied.selection.clone();
                self.deferred_selection = Some(applied.selection);
                self.mode = ViewMode::Source;
                self.active_diagram_origin = None;
                self.message = Some((false, "Diagram source applied to its Mermaid block.".into()));
            }
            Ok(_) => {
                self.message = Some((false, "The Mermaid source is already up to date.".into()));
            }
            Err(error) => self.set_error(error.to_string()),
        }
    }

    pub(crate) fn show(&mut self, ctx: &egui::Context, first_ui_pass: bool) {
        self.show_on_root(ctx, first_ui_pass, true, true, true);
    }

    pub(crate) fn show_on_root(
        &mut self,
        ctx: &egui::Context,
        first_ui_pass: bool,
        application_input_allowed: bool,
        root_visible: bool,
        root_input_allowed: bool,
    ) {
        self.prepare_open_presentation(ctx);
        #[cfg(test)]
        {
            self.test_editor_rect = None;
            self.test_save_as_id = None;
            self.test_save_as_rect = None;
            self.test_export_id = None;
            self.test_edit_id = None;
            self.test_format_menu_rect = None;
            self.test_bold_rect = None;
            self.test_discard_id = None;
            self.test_presentation_id = None;
            self.test_presentation_rect = None;
            self.test_presentation_clip_rect = None;
            self.test_presentation_enabled = None;
        }
        if !self.open {
            self.pending_effects = None;
            return;
        }
        let root_pass = ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT);
        let host = self.host_state.identity();
        let original_source = self.document.source().unwrap_or_default().to_owned();
        let revision = self.document.revision().unwrap_or(0);
        self.ensure_preview(ctx, revision, &original_source);
        let mut source = original_source.clone();
        let mut selection = self.selection.clone();
        let mut selection_to_install = None;
        let mut intent = None;
        let mut next_mode = None;
        let mut host_request = None;
        let focus_editor_after_open = self.open_focus_pending;
        let mut owner_context = ctx.clone();
        let mut owner_viewport_id = egui::ViewportId::ROOT;
        let mut owner_pass = None;
        let mut owner_pass_accepted = false;
        let mut callback_observed = false;
        let mut callback_class = egui::ViewportClass::Root;
        let mut native_viewport = None;
        let mut os_close_requested = false;
        let mut focus_host = false;
        let mut embedded_root_layer = None;
        let mut embedded_root_focused_id = None;

        if host.host == UtilityPresentation::Popout {
            ctx.show_viewport_immediate(
                studio_viewport_id(),
                studio_viewport_builder(),
                |ui, class| {
                    callback_observed = true;
                    callback_class = class;
                    let child_context = ui.ctx().clone();
                    let independent = class == egui::ViewportClass::Immediate
                        && child_context.viewport_id() == studio_viewport_id();
                    owner_context = child_context.clone();
                    owner_viewport_id = if independent {
                        child_context.viewport_id()
                    } else {
                        egui::ViewportId::ROOT
                    };
                    let pass = ViewportPassProvenance::current(
                        &child_context,
                        child_context.viewport_id(),
                    );
                    let pass_authorized = first_ui_pass
                        && ui.current_pass_index() == 0
                        && !ctx.will_discard()
                        && !child_context.will_discard();
                    if independent {
                        native_viewport = Some(ui.input(|input| input.viewport().clone()));
                    }
                    #[cfg(windows)]
                    if pass_authorized {
                        child_context.input_mut(|input| {
                            super::native_keyboard::reserve_windows_system_menu_alt_space(
                                &mut input.events,
                            );
                            super::native_keyboard::reserve_windows_system_menu_alt_space(
                                &mut input.raw.events,
                            );
                        });
                    }

                    let modal_open =
                        child_context.memory(|memory| memory.top_modal_layer().is_some());
                    let focused = child_context.input(|input| input.focused);
                    let visible_body = independent || (ui.is_visible() && !ui.is_sizing_pass());
                    let surface_input_allowed = if independent {
                        application_input_allowed
                            && child_context.viewport_id() == studio_viewport_id()
                    } else {
                        root_input_allowed && child_context.viewport_id() == egui::ViewportId::ROOT
                    };
                    let actions_allowed = pass_authorized
                        && surface_input_allowed
                        && focused
                        && !modal_open
                        && visible_body;
                    let keyboard_allowed = actions_allowed
                        && (independent || focused_on_layer(&child_context, ui.layer_id()));
                    let close_by_os = independent
                        && child_context.input(|input| input.viewport().close_requested());
                    os_close_requested = close_by_os;
                    if pass_authorized && self.document.pending_transition().is_none() {
                        if close_by_os || self.close_requested {
                            intent = Some(StudioIntent::Close);
                        } else if keyboard_close_requested(&child_context, keyboard_allowed) {
                            intent = Some(StudioIntent::Close);
                        }
                    }
                    let body_allowed = actions_allowed && intent.is_none();
                    let can_change_host = body_allowed
                        && self.document.pending_transition().is_none()
                        && !self.close_requested;
                    if independent {
                        egui::CentralPanel::default().show(ui, |ui| {
                            host_request = self.draw_studio_body(
                                ui,
                                &child_context,
                                host.host,
                                body_allowed,
                                can_change_host,
                                &original_source,
                                &mut source,
                                &mut selection,
                                &mut selection_to_install,
                                &mut intent,
                                &mut next_mode,
                            );
                        });
                    } else {
                        host_request = self.draw_studio_body(
                            ui,
                            &child_context,
                            host.host,
                            body_allowed,
                            can_change_host,
                            &original_source,
                            &mut source,
                            &mut selection,
                            &mut selection_to_install,
                            &mut intent,
                            &mut next_mode,
                        );
                        if child_context.viewport_id() == egui::ViewportId::ROOT {
                            embedded_root_layer = Some(ui.layer_id());
                            embedded_root_focused_id =
                                child_context.memory(|memory| memory.focused());
                        }
                    }
                    focus_host = self.host_state.focus_pending()
                        && pass_authorized
                        && surface_input_allowed
                        && !modal_open
                        && visible_body;
                    owner_pass_accepted = pass_authorized;
                    if independent && pass_authorized {
                        owner_pass = Some(pass);
                    }
                },
            );
        } else if root_visible || self.close_requested {
            let content_size = ctx.content_rect().size();
            let min_size = STUDIO_MINIMUM_SIZE.min(content_size);
            let mut window_open = true;
            let id = egui::Id::new(STUDIO_WINDOW_ID);
            let output = egui::Window::new("Markdown Studio")
                .id(id)
                .enabled(root_input_allowed)
                .collapsible(false)
                .resizable(true)
                .default_size(STUDIO_DEFAULT_SIZE)
                .min_size(min_size)
                .max_size(content_size)
                .show_utility(
                    ctx,
                    "markdown-studio",
                    id,
                    Some(&mut window_open),
                    UtilityWindowSizeRecipe::outer(STUDIO_DEFAULT_SIZE)
                        .with_minimum(min_size)
                        .with_maximum(content_size),
                    |ui| {
                        callback_observed = true;
                        callback_class = egui::ViewportClass::Root;
                        owner_context = ui.ctx().clone();
                        let pass_authorized =
                            first_ui_pass && !ctx.will_discard() && !ui.ctx().will_discard();
                        #[cfg(windows)]
                        if pass_authorized {
                            ui.ctx().input_mut(|input| {
                                super::native_keyboard::reserve_windows_system_menu_alt_space(
                                    &mut input.events,
                                );
                                super::native_keyboard::reserve_windows_system_menu_alt_space(
                                    &mut input.raw.events,
                                );
                            });
                        }
                        let visible_body = ui.is_visible() && !ui.is_sizing_pass();
                        let modal_open =
                            ui.ctx().memory(|memory| memory.top_modal_layer().is_some());
                        let focused = ui.ctx().input(|input| input.focused);
                        let actions_allowed = pass_authorized
                            && root_input_allowed
                            && ui.ctx().viewport_id() == egui::ViewportId::ROOT
                            && focused
                            && !modal_open
                            && visible_body;
                        let keyboard_allowed =
                            actions_allowed && focused_on_layer(ui.ctx(), ui.layer_id());
                        if pass_authorized && self.document.pending_transition().is_none() {
                            if self.close_requested {
                                intent = Some(StudioIntent::Close);
                            } else if keyboard_close_requested(ui.ctx(), keyboard_allowed) {
                                intent = Some(StudioIntent::Close);
                            }
                        }
                        owner_pass_accepted = pass_authorized;
                        let body_allowed = actions_allowed && intent.is_none();
                        let can_change_host = body_allowed
                            && self.document.pending_transition().is_none()
                            && !self.close_requested;
                        let owner_context = ui.ctx().clone();
                        host_request = self.draw_studio_body(
                            ui,
                            &owner_context,
                            host.host,
                            body_allowed,
                            can_change_host,
                            &original_source,
                            &mut source,
                            &mut selection,
                            &mut selection_to_install,
                            &mut intent,
                            &mut next_mode,
                        );
                    },
                );
            if output.is_some() && !window_open && owner_pass_accepted {
                intent = Some(StudioIntent::Close);
            }
        }

        if !callback_observed || !first_ui_pass {
            return;
        }
        let source_changed = source != original_source;
        if source_changed
            || intent.is_some()
            || host_request.is_some()
            || next_mode.is_some()
            || selection != self.selection
            || selection_to_install.is_some()
            || focus_editor_after_open
            || os_close_requested
            || focus_host
            || self.host_state.bootstrap_pending()
            || self.host_state.focus_pending()
            || callback_class == egui::ViewportClass::Immediate && native_viewport.is_some()
        {
            self.pending_effects = Some(RootEffects {
                root_pass,
                host,
                owner_pass,
                owner_viewport_id,
                owner_pass_accepted,
                owner_context,
                callback_observed,
                callback_class,
                native_viewport,
                os_close_requested,
                focus_host,
                embedded_root_layer,
                embedded_root_focused_id,
                source: source_changed.then_some(source),
                selection: (selection != self.selection).then_some(selection),
                selection_to_install,
                focus_editor_after_open,
                view_mode: next_mode,
                intent,
                host_request,
            });
        }
    }

    /// Commit once after the root retained-frame check. Source edits are
    /// applied before chooser, transition, export, or Diagram-copy intents.
    pub(crate) fn on_ui_frame_complete(
        &mut self,
        ctx: &egui::Context,
    ) -> (Option<MarkdownStudioEffect>, bool) {
        let Some(effects) = self.pending_effects.take() else {
            return (None, false);
        };
        let owner_pass_retained = effects.owner_pass_accepted
            && effects.callback_observed
            && match effects.owner_pass {
                Some(pass) => {
                    effects.host.host == UtilityPresentation::Popout
                        && effects.owner_viewport_id == pass.viewport_id
                        && pass.viewport_id == studio_viewport_id()
                        && pass.was_retained(ctx)
                }
                None => effects.owner_viewport_id == egui::ViewportId::ROOT,
            };
        if !effects.root_pass.was_retained(ctx)
            || effects.host != self.host_state.identity()
            || !owner_pass_retained
        {
            if effects.os_close_requested {
                ctx.send_viewport_cmd_to(studio_viewport_id(), egui::ViewportCommand::CancelClose);
            }
            return (None, false);
        }
        if effects.os_close_requested {
            ctx.send_viewport_cmd_to(studio_viewport_id(), egui::ViewportCommand::CancelClose);
        }
        self.host_state.complete_bootstrap(
            effects.host,
            effects.embedded_root_layer,
            effects.embedded_root_focused_id,
        );
        if effects.focus_host {
            if effects.callback_class == egui::ViewportClass::Immediate {
                ctx.send_viewport_cmd_to(studio_viewport_id(), egui::ViewportCommand::Focus);
            }
            self.host_state.complete_focus(effects.host);
        }
        let restore_native_geometry = !effects.os_close_requested
            && !matches!(effects.intent.as_ref(), Some(StudioIntent::Close))
            && effects.host_request != Some(UtilityPresentation::Docked);
        if let (Some(identity), Some(viewport_info)) = (
            native_geometry_identity(effects.host),
            effects.native_viewport.as_ref(),
        ) && effects.callback_class == egui::ViewportClass::Immediate
            && restore_native_geometry
        {
            super::utility_dialog_size::apply_native_utility_geometry_restore(
                ctx,
                effects.callback_class,
                true,
                "Markdown Studio",
                identity,
                viewport_info,
                STUDIO_DEFAULT_SIZE,
                Some(STUDIO_MINIMUM_SIZE),
                None,
            );
        }
        if let Some(mode) = effects.view_mode {
            self.mode = mode;
        }
        if let Some(source) = effects.source {
            match self.document.replace_source(source) {
                Ok(_) => {
                    self.cache = None;
                    self.message = None;
                }
                Err(error) => {
                    self.set_error(error.to_string());
                    return (None, false);
                }
            }
        }
        if let Some(selection) = effects.selection {
            self.selection = selection;
        }
        if let Some(selection) = effects.selection_to_install {
            self.selection = selection.clone();
            self.deferred_selection = Some(selection);
        }
        if effects.focus_editor_after_open {
            self.open_focus_pending = false;
            self.deferred_selection = Some(self.selection.clone());
            ctx.request_repaint_of(effects.owner_viewport_id);
        }
        let mut diagram_effect = None;
        let has_intent = effects.intent.is_some();
        if let Some(intent) = effects.intent {
            if matches!(&intent, StudioIntent::Close) {
                // Consume a host/title/keyboard close only after this frame was
                // retained. If it is discarded, the request remains available
                // for the next accepted frame.
                self.close_requested = false;
            }
            let native_identity = (effects.callback_class == egui::ViewportClass::Immediate)
                .then(|| native_geometry_identity(effects.host))
                .flatten();
            #[cfg(windows)]
            {
                self.picker_owner = Some((effects.owner_context.clone(), native_identity));
            }
            diagram_effect = self.apply_intent(intent, &effects.owner_context);
            #[cfg(windows)]
            {
                self.picker_owner = None;
            }
        }
        if let Some(MarkdownStudioEffect::OpenDiagram { origin, .. }) = diagram_effect.as_mut() {
            origin.host_origin = super::utility_host::AcceptedParentOpen::from_retained(
                ctx, effects.host,
                (self.host_epoch, self.document.path().map(Path::to_path_buf), self.document.revision()),
                effects.root_pass, effects.owner_pass.unwrap_or(effects.root_pass),
            );
        }
        if !self.open {
            let key = crate::settings::UtilityToolKey::parse("markdown-studio").expect("stable Markdown Studio key");
            super::utility_host::remember_presentation(ctx, key, effects.host.host);
            self.presentation_prepared = false;
            if let Some(identity) = native_geometry_identity(effects.host) {
                super::utility_dialog_size::close_native_utility_geometry_viewport(ctx, identity);
                ctx.send_viewport_cmd_to(studio_viewport_id(), egui::ViewportCommand::Close);
            }
            self.host_state.retire_to_docked();
            ctx.request_repaint_of(egui::ViewportId::ROOT);
            return (diagram_effect, true);
        }

        if !has_intent
            && let Some(requested) = effects.host_request
            && self.document.pending_transition().is_none()
            && requested != self.host_state.presentation()
        {
            let identity = native_geometry_identity(effects.host);
            if requested == UtilityPresentation::Docked {
                if let Some(identity) = identity {
                    super::utility_dialog_size::close_native_utility_geometry_viewport(
                        ctx, identity,
                    );
                }
                ctx.send_viewport_cmd_to(studio_viewport_id(), egui::ViewportCommand::Close);
            }
            if self.host_state.transition(requested) {
                let key = crate::settings::UtilityToolKey::parse("markdown-studio").expect("stable Markdown Studio key");
                super::utility_host::remember_presentation(ctx, key, requested);
                self.deferred_selection = Some(self.selection.clone());
                if requested == UtilityPresentation::Docked {
                    self.show_root_after_host_change = true;
                }
                ctx.request_repaint_of(egui::ViewportId::ROOT);
                if requested == UtilityPresentation::Popout {
                    ctx.request_repaint_of(studio_viewport_id());
                }
            }
        }
        (diagram_effect, false)
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_studio_body(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        presentation: UtilityPresentation,
        interaction_allowed: bool,
        host_change_allowed: bool,
        original_source: &str,
        source: &mut String,
        selection: &mut Range<usize>,
        selection_to_install: &mut Option<Range<usize>>,
        intent: &mut Option<StudioIntent>,
        next_mode: &mut Option<ViewMode>,
    ) -> Option<UtilityPresentation> {
        let mut host_request = None;
        ui.scope_builder(
            egui::UiBuilder::new().id(egui::Id::new(STUDIO_WINDOW_ID)),
            |ui| {
                let allowed = interaction_allowed;
                ui.add_enabled_ui(allowed, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("markdown-studio-workspace")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            self.header_ui(
                                ui,
                                presentation,
                                host_change_allowed,
                                &mut host_request,
                                source,
                                selection,
                                selection_to_install,
                                intent,
                                next_mode,
                            );
                            self.message_ui(ui);
                            self.transition_ui(ui, intent);
                            ui.separator();
                            let pending = self.document.pending_transition().is_some();
                            let current_mode = next_mode.unwrap_or(self.mode);
                            let width = ui.available_width();
                            if current_mode == ViewMode::Source {
                                self.editor_panel(
                                    ui,
                                    ctx,
                                    source,
                                    selection,
                                    selection_to_install,
                                    intent,
                                    allowed && !pending,
                                );
                            } else if current_mode == ViewMode::Preview {
                                self.preview_panel(
                                    ui,
                                    intent,
                                    allowed && !pending,
                                    original_source,
                                );
                            } else if width >= 880.0 {
                                ui.columns(2, |columns| {
                                    self.editor_panel(
                                        &mut columns[0],
                                        ctx,
                                        source,
                                        selection,
                                        selection_to_install,
                                        intent,
                                        allowed && !pending,
                                    );
                                    self.preview_panel(
                                        &mut columns[1],
                                        intent,
                                        allowed && !pending,
                                        original_source,
                                    );
                                });
                            } else {
                                self.editor_panel(
                                    ui,
                                    ctx,
                                    source,
                                    selection,
                                    selection_to_install,
                                    intent,
                                    allowed && !pending,
                                );
                                ui.separator();
                                self.preview_panel(
                                    ui,
                                    intent,
                                    allowed && !pending,
                                    original_source,
                                );
                            }
                        });
                });
            },
        );
        host_request
    }

    fn header_ui(
        &mut self,
        ui: &mut egui::Ui,
        presentation: UtilityPresentation,
        host_change_allowed: bool,
        host_request: &mut Option<UtilityPresentation>,
        source: &mut String,
        selection: &mut Range<usize>,
        selection_to_install: &mut Option<Range<usize>>,
        intent: &mut Option<StudioIntent>,
        mode: &mut Option<ViewMode>,
    ) {
        let pending = self.document.pending_transition().is_some();
        ui.horizontal_wrapped(|ui| {
            for (label, action) in [
                ("New", StudioIntent::New),
                ("Open…", StudioIntent::Open),
                ("Save", StudioIntent::Save),
                ("Save As…", StudioIntent::SaveAs),
            ] {
                let response = ui.add_enabled(!pending, egui::Button::new(label));
                #[cfg(test)]
                if label == "Save As…" && ui.is_visible() && !ui.is_sizing_pass() {
                    self.test_save_as_id = Some(response.id);
                    self.test_save_as_rect = Some(response.rect);
                }
                if response.clicked() {
                    *intent = Some(action);
                }
            }
            let _export_menu = ui.menu_button("Export", |ui| {
                if ui.button("Markdown (.md)").clicked() {
                    *intent = Some(StudioIntent::Export(MarkdownExportFormat::Markdown));
                    ui.close();
                }
                if ui.button("Static HTML (.html)").clicked() {
                    *intent = Some(StudioIntent::Export(MarkdownExportFormat::Html));
                    ui.close();
                }
            });
            #[cfg(test)]
            if ui.is_visible() && !ui.is_sizing_pass() {
                self.test_export_id = Some(_export_menu.response.id);
            }
            ui.separator();
            let title = self
                .document
                .path()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                .unwrap_or("Untitled");
            ui.strong(title);
            if self.document.is_dirty() {
                ui.colored_label(ui.visuals().warn_fg_color, "Unsaved changes");
            }
            let (control, requested) =
                presentation_button(ui, presentation, host_change_allowed && !pending);
            #[cfg(test)]
            if ui.is_visible() && !ui.is_sizing_pass() {
                self.test_presentation_id = Some(control.id);
                self.test_presentation_rect = Some(control.rect);
                self.test_presentation_clip_rect = Some(ui.clip_rect());
                self.test_presentation_enabled = Some(host_change_allowed && !pending);
            }
            if control.clicked() {
                *host_request = Some(requested);
            }
        });
        ui.horizontal_wrapped(|ui| {
            for (view, label) in [
                (ViewMode::Source, "Edit"),
                (ViewMode::Preview, "Preview"),
                (ViewMode::Split, "Split"),
            ] {
                let response = ui.selectable_label(mode.unwrap_or(self.mode) == view, label);
                #[cfg(test)]
                if view == ViewMode::Source && ui.is_visible() && !ui.is_sizing_pass() {
                    self.test_edit_id = Some(response.id);
                }
                if response.clicked() {
                    *mode = Some(view);
                }
            }
            ui.separator();
            if ui
                .add_enabled(self.document.can_undo(), egui::Button::new("Undo"))
                .clicked()
            {
                *intent = Some(StudioIntent::Undo);
            }
            if ui
                .add_enabled(self.document.can_redo(), egui::Button::new("Redo"))
                .clicked()
            {
                *intent = Some(StudioIntent::Redo);
            }
            ui.separator();
            self.authoring_tools(ui, source, selection, selection_to_install);
        });
    }

    fn authoring_tools(
        &mut self,
        ui: &mut egui::Ui,
        source: &mut String,
        selection: &mut Range<usize>,
        selection_to_install: &mut Option<Range<usize>>,
    ) {
        let pending = self.document.pending_transition().is_some();
        ui.add_enabled_ui(!pending, |ui| {
            let _format_menu = ui.menu_button("Format", |ui| {
                for (label, format) in [
                    ("Bold", MarkdownInlineFormat::Strong),
                    ("Italic", MarkdownInlineFormat::Emphasis),
                    ("Strike", MarkdownInlineFormat::Strike),
                    ("Inline code", MarkdownInlineFormat::Code),
                    (
                        "Link…",
                        MarkdownInlineFormat::Link {
                            destination: "https://".into(),
                        },
                    ),
                    (
                        "Image…",
                        MarkdownInlineFormat::Image {
                            destination: "images/".into(),
                        },
                    ),
                ] {
                    let response = ui.button(label);
                    #[cfg(test)]
                    if label == "Bold" && ui.is_visible() && !ui.is_sizing_pass() {
                        self.test_bold_rect = Some(response.rect);
                    }
                    if response.clicked() {
                        if let Err(error) = apply_prepared(
                            source,
                            selection,
                            selection_to_install,
                            prepare_inline_format(source, selection.clone(), format),
                        ) {
                            self.set_error(error);
                        }
                        ui.close();
                    }
                }
            });
            #[cfg(test)]
            {
                if ui.is_visible() && !ui.is_sizing_pass() {
                    self.test_format_menu_rect = Some(_format_menu.response.rect);
                }
            }
            ui.menu_button("Insert", |ui| {
                for level in 1..=3 {
                    if ui.button(format!("Heading {level}")).clicked() {
                        if let Err(error) = apply_prepared(
                            source,
                            selection,
                            selection_to_install,
                            prepare_block_snippet(
                                source,
                                selection.clone(),
                                MarkdownBlockSnippet::Heading { level },
                            ),
                        ) {
                            self.set_error(error);
                        }
                        ui.close();
                    }
                }
                for (label, snippet) in [
                    ("Quote", MarkdownBlockSnippet::Quote),
                    ("Bullet", MarkdownBlockSnippet::Bullet),
                    ("Task", MarkdownBlockSnippet::Task { checked: false }),
                    (
                        "Code fence",
                        MarkdownBlockSnippet::CodeFence {
                            language: String::new(),
                        },
                    ),
                    ("Wiki link", MarkdownBlockSnippet::WikiLink),
                ] {
                    if ui.button(label).clicked() {
                        if let Err(error) = apply_prepared(
                            source,
                            selection,
                            selection_to_install,
                            prepare_block_snippet(source, selection.clone(), snippet),
                        ) {
                            self.set_error(error);
                        }
                        ui.close();
                    }
                }
                if ui.button("2-column table").clicked() {
                    let headers = vec!["Column 1".to_owned(), "Column 2".to_owned()];
                    if let Err(error) = apply_prepared(
                        source,
                        selection,
                        selection_to_install,
                        prepare_insert_table(source, selection.start, &headers),
                    ) {
                        self.set_error(error);
                    }
                    ui.close();
                }
            });
            self.table_tools(ui, source, selection, selection_to_install);
        });
    }

    fn table_tools(
        &mut self,
        ui: &mut egui::Ui,
        source: &mut String,
        selection: &mut Range<usize>,
        selection_to_install: &mut Option<Range<usize>>,
    ) {
        let Ok(Some(table)) = table_at_byte(source, selection.start) else {
            return;
        };
        let column_count = table.alignments().len();
        let rows = table.rows().len();
        ui.menu_button("Table", |ui| {
            if rows > 0 && ui.button("Insert row below").clicked() {
                let cells = vec![String::new(); column_count];
                if let Err(error) = apply_prepared(
                    source,
                    selection,
                    selection_to_install,
                    prepare_insert_row(source, &table, rows - 1, &cells),
                ) {
                    self.set_error(error);
                }
                ui.close();
            }
            if ui.button("Insert column").clicked() {
                if let Err(error) = apply_prepared(
                    source,
                    selection,
                    selection_to_install,
                    prepare_insert_column(source, &table, column_count, "Column"),
                ) {
                    self.set_error(error);
                }
                ui.close();
            }
            if rows > 1 && ui.button("Remove last row").clicked() {
                if let Err(error) = apply_prepared(
                    source,
                    selection,
                    selection_to_install,
                    prepare_remove_row(source, &table, rows - 1),
                ) {
                    self.set_error(error);
                }
                ui.close();
            }
            if column_count > 1 && ui.button("Remove last column").clicked() {
                if let Err(error) = apply_prepared(
                    source,
                    selection,
                    selection_to_install,
                    prepare_remove_column(source, &table, column_count - 1),
                ) {
                    self.set_error(error);
                }
                ui.close();
            }
            if rows > 0 && column_count > 0 && ui.button("Set first cell").clicked() {
                if let Err(error) = apply_prepared(
                    source,
                    selection,
                    selection_to_install,
                    prepare_set_cell(source, &table, 0, 0, "Cell"),
                ) {
                    self.set_error(error);
                }
                ui.close();
            }
        });
    }

    fn editor_panel(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        source: &mut String,
        selection: &mut Range<usize>,
        selection_to_install: &mut Option<Range<usize>>,
        intent: &mut Option<StudioIntent>,
        enabled: bool,
    ) {
        ui.heading("Markdown source");
        let id = ui.make_persistent_id(SOURCE_EDITOR_ID);
        if enabled && ui.memory(|memory| memory.has_focus(id)) {
            ui.input_mut(|input| {
                if input.consume_key(egui::Modifiers::COMMAND, egui::Key::N) {
                    *intent = Some(StudioIntent::New);
                } else if input.consume_key(egui::Modifiers::COMMAND, egui::Key::O) {
                    *intent = Some(StudioIntent::Open);
                } else if input.consume_key(
                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                    egui::Key::S,
                ) {
                    *intent = Some(StudioIntent::SaveAs);
                } else if input.consume_key(egui::Modifiers::COMMAND, egui::Key::S) {
                    *intent = Some(StudioIntent::Save);
                } else if input.consume_key(
                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                    egui::Key::Z,
                ) || input.consume_key(egui::Modifiers::COMMAND, egui::Key::Y)
                {
                    *intent = Some(StudioIntent::Redo);
                } else if input.consume_key(egui::Modifiers::COMMAND, egui::Key::Z) {
                    *intent = Some(StudioIntent::Undo);
                }
            });
        }
        if let Some(range) = self.deferred_selection.take() {
            install_selection(ctx, id, source, range);
        }
        let _response = ui
            .add_enabled_ui(enabled, |ui| {
                egui::ScrollArea::both()
                    .id_salt("markdown-studio-source-scroll")
                    .max_height(600.0)
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(source)
                                .id(id)
                                .desired_rows(18)
                                .desired_width(ui.available_width())
                                .font(egui::TextStyle::Monospace)
                                .hint_text("# Notes\n\nWrite Markdown here…"),
                        )
                    })
                    .inner
            })
            .inner;
        #[cfg(test)]
        {
            self.test_editor_id = Some(id);
            self.test_editor_rect = Some(_response.rect);
        }
        // egui may collapse a TextEdit's mechanical cursor when a toolbar
        // button takes focus. Keep the dialog's source-bound logical
        // selection in that case; only a focused editor can author a new one.
        let pointer_click_outside_editor = ui.input(|input| {
            (input.pointer.any_pressed() || input.pointer.any_released())
                && input
                    .pointer
                    .interact_pos()
                    .is_some_and(|position| !_response.interact_rect.contains(position))
        }) && !_response.dragged()
            && !_response.changed();
        if !pointer_click_outside_editor
            && ui.memory(|memory| memory.has_focus(id))
            && let Some(range) = cursor_range_to_bytes(ctx, id, source)
        {
            *selection = range;
        }
        if let Some(analysis) = self.cache.as_ref().and_then(MarkdownPreviewCache::analysis)
            && !analysis.headings.is_empty()
        {
            ui.collapsing("Outline", |ui| {
                for heading in &analysis.headings {
                    if ui
                        .selectable_label(
                            heading.byte_range.contains(&selection.start),
                            &heading.title,
                        )
                        .clicked()
                    {
                        *selection_to_install = Some(heading.byte_range.clone());
                        *selection = heading.byte_range.clone();
                    }
                }
            });
        }
    }

    fn preview_panel(
        &mut self,
        ui: &mut egui::Ui,
        intent: &mut Option<StudioIntent>,
        enabled: bool,
        source_snapshot: &str,
    ) {
        ui.heading("Preview");
        if let Some(cache) = self.cache.as_ref() {
            cache.draw(ui, &mut self.commonmark_cache, &self.image_loader);
            let base_directory = self.document.path().and_then(Path::parent);
            if cache.matches(
                self.document.revision().unwrap_or(0),
                source_snapshot,
                base_directory,
            ) {
                for (index, _) in cache.fences().iter().enumerate() {
                    if ui
                        .add_enabled(
                            enabled,
                            egui::Button::new(format!("Open Diagram {}", index + 1)),
                        )
                        .clicked()
                    {
                        *intent = Some(StudioIntent::CopyDiagram {
                            index,
                            source_snapshot: source_snapshot.to_owned(),
                        });
                    }
                }
            }
        } else {
            ui.weak("Preview unavailable; source remains editable.");
        }
    }

    fn transition_ui(&mut self, ui: &mut egui::Ui, intent: &mut Option<StudioIntent>) {
        let Some(pending) = self.document.pending_transition() else {
            return;
        };
        let description = match pending {
            PendingMarkdownTransition::New => "Create a new document?",
            PendingMarkdownTransition::Open(_) => "Open another document?",
            PendingMarkdownTransition::Close => "Close Markdown Studio?",
        };
        ui.group(|ui| {
            ui.strong("Unsaved changes");
            ui.label(description);
            ui.horizontal_wrapped(|ui| {
                if ui.button("Save").clicked() {
                    *intent = Some(StudioIntent::Decision(MarkdownTransitionDecision::Save));
                }
                let discard = ui.button("Discard");
                #[cfg(test)]
                if ui.is_visible() && !ui.is_sizing_pass() {
                    self.test_discard_id = Some(discard.id);
                }
                if discard.clicked() {
                    *intent = Some(StudioIntent::Decision(MarkdownTransitionDecision::Discard));
                }
                if ui.button("Cancel").clicked() {
                    *intent = Some(StudioIntent::Decision(MarkdownTransitionDecision::Cancel));
                }
            });
        });
    }

    fn message_ui(&mut self, ui: &mut egui::Ui) {
        if let Some((error, message)) = self.message.as_ref() {
            ui.colored_label(
                if *error {
                    ui.visuals().error_fg_color
                } else {
                    ui.visuals().warn_fg_color
                },
                message,
            );
        }
    }

    fn ensure_preview(&mut self, ctx: &egui::Context, revision: u64, source: &str) {
        let base_directory = self
            .document
            .path()
            .and_then(Path::parent)
            .map(Path::to_path_buf);
        if self
            .cache
            .as_ref()
            .is_some_and(|cache| cache.matches(revision, source, base_directory.as_deref()))
        {
            return;
        }
        self.preview_generation = self.preview_generation.saturating_add(1);
        let generation = self.preview_generation;
        let local_assets = LocalAssets::new(base_directory.as_deref(), source);
        self.cache = match MarkdownPreviewCache::new(
            ctx,
            revision,
            generation,
            source,
            base_directory.as_deref(),
            Some(&local_assets),
            &self.image_loader,
        ) {
            Ok(cache) => Some(cache),
            Err(error) => {
                self.set_error(format!("Could not update Markdown preview: {error}"));
                None
            }
        };
    }

    fn apply_intent(
        &mut self,
        intent: StudioIntent,
        ctx: &egui::Context,
    ) -> Option<MarkdownStudioEffect> {
        match intent {
            StudioIntent::New => match self.document.request_new() {
                Ok(outcome) => self.finish_transition(outcome, false),
                Err(error) => self.set_error(error.to_string()),
            },
            StudioIntent::Open => {
                if let Some(path) = self.pick_markdown_file(false) {
                    match self.document.request_open(&path) {
                        Ok(outcome) => self.finish_transition(outcome, false),
                        Err(error) => {
                            self.set_error(format!("Could not open {}: {error}", path.display()))
                        }
                    }
                }
            }
            StudioIntent::Save => self.save_current(),
            StudioIntent::SaveAs => self.save_as_current(),
            StudioIntent::Export(format) => self.export_current(format),
            StudioIntent::Close => match self.document.request_close() {
                Ok(outcome) => self.finish_transition(outcome, true),
                Err(error) => self.set_error(error.to_string()),
            },
            StudioIntent::Decision(decision) => {
                let closes =
                    self.document.pending_transition() == Some(&PendingMarkdownTransition::Close);
                match self.document.resolve_pending(decision) {
                    Ok(MarkdownTransitionOutcome::SaveAsRequired)
                        if decision == MarkdownTransitionDecision::Save =>
                    {
                        if let Some(path) = self.pick_markdown_file(true) {
                            match self.document.resolve_pending_save_as(path) {
                                Ok(outcome) => self.finish_transition(outcome, closes),
                                Err(error) => self.set_error(error.to_string()),
                            }
                        }
                    }
                    Ok(outcome) => self.finish_transition(outcome, closes),
                    Err(error) => self.set_error(error.to_string()),
                }
            }
            StudioIntent::CopyDiagram {
                index,
                source_snapshot,
            } => {
                if self.document.source() != Some(source_snapshot.as_str()) {
                    self.set_error(
                        "The Markdown source changed before this Mermaid block could be opened."
                            .into(),
                    );
                    return None;
                }
                let current_revision = self.document.revision().unwrap_or(0);
                let Some(cache) = self.cache.as_ref().filter(|cache| {
                    cache.matches(
                        current_revision,
                        &source_snapshot,
                        self.document.path().and_then(Path::parent),
                    )
                }) else {
                    self.set_error(
                        "The Markdown preview changed before this Mermaid block could be opened."
                            .into(),
                    );
                    return None;
                };
                let Some(raw) = cache.selected_fence_source(index).map(str::to_owned) else {
                    self.set_error("The selected Mermaid block is no longer available.".into());
                    return None;
                };
                match DiagramSourceCopy::new(raw) {
                    Ok(copy) => {
                        let identity = DiagramSourceCopyOrigin::fresh();
                        let origin = MarkdownFenceOrigin {
                            identity: identity.clone(),
                            source_snapshot,
                            fence_index: index,
                            host_origin: None,
                        };
                        return Some(MarkdownStudioEffect::OpenDiagram {
                            source: copy.with_origin(identity),
                            origin,
                        });
                    }
                    Err(error) => self.set_error(error.to_string()),
                }
            }
            StudioIntent::Undo => match self.document.undo() {
                Ok(true) => self.cache = None,
                Ok(false) => {}
                Err(error) => self.set_error(error.to_string()),
            },
            StudioIntent::Redo => match self.document.redo() {
                Ok(true) => self.cache = None,
                Ok(false) => {}
                Err(error) => self.set_error(error.to_string()),
            },
        }
        ctx.request_repaint();
        None
    }

    fn finish_transition(&mut self, outcome: MarkdownTransitionOutcome, closes: bool) {
        match outcome {
            MarkdownTransitionOutcome::Applied => {
                self.host_epoch = self.host_epoch.wrapping_add(1);
                self.cache = None;
                self.selection = 0..0;
                self.deferred_selection = Some(0..0);
                self.message = None;
                self.open = !closes && self.document.source().is_some();
                self.active_diagram_origin = None;
                self.pending_diagram_origin = None;
            }
            MarkdownTransitionOutcome::DecisionRequired => self.message = None,
            MarkdownTransitionOutcome::Cancelled => {
                self.open = true;
                self.message = Some((
                    false,
                    "Document action cancelled; the draft is unchanged.".into(),
                ));
            }
            MarkdownTransitionOutcome::SaveAsRequired => {
                self.message = Some((false, "Choose a destination to continue saving.".into()))
            }
        }
    }

    fn save_current(&mut self) {
        match self.document.save() {
            Ok(MarkdownSaveOutcome::Saved) => {
                self.message = Some((false, "Markdown document saved.".into()))
            }
            Ok(MarkdownSaveOutcome::SaveAsRequired) => self.save_as_current(),
            Err(error) => self.set_error(error.to_string()),
        }
    }

    fn save_as_current(&mut self) {
        if let Some(path) = self.pick_markdown_file(true) {
            match self.document.save_as(&path) {
                Ok(MarkdownSaveOutcome::Saved) => {
                    self.message = Some((false, format!("Saved {}.", path.display())))
                }
                Ok(MarkdownSaveOutcome::SaveAsRequired) => {
                    self.set_error("Save As requires an active Markdown document.".into())
                }
                Err(error) => self.set_error(error.to_string()),
            }
        }
    }

    fn export_current(&mut self, format: MarkdownExportFormat) {
        let source = self.document.source().unwrap_or_default().to_owned();
        let artifact = match format {
            MarkdownExportFormat::Markdown => export_markdown(&source),
            MarkdownExportFormat::Html => {
                let preview = match MarkdownPreview::analyze(&source) {
                    Ok(preview) => preview,
                    Err(error) => {
                        self.set_error(error.to_string());
                        return;
                    }
                };
                let assets = LocalAssets::new(self.document.path().and_then(Path::parent), &source);
                export_html(&preview, Some(&assets))
            }
        };
        let artifact = match artifact {
            Ok(artifact) => artifact,
            Err(error) => {
                self.set_error(format!("Could not prepare export: {error}"));
                return;
            }
        };
        if let Some(path) = self.pick_export_path(format) {
            match publish_export(&artifact, &path, self.document.path()) {
                Ok(()) => {
                    self.message = Some((
                        false,
                        format!(
                            "Exported {} bytes to {}.",
                            artifact.bytes().len(),
                            path.display()
                        ),
                    ))
                }
                Err(error) => self.set_error(error),
            }
        }
    }

    #[cfg(not(test))]
    fn owned_file_dialog(&mut self) -> Option<rfd::FileDialog> {
        #[cfg(windows)]
        {
            let Some((ctx, identity)) = self.picker_owner.as_ref() else {
                self.set_error(
                    "Markdown Studio could not identify the current window for its file dialog. Focus the Studio and try again.".into(),
                );
                return None;
            };
            let hwnd = super::utility_dialog_size::utility_picker_hwnd(ctx, *identity)
                .and_then(|hwnd| std::num::NonZeroIsize::new(hwnd as isize));
            let Some(hwnd) = hwnd else {
                self.set_error(
                    "The current Markdown Studio window is not ready to own the file dialog. Focus it and try again.".into(),
                );
                return None;
            };
            Some(
                rfd::FileDialog::new()
                    .set_parent(&super::add_action_dialog::NativeDialogParent(hwnd)),
            )
        }
        #[cfg(not(windows))]
        {
            Some(rfd::FileDialog::new())
        }
    }

    fn pick_markdown_file(&mut self, save: bool) -> Option<PathBuf> {
        #[cfg(test)]
        {
            let _ = save;
            self.test_picker_calls = self.test_picker_calls.saturating_add(1);
            return self
                .test_next_picker_result
                .take()
                .unwrap_or(None)
                .map(|path| ensure_extension(path, "md"));
        }
        #[cfg(not(test))]
        {
            let mut picker = self
                .owned_file_dialog()?
                .add_filter("Markdown (*.md)", &["md"]);
            let directory = self.picker_directory();
            if directory.is_dir() {
                picker = picker.set_directory(directory);
            }
            if save {
                let name = self
                    .document
                    .path()
                    .and_then(Path::file_name)
                    .unwrap_or_else(|| std::ffi::OsStr::new("untitled.md"));
                picker
                    .set_file_name(name.to_string_lossy().into_owned())
                    .save_file()
                    .map(|path| ensure_extension(path, "md"))
            } else {
                picker.pick_file()
            }
        }
    }

    fn pick_export_path(&mut self, format: MarkdownExportFormat) -> Option<PathBuf> {
        #[cfg(test)]
        {
            self.test_picker_calls = self.test_picker_calls.saturating_add(1);
            return self
                .test_next_picker_result
                .take()
                .unwrap_or(None)
                .map(|path| {
                    ensure_extension(
                        path,
                        match format {
                            MarkdownExportFormat::Markdown => "md",
                            MarkdownExportFormat::Html => "html",
                        },
                    )
                });
        }
        #[cfg(not(test))]
        {
            let extension = match format {
                MarkdownExportFormat::Markdown => "md",
                MarkdownExportFormat::Html => "html",
            };
            let picker = self.owned_file_dialog()?;
            let stem = self
                .document
                .path()
                .and_then(Path::file_stem)
                .unwrap_or_else(|| std::ffi::OsStr::new("untitled"))
                .to_string_lossy();
            let mut picker = picker
                .add_filter("Markdown export", &[extension])
                .set_file_name(format!("{stem}.{extension}"));
            let directory = self.picker_directory();
            if directory.is_dir() {
                picker = picker.set_directory(directory);
            }
            picker
                .save_file()
                .map(|path| ensure_extension(path, extension))
        }
    }

    fn picker_directory(&self) -> &Path {
        self.document
            .path()
            .and_then(Path::parent)
            .filter(|path| path.is_dir())
            .unwrap_or(&self.default_directory)
    }

    fn reset_presentation(&mut self) {
        self.cache = None;
        self.selection = 0..0;
        self.mode = ViewMode::Split;
        self.message = None;
    }

    fn set_error(&mut self, error: String) {
        self.message = Some((true, error));
    }

    #[cfg(test)]
    pub(crate) fn test_source(&self) -> Option<&str> {
        self.document.source()
    }

    #[cfg(test)]
    pub(crate) fn test_is_dirty(&self) -> bool {
        self.document.is_dirty()
    }

    #[cfg(test)]
    pub(crate) fn test_undo_depth(&self) -> usize {
        self.document.undo_depth()
    }

    #[cfg(test)]
    pub(crate) fn test_editor_id(&self) -> Option<egui::Id> {
        self.test_editor_id
    }

    #[cfg(test)]
    pub(crate) fn test_save_as_id(&self) -> Option<egui::Id> {
        self.test_save_as_id
    }

    #[cfg(test)]
    pub(crate) fn test_save_as_rect(&self) -> Option<egui::Rect> {
        self.test_save_as_rect
    }

    #[cfg(test)]
    pub(crate) fn test_export_id(&self) -> Option<egui::Id> {
        self.test_export_id
    }

    #[cfg(test)]
    pub(crate) fn test_edit_id(&self) -> Option<egui::Id> {
        self.test_edit_id
    }

    #[cfg(test)]
    pub(crate) fn test_view_mode(&self) -> &'static str {
        match self.mode {
            ViewMode::Source => "source",
            ViewMode::Preview => "preview",
            ViewMode::Split => "split",
        }
    }

    #[cfg(test)]
    pub(crate) fn test_format_menu_rect(&self) -> Option<egui::Rect> {
        self.test_format_menu_rect
    }

    #[cfg(test)]
    pub(crate) fn test_bold_rect(&self) -> Option<egui::Rect> {
        self.test_bold_rect
    }

    #[cfg(test)]
    pub(crate) fn test_picker_calls(&self) -> usize {
        self.test_picker_calls
    }

    #[cfg(test)]
    pub(crate) fn test_discard_id(&self) -> Option<egui::Id> {
        self.test_discard_id
    }

    #[cfg(test)]
    pub(crate) fn test_replace_source(&mut self, source: &str) {
        self.document.replace_source(source).unwrap();
        self.cache = None;
    }

    #[cfg(test)]
    pub(crate) fn test_set_next_picker_result(&mut self, path: Option<PathBuf>) {
        self.test_next_picker_result = Some(path);
    }
}

fn apply_prepared(
    source: &mut String,
    selection: &mut Range<usize>,
    selection_to_install: &mut Option<Range<usize>>,
    result: Result<PreparedMarkdownEdit, impl std::fmt::Display>,
) -> Result<(), String> {
    let applied = result
        .map_err(|error| error.to_string())
        .and_then(|edit| edit.apply(source).map_err(|error| error.to_string()))?;
    *source = applied.source;
    *selection = applied.selection.clone();
    *selection_to_install = Some(applied.selection);
    Ok(())
}

fn install_selection(ctx: &egui::Context, id: egui::Id, source: &str, range: Range<usize>) {
    let mut state = egui::widgets::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::two(
            egui::text::CCursor::new(byte_to_char(source, range.start.min(source.len()))),
            egui::text::CCursor::new(byte_to_char(source, range.end.min(source.len()))),
        )));
    state.store(ctx, id);
    ctx.memory_mut(|memory| memory.request_focus(id));
}

fn keyboard_close_requested(ctx: &egui::Context, allowed: bool) -> bool {
    if !allowed {
        return false;
    }
    let popup_open = egui::Popup::is_any_open(ctx);
    ctx.input_mut(|input| {
        (!popup_open && input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
            || input.consume_key(egui::Modifiers::COMMAND, egui::Key::W)
    })
}

fn focused_on_layer(ctx: &egui::Context, layer: egui::LayerId) -> bool {
    ctx.memory(|memory| memory.focused()).is_some_and(|id| {
        ctx.read_response(id)
            .is_some_and(|response| response.layer_id == layer)
    })
}

fn cursor_range_to_bytes(ctx: &egui::Context, id: egui::Id, source: &str) -> Option<Range<usize>> {
    let state = egui::widgets::text_edit::TextEditState::load(ctx, id)?;
    let range = state.cursor.char_range()?.as_sorted_char_range();
    Some(char_to_byte(source, range.start.0)..char_to_byte(source, range.end.0))
}

fn char_to_byte(source: &str, index: usize) -> usize {
    source
        .char_indices()
        .nth(index)
        .map_or(source.len(), |(byte, _)| byte)
}

fn byte_to_char(source: &str, index: usize) -> usize {
    source[..index.min(source.len())].chars().count()
}

struct OwnedAsset {
    destination: String,
    bytes: Vec<u8>,
    format: crate::notes_markdown::LocalRasterFormat,
}

struct LocalAssets(Vec<OwnedAsset>);

impl LocalAssets {
    fn new(base: Option<&Path>, source: &str) -> Self {
        let Some(base) = base else {
            return Self(Vec::new());
        };
        let mut destinations = HashSet::new();
        for (event, _) in Parser::new_ext(source, Options::ENABLE_TABLES).into_offset_iter() {
            if let Event::Start(Tag::Image { dest_url, .. }) = event
                && destinations.len() < MAX_LOCAL_ASSET_COUNT
            {
                destinations.insert(dest_url.to_string());
            }
        }
        let Ok(base) = fs::canonicalize(base) else {
            return Self(Vec::new());
        };
        let mut total = 0usize;
        let mut assets = Vec::new();
        for destination in destinations {
            if destination.contains(':') || destination.starts_with(['/', '\\']) {
                continue;
            }
            let relative = Path::new(&destination);
            if relative
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
            {
                continue;
            }
            let Ok(path) = fs::canonicalize(base.join(relative)) else {
                continue;
            };
            if !path.starts_with(&base) {
                continue;
            }
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                continue;
            };
            if !metadata.file_type().is_file()
                || metadata.len() > crate::notes_markdown::MAX_LOCAL_RASTER_BYTES as u64
            {
                continue;
            }
            let Ok(bytes) = fs::read(&path) else { continue };
            if total.saturating_add(bytes.len()) > MAX_LOCAL_ASSET_AGGREGATE_BYTES {
                continue;
            }
            let format = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
                crate::notes_markdown::LocalRasterFormat::Png
            } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
                crate::notes_markdown::LocalRasterFormat::Jpeg
            } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
                crate::notes_markdown::LocalRasterFormat::Gif
            } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
                crate::notes_markdown::LocalRasterFormat::WebP
            } else {
                continue;
            };
            total += bytes.len();
            assets.push(OwnedAsset {
                destination,
                bytes,
                format,
            });
        }
        Self(assets)
    }
}

impl crate::notes_markdown::LocalRasterResolver for LocalAssets {
    fn resolve<'a>(
        &'a self,
        relative_destination: &str,
    ) -> Option<crate::notes_markdown::LocalRasterAsset<'a>> {
        self.0
            .iter()
            .find(|asset| asset.destination == relative_destination)
            .map(|asset| crate::notes_markdown::LocalRasterAsset {
                format: asset.format,
                bytes: &asset.bytes,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagram::DiagramSourceCopyOrigin;
    use crate::markdown_studio::MarkdownTransitionOutcome;

    fn dialog_with_source(source: &str) -> MarkdownStudioDialog {
        let directory = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_path(directory.path());
        let mut dialog = MarkdownStudioDialog::new(&root);
        assert_eq!(
            dialog.document.request_new().unwrap(),
            MarkdownTransitionOutcome::Applied
        );
        dialog.document.replace_source(source).unwrap();
        dialog.open = true;
        dialog
    }

    fn frame(dialog: &mut MarkdownStudioDialog, ctx: &egui::Context) {
        frame_with_events(dialog, ctx, Vec::new());
    }

    fn frame_with_events(
        dialog: &mut MarkdownStudioDialog,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) {
        frame_at_size(dialog, ctx, egui::vec2(1000.0, 760.0), events);
    }

    fn frame_at_size(
        dialog: &mut MarkdownStudioDialog,
        ctx: &egui::Context,
        size: egui::Vec2,
        events: Vec<egui::Event>,
    ) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            focused: true,
            events,
            ..Default::default()
        };
        let output = ctx.run_ui(input, |ui| {
            dialog.show(ui.ctx(), ui.ctx().current_pass_index() == 0);
        });
        output.drop_without_applying_deltas();
        let (effect, closed) = dialog.on_ui_frame_complete(ctx);
        assert!(effect.is_none());
        assert!(!closed || !dialog.open);
    }

    fn hosted_frame_with_root_events(
        dialog: &mut MarkdownStudioDialog,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) {
        super::super::render::with_immediate_viewport_renderer(ctx, || {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000.0, 760.0),
                    )),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| {
                    // Match the launcher, which checks ownership before showing the tool.
                    let _ = dialog.owns_root_input(ui.ctx());
                    dialog.show_on_root(
                        ui.ctx(),
                        ui.ctx().current_pass_index() == 0,
                        true,
                        true,
                        true,
                    );
                },
            );
            output.drop_without_applying_deltas();
            let (effect, closed) = dialog.on_ui_frame_complete(ctx);
            assert!(effect.is_none());
            assert!(!closed);
        });
    }

    fn hosted_frame_with_child_events(
        dialog: &mut MarkdownStudioDialog,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) {
        super::super::render::with_immediate_viewport_test_events(events, || {
            hosted_frame_with_root_events(dialog, ctx, Vec::new());
        });
    }

    #[test]
    fn markdown_studio_visible_host_buttons_pop_out_and_dock_the_same_document() {
        let source = "# Markdown Studio Ω\n\nDraft body\n";
        let mut dialog = dialog_with_source(source);
        let omega = source.find('Ω').unwrap();
        dialog.selection = omega..omega + 'Ω'.len_utf8();
        let original_selection = dialog.selection.clone();
        dialog.deferred_selection = Some(original_selection.clone());
        let original_revision = dialog.document.revision();
        let original_undo_depth = dialog.document.undo_depth();
        let original_dirty = dialog.document.is_dirty();
        assert!(original_dirty);
        let ctx = egui::Context::default();
        let directory = tempfile::tempdir().unwrap();
        let path = directory
            .path()
            .join("settings.json")
            .to_string_lossy()
            .into_owned();
        let workers = crate::settings_save::SettingsSaveWorkerRegistry::default();
        let manager = super::super::utility_presentation::UtilityPresentationManager::new(
            path.clone(),
            workers.clone(),
            crate::settings::UtilityPresentations::default(),
        );
        manager.install_in_context(&ctx);
        let key = crate::settings::UtilityToolKey::parse("markdown-studio").unwrap();

        hosted_frame_with_root_events(&mut dialog, &ctx, Vec::new());
        hosted_frame_with_root_events(&mut dialog, &ctx, Vec::new());
        let root_editor_id = dialog.test_editor_id.unwrap();
        ctx.memory_mut(|memory| memory.request_focus(root_editor_id));
        assert!(dialog.owns_root_input(&ctx));
        assert_eq!(
            dialog.selection, original_selection,
            "the editor starts with the Unicode range"
        );
        assert_eq!(dialog.test_presentation_enabled, Some(true));
        let root_button = dialog.test_presentation_rect.unwrap();
        assert!(
            dialog
                .test_presentation_clip_rect
                .unwrap()
                .contains_rect(root_button)
        );
        assert_eq!(
            ctx.layer_id_at(root_button.center()),
            Some(egui::LayerId::new(
                egui::Order::Middle,
                egui::Id::new(STUDIO_WINDOW_ID),
            )),
            "the visible Pop Out control must be on the retained Markdown window layer"
        );
        for pressed in [true, false] {
            hosted_frame_with_root_events(
                &mut dialog,
                &ctx,
                pointer_frame_event(root_button.center(), pressed),
            );
        }

        assert_eq!(
            dialog.host_state.presentation(),
            UtilityPresentation::Popout
        );
        assert_eq!(
            manager.preference(key),
            Some(crate::settings::RememberedPresentation::Popout)
        );
        assert_eq!(dialog.test_source(), Some(source));
        assert_eq!(dialog.document.revision(), original_revision);
        assert_eq!(dialog.document.undo_depth(), original_undo_depth);
        assert_eq!(dialog.document.is_dirty(), original_dirty);
        assert_eq!(dialog.selection, original_selection);

        hosted_frame_with_root_events(&mut dialog, &ctx, Vec::new());
        assert_eq!(dialog.test_editor_id, Some(root_editor_id));
        assert_eq!(dialog.test_presentation_enabled, Some(true));
        let child_button = dialog.test_presentation_rect.unwrap();
        assert!(
            dialog
                .test_presentation_clip_rect
                .unwrap()
                .contains_rect(child_button)
        );
        for pressed in [true, false] {
            hosted_frame_with_child_events(
                &mut dialog,
                &ctx,
                pointer_frame_event(child_button.center(), pressed),
            );
        }

        assert_eq!(
            dialog.host_state.presentation(),
            UtilityPresentation::Docked
        );
        assert!(dialog.take_show_root_after_host_change());
        assert_eq!(
            manager.preference(key),
            Some(crate::settings::RememberedPresentation::Docked)
        );
        hosted_frame_with_root_events(&mut dialog, &ctx, Vec::new());
        assert_eq!(dialog.test_editor_id, Some(root_editor_id));
        assert_eq!(dialog.test_source(), Some(source));
        assert_eq!(dialog.document.revision(), original_revision);
        assert_eq!(dialog.document.undo_depth(), original_undo_depth);
        assert_eq!(dialog.document.is_dirty(), original_dirty);
        assert_eq!(dialog.selection, original_selection);
        workers.join_all();
        assert_eq!(
            crate::settings::Settings::load(&path)
                .unwrap()
                .utility_presentations
                .get(key),
            Some(crate::settings::RememberedPresentation::Docked)
        );
    }

    #[test]
    fn markdown_studio_export_publication_preserves_dirty_document_and_history() {
        let source = "# Studio draft Ω\r\n\r\nbody\r\n";
        let mut dialog = dialog_with_source(source);
        let output_directory = tempfile::tempdir().unwrap();
        let output_path = output_directory.path().join("studio-export.md");
        dialog.test_set_next_picker_result(Some(output_path.clone()));

        let revision = dialog.document.revision();
        let path = dialog.document.path().map(Path::to_path_buf);
        let undo_depth = dialog.document.undo_depth();
        let redo_depth = dialog.document.redo_depth();
        let was_dirty = dialog.document.is_dirty();
        assert!(was_dirty);

        dialog.export_current(MarkdownExportFormat::Markdown);

        assert_eq!(std::fs::read(&output_path).unwrap(), source.as_bytes());
        assert_eq!(dialog.document.source(), Some(source));
        assert_eq!(dialog.document.path(), path.as_deref());
        assert_eq!(dialog.document.is_dirty(), was_dirty);
        assert_eq!(dialog.document.revision(), revision);
        assert_eq!(dialog.document.undo_depth(), undo_depth);
        assert_eq!(dialog.document.redo_depth(), redo_depth);
        assert_eq!(dialog.test_picker_calls(), 1);

        assert!(dialog.document.undo().unwrap());
        assert_eq!(dialog.document.source(), Some(""));
    }

    fn pointer_frame_event(point: egui::Pos2, pressed: bool) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(point),
            egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]
    }

    #[test]
    fn markdown_studio_host_close_cancel_does_not_repeat_on_next_retained_frame() {
        let source = "# draft\n";
        let mut dialog = dialog_with_source(source);
        let ctx = egui::Context::default();
        let directory = tempfile::tempdir().unwrap();
        let path = directory
            .path()
            .join("settings.json")
            .to_string_lossy()
            .into_owned();
        let workers = crate::settings_save::SettingsSaveWorkerRegistry::default();
        let manager = super::super::utility_presentation::UtilityPresentationManager::new(
            path,
            workers.clone(),
            crate::settings::UtilityPresentations::default(),
        );
        manager.install_in_context(&ctx);
        let key = crate::settings::UtilityToolKey::parse("markdown-studio").unwrap();
        manager.remember(&ctx, key, crate::settings::RememberedPresentation::Docked);
        // Settle the window before the host request so the tested frame is an
        // ordinary retained frame, not its initial sizing pass.
        frame(&mut dialog, &ctx);
        frame(&mut dialog, &ctx);

        dialog.request_close();
        frame(&mut dialog, &ctx);
        assert_eq!(
            dialog.document.pending_transition(),
            Some(&PendingMarkdownTransition::Close)
        );
        assert!(dialog.open);
        assert!(!dialog.close_requested);

        let _ = dialog.apply_intent(
            StudioIntent::Decision(MarkdownTransitionDecision::Cancel),
            &ctx,
        );
        assert_eq!(dialog.document.pending_transition(), None);
        assert!(dialog.open);

        frame(&mut dialog, &ctx);
        assert_eq!(dialog.document.pending_transition(), None);
        assert!(dialog.open);
        assert_eq!(dialog.document.source(), Some(source));
        assert_eq!(
            manager.preference(key),
            Some(crate::settings::RememberedPresentation::Docked)
        );
        workers.join_all();
    }

    #[cfg(windows)]
    #[test]
    fn markdown_studio_alt_space_cannot_activate_pending_discard_but_plain_space_can() {
        let source = "# unsaved draft\n";
        let mut dialog = dialog_with_source(source);
        let ctx = egui::Context::default();
        frame(&mut dialog, &ctx);
        frame(&mut dialog, &ctx);

        dialog.request_close();
        frame(&mut dialog, &ctx);
        assert_eq!(
            dialog.document.pending_transition(),
            Some(&PendingMarkdownTransition::Close)
        );
        // The first frame commits the close request. Render the decision UI
        // in its own ordinary frame before targeting its real Discard button.
        frame(&mut dialog, &ctx);
        let discard_id = dialog.test_discard_id().unwrap();
        ctx.memory_mut(|memory| memory.request_focus(discard_id));

        let alt = egui::Modifiers::ALT;
        frame_with_events(
            &mut dialog,
            &ctx,
            vec![
                egui::Event::ModifiersChanged(alt),
                egui::Event::Key {
                    key: egui::Key::Space,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: alt,
                },
                egui::Event::Text(" ".into()),
                egui::Event::Key {
                    key: egui::Key::Space,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers: alt,
                },
                egui::Event::ModifiersChanged(egui::Modifiers::NONE),
            ],
        );
        assert_eq!(
            dialog.document.pending_transition(),
            Some(&PendingMarkdownTransition::Close)
        );
        assert!(dialog.open);
        assert_eq!(dialog.document.source(), Some(source));

        ctx.memory_mut(|memory| memory.request_focus(discard_id));
        frame_with_events(
            &mut dialog,
            &ctx,
            vec![egui::Event::Key {
                key: egui::Key::Space,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(dialog.document.pending_transition(), None);
        assert!(!dialog.open);
        assert_eq!(dialog.document.source(), None);
    }

    #[test]
    fn markdown_studio_keeps_logical_unicode_selection_when_toolbar_takes_focus() {
        let source = "AéB";
        let mut dialog = dialog_with_source(source);
        let ctx = egui::Context::default();
        frame(&mut dialog, &ctx);
        let editor_id = dialog.test_editor_id.unwrap();
        install_selection(&ctx, editor_id, source, 1..3);
        frame(&mut dialog, &ctx);
        assert_eq!(dialog.selection, 1..3);

        let format_menu = dialog.test_format_menu_rect.unwrap();
        frame_with_events(
            &mut dialog,
            &ctx,
            pointer_frame_event(format_menu.center(), true),
        );
        frame_with_events(
            &mut dialog,
            &ctx,
            pointer_frame_event(format_menu.center(), false),
        );
        // The first open callback sizes the popup; wait for its ordinary
        // visible pass before using the child button geometry.
        frame(&mut dialog, &ctx);

        assert!(!ctx.memory(|memory| memory.has_focus(editor_id)));
        assert_eq!(dialog.selection, 1..3);
    }

    #[test]
    fn markdown_studio_format_toolbar_preserves_selected_unicode_source_range() {
        let source = "AéB";
        let mut dialog = dialog_with_source(source);
        let ctx = egui::Context::default();
        frame(&mut dialog, &ctx);
        frame(&mut dialog, &ctx);
        let editor_id = dialog.test_editor_id.unwrap();
        install_selection(&ctx, editor_id, source, 1..3);
        frame(&mut dialog, &ctx);
        assert_eq!(dialog.selection, 1..3);

        let format_menu = dialog.test_format_menu_rect.unwrap();
        frame_with_events(
            &mut dialog,
            &ctx,
            pointer_frame_event(format_menu.center(), true),
        );
        frame_with_events(
            &mut dialog,
            &ctx,
            pointer_frame_event(format_menu.center(), false),
        );
        frame(&mut dialog, &ctx);
        let bold = dialog
            .test_bold_rect
            .expect("the open Format menu renders Bold");
        frame_with_events(&mut dialog, &ctx, pointer_frame_event(bold.center(), true));
        frame_with_events(&mut dialog, &ctx, pointer_frame_event(bold.center(), false));
        frame(&mut dialog, &ctx);

        let formatted = dialog.document.source().unwrap();
        assert_eq!(formatted, "A**é**B");
        assert_eq!(&formatted[dialog.selection.clone()], "**é**");
        assert!(dialog.document.undo().unwrap());
        assert_eq!(dialog.document.source(), Some(source));
    }

    #[test]
    fn markdown_studio_narrow_scaled_format_control_remains_reachable() {
        let source = "AéB";
        let mut dialog = dialog_with_source(source);
        let ctx = egui::Context::default();
        let mut style = ctx.style_of(ctx.theme()).as_ref().clone();
        for font in style.text_styles.values_mut() {
            font.size *= 1.6;
        }
        ctx.set_style_of(ctx.theme(), style);

        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(700.0, 520.0));
        for _ in 0..3 {
            frame_at_size(&mut dialog, &ctx, screen.size(), Vec::new());
        }
        let editor_id = dialog.test_editor_id.unwrap();
        install_selection(&ctx, editor_id, source, 1..3);
        frame_at_size(&mut dialog, &ctx, screen.size(), Vec::new());
        assert_eq!(dialog.selection, 1..3);

        let format_menu = dialog.test_format_menu_rect.unwrap();
        assert!(screen.contains(format_menu.center()));
        frame_at_size(
            &mut dialog,
            &ctx,
            screen.size(),
            pointer_frame_event(format_menu.center(), true),
        );
        frame_at_size(
            &mut dialog,
            &ctx,
            screen.size(),
            pointer_frame_event(format_menu.center(), false),
        );
        // Re-observe Bold from the popup's ordinary visible pass after its
        // sizing callback.
        frame_at_size(&mut dialog, &ctx, screen.size(), Vec::new());
        let bold = dialog
            .test_bold_rect
            .expect("the constrained Format menu still exposes Bold");
        assert!(screen.contains(bold.center()));
        frame_at_size(
            &mut dialog,
            &ctx,
            screen.size(),
            pointer_frame_event(bold.center(), true),
        );
        frame_at_size(
            &mut dialog,
            &ctx,
            screen.size(),
            pointer_frame_event(bold.center(), false),
        );
        frame_at_size(&mut dialog, &ctx, screen.size(), Vec::new());

        let formatted = dialog.document.source().unwrap();
        assert_eq!(formatted, "A**é**B");
        assert_eq!(&formatted[dialog.selection.clone()], "**é**");
    }

    #[test]
    fn markdown_studio_diagram_source_handoff_replaces_exact_fence_as_one_undo() {
        let source = "before\r\n```mermaid\r\nflowchart TD\r\nA --> B\r\n```\r\nafter\r\n";
        let mut dialog = dialog_with_source(source);
        let identity = DiagramSourceCopyOrigin::fresh();
        dialog.active_diagram_origin = Some(MarkdownFenceOrigin {
            identity: identity.clone(),
            source_snapshot: source.into(),
            fence_index: 0,
                            host_origin: None,
        });
        let revision = dialog.document.revision().unwrap();
        let undo_depth = dialog.document.undo_depth();

        dialog.apply_diagram_source_handoff(super::super::DiagramMarkdownSource {
            origin: identity,
            source: "flowchart LR\nC --> D".into(),
        });

        let expected = "before\r\n```mermaid\r\nflowchart LR\r\nC --> D\r\n```\r\nafter\r\n";
        assert_eq!(dialog.document.source(), Some(expected));
        assert_eq!(dialog.document.revision(), Some(revision + 1));
        assert_eq!(dialog.document.undo_depth(), undo_depth + 1);
        assert!(dialog.active_diagram_origin.is_none());
        assert!(dialog.document.undo().unwrap());
        assert_eq!(dialog.document.source(), Some(source));
        assert!(dialog.document.redo().unwrap());
        assert_eq!(dialog.document.source(), Some(expected));
    }

    #[test]
    fn markdown_studio_diagram_source_handoff_rejects_changed_origin_source() {
        let source = "```mermaid\nflowchart TD\nA --> B\n```\n";
        let mut dialog = dialog_with_source(source);
        let identity = DiagramSourceCopyOrigin::fresh();
        dialog.active_diagram_origin = Some(MarkdownFenceOrigin {
            identity: identity.clone(),
            source_snapshot: source.into(),
            fence_index: 0,
                            host_origin: None,
        });
        let changed = format!("{source}\nchanged\n");
        dialog.document.replace_source(&changed).unwrap();

        dialog.apply_diagram_source_handoff(super::super::DiagramMarkdownSource {
            origin: identity,
            source: "flowchart LR\nC --> D".into(),
        });

        assert_eq!(dialog.document.source(), Some(changed.as_str()));
        assert_eq!(dialog.document.undo_depth(), 2);
        assert!(
            dialog
                .message
                .as_ref()
                .is_some_and(|(error, message)| { *error && message.contains("source changed") })
        );
    }

    #[test]
    fn markdown_studio_preview_uses_bounded_document_relative_assets() {
        use eframe::egui::load::ImageLoader as _;

        let workspace = tempfile::tempdir().unwrap();
        let document_directory = workspace.path().join("docs");
        std::fs::create_dir_all(&document_directory).unwrap();
        image_025::RgbaImage::from_pixel(1, 1, image_025::Rgba([10, 20, 30, 255]))
            .save(document_directory.join("local.png"))
            .unwrap();
        image_025::RgbaImage::from_pixel(1, 1, image_025::Rgba([40, 50, 60, 255]))
            .save(workspace.path().join("outside.png"))
            .unwrap();
        let source = "![local](local.png) ![outside](../outside.png) ![remote](https://example.invalid/image.png)";
        let assets = LocalAssets::new(Some(&document_directory), source);
        assert_eq!(assets.0.len(), 1);

        let ctx = egui::Context::default();
        let loader = MarkdownAssetImageLoader::new();
        let _cache = MarkdownPreviewCache::new(
            &ctx,
            1,
            1,
            source,
            Some(&document_directory),
            Some(&assets),
            &loader,
        )
        .unwrap();
        assert!(loader.byte_size() > 0);
    }
}
