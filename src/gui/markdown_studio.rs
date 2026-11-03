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
use std::io;
use std::ops::Range;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use super::markdown_preview::{MarkdownAssetImageLoader, MarkdownPreviewCache};

const STUDIO_WINDOW_ID: &str = "markdown-studio-window";
const SOURCE_EDITOR_ID: &str = "markdown-studio-source";
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
    frame_nr: u64,
    pass_nr: u64,
    source: Option<String>,
    selection: Option<Range<usize>>,
    selection_to_install: Option<Range<usize>>,
    focus_editor_after_open: bool,
    view_mode: Option<ViewMode>,
    intent: Option<StudioIntent>,
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
}

pub(crate) struct MarkdownStudioDialog {
    pub(crate) open: bool,
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
}

impl MarkdownStudioDialog {
    pub(crate) fn new(root: &AppDataRoot) -> Self {
        Self {
            open: false,
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
        self.open = true;
        self.deferred_selection = None;
        // The launcher can open the Studio while the Enter event that
        // activated the query result is still in this root pass. Defer source
        // focus until that pass has been retained, then install the saved
        // selection before the next editor pass.
        self.open_focus_pending = true;
        self.close_requested = false;
    }

    /// Pinned maintenance is passive and cannot resurrect a retired document.
    pub(crate) fn ensure_open(&mut self) {
        if self.document.source().is_some() {
            self.open = true;
        }
    }

    pub(crate) fn request_close(&mut self) {
        if self.open {
            self.close_requested = true;
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

    pub(crate) fn track_diagram_source_origin(&mut self, origin: MarkdownFenceOrigin) {
        self.pending_diagram_origin = Some(origin);
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
        }
        if !self.open {
            return;
        }
        let frame_nr = ctx.cumulative_frame_nr_for(egui::ViewportId::ROOT);
        let pass_nr = ctx.cumulative_pass_nr_for(egui::ViewportId::ROOT);
        let original_source = self.document.source().unwrap_or_default().to_owned();
        let revision = self.document.revision().unwrap_or(0);
        self.ensure_preview(ctx, revision, &original_source);
        let mut source = original_source.clone();
        let mut selection = self.selection.clone();
        let mut selection_to_install = None;
        let mut intent = None;
        let mut next_mode = None;
        let focus_editor_after_open = self.open_focus_pending;
        let authorized = first_ui_pass && !ctx.will_discard();
        #[cfg(windows)]
        if authorized {
            ctx.input_mut(|input| {
                super::native_keyboard::reserve_windows_system_menu_alt_space(&mut input.events);
                super::native_keyboard::reserve_windows_system_menu_alt_space(
                    &mut input.raw.events,
                );
            });
        }
        if authorized && self.document.pending_transition().is_none() {
            let popup_open = egui::Popup::is_any_open(ctx);
            if ctx.input_mut(|input| {
                (!popup_open && input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
                    || input.consume_key(egui::Modifiers::COMMAND, egui::Key::W)
            }) || self.close_requested
            {
                intent = Some(StudioIntent::Close);
            }
        }

        let content_size = ctx.content_rect().size();
        let min_size = egui::vec2(460.0, 320.0).min(content_size);
        let mut window_open = true;
        let output = egui::Window::new("Markdown Studio")
            .id(egui::Id::new(STUDIO_WINDOW_ID))
            .open(&mut window_open)
            .resizable(true)
            .default_size(egui::vec2(1180.0, 760.0))
            .min_size(min_size)
            .max_size(content_size)
            .show(ctx, |ui| {
                let allowed = authorized && ui.is_visible() && !ui.is_sizing_pass();
                ui.add_enabled_ui(allowed, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("markdown-studio-workspace")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            self.header_ui(
                                ui,
                                &mut source,
                                &mut selection,
                                &mut selection_to_install,
                                &mut intent,
                                &mut next_mode,
                            );
                            self.message_ui(ui);
                            self.transition_ui(ui, &mut intent);
                            ui.separator();
                            let pending = self.document.pending_transition().is_some();
                            let current_mode = next_mode.unwrap_or(self.mode);
                            let width = ui.available_width();
                            if current_mode == ViewMode::Source {
                                self.editor_panel(
                                    ui,
                                    ctx,
                                    &mut source,
                                    &mut selection,
                                    &mut selection_to_install,
                                    &mut intent,
                                    allowed && !pending,
                                );
                            } else if current_mode == ViewMode::Preview {
                                self.preview_panel(
                                    ui,
                                    &mut intent,
                                    allowed && !pending,
                                    &original_source,
                                );
                            } else if width >= 880.0 {
                                ui.columns(2, |columns| {
                                    self.editor_panel(
                                        &mut columns[0],
                                        ctx,
                                        &mut source,
                                        &mut selection,
                                        &mut selection_to_install,
                                        &mut intent,
                                        allowed && !pending,
                                    );
                                    self.preview_panel(
                                        &mut columns[1],
                                        &mut intent,
                                        allowed && !pending,
                                        &original_source,
                                    );
                                });
                            } else {
                                self.editor_panel(
                                    ui,
                                    ctx,
                                    &mut source,
                                    &mut selection,
                                    &mut selection_to_install,
                                    &mut intent,
                                    allowed && !pending,
                                );
                                ui.separator();
                                self.preview_panel(
                                    ui,
                                    &mut intent,
                                    allowed && !pending,
                                    &original_source,
                                );
                            }
                        });
                });
            });

        if output.is_none() || !authorized {
            return;
        }
        if !window_open {
            intent = Some(StudioIntent::Close);
        }
        let source_changed = source != original_source;
        if source_changed
            || intent.is_some()
            || next_mode.is_some()
            || selection != self.selection
            || selection_to_install.is_some()
            || focus_editor_after_open
        {
            self.pending_effects = Some(RootEffects {
                frame_nr,
                pass_nr,
                source: source_changed.then_some(source),
                selection: (selection != self.selection).then_some(selection),
                selection_to_install,
                focus_editor_after_open,
                view_mode: next_mode,
                intent,
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
        let retained = ctx.cumulative_frame_nr_for(egui::ViewportId::ROOT)
            == effects.frame_nr.saturating_add(1)
            && ctx.cumulative_pass_nr_for(egui::ViewportId::ROOT)
                == effects.pass_nr.saturating_add(1);
        if !retained {
            return (None, false);
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
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
        if let Some(intent) = effects.intent {
            if matches!(&intent, StudioIntent::Close) {
                // Consume a host/title/keyboard close only after this frame was
                // retained. If it is discarded, the request remains available
                // for the next accepted frame.
                self.close_requested = false;
            }
            let effect = self.apply_intent(intent, ctx);
            return (effect, !self.open);
        }
        (None, false)
    }

    fn header_ui(
        &mut self,
        ui: &mut egui::Ui,
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
        if ui.memory(|memory| memory.has_focus(id)) {
            if let Some(range) = cursor_range_to_bytes(ctx, id, source) {
                *selection = range;
            }
        }
        if let Some(analysis) = self.cache.as_ref().and_then(MarkdownPreviewCache::analysis) {
            if !analysis.headings.is_empty() {
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
            let mut picker = rfd::FileDialog::new().add_filter("Markdown (*.md)", &["md"]);
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
            let stem = self
                .document
                .path()
                .and_then(Path::file_stem)
                .unwrap_or_else(|| std::ffi::OsStr::new("untitled"))
                .to_string_lossy();
            let mut picker = rfd::FileDialog::new()
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

fn ensure_extension(mut path: PathBuf, extension: &str) -> PathBuf {
    if !path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case(extension))
    {
        path.set_extension(extension);
    }
    path
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
            if let Event::Start(Tag::Image { dest_url, .. }) = event {
                if destinations.len() < MAX_LOCAL_ASSET_COUNT {
                    destinations.insert(dest_url.to_string());
                }
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

fn publish_export(
    artifact: &MarkdownExportArtifact,
    destination: &Path,
    active_path: Option<&Path>,
) -> Result<(), String> {
    if let Some(active) = active_path {
        if paths_equal(active, destination).map_err(|error| error.to_string())? {
            return Err("Export cannot overwrite the active Markdown document.".into());
        }
    }
    match fs::symlink_metadata(destination) {
        Ok(_) => {
            return Err(format!(
                "Refusing to overwrite occupied export destination {}.",
                destination.display()
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "Could not inspect export destination {}: {error}",
                destination.display()
            ));
        }
    }
    crate::common::atomic_file::save_atomic(destination, artifact.bytes())
        .map_err(|error| format!("Could not write export {}: {error}", destination.display()))
}

fn paths_equal(left: &Path, right: &Path) -> io::Result<bool> {
    let left = resolved_path(left)?;
    let right = resolved_path(right)?;
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::Win32::Foundation::BOOL;
        use windows::Win32::Globalization::{CSTR_EQUAL, CompareStringOrdinal};
        let left = left.as_os_str().encode_wide().collect::<Vec<_>>();
        let right = right.as_os_str().encode_wide().collect::<Vec<_>>();
        if left.len() > i32::MAX as usize || right.len() > i32::MAX as usize {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "path is too long",
            ));
        }
        let result = unsafe { CompareStringOrdinal(&left, &right, BOOL(1)) };
        if result.0 == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(result == CSTR_EQUAL)
    }
    #[cfg(not(windows))]
    {
        Ok(left == right)
    }
}

fn resolved_path(path: &Path) -> io::Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut current = absolute;
    let mut missing = Vec::new();
    loop {
        match fs::canonicalize(&current) {
            Ok(mut resolved) => {
                for component in missing.iter().rev() {
                    resolved.push(component);
                }
                return Ok(resolved);
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let Some(name) = current.file_name() else {
                    return Err(error);
                };
                missing.push(name.to_os_string());
                let Some(parent) = current.parent() else {
                    return Err(error);
                };
                current = parent.to_path_buf();
            }
            Err(error) => return Err(error),
        }
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
